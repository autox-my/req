# M4-1 — split REQ-0007 into its two obligations; re-word REQ-0015's rationale

Task id: `M4-1`. Code baseline: current `main` (after M3, `req.claim/2`). Owner rulings of 2026-11-04:
split REQ-0007's two obligations (as REQ-0004 and REQ-0020 were split), and re-word REQ-0015's
rationale assumption **in the same Baseline re-signature** (both move a signed manifest, so one
signature covers both). Claims are the owner's wording; this unit applies the decision and verifies it
through the tool, it does not invent requirements.

**Model:** `claude-sonnet-5-5` (high) — judgment and prose, as M1-4.

## Why

`claims/REQ-0007.yml` says "Authoring shall be friction-free **and** the decision boundary shall be
deliberately high-friction, demanding role, meaning, and evidence there." Two `shall`s: the loader
derives it unstructured only because the one-obligation rule is not enforced on an unstructured
statement, which hides the rule rather than satisfying it (the CI review of #91 said so). And
`claims/REQ-0015.yml`'s rationale assumption "negation expressed via SHALL-NOT plus slots, not by
prose" describes the removed `req.claim/1` slots; under `req.claim/2` the negation is the statement's
`shall not`, derived as SHALL-NOT.

## What to do

1. **Split REQ-0007.**
   - `REQ-0007` keeps its id, its relation (`refines REQ-0004`) and its rationale (source ADR 0006).
     Statement: `Authoring shall be friction-free.`
   - New claim **`REQ-0023`** (the next free id; `REQ-0011` stays retired) carries the other half:
     `The decision boundary shall be deliberately high-friction, demanding role, meaning, and evidence
     there.` Same rationale source (`docs/adr/0006-friction-zones.md`) and the same rejected
     alternatives and qualitative-assumption line as REQ-0007, dated today; relation
     `refines REQ-0004`. Both are expected to be derived-unstructured (qualitative): say so in the PR,
     with the lints `req canon` shows for each; do not reword either to make it structured.
   - **Dependents.** REQ-0012, REQ-0017 and REQ-0019 point at REQ-0007. Read each; they are about
     authoring or about the whole, not the boundary, so leave their relations alone (editing one moves
     its `relations_hash`). If one of them is in fact about the decision boundary, **do not edit it**:
     name it in the PR for the owner.
2. **Evidence for each half.** REQ-0007 and REQ-0023 are `evidence: machine` and each needs a real
   linked test meeting the strict bar in `tests/evidence.rs` (*"if this test failed, would the
   requirement be rejected?"*). The existing test
   `authoring_is_frictionless_and_the_decision_boundary_is_not` measures both halves in one body: split
   it into two tests, one per claim, named for the claim (`…authoring_is_frictionless` and
   `…the_decision_boundary_is_not_frictionless` or similar), update `tests/linked-test-ids.txt`, and
   link each. Splitting moves REQ-0007's `tests_hash`; that is expected (it is re-signed). If a half
   genuinely cannot be proved by a test, use `evidence: process` **and say why in the rationale**;
   it is not an escape for a machine-checkable obligation.
3. **Re-word REQ-0015's rationale assumption** to: `negation is carried by the statement's "shall
   not", derived as SHALL-NOT, not by a double negative in prose`. Nothing else in REQ-0015 changes
   (statement, test, relation): its address and `tests_hash` must not move; its `rationale_hash` does.
4. **Record the moves.** Expected, and nothing else: REQ-0007's address and `tests_hash` (its rationale is
   unchanged, so its `rationale_hash` should not move: say what you measure); REQ-0015's
   `rationale_hash`; REQ-0023 appears in the manifest. State old → new (12 hex) per field in the PR. The other claims' entries
   must be byte for byte the signed ones. Add one bullet under "Recorded runs" in
   `docs/canonical-hashing.md` (a new entry; do not rewrite earlier ones). Check `decisions/`: no
   Decision names REQ-0007's old address (DEC-0001 is about REQ-0016); re-check and say so.
5. **Counts.** The corpus becomes 22 claims. Update what states it, from `req render`'s own counts:
   `tests/pins.rs` (`claims: 21 (1 unstructured)` → 22 and 2 unstructured), `tests/loader.rs` corpus
   ids, `claims/README.md` (table row for REQ-0007, a row for REQ-0023, the REQ-0007 notes),
   `docs/claim-format.md` (the seed-corpus paragraph), `AGENTS.md`'s applying/undecided phrase if it
   states one. `tests/claim_v2.rs` keeps its recorded 21-address list: REQ-0007 moves, so add it to
   `RE_WORDED_AFTER_MIGRATION` with the new address; REQ-0023 is not in the recorded list and the
   test's filter ignores it. `docs/design.md`'s dated measurements are left alone.
6. **Regenerate the projection**: `req render --out docs/spec.md`, then `req render --check
   docs/spec.md` exits 0.
7. **The signed Baseline is the owner's step.** Emit the replacement manifest with `req baseline --at
   <the current manifest's signed_at> deployment_mode=air_gapped` (confirm the variant from the
   committed manifest; the signing time is the owner's to change: say in the PR that they may re-emit
   with their own `--at` before signing), write it to `baselines/air-gapped.json`, commit it, and
   **stop**: `minisign -Sm baselines/air-gapped.json` is a human act and
   `baselines/air-gapped.json.minisig` is not yours. The gate is red only at the Baseline verify step
   and the two tests that verify the committed signature, until the owner signs.

## Mutation checks (envelope `mutations`)

- Delete REQ-0023's linked test in a scratch copy → the gate's `tests_linked` fails for it
  (`req gate --test-report`); show it.
- Put the "decision boundary" obligation back into REQ-0007's statement in a scratch copy → the
  address moves (the property the split must not lose); show the check.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run. Expected non-green: **only** the
Baseline verify and the two tests that verify the committed signature. Every other step green, no
SKIP, every `bench/results` record unchanged.

## Allowed paths

`claims/REQ-0007.yml`, `claims/REQ-0015.yml`, `claims/REQ-0023.yml` (new), `claims/README.md`,
`tests/evidence.rs`, `tests/linked-test-ids.txt`, `tests/pins.rs`, `tests/loader.rs`,
`tests/claim_v2.rs` (the re-worded list only), other `tests/**` only for a corpus-size pin,
`docs/spec.md`, `docs/claim-format.md`, `docs/canonical-hashing.md` (one new bullet), `AGENTS.md`
(a count phrase only), `baselines/air-gapped.json` (re-emitted, unsigned).

Forbidden: `baselines/air-gapped.json.minisig`, `baselines/trust.yml`, `bench/**`, `aliases/**`,
`decisions/**`, `src/**`, root `Cargo.toml` and `Cargo.lock`, `review/**`, `.forgejo/**`,
`tools/ocr/**`, any other claim, and any change to `RULES_V` or the canonicalizer.
