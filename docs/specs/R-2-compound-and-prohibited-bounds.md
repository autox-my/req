# R-2 — compound changes and bounds under a prohibition (ADR 0044)

Task id: `R-2`. Base: the commit that adds this spec and ADR 0044 (proposed; the PR that carries this work ratifies it).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0044-a-change-on-several-axes-has-a-direction-only-when-they-agree.md`. Where this spec and the ADR
disagree, the ADR wins; stop and report.

## What to do

1. **Tests first** (envelope `red`), public seams (`req::compare::compare`, `Direction::effect`, the `req compare` and
   `req diff` CLI), oracles from the ADR and English:
   - "should … within 10 ms" → "shall … within 100 ms" is `scope_divergence`; `req diff` classes it `reshaped`;
   - "shall … within 100 ms" → "should … within 200 ms" is `modality_weakening` (agreeing), and `req compare` lists
     `numeric_loosening` among the moved axes;
   - a modality change plus a guard added is `scope_divergence` (decision 4);
   - "shall not be more than 3 s" → "… 4 s" is `numeric_loosening`, the reverse `numeric_tightening`; "shall not exceed
     200 ms" → "… 300 ms" stays `numeric_loosening` (absorbed negation, an obligation);
   - **every single-axis expectation already in `tests/` still holds** (decision 2), and both committed direction records'
     per-pair classes are unchanged — prove it by re-running `req bench` on `direction-v1.yml` and `direction-v2.yml` and
     diffing the records (they must be byte-identical).
2. **The classifier** (`src/compare.rs`): classify each axis independently, then combine per decisions 1–5. Keep
   `Direction`'s variants and their strings unchanged. Add the per-axis list as a public function (e.g. `axes(a, b) ->
   Vec<Direction>`), used by the CLI output (decision 6).
3. **`req compare` / `req diff` output** (`src/compare.rs` `run`, `src/diff.rs`): the `also:` line(s) in text; `axes` in
   `req.diff/1` JSON (additive; `tests/json_format.rs` will need the new key). Help pages in `src/main.rs` say it in one
   sentence each.
4. **The gold set** (`tools/corpus/build_direction.py`, `--version 3`; v1 and v2 must regenerate **byte-identically**):
   `direction-v3.yml` = v2 plus (a) compound families — at least modality×bound in all four sense combinations and
   modality×guard — labelled by ADR 0044 decisions 3–4 from the two mutations' labels; (b) numeric mutations on statements
   whose modality is a prohibition, labelled with the swapped sense (decision 5), or skipped where the source has none — say
   which. Record `bench/results/direction-v3-rules-v9.json`; add its bench to the full `scripts/check.sh` beside v1/v2 (and
   to `tests/baseline.rs`'s plan list, as S-1 did for v2). The false-carry measurement stays on v2 (ADR 0040's note).
5. **Docs, one home each.** `docs/adopting.md`'s `req diff` class table (`reshaped` gains "a change on several axes that
   point different ways"); `docs/canonical-hashing.md` §6.1 item 3 and *Recorded runs* (v3 and its numbers); ADR 0044 gets a
   dated implementation note with the measured v3 numbers; ADR 0026 and ADR 0030 each gain one "Amended by ADR 0044" line;
   `CONTEXT.md` only if a new term is needed (none is expected).

## Mutation checks (envelope `mutations`)

- Return at the first moved axis again → a compound test fails.
- Drop the prohibition swap for bounds → the "shall not be more than" test fails.
- Make the generator label a compound pair by its first mutation only → the regenerated v3 differs from the committed one.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home), exit 0, no SKIP, tree clean after the run; v1/v2 records
byte-identical before and after.

## Allowed paths

`src/compare.rs`, `src/diff.rs`, `src/main.rs` (help sentences), `tests/**`, `tools/corpus/build_direction.py`,
`bench/corpus/direction-v3.yml` (new), `bench/results/direction-v3-rules-v9.json` (new), `scripts/check.sh` (the v3 bench
step only), `docs/adopting.md`, `docs/canonical-hashing.md`, `docs/adr/0044-…` (the implementation note only),
`docs/adr/0026-…`, `docs/adr/0030-…` (one line each).

Forbidden: `src/canon.rs`, `RULES_V`, `bench/corpus/direction-v1.yml`, `bench/corpus/direction-v2.yml`, every existing
`bench/results/*` record, `src/satisfy.rs`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `Cargo.toml`,
`Cargo.lock`.
