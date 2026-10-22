# Claim-file format — v1

Status: `req.claim/1`. Implements docs/adr/0002-git-is-transport-not-identity.md
(one claim per file), docs/adr/0019-claims-carry-authored-test-links.md (authored test
links, attested and outside identity) and docs/canonical-hashing.md (identity, tiers,
freeze).

## Layout

```
claims/REQ-0001.yml        # one claim, one file — filename = claim_id
aliases/subjects.yml       # the subject registry (T1.7 of the hashing spec)
bench/corpus/parity.yml    # labeled Equivalent / Not-equivalent pairs
baselines/                 # the trust anchor, and one signed manifest per variant
decisions/DEC-*.yml        # one YAML file per Decision — build step 8, landed
```

The Baseline has landed too, so neither line is a deferral now: `baselines/trust.yml` is the
committed trust anchor (ADR 0010), `baselines/<variant>.json` is the canonical manifest a
human signs with its `.minisig` beside it, and `req baseline --verify` re-derives every
address under the variant that manifest records. Claims never store approval
state: it is derived from `decisions/DEC-*.yml` and from the signed Baseline, never copied
into claim files.

The Decision log itself landed at build step 8 (ADR 0005), so its shape is no longer a
deferral: **one YAML file per Decision** under `decisions/`, named `DEC-<zero-padded>.yml`
so file order is chronological, each entry committing to its predecessor by
`sha256("req.decision.v1" ‖ prev_hash ‖ canonical content)`. Only `req decision --fill`
writes the derived `prev_hash`/`hash`, and it refuses to change a derived field already
present that disagrees with the content — recomputing over it would launder the edit
`req decision --verify` reports as TAMPERED. `--verify` walks the chain (0 VALID /
1 TAMPERED / 3 UNVERIFIABLE) and `scripts/check.sh` drives it on every gate run. The
reader is `src/decision.rs`, whose `log_paths` reads `root.join("decisions")`.

## Version axes (do not conflate)

| Axis | Where | Effect of a bump |
|---|---|---|
| File format | `schema: req.claim/1` | a migration; addresses unchanged — v1 adds the readable `tests:`/`evidence:` fields, an older declaration is a migration candidate (exit 3), and no `RULES_V` moves |
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
| `attribute` / `operator` / `value` / `unit` | the bounded obligation, or the qualitative annotation | operator per T1.5 {LT, LE, EQ, GE, GT, NE}; `unit: none` when there is no unit. `value`/`unit`/`operator` are checked against the parse (contradiction and omission are flags; a qualitative triple is an annotation — see the parity invariant); `attribute` is unverified by design |
| `conditions[]` | guard clauses **parsed from the statement** (when/if/unless/during) | part of identity; sorted at T1.8. `req canon` compares it with the parse, so a scope predicate written here but absent from the statement is a *flag*, not a nuance — a variant belongs in `applicability` |
| `applicability[]` | variant predicates (`applies_when`) — the **selector slot** | **required** (an omitted key is a parse error, never `[]`); part of identity (ADR 0003); since rules v5 an *input* to the canonical form, because the statement carries no formal selector syntax: the sorted authored list is hashed |
| `exception`, `temporal`, `quantifier` | reserved slots: no derivation, no field | `null` when unused; authoring one is **refused** by `req canon --fill` rather than silently dropped from identity |
| `unstructured` | true → the structured slots are all null | the claim gets its raw-form canonical bytes + address per hashing spec §4, and opts out of structural merge-direction analysis |
| `canonical` | `{rules_v, subject_registry, form}` | written **only** by the derivation — `req canon --fill` in an existing claim, `req init` for a new corpus's starter claim; `form` is stored as a YAML literal block so the hashed bytes are the stored bytes |
| `address` | `sha256("req.claim.v1\0" \|\| canonical_form_bytes)` | written by the same derivation (`req canon --fill`, or `req init`'s starter claim); `req canon --verify` enforces `sha256(tag \|\| stored form) == address` (REQ-0017) |
| `rationale` | object: `source`, `date`, `rejected_alternatives[]`, `assumptions[]` | all four **required** (an omitted list is a parse error, not an empty one); the losing side is preserved — never a string field |
| `relations[]` | `{type, to}` outgoing only | **required** (an omitted key is a parse error: relations sit outside the canonical form, so a dropped key leaves the address untouched while the edges vanish); incoming relations are always *derived* by traversal, never stored (REQ-0020); `relations: []` = a root claim |
| `tests` | the test ids that prove the claim — **authored**, optional | `[String]` of `classname::name` ids (the `classname` and the test's `name` the JUnit report writes, joined by `::`); **outside the canonical form**, so adding, removing or renaming a link moves no address (ADR 0019); on a **machine** claim (the default) absent or `[]` = no linked test, which the gate names (a finding, never a parse error); under `evidence: process` the list is empty by definition; attested in the signed Baseline by `tests_hash`, hashed over the **sorted** list; an empty or duplicated string in the list is a load error — a link that names no test, or names one twice, is not a link |
| `evidence` | how the claim's evidence is established — `machine` (default) or `process` | authored, optional (omitted = `machine`); `process` is an explicit declaration that the **process itself** establishes the claim — a Decision, a signature, a workflow — and it must carry **no** `tests:` (naming both is a load error: machine or process, never both); **outside identity**, attested together with the links by `tests_hash`; an empty `tests:` list never implies `process` — that is the *no linked test* finding, not a declaration |

Reference types (`to`, `source`) use `claim_id`s, addresses, or repo paths —
whatever stays stable under rewording.

## Test links and process evidence (ADR 0019)

`tests:` is authored and **outside the canonical form**: no address moves when a link is added,
removed or renamed. What the Baseline attests instead is the whole declaration — `tests_hash` =
`sha256("req.tests.v1\0" ‖ serde_json of {"evidence": "machine"|"process", "tests": <sorted ids>})`,
written beside `relations_hash`/`rationale_hash` since envelope v4 — so unlinking a claim, or flipping
its evidence mode, after signing is **FAIL**, exactly as a changed rationale is (ADR 0013 / ADR 0016). A **v3** manifest still verifies on its
historical signed bytes and says once that it predates v4: its tests are *not attested*, which is not a
failure because it never claimed to attest them. The gate reads the links when it is given a run's
report (`req gate --test-report <junit.xml>`), and its `tests_linked` policy answers four ways over the
**applying** claims: **PASS** when the claim links at least one test and at least one linked test
**passed**; **FAIL** when every linked test is present in the report and **none passed** (a `skipped`
test is not a pass); **UNVERIFIABLE — *no linked test*** when the claim carries no `tests:` entry — the
flagship answer, named per claim; and **UNVERIFIABLE — *linked test not in the report*** when a linked
id has no testcase in the report (renamed or deleted), which is never folded into PASS. The last two are
the rename defence: a test id is the test's *identity*, so renaming the test leaves the claim pointing at
an id the report no longer carries, and the answer is "cannot tell", never "passed". `FAIL` outranks any
UNVERIFIABLE and both outrank PASS — the exit code is the existing fold, not a new one. An absent,
unparsable or zero-testcase report is UNVERIFIABLE **once**, naming the reader's reason, never once per
claim.

The four outcomes judge **machine** evidence. A claim whose evidence is the process itself declares
`evidence: process` with `tests: []`: the gate answers it **PASS as declared**, names it
(`* process-evidenced (declared, not machine-proven): …`) under the `tests_linked` block and counts it in
the summary and in `req.gate/1`'s `process_evidenced` array — never folded into the machine pass.
**Process is never inferred**: an empty list on a machine claim is *no linked test*, UNVERIFIABLE, and
naming both process evidence and links is a load error (exit 3). With only process claims in scope, the
gate needs no `--test-report`.

## The parity invariant (what `req canon` must check)

The statement is the source of truth; the structured slots are an **authored parse**
of it. `req canon` re-parses the statement under the current rules and judges `value`,
`unit` and `operator` by four cases, per slot:

| the parse derived it? | the author wrote it? | verdict |
|---|---|---|
| yes | yes, and they disagree | **flag** — a contradiction |
| yes | no | **flag** — an omission |
| no | yes | **annotation** — counted and shown, never a flag |
| no | no | ok |

A contradiction or an omission is a lint ERROR — never silently "corrected" in either
direction, and `req canon --fill` refuses to write an address over it. An annotation is the
other case: the canonical form carries value/unit/operator **only for quantities**, so a
qualitative statement's authored triple is recorded rather than refused (ADR 0014's
Amendment). It appears in a cell of its own (`ann`), is counted in the summary, and was
never part of the address — reported and counted, never a flag, never silent.

The annotation is **only the qualitative triple** above. A quantity-shaped claim — a numeric
authored `value`, a comparator other than `EQ`, or an authored `unit` — that authors a slot
the parse did not derive is still a **contradiction and a flag**: the bound would otherwise
stay out of the address, and two claims differing only in it would collapse (a false merge).
`--fill` refuses it, exactly as before the amendment. `attribute` is **unverified by
design**: its lexicon has no parser to agree with, so comparing it would flag nearly every
claim and teach authors to ignore the report.

If no deterministic parse exists, the claim is re-filed with `unstructured: true` after lint
records why. This is the seam where writing a claim file *can be proven* against a
claim file, before merge direction analysis is trusted.

## Authoring rules

- One obligation per claim. A "shall … and shall …" statement is two claims, split
  before filing.
- Negation rides on `modality: SHALL-NOT`, never on reworded semantics, and a qualitative
  predicate is an authored **annotation**: record it as an `attribute`/`operator`/`value`
  triple with operator `EQ`, `unit: none`, and a non-numeric `value` ("shall not run a
  continuous sync" → attribute `continuous_bidirectional_file_sync`, operator `EQ`, value
  `absent`; "the unit of work is a claim" → `unit_of_work = claim`). `operator: EQ` is the
  form to author; `operator: none` or an omitted operator is the same no-operator marker and
  reads as the same annotation. The canonical form
  derives value/unit/operator only for quantities, so such a triple is not identity:
  `req canon` reports it as an annotation (`ann`), never as a flag and never as part of an
  address. Do not delete the triple to silence the report, and do not expect it to be
  verified — a non-numeric value is never compared with the parse.
- Values are **exact decimal strings** *in canonical form*: strip trailing zeros at
  write time (`"0.3"`, never `"0.30"`, never a float); never rounded, never truncated
  (hashing spec T1.3/T1.4). In a quantity claim, a non-canonical authored value is exactly
  what `req canon`'s flag exists to catch; in the qualitative convention above the value is
  a state name, and it is an annotation rather than a flag.
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
  them is the failure this project treats as catastrophic. `tests` is the deliberate
  exception (ADR 0019): it is **optional**, because "this claim has no linked test" must be a
  fact the gate can *name* — exit 3, UNVERIFIABLE — rather than a parse error, and because a
  claim whose proof does not exist yet is not a malformed claim. The omission is still not
  silent: `tests_linked` reports it on every run, per claim. `evidence` is optional for the opposite
  reason: its absence has a defined meaning (`machine`), so a required key would add nothing — while
  `process` is never a default, because it is a claim about the process that only a human can make.

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

- Formal predicate syntax for `applicability` (v1: informal strings).
- A formal timestamp authority for the signing time (RFC 3161). Envelope v3 (ADR 0016)
  records `signed_at` and the Decision-log head; the time is the signer's assertion, and
  replacing it with an externally attested one is still open. (The envelope itself is no
  longer deferred — this build writes **v4** (ADR 0019 adds `tests_hash`), and a v3
  manifest still verifies on its historical signed bytes with its tests *not attested*.)
- A lint rule comparing a claim's statement against its `rationale.source` ADR
  narrative — the human keeps them consistent for now; the check is proposal-tier
  work.
