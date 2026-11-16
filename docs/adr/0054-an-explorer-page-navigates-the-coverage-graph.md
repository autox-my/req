# 0054. An explorer page navigates the coverage graph

Extends ADR 0053 (the coverage matrix) by deciding what it left open: the interactive "graph half" of the hybrid view. Narrows
ADR 0046 decision 5 ("req writes archify's input; archify only renders it") for this one page, which req renders itself.
Nothing else is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: proposed.** The owner ratifies it by merging. Asked for by the owner on 2026-11-16 ("next: the interactive
explorer from the hybrid-view concept"), after ADR 0053 adopted the concept's matrix half and deferred this one. Date
proposed: 2026-11-16.

## Context

**What the concept asks for.** The hybrid-view note the owner evaluated (local, generic, not committed; its evidence is
uncited, so it is a lead) pairs the matrix with an exploration graph: nodes in layers **Source → Requirement →
Architecture → Verification**, typed edges, *focus* (select a node, dim the rest, show its chain), *filters* (by type,
by status), and *gaps only*. Its demo is one hand-written HTML page over 22 hand-written nodes.

**What req has.** Every layer is already in req's documents: `req.trace/1` nodes carry the statement, `area`, `source`,
`evidence` and `tests` (ADR 0046, 0051, 0053); `req.satisfy/1` carries each design link and its outcome (ADR 0040); the
architecture overlays map design files to components (ADR 0052). The coverage matrix shows all of it as rows. What a
matrix cannot do is *follow* a chain across rows: "which claims does `src/gate.rs` satisfy, and which tests verify
those?".

**Why archify cannot be that page.** archify draws a laid-out, validated picture; it has no filters, no focus on an
arbitrary node, and no layout of its own for thousands of nodes. ADR 0050 measured its limit: a 500-claim page passed,
a 1000-claim page was **not judged** (archify's checker overran its output buffer). An explorer must stay usable at the
size of a real specification, thousands of claims and, for req itself, 1,404 test cases.

**Why a hand-drawn graph cannot be either.** ADR 0046's trap stands: a graph a model draws can drop or invent an edge.

## Decision

1. **One generated page, `docs/explorer.html`.** A standalone HTML file with inline CSS and JavaScript: no network, no
   third-party script, no build step (REQ-0012). req's tooling writes it; it is committed and checked like
   `docs/overview.html` (`--out` / `--check`), and published by the Pages site.
2. **The data is req's, embedded verbatim; the page only reads it.** The page embeds one JSON document built
   deterministically from `req.trace/1`, `req.satisfy/1` and the architecture overlay's component map (files → component
   ids), with nothing added: every node and edge on the page is a node or edge in those documents. No model writes it
   (ADR 0004, ADR 0046 decision 6). The script is a fixed template beside the builder, the same bytes for every corpus.
3. **Four layers, typed edges.** Columns: *Source* (a claim's `rationale.source`, grouped by equal text), *Claim*
   (grouped by `area`, ADR 0051), *Design* (each design element, labelled with its component when an overlay maps it),
   *Tests*. Edges: *source of*, *satisfied by* (with the link's outcome: holds, holds (weakened), suspect, …), *verified
   by*. A gap is drawn as a gap (no design element, no test, no source), never left out (ADR 0053 decision 2).
4. **Built for scale, not for 22 nodes.**
   - Tests are **grouped per claim** (one "N tests" node a reader expands), never one node per test case by default.
   - The page opens **collapsed by area**: one row per area with its counts; a reader expands an area, or searches.
   - **Focus** on any node shows its whole chain and dims the rest. **Filters** by layer, by outcome and by area, and a
     **gaps only** switch, narrow what is drawn. Search by claim id, file path or test id.
   - The page never lays out more than a fixed number of nodes at once (a named constant, set by measurement); past it,
     it asks the reader to narrow instead of drawing an unreadable graph, and says how many nodes are hidden.
   - It must open and respond on a synthetic corpus of 5,000 claims (the generator ADR 0050 used), measured and recorded
     in the implementation note: time to first draw, file size, interaction with every filter.
5. **The explorer and archify have separate jobs.** archify draws the architecture (ADR 0052); the explorer navigates the
   coverage graph. Each links to the other: a design node opens its component on the architecture page
   (`#focus=<id>`, as ADR 0053 decision 6 does); the architecture and matrix pages link to the explorer.
6. **Test outcomes stay out of the committed page**, as in ADR 0053 decision 5: the committed explorer is a function of
   the repository. A run given a JUnit report may build an uncommitted explorer with outcomes.

## Alternatives rejected

- **archify's own focus views.** At most five, fixed in the diagram file, and bounded by archify's measured size limit.
- **A graph library from a CDN** (Cytoscape, D3, vis). It loads code from the network (REQ-0012) or adds a vendored
  dependency req would have to pin and review; the layered layout here is simple enough to draw in plain SVG.
- **A server or a `req serve` command.** The core has no network surface by design, and a static page is enough.
- **The concept's demo as is.** Hand-written data and an every-node layout; it shows 22 nodes and would not survive a
  real specification.

## Not decided here

- Editing from the page (proposing a link, recording one). The page is read-only; recording stays `req satisfy
  --record`, a human act in a terminal (ADR 0040 decision 2).
- A source as its own first-class node in the corpus (ADR 0053 *Not decided*): the page groups sources by equal text only.
- Variants: the page shows one selection (the one `req trace` is run with), as the matrix does.

## Consequences

- A reader can answer "what does this file satisfy, and is it still true?" and "which claims have no test?" in two
  clicks, at any corpus size the measurement admits.
- req carries one hand-written front-end template, its first. It is reviewed like code and tested by the page's
  `--check` and by a test that the embedded document equals what the builder computes; its behaviour in a browser is
  not in the gate (no browser in `check.sh`), which the implementation note states as the named gap.

## Falsification

This decision is wrong if any of these is observed:

1. the page shows a node or an edge that is not in `req.trace/1`, `req.satisfy/1` or the overlay map, or omits one;
2. a gap is hidden (a claim with no design element, no test or no source is not reachable through *gaps only*);
3. the page loads anything over the network;
4. two builds over the same repository give different bytes;
5. at the measured corpus size the page does not open or a filter does not respond, or it draws more nodes than its
   limit instead of asking the reader to narrow.
