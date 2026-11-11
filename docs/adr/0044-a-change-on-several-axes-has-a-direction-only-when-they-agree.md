# 0044. A change on several axes has a direction only when every axis agrees

Amends ADR 0026 §2 (the `weakened` / `tightened` mapping), ADR 0030 §6 (the prohibition sense swap, extended from sets to
bounds) and ADR 0041 (the effect of a scope class). No class is renamed, no `RULES_V` bump (the classifier is not on the
address path), no claim-schema or Baseline change.

**Status: proposed** — the owner ratifies it by merging the pull request that carries it, together with the fix (task R-2).
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
