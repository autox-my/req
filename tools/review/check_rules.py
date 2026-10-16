#!/usr/bin/env python3
"""Check both directions between review/rule.json and the tracked tree.

Why this exists: the review CLI **silently ignores** a rule whose field names it does
not recognise, and equally silently falls back to the built-in rubric for a `path` glob
that matches nothing. A renamed directory would therefore turn an entry into a no-op
that still "loads" — the review would quietly lose that checklist, and every exit code
would stay zero. So the *effect* is checked here, not the exit status.

Two directions, because only one of them was ever checked. **Rules → files**: every
entry in the rule file must still match at least one tracked file, or it is a silent
no-op. **Files → rules**: every tracked file must resolve to `source: custom`, or be
named in ALLOW_LIST below with its reason — otherwise the vendor rubric judges it by
accident, which is exactly how `build.rs`, `decisions/DEC-0001.yml` and `tools/req-d1.sh`
sat on the generic checklist while every exit code stayed zero.

The matcher is deliberately **not** re-implemented here. `fnmatch` does not implement
gitignore `**` (an attempt reported 58 uncovered paths where the CLI resolves 16), and a
second implementation is free to drift from the one that decides — the defect
`src/funnel.rs` already records for its tokenizer. Resolution is always
`ocr delegate rule`, the same CLI the review uses.

This is a check of a file we own, so a missing `ocr` is UNVERIFIABLE (exit 3), never a
pass: the same three-state rule as the rest of the gate.

Exit codes: 0 every entry matched at least one tracked file AND every tracked file
resolved to a custom rule or a listed exception; 1 a real gap (an entry matching no
tracked file, a file left to the built-in rubric, or an allow-list entry that has gone
stale); 3 could not answer (no `ocr`, no tracked files, or unparseable CLI output).
"""
import json
import pathlib
import shutil
import subprocess
import sys

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
}


def allowed(path: str) -> bool:
    """True when `path` is an allow-listed exact path or sits under an allow-listed prefix."""
    if path in ALLOW_LIST:
        return True
    return any(entry.endswith("/") and path.startswith(entry) for entry in ALLOW_LIST)


def entry_covers(entry: str, tracked: list) -> bool:
    """True when an allow-list entry still names something in the tree."""
    if entry.endswith("/"):
        return any(path.startswith(entry) for path in tracked)
    return entry in set(tracked)


def main() -> int:
    if shutil.which("ocr") is None:
        print("SKIP  ocr not installed; cannot verify review/rule.json globs")
        return 3
    try:
        rules = json.loads(RULE_FILE.read_text(encoding="utf-8"))["rules"]
    except Exception as exc:  # unreadable or malformed rule file
        print(f"FAIL  cannot read {RULE_FILE.relative_to(ROOT)}: {exc}")
        return 1

    try:
        tracked = subprocess.run(
            ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
        ).stdout.split()
    except subprocess.CalledProcessError as exc:  # git cannot list the tree: cannot answer
        print(f"SKIP  git ls-files cannot answer: {exc.stderr.strip()[:200]}")
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

    # Direction 1, rules -> files: a custom group with files is the only proof that an entry
    # took effect. The CLI emits an entry whose glob matches nothing just the same; it simply
    # never appears with files.
    matched = {
        group["pattern"]
        for group in groups
        if group.get("source") == "custom" and group.get("files")
    }
    dead = [rule["path"] for rule in rules if rule["path"] not in matched]
    for pattern in dead:
        print(f"FAIL  review/rule.json entry matches no tracked file: {pattern}")

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

    if dead or uncovered or stale:
        return 1

    print(
        f"review/rule.json: {len(rules)} entries match at least one tracked file, and all "
        f"{len(tracked)} tracked files resolve to a custom rule or one of "
        f"{len(ALLOW_LIST)} allow-listed exceptions"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
