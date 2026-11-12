# V-1 — the trace document carries its nodes, and archify renders it (ADR 0046)

Task id: `V-1`. Base: the commit that adds this spec and the revised ADR 0046 (proposed; the PR that carries this work
ratifies it). Owner go-ahead of 2026-11-13: render the graph with archify (`tt-a1i/archify`), with 0046 split out of
PR #66 into this unit.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0046-the-trace-document-is-the-graph-contract.md` (decisions 1–9 and its *Falsification*
list). Where this spec and the ADR disagree, the ADR wins; stop and report.

## The external tool (read at named provenance, AGENTS.md)

- **archify:** `https://github.com/tt-a1i/archify`, commit **`7e45f36`**, licence Apache-2.0 (`LICENSE`, and `license:`
  in `archify/SKILL.md`).
  - Read it in a gitignored scratch directory: `git clone` into `.scratch/archify` and `git checkout 7e45f36`.
  - Never commit any of its files, and never copy its text into our docs. Our docs name it, its commit and its licence.
- **What its input looks like:** `archify/SKILL.md` (the *Type router*), `archify/schemas/*.schema.json` and
  `archify/examples/*.json`. Read the schema of the type you choose, and `references/authoring-defaults.md`.
- **Offline:** `archify/bin/delivery-update.mjs` checks for updates unless `ARCHIFY_UPDATE_CHECK_DISABLED=1`. Every
  command you run, and every command the skill reference gives, sets it.

## Choices fixed here

1. **`nodes` in `req.trace/1`** (ADR decision 2), in `view::trace_document`.
   - One entry per row: every claim in the corpus, sorted by `claim_id`.
   - Fields, in this order: `claim_id`, `statement`, `modality`, `address`, `applies` (`Applies::as_str`, as the other
     documents spell it), `superseded_by`.
   - Place the key after `selection` and before `edges`. The text matrix, `docs/trace.md` and every `--out`/`--check`
     byte stay unchanged.
   - `tests/json_format.rs` gains the key. The `req trace --help` sentence listing the document's parts names `nodes`.
2. **The converter:** `tools/graph/trace_to_archify.py`, Python 3 standard library only, run as `python3 -I`.
   - Usage: `trace_to_archify.py <trace.json> <out.json>`. A `-` reads stdin.
   - It refuses any `schema` other than `req.trace/1` (exit 2, naming the schema) and malformed JSON (exit 2). It writes
     nothing on a refusal.
   - Output is the archify diagram file for **one** type: `architecture` or `dataflow`, whichever schema holds typed,
     labelled, directed edges between nodes with the least invention. Say which and why in the ADR implementation note.
   - **Every** node and edge comes from the document. A node's label is its `claim_id`; the statement and modality go
     in whatever detail field the schema offers. Each edge's label is its `relation`.
   - Placement: layer = longest `derives-from`/`refines` path to a root (any relation if that is simpler; say which),
     column order by `claim_id`. No randomness, no clock, sorted keys, a stable float-free layout (integers only).
   - `applies` = does-not-apply and withdrawn (`superseded_by` non-empty) are drawn distinctly, using whatever the
     schema offers (a tag, a style or a group). A dangling or undecided edge (from `findings`) is drawn distinctly too.
   - It imports no network module (`socket`, `urllib`, `http`, `ssl`, `subprocess`). A test asserts this statically.
   - The archify commit it was written against (`7e45f36`) is a constant in the file and is written into the output's
     metadata if the schema has a free-text field for it.
3. **The skill reference:** `examples/graph/README.md`, one page for an adopter and their agent.
   - Install archify at the pinned commit.
   - The three commands: `req trace --format json <selection> > trace.json`, the converter, and
     `ARCHIFY_UPDATE_CHECK_DISABLED=1 node <archify>/bin/archify.mjs finalize <type> <file> <out.html> --quality standard
     --json`.
   - The rule from ADR decision 6, stated as instructions to an agent: never add, remove, relabel or fix a node or an
     edge; on a geometry failure adjust placement fields only, and say so.
   - It states that the page is not committed and is as current as the run that made it.

## Tests first (envelope `red`)

Oracles come from the ADR and hand-written fixtures, never from a run.

- **`req.trace/1`**, CLI over a scratch corpus:
  - `nodes` lists every claim, including one with no relation, sorted, with the six fields;
  - a withdrawn claim carries `applies: "does_not_apply"` and its `superseded_by`;
  - two runs give identical bytes;
  - `trace --out` of the same corpus is byte-identical to the base build's (no text change).
- **The converter**, driven from a Rust test that runs `python3 -I tools/graph/trace_to_archify.py` the way `tests/`
  already drives `tools/` (follow the G-1 precedent), over a hand-written `req.trace/1` fixture of about five claims:
  - every node id and every `(from, relation, to)` appears in the output, and nothing else does (falsification 3);
  - two runs give identical bytes (falsification 1);
  - a `req.trace/2` document and malformed JSON are exit 2 with nothing written (falsification 5);
  - a withdrawn node and a dangling edge are marked distinctly;
  - the static no-network-import check (falsification 4).

Prove the req tests red at the base. The converter tests are red because the file does not exist.

## Verification outside the gate (report it in the envelope; do not add it to `check.sh`)

- Render this repository's own graph: `req trace --format json` for `.req/variants` line 1, then the converter, then
  archify `finalize` with `ARCHIFY_UPDATE_CHECK_DISABLED=1`.
- Report the receipt's exit code and gate results. If `browser-check` cannot run here (no browser), report that plainly;
  it is not a failure of this unit.
- If `finalize` fails on validation, fix the **converter**, never the output by hand. Keep the HTML in `.scratch/` and
  name its path in the envelope. Do not commit it.

## Mutation checks (envelope `mutations`)

- Drop claims with no relation from `nodes` → the "every claim" test fails.
- Skip one edge kind in the converter → the node/edge equality test fails.
- Iterate an unordered set when placing nodes, or add a timestamp → the determinism test fails (show the failure, then
  restore).
- Accept any `schema` → the `/2` test fails.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home): exit 0, no SKIP, tree clean after the run. `docs/spec.md`,
`docs/trace.md` and the three Baselines unchanged and VERIFIED.

## Docs, one home each

- `req trace --help`: the document's parts include `nodes`, and one sentence pointing at `examples/graph/README.md`.
- `docs/adopting.md`: one short paragraph beside the existing `req trace` text, pointing at `examples/graph/README.md`.
- ADR 0046: a dated implementation note, covering the archify type chosen, the layout rule and the verification result.

## Allowed paths

`src/view.rs` (the trace document only), `src/main.rs` (the trace help page only), `tests/**`,
`tools/graph/trace_to_archify.py` (new), `examples/graph/README.md` (new), `docs/adopting.md`,
`docs/adr/0046-the-trace-document-is-the-graph-contract.md` (the implementation note).

Forbidden: `src/canon.rs`, `src/compare.rs`, `src/contradict.rs`, `src/gate.rs`, `src/baseline.rs`, `RULES_V`,
`bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`, `docs/spec.md`, `docs/trace.md`,
`scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`, and any archify file. If a forbidden file must change,
stop and report.
