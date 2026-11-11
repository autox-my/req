# C-1 — structural contradictions and recorded Conflicts (ADR 0043, the code)

Task id: `C-1`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-12 ("next is to build ADR 0043
(contradiction detection)").

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0043-contradictions-are-found-structurally-and-recorded-as-conflicts.md` (decisions 1–6 and its
*Falsification* list). Where this spec and the ADR disagree, the ADR wins; stop and report.

## Choices fixed here

1. **The module.** `src/contradict.rs` (new), two public seams:
   - `contradicts(a: &CanonicalForm, b: &CanonicalForm) -> Pair` where `Pair` is `NotComparable(reason)`,
     `Consistent` or `Contradiction { slot, a, b }` (the slot and the two values as the report prints them) — decisions
     1–2 exactly, on `Decimal`, never floats. Comparable: both structured; equal `subject`, `action`, `qualifier`, `unit`,
     `exception`, `temporal`; one `conditions` set a subset of the other (the empty set is a subset of every set). Both
     modalities must be `SHALL`/`SHALL NOT`; `SHOULD`, `MAY` or no modality is not comparable (reason named).
   - `find(rows) -> Findings` over a selection's applying claims: bucket by the six equality fields (a `BTreeMap`, no
     `HashMap` iteration), compare within a bucket, return the contradictions (sorted by the two claim ids), the count of
     comparable pairs, the count of pairs in a bucket that were not comparable, and the recorded Conflicts (below).
2. **Bounds** (decision 2): treat each `SHALL` bound as an interval over exact decimals (`LE`/`LT` upper, `GE`/`GT` lower,
   `EQ` a point, open or closed ends from the operator); a contradiction is an empty intersection. A claim with no operator
   and no members (a bare action) only contradicts by polarity. A pair with one `IN` and one bound is not comparable.
3. **Recorded Conflicts** (decision 3). A relation `{type: conflicts-with, to: REQ-X}` on either claim records the
   Conflict for the unordered pair; recorded on both sides it is still one Conflict. A `to` the corpus does not hold is
   UNVERIFIABLE, named (a dangling Conflict cannot be shown resolved). `Findings` lists each recorded Conflict whose two
   claims both apply in the selection.
4. **The gate policy `contradictions`** (decision 4) — the eighth, before the invocation policy (which stays last):
   - a recorded Conflict between two applying claims → **UNVERIFIABLE**, naming both ids. This half needs no detector and
     is **judged from this unit on**;
   - a Tier-1 contradiction not recorded as a Conflict → **FAIL**, naming both ids, the slot and the two values — **only
     when the detector is admitted** (choice 5); until then each one is listed in the policy's details as *reported, not
     judged* and does not change the outcome;
   - otherwise **PASS**, worded as the ADR says: "no structural contradiction among *k* comparable pairs; *m* pairs not
     comparable" (plus, while not admitted, "detector reported, not judged (ADR 0043 decision 6)"). Never "consistent".
   `req check` inherits it per release line. `checked` carries *k*. The JSON policy entry carries the contradictions and
   Conflicts as offenders, additively.
5. **Admission is compiled in and pinned to the measurement** (the S-2 pattern). `DETECTOR_ADMITTED: bool` in the module
   is `true` exactly when every `bench/results/*-rules-v<RULES_V>.json` record that carries the new block reports
   `false_contradictions: 0`. A test reads the records and asserts equality, so a rules bump or a corpus that breaks the
   bound fails CI until the constant is changed deliberately. Never read a record at runtime.
6. **The measurement** (decision 6), in `src/bench.rs`, an additive `contradiction` block in each record:
   - **parity and external corpora:** every pair labelled equivalent is run through `contradicts` as two `SHALL`-applying
     claims; `false_contradictions` counts those reported as a contradiction, `comparable` and `not_comparable` count the
     rest; the pairs list names each false contradiction (empty is the expectation);
   - **direction gold sets (v1, v2, v3):** `false_contradictions` over every `numeric_loosening`, `numeric_tightening`,
     `membership_*` and `scope_*` pair; `recall {found, of, not_comparable}` over the `polarity_flip` pairs whose before
     modality is `SHALL` (reported, not gated). The direction corpora's other blocks are untouched.
   Regenerating the rules-v9 records adds only this block: the diff of every existing `*-rules-v9.json` must be the new
   key and nothing else (show it). No v8-or-older record changes. No new corpus file.
7. **The audit packet** (decision 4): a *Contradictions and Conflicts* section per Baseline listing the policy's
   contradictions (with the not-judged note while not admitted) and the applying recorded Conflicts; "none found among *k*
   comparable pairs, *m* not comparable" when empty.
8. **Out of this unit:** `req propose`'s "can both hold?" question (decision 5 — the existing `propose` path already
   proposes only; leave it, and say so in the implementation note); `req diff` flagging a new contradiction; anything in
   *Not decided here*.
9. **Docs, one home each.** The `req gate` and `req check` help pages (the policy in one sentence, the count eight);
   `docs/adopting.md` (a short "Contradictions and Conflicts" paragraph: how to record `conflicts-with` and how it is
   resolved); the `AGENTS.md` command table's gate row ("eight policies"); `docs/canonical-hashing.md` §6.3 (the
   false-contradiction bound beside FMR, one paragraph) and *Recorded runs* (the numbers); ADR 0043 gets a dated
   implementation note with the measured numbers and whether the detector was admitted. `CONTEXT.md` already has
   *Conflict*.

## Tests first (envelope `red`)

Public seams (`contradicts`, `find`, the gate JSON, the CLI exit codes), oracles from the ADR and arithmetic, never from a
run. At minimum one test per ADR *Falsification* item (1–5; for 5, assert the module and the gate path call no provider),
plus: `≤ 100` vs `≥ 200` contradicts; `≤ 100` vs `> 100` contradicts and `≤ 100` vs `≥ 100` does not; two different `EQ`
values contradict; `EQ 150` vs `≤ 100` contradicts; disjoint `IN` sets contradict; `SHALL IN S` vs `SHALL NOT IN T` with
`S ⊆ T` contradicts and with `S ⊄ T` does not; `SHALL` vs `SHALL NOT` same slot contradicts; `SHOULD` against `SHALL` is
not comparable; non-nested conditions are not comparable; nested conditions are comparable; two claims in different
`.req/variants` lines that contradict are not reported by `req check`; a recorded Conflict on one side only is found once;
a dangling `conflicts-with` is UNVERIFIABLE; the PASS line names both counts; the admission test of choice 5. Prove them red
at the base.

## Mutation checks (envelope `mutations`)

- Treat `≤ 100` vs `> 100` as overlapping (drop strictness at the meeting point) → a test fails.
- Drop the conditions-subset requirement → the non-nested test fails.
- Let a recorded Conflict PASS → the falsification-3 test fails.
- Compare across all claims instead of the selection's applying ones → the falsification-2 test fails.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home), exit 0, no SKIP, tree clean after the run, the three committed
Baselines VERIFIED. If this repository's own claims produce a contradiction or a recorded Conflict, **stop and report** —
do not edit a claim, add a relation or an applicability predicate to make the gate pass. If a new `check.sh` step is added
(none is expected), stub it in the fixtures that drive `check.sh`.

## Allowed paths

`src/contradict.rs` (new), `src/lib.rs`, `src/main.rs` (help pages), `src/gate.rs`, `src/check.rs`, `src/audit.rs`,
`src/bench.rs`, `tests/**`, `bench/results/*-rules-v9.json` (the additive block only), `docs/adopting.md`,
`docs/canonical-hashing.md` (§6.3 and *Recorded runs*), `AGENTS.md` (the gate row only),
`docs/adr/0043-contradictions-are-found-structurally-and-recorded-as-conflicts.md` (the implementation note only).

Forbidden: `src/canon.rs`, `src/compare.rs`, `src/satisfy.rs`, `RULES_V`, `bench/corpus/**`, every
`bench/results/*-rules-v8.json` and older, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**` of this
repository, `tools/**`, `Cargo.toml`, `Cargo.lock`, `.github/**`.
