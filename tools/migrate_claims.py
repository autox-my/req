#!/usr/bin/env python3
"""tools/migrate_claims.py — rewrite `req.claim/1` claim files as `req.claim/2` (ADR 0022).

A starting point an adopter copies and adjusts, like `tools/adopt.py` — not a req subcommand.

    python3 tools/migrate_claims.py [--write] [<claims dir or file> ...]   (default: claims/)
    python3 tools/migrate_claims.py --self-check

**Dry run by default**: it prints, per file, what it keeps, what it drops and the pins it would
create, and writes nothing. `--write` rewrites the files in place. It refuses a file that is
already `req.claim/2` (or declares any schema other than `req.claim/1`), and it never touches a
file it refused.

What a migration does (docs/claim-format.md, "Migrating from req.claim/1"):

* keeps `schema` (rewritten to `req.claim/2`), `claim_id`, `statement`, `applicability`,
  `relations`, `tests`, `evidence` and `rationale` **byte for byte** — the text of each kept
  block is copied, never re-serialized, so the rationale object, the folded statement and every
  comment inside a kept block survive exactly;
* turns the authored **quantity-shaped** slots into `pins:` — a numeric `value` (with the
  `operator` authored beside it, `EQ` included: an equality on a pinned value is load-bearing,
  and `exactly zero` must not weaken to `at most zero` unflagged), an `operator` other than
  `EQ`/`none`, a `unit` other than `none` (the cases ADR 0014 made a flag when the parse missed
  them, so a bound the author wrote is still checked against the parse). An operator that is
  not a `req.claim/2` comparator is refused, never carried into a pin the loader would reject;
* drops every other `req.claim/1` key and **prints each drop**: `modality`, `subject`,
  `attribute`, `conditions`, `exception`, `temporal`, `quantifier`, `unstructured` (the parse
  derives them), the qualitative `attribute`/`value` triple (not representable in the form, and
  the annotation category is removed), and the stored `canonical`/`address` (computed at load).

No YAML library is used (none is installed here, and none is needed): the files are split into
top-level key blocks by indentation, and the few scalar slots the pins read are parsed as the
plain or quoted scalars `req.claim/1` wrote. A file this tool cannot split is refused, never
guessed at.

Exit: 0 every file migrated (or would be, in a dry run), 1 a file was refused, 2 usage.
"""

import decimal
import os
import re
import sys

V1 = "req.claim/1"
V2 = "req.claim/2"

# Copied block for block (the schema line is rewritten).
KEPT = (
    "schema",
    "claim_id",
    "statement",
    "applicability",
    "relations",
    "tests",
    "evidence",
    "rationale",
)
# The req.claim/1 keys req.claim/2 removed (src/model.rs REMOVED_V1_KEYS).
REMOVED = (
    "modality",
    "subject",
    "attribute",
    "operator",
    "value",
    "unit",
    "conditions",
    "exception",
    "temporal",
    "quantifier",
    "unstructured",
    "canonical",
    "address",
)
# The comparators `req.claim/2` accepts in a pin (src/model.rs PIN_OPERATORS).
PIN_OPERATORS = ("LT", "LE", "EQ", "GE", "GT", "NE")
TOP_KEY = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*):(\s|$)")
DECIMAL = re.compile(r"^[+-]?(\d+(\.\d*)?|\.\d+)$")


class Refused(Exception):
    """A file this tool will not migrate, with the reason."""


# --- the pure half (--self-check table-drives all of it) --------------------


def split_blocks(text):
    """Split a claim file into `(key, lines)` blocks, in file order.

    A block starts at a top-level `key:` line and owns every following line that is indented,
    blank, a column-0 sequence item (`- x`), or a comment — until the next top-level key. A
    comment above a key belongs to the key it precedes (it is carried over to that block), so a
    comment about a dropped slot is dropped with it. Lines before the first key (a header
    comment) are a block of their own with key `None`.
    """
    blocks = []
    pending = []  # comment/blank lines waiting for the key they precede
    current_key, current = None, []
    for line in text.splitlines(keepends=True):
        bare = line.rstrip("\r\n")
        match = TOP_KEY.match(bare)
        if match:
            if current_key is not None or current:
                blocks.append((current_key, current))
            current_key, current = match.group(1), pending + [line]
            pending = []
        elif bare.startswith("#") and current_key is not None:
            # A column-0 comment after a block's content: hold it for the next key.
            pending.append(line)
        elif bare == "" and current_key is not None and pending:
            pending.append(line)
        elif bare.startswith((" ", "\t", "-")) or bare == "":
            if pending:
                # Indented content after a held comment: it was the block's, after all.
                current.extend(pending)
                pending = []
            current.append(line)
        elif current_key is None and bare.startswith("#"):
            current.append(line)
        else:
            raise Refused(f"cannot split the line {bare!r}: not a top-level key, an indented "
                          "line, a sequence item or a comment")
    if current_key is not None or current:
        blocks.append((current_key, current + pending))
    elif pending:
        blocks.append((None, pending))
    return blocks


def scalar(lines):
    """The plain or quoted scalar a one-line `key: value` block carries, or None for null."""
    # A comment above the key belongs to its block (split_blocks), so find the key line first and
    # judge only what follows it.
    index = next(i for i, l in enumerate(lines) if TOP_KEY.match(l.rstrip("\r\n")))
    if any(l.strip() and not l.lstrip().startswith("#") for l in lines[index + 1:]):
        raise Refused(f"expected a one-line scalar, got a block: {''.join(lines)!r}")
    head = lines[index].rstrip("\r\n")
    raw = head.split(":", 1)[1].strip()
    if raw[:1] in ("\"", "'"):
        # Quoted first: a ` #` inside the quotes is part of the string, and only a comment after
        # the closing quote is dropped.
        quote = raw[0]
        end = raw.find(quote, 1)
        while quote == "\"" and end > 0 and raw[end - 1] == "\\":
            end = raw.find(quote, end + 1)
        if end < 0:
            raise Refused(f"an unterminated quoted scalar: {raw!r}")
        rest = raw[end + 1:].strip()
        if rest and not rest.startswith("#"):
            raise Refused(f"text after a quoted scalar: {raw!r}")
        body = raw[1:end]
        if quote == "\"":
            body = body.replace("\\\"", "\"").replace("\\\\", "\\")
        return body
    if " #" in raw:
        raw = raw.split(" #", 1)[0].strip()
    if raw in ("", "null", "~"):
        return None
    return raw


# Letter- or underscore-first only: a digit-first scalar (`1`, `1.0`, `1e3`) reads back as a number.
PLAIN_SAFE = re.compile(r"^[A-Za-z_][A-Za-z0-9_./-]*$")
YAML_WORDS = {"y", "n", "yes", "no", "true", "false", "on", "off", "null", "~"}


def yaml_scalar(text):
    """`text` as a YAML scalar that reads back as the same string: plain when that is safe,
    otherwise double-quoted with escapes (`%`, `@`, a backtick, ` #`, `: ` and the like are not
    legal in a plain scalar)."""
    if PLAIN_SAFE.match(text) and text.lower() not in YAML_WORDS:
        return text
    escaped = text.replace("\\", "\\\\").replace("\"", "\\\"")
    return f'"{escaped}"'


def quote_value(text):
    """The canonical decimal, always double-quoted (a number must stay a string)."""
    return f'"{text}"'


def canonical_decimal(text):
    """The canonical exact decimal string req writes (`"0.3"`, never `"0.30"`), or None."""
    if not DECIMAL.match(text):
        return None
    # A context wide enough for the input: the default 28 digits would round a longer bound
    # silently, and a pin that is not the authored bound is a guess.
    digits = len(text.replace(".", "").lstrip("+-").lstrip("0")) + 2
    with decimal.localcontext() as context:
        context.prec = max(28, digits)
        value = decimal.Decimal(text)
        if value == value.to_integral_value():
            return str(value.quantize(decimal.Decimal(1)))
        return format(value.normalize(), "f")


def migrate_text(text):
    """`req.claim/1` text → (`req.claim/2` text, report lines). Raises Refused."""
    blocks = split_blocks(text)
    keys = [key for key, _ in blocks if key is not None]
    for key in keys:
        if key not in KEPT and key not in REMOVED and key != "pins":
            raise Refused(f"carries `{key}`, which is not a req.claim/1 key")
    if len(set(keys)) != len(keys):
        raise Refused("declares a key twice")
    by_key = {key: lines for key, lines in blocks if key is not None}
    if "schema" not in by_key:
        raise Refused("declares no `schema:`")
    schema = scalar(by_key["schema"])
    if schema == V2:
        raise Refused(f"is already {V2}: nothing to migrate")
    if schema != V1:
        raise Refused(f"declares {schema!r}: this tool migrates {V1} only")
    if "pins" in by_key:
        raise Refused(f"declares `pins:`, which {V1} does not have")

    def value_of(key):
        return scalar(by_key[key]) if key in by_key else None

    report = []
    pins = []
    value, operator, unit = value_of("value"), value_of("operator"), value_of("unit")
    numeric = canonical_decimal(value) if value is not None else None
    if numeric is not None:
        pins.append(("value", numeric))
        if numeric != value:
            report.append(f"value {value!r} canonicalized to {numeric!r}")
    if operator not in (None, "", "none") and (operator != "EQ" or numeric is not None):
        if operator not in PIN_OPERATORS:
            raise Refused(
                f"`operator: {operator}` is not a req.claim/2 comparator "
                f"({', '.join(PIN_OPERATORS)}): re-word the statement and decide the pin by hand"
            )
        pins.append(("operator", operator))
    if unit not in (None, "", "none"):
        if unit != unit.strip():
            raise Refused(
                f"`unit: {unit!r}` has leading or trailing space, which req.claim/2 refuses in "
                "a pin: fix the unit by hand first"
            )
        pins.append(("unit", unit))

    out = []
    dropped = []
    for key, lines in blocks:
        if key is None or key in KEPT:
            if key == "schema":
                # A comment above `schema:` rides in this block (split_blocks): rewrite the key's
                # own line, not whatever comes first.
                at = next(i for i, l in enumerate(lines) if TOP_KEY.match(l.rstrip("\r\n")))
                lines = lines[:at] + [re.sub(r"req\.claim/1", V2, lines[at], count=1)] + lines[at + 1:]
            out.extend(lines)
            continue
        shown = describe_drop(key, lines, pinned={k for k, _ in pins})
        dropped.append(shown)
    if out and not out[-1].endswith("\n"):
        out[-1] += "\n"
    if pins:
        out.append("pins:\n")
        for key, pinned in pins:
            written = quote_value(pinned) if key == "value" else yaml_scalar(pinned)
            out.append(f"  {key}: {written}\n")
    report.insert(0, "dropped " + "; ".join(dropped) if dropped else "dropped nothing")
    report.append(
        "pins: " + ", ".join(f"{k} {v}" for k, v in pins) if pins else "pins: none"
    )
    return "".join(out), report


def describe_drop(key, lines, pinned):
    """One dropped key, as the report names it."""
    if key in ("canonical", "address"):
        return f"{key} (derived; computed at load)"
    try:
        value = scalar(lines)
    except Refused:
        at = next(i for i, l in enumerate(lines) if TOP_KEY.match(l.rstrip("\r\n")))
        body = [l.strip().removeprefix("- ") for l in lines[at + 1:] if l.strip()]
        value = "[" + ", ".join(body) + "]"
    if key in pinned:
        return f"{key} {value} (now a pin)"
    shown = "null" if value is None else value
    return f"{key} {shown}"


# --- the impure half --------------------------------------------------------


def targets_of(paths):
    """Every claim file under the given paths, sorted: a directory contributes its REQ-*.yml."""
    found = []
    for path in paths:
        if os.path.isdir(path):
            for name in sorted(os.listdir(path)):
                if re.fullmatch(r"REQ-\d+\.yml", name):
                    found.append(os.path.join(path, name))
        elif os.path.isfile(path):
            found.append(path)
        else:
            raise Refused(f"{path}: no such file or directory")
    return found


def run(paths, write):
    try:
        files = targets_of(paths)
    except Refused as why:
        print(f"refused: {why}", file=sys.stderr)
        return 1
    if not files:
        print("refused: no claim files found", file=sys.stderr)
        return 1
    refused = 0
    planned = []
    for path in files:
        with open(path, encoding="utf-8", newline="") as handle:
            text = handle.read()
        try:
            migrated, report = migrate_text(text)
        except Refused as why:
            print(f"{path}: REFUSED — {why}")
            refused += 1
            continue
        print(f"{path}:")
        for line in report:
            print(f"  {line}")
        planned.append((path, migrated))
    if refused:
        print(f"{refused} file(s) refused; nothing written", file=sys.stderr)
        return 1
    if write:
        done = 0
        for path, migrated in planned:
            try:
                # Sibling temp file, then an atomic replace: a failure mid-write leaves the
                # original claim intact, never a truncated one.
                temp = f"{path}.migrating"
                with open(temp, "w", encoding="utf-8", newline="") as handle:
                    handle.write(migrated)
                os.replace(temp, path)
            except OSError as why:
                try:
                    os.remove(f"{path}.migrating")
                except OSError:
                    pass
                left = [p for p, _ in planned[done:]]
                print(f"{path}: cannot write ({why}); {done} of {len(planned)} file(s) were "
                      "migrated before it. A directory re-run would refuse those as already "
                      f"req.claim/2, so name the remaining file(s) explicitly: {' '.join(left)}",
                      file=sys.stderr)
                return 1
            done += 1
        print(f"migrated {len(planned)} file(s) to {V2}")
    else:
        print(f"dry run: {len(planned)} file(s) would be migrated to {V2}; nothing written "
              "(add --write)")
    return 0


V1_QUANTITY = """\
schema: req.claim/1
claim_id: REQ-0009
statement: >-
  The benchmark harness shall classify direction of change on the structural
  direction gold set with accuracy of at least 0.95.
modality: SHALL
subject: BENCH
attribute: direction_accuracy
operator: GE
value: "0.95"
unit: none
conditions: []
applicability:
  - set == direction_gold_structural
exception: null
temporal: null
quantifier: null
unstructured: false
tests:
  - "req::evidence::the_direction_gold_set_meets_its_accuracy_target"
evidence: machine
canonical:
  rules_v: 6
  subject_registry: 1
  form: |-
    {"action":"x"}
address: 9f25
rationale:
  source: docs/canonical-hashing.md
  date: 2026-10-07
  rejected_alternatives:
    - a softer bound
  assumptions: []
relations:
  - type: refines
    to: REQ-0008
"""

V2_QUANTITY = """\
schema: req.claim/2
claim_id: REQ-0009
statement: >-
  The benchmark harness shall classify direction of change on the structural
  direction gold set with accuracy of at least 0.95.
applicability:
  - set == direction_gold_structural
tests:
  - "req::evidence::the_direction_gold_set_meets_its_accuracy_target"
evidence: machine
rationale:
  source: docs/canonical-hashing.md
  date: 2026-10-07
  rejected_alternatives:
    - a softer bound
  assumptions: []
relations:
  - type: refines
    to: REQ-0008
pins:
  value: "0.95"
  operator: GE
"""

V1_QUALITATIVE = """\
schema: req.claim/1
claim_id: REQ-0001
statement: >-
  The req tool shall treat a single claim as the unit of requirements authorship.
modality: SHALL
subject: REQ_TOOL
attribute: unit_of_work
operator: EQ
value: claim
unit: none
conditions: []
applicability: []
exception: null
temporal: null
quantifier: null
unstructured: false
canonical:
  rules_v: 6
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/adr/0001-the-unit-is-a-claim.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
"""


def self_check():
    failures = 0

    def case(name, ok, detail=""):
        nonlocal failures
        if not ok:
            failures += 1
            print(f"FAIL  {name}  {detail}")
        rows.append(name)

    rows = []
    migrated, report = migrate_text(V1_QUANTITY)
    case("a quantity claim keeps its kept blocks byte for byte and gains its pins",
         migrated == V2_QUANTITY, repr(migrated))
    case("the report names the drops and the pins",
         "attribute direction_accuracy" in report[0] and report[-1] == "pins: value 0.95, operator GE",
         repr(report))
    case("the stored fields are dropped as derived",
         "canonical (derived; computed at load)" in report[0]
         and "address (derived; computed at load)" in report[0], repr(report))

    migrated, report = migrate_text(V1_QUALITATIVE)
    case("a qualitative triple creates no pin", "pins:" not in migrated, migrated)
    case("the qualitative triple is printed as dropped",
         "attribute unit_of_work" in report[0] and "value claim" in report[0]
         and "operator EQ" in report[0], repr(report))
    case("the schema is rewritten", migrated.startswith("schema: req.claim/2\n"), migrated)
    for key in REMOVED:
        case(f"`{key}` does not survive", f"\n{key}:" not in "\n" + migrated, migrated)

    eq_numeric = V1_QUALITATIVE.replace("value: claim", 'value: "1"')
    case("a numeric EQ value pins the value and its EQ",
         migrate_text(eq_numeric)[0].endswith('pins:\n  value: "1"\n  operator: EQ\n'),
         migrate_text(eq_numeric)[0])
    case("a qualitative EQ pins nothing", "pins:" not in migrate_text(V1_QUALITATIVE)[0])
    with_unit = V1_QUALITATIVE.replace("value: claim", 'value: "0.2"').replace(
        "unit: none", "unit: s").replace("operator: EQ", "operator: LE")
    case("a unit other than none is a pin",
         migrate_text(with_unit)[0].endswith('pins:\n  value: "0.2"\n  operator: LE\n  unit: s\n'),
         migrate_text(with_unit)[0])
    for unit, written in [("%", '"%"'), ("lb # force", '"lb # force"'), ("s", "s"),
                          ("a: b", '"a: b"'), ("@x", '"@x"'), ("yes", '"yes"')]:
        text = V1_QUALITATIVE.replace("value: claim", 'value: "1"').replace(
            "unit: none", f'unit: "{unit}"')
        got = migrate_text(text)[0]
        case(f"a unit {unit!r} is written as {written} and nothing else",
             got.endswith(f"  unit: {written}\n"), got)
    commented_schema = V1_QUALITATIVE.replace(
        "schema: req.claim/1", "# the format\nschema: req.claim/1")
    case("a comment above `schema:` does not stop the schema being rewritten",
         "schema: req.claim/2\n" in migrate_text(commented_schema)[0]
         and "schema: req.claim/1" not in migrate_text(commented_schema)[0],
         migrate_text(commented_schema)[0])
    for unit in ("1", "1.0", "1e3", "0x1f"):
        text = V1_QUALITATIVE.replace("value: claim", 'value: "1"').replace(
            "unit: none", f'unit: "{unit}"')
        case(f"a numeric-looking unit {unit!r} is quoted so it stays a string",
             migrate_text(text)[0].endswith(f'  unit: "{unit}"\n'), migrate_text(text)[0])
    padded_unit = V1_QUALITATIVE.replace("value: claim", 'value: "1"').replace(
        "unit: none", 'unit: " ms"')
    try:
        migrate_text(padded_unit)
        case("a padded unit is refused", False, "not refused")
    except Refused as why:
        case("a padded unit is refused", "leading or trailing space" in str(why), str(why))
    case("a quoted scalar containing ` #` keeps its text",
         scalar(['unit: "lb # force"  # why\n']) == "lb # force")
    case("an unquoted scalar loses a trailing comment", scalar(["unit: s  # why\n"]) == "s")
    padded = V1_QUALITATIVE.replace("value: claim", 'value: "0.30"')
    case("a non-canonical numeral is pinned canonically, and said so",
         migrate_text(padded)[0].endswith('pins:\n  value: "0.3"\n  operator: EQ\n')
         and any("canonicalized" in line for line in migrate_text(padded)[1]),
         repr(migrate_text(padded)))
    null_op = V1_QUALITATIVE.replace("operator: EQ", "operator: null").replace("value: claim", "value: null")
    case("a null operator and value create no pin", "pins:" not in migrate_text(null_op)[0])

    comment = V1_QUALITATIVE.replace("attribute: unit_of_work", "# why the attribute\nattribute: unit_of_work")
    case("a comment above a dropped key is dropped with it",
         "# why the attribute" not in migrate_text(comment)[0], migrate_text(comment)[0])
    kept_comment = V1_QUALITATIVE.replace("rationale:\n", "# where it came from\nrationale:\n")
    case("a comment above a kept key survives",
         "# where it came from\nrationale:\n" in migrate_text(kept_comment)[0])
    header = "# a header\n" + V1_QUALITATIVE
    case("a header comment survives", migrate_text(header)[0].startswith("# a header\nschema: req.claim/2\n"))

    commented = V1_QUALITATIVE.replace("value: claim", '# the bound\nvalue: "7"')
    case("a comment above a pinned slot does not break the scalar read",
         migrate_text(commented)[0].endswith('pins:\n  value: "7"\n  operator: EQ\n'),
         migrate_text(commented)[0])

    for name, text, needle in [
        ("a non-comparator operator is refused",
         V1_QUALITATIVE.replace("operator: EQ", "operator: APPROX").replace("value: claim", 'value: "1"'),
         "not a req.claim/2 comparator"),
        ("an already-v2 file is refused", V2_QUANTITY, "already req.claim/2"),
        ("another schema is refused", V1_QUALITATIVE.replace("req.claim/1", "req.claim/0"), "migrates req.claim/1 only"),
        ("an unknown key is refused", V1_QUALITATIVE + "colour: blue\n", "not a req.claim/1 key"),
        ("a duplicate key is refused", V1_QUALITATIVE + "unit: s\n", "twice"),
        ("an unsplittable line is refused", V1_QUALITATIVE + "}{\n", "cannot split"),
    ]:
        try:
            migrate_text(text)
            case(name, False, "not refused")
        except Refused as why:
            case(name, needle in str(why), str(why))

    import contextlib
    import io
    import tempfile

    def drive(paths, write):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = run(paths, write)
        return code, out.getvalue(), err.getvalue()

    with tempfile.TemporaryDirectory() as tmp:
        good = os.path.join(tmp, "REQ-0001.yml")
        also = os.path.join(tmp, "REQ-0002.yml")
        ignored = os.path.join(tmp, "NOTES.yml")
        for path, text in [(good, V1_QUALITATIVE), (also, V1_QUANTITY), (ignored, "x: 1\n")]:
            with open(path, "w", encoding="utf-8", newline="") as handle:
                handle.write(text)

        def read(path):
            with open(path, encoding="utf-8", newline="") as handle:
                return handle.read()

        def disk():
            return {p: read(p) for p in (good, also, ignored)}

        before = disk()
        code, _, _ = drive([tmp], write=False)
        case("a dry run exits 0 and writes nothing", code == 0 and disk() == before)
        code, out, _ = drive([tmp], write=True)
        after = disk()
        case("--write rewrites the claim files and exits 0",
             code == 0 and after[good].startswith("schema: req.claim/2\n")
             and after[also].endswith('pins:\n  value: "0.95"\n  operator: GE\n'), out)
        case("a directory selects REQ-*.yml only", after[ignored] == before[ignored])
        code, out, _ = drive([tmp], write=True)
        case("a second --write refuses the already-v2 files, exits 1 and changes nothing",
             code == 1 and disk() == after, out)

    with tempfile.TemporaryDirectory() as tmp:
        ok = os.path.join(tmp, "REQ-0001.yml")
        broken = os.path.join(tmp, "REQ-0002.yml")
        for path, text in [(ok, V1_QUALITATIVE), (broken, V1_QUALITATIVE + "colour: blue\n")]:
            with open(path, "w", encoding="utf-8", newline="") as handle:
                handle.write(text)
        code, _, err = drive([tmp], write=True)
        case("one refused file exits 1 and leaves every file untouched",
             code == 1 and read(ok) == V1_QUALITATIVE
             and "nothing written" in err, err)
        code, _, err = drive([os.path.join(tmp, "absent")], write=False)
        case("a missing path is refused with exit 1", code == 1, err)

    long = "0." + "1" * 40
    case("a bound longer than 28 digits is pinned exactly, not rounded",
         canonical_decimal(long) == long, repr(canonical_decimal(long)))
    for raw, want in [("0", "0"), ("0.95", "0.95"), ("0.30", "0.3"), ("1.0", "1"), ("x", None),
                      ("1_000", None), ("200ms", None), ("-2.50", "-2.5")]:
        case(f"canonical_decimal({raw!r})", canonical_decimal(raw) == want, repr(canonical_decimal(raw)))

    print(f"migrate_claims --self-check: {len(rows)} case(s), {failures} failure(s)")
    return 1 if failures else 0


def main(argv):
    if "--self-check" in argv:
        if len(argv) != 1:
            print("usage: tools/migrate_claims.py --self-check takes no other arguments",
                  file=sys.stderr)
            return 2
        return self_check()
    write = False
    paths = []
    for arg in argv:
        if arg == "--write":
            write = True
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        elif arg.startswith("-"):
            print(f"usage: tools/migrate_claims.py [--write] [<path> ...]\nunknown flag: {arg}",
                  file=sys.stderr)
            return 2
        else:
            paths.append(arg)
    return run(paths or ["claims"], write)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
