# R-1 — "no more than" is an upper bound (rules v9)

Task id: `R-1`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-11 (parser coverage for the numeric
direction pairs: "ok"); this unit is the defect that work found.

**Model:** writer `claude-sonnet-5-5` (the Tier-1 path: `p3-models.md`'s *hardest tasks*); reviewer `deepseek-v4.1-flash`,
read-only.

## The defect (measured on `main` at rules v8, 2026-11-11)

T1.5's comparator table (`COMPARATOR_PHRASES`, `src/canon.rs`) has `no less than` / `not less than` → GE but **not their
mirrors**. So `more than` (GT) matches inside them:

| statement | derived | should be |
| --- | --- | --- |
| "The system shall respond in no more than 3 seconds." | `op GT value 3 unit s` | `LE` |
| "… not more than 3 seconds." | `GT` | `LE` |

and `req compare` calls "no more than 3 s" → "no more than 4 s" **`numeric_tightening`, "permits less than before"** — a
loosening reported as a tightening, the dangerous confusion. Addresses did not falsely merge (the stray "no" stays in the
action, so "no more than 3" and "more than 3" differ), but the slot, a pin on it and every direction over it are wrong.

The gold set did not catch it because **the generator has the same substring bug**: `tools/corpus/build_direction.py` tests
`UPPER_WORDS` (which contains `less than`) and `LOWER_WORDS` (which contains `more than`) by substring, so "no more than"
is read as a lower bound and "no less than" as an upper one, and the labels agree with the parser's wrong reading. Two
pairs it produced are affected (`D-PR-P11:r545-NL/-NT`, labels swapped). Separately, `NUMBER_UNIT` matched `\92s` (an
escaped quote character followed by "s") as 92 seconds, producing `D-PR-P6:r310-NL/-NT`, which change no bound at all.

## What to do

1. **Tests first** (envelope `red`), public seams, oracles written from T1.5 and English, never from a run:
   - `canonicalize_statement` derives `LE` for "no more than", "not more than", "no greater than", "not greater than"
     (value and unit unchanged), and still `GT` for bare "more than" / "greater than", `GE` for "no less than";
   - `compare`: "no more than 3 s" → "no more than 4 s" is `numeric_loosening`; the reverse `numeric_tightening`;
   - "no more than 3 seconds" and "at most 3 seconds" (same subject and action) have **equal addresses** (E), and
     "no more than 3 seconds" vs "more than 3 seconds" **differ** (N);
   - add those two as hand-written parity pairs (an `E-` and an `N-` trap, in the existing parity style) so `req bench`
     carries them.
2. **The rule.** Add the four phrases to `COMPARATOR_PHRASES` → `Comp::Le` (longest match already wins). Nothing else in the
   table changes. Update T1.5 in `docs/canonical-hashing.md` (the closed table) in the same commit.
3. **Rules-change protocol** (AGENTS.md, `docs/canonical-hashing.md` §5): `RULES_V` 8 → 9; regenerate `req selftest`'s
   goldens deliberately (add one known answer for "no more than"); record the re-canonicalization candidates (claims in this
   repository that contain the phrases — expected none; say so); new `bench/results/*-rules-v9.json` records for every
   corpus the gate benches (the v8 records stay as history); the *Recorded runs* section gains the v9 numbers, including any
   external-corpus pair whose verdict moved, named.
4. **The generator's labels.** In `build_direction.py`: classify a bound by the **longest** matching phrase with the negated
   forms listed explicitly (`no more than`, `not more than`, `no greater than` upper; `no less than`, `not less than`,
   `no fewer than` lower), and do not read a number preceded by a backslash as a quantity. Regenerate `direction-v1.yml` and
   `direction-v2.yml`; the diff must be exactly the affected pairs (relabelled or dropped) — list them in the PR. Labels still
   come from the mutation; the fix is to the generator's reading of English, not to agree with the parser.
5. **Re-sign is the owner's.** The three Baseline manifests record `rules_v`, so after the bump re-emit them
   (`req baseline --at <now> --out baselines/<file> <selection>`, the selections as in `.req/variants` and
   `deployment_mode=air_gapped`), regenerate `docs/spec.md` and `docs/trace.md` (their headers carry `rules_v`), and **stop
   with status `blocked` and blocker "owner must sign three manifests"** once everything else is green. Do not commit
   unsigned manifests and do not push until the orchestrator says the signatures are in.

## Mutation checks (envelope `mutations`)

- Delete the four new table entries → the parity `N-`/`E-` traps and the compare test fail (show which).
- Revert the generator's longest-match → the regenerated v1 differs from the committed one in the affected pairs.

## Expected, not a target

`numeric_loosening`'s false-carry support stays far below 100 (17 labelled tightenings before this fix, at most a handful
more parse after it). Report the new number; do not chase it. More support needs more numeric source statements, which is
a different unit.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home) green except the three Baseline signature checks, which must fail
**only** with "signature does not verify" until the owner signs; `req selftest` passes; the tree is clean after a run.

## Allowed paths

`src/canon.rs` (the table only), `src/selftest.rs` (goldens), `tests/**`, `bench/corpus/parity.yml` (the two pairs),
`bench/corpus/direction-v1.yml`, `bench/corpus/direction-v2.yml` (regenerated only), `bench/results/*-rules-v9.json` (new),
`tools/corpus/build_direction.py`, `docs/canonical-hashing.md`, `docs/spec.md`, `docs/trace.md` (regenerated only),
`baselines/air-gapped.json`, `baselines/release-parity.json`, `baselines/release-parity-and-adversarial.json` (re-emitted
only).

Forbidden: every `bench/results/*-rules-v8.json` and older, `baselines/*.minisig`, `baselines/trust.yml`, `claims/**`,
`decisions/**`, `aliases/**`, `src/compare.rs` (the classifier is right once the slot is), `Cargo.toml`, `Cargo.lock`,
`rust-toolchain.toml`.
