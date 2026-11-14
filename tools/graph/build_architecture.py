#!/usr/bin/env python3
"""Build, or check, the generated half of docs/architecture/ (ADR 0052 decision 7, step 3).

`docs/architecture/req.archify.json` is the diagram an architect drew from the repository (reviewed like a
document, never generated). Everything else here is computed from it, from `req satisfy --format json` and from
`req trace --format json` of THIS repository, by `tools/graph/arch_overlay.py`:

    docs/architecture/overlay.json   the diagram with each component's status and claim ids (archify's input)
    docs/architecture/matrix.md      the claims x components matrix, and the claims no component reaches

Both are byte-for-byte what the converter writes and are never hand-edited. `docs/architecture.html` is archify's
rendering of overlay.json (pinned at 7e45f36); it needs Node and a browser, so this script never runs archify, and
the gate cannot check the page itself (the named gap in ADR 0052's note).

    python3 tools/graph/build_architecture.py [--req <path>] --out <dir>      write the two files into <dir>
    python3 tools/graph/build_architecture.py [--req <path>] --check <dir>    exit 0 current, 1 stale

Python standard library only, offline. `req` is taken from `--req`, else PATH, like tools/overview/build.py. The
corpus is the repository this script lives in (its parent's parent), so a copy of the repository checks itself.

Exit: 0 written / current; 1 stale (the file is named), or the converter refused or reported a stale citation, an
unknown claim or a tag that does not fit (its reason is printed): the page cannot be made honestly; 2 usage;
3 `req` cannot be run, or its output cannot be read (UNVERIFIABLE: nothing was compared).
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CONVERTER = os.path.join(ROOT, "tools", "graph", "arch_overlay.py")
DIAGRAM = "req.archify.json"
GENERATED = ("overlay.json", "matrix.md")


class Unverifiable(Exception):
    """`req` could not be run, or what it printed is not the document the converter reads."""


def req_json(binary, *args):
    """Run `req --root <repo> <args>` and return its stdout document.

    `satisfy` and `trace` exit non-zero for a verdict (a suspect link, an undecided edge) and still print the
    complete document; what matters here is that stdout is that document, so only a document that does not
    parse is unverifiable."""
    env = {k: v for k, v in os.environ.items() if k != "REQ_ROOT"}
    try:
        done = subprocess.run([binary, "--root", ROOT, *args], capture_output=True, text=True, check=False,
                              stdin=subprocess.DEVNULL, timeout=300, env=env)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`req {' '.join(args)}` could not be run: {why}")
    try:
        json.loads(done.stdout)
    except ValueError:
        raise Unverifiable(f"`req {' '.join(args)}` exited {done.returncode} without a JSON document: "
                           f"{done.stderr.strip()}")
    return done.stdout


def generate(binary, diagram, into):
    """Write overlay.json and matrix.md for `diagram` into the (new) directory `into`.

    Returns None when written, or the converter's reason when it refused or flagged the page."""
    with tempfile.TemporaryDirectory(prefix="req-architecture-in-") as work:
        satisfy = os.path.join(work, "satisfy.json")
        trace = os.path.join(work, "trace.json")
        for path, args in ((satisfy, ("satisfy", "--format", "json")), (trace, ("trace", "--format", "json"))):
            with open(path, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(req_json(binary, *args))
        done = subprocess.run([sys.executable, "-I", CONVERTER, "--root", ROOT, diagram, satisfy, trace, into],
                              capture_output=True, text=True, check=False, stdin=subprocess.DEVNULL, timeout=300)
    if done.returncode != 0:
        return f"arch_overlay.py exited {done.returncode}: {done.stderr.strip() or done.stdout.strip()}"
    return None


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
    diagram = os.path.join(directory, DIAGRAM)
    if not os.path.isfile(diagram):
        print(f"{diagram} cannot be read: the architect's diagram is not here", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory(prefix="req-architecture-out-") as scratch:
        built = os.path.join(scratch, "built")
        try:
            reason = generate(binary, diagram, built)
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
                  "docs/architecture.html again (examples/graph/README.md)", file=sys.stderr)
            return 1
        for name in GENERATED:
            print(f"[CURRENT] {os.path.join(directory, name)}")
        return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
