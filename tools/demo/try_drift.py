#!/usr/bin/env python3
"""Try what drift does, in one command (unit P-1; ADR 0040, ADR 0052).

The README's third command. It tightens one requirement of req's own corpus, REQ-0010, on a throw-away copy and shows what
`req satisfy` says about the part of the design that implements it:

    python3 tools/demo/try_drift.py [--req <path>]

What it does, in order:
  1. reads the tightening edit from `docs/architecture/drift/edits.json` (the same edit the guided page and the README table show);
  2. copies what `req satisfy` reads (`claims`, `aliases`, `.req`, `src`, `decisions`, `baselines`) to a temporary directory;
  3. applies the edit to the claim file IN THE COPY (the statement is replaced by the edit's `after`);
  4. runs `req satisfy` in the copy and prints its lines: the header, every link that does not hold, the verdict, and a count
     of the links that still hold (the `[MENTION]` lines, comments that merely name the token, are left out);
  5. deletes the copy.

It never opens this checkout for writing: the only file it changes is a claim file inside the temporary copy. It reads no
network. Python standard library only. `req` is taken from `--req`, else `target/release/req` of this checkout (what
`cargo build --release` makes), else `req` on PATH. An older `req` cannot read today's claims (`req.claim/3`), so build one.

The exit code is about the demonstration, not about req's verdict (`req satisfy` itself exits 3 for any suspect link):
  0  the demonstration ran and `req satisfy` gave the outcome the edit expects: SUSPECT for `src/bench.rs -> REQ-0010`;
  1  it ran and `req satisfy` gave a different outcome for that link: the demonstration did not show what it claims to;
  2  usage;
  3  UNVERIFIABLE, nothing was shown: `req` cannot be run or did not answer, the edit does not apply to the claim as committed,
     or the copy cannot be made. Never folded into 1 (a link that held is an answer; a run that never happened is not one).
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EDITS = os.path.join(ROOT, "docs", "architecture", "drift", "edits.json")
EDITS_SCHEMA = "req.drift-edits/1"
EDIT_ID = "tightening"
# What `req satisfy` reads: the corpus, the registry, the recorded links, the design files, the decisions and the baselines.
COPY_TREES = ("claims", "aliases", ".req", "src", "decisions", "baselines")
# `req satisfy`'s outcome label -> the word `edits.json` keys it by.
OUTCOMES = {"HOLDS": "holds", "HOLDS (weakened)": "holds_weakened", "SUSPECT": "suspect"}
LINK_LINE = re.compile(r"^\[(?P<outcome>[^\]]+)\] (?P<element>\S+) -> (?P<claim>[^\s:]+)")


class Unverifiable(Exception):
    """Nothing was shown: `req` cannot be run, the edit does not apply, or the copy cannot be made (exit 3)."""


def find_req(argument):
    """The `req` to run: `--req`, else this checkout's release build, else PATH."""
    if argument is not None:
        # `req` runs inside the copy, so a path given relative to here must be made absolute first.
        return os.path.abspath(argument) if os.sep in argument else (shutil.which(argument) or argument)
    built = os.path.join(ROOT, "target", "release", "req")
    if os.path.isfile(built):
        return built
    found = shutil.which("req")
    if found is None:
        raise Unverifiable("no `req` to run: build one with `cargo build --release`, or pass --req <path>")
    return found


def load_edit():
    try:
        with open(EDITS, encoding="utf-8") as handle:
            document = json.load(handle)
    except (OSError, ValueError) as why:
        raise Unverifiable(f"{EDITS} cannot be read: {why}")
    if document.get("schema") != EDITS_SCHEMA:
        raise Unverifiable(f"{EDITS} is not a {EDITS_SCHEMA} document")
    edit = next((e for e in document.get("edits", []) if e.get("id") == EDIT_ID), None)
    if edit is None:
        raise Unverifiable(f"{EDITS} has no edit {EDIT_ID!r}")
    return edit


def apply_edit(copy, edit):
    """Replace the statement of the edit's claim in the claim file under `copy` (a temporary copy, never this checkout)."""
    path = os.path.join(copy, "claims", edit["claim_id"] + ".yml")
    try:
        with open(path, encoding="utf-8") as handle:
            lines = handle.read().split("\n")
    except OSError as why:
        raise Unverifiable(f"claims/{edit['claim_id']}.yml cannot be read: {why}")
    starts = [i for i, line in enumerate(lines) if line == "statement: >-"]
    if len(starts) != 1:
        raise Unverifiable(f"claims/{edit['claim_id']}.yml has no folded `statement: >-` block this script can edit")
    end = starts[0] + 1
    while end < len(lines) and lines[end].startswith("  "):
        end += 1
    current = " ".join(line.strip() for line in lines[starts[0] + 1:end])
    if current != edit["before"]:
        raise Unverifiable(f"the edit does not apply: {edit['claim_id']} reads {current!r}, not the statement the edit starts from")
    lines[starts[0] + 1:end] = textwrap.wrap(edit["after"], width=78, initial_indent="  ", subsequent_indent="  ",
                                             break_long_words=False, break_on_hyphens=False)
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines))


def run_satisfy(binary, copy):
    """`req --root <copy> satisfy`: its stdout. Exit 0 (all hold) and 3 (a verdict) are answers; anything else is not."""
    env = {k: v for k, v in os.environ.items() if k != "REQ_ROOT"}
    try:
        done = subprocess.run([binary, "--root", copy, "satisfy"], capture_output=True, text=True, check=False, cwd=copy,
                              stdin=subprocess.DEVNULL, timeout=300, env=env)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`{binary} satisfy` could not be run: {why}")
    if done.returncode not in (0, 3):
        raise Unverifiable(f"`{binary} satisfy` exited {done.returncode}: {(done.stderr or done.stdout).strip()}")
    return done.stdout


def show(output, edit):
    """Print what `req satisfy` said, without the noise. Returns the outcome it gave the edit's link, or None."""
    expect = edit["expect"]
    held = 0
    answered = None
    for line in output.splitlines():
        if line.startswith("[MENTION]"):
            continue
        link = LINK_LINE.match(line)
        if link is None:
            print(line)
            continue
        if link.group("element") == expect["element"] and link.group("claim") == edit["claim_id"]:
            answered = OUTCOMES.get(link.group("outcome"), link.group("outcome"))
        if link.group("outcome") == "HOLDS":
            held += 1
            continue
        print(line)
    print(f"# {held} other link(s) hold and are not listed")
    return answered


def demonstrate(binary):
    edit = load_edit()
    with tempfile.TemporaryDirectory(prefix="req-try-drift-") as work:
        copy = os.path.join(work, "copy")
        os.makedirs(copy)
        for tree in COPY_TREES:
            source = os.path.join(ROOT, tree)
            try:
                shutil.copytree(source, os.path.join(copy, tree))
            except OSError as why:
                raise Unverifiable(f"{source} cannot be copied: {why}")
        apply_edit(copy, edit)
        print(f"# {edit['claim_id']} on a throw-away copy: {edit['before']!r}")
        print(f"#   becomes {edit['after']!r}")
        output = run_satisfy(binary, copy)
        answered = show(output, edit)
    # The copy is deleted here; this checkout was never written.
    expected = edit["expect"]["outcome"]
    if answered != expected:
        print(f"# not the demonstration: `req satisfy` said {answered!r} for {edit['expect']['element']} -> {edit['claim_id']}, "
              f"the edit expects {expected!r}", file=sys.stderr)
        return 1
    print("# The copy is deleted; your checkout was not touched.")
    return 0


def main(argv):
    binary = None
    args = argv[1:]
    while args:
        arg = args.pop(0)
        if arg == "--req" and args:
            binary = args.pop(0)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"usage: try_drift.py [--req <path>]  (unknown argument {arg!r})", file=sys.stderr)
            return 2
    try:
        return demonstrate(find_req(binary))
    except Unverifiable as why:
        print(f"UNVERIFIABLE: {why}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv))
