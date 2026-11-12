# 0049. A superseded claim is withdrawn where its successor applies

Decides ADR 0043's *Not decided here* item "`supersedes` as a resolution". No `RULES_V` bump, no address moves, no
claim-schema change, no Baseline envelope change. `supersedes` is already in the relation vocabulary (`CONTEXT.md`
*Relation*); this decision gives it a meaning.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it, together with the code (task
W-1). Asked for by the owner on 2026-11-12. Date proposed: 2026-11-12.

## Context

The loader accepts `relations: [{type: supersedes, to: REQ-X}]`, because relation types are free strings, but nothing
reads it. A superseded claim keeps applying: it is gated, rendered, traced and signed like any live claim.

Since ADR 0043's detector was admitted (PR #60), that is a defect and not only noise. Say REQ-0050 replaces REQ-0012 and
moves a bound from "at least 3" to "at most 2". Both claims apply in the same release, they admit no common value, and the
gate FAILs on an unrecorded contradiction. The author has two ways out, and neither resolves anything:

- delete REQ-0012, which loses the losing side a Conflict is meant to keep (`CONTEXT.md` *Conflict*);
- record `conflicts-with`, which leaves the release UNVERIFIABLE until one claim is edited away.

The author meant something narrower than either: "REQ-0050 is the requirement now; REQ-0012 is history."

## Decision

1. **Direction.** `supersedes` is authored on the **successor** and points at the claim it replaces, like every other
   relation: stored once, on the outgoing side (REQ-0020). "REQ-0050 supersedes REQ-0012" is written in REQ-0050's file.
2. **Withdrawal is per selection.** A claim's applicability in a selection is its own predicate's answer, then folded with
   each of its successors' **own** answer (the successor's predicate, not the successor's already-folded state):
   - any successor **applies** → the claim is **withdrawn**: it does not apply, and it is reported as superseded by that
     successor;
   - otherwise, any successor is **undecided** and the claim's own answer is not *does not apply* → the claim is
     **undecided**, because whether it still binds depends on a predicate the context does not decide;
   - otherwise, every successor does not apply → the claim keeps its own answer.

   A successor scoped to one variant therefore supersedes only there (ADR 0003: variants are selectors). Using the
   successor's own answer makes a chain work without recursion: if C supersedes B and B supersedes A, and both C and B
   would apply, both A and B are withdrawn.
3. **Withdrawn is not deleted.** A withdrawn claim stays in the corpus, keeps its address, and is listed wherever a view
   lists what it left out. The losing side is preserved, as for a Conflict. Every view and policy that consumes the
   selection's applying claims sees it as not applying: the gate's policies, `req check`, the Baseline's applying set, the
   audit packet, `render`, `trace`, `impact`, `select`, `req diff`'s contradiction pass, and the completeness step. There
   is one evaluation, so these cannot disagree.
4. **Supersession resolves a Conflict.** A recorded `conflicts-with` between a withdrawn claim and its successor no longer
   joins two applying claims, so the contradictions policy no longer reports it (ADR 0043 decision 4). The same holds for
   a Tier-1 contradiction between them.
5. **A cycle is a defect.** A claim that supersedes itself, or a `supersedes` cycle of any length, is **FAIL** under
   `relations_resolve` in every selection, naming the cycle in id order. Within a cycle nothing is withdrawn: each member
   keeps its own answer, so a defect never makes claims vanish.
6. **A dangling `supersedes` withdraws nothing.** The `to` is not in the corpus, so there is nothing to withdraw, and
   `relations_resolve` already reports the edge as UNVERIFIABLE when the successor applies. The edge is still reported
   when the successor does not apply, because an edge that silently names nothing is decay.
7. **Shapes are additive, and only appear when used.**
   - A view's JSON row gains `superseded_by: [ids]` (sorted, empty when none). `applies` keeps its three values: a
     withdrawn claim is `does_not_apply`.
   - Text views add a *Superseded* line or section **only when some claim is withdrawn**, so a corpus without
     `supersedes` renders byte for byte as before. That keeps this repository's committed projections, and its three
     signed Baselines, unchanged.

## Consequences

- An author replaces a requirement by adding the new claim with one relation. The release gates on the new claim alone,
  and the old one stays readable as history.
- The contradiction an author hits when they replace a bound has a resolution that keeps both sides.
- A Baseline signed before a successor is added still verifies against its own commit. At a later commit, the applying set
  changes when the successor lands, and that is a meaning change a human re-signs, which is the point.
- `select` and `render` can show a claim as not applying for a reason that is not in its own file. The view names the
  successor, so the reader can find the reason.

## Rejected

- **Delete the superseded claim.** It loses the losing side, and with it the history `req diff` and the audit packet show.
- **A `status: superseded` field on the old claim.** It duplicates the relation in a second place, and it is the wrong file:
  the decision to replace belongs to the new claim. It would also be a claim-schema change.
- **Withdraw everywhere, whatever the successor's applicability.** A successor scoped to one variant would silently remove
  the old claim from variants the successor never reaches. That is a requirement deleted without a replacement.
- **Fold through the successor's folded state.** That needs a recursion order, and in a chain A ← B ← C a withdrawn B would
  bring A back.
- **A fourth `Applies` value.** Every consumer of the tri-state would have to learn it, and the document shapes would
  break. *Does not apply* plus a named successor says the same thing additively.

## Not decided here

- **`req diff` flagging a supersession.** A newly withdrawn claim is visible today only through its successor's added
  relation and any resolved contradiction. A flag of its own is a follow-up.
- **`req satisfy` links to a withdrawn claim.** A design element that still satisfies a withdrawn claim is likely stale.
  Whether that is reported, and how, is its own decision.
- **Applying claims that relate to a withdrawn one** (`refines`, `derives-from`). Today a relation to a claim that does not
  apply passes. This decision keeps that, and treats a withdrawn claim like any other claim that does not apply.

## Falsification

This decision is wrong if any of these is observed:

1. a claim whose successor applies in a selection is gated, signed or rendered as applying there;
2. a claim whose successor does not apply in a selection is withdrawn there;
3. a claim whose successor is undecided is reported as applying, or as not applying, instead of undecided (when its own
   answer is not *does not apply*);
4. a `supersedes` cycle passes the gate, or withdraws any member of the cycle;
5. two views of the same selection disagree on whether a claim is withdrawn;
6. a corpus with no `supersedes` relation produces different bytes in any committed projection or Baseline manifest.

## Implementation note — 2026-11-13 (W-1: the code)

Decisions 1–7 are implemented as written; nothing in them moved.

- **One evaluation, in the loader.** `view::load_rows` folds each claim's own answer with its successors' own answers in a
  second pass (`withdraw_superseded`), after every row's `applies` is known. `view::supersession` reads the relations once
  (ordered maps and sets, no hash iteration) and returns the successors of each claim and every cycle; the loader, the gate and
  `select` share it. `Row` gains `superseded_by` (sorted; empty unless withdrawn).
- **`select` now reads the loader's rows.** It used to evaluate predicates on its own, which would have left it the one view
  that did not know a claim was withdrawn. Its output is unchanged without a `supersedes` relation.
- **Cycles.** Every claim with a path back to itself is excluded from the fold as a successor and as a target. Each cycle is
  named once (the shortest cycle through its lowest member, rotated to start at its lowest id), so a component with chords is
  named by every distinct cycle the members need, and no member goes unnamed. `relations_resolve` FAILs naming each in every
  selection, whether or not its members apply.
- **Dangling.** `relations_resolve` lists a `supersedes` whose `to` is missing as UNVERIFIABLE even when its claim does not
  apply; every other relation kind keeps the applying-only rule.
- **Text.** `render` (outside-the-selection list), `trace` (`[target …]`), `select` and `impact` say `does not apply
  (superseded by REQ-X)` for a withdrawn claim, and the audit packet has a *Superseded* section, each only when a claim is
  withdrawn. With no `supersedes` relation the only byte that moves is `render --format json`'s `superseded_by: []` on each row;
  `docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and verify.
- **Not built, as decided:** `select` has no `--format json`, so there is no JSON `select` row to carry the key; `req diff` still
  names a supersession only through its successor's added relation and the resolved contradiction.
- **Verified.** The five mutations each fail a test: folding through the successor's folded state (the chain test),
  withdrawing whatever the successor's applicability (the variant-scoped test), treating an undecided successor as not
  applying (the undecided-successor test), withdrawing within a cycle (the cycle tests), and computing withdrawal in the gate
  alone (the view-agreement test and eleven others).
