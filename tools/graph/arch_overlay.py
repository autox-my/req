#!/usr/bin/env python3
"""Overlay the claims a repository's files satisfy onto an archify `architecture` diagram (ADR 0052).

usage: python3 -I tools/graph/arch_overlay.py [--root <dir>] [--test-report <junit.xml>] [--matrix-dir <dir>] [--page <path>]
                                              <diagram.json> <satisfy.json> <trace.json> <out-dir>

Inputs, all read-only:
  <diagram.json>  an archify `architecture` file (schema_version 1) drawn from a repository: components cite files in
                  `sources`. A component without `sources` is kept and drawn as no claim.
  <satisfy.json>  `req satisfy --format json` (`req.satisfy/1`): every link `req-satisfies:` declares, with its outcome.
  <trace.json>    `req trace --format json` (`req.trace/1`): the claims, their statements, and what the matrix reads of each
                  (`source`, `evidence`, `tests`; a document without them reads as no source, machine evidence, no test).
                  The matrix rows are the claims that do not carry `applies: does_not_apply`.
  --root <dir>    the corpus root the cited paths and `.req/design` are read under (default: the current directory).
  --test-report <junit.xml>
                  a JUnit report (nextest's subset, `classname::name` ids, ADR 0024): the matrix then also says what each linked
                  test did (passed, failed, skipped, not found). Without it no outcome is written, so the committed matrix is a
                  function of the repository alone (ADR 0053 decision 5); a report that cannot be read is exit 2 naming it.
  --matrix-dir <dir>
                  the directory, relative to the root, the matrix will be read from (default: the root itself); the claim links
                  are written relative to it.
  --page <path>   the architecture page this diagram is rendered to, as the matrix will reach it (a relative URL such as
                  `../architecture.html`): each component in the matrix then links to its node there, `<path>#focus=<component
                  id>`, the fragment archify's viewer focuses a node by (7e45f36, `syncFocusFromHash`). Without it a component
                  is plain text: no page is guessed.

A component satisfies the union of the claims its cited files declare (ADR 0052 decision 1): a link is found by the
file path alone, never by the component's id, and a line range in `sources` is ignored. Its status is the worst
outcome among those links, worst first: stale (a cited path does not exist under the root; not an outcome),
unknown_claim, orphaned, suspect, unratified, holds_weakened, holds; a component with no link is "no claim".

Writes `<out-dir>/overlay.json` (the diagram, presentation only: ids, order, geometry, connections and labels are the
author's; the status line is the component's `sublabel`, the line archify draws at every zoom, and the author's own
`sublabel` moves to the `tag` after any tag the author wrote, so no text is lost; three cards are appended, the claim ids
are on the status card; the `meta.subtitle` says which revision, which `rules_v` and which archify the page is made from)
and `<out-dir>/matrix.md` (ADR 0053: the coverage matrix, a row per claim and four columns, Source | Claim | Satisfied by |
Verified by, whatever the number of components; a gap is named in words, never left blank: `no source`, `no design element`,
`no test`, `<evidence> evidence, no test asked`. *Satisfied by* names each component whose cited files link the claim, with
its worst outcome for it; the claims no component reaches are on the overlay's card).

The status line is the worst state and how many claims are in it: `HOLDS · 6 claims`, `SUSPECT · 1 of 3 claims`, `STALE`
(a cited path is missing), `no claim` (no link).

Exit: 0 written; 3 written, but a component is stale, a link names an unknown claim, or a status line or an author's text
does not fit its box (archify would refuse the overlay: component-sublabel-overflow, component-tag-overflow); the reason
is printed and the text is written whole; 2 refused (a wrong
schema, malformed JSON, an unreadable input or an out-dir that exists and is not empty), with nothing written.
Deterministic: the same inputs give the same bytes (no clock, no randomness, no network). Python standard library only;
it never opens an input for writing and writes only inside `<out-dir>`.

Written against archify `tt-a1i/archify` at commit 7e45f36 (Apache-2.0, `architecture` type); the pin is tooling
provenance, recorded here and in `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`.
"""

import json
import posixpath
import sys
import xml.etree.ElementTree as ET
import xml.parsers.expat as expat
from pathlib import Path

# The archify commit this converter was written against (ADR 0046 decision 7, ADR 0052 decision 8).
ARCHIFY_COMMIT = "7e45f36"
SATISFY_SCHEMA = "req.satisfy/1"
TRACE_SCHEMA = "req.trace/1"
DESIGN = ".req/design"

# The outcomes a `req.satisfy/1` link carries, worst first (ADR 0052 decision 2), and the word each is drawn with.
OUTCOMES = ("unknown_claim", "orphaned", "suspect", "unratified", "holds_weakened", "holds")
WORD = {
    "stale": "STALE",
    "unknown_claim": "UNKNOWN CLAIM",
    "orphaned": "ORPHANED",
    "suspect": "SUSPECT",
    "unratified": "UNRATIFIED",
    "holds_weakened": "HOLDS (weakened)",
    "holds": "HOLDS",
    "no_claim": "NO CLAIM",
}
# stale ranks worst of all; no claim has no rank (it is the absence of a link).
RANK = {"stale": -1, **{outcome: index for index, outcome in enumerate(OUTCOMES)}, "no_claim": len(OUTCOMES)}

# The word a link's outcome is shown with in the matrix (ADR 0053 decision 1).
OUTCOME_TEXT = {
    "unknown_claim": "unknown claim",
    "orphaned": "orphaned",
    "suspect": "suspect",
    "unratified": "unratified",
    "holds_weakened": "holds (weakened)",
    "holds": "holds",
}
HEADER = ("Source", "Claim", "Satisfied by", "Verified by")
GITHUB = "https://github.com/"

STATUS_CARD = "Components by status"
UNREACHED_CARD = "Claims no component reaches"
OUTSIDE_CARD = "Cited paths outside every design root"
# archify's component box when a component gives no `size`: `layout.defaultW` (renderers/architecture/render-architecture.mjs
# at 7e45f36). The grid's `cellW` is the step between boxes, never a box's width.
DEFAULT_WIDTH = 120
# archify draws a sublabel (at every zoom, `componentTextFit.sublabelMinimum` 6) and a tag (at the finest zoom,
# `tagMinimum` 6) each as one unwrapped line, shrunk to a 6 px floor at 0.6 px per column per px of font, inside the box
# less 8 px of padding (renderers/shared/text-fit.mjs and render-architecture.mjs at 7e45f36): (width - 8) / 3.6 columns.
# Both fields have the same floor, so one budget serves both. Integer arithmetic.
SEP = " | "


class Refusal(Exception):
    """The input is not a document this converter was written for."""


def refuse(message):
    raise Refusal(message)


# The code points archify counts as two columns, read from its `textUnits` rule (renderers/shared/utils.mjs at 7e45f36)
# and re-stated here as inclusive ranges: emoji-presentation symbols, Hangul Jamo, the CJK blocks and their neighbours,
# the full-width forms, and the Mahjong-to-symbols and CJK planes. A code point outside them is one column. It is not
# the Unicode East Asian Width table: archify's list leaves out, for example, U+1D360 (one column) and includes U+1F000.
WIDE = (
    (0x1100, 0x115F), (0x231A, 0x231B), (0x2329, 0x232A), (0x23E9, 0x23EC), (0x23F0, 0x23F0), (0x23F3, 0x23F3),
    (0x25FD, 0x25FE), (0x2614, 0x2615), (0x2630, 0x2637), (0x2648, 0x2653), (0x267F, 0x267F), (0x268A, 0x268F),
    (0x2693, 0x2693), (0x26A1, 0x26A1), (0x26AA, 0x26AB), (0x26BD, 0x26BE), (0x26C4, 0x26C5), (0x26CE, 0x26CE),
    (0x26D4, 0x26D4), (0x26EA, 0x26EA), (0x26F2, 0x26F3), (0x26F5, 0x26F5), (0x26FA, 0x26FA), (0x26FD, 0x26FD),
    (0x2705, 0x2705), (0x270A, 0x270B), (0x2728, 0x2728), (0x274C, 0x274C), (0x274E, 0x274E), (0x2753, 0x2755),
    (0x2757, 0x2757), (0x2795, 0x2797), (0x27B0, 0x27B0), (0x27BF, 0x27BF), (0x2B1B, 0x2B1C), (0x2B50, 0x2B50),
    (0x2B55, 0x2B55), (0x2E80, 0xA4CF), (0xA960, 0xA97C), (0xAC00, 0xD7A3), (0xF900, 0xFAFF), (0xFE10, 0xFE19),
    (0xFE30, 0xFE6F), (0xFF01, 0xFF60), (0xFFE0, 0xFFE6), (0x16FE0, 0x18DFF), (0x1AFF0, 0x1AFFF), (0x1B000, 0x1B2FF),
    (0x1F000, 0x1FAFF), (0x20000, 0x3FFFD),
)
VARIATION_FIRST, VARIATION_LAST, EMOJI_PRESENTATION = 0xFE00, 0xFE0F, 0xFE0F


def units(text):
    """Columns archify counts `text` as: a variation selector adds none; a character followed by U+FE0F takes two; a
    character in WIDE takes two; any other takes one. Surrogates are not special: a code point is one character."""
    codes = [ord(char) for char in text]
    total = 0
    for index, code in enumerate(codes):
        if VARIATION_FIRST <= code <= VARIATION_LAST:
            continue
        following = codes[index + 1] if index + 1 < len(codes) else -1
        wide = following == EMOJI_PRESENTATION or any(low <= code <= high for low, high in WIDE)
        total += 2 if wide else 1
    return total


def no_constant(name):
    raise ValueError(f"{name} is not JSON")


def load(path, what):
    try:
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
    except (OSError, UnicodeDecodeError) as error:
        refuse(f"cannot read {what} {path}: {error}")
    try:
        return json.loads(text, parse_constant=no_constant)
    except ValueError as error:
        refuse(f"malformed JSON in {what} {path}: {error}")


def check_diagram(document, path):
    if not isinstance(document, dict):
        refuse(f"{path}: the diagram is not a JSON object")
    if document.get("diagram_type") != "architecture":
        refuse(f"{path}: diagram_type is {document.get('diagram_type')!r}: this converter reads only an archify `architecture` diagram")
    if document.get("schema_version") != 1 or isinstance(document.get("schema_version"), bool):
        refuse(f"{path}: schema_version is {document.get('schema_version')!r}: this converter reads only schema_version 1")
    if not isinstance(document.get("meta"), dict):
        refuse(f"{path}: `meta` is not an object")
    components = document.get("components")
    if not isinstance(components, list):
        refuse(f"{path}: `components` is not an array")
    for index, component in enumerate(components):
        where = f"{path}: components[{index}]"
        if not isinstance(component, dict):
            refuse(f"{where} is not an object")
        for key in ("id", "label"):
            if not isinstance(component.get(key), str):
                refuse(f"{where} has no string `{key}`")
        if "tag" in component and not isinstance(component["tag"], str):
            refuse(f"{where}: `tag` is not a string")
        if "sources" in component:
            sources = component["sources"]
            if not isinstance(sources, list):
                refuse(f"{where}: `sources` is not an array")
            for source in sources:
                if not isinstance(source, dict) or not isinstance(source.get("path"), str):
                    refuse(f"{where}: a `sources` entry has no string `path`")
    if "cards" in document and not isinstance(document["cards"], list):
        refuse(f"{path}: `cards` is not an array")


def check_satisfy(document, path):
    if not isinstance(document, dict):
        refuse(f"{path}: the document is not a JSON object")
    if document.get("schema") != SATISFY_SCHEMA:
        refuse(f"{path}: unknown schema {document.get('schema')!r}: this converter reads only {SATISFY_SCHEMA}")
    rules_v = document.get("rules_v")
    if not isinstance(rules_v, int) or isinstance(rules_v, bool):
        refuse(f"{path}: `rules_v` is not an integer")
    links = document.get("links")
    if not isinstance(links, list):
        refuse(f"{path}: `links` is not an array")
    for index, link in enumerate(links):
        where = f"{path}: links[{index}]"
        if not isinstance(link, dict):
            refuse(f"{where} is not an object")
        for key in ("element", "claim_id", "outcome"):
            if not isinstance(link.get(key), str):
                refuse(f"{where} has no string `{key}`")
        if link["outcome"] not in OUTCOMES:
            refuse(f"{where}: unknown outcome {link['outcome']!r}: this converter reads {', '.join(OUTCOMES)}")


def check_trace(document, path):
    if not isinstance(document, dict):
        refuse(f"{path}: the document is not a JSON object")
    if document.get("schema") != TRACE_SCHEMA:
        refuse(f"{path}: unknown schema {document.get('schema')!r}: this converter reads only {TRACE_SCHEMA}")
    nodes = document.get("nodes")
    if not isinstance(nodes, list):
        refuse(f"{path}: `nodes` is not an array")
    for index, node in enumerate(nodes):
        where = f"{path}: nodes[{index}]"
        if not isinstance(node, dict):
            refuse(f"{where} is not an object")
        for key in ("claim_id", "statement"):
            if not isinstance(node.get(key), str):
                refuse(f"{where} has no string `{key}`")
        for key in ("source", "evidence"):
            if node.get(key) is not None and not isinstance(node[key], str):
                refuse(f"{where}: `{key}` is neither a string nor null")
        tests = node.get("tests", [])
        if not isinstance(tests, list) or not all(isinstance(test, str) for test in tests):
            refuse(f"{where}: `tests` is not an array of strings")


def design_roots(root):
    """The `.req/design` roots under `root`, repository-relative; none when the file is absent."""
    path = root / DESIGN
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return []
    except (OSError, UnicodeDecodeError) as error:
        refuse(f"{path} cannot be read ({error}): a design list that became unusable is not absent")
    roots = []
    for raw in text.split("\n"):
        line = raw.rstrip("\r").split("#", 1)[0].strip()
        if not line:
            continue
        while line.startswith("./"):
            line = line[2:]
        # `req satisfy` refuses an absolute path or a `..` segment (exit 2); such a line covers nothing here.
        if line.startswith(("/", "\\")) or ".." in line.replace("\\", "/").split("/"):
            continue
        # By segments, as `req satisfy` does: empty and `.` segments say nothing; none left is the whole repository.
        roots.append("/".join(part for part in line.split("/") if part and part != "."))
    return roots


def inside(path, roots):
    """Whether `path` is under a design root; the empty root is the whole repository."""
    return any(not base or path == base or path.startswith(base + "/") for base in roots)


def exists(root, path):
    """A cited path names a file or directory under the root. An absolute path or one that leaves it names nothing."""
    parts = path.split("/")
    if path.startswith("/") or "\\" in path or ".." in parts or not path:
        return False
    return (root / path).exists()


def applying(node):
    return node.get("applies") not in (False, "does_not_apply")


def clean(text):
    return " ".join(text.split())


def cell_label(text):
    return clean(text).replace("|", "\\|")


class ReportError(Exception):
    """A JUnit report this converter will not read; the reason names it."""


def local(tag):
    """An element's name without its namespace (`{urn:junit}testsuites` is `testsuites`); a comment's tag is not a string."""
    return tag.rsplit("}", 1)[-1] if isinstance(tag, str) else ""


def read_report(path):
    """The outcomes a JUnit report holds: {`classname::name`: passed | failed | skipped}.

    Read as `src/junit.rs` reads nextest's subset (ADR 0024), written again here and never copied: the root is `<testsuites>`
    or `<testsuite>` (matched by local name, so a default namespace is read as `req gate --test-report` reads it); a `<testcase>` needs a non-empty `classname` and `name` and is `failed` when it holds a `<failure>` or an
    `<error>`, `skipped` when it holds a `<skipped>` and no failure, else `passed`; a conflicting duplicate id is refused
    rather than chosen between, and a DTD is refused. An unreadable or malformed report is `ReportError`."""
    try:
        data = Path(path).read_bytes()
        text = data.decode("utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise ReportError(f"cannot read test report {path}: {error}") from None
    declared = []
    scan = expat.ParserCreate()
    scan.StartDoctypeDeclHandler = lambda *_: declared.append("a DOCTYPE")
    scan.EntityDeclHandler = lambda *_: declared.append("an ENTITY")
    try:
        scan.Parse(data, True)
    except expat.ExpatError as error:
        raise ReportError(f"cannot parse test report {path}: {error}") from None
    if declared:
        raise ReportError(f"cannot parse test report {path}: it declares {declared[0]}, which a JUnit report does not carry")
    try:
        root = ET.fromstring(data)
    except (ET.ParseError, ValueError) as error:
        raise ReportError(f"cannot parse test report {path}: {error}") from None
    if local(root.tag) not in ("testsuites", "testsuite"):
        raise ReportError(f"cannot parse test report {path}: unexpected root element <{local(root.tag)}>: expected <testsuites> or <testsuite>")
    cases = []
    for child in root:
        if local(child.tag) == "testcase":
            cases.append(child)
        elif local(root.tag) == "testsuites" and local(child.tag) == "testsuite":
            cases.extend(case for case in child if local(case.tag) == "testcase")
    outcomes = {}
    for case in cases:
        name, classname = case.get("name"), case.get("classname")
        if name is None or classname is None:
            raise ReportError(f"cannot parse test report {path}: a <testcase> lacks `name` or `classname`")
        if not name or not classname:
            raise ReportError(f"cannot parse test report {path}: a <testcase> has an empty `name` or `classname`: the id is `classname::name`")
        kinds = {local(child.tag) for child in case}
        outcome = "failed" if kinds & {"failure", "error"} else "skipped" if "skipped" in kinds else "passed"
        identity = f"{classname}::{name}"
        if outcomes.setdefault(identity, outcome) != outcome:
            raise ReportError(f"cannot parse test report {path}: duplicate test id `{identity}` with conflicting outcomes")
    return outcomes


def cell_text(text):
    """Text for one Markdown table cell: line breaks fold to a space and a pipe is escaped."""
    return text.replace("\r\n", " ").replace("\n", " ").replace("\r", " ").replace("|", "\\|")


def url_path(path, extra="/"):
    """`path` for a URL, keeping letters, digits and `-._~` (and `extra`) and writing every other byte as `%XX`."""
    safe = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~" + extra
    return "".join(char if char in safe else "".join(f"%{byte:02X}" for byte in char.encode("utf-8")) for char in path)


def github_blob(diagram):
    """`https://github.com/<owner>/<repo>/blob/<revision>` for a diagram whose repository is on GitHub and pinned; else None
    (a file is then named without a link, never pointed at a revision that is not there)."""
    repository = diagram["meta"].get("repository")
    if not isinstance(repository, dict):
        return None
    url, revision = repository.get("url"), repository.get("revision")
    if not isinstance(url, str) or not isinstance(revision, str) or not revision or not url.startswith(GITHUB):
        return None
    url = url.rstrip("/")
    if url.endswith(".git"):
        url = url[: -len(".git")]
    return f"{url}/blob/{url_path(revision)}"


def satisfied_by(claim, states, links_by_element, blob, page):
    """The *Satisfied by* cell: each component of the diagram whose cited files link `claim`, in diagram order, as
    `<label> (<worst outcome>)` (the label linked to the component's node on `page`, when there is one) and the files that
    carry the link, each once, linked at the pinned revision. No line anchor: a link's line comes from the working tree and
    the URL pins a revision, so after a line moves the anchor would name the wrong line of the pinned file."""
    parts = []
    for state in states:
        outcome = state["outcomes"].get(claim)
        if outcome is None:
            continue
        files = []
        for path in state["paths"]:
            ours = [link for link in links_by_element.get(path, []) if link["claim_id"] == claim]
            if not ours:
                continue
            name = f"`{path}`"
            files.append(f"[{name}]({blob}/{url_path(path)})" if blob else name)
        label = cell_label(state["component"]["label"])
        if page is not None:
            label = f"[{label}]({url_path(page)}#focus={url_path(state['component']['id'], extra='')})"
        text = f"{label} ({OUTCOME_TEXT[outcome]})"
        parts.append(text + (": " + ", ".join(files) if files else ""))
    return "<br>".join(parts) or "no design element"


def verified_by(node, report):
    """The *Verified by* cell: the linked tests, with their outcome when a report was given; a gap in words otherwise."""
    tests = node.get("tests") or []
    evidence = node.get("evidence") or "machine"
    if not tests:
        return "no test" if evidence == "machine" else f"{cell_text(evidence)} evidence, no test asked"
    listed = []
    for test in tests:
        item = cell_text(test)
        if report is not None:
            item += f" ({report.get(test, 'not found')})"
        listed.append(item)
    noun = "test" if len(tests) == 1 else "tests"
    text = f"{len(tests)} {noun}: " + ", ".join(listed)
    return text if evidence == "machine" else f"{text}; {cell_text(evidence)} evidence"


def coverage_matrix(nodes, states, links_by_element, blob, matrix_dir, report, page):
    """ADR 0053's matrix, as Markdown: one row per applying claim by id, four columns, a named gap in every empty cell."""
    claims_dir = posixpath.relpath("claims", matrix_dir)
    table = ["| " + " | ".join(HEADER) + " |", "| " + " | ".join(["---"] * len(HEADER)) + " |"]
    for node in nodes:
        claim = node["claim_id"]
        source = (node.get("source") or "").strip()
        claim_cell = f"[{claim}]({url_path(claims_dir)}/{url_path(claim)}.yml): {cell_text(clean(node['statement']))}"
        table.append(
            "| "
            + " | ".join(
                [
                    cell_text(source) if source else "no source",
                    claim_cell,
                    satisfied_by(claim, states, links_by_element, blob, page),
                    verified_by(node, report),
                ]
            )
            + " |"
        )
    return "\n".join(table) + "\n"


def budget(width):
    """Columns archify draws in a box of `width` px at its 6 px floor (the sublabel and the tag share this floor)."""
    return int(((width - 8) * 10) // 36)


def status_line(status, claims, outcomes):
    """The status word and how many claims are in that state: `HOLDS · 6 claims`, `SUSPECT · 1 of 3 claims`.

    A stale component says only STALE (a cited path is missing; its links are not the news), and a component with no link
    says `no claim`. A claim counts under its worst link."""
    if status == "stale":
        return WORD["stale"]
    if status == "no_claim":
        return "no claim"
    inside = sum(1 for claim in claims if outcomes[claim] == status)
    noun = "claim" if len(claims) == 1 else "claims"
    if inside == len(claims):
        return f"{WORD[status]} · {len(claims)} {noun}"
    return f"{WORD[status]} · {inside} of {len(claims)} {noun}"


def moved_text(component):
    """The author's tag, then the author's sublabel, which this overlay gives up the sublabel line for."""
    parts = [component[key] for key in ("tag", "sublabel") if component.get(key)]
    return SEP.join(parts)


def box_width(component):
    """archify's box: `size[0]`, else `layout.defaultW`."""
    size = component.get("size")
    if isinstance(size, list) and size and isinstance(size[0], (int, float)) and not isinstance(size[0], bool):
        return size[0]
    return DEFAULT_WIDTH


def overlay(diagram, satisfy, trace, root, report=None, matrix_dir=".", page=None):
    """The overlay diagram, the matrix text and the reasons the exit is 3."""
    links = satisfy["links"]
    by_element = {}
    for link in links:
        by_element.setdefault(link["element"], []).append(link)
    roots = design_roots(root)

    states = []
    for component in diagram["components"]:
        paths = []
        for source in component.get("sources", []):
            if source["path"] not in paths:
                paths.append(source["path"])
        stale = [path for path in paths if not exists(root, path)]
        own = [link for path in paths for link in by_element.get(path, [])]
        if stale:
            status = "stale"
        elif own:
            status = min((link["outcome"] for link in own), key=RANK.__getitem__)
        else:
            status = "no_claim"
        outcomes = {}
        for link in own:
            best = outcomes.get(link["claim_id"])
            if best is None or RANK[link["outcome"]] < RANK[best]:
                outcomes[link["claim_id"]] = link["outcome"]
        states.append(
            {
                "component": component,
                "paths": paths,
                "stale": stale,
                "status": status,
                "claims": sorted(outcomes),
                "outcomes": outcomes,
            }
        )

    # The components, status first (worst first), then in the diagram's order.
    ordered = sorted(range(len(states)), key=lambda index: (RANK[states[index]["status"]], index))
    status_items = []
    for index in ordered:
        state = states[index]
        item = f"[{WORD[state['status']]}] {state['component']['label']}"
        if state["stale"]:
            item += " — missing " + ", ".join(state["stale"])
        if state["claims"]:
            item += " — " + ", ".join(state["claims"])
        status_items.append(item)

    rows = []
    seen = set()
    nodes = []
    for node in trace["nodes"]:
        if node["claim_id"] not in seen and applying(node):
            seen.add(node["claim_id"])
            rows.append((node["claim_id"], clean(node["statement"])))
            nodes.append(node)
    rows.sort()
    nodes.sort(key=lambda node: node["claim_id"])
    reached = {claim for state in states for claim in state["claims"]}
    unreached = [(claim, statement) for claim, statement in rows if claim not in reached]

    outside = {}
    for state in states:
        for path in state["paths"]:
            if not inside(path, roots):
                outside.setdefault(path, []).append(state["component"]["label"])
    outside_items = [f"{path} (cited by {', '.join(labels)})" for path, labels in outside.items()]

    result = json.loads(json.dumps(diagram))  # a deep copy, in the author's key order
    overflow = []
    for component, state in zip(result["components"], states):
        width = box_width(component)
        moved = moved_text(component)
        line = status_line(state["status"], state["claims"], state["outcomes"])
        # Written whole either way: the status is never cut, the author's text is never dropped; one that does not fit is named.
        for field, text, reason in (
            ("sublabel", line, "widen `size`: the status line is never cut"),
            ("tag", moved, "widen `size` or shorten the author's tag and sublabel"),
        ):
            if text and units(text) > budget(width):
                overflow.append(
                    f"{field} overflow: component {component['id']!r} ({component['label']}): {text!r} needs "
                    f"{units(text)} columns and a {width} px box draws {budget(width)}; the text is kept whole, so "
                    f"archify validate would refuse the overlay (component-{field}-overflow): {reason}"
                )
        component["sublabel"] = line
        if moved:
            component["tag"] = moved
        else:
            component.pop("tag", None)

    cards = result.setdefault("cards", [])
    cards.append({"dot": "violet", "title": STATUS_CARD, "items": status_items or ["none"]})
    cards.append(
        {
            "dot": "amber",
            "title": UNREACHED_CARD,
            "items": [f"{claim}: {statement}" for claim, statement in unreached] or ["none"],
        }
    )
    cards.append({"dot": "slate", "title": OUTSIDE_CARD, "items": outside_items or ["none"]})

    repository = diagram["meta"].get("repository")
    revision = repository.get("revision") if isinstance(repository, dict) else None
    parts = []
    if isinstance(result["meta"].get("subtitle"), str) and result["meta"]["subtitle"]:
        parts.append(result["meta"]["subtitle"])
    parts.append(f"req overlay of diagram revision {revision}" if revision else "req overlay of a diagram with no pinned revision")
    parts.append(f"rules_v {satisfy['rules_v']}")
    parts.append(f"archify {ARCHIFY_COMMIT}")
    result["meta"]["subtitle"] = " · ".join(parts)

    matrix = coverage_matrix(nodes, states, by_element, github_blob(diagram), matrix_dir, report, page)

    reasons = []
    for state in states:
        for path in state["stale"]:
            reasons.append(f"stale: component {state['component']['id']!r} ({state['component']['label']}) cites {path}, which does not exist under {root}")
    for link in links:
        if link["outcome"] == "unknown_claim":
            reasons.append(f"unknown claim: {link['element']} -> {link['claim_id']} names a claim the corpus does not hold")
    return result, matrix, reasons + overflow


def render(value):
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


USAGE = (
    "usage: arch_overlay.py [--root <dir>] [--test-report <junit.xml>] [--matrix-dir <dir>] [--page <path>] "
    "<diagram.json> <satisfy.json> <trace.json> <out-dir>"
)


def parse(argv):
    """(root, report path or None, matrix dir, page or None, the four positionals), or None for a usage error."""
    args = argv[1:]
    options = {"--root": ".", "--test-report": None, "--matrix-dir": ".", "--page": None}
    given = set()
    while args and args[0].startswith("--"):
        key = args[0]
        if key not in options or key in given or len(args) < 2 or args[1].startswith("--"):
            return None
        given.add(key)
        options[key], args = args[1], args[2:]
    if len(args) != 4 or any(arg.startswith("--") for arg in args):
        return None
    return Path(options["--root"]), options["--test-report"], options["--matrix-dir"], options["--page"], args[0], args[1], args[2], Path(args[3])


def main(argv):
    parsed = parse(argv)
    if parsed is None:
        print(USAGE, file=sys.stderr)
        return 2
    root, report_path, matrix_dir, page, diagram_path, satisfy_path, trace_path, target = parsed
    try:
        diagram = load(diagram_path, "the diagram")
        satisfy = load(satisfy_path, "the satisfy document")
        trace = load(trace_path, "the trace document")
        check_diagram(diagram, diagram_path)
        check_satisfy(satisfy, satisfy_path)
        check_trace(trace, trace_path)
        try:
            report = read_report(report_path) if report_path is not None else None
        except ReportError as error:
            refuse(str(error))
        if target.exists() and (not target.is_dir() or any(target.iterdir())):
            refuse(f"{target} exists and is not an empty directory: a stale overlay must not mix with a new one")
        result, matrix, reasons = overlay(diagram, satisfy, trace, root, report, matrix_dir, page)
        outputs = {"overlay.json": render(result), "matrix.md": matrix}
    except Refusal as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    try:
        target.mkdir(parents=True, exist_ok=True)
        for name, text in sorted(outputs.items()):
            with open(target / name, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
    except OSError as error:
        print(f"cannot write {target}: {error}", file=sys.stderr)
        return 2
    print(f"wrote {target / 'overlay.json'} and {target / 'matrix.md'}")
    for reason in reasons:
        print(reason, file=sys.stderr)
    return 3 if reasons else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
