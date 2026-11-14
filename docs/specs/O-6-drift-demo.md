# O-6: what drift looks like on the architecture (ADR 0052 decision 7, step 4)

Task id: `O-6`. Base: the commit that adds this spec, on top of `dd78c81`, where O-5 (PR #83, the coverage matrix) is
merged. Decision 7 step 4 of ADR 0052, ratified by the owner: "a before-and-after demonstration of drift: a claim's
meaning changes on a **copy** of a corpus, and the components that satisfy it turn SUSPECT. Never on req's own claims."

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decision 7 step 4 and its notes;
- `docs/adr/0053-the-coverage-matrix-is-four-columns-with-a-status-in-each-cell.md`: the matrix;
- `docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md`: holds, holds (weakened), suspect; the admitted carries
  (`modality_weakening`, `numeric_loosening`);
- `tools/graph/build_architecture.py` and `tests/architecture_page.rs`: the generated-and-checked pattern this extends.

Where this spec and an ADR disagree, the ADR wins; stop and report.

## The point the page makes

req's difference from a presence check (Doorstop's suspect-on-any-edit, a tool that only checks a link exists) is that
**it judges a change by its direction**. The demo shows three edits to req's real claims, each on a throw-away copy,
side by side, against the real recorded links:

| edit (on a copy) | what req says | why it matters |
| --- | --- | --- |
| **cosmetic**: re-word a linked claim without changing its meaning | the link still **HOLDS** | no false alarm, so reviewers keep reading alarms |
| **loosening**: relax a linked bound (e.g. REQ-0010 `at most 0.30` → `at most 0.40`) | **HOLDS (weakened)**, named | what met the stronger requirement meets the weaker one (an admitted carry) |
| **tightening**: tighten the same bound (`at most 0.30` → `at most 0.20`) | **SUSPECT** (`numeric_tightening`), with the re-record command | the design must be re-checked by a human |

The orchestrator probed the third on a scratch copy at `dd78c81`: `req satisfy` reports
`[SUSPECT] src/bench.rs -> REQ-0010: numeric_tightening since it was recorded`. Verify all three yourself with
`req satisfy` and `req compare`; pick a different claim or wording if one does not classify as the table says, and say
which you used. The cosmetic edit must keep the address (same meaning fingerprint), so prove it with `req canon` or
`req compare` before you rely on it.

## Deliverables

1. **The edits are data, not prose: `docs/architecture/drift/edits.json`.** Each edit names the claim, the exact
   statement before and after, and the outcome req is expected to give. The build applies them to a copy; nothing in
   this repository's corpus changes.
2. **The build: `tools/graph/build_architecture.py --drift <dir>`** (or a sibling script beside it, if cleaner; say
   which). For each edit, on a fresh temporary copy of the repository (`git archive HEAD` or a copy of the tracked
   files; never this checkout):
   - apply the statement change to the claim file;
   - run `req satisfy --format json` and `req trace --format json` there, and the converter on the **overview**
     diagram (`docs/architecture/req-overview.archify.json`, unchanged) with the same `--page`/`--matrix-dir` rules as
     O-5, writing `overlay.json` and `matrix.md` per edit under `docs/architecture/drift/<edit-id>/`;
   - also save the exact `req satisfy` text output per edit (`satisfy.txt`), which is what an engineer would see.
   - `--check` regenerates all of it and compares, with the existing exit contract (0 current, 1 stale naming the
     file, 3 req cannot be run, 2 usage). The temporary copies are deleted.
3. **The page: `docs/architecture-drift.html`**, rendered by archify `7e45f36` from the **tightening** edit's overlay,
   so the Evidence box (or whichever box carries the edited claim) shows `SUSPECT · 1 of N claims`. Its subtitle or a
   card says plainly that this is a demonstration on a copy, names the edit, and links back to
   `architecture.html` (the real, all-HOLDS state).
4. **The README.** In the *Requirements on the architecture* section, a short **"What drift looks like"** paragraph and
   the three-row table above with req's real output words, generated numbers only, and links to the drift page and to
   the three `satisfy.txt` files. One image of the drift page (made as `req-overview.png` was: archify `visual-check`,
   light, cropped, under ~600 KB) as `docs/architecture/drift/req-drift.png`, placed beside or under the existing
   image.
5. **`docs/index.html`**: the architecture item gains one link to the drift page, in the existing voice.

## Tests first (envelope `red`)

In `tests/architecture_page.rs` (or a sibling `tests/drift_demo.rs`), with `CARGO_BIN_EXE_req`:
- `--check` on the committed drift directory exits 0;
- each edit's committed `satisfy.txt` contains the expected outcome word for the edited link (`holds`, `holds
  (weakened)` / its JSON word, `suspect` with the direction), and the overlay's status for the box that carries the
  edited claim matches it;
- the cosmetic edit leaves every link HOLDS (the whole `satisfy.txt` verdict is PASS);
- the repository's own corpus is untouched after the build (`git status --porcelain` empty in a copy-based test, and
  no write outside the drift directory);
- a hand-edited `edits.json` whose tightening no longer tightens makes `--check` report the drift directory stale.
- Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Apply the edits to the real checkout instead of a copy → the untouched-corpus test fails.
- Swap the loosening and tightening statements → the outcome tests fail.
- Skip `satisfy.txt` regeneration in `--check` → a stale-text test fails.

## Verification outside the gate (envelope)

- archify `validate` and `finalize --quality standard` on the tightening overlay: every gate.
- A screenshot of the drift page at normal zoom showing the SUSPECT box legibly.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Disk is limited (~23 GiB free): one target directory; delete every temporary copy.

## Docs, one home each

- ADR 0052: a dated note for step 4 (the three edits, how to regenerate, what the gate checks). Decision 7 is then
  complete; say so.
- `examples/graph/README.md`: one line pointing at `docs/architecture/drift/` as the worked example of drift.

## Allowed paths

- `docs/architecture/drift/**` (new), `docs/architecture-drift.html` (new), `docs/index.html` (its one item)
- `tools/graph/build_architecture.py` (or one new sibling script under `tools/graph/`)
- `tests/architecture_page.rs` or a new `tests/drift_demo.rs`
- `README.md` (the one section), `examples/graph/README.md` (the one line)
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the note)

Forbidden: `src/**`, `tools/graph/arch_overlay.py`, `tools/graph/trace_to_archify.py`, `claims/**` (the edits happen
only on copies), `.req/**`, `baselines/**`, `decisions/**`, `aliases/**`, `bench/**`, `docs/spec.md`,
`docs/trace.md`, the existing architecture diagrams, overlays, matrices and pages, `scripts/check.sh`, `Cargo.toml`,
`Cargo.lock`, `.github/**`, and any archify file. If a forbidden file must change, stop and report.
