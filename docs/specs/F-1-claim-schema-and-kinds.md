# F-1: `req.claim/4`, kind declarations and the loader that judges fields (ADR 0056, unit 1 of 3)

Task id: `F-1`. Base: the commit that adds this spec, on top of `origin/main` at `fca4ba0`. ADR 0056 is accepted: the owner
merged PR #103 on 2026-11-18, and PR #105 records the acceptance. The `tests_hash` task the ADR asks for before any v5 is
done (PR #104). The owner chose on 2026-11-18 to implement the ADR in three units, starting with this one.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0056-a-claim-may-carry-typed-fields-outside-its-address.md`: decisions 1–5, decision 8's
paragraph on `req diff` reading each revision's declarations, decision 9, and its *Falsification* list. Where this spec
and the ADR disagree, the ADR wins: stop and report. The ADR's line numbers are for `960e7ed`, and only `src/baseline.rs`
has moved since then.

## The three units

| unit | what it carries |
| --- | --- |
| **F-1** (this) | the format, the declarations, judging values at load, every command that loads claims, `req canon`'s display, and the release-notes line |
| F-2 | Baseline envelope v5: `fields_hash` and `pins_hash` (decision 7 was not struck), the `ClaimFile` key coverage test, the verifier naming each surface that fails, and leak tests L7–L9 |
| F-3 | reporting: `req diff`'s `fields-changed` flag, `kinds: changed`, `kinds_changed`, `same-address-as` (L10, L11); `req compare`'s fields line; the JSON views (L5); `missing_field`, `dangling_field_ref` and `strict fields`; the audit packet; the adopter's paragraph in `docs/adopting.md`; the ADR's implementation note |

Until F-2 lands, a `/4` claim's fields load but are attested by no Baseline. **Do not release from `main` between F-1 and
F-2.** This repository files no kind, so nothing here is at risk.

## Choices fixed here

1. **The format** (decision 1), in `src/canon.rs` and `src/model.rs`.
   - `CLAIM_SCHEMA_V` becomes 4. `READS_AS_IS` becomes `[2, 3]`: no message, no rewrite, no address moved.
   - `ClaimFile` gains `kind: Option<String>` and `fields`, an ordered map of field name to the raw YAML value (a
     `BTreeMap`; no `HashMap` where anything is iterated).
   - A `/2` or `/3` file carrying the **key** `kind` or `fields` is a parse error. It names the key and `req.claim/4`, and
     is judged on the key, not its value, exactly as `area` is in a `/2` file (`src/canon.rs:1973-1989`).
   - `area` stays valid in `/3` and `/4`.
   - `fields` without `kind` is a parse error.
   - `parse_claim` judges the file's own shape and the name grammar (decision 4, last bullet): kind and field names match
     `[a-z][a-z0-9_]*`. Judging values against a declaration is item 3, not `parse_claim`.
2. **The declarations** (decision 2): `.req/kinds/<kind>.yml`, `schema: req.kind/1`.
   - The keys are exactly those in decision 2: `schema`, `label` (optional), `fields`, and per field `type`, `values`,
     `ordered`, `required` and `of`.
   - Refused, naming the file and the key:
     - an unknown key, including `default:`;
     - a stem that is not a valid name;
     - a stray file under `.req/kinds/` (not `*.yml`, a subdirectory, a symlink);
     - `ordered` on anything but an `enum`, `values` on anything but an `enum` or a list of `enum`, `of` on anything but
       a `list`;
     - an `enum` with no values, a duplicate value or a token outside `[A-Za-z0-9][A-Za-z0-9_.-]*`;
     - a list of `bool` or of `list`.
   - A newer `req.kind/N` is NOT JUDGED, worded like a newer claim format.
   - A malformed declaration is **exit 2**, every bad file and key in one run, as `.req/areas` is (`read_areas`,
     `src/view.rs:869`). An unreadable one is exit 3.
   - **Where it is read:** once per load, from the root a command already resolved, next to the registry, in
     `load_claims` (`src/canon.rs:2092`). Today `load_claims` returns `Result<_, String>` and every caller maps that to
     exit 3. Carry the exit-2 class in the smallest way the callers allow. If that means touching every caller, do so,
     and say so in the envelope. No caller may fold the exit-2 class into exit 3, or the reverse.
   - A corpus with no `.req/kinds/` and no claim naming a kind loads exactly as today.
3. **Judging a value** (decisions 3 and 4): one public function in a new module, `src/kinds.rs`, is the single home.
   - The corpus loader and `req compare` (`src/compare.rs:469`, which holds a root) both call it. `compare` judges both
     files by the working tree's declarations.
   - The eight types and their refusals are exactly the table in decision 3. Lean on what `yaml_serde` delivers: a quoted
     `"3"` arrives as a string and an unquoted `3` as a number, so the two are told apart by kind, never by re-reading
     text.
   - For each spelling the table names, a test pins what the loader delivers: `true`, `True`, `TRUE`, `yes`, `on`,
     `"true"`, `3`, `"3"`, `0.3`, `"0.30"`, `null`, `~`, an unquoted `2027-03-01`, `"2027-02-29"`, `"2028-02-29"`.
   - `decimal` reuses `is_canonical_decimal` (`src/canon.rs:1852`).
   - `ref` reuses the claim-id grammar the loader already enforces. Do not write a second one.
   - `date` is checked by hand, with integer arithmetic only.
   - Refusals are exit 3, NOT JUDGED. Each names the claim, the kind, the field and the declared alternatives (the
     `values`, or the type):
     - a `kind` with no declaration file, and every kind when `.req/kinds/` is absent;
     - a field the kind does not declare;
     - a wrong type or a value outside an `enum`;
     - a list with a duplicate;
     - `null`.
   - Problems are **collected**, per file, in path order, the way `load_claims` collects them today (Q-1, item 3).
   - A missing required field loads. Its finding is F-3's.
   - A `ref` that points at no claim loads too. Its finding is also F-3's.
   - Keep the judged value somewhere F-2 can hash and F-3 can print: the field's declared type with its value in
     canonical text (a `bool` as a boolean, a `list` as its elements). Do not hash anything in this unit.
4. **Outside identity** (decision 5.1, the leak tests this unit can run).
   - `derive` (`src/canon.rs:1784`) does not read `kind` or `fields`.
   - Tests for L1, L2, L3, L4 and L12 as decision 5.1 states them. L2 includes the `CanonicalForm` key-set test
     (`tests/canonical_form.rs:35`), `req selftest` and `req bench` unchanged, and `RULES_V` still 9.
   - No text projection prints a field.
   - No gate policy, selector, projection or exit code reads a field's value (decision 9). A selector that names a field
     is not a new feature to add.
5. **`req diff` stages the declarations** (decision 8, the paragraph that begins "`req diff` must read each revision's
   declarations").
   - `.req/kinds/` joins the staged paths: the pathspec in `list_corpus`, the prefix check, `materialise`
     (`src/diff.rs:402-560`).
   - Each side is loaded with its own declarations.
   - A malformed or missing declaration at a staged revision makes that side fail to load: exit **3**, naming the file
     and the revision (ADR 0026 §3).
   - Do **not** add `kinds: changed`, `kinds_changed` or the `fields-changed` flag: those are F-3. `Side` may carry the
     kind files' bytes if that is the natural shape, but nothing compares them yet.
6. **`req canon`** prints `kind <name>` and `fields <name>=<value> …` (sorted by name) after `area` on the claim's line,
   as `area` is printed (`src/canon.rs:2401`). It is a diagnostic only.
7. **`req init`** writes `req.claim/4` through the constant, and needs no kind. Update the init README text that names a
   format. `req --version` reports `claim_schema req.claim/4`.
8. **The importers** (decision 1, its importer bullet; decision 10).
   - `ACCEPTED_SCHEMAS` in `tools/adopt.py`, `tools/needs_import.py` and `tools/reqif_import.py` gains `req.claim/4`.
   - Their drafts do not change: `/2`, or `/3` with an area.
   - Tests that use `req.claim/4` as "another schema" use `req.claim/5`, including `tests/area.rs:252`'s newer-format
     test.
9. **The release-notes line** (*Consequences*, "The release notes must say so").
   - `scripts/release-notes.sh` gains a list-reading extractor for `READS_AS_IS`, read at the **released** rev
     (`const_at "$full"`'s sibling).
   - The schema-change line says the old schema is "loaded as it is" when the previous schema is on that list, and
     "NOT JUDGED" only otherwise.
   - `tests/release_notes.rs` gets a row for each wording. The existing `/2 → /3` row (`:415-419`) becomes the
     "loaded as it is" case.
10. **This repository's claims stay `/3`.** Nothing under `claims/` or `.req/` changes. That is the live proof that
    `/3` loads as it is.

## Tests first (envelope `red`)

Use public seams only: the CLI over a scratch corpus, and the public functions of `req::canon`, `req::model` and
`req::kinds`. Oracles come from the ADR, never from a run. At minimum, one test for each of these *Falsification* items:
1 (address, selftest, bench, applicability and parse), 4 (format gating), 6 (refusals; "absent is never a default" is
shown by `req canon` printing no value), the non-reporting half of 12 (`req diff` over a kinded corpus does not exit 3
for want of declarations), and 13 (release notes). In addition:

- a `/3` claim, and the same claim declared `/4` with no kind, load with identical addresses, and neither is reported as
  a migration candidate;
- a `/4` claim with a kind and three fields has the same address as without them, and changing a field's value changes
  no address;
- a `/3` file carrying `kind`, a `/3` file carrying `fields` and a `/2` file carrying either are each a parse error naming
  the key and `req.claim/4`;
- `fields` without `kind` is a parse error;
- a kind with no declaration, a field not declared, a value outside an `enum`, a list duplicate, `null`, and each type's
  refused spellings from the decision 3 table are each exit 3 naming the claim, the kind and the field;
- the loader-behaviour pins listed in choice 3;
- a declaration with an unknown key, `default:`, a bad stem, a stray file, `ordered` on a `string` and an `enum` with a
  duplicate value is exit 2, every one in one run;
- `req.kind/2` is NOT JUDGED;
- `req diff` between two revisions of a kinded corpus loads both sides; a revision whose declaration is malformed is exit
  3 naming the file and the revision;
- `req compare` judges a `/4` file's fields against the working tree's declarations, refuses a wrong value (exit 3), and
  keeps its verdict and exit code otherwise;
- each importer accepts a `req` reporting `/2`, `/3` or `/4`, and refuses `/5`.

Prove each of these red at the base.

## Mutation checks (envelope `mutations`)

- Feed `fields` into the canonical form: the same-address test fails.
- Remove `3` from `READS_AS_IS`: the `/3`-loads test fails.
- Accept `kind` in a `/3` file: the parse-error test fails.
- Accept an unquoted integer for an `integer` field: its refusal test fails.
- Drop `.req/kinds/` from `req diff`'s pathspec: the kinded-diff test fails.
- Fold the exit-2 class into exit 3: the malformed-declaration test fails.

## Verify

- Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, and the tree is clean after the run.
- `docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and still verify, with no re-sign.
- `req --version` reports `claim_schema req.claim/4` and `rules_v 9`.
- `tools/quickstart.sh` still passes under 300 s.

## Docs, one home each

- `docs/claim-format.md`:
  - the status line;
  - *Version axes*: `/4`, and that `/2` and `/3` load as they are;
  - the field reference gains `kind` and `fields`. Both are outside identity. A field is never ratification (decision 9).
    The Baseline attests nothing yet; F-2 changes that sentence, so phrase it as of this unit;
  - the `.req/kinds/` grammar, and the eight types with their canonical spellings;
  - the migration section: `/3` needs none.
- The `req --help` pages that describe the claim format (`src/main.rs:192-205`), `req canon --help` and `req init --help`.
- Where a doc or example states the schema **this build** reads or `req init` writes, update it to `/4`. That includes
  `examples/agents-snippet.md` and `examples/handoff-brief.md` if `tests/ci_template.rs` allows, and `docs/adopting.md`'s
  starter-claim row. Leave lines about released builds, and history ("predate `req.claim/3`"), as they are.
- `CONTEXT.md` already defines Kind and Field. Change it only if a term is missing.

## Allowed paths

- `src/canon.rs`, for the format judgement, the declarations read, collecting refusals and the display. Nothing in the
  canonical form or `derive`.
- `src/model.rs`, `src/kinds.rs` (new), `src/lib.rs` (the module line), `src/compare.rs`, `src/diff.rs` (staging only,
  choice 5), `src/main.rs` (help pages and exit mapping), `src/init.rs`.
- The other callers of `load_claims` (`src/view.rs`, `src/satisfy.rs`, `src/contradict.rs`), only to carry the exit-2
  class.
- `scripts/release-notes.sh`.
- `tools/adopt.py`, `tools/needs_import.py`, `tools/reqif_import.py` (`ACCEPTED_SCHEMAS` and the test-facing text only).
- `tests/**`.
- `docs/claim-format.md`, `docs/adopting.md` (choice-7 rows only), `examples/agents-snippet.md`,
  `examples/handoff-brief.md`, `CONTEXT.md`.

Forbidden:
- `RULES_V` and any change to the canonical form;
- `src/baseline.rs` and any attested hash;
- `src/select.rs`, `src/gate.rs`, `src/completeness.rs`, `src/check.rs`, `src/audit.rs`;
- `bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`;
- `docs/spec.md`, `docs/trace.md`, `docs/adr/**`, `scripts/check.sh`;
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/**`.

If a forbidden file must change, stop and report.
