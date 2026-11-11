# 0041. A condition added to an obligation weakens it

Amends ADR 0026 §2 (the `weakened` / `tightened` mapping) and ADR 0030 §6's last clause ("`is_loosening` includes the
widening"). No class is renamed, no corpus label changes, no `RULES_V` bump, no address moves.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it, together with the fix (2026-11-11, GitHub PR #50). Asked for
by the owner on 2026-11-11 ("write the ADR 0026 amendment and the fix next"), after ADR 0040's *Consequences* recorded the
finding. Date proposed: 2026-11-11.

## Context

`req compare` names a condition change by how often the claim **applies**: a guard added (`… when the tank is full`) is
`scope_narrowing`, a guard removed is `scope_widening`. Those names are right. The defect was one step later, where a class is
turned into a safety reading. `Direction::is_loosening` counted `scope_widening` as "permits strictly more than before", and
ADR 0026 §2 mapped `scope_widening` to `weakened` and `scope_narrowing` to `tightened`.

For an obligation that is backwards. "The pump shall stop within 2 s" binds in every situation; "… when the tank is full"
binds only in some, so strictly more behaviour complies. Probed on 2026-11-11 (rules v8): adding the guard printed
`scope_narrowing` and `req diff` classed it `tightened`; removing it printed "this change permits strictly more than before".
That is the dangerous direction under the asymmetric cost: a weakening shown to a ratifier as a tightening, and invisible to
`req diff --fail-on weakened`.

The direction gold set did not catch it because it scores the *class*, not the safety reading: its labels name the mutation
(`d_scope_narrowing`: "A guard added: the claim applies less often"), and the classifier agrees with them.

## Decision

1. **The scope classes keep their names and their meaning**: how often the claim applies. The classifier is unchanged, the
   gold-set labels are unchanged (they name the mutation, which is still true), and so is every recorded direction count.
2. **What a change does to what complies is a separate answer, and for scope it depends on the modality.**
   `Direction::effect(modality)` replaces `is_loosening` and answers *permits more*, *permits less* or *no direction*:
   - modality weakening, numeric loosening, membership widening: permits more; their mirrors: permits less (unchanged);
   - **SHALL, SHOULD, SHALL NOT** — an obligation, a recommendation or a prohibition binds less often when it applies less
     often: `scope_narrowing` **permits more**, `scope_widening` **permits less**;
   - **MAY**, or no modality: a permission obligates nothing, and a guard on it is either a narrower grant or no change to any
     obligation; which one is a reading of intent, so **no direction** is claimed;
   - polarity flip, scope divergence, unrelated, identical, unclassifiable: no direction.
3. **ADR 0026 §2 now reads from the effect.** `weakened` is *permits more* (modality weakening, numeric loosening, membership
   widening, a guard added to an obligation or a prohibition); `tightened` is *permits less* (the mirrors, a guard removed);
   a guard changed on a permission is `reshaped`, so a human reads it. `not-judged` is unchanged. The modality is the base
   side's; a scope class is only reached when both sides share it.
4. **`req compare` says it in words**: "this change permits strictly more than before", "this change permits less than
   before", or, for a guard on a permission, that the direction is for a human to read.
5. **"Only when" is not a guard added.** "Shall allow X only when Y" forbids X outside Y — a restriction. Tier 1 keeps "only"
   in the action, so that pair is `unrelated` (a different predicate, `reshaped`), never a scope direction. A test holds it
   there: if a later rule strips "only", that pair must not become a loosening silently.

## Consequences

- `req diff --fail-on weakened` now catches a requirement made conditional, which it missed before. A repository that
  relied on the old mapping will see condition additions move from `tightened` to `weakened` (and removals the reverse); no
  verdict changes elsewhere, because the gate, `req check` and the Baseline do not read the direction.
- ADR 0040 is unaffected: it carries no scope class whatever its effect (its decision 6).
- The direction gate's "dangerous confusion" (expected class read as no change or as its opposite) is unchanged and still
  scores classes; the effect mapping is held by `tests/direction_edges.rs` and `tests/diff.rs` instead, written from the
  deontic reading above and mutation-checked (swapping the two scope arms, dropping the modality condition, and removing
  SHOULD or SHALL NOT from the binding modalities each fail a test).
- **Named gap.** An "unless" guard is read as a condition like "when" (`extract_conditions`), so removing an exception is a
  strengthening and adding one a weakening — the same sense, by the same argument. A statement whose guard *enlarges* what
  it obligates ("shall also log when …") is not distinguishable from a restriction at Tier 1; it is classified as a guard.

## Rejected

- **Renaming the classes** (`scope_narrowing` → a "loosening" name). It would re-label 239 gold-set pairs whose labels are
  correct as descriptions of the mutation, and would still be wrong for MAY.
- **Calling every guard change `reshaped`.** Safe, but it would hide the most common real weakening — a requirement made
  conditional — behind "read it", which is the noise ADR 0040 argues against.
- **Reading a guard on MAY as a tightening.** Defensible under a closed-world reading of permissions, but a guess about
  intent; the asymmetric cost says to claim no direction rather than a possibly wrong one.
