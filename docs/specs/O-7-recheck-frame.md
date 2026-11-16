# O-7: a box that needs a human is framed in red (ADR 0052 decision 3, presentation)

Task id: `O-7`. Base: the commit that adds this spec, on top of `b250c3d`, where O-6 (PR #84, the drift demonstration) is
merged. The owner chose this on 2026-11-14 over status colours through an archify fork: on the drift page the SUSPECT box
reads `SUSPECT · 1 of 4 claims` but is drawn in the same colour as every HOLDS box, so a visitor does not see it.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decision 3 (the converter changes
  presentation only: it never adds, removes or moves a component or a connection) and its notes (O-1, O-4, O-6);
- `docs/adr/0053-the-coverage-matrix-is-four-columns-with-a-status-in-each-cell.md` (the matrix, unchanged here);
- archify `7e45f36`'s architecture schema: `boundaries[]` is `{kind: "region" | "security-group", label, wraps[], pad?}`.

Where this spec and an ADR disagree, the ADR wins; stop and report.

## The change (`tools/graph/arch_overlay.py`)

1. **One frame per box that needs a human.** For every component whose status is not `holds`, `holds_weakened` or
   `no claim` (so: `suspect`, `unratified`, `orphaned`, `unknown_claim`, `stale`), the overlay gains one archify boundary
   `{"kind": "security-group", "label": <short label naming the state, e.g. "re-check: SUSPECT">, "wraps": [<that
   component id>]}`. One boundary per component, never one boundary around several (a frame around boxes that are not
   neighbours would enclose boxes that need nothing).
2. **Nothing else moves.** Components, connections, their order and geometry, the author's boundaries, the cards and the
   matrix are as today. A diagram whose components all hold gets **no** frame, so `docs/architecture.html` and
   `docs/architecture-detail.html` stay byte-identical (prove it: regenerate with `build_architecture.py --out` and
   re-render; nothing may change).
3. **Order and determinism.** Added boundaries go after the author's, in diagram component order; two runs give
   identical bytes.
4. **It must render.** archify `validate` and `finalize --quality standard` must pass on the drift page with the frame,
   and on the detailed diagram with a frame nested inside an author's `region` (prove the nesting with a hand-made
   fixture if no real case exists). If archify refuses a single-component `security-group`, or a nested one, stop and
   report with archify's message rather than working around it.
5. **The status card** gains one plain line saying a red frame marks a box whose links need a human (the states above).

## Regenerate

- `build_architecture.py --drift docs/architecture/drift`: the tightening overlay gains the frame; cosmetic and loosening
  do not. Re-render `docs/architecture-drift.html` with archify `7e45f36` and re-make `docs/architecture/drift/req-drift.png`
  the way O-6 made it (visual-check, light, cropped, under ~600 KB). The README text needs no change unless it describes the
  picture; if it does, keep it to what the picture shows.

## Tests first (envelope `red`)

- `tests/graph_overlay.rs`: a suspect component gets exactly one `security-group` boundary wrapping only it, with its
  state in the label; each non-holding state gets one; `holds`, `holds_weakened` and `no claim` get none; two suspect
  components get two boundaries; the author's boundaries, components and connections are unchanged; determinism.
- `tests/drift_demo.rs`: the tightening overlay has the frame around the box that carries REQ-0010; the cosmetic and
  loosening overlays have none.
- `tests/architecture_page.rs`: the real overlays have no added boundary.
- Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Frame `holds_weakened` too → the weakened test fails.
- Wrap all non-holding components in one boundary → the two-components test fails.
- Drop the frame → the drift tightening test fails.

## Verification outside the gate (envelope)

- archify `validate` + `finalize --quality standard` on the drift overlay and the nested fixture: every gate.
- The new drift screenshot, showing the red frame legibly at normal zoom; give its path.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Disk is tight (~20 GiB free): one target directory, delete every temporary copy and render dir.

## Docs, one home each

- ADR 0052: a dated note for O-7 (the frame, which states, why one per box, that real pages are unchanged).
- `examples/graph/README.md`: the overlay section says which states are framed.

## Allowed paths

- `tools/graph/arch_overlay.py`
- `tests/graph_overlay.rs`, `tests/drift_demo.rs`, `tests/architecture_page.rs`, new fixtures under `tests/fixtures/`
- `docs/architecture/drift/**` (regenerated), `docs/architecture-drift.html` (re-rendered)
- `README.md` (only if it describes the drift picture), `examples/graph/README.md`
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the note)

Forbidden: `src/**`, `tools/graph/build_architecture.py` (unless the drift build must pass something new; then stop and
report first), `tools/graph/trace_to_archify.py`, `claims/**`, `.req/**`, `baselines/**`, `decisions/**`, `aliases/**`,
`bench/**`, `docs/spec.md`, `docs/trace.md`, the real architecture diagrams, overlays, matrices and pages
(`docs/architecture.html`, `docs/architecture-detail.html`, `docs/architecture/*.json`, `docs/architecture/*.md`,
`docs/architecture/req-overview.png`), `scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`, and any archify file.
