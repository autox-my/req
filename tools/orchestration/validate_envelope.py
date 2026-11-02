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
  (AGENTS.md, "Mutation-check the load-bearing rule"). Well formed whenever present, and a fix
  unit must carry it too, exactly like `red`: both are what a fix unit's claim to have tested
  the load-bearing rule rests on.

The published schema (docs/specs/task-envelope.schema.json) states the same contract, and
`--self-check` compares the two (`schema_disagreements`), so a rule only one of them states
fails the gate. The validator is the stricter authority; the summary's line cap is enforced
here and described, not encoded, in the schema.
"""
from __future__ import annotations

import json
import pathlib
import re
import sys
import tempfile

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


def is_fix_unit(doc: dict) -> bool:
    task_id = doc.get("task_id")
    return isinstance(task_id, str) and bool(FIX_TASK.search(task_id))


def validate_red(doc: dict, errors: list[str]) -> None:
    red = doc.get("red")
    is_fix = is_fix_unit(doc)

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
        if is_fix_unit(doc):
            errors.append("a fix envelope (task_id ...-fix) requires mutations [{change, test_that_failed}]")
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

    for key in ("task_id", "branch"):
        value = doc.get(key)
        if isinstance(value, str) and not value.strip():
            errors.append(f"{key} must not be empty")

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


SCHEMA = pathlib.Path(__file__).resolve().parents[2] / "docs" / "specs" / "task-envelope.schema.json"


def _conditional(schema: dict, discriminator: tuple[str, str, str]) -> dict | None:
    """The `allOf` entry whose `if` pins `properties.<field>.<keyword>` to a value, or None."""
    field, keyword, value = discriminator
    for entry in schema.get("allOf", []):
        pinned = entry.get("if", {}).get("properties", {}).get(field, {})
        if pinned.get(keyword) == value:
            return entry
    return None


def schema_disagreements(path: pathlib.Path = SCHEMA) -> list[str]:
    """Where the published schema and this validator say different things.

    The validator is the authority — it is what the orchestrator runs — and the schema is the
    published description of the same contract, so a rule only one of them states is a contract
    that silently does not hold. This compares the rules both can express: the required keys,
    the status vocabulary, the identifier and commit lengths, and the three conditional rules
    (fix unit, blocked, review). What a schema cannot express (the summary's line cap) is
    stated in the schema's own description and enforced here only.
    """
    try:
        schema = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return [f"cannot read the schema at {path}: {exc}"]
    out: list[str] = []
    props = schema.get("properties", {})
    if set(schema.get("required", [])) != set(REQUIRED):
        out.append(f"required keys differ: schema {sorted(schema.get('required', []))} vs validator {sorted(REQUIRED)}")
    if set(props.get("status", {}).get("enum", [])) != STATUSES:
        out.append("the status vocabulary differs")
    if props.get("base_commit", {}).get("minLength") != MIN_COMMIT:
        out.append(f"base_commit minLength is not {MIN_COMMIT}")
    for key in ("task_id", "branch"):
        if props.get(key, {}).get("minLength", 0) < 1:
            out.append(f"{key} is not declared non-empty")
    fix = _conditional(schema, ("task_id", "pattern", FIX_TASK.pattern))
    if fix is None or set(fix.get("then", {}).get("required", [])) != {"red", "mutations"}:
        out.append("the fix-unit rule (task_id ...-fix requires red and mutations) is not in the schema")
    blocked = _conditional(schema, ("status", "const", "blocked"))
    if blocked is None or "else" not in blocked or "then" not in blocked:
        out.append("the blocked rule (one question when blocked, null otherwise) is not in the schema")
    review = _conditional(schema, ("task_id", "pattern", "-review-"))
    if review is None or review.get("else", {}).get("properties", {}).get("files_changed", {}).get("minItems") != 1:
        out.append("the review rule (a non-review envelope names at least one file) is not in the schema")
    return out


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
MUT = [{"change": "drop the rule", "test_that_failed": "canon::x"}]

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
    ("fix unit with red but no mutations", _base(task_id="M1-1-fix", red=RED), False),
    ("fix unit with mutations but no red", _base(task_id="M1-1-fix", mutations=MUT), False),
    ("fix unit with red and mutations", _base(task_id="M1-1-fix", red=RED, mutations=MUT), True),
    ("numbered fix unit with red and mutations", _base(task_id="M1-1-fix2", red=RED, mutations=MUT), True),
    ("non-fix unit needs neither", _base(), True),
    ("empty task_id", _base(task_id=""), False),
    ("blank branch", _base(branch="   "), False),
    ("red missing failure", _base(red={"test": "canon::x"}), False),
    ("red with short base_commit", _base(red={**RED, "base_commit": "abc"}), False),
    ("mutations well formed", _base(mutations=[{"change": "drop rule", "test_that_failed": "t"}]), True),
    ("mutations empty list", _base(mutations=[]), False),
    ("mutation missing its test", _base(mutations=[{"change": "drop rule"}]), False),
    ("pr url present", _base(pr="https://git.example/fdx/req/pulls/1"), True),
    ("pr blank", _base(pr=" "), False),
    ("top level not an object", ["not", "an", "object"], False),
]


def _read_cases() -> list[tuple[str, int, int]]:
    """(name, expected exit, got exit) for the file-reading half, in a throwaway directory."""
    import contextlib
    import io

    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        (root / "binary.json").write_bytes(b"\xff\xfe\x00{")
        (root / "notjson.json").write_text("{nope", encoding="utf-8")
        (root / "good.json").write_text(json.dumps(_base()), encoding="utf-8")
        for name, target, expected in (
            ("a non-UTF-8 file is invalid, not a traceback", root / "binary.json", 1),
            ("a file that is not JSON is invalid", root / "notjson.json", 1),
            ("a good file is valid", root / "good.json", 0),
            ("a missing file is an IO error", root / "absent.json", 2),
        ):
            sink = io.StringIO()
            try:
                with contextlib.redirect_stderr(sink), contextlib.redirect_stdout(sink):
                    got = check_file(str(target))
            except Exception as exc:  # a traceback is the failure this case exists to catch
                got = -1
                print(f"      {name}: raised {type(exc).__name__}")
            rows.append((name, expected, got))
    return rows


def self_check() -> int:
    bad = 0
    for name, doc, expected in CASES:
        errors = validate(doc)
        if (not errors) != expected:
            bad += 1
            print(f"FAIL  {name}: expected {'valid' if expected else 'invalid'}, got {errors or 'valid'}")
    reads = _read_cases()
    for name, expected, got in reads:
        if got != expected:
            bad += 1
            print(f"FAIL  {name}: expected exit {expected}, got {got}")
    drift = schema_disagreements()
    for line in drift:
        bad += 1
        print(f"FAIL  schema and validator disagree: {line}")
    total = len(CASES) + len(reads) + 1
    if bad:
        print(f"validate_envelope self-check: {bad} failure(s) over {total} check(s)")
        return 1
    print(f"validate_envelope self-check: {total} check(s) ok")
    return 0


def check_file(path: str | None) -> int:
    """Validate one envelope read from `path` (None means stdin). Returns the exit code.

    1 is reserved for "this is not a valid envelope", which includes bytes that are not UTF-8;
    2 is for a file that cannot be read at all. Neither is ever a traceback.
    """
    try:
        if path is None:
            data = sys.stdin.buffer.read()
        else:
            with open(path, "rb") as handle:
                data = handle.read()
    except OSError as exc:
        print(f"cannot read {path}: {exc}", file=sys.stderr)
        return 2
    try:
        raw = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        print(f"INVALID: not UTF-8: {exc}", file=sys.stderr)
        return 1
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


def main() -> int:
    args = sys.argv[1:]
    if args == ["--self-check"]:
        return self_check()
    if len(args) > 1 or (args and args[0].startswith("-")):
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: validate_envelope.py [path | --self-check]", file=sys.stderr)
        return 2
    return check_file(args[0] if args else None)


if __name__ == "__main__":
    raise SystemExit(main())
