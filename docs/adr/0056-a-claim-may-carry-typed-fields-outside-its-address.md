# 0056. A claim may carry typed fields that never move its address

Extends ADR 0022 (`req.claim/2`: claims are authored, fingerprints are derived) and ADR 0051 (`area`, the first optional
authored key outside identity); amends ADR 0013 and ADR 0019 (what the Baseline attests), ADR 0026 (what `req diff`
reports) and ADR 0039 (`req check`'s completeness step gains two findings, and `.req/completeness` gains the word `strict
fields`: its word list is closed, `src/completeness.rs:135-147`, so this is a change to 0039 decisions 1-3). It proposes two optional authored keys, `kind` and `fields`; the claim file format moves to `req.claim/4`
(decision 1) and the Baseline envelope to v5 (decision 6). No `RULES_V` bump and no address moves: the keys are outside
identity. Decision 7 also closes the loss ADR 0022 named, the unattested pin set. Line numbers below are for
`960e7ed` (`req 0.5.0`, `rules_v 9`, `baseline_envelope 4`, `claim_schema req.claim/3`).

**Status: accepted** — ratified by the owner's merge of PR #103 (2026-11-18); implemented by PRs #106, #107 and #108. Asked for by the owner on 2026-11-17 (task D-1), the first of two
decisions informed by the X-1 experiments; the second, repository-defined checks, depends on this one and is not drafted
here. Date proposed: 2026-11-17.

## Context

An organisation's requirements carry attributes that are not the requirement: a safety integrity level, a verification
method, a review date, the claims that mitigate this one. ADR 0051 named them and left them open: the classifications that
cut across sections "are **not** areas" and are "not decided here" (*Context*, decision 4, *Not decided here*). Today a
claim has no place for one. The loader is strict, so an author who writes `fields: {asil: C}` gets
`[UNVERIFIABLE] … unknown field `fields`, expected one of …` (exit 3) from `canon`, `baseline --verify` and `gate` alike
(`docs/research/fields-separation.md` §1 probe D; the key list is `ClaimFile`, `src/model.rs:313-314`). Of the ten
repository rules written in `docs/research/checks-expressibility.md`, two cannot be stated until a typed field exists (R1:
a claim of integrity level C or D needs a linked test; R2: no claim carries a lower level than the one it refines; R2 also
needs the field's declared *order*, §3), and a third, R4, moves to typed fields if components are not claims (§3, R4).

The evidence on whether such a key can stay out of the address while the Baseline still catches an edit to it is
**partial**, and this ADR does not call it proven. `docs/research/fields-separation.md` measured an analogue, on one
claim, one signed Baseline and one machine (§5):
- an edit to `rationale.source` or a `tests:` link moves no address (`f559f5a9…e89e28` before and after) and fails
  `req baseline --verify` at exit 1 (probes A, B1, B2);
- a key outside identity and **not wired** into the Baseline is not caught: deleting a `pins:` entry left the address
  and `[VERIFIED]` unchanged (probe C2), which is ADR 0022's *Named loss: the pin set is not attested*;
- the separation is **wiring, not a property** (§4, point 2): protecting a new attested key touches nine sites (§2.3)
  and a key added without that wiring is silently unprotected, with no test failing;
- the channel that tells a human is weak: the verify failure names no hash, `compare` says `identical`, `diff` exits 0
  and has no failable class for a rationale-only edit (§1, the table);
- nothing typed was measured: the analogue is a string and a list of strings, with no enumeration, no order and no
  validation against a declaration (§4, point 1); and the absence of a *reader* that lets a field into identity was
  established by reading twelve places (§3, L1-L12), not by running them (§4, point 4).

The design below is therefore a proposal whose separation claim is falsifiable, and decision 5 turns the twelve risks into
named tests and a coverage test so that the claim is checked, not asserted.

## Decision

1. **Two optional authored keys, `kind` and `fields`, in a new format, `req.claim/4`.**
   ```yaml
   schema: req.claim/4
   claim_id: REQ-0101
   kind: safety
   fields:
     asil: C
     mitigated_by: [REQ-0102, REQ-0107]
     review_due: 2027-03-01
   ```
   - `kind` is one name, the claim's class; `fields` is a map of field name to value. Both are authored, optional and
     absent by default (ADR 0006: no friction in authoring). `fields` without `kind` is a load error: a value has no type
     until a kind declares it.
   - `req.claim/4` is `req.claim/3` plus these two optional keys and nothing else. **`/3` and `/2` keep loading as they
     are**: `READS_AS_IS` (`src/canon.rs:79`) becomes `[2, 3]`, no message, no rewrite, no address moved, and an adopter
     who files no kind never edits a claim. A `/4` build reading `/3` is the rule ADR 0051 decision 6 set for `/2`.
   - A `/3` or `/2` file carrying `kind` or `fields` is a parse error naming the key and `req.claim/4`, judged on the
     **key** and not its value, exactly as `area` is (`src/canon.rs:1973-1989`). A `/3` build (today's) reading a `/4` file
     reports a newer format, NOT JUDGED (`src/canon.rs:174-178`, pinned for `/4` by `tests/area.rs:252`).
   - *Why not an optional addition to `/3`.* A `/3` build would reject the file as `does not parse as a claim: unknown field
     `kind`` (probe D's message), a parse error that sends an author looking for a typo when the reader is old, and one
     `/3` file would mean two things in two builds. ADR 0051 rejected the same shape for `area`
     (`src/canon.rs:1973-1977`).
   - *What the bump costs, and why it is accepted.* `scripts/release-notes.sh:142-144` reports a claim-schema change as
     identity **moved**, which makes the release a minor bump under ADR 0055 decision 2; but decision 6 moves the envelope in
     the same release, which already does (`release-notes.sh:148-150`), so the schema adds no second event. No adopter
     migrates. Three importers refuse a build that reports a schema outside `ACCEPTED_SCHEMAS`
     (`tools/adopt.py:90,633-640`, `tools/reqif_import.py:121,1362`, `tools/needs_import.py:109`), so each gains
     `req.claim/4` in the implementing change; they keep drafting `req.claim/2`.
   - The spelling `fields:` (one map) and not one top-level key per field, because the claim file's key list is closed
     (`deny_unknown_fields`, `src/model.rs:313`) and a flat spelling would open it and collide with every future key.

2. **Kinds are declared in `.req/kinds/<kind>.yml`, one file per kind.**
   ```yaml
   schema: req.kind/1
   label: Safety-related requirement      # optional; the reader-facing name, any language
   fields:
     asil:
       type: enum
       values: [QM, A, B, C, D]
       ordered: true                       # the order of `values` is the order, ascending
       required: true
     mitigated_by:
       type: list
       of: ref
     review_due:
       type: date
   ```
   - The file's stem is the kind's name, and a kind and a field name are `[a-z][a-z0-9_]*`: ASCII, lower case, and
     usable as an identifier by a later check (a hyphen would read as a minus). The `label` is the only reader-facing
     name, as for areas (ADR 0051 decision 2).
   - **One file per kind**, as one claim is one file (ADR 0002) and a decision is one file: two authors editing two kinds
     do not conflict, a diff shows one kind, and a duplicate kind is impossible by construction. One `.req/kinds.yml`
     would be one read and one place, and every edit to any kind would touch it. The directory is new under `.req/`, whose
     other entries are line lists; a declaration is structured, so it is YAML like `aliases/subjects.yml` and a claim.
   - Strict, like every authored file here: a kind file has the keys above and no other (`default:` among them,
     decision 4), a file whose stem is not a valid name is refused, and a stray file under `.req/kinds/` is refused naming
     it. `schema: req.kind/1` lets the declaration format move on its own; a newer one is NOT JUDGED, as a newer claim
     format is.
   - A malformed kind file is exit 2 naming `<file>` and the key, every bad one in one run (the way `.req/areas` is
     refused, ADR 0051 *Implementation note A-1*); an unreadable one is exit 3.
   - **The declarations are not attested** (decision 6 says what the Baseline does cover and the named loss below).

3. **Eight field types, each a canonical spelling.** The file carries the value as text where YAML would guess.
   | `type` | the value in a claim | refused |
   | --- | --- | --- |
   | `string` | one line of text; a plain scalar that YAML would type (`true`, `3`, `1.5`, `null`) must be **quoted**, so the value is text and not a boolean or a number | empty, a newline or other control character, leading or trailing space (as a pin's `subject` and `unit` are, `src/model.rs:289-298`), an unquoted scalar the loader delivers as a boolean, a number or null |
   | `integer` | a **quoted** canonical integer, `"3"`, `"-12"`, within `i64` | `3` unquoted, `"007"`, `"+1"`, `"-0"` |
   | `decimal` | a **quoted** canonical exact decimal, `"0.3"`, by the rule `is_canonical_decimal` (`src/canon.rs:1852`) | `0.3` unquoted (a YAML float), `"0.30"`, `".3"`, any exponent |
   | `bool` | a scalar the loader delivers as a YAML boolean. `yaml_serde` 0.10 delivers `true`, `True` and `TRUE` (and the false forms) as one boolean, so those spellings are **one value**, normalised to the JSON `true` or `false` in the hash (decision 6); the raw spelling is not kept | `yes`, `on`, `"true"` (a string), and any scalar not delivered as a boolean; a test pins the loader's behaviour for each |
   | `enum` | one of the kind's `values` (distinct tokens `[A-Za-z0-9][A-Za-z0-9_.-]*`, compared exactly and case-sensitively; a token YAML would type, such as `true` or `3`, is quoted in the claim as a `string` is) | a value not in the list |
   | `ref` | the id of a claim, in the id grammar | a malformed id |
   | `date` | `YYYY-MM-DD`, read as **text** (never a timestamp), calendar-valid including leap years, no clock and no zone | any other spelling or an impossible date |
   | `list` | a **set** of one element type, `of:` one of `string`, `integer`, `decimal`, `enum` (with its own `values`), `ref`, `date`; a duplicate element is refused | a list of `bool`, a list of lists, a duplicate |
   - **Numbers are strings, as `value` is** (`docs/claim-format.md`: "`value` is a canonical exact decimal string", and
     probe C2's `value: "2"`): a float never enters a claim, which keeps `float_arithmetic` and `as_conversions`
     (`Cargo.toml`) unviolated on the digest path and keeps `"0.30"` from meaning a second thing.
   - `ordered: true` is valid on an `enum` only and states that its `values` are in ascending order; **this ADR declares the
     order and nothing reads it** (decision 9 names the readers it leaves open).
   - A list is a set because `tests:` is (ADR 0019 hashes it sorted: a meaningless reordering must not change what the
     Baseline attests). A sequence, where order carries meaning, is outside (decision 10).
   - `required: true` is the only constraint. There is **no** `default`, no range, no pattern and no cross-field rule.

4. **Absent is UNKNOWN; every other failure is a refusal, and none is folded into PASS.**
   - *Absent.* A field a claim does not carry has no value. A reader sees it **absent** (in JSON the key is missing from
     `fields`, never `null` and never a default), so a consumer must treat it as UNKNOWN: `checks-expressibility.md` R1
     measured that "absent is unknown" is the only choice that agrees with `AGENTS.md`, because "absent means QM"
     silently judges zero claims. A `default:` is therefore not offered: it would turn UNKNOWN into a value.
     `asil: null` is refused too: a null is neither absent nor a value, so the key is omitted instead.
   - *Absent and required.* A claim of a kind whose field has `required: true` and that carries no value is a **finding**,
     `missing_field`, naming the claim, the kind and the field, in `req check`'s completeness step beside `undeclared_area`
     (ADR 0039; ADR 0051 *Implementation note A-1*). Answer: **UNVERIFIABLE** (exit 3), as `in_no_baseline` is: a claim being
     drafted is normal and the value is unknown, not wrong. `strict fields` in `.req/completeness` promotes it to FAIL. It is
     a finding and not a load error because a claim written before the kind gains a required field must still load.
   - *Refusals, at load, exit 3, NOT JUDGED, naming the claim, the kind, the field and the declared alternatives:*
     a `kind` with no `.req/kinds/<kind>.yml` (and every kind when `.req/kinds/` does not exist: with no declaration there
     is no type, so no claim is "accepted untyped"; **every command that loads claims from a revision needs that revision's
     declarations, decision 8 says how `req diff` gets them**); a field the kind does not declare; a value of the wrong type or outside
     its `enum`; a `list` with a duplicate; `fields` without `kind`; a `null`. These are the same class as a misspelled
     `pins:` key (`src/canon.rs:1959-1967`): an authored error with one remedy.
   - *A `ref` that points at no claim* is not a load error (the target may be authored later) but a finding,
     `dangling_field_ref`, **UNVERIFIABLE**, the class of a dangling relation (`req trace`'s `findings.dangling`, exit 3).
     It is emitted by **`req check`'s completeness step**, not by `trace` (which reads relations only), beside `missing_field`.
     ADR 0039 decision 4 has the audit packet print the completeness findings for its selection, so it prints these two too;
     it prints no field *value* (decision 10).
     **A `ref` field is not a relation**: it adds no edge to `trace`, `impact` or the graph; a claim depends on another
     only through `relations`.
   - `parse_claim` (`src/canon.rs:1922`) judges the file's own shape, the format gating and the grammar of names; the
     corpus loader and `compare`, which both hold a root (`src/compare.rs:392`), judge values against the declarations
     through one shared function, so there is one home for the judgement, as `parse_claim` is for the rest.

5. **Fields are outside the canonical form, and four things enforce it.** `derive` (`src/canon.rs:1784`) is the one place
   an address is computed; it reads `statement` and `applicability` and nothing else, and `kind`/`fields` are not read by it.
   That is a design intent until a test runs; the implementing change carries:
   1. **The leak tests of the research record that matter** (`fields-separation.md` §3), each with a test that fails when
      the leak exists: **L1** the same claim without a kind, with one, and with another value has one address, and one
      `canonical_json`; **L2** `CanonicalForm`'s documented key set is unchanged (`tests/canonical_form.rs:35`), `req selftest`
      and `req bench` give the same answers, `RULES_V` stays 9; **L3** a claim's applicability verdict and derived parse do
      not change with a field; **L4** `docs/spec.md` and `docs/trace.md` pass `--check` unchanged, because **no text
      projection prints a field**; **L5** the JSON views gain `kind` and `fields` after `area` and replace or reorder
      nothing; **L7** the digest is deterministic over an independent oracle (decision 6); **L8** the attested-surface tags
      (`relations`, `rationale`, `tests`, `fields`, and `pins` if decision 7 is kept) give one distinct digest each over the
      same bytes; **L9** a v4 manifest keeps its bytes under a v5 build; **L10** `diff` sees a
      field edit (decision 8); **L11** a claim added with the address of a removed one, and different fields, says so
      (decision 8); **L12** two files of one id that differ only in a field are a duplicate id (`src/canon.rs:2193`).
   2. **A coverage test over `ClaimFile`'s keys**, the missing piece the research found (§4, last list). The test
      enumerates every key a fully populated `ClaimFile` serialises (`src/model.rs:312`) and requires each to be in exactly
      one of three places: **identity** (`statement`, `applicability`), **attested** (`relations`, `rationale`, `tests` with
      `evidence`, `kind` with `fields`, and `pins` if decision 7 is kept), or the **named unattested allowlist** (`area`, by
      ADR 0051 decision 4; `pins`, by ADR 0022's named loss, if decision 7 is struck; `schema`, which is out of the manifest by
      ADR 0022 — "the Baseline manifest carries no claim schema", and `ManifestClaim` has none, `src/baseline.rs:71` — so a
      format bump moves no signature; and `claim_id`, the id the manifest records beside the address). A key added to `ClaimFile`
      without a classification fails it, so an omission is a failing test and no longer a silence.
   3. `req.claim/4`'s file contract names `kind` and `fields` as outside identity, as `area` is.
   4. **The fingerprint bet is untouched and says so.** `req bench` and the corpora read statements, not claim files
      (`fields-separation.md` §3), so FMR = 0 is not measured by any of this and is not weakened. Two claims with the same
      statement and different fields share an address, as two claims differing only in rationale do today: the address is
      a drift signal and the claim id is the identity (ADR 0020).

6. **A `fields_hash` in Baseline envelope v5.**
   - `BASELINE_VERSION` becomes 5 (`src/baseline.rs:47`). Each manifest claim gains `fields_hash`
     (`ManifestClaim`, `:71`), appended after `tests_hash` in the signed bytes **only for `version >= 5`**, exactly as
     `tests_hash` is gated on `>= 4` (`:195-203`): the comment there is the reason, that a key added for old claims
     "would silently stop every old signature from verifying".
   - **What it covers**: the claim's `kind` and every field it carries, as the manifest attests the other surfaces: the
     digest is `sha256("req.fields.v1\0" ‖ bytes)` (a new domain tag beside `RELATIONS_TAG`, `RATIONALE_TAG` and `TESTS_TAG`,
     `src/model.rs:49-56`), over the compact JSON
     ```
     {"kind":<name or null>,"fields":{<name>:{"type":<type>[,"of":<type>],"value":<canonical value>},…}}
     ```
     with field names sorted by byte, the keys of each object in the order shown, `integer` and `decimal` as their canonical
     strings, `bool` as a JSON boolean, a `list` as the array of its canonical element texts **sorted by byte** (no number is
     parsed, so no float is possible), and no whitespace. A claim with no kind and no fields hashes `{"kind":null,"fields":{}}`,
     so every claim in a v5 manifest carries one and *adding* a kind later is a change. An absent field is not in the object; an empty list
     is `"value":[]`; the two differ. The type tag rides in the hash so a field redeclared as another type changes it.
   - **What it does not cover**: the *declarations* in `.req/kinds/`. Editing a kind after signing (relaxing `required`,
     reordering an `enum`) changes no `fields_hash`. This is a **named loss**, in the manner of ADR 0022's: the witnesses are
     the git history, the load-time refusal of a value the new declaration rejects, and the `kinds: changed` line `req diff`
     prints once (decision 8). It matters the day a check reads an enum's order, because an unsigned order would sit
     under a signed-looking verdict; decision 9 puts that on the checks ADR.
   - **The nine sites** of the research record (§2.3) are the implementing change's list, with the same gate on each:
     the digest and tag (`model.rs`), the row (`view.rs:77`, computed beside `:1237-1241`), `ManifestClaim`, `manifest()`
     (`:118`), `canonical_bytes` (`:184`), `from_json` (`:282`), `drift` (`:487`), the disclosure (`:2045`) and
     `metadata_differs` (`src/diff.rs:704`), and a tenth the research did not list: `req diff`'s staging, `Side` and the
     `kinds_changed` key, which carry the declarations (decision 8). Two hazards the research did not name:
     - `from_json` refuses a manifest missing `tests_hash` only when `version == BASELINE_VERSION` (`:325-332`); under a v5
       build that equality is false for v4, so a v4 manifest stripped of its `tests_hash` would be accepted. It becomes
       "at or above the version that introduced the key", named by constants and not literals: the existing
       `TESTS_ATTESTED_FROM` (`:51`), and new `FIELDS_ATTESTED_FROM` (and `PINS_ATTESTED_FROM` if decision 7 is kept). It is
       **a task of its own, done before any v5** (*Consequences*).
     - `drift`'s guard for each hash is the closed window `introduced..=BASELINE_VERSION` (`:515-526`), never `>=`, so a
       future envelope's hash is not this build's to compare; the literal `4` there becomes the constant.
   - **What v5 means for existing signatures.** A v4 manifest verifies as before, because `canonical_bytes` keeps the v4
     bytes for `version == 4`: the three committed manifests (`baselines/air-gapped.json`, `release-parity.json`,
     `release-parity-and-adversarial.json`, all `"version":4`) stay `[VERIFIED]` and need no action for the implementing change to
     land green. They do **not** attest fields (or pins, if decision 7 is kept), and the verifier says so once: `predates envelope v5: the
     authored fields and pins are NOT attested by this manifest` (or, if decision 7 is struck, `… the authored fields are
     NOT attested …`), keyed on the support floor and the constant as the v4 line is
     (`:2045`). A field or a pin edited after signing is invisible to such a manifest, which is probe C2's behaviour today.
   - **The ordering is the owner's, and release-bound.** A build that does not know v5 reports a v5 manifest `[UNVERIFIABLE]
     manifest version 5 is newer than the version this build writes (4)`, exit 3, and does not judge the signature
     (probe F; `src/baseline.rs:1447-1451`, `:1550`). So the build that reads v5 ships first; only then does the owner re-emit
     (`req baseline --at <ts>`) and re-sign (`minisign -Sm`; req never signs) the three manifests, and every adopter does
     the same for theirs. Because this build writes v5 by default, a consumer pinned to an older build turns red on the
     first manifest it re-emits; that is why the pin moves with the release (ADR 0055 decision 7).

7. **Recommendation: the same envelope bump closes the pins loss with a `pins_hash`.** *(Severable: the owner may strike this
   decision and keep v5 for fields alone; the cost of that is below.)*
   - **The loss is measured, not hypothetical.** Deleting a `pins:` entry left every attested value byte-identical and
     `baseline --verify` `[VERIFIED]`; `pins_hold` then judged one pin fewer and passed (probe C2, `checked: 6` for 7). ADR 0022
     named it, named the remedy ("a `pins_hash` beside `tests_hash` (envelope v5)") and made it the follow-up "if pins become
     load-bearing for a release". They are: `pins_hold` is one of the gate's eight policies, `AGENTS.md` forbids editing a pin to
     fit a parse, and ADR 0026 calls an edited pin "how a flagged parse gets silenced".
   - **One re-signature against two.** An envelope bump is a release the notes call identity-moved (a minor bump, ADR 0055),
     a reader-before-writer ordering, and a human `minisign -Sm` over every manifest. Doing `fields_hash` now and
     `pins_hash` later is two of each for three manifests here and for every adopter's, and a v5 manifest would then itself
     predate v6. The marginal cost of `pins_hash` inside this bump is one digest function, one more key and one more clause in
     the disclosure; the sites are the same.
   - **The digest**: `sha256("req.pins.v1\0" ‖ bytes)` over the compact JSON array of `[key, value]` pairs in the fixed order of
     `Pins::entries` (`src/model.rs:242`: `modality, subject, operator, value, unit`), only the keys present. `pins: {}` and no
     `pins:` are the same empty array, as `req diff` already compares them (`src/diff.rs:688-695`). It attests what the author
     pinned, never a verdict against the parse, which is derived.
   - **The trade-off, stated against it.** The decision is wider than this ADR's title, and an edit to a pin after signing
     becomes FAIL until a re-sign. That is the point (ADR 0013: provenance is what an auditor relies on; a pin edited to match a
     parse is the failure `AGENTS.md` names), but it is a cost to an author who corrects a mistaken pin. *If struck:* v5
     carries `fields_hash` only, `pins` stays on the coverage test's allowlist with ADR 0022 as its reason, and `pins_hash`
     waits for an envelope v6 that repeats this decision's ordering and re-signature.

8. **A field change is reported, and not given a direction.**
   - **`req diff`.** A claim whose `kind` or fields differ is `metadata` (ADR 0026 §2: "only fields outside identity changed")
     when its statement and applicability did not, and carries a flag **`fields-changed <name>[,<name>…]`**, naming the fields
     (`kind` when the kind changed), sorted. The flag is raised under any class, as `pins-changed` and `tests-lost` are, so a
     field edit made beside a reworded statement is still shown. **`--fail-on fields-changed`** joins the accepted spellings
     (`FailOn`, `src/diff.rs:143`; the flag, `:114`) and is **opt-in per repository**, as every `--fail-on` class is (ADR 0026
     §4: nothing is on by default; a repository that wants a field edit to block a merge names it); `metadata_differs` (`:704`) compares the fields digest, so a list reordering
     is not a change. A change to the declarations is reported once at the top as `kinds: changed`, as a registry change is
     (`registry: changed`, printed at `src/diff.rs:1152-1153` from the comparison at `:849`, ADR 0026 §2), and `req.diff/1` gains a top-level boolean `kinds_changed` beside
     `registry_changed` (additive; ADR 0026 §6). `same-address-as <id>` (`:121`) also names it when the added and the
     removed claim differ in kind or fields (research L11): a copy with another level is not a bare renumber.
   - **`req diff` must read each revision's declarations, and today it cannot.** It asks git for `claims` and `aliases`
     only (`list_corpus`, `src/diff.rs:402-412`), refuses any other path (`:437-441`), writes only those (`materialise`,
     `:542-560`), and its `Side` holds claims and the registry (`:619-625`), with `load_claims` run on each staged side
     (`:637`); `view::load_rows` (`src/view.rs:1212`) depends on the same directory. Left alone, decision 4's refusal would
     make every `req diff` over a kinded corpus exit 3, and `kinds: changed` would have no base-side bytes. So `.req/kinds/`
     joins the staged paths (the pathspec, the prefix check and `materialise`) and `Side` holds the kind files' bytes beside
     `registry`, compared for `kinds: changed`. **Each side is judged by its own declarations**, as each is derived under its
     own registry (ADR 0026 §1); a revision with no `.req/kinds/` and no claim naming a kind loads as before. A malformed kind
     file at a staged revision is that side failing to load, so `req diff` exits **3** naming the file and the revision (ADR
     0026 §3), not decision 2's exit 2, which is for the working tree a command reads directly.
   - **`req compare`** keeps its verdict, which is about direction and stays `identical` when two forms are, and prints one more
     line when the two files differ in kind or fields: `fields: changed (<names>) — not part of meaning; see req diff`. It has one
     root (`src/compare.rs:392`), so **both files are judged by the working tree's declarations**. Today a
     reviewer who runs `compare` alone sees an integrity level lowered and the word `identical` (`fields-separation.md` §1,
     the table). Exit code unchanged.
   - **No direction.** Neither command says an edit to a field strengthens or weakens a claim, even for an `ordered` enum: req
     cannot know whether a higher value is the stricter one (a level, a priority). Which way a field points is a human's
     reading, or a future check's (decision 9). `req satisfy` is unchanged: a design link is judged by the direction of the
     claim's meaning, and a field is not meaning. A Decision binds the address (`src/decision.rs:72`), so **an `accept` stays
     valid across a field edit**; that is the loss to name, and the Baseline (`fields_hash`) and `--fail-on fields-changed`
     on the pull request are what catch the edit.
   - **The verify failure names the surface.** One sentence now covers three hashes (`Drift::reasons`, `src/baseline.rs:401-413`;
     one `attested_changed` list, `:386`). With four attested surfaces (five if decision 7 is kept) it cannot stay one. A failure names the claim and the
     surfaces whose digests no longer recompute: `REQ-0101: the rationale and the fields no longer recompute — edited after
     signing (ADR 0013, ADR 0019, ADR 0056)`. It names the surface, never a hash value. When drift is the reason, the output
     also says the manifest's signature itself verified, because the manifest was untouched and the working tree moved
     (`fields-separation.md` §1, the table): today a reader cannot tell. `req audit` takes its coverage from `drift`
     (`fields-separation.md` §2.3) and so follows.
   - The JSON views (`req.trace/1`, `req.render/1` and the others that serialise a row) gain `kind` (`null` when absent)
     and `fields` (an object holding only the fields present) after `area`, additively under ADR 0046 decision 9. Integers
     and decimals are the canonical *strings*, booleans JSON booleans, lists arrays. No text projection changes. `req canon` prints `kind <name>` and `fields <name>=<value> …` after `area`, as diagnostics
     (`src/canon.rs:2402` prints `area` the same way); it reads the values only to show them.

9. **What fields are not.**
   - **A field is never ratification.** A field named `status` with the value `approved` is data an author wrote. It is not a
     signed Decision (ADR 0005: authority is a signed Decision, not a hash and not a label) and nothing reads it to accept, merge or
     release a claim; the model proposes and a human disposes (ADR 0004), and an importer's source status is not ratification
     either (ADR 0027 decision 4 and *Rejected*, ADR 0028 decision 7). The misreading is prevented in the places an adopter
     reads: the glossary entry says it and lists `status` and `approval` under *Avoid*; `docs/claim-format.md`'s field
     reference and `req --help` state it beside the `kind` and `fields` description; and the rule is a contract, not a word
     list: **no gate policy, selector, projection or exit code reads a field's value**. The only readers are the loader
     (validation), `fields_hash`, `diff` and `compare` (reporting), `req canon` and the JSON views (display) and the two findings of decision 4.
     A reserved-word list was rejected: an author writes `sign_off`.
   - **A field is not an area (ADR 0051).** An area is one place in the reader's layout, one per claim, outside the Baseline
     on purpose. A field is a typed attribute that cuts across sections and **is** attested. This ADR answers ADR 0051's open
     question (*Not decided here*: where the cross-cutting classifications live): in fields. `area` stays what it is and is
     not migrated into a field, which would attest placement and undo ADR 0051 decision 4. A field gives no layout: a claim
     with a list field is in several values and a page cannot show it once, so no view groups by a field here.
   - **A field is not applicability (ADR 0003).** A variant is a selector over a context (`key=value` words), evaluated by
     `parse_predicate` and `applies` (`src/select.rs:69,123`), and applicability is an input to the address. A field describes
     the claim; a variant key describes the product. **A selector may not read a field**, in this ADR or by implication: it
     would make a claim's variant membership depend on a datum outside its address, so an edit would change whether a claim
     applies and move no address (research L3). Whether a repository *check* may read a field (`when: claim.fields.asil in
     [C, D]`) is evaluation, not selection, and is future work for the checks ADR, which must also decide whether an
     enum's declared order is attested before a check reads it (decision 6).

10. **What stays outside.** Inheritance between kinds, a kind with several parents, unions, tuples, nested objects and
    sequences (a list is a set); a default, a range, a pattern or a cross-field rule ("if the level is D, a review date
    is required"); a field of one claim defined in terms of another's; `fields` printed in a text projection, a view grouped
    by a field and a field *value* in the audit packet (the findings of decision 4 are printed there). **Importer mapping is out of scope** here and for the first implementation:
    the three importers keep drafting `req.claim/2`, with no kind and no field, and keep reporting a source's `status`,
    tags, safety and security levels as **not carried** (ADR 0027 decision 4, ADR 0028 decision 7). Mapping a source
    attribute to a field is a follow-up per importer, on the pattern of ADR 0051's I-1: opt-in (`--fields`), the kind declared
    by the adopter and never invented, a source status mapped only to a field named for what it is (`source_status`), and
    a source object with no value for a required field left absent, never defaulted. Tags, which ADR 0051 *Rejected* as a
    layout, are a `list` field.

## Consequences

- An adopter files an integrity level, a verification method or a review date on the claim, validated against a declaration
  they own, and a rule on it (checks R1, R2) can be written once the checks ADR exists. An edit to one after signing fails
  `req baseline --verify` against a v5 manifest, names the surface, and is a `--fail-on fields-changed` on the pull request.
- No adopter migrates. A corpus that files no kind keeps its `/2` or `/3` claims; `req init` writes `req.claim/4`
  (`src/init.rs:336` derives it from the constant); an adopter who wants a field edits one claim's `schema:` line and adds a
  kind file.
- **The release notes must say so, and today they say the opposite.** `scripts/release-notes.sh:143` prints "a claim file
  declaring the old schema is NOT JUDGED by this build" for any schema change, which is false for a format on `READS_AS_IS`
  (it was already false for `/2` → `/3`) and would tell every `/3` adopter that decision 1 does not hold. The implementing
  change makes the line say "loaded as it is" when the previous schema is on the **released** rev's `READS_AS_IS` (read with
  `const_at "$full"`, never the previous rev's, whose list is what would reprint the false line), and "NOT JUDGED" only
  otherwise. `READS_AS_IS` is a `&[u32]` (`src/canon.rs:79`), and `const_at` matches only `pub const NAME: u32 = <digits>;`
  (`scripts/release-notes.sh:94`), so the script needs a second, list-reading extractor, with a row in `tests/release_notes.rs` for each
  (the existing row, `:415-419`, pins `req.claim/2 → req.claim/3`).
- **A task of its own, before any v5.** `from_json` must require `tests_hash` of every manifest at or above
  `TESTS_ATTESTED_FROM` (`src/baseline.rs:51`), not only of `BASELINE_VERSION`'s (`:325`), and `drift`'s literal `4` (`:526`)
  becomes the constant; the new keys get `FIELDS_ATTESTED_FROM` (and `PINS_ATTESTED_FROM` if decision 7 is kept). It lands as
  its own change (PR #104), with the rule as a version-parameterised helper, `requires_tests_hash(version)`, tested at
  `BASELINE_VERSION + 1`, since `BASELINE_VERSION` is a compile-time constant and a test against it is vacuous until it is 5,
  because once v5 ships the accepted manifest is a silent loss of an attested surface.
- **The release is a minor bump** (ADR 0055 decision 2): the claim schema and the envelope both move. `docs/claim-format.md`
  (the *Version axes* table and the field reference), `docs/adopting.md`, the `req --help` pages (`src/main.rs:192-205`), the
  three importers' accepted schemas, `req init`'s starter and the glossary change with it.
- The three committed Baselines stay v4 and green until the owner re-signs them after the release: until then fields and
  pins are unattested for req's own corpus (it files no kind; it has nine pins on five claims) and the verifier says so.
- **A ratification outlives a field edit.** `accept` Decisions bind the address, so a Decision recorded at one integrity level
  is still valid after the level is changed. For a safety attribute that is a real gap: only the Baseline, a `diff` gate and
  review stand between the edit and a release. It is recorded, not closed (decision 8).
- **The declarations are unsigned** (decision 6): a reordered `enum` or a relaxed `required` is visible in git and in
  `kinds: changed`, and in no manifest.
- Cost: a digest per claim for one or two more surfaces, derived from rows verification already loads; no network, no model I/O; no
  new dependency (a date is validated by hand and no number is parsed on the digest path).
- The separation is **a design to be tested, not a result**: `fields-separation.md` §5 lists what it does not show, and
  nothing here changes that until decision 5's tests run on a real implementation.

## Rejected

- **An untyped `fields:` map with no declaration.** It is the shortest thing, and it would reproduce the silent-loss class the
  strict loader exists to remove (`src/model.rs:11-15`): a misspelled field would be a new field, with no order and no
  `required`, so checks R1 and R2 would still be unstateable.
- **A global field list with no kinds.** `required` would then bind every claim, or none; "a safety claim carries a level and a
  design note does not" could not be said.
- **A default value.** It turns UNKNOWN into a value; see decision 4 and R1.
- **Fields in the canonical form.** A field edit would then be a meaning change that moves the address, needs re-ratification,
  and makes every claim that carries the field a `RULES_V`-class event (research L1, L2). It would also put an attribute inside
  the fingerprint that FMR = 0 is measured on.
- **Leave fields unattested, as `area` is.** ADR 0051 accepted that for placement, "not meaning". A safety attribute is what an
  auditor relies on (ADR 0013), and probe C2 shows what an unwired key costs.
- **A reserved list of names** (`status`, `approved`): lexical, and an author writes `sign_off`.
- **A selector that reads a field**, and **migrating `area` into a field**: both in decision 9.
- **`pins_hash` in a later envelope**: decision 7, offered as the owner's alternative to the recommendation.
- **Accept `kind` and `fields` inside `/3`**: decision 1.

## Not decided here

- The checks ADR: whether a repository check reads a field and how the order of an `enum` is attested before one does.
- A **direction** for a field change, including an `ordered` enum's.
- A flag for a rationale-only or relations-only `diff` edit (`fields-separation.md` §1: `--fail-on` has none); decision 8 adds
  the one for fields and the same shape would serve.
- A Decision that covers fields, or a ratification that a field edit invalidates (it needs a decision-chain version).
- Importer mappings, a view grouped by a field, and a field in a text projection or the audit packet.

## Falsification

This decision is wrong if any of these is observed:

1. adding, changing or removing a claim's `kind` or a field moves its address, any other claim's address, a `req selftest`
   answer or `req bench`'s result, or changes a claim's applicability or derived parse;
2. a field edit **after signing** leaves a v5 manifest `[VERIFIED]`, or (if decision 7 is kept) a pin edit does (probe C2
   repeated against v5), or a `kind` change does;
3. a v4 manifest signed before this ADR stops verifying under a v5 build, or a v4 manifest lacking `tests_hash` is accepted;
4. a `/3` or `/2` file carrying `kind` or `fields` loads, a `/4` build refuses or rewrites a `/3` or `/2` file, or a `/3` build
   reading a `/4` file reports anything other than a newer format;
5. the same kind and fields hash to two digests (a list reordered, a machine, a run), an absent field and an empty list hash
   alike, or two surfaces trade a digest;
6. an absent value reaches a consumer as a default, `null` or `false`, a required field missing is folded into PASS, or an
   unknown kind, an unknown field or a wrong type loads;
7. `req diff` reports a claim whose only change is a field as `unchanged`, or exits 0 under `--fail-on fields-changed`; `compare`
   prints a field edit as a direction;
8. a key added to `ClaimFile` with no classification passes the coverage test;
9. any gate policy, selector, projection or exit code reads a field, or any command treats a field value as ratification;
10. a real adopter's attributes cannot be expressed in the eight types and the declared constraints, or an adopter's
    requirement for a default or a cross-field rule is the first thing they ask for;
11. an added claim and a removed claim that differ **only** in `kind` or fields are reported by `req diff` without
    `same-address-as`, or with it and without saying the kind or fields differ (research L11);
12. an edit to a `.req/kinds/` file between two revisions is not reported by `req diff` as `kinds: changed`, or `req diff` over a
    kinded corpus exits 3 for want of a revision's declarations;
13. the release notes of the release that ships `req.claim/4` say a `/3` file is NOT JUDGED.

## Implementation note — 2026-11-18 (F-1, F-2 and F-3)

Not an edit of the decisions above; where they now live. The work was three units, in this order (`docs/specs/F-*.md`).

- **F-1, PR #106 — the format and the loader.** `req.claim/4` and `READS_AS_IS = [2, 3]` (decision 1); `.req/kinds/` and its
  strict reader, `src/kinds.rs` (decision 2); the eight types and the one function that judges a claim's values against the
  declarations, shared by the corpus loader and `req compare` (decisions 3 and 4); `req diff` staging `.req/kinds/` beside
  `claims/` and `aliases/`, each side judged by its own declarations (decision 8, the staging half); `req canon`'s display;
  the release-notes line (*Falsification* 13).
- **F-2, PR #107 — the Baseline.** Envelope v5, `fields_hash` and `pins_hash` (decisions 6 and 7: decision 7 was kept, so v5
  attests the pins as well), `FIELDS_ATTESTED_FROM` and `PINS_ATTESTED_FROM`, the verify failure naming the surface, the
  one-line disclosure on a v4 manifest, the coverage test over `ClaimFile`'s keys (decision 5) and leaks L7 to L9. Task PR #104 had landed first: `from_json` requires `tests_hash` from the envelope that
  introduced it.
- **F-3, PR #108 — reporting.** `req diff`'s `fields-changed` flag, `--fail-on fields-changed`, `kinds: changed` and
  `kinds_changed`, and `same-address-as <id> (fields differ: …)` (decision 8, L10 and L11); `req compare`'s one added line;
  the `kind` and `fields` keys of the JSON views (L5); `missing_field`, `dangling_field_ref` and `strict fields` in
  `req check`'s completeness step, and the audit packet's names (decision 4); `req pin --verify` covering `.req/kinds/` (a
  finding deferred from F-1's review); and the readers-closed test (decision 9, *Falsification* 9), which scans the gate, the
  selectors, the contradiction detector and the text renderers for any read of a kind or a field.

Where the work departed from the ADR's wording, or chose where it was silent:
- **`kinds: changed` is printed only when it is true.** `registry:` prints `unchanged` as well; a second always-printed line would
  change every `req diff` of a corpus with no kinds. `req.diff/1`'s `kinds_changed` is always present, `false` when nothing changed.
- **`fields-changed` names `kind` among the field names, sorted by byte** (decision 8: "`kind` when the kind changed, sorted"); a
  field a kind declares under the name `kind` would be indistinguishable in the flag, and the declaration grammar permits it. It is
  recorded, not prevented.
- **The audit packet names the claim, the kind and the field for `dangling_field_ref`, and not the missing id**, because a `ref`
  target is a field value and decision 4 keeps the value out of the packet (decision 10). `req check` names the id, as decision 4
  says. The packet's findings are reported, not judged, as its `unrelated` is.
- **`strict fields` promotes `missing_field` only.** A dangling `ref` stays UNVERIFIABLE under every policy, the class of a dangling
  relation; the ADR states it for `missing_field` and is silent on whether the word reaches both.
- **The completeness step's label counts one more check when `.req/kinds/` declares a kind** (`structural omissions (4 checks)`),
  as `.req/areas` does; a corpus with no kind prints the label it always printed.
- **`req compare`'s `fields:` line follows the verdict and its accompanying lines**, and is printed on a pair the classifier
  cannot read as well, before its exit 3.
- **`req canon`'s help said the Baseline attests nothing about a field in this build**; the sentence predated envelope v5 and was false once F-2 merged.
  It now says the Baseline attests them from envelope v5.
