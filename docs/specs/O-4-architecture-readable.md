# O-4: the architecture page shows the requirements at first sight (ADR 0052 decision 3, follow-up to O-3)

Task id: `O-4`. Base: the commit that adds this spec, on top of `9793834`, where O-3 (PR #80) is merged. The owner merged
PR #80 "as it is" on 2026-11-14 and asked for the improvements as a follow-up.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decisions 1–5 and 7, its O-1 and
  O-3 notes. Decision 3 leaves *which archify fields carry the status* to the implementation; this unit changes that
  choice, and nothing else in the ADR;
- `docs/adr/0046-the-trace-document-is-the-graph-contract.md` decision 7 (archify `7e45f36`, offline);
- `docs/specs/O-3-architecture-page.md` for the files, the build script and the `--check` contract, which stay.

Where this spec and an ADR disagree, the ADR wins; stop and report.

## The two problems (found on PR #80's page)

1. **The status is invisible at normal zoom.** The converter puts it in a component's `tag`, which archify 7e45f36 draws
   only at the finest zoom (`data-detail="fine"`, `renderers/architecture/render-architecture.mjs:1093`). The
   `sublabel` is drawn at the overview zoom (`data-detail="context"`, :1090). A reader of the published page and the
   README image sees req's architecture, not which part satisfies which requirement.
2. **The diagram is too dense to read.** 27 components, 50 connections, 28 crossings by archify's own readability
   signal. Correct, but not legible as a front page.

## Part A: the status goes where archify always draws it (`tools/graph/arch_overlay.py`)

1. A component's **`sublabel`** becomes its status line: the state word and the count, for example
   `HOLDS · 6 claims`, `SUSPECT · 1 of 3 claims`, `STALE`, `no claim`. Exact wording is yours; it must name the state
   and, when there are links, how many claims, and the worst state must be readable at a glance.
2. The **author's `sublabel`** is not lost: it moves to `tag` (after the author's own tag text, if any, as today), so it
   shows at the fine zoom and in the passport panel. The claim ids stay on the status card.
3. Fit: measure the sublabel with archify's own sublabel rule (read it at 7e45f36, as O-1 did for the tag; never copy
   archify's code). A status line or a moved author's text that cannot fit is still written, named on stderr, and the
   exit is 3, exactly as O-1-fix made it for the tag.
4. Everything else in the converter's contract is unchanged: determinism, inputs never written, the worst-of order,
   links keyed on `sources`, the cards, the matrix, the exit codes.

## Part B: an overview diagram for the front page

1. **`docs/architecture/req-overview.archify.json`**: a second, smaller architecture diagram of req in repository mode,
   pinned to the same revision as `req.archify.json` (O-3), at most **12 components and about 20 connections**.
   - One component per area of `.req/areas` that has code (authoring, identity, decisions, variants, verification,
     baselines, evidence, operation, merged or split as the code warrants), plus the stores and the CLI entry. Each
     cites the files that carry its claims (at most three `sources`; choose the files that hold the most links, and
     say in the note which linked files the overview leaves to the detailed view).
   - Connections are real calls or reads/writes (repository-authoring step 3), only the ones a reader needs to follow
     a claim from authoring to a signed Baseline. archify's readability signal must report **at most 8 crossings**.
2. The detailed diagram `req.archify.json` stays as it is, and keeps its own page.
3. **Pages:** `docs/architecture.html` becomes the overview page; the detailed page moves to
   `docs/architecture-detail.html`. Each links to the other in a card (one line), and `docs/index.html` keeps its
   one item.
4. **The build script** (`tools/graph/build_architecture.py`) generates and checks both overlays: `overlay.json` and
   `matrix.md` for the detail, `overview-overlay.json` and `overview-matrix.md` for the overview, in
   `docs/architecture/`. `--check` covers all four, with O-3's exit contract.
5. **The README image** becomes the overview, made as O-3-fix made `req.png` (archify `visual-check`, light theme,
   cropped), readable at README width, under ~600 KB. The README section's counts still come from the matrix.

## Tests first (envelope `red`)

- `tests/graph_overlay.rs`: the status is in `sublabel` (worst state and count), the author's sublabel is in `tag`
  after the author's tag, a status line that cannot fit is named and exit 3; update the O-1 tests that assumed the tag
  carries the status, and say which you changed and why.
- `tests/architecture_page.rs`: `--check` is current for both diagrams; a tightened linked claim on a copy makes both
  overviews stale (or the one that cites it); the overview has at most 12 components; the embedded source-evidence
  check (O-3-fix) runs for both pages.
- Prove the new tests red at the base.

## Mutation checks (envelope `mutations`)

- Put the status back in `tag` → the sublabel test fails.
- Drop the author's sublabel instead of moving it → the keep-text test fails.
- `--check` skips the overview files → the overview stale test fails.

## Verification outside the gate (envelope)

- archify `validate` and `finalize --quality standard` on both overlays: every gate, and the crossing count of each.
- A screenshot of each page at normal zoom showing the status lines legibly; attach the README image path.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Disk is limited (~26 GiB free): one target directory; delete scratch copies.

## Docs, one home each

- ADR 0052: a dated note for O-4: the field change (status in `sublabel`, the author's text in `tag`, why), the two
  views and how to regenerate them.
- `examples/graph/README.md`: the overlay section says which field carries the status.
- `README.md`: the section's image and links (overview page first, detail page second).

## Allowed paths

- `tools/graph/arch_overlay.py`, `tools/graph/build_architecture.py`
- `tests/graph_overlay.rs`, `tests/architecture_page.rs`
- `docs/architecture/**`, `docs/architecture.html`, `docs/architecture-detail.html` (new), `docs/index.html` (its one item)
- `README.md` (the one section), `examples/graph/README.md`
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the note)

Forbidden: `src/**`, `tools/graph/trace_to_archify.py`, `claims/**`, `.req/**`, `baselines/**`, `decisions/**`,
`aliases/**`, `bench/**`, `docs/spec.md`, `docs/trace.md`, `docs/overview.html`, `scripts/check.sh`, `Cargo.toml`,
`Cargo.lock`, `.github/**`, and any archify file. If a forbidden file must change, stop and report.
