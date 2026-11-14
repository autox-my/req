# A graph page for a product owner, from the trace document

`req trace` already holds the graph. This page is for an adopter and their agent: turn that document into one standalone
page you can open or hand to someone who will not read a matrix. The contract is
[ADR 0046](../../docs/adr/0046-the-trace-document-is-the-graph-contract.md); the renderer is
[archify](https://github.com/tt-a1i/archify) (Apache-2.0), used at one pinned commit.

## What this is, and is not

- **The `req.trace/1` document is the graph.** The page is a rendering of it. It is never the corpus, never a source of
  truth, and deleting it removes nothing.
- **It is not committed, and it is as current as the run that made it.** The page does not know when the corpus changes.
  Re-run the commands below, and do not check the HTML in.
- **A model does not draw it.** `tools/graph/trace_to_archify.py` writes archify's input from the document,
  deterministically: the same document gives the same bytes. archify validates and renders; nobody places a node by
  hand.
- **Every page says which slice of the graph it shows.** A page is never presented as the whole graph unless it is: see
  [What a page shows](#what-a-page-shows).

## Install archify at the pin

The converter was written against archify commit **`7e45f36`**. Install that commit, not the latest one:

```sh
git clone https://github.com/tt-a1i/archify ../archify
git -C ../archify checkout 7e45f36
```

archify checks for updates over the network unless told not to. **Every command here sets
`ARCHIFY_UPDATE_CHECK_DISABLED=1`**, so rendering needs no network (REQ-0012); keep it on every archify command you add.
Its own install steps are its own; take them from its repository at that commit.

## The three commands

From the corpus root, for the selection you want drawn (the same `key=value` words `req trace` takes):

```sh
req trace --format json deployment_mode=air_gapped > trace.json
python3 -I tools/graph/trace_to_archify.py trace.json graph
for page in graph/*.json; do
  ARCHIFY_UPDATE_CHECK_DISABLED=1 node ../archify/archify/bin/archify.mjs \
    finalize architecture "$page" "${page%.json}.html" --quality standard --json
done
```

- The converter writes a **directory**: `graph/all.json` when the document is small, or `graph/index.json` plus one
  `graph/<page>.json` per group when it is not (see below). It refuses an existing non-empty directory (exit 2, naming
  it), so pages from an earlier run never mix with this one: delete the directory or pick a new one.
- `req trace --format json` exits 3 while an edge is dangling or undecided; the document is complete either way, and
  the pages draw those edges distinctly. Judge the verdict with `req trace`, not with the page.
- `req trace --format json` reads `.req/areas` when it exists (and only for `--format json`): a malformed registry is exit
  2 naming every bad `<file>:<line>`, with nothing on stdout, as `req check` names it.
- The converter reads `req.trace/1` and refuses any other schema, naming it (exit 2), and writes nothing on a refusal. If
  `req` moves the schema to `/2`, the converter says so rather than guessing.
- `finalize` exits non-zero on any failed gate. A non-zero exit is not a page; read its receipt, one per page.
- Each page draws one picture per claim (a node labelled with its claim id, carrying its modality and, where it is not
  an ordinary claim, its state) and one arrow per relation (labelled with the relation). Claims withdrawn by
  `supersedes`, claims outside the selection, claims undecided in it, and edges that are dangling or undecided are each
  drawn differently; the page's legend names them. The cards below the graph list the findings first (every claim that
  is withdrawn, undecided, not applying or dangling, with its reason), then each group's claims as `claim id: statement`.

## What a page shows

The graph is cut into **groups**, computed from the document alone (ADR 0050, ADR 0051). There are two ways to cut it, and
the document says which: **by area when any claim carries an `area`, and by family otherwise.**

- **Families** (no claim has an area). A **root** is a claim that derives from and refines nothing the document holds; a
  claim's **family** is the lowest-id root it reaches over `derives-from` and `refines`; a claim with no edge at all is
  **standalone**. A family is named by its root's id.
- **Areas** (any claim has one, ADR 0051). A group is the claims whose `area` is one path exactly, so `diagnostics` and
  `diagnostics/startup` are two groups and a claim is in exactly one. Groups run in the order of `.req/areas`, then any area
  the registry does not declare (by path), then **`no area`**, last. A group is named by its **label** in `.req/areas`, or
  by its path when undeclared; `req trace --format json` carries the registry as `areas`, so the converter needs nothing
  but the document. Families are not computed. Inside a group the layers follow only the `derives-from`/`refines` edges
  whose two ends are both in it, and a claim with no such edge sits in a grid under them, inside the group's frame; an
  edge to another group is drawn like any edge that leaves a group (below). On a split run a page is `area-<path>.json`,
  with `/` written as `.` (`area-diagnostics.startup.json`), and `no-area.json`. A label is free text, so the converter
  counts its width at least as wide as archify does and cuts what would not fit: the index labels a node with the path's
  last segment and the name rides in its tag, a frame title is cut with an ellipsis, and the whole name is in the page's
  title, subtitle and cards. The count errs on the side of cutting; it is not archify's check, so run `finalize` on every
  page as above.

- **Up to 150 claims: one page**, `all.json`. Each family (or area) gets a labelled frame and, when there are five or
  fewer, a focus view; standalone claims sit outside the frames.
- **More than 150 claims: an index plus one page per group.** `index.json` has one node per group and one connection per
  pair of groups and relation, labelled `<relation> ×<count>`; its cards list each group page's file name (archify has
  no link from one page to another, so open the file by name). A group larger than 150 claims is cut into parts by layer,
  `family-REQ-0001.json`, `family-REQ-0001-part-2.json`, and each part says `part 2 of 3`.
- **An edge that leaves a page is still drawn.** It ends on a stub node labelled with the claim at its far end, inside a
  frame that names the page holding that claim (an edge from another group into a claim of the page starts on a stub the
  same way). A dependency is never dropped; you can see that it exists and where it lives.
- **The subtitle states the slice:** `<selection> · <group, or all claims> · <N> of <M> claims · <E> relations ·
  rules_v <n> · archify 7e45f36`. `N` and `E` count what this page draws, `M` what the document holds. An index page
  says `index of <G> groups` and counts the relations that run between groups.

Within a page, claims sit in layers by `derives-from`/`refines` depth, ordered to cross as few edges as it can. A layer
wider than seven boxes (`WRAP`) continues on further bands directly below it, in the same order, and an edge that skips a
layer passes through a reserved slot in it. Families are blocks running left to right; a block that would take the page
past 1710 px (`MAX_WIDTH`) starts a new row of blocks. The target of a dangling edge is drawn below every frame, outside
all of them. When there are more groups than the limit, the index is cut into parts (`index.json`, `index-part-2.json`)
that say `part 2 of 3`, and a connection to a group on another part ends on a stub that names that part.

The 150-claim limit is `PAGE_LIMIT` in the converter. A test (and only a test) lowers it with the environment variable
`REQ_GRAPH_PAGE_LIMIT`, read only when set; a value that is not a positive integer is a refusal.

**Why pages are no wider than that.** archify's `browser-check` fits the drawing to the reader's window and refuses text it
would shrink below 6 px. Measured at its default 1440x900 window (archify 7e45f36): a canvas 1680 px wide passes and one 1900
px wide fails, so `WRAP` and `MAX_WIDTH` keep every page under 1710 px (the measurements are in ADR 0050's implementation
note). If a page still fails a gate, read its receipt: the page is a valid rendering of its slice, but do not call it
checked.

## Rules for an agent running this

These are instructions, and they are the point of the page.

1. **Never add, remove, relabel or "fix" a node or an edge.** Every node and every edge comes from `trace.json`. If
   something looks wrong in the graph, the corpus is wrong or the selection is, and the fix is a claim, not the page.
2. **If `finalize` reports a geometry failure, change placement only.** That is node `pos` and `size` and an edge's
   routing fields in the page's `.json` (or, better, the placement rule in the converter, so the next run keeps the fix). Do
   not touch an `id`, a `label`, a `from`, a `to`, a statement or a tag. **Say in your report that you adjusted
   placement.**
3. **Do not write to the corpus, and do not commit the page.** The commands above read `claims/` and write only the files
   you name.
4. **Report what ran.** State the exit code of `finalize` and which of its gates passed. A page whose browser gate could
   not run (no browser on the machine) has not had it; say so instead of calling it checked.

## Architecture overlay

The trace graph above draws claims. An **architecture diagram** (an archify `architecture` file an architect or agent
drew from the repository, each component citing the files in `sources`) can show the claims its components satisfy
([ADR 0052](../../docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md)). The link stays
where [ADR 0040](../../docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning-and-a-change-of-meaning-is-judged-by-its-direction.md)
puts it: a `req-satisfies:` line in a file under a `.req/design` root, recorded by a human with `req satisfy --record`. A
component satisfies the union of the claims the files it cites declare; the diagram stores nothing and is never edited.

```sh
req satisfy --format json > satisfy.json
req trace --format json > trace.json
python3 -I tools/graph/arch_overlay.py [--root .] [--test-report junit.xml] [--page overlay.html] diagram.json satisfy.json trace.json overlay
ARCHIFY_UPDATE_CHECK_DISABLED=1 node ../archify/archify/bin/archify.mjs \
  validate architecture overlay/overlay.json --repo-root . --json
ARCHIFY_UPDATE_CHECK_DISABLED=1 node ../archify/archify/bin/archify.mjs \
  finalize architecture overlay/overlay.json overlay/overlay.html --repo-root . --quality standard --json
```

- **Three inputs**, none opened for writing: the diagram, `req satisfy --format json` (`req.satisfy/1`) and `req trace
  --format json` (`req.trace/1`, for the claims and their statements). `--root` is the corpus root the cited paths and
  `.req/design` are read under (default: the current directory). It writes into `<out-dir>` only, and refuses an existing
  non-empty one.
- **`overlay.json`** is a new archify file: every component and connection, in the author's order with the author's
  geometry and labels. **The status is the component's `sublabel`**, the line archify draws at every zoom: the worst state and
  how many claims are in it, `HOLDS · 6 claims`, `SUSPECT · 1 of 3 claims`, `STALE`, `no claim`. The author's own `sublabel`
  moves to the `tag` (after the author's tag, if any; archify draws a tag only at the finest zoom and in the passport panel),
  so no text is lost; the claim ids are on the status card. Status is the worst outcome of the links of the cited files, worst
  first: `UNKNOWN CLAIM`, `ORPHANED`, `SUSPECT`, `UNRATIFIED`, `HOLDS (weakened)`, `HOLDS`. Two more states are not outcomes:
  `STALE` (a cited path does not exist under `--root`; worst of all) and `no claim` (no link; not a finding). Three cards
  are appended: components by status, claims no component reaches, and cited paths outside every `.req/design` root (the
  satisfy document holds no link for those). The subtitle names the diagram's pinned revision, the `rules_v` and archify
  `7e45f36`, so the page never claims to be current.
- **`matrix.md`** is the coverage matrix (ADR 0053): a Markdown table with a row per applying claim, by id, and four columns
  whatever the number of components: **Source** (the claim's `rationale.source`), **Claim** (its id, linked to its file, and its
  statement), **Satisfied by** (each component whose cited files link the claim, in diagram order, as `Pump (suspect)`: the
  worst outcome of its links, then the files that carry them, linked at the diagram's pinned revision, with no line anchor,
  when the repository is on GitHub; with `--page` the component's name links to its node on that page) and **Verified by** (the claim's tests, by count and id). A gap is named, never blank: `no source`,
  `no design element`, `no test`, and `process evidence, no test asked` for a claim whose evidence the gate does not ask a test
  of (ADR 0019). The claims no component reaches are the rows that say `no design element`, and the overlay's card.
- **`--test-report <junit.xml>`** (optional) adds what each linked test did, from a JUnit report (`classname::name` ids, as
  `req gate --test-report` reads): `(passed)`, `(failed)`, `(skipped)` or `(not found)` after each id. An outcome depends on a
  run, not on the repository, so a run with a report is not committed, and the converter never opens a report you did not name;
  an unreadable or malformed report is exit `2` naming it. **`--matrix-dir <dir>`** (optional; relative to `--root`, default
  the root) is the directory the matrix will be read from, so its claim links are right there. **`--page <page>`** (optional) is
  the page the diagram is rendered to: each component then links to `<page>#focus=<component id>`, which archify's viewer
  opens with that part selected. Give an absolute `https://` URL (used as given; a fragment of its own or another scheme is
  exit `2`) for a matrix read on GitHub, where a relative link to an `.html` file opens its source; a relative path
  (`../architecture.html`) is percent-encoded. Without it no page is guessed and a component is plain text. The report is read by element local name (a default namespace is accepted); a `DOCTYPE` or `ENTITY`
  declaration is refused, text that only looks like one inside a comment or CDATA section is not.
- **`--repo-root`** is archify's: a diagram that cites `sources` is verified against the committed bytes at its pinned
  revision, in a checkout whose `origin` matches the diagram's `meta.repository.url`. Without it archify refuses the page.
- **Exit:** `0` written; `3` written, but a component is `STALE`, a link names an unknown claim, or a status line or the
  author's moved text does not fit its box (archify would refuse it as `component-sublabel-overflow` or
  `component-tag-overflow`; the box is `size[0]`, else 120 px, and holds `(width - 8) / 3.6` columns) (the reason is printed); `2` refused (a wrong schema, malformed JSON, an unreadable input, a non-empty out-dir), nothing written.
- Two runs over the same inputs give the same bytes. Like the trace pages, the overlay is not committed and is as current
  as the run that made it. A model never edits it: the status comes from `req satisfy` alone.

The overlays that are committed are this repository's own architecture, as a worked example: [`docs/architecture/`](../../docs/architecture/) holds two diagrams (an overview of at most twelve components, the front page, and the detail), the generated overlay and matrix of each, and `tools/graph/build_architecture.py` regenerates and checks all four files.

[`docs/architecture/drift/`](../../docs/architecture/drift/) is the worked example of drift: three edits to one claim, each made on a copy of the corpus, with the overlay, matrix and `req satisfy` output they give (`build_architecture.py --drift`).
