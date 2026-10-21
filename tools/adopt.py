#!/usr/bin/env python3
"""tools/adopt.py — draft req claims from a document you already have (issue #44).

The blocker for an adopter with a `spec.md`, a backlog or a capabilities table is not parsing:
a claim's structured slots must be *authored* (the parity invariant), so a tool that guessed
`modality`, `value` or `operator` from prose would be the one path that can enshrine a
contradicted parse. What is honest to draft is the part the claim format already sanctions —
an **unstructured** claim: a statement, its provenance, and `unstructured: true`, with every
structured slot absent. That claim is legal as written (`req canon` reports 0 flags), it is
*unfilled* (`req canon --fill` derives `canonical.form` and `address`), and the human then
either leaves it qualitative or authors the slots. The drafts are proposals (ADR 0004).

This script is a starting point adopters **copy and adjust**; it is deliberately not a req
subcommand (issue #44's scope) and the mapping is per-project. Two conventions are built in,
and neither is a heuristic:

  bullets    each markdown list item (`- `, `* `, `1. `) under `--under` (or the whole
             document) is one candidate statement — its text, trimmed, wrapped lines joined
             onto one line. A list item inside a fenced code block is code, not a candidate.
  req-lines  each line matching `- REQ-<n>: <text>` is one candidate: the text after the
             colon. The source's id is **ignored** — req allocates the claim id — and the
             report says so.

A line that looks like a candidate and is not drafted is **named with its reason** and
counted: an empty item, a `req-lines` item with no id, an item outside the named heading. A
skipped line is never silent, because a requirement silently dropped at adoption is the
failure this project exists to prevent. (`bullets` cannot tell a four-space-indented code
sample from a nested item without a Markdown parser; the dry run is where a human sees one.)

The provenance a draft records is `<document>#L<line>`, **relative to the corpus root**, and
the date is either `--date` or the source document's last commit date — with the commit named
in the report. A document with uncommitted changes has no commit that describes its text, so
the run asks for `--date` rather than borrowing a revision that does not contain it.

Exit codes, the same three-state discipline as req:

  0  drafted (dry run, or written with --write)
  1  a definite problem — nothing extracted, the target already exists, or the document is
     already drafted into this corpus; nothing was written
  2  usage — a missing `--from`/`--subject`, an unreadable `--date`, or no date the tool can
     honestly borrow (not in git, or changed since its last commit)
  3  UNVERIFIABLE — the document, the corpus, the subject registry, or `req` itself cannot be
     read, so nothing can be drafted honestly

Nothing derived is ever written: `canonical.form` stays the empty placeholder and `address`
stays `null`. The values a draft must carry (`schema`, `rules_v`) are read from
`req --version`, and the registry version from `aliases/subjects.yml` — never hardcoded, so a
rules bump cannot leave this template behind.
"""
import argparse
import datetime
import os
import pathlib
import re
import shutil
import subprocess
import sys

# --- conventions (the whole mapping) ---------------------------------------

# A markdown list item. The two conventions name `- `, `* ` and `1. `; `+ ` is not one of
# them, and a convention with a heuristic in it is not a convention.
BULLET_RE = re.compile(r"^(?P<indent>[ \t]*)(?:[-*]|\d+\.)[ \t]+(?P<text>.*)$")
# Any list marker at any indent: a continuation line that starts one is a new item, never the
# wrapped tail of the previous one.
ANY_BULLET_RE = re.compile(r"^[ \t]*(?:[-*]|\d+\.)[ \t]+")
REQ_LINE_RE = re.compile(r"^[ \t]*-[ \t]+REQ-(?P<id>\d+)[ \t]*:[ \t]*(?P<text>.*)$")
HEADING_RE = re.compile(r"^(?P<marks>#{1,6})[ \t]+(?P<text>.*)$")

# A claim file's declared id, read without a YAML parser: the corpus contract puts it at the
# top level, and `yaml_serde` refuses a file without it, so a line read is the same fact the
# loader uses.
CLAIM_ID_RE = re.compile(r"^claim_id:[ \t]*(?P<value>\S.*?)[ \t]*$", re.MULTILINE)
CLAIM_SOURCE_RE = re.compile(r"^[ \t]+source:[ \t]*(?P<value>.+?)[ \t]*$", re.MULTILINE)
# The provenance the draft writes is `<document>#L<n>`. Stripping that suffix (and nothing
# else) is what lets the refusal below compare a stored source to a document path.
SOURCE_LINE_SUFFIX_RE = re.compile(r"#L\d+$")
REGISTRY_VERSION_RE = re.compile(r"^version:[ \t]*(?P<value>\d+)[ \t]*$", re.MULTILINE)

VERSION_RULES_RE = re.compile(r"^[ \t]*rules_v[ \t]+(?P<value>\d+)[ \t]*$", re.MULTILINE)
VERSION_SCHEMA_RE = re.compile(r"^[ \t]*claim_schema[ \t]+(?P<value>\S+)[ \t]*$", re.MULTILINE)
CLAIM_SCHEMA_RE = re.compile(r"^req\.claim/\d+$")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
SUBJECT_RE = re.compile(r"^[A-Za-z0-9_.\-]+$")

CLAIMS_DIR = "claims"
REGISTRY_PATH = "aliases/subjects.yml"
REQ_BINARY = "req"

# The draft claim, byte for byte: a fixed template, not a serialized mapping. python3's
# standard library has no YAML writer, and a round-trip through a serializer would produce a
# file whose diff is unreadable — while this shape is the contract `docs/adopting.md` states
# and the sibling of `src/init.rs`'s starter template. Every field the claim's parse requires
# is here; every derived field is the placeholder `req canon --fill` rewrites.
CLAIM_TEMPLATE = """\
# A draft claim, adopted from {document}. `statement` is the whole parse here: the structured
# slots are the human's (a draft is a proposal, ADR 0004). Next: run `req canon --fill` — it
# writes the derived `canonical.form` and `address` below — then either leave the claim
# qualitative or author the slots. Never hand-write either derived field.
schema: {schema}
claim_id: {claim_id}
statement: {statement}
modality: UNSTRUCTURED
subject: {subject}
applicability: []
unstructured: true
canonical:
  rules_v: {rules_v}
  subject_registry: {registry_v}
  form: ""
address: null
rationale:
  source: {quoted_source}
  date: {date}
  rejected_alternatives: []
  assumptions: []
relations: []
"""

HELP_EPILOG = """\
conventions (no heuristics, no model):
  bullets    (default) each markdown list item — `- `, `* `, `1. ` — under --under (or the
             whole document) is one candidate statement: its text, trimmed, its wrapped lines
             joined onto one line. Fenced code blocks are code: their lines are never
             candidates.
  req-lines  each line matching `- REQ-<n>: <text>` is one candidate: the text after the
             colon. The source's id is IGNORED — req allocates the claim id.

  A line that looks like a candidate and is not drafted is named with its reason and counted
  (`empty text`, `no id`, `outside the heading`). A skipped line is never silent.

  --subject is required: one decision per document, not per claim, and the value is used
  verbatim in the claim (an unregistered surface stays literal, so aliasing it is a later,
  deliberate act).
  --date is the rationale date; without it the source document's last git commit date is used,
  and the report names the commit. A date is never invented: no commit to borrow — untracked,
  or changed since its last commit — is a usage error asking for --date.

the draft this writes: an `unstructured: true` claim with its statement and provenance, and
NOTHING derived — `canonical.form` is the empty placeholder and `address` is null. `req canon`
accepts it, `req canon --fill` completes it, and the structured slots stay the human's.

The corpus root is --root, else $REQ_ROOT, else the working directory — never an ancestor: this
tool writes claims, so a run inside a subdirectory must not draft into a parent you never
named. A relative --from is resolved against that root, and the provenance is recorded
root-relative, so a run from a subdirectory with --root records the same path the next run
compares. A document already drafted into the corpus is refused (exit 1), so a second run over
the same document writes nothing.

exit: 0 drafted (dry run, or written) · 1 a definite problem, nothing written (nothing
extracted, the target already exists, or the document is already drafted into this corpus) ·
2 usage · 3 the document, the corpus, the subject registry or `req` cannot be read.

This script is a starting point to COPY and adjust for your project's document shape — it is
not a req subcommand.
"""


class Unverifiable(Exception):
    """A question this run could not settle (exit 3)."""


class Problem(Exception):
    """A definite finding (exit 1)."""


class Usage(Exception):
    """A malformed invocation, or a date the caller must supply (exit 2)."""


# --- the pure half (--self-check table-drives all of it) --------------------


def heading(line):
    """The (level, text) of an ATX heading line, or None. A closing `###` is stripped."""
    match = HEADING_RE.match(line)
    if not match:
        return None
    text = re.sub(r"[ \t]+#+[ \t]*$", "", match.group("text").strip()).strip()
    return len(match.group("marks")), text


def fence_state(lines):
    """Per line: is it inside (or is it the boundary of) a fenced code block?

    A list item inside a fence is a code sample, not a markdown list item, and a `#` there is
    not a heading — so the conventions below never look at these lines. A fence opens on a
    line whose first three characters are ``` or ~~~ and closes on the next such line.
    Indented (four-space) code blocks are deliberately not guessed at: an indented line that
    looks like a list item is a nested item in this tool's stated convention, and the dry run
    is where a human sees it.
    """
    inside = []
    fence = None
    for line in lines:
        stripped = line.strip()
        marker = stripped[:3]
        if fence is None:
            if marker in ("```", "~~~"):
                fence = marker
                inside.append(True)
            else:
                inside.append(False)
        else:
            inside.append(True)
            if marker == fence:
                fence = None
    return inside


def heading_scope(lines, wanted):
    """The half-open [start, end) line range under the heading named `wanted`, or None.

    A heading's scope runs to the next heading of the same or a higher level, so a subsection
    is under it. The text is compared after trimming, case-insensitively: a heading is a label
    a human typed twice, not an identifier. A heading inside a fenced code block is a line of
    code, not a heading.
    """
    fenced = fence_state(lines)
    start = level = None
    for index, line in enumerate(lines):
        if fenced[index]:
            continue
        current = heading(line)
        if current is None:
            continue
        if start is None:
            if current[1].lower() == wanted.strip().lower():
                start, level = index, current[0]
        elif current[0] <= level:
            return start, index
    return None if start is None else (start, len(lines))


def normalize(text):
    """A candidate's text as one line: whitespace runs collapsed, ends trimmed."""
    return " ".join(text.split())


def joined_item(lines, index, indent):
    """One list item's text: the marker line plus its wrapped lines, joined onto one line.

    A blank line, a heading, another list marker, or a line at or above the marker's own
    indent ends the item — the markdown a backlog is written in, with no lazy-continuation
    guessing. Returns `(text, next_index)`.
    """
    parts = [BULLET_RE.match(lines[index]).group("text")]
    cursor = index + 1
    while cursor < len(lines):
        line = lines[cursor]
        if (
            not line.strip()
            or heading(line)
            or ANY_BULLET_RE.match(line)
            or line.strip()[:3] in ("```", "~~~")
        ):
            break
        if len(line) - len(line.lstrip(" \t")) <= indent:
            break
        parts.append(line.strip())
        cursor += 1
    return normalize(" ".join(parts)), cursor


def extract(lines, convention, under=None):
    """The drafts and the skips of one document.

    Returns `(drafts, skips, error)`: `drafts` is `[(line_number, text)]` (1-based),
    `skips` is `[(line_number, reason)]`, and `error` is a definite finding about the document
    itself (a heading the caller named and the document does not have).
    """
    scope = None
    if under is not None:
        scope = heading_scope(lines, under)
        if scope is None:
            return [], [], f'the heading "{under}" does not appear in the document'

    def in_scope(index):
        return scope is None or scope[0] < index < scope[1]

    fenced = fence_state(lines)
    drafts, skips = [], []
    index = 0
    while index < len(lines):
        line = lines[index]
        if fenced[index]:
            # A line inside a fence is code: not a candidate, and not a skip to explain.
            index += 1
            continue
        if convention == "bullets":
            if not BULLET_RE.match(line):
                index += 1
                continue
            if not in_scope(index):
                skips.append((index + 1, "outside the heading"))
                index += 1
                continue
            indent = len(line) - len(line.lstrip(" \t"))
            text, following = joined_item(lines, index, indent)
            if not text:
                skips.append((index + 1, "empty text"))
            else:
                drafts.append((index + 1, text))
            index = following
            continue

        # req-lines: a list item is a candidate; only the `- REQ-<n>: text` shape drafts.
        if not ANY_BULLET_RE.match(line):
            index += 1
            continue
        if not in_scope(index):
            skips.append((index + 1, "outside the heading"))
            index += 1
            continue
        match = REQ_LINE_RE.match(line)
        if match is None:
            skips.append((index + 1, 'no id (expected `- REQ-<n>: text`)'))
        else:
            text = normalize(match.group("text"))
            if text:
                drafts.append((index + 1, text))
            else:
                skips.append((index + 1, "empty text"))
        index += 1
    return drafts, skips, None


def claim_number_from_name(name):
    """The claim number a file name carries, or None.

    Mirrors the corpus loader's gate (`src/canon.rs::looks_like_claim_stem`): a leading dot is
    not part of the stem, and the number is the stem's — `REQ-0008.yml`, `.REQ-0008` and
    `REQ-0008.yml.disabled` all name claim 8.
    """
    visible = name.lstrip(".")
    stem = visible.split(".", 1)[0]
    match = re.fullmatch(r"REQ-(\d+)", stem, re.IGNORECASE)
    return int(match.group(1)) if match else None


def is_claim_candidate(name):
    """Whether a `claims/` entry is claim-shaped — the loader's gate, for the same reason.

    Anything else in `claims/` (the README, editor droppings) is not a claim and is left
    alone; a candidate that does not parse is a corpus error, never a silent drop and never a
    number that can be handed out twice.
    """
    lowered = name.lower()
    return (
        lowered.endswith(".yml")
        or lowered.endswith(".yaml")
        or claim_number_from_name(name) is not None
    )


def next_claim_number(used):
    """The next free claim number: one after the highest in use, or 1.

    `max + 1` rather than "the first gap": a gap is a claim that was deleted deliberately, and
    handing its id to a different requirement would make git history read as if the same claim
    had changed meaning. 1 when the corpus holds no claim.
    """
    return max(used) + 1 if used else 1


def parse_version(text):
    """`(rules_v, claim_schema)` from `req --version`, or None.

    Pure so the self-check can table-drive the shapes without a binary: the two values are what
    a draft is judged by, and a hardcoded copy of either is exactly the drift this avoids.
    """
    rules = VERSION_RULES_RE.search(text)
    schema = VERSION_SCHEMA_RE.search(text)
    if rules is None or schema is None:
        return None
    value = schema.group("value")
    if not CLAIM_SCHEMA_RE.match(value):
        return None
    return int(rules.group("value")), value


def valid_date(text):
    """Whether `text` is a real `YYYY-MM-DD` calendar date."""
    if not DATE_RE.match(text):
        return False
    try:
        datetime.date.fromisoformat(text)
    except ValueError:
        return False
    return True


def yaml_quote(text):
    """A YAML double-quoted scalar, for every freely-authored value.

    Quotes and backslashes alone are not enough: a statement carrying a control character
    (a NUL pasted in from a table, a stray bell) would produce a file the loader cannot parse
    while the tool still reported success. A backslash-x escape is YAML's own form for those, and the
    loader parses it back to the same character.
    """
    out = []
    for char in text:
        if char == "\\":
            out.append("\\\\")
        elif char == '"':
            out.append('\\"')
        elif char == "\n":
            out.append("\\n")
        elif char == "\t":
            out.append("\\t")
        elif char == "\r":
            out.append("\\r")
        elif ord(char) < 0x20 or ord(char) == 0x7F:
            out.append(f"\\x{ord(char):02x}")
        else:
            out.append(char)
    return '"' + "".join(out) + '"'


def render_claim(document, source, statement, date, subject, claim_id, rules_v, registry_v, schema):
    """One draft claim file's exact bytes.

    `document` is the path the reader is pointed at; `source` is the provenance the rationale
    records — the document plus the line the statement came from (`<doc>#L<n>`), because a claim
    whose provenance cannot be followed back to a line is a claim nobody can check.
    """
    return CLAIM_TEMPLATE.format(
        document=document,
        schema=schema,
        claim_id=claim_id,
        statement=yaml_quote(statement),
        subject=subject,
        rules_v=rules_v,
        registry_v=registry_v,
        quoted_source=yaml_quote(source),
        date=date,
    )


# --- the filesystem half ----------------------------------------------------


class Corpus:
    """What the extractor needs to know about the corpus it drafts into."""

    def __init__(self, root, used, sources, registry_v):
        self.root = root
        self.used = used
        self.sources = sources
        self.registry_v = registry_v


def resolve_root(flag):
    """`--root`, else `REQ_ROOT`, else the working directory — never an ancestor.

    req's own walk-up (ADR 0012) is deliberately not copied: this tool writes claims, so a run
    inside a subdirectory must not put drafts in a parent the caller never named. The chosen
    root is printed in the report.
    """
    chosen = flag or os.environ.get("REQ_ROOT") or "."
    root = pathlib.Path(chosen).expanduser()
    if not root.is_dir():
        raise Unverifiable(f"the corpus root is not a directory: {root}")
    return root.resolve()


def parse_source(value, root):
    """A stored `rationale.source` as `(absolute path, carries a line number)`.

    The line number is what makes a source *this tool's* provenance: `req init`'s starter claim
    carries a pencil sketch (`source: docs/requirements.md`) that no adoption produced, and
    treating it as an earlier draft would refuse the very document an adopter is most likely to
    have. Only `<doc>#L<n>` is evidence that the document was drafted here before.
    """
    text = value.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        text = text[1:-1]
    has_line = SOURCE_LINE_SUFFIX_RE.search(text) is not None
    text = SOURCE_LINE_SUFFIX_RE.sub("", text)
    path = pathlib.Path(text).expanduser()
    if not path.is_absolute():
        path = root / path
    return path.resolve(), has_line


def read_corpus(root):
    """Read the claim ids in use, the provenance already recorded, and the registry version."""
    claims_dir = root / CLAIMS_DIR
    if not claims_dir.is_dir():
        raise Unverifiable(
            f"no corpus at {root}: {CLAIMS_DIR}/ is not there — run `req init` here first, "
            "or pass --root <dir>"
        )

    used = set()
    sources = {}
    for entry in sorted(claims_dir.iterdir(), key=lambda path: path.name):
        if not is_claim_candidate(entry.name):
            continue
        if not entry.is_file() or entry.is_symlink():
            # The loader refuses a symlinked claim for the same reason (a link is not a claim
            # file), and a number read through one could be a number the corpus does not own.
            raise Unverifiable(
                f"{entry}: a claim file must be a regular file — `claims/` is flat, one claim "
                "per file"
            )
        try:
            text = entry.read_text(encoding="utf-8")
        except OSError as error:
            raise Unverifiable(f"cannot read {entry}: {error}") from error
        match = CLAIM_ID_RE.search(text)
        if match is None:
            raise Unverifiable(
                f"{entry}: no `claim_id:` line — this corpus cannot be read as a corpus; "
                "`req canon --verify` names the defect"
            )
        claim_id = match.group("value").strip()
        # Both numbers are reserved: the declared id is what the claim is, and the file name is
        # where the next draft would land — a claim whose id and file name disagree must not let
        # either number be handed out twice.
        declared = number_or_none(claim_id)
        if declared is not None:
            used.add(declared)
        named = claim_number_from_name(entry.name)
        if named is not None:
            used.add(named)
        source = CLAIM_SOURCE_RE.search(text)
        if source is not None:
            path, from_a_draft = parse_source(source.group("value"), root)
            if from_a_draft:
                sources[claim_id] = path

    registry = root / REGISTRY_PATH
    try:
        registry_text = registry.read_text(encoding="utf-8")
    except OSError as error:
        raise Unverifiable(
            f"cannot read the subject registry ({registry}): {error} — run `req init` here "
            "first"
        ) from error
    version = REGISTRY_VERSION_RE.search(registry_text)
    if version is None:
        raise Unverifiable(
            f"{registry}: no `version:` line — the registry cannot be read, and a draft's "
            "`canonical.subject_registry` would be a guess"
        )
    return Corpus(root, used, sources, int(version.group("value")))


def number_or_none(value):
    """A declared claim id's number, or None when it is not a `REQ-<n>` shape."""
    match = re.fullmatch(r"REQ-(\d+)", value, re.IGNORECASE)
    return int(match.group(1)) if match else None


def read_axes():
    """`(rules_v, claim_schema)` from the `req` on PATH — the values a draft must carry."""
    binary = shutil.which(REQ_BINARY)
    if binary is None:
        raise Unverifiable(
            "`req` is not on PATH — install it at a pinned rev (docs/adopting.md), because a "
            "draft's `rules_v` and claim schema come from the binary that will judge it"
        )
    try:
        completed = subprocess.run(
            [binary, "--version"], capture_output=True, text=True, check=False
        )
    except OSError as error:
        raise Unverifiable(f"cannot run `{binary} --version`: {error}") from error
    if completed.returncode != 0:
        raise Unverifiable(
            f"`{binary} --version` exited {completed.returncode}: "
            f"{completed.stderr.strip() or 'no output'}"
        )
    axes = parse_version(completed.stdout)
    if axes is None:
        raise Unverifiable(
            f"`{binary} --version` does not name `rules_v` and `claim_schema` — a build "
            "older than the claim format this tool writes cannot be drafted against"
        )
    return axes


def git_date(document):
    """The document's last commit date and short SHA, or None.

    The same discipline as `req baseline --at`: a time is *authored*, never invented. `%cs` is
    the committer date — a date, not a wall-clock reading — and the SHA is reported with it so
    the draft says which revision its date describes.
    """
    try:
        completed = subprocess.run(
            [
                "git",
                "-C",
                str(document.parent),
                "log",
                "-1",
                "--format=%cs %h",
                "--",
                document.name,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    if completed.returncode != 0:
        return None
    parts = completed.stdout.split()
    if len(parts) != 2 or not valid_date(parts[0]):
        return None
    return parts[0], parts[1]


def git_dirty(document):
    """Whether the document differs from its last commit; None when git cannot answer."""
    try:
        completed = subprocess.run(
            [
                "git",
                "-C",
                str(document.parent),
                "status",
                "--porcelain",
                "--",
                document.name,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    if completed.returncode != 0:
        return None
    return bool(completed.stdout.strip())


def headings_of(lines):
    """The document's headings, for the 'heading not found' message."""
    fenced = fence_state(lines)
    found = [
        f'"{text}"'
        for index, line in enumerate(lines)
        if not fenced[index]
        for level, text in [heading(line) or (0, "")]
        if level
    ]
    if not found:
        return "  the document has no markdown headings"
    return "  headings found: " + ", ".join(found)


def report(args, corpus, source, drafts, skips, targets, written, rules_v):
    """Print what was drafted, what was skipped and why, and what was written.

    `source` is the provenance label the claims record — relative to the corpus root — so the
    report and the claim files name one path, whichever directory the run happened in.
    """
    scope = f'under the heading "{args.under}"' if args.under else "whole document"
    print(f"source: {source} ({args.convention}, {scope})")
    print(f"subject: {args.subject}")
    print(f"date: {args.date} ({args.date_origin})")
    print(f"root: {corpus.root}")
    print(f"req: rules_v {rules_v}, registry {corpus.registry_v}")
    if args.convention == "req-lines":
        print(
            "note: the source's REQ-<n> ids are ignored — req allocates the claim_id, and a "
            "source id is not a req claim id."
        )
    print()
    for (line, statement), (claim_id, target) in zip(drafts, targets):
        print(f"draft  {claim_id}  {relative(corpus.root, target)}  {source}#L{line}")
        print(f"       statement: {statement}")
    for line, reason in skips:
        print(f"skip   {source}#L{line}  {reason}")
    print()
    print(f"drafted: {len(drafts)}")
    print(f"skipped: {len(skips)}")
    if written:
        names = ", ".join(relative(corpus.root, target) for _claim_id, target in targets)
        print(f"wrote: {len(written)} claim(s) — {names}")
    else:
        print("dry run: nothing written — pass --write to write them")


def relative(root, path):
    """A path as the report shows it: root-relative when it is under the root."""
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def write_claims(targets, texts):
    """Write every target exclusively, or write nothing at all.

    `x` mode, not a check-then-write: the pre-flight in `run` covers the ordinary case, so a
    path that appears in between (a concurrent run) is refused by the write itself rather than
    overwritten. What this run did write is removed on failure — the `init` discipline: a
    half-written corpus is red *and* refuses the next run.
    """
    written = []
    try:
        for target, text in zip(targets, texts):
            try:
                handle = open(target, "x", encoding="utf-8")
            except FileExistsError as error:
                raise Problem(
                    f"refusing to write {target}: it already exists — a claim file is never "
                    "overwritten"
                ) from error
            # Remembered *before* the write: a failure part-way leaves a partial file, and a
            # cleanup that only knows about completed writes would leave it behind as a claim
            # nobody wrote.
            written.append(target)
            with handle:
                handle.write(text)
    except OSError as error:
        remove_written(written)
        raise Unverifiable(
            f"cannot write {error.filename or 'the claims'}: {error}"
        ) from error
    except Problem:
        remove_written(written)
        raise
    return written


def remove_written(written):
    """Best-effort removal, reported — never swallowed."""
    leftovers = []
    for path in written:
        try:
            path.unlink()
        except OSError as error:
            leftovers.append(f"{path}: {error}")
    if leftovers:
        print(
            "[UNVERIFIABLE] could not remove what a failed run wrote — remove it by hand:\n  "
            + "\n  ".join(leftovers),
            file=sys.stderr,
        )


# --- the driver -------------------------------------------------------------


def run(args):
    """Draft, report, and (with --write) write. Returns the exit code."""
    corpus = read_corpus(resolve_root(args.root))

    # The document is resolved against the **corpus root**, not the working directory, and the
    # provenance is recorded root-relative. One basis for writing and for reading back: a run
    # from a subdirectory with `--root ..` records `docs/x.md#L3`, and the next run's guard
    # compares the same absolute path. Reading the document relative to the cwd while recording
    # the path relative to the root was a real bug — the guard then never matched, and a second
    # `--write` re-drafted the document under fresh ids.
    document = pathlib.Path(args.document).expanduser()
    if not document.is_absolute():
        document = corpus.root / document
    try:
        text = document.read_text(encoding="utf-8")
    except OSError as error:
        raise Unverifiable(f"cannot read the document {args.document}: {error}") from error
    document = document.resolve()
    source = relative(corpus.root, document)

    # A document already adopted is refused before anything else: the drafts are in the
    # corpus, and drafting them again would duplicate claims (two claims sharing a statement
    # and a subject share an address). This is what makes a second `--write` over the same
    # document refuse and overwrite nothing. Only this tool's own `<doc>#L<n>` provenance is
    # evidence of an earlier draft — `req init`'s starter claim carries a pencil sketch
    # (`source: docs/requirements.md`) that no adoption produced.
    already = sorted(
        claim_id for claim_id, stored in corpus.sources.items() if stored == document
    )
    if already:
        raise Problem(
            f"refusing to draft: {source} is already drafted into this corpus "
            f"({', '.join(already)}).\n"
            "  edit those claims, or delete them deliberately, before drafting from this "
            "document again — or copy this script and adjust the mapping"
        )

    if args.date is None:
        borrowed = git_date(document)
        if borrowed is None:
            raise Usage(
                "no --date, and the document is not in git history (or git is unavailable): "
                "pass --date YYYY-MM-DD. A date is authored, never invented — this tool will "
                "not stamp today's."
            )
        if git_dirty(document):
            raise Usage(
                f"{source} has uncommitted changes, so its last commit date ({borrowed[0]}) "
                "does not describe the text being drafted: pass --date YYYY-MM-DD. A date is "
                "authored, never borrowed from a revision that does not contain the text."
            )
        args.date = borrowed[0]
        args.date_origin = f"the last commit touching {source}: {borrowed[1]}"

    rules_v, schema = read_axes()

    lines = text.splitlines()
    drafts, skips, error = extract(lines, args.convention, args.under)
    if error is not None:
        raise Problem(f"nothing extracted: {error}\n{headings_of(lines)}")
    if not drafts:
        scope = f"under {args.under!r}" if args.under else "whole document"
        # The skipped lines are named even here: "nothing extracted" is the finding, and *why*
        # each candidate was skipped is its remedy.
        reasons = "".join(f"\n  skip {source}#L{line}  {reason}" for line, reason in skips)
        raise Problem(
            f"nothing extracted from {source} ({args.convention}, {scope})\n"
            "  bullets are `- `, `* ` and `1. `; req-lines are `- REQ-<n>: text`" + reasons
        )

    number = next_claim_number(corpus.used)
    targets, texts = [], []
    for offset, (line, statement) in enumerate(drafts):
        claim_id = f"REQ-{number + offset:04d}"
        targets.append((claim_id, corpus.root / CLAIMS_DIR / f"{claim_id}.yml"))
        texts.append(
            render_claim(
                document=source,
                source=f"{source}#L{line}",
                statement=statement,
                date=args.date,
                subject=args.subject,
                claim_id=claim_id,
                rules_v=rules_v,
                registry_v=corpus.registry_v,
                schema=schema,
            )
        )

    if args.write:
        for _claim_id, target in targets:
            if target.exists() or target.is_symlink():
                raise Problem(
                    f"refusing to write {target}: it already exists — a claim file is never "
                    "overwritten"
                )
        written = write_claims([target for _claim_id, target in targets], texts)
    else:
        written = []

    report(args, corpus, source, drafts, skips, targets, written, rules_v)
    return 0


def field(text, prefix):
    """The first line of `text` starting with `prefix`, trimmed — a field, not a substring."""
    for line in text.splitlines():
        if line.startswith(prefix):
            return line.strip()
    return None


def self_check():
    """Table-drive the pure half. No filesystem, no git, no `req`, no document.

    Every row is a fact the conventions promise: what a list item is, what the heading scope
    is, why a line is skipped, that a source id is ignored, how the next claim number is
    chosen, and what the two version axes must look like. The filesystem half — the corpus,
    the registry, `--write`, the refusal — is exercised by `tests/adopt.rs`, which is where a
    subprocess can actually answer.
    """
    # Line numbers below are 1-based, so the table's `#L<n>` values are the report's.
    doc = [
        "# Capabilities",  # 1
        "",  # 2
        "- The copilot reads each submitted document before judging it.",  # 3
        "- The copilot cites every source it quotes.",  # 4
        "- ",  # 5
        "",  # 6
        "## Detail",  # 7
        "",  # 8
        "- A subsection is under the heading.",  # 9
        "",  # 10
        "# Elsewhere",  # 11
        "",  # 12
        "- Outside.",  # 13
        "",  # 14
        "- REQ-7: The system shall log every decision.",  # 15
        "- A bullet with no id at all.",  # 16
        "- REQ-8:",  # 17
    ]
    whole_bullets = (
        [
            (3, "The copilot reads each submitted document before judging it."),
            (4, "The copilot cites every source it quotes."),
            (9, "A subsection is under the heading."),
            (13, "Outside."),
            (15, "REQ-7: The system shall log every decision."),
            (16, "A bullet with no id at all."),
            (17, "REQ-8:"),
        ],
        [(5, "empty text")],
        None,
    )
    whole_req_lines = (
        [(15, "The system shall log every decision.")],
        [
            (3, "no id (expected `- REQ-<n>: text`)"),
            (4, "no id (expected `- REQ-<n>: text`)"),
            (5, "no id (expected `- REQ-<n>: text`)"),
            (9, "no id (expected `- REQ-<n>: text`)"),
            (13, "no id (expected `- REQ-<n>: text`)"),
            (16, "no id (expected `- REQ-<n>: text`)"),
            (17, "empty text"),
        ],
        None,
    )
    scoped_bullets = (
        [
            (3, "The copilot reads each submitted document before judging it."),
            (4, "The copilot cites every source it quotes."),
            (9, "A subsection is under the heading."),
        ],
        [
            (5, "empty text"),
            (13, "outside the heading"),
            (15, "outside the heading"),
            (16, "outside the heading"),
            (17, "outside the heading"),
        ],
        None,
    )
    draft = render_claim(
        document="docs/capabilities.md",
        source="docs/capabilities.md#L3",
        statement='The copilot reads "each" document \\ once.',
        date="2026-10-21",
        subject="system",
        claim_id="REQ-0016",
        rules_v=5,
        registry_v=1,
        schema="req.claim/0",
    )

    rows = [
        ("bullets: every list item is one draft, the empty one is skipped", extract(doc, "bullets"), whole_bullets),
        (
            "bullets: a wrapped item is joined onto one line",
            extract(["- The copilot reads each", "  submitted document."], "bullets"),
            ([(1, "The copilot reads each submitted document.")], [], None),
        ),
        (
            "bullets: a nested item is a new draft, not a wrapped line",
            extract(["- Parent.", "  - Child."], "bullets"),
            ([(1, "Parent."), (2, "Child.")], [], None),
        ),
        (
            "bullets: a star item and a numbered item are list items",
            extract(["* One.", "1. Two."], "bullets"),
            ([(1, "One."), (2, "Two.")], [], None),
        ),
        (
            "req-lines: the text after the colon is the statement, the id is not",
            extract(doc, "req-lines"),
            whole_req_lines,
        ),
        (
            "req-lines: `- REQ-8:` with no text is skipped, not drafted",
            extract(["- REQ-8:"], "req-lines"),
            ([], [(1, "empty text")], None),
        ),
        (
            "req-lines: a line without a list marker is not a candidate",
            extract(["REQ-7: no marker.", "- REQ-7: a marker."], "req-lines"),
            ([(2, "a marker.")], [], None),
        ),
        (
            "heading: a subsection is under it, and items outside it are skipped",
            extract(doc, "bullets", "Capabilities"),
            scoped_bullets,
        ),
        (
            "heading: the comparison trims and ignores case",
            extract(["# Capabilities", "- In."], "bullets", "capabilities")[0],
            [(2, "In.")],
        ),
        (
            "heading: an ATX closing sequence is not part of the heading text",
            extract(["## Capabilities ##", "- In."], "bullets", "Capabilities")[0],
            [(2, "In.")],
        ),
        (
            "heading: a heading the document does not have is a definite finding",
            extract(doc, "bullets", "Absent")[2],
            'the heading "Absent" does not appear in the document',
        ),
        (
            "fences: a list item inside a fenced code block is code, not a candidate",
            extract(["```", "- not a list item", "```", "- real"], "bullets"),
            ([(4, "real")], [], None),
        ),
        (
            "fences: a heading inside a fence does not scope",
            extract(
                ["```", "# Capabilities", "```", "# Capabilities", "- in"],
                "bullets",
                "Capabilities",
            )[0],
            [(5, "in")],
        ),
        (
            "draft: a control character is escaped, never emitted raw",
            yaml_quote("a\x00b"),
            '"a\\x00b"',
        ),
        ("ids: claim 8 from a file name", claim_number_from_name("REQ-0008.yml"), 8),
        ("ids: a leading dot is not part of the stem", claim_number_from_name(".REQ-0008"), 8),
        (
            "ids: a renamed claim still names its number",
            claim_number_from_name("REQ-0008.yml.disabled"),
            8,
        ),
        ("ids: a lower-case name is still claim-shaped", claim_number_from_name("req-8.yml"), 8),
        ("ids: an unrelated name has no number", claim_number_from_name("notes.yml"), None),
        ("ids: the README is not a claim", is_claim_candidate("README.md"), False),
        ("ids: any yml under claims/ is a claim candidate", is_claim_candidate("notes.yml"), True),
        ("next: an empty corpus starts at 1", next_claim_number(set()), 1),
        ("next: one past the highest", next_claim_number({1, 2}), 3),
        (
            "next: a deleted claim's number is not reused before the highest",
            next_claim_number({1, 3}),
            4,
        ),
        (
            "version: the two axes are read from the binary's output",
            parse_version(
                "req 0.1.0\n  rules_v            5\n  baseline_envelope  3\n"
                "  decision_chain     1\n  claim_schema       req.claim/0\n"
                "  built from         deadbee\n"
            ),
            (5, "req.claim/0"),
        ),
        (
            "version: a build that does not name the axes cannot be drafted against",
            parse_version("req 0.1.0\n  built from         deadbee\n"),
            None,
        ),
        (
            "version: a schema of no req.claim/<n> shape is not a claim format",
            parse_version("  rules_v  5\n  claim_schema  req.something/0\n"),
            None,
        ),
        ("date: a real date is usable", valid_date("2026-10-21"), True),
        ("date: a short form is not", valid_date("2026-1-2"), False),
        ("date: a day that does not exist is not", valid_date("2026-02-30"), False),
        ("date: a timestamp is not a date", valid_date("2026-10-21T00:00:00Z"), False),
        (
            "draft: the form is the empty placeholder, and there is no address",
            (field(draft, "  form: "), field(draft, "address: ")),
            ('form: ""', "address: null"),
        ),
        (
            "draft: the claim is unstructured, which is what makes it legal as written",
            (field(draft, "modality: "), field(draft, "unstructured: ")),
            ("modality: UNSTRUCTURED", "unstructured: true"),
        ),
        (
            "draft: a statement's quotes and backslashes are escaped for YAML",
            field(draft, "statement: "),
            'statement: "The copilot reads \\"each\\" document \\\\ once."',
        ),
        (
            "draft: the provenance is the document and the line it came from",
            field(draft, "  source: "),
            'source: "docs/capabilities.md#L3"',
        ),
    ]

    failures = 0
    for label, got, want in rows:
        ok = got == want
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'} {label:<62} {got!r}")
    print(f"adopt --self-check: {len(rows)} case(s), {failures} failure(s)")
    return 1 if failures else 0


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="tools/adopt.py",
        description=(
            "Draft req claims from a document you already have: one `unstructured` claim per "
            "statement, with its provenance, and nothing derived."
        ),
        epilog=HELP_EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--from",
        dest="document",
        required=True,
        metavar="<doc>",
        help=(
            "the markdown document to read; a relative path is resolved against the corpus "
            "root and recorded root-relative as the rationale source"
        ),
    )
    parser.add_argument(
        "--under",
        metavar="<heading>",
        help="only the section under this markdown heading (a subsection is under it)",
    )
    parser.add_argument(
        "--convention",
        choices=("bullets", "req-lines"),
        default="bullets",
        help="bullets (default): each list item; req-lines: `- REQ-<n>: text`",
    )
    parser.add_argument(
        "--subject",
        required=True,
        metavar="<id>",
        help="the claim's subject id, used verbatim (one decision per document)",
    )
    parser.add_argument(
        "--date",
        metavar="YYYY-MM-DD",
        help="the rationale date; without it, the document's last git commit date",
    )
    parser.add_argument(
        "--root",
        metavar="<dir>",
        help=(
            "the corpus root (default: $REQ_ROOT, else the working directory) — a relative "
            "--from is resolved against it"
        ),
    )
    parser.add_argument(
        "--write", action="store_true", help="write the drafts (the default is a dry run)"
    )
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="table-drive the pure half; read no document, no corpus, no `req`",
    )
    return parser.parse_args(argv)


def main(argv):
    # `--self-check` stands alone: it must work before a corpus exists and without `req`, so it
    # is answered before the required arguments are demanded of it. Any other argument beside
    # it is a usage error, not a quiet run of two things at once.
    if "--self-check" in argv:
        if len(argv) != 1:
            print(
                "usage: tools/adopt.py --self-check takes no other arguments", file=sys.stderr
            )
            return 2
        return self_check()

    args = parse_args(argv)
    args.date_origin = "--date" if args.date is not None else None
    if not SUBJECT_RE.match(args.subject):
        print(
            f"usage: --subject must be a subject id (letters, digits, `_`, `.`, `-`), not "
            f"{args.subject!r} — its value is used verbatim in the claim",
            file=sys.stderr,
        )
        return 2
    if args.date is not None and not valid_date(args.date):
        print(
            f"usage: --date must be a calendar date in YYYY-MM-DD form, not {args.date!r}",
            file=sys.stderr,
        )
        return 2
    try:
        return run(args)
    except Usage as error:
        print(f"usage: {error}", file=sys.stderr)
        return 2
    except Problem as error:
        print(str(error), file=sys.stderr)
        return 1
    except Unverifiable as error:
        print(f"[UNVERIFIABLE] {error}", file=sys.stderr)
        return 3
    except Exception as error:  # noqa: BLE001 - an unexpected failure is not a verdict
        # A crash is not "a definite problem" about the document: the tri-state answer to a
        # question this run could not settle is UNVERIFIABLE, never a FAIL the caller reads as
        # their input's fault.
        print(f"[UNVERIFIABLE] internal error: {error!r}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
