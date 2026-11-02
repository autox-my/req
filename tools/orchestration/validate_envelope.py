#!/usr/bin/env python3
"""Validate a worker's task envelope against docs/specs/task-envelope.schema.json.

A *task envelope* is the only accepted final message of a delegated worker or reviewer (see
AGENTS.md, "Orchestration"). It is process tooling, unrelated to the Baseline manifest's
`baseline_envelope` version: the word is shared, the objects are not.

Dependency-free on purpose: jsonschema is not installed here, and the gate must not need a
network to check it. Ported from the PanduMY repository's validator, unchanged in behaviour
except that it also accepts an optional `pr` string (this repository merges by pull request).

Usage:
    python3 tools/orchestration/validate_envelope.py path/to/envelope.json
    cat envelope.json | python3 tools/orchestration/validate_envelope.py
    python3 tools/orchestration/validate_envelope.py --self-check

Exit 0 = valid, 1 = invalid, 2 = usage/IO error. `--self-check` exits 0 when every table
case answers as recorded, 1 otherwise.

Beyond the required fields, the two TDD evidence fields are checked:

- `red` {test, base_commit, failure}: the red-at-base proof. It must be well formed whenever it
  is present, and a fix unit must carry it: `task_id` ends in `-fix` (or a numbered `-fix1`).
- `mutations` [{change, test_that_failed}]: named mutations and the test each must turn red
  (AGENTS.md, "Mutation-check the load-bearing rule").
"""
from __future__ import annotations

import json
import re
import sys

REQUIRED = {
    "task_id": str,
    "status": str,
    "branch": str,
    "base_commit": str,
    "files_changed": list,
    "tests": str,
    "diff_stat": str,
    "committed": bool,
    "blockers": (str, type(None)),
    "summary": str,
}
STATUSES = {"done", "blocked", "failed"}
SUMMARY_MAX_LINES = 10
MIN_COMMIT = 7
# `M1-1-fix`, `M1-1-fix2`: a fix unit carries the red proof that its test had teeth.
FIX_TASK = re.compile(r"-fix[0-9]*$")


def validate_red(doc: dict, errors: list[str]) -> None:
    red = doc.get("red")
    task_id = doc.get("task_id")
    is_fix = isinstance(task_id, str) and bool(FIX_TASK.search(task_id))

    if red is None:
        if is_fix:
            errors.append("a fix envelope (task_id ...-fix) requires red {test, base_commit, failure}")
        return
    if not isinstance(red, dict):
        errors.append("red must be an object {test, base_commit, failure}")
        return

    for key in ("test", "failure"):
        value = red.get(key)
        if not isinstance(value, str) or not value.strip():
            errors.append(f"red.{key} must be a non-empty string")

    base = red.get("base_commit")
    if base is not None and (not isinstance(base, str) or len(base.strip()) < MIN_COMMIT):
        errors.append(f"red.base_commit must be at least {MIN_COMMIT} characters when present")


def validate_mutations(doc: dict, errors: list[str]) -> None:
    mutations = doc.get("mutations")
    if mutations is None:
        return
    if not isinstance(mutations, list) or not mutations:
        errors.append("mutations must be a non-empty array of {change, test_that_failed}")
        return
    for index, mutation in enumerate(mutations):
        if not isinstance(mutation, dict):
            errors.append(f"mutations[{index}] must be an object")
            continue
        for key in ("change", "test_that_failed"):
            value = mutation.get(key)
            if not isinstance(value, str) or not value.strip():
                errors.append(f"mutations[{index}].{key} must be a non-empty string")


def validate(doc: object) -> list[str]:
    errors: list[str] = []
    if not isinstance(doc, dict):
        return ["top level must be a JSON object"]

    for key, typ in REQUIRED.items():
        if key not in doc:
            errors.append(f"missing key: {key}")
            continue
        value = doc[key]
        # bool is a subclass of int; reject it where str is expected, and vice versa
        if typ is bool:
            if not isinstance(value, bool):
                errors.append(f"{key} must be a boolean")
        elif isinstance(typ, tuple):
            if not isinstance(value, typ):
                errors.append(f"{key} must be a string or null")
        elif not isinstance(value, typ):
            errors.append(f"{key} must be {typ.__name__}")

    base = doc.get("base_commit")
    if isinstance(base, str) and len(base.strip()) < MIN_COMMIT:
        errors.append(f"base_commit must be at least {MIN_COMMIT} characters")

    status = doc.get("status")
    if isinstance(status, str) and status not in STATUSES:
        errors.append(f"status must be one of {sorted(STATUSES)}")

    files = doc.get("files_changed")
    if isinstance(files, list):
        if not all(isinstance(x, str) for x in files):
            errors.append("files_changed must contain only strings")
        task_id = doc.get("task_id")
        is_review = isinstance(task_id, str) and "-review-" in task_id
        if not files and not is_review:
            errors.append("files_changed must not be empty (review envelopes may be empty)")

    blockers = doc.get("blockers")
    if status == "blocked":
        if not (isinstance(blockers, str) and blockers.strip()):
            errors.append("status blocked requires a non-empty blockers string")
        else:
            text = blockers.strip()
            if "\n" in text or "\r" in text:
                errors.append("blockers must be a single line")
            if text.count("?") != 1 or not text.endswith("?"):
                errors.append("blockers must be exactly one question ending in '?'")
    elif blockers is not None:
        errors.append("blockers must be null unless status is blocked")

    summary = doc.get("summary")
    if isinstance(summary, str):
        lines = [ln for ln in summary.strip().splitlines() if ln.strip()]
        if len(lines) > SUMMARY_MAX_LINES:
            errors.append(f"summary must be <= {SUMMARY_MAX_LINES} lines, got {len(lines)}")
        if not lines:
            errors.append("summary must not be empty")

    pr = doc.get("pr")
    if pr is not None and (not isinstance(pr, str) or not pr.strip()):
        errors.append("pr must be a non-empty string (the pull request URL) when present")

    validate_red(doc, errors)
    validate_mutations(doc, errors)

    # unknown keys are allowed, but the contract ones above are fixed
    return errors


def _base(**over) -> dict:
    doc = {
        "task_id": "M1-1",
        "status": "done",
        "branch": "feature/m1-identity-coverage",
        "base_commit": "b56bca3",
        "files_changed": ["src/canon.rs"],
        "tests": "scripts/check.sh exit 0 (full run)",
        "diff_stat": "+10/-2 across 1 file",
        "committed": True,
        "blockers": None,
        "summary": "one line",
    }
    doc.update(over)
    return doc


def _without(key: str) -> dict:
    doc = _base()
    del doc[key]
    return doc


RED = {"test": "canon::x", "base_commit": "b56bca3", "failure": "assertion failed"}

# (name, document, expected valid). Each invalid case breaks exactly one rule.
CASES = [
    ("minimal worker envelope", _base(), True),
    ("review envelope may flag no files", _base(task_id="M1-review-R1", files_changed=[]), True),
    ("worker envelope must name a file", _base(files_changed=[]), False),
    ("missing required key", _without("summary"), False),
    ("committed must be a real boolean", _base(committed=1), False),
    ("unknown status", _base(status="green"), False),
    ("short base_commit", _base(base_commit="b56"), False),
    ("blocked needs one question", _base(status="blocked", blockers="Which corpus?"), True),
    ("blocked without a question mark", _base(status="blocked", blockers="need a corpus"), False),
    ("blocked with two questions", _base(status="blocked", blockers="A? or B?"), False),
    ("blockers set while done", _base(blockers="Why?"), False),
    ("summary over ten lines", _base(summary="\n".join(["x"] * 11)), False),
    ("empty summary", _base(summary="   "), False),
    ("fix unit without red", _base(task_id="M1-1-fix"), False),
    ("numbered fix unit without red", _base(task_id="M1-1-fix2"), False),
    ("fix unit with red", _base(task_id="M1-1-fix", red=RED), True),
    ("red missing failure", _base(red={"test": "canon::x"}), False),
    ("red with short base_commit", _base(red={**RED, "base_commit": "abc"}), False),
    ("mutations well formed", _base(mutations=[{"change": "drop rule", "test_that_failed": "t"}]), True),
    ("mutations empty list", _base(mutations=[]), False),
    ("mutation missing its test", _base(mutations=[{"change": "drop rule"}]), False),
    ("pr url present", _base(pr="https://git.example/fdx/req/pulls/1"), True),
    ("pr blank", _base(pr=" "), False),
    ("top level not an object", ["not", "an", "object"], False),
]


def self_check() -> int:
    bad = 0
    for name, doc, expected in CASES:
        errors = validate(doc)
        if (not errors) != expected:
            bad += 1
            print(f"FAIL  {name}: expected {'valid' if expected else 'invalid'}, got {errors or 'valid'}")
    if bad:
        print(f"validate_envelope self-check: {bad} of {len(CASES)} case(s) wrong")
        return 1
    print(f"validate_envelope self-check: {len(CASES)} case(s) ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args == ["--self-check"]:
        return self_check()
    if len(args) > 1 or (args and args[0].startswith("-")):
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: validate_envelope.py [path | --self-check]", file=sys.stderr)
        return 2
    if args:
        try:
            with open(args[0], "r", encoding="utf-8") as handle:
                raw = handle.read()
        except OSError as exc:
            print(f"cannot read {args[0]}: {exc}", file=sys.stderr)
            return 2
    else:
        raw = sys.stdin.read()

    try:
        doc = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(f"INVALID: not JSON: {exc}", file=sys.stderr)
        return 1

    errors = validate(doc)
    if errors:
        for error in errors:
            print(f"INVALID: {error}", file=sys.stderr)
        return 1

    print("VALID")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
