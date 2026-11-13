#!/usr/bin/env python3
"""tools/needs_import.py — draft req claims from a Sphinx-Needs `needs.json` (issue #124, ADR 0027).

An adopter on Sphinx-Needs already has its requirements as `needs` objects: a stable id, a
title, a body (`content`), a status and typed links. This script reads the export and writes
the same legal `req.claim/2` drafts `tools/adopt.py` writes — the statement, its provenance,
empty `applicability` and `relations`, nothing derived and no pin (ADR 0022). The drafts are
proposals (ADR 0004); a source status is not a ratification, so no status is carried.

It is a sibling of `tools/adopt.py`, not a client of it: the corpus reader, the id allocation
and the claim template are copied, and adopters **copy and adjust** this file (the mapping is
per project). It is deliberately not a req subcommand.

What it decides, and what it refuses to guess:

  types      the caller names the need types to read (`--type`, required, repeatable). There is
             no default list: the type names are the project's own (`feat_req` in one export is
             `sysreq` in another). A need of a type that is not named is counted, by type, in
             the report — never silently ignored.
  statement  the need's `content`, only when it is one plain paragraph: its lines trimmed and
             joined with one space. Anything else is skipped and NAMED with its reason —
             `external`, `no content`, `rst directive`, `multi-paragraph`, `rst role` — in that
             order, the first that applies. The reader does not strip markup or cut a need down
             to make it fit: whether a second paragraph is part of the obligation or a note is
             a human decision.
  claim id   req allocates it (the next free REQ-NNNN). The source's need id is ignored for the
             claim id and kept as provenance: `rationale.source` is `<origin>#<need id>`, where
             `<origin>` is what the caller passes as `--origin` (the URL or path the export came
             from). A need already drafted from the same origin is refused.
  date       `--date`, required. An export has no trustworthy date, and a date is authored.
  links      typed links name need ids, and a claim's `relations` name claim ids that do not
             exist until req allocates them. They are not carried; the report counts the drafted
             needs that had them and names the fields.
  area       opt-in, `--areas` (ADR 0051 decision 2): a need's place is its `docname`. The name is
             split on `/`, each part is made a key (lowercase it; replace each run of characters
             outside [a-z0-9] with one `-`; strip `-` from both ends), and the keys joined by `/`
             are the claim's `area`. Never a section title or number: both move when the source is
             edited. A part that leaves no key, or two names that make one path, is exit 1 naming
             them (never merged). A need with no `docname` gets no area. `--area-depth N` keeps the
             outermost N parts. The draft is then `req.claim/3` with `area:` after `claim_id`;
             without an area it stays `req.claim/2`. The statement is the same either way (area
             supplies no meaning). The reader also proposes lines for `.req/areas` (`<path>
             <label>`, the label the docname as written, cut to the kept depth); `--write` appends
             those the registry does not declare, and a path it already declares is left alone.
             Without `--areas` every byte of the output stays as it was.

The output accounts for every need in the file and ends with one line that must add up:

  needs N = drafted D + skipped S + not selected M (by type: ...)

Exit codes, the same three-state discipline as req (ADR 0027 section 7):

  0  drafted (dry run, or written with --write)
  1  a definite problem — nothing drafted, a named type the file does not have, a need
     already drafted from this origin, or (--areas) a docname part that leaves no key or two
     names that make one path; nothing was written
  2  usage — a missing or malformed argument, a bad date
  3  UNVERIFIABLE — the export, the corpus, the subject registry or `req` cannot be read
     honestly (not JSON, no `versions`, several versions and no `current_version`, a corpus the
     loader rejects, a `req` judging another claim schema, `--areas` against a `req` that does
     not report `claim_schema req.claim/3`, an `.req/areas` that is malformed)

Nothing derived is ever written, and no pin is guessed.
"""
import argparse
import datetime
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

# --- conventions (the whole mapping) ---------------------------------------

# The reasons a selected need is not drafted, in the order they are checked.
REASONS = ("external", "no content", "rst directive", "multi-paragraph", "rst role")

# An inline reStructuredText role: `:name:` immediately followed by a backtick.
ROLE_RE = re.compile(r":[A-Za-z][A-Za-z0-9_.+-]*:`")

# Fields the report does not list as "not carried": the content is the statement, the id is the
# provenance, and the document name and line are the header comment.
USED_FIELDS = ("content", "id", "docname", "lineno")

# A claim file's declared id, and every source line, read without a YAML parser: the corpus
# contract puts the id at the top level, and `yaml_serde` refuses a file without it.
CLAIM_ID_RE = re.compile(r"^claim_id:[ \t]*(?P<value>\S.*?)[ \t]*$", re.MULTILINE)
CLAIM_SOURCE_RE = re.compile(r"^[ \t]+source:[ \t]*(?P<value>.+?)[ \t]*$", re.MULTILINE)
REGISTRY_VERSION_RE = re.compile(r"^version:[ \t]*(?P<value>\d+)[ \t]*$", re.MULTILINE)

VERSION_RULES_RE = re.compile(r"^[ \t]*rules_v[ \t]+(?P<value>\d+)[ \t]*$", re.MULTILINE)
VERSION_SCHEMA_RE = re.compile(r"^[ \t]*claim_schema[ \t]+(?P<value>\S+)[ \t]*$", re.MULTILINE)
CLAIM_SCHEMA_RE = re.compile(r"^req\.claim/\d+$")
# The one schema `CLAIM_TEMPLATE` writes; a binary reporting another cannot load the draft.
DRAFT_SCHEMA = "req.claim/2"
# The schemas a `req` may report for this tool to draft against (ADR 0051 decision 6): a `req.claim/3`
# build loads a `req.claim/2` draft as it is, and the draft carries no area, so the tool keeps
# drafting `DRAFT_SCHEMA` and accepts either. Any other value is refused. A later format that keeps
# `req.claim/2` readable is added here deliberately, not discovered.
ACCEPTED_SCHEMAS = (DRAFT_SCHEMA, "req.claim/3")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

# The one schema a draft with an `area` declares (ADR 0051 decision 6): `area` is `req.claim/3` only.
AREA_SCHEMA = "req.claim/3"
AREAS_PATH = ".req/areas"
# A key is made from an identifier by these rules, one rule in both readers (docs/specs/I-1).
KEY_RUN_RE = re.compile(r"[^a-z0-9]+")
ASCII_LOWER = {code: code + 32 for code in range(ord("A"), ord("Z") + 1)}
# The reasons a drafted need is filed under no area, in report order.
NO_AREA_REASONS = ("no docname",)

CLAIMS_DIR = "claims"
REGISTRY_PATH = "aliases/subjects.yml"
REQ_BINARY = "req"

# The draft claim, byte for byte: a fixed template, not a serialized mapping (python3's standard
# library has no YAML writer, and a serializer's diff is unreadable). It is a `req.claim/2`
# claim (`req.claim/3`, with an `area:` line, only under --areas): the authored fields only, nothing
# derived and no pin (ADR 0022). The header comment
# names the export and the place in the source the need came from; it carries no source field.
CLAIM_TEMPLATE = """\
# A draft claim, drafted from the Sphinx-Needs export {export} ({location}). The need's id is kept
# in rationale.source. Nothing derived is stored: `req canon` shows the parse this statement
# derives. A draft is a proposal (ADR 0004): edit the statement, add the applicability and
# relations it needs, and pin a load-bearing bound under `pins:` if it has one.
schema: {schema}
claim_id: {claim_id}
{area_line}statement: {statement}
applicability: []
rationale:
  source: {quoted_source}
  date: {date}
  rejected_alternatives: []
  assumptions: []
relations: []
"""

HELP_EPILOG = """\
what is drafted, and what is named instead (no heuristics, no model):
  --type     the need types to read; required, repeatable, no default. A type the file does not
             have is exit 1 and the message lists the types it does have. Needs of other types
             are not selected: counted by type, never silently dropped.
  --status   optional, repeatable: only needs with one of these statuses are considered; the
             rest count as not selected. The status itself is never written to a claim.

  A selected need becomes a draft only when its `content` is one plain paragraph (lines trimmed,
  joined with one space). Otherwise it is skipped and named, by the first reason that applies:
    external         is_external is true: the export does not own it
    no content       `content` is absent, null or blank
    rst directive    a line starts with `..`
    multi-paragraph  a blank line inside the trimmed content
    rst role         an inline `:role:` followed by a backtick
  The reader does not strip markup, and it does not cut a need down to its first paragraph.

provenance: each draft's rationale.source is `<origin>#<need id>`; req allocates the claim id.
A selected, draftable need already drafted from the same origin is refused (exit 1, nothing
written), so a second run over the same export does not duplicate; the same file under another
--origin is a different provenance and is allowed. A partial overlap is refused too.

not carried, and said so in the report: status, tags, safety, security, reqtype, every other
field, and the typed links. The link types are the ones the export declares: a field whose
`versions.<v>.needs_schema.properties.<name>.field_type` is `links` or `backlinks` (the report says
"declared by needs_schema"). An export with no `needs_schema` (an older Sphinx-Needs) falls back to
inference, and the report says "inferred": a non-empty list of strings that is `links`, ends in
`_back`, or has a `<name>_back` partner field somewhere in the file. Every other field is listed
under "fields", so nothing is hidden; `relations` stays `[]`.

--areas     opt-in: file each draft under an area made from the need's `docname` (ADR 0051).
             The docname is split on `/`; each part becomes a key: lowercase it, replace each run
             of characters outside [a-z0-9] with one `-`, strip `-` from both ends. The keys
             joined by `/` are the `area`, so `requirements/Diag_Startup` is
             `requirements/diag-startup`. A section title or number is never a key (both move
             when the source is edited).
             A need with no `docname` gets no area and stays `req.claim/2`. A docname part that
             leaves no key, or two names that make one path (`Diag_Startup` and `diag-startup`),
             is exit 1 naming them: they are never merged.
             A draft with an area is `req.claim/3` with `area: <path>` after `claim_id`; the
             statement is exactly what it is without `--areas`.
             The area registry `.req/areas` (`<path> <label>`, one line per area, the line order
             is the reading order) is proposed, not imposed: a dry run prints the lines it would
             add, `--write` appends those for paths the file does not declare (in the order the
             export first shows them; the file is created when there is none). A path it already
             declares is left alone, whatever its label. The label is the docname as the export
             writes it, cut to the kept depth. If the claims then fail to write, the registry's
             earlier bytes come back.
             The report gains `areas: <P> paths; filed <F> of <D> drafted`, `no area: <K> (no
             docname <n>)` when K > 0, and `registry: <A> lines to add, <E> already declared`.
             `--areas` needs a `req` that reports `claim_schema req.claim/3`; any other is exit 3.
  --area-depth N  with --areas: keep only the outermost N parts of each docname (N >= 1).

--date is the rationale date (the retrieval date of the export) and is required. It is never
borrowed from the clock or from a commit.

The corpus root is --root, else $REQ_ROOT, else the working directory — never an ancestor: this
tool writes claims, so a run inside a subdirectory must not draft into a parent you never
named. A relative --from is resolved against that root.

dry run is the default; --write writes. The report ends with one line that adds up:
  needs N = drafted D + skipped S + not selected M (by type: ...)
`needs_amount` and the other count keys of an export are never trusted: N is the size of the
needs mapping that was read.

exit: 0 drafted (dry run, or written) · 1 a definite problem, nothing written (nothing
drafted, a --type the file does not have, a need already drafted from this origin, or with
--areas a docname part that leaves no key or two names that make one path) · 2 usage · 3 the
export, the corpus, the subject registry, the area registry or `req` cannot be read.

This script is a starting point to COPY and adjust for your project — it is not a req
subcommand.
"""


class Unverifiable(Exception):
    """A question this run could not settle (exit 3)."""


class Problem(Exception):
    """A definite finding (exit 1)."""


class Usage(Exception):
    """A malformed invocation (exit 2)."""


# --- the pure half (--self-check table-drives all of it) --------------------


def one_line(text):
    """`text` as one printable line: control characters and every line break become a space.

    A need id, a document name or a path is *text* in a report line or a comment, not YAML; a
    newline (legal in a JSON string) would end the comment and inject a line into the draft.
    """
    cleaned = "".join(
        " " if ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F else char for char in str(text)
    )
    return " ".join(cleaned.split())


def paragraphs_ok(lines):
    """Whether `lines` (the trimmed content, split on newlines) hold no blank line."""
    return all(line.strip() for line in lines)


def classify(need_id, need, types, statuses):
    """`(kind, payload)` for one need.

    kind `not selected`: payload is the need's type name (`(none)` when it has none).
    kind `skip`:         payload is the first reason in `REASONS` that applies.
    kind `draft`:        payload is the statement.
    A need is selected by type (and by status when `--status` was given); the reasons below are
    checked in the order of `REASONS`. A `content` that is neither text nor null is not a
    question this tool can settle: Unverifiable, naming the need.
    """
    raw_type = need.get("type")
    type_name = raw_type if isinstance(raw_type, str) and raw_type else "(none)"
    status = need.get("status")
    if type_name not in types or (statuses and not (isinstance(status, str) and status in statuses)):
        return "not selected", type_name
    if need.get("is_external") is True:
        return "skip", "external"
    content = need.get("content")
    if content is not None and not isinstance(content, str):
        raise Unverifiable(
            f"need {one_line(need_id)}: `content` is not text ({type(content).__name__}), so "
            "whether it is a statement cannot be judged"
        )
    # reStructuredText counts a newline as a line break, and nothing else: `splitlines` would
    # also split on form feeds and U+2028, and join them with a space.
    trimmed = (content or "").replace("\r\n", "\n").replace("\r", "\n").strip()
    if not trimmed:
        return "skip", "no content"
    lines = trimmed.split("\n")
    if any(line.strip().startswith("..") for line in lines):
        return "skip", "rst directive"
    if not paragraphs_ok(lines):
        return "skip", "multi-paragraph"
    if ROLE_RE.search(trimmed):
        return "skip", "rst role"
    return "draft", " ".join(line.strip() for line in lines)


def is_link_value(value):
    """A typed link's value: a non-empty list of strings."""
    return isinstance(value, list) and bool(value) and all(isinstance(item, str) for item in value)


def declared_link_fields(export, version):
    """The link field names `versions.<version>.needs_schema` declares, or None if it declares none.

    Sphinx-Needs (>= 5) writes a JSON schema whose `properties.<name>.field_type` is `links` for a
    link type and `backlinks` for its partner. None (no schema, or one of another shape) means
    the caller must infer.
    """
    entry = export.get("versions", {}).get(version)
    schema = entry.get("needs_schema") if isinstance(entry, dict) else None
    properties = schema.get("properties") if isinstance(schema, dict) else None
    if not isinstance(properties, dict):
        return None
    return frozenset(
        name
        for name, prop in properties.items()
        if isinstance(prop, dict) and prop.get("field_type") in ("links", "backlinks")
    )


def link_fields_of(need, all_keys, declared=None):
    """The names of the typed-link fields on one need.

    With `declared` (from `needs_schema`) a link field is a declared one holding a non-empty list
    of strings. Without it, it is inferred, and the report says so: a non-empty list of strings
    that is `links`, ends in `_back`, or has a `<name>_back` partner field on some need in the
    file (Sphinx-Needs creates the partner for every link type).
    """
    if declared is not None:
        return sorted(key for key, value in need.items() if key in declared and is_link_value(value))
    return sorted(
        key
        for key, value in need.items()
        if is_link_value(value)
        and (key == "links" or key.endswith("_back") or f"{key}_back" in all_keys)
    )


def meaningful(value):
    """Whether a field holds something: not null, empty, or false."""
    return value is not None and value is not False and value != "" and value != [] and value != {}


def not_carried(drafted, all_keys, declared=None):
    """`(fields, link_names, with_links)` over the drafted needs.

    `fields` are the names of the fields set on at least one drafted need, other than the ones
    the draft uses and the typed links; `link_names` the typed-link fields; `with_links` how many
    drafted needs have at least one.
    """
    fields, links, with_links = set(), set(), 0
    for need in drafted:
        mine = link_fields_of(need, all_keys, declared)
        if mine:
            with_links += 1
        links.update(mine)
        for key, value in need.items():
            if key not in USED_FIELDS and key not in mine and meaningful(value):
                fields.add(key)
    return sorted(fields), sorted(links), with_links


def breakdown(counts):
    """`type n, type n` in type order."""
    return ", ".join(f"{name} {counts[name]}" for name in sorted(counts))


def reconcile_line(total, drafted, skipped, not_selected):
    """The report's last line: the file's needs and where each went."""
    selected_out = sum(not_selected.values())
    line = f"needs {total} = drafted {drafted} + skipped {skipped} + not selected {selected_out}"
    if not_selected:
        line += f" (by type: {breakdown(not_selected)})"
    return line


def select_version(export, requested):
    """`(version key, needs mapping)` from a parsed export, or Unverifiable naming what is wrong."""
    if not isinstance(export, dict):
        raise Unverifiable(
            "the file is JSON but not an object — a Sphinx-Needs export is an object with a "
            "`versions` mapping"
        )
    versions = export.get("versions")
    if not isinstance(versions, dict):
        raise Unverifiable(
            "the file has no `versions` mapping — this is not a needs.json export "
            "(Sphinx-Needs writes `versions.<version>.needs`)"
        )
    if not versions:
        raise Unverifiable("`versions` is empty: there is no version to read")
    names = ", ".join(repr(name) for name in versions)
    key = requested
    if key is None:
        current = export.get("current_version")
        if current is None:
            if len(versions) != 1:
                raise Unverifiable(
                    f"the file holds several versions ({names}) and no `current_version`: pass "
                    "--version-key <version>"
                )
            key = next(iter(versions))
        elif isinstance(current, str):
            key = current
        else:
            raise Unverifiable(f"`current_version` is not text: {current!r}")
    if key not in versions:
        raise Unverifiable(f"version {key!r} is not in the file; it has: {names}")
    entry = versions[key]
    needs = entry.get("needs") if isinstance(entry, dict) else None
    if not isinstance(needs, dict):
        raise Unverifiable(f"`versions.{key}.needs` is not a mapping of need id to need")
    for need_id, need in needs.items():
        if not isinstance(need, dict):
            raise Unverifiable(f"need {one_line(need_id)} is not an object")
    return key, needs


def no_duplicate_keys(pairs):
    """`json` object hook: a repeated key would silently drop a need, so it is refused."""
    seen = {}
    for key, value in pairs:
        if key in seen:
            raise ValueError(f"the key {key!r} appears twice in one object")
        seen[key] = value
    return seen


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
    """Whether a `claims/` entry is claim-shaped — the loader's gate, for the same reason."""
    lowered = name.lower()
    return (
        lowered.endswith(".yml")
        or lowered.endswith(".yaml")
        or claim_number_from_name(name) is not None
    )


def next_claim_number(used):
    """The next free claim number: one after the highest in use, or 1.

    `max + 1` rather than "the first gap": a gap is a claim that was deleted deliberately, and
    handing its id to a different requirement would make history read as if the same claim had
    changed meaning.
    """
    return max(used) + 1 if used else 1


def number_or_none(value):
    """A declared claim id's number, or None when it is not a `REQ-<n>` shape."""
    match = re.fullmatch(r"REQ-(\d+)", value, re.IGNORECASE)
    return int(match.group(1)) if match else None


def parse_version(text):
    """`(rules_v, claim_schema)` from `req --version`, or None."""
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

    Quotes and backslashes alone are not enough: a control character (a NUL pasted in from a
    table, a C1 control from a Windows-sourced paste) would produce a file the loader cannot
    parse while the tool still reported success. A backslash-x escape is YAML's own form for
    those, and the loader parses it back to the same character.
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
        elif ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F:
            out.append(f"\\x{ord(char):02x}")
        elif char in "\u2028\u2029":
            # The YAML loader counts these as line breaks and folds them; the escape keeps the character.
            out.append(f"\\u{ord(char):04x}")
        else:
            out.append(char)
    return '"' + "".join(out) + '"'


_SIMPLE_ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "/": "/", " ": " ", '"': '"', "\\": "\\"}


def unquote_scalar(value):
    """The text of a stored YAML scalar: double-quoted (what `yaml_quote` writes), single-quoted
    or plain. A trailing comment after the closing quote is ignored.

    Used to compare a stored `rationale.source` with the one a draft would write, so it reads
    back what `yaml_quote` wrote exactly and is lenient about the rest.
    """
    text = value.strip()
    if text.startswith('"'):
        out, index = [], 1
        while index < len(text):
            char = text[index]
            if char == '"':
                break
            if char == "\\" and index + 1 < len(text):
                index += 1
                code = text[index]
                width = {"x": 2, "u": 4, "U": 8}.get(code)
                digits = text[index + 1 : index + 1 + width] if width else ""
                if width and len(digits) == width:
                    try:
                        out.append(chr(int(digits, 16)))
                        index += width + 1
                        continue
                    except (ValueError, OverflowError):
                        pass
                out.append(_SIMPLE_ESCAPES.get(code, code))
            else:
                out.append(char)
            index += 1
        return "".join(out)
    if text.startswith("'"):
        out, index = [], 1
        while index < len(text):
            char = text[index]
            if char == "'":
                if text[index + 1 : index + 2] == "'":
                    out.append("'")
                    index += 2
                    continue
                break
            out.append(char)
            index += 1
        return "".join(out)
    return text.split(" #", 1)[0].rstrip()


def render_claim(export, location, statement, source, date, claim_id, schema, area=None):
    """One draft claim file's exact bytes. An `area` makes the draft `AREA_SCHEMA` with an `area:`
    line after `claim_id`; with none, `schema` is written and the bytes are what they always were."""
    return CLAIM_TEMPLATE.format(
        export=one_line(export),
        location=one_line(location),
        schema=AREA_SCHEMA if area is not None else schema,
        claim_id=claim_id,
        area_line=f"area: {area}\n" if area is not None else "",
        statement=yaml_quote(statement),
        quoted_source=yaml_quote(source),
        date=date,
    )


def area_key(text):
    """The key a source identifier makes: lowercase it (ASCII only, so no locale or Unicode case
    mapping decides), replace each run of characters outside `[a-z0-9]` with one `-`, strip `-`
    from both ends. Empty when nothing is left (the caller names the identifier)."""
    return KEY_RUN_RE.sub("-", str(text).translate(ASCII_LOWER)).strip("-")


def docname_parts(need):
    """A need's docname split on `/`, or None when it has none (absent, null, not text, blank)."""
    docname = need.get("docname")
    if not isinstance(docname, str) or not docname.strip():
        return None
    return docname.split("/")


def need_area(need_id, need, depth):
    """`("area", path, parts)` for a need with a docname, `("none", reason)` for one without.

    `parts` are the docname's parts as written, cut to `depth` (None keeps all of them); `path` is
    their keys joined by `/`. A part that leaves no key is a Problem naming the docname."""
    parts = docname_parts(need)
    if parts is None:
        return "none", "no docname"
    kept = parts if depth is None else parts[:depth]
    keys = []
    for part in kept:
        key = area_key(part)
        if not key:
            raise Problem(
                f"need {one_line(need_id)}: the docname {one_line(need['docname'])!r} has a part "
                f"({one_line(part)!r}) that leaves no key (a key needs a letter or a digit from "
                "[A-Za-z0-9]) — rename the document, or pass --area-depth to stop above it"
            )
        keys.append(key)
    return "area", "/".join(keys), kept


def plan_areas(entries, depth):
    """`(placed, wanted)` for `[(need id, need)]`, in file order.

    `placed[i]` is `("area", path)` or `("none", reason)`. `wanted` is `[(path, label)]` in the
    order the export first shows each path; the label is the docname as written, cut to the kept
    depth, as one line. Two different spellings that make one path — at any depth, so
    `Requirements/a` and `requirements/b` too — are a Problem naming both: a place is never merged.
    """
    owners, placed, wanted, seen = {}, [], [], set()
    for need_id, need in entries:
        found = need_area(need_id, need, depth)
        if found[0] == "none":
            placed.append(found)
            continue
        _kind, path, parts = found
        keys = path.split("/")
        for index, key in enumerate(keys):
            prefix = "/".join(keys[: index + 1])
            first = owners.setdefault(prefix, (parts[index], need_id, need["docname"]))
            if first[0] != parts[index]:
                raise Problem(
                    f"two different names make the area path {prefix!r}: "
                    f"{one_line(first[0])!r} (need {one_line(first[1])}, docname "
                    f"{one_line(first[2])!r}) and {one_line(parts[index])!r} (need "
                    f"{one_line(need_id)}, docname {one_line(need['docname'])!r}) — they are never "
                    "merged: rename one of the documents"
                )
        placed.append(("area", path))
        if path not in seen:
            seen.add(path)
            wanted.append((path, one_line("/".join(parts))))
    return placed, wanted


def declared_paths(text):
    """The paths an `.req/areas` text declares: the first word of each line that is not blank and
    does not start with `#`. (The file is validated by `req` itself; this only reads the keys.)"""
    paths = set()
    for raw in text.split("\n"):
        line = raw.rstrip("\r").strip()
        if line and not line.startswith("#"):
            paths.add(re.split(r"[ \t]", line, maxsplit=1)[0])
    return paths


def registry_suffix(existing, wanted):
    """`(suffix, added, declared)` for the registry: the bytes to append to `existing` (None when
    the file does not exist), the `[(path, label)]` lines that adds, and how many wanted paths the
    file already declares. A declared path is left alone whatever its label."""
    have = declared_paths(existing or "")
    added = [(path, label) for path, label in wanted if path not in have]
    declared = len(wanted) - len(added)
    if not added:
        return "", added, declared
    separator = "\n" if existing and not existing.endswith("\n") else ""
    return separator + "".join(f"{path} {label}\n" for path, label in added), added, declared


def location_of(need):
    """`<docname>:<lineno>` for a need, `?` for a part the export does not give."""
    docname = need.get("docname")
    lineno = need.get("lineno")
    name = one_line(docname) if isinstance(docname, str) and docname.strip() else "?"
    line = lineno if isinstance(lineno, int) and not isinstance(lineno, bool) else "?"
    return f"{name}:{line}"


# --- the filesystem half ----------------------------------------------------


class Corpus:
    """What the importer needs to know about the corpus it drafts into."""

    def __init__(self, root, used, sources, registry_v):
        self.root = root
        self.used = used
        self.sources = sources  # claim id -> set of the `source:` values it records
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
        # A directory (or a link to one) is refused *before* the name filter: `claims/` is flat,
        # and the loader refuses the corpus, so the run must not report drafts it wrote into a
        # corpus that is already red.
        if entry.is_dir():
            raise Unverifiable(
                f"{entry}: `{CLAIMS_DIR}/` is flat, one claim per file — a directory here is an "
                "error, not a place a claim can live"
            )
        if not is_claim_candidate(entry.name):
            continue
        if not entry.is_file() or entry.is_symlink():
            raise Unverifiable(
                f"{entry}: a claim file must be a regular file — `claims/` is flat, one claim "
                "per file"
            )
        try:
            text = entry.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            raise Unverifiable(f"cannot read {entry}: {error}") from error
        match = CLAIM_ID_RE.search(text)
        if match is None:
            raise Unverifiable(
                f"{entry}: no `claim_id:` line — this corpus cannot be read as a corpus; "
                "`req canon` names the defect"
            )
        claim_id = match.group("value").strip()
        # Both numbers are reserved: the declared id is what the claim is, and the file name is
        # where the next draft would land.
        declared = number_or_none(claim_id)
        if declared is not None:
            used.add(declared)
        named = claim_number_from_name(entry.name)
        if named is not None:
            used.add(named)
        sources[claim_id] = {
            unquote_scalar(found.group("value")) for found in CLAIM_SOURCE_RE.finditer(text)
        }

    registry = root / REGISTRY_PATH
    try:
        registry_text = registry.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise Unverifiable(
            f"cannot read the subject registry ({registry}): {error} — run `req init` here "
            "first"
        ) from error
    version = REGISTRY_VERSION_RE.search(registry_text)
    if version is None:
        raise Unverifiable(
            f"{registry}: no `version:` line — the registry cannot be read, so the parse the "
            "drafts derive could not be named"
        )
    return Corpus(root, used, sources, int(version.group("value")))


def req_binary():
    """The `req` on PATH — the binary whose judgement this tool defers to."""
    binary = shutil.which(REQ_BINARY)
    if binary is None:
        raise Unverifiable(
            "`req` is not on PATH — install it at a pinned rev (docs/adopting.md), because a "
            "that binary judges the drafts and must read the schema they declare"
        )
    return binary


def loader_check(binary, root, targets, texts, registry=None):
    """Run the corpus plus the drafts through the real loader (`req canon`) before anything is
    written, in a scratch copy of the corpus, and raise Unverifiable if it does not load.

    The loader is the only judge of a claim file: a template that drifted from the format would
    be written and reported as drafted all the same. The same run is what turns an older-schema
    corpus away (its message names tools/migrate_claims.py).

    `registry` is the text `.req/areas` would hold after the run (None: there would be none). The
    scratch copy gets it, and `req trace --format json`, the command that validates it, judges it.
    """
    with tempfile.TemporaryDirectory(prefix="req-needs-") as scratch:
        copy = pathlib.Path(scratch)
        for name in (CLAIMS_DIR, "aliases"):
            if (root / name).is_dir():
                shutil.copytree(root / name, copy / name)
        (copy / CLAIMS_DIR).mkdir(exist_ok=True)
        # `copytree` copies the directory's mode: a read-only `claims/` must not stop the drafts
        # from being placed in the copy, since it is the loader that is being asked.
        os.chmod(copy / CLAIMS_DIR, 0o755)
        for (_claim_id, target), text in zip(targets, texts):
            (copy / CLAIMS_DIR / target.name).write_text(text, encoding="utf-8")
        if registry is not None:
            (copy / AREAS_PATH).parent.mkdir(exist_ok=True)
            (copy / AREAS_PATH).write_text(registry, encoding="utf-8")
        try:
            completed = subprocess.run(
                [binary, "--root", str(copy), "canon"],
                capture_output=True, text=True, check=False,
            )
            traced = (
                subprocess.run(
                    [binary, "--root", str(copy), "trace", "--format", "json"],
                    capture_output=True, text=True, check=False,
                )
                if registry is not None and completed.returncode == 0
                else None
            )
        except OSError as error:
            raise Unverifiable(f"cannot run `{binary} canon`: {error}") from error
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or "no output"
        raise Unverifiable(
            f"the corpus plus these drafts does not load (`req canon` exited "
            f"{completed.returncode}), so nothing was written. The loader says why:\n{detail}"
        )
    # Exit 2 is the usage class: `req trace --format json` refuses a malformed registry that way,
    # naming each bad line. Its other exits are verdicts about the corpus, not about the registry.
    if traced is not None and traced.returncode == 2:
        raise Unverifiable(
            f"the area registry this run would leave ({AREAS_PATH}) is refused by `req`, so nothing "
            f"was written. `req` says why:\n{traced.stderr.strip() or 'no output'}"
        )


def check_registry(binary, root):
    """The text of `<root>/.req/areas` (None when there is none), after `req` has judged it.

    A registry that is malformed before the run is not this tool's to repair: it is Unverifiable,
    with what `req` names (every bad `.req/areas:<line>`). `req trace --format json` is the
    command that reads it, and exit 2 is how it refuses one."""
    path = root / AREAS_PATH
    if not path.exists() and not path.is_symlink():
        return None
    try:
        text = path.read_bytes().decode("utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise Unverifiable(f"cannot read the area registry ({path}): {error}") from error
    try:
        completed = subprocess.run(
            [binary, "--root", str(root), "trace", "--format", "json"],
            capture_output=True, text=True, check=False,
        )
    except OSError as error:
        raise Unverifiable(f"cannot run `{binary} trace`: {error}") from error
    if completed.returncode == 2:
        raise Unverifiable(
            f"the area registry ({path}) is malformed, and this tool does not repair one. `req` "
            f"says:\n{completed.stderr.strip() or 'no output'}"
        )
    return text


def read_axes(areas=False):
    """`(rules_v, DRAFT_SCHEMA)`, after asking the `req` on PATH what it reports.

    The schema a draft declares is `DRAFT_SCHEMA`, always — never the one the binary reports (a
    draft with an area declares `AREA_SCHEMA`, see `render_claim`). The reported `claim_schema` only
    decides whether this tool may draft against that build at all (`ACCEPTED_SCHEMAS`); with
    `areas` it must be `AREA_SCHEMA`, since a `req.claim/2` build would refuse `area`.
    `rules_v` is the rules version the drafts' parse is derived under (reported)."""
    binary = req_binary()
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
    if axes[1] not in ACCEPTED_SCHEMAS:
        raise Unverifiable(
            f"`{binary}` reports claim_schema {axes[1]}, but this tool drafts {DRAFT_SCHEMA} "
            f"claims and accepts a req that reports {' or '.join(ACCEPTED_SCHEMAS)}: a draft under "
            "another schema would not load — use a matching req build or the matching copy of "
            "this tool"
        )
    if areas and axes[1] != AREA_SCHEMA:
        raise Unverifiable(
            f"`{binary}` reports claim_schema {axes[1]}, but --areas drafts {AREA_SCHEMA} claims (an "
            f"`area` needs {AREA_SCHEMA}) and accepts only a req that reports {AREA_SCHEMA}: a draft "
            "with an area would not load — use a matching req build, or run without --areas"
        )
    # The binary was judged acceptable; the draft declares the schema this tool is written for.
    return axes[0], DRAFT_SCHEMA


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
    overwritten. What this run did write is removed on failure.
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
            # cleanup that only knows completed writes would leave it behind.
            written.append(target)
            with handle:
                handle.write(text)
    except OSError as error:
        remove_written(written)
        raise Unverifiable(f"cannot write {error.filename or 'the claims'}: {error}") from error
    except BaseException:
        # Anything that ends the write — a refusal, an interrupt — takes the claims it wrote with it.
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


class RegistryWrite:
    """The change a run made to `.req/areas`, and how to take it back."""

    def __init__(self, path, before, made_dir):
        self.path = path
        self.before = before  # the file's earlier bytes, or None when the run created it
        self.made_dir = made_dir

    def rollback(self):
        """Put the earlier bytes back, or remove the file the run created. Reported, never swallowed."""
        try:
            if self.before is None:
                self.path.unlink()
                if self.made_dir:
                    self.path.parent.rmdir()
            else:
                self.path.write_bytes(self.before)
        except OSError as error:
            print(
                f"[UNVERIFIABLE] could not restore {self.path} after a failed run — restore it by "
                f"hand: {error}",
                file=sys.stderr,
            )


def write_registry(path, existing, suffix):
    """Append `suffix` to `path` (creating it, and `.req/`, when `existing` is None); the
    `RegistryWrite` that undoes it. `existing` is the text read before, so the bytes to restore
    are exactly those."""
    made_dir = not path.parent.exists()
    before = None if existing is None else path.read_bytes()
    try:
        path.parent.mkdir(exist_ok=True)
        handle = open(path, "xb" if before is None else "ab")
    except OSError as error:
        raise Unverifiable(f"cannot write {path}: {error}") from error
    undo = RegistryWrite(path, before, made_dir)
    try:
        with handle:
            handle.write(suffix.encode("utf-8"))
    except OSError as error:
        undo.rollback()
        raise Unverifiable(f"cannot write {path}: {error}") from error
    return undo


# --- the driver -------------------------------------------------------------


def load_export(path, label):
    """The parsed export at `path`, or Unverifiable."""
    try:
        text = path.read_bytes().decode("utf-8-sig")
    except (OSError, UnicodeDecodeError) as error:
        raise Unverifiable(f"cannot read the export {label}: {error}") from error
    try:
        return json.loads(text, object_pairs_hook=no_duplicate_keys)
    except ValueError as error:
        raise Unverifiable(f"{label} is not JSON a reader can trust: {error}") from error


def sort_needs(needs, types, statuses):
    """Classify every need, in file order: `(drafts, skips, not_selected)`.

    `drafts` is `[(need id, need, statement)]`, `skips` is `[(need id, reason)]`, and
    `not_selected` maps a type name to its count.
    """
    drafts, skips, not_selected = [], [], {}
    for need_id, need in needs.items():
        kind, payload = classify(need_id, need, types, statuses)
        if kind == "draft":
            drafts.append((need_id, need, payload))
        elif kind == "skip":
            skips.append((need_id, payload))
        else:
            not_selected[payload] = not_selected.get(payload, 0) + 1
    return drafts, skips, not_selected


def type_counts(needs):
    """The need types a version holds, with counts, in type order."""
    counts = {}
    for need in needs.values():
        raw = need.get("type")
        name = raw if isinstance(raw, str) and raw else "(none)"
        counts[name] = counts.get(name, 0) + 1
    return counts


def skip_lines(skips):
    return [f"skip {one_line(need_id)}  {reason}" for need_id, reason in skips]


def run(args):
    """Draft, report, and (with --write) write. Returns the exit code."""
    corpus = read_corpus(resolve_root(args.root))

    # The export is resolved against the **corpus root**, not the working directory, like the
    # sibling `adopt.py`: one basis for reading and for the name the report gives.
    document = pathlib.Path(args.document).expanduser()
    if not document.is_absolute():
        document = corpus.root / document
    document = pathlib.Path(os.path.normpath(str(document)))
    label = relative(corpus.root, document)

    export = load_export(document, args.document)
    version, needs = select_version(export, args.version_key)

    present = type_counts(needs)
    missing = [name for name in args.type if name not in present]
    if missing:
        have = ", ".join(f"{name} ({present[name]})" for name in sorted(present)) or "none"
        raise Problem(
            f"--type names a type {label} does not have: {', '.join(missing)}\n"
            f"  the file has: {have}\n"
            "  there is no default type list: name the types your project uses for requirements"
        )

    drafts, skips, not_selected = sort_needs(needs, args.type, args.status)
    total = len(needs)
    recon = reconcile_line(total, len(drafts), len(skips), not_selected)

    if not drafts:
        # The skipped needs are named even here: the finding is that nothing was drafted, and
        # *why* each selected need was skipped is its remedy.
        lines = "".join(f"\n  {line}" for line in skip_lines(skips))
        raise Problem(f"nothing drafted from {label} (version {version}){lines}\n  {recon}")

    # A need already drafted from this origin is refused before anything else: the drafts are in
    # the corpus, and drafting them again would duplicate claims. Only a selected, draftable need
    # counts — a skipped one was never a draft.
    clashes = []
    for need_id, _need, _statement in drafts:
        wanted = f"{args.origin}#{need_id}"
        owners = sorted(
            claim_id for claim_id, stored in corpus.sources.items() if wanted in stored
        )
        if owners:
            clashes.append(f"  {one_line(need_id)}  ({', '.join(owners)})")
    if clashes:
        raise Problem(
            f"refusing to draft: {len(clashes)} need(s) in {label} are already drafted from "
            f"this origin ({args.origin}):\n" + "\n".join(clashes) + "\n"
            "  edit those claims, or delete them deliberately, before drafting from this origin "
            "again — or pass the --origin the export was retrieved from this time"
        )

    rules_v, schema = read_axes(args.areas)

    # --areas: where each draft is filed, and the registry lines that proposes. Planned before any
    # claim is rendered, so a name that cannot be a key stops the run with nothing written.
    placed, wanted, existing, suffix, added, registry_declared = None, [], None, "", [], 0
    if args.areas:
        placed, wanted = plan_areas([(need_id, need) for need_id, need, _s in drafts], args.area_depth)
        existing = check_registry(req_binary(), corpus.root)
        suffix, added, registry_declared = registry_suffix(existing, wanted)

    number = next_claim_number(corpus.used)
    targets, texts = [], []
    for offset, (need_id, need, statement) in enumerate(drafts):
        claim_id = f"REQ-{number + offset:04d}"
        targets.append((claim_id, corpus.root / CLAIMS_DIR / f"{claim_id}.yml"))
        texts.append(
            render_claim(
                export=label,
                location=location_of(need),
                statement=statement,
                source=f"{args.origin}#{need_id}",
                date=args.date,
                claim_id=claim_id,
                schema=schema,
                area=placed[offset][1] if placed is not None and placed[offset][0] == "area" else None,
            )
        )

    final_registry = None if not args.areas or (existing is None and not added) else (existing or "") + suffix
    loader_check(req_binary(), corpus.root, targets, texts, final_registry)

    written, registry_write = [], None
    if args.write:
        for _claim_id, target in targets:
            if target.exists() or target.is_symlink():
                raise Problem(
                    f"refusing to write {target}: it already exists — a claim file is never "
                    "overwritten"
                )
        # The registry first, then the claims: if anything fails once the registry changed, its
        # earlier bytes come back, so a run is all of it or none of it.
        try:
            if added:
                registry_write = write_registry(corpus.root / AREAS_PATH, existing, suffix)
            written = write_claims([target for _claim_id, target in targets], texts)
        except BaseException:
            if registry_write is not None:
                registry_write.rollback()
            raise

    all_keys = {key for need in needs.values() for key in need}
    declared = declared_link_fields(export, version)
    fields, link_names, with_links = not_carried(
        [need for _id, need, _s in drafts], all_keys, declared
    )
    filing = (placed, wanted, added, registry_declared) if args.areas else None
    report(args, corpus, label, version, drafts, skips, targets, written, rules_v,
           (fields, link_names, with_links, declared is not None), recon, filing, registry_write)
    return 0


def report(args, corpus, label, version, drafts, skips, targets, written, rules_v, carried, recon,
           filing=None, registry_write=None):
    """Print what was drafted, what was skipped and why, what was not carried, and what was
    written — ending with the line that adds up."""
    fields, link_names, with_links, from_schema = carried
    print(f"export: {label} (version {version})")
    print(f"origin: {args.origin}")
    print(f"date: {args.date} (--date)")
    print(f"root: {corpus.root}")
    print(f"req: rules_v {rules_v}, registry {corpus.registry_v}")
    print(f"types: {', '.join(args.type)}" + (f"; statuses: {', '.join(args.status)}" if args.status else ""))
    print()
    for (need_id, need, statement), (claim_id, _target) in zip(drafts, targets):
        print(f"draft {claim_id}  {one_line(need_id)}  {location_of(need)}")
        print(f"    statement: {statement}")
    for line in skip_lines(skips):
        print(line)
    print()
    shown = ", ".join(fields) if fields else "none"
    how = "declared by needs_schema" if from_schema else "inferred: no needs_schema"
    carried_links = f" ({', '.join(link_names)}; {how})" if link_names else f" ({how})"
    print(
        f"not carried: {shown}; typed links on {with_links} of {len(drafts)} drafted needs"
        f"{carried_links} — `relations` stays []"
    )
    if filing is not None:
        placed, wanted, added, registry_declared = filing
        for line in area_report_lines(placed, wanted, added, registry_declared, len(drafts)):
            print(line)
    print(
        "note: the source need ids are ignored for the claim_id (req allocates it) and kept as "
        "provenance in rationale.source"
    )
    print(
        "next: `req canon` shows the parse each draft derives (nothing derived is stored); a "
        "draft is a proposal — edit it, add its applicability and relations, pin a load-bearing "
        "bound under `pins:`"
    )
    if written:
        names = ", ".join(relative(corpus.root, target) for target in written)
        print(f"wrote: {len(written)} claim(s) — {names}")
        if registry_write is not None:
            print(f"wrote: {len(filing[2])} line(s) appended to {AREAS_PATH}")
    else:
        print("dry run: nothing written — pass --write to write them")
    print(recon)


def area_report_lines(placed, wanted, added, registry_declared, drafted):
    """The report's `--areas` lines: where the drafts were filed, why some were not, and the registry.

    `areas:` counts distinct paths and the drafts filed; `no area:` (only when some draft has none)
    counts them per reason in `NO_AREA_REASONS` order; `registry:` counts the lines to add and the
    paths already declared; each line to add follows as `  <path> <label>`, ready to paste into `.req/areas`."""
    lines = []
    filed = sum(1 for kind, _value in placed if kind == "area")
    lines.append(f"areas: {len(wanted)} paths; filed {filed} of {drafted} drafted")
    reasons = {}
    for kind, value in placed:
        if kind == "none":
            reasons[value] = reasons.get(value, 0) + 1
    if reasons:
        shown = ", ".join(f"{name} {reasons[name]}" for name in NO_AREA_REASONS if name in reasons)
        lines.append(f"no area: {sum(reasons.values())} ({shown})")
    lines.append(f"registry: {len(added)} lines to add, {registry_declared} already declared")
    lines.extend(f"  {path} {label}" for path, label in added)
    return lines


def field(text, prefix):
    """The first line of `text` starting with `prefix`, trimmed — a field, not a substring."""
    for line in text.splitlines():
        if line.startswith(prefix):
            return line.strip()
    return None


def self_check():
    """Table-drive the pure half. No filesystem, no corpus, no `req`, no export.

    Every row is a fact the conventions promise: what is selected, which reason a need is
    skipped for and in what order, how the statement is joined, what the report counts, and what
    a draft carries. The filesystem half is exercised by `tests/needs_import.rs`.
    """
    types = ("req",)

    def judged(content, **more):
        need = {"type": "req", "content": content}
        need.update(more)
        return classify("N", need, types, ())

    draft = render_claim(
        export="needs.json",
        location="docs/a:12",
        statement='The tool shall print "a: b" and a back\\slash.',
        source="https://example.test/needs.json#N_1",
        date="2026-11-05",
        claim_id="REQ-0016",
        schema="req.claim/2",
    )
    keys = {"derived_from", "derived_from_back", "tags", "links", "links_back", "satisfied_by"}
    rows = [
        ("select: a named type is selected", judged("Ok.")[0], "draft"),
        (
            "select: a type that is not named is not selected, under its type",
            classify("N", {"type": "doc", "content": "x"}, types, ()),
            ("not selected", "doc"),
        ),
        (
            "select: a need with no type is counted under (none)",
            classify("N", {"content": "x"}, types, ()),
            ("not selected", "(none)"),
        ),
        (
            "select: --status keeps only the named statuses",
            classify("N", {"type": "req", "status": "open", "content": "x"}, types, ("valid",)),
            ("not selected", "req"),
        ),
        (
            "select: a null status never matches --status",
            classify("N", {"type": "req", "status": None, "content": "x"}, types, ("valid",)),
            ("not selected", "req"),
        ),
        (
            "select: a matching status is selected",
            classify("N", {"type": "req", "status": "valid", "content": "x"}, types, ("valid",))[0],
            "draft",
        ),
        ("statement: wrapped lines are trimmed and joined with one space",
         judged("  first line,\n   second line.  \n"), ("draft", "first line, second line.")),
        ("statement: CRLF line breaks are line breaks", judged("a\r\nb"), ("draft", "a b")),
        ("skip: an external need", judged("Ok.", is_external=True), ("skip", "external")),
        ("skip: external is checked before content", judged("", is_external=True), ("skip", "external")),
        ("skip: is_external false is not external", judged("Ok.", is_external=False)[0], "draft"),
        ("skip: an empty content", judged(""), ("skip", "no content")),
        ("skip: a blank content", judged(" \n\t "), ("skip", "no content")),
        ("skip: a null content", judged(None), ("skip", "no content")),
        (
            "skip: an absent content",
            classify("N", {"type": "req"}, types, ()),
            ("skip", "no content"),
        ),
        ("skip: a directive", judged(".. note::\n   body"), ("skip", "rst directive")),
        ("skip: an indented directive line", judged("Text.\n   .. image:: x"), ("skip", "rst directive")),
        (
            "skip: a directive is named before the blank line that follows it",
            judged(".. code-block:: text\n\n   x"),
            ("skip", "rst directive"),
        ),
        ("skip: a blank line inside is a second paragraph", judged("One.\n\nTwo."), ("skip", "multi-paragraph")),
        (
            "skip: a whitespace-only line inside is a blank line",
            judged("One.\n  \nTwo."),
            ("skip", "multi-paragraph"),
        ),
        (
            "skip: multi-paragraph is named before a role",
            judged("One.\n\nSee :ref:`x`."),
            ("skip", "multi-paragraph"),
        ),
        ("skip: an inline role", judged("See :need:`X_1` here."), ("skip", "rst role")),
        ("skip: a domain role", judged("Call :py:func:`run`."), ("skip", "rst role")),
        (
            "skip: a backtick that follows no role is plain text",
            judged("Write to the `log` file: and `more`.")[0],
            "draft",
        ),
        ("skip: a colon before a space is not a role", judged("Note: `x` is quoted.")[0], "draft"),
        ("skip: the order is the documented one", REASONS, (
            "external", "no content", "rst directive", "multi-paragraph", "rst role")),
        (
            "text: a trimmed ellipsis is not a statement (a line starting `..`)",
            judged("...and so on.")[0:2],
            ("skip", "rst directive"),
        ),
        (
            "links: a field with a _back partner is a link",
            link_fields_of({"derived_from": ["A[version==1]"], "tags": ["x"]}, keys),
            ["derived_from"],
        ),
        (
            "links: a _back field and `links` are links, tags are not",
            link_fields_of({"derived_from_back": ["B"], "links": ["C"], "tags": ["t"]}, keys),
            ["derived_from_back", "links"],
        ),
        (
            "links: an empty list is no link",
            link_fields_of({"derived_from": [], "links": None}, keys),
            [],
        ),
        (
            "links: with no needs_schema, a field with no partner is a plain field",
            link_fields_of({"satisfied_by": ["X"]}, {"satisfied_by"}),
            [],
        ),
        (
            "links: a field the schema declares is a link without a partner in the file",
            link_fields_of({"satisfied_by": ["X"], "tags": ["web"]}, {"satisfied_by"}, frozenset({"satisfied_by"})),
            ["satisfied_by"],
        ),
        (
            "links: a field the schema does not declare is not a link, whatever its name",
            link_fields_of({"links": ["X"]}, {"links"}, frozenset()),
            [],
        ),
        (
            "links: declared names come from field_type links/backlinks",
            sorted(declared_link_fields({"versions": {"1": {"needs_schema": {"properties": {
                "satisfied_by": {"field_type": "links"}, "satisfied_by_back": {"field_type": "backlinks"},
                "tags": {"field_type": "core"}}}}}}, "1")),
            ["satisfied_by", "satisfied_by_back"],
        ),
        (
            "links: no needs_schema declares nothing (None, so the caller infers)",
            declared_link_fields({"versions": {"1": {"needs": {}}}}, "1"),
            None,
        ),
        (
            "not carried: fields exclude what the draft uses and the typed links",
            not_carried(
                [{"content": "x", "id": "N", "docname": "d", "lineno": 3, "status": "valid",
                  "tags": None, "safety": "QM", "derived_from": ["A"], "is_external": False}],
                keys,
            ),
            (["safety", "status"], ["derived_from"], 1),
        ),
        (
            "reconcile: the line adds up and breaks the remainder down by type",
            reconcile_line(10, 5, 3, {"document": 1, "arch": 1}),
            "needs 10 = drafted 5 + skipped 3 + not selected 2 (by type: arch 1, document 1)",
        ),
        (
            "reconcile: nothing not selected has no breakdown",
            reconcile_line(2, 1, 1, {}),
            "needs 2 = drafted 1 + skipped 1 + not selected 0",
        ),
        (
            "version: one version and no current_version is that version",
            select_version({"versions": {"1.0": {"needs": {}}}}, None),
            ("1.0", {}),
        ),
        (
            "version: --version-key picks among several",
            select_version({"versions": {"1": {"needs": {}}, "2": {"needs": {"A": {}}}}}, "2")[0],
            "2",
        ),
        (
            "version: current_version is the default",
            select_version({"current_version": "2", "versions": {"1": {"needs": {}}, "2": {"needs": {}}}}, None)[0],
            "2",
        ),
        ("ids: claim 8 from a file name", claim_number_from_name("REQ-0008.yml"), 8),
        ("ids: a leading dot is not part of the stem", claim_number_from_name(".REQ-0008"), 8),
        ("ids: an unrelated name has no number", claim_number_from_name("notes.yml"), None),
        ("ids: the README is not a claim", is_claim_candidate("README.md"), False),
        ("next: an empty corpus starts at 1", next_claim_number(set()), 1),
        ("next: one past the highest, not the first gap", next_claim_number({1, 3}), 4),
        (
            "version: the two axes are read from the binary's output",
            parse_version("req 0.1.0\n  rules_v            5\n  claim_schema       req.claim/2\n"),
            (5, "req.claim/2"),
        ),
        (
            "version: a build that does not name the axes cannot be drafted against",
            parse_version("req 0.1.0\n  built from         deadbee\n"),
            None,
        ),
        ("date: a real date is usable", valid_date("2026-10-21"), True),
        ("date: a day that does not exist is not", valid_date("2026-02-30"), False),
        ("date: a timestamp is not a date", valid_date("2026-10-21T00:00:00Z"), False),
        ("draft: a control character is escaped", yaml_quote("a\x00b"), '"a\\x00b"'),
        ("draft: U+2028 is escaped, not left as a YAML line break", yaml_quote("a\u2028b"), '"a\\u2028b"'),
        (
            "draft: a quoted source reads back as written",
            unquote_scalar(yaml_quote('o"r\\i\ngin\x07 #N')),
            'o"r\\i\ngin\x07 #N',
        ),
        ("draft: a single-quoted source reads back", unquote_scalar("'it''s #1'"), "it's #1"),
        ("draft: a plain source loses a trailing comment", unquote_scalar("abc#N  # why"), "abc#N"),
        (
            "draft: nothing derived and no source field in the file",
            [
                key
                for key in (
                    "modality", "subject", "attribute", "operator", "value", "unit",
                    "conditions", "exception", "temporal", "quantifier", "unstructured",
                    "canonical", "address", "pins",
                )
                if field(draft, f"{key}:") is not None
            ],
            [],
        ),
        (
            "draft: the authored fields are all there, empty where unknown",
            [field(draft, f"{key}:") for key in ("applicability", "relations")],
            ["applicability: []", "relations: []"],
        ),
        (
            "draft: the provenance is the origin and the need id",
            field(draft, "  source: "),
            'source: "https://example.test/needs.json#N_1"',
        ),
        (
            "draft: the statement's quotes and backslashes are escaped",
            field(draft, "statement: "),
            'statement: "The tool shall print \\"a: b\\" and a back\\\\slash."',
        ),
        (
            "draft: the header names the export and the place in it",
            "needs.json (docs/a:12)" in draft,
            True,
        ),
        (
            "report: a location without a name or line says `?`",
            location_of({}),
            "?:?",
        ),
        ("report: a location is docname:lineno", location_of({"docname": "d/x", "lineno": 7}), "d/x:7"),
        ("report: a newline in a name cannot end a line", one_line("a\nb\x00c"), "a b c"),
        (
            "export: a repeated key is refused, not silently dropped",
            _refuses(lambda: json.loads('{"a": 1, "a": 2}', object_pairs_hook=no_duplicate_keys)),
            True,
        ),
        (
            "export: no versions mapping is unverifiable",
            _refuses(lambda: select_version({"needs": {}}, None)),
            True,
        ),
        (
            "export: several versions and no current_version is unverifiable",
            _refuses(lambda: select_version({"versions": {"1": {"needs": {}}, "2": {"needs": {}}}}, None)),
            True,
        ),
        (
            "export: content that is not text is unverifiable",
            _refuses(lambda: classify("N", {"type": "req", "content": ["x"]}, types, ())),
            True,
        ),
        ("key: lowercases", area_key("Diag"), "diag"),
        ("key: a run of other characters is one hyphen", area_key("a__ .b"), "a-b"),
        ("key: the ends are stripped", area_key("_Sec.A 12_"), "sec-a-12"),
        ("key: digits stay", area_key("X2"), "x2"),
        ("key: nothing left is empty", area_key("___"), ""),
        ("key: a non-ASCII letter is not a key character", area_key("\u00dcn\u00ef"), "n"),
        ("key: only ASCII is lowercased (the Kelvin sign is no k)", area_key("\u212a"), ""),
        (
            "area: a docname is split on `/`, each part a key",
            need_area("N", {"docname": "requirements/Diag_Startup"}, None),
            ("area", "requirements/diag-startup", ["requirements", "Diag_Startup"]),
        ),
        (
            "area: the depth keeps the outermost parts",
            need_area("N", {"docname": "a/B/c"}, 2),
            ("area", "a/b", ["a", "B"]),
        ),
        ("area: no docname is no area", need_area("N", {}, None), ("none", "no docname")),
        ("area: a null docname is no area", need_area("N", {"docname": None}, None), ("none", "no docname")),
        ("area: a blank docname is no area", need_area("N", {"docname": "  "}, None), ("none", "no docname")),
        (
            "area: a part that leaves no key is a problem",
            _problem(lambda: need_area("N", {"docname": "a/__/b"}, None)),
            True,
        ),
        (
            "area: a part above the depth is never judged",
            need_area("N", {"docname": "a/__/b"}, 1)[1],
            "a",
        ),
        (
            "area: two spellings of one path are a problem, never merged",
            _problem(lambda: plan_areas(
                [("A", {"docname": "Diag_Startup"}), ("B", {"docname": "diag-startup"})], None)),
            True,
        ),
        (
            "area: two spellings of one prefix are a problem too",
            _problem(lambda: plan_areas(
                [("A", {"docname": "Req/a"}), ("B", {"docname": "req/b"})], None)),
            True,
        ),
        (
            "area: one spelling twice is one path, labelled as written",
            plan_areas([("A", {"docname": "R/Dia g"}), ("B", {"docname": "R/Dia g"})], None),
            ([("area", "r/dia-g"), ("area", "r/dia-g")], [("r/dia-g", "R/Dia g")]),
        ),
        (
            "area: the paths come in the order the export first shows them",
            plan_areas([("A", {"docname": "z"}), ("B", {"docname": "a"}), ("C", {})], None)[1],
            [("z", "z"), ("a", "a")],
        ),
        (
            "area: the label is the docname cut to the kept depth",
            plan_areas([("A", {"docname": "Req/Dia g/x"})], 2)[1],
            [("req/dia-g", "Req/Dia g")],
        ),
        (
            "registry: the declared paths are the first words of the lines that are not comments",
            sorted(declared_paths("# c\n\na One\r\nb/c\tTwo # x\n")),
            ["a", "b/c"],
        ),
        (
            "registry: a declared path is left alone, whatever its label",
            registry_suffix("a Mine\n", [("a", "Theirs"), ("b", "B")]),
            ("b B\n", [("b", "B")], 1),
        ),
        (
            "registry: a file with no final newline gets one before the appended lines",
            registry_suffix("a Mine", [("b", "B")])[0],
            "\nb B\n",
        ),
        (
            "registry: no file, or an empty one, is appended to as it is",
            [registry_suffix(None, [("b", "B")])[0], registry_suffix("", [("b", "B")])[0]],
            ["b B\n", "b B\n"],
        ),
        (
            "registry: everything declared adds nothing",
            registry_suffix("a A\n", [("a", "X")]),
            ("", [], 1),
        ),
        (
            "report: the areas lines count paths, drafts filed, the reasons and the registry",
            area_report_lines([("area", "a"), ("none", "no docname"), ("area", "a")], [("a", "A")], [("a", "A")], 0, 3),
            ["areas: 1 paths; filed 2 of 3 drafted", "no area: 1 (no docname 1)",
             "registry: 1 lines to add, 0 already declared", "  a A"],
        ),
        (
            "report: with every draft filed there is no `no area` line",
            area_report_lines([("area", "a")], [("a", "A")], [], 1, 1),
            ["areas: 1 paths; filed 1 of 1 drafted", "registry: 0 lines to add, 1 already declared"],
        ),
        (
            "draft: an area makes it req.claim/3 with `area:` after claim_id",
            [line for line in render_claim(
                export="needs.json", location="d:1", statement="x", source="o#N", date="2026-11-05",
                claim_id="REQ-0016", schema="req.claim/2", area="a/b",
            ).splitlines() if line.startswith(("schema:", "claim_id:", "area:", "statement:"))],
            ["schema: req.claim/3", "claim_id: REQ-0016", "area: a/b", 'statement: "x"'],
        ),
        (
            "draft: with no area the draft is the req.claim/2 it always was",
            [line for line in render_claim(
                export="needs.json", location="d:1", statement="x", source="o#N", date="2026-11-05",
                claim_id="REQ-0016", schema="req.claim/2",
            ).splitlines() if line.startswith(("schema:", "area:"))],
            ["schema: req.claim/2"],
        ),
    ]

    failures = 0
    for label, got, want in rows:
        ok = got == want
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'} {label:<62} {got!r}")
    print(f"needs_import --self-check: {len(rows)} case(s), {failures} failure(s)")
    return 1 if failures else 0


def _refuses(call):
    """Whether `call()` raises the refusal this tool turns into a clean exit."""
    try:
        call()
    except (Unverifiable, ValueError):
        return True
    return False


def _problem(call):
    """Whether `call()` raises a definite problem (exit 1)."""
    try:
        call()
    except Problem:
        return True
    return False


# --- arguments --------------------------------------------------------------


def value_arg(text):
    """A value that is not a flag: one beginning with `-` is a usage error, not a value."""
    if text.startswith("-"):
        raise argparse.ArgumentTypeError(f"{text!r} begins with `-`; a value was expected")
    return text


def named_arg(text):
    """A non-blank value that is not a flag."""
    value_arg(text)
    if not text.strip():
        raise argparse.ArgumentTypeError("the value is empty")
    return text


def depth_arg(text):
    """A positive integer in ASCII digits: a depth below 1 keeps nothing, so it is a usage error."""
    if not re.fullmatch(r"[0-9]+", text) or int(text) < 1:
        raise argparse.ArgumentTypeError(f"{text!r} is not a whole number of at least 1")
    return int(text)


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="tools/needs_import.py",
        description=(
            "Draft req claims from a Sphinx-Needs needs.json: one `req.claim/2` claim per plain "
            "requirement, with the need id in its provenance, and nothing derived."
        ),
        epilog=HELP_EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
        allow_abbrev=False,
    )
    parser.add_argument(
        "--from", dest="document", required=True, type=value_arg, metavar="<needs.json>",
        help="the export to read; a relative path is resolved against the corpus root",
    )
    parser.add_argument(
        "--origin", required=True, type=named_arg, metavar="<url-or-path>",
        help="where the export was retrieved from; recorded as `<origin>#<need id>` in each draft",
    )
    parser.add_argument(
        "--type", action="append", required=True, type=named_arg, metavar="<type>",
        help="a need type to read (required, repeatable, no default)",
    )
    parser.add_argument(
        "--status", action="append", default=[], type=named_arg, metavar="<status>",
        help="only needs with one of these statuses are considered (repeatable)",
    )
    parser.add_argument(
        "--date", required=True, type=value_arg, metavar="YYYY-MM-DD",
        help="the rationale date (the export's retrieval date); never invented",
    )
    parser.add_argument(
        "--areas", action="store_true",
        help="file each draft under an area made from its docname, and propose `.req/areas` lines",
    )
    parser.add_argument(
        "--area-depth", type=depth_arg, metavar="<N>",
        help="with --areas: keep only the outermost N parts of each docname (N >= 1)",
    )
    parser.add_argument(
        "--version-key", type=named_arg, metavar="<version>",
        help="the version of the export to read (default: its current_version)",
    )
    parser.add_argument(
        "--root", type=value_arg, metavar="<dir>",
        help="the corpus root (default: $REQ_ROOT, else the working directory)",
    )
    parser.add_argument("--write", action="store_true", help="write the drafts (the default is a dry run)")
    parser.add_argument(
        "--self-check", action="store_true",
        help="table-drive the pure half; read no export, no corpus, no `req`",
    )
    return parser.parse_args(argv)


def main(argv):
    # `--self-check` stands alone: it must work before a corpus exists and without `req`, so it
    # is answered before the required arguments are demanded of it. Any other argument beside
    # it is a usage error, not a quiet run of two things at once.
    if "--self-check" in argv:
        if len(argv) != 1:
            print(
                "usage: tools/needs_import.py --self-check takes no other arguments",
                file=sys.stderr,
            )
            return 2
        return self_check()

    args = parse_args(argv)
    args.type = list(dict.fromkeys(args.type))
    args.status = list(dict.fromkeys(args.status))
    if not valid_date(args.date):
        print(
            f"usage: --date must be a calendar date in YYYY-MM-DD form, not {args.date!r}",
            file=sys.stderr,
        )
        return 2
    if args.area_depth is not None and not args.areas:
        print("usage: --area-depth belongs to --areas", file=sys.stderr)
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
        # A crash is not "a definite problem" about the export: the tri-state answer to a
        # question this run could not settle is UNVERIFIABLE, never a FAIL the caller reads as
        # their input's fault.
        print(f"[UNVERIFIABLE] internal error: {error!r}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
