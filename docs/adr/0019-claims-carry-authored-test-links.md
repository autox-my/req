# 0019. Claims carry authored test links: attested, never identity

**Status: proposed** — the human's ruling of 2026-10-22: a claim's link to the tests that prove it is
**authored on the claim, attested in the signed Baseline, and never part of identity**. It was chosen
from three options — a separate mapping file, a naming convention, and the claim's own field — and the
reason is one sentence: a test rename must never move a claim's address, while the evidence must ride
the signed Baseline so an agent cannot quietly unlink what it cannot prove. Date proposed: 2026-10-22.

**Scope of this change.** This ADR lands the foundation: the field, its hash, the claim-file schema
bump and the Baseline envelope bump. The **release-gate policy** that reads a JUnit report and answers
"does a test that proves this claim pass?" — with its four outcomes — is the flagship follow-up (branch
B), and §6 records what it will add. Deferring it is deliberate: the field must be attested before a
policy consumes it, and the attestation is reviewable on its own.

## Context

The question the gate must eventually answer is the flagship one: **the agent says done; req says which
claims have no passing test.** Before this decision nothing in the model carried that fact.
`rationale_present` asks for a source and a date — provenance, not evidence. `audit` reports coverage
against a signed Baseline, and coverage is not test coverage. A claim with no test at all and a claim
whose test has been deleted were the same silence, and a silence in a green build reads as success.

Some claims are also established by the **process itself** — a recorded Decision, a signature, a
workflow — and no test can prove them. A claim must be able to say so, because an empty `tests:` list
is a *missing* answer, never a declaration.

The facts that shape the answer were read from the code rather than assumed:

- The claim-file loader **rejects unknown fields** (`src/model.rs`): adding a `tests:` key without
  teaching the parser is a parse error, so the field must be parsed, not tolerated.
- The file-format version was `req.claim/0` (`CLAIM_SCHEMA_V`, `src/canon.rs`), and the version axes
  (docs/claim-format.md) say a **format bump moves no address**. There is nothing to re-hash and no
  `RULES_V` bump.
- The Baseline envelope was v3 (`BASELINE_VERSION`, `src/baseline.rs`) and a manifest claim record
  already attests the claim's **audit surface** beside its address — `relations_hash` and
  `rationale_hash` (ADR 0013), over parsed values, domain-separated by `req.relations.v1\0` /
  `req.rationale.v1\0`.
- The gate is a list of `PolicyResult`s with the tri-state `Outcome` and one fold: **FAIL outranks any
  UNVERIFIABLE, and both outrank PASS** (`src/gate.rs`, ADR 0015). A new policy will be a sibling of
  `rationale_present`, not a new kind of answer.

## Decision

### 1. The field

A claim carries `tests: [String]`, authored, optional. Each entry is one test id, **`classname::name`**
exactly as the JUnit report writes it — the id is the test's identity, which is why a rename shows up
as "linked test not in the report" rather than as a pass. An absent field or `[]` on a machine claim
means *no link*; it is never a parse error, because a claim whose proof does not exist yet is not a
malformed claim. The loader refuses an empty id and a duplicated id.

Alongside it, `evidence: machine | process` (default `machine`) is the **declared evidence mode**.
`process` says the process itself establishes the claim, and it must carry no `tests:` — naming both is
a load error (machine or process, never both). Process is never inferred: an empty `tests:` list on a
machine claim is *no link*, not a declaration.

Neither field is in the canonical form. A format bump moves no address, and adding, removing or
renaming a link moves no address either — pinned by `tests/fill.rs::adding_or_removing_a_test_link_moves_no_address`.

### 2. The hash

The Baseline attests the whole declaration, not a list:

```
tests_hash = sha256("req.tests.v1\0" ‖ serde_json::to_string(
    {"evidence": "machine" | "process", "tests": [<ids, sorted>]}))
```

- Plain serde JSON of a typed struct, the same writer as `relations_digest` — the byte format is
  unambiguous and the fields have a fixed declaration order (`evidence` first).
- The list is **sorted at hash time**: the authored order means nothing, and a meaningless reordering
  must not change what the Baseline attests.
- The evidence mode rides the same digest because it is part of the same declaration: without it,
  flipping `machine` to `process` (and dropping the links) would move neither address nor attested
  list — precisely the quiet unlink the Baseline exists to catch.

### 3. Schema `req.claim/1`

Every claim file's `schema:` line moves `req.claim/0` → `req.claim/1` (mechanical; no address moves).
The loader already judges the declaration by direction, so a `req.claim/0` file is a migration
candidate (NOT JUDGED, exit 3) rather than silently accepted by a build that writes fields the old
contract did not have.

### 4. Envelope v4

`BASELINE_VERSION` moves 3 → 4. The manifest's per-claim record gains `tests_hash`, written by
`baseline --at` and compared by `--verify`; a changed `tests_hash` after signing is **FAIL**, the same
class as a changed address. The support floor moves independently
(`OLDEST_VERIFIABLE_BASELINE_VERSION = 3`): a **v3** manifest still verifies on its historical signed
bytes and says **once** that it predates v4, so its tests are **not attested** — never a FAIL, because
it never claimed to attest them. Re-issuing and re-signing the committed Baseline as v4 is a human act
and is not performed by this change.

## Consequences

- The evidence a claim is ratified on now rides the signature. An agent can edit a claim file, but the
  next `baseline --verify` reports the change; the address alone could not.
- A test rename is visible as a missing id (the rename defence), once the policy lands; until then the
  fact is recorded and attested but no build step consumes it.
- The two axes stay independent: `RULES_V` is untouched (no address moves), `CLAIM_SCHEMA_V` is 1 and
  `BASELINE_VERSION` is 4.
- The committed v3 Baseline keeps verifying; a release that wants test attestation must re-issue and
  re-sign as v4.

## Deferred to the flagship follow-up

Recorded here so the review of this change can see the whole shape; none of it is implemented by this
ADR's change:

- `req gate --test-report <junit.xml>` and the `tests_linked` policy, a sibling of
  `rationale_present`, with four outcomes: **PASS** (a linked test passed), **FAIL** (every linked test
  present and none passed; a skipped test is not a pass), **UNVERIFIABLE — no linked test**, and
  **UNVERIFIABLE — linked test not in the report**. An absent/unparsable/zero-testcase report is
  UNVERIFIABLE once, naming the reader's reason.
- The JUnit reader (`src/junit.rs` lands here for its tests; the CLI seam is the follow-up's), the
  nextest pin and its reviewed profile, `scripts/check.sh`'s one-run test step, and the dogfood links
  (19 machine-evidenced claims, 2 process-evidenced).

## Alternatives considered

- **A separate mapping file** — rejected: it is a second identity surface beside the claim, and the
  Baseline would attest a file's digest rather than the claim's own declaration.
- **A naming convention** (`test_<claim_id>`) — rejected: it makes a rename a silent unlink, imposes a
  test-naming policy on adopters, and cannot express one test proving several claims.
- **Tests in the canonical form** — rejected: identity is meaning, and a rename is not a meaning
  change. It would also force every adopter's test names into their addresses.
- **A required `tests:` field** — rejected: "no link" must be a finding the gate can name, not a load
  error.
- **Hashing the whole report into the manifest** — rejected: the manifest attests the claim's
  *declaration*, not a run. A report changes with every run; the claim does not.

## Falsification

- Put `tests` into the canonical form: the address-invariance test fails (measured as a mutation).
- Drop the `evidence` field from the digest: the machine↔process flip becomes invisible to `--verify`
  (a test pins the two digests apart against an independent shell oracle).
- Make a v3 manifest fail under this build: the committed Baseline's verify would return FAIL, and the
  gate step has no SKIP — it would be red.
