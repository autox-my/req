# 0019. Claims carry authored test links: attested, never identity

**Status: accepted** — the human's ruling of 2026-10-22: a claim's link to the tests that prove it is
**authored on the claim, attested in the signed Baseline, and never part of identity**. It was chosen
from three options — a separate mapping file, a naming convention, and the claim's own field — and the
reason is one sentence: a test rename must never move a claim's address, while the evidence must ride
the signed Baseline so an agent cannot quietly unlink what it cannot prove. Date: 2026-10-22.

**Amended 2026-10-22** — the same ruling adds a declared **evidence mode**: a claim says
`evidence: machine` (the default) or `evidence: process`, and the declaration rides the same attested
digest as its links. §7 records it; the four outcomes are unchanged.

## Context

The question the gate could not answer is the flagship one: **the agent says done; req says which claims
have no passing test.** Before this decision, nothing in the model carried that fact. `rationale_present`
asks for a source and a date — provenance, not evidence. `audit` reports coverage against a signed
Baseline, and coverage is not test coverage. A claim with no test at all and a claim whose test has been
deleted were the same silence, and a silence in a green build reads as success.

The ruling was amended the same day, for the other half of the same question: some claims are
established by the **process itself** — a recorded Decision, a signature, a workflow — and no test can
prove them. A claim must be able to say so, because an empty `tests:` list is a *missing* answer, never a
declaration.

The facts that shape the answer were read from the code rather than assumed:

- The claim-file loader **rejects unknown fields** (`src/model.rs`): adding a `tests:` key to a file
  without teaching the parser is a parse error, so the field must be parsed, not tolerated.
- The file-format version is `req.claim/0` (`CLAIM_SCHEMA_V`, `src/canon.rs`), and the version axes
  (docs/claim-format.md) say a **format bump moves no address**. There is nothing to re-hash and no
  `RULES_V` bump.
- The Baseline envelope is v3 (`BASELINE_VERSION`, `src/baseline.rs`) and a manifest claim record already
  attests the claim's **audit surface** beside its address — `relations_hash` and `rationale_hash`
  (ADR 0013), over parsed values, domain-separated by `req.relations.v1\0` / `req.rationale.v1\0`.
- The gate is a list of `PolicyResult`s with the tri-state `Outcome` and one fold: **FAIL outranks any
  UNVERIFIABLE, and both outrank PASS** (`src/gate.rs`, ADR 0015). A new policy is a sibling of
  `rationale_present`, not a new kind of answer.
- A machine-readable test report already exists in this toolchain and is worth adopting for that reason
  alone: nextest writes a JUnit XML under a reviewed profile. It is **not** faster than `cargo test`
  (measured 2026-10-21: warm `cargo test` 2 s vs `cargo nextest run` 2–3 s, both paying the ~24 s link,
  the same 378 tests, zero doctests).

## Decision

### 1. The fields: `tests: [String]` and `evidence:`, authored and optional

A claim may carry `tests:`, a list of **test ids** — `classname::name`, the pair the JUnit report's
`<testcase>` carries, joined by `::`. It is authored like every other authored slot — a
human writes it, and it rides the same review as a `rationale`. It is optional: an absent list (or an
empty one) means **no linked test**, which is a *finding the gate names*, never an exemption and never a
parse error. The loader refuses an empty or duplicated **string** in the list, because a link that names
no test, or names one twice, is not a link.

A claim also declares **how** its evidence is established — `evidence: machine` (the default when omitted)
or `evidence: process` (§7). The declaration is authored, optional, outside identity, and attested
together with the links.

### 2. The hash: attested beside the address, outside the canonical form

`tests` is **not** in the canonical form. Adding, removing or renaming a link therefore moves no
address — that is the ruling, not an implementation detail, and a test pins it.

What the Baseline attests instead is the declaration and its links:

```
tests_hash = sha256("req.tests.v1\0" ‖ serde_json::to_string(
                 {"evidence": "machine" | "process", "tests": [<sorted ids>]}))
```

- The declaration is hashed **first**, and it is inside the digest on purpose: without it, flipping
  `machine` to `process` and dropping the links would move neither the address nor the attested list —
  exactly the quiet unlink the Baseline exists to catch;
- the ids are **sorted at hash time** (the list's authored order is not meaning);
- the writer is the plain `serde_json` serialization of that parsed declaration — the same writer ADR 0013
  names for `relations_hash`/`rationale_hash`, not `canon::canonical_json`, which serializes the canonical
  form and cannot carry this;
- the tag is NUL-delimited (`req.tests.v1\0`), so one surface's bytes can never straddle another's. The
  field and the tag keep the name `tests_hash`; the digest attests the whole declaration.

`tests_hash` is computed **from the parsed claim at load** (`Row`) and written by `baseline --at`
**beside** `relations_hash`/`rationale_hash`. `baseline --verify` re-derives it. A test list or evidence
declaration changed after signing is **FAIL**, the same class as a changed address or a changed
rationale.

### 3. The file format moves to `req.claim/1`

Adding the fields is a format bump: `CLAIM_SCHEMA_V` becomes 1 and every claim file's `schema:` line
moves `req.claim/0` → `req.claim/1`. The move is mechanical — **no address changes** — and it is what
makes the new fields readable by a build that writes them. A `req.claim/0` file is a **migration candidate**, exit 3
(NOT JUDGED) through the class the loader already has (`claim_schema_reason`, `src/canon.rs`): the file is
not wrong, the reader is newer, and the remedy is a re-fill, never a retype.

### 4. The envelope moves to v4

`BASELINE_VERSION = 4`. The version is part of the signed bytes, so it is bumped deliberately: an
envelope change is not a re-interpretation of old signatures.

- A **v4** manifest attests each claim's `tests_hash`; `--verify` re-derives it and a mismatch is FAIL.
- A **v3** manifest still verifies **on its historical signed bytes** — nothing about v3 is rewritten —
  and `--verify` prints **once** that it predates v4, so its tests are **not attested**. That is not a
  FAIL: a v3 signature never claimed to cover test links, and absence is not tampering (ADR 0016's rule
  for absent evidence). A release that wants test attestation re-issues and re-signs as v4.
- A v4 manifest read by a v3-only build is the existing **"newer envelope"** UNVERIFIABLE — verify with
  a build that writes it — not a new class.

### 5. The policy: `tests_linked`, four outcomes and no fifth

`req gate [--test-report <junit.xml>] [key=value ...]` gains the flag and its sibling policy
`tests_linked`, over the **applying** claims only, exactly like `rationale_present`. The report is an
**input, not a verdict**: the gate still decides. The four outcomes below judge **machine** evidence; a
claim that declares `evidence: process` is answered by its declaration (§7) — a reported fact, not a
fifth outcome.

| outcome | condition | fold |
|---|---|---|
| **PASS** | the claim links at least one test and at least one linked test **passed** | contributes pass |
| **FAIL** | every linked test is present in the report and **none passed** (a `skipped` test is not a pass) | exit 1 |
| **UNVERIFIABLE — *no linked test*** | the claim carries no `tests:` entry — the flagship answer, named per claim | exit 3 |
| **UNVERIFIABLE — *linked test not in the report*** | a linked test id has no testcase in the report (renamed or deleted) — never folded into PASS | exit 3 |

Three rules hold the table together:

- **A `skipped` test is not a pass.** It is present and it did not run; folding it into PASS is how a
  report that proves nothing turns a release green.
- **The report is judged once, not per claim.** An absent, unparsable, or zero-testcase report is
  UNVERIFIABLE **once**, naming the reader's reason, never repeated for every applying claim — noise is
  how a real red gets ignored.
- **`FAIL` outranks any UNVERIFIABLE, and both outrank PASS** — the fold `gate.rs::exit_code` already
  applies. One consequence inside a single claim: a **failed** linked test outranks a passing one, so
  one passing link does not launder a failing one and the claim is FAIL; a **skipped** link beside a
  passing one is not a failure, because a skipped test is simply not a pass. Both are the code's
  behaviour and are pinned by tests. No new exit code is invented: 0 every linked test passed and every
  claim decided, 1 a defect, 3 something that cannot be confirmed.

The **rename defence** lives in the fourth row. A test id is the *identity* of the test, so renaming the
test leaves the claim pointing at an id the report no longer carries: the answer is "cannot tell", never
"passed". Folding a missing linked test into PASS would convert every rename into a silent green.

### 6. The run: nextest for the artefact, not for speed

*Amended by [ADR 0024](0024-test-evidence-is-a-set-of-reports.md) (§6): nextest is no longer the only producer of the report, and `--test-report` may be repeated.*

`cargo test` cannot produce the report, so the suite runs under **nextest** with a reviewed
`.config/nextest.toml`:

```toml
[profile.ci]
fail-fast = false

[profile.ci.junit]
path = "junit.xml"
report-skipped = "ignored"
```

`cargo nextest run --profile ci` writes `target/nextest/ci/junit.xml`; that path is what
`req gate --test-report` reads. `fail-fast = false` keeps one failing test from hiding the rest
(its CLI twin `--no-fail-fast` is redundant — the profile is the single home), and
`report-skipped = "ignored"` makes a skipped test *present* in the report as skipped rather than
absent from it. That key belongs **under `[profile.ci.junit]`**: in 0.9.149 the top-level spelling
is silently ignored with a warning, which would drop every `#[ignore]`d test from the artefact —
the exact "absence reads as a pass" failure this record forbids. The value is `"ignored"` and not
`"all"`, because `"all"` would also list tests skipped by filters or partitions. Without those two
settings the four outcomes could not be observed honestly.

nextest is pinned at **0.9.149**, installed with
`cargo install cargo-nextest --version 0.9.149 --locked` and verified at pin time — a CI-side pin
like `tools/ocr`'s, **not** a `Cargo.lock` change (a lockfile bump is a Tier-1 rules bump). It is
adopted for the **JUnit artefact, not for speed**: measured 2026-10-21, a warm `cargo test` took
2 s and `cargo nextest run` 2–3 s, both paying the ~24 s link, over the same 378 tests.

**`cargo test` is replaced, not kept beside it** — one run, one truth; two test commands would be two
claims about what the suite is. One gap is named rather than hidden: nextest does not run doctests, so a
future `#[doc]` example needs its own step in the gate. Today the suite has zero doctests, so the
replacement loses nothing measurable.

### 7. Process evidence: a declared fact, not a fifth outcome

Some claims are established by the process rather than by a test: the Decision that ratified the
repository's authority model, the signature on a Baseline, the workflow that gates a merge. No test can
prove them, and pretending otherwise produces a link that proves nothing. The claim says so, explicitly:

```yaml
evidence: process
tests: []
```

- `evidence` is authored and optional; **omitted means `machine`**, the honest default.
- **`process` is never inferred.** An empty `tests:` list on a machine claim is *no linked test* —
  UNVERIFIABLE, exit 3 — not a process pass. The default is load-bearing: silence is not a declaration,
  and a mutation check (empty the list, keep the default) exists to prove the gate never reads one as the
  other.
- **The contradiction is refused.** `evidence: process` with a non-empty `tests:` list is a corpus load
  error (exit 3): a claim's evidence is machine (proven by its linked tests) or process (declared), never
  both. A link nobody runs and a declaration nobody can test are both worse than the honest single claim.
- **It is attested** by the same `tests_hash` (§2), so flipping `machine` to `process` — or back — is a
  changed digest after signing, i.e. FAIL, rather than a silent reclassification.
- **It is reported, not folded in.** Under the `[PASS] tests_linked` block the gate prints
  `  * process-evidenced (declared, not machine-proven): REQ-A, REQ-B`, and the summary carries
  `process-evidenced: <n> — declared, not machine-proven: <ids>`. `req.gate/1` gains a top-level
  `process_evidenced` array (sorted claim ids) beside its six `policies` — `tests_linked` after
  `rationale_present`. A consumer reading only the machine count would otherwise overstate what was
  proven.
- **The report is needed only when something machine-evidenced is in scope.** A corpus whose applying
  claims all declare `process` passes with **no** `--test-report` (nothing needs a report); with any
  machine claim, no report is still UNVERIFIABLE, reported once. Process evidence is a fifth *reported
  fact*, never a fifth *outcome*: an outcome answers "did the machine evidence pass", and a declaration
  answers a different question.

The reason this is a declaration and not a `tests:`-shaped convention: the whole point of `tests:` is that
it names evidence a machine can check. A claim that cannot be checked that way says so in words a human
authored and a reviewer reads — the same bargain as a `rationale`, and the same reason a wrong link is
worse than no link.

## Consequences

- **Every claim file migrates** its `schema:` line; `canon --verify` must stay 0 on every claim after the
  move, because no address moved. A `req.claim/0` file left behind is a migration candidate (exit 3),
  which is the loader's existing answer, not a new failure.
- **A Baseline must be re-issued and re-signed to attest tests.** The v3 signatures keep verifying, and
  the gate prints that their tests are not attested. The signing act is the human's (ADR 0010); req emits
  and verifies.
- **The release gate now needs the report.** With no `--test-report`, `tests_linked` cannot be exercised,
  and a policy that cannot be exercised is not a pass: a bare `req gate` is **UNVERIFIABLE (exit 3)** —
  unless every applying claim declares `evidence: process`, in which case nothing machine-evidenced is in
  scope and no report is needed. That is the one visible cost to adopters, and it is the same rule that
  keeps a skipped check out of a green build. `scripts/check.sh` and every CI invocation gain the flag; the CI snippet changes in its
  three byte-checked copies, so `tests/ci_template.rs` reddens until all three agree.
- **The dogfood work is real work.** Linking req's own claims to the tests that prove them is done by
  hand, claim by claim; a claim whose proof does not exist is left unlinked and named by the gate. The
  declared variants exited 3 until the links existed, and the count fell by adding real links, never by
  relaxing the policy.
- **No `RULES_V` bump, no `Cargo.lock` change, no address moves.** The new reader for nextest's JUnit
  subset is hand-written (`src/junit.rs`), so the core gains no dependency and still no network.
- **A test rename now needs an edit to the claim, or the gate says UNVERIFIABLE.** That is the price of
  the rename defence, and it is the affordable direction: the failure is a question, never a false green.
- **A corpus with no tests at all cannot be green.** Every applying claim is UNVERIFIABLE — *no linked
  test* — which is the flagship answer stated plainly, not a defect being punished. Claims the process
  itself establishes declare `evidence: process` instead, and are reported PASS-as-declared and counted;
  that is a different answer, not an exemption from this one.
- **The declaration is reviewed like the link.** A human writes `evidence: process`; a reviewer asks
  whether the process really establishes the claim, because the gate can check only that the declaration
  is consistent (no links) and attested, never that it is true.

### Dogfood dispositions (2026-10-22)

The links are claims about evidence and were made one by one:

- **machine, with a test added to `tests/evidence.rs`:** REQ-0012
  (`the_core_crate_carries_no_network_client` — the dependency graph carries no network/async client and
  `src/` uses no socket or concurrency API); REQ-0015
  (`no_command_starts_a_sync_or_polling_loop` — no runtime or spawned loop in `src/`, and `req --help`
  names no sync/bridge/watch/daemon/poll verb); REQ-0009
  (`the_direction_gold_set_meets_its_accuracy_target` — accuracy at or above 0.95 with zero dangerous
  confusions); REQ-0010 (`the_parity_corpus_meets_its_false_split_target` — false-split rate at or below
  0.30 on a non-zero denominator); REQ-0018
  (`canonical_forms_serialize_quantities_as_exact_decimal_strings` — a canonical `value` is the JSON
  string `"0.3"`, never `"0.30"`, never a float).
- **process:** REQ-0013 and REQ-0014. No proving test exists; both declare `tests: []` and
  `evidence: process`, are reported PASS-as-declared, and are counted in `process-evidenced`. They are
  **not** machine-proven, and the PR says so in those words.
- **machine, linked into the existing suite:** ten claims — REQ-0002…REQ-0005, REQ-0008, REQ-0011,
  REQ-0016, REQ-0017, REQ-0019 and REQ-0020 — each naming a test whose *name states the obligation* it is
  attached to (`a_claims_applicability_is_its_selector`, `fmr_is_zero_on_the_seed_corpus`,
  `a_pass_requires_a_measured_flip_rate_and_no_false_merge`, …). Fourteen originally borrowed a nearby
  test; the strict bar below replaced four of those, and the review caught one borrowed link directly
  (`impact`'s traversal test builds rows in memory, so it would stay green if the loader began refusing
  derived claims).
- **machine, written for the claim:** nine claims link tests in `tests/evidence.rs` — the five
  structural/measured ones (REQ-0009, REQ-0010, REQ-0012, REQ-0015, REQ-0018) and four written for claims
  whose evidence had been borrowed (REQ-0001, REQ-0006, REQ-0007, REQ-0021). Each asserts exactly what its
  obligation obliges, through the CLI seam: a three-deep derived chain no command refuses, one command
  that boots a green corpus beside a boundary that refuses to pass on an undecided variant, a claim that
  is one file/one row/one address, and read-only commands that leave the corpus byte-identical.

The 21 dogfood claims are therefore: **19 machine-evidenced with links** (9 in `tests/evidence.rs` and 10
in the existing suite) and **2 process-evidenced** (REQ-0013, REQ-0014). Both declared variants are green
with the real report — 20 applying claims, 0 undecided, `process-evidenced: 2`.

**The bar for a link (ruled 2026-10-22): the strict question — "if this test failed, would the
requirement be rejected?"** A test that merely exercises nearby behaviour does not qualify, and neither
does one the claim's author did not choose for it. Under the loose bar ("it exercises the behaviour")
fourteen borrowed links were acceptable; under the strict bar four were not, and those four were replaced
with tests written for the obligation rather than the link relaxed. The same bar applies to every future
claim: a machine claim with no such test is UNVERIFIABLE, named, and red — that is the product.

## The honest limit

A link is a **claim about evidence**, not evidence. The gate attests the report it was given; it does not
run the tests, and it cannot tell whether the named test proves the claim it is linked to. A fabricated or
mistaken link passes the policy as long as some passing testcase carries that id, which is why **a wrong
link is worse than no link**: no link is UNVERIFIABLE and says so, while a wrong link manufactures
confidence. The links are therefore reviewed as claims — the dogfood links are reviewed one by one, and
"this test proves this requirement" is a human's judgement, not the tool's.

Three things follow, and none of them is a defect to be papered over:

- **The report is not signed, and the run is not attested.** Re-running the suite changes the report
  without touching the Baseline; the Baseline attests the *links*, not a run of the tests.
- **Unlinked is UNVERIFIABLE, not FAIL.** The flagship answer is "which claims have no passing test",
  named per claim, exit 3. A run in which every unlinked claim were a hard failure would push authors to
  link something — anything — to get green, which is the failure mode this decision exists to avoid.
- **A passing test proves what the test asserts**, not that the claim holds. The link records which test
  the project relies on; that reliance is ratified by a human (ADR 0004).

## Falsification

This decision is wrong if any of these is observed:

1. adding, removing or renaming a `tests:` entry moves the claim's `address` — the ruling is violated;
2. a claim with no link reads as PASS (drop the *no linked test* branch and the suite must redden);
3. a `skipped` test reads as PASS;
4. a renamed linked test reads as PASS instead of UNVERIFIABLE — the rename defence is not built;
5. a changed `tests_hash` after signing verifies, or a v3 manifest is FAILed for not attesting tests;
6. an unreadable report is reported once per claim instead of once, or reads as "no failures found";
7. `cargo test` still runs beside nextest — two runs, two truths;
8. an empty `tests:` list on a machine claim reads as process-evidenced (the never-inferred rule);
9. `evidence: process` with a non-empty `tests:` list parses instead of failing the corpus;
10. a corpus with an applying machine claim passes with no `--test-report`, or a process claim is counted
    as machine-proven.

## Alternatives considered

1. **A separate mapping file (`tests.yml`, claim id → test ids).** Two homes for one fact, and the second
   is not the signed one: unlink after signing would be invisible unless the mapping carried its own
   signature, which doubles the human's signing act — ADR 0016's argument against a separate
   `authority.json`, applied again. An agent could also drop a line from an unsigned mapping without
   touching any claim, and not a single address would move.
2. **A naming convention** (test names carry the claim id, or claims name tests by prefix). Not authored:
   the evidence would be *derived from names at read time*, so the Baseline could not attest it. Renaming
   either side silently converts a linked claim into an unlinked one, and the manifest cannot tell. It
   also makes "which test proves this" a fact about spelling, which is exactly what this project's
   identity work refuses: a claim id is a label, never identity.
3. **Put `tests` into the canonical form.** Then a test rename moves the claim's address — the one thing
   the ruling forbids. Identity is meaning (ADR 0007); test names are not meaning.
4. **Make `tests` required.** Every existing file and every adopted draft would need an edit to parse, and
   a fact that is not part of the claim's assertion would become a parse error. Absence is a finding the
   gate names, not a malformed claim.
5. **Attest a hash of the whole report instead of per-claim links.** That attests that *a* report existed,
   not which claim relies on which test — and it would move the signed manifest on every unrelated test
   edit, the drift ADR 0013 rejected in another shape.

## References

- ADR 0004 (the model proposes, a human disposes), 0007 (identity collapses only on meaning-preserving
  rules), 0010 (a signed Baseline, and req never signs), 0013 (the manifest attests the audit surface
  beside the address — the pattern `tests_hash` follows), 0015 (undecided is not absent, and the fold's
  ordering), 0016 (a v3 envelope still verifies on its historical bytes).
- `docs/claim-format.md` — the field, the version axes (a format bump moves no address), and the
  loader's contract.
- `docs/canonical-hashing.md` — the freeze policy that keeps `tests` out of the canonical form, and why
  a `Cargo.lock` bump would be a Tier-1 rules bump.
