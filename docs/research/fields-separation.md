# Would a typed claim field stay out of the address while the Baseline still catches a change to it?

Task X-1, experiment 2 (`docs/specs/X-1-checks-and-fields-experiments.md`). Run 2026-11-17 with `req 0.5.0` built from `30f85a5`
(`rules_v 9`, `baseline_envelope 4`, `claim_schema req.claim/3`) and `minisign 0.12`. **`fields:` is not implemented and this
record changes no behaviour of req.** It measures the analogue that exists today and maps the code.

**Question.** Does req's architecture already separate authored metadata from meaning in a way a `fields:` block could reuse,
and where exactly must a field never enter?

**Method.** The probes ran on a temporary clone of this repository (`git clone --no-hardlinks`) in a scratch directory,
with the `req` built from this checkout; this checkout was never edited by them, and the clone was deleted afterwards. The
subject is `REQ-0001` (one claim) against `baselines/air-gapped.json` (36 claims, key `req-baseline-1`, signed 2026-11-11T13:06:52Z).
The address comes from `req trace --format json deployment_mode=air_gapped` (`nodes[].address`): `req canon` does not print it.
`<copy>` stands for the clone's path, and the `corpus root:` line each command prints is dropped.

## 1. The analogue, run for real

### Control (no edit)

```
$ req trace --format json deployment_mode=air_gapped | jq  (REQ-0001 .address)
sha256:f559f5a905e4b8d595d064ec960aa695450c84176f9e1577cf2a060904e89e28
$ req baseline --verify baselines/air-gapped.json
# Baseline baselines/air-gapped.json: 36 claim(s), rules v9, key req-baseline-1, signed at 2026-11-11T13:06:52Z, verified at 2026-11-11T13:06:52Z
[VERIFIED]                                                    (exit 0)
$ req diff --base HEAD
counts: added 0, removed 0, weakened 0, tightened 0, reshaped 0, not-judged 0, reworded 0, metadata 0, unchanged 40   (exit 0)
```

So the signature check ran (minisign is present and the trust anchor `baselines/trust.yml` resolves); nothing below is
UNVERIFIABLE for want of a verifier.

### A. `rationale.source` edited

`claims/REQ-0001.yml` line 12: `docs/adr/0001-the-unit-is-a-claim.md` → `docs/adr/0002-git-is-transport-not-identity.md`.

```
address after:   sha256:f559f5a905e4b8d595d064ec960aa695450c84176f9e1577cf2a060904e89e28      (unchanged)
$ req baseline --verify baselines/air-gapped.json
[FAIL]
  - REQ-0001: the relations, rationale or authored test links no longer recompute — edited after signing (ADR 0013, ADR 0019)   (exit 1)
$ req diff --base HEAD
metadata REQ-0001
counts: ... metadata 1, unchanged 39                                                                                            (exit 0)
$ git show HEAD:claims/REQ-0001.yml > <scratch>/REQ-0001.base.yml     # compare takes two file paths; `HEAD:claims/REQ-0001.yml` exits 3
$ req compare <scratch>/REQ-0001.base.yml claims/REQ-0001.yml
REQ-0001 -> REQ-0001: identical
  f559f5a905e4 -> f559f5a905e4                                                                                                  (exit 0)
```

### B1. a `tests:` entry renamed

`req::evidence::a_claim_is_the_unit_of_authorship_review_and_merge` → `…_merge_v2`.

```
address after:   sha256:f559f5a9...e89e28                                    (unchanged)
$ req baseline --verify ...   [FAIL]  - REQ-0001: the relations, rationale or authored test links no longer recompute — ...   (exit 1)
$ req diff --base HEAD        metadata REQ-0001 tests-lost tests-added ; counts: ... metadata 1, unchanged 39                    (exit 0)
$ req diff --base HEAD --fail-on tests-lost    (same edit)   ... fail-on: tests-lost — 1 triggered                                 (exit 1)
$ req compare <scratch>/REQ-0001.base.yml claims/REQ-0001.yml   REQ-0001 -> REQ-0001: identical                              (exit 0)
```

### B2. the `tests:` entry removed (`tests: []`)

```
address after:   sha256:f559f5a9...e89e28                                    (unchanged)
$ req baseline --verify ...   [FAIL]  - REQ-0001: the relations, rationale or authored test links no longer recompute — ...   (exit 1)
$ req diff --base HEAD        metadata REQ-0001 tests-lost                                                                       (exit 0)
$ req compare <scratch>/REQ-0001.base.yml claims/REQ-0001.yml   REQ-0001 -> REQ-0001: identical                              (exit 0)
```

### C. extra: `area` edited (`authoring` → `identity`): the intended result, kept as a control

`area` is **not** an analogue of a typed field. ADR 0051 decision 4 puts it "outside identity and outside the Baseline" on
purpose ("where a requirement is filed is not what it means"), its Consequences accept that "a reviewer cannot rely on a Baseline
to show where a claim was filed when it was signed", and its falsification 1 says the decision is wrong if an `area` edit "moves its
address or breaks a Baseline's verification". The same ADR says the classifications that cut across sections (it names safety level)
"are not areas" and are "not decided here", so typed fields are an open question that `area` does not answer. The probe is kept
because it shows the mechanics a new key goes through, not a defect:

```
address after:   sha256:f559f5a9...e89e28                                    (unchanged)
$ req baseline --verify baselines/air-gapped.json
[VERIFIED]                                                                                                                       (exit 0)
$ req diff --base HEAD
counts: added 0, removed 0, weakened 0, tightened 0, reshaped 0, not-judged 0, reworded 0, metadata 0, unchanged 40              (exit 0)
```

`[VERIFIED]` is what ADR 0051 asks for. `diff` printing nothing is consistent with its contract: `req diff --help` and ADR 0026 §2
define `metadata` as "only relations, rationale, tests, evidence or pins changed", and `area` is not on that list. Neither result
is a gap.

### C2. extra: `pins:` deleted from REQ-0040: a key that is authored, outside identity and **not wired** into the Baseline

`pins:` (`value: "2"`) removed from `claims/REQ-0040.yml`, which `baselines/air-gapped.json` covers:

```
address before and after:   sha256:faccefaea6706f65ceacbc428a3b066a3785e15646ed8f5d977ac037494ecab2     (unchanged)
$ req baseline --verify baselines/air-gapped.json
[VERIFIED]                                                                                                                       (exit 0)
$ req diff --base HEAD
metadata REQ-0040 pins-changed ; counts: ... metadata 1, unchanged 39                                                            (exit 0)
$ req diff --base HEAD --fail-on pins-changed                                                                                     (exit 1)
$ req gate --format json <variant 2> | jq -c '.policies[]|select(.policy=="pins_hold")'
{"policy":"pins_hold","outcome":"pass","details":[],"checked":6}                  (it was 7 before the edit: one pin fewer, judged as passing)
```

This is ADR 0022's "**Named loss: the pin set is not attested**": removing `pins:` and re-emitting leaves every attested value
byte-identical and `baseline --verify` green, and "a `pins_hash` beside `tests_hash` (envelope v5) would close it". It is the evidence
for the point of §4: whether an authored key is attested depends on whether someone wired it, and for `pins` that was a recorded
decision with a recorded remedy. Here `diff` does see the edit (`pins-changed`, failable) because `metadata_differs` compares pins.

### D. extra: what the loader does with a `fields:` key today

`evidence: machine` followed by `fields: {asil: C}` in `claims/REQ-0001.yml`:

```
$ req canon              [UNVERIFIABLE] <copy>/claims/REQ-0001.yml: does not parse as a claim: unknown field `fields`, expected one of `schema`,
                         `claim_id`, `statement`, `applicability`, `relations`, `tests`, `evidence`, `rationale`, `pins`, `area` at line 11 column 1   (exit 3)
$ req baseline --verify  (same message)                                                                                                              (exit 3)
$ req gate deployment_mode=air_gapped  (same message)                                                                                                (exit 3)
```

### E. extra: projections

`.req/projections` pins `docs/spec.md` and `docs/trace.md`. For A, B1 and C (the `area` edit):

```
$ req render --check docs/spec.md   [CURRENT] docs/spec.md (35 applying claim(s), sha256:0ca408b5...05e77)   (exit 0)
$ req trace --check docs/trace.md   [CURRENT] docs/trace.md (39 applying claim(s), sha256:b75f1a73...2eba1)  (exit 0)
```

Both are byte-identical to the control. The text projections print the address, modality, statement, selector and relations
only, so none of the three edits reaches them. (A punctuation edit to the statement does make both `--check` calls exit 1, because the
statement text is in the bytes: that probe says only that the statement is printed, not that its address moved.)

### F. extra: an envelope newer than the build

`baselines/air-gapped.json` with `"version":4` rewritten to `"version":5` (signature file copied; the signature is not judged for a
newer envelope):

```
$ req baseline --verify baselines/fake-v5.json
[UNVERIFIABLE]
  - manifest version 5 is newer than the version this build writes (4) — verify with a build that writes it                       (exit 3)
```

### Where the expectation was wrong, and where it held

The spec's expectation: *the address is unchanged; verification fails on that hash; `diff` reports a non-meaning class.*

| claim | result |
| --- | --- |
| address unchanged | **held** in A, B1, B2, C (`f559f5a9…e89e28` before and after) and C2 (REQ-0040, `faccefae…ecab2`) |
| verification fails on the changed hash | **held in outcome, wrong in detail.** It FAILs (exit 1), but the message names no hash: one sentence covers `relations_hash`, `rationale_hash` and `tests_hash` (`Drift::reasons`, `src/baseline.rs:401`), and `Drift` has one `attested_changed` list for all three. A reader cannot tell a changed source from an unlinked test from the message |
| the signature fails | **not what happens, and I had half-expected it.** The manifest file is untouched, so its signature still verifies; `verify` fails on **drift** between the working tree and the signed bytes (`baseline.rs:487`). Nothing in the output says the signature is intact |
| `diff` reports a non-meaning class | **held, with two gaps.** The class is `metadata`; a `tests:` change adds `tests-lost`/`tests-added`, and `req diff --base HEAD --fail-on tests-lost` exits 1 on the rename (B1) and on the removal (B2). But (a) plain `diff` exits **0** for every probe, and **a rationale-only edit (A) has no class to fail on**: `--fail-on` accepts `weakened, removed, reshaped, not-judged, pins-changed, pin-flagged, tests-lost, contradiction-added`, each of which exits 0 on A, and `--fail-on metadata` is a usage error (exit 2); (b) an edit to `rationale.source` carries no flag at all |
| `compare` for the pair | **expectation unmet.** `req compare` says `identical`, the same word it uses for a cosmetic edit; it reads canonical forms and cannot see metadata. A reviewer who runs `compare` alone, as the spec's "direction" channel, sees no change at all |
| (not expected) `area` | **the intended result, not a gap** (probe C). ADR 0051 decision 4 puts `area` outside the Baseline on purpose and accepts the consequence; `[VERIFIED]` and `metadata 0` are what it and `req diff --help` / ADR 0026 §2 say. `area` is not evidence about typed fields |
| (not expected) `pins` | **a recorded loss** (probe C2). Authored, outside identity, not attested: `[VERIFIED]` after deleting a pin. ADR 0022 names it and names the remedy (`pins_hash`, envelope v5) |

## 2. The code map

Line numbers are for `30f85a5`.

### 2.1 Every input to the address

`derive` (`src/canon.rs:1784`) is the one place an address is computed; every reader goes through it.

1. `claim.statement`, parsed by `canonicalize_statement(text, registry)` (`canon.rs:1375`): the statement text and the subject
   registry `aliases/subjects.yml`. The rules themselves (`RULES_V = 9`, `canon.rs:49`) are the algorithm, not a hashed byte; they
   reach the address through what the parse returns.
2. `claim.applicability`, injected by `form.applicability = claim.applicability.iter().cloned().collect()` (`canon.rs:1789`), a
   `BTreeSet`, so sorted. This is the only claim *key* other than `statement` that enters (ADR 0003: the statement carries no selector
   syntax, so the authored predicates are an input).
3. The serialisation `canonical_json(&CanonicalForm)` (`canon.rs:366`; struct at `canon.rs:270`) with fixed alphabetical keys:
   `action, applicability, conditions, exception, modality, operator, qualifier, raw, subject, temporal, unstructured, unit, value`
   (an `IN` set rides in the `value` slot as a sorted array).
4. The domain tag `ADDRESS_TAG = b"req.claim.v1\0"` (`canon.rs:191`) and `address_hex` (`canon.rs:427`): `sha256(tag ‖ canonical bytes)`.

Not inputs: `claim_id`, `schema`, `relations`, `tests`, `evidence`, `rationale`, `pins` (judged against the form after it is hashed,
`evaluate_pins`, `canon.rs:1870`), `area`. Probes A, B1, B2 and C confirm four of those by measurement.

### 2.2 The strict-key list

- `ClaimFile` (`src/model.rs:314`, `#[serde(deny_unknown_fields)]` at `model.rs:313`): `schema, claim_id, statement, applicability,
  relations, tests, evidence, rationale, pins, area`. The error in probe D prints exactly this list.
- Nested strict structs: `Rationale` (`model.rs:23`: `source, date, rejected_alternatives, assumptions`, all required), `Relation`
  (`model.rs:35`: `type, to`), `Pins` (`model.rs:221`: `modality, subject, operator, value, unit`).
- `parse_claim` (`canon.rs:1922`): schema judgement first, then a key-presence pre-check (`canon.rs:1937`) that names any of the 13
  removed `req.claim/1` keys (`REMOVED_V1_KEYS`, `model.rs:193`), then the strict deserialisation (`canon.rs:1959`), then
  `validate_pins` (`:1962`), `validate_tests` (`:1967`), and the area gating (`:1978`, `AREA_SINCE_FORMAT`, `model.rs:352`).
- Precedent for a new key: ADR 0051 added `area` **with a new schema numeral** (`CLAIM_SCHEMA_V = 3`, `canon.rs:74`), so an older build
  reading a `/3` file says "newer format, NOT JUDGED" instead of a parse error; the contract is pinned by `tests/area.rs:252`
  (`v1_stays_a_migration_candidate_and_v4_a_newer_format`). A `fields:` key would follow it with `req.claim/4`; the
  alternative is probe D's parse error. Both are exit 3 today.

### 2.3 Where a new attested hash goes, and what a v5 envelope does

Today's envelope is v4 (`BASELINE_VERSION`, `src/baseline.rs:47`), with `OLDEST_VERIFIABLE_BASELINE_VERSION = 3` (`:62`).

A new attested key touches **nine sites**, one list (the verdict in §4 points here):

1. **Digest function and tag:** a `fields_digest` beside `rationale_digest` (`model.rs:74`) and `tests_digest` (`model.rs:123`), with
   its own domain tag next to `RELATIONS_TAG`/`RATIONALE_TAG`/`TESTS_TAG` (`model.rs:49-56`, "must never trade hashes").
2. **Row:** a `fields_hash` next to `tests_hash` (`src/view.rs:77`), computed in `load_rows` beside `view.rs:1237-1241`.
3. **Manifest record:** `ManifestClaim` (`baseline.rs:71`) gains `fields_hash: Option<String>` (an `Option` so a v4 file still
   parses, as `tests_hash` is for v3).
4. **Manifest builder:** `manifest()` (`baseline.rs:118`) fills it.
5. **Signed bytes:** `canonical_bytes` (`:184`) appends `"fields_hash"` only `if manifest.version >= 5`, exactly as `tests_hash` is
   gated on `>= 4` (`:199`). The comment at that site states the reason: adding a key with a null value to old claims "would silently
   stop every old signature from verifying".
6. **Parse:** `from_json` (`:282`) must reject a v5 header whose claims lack `fields_hash`, as it does for v4 and `tests_hash`.
7. **Drift:** `drift` (`:487`): the comparison tuple grows a field, and the version guard for it must be `5..=BASELINE_VERSION`, not
   `>= 5` (see the comment at `:515-525` and the guard at `:526`: a future envelope's hash is not this build's to compare).
8. **Disclosure:** the "predates envelope v4: the authored test links are NOT attested" line (`baseline.rs:2045`) is keyed on
   `OLDEST_VERIFIABLE..TESTS_ATTESTED_FROM`; a v4 manifest would need the same disclosure for fields.
9. **`req diff`:** `metadata_differs` (`src/diff.rs:704`) must compare the new digest, or an edit to the field is invisible to
   `diff` (the audit packet needs no site of its own: its coverage comes from `drift`).

Not sites, but they move with the constant: `judges_signing_time` (`:270`) is the closed window `3..=BASELINE_VERSION`.

**What a v5 envelope changes for existing signatures.**

- An existing v4 signature still verifies **only if** `canonical_bytes` keeps the v4 shape for `version == 4`. If it does, three
  manifests (`baselines/air-gapped.json`, `release-parity.json`, `release-parity-and-adversarial.json`) stay `VERIFIED`.
- They do not attest fields. Until they are re-emitted with `req baseline --at <ts>` and **re-signed by the owner** (`minisign -Sm`;
  req never signs), a field edit after signing is invisible to `baseline --verify`, which is probe C2's behaviour for `pins`.
- A build that does not know v5 reports a v5 manifest `[UNVERIFIABLE] manifest version 5 is newer than the version this build writes (4)`,
  exit 3 (probe F), and does not judge the signature (`baseline.rs:1550`). So the build that reads v5 must ship before the first v5
  manifest is committed, because consumers pinned to an older build would turn red.
- `req audit` takes its coverage from `drift` (site 7), so it follows the same change.

## 3. Where a `fields:` value could leak, and the test that would catch it

"Existing" names a test that already pins the same property for an analogue; "new" is a test that would have to be written.

| # | place | how a field would leak | catching test |
| --- | --- | --- | --- |
| L1 | `derive`, `canon.rs:1789` | Copied into the form the way `applicability` is: every claim carrying it changes address | new, modelled on `tests/area.rs:450` `the_area_is_not_an_input_to_the_address` (same claim without, with, and with another value: equal addresses) |
| L2 | `CanonicalForm` / `canonical_json`, `canon.rs:270`, `:366` | A field added to the struct puts a new key in **every** claim's canonical bytes, so every address moves at once, a `RULES_V`-class event with no bump | existing: `tests/canonical_form.rs:35` (the documented key set), the signed manifests (`req baseline --verify` fails), `req selftest` and `req bench` goldens |
| L3 | `select::parse_predicate`/`applies`, `src/select.rs:69,123` | A selector that reads a field (`asil >= B`) makes variant membership depend on a datum outside the address: an edit changes whether a claim applies and moves no address (the Baseline would show it as `missing`/`added`) | new, modelled on `tests/area.rs:544` `an_area_changes_no_applicability_verdict_or_derived_parse` |
| L4 | `identity_digest`, `src/view.rs:544` | It hashes sorted addresses only, so it is safe while the field stays out of the address. Printing the field in a text projection changes the bytes `--check` compares, not the `identity=` header | existing: `tests/projection.rs:199` pins the definition; new: modelled on `tests/area.rs:646` `text_projections_do_not_change_with_an_area` (probe E is the same check by hand) |
| L5 | `Row`'s `Serialize`, `view.rs:312` | Adding `fields` to the render/trace JSON rows is additive, but if it replaced a key or reordered, consumers of `req.render/1`/`req.trace/1` break | existing for the analogue: `tests/area.rs:605` `the_views_carry_area_and_null_when_absent` |
| L6 | `manifest()`, `baseline.rs:118` / `drift`, `:487` | A field left out of every hash is silently unattested, as `pins` is (probe C2; a recorded loss in ADR 0022) | new, modelled on `tests/baseline.rs:496` `a_v4_manifest_attests_the_tests_hash_and_a_change_is_a_failure` and `:765` `an_edited_rationale_after_signing_fails_verification` |
| L7 | the new digest writer | Float math or a `HashMap` iteration makes the bytes nondeterministic; a number as `f64` also breaks the canonical-decimal rule | existing: the six clippy lints (`float_arithmetic`, `as_conversions`, …); new: an independent-oracle test like `tests/address_recompute.rs:152` `the_evidence_digest_matches_an_independent_oracle` |
| L8 | domain tag | Re-using `RATIONALE_TAG` or `TESTS_TAG` lets two surfaces trade hashes | new: the same bytes under each of the four tags give four digests |
| L9 | `canonical_bytes`, `baseline.rs:184` | Appending `fields_hash` to every version breaks every old signature | existing for v3: `tests/baseline.rs:409` `a_v3_manifest_keeps_its_historical_bytes_and_still_verifies`; new: the same for a v4 manifest under a v5 build |
| L10 | `metadata_differs`, `src/diff.rs:704` | A new field not compared there is invisible to `req diff`. For `area` that is the contract (`metadata` is relations, rationale, tests, evidence, pins); for an attested field it would be a defect | new, modelled on `tests/diff.rs:619` `a_removed_test_link_alone_is_metadata_with_tests_lost` |
| L11 | `Flag::SameAddressAs`, `diff.rs:135` | Two claims that differ only in a field share an address: a copy with another ASIL is reported as a renumber candidate, not as a different requirement | new: an added claim differing from a removed one only by the field must carry `same-address-as` and a field change class |
| L12 | `canon.rs:2196` duplicate `claim_id` check | Compares file text, not parsed content, so it is unaffected by a field, but a field-only difference between two files of one id is a duplicate-id error | existing: `tests/loader.rs:192` `duplicate_claim_ids_error_and_the_content_is_compared` |

**Places the field would be invisible by design, not leaking** (probe A/B show they are invisible for metadata today):

- `req compare` classifies direction from canonical forms (`src/compare.rs`), so an ASIL D → B edit is `identical`. For a safety
  field that is a **weakening of the obligation that no direction command would name**; `diff` would call it `metadata`, exit 0.
- `req satisfy` stores the statement verbatim and judges by direction (`src/satisfy.rs`), so a raised ASIL leaves a satisfaction link
  `holds`.
- A Decision's canonical bytes cover `address` and not any field (`src/decision.rs:72`), so a ratification stays valid across a field edit.
- `req bench` and the corpora read statements, not claim files: no field reaches them. `FMR = 0` is untouched.

## 4. Verdict

**Is the separation already proven by the analogue? Partly, and the proven part is the narrow one.**

Proven, on one claim and one signed Baseline, by measurement: authored data that sits outside the canonical form moves no address
(A, B1, B2: `f559f5a9…e89e28` before and after), the Baseline's per-claim hashes catch an edit after signing (A, B1, B2 all FAIL at exit
1), and `diff` names the edit `metadata` (A, B1, B2). The architecture supports "outside identity, inside the attestation" for strings
and lists.

Not proven, or shown against:

1. **Nothing typed was measured.** The analogue is a string and a list of strings. No enumeration, no order, no validation against a
   registry and no "absent" semantics exist to test.
2. **The separation is wiring, not a property.** `pins` is authored and outside identity like the analogue, and is **not** attested
   (probe C2; ADR 0022's named loss, with `pins_hash` in v5 as its named remedy). Making a key protected means touching the nine sites of
   §2.3 (L6, L10). A key added without wiring is unprotected and no test fails; for `pins` and `area` that is a recorded decision, and
   a typed-fields ADR has to record the same decision for its key, deliberately, and not by omission.
3. **The channel that tells a human is weak.** For the three attested edits, the FAIL message names no hash, `compare` says
   `identical`, `diff` exits 0, and `--fail-on` has no class for a rationale-only change (a `tests:` change does: `tests-lost`).
4. **A negative was measured, not the absence of readers.** The probes show these three keys do not move the address. They do not
   show that no code path reads a field and lets it into identity or an identity digest. §3 lists the paths by reading, not by running
   them.

**What the typed-fields ADR would have to add** (each item follows from an observation above; none is a recommendation to build):

- A schema numeral (`req.claim/4`) with the ADR 0051 precedent for how older builds answer, and a declared type for each field
  (at least an ordered enum for something like ASIL), with a value outside the type a load error and a missing value UNKNOWN
  (see `checks-expressibility.md` R1).
- A `fields_hash` in envelope v5 with its own domain tag, the compatibility rule of §2.3 (old bytes unchanged), and a disclosure line
  for v4 manifests.
- A statement of what each of `compare`, `diff --fail-on`, `satisfy` and Decisions does on a field change. The evidence says
  today's answer would be "nothing" for all four.
- A coverage test over `ClaimFile`'s keys: each key is in identity, in an attested hash, or on a named allowlist of unattested ones
  (today `area`, by ADR 0051 decision 4, and `pins`, by ADR 0022's named loss). It would make an omission for a new key a failing
  test rather than a silence.
- The tests of §3 L1, L3, L6, L9, L10 and L11, because none exists for a field.
- The re-signing step, owned by the human: three manifests and a release ordering (reader before writer).

## 5. What this does not show

- It does not show that a `fields:` block can be built without moving an address: nothing was built. L1-L12 are risks found by
  reading; a run would be the evidence.
- One claim (REQ-0001), one Baseline (`air-gapped`, 36 of 40 claims), one machine (macOS, `minisign 0.12`), one run. Probes A, B, C and C2
  were not repeated on the two release manifests.
- The analogue values are free strings and string lists. They say nothing about numbers, enumerations or order, and the
  canonical-decimal rule (`claim-format.md`, "values are canonical exact decimal strings") was not exercised.
- Probe F rewrites the version number and leaves the signature untouched, so it shows only what this build says about a newer
  envelope. No v5 manifest was emitted or signed.
- No full `req check` or `req gate` was run on the edited copies; only the commands in §1.
- Whether an owner should accept a v5 envelope is a decision, and this record has no bearing on it. No external material is used.
