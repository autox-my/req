# A-1: `req.claim/3`, the `area` key and `.req/areas` (ADR 0051)

Task id: `A-1`. Base: the commit that adds this spec, on top of `7db28b5`, where ADR 0051 is merged and decision 6 is
ruled. Owner go-ahead of 2026-11-13: ADR 0051 and its decision 6 merged as PRs #69 and #71.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0051-a-claim-may-name-the-area-it-belongs-to.md` (decisions 1–6 and its *Falsification*
list). Where this spec and the ADR disagree, the ADR wins; stop and report.

The graph converter is **not** in this unit. Grouping the graph by area is A-2, after V-2 (PR #70) merges, because
both change `tools/graph/trace_to_archify.py`.

## Choices fixed here

1. **The format** (ADR decision 6), in `src/canon.rs` and `src/model.rs`.
   - `CLAIM_SCHEMA_V` becomes 3, and `claim_schema()` writes `req.claim/3`.
   - A declared `req.claim/2` loads as a `/3` file with no area: no migration message, nothing rewritten, and the
     same address.
   - `req.claim/1` and older stay migration candidates, with today's message. A newer value or a malformed one is
     judged as today.
   - Model the "readable older format" as a named rule beside `classify_claim_schema_at`. The set of formats a build
     loads as they are is stated once, as data, so a later format does not re-derive it.
   - `ClaimFile` gains `area: Option<String>`. A `/2` file carrying `area` is a parse error that names the key and
     says it needs `req.claim/3`.
2. **The key** (ADR decisions 1, 2 and 4).
   - The value is segments joined by `/`, each matching `[a-z0-9][a-z0-9-]*`, with no empty segment and no leading or
     trailing `/`. Anything else is a load error naming the claim and `area`.
   - `area` is **not** an input to the canonical form or to any attested hash: no `relations_hash`,
     `rationale_hash`, `tests_hash` or manifest field carries it.
   - `RULES_V` does not move, and `req selftest`'s known answers do not change.
3. **The row.** `Row` (`src/view.rs`) gains `area: Option<String>`.
   - Every `req.*/1` document that serialises rows gains the key `area` additively (`null` when absent), on the
     `superseded_by` precedent (ADR 0049).
   - `req.trace/1`'s `nodes` gain `area` after `superseded_by`.
   - Text projections do not change, so `docs/spec.md` and `docs/trace.md` pass `--check` unchanged.
4. **The registry** (ADR decision 3): `.req/areas`, optional.
   - Grammar as the other `.req/` lists (`src/completeness.rs`'s `parse_policy` is the model): `#` comments, blank
     lines skipped, trailing CR stripped.
   - A line is `<path> <label>`; the label is the rest of the line, trimmed, any language, never empty.
   - A malformed path, an empty label or a path declared twice is refused, naming `<file>:<line>`, with every bad line
     reported in one run. Line order is the reading order. A parent need not be declared.
5. **The finding** (ADR decision 3): `req check`'s completeness step (ADR 0039) gains `undeclared_area`.
   - It names each claim whose area `.req/areas` does not declare.
   - It is reported and does not fail, like `unrelated`. It becomes FAIL only when `.req/completeness` says
     `strict areas` (a new policy word, parsed beside `strict unrelated`).
   - With no `.req/areas` file there is no finding. A malformed `.req/areas` is handled exactly as a malformed
     `.req/completeness` is handled.
   - `req canon` also prints a claim's area on its line when it has one.
6. **`req init`** writes its starter claim with `claim_schema()` (it already does), so a new corpus starts at `/3`.
   The init README text that names `req.claim/2` names the current format.
7. **The importers.** `tools/adopt.py`, `tools/needs_import.py` and `tools/reqif_import.py` refuse a `req` whose
   `claim_schema` differs from the one they draft.
   - They keep drafting `req.claim/2`, because their drafts carry no area and a `/3` build loads `/2` as is.
   - They accept a `req` that reports `req.claim/2` **or** `req.claim/3`, and refuse any other value with today's
     message.
   - Tests that use `req.claim/3` as "another schema" use `req.claim/4`.
   - Mapping a source's hierarchy onto `area` is a later unit (ADR 0051 *Not decided here*).
8. **This repository's claims stay `/2`.** Nothing under `claims/` changes. That is the live proof of ADR decision 6.

## Tests first (envelope `red`)

Public seams only: the CLI over a scratch corpus, and `req::canon` / `req::model` public functions. Oracles come from
ADR 0051, never from a run. At minimum, one test per ADR *Falsification* item (1–6), plus:

- a `/2` file and the same claim declared `/3` load with **identical addresses**, and neither is reported as a
  migration candidate;
- a `/3` file with `area: diagnostics/startup` has the same address as without it, and changing the area changes no
  byte of `req baseline` emit for that claim (falsification 1);
- a `/2` file carrying `area` is a parse error naming `area` and `req.claim/3`;
- a `req.claim/1` file is still a migration candidate, and `req.claim/4` is still a newer format (falsification 4);
- each malformed path is a load error naming the claim (`Diag`, `a//b`, `/a`, `a/`, `a b`, `-a`, an empty string);
- `.req/areas`: a duplicate, an empty label and a malformed path are refused with `<file>:<line>`, every one in one
  run;
- `req check` names an undeclared area and still passes, fails under `strict areas`, and reports nothing with no
  `.req/areas`;
- `render --format json` rows and `trace --format json` nodes carry `area` (`null` when absent);
- an applying claim's gate verdict, applicability and derived parse are the same with and without an area
  (falsification 6);
- each importer accepts a `req` reporting `/2` or `/3` and refuses `/4`.

Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Feed `area` into the canonical form → the same-address test fails.
- Treat `/2` as a migration candidate again → the `/2`-loads test fails.
- Accept `area` in a `/2` file → the parse-error test fails.
- Drop the duplicate check in `.req/areas` → the registry test fails.
- Make `undeclared_area` FAIL without `strict areas` → the check test fails.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
`docs/spec.md`, `docs/trace.md` and the three Baselines unchanged and VERIFIED, with no re-sign. `req --version` reports
`claim_schema req.claim/3`. `tools/quickstart.sh` still passes under 300 s.

## Docs, one home each

- `docs/claim-format.md`:
  - the field reference gains `area`;
  - *Version axes* states that `/3` loads `/2` as is;
  - the migration section says `/2` needs none;
  - the `.req/areas` grammar.
- `req canon --help` and `req check --help` (the completeness paragraph names `undeclared_area` and `strict areas`);
  `req trace --help`'s list of node fields.
- `docs/adopting.md`: one short paragraph on filing claims by area.
- ADR 0051: a dated implementation note.

## Allowed paths

`src/canon.rs` (the format judgement and the `area` check only; nothing in the canonical form), `src/model.rs`,
`src/view.rs`, `src/completeness.rs`, `src/check.rs` (only if the finding needs it), `src/main.rs` (help pages),
`src/init.rs` (README text), `src/selftest.rs` (only if a printed format line needs it; its known answers do not
change), `tools/adopt.py`, `tools/needs_import.py`, `tools/reqif_import.py`, `tests/**`, `docs/claim-format.md`,
`docs/adopting.md`, `docs/adr/0051-a-claim-may-name-the-area-it-belongs-to.md` (the implementation note).

Forbidden: `RULES_V` and any change to the canonical form or an attested hash, `src/compare.rs`, `src/contradict.rs`,
`src/baseline.rs`, `tools/graph/**`, `bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`,
`docs/spec.md`, `docs/trace.md`, `scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`. If a forbidden file must
change, stop and report.
