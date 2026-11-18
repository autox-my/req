# F-3: reporting a field change, the JSON views and the two findings (ADR 0056, unit 3 of 3)

Task id: `F-3`. Base: the commit that adds this spec, on top of `origin/main` at `a152ef7`. F-1 (PR #106) and F-2
(PR #107) are merged:
- `req.claim/4` and `.req/kinds/`;
- `LoadedClaim::fields` (`src/canon.rs:2071`);
- envelope v5, with `fields_digest` and `pins_digest` in `src/model.rs`, and `Row::fields_hash` and `Row::pins_hash`.

The unit table is in `docs/specs/F-1-claim-schema-and-kinds.md`. This is the last unit. When it merges, ADR 0056 is
implemented.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0056-a-claim-may-carry-typed-fields-outside-its-address.md`. This unit covers:
- decision 4: the `missing_field` and `dangling_field_ref` findings, `strict fields`, and the audit packet;
- decision 5.1: L5, L10 and L11;
- decision 8: `req diff`, `req compare` and the JSON views;
- decision 9: the readers are closed;
- *Falsification* items 6 (the "folded into PASS" half), 7, 9, 11 and 12.

It also carries one finding deferred from F-1's review: `req pin --verify` ignores `.req/kinds/`.

Where this spec and the ADR disagree, the ADR wins: stop and report.

## Choices fixed here

1. **`req diff`** (decision 8, first bullet). Line numbers are for `src/diff.rs` at the base.
   - **`metadata_differs`** (`:712`) also compares the fields digest, which covers the kind. A reordered list is
     therefore not a change.
   - **The flag `fields-changed <name>[,<name>…]`** (`Flag`, `:114`): the field names whose value was added, removed or
     changed, plus `kind` when the kind changed, sorted by byte. It is raised under **any** class, as `pins-changed`
     is (`:756`).
   - **`--fail-on fields-changed`** joins `FailOn` (`:143`). It is opt-in: nothing is on by default.
   - **`kinds: changed`** is printed once at the top, beside `registry: changed` (`:1160`). It is computed by comparing
     the two sides' `.req/kinds/` files as bytes. F-1 already stages them; give `Side` the bytes if it does not hold
     them yet.
   - `req.diff/1` gains a top-level boolean `kinds_changed` beside `registry_changed` (`:1278`). The change is
     additive (ADR 0026 §6); update the documented key list wherever `tests/diff_template.rs` or `tests/json_format.rs`
     pins it.
   - **`same-address-as <id>`** (`:135`): when the added claim and the removed claim differ in kind or fields, the flag
     also says so, naming the fields and `kind` (L11). For example: `same-address-as REQ-0002 (fields differ: asil)`.
     A bare renumber keeps today's spelling exactly.
   - **No direction** is given for any field change, even for an `ordered` enum.
2. **`req compare`** (decision 8, second bullet).
   - When the two files differ in kind or fields, print one more line after the verdict: `fields: changed (<names>) —
     not part of meaning; see req diff`. The names are sorted, with `kind` first if the kind changed.
   - The verdict and the exit code are unchanged, and `identical` stays `identical`.
3. **The JSON views** (decision 8, last bullet; L5).
   - `Row` gains `kind` and `fields`.
   - Every `req.*/1` document that serialises a row (`src/view.rs:343-346`, and `req.trace/1`'s `nodes`) gains, right
     after `area`:
     - `kind`: `null` when absent;
     - `fields`: an object holding only the fields present, keys sorted by byte.
   - Values: `integer` and `decimal` as their canonical **strings**, `bool` as a JSON boolean, a list as an array of
     its element texts sorted by byte.
   - Replace and reorder nothing.
   - **No text projection changes:** `docs/spec.md` and `docs/trace.md` pass `--check` unchanged.
   - `fields_hash` and `pins_hash` stay out of every view, as the other row hashes do (`src/view.rs:294`).
4. **The two findings** (decision 4), in `req check`'s completeness step (`src/completeness.rs`), beside
   `undeclared_area`:
   - **`missing_field`**: a claim whose kind declares a field `required: true` and that carries no value for it. It
     names the claim, the kind and the field.
     - Its answer is **UNVERIFIABLE** (exit 3), as `in_no_baseline` is.
     - **`strict fields`** in `.req/completeness` promotes it to FAIL. It is a new word in the closed list (`:135-147`,
       and both error messages there).
   - **`dangling_field_ref`**: a `ref` value, or a `list` of `ref`, naming a claim id the corpus does not hold. It names
     the claim, the field and the missing id. Its answer is **UNVERIFIABLE**.
   - A corpus with no kinds produces neither finding and no new line. `req check`'s output for this repository does
     not change.
   - A `ref` field adds **no** edge to `trace`, `impact` or the graph (decision 4).
5. **The audit packet** (`src/audit.rs`, near `:312`; ADR 0039 decision 4) prints both findings for its selection,
   with the claim, kind and field names only. It prints **no field value** (decision 10).
6. **`req pin --verify`** (`src/pin.rs:187-207`). The uncommitted-change check covers `.req/kinds/` beside `claims/` and
   `aliases/`, and its message names all three. `req pin --drift` reports `kinds: changed` through `req diff` if it
   reuses diff's report; check that, and test it.
7. **The readers are closed** (decision 9; *Falsification* 9). Add a test that fails if a gate policy, a selector or a
   text projection reads a field's value. One approach:
   - a source scan with a positive count, which the F-1 guard did, over `src/gate.rs`, `src/select.rs`,
     `src/contradict.rs` and the text renderers in `src/view.rs`, for any read of `.fields` or `.kind`;
   - plus a behavioural leg: two corpora that differ only in a field give byte-identical `req gate` and `req select`
     output and exit codes.
8. **ADR 0056's status.** Change its status line to "accepted — ratified by the owner's merge of PR #103 (2026-11-18);
   implemented by PRs #106, #107 and this one". Add a short, dated *Implementation note*: the three units, what each
   carried, and where the work departed from the ADR's wording, if anywhere.

## Tests first (envelope `red`)

Use public seams only: the CLI over a scratch git corpus with `.req/kinds/`. Oracles come from the ADR. At minimum:
- *Falsification* 7:
  - a claim whose only change is a field is `metadata`, not `unchanged`, and carries `fields-changed <name>`;
  - `--fail-on fields-changed` exits non-zero on it, and 0 on a corpus with no field change;
  - a field edit beside a reworded statement still carries the flag;
  - `compare` prints the fields line, with no direction, and its exit code is unchanged.
- **L10:** a reordered list value is not a change.
- *Falsification* 11 (L11):
  - an added and a removed claim with one address and a different `asil` give `same-address-as` naming `asil`;
  - a bare renumber keeps today's spelling exactly.
- *Falsification* 12:
  - an edit to a `.req/kinds/` file between two revisions prints `kinds: changed` and sets `kinds_changed: true`;
  - no edit gives `false` and no line.
- **L5:**
  - `render --format json` rows and `trace --format json` nodes carry `kind` (`null` when absent) and `fields` after
    `area`;
  - every key that existed before keeps its position;
  - an absent field is absent, never `null`;
  - `docs/spec.md` and `docs/trace.md` are unchanged.
- *Falsification* 6:
  - `missing_field` is UNVERIFIABLE by default, never PASS, and FAIL under `strict fields`;
  - `dangling_field_ref` is UNVERIFIABLE;
  - neither appears on a corpus with no kinds.
- The audit packet names both findings and prints no value: assert that a distinctive field value is absent from its
  output.
- *Falsification* 9: the readers-closed test (choice 7).
- `req pin --verify` with an uncommitted `.req/kinds/` edit is a FAIL naming the file.

Prove each red at the base.

## Mutation checks (envelope `mutations`)

- Drop the fields digest from `metadata_differs`: the field-only-change test fails.
- Raise `fields-changed` only when the class is `metadata`: the reworded-plus-field test fails.
- Compare `.req/kinds/` sides as equal: the `kinds: changed` test fails.
- Make `missing_field` PASS without `strict fields`: the finding test fails.
- Print a field value in the audit packet: the audit test fails.
- Drop `.req/kinds` from `pin --verify`'s pathspec: the pin test fails.
- Make `req select` read a field: the readers-closed test fails.

## Verify

- Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
- `docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and still verify.
- `req check` on this repository gives the same answer and output as at the base.
- `RULES_V` is still 9.

## Docs, one home each

- `req diff --help`: the flag, `--fail-on fields-changed`, `kinds: changed` and `kinds_changed`.
- `req compare --help`: the fields line.
- `req check --help`: the completeness paragraph names `missing_field`, `dangling_field_ref` and `strict fields`.
- `req trace --help`: the node keys. `req audit --help`, if it lists the findings. `req pin --help`, for the pathspec.
- `docs/claim-format.md`:
  - the readers list gains `diff`, `compare` and the JSON views;
  - the `.req/completeness` grammar gains `strict fields`.
- `docs/adopting.md`: one short paragraph on filing a field. Declare a kind, set `schema: req.claim/4`, and know four
  things: what a v5 Baseline attests, what `--fail-on fields-changed` does, that an absent field is UNKNOWN, and that a
  field is never ratification.
- ADR 0056: the status line and the implementation note (choice 8). Nothing else in the ADR changes.

## Allowed paths

- `src/diff.rs`, `src/compare.rs`, `src/view.rs`, `src/completeness.rs`, `src/check.rs` (only if the finding needs it),
  `src/audit.rs`, `src/pin.rs`, `src/main.rs` (help pages).
- `src/canon.rs` and `src/kinds.rs`, only to expose what the findings or views need (a required-field list, for
  example). Nothing in `derive` or `parse_claim`.
- `tests/**`.
- `docs/claim-format.md`, `docs/adopting.md`, `docs/adr/0056-a-claim-may-carry-typed-fields-outside-its-address.md`
  (choice 8 only).
- `experiments/tier2/tests/funnel.rs`, only if `Row` gains a field and its struct literal needs it, as in F-2.

Forbidden:
- `RULES_V`, the canonical form and `derive`;
- `src/baseline.rs`, `src/model.rs`'s digests, and any attested hash;
- `src/select.rs`, `src/gate.rs`, `src/contradict.rs` (the readers-closed test reads them; it does not change them);
- `baselines/**`, `claims/**`, `bench/**`, `decisions/**`, `aliases/**`, `.req/**`;
- `docs/spec.md`, `docs/trace.md`, the other ADRs, `scripts/**`;
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/**`.

If a forbidden file must change, stop and report.
