# M1-4 — re-word the three dogfood claims M1-1 left unstructured or literal-subject

Task id: `M1-4`. Code baseline: `50e38f2` (rules v6; PR #73 M1-1, #77 M1-3 merged). Owner decision of
2026-11-03: apply the three re-wordings M1-1's PR (#73) proposed and did not apply. Claims are the
owner's wording; this unit applies the decision and verifies it through the tool, it does not
invent requirements.

## Why

Rules v6's coverage invariant (hashing spec T1.10) and whole-span subject matching (T1.7) fixed two
false merges the dogfood corpus carried, and left three claims in a degraded shape:

- **REQ-0020** ("…serialize each relation **exactly once**, on the outgoing side of its origin claim")
  is `unstructured: true`: `once` is not a number word, so the claim cannot be structured while the
  statement keeps it, and its authored `outgoing_side_once` annotation was dropped with it.
- **REQ-0004** and **REQ-0021** author `subject: the req tool semantic layer` as a literal, because the
  registry no longer matches a prefix. The literal works but the phrase is an unregistered subject.

## What to do

1. **Split REQ-0020.**
   - `REQ-0020` keeps its id, its relation (`derives-from REQ-0002`) and its test link
     (`req::view::incoming_relations_are_derived_from_the_outgoing_side`). Its statement becomes
     `The req tool shall serialize each relation on the outgoing side of its origin claim.` (verified
     in a throwaway corpus at M1-1 to stay structured). Re-fill it structured; `unstructured: false`.
   - A **new claim `REQ-0022`** carries the "exactly once" half. Try a value-shaped phrasing that
     `req canon` parses with **0 flags** (for example `…each relation in exactly one claim file.`);
     the only criterion is that `req canon` shows no contradiction or omission and that the
     statement says the same obligation. If no phrasing survives that, REQ-0022 is
     `unstructured: true` (REQ-0007's shape) with the statement `The req tool shall serialize each
     relation exactly once.`. Report which one, and the statements you tried, in the envelope
     summary. Relation: `refines REQ-0020`, plus `derives-from REQ-0002` if the corpus' convention
     for a sibling needs it; read `docs/claim-format.md` rather than guessing.
   - REQ-0022 is `evidence: machine` and needs a **real** linked test: one for which the strict bar
     in `tests/evidence.rs` (*"if this test failed, would the requirement be rejected?"*) holds. If
     no existing test meets it, write one named for the claim, red first, and add its id to
     `tests/linked-test-ids.txt`. `evidence: process` is not an escape for a machine-checkable
     obligation.
2. **Drop the head from REQ-0004 and REQ-0021.** The statements become
   `The req tool shall emit proposals for reviewer ratification.` and
   `The req tool shall never rewrite a claim or merge two claims automatically.`; `subject` becomes
   `REQ_TOOL`. **Registering `the req tool semantic layer` as an alias or a new subject is not
   done**: it is a collapse decision, and `aliases/**` is forbidden. Re-fill each; their authored
   slots (`attribute`, `operator`, `value`) must still agree with the derived parse. If `req canon`
   flags a contradiction or omission, fix the authored slot declaration per
   `docs/claim-format.md`; **do not touch the statement beyond the wording above**.
   Keep each claim's `rationale` as is; add nothing that reads as history rewritten.
3. **Derived fields only through the tool.** `req canon --fill` writes `canonical.form` and
   `address`; never type either (a hand-written digest is a critical finding). Run `req canon
   --verify` afterwards: 0 mismatches, 0 candidates.
4. **Record the address moves.** REQ-0004, REQ-0020, REQ-0021 move; REQ-0022 is new; the other 18
   must not. State old → new (12 hex) for each in the PR and add one bullet under "Recorded runs"
   in `docs/canonical-hashing.md` (a new entry; do not rewrite the rules-v6 entry). No Decision in
   `decisions/` names any of these addresses (checked on the baseline: only `DEC-0001`, about
   REQ-0016), so no Decision is orphaned; re-check and say so.
5. **Corpus-size statements.** `tests/loader.rs` pins the corpus ids (REQ-0001..REQ-0021) and says so
   in its message: add REQ-0022 there and nowhere else in that test. Update `docs/claim-format.md`
   (the "REQ-0001 … REQ-0021 … 21 verified" paragraph and the REQ-0020/0004/0021 sentence, which
   will no longer be true) and `AGENTS.md`'s single "(15 applying, 6 undecided)" phrase to the real
   counts from `req render`. `docs/design.md`'s "15 applying + 6 undecided = its 21 files" is a
   dated measurement: leave it and say so.
6. **The hand-written parity pairs stay as they are.** `bench/corpus/parity.yml` `N-008`/`N-009` and
   the constants in `tests/canon_coverage.rs` quote the *old* wordings on purpose (they are the
   labelled regression pairs); do not edit either. `tests/canon_coverage.rs` must stay green
   untouched.
7. **Regenerate the projection** `req render --out docs/spec.md`, then `req render --check
   docs/spec.md` exits 0.
8. **The signed Baseline is the owner's step**, exactly as in M1-1. Claim statements moved addresses,
   so `baselines/air-gapped.json` no longer matches. Emit the replacement manifest with `req
   baseline --at <now, RFC 3339 UTC> deployment_mode=air_gapped` (the variant `baseline --verify`
   currently reads from the committed manifest; confirm it), write it to
   `baselines/air-gapped.json`, commit it, and **stop**: signing is `minisign -Sm`, a human act, and
   `baselines/air-gapped.json.minisig` is not yours. Report the gate honestly: the one blocking step
   is the Baseline verify (and the two tests that verify the committed signature); nothing folded
   into a pass. CI's gate is therefore red on this PR until the owner signs and pushes the
   signature, which is the same sequence as #73.

## Mutation checks (envelope `mutations`)

- Re-add the head `semantic layer` to the REQ-0004 statement in a scratch copy → `req canon` reports
  the literal-subject state again (the subject no longer `REQ_TOOL`); show the check you used.
- Delete REQ-0022's linked test → the gate's `tests_linked` answers UNVERIFIABLE/FAIL for it
  (`req gate --test-report`); show it.
- Swap `outgoing` for `incoming` in REQ-0020's statement in a scratch copy → the address moves
  (this is the property the split must not lose).

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run. Expected non-green: **only** the
Baseline verify and the tests that verify the committed signature, for the reason in item 8. Every
other step PASS with no SKIP. Report per corpus (never pooled) that parity and external-v1..v4 are
unchanged: these edits move claim addresses only, not a bench pair, so every `bench/results` record
must come out identical; any difference is a defect, and no `bench/results` file is yours to
change.

## Allowed paths

`claims/REQ-0004.yml`, `claims/REQ-0020.yml`, `claims/REQ-0021.yml`, `claims/REQ-0022.yml` (new),
`tests/loader.rs` (the id list), `tests/evidence.rs` and/or `tests/view.rs` (only a new test for
REQ-0022), `tests/linked-test-ids.txt`, `docs/spec.md`, `docs/claim-format.md` (the paragraph of
item 5), `docs/canonical-hashing.md` (one new "Recorded runs" bullet), `AGENTS.md` (the one counts
phrase), `baselines/air-gapped.json`.

Forbidden: `src/**`, `aliases/**`, `decisions/**`, `baselines/trust.yml`, `baselines/*.minisig`,
`bench/**`, `review/**`, `.forgejo/**`, `docs/adr/**`, `tests/canon_coverage.rs`, every other
`claims/**` file.
