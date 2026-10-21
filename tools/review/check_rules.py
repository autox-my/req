#!/usr/bin/env python3
"""Check both directions between review/rule.json and the tracked tree, and the text itself.

Why this exists: the review CLI **silently ignores** a rule whose field names it does
not recognise, and equally silently falls back to the built-in rubric for a `path` glob
that matches nothing. A renamed directory would therefore turn an entry into a no-op
that still "loads" — the review would quietly lose that checklist, and every exit code
would stay zero. So the *effect* is checked here, not the exit status.

Three things are checked, because only the first was ever checked. **Rules → files**: every
entry in the rule file must still match at least one tracked file, or it is a silent
no-op. **Files → rules**: every tracked file must resolve to `source: custom`, or be
named in ALLOW_LIST below with its reason — otherwise the vendor rubric judges it by
accident, which is exactly how `build.rs`, `decisions/DEC-0001.yml` and `tools/req-d1.sh`
sat on the generic checklist while every exit code stayed zero. **The text**: `rule` is
optional to `json`, and the CLI answers `source: "custom"` with whatever text it was
handed — measured, a whitespace-only `rule` on `src/**/*.rs` came back `source: custom,
rule: "   "`, so both directions above stayed green while the reviewer would have run an
empty checklist for the engine's own files. The declared side is therefore checked for a
usable `path` and `rule` and for a duplicated `path` (the CLI resolves one entry per path,
so every other entry's checklist reaches nobody), and the resolved text must equal the
declared text verbatim. `.forgejo/**` is the one entry whose text is followed all the way
into the provider request, by `tools/review/check_review_scope.py`; this file covers every
other entry — no count, because a count in a docstring goes stale the moment an entry is
added, and this project's docs are claims.

The matcher is deliberately **not** re-implemented here. `fnmatch` does not implement
gitignore `**` (an attempt reported 58 uncovered paths where the CLI resolves 16), and a
second implementation is free to drift from the one that decides — the defect
`src/funnel.rs` already records for its tokenizer. Resolution is always
`ocr delegate rule`, the same CLI the review uses.

This is a check of a file we own, so a missing `ocr` is UNVERIFIABLE (exit 3), never a
pass: the same three-state rule as the rest of the gate. `--self-check` table-drives the
pure half — the tree listing, the membership rule, the entry reader, and the preview's JSON
shape — and runs no review, so it is testable with no `ocr` and no git, which is exactly
where the rest of this script can only SKIP.

**Two steps, two attributions.** `ocr delegate rule` runs **first** in this check, and it is
where a document this CLI cannot use is attested: a non-zero exit there is a failure of our own
file. The `ocr delegate preview` probe runs afterwards and answers a different question — the
*selection* — so its non-zero exit is UNVERIFIABLE here: it can be an unknown flag, a crash, or a
throwaway repository that broke, and none of those is a finding about `review/rule.json`. A
preview whose JSON is not the object shape `preview_shape_problems` accepts is UNVERIFIABLE for
the same reason; the probe is last precisely so this attribution is not a claim about an order
that does not hold.

Exit codes: 0 every entry matched a tracked file with its text arriving verbatim AND every
tracked file resolved to a custom rule or a listed exception; 1 a real gap (an entry with
no usable path or text, a duplicated path, a text that did not arrive verbatim, an entry
matching no tracked file, a file left to the built-in rubric, an allow-list entry that has
gone stale, or an exclusion the CLI does not honour); 3 could not answer (no `ocr`, no
tracked files, a preview that exited non-zero or answered an unexpected JSON shape, or
unparseable CLI output).
"""
import argparse
import json
import os
import tempfile
import pathlib
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
RULE_FILE = ROOT / "review" / "rule.json"

# Files the built-in rubric legitimately judges. This list is the deliverable, not a
# convenience: it turns "which files the vendor rubric judges" from an emergent property
# into a reviewed decision, so the next new directory announces itself here instead of
# falling through. An entry is an **exact path**, or a **directory prefix with a trailing
# `/`** for a whole generated class. Every entry carries its one-line reason, and an entry
# that stops matching a tracked file is itself a failure — it would hide the next gap at
# exactly that path.
ALLOW_LIST = {
    ".gitignore": "ignore rules only — no requirement semantics; reviewed in the diff by hand",
    "Cargo.lock": "lockfile — the review CLI excludes lockfiles (default_path), and pins are provenance",
    "README.md": "markdown — the CLI excludes it as unsupported_ext; hand-reviewed by rule",
    "CONTEXT.md": "markdown, glossary only — hand-reviewed by rule (the CLI excludes it)",
    "claims/README.md": "markdown — hand-reviewed by rule (the CLI excludes it)",
    "bench/corpus/tier2-positives-v1-captures/": "generated capture data — reviewed through its generator, not line by line",
    "tools/ocr/package.json": "the pinned review CLI's own manifest — reviewed at pin time, not by our rule file",
    "tools/ocr/package-lock.json": "lockfile — the review CLI excludes lockfiles (default_path)",
    "baselines/air-gapped.json.minisig": (
        "a minisign signature — excluded from review by decision: the CLI cannot read the "
        "extension, and `req baseline --verify` (in the gate, no SKIP) is what verifies it"
    ),
}


def parse_tracked(stdout: str) -> list:
    """`git ls-files -z` output → the tracked paths.

    `-z`, never the newline-separated form: git C-quotes a path containing a space, a quote
    or a non-ASCII byte, so `"dir with space/note.md"` split on whitespace becomes
    `['"dir', 'with', 'space/note.md"']` — three strings that name nothing. The real file
    then reads as uncovered, and the CLI is handed paths that do not exist, which is how
    this check came to report a correct glob as dead. `build.rs` reads its own listing with
    `-z` for the same reason.
    """
    return [path for path in stdout.split("\0") if path]


def usable_rule_text(text) -> bool:
    """True only for a string with something in it.

    `rule` is optional to `json`, and a blank text is what makes a matched entry an inert
    checklist without failing anything: the CLI still answers `source: "custom"`.
    """
    return isinstance(text, str) and bool(text.strip())


def covers(entry: str, path: str) -> bool:
    """One membership rule for the allow-list: an exact path, or a directory prefix.

    Deliberately one implementation: `allowed` ("does this path have an exception?") and
    `entry_covers` ("is this exception still needed?") are the same question asked in two
    directions, and a drift between them would silently change what the list means.
    """
    if entry.endswith("/"):
        return path.startswith(entry)
    return path == entry


def allowed(path: str) -> bool:
    """True when `path` is an allow-listed exact path or sits under an allow-listed prefix."""
    return any(covers(entry, path) for entry in ALLOW_LIST)


def entry_covers(entry: str, tracked: list) -> bool:
    """True when an allow-list entry still names something in the tree."""
    return any(covers(entry, path) for path in tracked)


def duplicate_paths(paths: list) -> list:
    """The paths declared more than once, sorted."""
    counts: dict = {}
    for path in paths:
        counts[path] = counts.get(path, 0) + 1
    return sorted(path for path, count in counts.items() if count > 1)


def read_entries(rules: list) -> tuple:
    """The declared side: paths in file order, usable text per path, and what is wrong.

    Never raises. `rule` is optional to `json` and `path` is only a convention, so a
    hand-edited file can carry neither — and indexing either was a traceback and an exit
    code nobody chose until this reader existed.
    """
    paths: list = []
    texts: dict = {}
    problems: list = []
    for index, entry in enumerate(rules):
        if not isinstance(entry, dict):
            problems.append(f"entry {index} is not an object")
            continue
        path, text = entry.get("path"), entry.get("rule")
        if not isinstance(path, str) or not path.strip():
            problems.append(f"entry {index} has no usable 'path'")
            continue
        paths.append(path)
        if usable_rule_text(text):
            texts[path] = text
        else:
            problems.append(f"entry {index} ({path!r}) carries no usable 'rule' text")
    for path in duplicate_paths(paths):
        problems.append(
            f"{path!r} is declared more than once; the CLI resolves one entry per path, so "
            "every other entry's checklist can never reach a reviewer"
        )
    return paths, texts, problems


def _shape(rules: list) -> tuple:
    """A self-check row's view of the reader: the paths it kept, and how many problems."""
    paths, _texts, problems = read_entries(rules)
    return paths, len(problems)


# The two keys a review document may carry. `ocr` ignores unknown keys silently (Go drops
# them), so a misspelled "excludes" would be a no-op that still loads — the same trap this
# script exists to catch in entry fields, one level up.
TOP_LEVEL_KEYS = frozenset({"rules", "exclude"})
# The blind spot the `baselines/**` checklist declares: `ocr` cannot see a `.minisig`, so a
# change to a signed Baseline always looks like it is missing one. The declaration is pinned
# here because a declaration that can vanish silently is worth nothing.
SIGNATURE_EXCLUDE = "baselines/*.minisig"
SIGNATURE_DECLARATION = "outside your view"
# The *load-bearing* half of the declaration: the instruction the reviewer must carry. Pinning the
# diagnosis without it would let the instruction be deleted while this script still passed.
SIGNATURE_INSTRUCTION = (
    'do not report "no updated signature is part of this change" as a finding'
)


def declared_document_problems(document) -> list:
    """What is wrong with the document's own shape, before any CLI is consulted."""
    if not isinstance(document, dict):
        return ["the document is not a JSON object"]
    problems = [
        f"unknown top-level key {key!r} — the CLI ignores unknown keys silently, so a typo "
        "is a no-op"
        for key in sorted(set(document) - TOP_LEVEL_KEYS)
    ]
    if not isinstance(document.get("rules"), list):
        problems.append("no 'rules' list")
    exclude = document.get("exclude", [])
    if not isinstance(exclude, list):
        problems.append("'exclude' must be a list of gitignore-style patterns")
    else:
        for pattern in exclude:
            if not isinstance(pattern, str) or not pattern.strip():
                problems.append(f"'exclude' holds a value that is not a usable pattern: {pattern!r}")
    return problems


def preview_shape_problems(parsed) -> list:
    """What is wrong with a preview payload's own shape, before any entry is read.

    Pure, so `--self-check` can table-drive every shape the CLI could answer. A preview that is
    not a JSON object is not "a selection with no files": indexing it was an `AttributeError`
    raised **outside** the `json.loads` try, which the outer handler does not catch — a
    traceback in the gate instead of a verdict.
    """
    if not isinstance(parsed, dict):
        return ["the preview is not a JSON object — the CLI's JSON shape changed"]
    problems = []
    for key in ("reviewable_files", "excluded_files"):
        entries = parsed.get(key)
        if not isinstance(entries, list):
            problems.append(f"the preview has no {key} list — the CLI's JSON shape changed")
            continue
        if any(
            not isinstance(entry, dict) or not isinstance(entry.get("path"), str)
            for entry in entries
        ):
            problems.append(
                f"the preview's {key} entries have no string path — the CLI's JSON shape changed"
            )
    return problems


def selection_problems(document: dict) -> tuple:
    """Does the CLI *honour* the exclusion? A pinned spelling is not an effect.

    `ocr` is the only implementation of the selection, so the effect is measured on a throwaway
    repository: a **probe** exclude naming a reviewable path (`src/keep.rs`) must take it out with
    `user_exclude`. That is the load-bearing assertion — it goes red if a future CLI drops or
    renames the top-level key.

    What is deliberately **not** asserted: that a `.minisig` is unreviewable. `ocr` filters that
    extension already, so the assertion would pass without our exclusion doing anything.

    Returns `(status, problems)`: **0** measured clean, **1** a defect in our document or its
    effect, **3** could not answer. The three are not interchangeable: a defect is ours to fix, an
    unanswerable measurement is not a pass, and the caller must not read 0 as "no problems" while
    a list is non-empty — which is why 1 is returned wherever a real problem is found.
    """
    identity = {
        "GIT_AUTHOR_NAME": "req",
        "GIT_AUTHOR_EMAIL": "req@localhost",
        "GIT_COMMITTER_NAME": "req",
        "GIT_COMMITTER_EMAIL": "req@localhost",
    }
    work = None
    try:
        work = pathlib.Path(tempfile.mkdtemp(prefix="req-review-effect-"))
        # **`GIT_*` is stripped.** Run from a git hook (this gate is one), the environment carries
        # `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE` pointing at the caller's repository, and
        # `git init` here would then operate on *that* repository rather than the throwaway one.
        env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        env.update(identity)

        def git(*args: str) -> None:
            # `-c commit.gpgsign=false`: a developer whose global config signs every commit would
            # otherwise fail this check locally and pass in CI. `core.hooksPath=/dev/null` keeps a
            # global hook out of a throwaway repository.
            subprocess.run(
                ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args],
                cwd=work, check=True, capture_output=True, text=True, env=env,
            )

        def preview(rule: pathlib.Path) -> tuple:
            """`(status, payload)`: 0 a usable selection, 3 could not answer.

            A non-zero exit is **not** a verdict on `rule`. It can be an unknown flag, a crash, or
            a throwaway repository that broke under us, and only one of those is about our
            document — so attributing it to the document here would be a false finding. The
            document's own usability is attested by the `ocr delegate rule` step, which this
            check runs **before** this probe; what this call answers is the **selection**, and
            "could not answer" is the honest answer when it cannot measure it.

            `cwd=work` alone selects the throwaway repository: `--repo` is not part of the preview
            surface this project documents, and a hidden dependency on it would silently preview
            the *real* repository if a future CLI dropped the flag.
            """
            proc = subprocess.run(
                ["ocr", "delegate", "preview", "--format", "json",
                 "--from", "HEAD~1", "--to", "HEAD", "--rule", str(rule)],
                cwd=work, capture_output=True, text=True, env=env,
            )
            if proc.returncode != 0:
                return 3, f"the preview exited {proc.returncode}: {proc.stderr.strip()[:200]}"
            try:
                parsed = json.loads(proc.stdout)
            except Exception as exc:
                return 3, f"unparsable preview: {exc}"
            problems = preview_shape_problems(parsed)
            if problems:
                return 3, "; ".join(problems)
            return 0, parsed

        git("init", "-q", ".")
        (work / "baselines").mkdir()
        (work / "src").mkdir()
        (work / "src" / "keep.rs").write_text("x\n")
        (work / "baselines" / "manifest.json").write_text("{}\n")
        git("add", "-A")
        git("commit", "-qm", "base")
        (work / "src" / "keep.rs").write_text("y\n")
        (work / "baselines" / "manifest.json").write_text('{"v":3}\n')
        git("add", "-A")
        git("commit", "-qm", "head")

        status, base = preview(RULE_FILE)
        if status == 3:
            return 3, [f"cannot measure the selection: {base}"]
        if "src/keep.rs" not in [entry["path"] for entry in base["reviewable_files"]]:
            return 1, ["the exclusion is too wide: src/keep.rs is not reviewable"]

        # The probe is built from the document `main()` already validated, never re-read: a second
        # read would test bytes nobody checked (and could raise on a file that changed under us).
        probe = dict(document)
        probe["exclude"] = [SIGNATURE_EXCLUDE, "src/keep.rs"]
        probe_file = work / "probe-rule.json"
        probe_file.write_text(json.dumps(probe), encoding="utf-8")
        status, probed = preview(probe_file)
        if status == 3:
            return 3, [f"cannot measure the exclusion effect: {probed}"]
        excluded = {
            entry["path"]: entry.get("exclude_reason") for entry in probed["excluded_files"]
        }
        if excluded.get("src/keep.rs") != "user_exclude":
            return 1, [
                "the top-level exclude key is not honoured by this `ocr` "
                f"(src/keep.rs is {excluded.get('src/keep.rs')!r}) — the blind spot is no longer "
                "declared in a way the CLI acts on, and the next signing PR will be flagged"
            ]
        return 0, []
    except (OSError, subprocess.CalledProcessError) as exc:
        # A missing `git`, a read-only or full `/tmp`, a hook: this check cannot answer, and a
        # traceback in the gate neither fails with a reason nor stays quiet.
        return 3, [f"cannot build the throwaway repository: {exc}"]
    finally:
        if work is not None:
            shutil.rmtree(work, ignore_errors=True)


def self_check() -> int:
    """Table-drive the pure half. No `ocr`, no git, no review.

    The listing and the membership rule are pure functions, and the entry reader is handed
    hand-authored documents — so every shape it can meet is a row here. The second row is
    why it reads `-z`; the blank-text rows are the inert checklist both directions above
    used to pass. The preview's shape check is pure too, and it must not spawn anything:
    an `ocr` or `git` case belongs in the real run, which is where a subprocess can actually
    answer.
    """
    rows = [
        ("the listing is NUL-separated", parse_tracked("a\0b\0"), ["a", "b"]),
        (
            "a path containing a space stays one path",
            parse_tracked("dir with space/note.md\0plain.txt\0"),
            ["dir with space/note.md", "plain.txt"],
        ),
        (
            "a path containing a quote stays one path",
            parse_tracked('quote"name.md\0'),
            ['quote"name.md'],
        ),
        ("a path containing a newline stays one path", parse_tracked("a\nb.md\0"), ["a\nb.md"]),
        ("a listing without a trailing NUL", parse_tracked("a\0b"), ["a", "b"]),
        ("an empty listing is empty", parse_tracked(""), []),
        ("a real rule text is usable", usable_rule_text("check this"), True),
        ("a whitespace-only rule text is not", usable_rule_text("   "), False),
        ("an absent rule text is not", usable_rule_text(None), False),
        ("a non-string rule text is not", usable_rule_text(7), False),
        ("an exact path covers itself", covers("Cargo.lock", "Cargo.lock"), True),
        ("an exact path does not cover a sibling", covers("Cargo.lock", "Cargo.lock.bak"), False),
        (
            "a trailing slash covers a file under it",
            covers("baselines/", "baselines/trust.yml"),
            True,
        ),
        (
            "a trailing slash is a boundary, not a prefix",
            covers("baselines/", "baselines-old/x"),
            False,
        ),
        (
            "an allow-listed prefix admits a file under it",
            allowed("bench/corpus/tier2-positives-v1-captures/x.json"),
            True,
        ),
        ("a path no entry names is not allow-listed", allowed("src/canon.rs"), False),
        ("a repeated path is reported once", duplicate_paths(["a", "b", "a"]), ["a"]),
        ("distinct paths are not reported", duplicate_paths(["a", "b"]), []),
        ("a well-formed entry is read", _shape([{"path": "a", "rule": "x"}]), (["a"], 0)),
        ("an entry with no 'path' is reported", _shape([{"rule": "x"}]), ([], 1)),
        ("an entry with no 'rule' is reported", _shape([{"path": "a"}]), (["a"], 1)),
        (
            "an entry with a blank 'rule' is reported",
            _shape([{"path": "a", "rule": "  "}]),
            (["a"], 1),
        ),
        ("an entry that is not an object is reported", _shape(["nonsense"]), ([], 1)),
        (
            "a duplicated path is reported",
            _shape([{"path": "a", "rule": "x"}, {"path": "a", "rule": "y"}]),
            (["a", "a"], 1),
        ),
        (
            "a malformed entry does not hide the others",
            _shape([{"rule": "x"}, {"path": "a", "rule": "y"}]),
            (["a"], 1),
        ),
        (
            "a misspelled top-level key is refused",
            declared_document_problems({"excludes": [], "rules": []}),
            [
                "unknown top-level key 'excludes' — the CLI ignores unknown keys silently, so a "
                "typo is a no-op"
            ],
        ),
        (
            "a rules list plus a usable exclude is clean",
            declared_document_problems({"exclude": ["baselines/*.minisig"], "rules": []}),
            [],
        ),
        (
            "an exclude that is not a list is refused",
            declared_document_problems({"exclude": "baselines/*.minisig", "rules": []}),
            ["'exclude' must be a list of gitignore-style patterns"],
        ),
        (
            "a blank exclude pattern is refused",
            declared_document_problems({"exclude": ["  "], "rules": []}),
            ["'exclude' holds a value that is not a usable pattern: '  '"],
        ),
        (
            "a document that is not an object is refused",
            declared_document_problems([]),
            ["the document is not a JSON object"],
        ),
        (
            "a well-formed preview has no shape problem",
            preview_shape_problems(
                {
                    "reviewable_files": [{"path": "src/keep.rs"}],
                    "excluded_files": [{"path": "baselines/x.json.minisig"}],
                }
            ),
            [],
        ),
        (
            "a preview that is a JSON array is refused, not indexed",
            preview_shape_problems([]),
            ["the preview is not a JSON object — the CLI's JSON shape changed"],
        ),
        (
            "a preview with no reviewable list is refused",
            preview_shape_problems({"excluded_files": []}),
            ["the preview has no reviewable_files list — the CLI's JSON shape changed"],
        ),
        (
            "a preview entry with no string path is refused",
            preview_shape_problems({"reviewable_files": [{"path": 7}], "excluded_files": []}),
            [
                "the preview's reviewable_files entries have no string path — the CLI's JSON "
                "shape changed"
            ],
        ),
        (
            "a preview whose excluded list is not a list is refused",
            preview_shape_problems({"reviewable_files": [], "excluded_files": "none"}),
            ["the preview has no excluded_files list — the CLI's JSON shape changed"],
        ),
        (
            "a preview entry that is not an object is refused",
            preview_shape_problems({"reviewable_files": [1], "excluded_files": []}),
            [
                "the preview's reviewable_files entries have no string path — the CLI's JSON "
                "shape changed"
            ],
        ),
        (
            "a preview missing both lists reports both, and the caller joins them",
            preview_shape_problems({}),
            [
                "the preview has no reviewable_files list — the CLI's JSON shape changed",
                "the preview has no excluded_files list — the CLI's JSON shape changed",
            ],
        ),
    ]
    failures = 0
    for label, got, want in rows:
        ok = got == want
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'} {label:<48} {got!r}")
    print(f"  {len(rows)} case(s), {failures} failure(s)")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="check review/rule.json against the tree")
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="table-drive the pure checks; run no review",
    )
    args = parser.parse_args()
    if args.self_check:
        return self_check()

    if shutil.which("ocr") is None:
        print("SKIP  ocr not installed; cannot verify review/rule.json globs")
        return 3
    try:
        document = json.loads(RULE_FILE.read_text(encoding="utf-8"))
    except Exception as exc:  # unreadable or malformed rule file
        print(f"FAIL  cannot read {RULE_FILE.relative_to(ROOT)}: {exc}")
        return 1
    # A document that is not `{"rules": [...]}` is our file being wrong, not the CLI's: the
    # shape is settled here so the reader below can never meet a `null` or a string, and so
    # a malformed rule file is reported as ours instead of as an `ocr` exit nobody chose.
    problems = declared_document_problems(document)
    if problems:
        for problem in problems:
            print(f"FAIL  {RULE_FILE.relative_to(ROOT)}: {problem}")
        return 1
    rules = document["rules"]
    exclude = document.get("exclude", [])

    # The declared side first: it is what the two directions below can even be about, and a
    # defect in our own file needs no CLI to establish.
    paths, texts, problems = read_entries(rules)
    if problems:
        for problem in problems:
            print(f"FAIL  review/rule.json: {problem}")
        return 1

    # **The declared blind spot.** A signed artefact's review is its signature, and `ocr` cannot
    # read a `.minisig`: the exclusion is explicit here and the `baselines/**` checklist tells the
    # reviewer so, or the next signing PR is flagged for a signature the reviewer was never shown.
    # Exact, not "contains": an exclusion is how content is made invisible to review, so adding a
    # pattern is a decision that must touch this check rather than arrive as a quiet edit.
    if list(exclude) != [SIGNATURE_EXCLUDE]:
        print(
            f"FAIL  review/rule.json: the exclude list must be exactly [{SIGNATURE_EXCLUDE!r}] — "
            "an exclusion hides content from review, so changing it is a decision, not an edit"
        )
        return 1
    # **The order is the attribution.** `ocr delegate rule` (below) runs before the effect probe
    # (at the end): that step is where a document the CLI cannot use is attested as OUR failure.
    # By the time the probe runs, the document has already been accepted, so a non-zero preview
    # exit cannot be attributed to it — the honest class is UNVERIFIABLE (3), because an unknown
    # flag, a crash or a throwaway repository that broke is not a finding about review/rule.json.
    try:
        tracked = parse_tracked(
            subprocess.run(
                ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, text=True, check=True
            ).stdout
        )
    except (subprocess.CalledProcessError, OSError, UnicodeDecodeError) as exc:
        # "cannot answer", whether git failed, is not installed at all (OSError covers a
        # missing `git`), or listed a path this decode cannot represent: none of those is a
        # CalledProcessError, and each would otherwise escape as a traceback with exit 1 —
        # reporting an absent tool, or an unrepresentable filename, as a finding about the
        # tree. Every other exit in the contract is unchanged.
        git_stderr = getattr(exc, "stderr", "") or ""
        print(f"SKIP  git ls-files cannot answer: {(git_stderr or str(exc)).strip()[:200]}")
        return 3
    if not tracked:
        print("SKIP  no tracked files to match against")
        return 3

    proc = subprocess.run(
        ["ocr", "delegate", "rule", "--format", "json", "--rule", str(RULE_FILE), *tracked],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(f"FAIL  ocr delegate rule exited {proc.returncode}: {proc.stderr.strip()[:200]}")
        return 1
    try:
        groups = json.loads(proc.stdout)["groups"]
    except Exception as exc:
        print(f"FAIL  could not read ocr output: {exc}")
        return 3

    # The declaration must reach the reviewer for the **manifest**, whatever spelling the entry's
    # path uses: assert on the text the CLI resolved for that file, not on the rule file's key.
    manifest = "baselines/air-gapped.json"
    resolved_group = next(
        (group for group in groups if manifest in (group.get("files") or [])), None
    )
    if resolved_group is None:
        print(f"FAIL  review/rule.json: {manifest} resolves to no rule")
        return 1
    if resolved_group.get("source") != "custom":
        print(
            f"FAIL  review/rule.json: {manifest} falls to the built-in rubric "
            f"({resolved_group.get('source')}) — the signature declaration reaches nobody"
        )
        return 1
    declared = resolved_group.get("rule") or ""
    for phrase, what in (
        (SIGNATURE_DECLARATION, "declare that the signature file is outside the reviewer's view"),
        (SIGNATURE_INSTRUCTION, "carry the instruction not to report the missing signature"),
    ):
        if phrase not in declared:
            print(
                f"FAIL  review/rule.json: the checklist resolved for {manifest} must {what} "
                f"({phrase!r} is missing) — the reviewer reports a missing signature otherwise"
            )
            return 1

    # Direction 1, rules -> files: a custom group with files is the only proof that an entry
    # took effect. The CLI emits an entry whose glob matches nothing just the same; it simply
    # never appears with files.
    resolved = {
        group["pattern"]: group.get("rule")
        for group in groups
        if group.get("source") == "custom" and group.get("files")
    }
    dead = [path for path in paths if path not in resolved]
    for pattern in dead:
        print(f"FAIL  review/rule.json entry matches no tracked file: {pattern}")

    # The text this entry will actually be judged by. It equals the declared text on all of
    # today's entries (measured), so a difference is a fact about the CLI — a truncated or
    # normalised checklist is a checklist silently lost — and the remedy is to re-establish
    # that fact at pin time, never to relax this comparison.
    mismatched = [
        pattern
        for pattern in paths
        if pattern not in dead and resolved[pattern] != texts[pattern]
    ]
    for pattern in mismatched:
        got, want = resolved[pattern], texts[pattern]
        print(
            f"FAIL  review/rule.json entry {pattern!r} did not reach the reviewer verbatim: "
            f"declared {len(want)} chars, the CLI resolved {len(got or '')}"
        )

    # Direction 2, files -> rules: every tracked path resolves to our rubric or is a listed
    # exception. A path the CLI sends to `default` is a path judged by the vendor checklist,
    # which must be a decision someone made here rather than an accident.
    covered = set()
    for group in groups:
        if group.get("source") == "custom":
            covered.update(group.get("files") or [])
    uncovered = [path for path in tracked if path not in covered and not allowed(path)]
    for path in uncovered:
        print(
            "FAIL  tracked file is left to the built-in rubric and is not allow-listed: "
            f"{path}"
        )

    # A stale entry is an exception nobody needs any more, and it is worse than clutter: it
    # would silently excuse the next uncovered file at exactly that path.
    stale = [entry for entry in ALLOW_LIST if not entry_covers(entry, tracked)]
    for entry in stale:
        print(f"FAIL  allow-list entry matches no tracked file: {entry}")

    if dead or mismatched or uncovered or stale:
        return 1

    # Spelling is not effect: ask the CLI whether it acts on the exclusion. The **status** is
    # authoritative — 1 means a measured defect, 3 means the measurement could not be made — and
    # the problem list is checked as well, because status 0 with problems is exactly the split
    # this contract used to have: a caller that trusted the code alone would report a pass over
    # a defect, and a measured problem must never be able to pass because a status said 0.
    status, selection = selection_problems(document)
    if status == 3:
        for problem in selection:
            print(f"UNVERIFIABLE  {problem}")
        return 3
    if status != 0 or selection:
        for problem in selection or [
            "the selection check measured a defect without naming it — a status-only pass"
        ]:
            print(f"FAIL  review/rule.json: {problem}")
        return 1

    print(
        f"review/rule.json: {len(paths)} entries match at least one tracked file with their "
        f"text verbatim, and all {len(tracked)} tracked files resolve to a custom rule or one "
        f"of {len(ALLOW_LIST)} allow-listed exceptions"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
