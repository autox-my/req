# Claim-file format — v2

Status: `req.claim/2`. Implements docs/adr/0002-git-is-transport-not-identity.md
(one claim per file), docs/adr/0019-claims-carry-authored-test-links.md (authored test
links, attested and outside identity), docs/adr/0022-claims-are-authored-fingerprints-are-derived.md
(a claim is authored, its parse is derived at load, no fingerprint is stored) and
docs/canonical-hashing.md (identity, tiers, freeze).

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
state, and since `req.claim/2` they store no fingerprint either: approval is derived from
`decisions/DEC-*.yml` and from the signed Baseline, and the address is derived from the claim
when it is loaded — neither is copied into claim files. The signed manifest is where an
address *is* stored, per claim, and drift against it is the check that replaced the stored
digest (ADR 0022).

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
| File format | `schema: req.claim/2` | a migration; addresses unchanged — v2 removes the authored slots and the stored fingerprint (ADR 0022), an older declaration is a migration candidate (exit 3, `tools/migrate_claims.py`), and no `RULES_V` moves |
| Normalization rules | the build (`RULES_V`, `req --version`) | moves the derived addresses of affected claims → *candidates* needing re-ratification, seen as drift against the signed manifest (hashing spec §5); history never silently rewritten |
| Subject registry | `aliases/subjects.yml` version | same effect as a rules bump, scoped to claims using affected subjects |

The rules and registry versions are **properties of the build and the registry**, reported by
`req --version` and carried in every manifest and projection header; a claim file names
neither, because it stores nothing they produce.

The declared `schema:` is **judged when a file is loaded**, first, against the value this build
writes (`CLAIM_SCHEMA_V` in `src/canon.rs`, reported by `req --version` as `claim_schema`). A file
declaring an *older* format is a migration candidate — the message names
`tools/migrate_claims.py`; a *newer* one is a question this build cannot answer, because the file
is not wrong — the reader is old; and a value that is not of the `req.claim/<n>` shape at all is a
broken file, not a version question. All three are **NOT JUDGED** (exit 3): nothing inside the
file was compared to anything, and the declared format is what makes the rest of it readable as
a claim of this build's contract. The judgement lives in `parse_claim` (`src/canon.rs`), which the
loader calls and so does every reader that reads a claim by path (`compare`, `propose`) — one
home, so no command can ignore the declaration.

The class is assigned by the **load loop**, not by the schema check alone: an equally definite
**duplicate `claim_id`** — two files declaring one id, their content compared — is UNVERIFIABLE
(3) in the same loop (`load_claims` in `src/canon.rs`), and every command that reads the corpus
reports it the same way. The two classes are therefore **coupled**: if either ever moves to FAIL
(1), the other must be revisited in the same change, and neither should move alone.

## Field reference

A claim file carries what a human **authors**, and nothing derived.

```yaml
schema: req.claim/2
claim_id: REQ-0009
statement: >-
  The benchmark harness shall classify direction of change on the structural
  direction gold set with accuracy of at least 0.95.
applicability:
  - set == direction_gold_structural
tests:
  - "req::evidence::the_direction_gold_set_meets_its_accuracy_target"
evidence: machine
rationale:
  source: docs/canonical-hashing.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations:
  - type: refines
    to: REQ-0008
pins:
  value: "0.95"
  operator: GE
```

| Field | Meaning | Notes |
|---|---|---|
| `schema` | `req.claim/2` | judged first, by direction (above) |
| `claim_id` | stable human label | never derived — it survives rewording (CONTEXT.md) |
| `statement` | the human sentence | **the source of truth**: its parse, canonical form and address are derived from it at load |
| `applicability[]` | variant predicates (`applies_when`) — the **selector slot** | **required** (an omitted key is a parse error, never `[]`); part of identity (ADR 0003): an *input* to the canonical form, because the statement carries no formal selector syntax — the sorted authored list is hashed |
| `relations[]` | `{type, to}` outgoing only | **required** (an omitted key is a parse error: relations sit outside the canonical form, so a dropped key leaves the address untouched while the edges vanish); incoming relations are always *derived* by traversal, never stored (REQ-0020); `relations: []` = a root claim |
| `tests` | the test ids that prove the claim — **authored**, optional | `[String]` of `classname::name` ids (the `classname` and the test's `name` the JUnit report writes, joined by `::`); **outside the canonical form**, so adding, removing or renaming a link moves no address (ADR 0019); on a **machine** claim (the default) absent or `[]` = no linked test, which the gate names (a finding, never a parse error); under `evidence: process` the list is empty by definition; attested in the signed Baseline by `tests_hash`, hashed over the **sorted** list; an empty or duplicated string in the list is a load error — a link that names no test, or names one twice, is not a link |
| `evidence` | how the claim's evidence is established — `machine` (default) or `process` | authored, optional (omitted = `machine`); `process` is an explicit declaration that the **process itself** establishes the claim — a Decision, a signature, a workflow — and it must carry **no** `tests:` (naming both is a load error: machine or process, never both); **outside identity**, attested together with the links by `tests_hash`; an empty `tests:` list never implies `process` — that is the *no linked test* finding, not a declaration |
| `rationale` | object: `source`, `date`, `rejected_alternatives[]`, `assumptions[]` | all four **required** (an omitted list is a parse error, not an empty one); the losing side is preserved — never a string field |
| `pins` | what the parse **must** be, where it is load-bearing — optional | an object with any of `modality`, `subject`, `operator`, `value`, `unit` (any other key is a parse error); judged against the derived parse, never part of identity (see *Pins* below) |

Reference types (`to`, `source`) use `claim_id`s, addresses, or repo paths —
whatever stays stable under rewording.

## The derived view (computed at load, stored nowhere)

When a claim is loaded, `canonicalize_statement` parses its `statement` against the corpus's
`aliases/subjects.yml`, the authored `applicability` is injected, and the loader exposes what
that **derives**: the canonical form, its canonical bytes (T1.9), the address
`sha256("req.claim.v1\0" ‖ canonical bytes)`, the lints, and whether the statement is
**unstructured** (the §4 fallback, as REQ-0007 is). Every command that reads an address —
`render`, `trace`, `impact`, `compare`, `baseline` (emit, verify, drift), `audit`, `gate`,
`init` — reads this one. `req canon` prints it per claim:

| derived | meaning |
|---|---|
| structure | `structured`, or `unstructured` when no deterministic parse exists (the raw form is hashed) |
| modality | SHALL / SHALL-NOT / MAY / SHOULD — hashing spec T1.6 |
| subject | the registry id the subject span resolves to (whole span only, T1.7); an unregistered surface stays literal |
| operator / value / unit | the bound — operator per T1.5 {LT, LE, EQ, GE, GT, NE}; value a canonical exact decimal in canonical SI (`200 ms` → `0.2` `s`) |
| conditions | guard clauses parsed from the statement (when/if/unless/during), sorted at T1.8 — part of identity |
| lints | what the rules could not decide (`untestable_comparator`, `uncovered_content`, …) — reported, never guessed (REQ-0019) |

There is nothing to fill and nothing to verify: no command writes a derived field, and no file
can carry a stale or hand-written one. A rules bump moves derived addresses at the next load —
which is why a rules bump is still a deliberate, recorded event (hashing spec §5), seen as drift
against the signed manifest rather than as a rewrite of every claim file.

## Pins

A pin is the author's statement of what a claim must parse to, where the parse is
load-bearing — the bound of a quantity, its comparator, its unit. It is judged against the
derived parse, and there are exactly two outcomes besides agreement, **both flags**:

| the parse holds a value for the key? | equal to the pin? | verdict |
|---|---|---|
| yes | yes | **agrees** |
| yes | no | **flag — contradiction**: the parse says something else, and the flag names it |
| no | — | **flag — not carried**: the parse holds no such value (an unstructured statement, or one with no comparator, number or unit) |

There is **no annotation category** (ADR 0022 removed it): a pin the parse does not carry is
exactly the bound that would otherwise leave identity in silence. Values are compared
**exactly**: `value` is a canonical exact decimal string in the parse's canonical unit (`"0.2"`
for `200 ms`, never `"0.20"` or `".2"`), `unit` is the canonical unit symbol (`s`, not `ms`) or
`none`, and `modality`/`operator` are the canonicalizer's own words. A pin value that could
never agree — a non-canonical decimal, a comparator outside T1.5 — is a **load error**, not a
flag whose cause is a typo. `unit: none` agrees with a unitless parse.

A flagged pin **moves no address** — pins are outside the canonical form. It reports: `req
canon` prints each pin's verdict (and exits 0, it never gates), and the release gate's
`pins_hold` policy fails (exit 1) on any flagged pin of an applying claim, naming it. With no
pin in the selection, `pins_hold` passes and says it held zero pins — exercised, not skipped.

## Authoring rules

- One obligation per claim. A "shall … and shall …" statement is two claims, split
  before filing.
- Edit the statement, never a parse: there is no authored parse to edit. To see what a
  statement derives, run `req canon`; to make a load-bearing slot checked, pin it.
- Pin the bound of a quantity claim. A numeric threshold, its comparator and its unit are
  what a rewording most easily moves without anyone noticing; a pin turns that into a
  `pins_hold` failure. Pin values are **exact decimal strings** *in canonical form* (`"0.3"`,
  never `"0.30"`, never a float); never rounded, never truncated (hashing spec T1.3/T1.4).
- Negation rides on the statement's modality (`shall not` → `SHALL-NOT`), never on reworded
  semantics; pin `modality: SHALL-NOT` where the prohibition is load-bearing.
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
  and `claims: 0` must never read as success.
- **Unique ids.** Two files declaring one `claim_id` is a load error, and their
  **content is compared**: an identical duplicate (the fully silent case — it never
  shows as drift) and a divergent one are named differently. The duplicate pass runs
  over every candidate before the naming pass, so a second file claiming an existing
  id is reported as the duplicate it is, not merely as a filename complaint.
- **Strict fields, at every level.** An unrecognized key is a parse error, in the claim
  file and in its nested objects (`rationale`, `relations[]`, `pins`): `applicabilty:`
  does not parse as an empty default, and neither does a misspelled
  `rejected_alternatives:` inside `rationale`. A key `req.claim/1` carried and `req.claim/2`
  removed (`modality`, `subject`, `attribute`, `operator`, `value`, `unit`, `conditions`,
  `exception`, `temporal`, `quantifier`, `unstructured`, `canonical`, `address`) is a parse
  error that **names the key** and says it was removed — never silently ignored, because an
  author who wrote `value: "0.3"` beside the statement meant something, and the place to say
  it now is `pins:`. The keys that carry meaning *outside* the canonical form are
  **required**, because losing one moves no address and nothing else would notice:
  `applicability` (a variant-scoped claim would otherwise become unconditional —
  applicability is an input to identity, so that is two requirements collapsed into one),
  `relations` (the edges would vanish from `trace` and `impact`), and the rationale's
  `rejected_alternatives`/`assumptions` (the losing side of a decision would be dropped from
  the record). Writing them as `[]` is how a claim says "none"; omitting them is the failure
  this project treats as catastrophic. `tests` is the deliberate exception (ADR 0019): it is
  **optional**, because "this claim has no linked test" must be a fact the gate can *name* —
  exit 3, UNVERIFIABLE — rather than a parse error, and because a claim whose proof does not
  exist yet is not a malformed claim. The omission is still not silent: `tests_linked`
  reports it on every run, per claim. `evidence` is optional for the opposite reason: its
  absence has a defined meaning (`machine`), so a required key would add nothing — while
  `process` is never a default, because it is a claim about the process that only a human
  can make. `pins` is optional because most claims have no load-bearing quantity.

Directory entries that are not claims (`README.md`, editor droppings) are left alone;
that is a naming rule, not a silence. One deliberate exception runs the other way: an
entry whose *name* is not valid UTF-8 cannot be classified at all — it might be a claim
written in some other encoding — so it fails the corpus rather than being skipped.

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
the summary and in `req.gate/2`'s `process_evidenced` array — never folded into the machine pass.
**Process is never inferred**: an empty list on a machine claim is *no linked test*, UNVERIFIABLE, and
naming both process evidence and links is a load error (exit 3). With only process claims in scope, the
gate needs no `--test-report`.

## Migrating from `req.claim/1`

A `req.claim/1` file is a migration candidate: every command reports it NOT JUDGED (exit 3) and
names the tool. `tools/migrate_claims.py` (a starting point an adopter copies, like
`tools/adopt.py`; dry run by default, `--write` to rewrite, `--self-check` for its table) moves a
file across:

| `req.claim/1` | `req.claim/2` |
|---|---|
| `schema`, `claim_id`, `statement`, `applicability`, `relations`, `tests`, `evidence`, `rationale` | kept, byte for byte (`schema` rewritten to `req.claim/2`) |
| a numeric `value`, an `operator` other than `EQ`/none, a `unit` other than `none` | `pins:` — the bound the author wrote is still checked against the parse |
| `modality`, `subject`, `conditions`, `unstructured` | dropped — derived from `statement` at load (pin `modality`/`subject` by hand where load-bearing) |
| the qualitative `attribute`/`value` triple | dropped — not representable in the form, and the annotation category is removed |
| `exception`, `temporal`, `quantifier` | dropped — never represented; authoring one is now a parse error |
| `canonical`, `address` | dropped — computed at load |

The tool prints, per file, every key it dropped with its value and every pin it created, so the
drops are reviewed rather than trusted. It refuses a file that is already `req.claim/2`. A format
bump moves no address: the derivation, the canonical bytes and `RULES_V` are unchanged, which this
repository's own migration proved by keeping all 21 dogfood addresses and its signed Baseline
`[VERIFIED]` under the unchanged signature (M3, rules v6).

## State of the seed corpus

`claims/REQ-0001 … claims/REQ-0022` (less REQ-0011, retired by ADR 0021 and not renumbered) were
authored **structured-first** under `req.claim/1` and migrated to `req.claim/2` by
`tools/migrate_claims.py` (ADR 0022) with **no address moving** — the 21 addresses `req render`
derives at rules v6 equal the ones the v1 files stored, and `baselines/air-gapped.json`, which
attests 17 of them, still verifies under its original signature. The migration created six pins
on four claims — REQ-0008 (`value`), REQ-0009 (`value`, `operator`), REQ-0010 (`value`,
`operator`), REQ-0022 (`value`) — all of which agree with the derived parse. REQ-0007 is derived
unstructured, as it was authored unstructured. Rules v6's coverage invariant (hashing spec T1.10)
refuses a structured parse that leaves a clause no slot holds, which is why the "exactly once" half
of the old REQ-0020 is its own claim, REQ-0022 (one obligation per claim). REQ-0004 and REQ-0021
state their obligation without the head "semantic layer", so their subject is the registered
`REQ_TOOL`; the registry matches a whole subject span only, and no alias was added for the dropped
head.

REQ-0017's statement still describes the `req.claim/1` mechanism (a stored canonical form whose
digest reproduces a stored address). Re-wording it moves its address, which drifts the signed
Baseline: that is a Decision and a re-signature, recorded as a follow-up rather than done in the
format change.

`claims/` also seeds `bench/corpus/parity.yml` (E-008/E-009 come from this project's own
genuine paraphrases).

## Deliberately deferred

- Formal predicate syntax for `applicability` (v2: informal strings).
- A formal timestamp authority for the signing time (RFC 3161). Envelope v3 (ADR 0016)
  records `signed_at` and the Decision-log head; the time is the signer's assertion, and
  replacing it with an externally attested one is still open. (The envelope itself is no
  longer deferred — this build writes **v4** (ADR 0019 adds `tests_hash`), and a v3
  manifest still verifies on its historical signed bytes with its tests *not attested*.)
- A lint rule comparing a claim's statement against its `rationale.source` ADR
  narrative — the human keeps them consistent for now; the check is proposal-tier
  work.
