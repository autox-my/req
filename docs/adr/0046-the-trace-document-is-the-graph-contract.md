# 0046. The trace document is the graph contract, and a viewer is an asset

Extends ADR 0018 (committed projections) and ADR 0023 (`req check`). Adds **no** command, flag or projection. No
`RULES_V` bump, no claim-schema or Baseline change, no address moves: a document a command already prints is named a
contract, and nothing about what req computes moves.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Raised from a comparative
review of an adopter-surface tool (reqord, `f043777`) whose one capability req lacks is a readable graph. Revised
2026-11-13 after review (PR #66, finding 1): decisions 2–3 previously assumed a committed JSON projection, which
`--out`/`--check` refuse by design. Date proposed: 2026-11-12.

## Context

`req trace` already prints the graph: "the outgoing edges, the **derived** incoming edges" for a selection. It also
emits a versioned document, `req.trace/1`, under `--format json`.

That document is **stdout only**, and deliberately so: `--out` and `--check` are text projections, pairing either with
`--format json` is a usage error (exit 2), and the help page says why — "a JSON projection on disk with
`--out`/`--check` is a follow-on, not this version" (`src/main.rs:409`). So the graph has a data model, a schema name and
a version, and no committed form at all.

The gap is the reader. `docs/trace.md` is legible to the person who wrote the corpus; a product owner or an auditor gets
a matrix. A second requirements tool ships a dashboard for exactly this, and it is the one thing it does better.

The temptation is to generate an HTML or SVG graph directly from `trace`. That is how a view becomes a second source of
truth: the artefact is edited by hand once, then quietly stops matching the corpus — the drift this project exists to
name, reproduced inside its own deliverable.

## Decision

1. **The document is the contract; the viewer is an asset.** `req.trace/1` is frozen as the public graph schema. A
   viewer reads the feed and never writes; it is never truth, and deleting it removes nothing from the corpus.
2. **This decision adds no projection.** `--out`/`--check` stay text-only, and a JSON projection on disk remains the
   follow-on the help page names. Nothing here is committed, so nothing here needs `--check`, and no line enters
   `.req/projections` (ADR 0018 §4 stays untouched).
3. **A viewer takes the file from the user, not from a path it fetches.** Browsers block `fetch` on `file://` pages, so
   the viewer offers a file picker and reads what the person selects. It opens no socket and fetches nothing (REQ-0012).
4. **The document is deterministic even though nothing commits it.** Two runs over one corpus produce identical bytes:
   nodes ordered by claim id, edge lists sorted, no timestamp, no randomness. A consumer — a viewer, a diff, a test —
   may compare two runs, and a stable document is what makes that comparison mean anything.
5. **Drilldown is `impact`, not a second graph.** `req impact <id>` already answers nearest-first. If its shape is
   wanted as a document it is an additive `req.impact/1` declared on `req impact --help`.
6. **The viewer ships as an example, not a package.** `examples/graph/` — one HTML file, no build step, no dependency,
   opened from disk.
7. **Compatibility is stated in the document.** `req.trace/1` gains fields additively; a breaking change is `/2`, and a
   `/1` consumer says it cannot read `/2` rather than guessing, the way `render --check` reports another build's
   `rules_v`.

## Consequences

- An adopter can render a graph their product owner will read, without a server and without a second copy of the corpus.
- `req.trace/1` becomes a compatibility promise for the first time. It is already versioned and already carries a
  `schema` field; this decision is what makes the version mean something to a consumer outside req.
- The drift risk is bounded by construction: there is no committed artefact to drift, because there is no artefact.
- No rendered file enters `docs/`, and `req check` learns nothing new.

## Rejected

- **Adding `--out`/`--check` for `--format json` now.** A command-contract change that deserves its own decision, and
  the benefit here does not depend on it: a viewer reads a feed, not a pinned file.
- **SVG from `trace --out`.** A second artifact format to pin, check and diff, for a rendering concern.
- **A force-directed layout in the generator.** Non-deterministic, so two runs would differ and no comparison could be
  made.
- **A `req graph` command.** Each command is a contract on its `--help` page; the data is already answerable from
  `trace`.
- **Committing the graph in req's own repository.** It answers a question nobody here asks, at the cost of a projection
  every claim change must regenerate.

## Falsification

This decision is wrong if any of these is observed:

1. two runs over one corpus produce different bytes for the same `req.trace/1` document;
2. the field set or field order changes without the schema version moving;
3. the viewer needs a network connection, or writes to the corpus;
4. a `/1` consumer cannot tell a `/2` document from a `/1` one.
