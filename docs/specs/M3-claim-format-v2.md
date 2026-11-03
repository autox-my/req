# M3 — `req.claim/2`: author the claim, derive the parse, store no fingerprint

Task id: `M3`. Code baseline: current `main`. Owner rulings of 2026-11-03 (architecture review F2/F3
and the follow-up): (1) a claim's authored fields are the five below plus an optional `pins:`;
(2) a pin the parse does not carry is always a flag, never an annotation, and the qualitative
`attribute`/`value` annotation category is removed; (3) the fingerprint is computed at load and
stored nowhere in a claim file; (4) the 21 dogfood claims migrate in the same PR, and an older-schema
file stays a *migration candidate* (NOT JUDGED), which the loader already says.

**Model:** run as `claude-opus-5-5` (high). This is the one unit that changes the file contract
under every command; the others in this plan did not.

## Why

The review's F2/F3 and the P-1 measurement agree. The address is a meaning *fingerprint* (ADR 0020):
a drift and direction check, not a paraphrase collapser and not an identity. Storing it, and 15
hand-authored slots that re-state the parse, in the file people author, is what produces `--fill`,
`--verify`, the "candidate" state, the four-case contradiction/omission/annotation logic of ADR
0014, the critical-finding rule against hand-written digests, and a rewrite of every claim on each
`RULES_V` bump. And the annotation category is exactly what hid REQ-0020's defect.

## The contract (what a v2 claim is)

```yaml
schema: req.claim/2
claim_id: REQ-0020
statement: >-
  The req tool shall serialize each relation on the outgoing side of its origin claim.
applicability: []          # required, as today (a dropped key must not silently widen a claim)
relations:                 # required, as today
  - {type: derives-from, to: REQ-0002}
tests: ["req::view::incoming_relations_are_derived_from_the_outgoing_side"]   # optional, as today
evidence: machine          # optional, default machine, as today (ADR 0019)
rationale: {source: …, date: …, rejected_alternatives: [], assumptions: []}   # required object, as today
pins:                      # OPTIONAL
  modality: SHALL
  value: "0.02"
  operator: LE
  unit: percent
```

1. **Strict.** `deny_unknown_fields` stays. A v2 file that still carries any v1 slot (`modality`,
   `subject`, `attribute`, `operator`, `value`, `unit`, `conditions`, `exception`, `temporal`,
   `quantifier`, `unstructured`, `canonical`, `address`) is a **parse error naming the key** and
   saying it was removed in `req.claim/2`, not silently ignored.
2. **Derived at load, stored nowhere.** `canon::load_claims` runs `canonicalize_statement` with the
   corpus' `aliases/subjects.yml` (the authored `applicability` is an input to the form, as in
   `evaluate_claim` today) and exposes the form, the address, the lints and `unstructured` on the
   loaded claim. `unstructured` is **derived**, never authored: a statement the canonicalizer cannot
   structure is derived-unstructured exactly as REQ-0007 is today. The address derivation
   (`sha256("req.claim.v1\0" ‖ canonical bytes)`), the canonical bytes and `RULES_V = 6` do **not
   change**. Rules and registry versions are properties of the build and the registry (reported by
   `--version` and in manifests), not of a claim file.
3. **Pins** (`pins:`, optional): an author states what a claim must parse to where the parse is
   load-bearing. Pinnable keys: `modality`, `subject`, `operator`, `value`, `unit`; any other key is a
   parse error. A pin is judged against the derived form with **no annotation category**: it *agrees*,
   or it is a **flag — contradiction** (the parse says something else), or a **flag — not carried**
   (the pin states a value, comparator or unit the derived form does not hold). Values are
   compared exactly (`value` is a canonical decimal string, as in `claim-format.md`). A flagged pin
   never moves an address: it reports.
4. **`req canon`** prints the derived parse per claim (modality, subject, operator, value, unit,
   structure, lints) and each pin's verdict; it stays diagnostics (exit 0), as today. **`--fill` and
   `--verify` are removed**: invoked, they exit **2** with a usage error that says nothing derived is
   stored in `req.claim/2`, so there is nothing to fill or verify and the check for pins is `req gate`.
5. **Gate.** The policy `addresses_current` (a stored address recomputes) has no subject any more and
   is **replaced by `pins_hold`** (every pin of every applying claim agrees with the derived form),
   in the same position in the list. `pins_hold` fails (exit 1) on any flagged pin; with no pins
   anywhere it is reported as passing *because there is nothing to hold*, and says so in its detail
   (the policy was exercised on zero pins, not skipped). The unfilled-claim UNVERIFIABLE paths go
   with the stored fields. `req.gate/1` becomes **`req.gate/2`** (a policy was renamed); its exit
   contract is otherwise unchanged. Update `--help`.
6. **Schema version.** `CLAIM_SCHEMA_V = 2`. A `req.claim/1` file is a migration candidate (NOT JUDGED,
   exit 3) through the existing `claim_schema_reason`; its message names `tools/migrate_claims.py`.
   Newer and malformed values behave as today. `--version` reports `claim_schema req.claim/2`.
7. **Everything that read a stored address reads the derived one**, with the same bytes: `render`,
   `trace`, `impact`, `compare`, `baseline` (emit, verify, drift), `audit`, `init`. The Baseline
   manifest content depends only on derived addresses, `relations_hash`, `rationale_hash` and
   `tests_hash`, none of which moves.

## Acceptance property that makes this safe (an independent oracle)

**No address, relations/rationale/tests hash moves.** Before changing anything, record the 21 claims'
current `claim_id → address` list from the committed `baselines/air-gapped.json` and from `req render`
at the baseline commit, and keep it in the PR description. After the migration `req render` must give
the identical addresses, and **`req baseline --verify baselines/air-gapped.json` must stay
`[VERIFIED]` with the existing, unchanged `.minisig`.** If any address moves, the migration or the
derive-at-load path is wrong: stop and report it, never re-emit or re-sign the Baseline to make it
pass. (The manifest does not carry the claim schema, so a pure format change leaves it valid; this
is the check that the change is in fact pure.)

## What to build

1. **ADR 0022** — `docs/adr/0022-claims-are-authored-fingerprints-are-derived.md`, `**Status:
   accepted**` (the owner's rulings of 2026-11-03): the five authored fields plus `pins:`; derive at
   load; no stored fingerprint; the annotation category removed; `--fill`/`--verify` removed; the
   gate policy swap; the loss named, as ADR 0020 required: *"a hand-edited digest is a critical
   finding" disappears with the stored digest, and the replacement is `pins_hold` plus drift against
   a signed manifest, per claim*. It supersedes ADR 0014 (authored slots are checked whatever their
   shape) with a status line, following the 0008 → 0009 precedent, and notes ADR 0019's `tests:`/`evidence:`
   are unchanged. Add a one-line *Amended* note in ADR 0020's Consequences ("enabled, not performed"
   → performed by ADR 0022).
2. **Model and loader** (`src/model.rs`, `src/canon.rs`): the v2 `ClaimFile`, `Pins`, the derive-at-load
   `LoadedClaim`, `evaluate_pins` (pure, table-tested), the new schema version, the parse errors of item
   1. Remove `CanonicalCache`, `evaluate_claim`'s four-case slot logic, `SlotCheck::Annotation`,
   `is_qualitative_convention` and what only they used. `src/fill.rs` is deleted; the
   `StoredState`-style code that existed to describe stored-vs-fresh goes with it. Keep the
   audit-surface digests (`relations_digest`, `rationale_digest`, `tests_digest`) exactly as they are.
3. **Commands:** `canon` (item 4), `gate` (item 5), `init` (it writes a v2 starter claim: statement,
   applicability, relations, rationale; nothing derived, and its next-steps text and the CI snippet it
   prints no longer mention `--fill`/`--verify`), `baseline`/`audit`/`render`/`trace`/`impact`/`compare`
   adapted to the derived fingerprint; `--help` pages and the top-level help updated; their pins in
   `tests/cli_version.rs` updated.
4. **Migration tool** `tools/migrate_claims.py` (a starting point adopters copy, like `tools/adopt.py`,
   not a subcommand; `--self-check` table-driven, dry run by default, refuses an already-v2 file):
   reads a `req.claim/1` file and writes `req.claim/2`: keeps `claim_id`, `statement`,
   `applicability`, `relations`, `tests`, `evidence`, `rationale`; drops every other v1 key; **turns
   authored quantity-shaped slots into pins** — a numeric authored `value`, an `operator` other than
   `EQ`/none, or a `unit` other than `none` become `pins:` entries (the cases ADR 0014 made a flag
   when unparsed); the qualitative `attribute`/`value` triple and the stored `canonical`/`address`
   are dropped, and the tool prints what it dropped per file. No YAML library is installed here:
   write it without one (the project's tools already do), preserving the rationale object. Run it on
   the 21 claims; **review the printed drops** and list the pins it created (expected: the numeric
   claims, e.g. REQ-0008..0010, REQ-0014, REQ-0022) in the PR description.
5. **`tools/adopt.py`** drafts v2 claims (statement only; nothing derived, nothing to fill) and its
   `--self-check`, `tests/adopt.rs` and the quickstart rehearsal follow.
6. **Docs:** `docs/claim-format.md` rewritten for v2 (field reference, pins, the derived view, what
   moved where); `docs/adopting.md` (no more fill/verify steps; the authoring loop is
   edit → `req canon` to see the parse → `req gate`); `README.md` and `CONTEXT.md` (terms: *pin*;
   *derived parse*; adjust *Meaning fingerprint* if needed); `AGENTS.md` (the derived-fields,
   claim-diagnostics and parity-invariant bullets and the "never hand-write" rule restated for a format
   where there is nothing to hand-write); `examples/agents-snippet.md` and
   `examples/github-workflows/req.yml` — the CI snippet's `req canon --verify` step goes, and
   `tests/ci_template.rs` keeps the **three-way byte equality** (init const, adopting page block,
   example file) with the new text. `docs/spec.md` regenerated (`req render --out`, `--check` 0).
7. **Tests.** Migrate the fixtures that embed v1 claims (`tests/adopt.rs`, `canon_coverage.rs`,
   `claim_slots.rs`, `compare.rs`, `evidence.rs`, `loader.rs`, `qualitative.rs`, `release_gate.rs`,
   `select.rs`, `parity_annotations.rs`, `cli_version.rs` and `tests/common`); delete the tests that
   only existed for the removed mechanisms (`tests/fill.rs`, the annotation tests) and **list each
   deleted test with the rule it covered and where that rule now lives or "deleted: <reason>"** in the
   PR description (a ledger, as M2-3 did). New tests, expectations from independent oracles: (a) the
   21 claims' addresses equal the recorded pre-migration list; (b) a v2 file carrying a v1 slot is a
   parse error naming the key; (c) pins: agree / contradiction / not-carried, each through the CLI; (d)
   a v1 file → exit 3, message names the migration tool; (e) `canon --fill` and `--verify` → exit 2
   with the message; (f) `pins_hold` fails on a flagged pin and says "zero pins" when there are none;
   (g) `req.gate/2` JSON; (h) the REQ-0020 regression pair (outgoing vs incoming) still gets different
   addresses through the v2 loader. Keep `tests/canon_coverage.rs` and the parity corpus intact in
   meaning: **no Tier-1 rule changes** in this unit.
8. **Experiments** that depend on the library (`experiments/tier2`, `experiments/paraphrase`) keep
   compiling; touch them only to follow an API rename, and say which.

## Mutation checks (envelope `mutations`)

- Make pins always agree → test (c)'s contradiction case fails.
- Make the loader ignore an unknown v1 slot → test (b) fails.
- Make derive-at-load skip `applicability` → the address list test (a) fails.
- Restore `addresses_current` semantics (read a stored address) → ... no stored address exists; instead
  swap `pins_hold` for a pass-through → test (f) fails.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP, the Baseline step
`[VERIFIED]` against the **unchanged signature**, `QUICKSTART <n>s` present, every `bench/results`
record unchanged (parity and external-v1..v4 and direction pass; no bench pair moved). If the child finds
the unit cannot land green in one PR it **stops and reports `blocked` with a proposed split**; it never
merges a half-migrated state.

## Allowed paths

`src/**` (model, canon, fill (deleted), init, gate, main, view, baseline, audit, compare, impact,
propose as needed), `tests/**`, `claims/REQ-*.yml`, `claims/README.md`, `tools/migrate_claims.py` (new),
`tools/adopt.py`, `tools/quickstart.sh`, `scripts/check.sh` (steps that name fill/verify and the new
self-check), `docs/**` (claim-format, adopting, design, canonical-hashing as needed, `spec.md`,
`adr/0022-…`, status/Amended lines on ADR 0014 and 0020), `README.md`, `CONTEXT.md`, `AGENTS.md`,
`examples/**`, `experiments/**` (API-follow edits only).

Forbidden: `baselines/**` (nothing there changes: `[VERIFIED]` is the proof), `bench/**`,
`aliases/**`, `decisions/**`, root `Cargo.toml` and `Cargo.lock` (no dependency may be added), `review/**`,
`.forgejo/**`, `tools/ocr/**`, and any change to `canonicalize_statement`'s rules or `RULES_V`.
