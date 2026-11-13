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
- The converter reads `req.trace/1` and refuses any other schema, naming it (exit 2), and writes nothing on a refusal. If
  `req` moves the schema to `/2`, the converter says so rather than guessing.
- `finalize` exits non-zero on any failed gate. A non-zero exit is not a page; read its receipt, one per page.
- Each page draws one picture per claim (a node labelled with its claim id, carrying its modality and, where it is not
  an ordinary claim, its state) and one arrow per relation (labelled with the relation). Claims withdrawn by
  `supersedes`, claims outside the selection, claims undecided in it, and edges that are dangling or undecided are each
  drawn differently; the page's legend names them. The cards below the graph list the findings first (every claim that
  is withdrawn, undecided, not applying or dangling, with its reason), then each group's claims as `claim id: statement`.

## What a page shows

The graph is cut into **groups**, computed from the document alone (ADR 0050). A **root** is a claim that derives from
and refines nothing the document holds; a claim's **family** is the lowest-id root it reaches over `derives-from` and
`refines`; a claim with no edge at all is **standalone**. A family is named by its root's id.

- **Up to 150 claims: one page**, `all.json`. Each family gets a labelled frame and, when there are five or fewer, a
  focus view; standalone claims sit outside the frames.
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
