# 0044. A change on several axes has a direction only when every axis agrees

Amends ADR 0026 §2 (the `weakened` / `tightened` mapping), ADR 0030 §6 (the prohibition sense swap, extended from sets to
bounds) and ADR 0041 (the effect of a scope class). No class is renamed, no `RULES_V` bump (the classifier is not on the
address path), no claim-schema or Baseline change.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it, together with the fix (task R-2;
2026-11-12, GitHub PR #58).
Asked for by the owner on 2026-11-11 ("yes": write the amendment and the fix after R-1 merged). Date proposed: 2026-11-11.

## Context

`compare` (`src/compare.rs`) checks the axes of a change in a fixed order — polarity, modality, conditions, qualifier and
unit, then the bound or the set — and **returns at the first axis that moved**. Two defects follow, both the dangerous
confusion (a loosening reported as no change or as a tightening), both found on 2026-11-11 and neither measured by the
direction gold set, whose every pair carries exactly one mutation:

1. **Compound changes.** "The system should respond within 10 ms." → "The system shall respond within 100 ms." strengthens
   the modality and loosens the bound tenfold; `req compare` reports `modality_strengthening`, "permits less than before",
   and `req diff` classes it `tightened`, so `--fail-on weakened` lets it through. Found by review S-2-review-1, where the
   same shadowing let a satisfaction link carry a compound edit (fixed locally in `src/satisfy.rs`, PR #55).
2. **A bound under a prohibition.** "The system shall not be more than 3 s late" parses as `SHALL-NOT`, `GT 3`. The
   forbidden region is "more than 3"; raising it to "more than 4" forbids less, a loosening. `numeric_direction` ignores the
   modality and reports `numeric_tightening`. ADR 0030 already swaps the senses for a forbidden *set*; bounds were missed.
   Found by review R-1-review-1.

## Decision

1. **Every axis is classified, not only the first.** `compare` derives the change on each axis independently: modality
   (weakening / strengthening), conditions (narrowing / widening / divergence), bound (loosening / tightening), set
   (widening / narrowing), and the identity fields (qualifier, unit, exception, temporal: any change is divergence).
   Polarity stays first and alone: obligation ↔ prohibition is a different obligation whatever else moved.
2. **One axis moved → that axis's class**, exactly as today.
3. **Several axes moved → a direction only if every moved axis has the same effect** (`Direction::effect`, ADR 0041): all
   *permits more*, or all *permits less*. The reported class is then the first moved axis's class in the existing order (so
   "SHALL→SHOULD and 100→200 ms" is `modality_weakening`, and its effect — permits more — is right); otherwise, or when any
   moved axis has *no direction*, the class is `scope_divergence` (`req diff`: `reshaped`), so a human reads it. A
   compound change is never classed by one axis while another points the other way.
4. **A scope axis that moves together with the modality has no direction.** ADR 0041 reads a guard against the modality;
   when the modality itself changed there is no single modality to read it against, so the pair is `scope_divergence`.
5. **Under a prohibition the bound's senses swap**, as ADR 0030 §6 swaps a set's: for `SHALL NOT` with a bound, a change
   that would tighten the bound of an obligation loosens the prohibition, and the reverse. Membership keeps its existing
   swap. Absorbed negations (`shall not exceed 200` → `SHALL`, `LE 200`) are obligations and are unaffected.
6. **`req compare` and `req diff` name every moved axis.** Beside the class, the output lists each axis that moved with its
   own class (`also: numeric_loosening`), so a reader sees a compound change as compound even when its axes agree.
   `req diff`'s JSON gains the list (`axes`), additively.
7. **The gold set measures it.** A new corpus version carries the compound families — two mutations applied to one
   statement, labelled by the rule in decisions 3–4 from the mutations' own labels, never from the classifier — and numeric
   mutations on prohibitions, labelled with the swapped sense. The generator must label a numeric mutation under a
   prohibition with the swapped sense (or skip it); a label that ignores the modality would share the classifier's defect,
   as R-1 found for "no more than".

## Consequences

- `req diff --fail-on weakened` no longer misses a loosened bound hidden behind a strengthened modality; such a change is
  `reshaped`, which a repository can also fail on.
- Some changes that were classed by their first axis become `scope_divergence`: accuracy on compound pairs is reported
  against labels that say so, and no single-axis pair changes class (the existing corpora's numbers are unchanged).
- ADR 0040's carry guard in `src/satisfy.rs` (single-axis only) stays: it is now redundant for mixed effects but still
  needed for agreeing compound changes, which the false-carry measurement (single-axis) never covered.

## Rejected

- **A new class per compound shape** (`compound_loosening`, …): it multiplies the vocabulary, the gold-set labels and
  `req diff`'s classes for no safety gain; *agreeing* compounds already have a correct effect, and *mixed* ones need a human.
- **Reporting the "dominant" axis** (the larger move): there is no common scale between a modality and a bound.

## Falsification

This decision is wrong if any of these is observed:

1. a pair whose moved axes have opposite effects is reported with a direction (anything but `scope_divergence` or
   `polarity_flip`);
2. a single-axis pair's class changes from what it was before this decision;
3. a bound loosened under a prohibition is reported as a tightening, or the reverse;
4. the generator labels a compound or prohibition pair from the classifier's output.

## Implementation note (2026-11-11, R-2)

Implemented in `src/compare.rs` (`axes`, then `compare` combining per decisions 1–5; `Direction`'s variants and strings
unchanged), `req compare` / `req diff` (`also:`; `axes` in `req.diff/1`, additively) and the gold set
(`build_direction.py --version 3`, `bench/corpus/direction-v3.yml`). Two readings the text left open:

- A modality that appears or disappears (no rank to compare) is a `scope_divergence` entry on the modality axis, and
  `applicability` is an identity axis with the same entry. Neither changes a single-axis pair (the catch-all already
  answered `scope_divergence`); they change only compounds, which now have no direction where the other axis's class
  used to be returned.
- `also:` lists each moved axis's class less the first moved axis when the reported class is taken from it (the
  axes agree); a mixed compound (a synthesised `scope_divergence`) names all of its axes, a divergent one included
  (review R-2-review-1 found the first version dropping it).

Measured on `direction-v3` at rules v9 (`bench/results/direction-v3-rules-v9.json`): 1827 pairs, FMR 0/1548, FSR 0/199,
hard gate PASS, **0 dangerous confusions**, accuracy 0.9849 over 1587 classified (1563 correct). The 436 compound pairs
(370 scored) have 8 mismatches, all `scope_divergence` on two PROMISE statements the parser cannot read for the value slot.
The prohibition-bound family is 16 pairs from 4 bases, **none a native `shall not` statement** (the pinned sources hold
none with one bound; the bases are flipped SHALLs, said so in the corpus header): 8 correct, 8 `scope_divergence` on
those same two statements, so decision 5 is measured on two statements here, plus `tests/compare.rs`. The v1 and v2
records are byte-identical before and after (falsification 2 not observed on either), and the false-carry
measurement stays on v2.
