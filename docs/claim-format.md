# Claim-file format — v0

Status: `req.claim/0`. Implements docs/adr/0002-git-is-transport-not-identity.md
(one claim per file) and docs/canonical-hashing.md (identity, tiers, freeze).

## Layout

```
claims/REQ-0001.yml        # one claim, one file — filename = claim_id
aliases/subjects.yml       # the subject registry (T1.7 of the hashing spec)
bench/corpus/parity.yml    # labeled Equivalent / Not-equivalent pairs
baselines/                 # signed Baselines — deferred to build step 8
decisions.jsonl            # append-only Decisions log — deferred to build step 8
```

Decisions and Baselines are **not** defined here yet. Claims never store approval
state: it is always derived from `decisions.jsonl`, never copied into claim files.

## Version axes (do not conflate)

| Axis | Where | Effect of a bump |
|---|---|---|
| File format | `schema: req.claim/0` | a migration; addresses unchanged |
| Normalization rules | `canonical.rules_v` | re-canonicalization of affected claims → *candidates* needing re-ratification (hashing spec §5); history never silently rewritten |
| Subject registry | `aliases/subjects.yml` version | same effect as a rules bump, scoped to claims using affected subjects |

## Field reference

| Field | Meaning | Notes |
|---|---|---|
| `claim_id` | stable human label | never derived — it survives rewording (CONTEXT.md) |
| `statement` | the human sentence | **the source of truth in prose** |
| `modality` | SHALL / SHALL-NOT / MAY / SHOULD | mapping table = hashing spec T1.6; `UNSTRUCTURED` when unparseable (see below) |
| `subject` | registry id | must resolve in `aliases/subjects.yml` |
| `attribute` / `operator` / `value` / `unit` | the bounded obligation | operator per T1.5 {LT, LE, EQ, GE, GT, NE}; `unit: none` for non-SI quantities |
| `conditions[]` | guard clauses parsed from the statement (when/if/unless/during) | part of identity; sorted at T1.8 |
| `applicability[]` | variant predicates (`applies_when`) | part of identity (ADR 0003); informal predicate strings in v0, formal syntax deferred |
| `exception`, `temporal`, `quantifier` | reserved slots | `null` when unused |
| `unstructured` | true → the structured slots are all null | the claim gets its raw-form canonical bytes + address per hashing spec §4, and opts out of structural merge-direction analysis |
| `canonical` | `{rules_v, subject_registry, form}` | **null in v0 seed files** — filled by `req canon` (build step 3) |
| `address` | `sha256("req.claim.v1\0" \|\| canonical_form_bytes)` | null until then; once set, `sha256(tag \|\| stored form) == address` must always hold (REQ-0017) |
| `rationale` | object: `source`, `date`, `rejected_alternatives[]`, `assumptions[]` | the losing side is preserved — never a string field |
| `relations[]` | `{type, to}` outgoing only | incoming relations are always *derived* by traversal, never stored (REQ-0020); `relations: []` = a root claim |

Reference types (`to`, `source`) use `claim_id`s, addresses, or repo paths —
whatever stays stable under rewording.

## The parity invariant (what `req canon` must check)

The statement is the source of truth; the structured slots are an **authored parse**
of it. `req canon` re-parses the statement under the current rules and demands:

```
parse(statement, rules_v, registry_v) == authored slots
```

A mismatch is a lint ERROR — never silently "corrected" in either direction. If no
deterministic parse exists, the claim is re-filed with `unstructured: true` after lint
records why. This is the seam where writing a claim file *can be proven* against a
claim file, before merge direction analysis is trusted.

## Authoring rules

- One obligation per claim. A "shall … and shall …" statement is two claims, split
  before filing.
- Negation rides on `modality: SHALL-NOT` composed with the `attribute`/`value`
  slots ("shall not run a continuous sync" → attribute `continuous_bidirectional_file_sync`,
  operator EQ, value `absent`), never on reworded semantics.
- Values are **exact decimal strings** *in canonical form*: strip trailing zeros at
  write time (`"0.3"`, never `"0.30"`, never a float); never rounded, never truncated
  (hashing spec T1.3/T1.4). A non-canonical authored value is exactly what `req
  canon`'s flag exists to catch.
- Anything the rules cannot decide — bare `dB`, "under 200 ms" (ambiguous), "fast" —
  yields a lint finding (`untestable_quantity`, `untestable_comparator`,
  `ambiguous_modality`), never a guess (REQ-0019).
- Alias sets are disjoint per subject; `req` is deliberately excluded from
  `REQ_TOOL`'s aliases because it names the CLI binary in other contexts.

## State of the v0 seed corpus

`claims/REQ-0001 … claims/REQ-0020` are authored **structured-first**: the author
supplies the parse deliberately, `canonical.form` and `address` are `null`, and
`req canon` fills them at build step 3. Until then the corpus is chompable for lint
and review but not yet address-addressed — by design, so that no hash is invented
before the rules that define it exist. `claims/` also seeds `bench/corpus/parity.yml`
(E-008/E-009 come from this project's own genuine paraphrases).

## Deliberately deferred

- Formal predicate syntax for `applicability` (v0: informal strings).
- `decisions.jsonl` schema + hash chain — build step 8 (ADR 0005).
- `baselines/` signing format — build step 8.
- A lint rule comparing a claim's statement against its `rationale.source` ADR
  narrative — the human keeps them consistent for now; the check is proposal-tier
  work.
