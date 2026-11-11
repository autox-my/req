# S-2 — satisfaction links (ADR 0040, the code)

Task id: `S-2`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-11 ("Next: ADR 0040's satisfaction-link
code"), after S-1 measured the carry-over (PR #54).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md` (decisions 1–6, its *Falsification* list and its
2026-11-11 implementation note). This spec fixes the choices the ADR leaves to the code; where they disagree, the ADR wins
and you stop and report.

## Choices fixed here

1. **Design roots.** `.req/design`, optional, the grammar of the other `.req/` lists (`#` comments, blank lines skipped; a
   line is a repo-relative directory or file; absolute or `..` is a malformed line, exit 2, naming `<file>:<line>`). Files
   under a root are read recursively; a file that is not UTF-8 is UNVERIFIABLE, named, never skipped silently.
2. **Declaration.** Any line containing `req-satisfies:` followed by one claim id (the corpus's id grammar, `src/model.rs`)
   declares that the file (its repo-relative path, `/`-separated, is the element id) satisfies that claim. One id per
   occurrence; several occurrences per file are fine.
3. **The record.** `.req/satisfied` is YAML: `schema: req.satisfied/1` and `links:`, a list of
   `{element, claim_id, statement}` sorted by `(element, claim_id)`; `statement` is the claim's statement **verbatim** at
   recording time. No address, no rules version, no date is stored (the ADR: it survives a `RULES_V` bump and needs no git
   history). An unknown key is a load error naming it (the house rule for `.req/` files and claims).
4. **The command.** `req satisfy [--format text|json]` lists every declared link with its outcome; `req satisfy --record
   <element> <claim-id>` writes or replaces that one record (the human act — req never records on its own). `--record`
   refuses (exit 2) a link that is not declared, or a claim id the corpus does not hold. Exit for the listing: 0 every link
   holds (or there are none, said so), 3 any link is UNVERIFIABLE; 2 usage. Help page in `src/main.rs` with an `exit:` line.
5. **Judging** (ADR decision 3): re-derive the recorded statement and the claim's current statement under the current rules,
   then: same address → **holds**; `compare` direction in the **admitted** set → **holds (weakened)**, naming the direction;
   any other direction → **suspect** (UNVERIFIABLE, naming element, claim and direction); declared with no record →
   **unratified** (UNVERIFIABLE); a declared claim id the corpus does not hold → UNVERIFIABLE. A record with no declaration
   left is reported as **orphaned** (listed, not a finding).
6. **Admission is compiled in, and pinned to the measurement.** `ADMITTED_CARRIES: &[Direction]` in the new module holds
   exactly the classes `bench/results/direction-v2-rules-v<RULES_V>.json` reports `admitted: true` (today:
   `modality_weakening`). A test reads that record and asserts equality, so a rules bump or a re-measurement that changes
   admission fails CI until the constant is changed deliberately. Never read the record at runtime: adopters do not have it.
   Scope classes are never admitted, whatever the record says (ADR decision 6) — assert that too.
7. **Enforcement.** A new gate policy `design_satisfied` over the selection's applying claims: PASS when every link of an
   applying claim holds (or holds weakened), and its line names the counts (links judged, holding, weakened); UNVERIFIABLE
   when any is suspect, unratified or names an unknown claim; it never FAILs (a suspect link may still comply — nobody has
   shown it). No `.req/design` → PASS over zero links, said so. `req check` inherits it per release line. `.req/completeness`
   gains the line `need design`: with it, an applying claim no element satisfies is a structural omission `unsatisfied`
   (UNVERIFIABLE) in the completeness step and the audit's *Structural omissions* section.
8. **Docs, one home each.** The help pages; `docs/adopting.md` (a short "Satisfaction links" section in the loop); the
   AGENTS.md command table (one row for `satisfy`; the gate row's policy count); ADR 0040 gets a dated implementation note
   naming the module, the file format and the admitted set. `CONTEXT.md` already has *Satisfaction link* and *False carry*.

## Tests first (envelope `red`)

Public seams (the CLI exit codes, the gate JSON, the judging function), oracles written from the ADR. At minimum, one test per
ADR *Falsification* item (1–6), plus: an orphaned record is listed and is not a finding; a malformed `.req/design` line is
exit 2 naming the line; the admitted-set test of choice 6; `need design` names an unsatisfied applying claim and leaves an
excluded one alone. Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Make `holds (weakened)` accept any loosening (ignore `ADMITTED_CARRIES`) → a falsification test fails.
- Store the address instead of the statement → the `RULES_V` test fails.
- Let `design_satisfied` PASS on an unratified link → a test fails.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home), exit 0, no SKIP, tree clean after the run. If a new `check.sh`
step is added (none is expected), stub it in the fixtures that drive `check.sh`.

## Allowed paths

`src/satisfy.rs` (new), `src/lib.rs`, `src/main.rs` (dispatch + help), `src/gate.rs`, `src/check.rs`,
`src/completeness.rs`, `src/audit.rs`, `tests/**`, `docs/adopting.md`, `AGENTS.md` (the command table only),
`docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md` (the implementation note only), `examples/**` (only if a copy-ready
`.req/design` sample helps the adopter page).

Forbidden: `src/canon.rs`, `src/compare.rs`, `RULES_V`, `bench/**`, `claims/**`, `baselines/**`, `decisions/**`,
`aliases/**`, `.req/**` of this repository (no dogfood links in this unit), `Cargo.toml`, `Cargo.lock`.
