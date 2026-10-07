# claims/ — the dogfood seed corpus

The req tool's own requirement set, in the req claim-file format. This is build
step 2 of [docs/design.md](../docs/design.md), executed with step 1 already on disk
([docs/claim-format.md](../docs/claim-format.md)).

## What is here

| ID | The claim | Rationale source |
|---|---|---|
| REQ-0001 | A claim is the unit, not a document | [ADR 0001](../docs/adr/0001-the-unit-is-a-claim.md) |
| REQ-0002 | Authority = signed Decisions | [ADR 0005](../docs/adr/0005-authority-is-a-signed-decision.md) |
| REQ-0003 | Variants are selectors | [ADR 0003](../docs/adr/0003-variants-are-selectors.md) |
| REQ-0004 | Proposals, never auto-merge | [ADR 0004](../docs/adr/0004-model-proposes-human-disposes.md) |
| REQ-0005 | Merging requires human ratification | [ADR 0004](../docs/adr/0004-model-proposes-human-disposes.md) |
| REQ-0006 | Splitting is always allowed | [ADR 0004](../docs/adr/0004-model-proposes-human-disposes.md) |
| REQ-0007 | Authoring is friction-free; the decision boundary is not | [ADR 0006](../docs/adr/0006-friction-zones.md) — intentionally unstructured (see lint note) |
| REQ-0008 | Benchmark false-merge rate = 0.0 (hard gate) | [canonical-hashing.md §6](../docs/canonical-hashing.md) |
| REQ-0009 | Direction classification ≥ 0.95 | same |
| REQ-0010 | False-split soft target ≤ 0.30 (SHOULD) | same |
| REQ-0011 | Tier-2 provider stability ≤ 2% flip rate | same |
| REQ-0012 | No network needed for claim ops when air-gapped | design.md §2 (tensions 2, 5) |
| REQ-0013 | Non-engineer edits produce Proposals in a Working set | design.md §5 (gap 4 closure) |
| REQ-0014 | One commit per ratified logical Decision | design.md §5 |
| REQ-0015 | No continuous bi-directional file sync (SHALL-NOT) | design.md §5 (gap 3 closure) |
| REQ-0016 | Undecided scope resolves UNKNOWN, never FALSE | [ADR 0003](../docs/adr/0003-variants-are-selectors.md) |
| REQ-0017 | Canonical bytes stored; address recomputable | [canonical-hashing.md §5](../docs/canonical-hashing.md) |
| REQ-0018 | Numeric values are exact decimal strings | [canonical-hashing.md T1.3-4](../docs/canonical-hashing.md) |
| REQ-0019 | Undecidable parses lint, never guess | [canonical-hashing.md](../docs/canonical-hashing.md) |
| REQ-0020 | Relations stored once, on the outgoing side | [claim-format.md](../docs/claim-format.md) |

## Reading a file

`schema` is the file-format version. The structured slots are an authored parse of
`statement` (parity invariant — [docs/claim-format.md](../docs/claim-format.md)):
`req canon` will demand `parse(statement) == slots` and error on divergence. Until
then `canonical` and `address` are `null` on purpose — no hash is written before the
rules that define it exist (claiming `address:` more than filling is exactly the
"evidence by assertion" failure mode the tool exists to prevent).

## The first `req canon` run (2026-10-07) — 15/20 fully verified

The v0 Tier-1 pipeline verifies unstructured-tag, modality, subject (registry), value,
unit, and operator per claim. 15 of 20 claims pass all five checks. The five flagged
claims are *known v0 gaps*, each itemized — nothing was silently passed:

- **REQ-0004 / REQ-0017 / REQ-0018 / REQ-0020** — the *subject-phrase boundary*:
  possessives and multi-noun pre-modality phrases ("the req tool's semantic layer",
  "each claim file") need a boundary rule the v0 subject scan lacks.
- **REQ-0007** — the authored `unstructured: true` is correct by design
  ("friction-free" is qualitative); the v0 parse cannot detect untestability, so the
  flag is the honest state of an intentional divergence.
- **REQ-0008** — the statement's "exactly zero" is a *word-number*; the value lexicon
  that parses "zero" to `0.0` is not in Tier-1 v0.
- **REQ-0011** — the *subject contains a digit* ("a Tier 2 provider's proposal
  output…"); the digit-check condemns the sentence to the unstructured fallback. A
  conservative false split (name-tokens with digits inside need the subject
  boundary fix above) — never a merge.
- **REQ-0010 was a real seed-corpus bug the diagnostic caught**: the authored value
  `"0.30"` was not in canonical form; corrected to `"0.3"` — the format's own rule
  working as designed.

The parity invariant *completes* at the attribute-lexicon step (v1); until then the
diagnostic is deliberately noisy rather than silently permissive.


## Notes you can see in the corpus

- **REQ-0007 is intentionally `unstructured: true`** — "friction-free" and
  "deliberately high-friction" are qualitative by nature; `req lint` will emit
  `untestable_comparator` and that is correct, not a bug.
- **REQ-0010 uses `modality: SHOULD`** — soft targets are expressed with a modality,
  not with prose.
- **REQ-0015 negates via `SHALL-NOT` + slots**, not with a double negative in prose.
- **REQ-0012 carries an `applicability` predicate** (`deployment_mode == air_gapped`)
  — our own corpus exercises a selector end to end.
- ADRs keep the narrative; each claim points to its ADR in `rationale.source`. The
  automated cross-check (statement ↔ ADR text) is deliberately deferred (claim-format
  §deferred) — until then the pointer carries the link by discipline, not tooling.
