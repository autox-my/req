# O-5: the coverage matrix (ADR 0053)

Task id: `O-5`. Base: the commit that adds this spec, on top of `44227f5`, where ADR 0053 (PR #82) is merged; the owner
ratified it on 2026-11-14.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0053-the-coverage-matrix-is-four-columns-with-a-status-in-each-cell.md`: every decision and its
  *Falsification* list;
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decisions 1–5 and its notes (the
  converter, `build_architecture.py`, the two views); decision 3's matrix is what ADR 0053 replaces;
- `docs/adr/0046-the-trace-document-is-the-graph-contract.md`: decisions 2 and 7 (`req.trace/1` grows additively);
- ADR 0019 and ADR 0024 (a claim's `tests:`, its `evidence`, and how a JUnit report's outcomes are read: `src/junit.rs`).

Where this spec and an ADR disagree, the ADR wins; stop and report.

## Part A: `req.trace/1` nodes carry what the matrix needs (`src/view.rs`)

1. Each node gains, after `area`: `source` (the claim's `rationale.source`, verbatim, `null` when absent), `evidence`
   (the claim's `evidence` value, `null` when absent) and `tests` (the claim's `tests:` ids in file order, `[]` when
   none). Additive under `/1`; nothing else in the document changes.
2. Text `req trace` and `docs/trace.md` are unchanged (`--check` passes untouched).
3. `req trace --help` names the three fields in the sentence that lists the node fields.

## Part B: the converter writes the coverage matrix (`tools/graph/arch_overlay.py`)

1. `matrix.md` becomes ADR 0053's matrix: one row per node of `req.trace/1` whose `applies` is not `false`, ordered by
   claim id; columns **Source | Claim | Satisfied by | Verified by**.
   - *Source*: `source`, verbatim (escape `|` and newlines for Markdown); `no source` when `null` or empty.
   - *Claim*: the id, linked to `claims/<id>.yml` relative to the matrix file, then the statement.
   - *Satisfied by*: each component of this diagram whose cited files carry a link to the claim, in diagram order, as
     `<component label> (<outcome>)`, with the outcome word of the worst link that component has for this claim
     (ADR 0052 decision 2's order); `no design element` when there is none. Name each linked file once under its
     component when it helps a reader, linked to its lines on GitHub at `meta.repository.revision` (ADR 0053
     decision 6); keep the cell readable.
   - *Verified by*: the count and ids of `tests`; `no test` when empty and `evidence` is `machine` (or absent);
     `<evidence> evidence, no test asked` when `evidence` is another value (ADR 0019: the gate does not ask it).
2. **Test outcomes only with a report.** A new optional `--test-report <junit.xml>` reads JUnit `<testcase>` elements
   (id `<classname>::<name>`, ADR 0024) and adds each linked test's outcome: passed, failed, skipped, not found. Read
   the mapping from `src/junit.rs` and `tools/junit_case.py` and follow it (never copy code). An unreadable or
   malformed report is exit 2 naming it. Without the flag, no outcome is written (ADR 0053 decision 5).
3. Determinism, inputs never written, the exit codes and every other output of the converter are unchanged. The status
   card, the unreached-claims card and the overlay JSON keep their contract.

## Part C: the committed matrices and the page

- `tools/graph/build_architecture.py` never passes `--test-report`, so `docs/architecture/matrix.md` and
  `overview-matrix.md` stay a function of the repository; regenerate both with `--out`. `--check` keeps its contract.
- The README section and `docs/index.html` keep their counts, now read from the new matrix shape; update the parsing in
  `tests/architecture_page.rs` that reads them.
- The architecture pages do not change unless the overlay JSON changes (it should not); say so.

## Tests first (envelope `red`)

Oracles from ADR 0053 and hand-written fixtures, never from a run.
- `tests/trace_nodes.rs` (and `tests/json_format.rs` if it pins node keys): `source`, `evidence`, `tests` on each node,
  in that position, `null`/`[]` when absent; text `trace` unchanged.
- `tests/graph_overlay.rs`:
  - four columns whatever the number of components (*Falsification* 5: add a component, same column count);
  - each gap named: no source, no design element, no test, and the non-machine evidence wording (*Falsification* 1);
  - a suspect link shows `(suspect)` in *Satisfied by*, never as plain covered (*Falsification* 2);
  - with `--test-report`: passed, failed, skipped and not found each shown; without it, no outcome;
  - a committed-style run (no report) is byte-identical when only a report file changes (*Falsification* 3);
  - determinism (*Falsification* 4); a malformed report is exit 2.
- Update the O-1/O-4 tests that read the old grid; say which and why.

## Mutation checks (envelope `mutations`)

- Leave a gap cell blank → the gap test fails.
- Drop the outcome from *Satisfied by* → the suspect test fails.
- Write test outcomes without `--test-report` → the no-report test fails.
- Emit one column per component again → the column-count test fails.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. `docs/spec.md`, `docs/trace.md` and the three Baselines unchanged and VERIFIED.

## Docs, one home each

- `req trace --help`: the three node fields.
- `examples/graph/README.md`: the overlay section describes the matrix's columns and `--test-report`.
- ADR 0053: **Status** becomes accepted (ratified by the owner's merge of PR #82, 2026-11-14) and a dated
  implementation note. ADR 0052: one line in its notes pointing to ADR 0053 for the matrix.

## Allowed paths

- `src/view.rs` (the node fields only), `src/main.rs` (the trace help page only)
- `tools/graph/arch_overlay.py`, `tools/graph/build_architecture.py`
- `tests/trace_nodes.rs`, `tests/json_format.rs`, `tests/graph_overlay.rs`, `tests/architecture_page.rs`, new fixtures
  under `tests/fixtures/`
- `docs/architecture/matrix.md`, `docs/architecture/overview-matrix.md` (regenerated), `README.md` (only if a number
  or link must change), `docs/index.html` (likewise)
- `examples/graph/README.md`
- `docs/adr/0053-the-coverage-matrix-is-four-columns-with-a-status-in-each-cell.md`,
  `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the one line)

Forbidden: `RULES_V` and any change to the canonical form or an attested hash, `src/canon.rs`, `src/model.rs`,
`src/baseline.rs`, `src/satisfy.rs`, `src/junit.rs`, `claims/**`, `.req/**`, `baselines/**`, `decisions/**`,
`aliases/**`, `bench/**`, `docs/spec.md`, `docs/trace.md`, `scripts/check.sh`, `Cargo.toml`, `Cargo.lock`,
`.github/**`, `tools/graph/trace_to_archify.py`, the archify diagrams and pages under `docs/architecture*` other than
the two matrices, and any archify file. If a forbidden file must change, stop and report.
