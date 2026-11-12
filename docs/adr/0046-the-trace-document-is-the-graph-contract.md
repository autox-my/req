# 0046. The trace document is the graph contract, and archify renders it

Extends ADR 0018 (committed projections) and ADR 0023 (`req check`). It adds no command, flag or projection. No
`RULES_V` bump, no claim-schema or Baseline change, no address moves: a document that a command already prints becomes a
contract, gains one additive field, and nothing req computes changes.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it, together with the code (task
V-1).
- First drafted in PR #66 (2026-11-12), raised from a comparative review of an adopter-surface tool whose one capability
  req lacks is a readable graph. That draft planned a hand-built viewer.
- Revised 2026-11-13 on the owner's decision to render with archify, and moved out of PR #66 into its own unit.

Date proposed: 2026-11-12.

## Context

`req trace` already prints the graph, "the outgoing edges, the **derived** incoming edges", for a selection, and under
`--format json` it prints a versioned document, `req.trace/1`. That document is stdout only, by design: `--out` and
`--check` are text projections, and pairing either with `--format json` is a usage error (exit 2).

**The gap is the reader.** `docs/trace.md` is legible to the person who wrote the corpus. A product owner or an auditor
gets a matrix.

**The document is not yet a graph.** `req.trace/1` holds `edges`, `derived_incoming` and `findings`, and nothing else:
- a claim with no relation does not appear in it at all;
- no node carries a statement, a modality, its applicability in the selection, or the successor that withdrew it
  (ADR 0049).

**A renderer exists.** archify (`tt-a1i/archify`, Apache-2.0) is an agent skill. It takes a JSON diagram file, validates
it, and writes one standalone, offline HTML page, checked in a real browser. Its normal path has an agent write that file,
including node placement.

**The trap is a graph a model draws.** A graph drawn by a model is a model's reading of the corpus: it can drop an edge or
invent one, and two runs give two files. That is the drift this project exists to name, reproduced inside its own
deliverable. It also breaks ADR 0004: the model proposes, and here nobody would dispose.

## Decision

1. **The document is the contract.** `req.trace/1` is the public graph schema. A renderer reads it and never writes the
   corpus. A rendered page is never truth, and deleting one removes nothing.
2. **The document carries its nodes.** `req.trace/1` gains `nodes`: one entry per claim in the corpus, sorted by claim id,
   each with `claim_id`, `statement`, `modality`, `address`, `applies` (the tri-state in this selection) and
   `superseded_by` (ADR 0049). The field is additive under decision 7. The text matrix and `docs/trace.md` do not change.
3. **No projection is added.** `--out`/`--check` stay text-only. Nothing in this decision is committed, so nothing needs
   `--check`, and `.req/projections` and ADR 0018 §4 are untouched.
4. **The document is deterministic.** Two runs over one corpus give identical bytes: nodes and edges ordered by claim id,
   no timestamp, no randomness. A consumer can compare two runs, and a stable document is what makes that comparison
   mean anything.
5. **req writes archify's input; archify only renders it.** A req-side converter turns one `req.trace/1` document into
   one archify diagram file, **deterministically**. Nodes and edges come from the document and nothing else. Placement
   is computed from the graph (layers by relation depth, ties broken by claim id), never chosen by a model.
   - The converter is a tool beside the importers, Python standard library only, and offline. The core never runs it,
     and `scripts/check.sh` never needs archify or a browser.
   - Withdrawn claims, dangling edges and undecided edges are drawn distinctly, so the page shows what the matrix's
     findings say.
6. **The agent may not author the graph.** A skill reference for archify tells an agent to run `req trace --format
   json`, then the converter, then `archify finalize`, and **never** to add, remove, relabel or "fix" a node or an edge.
   If `finalize` reports a geometry failure, the agent may adjust placement fields only, and says so.
7. **archify is pinned and offline.**
   - It is used at a named commit, recorded beside the converter. Like the OCR pin, that is tooling provenance, not a
     fingerprint-path pin.
   - Every invocation the skill reference gives sets `ARCHIFY_UPDATE_CHECK_DISABLED=1`, because `finalize` otherwise
     checks for updates over the network (REQ-0012).
8. **Drilldown is `impact`, not a second graph.** `req impact <id>` already answers nearest-first. If its shape is wanted
   as a document, that is an additive `req.impact/1` declared on `req impact --help`.
9. **Compatibility is stated in the document.** `req.trace/1` gains fields additively; a breaking change is `/2`. The
   converter refuses a schema it does not know, naming it, rather than guessing, the way `render --check` reports
   another build's `rules_v`.

## Consequences

- An adopter can give a product owner an explorable graph page with no server and no second copy of the corpus.
- The page is as current as the run that made it. It is not committed, so it never claims to be current.
- `req.trace/1` becomes a compatibility promise for the first time.
- req depends on archify only at the adopter's edge. If archify changes its input schema, the converter breaks and says
  so, and the corpus, the gate and every committed projection are unaffected.

## Rejected

- **A hand-built viewer in `examples/graph/`** (the PR #66 draft). It is a second renderer to build and maintain, for a
  job a maintained renderer already does.
- **Let the agent draw the graph from the trace** (archify's usual path). The graph would be a model's reading, would
  differ between runs, and could lose an edge silently.
- **SVG or HTML generated by req itself.** That is a rendering concern inside the core, and a second artefact format to
  pin and check.
- **A force-directed layout.** It is non-deterministic, so two runs would never compare equal.
- **A `req graph` command.** The data is already answered by `trace`, and a second command would be a second home for
  one question.
- **Joining `req.render/1` and `req.trace/1` in the converter instead of adding `nodes`.** Two documents to keep in step,
  and the trace document would stay an incomplete graph.

## Falsification

This decision is wrong if any of these is observed:

1. two runs over one corpus produce different bytes for the same `req.trace/1` document, or the converter produces
   different bytes from the same document;
2. a field is removed, renamed or changes meaning without the schema version moving;
3. a claim, an edge or a relation label in the document is missing from the converter's output, or one is present
   there that the document does not hold;
4. the converter, or any invocation the skill reference gives, needs a network connection or writes to the corpus;
5. the converter accepts a schema version it was not written for.

## Implementation note (2026-11-13, unit V-1)

- **`nodes`** is in `req.trace/1` (`view::trace_document`): one entry per claim in the corpus, sorted by claim id, with
  `claim_id`, `statement`, `modality`, `address` (spelled `sha256:<hex>`, as the other documents spell it), `applies` and
  `superseded_by`. The key sits between `selection` and `edges`, and each node's fields keep the order above in the bytes.
  The text matrix, `docs/trace.md` and every `--out`/`--check` byte are unchanged; `req trace --help` names `nodes`.
- **The archify type is `architecture`.** archify's `dataflow` takes labelled directed flows but caps a diagram at five
  stages and five rows, which a claim graph of any size exceeds, and its `lifecycle`, `workflow` and `sequence` types
  assume a main path or a time order a claim graph does not have. `architecture` has typed nodes, labelled and
  variant-carrying directed connections, free integer `pos`/`size` placement with no stage cap, and free-text `tag` and
  `cards`. It is the type with the least invention.
- **What each field carries.** A node's label is its `claim_id`; its `tag` is the modality, followed by the state when the
  claim is not an ordinary applying one. A statement is far longer than a node box and archify draws a node's sublabel on
  one unwrapped line, so the full statement (and, for a withdrawn claim, its successors) is the claim's **card**; nothing
  is shortened. An edge's label is its `relation`. Four component types stand for four states (applies, undecided, does not
  apply, withdrawn), a fifth for a target an edge names that the corpus does not hold, and `meta.legend` names each in
  req's words. An undecided edge is a `dashed` connection and a dangling edge a `security` one; archify's architecture
  legend has no entry for a connection variant, so the page says it in the node tag and card, not in the legend.
- **A dangling edge's target is drawn as one node** ("not in the corpus"), because archify refuses a connection to an id it
  does not hold. The id comes from the document's own edge, so the output still holds nothing the document does not.
- **Layout rule.** A node's layer is the longest `derives-from`/`refines` path to a root (a root is a claim with none of
  either); any other relation does not move it, and a cycle's closing edge is not followed. Layers are rows, top to
  bottom; inside a layer, columns run in claim id order and wrap after six. Every coordinate is an integer. **Every**
  edge is routed explicitly, only through the channels between rows and the gutters between columns, with its label on
  the first routed segment: no route can cross an unrelated node, and parallel routes fan out over fixed lane offsets in
  edge order. (Neighbours were first left to archify's automatic route, which puts the label of an adjacent-row edge
  inside its source node; review V-1-review-1 found it, and no edge is left to automatic placement now.) The converter
  writes the archify commit (`7e45f36`) into `meta.subtitle`; it refuses a document with no nodes, or a `superseded_by`
  that is not claim ids, by name.
- **Falsification, as tested.** `tests/graph_archify.rs` (the converter, over a hand-written fixture) pins items 1, 3, 4
  and 5; `tests/trace_nodes.rs` and `tests/json_format.rs` pin the document, and item 1 for the document itself.
- **Verification outside the gate** (this repository's own graph: `.req/variants` line 1, 40 claims and 40 edges, none
  dangling or undecided): the converter's output is byte-identical across two runs, and `archify finalize architecture
  … --quality standard --json` with `ARCHIFY_UPDATE_CHECK_DISABLED=1` exited 0, with `validate`, `deliver`, `check` and
  `browser-check` all `pass` and no diagnostic; so did three small corpora `req` produced itself (a downward
  `supersedes` edge, a parent-to-child edge, an edge into a claim the corpus does not hold). The page was **not visually reviewed**; archify advises a perceptual look
  at route bends (`visualReview: not-requested`). The HTML is in the session scratch directory and is not committed.
