#!/usr/bin/env python3
"""Overlay the claims a repository's files satisfy onto an archify `architecture` diagram (ADR 0052).

usage: python3 -I tools/graph/arch_overlay.py [--root <dir>] <diagram.json> <satisfy.json> <trace.json> <out-dir>

Inputs, all read-only:
  <diagram.json>  an archify `architecture` file (schema_version 1) drawn from a repository: components cite files in
                  `sources`. A component without `sources` is kept and drawn as no claim.
  <satisfy.json>  `req satisfy --format json` (`req.satisfy/1`): every link `req-satisfies:` declares, with its outcome.
  <trace.json>    `req trace --format json` (`req.trace/1`): the claims and their statements. The matrix rows are the
                  claims that do not carry `applies: does_not_apply`.
  --root <dir>    the corpus root the cited paths and `.req/design` are read under (default: the current directory).

A component satisfies the union of the claims its cited files declare (ADR 0052 decision 1): a link is found by the
file path alone, never by the component's id, and a line range in `sources` is ignored. Its status is the worst
outcome among those links, worst first: stale (a cited path does not exist under the root; not an outcome),
unknown_claim, orphaned, suspect, unratified, holds_weakened, holds; a component with no link is "no claim".

Writes `<out-dir>/overlay.json` (the diagram, presentation only: ids, order, geometry, connections and labels are the
author's; the status and the claim ids are added to the component's `tag`, after any tag the author wrote; three cards
are appended; the `meta.subtitle` says which revision, which `rules_v` and which archify the page is made from) and
`<out-dir>/matrix.md` (a claims x components table, then the claims no component reaches).

Exit: 0 written; 3 written, but a component is stale, a link names an unknown claim, or an author's tag leaves no room
for the status (archify would refuse the overlay: component-tag-overflow); the reason is printed; 2 refused (a wrong
schema, malformed JSON, an unreadable input or an out-dir that exists and is not empty), with nothing written.
Deterministic: the same inputs give the same bytes (no clock, no randomness, no network). Python standard library only;
it never opens an input for writing and writes only inside `<out-dir>`.

Written against archify `tt-a1i/archify` at commit 7e45f36 (Apache-2.0, `architecture` type); the pin is tooling
provenance, recorded here and in `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`.
"""

import json
import sys
import unicodedata
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

STATUS_CARD = "Components by status"
UNREACHED_CARD = "Claims no component reaches"
OUTSIDE_CARD = "Cited paths outside every design root"
# archify's component box when a component gives no `size`: `layout.defaultW` (renderers/architecture/render-architecture.mjs
# at 7e45f36). The grid's `cellW` is the step between boxes, never a box's width.
DEFAULT_WIDTH = 120
# archify draws a tag as one unwrapped line, shrunk to a 6 px floor at 0.6 px per column per px of font, inside the box
# less 8 px of padding (renderers/shared/text-fit.mjs at 7e45f36): (width - 8) / 3.6 columns. Integer arithmetic.
SEP = " | "


class Refusal(Exception):
    """The input is not a document this converter was written for."""


def refuse(message):
    raise Refusal(message)


def units(text):
    """Columns `text` takes: one per character, two for a wide one (a bound that errs high, never low)."""
    return sum(2 if unicodedata.east_asian_width(char) in ("W", "F") else 1 for char in text)


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
        while line.startswith("./"):
            line = line[2:]
        line = line.rstrip("/")
        if line:
            roots.append(line)
    return roots


def inside(path, roots):
    return any(path == base or path.startswith(base + "/") for base in roots)


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


def tag_budget(width):
    return int(((width - 8) * 10) // 36)


def fit_tag(existing, word, claims, width):
    """The author's tag (kept), then the status word and as many claim ids as fit the box; the card holds them all.

    Returns the text and whether it fits the box. Even the author's tag plus the word may not: it is written all the same
    (the author's text is kept), and the caller says the page would not pass archify's `component-tag-overflow`.
    """
    budget = tag_budget(width)
    prefix = existing + SEP if existing else ""
    if not claims:
        text = prefix + word
        return text, units(text) <= budget
    for shown in range(len(claims), -1, -1):
        if shown == len(claims):
            ids = ", ".join(claims)
        elif shown:
            ids = ", ".join(claims[:shown]) + f", +{len(claims) - shown}"
        else:
            ids = f"{len(claims)} claims"
        text = f"{prefix}{word} · {ids}"
        if units(text) <= budget:
            return text, True
    text = f"{prefix}{word}"
    return text, units(text) <= budget


def box_width(component):
    """archify's box: `size[0]`, else `layout.defaultW`."""
    size = component.get("size")
    if isinstance(size, list) and size and isinstance(size[0], (int, float)) and not isinstance(size[0], bool):
        return size[0]
    return DEFAULT_WIDTH


def overlay(diagram, satisfy, trace, root):
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
    for node in trace["nodes"]:
        if node["claim_id"] not in seen and applying(node):
            seen.add(node["claim_id"])
            rows.append((node["claim_id"], clean(node["statement"])))
    rows.sort()
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
        component["tag"], fits = fit_tag(component.get("tag", ""), WORD[state["status"]], state["claims"], width)
        if not fits:
            overflow.append(
                f"tag overflow: component {component['id']!r} ({component['label']}): {component['tag']!r} needs "
                f"{units(component['tag'])} columns and a {width} px box draws {tag_budget(width)}; the author's tag is kept, "
                "so archify validate would refuse the overlay (component-tag-overflow): widen `size` or shorten the tag"
            )

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

    labels = [cell_label(state["component"]["label"]) for state in states]
    table = ["| " + " | ".join(["claim"] + labels) + " |", "| " + " | ".join(["---"] * (len(labels) + 1)) + " |"]
    for claim, _statement in rows:
        cells = []
        for state in states:
            outcome = state["outcomes"].get(claim)
            cells.append(outcome or "")
        table.append("| " + " | ".join([claim] + cells) + " |")
    matrix = "\n".join(table) + "\n\n## Claims no component reaches\n\n"
    if unreached:
        matrix += "".join(f"- {claim}: {statement}\n" for claim, statement in unreached)
    else:
        matrix += "none\n"

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


def parse(argv):
    args = argv[1:]
    root = Path(".")
    if args[:1] == ["--root"]:
        if len(args) < 2:
            return None
        root, args = Path(args[1]), args[2:]
    if len(args) != 4 or any(arg.startswith("--") for arg in args):
        return None
    return root, args[0], args[1], args[2], Path(args[3])


def main(argv):
    parsed = parse(argv)
    if parsed is None:
        print(
            "usage: arch_overlay.py [--root <dir>] <diagram.json> <satisfy.json> <trace.json> <out-dir>",
            file=sys.stderr,
        )
        return 2
    root, diagram_path, satisfy_path, trace_path, target = parsed
    try:
        diagram = load(diagram_path, "the diagram")
        satisfy = load(satisfy_path, "the satisfy document")
        trace = load(trace_path, "the trace document")
        check_diagram(diagram, diagram_path)
        check_satisfy(satisfy, satisfy_path)
        check_trace(trace, trace_path)
        if target.exists() and (not target.is_dir() or any(target.iterdir())):
            refuse(f"{target} exists and is not an empty directory: a stale overlay must not mix with a new one")
        result, matrix, reasons = overlay(diagram, satisfy, trace, root)
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
