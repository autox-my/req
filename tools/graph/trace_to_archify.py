#!/usr/bin/env python3
"""Turn one `req.trace/1` document into one archify `architecture` diagram file (ADR 0046, unit V-1).

usage: python3 -I tools/graph/trace_to_archify.py <trace.json> <out.json>     (`-` reads the document from stdin)

The trace document is the graph contract. Every node and every edge of the output comes from it and from nothing
else: a node per entry of `nodes` (plus, for an edge into a claim the corpus does not hold, one node for that
named target), an edge per entry of `edges`, labelled with its `relation`. Placement is computed from the graph,
never chosen by a model: layer = the longest `derives-from`/`refines` path to a root, column order by claim id,
integers only. No clock, no randomness, sorted keys: one document gives one file.

It refuses a schema other than `req.trace/1` (exit 2, naming it) and malformed JSON (exit 2), and writes nothing on a
refusal. It uses only the standard library and never touches the network or the corpus.

Written against archify `tt-a1i/archify` at commit 7e45f36 (Apache-2.0, `architecture` type); the pin is tooling
provenance, recorded here and in `docs/adr/0046-the-trace-document-is-the-graph-contract.md`.
"""

import json
import re
import sys

# The archify commit this converter was written against (ADR 0046 decision 7).
ARCHIFY_COMMIT = "7e45f36"
SCHEMA = "req.trace/1"
# The relations whose direction says "this claim rests on that one": they decide a node's layer.
LAYERING = ("derives-from", "refines")
# archify's own id grammar (`common.schema.json#/$defs/id`): an id outside it is refused, never rewritten.
ARCHIFY_ID = re.compile(r"^[a-zA-Z][a-zA-Z0-9_-]*$")

# Placement, in integers: a node box, the pitch between boxes, the margin, and the width of a row.
NODE_W, NODE_H = 130, 64
PITCH_X, PITCH_Y = 190, 170
MARGIN = 40
PER_ROW = 6
# Parallel routes through one channel or gutter fan out over LANES offsets, LANE_STEP apart.
LANES, LANE_STEP = 5, 9

# One archify component type per state a claim can be in. The legend names each state in req's words.
STATE_TYPE = {
    "applies": "backend",
    "unknown": "messagebus",
    "does_not_apply": "external",
    "withdrawn": "security",
    "missing": "cloud",
}
STATE_LEGEND = {
    "applies": "applies in this selection",
    "unknown": "undecided in this selection",
    "does_not_apply": "does not apply in this selection",
    "withdrawn": "withdrawn: superseded where its successor applies",
    "missing": "named by an edge, not in the corpus",
}
STATE_TAG = {
    "unknown": "undecided",
    "does_not_apply": "does not apply",
    "withdrawn": "withdrawn",
}
STATE_DOT = {
    "applies": "cyan",
    "unknown": "amber",
    "does_not_apply": "slate",
    "withdrawn": "rose",
    "missing": "orange",
}


class Refusal(Exception):
    """The input is not a document this converter was written for."""


def refuse(message):
    raise Refusal(message)


def check_document(document):
    """Fail, naming what is wrong, unless `document` is a well-formed `req.trace/1`."""
    if not isinstance(document, dict):
        refuse("the document is not a JSON object")
    schema = document.get("schema")
    if schema != SCHEMA:
        refuse(f"unknown schema {schema!r}: this converter reads only {SCHEMA}")
    for key in ("selection", "nodes", "edges", "findings"):
        if key not in document:
            refuse(f"the {SCHEMA} document has no `{key}`")
    if not isinstance(document["selection"], dict):
        refuse("`selection` is not an object")
    for key in ("nodes", "edges"):
        if not isinstance(document[key], list):
            refuse(f"`{key}` is not an array")
    if not document["nodes"]:
        refuse("`nodes` is empty: there is no claim to draw, and archify refuses an empty diagram")
    for node in document["nodes"]:
        if not isinstance(node, dict):
            refuse("a node is not an object")
        for key in ("claim_id", "statement", "modality", "applies"):
            if not isinstance(node.get(key), str):
                refuse(f"a node has no string `{key}`: {node!r}")
        successors = node.get("superseded_by")
        if not isinstance(successors, list) or not all(isinstance(item, str) for item in successors):
            refuse(f"a node has no `superseded_by` array of claim ids: {node!r}")
    for edge in document["edges"]:
        if not isinstance(edge, dict):
            refuse("an edge is not an object")
        for key in ("from", "relation", "to"):
            if not isinstance(edge.get(key), str):
                refuse(f"an edge has no string `{key}`: {edge!r}")
    findings = document["findings"]
    if not isinstance(findings, dict):
        refuse("`findings` is not an object")
    for key in ("dangling", "undecided_edges"):
        if not isinstance(findings.get(key), list):
            refuse(f"`findings.{key}` is not an array")


def state_of(node):
    """A claim's drawn state: withdrawn (ADR 0049) is a kind of does-not-apply, drawn on its own."""
    if node["superseded_by"]:
        return "withdrawn"
    if node["applies"] in ("applies", "unknown", "does_not_apply"):
        return node["applies"]
    refuse(f"unknown `applies` {node['applies']!r} on {node['claim_id']}")


def layers_of(ids, edges):
    """Layer = the longest layering-relation path to a root; a cycle's closing edge is not followed."""
    known = set(ids)
    targets = {i: [] for i in ids}
    for edge in edges:
        if edge["relation"] in LAYERING and edge["from"] in known and edge["to"] in known:
            if edge["from"] != edge["to"]:
                targets[edge["from"]].append(edge["to"])
    for i in ids:
        targets[i] = sorted(set(targets[i]))

    layer = {}
    state = {}  # absent = unseen, 1 = on the path, 2 = done
    for root in sorted(ids):
        if root in state:
            continue
        # Iterative post-order walk: `(id, next index into its sorted targets)`.
        stack = [(root, 0)]
        state[root] = 1
        while stack:
            current, index = stack.pop()
            if index < len(targets[current]):
                stack.append((current, index + 1))
                following = targets[current][index]
                if following not in state:
                    state[following] = 1
                    stack.append((following, 0))
                continue
            depth = 0
            for following in targets[current]:
                if state.get(following) == 2:  # a back edge (state 1) closes a cycle: skipped
                    depth = max(depth, layer[following] + 1)
            layer[current] = depth
            state[current] = 2
    return layer


def tag_of(node, state):
    """The modality, then the state when it is not the ordinary one: colour alone never says it."""
    if state == "applies":
        return node["modality"]
    return f"{node['modality']} \u00b7 {STATE_TAG[state]}"


def lane(lanes, coordinate):
    """The next free offset beside `coordinate`: routes sharing a channel or a gutter do not lie on one line."""
    used = lanes.get(coordinate, 0)
    lanes[coordinate] = used + 1
    return coordinate + (used % LANES) * LANE_STEP - (LANES // 2) * LANE_STEP


def route_of(source, target, lanes):
    """Fields that route one edge through the free channels between rows and the gutters between columns.

    Nodes sit on a grid, so a straight line can cross an unrelated node, and archify's automatic route puts the label
    of a neighbour's edge on a node; a route that runs only in channels (the horizontal gaps between rows) and gutters
    (the vertical gaps between columns), with its label on the channel segment, cannot. **Every** edge gets one: no
    edge is left to archify's automatic route or label placement.
    """
    (src_row, src_col), (dst_row, dst_col) = source, target
    row_gap = abs(src_row - dst_row)
    src_x = MARGIN + src_col * PITCH_X + NODE_W // 2
    dst_x = MARGIN + dst_col * PITCH_X + NODE_W // 2

    def above(row):
        return MARGIN + row * PITCH_Y - (PITCH_Y - NODE_H) // 2

    def below(row):
        return MARGIN + row * PITCH_Y + NODE_H + (PITCH_Y - NODE_H) // 2

    if row_gap == 0:
        channel = lane(lanes, above(src_row))
        return {"fromSide": "top", "toSide": "top", "via": [[src_x, channel], [dst_x, channel]], "labelSegment": 1}
    if dst_row > src_row:
        out_y, in_y, from_side, to_side = below(src_row), above(dst_row), "bottom", "top"
    else:
        out_y, in_y, from_side, to_side = above(src_row), below(dst_row), "top", "bottom"
    if out_y == in_y:
        channel = lane(lanes, out_y)
        via = [[src_x, channel], [dst_x, channel]]
    else:
        out_y, in_y = lane(lanes, out_y), lane(lanes, in_y)
        gutter = MARGIN + max(src_col, dst_col) * PITCH_X + NODE_W + (PITCH_X - NODE_W) // 2
        gutter = lane(lanes, gutter)
        via = [[src_x, out_y], [gutter, out_y], [gutter, in_y], [dst_x, in_y]]
    return {"fromSide": from_side, "toSide": to_side, "via": via, "labelSegment": 1}


def convert(document):
    """The archify `architecture` diagram for a checked document. Pure: same document, same value."""
    nodes = sorted(document["nodes"], key=lambda n: n["claim_id"])
    ids = [n["claim_id"] for n in nodes]
    if len(set(ids)) != len(ids):
        refuse("two nodes share one claim_id")
    edges = document["edges"]
    known = set(ids)
    missing = sorted({e["to"] for e in edges if e["to"] not in known} | {e["from"] for e in edges if e["from"] not in known})

    for identifier in ids + missing:
        if not ARCHIFY_ID.match(identifier):
            refuse(f"the id {identifier!r} is outside archify's id grammar; nothing was rewritten")

    layer = layers_of(ids, edges)
    rows = {}  # layer -> ids, in claim id order
    for identifier in ids:
        rows.setdefault(layer[identifier], []).append(identifier)

    cell = {}  # id -> (visual row, column)
    row_index = 0
    for depth in sorted(rows):
        members = rows[depth]
        for offset, identifier in enumerate(members):
            cell[identifier] = (row_index + offset // PER_ROW, offset % PER_ROW)
        row_index += (len(members) + PER_ROW - 1) // PER_ROW
    for offset, identifier in enumerate(missing):
        cell[identifier] = (row_index + offset // PER_ROW, offset % PER_ROW)
    position = {i: (MARGIN + c * PITCH_X, MARGIN + r * PITCH_Y) for i, (r, c) in cell.items()}

    components = []
    cards = []
    for node in nodes:
        state = state_of(node)
        x, y = position[node["claim_id"]]
        components.append(
            {
                "id": node["claim_id"],
                "type": STATE_TYPE[state],
                "label": node["claim_id"],
                "tag": tag_of(node, state),
                "pos": [x, y],
                "size": [NODE_W, NODE_H],
            }
        )
        items = [node["statement"], f"{node['modality']} · {STATE_LEGEND[state]}"]
        if node["superseded_by"]:
            items.append("superseded by " + ", ".join(node["superseded_by"]))
        cards.append({"dot": STATE_DOT[state], "title": node["claim_id"], "items": items})
    for identifier in missing:
        x, y = position[identifier]
        components.append(
            {
                "id": identifier,
                "type": STATE_TYPE["missing"],
                "label": identifier,
                "tag": "not in the corpus",
                "pos": [x, y],
                "size": [NODE_W, NODE_H],
            }
        )

    dangling = set(document["findings"]["dangling"])
    undecided = set(document["findings"]["undecided_edges"])
    lanes = {}  # a channel or gutter coordinate -> how many routes already run along it
    connections = []
    for index, edge in enumerate(edges):
        connection = {
            "id": f"edge-{index + 1:04d}",
            "from": edge["from"],
            "to": edge["to"],
            "label": edge["relation"],
        }
        connection.update(route_of(cell[edge["from"]], cell[edge["to"]], lanes))
        if f"{edge['relation']} -> {edge['to']} (referenced by {edge['from']}, not in the corpus)" in dangling or edge["to"] not in known:
            connection["variant"] = "security"
        elif f"{edge['from']} -{edge['relation']}-> {edge['to']} (target undecided)" in undecided:
            connection["variant"] = "dashed"
        connections.append(connection)

    selection = " ".join(f"{k}={v}" for k, v in sorted(document["selection"].items())) or "no context"
    used = sorted({STATE_TYPE[state_of(n)] for n in nodes} | ({STATE_TYPE["missing"]} if missing else set()))
    legend = {}
    for state, kind in STATE_TYPE.items():
        if kind in used:
            legend[kind] = {"label": STATE_LEGEND[state]}

    return {
        "schema_version": 1,
        "diagram_type": "architecture",
        "meta": {
            "title": f"req trace: {selection}",
            "subtitle": f"{SCHEMA}, rules_v {document.get('rules_v', '?')}; converter written for archify {ARCHIFY_COMMIT}",
            "output": "req-trace-graph.html",
            "quality_profile": "standard",
            "legend": {"mode": "auto", "entries": legend},
        },
        "components": components,
        "connections": connections,
        "cards": cards,
    }


def render(value):
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def main(argv):
    if len(argv) != 3:
        print("usage: trace_to_archify.py <trace.json> <out.json>   (`-` reads stdin)", file=sys.stderr)
        return 2
    source, target = argv[1], argv[2]
    try:
        if source == "-":
            text = sys.stdin.read()
        else:
            with open(source, encoding="utf-8") as handle:
                text = handle.read()
    except (OSError, UnicodeDecodeError) as error:
        print(f"cannot read {source}: {error}", file=sys.stderr)
        return 2
    try:
        document = json.loads(text)
    except ValueError as error:
        print(f"malformed JSON in {source}: {error}", file=sys.stderr)
        return 2
    try:
        check_document(document)
        output = render(convert(document))
    except Refusal as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    try:
        with open(target, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(output)
    except OSError as error:
        print(f"cannot write {target}: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
