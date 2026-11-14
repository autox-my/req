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

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CONVERTER = os.path.join(ROOT, "tools", "graph", "arch_overlay.py")
SATISFY_SCHEMA = "req.satisfy/1"
TRACE_SCHEMA = "req.trace/1"
# (the architect's diagram, the overlay and the matrix made from it, the page archify renders the overlay to as the matrix
# reaches it from docs/architecture/), the overview first.
VIEWS = (
    ("req-overview.archify.json", "overview-overlay.json", "overview-matrix.md", "../architecture.html"),
    ("req.archify.json", "overlay.json", "matrix.md", "../architecture-detail.html"),
)
# Where the committed matrices are read from, so the converter writes their claim links relative to it.
MATRIX_DIR = "docs/architecture"
GENERATED = tuple(name for _, overlay, matrix, _ in VIEWS for name in (overlay, matrix))


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


def main(argv):
    mode, directory, binary = None, None, None
    args = iter(argv)
    for arg in args:
        if arg in ("--out", "--check"):
            mode, directory = arg, next(args, None)
        elif arg == "--req":
            binary = next(args, None)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"usage: build_architecture.py [--req <path>] --out <dir> | --check <dir> (unknown: {arg})",
                  file=sys.stderr)
            return 2
    if mode is None or not directory or binary == "":
        print("usage: build_architecture.py [--req <path>] --out <dir> | --check <dir>", file=sys.stderr)
        return 2
    binary = binary or shutil.which("req")
    if binary is None:
        print("UNVERIFIABLE: `req` is not on PATH (and no --req was given); the overlay is computed from its "
              "real output and cannot be built or checked without it", file=sys.stderr)
        return 3
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
