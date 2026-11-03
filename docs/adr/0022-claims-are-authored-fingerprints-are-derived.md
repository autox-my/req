# 0022. Claims are authored, fingerprints are derived: `req.claim/2`

**Status: accepted** — the owner's rulings of 2026-11-03 on findings F2/F3 of the architecture review of
2026-11-02 and the follow-up (recorded in `docs/specs/M3-claim-format-v2.md`). Date accepted:
2026-11-03. **Supersedes [ADR 0014](0014-authored-slots-are-checked-whatever-their-shape.md)** (authored
slots are checked whatever their shape): there are no authored slots left to check. Performs the change
[ADR 0020](0020-the-address-is-a-meaning-fingerprint.md) enabled and did not make. [ADR
0019](0019-claims-carry-authored-test-links.md)'s `tests:` and `evidence:` are **unchanged**.

## Context

ADR 0020 settled that the address is a meaning *fingerprint* — a drift and direction check, not a
paraphrase collapser and not an identity. `req.claim/1` nevertheless stored it, together with its
canonical form and fifteen hand-authored slots that re-stated the parse, in the file people author.
That one choice is what produced `req canon --fill` and `--verify`, the "re-canonicalization candidate"
state, the four-case contradiction / omission / annotation / ok logic of ADR 0014, the critical-finding
rule against a hand-written digest, and a rewrite of every claim file on each `RULES_V` bump.

Two measurements agree that the slots were paying for less than they cost. The P-1 paraphrase probe
(`experiments/paraphrase/`, rules v6) measured Tier-1 as a fingerprint, not a collapser, which is what
ADR 0020 named. And the annotation category ADR 0014's amendment introduced — an authored qualitative
`attribute`/`value` triple the form does not carry, reported and never a flag — is exactly what hid
REQ-0020's defect: a claim whose authored slots looked fine while one meaning-bearing word sat in no
slot of the form (the T1.10 finding of rules v6). A check with a non-flagging outcome is a check an
author can satisfy by writing more prose beside the statement.

## Decision

1. **A claim's authored fields are five, plus an optional `pins:`.** `statement`, `applicability`,
   `relations`, `rationale` (required, as before), `tests` and `evidence` (optional, ADR 0019), and
   `pins:`. Every `req.claim/1` slot — `modality`, `subject`, `attribute`, `operator`, `value`, `unit`,
   `conditions`, `exception`, `temporal`, `quantifier`, `unstructured` — and the stored `canonical` and
   `address` are removed. A `req.claim/2` file carrying one is a **parse error naming the key**, never
   silently ignored. The file stays strict (`deny_unknown_fields`).
2. **The parse is derived at load and stored nowhere.** `canon::load_claims` runs
   `canonicalize_statement` against the corpus's registry, injects the authored `applicability` (an
   input to the form, ADR 0003), and exposes the form, the canonical bytes, the address, the lints and
   the derived `unstructured` on the loaded claim. Every command that reads an address reads this one.
   The derivation (`sha256("req.claim.v1\0" ‖ canonical bytes)`), the canonical bytes and `RULES_V`
   do **not** change; rules and registry versions are properties of the build and the registry.
3. **A pin agrees or it is a flag — there is no annotation category.** Pinnable keys: `modality`,
   `subject`, `operator`, `value`, `unit`. A pin is judged against the derived form: it *agrees*, or it
   is a **contradiction** (the parse says something else) or **not carried** (the parse holds no such
   value). Values compare exactly. A flagged pin never moves an address: pins are outside the form.
4. **`req canon --fill` and `--verify` are removed.** With nothing stored there is nothing to write or
   check; invoked, they are a usage error (exit 2) that says so. `req canon` prints the derived parse
   and each pin's verdict, and stays diagnostics (exit 0).
5. **The gate policy `addresses_current` is replaced by `pins_hold`**, in the same position: every pin
   of every applying claim agrees with the derived form. A flagged pin fails it (exit 1); with no pins
   it passes and says it held zero. The unfilled-claim UNVERIFIABLE paths go with the stored fields. A
   policy was renamed, so the gate document is **`req.gate/2`**; its exit contract is unchanged.
6. **`CLAIM_SCHEMA_V = 2`.** A `req.claim/1` file is a migration candidate (NOT JUDGED, exit 3) and the
   message names `tools/migrate_claims.py`, which turns the authored quantity-shaped slots (a numeric
   `value` with the operator authored beside it (`EQ` included), an operator other than `EQ`/none, a unit
   other than `none` — the cases ADR 0014 made a flag when unparsed) into pins and drops the rest, printing every drop.

## Consequences

**The loss, named as ADR 0020 required.** *"A hand-edited digest is a critical finding" disappears with
the stored digest*: there is no digest in a claim file to hand-edit. The replacement is **`pins_hold`
plus drift against a signed manifest, per claim** — a pin states the load-bearing part of the parse and
the gate fails when the statement stops deriving it, and the Baseline attests each applying claim's
fingerprint so `req baseline --verify` / `req audit` report any claim whose derived address moved since
signing. That is a weaker guarantee for a claim in no signed Baseline and with no pins: an edit to it
between signings is visible only as a moved address in `render`/`trace`, which is the trade ADR 0020
priced (an authoring-time check for a signing-time one).

**What it buys.** No command writes a claim file's derived field, so none can be stale, hand-written or
filled under other rules; a `RULES_V` bump rewrites no claim file; the "candidate" state, the fill
refusal, the four-case table and the annotation count are gone with their code (`src/fill.rs`,
`CanonicalCache`, `evaluate_claim`'s slot logic, `SlotCheck::Annotation`, `is_qualitative_convention`).
The audit-surface digests (`relations_hash`, `rationale_hash`, `tests_hash`) are untouched.

**Proven pure, not asserted.** The Baseline manifest carries no claim schema, so a pure format change
leaves it valid. This repository's 21 dogfood claims migrated in the same change with every address
unchanged against the list recorded before the edit (rules v6), and `baselines/air-gapped.json` (17 of
them) stayed `[VERIFIED]` under its original signature. The migration created eight pins on four claims
(REQ-0008 `value`+`operator`, REQ-0009 `value`+`operator`, REQ-0010 `value`+`operator`, REQ-0022
`value`+`operator`), all agreeing. (The first cut pinned the value alone where the authored operator
was `EQ`; the CI review of M3 showed `exactly zero` could then weaken to `at most zero` with the pin
still agreeing, so an authored operator beside a pinned value is pinned too.)

**REQ-0017 was re-worded in the same change.** Its old statement (each claim file stores its canonical
form so the stored address recomputes) became false, so it now says the address is derived at load and
attested in the signed Baseline, and its test link moved to `req::address_recompute::…`. That moved
REQ-0017's address and `tests_hash` and nothing else: the Baseline was re-emitted and re-signed by the
owner, so the "no address moves" property above holds for 20 of the 21 claims and REQ-0017 is the one
deliberate exception. `tools/migrate_claims.py` is a starting point
an adopter copies, not a subcommand: req never rewrites a claim on its own (REQ-0021).

## Considered Options

- **Keep the authored slots and stop storing only the digest.** Rejected: the slots are the source of the
  four-case logic and the annotation category; keeping them keeps the defect class that hid REQ-0020.
- **Keep an annotation category for pins** (a pinned value the parse does not carry is reported, not
  flagged). Rejected by ruling (2): a pin the parse does not carry is the bound that would leave identity
  in silence — the false-merge direction.
- **Make the migration a subcommand.** Rejected: a claim rewrite is a human's act (ADR 0004, REQ-0021);
  a copyable tool with a dry run by default keeps it one.
