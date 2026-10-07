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

## The `req canon` runs (2026-10-07) — 18/20 fully verified after Tier-1 v1

The v0 run verified 15/20 (14 discrepancy flags, every one itemized, nothing
silently passed). Tier-1 v1 then added word-numbers ("exactly zero"), the
possessive strip ("provider's" → "provider"), the subject alias-prefix boundary,
comparator-anchored value search (a name-digit inside "tier 2 provider" no longer
becomes the claim's value), and the percent scale (×10⁻² exact). Re-run: **18/20
fully verified; 2 flags remain, both in REQ-0007.**

- **REQ-0010's authored value `"0.30"` and REQ-0008's `"0.0"` were real seed
  bugs the diagnostic caught**: non-canonical decimal strings; fixed to `"0.3"`
  and `"0"` — the format's own authoring rule working as designed.
- **REQ-0011 / REQ-0017 / REQ-0018 / REQ-0020** — the statements did not lead
  with their actual subjects; re-worded to subject-led phrasing (semantics
  unchanged) so the parse resolves the registry subject directly.
- **REQ-0007 stays flagged, by design**: the claim is authored `unstructured:
  true` on purpose ("friction-free" is qualitative; no measurable bound exists),
  and the testability-lint pass that would *derive* that classification instead
  of trusting the author flag is v2 work — the flag is the honest state of an
  intentional divergence, not a parse failure.



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
