#!/usr/bin/env python3
"""Build, or check, the generated half of docs/architecture/ (ADR 0052 decision 7, step 3).

There are two diagrams an architect drew from the repository (reviewed like a document, never generated): the
overview, `docs/architecture/req-overview.archify.json` (at most 12 components, the front page), and the detail,
`docs/architecture/req.archify.json` (every part). Everything else here is computed from them, from
`req satisfy --format json` and from `req trace --format json` of THIS repository, by `tools/graph/arch_overlay.py`:

    docs/architecture/overview-overlay.json   the overview with each component's status line (archify's input)
    docs/architecture/overview-matrix.md      the coverage matrix of the overview (ADR 0053)
    docs/architecture/overlay.json            the detail with each component's status line (archify's input)
    docs/architecture/matrix.md               the coverage matrix of the detail (ADR 0053)

The matrices are four columns (Source | Claim | Satisfied by | Verified by) and carry no test outcome: this script never
passes the converter a `--test-report`, so each is a function of the repository (ADR 0053 decision 5). All four are byte-for-byte what the converter writes and are never hand-edited. `docs/architecture.html` (the overview)
and `docs/architecture-detail.html` are archify's renderings of the two overlays (pinned at 7e45f36); they need Node and
a browser, so this script never runs archify, and the gate cannot check the pages themselves (the named gap in ADR
0052's note).

    python3 tools/graph/build_architecture.py [--req <path>] --out <dir>      write the four files into <dir>
    python3 tools/graph/build_architecture.py [--req <path>] --check <dir>    exit 0 current, 1 stale
    python3 tools/graph/build_architecture.py [--req <path>] --drift <dir>    write the drift demonstration into <dir>
    python3 tools/graph/build_architecture.py [--req <path>] --check-drift <dir>  exit 0 current, 1 stale

The drift demonstration (ADR 0052 decision 7, step 4, unit O-6) is a second generated set, under
`docs/architecture/drift/`. `edits.json` there names edits to one of this repository's own claims (a cosmetic re-wording, a
loosening, a tightening: statement before, statement after, the outcome `req satisfy` is expected to give). Each edit is
applied to a FRESH TEMPORARY COPY of the corpus, never to this checkout: `req satisfy` and `req trace` run in the copy, and the
converter writes, for the overview diagram and that copy's output, `<edit-id>/overlay.json` and `<edit-id>/matrix.md`, and this
script then prepends the matrix's "made from a temporary copy" line and points the edited claim's link at `../edits.json`;
`<edit-id>/satisfy.txt` is the exact text `req satisfy` printed, what an engineer would see. A directory or file that no edit
produces (beside `edits.json` and the README image `req-drift.png`) is STALE: `--drift` writes the set and still exits 1 until
it is deleted, and `--check-drift` names it. `docs/architecture-drift.html` is
archify's rendering of the tightening overlay. `--check-drift` regenerates everything and compares; the copies are deleted.

Python standard library only, offline. `req` is taken from `--req`, else PATH, like tools/overview/build.py. The
corpus is the repository this script lives in (its parent's parent), so a copy of the repository checks itself.

Exit: 0 written / current; 1 stale (the file is named), or the converter reported a stale citation, an
unknown claim or a status line or tag that does not fit (its reason is printed): the page cannot be made honestly; 2 usage;
3 UNVERIFIABLE, nothing was compared or written: `req` cannot be run, or did not print the complete `req.satisfy/1` and
`req.trace/1` documents (a schema it was not written against, or a file `req satisfy` could not read), or the converter
refused its input (a regenerate cannot fix that).
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CONVERTER = os.path.join(ROOT, "tools", "graph", "arch_overlay.py")
SATISFY_SCHEMA = "req.satisfy/1"
TRACE_SCHEMA = "req.trace/1"
# Where the pages are published (the URL README.md links). A matrix is read on github.com, where a relative link to an
# `.html` file opens its source, so each part links to the published page by this absolute URL (ADR 0053 decision 6).
PAGES_URL = "https://autox-my.github.io/req"
# (the architect's diagram, the overlay and the matrix made from it, the published page the overlay is rendered to), the
# overview first.
VIEWS = (
    ("req-overview.archify.json", "overview-overlay.json", "overview-matrix.md", f"{PAGES_URL}/architecture.html"),
    ("req.archify.json", "overlay.json", "matrix.md", f"{PAGES_URL}/architecture-detail.html"),
)
# Where the committed matrices are read from, so the converter writes their claim links relative to it.
MATRIX_DIR = "docs/architecture"
DRIFT_DIR = "docs/architecture/drift"
GENERATED = tuple(name for _, overlay, matrix, _ in VIEWS for name in (overlay, matrix))


# The drift demonstration (O-6). The overview is drawn from the real repository and is never edited: a copy of the corpus is.
DRIFT_SCHEMA = "req.drift-edits/1"
DRIFT_EDITS = "edits.json"
DRIFT_DIAGRAM = os.path.join(ROOT, "docs", "architecture", "req-overview.archify.json")
DRIFT_PAGE_FILE = "docs/architecture-drift.html"
DRIFT_PAGE = f"{PAGES_URL}/architecture-drift.html"
# What a copy holds: everything `req satisfy` and `req trace` read. The cited files are read under the real repository.
DRIFT_COPY = ("claims", "aliases", ".req", "src", "decisions", "baselines")
DRIFT_FILES = ("overlay.json", "matrix.md", "satisfy.txt")
DRIFT_OUTCOMES = ("holds", "holds_weakened", "suspect")
# The one edit `docs/architecture-drift.html` is rendered from; only its matrix links components to that page, so a HOLDS row
# of another edit never points at a SUSPECT picture.
DRIFT_PAGE_EDIT = "tightening"


class Unverifiable(Exception):
    """`req` could not be run, or what it printed is not the document the converter reads."""


def req_json(binary, schema, *args):
    """Run `req --root <repo> <args>` and return its stdout document, if it is the complete document `schema` names.

    `satisfy` and `trace` exit non-zero for a verdict (a suspect link, an undecided edge) and still print the complete
    document, so the exit code is not what is judged. What is judged is the document: it must carry `schema`, and a
    `satisfy` document must list nothing `unreadable` (a file req could not read has no links in it, so the overlay
    would draw its parts as NO CLAIM). Anything else is unverifiable, naming what req reported."""
    env = {k: v for k, v in os.environ.items() if k != "REQ_ROOT"}
    command = f"req {' '.join(args)}"
    try:
        done = subprocess.run([binary, "--root", ROOT, *args], capture_output=True, text=True, check=False,
                              stdin=subprocess.DEVNULL, timeout=300, env=env)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`{command}` could not be run: {why}")
    try:
        document = json.loads(done.stdout)
    except ValueError:
        raise Unverifiable(f"`{command}` exited {done.returncode} without a JSON document: {done.stderr.strip()}")
    if not isinstance(document, dict) or document.get("schema") != schema:
        found = document.get("schema") if isinstance(document, dict) else type(document).__name__
        raise Unverifiable(f"`{command}` printed schema {found!r}, not {schema!r} (exit {done.returncode}): "
                           "this build reads only the schema it was written against")
    if schema == SATISFY_SCHEMA:
        unreadable = document.get("unreadable")
        if not isinstance(unreadable, list):
            raise Unverifiable(f"`{command}` printed no `unreadable` list (exit {done.returncode})")
        if unreadable:
            raise Unverifiable(f"`{command}` reported what it could not read (exit {done.returncode}), so its links "
                               "are incomplete: " + "; ".join(str(item) for item in unreadable))
    return done.stdout


def generate(binary, directory, into):
    """Write the overlay and the matrix of each diagram in `directory` into the (new) directory `into`.

    Returns None when written, or the reason the page cannot be made honestly (the converter's exit 3: a stale citation,
    an unknown claim, a tag with no room). A converter that refused its input (exit 2) is Unverifiable: regenerating
    cannot fix an input it will not read. Every diagram is converted before the first reason is returned, so one run
    reports them all."""
    reasons = []
    os.makedirs(into)
    with tempfile.TemporaryDirectory(prefix="req-architecture-in-") as work:
        satisfy = os.path.join(work, "satisfy.json")
        trace = os.path.join(work, "trace.json")
        for path, schema, args in ((satisfy, SATISFY_SCHEMA, ("satisfy", "--format", "json")),
                                   (trace, TRACE_SCHEMA, ("trace", "--format", "json"))):
            with open(path, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(req_json(binary, schema, *args))
        for name, overlay, matrix, page in VIEWS:
            diagram = os.path.join(directory, name)
            if not os.path.isfile(diagram):
                raise Unverifiable(f"{diagram} cannot be read: the architect's diagram is not here")
            one = os.path.join(work, "one-" + overlay)
            done = subprocess.run([sys.executable, "-I", CONVERTER, "--root", ROOT, "--matrix-dir", MATRIX_DIR, "--page", page, diagram, satisfy, trace, one],
                                  capture_output=True, text=True, check=False, stdin=subprocess.DEVNULL, timeout=300)
            said = done.stderr.strip() or done.stdout.strip()
            if done.returncode not in (0, 3):
                raise Unverifiable(f"arch_overlay.py exited {done.returncode} and wrote nothing for {name}: {said}")
            if done.returncode == 3:
                reasons.append(f"arch_overlay.py exited 3 for {name}: {said}")
            for written, final in (("overlay.json", overlay), ("matrix.md", matrix)):
                shutil.copyfile(os.path.join(one, written), os.path.join(into, final))
    return "\n".join(reasons) or None


def load_edits(directory):
    """Read `<directory>/edits.json`: a list of edits, each `{id, kind, claim_id, before, after, expect}`.

    A shape the build does not read is Unverifiable (regenerating cannot fix a hand-edited file)."""
    path = os.path.join(directory, DRIFT_EDITS)
    try:
        with open(path, encoding="utf-8") as handle:
            document = json.load(handle)
    except (OSError, ValueError) as why:
        raise Unverifiable(f"{path} cannot be read: {why}")
    if not isinstance(document, dict) or document.get("schema") != DRIFT_SCHEMA:
        raise Unverifiable(f"{path} is not a {DRIFT_SCHEMA} document")
    edits = document.get("edits")
    if not isinstance(edits, list) or not edits:
        raise Unverifiable(f"{path} lists no edits")
    seen = set()
    for edit in edits:
        if not isinstance(edit, dict):
            raise Unverifiable(f"{path}: an edit is not an object")
        for key in ("id", "kind", "claim_id", "before", "after"):
            if not isinstance(edit.get(key), str) or not edit[key]:
                raise Unverifiable(f"{path}: an edit has no `{key}`")
        expect = edit.get("expect")
        if (not isinstance(expect, dict) or expect.get("outcome") not in DRIFT_OUTCOMES
                or not isinstance(expect.get("element"), str) or not expect["element"]):
            raise Unverifiable(f"{path}: edit {edit['id']!r} has no `expect` with an element and one of {DRIFT_OUTCOMES}")
        if "direction" in expect and not isinstance(expect["direction"], str):
            raise Unverifiable(f"{path}: edit {edit['id']!r} has a `direction` that is not a string")
        if not edit["id"].replace("-", "").isalnum() or edit["id"] != edit["id"].lower() or edit["id"] in seen:
            raise Unverifiable(f"{path}: edit id {edit['id']!r} is not a unique lower-case word (it names a directory)")
        seen.add(edit["id"])
    return edits


def apply_edit(copy, edit):
    """Change the statement of `edit['claim_id']` in the claim file under `copy` (a temporary copy, never this checkout).

    Returns None when done, or the reason the edit does not apply (the statement is not the one `before` names)."""
    path = os.path.join(copy, "claims", edit["claim_id"] + ".yml")
    try:
        with open(path, encoding="utf-8") as handle:
            lines = handle.read().split("\n")
    except OSError as why:
        raise Unverifiable(f"claims/{edit['claim_id']}.yml cannot be read: {why}")
    starts = [i for i, line in enumerate(lines) if line == "statement: >-"]
    if len(starts) != 1:
        raise Unverifiable(f"claims/{edit['claim_id']}.yml has no folded `statement: >-` block this build can edit")
    end = starts[0] + 1
    while end < len(lines) and lines[end].startswith("  "):
        end += 1
    current = " ".join(line.strip() for line in lines[starts[0] + 1:end])
    if current != edit["before"]:
        return (f"edit {edit['id']!r}: `before` is not the statement of {edit['claim_id']} as committed "
                f"(it reads {current!r})")
    wrapped = textwrap.wrap(edit["after"], width=78, initial_indent="  ", subsequent_indent="  ",
                            break_long_words=False, break_on_hyphens=False)
    lines[starts[0] + 1:end] = wrapped
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines))
    return None


def run_req(binary, cwd, *args):
    """`req --root . <args>` run in `cwd` (a relative root, so no machine path reaches the output): (code, stdout)."""
    env = {k: v for k, v in os.environ.items() if k != "REQ_ROOT"}
    command = f"req {' '.join(args)}"
    try:
        done = subprocess.run([binary, "--root", ".", *args], capture_output=True, text=True, check=False, cwd=cwd,
                              stdin=subprocess.DEVNULL, timeout=300, env=env)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`{command}` could not be run: {why}")
    return done.returncode, done.stdout, done.stderr


def check_document(command, schema, code, out, err):
    """The complete document `schema` names, or Unverifiable: as `req_json`, for output already captured."""
    try:
        document = json.loads(out)
    except ValueError:
        raise Unverifiable(f"`{command}` exited {code} without a JSON document: {err.strip()}")
    if not isinstance(document, dict) or document.get("schema") != schema:
        raise Unverifiable(f"`{command}` printed another schema than {schema!r} (exit {code})")
    if schema == SATISFY_SCHEMA:
        if not isinstance(document.get("unreadable"), list) or document["unreadable"]:
            raise Unverifiable(f"`{command}` could not read everything (exit {code}), so its links are incomplete")
    return document


def drift_diagram(edit, line):
    """The overview diagram for one edit: the architect's file with a demonstration card, a title and a subtitle added.

    Components, connections and geometry are the architect's, untouched; the converter reads this and writes the overlay.
    A card is plain text in archify, so the page names the real one by file name."""
    with open(DRIFT_DIAGRAM, encoding="utf-8") as handle:
        diagram = json.load(handle)
    diagram["meta"]["title"] = "req drift demo"
    diagram["meta"]["output"] = DRIFT_PAGE_FILE
    diagram["meta"]["subtitle"] = (f"a demonstration on a copy of the corpus: {edit['claim_id']} is {edit['kind']}ed on the "
                                   "copy, the real claims are not changed")
    diagram["cards"] = [{
        "dot": "amber",
        "title": "A demonstration, on a copy",
        "items": [
            f"{edit['claim_id']} was edited on a throw-away copy of the corpus ({edit['id']}); the real claims are not changed, "
            "and architecture.html is the real state, where every link holds",
            f"It read \"{edit['before']}\"",
            f"On the copy it reads \"{edit['after']}\"",
            f"What req satisfy prints for it: {line}",
        ],
    }] + diagram["cards"]
    return diagram


def drift_matrix_header(edit, has_page):
    """The one line above a drift matrix: what it is made from, where its edited claim links, and whether parts link."""
    parts = ("the parts link to the drift page" if has_page else
             "the parts are named without a link, since no page is rendered for this edit")
    return (f"Made from a temporary copy of the corpus with the edit `{edit['id']}` applied to {edit['claim_id']}, never on "
            f"req's own claims; {edit['claim_id']} links to [{DRIFT_EDITS}](../{DRIFT_EDITS}), which holds its statement "
            f"before and after, not to claims/{edit['claim_id']}.yml, which holds the unedited one; {parts}.")


def generate_drift(binary, directory, into):
    """Write each edit's overlay, matrix and `req satisfy` text into the (new) directory `into`, one subdirectory per edit.

    Returns None when written, or the reasons the demonstration cannot be made honestly (an edit that no longer applies,
    one whose outcome is not the one it expects, a converter exit 3). Every edit is tried before the first reason is
    returned. Each edit runs on its own temporary copy, deleted after."""
    reasons = []
    os.makedirs(into)
    for edit in load_edits(directory):
        with tempfile.TemporaryDirectory(prefix="req-architecture-drift-") as work:
            copy = os.path.join(work, "copy")
            os.makedirs(copy)
            for tree in DRIFT_COPY:
                source = os.path.join(ROOT, tree)
                if not os.path.isdir(source):
                    raise Unverifiable(f"{source} is not here: the copy of the corpus cannot be made")
                shutil.copytree(source, os.path.join(copy, tree))
            reason = apply_edit(copy, edit)
            if reason is not None:
                reasons.append(reason)
                continue
            code, text, _ = run_req(binary, copy, "satisfy")
            if code not in (0, 3):
                raise Unverifiable(f"`req satisfy` exited {code} on the copy for edit {edit['id']!r}")
            documents = {}
            for schema, args in ((SATISFY_SCHEMA, ("satisfy", "--format", "json")), (TRACE_SCHEMA, ("trace", "--format", "json"))):
                code, out, err = run_req(binary, copy, *args)
                check_document(f"req {' '.join(args)}", schema, code, out, err)
                path = os.path.join(work, schema.split("/")[0] + ".json")
                with open(path, "w", encoding="utf-8", newline="\n") as handle:
                    handle.write(out)
                documents[schema] = (path, json.loads(out))
            expect = edit["expect"]
            links = [link for link in documents[SATISFY_SCHEMA][1]["links"]
                     if link.get("element") == expect["element"] and link.get("claim_id") == edit["claim_id"]]
            if len(links) != 1:
                reasons.append(f"edit {edit['id']!r}: req satisfy lists {len(links)} links from {expect['element']} to "
                               f"{edit['claim_id']}, not one")
                continue
            link = links[0]
            if link.get("outcome") != expect["outcome"] or ("direction" in expect and link.get("direction") != expect["direction"]):
                # Said, and still written: the check then names every file that no longer matches what req says.
                reasons.append(f"edit {edit['id']!r}: expected {expect['outcome']}"
                               + (f" ({expect['direction']})" if "direction" in expect else "")
                               + f", req satisfy says {link.get('outcome')} ({link.get('direction')})")
            line = next((l for l in text.splitlines() if f" {expect['element']} -> {edit['claim_id']}" in l), None)
            if line is None:
                raise Unverifiable(f"`req satisfy` printed no line for {expect['element']} -> {edit['claim_id']}")
            diagram = os.path.join(work, "diagram.json")
            with open(diagram, "w", encoding="utf-8", newline="\n") as handle:
                json.dump(drift_diagram(edit, line), handle, indent=2, ensure_ascii=False)
                handle.write("\n")
            one = os.path.join(work, "one")
            has_page = edit["id"] == DRIFT_PAGE_EDIT
            done = subprocess.run([sys.executable, "-I", CONVERTER, "--root", ROOT, "--matrix-dir",
                                   f"{DRIFT_DIR}/{edit['id']}", *(("--page", DRIFT_PAGE) if has_page else ()), diagram,
                                   documents[SATISFY_SCHEMA][0], documents[TRACE_SCHEMA][0], one],
                                  capture_output=True, text=True, check=False, stdin=subprocess.DEVNULL, timeout=300)
            said = done.stderr.strip() or done.stdout.strip()
            if done.returncode not in (0, 3):
                raise Unverifiable(f"arch_overlay.py exited {done.returncode} and wrote nothing for edit {edit['id']!r}: {said}")
            if done.returncode == 3:
                reasons.append(f"arch_overlay.py exited 3 for edit {edit['id']!r}: {said}")
            target = os.path.join(into, edit["id"])
            os.makedirs(target)
            shutil.copyfile(os.path.join(one, "overlay.json"), os.path.join(target, "overlay.json"))
            with open(os.path.join(one, "matrix.md"), encoding="utf-8") as handle:
                matrix = handle.read()
            claim_link = f"[{edit['claim_id']}](../../../../claims/{edit['claim_id']}.yml)"
            if matrix.count(claim_link) != 1:
                raise Unverifiable(f"the matrix of edit {edit['id']!r} does not link {edit['claim_id']} exactly once as "
                                   f"`{claim_link}`: the converter's format is not the one this build reads")
            with open(os.path.join(target, "matrix.md"), "w", encoding="utf-8", newline="\n") as handle:
                handle.write(drift_matrix_header(edit, has_page) + "\n\n"
                             + matrix.replace(claim_link, f"[{edit['claim_id']}](../{DRIFT_EDITS})"))
            with open(os.path.join(target, "satisfy.txt"), "w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
    return "\n".join(reasons) or None


# The files the drift directory holds beside the edit directories: the edits (input) and the README image (made by hand from
# archify's visual-check, ADR 0052's O-6 note). Anything else at the root is stale.
DRIFT_ROOT_FILES = ("edits.json", "req-drift.png")


def drift_orphans(directory, edits):
    """What sits under the drift directory that no edit in edits.json produces, named: an edit renamed or removed leaves its
    directory behind, and a committed directory nobody regenerates would still read as current."""
    known = {edit["id"] for edit in edits}
    orphans = []
    for entry in sorted(os.listdir(directory)):
        path = os.path.join(directory, entry)
        if not os.path.isdir(path):
            if entry not in DRIFT_ROOT_FILES:
                orphans.append(f"STALE {path} (not a file the drift directory holds: delete it)")
            continue
        if os.path.isdir(path):
            if entry not in known:
                orphans.append(f"STALE {path} (edits.json names no edit `{entry}`: delete it)")
                continue
            for name in sorted(os.listdir(path)):
                if name not in DRIFT_FILES:
                    orphans.append(f"STALE {os.path.join(path, name)} (not a file this build writes: delete it)")
    return orphans


def main_drift(mode, directory, binary):
    """`--drift <dir>` writes, `--check-drift <dir>` compares: the O-3 exit contract."""
    with tempfile.TemporaryDirectory(prefix="req-architecture-drift-out-") as scratch:
        built = os.path.join(scratch, "built")
        try:
            edits = load_edits(directory)
            reason = generate_drift(binary, directory, built)
        except Unverifiable as why:
            print(f"UNVERIFIABLE: {why}", file=sys.stderr)
            return 3
        names = [f"{edit['id']}/{name}" for edit in edits for name in DRIFT_FILES]
        if mode == "--drift":
            if reason is not None:
                print(f"the demonstration cannot be made honestly: {reason}", file=sys.stderr)
                return 1
            orphans = drift_orphans(directory, edits)
            for name in names:
                os.makedirs(os.path.join(directory, os.path.dirname(name)), exist_ok=True)
                shutil.copyfile(os.path.join(built, name), os.path.join(directory, name))
                print(f"wrote {os.path.join(directory, name)}")
            for line in orphans:
                print(line, file=sys.stderr)
            return 1 if orphans else 0
        stale = drift_orphans(directory, edits)
        for name in names:
            shown = os.path.join(directory, name)
            built_path = os.path.join(built, name)
            if not os.path.isfile(built_path):
                stale.append(f"STALE {shown} (its edit did not apply)")
                continue
            try:
                with open(shown, "rb") as handle:
                    current = handle.read()
            except OSError as why:
                stale.append(f"{shown} cannot be read: {why}")
                continue
            with open(built_path, "rb") as handle:
                if current != handle.read():
                    stale.append(f"STALE {shown}")
        if reason is not None:
            print(f"the demonstration cannot be made honestly: {reason}", file=sys.stderr)
        for line in stale:
            print(line, file=sys.stderr)
        if reason is not None or stale:
            print(f"regenerate with `python3 tools/graph/build_architecture.py --drift {directory}`, then render "
                  f"{DRIFT_PAGE_FILE} again from {directory}/tightening/overlay.json (examples/graph/README.md)", file=sys.stderr)
            return 1
        for name in names:
            print(f"[CURRENT] {os.path.join(directory, name)}")
        return 0


def main(argv):
    mode, directory, binary = None, None, None
    args = iter(argv)
    for arg in args:
        if arg in ("--out", "--check", "--drift", "--check-drift"):
            mode, directory = arg, next(args, None)
        elif arg == "--req":
            binary = next(args, None)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"usage: build_architecture.py [--req <path>] --out <dir> | --check <dir> | --drift <dir> | "
                  f"--check-drift <dir> (unknown: {arg})",
                  file=sys.stderr)
            return 2
    if mode is None or not directory or binary == "":
        print("usage: build_architecture.py [--req <path>] --out <dir> | --check <dir> | --drift <dir> | --check-drift <dir>",
              file=sys.stderr)
        return 2
    binary = os.path.abspath(binary) if binary else shutil.which("req")
    if binary is None:
        print("UNVERIFIABLE: `req` is not on PATH (and no --req was given); the overlay is computed from its "
              "real output and cannot be built or checked without it", file=sys.stderr)
        return 3
    if mode in ("--drift", "--check-drift"):
        return main_drift(mode, directory, binary)
    missing = [os.path.join(directory, name) for name, _, _, _ in VIEWS if not os.path.isfile(os.path.join(directory, name))]
    if missing:
        for path in missing:
            print(f"{path} cannot be read: the architect's diagram is not here", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory(prefix="req-architecture-out-") as scratch:
        built = os.path.join(scratch, "built")
        try:
            reason = generate(binary, directory, built)
        except Unverifiable as why:
            print(f"UNVERIFIABLE: {why}", file=sys.stderr)
            return 3
        if reason is not None:
            print(f"the page cannot be made honestly: {reason}", file=sys.stderr)
            return 1
        if mode == "--out":
            for name in GENERATED:
                shutil.copyfile(os.path.join(built, name), os.path.join(directory, name))
                print(f"wrote {os.path.join(directory, name)}")
            return 0
        stale = []
        for name in GENERATED:
            try:
                with open(os.path.join(directory, name), "rb") as handle:
                    current = handle.read()
            except OSError as why:
                stale.append(f"{os.path.join(directory, name)} cannot be read: {why}")
                continue
            with open(os.path.join(built, name), "rb") as handle:
                if current != handle.read():
                    stale.append(f"STALE {os.path.join(directory, name)}")
        if stale:
            for line in stale:
                print(line, file=sys.stderr)
            print(f"regenerate with `python3 tools/graph/build_architecture.py --out {directory}`, then render "
                  "docs/architecture.html and docs/architecture-detail.html again (examples/graph/README.md)",
                  file=sys.stderr)
            return 1
        for name in GENERATED:
            print(f"[CURRENT] {os.path.join(directory, name)}")
        return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
