# A graph page for a product owner, from the trace document

`req trace` already holds the graph. This page is for an adopter and their agent: turn that document into one standalone
page you can open or hand to someone who will not read a matrix. The contract is
[ADR 0046](../../docs/adr/0046-the-trace-document-is-the-graph-contract.md); the renderer is
[archify](https://github.com/tt-a1i/archify) (Apache-2.0), used at one pinned commit.

## What this is, and is not

- **The `req.trace/1` document is the graph.** The page is a rendering of it. It is never the corpus, never a source of
  truth, and deleting it removes nothing.
- **It is not committed, and it is as current as the run that made it.** The page does not know when the corpus changes.
  Re-run the three commands below, and do not check the HTML in.
- **A model does not draw it.** `tools/graph/trace_to_archify.py` writes archify's input from the document,
  deterministically: the same document gives the same bytes. archify validates and renders; nobody places a node by
  hand.

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
python3 -I tools/graph/trace_to_archify.py trace.json graph.json
ARCHIFY_UPDATE_CHECK_DISABLED=1 node ../archify/archify/bin/archify.mjs \
  finalize architecture graph.json trace-graph.html --quality standard --json
```

- `req trace --format json` exits 3 while an edge is dangling or undecided; the document is complete either way, and
  `graph.json` draws those edges distinctly. Judge the verdict with `req trace`, not with the page.
- The converter reads `req.trace/1` and refuses any other schema, naming it (exit 2), and writes nothing on a refusal. If
  `req` moves the schema to `/2`, the converter says so rather than guessing.
- `finalize` exits non-zero on any failed gate. A non-zero exit is not a page; read its receipt.
- The page draws one picture per claim (a node labelled with its claim id, carrying its modality and, where it is not
  an ordinary claim, its state) and one arrow per relation (labelled with the relation). The full statements are on
  the page's cards. Claims withdrawn by `supersedes`, claims outside the selection, claims undecided in it, and edges
  that are dangling or undecided are each drawn differently; the page's legend names them.

## Rules for an agent running this

These are instructions, and they are the point of the page.

1. **Never add, remove, relabel or "fix" a node or an edge.** Every node and every edge comes from `trace.json`. If
   something looks wrong in the graph, the corpus is wrong or the selection is, and the fix is a claim, not the page.
2. **If `finalize` reports a geometry failure, change placement only.** That is node `pos` and `size` and an edge's
   routing fields in `graph.json` (or, better, the placement rule in the converter, so the next run keeps the fix). Do
   not touch an `id`, a `label`, a `from`, a `to`, a statement or a tag. **Say in your report that you adjusted
   placement.**
3. **Do not write to the corpus, and do not commit the page.** The commands above read `claims/` and write only the files
   you name.
4. **Report what ran.** State the exit code of `finalize` and which of its gates passed. A page whose browser gate could
   not run (no browser on the machine) has not had it; say so instead of calling it checked.
