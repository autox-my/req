# 0054. An explorer page navigates the coverage graph

Extends ADR 0053 (the coverage matrix) by deciding what it left open: the interactive "graph half" of the hybrid view. Narrows
ADR 0046 decision 5 ("req writes archify's input; archify only renders it") for this one page, which req renders itself.
Nothing else is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: accepted** (ratified by the owner's merge of PR #91, 2026-11-16, with decision 7: plain SVG first, cytoscape.js
then sigma.js as the measured fallback). Asked for by the owner on 2026-11-16 ("next: the interactive explorer from the
hybrid-view concept"), after ADR 0053 adopted the concept's matrix half and deferred this one. Date proposed: 2026-11-16.

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
7. **Plain SVG first; a pinned graph library is the planned fallback, chosen by measurement.** The page draws with plain
   SVG and hand-written script, so req carries no JavaScript dependency. The positions are computed by the builder, not
   by a force simulation, so the page stays deterministic whatever draws it. If the measurement in decision 4 fails with
   plain SVG, the fallback is a graph library **copied into the page** (inline, never from a CDN), **pinned** like the
   other provenance pins in `AGENTS.md` (verified at pin time, never bumped opportunistically) and fed the builder's
   positions. The two candidates, compared from the npm registry and unpkg on 2026-11-16 (facts, not yet measured on a
   req corpus):

   | | cytoscape.js 3.35.0 | sigma.js 4.0.6 (+ graphology 0.26.0) |
   | --- | --- | --- |
   | licence | MIT | MIT (both) |
   | single browser file, no build step | `dist/cytoscape.min.js`, 443 KB | `dist/sigma.min.js`, 394 KB, plus graphology's UMD build, 74 KB |
   | runtime dependencies | none | `events`, `graphology-utils` (inside the bundle) |
   | drawing | canvas | WebGL (needs a WebGL-capable browser) |
   | groups (an area holding its claims) | compound nodes, built in | none: drawn by the page |
   | layered layout | preset positions; `cytoscape-dagre` 4.1.0 (MIT, one file, 46 KB) if needed | preset positions only; its layouts are force-directed |
   | strength | rich selectors and styling, groups, edge labels | many thousands of nodes on screen at once |

   **Cytoscape.js is the first fallback**: no build step, no WebGL requirement, and built-in groups match decision 3's
   areas. **sigma.js** is chosen instead only if the measurement shows the page must draw more nodes at once than canvas
   can keep responsive, which decision 4's collapse and node limit are meant to avoid. `elkjs`, the other layered
   layout, is not a candidate: it is licensed `EPL-2.0 OR GPL-3.0-or-later`. Before either library is adopted, a
   spike renders the 5,000-claim corpus with plain SVG and with the candidate, and its numbers are recorded; adopting
   one is an amendment to this ADR, naming the version and its digest.

## Alternatives rejected

- **archify's own focus views.** At most five, fixed in the diagram file, and bounded by archify's measured size limit.
- **A graph library loaded from a CDN.** It loads code from the network (REQ-0012). A copied, pinned library is not
  rejected: it is decision 7's fallback.
- **Quarkdown** (`iamgio/quarkdown`, GPL-3.0, Kotlin; GitHub, 2026-11-16). A Markdown typesetting system that compiles one
  source to a paper, book, slides or static site. It writes documents, not an interactive graph: no focus, filters or
  graph layout. It also needs a JVM and installs by piping a script into `sudo bash`.
- **mdxcn** (`shadcn-labs/mdxcn`, MIT; GitHub, 2026-11-16). React components for MDX (callouts, steps, terminals, charts
  drawn with text glyphs), copied in with the shadcn CLI. Page decoration for a React/MDX site: it would bring npm and
  React into req and has nothing for drawing a large graph.
- **StrictDoc** (`strictdoc-project/strictdoc`, Apache-2.0, Python) and **Sphinx-Needs** (`useblocks/sphinx-needs`, MIT,
  its `needflow` graphs): exporting req's data into a requirements tool's own viewer. Each renders its own data model,
  so req's status on each link (holds, suspect, by the direction of a change) would be lost in translation, and an
  adopter would need a second toolchain. Their graph views at a real specification's size were not measured here.
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
   limit instead of asking the reader to narrow;
6. a graph library reaches the page from the network, unpinned, or without the spike's recorded numbers and an amendment
   naming its version and digest.

## Implementation note — 2026-11-16 (E-1: the explorer page)

**Plain SVG passed the measurement at 5,000 claims, so decision 7's fallback is not used and no JavaScript dependency was
added.** The page is first-party HTML, CSS and script only.

- **Files.** `tools/explorer/build.py` (Python standard library; `--out <page>` / `--check <page>`, `--req <path>` or PATH; exit
  0 / 1 stale / 2 usage / 3 UNVERIFIABLE, as `tools/overview/build.py`), `tools/explorer/explorer.html` (the template: the same
  bytes for every corpus, the data inserted at one token), `docs/explorer.html` (the page, committed, published by Pages) and
  `tests/explorer_page.rs`. Regenerate with `python3 tools/explorer/build.py --out docs/explorer.html`. The check is part of the gate
  (`cargo nextest run`), so a stale page fails it.
- **The data document** (`req.explorer/1`, in `<script type="application/json" id="req-explorer-data">`, sorted keys, `</` escaped,
  no timestamp): `claims`, `sources` (grouped by equal text), `design` (each element, with its component and page when an overlay
  cites the file: the overview's first, else the detail's), `tests` (one group per claim, ids only), `edges` (`source_of`,
  `satisfied_by` with the link's outcome and line, `verified_by`), `gaps` (`no_source`, `no_design`, `no_test` per claim),
  `not_asked` (a non-machine-evidence claim with no test: not a gap, ADR 0053 decision 2) and `dangling_links` (a link naming a
  claim the selection does not hold: listed, not drawn). The selection is the first line of `.req/variants`, the one
  `docs/trace.md` uses, and the page states it. A claim the selection does not apply (`does_not_apply`) is a node but carries no
  gap, as the matrix leaves it out (ADR 0053). No test outcome is in the data, and the builder takes no test report (decision 6).
  The builder is UNVERIFIABLE (exit 3) on another schema or an unreadable overlay, and on a `req satisfy` that lists an
  `unreadable` file, as `build_architecture.py` is.
- **Layout.** Decision 7 has the builder compute positions for a library. With plain SVG there is no library to feed, so the page's
  script lays out what is visible as a pure function of the data and the view (four columns, a source or design node at the mean row
  of the claims it connects, snapped to the claim grid; no force simulation, no randomness): the same view is the same picture. A
  `claim → tests` edge runs along the lane between two rows, so it never passes through a design node.
- **The node limit is `NODE_LIMIT = 2000`** (a named constant in the template). Past it the page draws what fits in claim order and
  says `Narrow the view to see N hidden nodes` (area rows, claims, their source, design and test nodes and the drawn gaps all count;
  *Falsification* 5). Why 2,000: one area of the 5,000-claim corpus below (about 417 claims, 12 areas) is 1,644 nodes, so one whole
  area fits; the response stayed under about 70 ms there. The same pages built with 4,000 and 8,000 gave 134 ms and 266 ms for the
  heaviest filter (`outcome = holds`), so speed is not what sets the limit: readability is, a column of about 500 rows is already a
  scroll of some 20,000 px.
- **Measurement** (2026-11-16, Google Chrome 157.0.8103.52 headless, `--headless=new`, Apple M1, macOS 27.2.1, Node 24.18.1 driving it
  over the DevTools protocol; `tools/explorer/spike/browser.mjs measure`). The corpora are synthetic, made by
  `tools/explorer/spike/synthetic_corpus.py` (not committed as data): a heap-shaped `derives-from` tree, a `refines` link on every
  seventh claim, every tenth standalone; 12 areas at two depths; a `rationale.source` on 95 % of claims (three claims share one);
  two to five test ids (none on 3 %); a design link on about 70 % (some files are real component files of the overlays), after which a
  few claims are tightened (SUSPECT), loosened (HOLDS (weakened)), left unrecorded (UNRATIFIED) or orphaned. The 500-claim corpus's
  record was made with `req satisfy --record`; at 5,000 claims one `--record` call per link loads the whole corpus each time, so the
  generator writes `.req/satisfied` in one pass, and its file is byte-for-byte the one `--record` wrote at 500 claims. A time is
  dispatch to the second animation frame after it (layout and paint included), the median of 5 runs, so about 28 ms is the two-frame
  floor of the machine, not the page's work; first draw is the median of 5 launches, from navigation start to the first frame after
  the first render.

  | claims (design links) | page bytes | first draw | expand an area | focus a claim | focus a design node | a layer off | an outcome (worst of 5) | an area | gaps only | search (worst of 4) | most nodes drawn |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | 500 (360) | 280,738 | 36.5 ms | 28.1 ms | 28.4 ms | 27.9 ms | 29.7 ms | 30.4 ms | 28.7 ms | 30.7 ms | 31.0 ms | 958 |
  | 1,000 (720) | 520,372 | 37.3 ms | 26.8 ms | 27.6 ms | 27.8 ms | 28.3 ms | 59.6 ms | 29.3 ms | 32.3 ms | 31.3 ms | 1,872 |
  | 5,000 (3,600) | 2,453,417 | 48.5 ms | 48.4 ms | 53.9 ms | 53.7 ms | 41.3 ms | 61.9 ms | 46.9 ms | 49.8 ms | 31.0 ms | 1,998 |

  At 5,000 claims `outcome = holds` and `gaps only` hit the limit (1,998 and 1,997 nodes drawn, the message shown); narrowing by area
  shows the rest. Every interaction is under 100 ms, against the 1 s the spec allowed.
- **Keyboard** (real key events through the same driver, `browser.mjs keys`): Tab reaches the link back, the search, three layer
  boxes, the outcome and area selects, *gaps only*, *reset* and then every node; Enter on an area row opens it, Enter on a node
  focuses it and keeps the keyboard focus on it, Esc clears. The page reads at phone width (390 px: controls wrap, the focus panel
  comes before the graph, the graph scrolls inside its own frame and the page does not).
- **The named gap: how the page behaves in a browser is not in the gate.** `scripts/check.sh` has no browser. What is held: the data
  document equals what `req` says (nodes, edges, outcomes, gaps, computed independently in `tests/explorer_page.rs`), the page is the
  template around that document and builds to the same bytes twice, nothing is fetched. Whether the script draws and responds is
  checked by hand with `browser.mjs`, and a change to the template needs that run again.
- **What was not done.** Variants are not switched on the page (*Not decided here*); the matrices and the architecture pages do not
  yet link back to the explorer (the converter writes the matrices, and the architecture pages are archify's); and a design link
  whose claim the selection does not hold is listed in the data and counted on the page, not drawn.
