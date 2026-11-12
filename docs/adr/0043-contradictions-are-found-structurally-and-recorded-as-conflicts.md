# 0043. Two applying claims that cannot both hold are found structurally, and a known one is recorded as a Conflict

Extends ADR 0003 (variants are selectors), ADR 0007 (collapse tiers), ADR 0021 (a provider proposes one pair) and ADR 0039
(structural completeness). Nothing is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-11, GitHub PR #53). Asked for by the owner on
2026-11-11 ("check also in design any remaining gaps"; `docs/design.md` §4 row 8), and sequenced by the owner after the
unrelated claims were resolved. Date proposed: 2026-11-11. This ADR decides the contract; the code follows in its own pull
request, test first, with the measurement in decision 6.

Extended by [ADR 0049](0049-a-superseded-claim-is-withdrawn-where-its-successor-applies.md): `supersedes` resolves a Conflict by withdrawing the superseded claim where its successor applies (the *Not decided here* item below).

Extended by [ADR 0045](0045-req-diff-names-the-contradictions-a-change-introduces.md): `req diff` names the contradictions a change adds or resolves (the follow-up *Not decided here* names below).

## Context

`docs/design.md` §4 row 8, *contradictions found in the field*, is **not built**: nothing tells an author that REQ-A says
"within 100 ms" while REQ-B, applying in the same release, says "in at least 200 ms". `CONTEXT.md` defines a **Conflict**
— "two claims that cannot both hold in the same scope, recorded as a first-class object that preserves the losing side; it
is information, not an error" — and the relation vocabulary already lists `conflicts-with`, but nothing reads either.

Contradiction in general is undecidable from prose: "the pump shall stop" and "the drive shall remain energised" contradict
only through physics. What Tier 1 *can* decide is narrow and exact: two claims about **the same subject, action, qualifier
and unit**, under nested conditions, whose obligations admit **no common value**. That is the same slot-equality the meaning
fingerprint already rests on, so it inherits the fingerprint's measured property (FMR = 0) rather than inventing a new one.

## Decision

1. **Comparable pairs.** Two claims are comparable when both are structured, both **apply in the same selection** (a gate
   invocation, or a `.req/variants` line under `req check` — never the 150% model, where variants that never ship together
   would collide), and their canonical forms have equal `subject`, `action`, `qualifier`, `unit`, `exception` and
   `temporal`, and one claim's `conditions` are a subset of the other's (so a situation exists where both bind). Pairs are
   bucketed by those fields, so the search is near-linear, not quadratic.
2. **A comparable pair is a contradiction exactly when no value satisfies both obligations**, decided on exact decimals,
   never floats:
   - **polarity** — one `SHALL`, the other `SHALL NOT`, with the same operator, value and members;
   - **bounds** — both `SHALL`, and the feasible sets are disjoint: an upper bound below a lower bound (`≤ 100` and `≥ 200`),
     strictness at the meeting point (`≤ 100` and `> 100`), two different exact values, or an exact value outside the other
     claim's bound;
   - **membership** — both `SHALL` with disjoint `IN` sets, or `SHALL IN S` against `SHALL NOT IN T` with `S ⊆ T`.
   `SHOULD` and `MAY` never contradict at Tier 1 (a recommendation or a permission does not exclude a value); different
   conditions that are not nested, different units after canonical conversion, and unstructured claims are **not
   comparable** and are counted as such, never guessed.
3. **A recorded Conflict is the human's statement of a contradiction Tier 1 cannot see — or of one it can.** A claim may
   carry `relations: [{type: conflicts-with, to: REQ-X}]`. It is a Conflict object in the glossary's sense: authored,
   reviewed, preserving both sides.
4. **The gate gains a policy, `contradictions`, over the selection's applying claims:**
   - a Tier-1 contradiction **not** recorded as a Conflict is **FAIL** — a defect nobody has looked at, named with both
     claim ids, the slot and the two values;
   - a recorded Conflict whose two claims both apply is **UNVERIFIABLE** — information, not an error, as `CONTEXT.md` says,
     but a release cannot be shown to satisfy both sides of it (REQ-0041). It is resolved by an edit, a retirement, or an
     applicability predicate that keeps the two claims out of the same release, never by a flag;
   - otherwise **PASS**, worded for what was examined: "no structural contradiction among *k* comparable pairs; *m* pairs
     not comparable". The policy never says the corpus is consistent.
   `req check` inherits it per release line, and the audit packet gains a section listing the contradictions and Conflicts
   per Baseline.
5. **A provider may propose a Conflict, never record one.** `req propose` (ADR 0021) may put a pair to a model with the
   question "can both hold?"; its answer is an unratified proposal, and the human records `conflicts-with` or does not
   (ADR 0004). Nothing in this decision depends on a model.
6. **The rule is measured before it gates.** Two counts, labels never from the detector:
   - **false contradictions = 0** on every pair labelled *equivalent* (parity and external corpora: two statements of the
     same meaning cannot contradict) and on every `numeric_loosening` / `numeric_tightening` / `membership_*` /
     `scope_*` pair of the direction gold set (nested sets, never disjoint) — the analogue of FMR = 0, a hard bound;
   - **recall** on the direction gold set's `polarity_flip` pairs whose modality is `SHALL`, which are contradictions by
     construction — reported, not gated, with the count of pairs the parser could not make comparable.
   `req bench` reports both; the policy ships disabled (reported, not judged) until the first count is zero on every
   corpus.

## Not decided here

- **Cross-subject contradictions** (through an alias the registry does not hold, or through physics) — Tier 2 at most.
- **`supersedes` as a resolution** — what it means for a superseded claim to keep applying is its own decision.
- **Contradictions introduced by a change** — `req diff` flagging the pull request that creates one is a follow-up once the
  policy exists.
- **Permission against prohibition** (`MAY X` against `SHALL NOT X`) — a deontic tension whose reading depends on intent,
  as ADR 0041 found for guards on permissions; reported nowhere until decided.

## Consequences

- An author learns of a contradiction in the claim set at the gate, with the two claims and the two values named, instead of
  in the field.
- The Conflict object becomes real without a new file type: it is a relation, attested by the Baseline's relations hash
  like every other edge, so recording or deleting one after signing is visible.
- The detector is exactly as wide as Tier-1 parsing: claims the parser leaves unstructured or reads with different slots are
  invisible to it, and the policy's PASS line says how many pairs that was.
- Cost: one more gate policy (seven, with ADR 0040's still unbuilt), a bench count, and a bucketing pass over the corpus.

## Rejected

- **Comparing across the 150% model.** Two variants that never ship together may legitimately contradict ("the EU line
  stores data in the EU; the US line in the US"). Contradiction is a property of a selection.
- **FAIL for a recorded Conflict.** `CONTEXT.md` defines a Conflict as information. Recording one is the honest act; making
  it as severe as an unseen contradiction would punish recording.
- **A model as the detector.** Its false contradictions would be noise at the gate, and its misses would read as consistency.
  It may propose (decision 5); it never judges.
- **Treating SHOULD as SHALL for contradiction.** A recommendation that cannot be met alongside an obligation is a design
  tension, not a contradiction; flagging it would make the policy noisy before it is trusted.

## Falsification

This decision is wrong if any of these is observed:

1. a pair labelled equivalent, or a nested-set direction pair, is reported as a contradiction;
2. two claims that never apply in the same selection are reported as contradicting;
3. a recorded Conflict between two applying claims lets the policy PASS;
4. the policy's PASS line omits the count of pairs it could not compare;
5. any part of the decision is made by a model rather than proposed by one.

## Implementation note — 2026-11-12 (C-1: the code)

Decisions 1–4 and 6 are implemented (`src/contradict.rs`; the policy in `src/gate.rs`; the audit section in `src/audit.rs`;
the measurement in `src/bench.rs`). Decision 5 needed no code: `req propose` already puts one pair to a provider and returns an
unratified proposal, and the "can both hold?" question is **not** added here; `req diff` flagging a contradiction a change
introduces stays the follow-up *Not decided here* names.

- **Measured, then admitted.** Every `bench/results/*-rules-v9.json` now carries a `contradiction` block (that key and nothing
  else changed). **False contradictions: 0 on all eight corpora** — parity 0/12 scored pairs, external-v1 0/178, v2 0/178,
  v3 0/499, v4 0/499, direction-v1 0/533, v2 0/548, v3 0/992 (labelled-equivalent pairs plus the `numeric_*` / `scope_*`
  direction pairs; the direction corpora hold no `membership_*` pair, so the membership arm of the bound is carried by
  `tests/contradict.rs` and not by the corpora). **Recall** on the `polarity_flip` pairs whose earlier claim is a `SHALL`:
  176 of 176 on each direction corpus, none not comparable (reported, not gated). So `DETECTOR_ADMITTED` is `true`: an
  unrecorded contradiction is FAIL. The constant is compiled in and pinned to the records by a test, so a rules bump or a
  corpus that breaks the bound fails CI until it is changed on purpose; the policy never reads a record at runtime.
- **The recorded-Conflict half was judged from the first unit**: a `conflicts-with` between two applying claims, or naming a
  claim the corpus does not hold, is UNVERIFIABLE with no detector involved.
- **A prohibited bound is read for what it leaves.** `SHALL NOT GT 100` obliges `LE 100` (ADR 0044 decision 5 reads the
  forbidden region the same way), so two prohibitions, or a `SHALL` and a `SHALL NOT` with different operators, are judged
  as two `SHALL`s on what they leave: "shall not take more than 100 ms" against "shall not take less than 200 ms" is a
  contradiction (review C-1-review-1 found the first version calling every pair of prohibitions consistent).
- **Rules without a Tier-1 verdict are not comparable, not guessed.** A membership against a bound, two forbidden sets, and
  a bare prohibition against a bound are counted as not comparable. Two bare prohibitions are consistent (abstaining
  satisfies both), and so is a bare action against anything but its negation.
- **Every applying pair is counted.** *m* in the PASS line is every pair of applying claims that was not comparable: a
  different slot (another bucket), an unstructured claim, or a pair the rules leave undecided — so a selection whose claims
  share no slot says so (review C-1-review-1 found the first version counting only pairs inside a bucket). A FAIL still
  names the recorded Conflicts of the same selection beside the contradictions.
- **A finding about the parser, not the detector.** The parse keeps a leading preposition in the *action*: `respond within
  100 ms` has action `respond` while `respond in at least 200 ms` has action `respond in`, so the two sentences this ADR's
  *Context* opens with differ in the action slot and are **not comparable** at Tier 1 today (the same pair is found when both
  read `in no more than` / `in at least`, or both `within` / `at least`). The detector is exactly as wide as the parse, as
  the *Consequences* say, and the PASS line counts such pairs; widening the parse is a rules change (`RULES_V` bump, goldens
  regenerated) and is not made here.
- **Not in the audit's JSON.** The *Contradictions and Conflicts* section is in the audit packet's text; `req.audit/1` is
  unchanged. The gate's `req.gate/2` carries the policy additively (`checked` = comparable pairs, `details` = the
  contradictions and Conflicts).
