#!/usr/bin/env python3
"""Turn one `req.trace/1` document into a directory of archify `architecture` diagram files (ADR 0046, 0050).

usage: python3 -I tools/graph/trace_to_archify.py <trace.json> <out-dir>     (`-` reads the document from stdin)

The trace document is the graph contract. Every node and every edge of the output comes from it and from nothing
else: a node per entry of `nodes`, an edge per entry of `edges`, labelled with its `relation`, and, for an edge that
leaves a page, a stub node named for the claim at its far end. Placement is computed from the graph, never chosen by a
model, in integers only; there is no clock and no randomness, keys are sorted, and one document gives one directory,
byte for byte.

A page shows a declared slice (ADR 0050). Claims are grouped into families; a document of at most PAGE_LIMIT claims is
one page, `all.json`, and a larger one is an index page plus one page per group (a group past the limit is cut into
parts by layer). The directory holds `index.json` only when there are several pages, and one `<page>.json` per page.
It refuses an existing non-empty directory, a schema other than `req.trace/1` and malformed JSON (exit 2, naming what is
wrong), and writes nothing on a refusal. It uses only the standard library and never touches the network or the corpus.

Written against archify `tt-a1i/archify` at commit 7e45f36 (Apache-2.0, `architecture` type); the pin is tooling
provenance, recorded here and in `docs/adr/0046-the-trace-document-is-the-graph-contract.md`.
"""

import json
import re
import sys
from math import isqrt
from os import environ
from pathlib import Path

# The archify commit this converter was written against (ADR 0046 decision 7).
ARCHIFY_COMMIT = "7e45f36"
SCHEMA = "req.trace/1"
# The relations whose direction says "this claim rests on that one": they decide a node's layer and its family.
LAYERING = ("derives-from", "refines")
# archify's own id grammar (`common.schema.json#/$defs/id`): an id outside it is refused, never rewritten.
ARCHIFY_ID = re.compile(r"^[a-zA-Z][a-zA-Z0-9_-]*$")

# A page holds at most this many claims (ADR 0050 decision 3). A test sets REQ_GRAPH_PAGE_LIMIT to a smaller number;
# the variable is read only when it is set.
PAGE_LIMIT = 150
LIMIT_ENV = "REQ_GRAPH_PAGE_LIMIT"
# Median ordering runs this many passes over the layers, alternately down and up, then the coordinate pass runs
# ALIGN_PASSES times (ADR 0050 decision 5). Fixed counts, so one document always gives one layout.
SWEEPS = 8
ALIGN_PASSES = 4

# Placement, in integers. A node box and the pitch between slots: a row is PITCH_Y apart, and a slot (a node, or the
# waypoint a long edge reserves in a layer) PITCH_X apart. Family blocks sit BLOCK_GAP apart so their frames (30 px of
# padding each, archify's) leave room for the gutter between them.
NODE_W, NODE_H = 130, 64
PITCH_X, PITCH_Y = 220, 190
# The left margin holds the gutter beside the first block (BLOCK_GAP / 2 out, 18 px of lanes) and the frame's 30 px of
# padding with room for its title to widen it (archify widens a frame, towards both sides, until its title fits: for the
# longest id archify draws in a node box, about 19 characters, that is 13 px); the top margin holds the channel above the
# first row (63 px above it, and 18 px of lanes), then the frame's 30 px of padding and its title rail.
MARGIN_X, MARGIN_Y = 130, 110
BLOCK_GAP = 200
PER_ROW = 6
# archify's browser-check fits the drawing to the reader's window and refuses text it would project below 6 px. Measured
# against archify 7e45f36 at its default 1440x900 window: a canvas 1680 px wide (a row of 7 boxes here) passes and one
# 1900 px wide (a row of 8) fails on edge labels. A page is therefore laid out inside MAX_WIDTH, the width of the canvas
# with both margins, which leaves archify's own margin and the route lanes room under the failing width; a layer wider
# than WRAP slots continues on bands below, so one band is (WRAP - 1) * PITCH_X + NODE_W wide.
WRAP = 7
MAX_WIDTH = 1710
BAND_CAP = (WRAP - 1) * PITCH_X
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
    "missing": "not on this page: see its tag",
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
TAG_STUB = "on another page"
TAG_DANGLING = "not in the corpus"

# The panel's and legend's own words, in req's (ADR 0050 decision 6). Keys are archify's `locales/en.json` keys.
# archify's `upstream` follows an edge backwards, to the claims that point at the focused one: those are the claims
# that rest on it (an edge runs from the claim that derives or refines, to the claim it rests on).
CLAIM_WORDS = {
    "node.context.architecture": "Claim",
    "viewer.passport.eyebrow": "Claim",
    "viewer.passport.reach": "Dependencies",
    "viewer.passport.upstream": "Rests on this",
    "viewer.passport.downstream": "This rests on",
    "viewer.passport.upstream.trace": "Trace the claims that rest on this one",
    "viewer.passport.downstream.trace": "Trace the claims this one rests on",
    "viewer.passport.reach.upstream.one": "Trace {count} claim that rests on this one",
    "viewer.passport.reach.upstream.other": "Trace {count} claims that rest on this one",
    "viewer.passport.reach.downstream.one": "Trace {count} claim this one rests on",
    "viewer.passport.reach.downstream.other": "Trace {count} claims this one rests on",
    "viewer.passport.reach.noUpstream": "No claim rests on this one",
    "viewer.passport.reach.noDownstream": "This claim rests on no other claim",
    "viewer.passport.close": "Close claim panel",
    "viewer.passport.move": "Move claim panel. Drag, use arrow keys, or press Home to reset.",
}
GROUP_WORDS = {
    "node.context.architecture": "Group",
    "viewer.passport.eyebrow": "Group",
    "viewer.kind.backend": "Group",
    "legend.architecture.backend": "Group",
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


def page_limit():
    """PAGE_LIMIT, or the test-only override when the variable is set (a positive integer, else a refusal)."""
    text = environ.get(LIMIT_ENV)
    if text is None:
        return PAGE_LIMIT
    if not re.fullmatch(r"[0-9]+", text) or int(text) < 1:
        refuse(f"{LIMIT_ENV}={text!r} is not a positive integer")
    return int(text)


def state_of(node):
    """A claim's drawn state: withdrawn (ADR 0049) is a kind of does-not-apply, drawn on its own."""
    if node["superseded_by"]:
        return "withdrawn"
    if node["applies"] in ("applies", "unknown", "does_not_apply"):
        return node["applies"]
    refuse(f"unknown `applies` {node['applies']!r} on {node['claim_id']}")


def layering(ids, edges):
    """Layer = the longest layering-relation path to a root, and each claim's followed targets.

    A cycle's closing edge is not followed (ADR 0046): a claim whose only layering edges close a cycle has no followed
    target, so it is a root, at layer 0. Returns `(layer, followed)`.
    """
    known = set(ids)
    targets = {i: [] for i in ids}
    for edge in edges:
        if edge["relation"] in LAYERING and edge["from"] in known and edge["to"] in known:
            if edge["from"] != edge["to"]:
                targets[edge["from"]].append(edge["to"])
    for i in ids:
        targets[i] = sorted(set(targets[i]))

    layer = {}
    followed = {}
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
            chosen = [f for f in targets[current] if state.get(f) == 2]  # a back edge (state 1) is skipped
            layer[current] = max((layer[f] + 1 for f in chosen), default=0)
            followed[current] = chosen
            state[current] = 2
    return layer, followed


def families_of(ids, edges):
    """Each claim's family root, and the set of standalone claims (ADR 0050 decision 2).

    The family is the lowest-id root the claim reaches over `derives-from` and `refines`. A root is a claim with no
    such edge out; a cycle with no edge out of it is one root, named by its lowest-id member (the layout may call another
    member the root of the cycle, which is a drawing choice and does not name a family).
    """
    known = set(ids)
    succ = {i: set() for i in ids}
    for edge in edges:
        if edge["relation"] in LAYERING and edge["from"] in known and edge["to"] in known and edge["from"] != edge["to"]:
            succ[edge["from"]].add(edge["to"])

    # Tarjan's strongly connected components, iterative, in sorted order; components come out sinks first.
    index, low, on_stack, stack, components = {}, {}, set(), [], []
    for start in sorted(ids):
        if start in index:
            continue
        index[start] = low[start] = len(index)
        stack.append(start)
        on_stack.add(start)
        work = [(start, iter(sorted(succ[start])))]
        while work:
            node, following = work[-1]
            descended = False
            for target in following:
                if target not in index:
                    index[target] = low[target] = len(index)
                    stack.append(target)
                    on_stack.add(target)
                    work.append((target, iter(sorted(succ[target]))))
                    descended = True
                    break
                if target in on_stack:
                    low[node] = min(low[node], index[target])
            if descended:
                continue
            work.pop()
            if work:
                low[work[-1][0]] = min(low[work[-1][0]], low[node])
            if low[node] == index[node]:
                members = []
                while True:
                    member = stack.pop()
                    on_stack.discard(member)
                    members.append(member)
                    if member == node:
                        break
                components.append(sorted(members))

    component_of = {i: n for n, members in enumerate(components) for i in members}
    named = {}
    family = {}
    for n, members in enumerate(components):
        leaving = {component_of[t] for i in members for t in succ[i] if component_of[t] != n}
        named[n] = min(named[o] for o in leaving) if leaving else members[0]
        for i in members:
            family[i] = named[n]

    touched = set()
    for edge in edges:
        touched.add(edge["from"])
        touched.add(edge["to"])
    return family, {i for i in ids if i not in touched}


# ---------------------------------------------------------------------------------------------------------------
# Layout: median ordering, waypoints, coordinates
# ---------------------------------------------------------------------------------------------------------------


def median2(positions):
    """Twice the median of a sorted list (integers only: an even count averages its two middles)."""
    count = len(positions)
    if count % 2:
        return 2 * positions[count // 2]
    return positions[count // 2 - 1] + positions[count // 2]


def place(desired):
    """Integer x positions as close to `desired` as possible, in order, at least PITCH_X apart.

    The least-squares fit under the spacing constraint: shift each slot by its index, pool neighbours that violate the
    order into their mean (floor division), and shift back.
    """
    blocks = []  # [sum, count] of the shifted values
    for index, value in enumerate(desired):
        blocks.append([value - index * PITCH_X, 1])
        while len(blocks) > 1 and blocks[-2][0] * blocks[-1][1] > blocks[-1][0] * blocks[-2][1]:
            total, count = blocks.pop()
            blocks[-1][0] += total
            blocks[-1][1] += count
    result = []
    for total, count in blocks:
        mean = total // count
        for _ in range(count):
            result.append(mean + len(result) * PITCH_X)
    return result


def clipped(fitted):
    """`fitted` kept inside [0, BAND_CAP], in order and still PITCH_X apart (a band holds at most WRAP slots, so it fits)."""
    xs = list(fitted)
    for i in range(len(xs)):
        xs[i] = max(xs[i], xs[i - 1] + PITCH_X if i else 0)
    for i in range(len(xs) - 1, -1, -1):
        xs[i] = min(xs[i], xs[i + 1] - PITCH_X if i < len(xs) - 1 else BAND_CAP)
    return xs


def layered(ids, layer_of, edge_pairs):
    """Order and place one block of nodes in layers (ADR 0050 decision 5).

    `ids` are the block's nodes, `layer_of` gives each a layer, `edge_pairs` are `(index, from, to)` with both ends in
    the block. A layer is ordered by repeated median sweeps; a layer wider than WRAP slots then continues on further
    bands below, in the same order. Returns `(layers, x, row_of, chains, width, row_count)`: the layers' slots in order
    (a slot is a node id, or the reserved waypoint of a long edge in one layer it crosses), each slot's x (the left edge of
    its box) from 0, its visual row (a band), each edge's slots in travel order, the block's width and its visual rows.
    """
    distinct = sorted({layer_of[i] for i in ids})
    rank = {value: index for index, value in enumerate(distinct)}
    layer_rank = {i: rank[layer_of[i]] for i in ids}
    sortkey = {i: i for i in ids}
    rows = [[] for _ in distinct]
    for i in sorted(ids):
        rows[layer_rank[i]].append(i)
    up = {i: [] for i in ids}
    down = {i: [] for i in ids}
    chains = {}
    for index, source, target in edge_pairs:
        first, last = (source, target) if layer_rank[source] <= layer_rank[target] else (target, source)
        chain = [first]
        for r in range(layer_rank[first] + 1, layer_rank[last]):
            waypoint = f"~{index:06d}-{r}"
            layer_rank[waypoint] = r
            sortkey[waypoint] = f"{first}|{last}|{index:06d}"
            up[waypoint] = []
            down[waypoint] = []
            rows[r].append(waypoint)
            chain.append(waypoint)
        chain.append(last)
        if layer_rank[first] != layer_rank[last]:
            for above, below in zip(chain, chain[1:]):
                down[above].append(below)
                up[below].append(above)
        chains[index] = chain if first == source else chain[::-1]
    for row in rows:
        row.sort(key=lambda s: sortkey[s])

    position = {s: i for row in rows for i, s in enumerate(row)}
    for sweep in range(SWEEPS):
        if sweep % 2 == 0:
            order, neighbours = range(1, len(rows)), up
        else:
            order, neighbours = range(len(rows) - 2, -1, -1), down

        for r in order:

            def key(slot):
                spots = sorted(position[n] for n in neighbours[slot])
                return (median2(spots) if spots else 2 * position[slot], sortkey[slot])

            rows[r].sort(key=key)
            for i, slot in enumerate(rows[r]):
                position[slot] = i

    # A layer wider than WRAP continues on bands directly below it, in the same order.
    bands = []
    for row in rows:
        bands.extend(row[k : k + WRAP] for k in range(0, len(row), WRAP))
    row_of = {s: v for v, band in enumerate(bands) for s in band}

    x = {s: i * PITCH_X for band in bands for i, s in enumerate(band)}
    for sweep in range(ALIGN_PASSES):
        for v in range(len(bands)) if sweep % 2 == 0 else range(len(bands) - 1, -1, -1):
            band = bands[v]
            desired = []
            for slot in band:
                around = up[slot] + down[slot]
                desired.append(sum(x[n] for n in around) // len(around) if around else x[slot])
            for slot, value in zip(band, clipped(place(desired))):
                x[slot] = value
    low = min(x.values())
    x = {s: v - low for s, v in x.items()}
    width = max(x.values()) + NODE_W
    return rows, x, row_of, chains, width, len(bands)


def standalone_grid(ids):
    """Claims with no edge: a grid in claim id order, as square as it can be and at most PER_ROW wide."""
    columns = min(PER_ROW, isqrt(len(ids) - 1) + 1)
    cells = {i: (n // columns, n % columns) for n, i in enumerate(sorted(ids))}
    x = {i: c * PITCH_X for i, (r, c) in cells.items()}
    row_of = {i: r for i, (r, c) in cells.items()}
    return x, row_of, (columns - 1) * PITCH_X + NODE_W, max(row_of.values()) + 1


def channel_y(row):
    """The middle of the free channel below `row` (and above `row + 1`); row -1 is the channel above the first row."""
    return MARGIN_Y + row * PITCH_Y + NODE_H + (PITCH_Y - NODE_H) // 2


def lane(lanes, coordinate, axis="y"):
    """The next free offset beside `coordinate`: routes sharing a channel (`y`) or a gutter (`x`) do not lie on one line."""
    used = lanes.get((axis, coordinate), 0)
    lanes[(axis, coordinate)] = used + 1
    return coordinate + (used % LANES) * LANE_STEP - (LANES // 2) * LANE_STEP


def travel_route(slots, row_of, centre, lanes, corridor):
    """Fields routing one edge slot to slot, through the channel between each pair of rows and the waypoints.

    The route runs only in channels (horizontal) and in the slots reserved for it (vertical), so it crosses no node. A hop
    that crosses more than one row (a layer's bands) turns into a corridor chosen among the gaps free in every row it
    crosses. Its label sits on routed segment 1, the first run along a channel: no edge is left to archify's automatic
    route.
    """
    first, last = slots[0], slots[-1]
    if row_of[first] == row_of[last]:
        channel = lane(lanes, channel_y(row_of[first] - 1))
        via = [[centre(first), channel], [centre(last), channel]]
        return {"fromSide": "top", "toSide": "top", "via": via, "labelSegment": 1}
    going_down = row_of[last] > row_of[first]
    via = []
    for k, (a, b) in enumerate(zip(slots, slots[1:])):
        (ra, ca), (rb, cb) = (row_of[a], centre(a)), (row_of[b], centre(b))
        if abs(ra - rb) <= 1:
            channel = lane(lanes, channel_y(min(ra, rb)))
            points = [[ca, channel], [cb, channel]]
        else:
            down = rb > ra
            out_y = lane(lanes, channel_y(ra if down else ra - 1))
            in_y = lane(lanes, channel_y(rb - 1 if down else rb))
            gap = lane(lanes, corridor(min(ra, rb) + 1, max(ra, rb) - 1, ca, cb), "x")
            points = [[ca, out_y], [gap, out_y], [gap, in_y], [cb, in_y]]
        if k and points[0] == points[1]:
            points = points[:1]
        if k < len(slots) - 2:  # b is the reserved waypoint of a layer the edge crosses: the route passes through it
            points.append([cb, MARGIN_Y + rb * PITCH_Y + NODE_H // 2])
        via.extend(points)
    return {
        "fromSide": "bottom" if going_down else "top",
        "toSide": "top" if going_down else "bottom",
        "via": via,
        "labelSegment": 1,
    }


def corridor_in(boxes_by_row, left, right):
    """A function choosing the x of a vertical run across rows `lo..hi`: a gap free of every box in those rows.

    `boxes_by_row` maps a row to the left edges of its slots; `left` and `right` are the left edges of the block's
    first and last slot, outside which (by 60 px) a run is always free. The gap nearest the hop's two ends wins, ties to the
    smaller x, so the choice is a pure function.
    """
    half_gap = (PITCH_X - NODE_W) // 2
    clearance = 30

    def corridor(lo, hi, from_x, to_x):
        boxes = [bx for r in range(lo, hi + 1) for bx in boxes_by_row.get(r, [])]
        candidates = {left - 60, right + NODE_W + 60}
        for bx in boxes:
            candidates.add(bx - half_gap)
            candidates.add(bx + NODE_W + half_gap)
        free = [
            c
            for c in candidates
            if all(not (bx - clearance < c < bx + NODE_W + clearance) for bx in boxes)
        ]
        return min(free, key=lambda c: (abs(c - from_x) + abs(c - to_x), c))

    return corridor


def cross_route(source, target, gutter_x, lanes):
    """Fields routing an edge between two blocks: along the channel beside the source, down the gutter left of the
    target's block (free of nodes and frames), and along the channel beside the target. `source` and `target` are
    `(row, centre x)`."""
    (src_row, src_x), (dst_row, dst_x) = source, target
    if src_row == dst_row:
        channel = lane(lanes, channel_y(src_row - 1))
        return {
            "fromSide": "top",
            "toSide": "top",
            "via": [[src_x, channel], [dst_x, channel]],
            "labelSegment": 1,
        }
    going_down = dst_row > src_row
    out_row = src_row if going_down else src_row - 1
    in_row = dst_row - 1 if going_down else dst_row
    if out_row == in_row:
        channel = lane(lanes, channel_y(out_row))
        via = [[src_x, channel], [dst_x, channel]]
    else:
        out_y, in_y = lane(lanes, channel_y(out_row)), lane(lanes, channel_y(in_row))
        gutter = lane(lanes, gutter_x, "x")
        via = [[src_x, out_y], [gutter, out_y], [gutter, in_y], [dst_x, in_y]]
    return {
        "fromSide": "bottom" if going_down else "top",
        "toSide": "top" if going_down else "bottom",
        "via": via,
        "labelSegment": 1,
    }


def grid_route(source, target, lanes):
    """The V-1 route between two cells `(row, column)` of a plain grid (the index page): channels and gutters only."""
    (src_row, src_col), (dst_row, dst_col) = source, target
    src_x = MARGIN_X + src_col * PITCH_X + NODE_W // 2
    dst_x = MARGIN_X + dst_col * PITCH_X + NODE_W // 2
    if src_row == dst_row:
        channel = lane(lanes, channel_y(src_row - 1))
        return {"fromSide": "top", "toSide": "top", "via": [[src_x, channel], [dst_x, channel]], "labelSegment": 1}
    going_down = dst_row > src_row
    out_row = src_row if going_down else src_row - 1
    in_row = dst_row - 1 if going_down else dst_row
    if out_row == in_row:
        channel = lane(lanes, channel_y(out_row))
        via = [[src_x, channel], [dst_x, channel]]
    else:
        out_y, in_y = lane(lanes, channel_y(out_row)), lane(lanes, channel_y(in_row))
        gutter = lane(lanes, MARGIN_X + max(src_col, dst_col) * PITCH_X + NODE_W + (PITCH_X - NODE_W) // 2, "x")
        via = [[src_x, out_y], [gutter, out_y], [gutter, in_y], [dst_x, in_y]]
    return {
        "fromSide": "bottom" if going_down else "top",
        "toSide": "top" if going_down else "bottom",
        "via": via,
        "labelSegment": 1,
    }


# ---------------------------------------------------------------------------------------------------------------
# Groups, parts and pages
# ---------------------------------------------------------------------------------------------------------------


def cut_into_parts(group, layer, edges, limit):
    """Consecutive parts of at most `limit` claims, cut between layers (ADR 0050 decision 3).

    Layers fill a part whole until the next would not fit; a layer larger than the limit is cut in claim id order, into
    consecutive parts of at most `limit` (a standalone group is one such layer).
    """
    claims = group["claims"]
    if len(claims) <= limit:
        return [claims]
    inside = set(claims)
    pairs = [(n, e["from"], e["to"]) for n, e in enumerate(edges) if e["from"] in inside and e["to"] in inside]
    rows = layered(claims, layer, pairs)[0]
    ordered_layers = [[s for s in row if s in inside] for row in rows]
    parts, current = [], []
    for members in ordered_layers:
        if len(members) > limit:
            if current:
                parts.append(current)
            by_id = sorted(members)
            chunks = [by_id[i : i + limit] for i in range(0, len(by_id), limit)]
            parts.extend(chunks[:-1])
            current = chunks[-1]
        elif len(current) + len(members) > limit:
            parts.append(current)
            current = list(members)
        else:
            current = current + members
    if current:
        parts.append(current)
    return parts


def claim_component(node, x, y):
    state = state_of(node)
    modality = node["modality"]
    return {
        "id": node["claim_id"],
        "type": STATE_TYPE[state],
        "label": node["claim_id"],
        "tag": modality if state == "applies" else f"{modality} · {STATE_TAG[state]}",
        "pos": [x, y],
        "size": [NODE_W, NODE_H],
    }


def outside_component(identifier, tag, x, y):
    return {
        "id": identifier,
        "type": STATE_TYPE["missing"],
        "label": identifier,
        "tag": tag,
        "pos": [x, y],
        "size": [NODE_W, NODE_H],
    }


def finding_reasons(node, outgoing_dangling):
    """The reasons a claim is a finding, in req's words (ADR 0050 decision 7); empty for an ordinary claim."""
    reasons = []
    state = state_of(node)
    if state == "withdrawn":
        reasons.append("withdrawn: superseded by " + ", ".join(node["superseded_by"]))
    elif state == "unknown":
        reasons.append("undecided in this selection")
    elif state == "does_not_apply":
        reasons.append("does not apply in this selection")
    for relation, target in outgoing_dangling:
        reasons.append(f"dangling: {relation} {target}, not in the corpus")
    return reasons


def compose(ctx, name, group_phrase, title_group, blocks, drawn, where):
    """One group page, or the single page of a small document.

    `blocks` is a list of `{"claims": [...], "kind": "family"|"standalone", "label": root or None}`, left to right.
    `drawn` is the list of `(index, edge)` the page draws: every edge with an end on it. An end that is not a claim of
    the page becomes a stub (a claim held on another page, naming it through `where`) or a dangling target.
    """
    nodes, known, edges = ctx["nodes"], ctx["known"], ctx["edges"]
    page_claims = {i for block in blocks for i in block["claims"]}
    layer = ctx["layer"]

    # The ends outside the page, each attached to the block of the lowest-id claim it is joined to. On the single page
    # of a small document the only such ends are dangling targets, and they go in a block of their own below every
    # frame, so no frame draws a claim the corpus does not hold.
    block_of = {i: b for b, block in enumerate(blocks) for i in block["claims"]}
    joined = {}
    for _, edge in drawn:
        for end, other in ((edge["to"], edge["from"]), (edge["from"], edge["to"])):
            if end not in page_claims and other in page_claims:
                joined.setdefault(end, set()).add(other)
    extras = {end: sorted(others) for end, others in joined.items()}
    layout_blocks = list(blocks)
    if name == "all" and extras:
        layout_blocks.append({"kind": "outside", "claims": []})
        extra_block = {end: len(blocks) for end in extras}
    else:
        extra_block = {end: block_of[others[0]] for end, others in extras.items()}
    page_layer = dict(layer)
    for end, others in extras.items():
        page_layer[end] = max(layer[o] for o in others) + 1

    # Layout, block by block, in rows of blocks: a block that would take the page past MAX_WIDTH starts a new row.
    x0 = MARGIN_X
    row_offset = 0  # the first visual row of the current row of blocks
    row_height = 0
    placed = {}  # id -> (absolute x, visual row)
    block_layout = []  # per block: (chains, grow, x, x0, width, block row)
    gutter_x = []
    block_row = 0
    for b, block in enumerate(layout_blocks):
        members = list(block["claims"]) + sorted(e for e, k in extra_block.items() if k == b)
        if block["kind"] in ("standalone", "outside"):
            x, row_of, width, count = standalone_grid(members)
            chains = {}
        else:
            inside = set(members)
            pairs = [(n, e["from"], e["to"]) for n, e in drawn if e["from"] in inside and e["to"] in inside]
            _, x, row_of, chains, width, count = layered(members, page_layer, pairs)
        starts_a_row = b > 0 and (block["kind"] == "outside" or x0 + width > MAX_WIDTH - MARGIN_X)
        if starts_a_row:
            x0, row_offset, row_height, block_row = MARGIN_X, row_offset + row_height + 1, 0, block_row + 1
        grow = {slot: row_offset + r for slot, r in row_of.items()}
        for i in members:
            placed[i] = (x0 + x[i], grow[i])
        block_layout.append((chains, grow, x, x0, width, block_row))
        gutter_x.append(x0 - BLOCK_GAP // 2)
        x0 += width + BLOCK_GAP
        row_height = max(row_height, count)

    def position(identifier):
        px, row = placed[identifier]
        return px, MARGIN_Y + row * PITCH_Y

    components = []
    boundaries = []
    for block in blocks:
        for i in block["claims"]:
            px, py = position(i)
            components.append((placed[i][0], placed[i][1], i, claim_component(nodes[i], px, py)))
    for end in sorted(extras):
        px, py = position(end)
        if end in known:
            tag = TAG_STUB
            boundaries.append({"kind": "region", "label": f"page {where[end]}", "wraps": [end]})
        else:
            tag = TAG_DANGLING
        components.append((placed[end][0], placed[end][1], end, outside_component(end, tag, px, py)))
    components.sort(key=lambda c: (c[1], c[0], c[2]))
    components = [c[3] for c in components]

    # Family frames, only where the page holds several families.
    family_frames = []
    if name == "all":
        for block in blocks:
            if block["kind"] == "family":
                family_frames.append({"kind": "region", "label": block["label"], "wraps": sorted(block["claims"])})
    boundaries = family_frames + boundaries

    # Connections, every one routed explicitly.
    dangling = set(ctx["dangling"])
    undecided = set(ctx["undecided"])
    lanes = {}
    corridors = []
    for chains, grow, x, x0b, width, _ in block_layout:
        by_row = {}
        for slot, r in grow.items():
            by_row.setdefault(r, []).append(x0b + x[slot])
        corridors.append(corridor_in(by_row, x0b, x0b + width - NODE_W))
    connections = []
    for index, edge in drawn:
        source, target = edge["from"], edge["to"]
        sb = block_of.get(source, extra_block.get(source))
        tb = block_of.get(target, extra_block.get(target))
        if sb == tb:
            chains, grow, x, x0b, _, _ = block_layout[sb]
            slots = [source, target] if layout_blocks[sb]["kind"] in ("standalone", "outside") else chains[index]

            def centre(slot, x=x, x0b=x0b):
                return x0b + x[slot] + NODE_W // 2

            route = travel_route(slots, grow, centre, lanes, corridors[sb])
        else:
            (sx, srow), (tx, trow) = placed[source], placed[target]
            same_row_of_blocks = block_layout[sb][5] == block_layout[tb][5]
            gutter = gutter_x[tb] if same_row_of_blocks else MARGIN_X - BLOCK_GAP // 2
            route = cross_route((srow, sx + NODE_W // 2), (trow, tx + NODE_W // 2), gutter, lanes)
        connection = {
            "id": f"edge-{len(connections) + 1:04d}",
            "from": source,
            "to": target,
            "label": edge["relation"],
        }
        connection.update(route)
        reference = f"{edge['relation']} -> {target} (referenced by {source}, not in the corpus)"
        if reference in dangling or target not in known or source not in known:
            connection["variant"] = "security"
        elif f"{source} -{edge['relation']}-> {target} (target undecided)" in undecided:
            connection["variant"] = "dashed"
        connections.append(connection)

    # Cards: the findings first, only when there are any, then one card per group on the page.
    dangling_of = {}
    for _, edge in drawn:
        if edge["to"] not in known and edge["from"] in page_claims:
            dangling_of.setdefault(edge["from"], []).append((edge["relation"], edge["to"]))
    findings = []
    for i in sorted(page_claims):
        reasons = finding_reasons(nodes[i], dangling_of.get(i, []))
        if reasons:
            findings.append(f"{i}: {nodes[i]['statement']} ({'; '.join(reasons)})")
    cards = []
    if findings:
        cards.append({"dot": "rose", "title": f"Findings ({len(findings)})", "items": findings})
    for block in blocks:
        label = block["name"]
        items = [f"{i}: {nodes[i]['statement']}" for i in sorted(block["claims"])]
        cards.append({"dot": "cyan", "title": f"{label} ({len(items)} claims)", "items": items})

    used = {c["type"] for c in components}
    meta = page_meta(ctx, name, title_group, group_phrase, len(page_claims), len(connections), used)
    if name == "all" and 0 < len(family_frames) <= 5:
        meta["views"] = [
            {
                "id": f"view-{k + 1}",
                "label": block["name"],
                "focus": sorted(block["claims"]),
                "note": f"{len(block['claims'])} claims",
            }
            for k, block in enumerate(b for b in blocks if b["kind"] == "family")
        ]
    page = {
        "schema_version": 1,
        "diagram_type": "architecture",
        "meta": meta,
        "components": components,
        "connections": connections,
        "cards": cards,
    }
    if boundaries:
        page["boundaries"] = boundaries
    return page


def page_meta(ctx, name, title_group, group_phrase, claim_count, relation_count, used_types, words=None):
    legend = {}
    translations = dict(CLAIM_WORDS if words is None else words)
    for state, kind in STATE_TYPE.items():
        translations[f"viewer.kind.{kind}"] = STATE_LEGEND[state]
        translations[f"legend.architecture.{kind}"] = STATE_LEGEND[state]
    if words is not None:
        translations.update(words)
    for state, kind in STATE_TYPE.items():
        if kind in used_types:
            legend[kind] = {"label": translations[f"legend.architecture.{kind}"]}
    title = "Requirement graph" if title_group is None else f"Requirement graph: {title_group}"
    subtitle = " · ".join(
        [
            ctx["selection"],
            group_phrase,
            f"{claim_count} of {len(ctx['nodes'])} claims",
            f"{relation_count} relations",
            f"rules_v {ctx['rules_v']}",
            f"archify {ARCHIFY_COMMIT}",
        ]
    )
    return {
        "title": title,
        "subtitle": subtitle,
        # archify applies `translations` only for a document that names its locale.
        "locale": "en",
        "output": f"{name}.html",
        "quality_profile": "standard",
        "legend": {"mode": "auto", "entries": legend},
        "translations": translations,
    }


def index_pages(ctx, groups, page_names, limit):
    """The index: one node per group and one connection per ordered pair of groups and relation (ADR 0050 decision 3).

    More than `limit` groups are cut into consecutive parts, `index`, `index-part-2` and so on, each declaring itself. A
    connection is drawn on every part that holds either of its groups; a group on another part is a stub, naming that part.
    """
    group_of = {}
    for g, group in enumerate(groups):
        for i in group["claims"]:
            group_of[i] = g
    counts = {}
    for edge in ctx["edges"]:
        a, b = group_of.get(edge["from"]), group_of.get(edge["to"])
        if a is None or b is None or a == b:
            continue
        counts[(a, b, edge["relation"])] = counts.get((a, b, edge["relation"]), 0) + 1

    parts = [list(range(k, min(k + limit, len(groups)))) for k in range(0, len(groups), limit)]
    names = ["index" if k == 0 else f"index-part-{k + 1}" for k in range(len(parts))]
    part_of = {g: names[k] for k, part in enumerate(parts) for g in part}
    pages = {}
    for k, (name, part) in enumerate(zip(names, parts)):
        inside = set(part)
        drawn = sorted(key for key in counts if key[0] in inside or key[1] in inside)
        stubs = sorted({g for key in drawn for g in key[:2] if g not in inside})
        shown = part + stubs
        cells = {g: divmod(n, PER_ROW) for n, g in enumerate(shown)}
        components, boundaries = [], []
        for g in shown:
            row, column = cells[g]
            group = groups[g]
            if g in inside:
                count = len(page_names[g])
                tag = f"{len(group['claims'])} claims" + (f" \u00b7 {count} pages" if count > 1 else "")
                kind = STATE_TYPE["applies"]
            else:
                tag, kind = TAG_STUB, STATE_TYPE["missing"]
                boundaries.append({"kind": "region", "label": f"page {part_of[g]}", "wraps": [page_names[g][0]]})
            components.append(
                {
                    "id": page_names[g][0],
                    "type": kind,
                    # A node box holds about 19 characters: the root id fits, "family <id>" does not.
                    "label": group["label"] or "standalone",
                    "tag": tag,
                    "pos": [MARGIN_X + column * PITCH_X, MARGIN_Y + row * PITCH_Y],
                    "size": [NODE_W, NODE_H],
                }
            )
        lanes = {}
        connections = []
        for a, b, relation in drawn:
            connection = {
                "id": f"edge-{len(connections) + 1:04d}",
                "from": page_names[a][0],
                "to": page_names[b][0],
                "label": f"{relation} \u00d7{counts[(a, b, relation)]}",
            }
            connection.update(grid_route(cells[a], cells[b], lanes))
            connections.append(connection)

        cards = [
            {
                "dot": "cyan",
                "title": f"Group pages ({sum(len(page_names[g]) for g in part)})",
                "items": [
                    f"{page}.json: {groups[g]['name']}, {len(groups[g]['claims'])} claims"
                    for g in part
                    for page in page_names[g]
                ],
            }
        ]
        if len(parts) > 1:
            cards.append(
                {
                    "dot": "slate",
                    "title": f"Index pages ({len(parts)})",
                    "items": [f"{n}.json: {groups[p[0]]['name']} to {groups[p[-1]]['name']}" for n, p in zip(names, parts)],
                }
            )
        phrase = f"index of {len(groups)} groups" + (f", part {k + 1} of {len(parts)}" if len(parts) > 1 else "")
        title_group = None if len(parts) == 1 else f"index, part {k + 1} of {len(parts)}"
        claim_count = sum(len(groups[g]["claims"]) for g in part)
        used = {c["type"] for c in components}
        meta = page_meta(
            ctx, name, title_group, phrase, claim_count, sum(counts[key] for key in drawn), used, words=GROUP_WORDS
        )
        page = {
            "schema_version": 1,
            "diagram_type": "architecture",
            "meta": meta,
            "components": components,
            "connections": connections,
            "cards": cards,
        }
        if boundaries:
            page["boundaries"] = boundaries
        pages[name] = page
    return pages


def convert(document):
    """The archify pages for a checked document, by page name. Pure: same document, same value."""
    node_list = sorted(document["nodes"], key=lambda n: n["claim_id"])
    ids = [n["claim_id"] for n in node_list]
    if len(set(ids)) != len(ids):
        refuse("two nodes share one claim_id")
    nodes = {n["claim_id"]: n for n in node_list}
    known = set(ids)
    edges = document["edges"]
    outside = sorted({e["to"] for e in edges if e["to"] not in known} | {e["from"] for e in edges if e["from"] not in known})
    for identifier in ids + outside:
        if not ARCHIFY_ID.match(identifier):
            refuse(f"the id {identifier!r} is outside archify's id grammar; nothing was rewritten")
    for edge in edges:
        if edge["from"] not in known and edge["to"] not in known:
            refuse(f"the edge {edge['from']} -{edge['relation']}-> {edge['to']} has no end in the corpus; no page could hold it")
    limit = page_limit()

    layer, _ = layering(ids, edges)
    family, standalone = families_of(ids, edges)
    selection = " ".join(f"{k}={v}" for k, v in sorted(document["selection"].items())) or "no context"
    ctx = {
        "nodes": nodes,
        "known": known,
        "edges": edges,
        "layer": layer,
        "selection": selection,
        "rules_v": document.get("rules_v", "?"),
        "dangling": document["findings"]["dangling"],
        "undecided": document["findings"]["undecided_edges"],
    }

    groups = []
    for root in sorted({family[i] for i in ids if i not in standalone}):
        members = sorted(i for i in ids if i not in standalone and family[i] == root)
        groups.append({"kind": "family", "label": root, "name": f"family {root}", "page": f"family-{root}", "claims": members})
    if standalone:
        groups.append(
            {"kind": "standalone", "label": None, "name": "standalone claims", "page": "standalone", "claims": sorted(standalone)}
        )

    pages = {}
    if len(ids) <= limit:
        blocks = [dict(group) for group in groups]
        drawn = list(enumerate(edges))
        pages["all"] = compose(ctx, "all", "all claims", None, blocks, drawn, {})
        return pages

    parts_of = [cut_into_parts(group, layer, edges, limit) for group in groups]
    page_names = [
        [group["page"] if k == 0 else f"{group['page']}-part-{k + 1}" for k in range(len(parts))]
        for group, parts in zip(groups, parts_of)
    ]
    where = {}
    for names, parts in zip(page_names, parts_of):
        for name, part in zip(names, parts):
            for i in part:
                where[i] = name
    for group, names, parts in zip(groups, page_names, parts_of):
        for k, (name, part) in enumerate(zip(names, parts)):
            inside = set(part)
            drawn = [(n, e) for n, e in enumerate(edges) if e["from"] in inside or e["to"] in inside]
            block = dict(group, claims=list(part))
            phrase = group["name"] if len(parts) == 1 else f"{group['name']}, part {k + 1} of {len(parts)}"
            pages[name] = compose(ctx, name, phrase, phrase, [block], drawn, where)
    pages.update(index_pages(ctx, groups, page_names, limit))
    return pages


def render(value):
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def main(argv):
    if len(argv) != 3:
        print("usage: trace_to_archify.py <trace.json> <out-dir>   (`-` reads stdin)", file=sys.stderr)
        return 2
    source, target = argv[1], Path(argv[2])
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
        pages = convert(document)
        if target.exists() and (not target.is_dir() or any(target.iterdir())):
            refuse(f"{target} exists and is not an empty directory: stale pages must not mix with new ones")
        outputs = {f"{name}.json": render(page) for name, page in pages.items()}
    except Refusal as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    try:
        target.mkdir(parents=True, exist_ok=True)
        for file_name, output in sorted(outputs.items()):
            with open(target / file_name, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(output)
    except OSError as error:
        print(f"cannot write {target}: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
