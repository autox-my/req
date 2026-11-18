# F-2: Baseline envelope v5, `fields_hash` and `pins_hash` (ADR 0056, unit 2 of 3)

Task id: `F-2`. Base: the commit that adds this spec, on top of `origin/main` at `8391292`, where F-1 (PR #106) is merged:
`req.claim/4`, `.req/kinds/`, and `LoadedClaim::fields` (`src/canon.rs:2071`) holding each claim's judged fields. The
unit table is in `docs/specs/F-1-claim-schema-and-kinds.md`.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0056-a-claim-may-carry-typed-fields-outside-its-address.md`. This unit covers:
- decision 5.2, the coverage test, and the L7, L8 and L9 tests of decision 5.1;
- decision 6, `fields_hash` and envelope v5;
- decision 7, `pins_hash`. The owner merged the ADR without striking it, so it is **kept**;
- decision 8's paragraph "The verify failure names the surface";
- *Falsification* items 2, 3, 5 and 8.

Where this spec and the ADR disagree, the ADR wins: stop and report. The ADR's line numbers are for `960e7ed`; use the
current ones given here.

**Not in this unit:** `metadata_differs` (`src/diff.rs:712`), every `req diff` and `req compare` report, and the JSON
views. Those are F-3. No row hash is serialised into a projection or a JSON view (the rule above `src/view.rs:294`).

## Choices fixed here

1. **Two digests** in `src/model.rs`, beside `RELATIONS_TAG`, `RATIONALE_TAG` and `TESTS_TAG` (`:49-56`).
   - **`FIELDS_TAG = b"req.fields.v1\0"`**. `fields_digest(kind, fields)` is computed over the compact JSON that
     decision 6 specifies, byte for byte:
     - field names sorted by byte, and object keys in the order the ADR shows;
     - `integer` and `decimal` as their canonical strings, `bool` as a JSON boolean;
     - a list as its element texts sorted by byte;
     - `{"kind":null,"fields":{}}` for a claim with neither;
     - `"value":[]` for an empty list, and an absent field not in the object at all.

     Build it from `LoadedClaim::fields` (`kinds::JudgedField`), never from the raw YAML. No number is parsed.
   - **`PINS_TAG = b"req.pins.v1\0"`**. `pins_digest(pins)` is computed over the compact JSON array of `[key, value]`
     pairs in `Pins::entries` order (`src/model.rs:242`), only the keys present. `pins: {}` and no `pins:` give the same
     empty array.
   - The JSON is written by a deterministic path, with no `HashMap` iteration and no float.
2. **The row and the manifest** (`src/baseline.rs`).
   - `BASELINE_VERSION` becomes 5, with new constants `FIELDS_ATTESTED_FROM = 5` and `PINS_ATTESTED_FROM = 5` beside
     `TESTS_ATTESTED_FROM` (`:51`). `OLDEST_VERIFIABLE_BASELINE_VERSION` stays 3.
   - `Row` (`src/view.rs`, beside `tests_hash` at `:77`, computed beside `:1237-1241`) gains `fields_hash` and
     `pins_hash`, and `ManifestClaim` (`:71`) gains both.
   - `manifest()` (`:118`) writes them.
   - `canonical_bytes` (`:184`) appends `fields_hash`, then `pins_hash`, after `tests_hash`, **only for
     `version >= FIELDS_ATTESTED_FROM`** (and the pins constant), exactly as `tests_hash` is gated (`:199`).
   - A v3 or v4 manifest keeps its bytes.
3. **Reading a manifest** (`from_json`, `:290`).
   - Requiring each new hash follows the pattern of `requires_tests_hash` (`:278`, PR #104): `requires_fields_hash` and
     `requires_pins_hash`, keyed on their own constants and never on `BASELINE_VERSION`. Unit-test each at its constant
     minus 1, the constant, `BASELINE_VERSION` and `BASELINE_VERSION + 1`.
   - A v5 manifest missing either hash is refused.
   - A v4 manifest that carries either hash is treated exactly as a v3 manifest carrying `tests_hash` is today:
     `canonical_bytes` leaves it out of what is verified. Do not invent a new refusal. Add a test that pins this
     behaviour, so a later change to it is deliberate.
4. **Drift** (`drift`, `:495`).
   - Each new hash is compared only inside its closed window, `FIELDS_ATTESTED_FROM..=BASELINE_VERSION` (and the pins
     one), never `>=` (`:530-534`).
   - `Drift` records **which** surfaces changed for each claim, not one `attested_changed` list (`:394`).
   - `Drift::reasons` (`:409`) names them in a fixed order (relations, rationale, tests, fields, pins), joined as plain
     prose, as decision 8 shows: `REQ-0101: the rationale and the fields no longer recompute — edited after signing
     (ADR 0013, ADR 0019, ADR 0056)`.
   - It names the surface, never a hash value. Cite ADR 0056 only when a fields or pins surface is among them.
   - When drift is the reason for a failure, the output also says once that the manifest's signature itself verified,
     since the working tree moved and not the manifest (decision 8).
   - `req audit` takes its coverage from `drift` and follows. Check that its wording still reads true.
5. **The disclosure** (`:2053`). Key each line on its own constant, so no manifest is told it predates itself:
   - a manifest below `TESTS_ATTESTED_FROM`: the test links are not attested;
   - a manifest below `FIELDS_ATTESTED_FROM`: `predates envelope v5: the authored fields and pins are NOT attested by
     this manifest`, using the constant, not a literal 5.

   Today's v3 line prints `v{BASELINE_VERSION}`, which becomes wrong at 5: fix it to name the version that introduced
   test links. A v3 manifest gets both lines; a v4 manifest gets the fields-and-pins line only; a v5 manifest gets none.
6. **The coverage test** (decision 5.2). It goes in a new `tests/claim_file_coverage.rs`.
   - It serialises a fully populated `ClaimFile`, with every optional key present.
   - It requires each top-level key to be in exactly one of three lists:
     - **identity**: `statement`, `applicability`;
     - **attested**: `relations`, `rationale`, `tests`, `evidence`, `kind`, `fields`, `pins`;
     - **named unattested**: `area` (ADR 0051 decision 4), `schema` (ADR 0022), `claim_id`.
   - Each list entry carries a one-line reason.
   - It must not pass vacuously. Assert the serialised key count equals the three lists' total, and add a mutation: add
     a field to `ClaimFile` and the test fails.
   - For each **attested** key, a behavioural leg: changing that key after emitting the manifest makes `drift` report a
     change. A classification that is not wired fails there.
7. **The three committed Baselines stay v4** (`baselines/*.json`, `"version":4`).
   - They stay `[VERIFIED]` under this build, with the fields-and-pins disclosure line. Do **not** re-emit or re-sign
     them: the owner does that after the release (decision 6, the ordering paragraph).
   - `req baseline` now emits v5 by default.
   - Existing tests that pin an emitted `"version":4` move to 5. Tests that verify a committed or fixture v4 manifest
     keep verifying it.

## Tests first (envelope `red`)

Use public seams only: the CLI over a scratch corpus with a minisign stub or the existing test key, and the public
functions of `req::model` and `req::baseline`. Oracles are independent:

- **L7 (determinism):** for one fixed kind and set of fields, the expected `fields_digest` hex is computed outside the
  code under test. Do it once with `printf '%s' 'req.fields.v1' | …` plus the NUL and the literal JSON through `shasum -a
  256`, or with Python `hashlib`, and paste the hex and the command into the test's comment. Do the same for one
  `pins_digest`. Both must also hold with the list given in reverse order and the fields given in another order.
- **L8 (domain separation):** the same bytes under the five tags give five distinct digests.
- **L9:** a v4 manifest (a fixture, or a committed one) has the same `canonical_bytes` under this build as at the base.
  Its signature still verifies, and the disclosure line appears.
- *Falsification* 2:
  - against a v5 manifest emitted and signed in the test, a field edit, a `kind` change and a pin deletion (probe C2)
    each fail `req baseline --verify` at exit 1;
  - the output names the surface ("the fields", "the pins") and says the signature verified.
- *Falsification* 3:
  - a v4 manifest signed before this change still verifies;
  - a v4 manifest stripped of `tests_hash` is refused;
  - a v5 manifest stripped of `fields_hash` or `pins_hash` is refused.
- *Falsification* 5: an absent field and an empty list hash differently; a reordered list hashes the same; two surfaces
  never trade a digest.
- *Falsification* 8: the coverage test and its mutation.
- A claim with no kind and no fields carries `fields_hash` = the digest of `{"kind":null,"fields":{}}`; adding a kind
  later fails verify.
- `pins: {}` and no `pins:` hash alike.

Prove each red at the base.

## Mutation checks (envelope `mutations`)

- Drop `fields_hash` from `canonical_bytes`: the field-edit verify test fails.
- Drop `pins_hash` from `canonical_bytes`: the pin-deletion verify test fails.
- Gate the new hashes on `== BASELINE_VERSION` in `from_json`: the `requires_*` unit tests at `BASELINE_VERSION + 1`
  fail.
- Write v5 bytes for a v4 manifest: L9 fails.
- Sort fields by insertion instead of by byte: L7 fails.
- Reuse `TESTS_TAG` for fields: L8 fails.

## Verify

- Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
- The three committed Baselines are unchanged and `[VERIFIED]`, each printing the fields-and-pins disclosure line.
- `req --version` reports `baseline_envelope 5`, `claim_schema req.claim/4` and `rules_v 9`. `RULES_V` does not move.
- `docs/spec.md` and `docs/trace.md` pass `--check` unchanged.

## Docs, one home each

- `req baseline --help`:
  - what v5 attests (fields and pins added);
  - what a v4 manifest still verifies, and the disclosure line;
  - that the declarations in `.req/kinds/` are not attested (decision 6's named loss).
- `docs/claim-format.md`: the field reference's `kind`/`fields` and `pins` rows state that they are attested by envelope
  v5 (F-1 phrased this "as of this unit": replace that sentence). Also update the *Version axes* row for the envelope.
- `docs/releasing.md`: one step after a release that moves the envelope. The owner re-emits (`req baseline --at <ts>`)
  and re-signs (`minisign -Sm`) the three manifests, in a PR of their own, after the build that reads v5 is released.
- `CONTEXT.md`: change the Baseline entry only if it lists what is attested.

## Allowed paths

- `src/model.rs` (the two digests and tags), `src/baseline.rs`, `src/view.rs` (the row's two hashes only), `src/audit.rs`
  (wording that follows `drift`, if needed), `src/main.rs` (help pages).
- `tests/**`.
- `docs/claim-format.md`, `docs/releasing.md`, `CONTEXT.md`.

Forbidden:
- `RULES_V` and any change to the canonical form or `derive`;
- `src/canon.rs`, `src/kinds.rs`, `src/diff.rs`, `src/compare.rs`, `src/pin.rs`;
- `baselines/**` (no re-emit, no re-sign), `claims/**`, `bench/**`, `decisions/**`, `aliases/**`, `.req/**`;
- `docs/spec.md`, `docs/trace.md`, `docs/adr/**`, `scripts/**`;
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/**`.

If a forbidden file must change, stop and report.
