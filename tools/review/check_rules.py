#!/usr/bin/env python3
"""Check that every rule in review/rule.json still matches real files.

Why this exists: the review CLI **silently ignores** a rule whose field names it does
not recognise, and equally silently falls back to the built-in rubric for a `path` glob
that matches nothing. A renamed directory would therefore turn an entry into a no-op
that still "loads" — the review would quietly lose that checklist, and every exit code
would stay zero. So the *effect* is checked here, not the exit status.

This is a check of a file we own, so a missing `ocr` is UNVERIFIABLE (exit 3), never a
pass: the same three-state rule as the rest of the gate.

Exit codes: 0 every entry matched at least one tracked file; 1 an entry matched none;
3 could not answer (no `ocr`, no tracked files, or unparseable CLI output).
"""
import json
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
RULE_FILE = ROOT / "review" / "rule.json"


def main() -> int:
    if shutil.which("ocr") is None:
        print("SKIP  ocr not installed; cannot verify review/rule.json globs")
        return 3
    try:
        rules = json.loads(RULE_FILE.read_text(encoding="utf-8"))["rules"]
    except Exception as exc:  # unreadable or malformed rule file
        print(f"FAIL  cannot read {RULE_FILE.relative_to(ROOT)}: {exc}")
        return 1

    tracked = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
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

    # A custom group with files is the only proof that an entry took effect.
    matched = {
        group["pattern"]
        for group in groups
        if group.get("source") == "custom" and group.get("files")
    }
    dead = [rule["path"] for rule in rules if rule["path"] not in matched]
    for pattern in dead:
        print(f"FAIL  review/rule.json entry matches no tracked file: {pattern}")
    if dead:
        return 1

    print(f"review/rule.json: all {len(rules)} entries match at least one tracked file")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
