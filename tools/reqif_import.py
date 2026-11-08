#!/usr/bin/env python3
"""tools/reqif_import.py — draft req claims from a ReqIF file (issue #135, ADR 0028).

ReqIF (OMG, XML) is what DOORS, Polarion, Codebeamer, Jama and Eclipse ProR export. This script
reads one `.reqif` file and writes the same legal `req.claim/2` drafts `tools/adopt.py` and
`tools/needs_import.py` write: the statement, its provenance, empty `applicability` and
`relations`, nothing derived and no pin (ADR 0022). The drafts are proposals (ADR 0004); a status
in the source tool is not a ratification, so none is carried.

It is a sibling of `tools/needs_import.py`, not a client of it: what it needs is copied, and
adopters **copy and adjust** this file (the mapping is per project). It is deliberately not a req
subcommand. It was built on three small public files: it proves the *structure* of the reader and
fails loud on what it cannot honestly flatten. **The yield on a real specification is unmeasured.**

What it decides, and what it refuses to guess:

  --list     with only `--from`, it prints what is in the file (the header, each specification and
             its object count, each object type with its attributes) and writes nothing. A DOORS
             object type has no name to guess; `(unnamed)` is display only and never a selector.
  select     the caller names the objects to read: `--type <t>` (repeatable; an object type's
             LONG-NAME, else its IDENTIFIER) and/or `--where NAME=VALUE` (repeatable, all must
             hold). At least one is required. There is no default: the type names and the
             attribute names are the project's own. An object that is not selected is counted, by
             type, in the report — never silently ignored.
  statement  `--text-attr NAME` (required), and only when its value is one plain paragraph. The
             kind is read from the *value element*, not the declaration. Anything else is skipped
             and NAMED — `no text`, `multi-paragraph`, `rich markup (<element>)`, in that order.
             Nothing is flattened: `m<sub>2</sub>` as `m2` is a changed meaning.
  claim id   req allocates it (the next free REQ-NNNN). The object's IDENTIFIER is kept as
             provenance: `rationale.source` is `<origin>#<IDENTIFIER>`. `--id-attr NAME` names the
             project's own human id; it is shown in the report and the header comment only.
  date       `--date`, required. The header's CREATION-TIME is shown and never used.
  not carried  every attribute other than the text attribute, the hierarchy and the relations are
             counted in the report and not written (`relations` stays `[]`).

The output accounts for every SPEC-OBJECT in the file and ends with one line that must add up:

  objects N = drafted D + skipped S + not selected M (by type: ...)

Exit codes, the same three-state discipline as req (ADR 0028 section 9):

  0  drafted (dry run, or written with --write), or --list
  1  a definite problem — nothing selected or extracted, a --type the file does not have, a text
     attribute no selected type defines, or an object already drafted from this origin; nothing
     was written
  2  usage — a missing or malformed argument, a bad date
  3  UNVERIFIABLE — the file, the corpus, the subject registry or `req` cannot be read honestly
     (not XML, a DOCTYPE or entity declaration, not a ReqIF 20110401 REQ-IF, a .reqifz, a missing
     or repeated object IDENTIFIER, a reference that does not resolve, an attribute name matching
     two definitions, a corpus the loader rejects, a `req` judging another claim schema)

Nothing derived is ever written, and no pin is guessed. The script opens no archive, fetches
nothing and writes nowhere outside the corpus.
"""
import argparse
import datetime
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

# --- conventions (the whole mapping) ---------------------------------------

REQIF_NS = "http://www.omg.org/spec/ReqIF/20110401/reqif.xsd"
XHTML_NS = "http://www.w3.org/1999/xhtml"

# The reasons a selected object is not drafted, in the order they are checked.
REASONS = ("no text", "multi-paragraph", "rich markup")
UNNAMED = "(unnamed)"
NONE = "(none)"

ASCII_WS_RE = re.compile(r"[ \t\r\n]+")
DOCTYPE_RE = re.compile(rb"<!DOCTYPE", re.IGNORECASE)
ENTITY_RE = re.compile(rb"<!ENTITY", re.IGNORECASE)

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
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

CLAIMS_DIR = "claims"
REGISTRY_PATH = "aliases/subjects.yml"
REQ_BINARY = "req"

# The draft claim, byte for byte: a fixed template, not a serialized mapping (python3's standard
# library has no YAML writer, and a serializer's diff is unreadable). It is a `req.claim/2`
# claim: the authored fields only, nothing derived and no pin (ADR 0022). The header comment
# names the file, the object's IDENTIFIER and the human id; it carries no source field.
CLAIM_TEMPLATE = """\
# A draft claim, drafted from the ReqIF file {export} (object {identifier}; {human}). The object's
# IDENTIFIER is kept in rationale.source. Nothing derived is stored: `req canon` shows the parse
# this statement derives. A draft is a proposal (ADR 0004): edit the statement, add the
# applicability and relations it needs, and pin a load-bearing bound under `pins:` if it has one.
schema: {schema}
claim_id: {claim_id}
statement: {statement}
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
  --list     print the file's header, specifications and object types (with each attribute, its
             declared kind and how many objects carry a value); writes nothing. A DOORS type has
             no name: select it by its IDENTIFIER. `(unnamed)` is display only, not a selector.
  --type     an object type to read, by LONG-NAME else IDENTIFIER (repeatable; any one matches).
  --where    NAME=VALUE (split at the first `=`; repeatable; all must hold). NAME is an attribute's
             LONG-NAME. An enumeration compares its resolved value names (any one matching);
             other kinds compare the value's text. At least one of --type/--where is required:
             there is no default selection. Objects not selected are counted by type, never
             dropped silently. A --type the file lacks is exit 1 and lists the types it has.
  --text-attr  the attribute whose value is the statement (required, no default: RMF has no
             ReqIF.Text and a DOORS module may rename its columns).
  --id-attr  optional: the attribute holding your own human id; shown in the report and the
             header comment, never written as the claim_id or into rationale.

  A selected object becomes a draft only when its text attribute is one plain paragraph: a plain
  string (lines trimmed, joined with one space) or an XHTML value whose one wrapper is `div` or
  `p` holding text only (whitespace collapsed and trimmed). The value element, not the
  declaration, says which. Otherwise it is skipped and named, by the first reason that applies:
    no text          the attribute is absent, or blank
    multi-paragraph  a blank line in a plain string; more than one block element in XHTML
    rich markup (E)  any element inside the wrapper, or a wrapper that is not div/p; E is the
                     first one: br, span, sub, ul, table, object ...
  (A value that is not text at all, e.g. an integer, is skipped as `not text (KIND)`.)
  The reader never strips markup to make an object fit.

provenance: each draft's rationale.source is `<origin>#<IDENTIFIER>`; req allocates the claim id.
An object already drafted from the same origin is refused (exit 1, nothing written); the same
file under another --origin is a different provenance and is allowed. Whether an IDENTIFIER
survives a second export of the same module is NOT verified here.

not carried, and said so in the report: every attribute other than the text attribute (with the
count of drafted objects carrying it), the specification hierarchy, and the relations (the report
counts the file's SPEC-RELATIONs and names their types); `relations` stays `[]`.

--date is the rationale date (the retrieval date of the file) and is required. It is never
borrowed from the clock, a commit, or the header's CREATION-TIME.

Only a `.reqif` (XML) file is read: unzip a `.reqifz` first. A DOCTYPE or an entity declaration
is refused before anything is parsed.

The corpus root is --root, else $REQ_ROOT, else the working directory — never an ancestor: this
tool writes claims, so a run inside a subdirectory must not draft into a parent you never named.
A relative --from is resolved against that root.

dry run is the default; --write writes. The report ends with one line that adds up:
  objects N = drafted D + skipped S + not selected M (by type: ...)
N is the number of SPEC-OBJECT elements that were read, under a hierarchy node or not.

exit: 0 drafted (dry run, written, or --list) · 1 a definite problem, nothing written (nothing
selected or extracted, a --type the file does not have, a text attribute no selected type
defines, or an object already drafted from this origin) · 2 usage · 3 the file, the corpus, the
subject registry or `req` cannot be read.

This script is a starting point to COPY and adjust for your project — it is not a req
subcommand. The yield on a real specification is unmeasured: it was built on three small public
files.
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

    An identifier, a name or a path is *text* in a report line or a comment, not YAML; a newline
    (legal in an XML attribute through a character reference) would end the comment and inject a
    line into the draft.
    """
    cleaned = "".join(
        " " if ord(char) < 0x20 or 0x7F <= ord(char) <= 0x9F else char for char in str(text)
    )
    return " ".join(cleaned.split())


def collapse(text):
    """`text` with every run of ASCII whitespace as one space, trimmed.

    ASCII only, on purpose: a no-break space inside a statement is the author's character, not
    layout, and the standard `split()` would turn it into a space.
    """
    return ASCII_WS_RE.sub(" ", text).strip(" ")


def local_name(element):
    """An element's tag without its namespace."""
    return element.tag.rsplit("}", 1)[-1]


def is_block(element):
    """Whether `element` is an XHTML `div` or `p`: the only wrappers a statement may have."""
    return element.tag in (f"{{{XHTML_NS}}}div", f"{{{XHTML_NS}}}p")


def plain_statement(text):
    """`(kind, payload)` for a plain string value: `("draft", statement)` or `("skip", reason)`."""
    trimmed = text.replace("\r\n", "\n").replace("\r", "\n").strip()
    if not trimmed:
        return "skip", "no text"
    lines = trimmed.split("\n")
    if any(not line.strip() for line in lines):
        return "skip", "multi-paragraph"
    return "draft", collapse(" ".join(line.strip() for line in lines))


def xhtml_statement(holder):
    """`(kind, payload)` for an XHTML value: the `THE-VALUE` element (or None when absent).

    One wrapper element, `div` or `p` in the XHTML namespace, holding text only. The checks run in
    the order of `REASONS`; a skip is never a guess at what the author meant.
    """
    if holder is None:
        return "skip", "no text"
    blocks = list(holder)
    inner = [element for block in blocks for element in block.iter()]
    # Blank: no text anywhere and nothing but empty div/p wrappers. A bare object or `br` is
    # content, not blank.
    if not "".join(holder.itertext()).strip() and all(is_block(element) for element in inner):
        return "skip", "no text"
    if len(blocks) > 1:
        return "skip", "multi-paragraph"
    outside = (holder.text or "") + "".join(block.tail or "" for block in blocks)
    if not blocks or outside.strip():
        return "skip", "rich markup (text outside a div or p wrapper)"
    wrapper = blocks[0]
    if not is_block(wrapper):
        return "skip", f"rich markup ({local_name(wrapper)})"
    descendants = list(wrapper.iter())[1:]
    if descendants:
        return "skip", f"rich markup ({local_name(descendants[0])})"
    return "draft", collapse("".join(wrapper.itertext()))


class Definition:
    """An attribute definition: what a value's DEFINITION reference resolves to."""

    def __init__(self, ident, name, kind, enum_type):
        self.ident = ident
        self.name = name  # LONG-NAME, or None
        self.kind = kind  # the declared kind: STRING, XHTML, INTEGER, ENUMERATION, ...
        self.enum_type = enum_type  # an enumeration's datatype identifier, or None


class ObjectType:
    def __init__(self, ident, name, defs):
        self.ident = ident
        self.name = name  # LONG-NAME, or None: a DOORS type has none
        self.defs = defs

    def display(self):
        """The type as a count key: its name, else its identifier."""
        return self.name or self.ident


class Value:
    """One attribute value. `kind` is the **value element's** kind, which is what decides how it
    is read (a Polarion `ReqIF.Text` is declared XHTML but one object's value is a `STRING`)."""

    def __init__(self, definition, kind, text, holder, enums):
        self.definition = definition
        self.kind = kind
        self.text = text  # THE-VALUE, for every kind but XHTML and ENUMERATION
        self.holder = holder  # the THE-VALUE element, for XHTML
        self.enums = enums  # resolved ENUM-VALUE LONG-NAMEs, for ENUMERATION


class SpecObject:
    def __init__(self, ident, object_type, values):
        self.ident = ident
        self.type = object_type
        self.values = values


class Specification:
    def __init__(self, name, nodes):
        self.name = name  # LONG-NAME, or None
        self.nodes = nodes  # [(object ref, depth)] over the whole hierarchy


class Relation:
    def __init__(self, type_name, source, target):
        self.type_name = type_name
        self.source = source
        self.target = target


class Reqif:
    def __init__(self, header, types, objects, specs, relations, enum_types, enum_names):
        self.header = header
        self.types = types
        self.objects = objects
        self.specs = specs
        self.relations = relations
        self.enum_types = enum_types  # datatype identifier -> [value names]
        self.enum_names = enum_names  # ENUM-VALUE identifier -> name


def values_named(obj, name):
    """The values of `obj` whose definition is called `name`."""
    return [value for value in obj.values if value.definition.name == name]


def has_value(value):
    """Whether a value says anything: not absent, not blank."""
    if value.kind == "ENUMERATION":
        return bool(value.enums)
    if value.kind == "XHTML":
        holder = value.holder
        if holder is None:
            return False
        return bool("".join(holder.itertext()).strip()) or any(
            not is_block(element) for block in holder for element in block.iter()
        )
    return bool(value.text.strip())


def value_statement(value):
    """`(kind, payload)` for the text attribute's value (None when the object has none)."""
    if value is None:
        return "skip", "no text"
    if value.kind == "STRING":
        return plain_statement(value.text)
    if value.kind == "XHTML":
        return xhtml_statement(value.holder)
    return "skip", f"not text ({value.kind})"


def comparable(value):
    """The texts a `--where` value is compared with: resolved names for an enumeration, the
    flattened paragraph for XHTML (none when it is not one), the text otherwise."""
    if value.kind == "ENUMERATION":
        return list(value.enums)
    if value.kind == "XHTML":
        kind, payload = xhtml_statement(value.holder)
        return [payload] if kind == "draft" else []
    return [value.text]


def human_text(value):
    """A value as the report shows a human id: the decimal text of an integer, the names of an
    enumeration, the flattened text of XHTML (all its text when it is not one paragraph)."""
    if value.kind == "ENUMERATION":
        return ",".join(value.enums)
    if value.kind == "XHTML":
        if value.holder is None:
            return ""
        return collapse("".join(value.holder.itertext()))
    return collapse(value.text)


def type_matches(object_type, wanted):
    """A `--type` names an object type by LONG-NAME, else IDENTIFIER. `(unnamed)` is no name."""
    return (object_type.name is not None and object_type.name in wanted) or object_type.ident in wanted


def selected(obj, types, where):
    """Whether `obj` is selected: `--type` (any one) and every `--where` clause must hold."""
    if types and not type_matches(obj.type, types):
        return False
    for name, wanted in where:
        if not any(wanted in comparable(value) for value in values_named(obj, name)):
            return False
    return True


def classify(obj, types, where, text_attr):
    """`(kind, payload)` for one object.

    kind `not selected`: payload is the object's type (name, else identifier).
    kind `skip`:         payload is the first reason in `REASONS` that applies.
    kind `draft`:        payload is the statement.
    Two values called `text_attr` on one object is not a question this tool can settle.
    """
    if not selected(obj, types, where):
        return "not selected", obj.type.display()
    found = values_named(obj, text_attr)
    if len(found) > 1:
        raise Unverifiable(
            f"object {one_line(obj.ident)} has {len(found)} values for the attribute "
            f"`{text_attr}`: which one is the statement cannot be judged"
        )
    return value_statement(found[0] if found else None)


def breakdown(counts):
    """`type n, type n` in type order."""
    return ", ".join(f"{name} {counts[name]}" for name in sorted(counts))


def reconcile_line(total, drafted, skipped, not_selected):
    """The report's last line: the file's objects and where each went."""
    selected_out = sum(not_selected.values())
    line = f"objects {total} = drafted {drafted} + skipped {skipped} + not selected {selected_out}"
    if not_selected:
        line += f" (by type: {breakdown(not_selected)})"
    return line


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

    Quotes and backslashes alone are not enough: a control character or a C1 control would produce
    a file the loader cannot parse while the tool still reported success. A backslash-x escape is
    YAML's own form for those, and the loader parses it back to the same character.
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
        elif char in "  ":
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


def render_claim(export, identifier, human, statement, source, date, claim_id, schema):
    """One draft claim file's exact bytes."""
    return CLAIM_TEMPLATE.format(
        export=one_line(export),
        identifier=one_line(identifier),
        human=one_line(human),
        schema=schema,
        claim_id=claim_id,
        statement=yaml_quote(statement),
        quoted_source=yaml_quote(source),
        date=date,
    )


# --- reading the ReqIF file --------------------------------------------------


def q(name):
    """A ReqIF element's namespace-qualified name: matched by namespace, never by prefix."""
    return f"{{{REQIF_NS}}}{name}"


def text_of(element):
    """An element's text, trimmed; empty when there is no element."""
    return (element.text or "").strip() if element is not None else ""


def check_bytes(data, label):
    """Refuse what must never reach the parser: a DOCTYPE or an entity declaration (a ReqIF file
    needs neither, and a declared entity is how an XML file becomes a bomb or a file read), and
    anything the byte scan could not see (a UTF-16 or UTF-32 file holds NULs)."""
    if b"\x00" in data:
        raise Unverifiable(
            f"{label} holds NUL bytes: a UTF-16 or UTF-32 file cannot be checked for a DOCTYPE "
            "before it is parsed, so it is not read — save it as UTF-8"
        )
    if ENTITY_RE.search(data):
        raise Unverifiable(
            f"{label} declares an entity (<!ENTITY): a ReqIF file needs none, and a declared entity "
            "is how an XML file becomes a bomb or reads another file — nothing was parsed"
        )
    if DOCTYPE_RE.search(data):
        raise Unverifiable(
            f"{label} has a DOCTYPE: a ReqIF file needs none, and a DTD can declare entities — "
            "nothing was parsed"
        )


def parse_root(data, label):
    """The root element of `data` (bytes), or Unverifiable naming what is wrong."""
    check_bytes(data, label)
    parser = ET.XMLParser()
    try:
        parser.feed(data)
        root = parser.close()
    except ET.ParseError as error:
        raise Unverifiable(f"{label} is not well-formed XML: {error}") from error
    if root.tag != q("REQ-IF"):
        found = root.tag
        if local_name(root) == "REQ-IF":
            namespace = (
                f"in the namespace {found.rsplit('}', 1)[0].lstrip('{')!r}"
                if "}" in found
                else "in no namespace"
            )
            raise Unverifiable(
                f"{label}: the root REQ-IF is {namespace}, not in the ReqIF one ({REQIF_NS}, 20110401)"
            )
        raise Unverifiable(
            f"{label}: the root element is {found!r}, not REQ-IF in the ReqIF 20110401 namespace"
        )
    return root


def read_definitions(type_element):
    """The attribute definitions declared inside one `SPEC-ATTRIBUTES` of a type."""
    defs = []
    attributes = type_element.find(q("SPEC-ATTRIBUTES"))
    if attributes is None:
        return defs
    for element in attributes:
        name = local_name(element)
        if not name.startswith("ATTRIBUTE-DEFINITION-"):
            continue
        type_ref = element.find(q("TYPE"))
        enum_ref = type_ref.find(q("DATATYPE-DEFINITION-ENUMERATION-REF")) if type_ref is not None else None
        defs.append(
            Definition(
                ident=element.get("IDENTIFIER", ""),
                name=element.get("LONG-NAME") or None,
                kind=name[len("ATTRIBUTE-DEFINITION-"):],
                enum_type=text_of(enum_ref) or None,
            )
        )
    return defs


def read_hierarchy(children, depth, out):
    """Every `(object ref, depth)` under a `CHILDREN` element, depth 0 being a specification's own."""
    if children is None:
        return
    for hierarchy in children.findall(q("SPEC-HIERARCHY")):
        reference = hierarchy.find(q("OBJECT") + "/" + q("SPEC-OBJECT-REF"))
        if reference is not None:
            out.append((text_of(reference), depth))
        read_hierarchy(hierarchy.find(q("CHILDREN")), depth + 1, out)


def read_value(obj_ident, element, definitions, enum_names):
    """One attribute value, with its definition and any enumeration references resolved."""
    kind = local_name(element)[len("ATTRIBUTE-VALUE-"):]
    reference_holder = element.find(q("DEFINITION"))
    reference = next(iter(reference_holder), None) if reference_holder is not None else None
    wanted = text_of(reference)
    if not wanted:
        raise Unverifiable(
            f"object {one_line(obj_ident)}: an attribute value has no DEFINITION reference"
        )
    found = definitions.get(wanted)
    if not found:
        raise Unverifiable(
            f"object {one_line(obj_ident)}: an attribute value refers to the definition "
            f"{one_line(wanted)}, which the file does not declare"
        )
    if len(found) > 1:
        raise Unverifiable(
            f"object {one_line(obj_ident)}: the definition {one_line(wanted)} is declared "
            f"{len(found)} times, so which attribute the value belongs to cannot be judged"
        )
    enums = []
    if kind == "ENUMERATION":
        values = element.find(q("VALUES"))
        for ref in (values if values is not None else []):
            ref_id = text_of(ref)
            if ref_id not in enum_names:
                raise Unverifiable(
                    f"object {one_line(obj_ident)}: an enumeration value refers to "
                    f"{one_line(ref_id)}, which the file does not declare (ENUM-VALUE-REF)"
                )
            enums.append(enum_names[ref_id])
    return Value(
        definition=found[0],
        kind=kind,
        text=element.get("THE-VALUE", ""),
        holder=element.find(q("THE-VALUE")) if kind == "XHTML" else None,
        enums=enums,
    )


def load_reqif(root):
    """The model of a parsed ReqIF document. Strict: every reference an object holds must resolve."""
    header_element = root.find(q("THE-HEADER") + "/" + q("REQ-IF-HEADER"))

    def head(name):
        return text_of(header_element.find(q(name))) if header_element is not None else ""

    header = {
        "tool": head("REQ-IF-TOOL-ID") or NONE,
        "created": head("CREATION-TIME") or NONE,
        "version": head("REQ-IF-VERSION") or NONE,
        "title": head("TITLE") or NONE,
    }
    content = root.find(q("CORE-CONTENT") + "/" + q("REQ-IF-CONTENT"))
    if content is None:
        content = ET.Element(q("REQ-IF-CONTENT"))

    enum_types, enum_names = {}, {}
    datatypes = content.find(q("DATATYPES"))
    for datatype in (datatypes if datatypes is not None else []):
        if local_name(datatype) != "DATATYPE-DEFINITION-ENUMERATION":
            continue
        names = []
        specified = datatype.find(q("SPECIFIED-VALUES"))
        for value in (specified if specified is not None else []):
            if local_name(value) != "ENUM-VALUE":
                continue
            name = value.get("LONG-NAME") or value.get("IDENTIFIER", "")
            names.append(name)
            enum_names.setdefault(value.get("IDENTIFIER", ""), name)
        enum_types[datatype.get("IDENTIFIER", "")] = names

    types, definitions, relation_types = [], {}, {}
    spec_types = content.find(q("SPEC-TYPES"))
    for element in (spec_types if spec_types is not None else []):
        kind = local_name(element)
        defs = read_definitions(element)
        for definition in defs:
            definitions.setdefault(definition.ident, []).append(definition)
        if kind == "SPEC-OBJECT-TYPE":
            types.append(ObjectType(element.get("IDENTIFIER", ""), element.get("LONG-NAME") or None, defs))
        elif kind == "SPEC-RELATION-TYPE":
            relation_types[element.get("IDENTIFIER", "")] = element.get("LONG-NAME") or element.get("IDENTIFIER", "")
    types_by_id = {object_type.ident: object_type for object_type in types}

    objects, seen = [], set()
    spec_objects = content.find(q("SPEC-OBJECTS"))
    for position, element in enumerate(spec_objects if spec_objects is not None else [], start=1):
        if local_name(element) != "SPEC-OBJECT":
            continue
        ident = element.get("IDENTIFIER", "")
        if not ident.strip():
            raise Unverifiable(
                f"SPEC-OBJECT number {position} (in file order) has no IDENTIFIER: it is the join "
                "key, so the object cannot be told apart"
            )
        if ident in seen:
            raise Unverifiable(
                f"the object IDENTIFIER {one_line(ident)} occurs twice among the SPEC-OBJECTs: it "
                "is the join key, so the objects cannot be told apart"
            )
        seen.add(ident)
        type_ref = text_of(element.find(q("TYPE") + "/" + q("SPEC-OBJECT-TYPE-REF")))
        if type_ref not in types_by_id:
            raise Unverifiable(
                f"object {one_line(ident)}: its type {one_line(type_ref) or '(none)'} is not a "
                "SPEC-OBJECT-TYPE the file declares"
            )
        values = []
        holder = element.find(q("VALUES"))
        for value in (holder if holder is not None else []):
            if local_name(value).startswith("ATTRIBUTE-VALUE-"):
                values.append(read_value(ident, value, definitions, enum_names))
        objects.append(SpecObject(ident, types_by_id[type_ref], values))

    specs = []
    specifications = content.find(q("SPECIFICATIONS"))
    for element in (specifications if specifications is not None else []):
        if local_name(element) != "SPECIFICATION":
            continue
        nodes = []
        read_hierarchy(element.find(q("CHILDREN")), 0, nodes)
        specs.append(Specification(element.get("LONG-NAME") or None, nodes))

    relations = []
    relation_elements = content.find(q("SPEC-RELATIONS"))
    for element in (relation_elements if relation_elements is not None else []):
        if local_name(element) != "SPEC-RELATION":
            continue
        type_ref = text_of(element.find(q("TYPE") + "/" + q("SPEC-RELATION-TYPE-REF")))
        relations.append(
            Relation(
                relation_types.get(type_ref) or type_ref or UNNAMED,
                text_of(element.find(q("SOURCE") + "/" + q("SPEC-OBJECT-REF"))),
                text_of(element.find(q("TARGET") + "/" + q("SPEC-OBJECT-REF"))),
            )
        )
    return Reqif(header, types, objects, specs, relations, enum_types, enum_names)


def reachable(spec, known):
    """The distinct known objects a specification's hierarchy reaches."""
    return {ref for ref, _depth in spec.nodes if ref in known}


def object_counts(model):
    """Objects per object type identifier."""
    counts = {}
    for obj in model.objects:
        counts[obj.type.ident] = counts.get(obj.type.ident, 0) + 1
    return counts


def not_selected_by_type(model):
    """Every object counted under its type (name, else identifier): what nothing selected leaves."""
    counts = {}
    for obj in model.objects:
        counts[obj.type.display()] = counts.get(obj.type.display(), 0) + 1
    return counts


def check_unambiguous(model, name, flag, only=None):
    """Unverifiable when `name` matches two definitions on one object type.

    `only` limits the check to those types (the selected objects' types); None checks every type
    that has an object.
    """
    having = {obj.type.ident for obj in model.objects}
    for object_type in model.types:
        if object_type.ident not in having:
            continue
        if only is not None and object_type.ident not in only:
            continue
        if sum(1 for definition in object_type.defs if definition.name == name) > 1:
            raise Unverifiable(
                f"{flag} {name!r} matches two attribute definitions on the object type "
                f"{one_line(object_type.display())}, so which one is meant cannot be judged"
            )


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
            f"no corpus at {root}: {CLAIMS_DIR}/ is not there — run `req init` here "
            "first, or pass --root <dir>"
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
            "draft's claim schema comes from the binary that will judge it"
        )
    return binary


def loader_check(binary, root, targets, texts):
    """Run the corpus plus the drafts through the real loader (`req canon`) before anything is
    written, in a scratch copy of the corpus, and raise Unverifiable if it does not load.

    The loader is the only judge of a claim file: a template that drifted from the format would
    be written and reported as drafted all the same. The same run is what turns an older-schema
    corpus away (its message names tools/migrate_claims.py).
    """
    with tempfile.TemporaryDirectory(prefix="req-reqif-") as scratch:
        copy = pathlib.Path(scratch)
        for name in (CLAIMS_DIR, "aliases"):
            if (root / name).is_dir():
                shutil.copytree(root / name, copy / name)
        (copy / CLAIMS_DIR).mkdir(exist_ok=True)
        for (_claim_id, target), text in zip(targets, texts):
            (copy / CLAIMS_DIR / target.name).write_text(text, encoding="utf-8")
        try:
            completed = subprocess.run(
                [binary, "--root", str(copy), "canon"],
                capture_output=True, text=True, check=False,
            )
        except OSError as error:
            raise Unverifiable(f"cannot run `{binary} canon`: {error}") from error
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or "no output"
        raise Unverifiable(
            f"the corpus plus these drafts does not load (`req canon` exited "
            f"{completed.returncode}), so nothing was written. The loader says why:\n{detail}"
        )


def read_axes():
    """`(rules_v, claim_schema)` from the `req` on PATH — the schema a draft declares, and the
    rules version its parse is derived under (reported)."""
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
    if axes[1] != DRAFT_SCHEMA:
        raise Unverifiable(
            f"`{binary}` reports claim_schema {axes[1]}, but this tool drafts {DRAFT_SCHEMA} "
            "claims: a draft under another schema would not load — use a matching req build or "
            "the matching copy of this tool"
        )
    return axes


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


def load_file(args, root):
    """`(label, model)` for the file `--from` names, or Unverifiable.

    The file is resolved against the **corpus root**, not the working directory, like the siblings:
    one basis for reading and for the name the report gives.
    """
    document = pathlib.Path(args.document).expanduser()
    if document.suffix.lower() == ".reqifz":
        raise Unverifiable(
            f"{args.document} is a .reqifz, a zip archive: unzip it first and pass the .reqif "
            "inside (this script opens no archive)"
        )
    if not document.is_absolute():
        document = root / document
    document = pathlib.Path(os.path.normpath(str(document)))
    label = relative(root, document)
    try:
        data = document.read_bytes()
    except OSError as error:
        raise Unverifiable(f"cannot read the file {args.document}: {error}") from error
    return label, load_reqif(parse_root(data, args.document))


def header_line(model):
    header = model.header
    return (
        f"reqif: tool {one_line(header['tool'])}; created {one_line(header['created'])}; "
        f"ReqIF {one_line(header['version'])}; title {one_line(header['title'])}"
    )


def run_list(args):
    """Print what the file holds. Writes nothing and needs no corpus. Returns the exit code."""
    root = resolve_root(args.root)
    label, model = load_file(args, root)
    known = {obj.ident for obj in model.objects}
    counts = object_counts(model)
    print(f"file: {label}")
    print(header_line(model))
    for spec in model.specs:
        print(f"specification {one_line(spec.name or UNNAMED)}  objects {len(reachable(spec, known))}")
    for object_type in model.types:
        print(
            f"type {one_line(object_type.name or UNNAMED)}  {one_line(object_type.ident)}  "
            f"objects {counts.get(object_type.ident, 0)}"
        )
        mine = [obj for obj in model.objects if obj.type is object_type]
        for definition in object_type.defs:
            carrying = sum(
                1
                for obj in mine
                if any(v.definition is definition and has_value(v) for v in obj.values)
            )
            print(
                f"  {one_line(definition.name or UNNAMED)}  {definition.kind}  "
                f"{carrying} with a value"
            )
    print(f"objects {len(model.objects)} in the file; --list writes nothing")
    return 0


def values_seen(model, name):
    """The values objects carry for the attribute called `name`, and an enumeration's declared ones."""
    carried, declared = set(), set()
    for obj in model.objects:
        for value in values_named(obj, name):
            carried.update(comparable(value))
    for object_type in model.types:
        for definition in object_type.defs:
            if definition.name == name and definition.enum_type in model.enum_types:
                declared.update(model.enum_types[definition.enum_type])
    return sorted(carried), sorted(declared)


def shown(names, limit=20):
    if not names:
        return "none"
    text = ", ".join(one_line(name) for name in names[:limit])
    return text + (f", ... ({len(names) - limit} more)" if len(names) > limit else "")


def type_summary(model):
    counts = object_counts(model)
    return (
        ", ".join(
            f"{one_line(t.display())} ({counts.get(t.ident, 0)})"
            for t in sorted(model.types, key=lambda t: t.display())
        )
        or "none"
    )


def not_carried(drafted, text_attr):
    """`{attribute name: drafted objects carrying a value}` for every attribute but the text one."""
    counts = {}
    for obj in drafted:
        names = {
            (value.definition.name or UNNAMED)
            for value in obj.values
            if value.definition.name != text_attr and has_value(value)
        }
        for name in names:
            counts[name] = counts.get(name, 0) + 1
    return counts


def hierarchy_of(model, drafted):
    """`(drafted objects under a parent, names of the specifications that reach a drafted object)`."""
    ids = {obj.ident for obj in drafted}
    under, names = set(), []
    for spec in model.specs:
        reached = {ref for ref, _depth in spec.nodes if ref in ids}
        if reached:
            names.append(spec.name or UNNAMED)
        under.update(ref for ref, depth in spec.nodes if depth > 0 and ref in ids)
    return len(under), names


def relations_of(model, drafted):
    """`(relations in the file, drafted objects that are a source or target, type names)`."""
    ids = {obj.ident for obj in drafted}
    touched = set()
    for relation in model.relations:
        touched.update(ref for ref in (relation.source, relation.target) if ref in ids)
    return len(model.relations), len(touched), sorted({r.type_name for r in model.relations})


def sort_objects(model, types, where, text_attr):
    """Classify every object, in file order: `(drafts, skips, not_selected)`.

    `drafts` is `[(object, statement)]`, `skips` is `[(object, reason)]`, and `not_selected` maps a
    type (name, else identifier) to its count.
    """
    drafts, skips, not_selected = [], [], {}
    for obj in model.objects:
        kind, payload = classify(obj, types, where, text_attr)
        if kind == "draft":
            drafts.append((obj, payload))
        elif kind == "skip":
            skips.append((obj, payload))
        else:
            not_selected[payload] = not_selected.get(payload, 0) + 1
    return drafts, skips, not_selected


def skip_lines(skips):
    return [f"skip {one_line(obj.ident)}  {reason}" for obj, reason in skips]


def human_id(obj, id_attr):
    """The project's own id for `obj`, as text, or `-`."""
    if id_attr is None:
        return "-"
    found = values_named(obj, id_attr)
    if len(found) > 1:
        raise Unverifiable(
            f"object {one_line(obj.ident)} has {len(found)} values for the attribute `{id_attr}`: "
            "which one is the human id cannot be judged"
        )
    return (human_text(found[0]) if found else "") or "-"


def run(args):
    """Draft, report, and (with --write) write. Returns the exit code."""
    corpus = read_corpus(resolve_root(args.root))
    label, model = load_file(args, corpus.root)

    # A --type the file does not have is a definite problem, and the types it does have are the remedy.
    missing = [
        name for name in args.type if not any(type_matches(t, {name}) for t in model.types)
    ]
    if missing:
        raise Problem(
            f"--type names a type {label} does not have: {', '.join(missing)}\n"
            f"  the file has: {type_summary(model)}\n"
            f"  ({UNNAMED} is display only, not a selector: select a type with no name by its "
            "IDENTIFIER, see --list)\n"
            "  there is no default selection: name the types your project uses for requirements"
        )
    for name, _value in args.where:
        check_unambiguous(model, name, "--where")

    chosen = [obj for obj in model.objects if selected(obj, set(args.type), args.where)]
    total = len(model.objects)
    if not chosen:
        lines = []
        for name, wanted in args.where:
            carried, declared = values_seen(model, name)
            lines.append(
                f"  --where {one_line(name)}={one_line(wanted)} selects no object; values of "
                f"{one_line(name)}: declared {shown(declared)}; carried by objects: {shown(carried)}"
            )
        raise Problem(
            f"nothing selected from {label}\n"
            + ("\n".join(lines) + "\n" if lines else "")
            + f"  the file has: {type_summary(model)}\n"
            + f"  {reconcile_line(total, 0, 0, not_selected_by_type(model))}"
        )

    chosen_types = {obj.type.ident for obj in chosen}

    def defined_on(name):
        return any(
            definition.name == name
            for object_type in model.types
            if object_type.ident in chosen_types
            for definition in object_type.defs
        )

    if not defined_on(args.text_attr):
        raise Problem(
            f"--text-attr {args.text_attr!r} is defined on none of the selected objects' types "
            f"({', '.join(sorted({one_line(o.type.display()) for o in chosen}))}) in {label}\n"
            "  name the attribute that holds the statement: see --list"
        )
    if args.id_attr is not None and not defined_on(args.id_attr):
        raise Problem(
            f"--id-attr {args.id_attr!r} is defined on none of the selected objects' types "
            f"({', '.join(sorted({one_line(o.type.display()) for o in chosen}))}) in {label}\n"
            "  name the attribute that holds your own id, or leave --id-attr out: see --list"
        )
    check_unambiguous(model, args.text_attr, "--text-attr", chosen_types)
    if args.id_attr is not None:
        check_unambiguous(model, args.id_attr, "--id-attr", chosen_types)

    drafts, skips, not_selected = sort_objects(model, set(args.type), args.where, args.text_attr)
    recon = reconcile_line(total, len(drafts), len(skips), not_selected)

    if not drafts:
        # The skipped objects are named even here: the finding is that nothing was drafted, and
        # *why* each selected object was skipped is its remedy.
        lines = "".join(f"\n  {line}" for line in skip_lines(skips))
        raise Problem(f"nothing drafted from {label}{lines}\n  {recon}")

    # An object already drafted from this origin is refused before anything else: the drafts are in
    # the corpus, and drafting them again would duplicate claims. Only a selected, draftable
    # object counts — a skipped one was never a draft.
    clashes = []
    for obj, _statement in drafts:
        wanted = f"{args.origin}#{obj.ident}"
        owners = sorted(
            claim_id for claim_id, stored in corpus.sources.items() if wanted in stored
        )
        if owners:
            clashes.append(f"  {one_line(obj.ident)}  ({', '.join(owners)})")
    if clashes:
        raise Problem(
            f"refusing to draft: {len(clashes)} object(s) in {label} are already drafted from "
            f"this origin ({args.origin}):\n" + "\n".join(clashes) + "\n"
            "  edit those claims, or delete them deliberately, before drafting from this origin "
            "again — or pass the --origin the file was retrieved from this time"
        )

    humans = [human_id(obj, args.id_attr) for obj, _statement in drafts]
    rules_v, schema = read_axes()

    number = next_claim_number(corpus.used)
    targets, texts = [], []
    for offset, ((obj, statement), human) in enumerate(zip(drafts, humans)):
        claim_id = f"REQ-{number + offset:04d}"
        targets.append((claim_id, corpus.root / CLAIMS_DIR / f"{claim_id}.yml"))
        texts.append(
            render_claim(
                export=label,
                identifier=obj.ident,
                human=f"{args.id_attr} {human}" if args.id_attr is not None else "no --id-attr",
                statement=statement,
                source=f"{args.origin}#{obj.ident}",
                date=args.date,
                claim_id=claim_id,
                schema=schema,
            )
        )

    loader_check(req_binary(), corpus.root, targets, texts)

    written = []
    if args.write:
        for _claim_id, target in targets:
            if target.exists() or target.is_symlink():
                raise Problem(
                    f"refusing to write {target}: it already exists — a claim file is never "
                    "overwritten"
                )
        written = write_claims([target for _claim_id, target in targets], texts)

    report(args, corpus, label, model, drafts, humans, skips, targets, written, rules_v, recon)
    return 0


def report(args, corpus, label, model, drafts, humans, skips, targets, written, rules_v, recon):
    """Print what was drafted, what was skipped and why, what was not carried, and what was
    written — ending with the line that adds up."""
    print(f"file: {label}")
    print(header_line(model))
    print(f"origin: {args.origin}")
    print(f"date: {args.date} (--date)")
    print(f"root: {corpus.root}")
    print(f"req: rules_v {rules_v}, registry {corpus.registry_v}")
    selection = []
    if args.type:
        selection.append(f"types: {', '.join(args.type)}")
    if args.where:
        selection.append("where: " + "; ".join(f"{name}={value}" for name, value in args.where))
    selection.append(f"text attribute: {args.text_attr}")
    if args.id_attr is not None:
        selection.append(f"id attribute: {args.id_attr}")
    print("selection: " + "; ".join(selection))
    print()
    for ((obj, statement), human), (claim_id, _target) in zip(zip(drafts, humans), targets):
        print(f"draft {claim_id}  {one_line(obj.ident)}  {one_line(human)}")
        print(f"    statement: {statement}")
    for line in skip_lines(skips):
        print(line)
    print()
    objects = [obj for obj, _statement in drafts]
    counts = not_carried(objects, args.text_attr)
    shown_counts = ", ".join(f"{one_line(name)} {counts[name]}" for name in sorted(counts)) or "none"
    print(f"not carried: {shown_counts} (drafted objects carrying each attribute; only the text attribute is carried)")
    under, names = hierarchy_of(model, objects)
    where_names = ", ".join(one_line(name) for name in names) if names else "no specification"
    print(
        f"hierarchy: {under} of {len(objects)} drafted objects sit under a parent; "
        f"specification: {where_names} — the structure is not carried"
    )
    in_file, touched, kinds = relations_of(model, objects)
    print(
        f"relations: {in_file} in the file; {touched} of {len(objects)} drafted objects are the "
        f"source or target of one; types: {', '.join(one_line(k) for k in kinds) or 'none'} — "
        "not carried, `relations` stays []"
    )
    print(
        "note: the object IDENTIFIERs are kept as provenance in rationale.source and are not the "
        "claim_id (req allocates it)"
    )
    print(
        "next: `req canon` shows the parse each draft derives (nothing derived is stored); a "
        "draft is a proposal — edit it, add its applicability and relations, pin a load-bearing "
        "bound under `pins:`"
    )
    if written:
        names = ", ".join(relative(corpus.root, target) for target in written)
        print(f"wrote: {len(written)} claim(s) — {names}")
    else:
        print("dry run: nothing written — pass --write to write them")
    print(recon)


def field(text, prefix):
    """The first line of `text` starting with `prefix`, trimmed — a field, not a substring."""
    for line in text.splitlines():
        if line.startswith(prefix):
            return line.strip()
    return None


# --- --self-check -----------------------------------------------------------


def _fragment(xml):
    """A `THE-VALUE` element holding `xml`, with the XHTML namespace on the `x:` prefix."""
    return ET.fromstring(
        f'<THE-VALUE xmlns:x="{XHTML_NS}" xmlns:other="urn:other">{xml}</THE-VALUE>'
    )


def _refuses(call):
    """Whether `call()` raises the refusal this tool turns into a clean exit."""
    try:
        call()
    except (Unverifiable, ET.ParseError):
        return True
    return False


def _value(kind, definition_name="Text", text="", holder=None, enums=()):
    return Value(Definition("D", definition_name, "XHTML", None), kind, text, holder, list(enums))


def _object(ident, type_name, values):
    return SpecObject(ident, ObjectType("T-" + type_name, type_name, []), values)


def self_check():
    """Table-drive the pure half. No filesystem, no corpus, no `req`, no ReqIF file.

    Every row is a fact the conventions promise: how a statement is read from each kind of value,
    which reason an object is skipped for and in what order, what is selected, what the report
    counts, and what a draft carries. The filesystem half is exercised by `tests/reqif_import.rs`.
    """

    def xhtml(xml):
        return xhtml_statement(_fragment(xml))

    def judged(value, **more):
        obj = _object("O", "Requirement", [] if value is None else [value])
        return classify(obj, {"Requirement"}, more.get("where", []), "Text")

    draft = render_claim(
        export="module.reqif",
        identifier="_1_abc-def",
        human="ReqIF.ForeignID 12",
        statement='The tool shall print "a: b" and a back\\slash.',
        source="https://example.test/module.reqif#_1_abc-def",
        date="2026-11-08",
        claim_id="REQ-0016",
        schema="req.claim/2",
    )
    status = _value("ENUMERATION", "Status", enums=["Draft", "Approved"])
    number = _value("INTEGER", "Num", text="5")
    rows = [
        ("plain: lines are trimmed and joined with one space", plain_statement("  a,\n   b.  \n"), ("draft", "a, b.")),
        ("plain: CRLF line breaks are line breaks", plain_statement("a\r\nb"), ("draft", "a b")),
        ("plain: interior whitespace runs collapse", plain_statement("a    b"), ("draft", "a b")),
        ("plain: a no-break space is the author's, not layout", plain_statement("10 kg"), ("draft", "10 kg")),
        ("plain: blank is no text", plain_statement(" \n\t "), ("skip", "no text")),
        ("plain: a blank line is a second paragraph", plain_statement("One.\n\nTwo."), ("skip", "multi-paragraph")),
        ("plain: a whitespace-only line is a blank line", plain_statement("One.\n  \nTwo."), ("skip", "multi-paragraph")),
        ("xhtml: a div of text is a statement", xhtml("<x:div>The a shall hold.</x:div>"), ("draft", "The a shall hold.")),
        ("xhtml: a p is a wrapper too", xhtml("<x:p> The  a\n shall hold. </x:p>"), ("draft", "The a shall hold.")),
        ("xhtml: entities are decoded by the parser", xhtml("<x:div>a &amp; b &lt; c</x:div>"), ("draft", "a & b < c")),
        ("xhtml: no value is no text", xhtml_statement(None), ("skip", "no text")),
        ("xhtml: an empty value is no text", xhtml(""), ("skip", "no text")),
        ("xhtml: a blank div is no text", xhtml("<x:div>  </x:div>"), ("skip", "no text")),
        ("xhtml: blank divs are no text, not multi-paragraph", xhtml("<x:div> </x:div><x:div/>"), ("skip", "no text")),
        ("xhtml: two divs are multi-paragraph", xhtml("<x:div>One.</x:div><x:div>Two.</x:div>"), ("skip", "multi-paragraph")),
        ("xhtml: a br is rich markup, named", xhtml("<x:div>a<x:br/>b</x:div>"), ("skip", "rich markup (br)")),
        ("xhtml: a span is rich markup, named", xhtml("<x:div>a <x:span>b</x:span></x:div>"), ("skip", "rich markup (span)")),
        ("xhtml: a sub is never flattened", xhtml("<x:div>m<x:sub>2</x:sub></x:div>"), ("skip", "rich markup (sub)")),
        ("xhtml: a list is rich markup", xhtml("<x:div><x:ul><x:li>x</x:li></x:ul></x:div>"), ("skip", "rich markup (ul)")),
        ("xhtml: an object is not blank", xhtml("<x:div><x:object data='a.png'/></x:div>"), ("skip", "rich markup (object)")),
        ("xhtml: a br alone is not blank", xhtml("<x:div><x:br/></x:div>"), ("skip", "rich markup (br)")),
        ("xhtml: a wrapper that is not div/p is named", xhtml("<x:ul><x:li>x</x:li></x:ul>"), ("skip", "rich markup (ul)")),
        ("xhtml: a div in another namespace is not a wrapper", xhtml("<other:div>x</other:div>"), ("skip", "rich markup (div)")),
        ("xhtml: bare text has no wrapper", xhtml("just text"), ("skip", "rich markup (text outside a div or p wrapper)")),
        ("xhtml: a nested block is named", xhtml("<x:div><x:div>x</x:div></x:div>"), ("skip", "rich markup (div)")),
        ("skip: the order is the documented one", REASONS, ("no text", "multi-paragraph", "rich markup")),
        ("value: a string value is read as plain text", value_statement(_value("STRING", text="A. B.")), ("draft", "A. B.")),
        ("value: an absent value is no text", value_statement(None), ("skip", "no text")),
        ("value: an integer is not text", value_statement(number), ("skip", "not text (INTEGER)")),
        ("select: a named type is selected", judged(_value("STRING", text="Ok."))[0], "draft"),
        (
            "select: another type is not selected, counted under its name",
            classify(_object("O", "Note", []), {"Requirement"}, [], "Text"),
            ("not selected", "Note"),
        ),
        (
            "select: an unnamed type is counted under its identifier",
            classify(SpecObject("O", ObjectType("T-9", None, []), []), {"Requirement"}, [], "Text"),
            ("not selected", "T-9"),
        ),
        ("select: a type is matched by LONG-NAME", type_matches(ObjectType("T-1", "Req", []), {"Req"}), True),
        ("select: a type is matched by IDENTIFIER", type_matches(ObjectType("T-1", "Req", []), {"T-1"}), True),
        ("select: an unnamed type is not matched by `(unnamed)`", type_matches(ObjectType("T-1", None, []), {UNNAMED}), False),
        ("where: an enumeration matches either value", selected(_object("O", "Requirement", [status]), set(), [("Status", "Approved")]), True),
        ("where: every clause must hold", selected(_object("O", "Requirement", [status, number]), set(), [("Status", "Draft"), ("Num", "6")]), False),
        ("where: an integer compares by its decimal text", selected(_object("O", "Requirement", [number]), set(), [("Num", "5")]), True),
        ("where: an absent attribute never matches", selected(_object("O", "Requirement", []), set(), [("Num", "5")]), False),
        ("where: --type and --where both hold", selected(_object("O", "Note", [number]), {"Requirement"}, [("Num", "5")]), False),
        (
            "where: XHTML compares its flattened paragraph",
            selected(_object("O", "R", [_value("XHTML", "T", holder=_fragment("<x:div>a  b</x:div>"))]), set(), [("T", "a b")]),
            True,
        ),
        ("human id: an integer is its decimal text", human_text(number), "5"),
        ("human id: an enumeration is its names", human_text(status), "Draft,Approved"),
        ("carried: a blank string says nothing", has_value(_value("STRING", text="  ")), False),
        ("carried: an enumeration with a value says something", has_value(status), True),
        ("carried: an integer 0 says something", has_value(_value("INTEGER", text="0")), True),
        (
            "reconcile: the line adds up and breaks the remainder down by type",
            reconcile_line(10, 5, 3, {"Note": 1, "Heading": 1}),
            "objects 10 = drafted 5 + skipped 3 + not selected 2 (by type: Heading 1, Note 1)",
        ),
        (
            "reconcile: nothing not selected has no breakdown",
            reconcile_line(2, 1, 1, {}),
            "objects 2 = drafted 1 + skipped 1 + not selected 0",
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
        ("draft: U+2028 is escaped, not left as a YAML line break", yaml_quote("a b"), '"a\\u2028b"'),
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
            "draft: the provenance is the origin and the IDENTIFIER",
            field(draft, "  source: "),
            'source: "https://example.test/module.reqif#_1_abc-def"',
        ),
        (
            "draft: the statement's quotes and backslashes are escaped",
            field(draft, "statement: "),
            'statement: "The tool shall print \\"a: b\\" and a back\\\\slash."',
        ),
        (
            "draft: the header names the file, the IDENTIFIER and the human id",
            "module.reqif (object _1_abc-def; ReqIF.ForeignID 12)" in draft,
            True,
        ),
        ("report: a newline in a name cannot end a line", one_line("a\nb\x00c"), "a b c"),
        ("file: a DOCTYPE is refused before parsing", _refuses(lambda: check_bytes(b"<!DOCTYPE x>", "f")), True),
        ("file: an entity declaration is refused", _refuses(lambda: check_bytes(b"<!ENTITY a 'b'>", "f")), True),
        ("file: UTF-16 is refused, its DOCTYPE would be invisible", _refuses(lambda: check_bytes("<a/>".encode("utf-16"), "f")), True),
        ("file: text that is not XML is refused", _refuses(lambda: parse_root(b"not xml", "f")), True),
        ("file: a root that is not REQ-IF is refused", _refuses(lambda: parse_root(f'<A xmlns="{REQIF_NS}"/>'.encode(), "f")), True),
        ("file: another namespace is refused", _refuses(lambda: parse_root(b'<REQ-IF xmlns="urn:other"/>', "f")), True),
        ("file: no namespace is refused", _refuses(lambda: parse_root(b"<REQ-IF/>", "f")), True),
        ("file: a REQ-IF in the ReqIF namespace is read", parse_root(f'<REQ-IF xmlns="{REQIF_NS}"/>'.encode(), "f").tag, q("REQ-IF")),
        (
            "file: a prefix is not a namespace (the XHTML prefix may be anything)",
            is_block(ET.fromstring(f'<q:div xmlns:q="{XHTML_NS}"/>')),
            True,
        ),
    ]

    failures = 0
    for label, got, want in rows:
        ok = got == want
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'} {label:<68} {got!r}")
    print(f"reqif_import --self-check: {len(rows)} case(s), {failures} failure(s)")
    return 1 if failures else 0


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


def where_arg(text):
    """`NAME=VALUE`, split at the first `=`; an empty NAME is a usage error."""
    value_arg(text)
    name, separator, value = text.partition("=")
    if not separator or not name.strip():
        raise argparse.ArgumentTypeError(
            f"{text!r} is not NAME=VALUE (split at the first `=`; NAME must not be empty)"
        )
    return name, value


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="tools/reqif_import.py",
        description=(
            "Draft req claims from a ReqIF file: one `req.claim/2` claim per plain requirement, "
            "with the object IDENTIFIER in its provenance, and nothing derived."
        ),
        epilog=HELP_EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
        allow_abbrev=False,
    )
    parser.add_argument(
        "--from", dest="document", required=True, type=value_arg, metavar="<file.reqif>",
        help="the file to read; a relative path is resolved against the corpus root",
    )
    parser.add_argument(
        "--list", action="store_true",
        help="print the header, specifications and object types; write nothing, needs no corpus",
    )
    parser.add_argument(
        "--origin", type=named_arg, metavar="<url-or-path>",
        help="where the file was retrieved from; recorded as `<origin>#<IDENTIFIER>` in each draft",
    )
    parser.add_argument(
        "--type", action="append", default=[], type=named_arg, metavar="<type>",
        help="an object type to read, by LONG-NAME else IDENTIFIER (repeatable)",
    )
    parser.add_argument(
        "--where", action="append", default=[], type=where_arg, metavar="NAME=VALUE",
        help="only objects whose attribute NAME has VALUE (repeatable; all must hold)",
    )
    parser.add_argument(
        "--text-attr", type=named_arg, metavar="<NAME>",
        help="the attribute whose value is the statement (required, no default)",
    )
    parser.add_argument(
        "--id-attr", type=named_arg, metavar="<NAME>",
        help="the attribute holding your own id; shown in the report, never the claim_id",
    )
    parser.add_argument(
        "--date", type=value_arg, metavar="YYYY-MM-DD",
        help="the rationale date (the file's retrieval date); never invented",
    )
    parser.add_argument(
        "--root", type=value_arg, metavar="<dir>",
        help="the corpus root (default: $REQ_ROOT, else the working directory)",
    )
    parser.add_argument("--write", action="store_true", help="write the drafts (the default is a dry run)")
    parser.add_argument(
        "--self-check", action="store_true",
        help="table-drive the pure half; read no file, no corpus, no `req`",
    )
    return parser.parse_args(argv)


def check_usage(args):
    """The rules argparse cannot express: what `--list` forbids and what drafting requires."""
    drafting = {
        "--origin": args.origin,
        "--type": args.type or None,
        "--where": args.where or None,
        "--text-attr": args.text_attr,
        "--id-attr": args.id_attr,
        "--date": args.date,
    }
    if args.list:
        given = [flag for flag, value in drafting.items() if value is not None]
        if args.write:
            given.append("--write")
        if given:
            raise Usage(f"--list reads and writes nothing; it cannot take {', '.join(given)}")
        return
    for flag in ("--origin", "--text-attr", "--date"):
        if drafting[flag] is None:
            raise Usage(f"{flag} is required (unless --list)")
    if not args.type and not args.where:
        raise Usage(
            "at least one of --type and --where is required: there is no default selection (run "
            "--list to see what the file holds)"
        )
    if not valid_date(args.date):
        raise Usage(f"--date must be a calendar date in YYYY-MM-DD form, not {args.date!r}")


def main(argv):
    # `--self-check` stands alone: it must work before a corpus exists and without `req`, so it
    # is answered before the required arguments are demanded of it. Any other argument beside
    # it is a usage error, not a quiet run of two things at once.
    if "--self-check" in argv:
        if len(argv) != 1:
            print(
                "usage: tools/reqif_import.py --self-check takes no other arguments",
                file=sys.stderr,
            )
            return 2
        return self_check()

    args = parse_args(argv)
    args.type = list(dict.fromkeys(args.type))
    try:
        check_usage(args)
        return run_list(args) if args.list else run(args)
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
        # A crash is not "a definite problem" about the file: the tri-state answer to a question
        # this run could not settle is UNVERIFIABLE, never a FAIL the caller reads as their
        # input's fault.
        print(f"[UNVERIFIABLE] internal error: {error!r}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
