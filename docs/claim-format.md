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

The declared `schema:` is **judged when a file is loaded**, against the value this build writes
(`CLAIM_SCHEMA_V` in `src/canon.rs`, reported by `req --version` as `claim_schema`). A file
declaring an *older* format is a migration candidate; a *newer* one is a question this build
cannot answer, because the file is not wrong — the reader is old; and a value that is not of the
`req.claim/<n>` shape at all is a broken file, not a version question. All three are **NOT
JUDGED** (exit 3), never `verified`: nothing inside the file was compared to anything, and the
declared format is what makes the rest of it readable as a claim of this build's contract. The
judgement lives in `claim_schema_reason` (`src/canon.rs`), which the loader calls and so does
every reader that reads a claim by path (`compare`, the fill path) — one home, so no command can
ignore the declaration.

The class is assigned by the **load loop**, not by the schema check alone: an equally definite
**duplicate `claim_id`** — two files declaring one id, their content compared — is UNVERIFIABLE
(3) in the same loop (`load_claims` in `src/canon.rs`), and every command that reads the corpus
reports it the same way. The two classes are therefore **coupled**: if either ever moves to FAIL
(1), the other must be revisited in the same change, and neither should move alone.

## Field reference

| Field | Meaning | Notes |
|---|---|---|
| `claim_id` | stable human label | never derived — it survives rewording (CONTEXT.md) |
| `statement` | the human sentence | **the source of truth in prose** |
| `modality` | SHALL / SHALL-NOT / MAY / SHOULD | mapping table = hashing spec T1.6; `UNSTRUCTURED` when unparseable (see below) |
| `subject` | registry id | must resolve in `aliases/subjects.yml` |
| `attribute` / `operator` / `value` / `unit` | the bounded obligation | operator per T1.5 {LT, LE, EQ, GE, GT, NE}; `unit: none` for non-SI quantities |
| `conditions[]` | guard clauses **parsed from the statement** (when/if/unless/during) | part of identity; sorted at T1.8. `req canon` compares it with the parse, so a scope predicate written here but absent from the statement is a *flag*, not a nuance — a variant belongs in `applicability` |
| `applicability[]` | variant predicates (`applies_when`) — the **selector slot** | **required** (an omitted key is a parse error, never `[]`); part of identity (ADR 0003); since rules v5 an *input* to the canonical form, because the statement carries no formal selector syntax: the sorted authored list is hashed |
| `exception`, `temporal`, `quantifier` | reserved slots: no derivation, no field | `null` when unused; authoring one is **refused** by `req canon --fill` rather than silently dropped from identity |
| `unstructured` | true → the structured slots are all null | the claim gets its raw-form canonical bytes + address per hashing spec §4, and opts out of structural merge-direction analysis |
| `canonical` | `{rules_v, subject_registry, form}` | written **only** by `req canon --fill`; `form` is stored as a YAML literal block so the hashed bytes are the stored bytes |
| `address` | `sha256("req.claim.v1\0" \|\| canonical_form_bytes)` | written by `req canon --fill`; `req canon --verify` enforces `sha256(tag \|\| stored form) == address` (REQ-0017) |
| `rationale` | object: `source`, `date`, `rejected_alternatives[]`, `assumptions[]` | all four **required** (an omitted list is a parse error, not an empty one); the losing side is preserved — never a string field |
| `relations[]` | `{type, to}` outgoing only | **required** (an omitted key is a parse error: relations sit outside the canonical form, so a dropped key leaves the address untouched while the edges vanish); incoming relations are always *derived* by traversal, never stored (REQ-0020); `relations: []` = a root claim |

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

## Corpus integrity (the loader's contract)

One claim per file means the *file set* is the corpus: a file that drops out silently
deletes a requirement, and a file that joins silently adds one. So the loader is
strict, and every command that reads the corpus returns UNVERIFIABLE (exit 3) rather
than proceeding over a corpus it could not read:

- **Filename = `claim_id`, spelled exactly.** Every `*.yml`/`*.yaml` in `claims/` is a
  claim file, and so is anything whose leading dot-segment is claim-shaped up to case —
  `REQ-0008.txt`, `REQ-0008`, `REQ-0008.yml.disabled`, `.REQ-0008`, `req-0008.txt`. One
  that is not named `REQ-<digits>.yml` (`req-0008.yml`, `REQ-0008.yaml`, `notes.yml`) is a
  load error — the old behaviour, filtering on prefix and suffix and discarding the rest,
  made a misnamed claim vanish with every exit code still zero. A file whose stem is not
  the `claim_id` it declares is a load error too. **Renaming a claim out of the contract
  does not disable it**: a claim-shaped name is still a candidate, and an unloadable
  candidate fails the corpus rather than quietly leaving it. Candidacy is
  case-insensitive; the *canonical* name is not — a lowercase `req-0008.txt` is reported,
  and `REQ-0008.yml` is the only filename that loads.
- **The corpus is flat.** One claim per file in `claims/`, no subdirectories: a directory
  there is a load error, because `mv claims/REQ-0008.yml claims/archive/` is the natural
  way to move a claim out of the way and it would otherwise be a silent deletion. A claim
  file must also be a regular file — a symlink is not one.
- **A corpus is never empty.** Zero claim files is a load error, not a pass: an empty
  corpus and a corpus whose every claim vanished are indistinguishable from the file set,
  and `claims: 0; discrepancy flags: 0` must never read as success.
- **Unique ids.** Two files declaring one `claim_id` is a load error, and their
  **content is compared**: an identical duplicate (the fully silent case — it never
  shows as drift) and a divergent one are named differently. The duplicate pass runs
  over every candidate before the naming pass, so a second file claiming an existing
  id is reported as the duplicate it is, not merely as a filename complaint.
- **Strict fields, at every level.** An unrecognized key is a parse error, in the claim
  file and in its nested objects (`rationale`, `canonical`, `relations[]`): `applicabilty:`
  does not parse as an empty default, and neither does a misspelled
  `rejected_alternatives:` inside `rationale`. The keys that carry meaning *outside* the
  canonical form are **required**, because losing one moves no address and nothing else
  would notice: `applicability` (a variant-scoped claim would otherwise become
  unconditional — identity *is* the address, so that is two requirements collapsed into
  one), `relations` (the edges would vanish from `trace` and `impact`), and the
  rationale's `rejected_alternatives`/`assumptions` (the losing side of a decision would
  be dropped from the record). Writing them as `[]` is how a claim says "none"; omitting
  them is the failure this project treats as catastrophic.

Directory entries that are not claims (`README.md`, editor droppings) are left alone;
that is a naming rule, not a silence. One deliberate exception runs the other way: an
entry whose *name* is not valid UTF-8 cannot be classified at all — it might be a claim
written in some other encoding — so it fails the corpus rather than being skipped.

## State of the seed corpus

`claims/REQ-0001 … claims/REQ-0021` were authored **structured-first**: the author
supplied the parse deliberately, and `canonical.form`/`address` stayed `null` until the
rules that define them existed. They are now **filled under rules v5** by
`req canon --fill`, and `req canon --verify` reports `21 verified, 0 candidates, 0
mismatches`, so the corpus is address-addressed end to end.

Two properties make the filled state trustworthy, and both are enforced rather than
assumed:

- **`--fill` refuses a divergent claim.** If the parse disagrees with the authored
  slots, the claim is left unfilled with a reported flag: an address over a parse that
  contradicts the claim would enshrine the contradiction. A refusal exits 3
  (UNVERIFIABLE), never 0.
- **`--verify` is in the gate**, so a rules bump without a re-fill exits 3 (every claim
  becomes a re-canonicalization *candidate*) and a hand-edited digest exits 1
  (mismatch). `sha256(tag ‖ stored bytes) == stored address` is checked, not promised.

`claims/` also seeds `bench/corpus/parity.yml` (E-008/E-009 come from this project's own
genuine paraphrases).

## Deliberately deferred

- Formal predicate syntax for `applicability` (v0: informal strings).
- `decisions.jsonl` schema + hash chain — build step 8 (ADR 0005).
- `baselines/` signing format — build step 8.
- A lint rule comparing a claim's statement against its `rationale.source` ADR
  narrative — the human keeps them consistent for now; the check is proposal-tier
  work.
