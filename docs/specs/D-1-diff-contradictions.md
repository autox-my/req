# D-1 — `req diff` names the contradictions a change introduces (ADR 0045)

Task id: `D-1`. Base: the commit that adds this spec and ADR 0045 (proposed; the PR that carries this work ratifies it).
Owner go-ahead of 2026-11-12 (the second item after PR #60).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0045-req-diff-names-the-contradictions-a-change-introduces.md` (decisions 1–6 and its
*Falsification* list). It builds on ADR 0043's detector (`src/contradict.rs`: `find`, `Findings`, `DETECTOR_ADMITTED`).
Where this spec and the ADR disagree, the ADR wins; stop and report.

## Choices fixed here

1. **Reuse, do not re-derive.**
   - Contradictions come from `contradict::find`, called on each side's rows for each variants line. Build the rows with
     the same seam `req check` / `req gate` use (`view::load_rows` or its equivalent over a loaded root).
   - Forms come from that side's claims, loaded from the temporary directory `req diff` already extracts.
   - Parse `.req/variants` with `check.rs`'s parser, made `pub(crate)` if needed. Do not write a second parser. A
     malformed head list is UNVERIFIABLE (exit 3), naming `<file>:<line>`.
2. **Reading the head list.**
   - With no `--head`, read `.req/variants` from the working tree beside the corpus.
   - With `--head <rev>`, read it from git the same way the claims are read: a raw blob, absent if the revision has none.
3. **Pair identity.** `(variants line text, lower id, higher id)`. Added means present at head and unrecorded, and absent
   at base. Resolved means present at base and absent at head, or recorded at head (ADR decision 2). Sort by line, then
   the ids.
4. **Output.**
   - The text has a `contradictions:` block after the classes. It has one line per added or resolved pair, naming the
     line, both ids, the slot and the two values (`Contradiction::describe`), and appends `(reported, not judged)` while
     not admitted.
   - With nothing added or resolved, the block reads `contradictions: none added or resolved across <n> selection(s)`.
   - The flag `contradiction-added <other-id>` goes on each claim's entry. A claim that is otherwise `unchanged` is then
     listed, not just counted, so the flag is visible.
   - JSON: `contradictions: {added: [{line, a, b, slot, a_value, b_value}], resolved: [...]}`, and the flag in `flags`.
     `tests/json_format.rs` will need the new key.
5. **`--fail-on contradiction-added`**: exit 1 when any flag was raised (never while not admitted). Add it to the
   `--fail-on` spellings and the help page's list.

## Tests first (envelope `red`)

Use the CLI over a scratch git repository, in the style `tests/diff.rs` already uses, with oracles from the ADR. At
minimum, one test per ADR *Falsification* item (1–4), plus:

- adding a claim that contradicts an existing one lists the pair as added, flags both claims and exits 1 with
  `--fail-on contradiction-added`;
- an applicability edit that brings two contradicting claims into the same line is added, and the edited claim's class
  stays whatever its own change is;
- editing one side so the pair no longer contradicts lists it as resolved, with no flag;
- recording `conflicts-with` at head resolves it;
- two `.req/variants` lines where the pair contradicts under only one name that line;
- a malformed head `.req/variants` is exit 3 naming the line;
- `--head <rev>` reads that revision's list, not the working tree's.

Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Report every head contradiction as added (ignore base) → falsification 1 fails.
- Read `.req/variants` from each side separately → the "list from head" test fails.
- Raise the flag regardless of `DETECTOR_ADMITTED` → a test fails. Drive it through a seam that takes the admission as
  a parameter, since the constant is `true` today.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home), exit 0, no SKIP, tree clean after the run, the three committed
Baselines VERIFIED. `req diff --base origin/main` on this repository lists no added contradiction.

## Docs, one home each

- The `req diff` help page: the section, the flag, the `--fail-on` spelling and one sentence on selections.
- `docs/adopting.md`: one sentence beside the existing `req diff` class table.
- ADR 0045: a dated implementation note.
- ADR 0026 and ADR 0043: one "Extended by ADR 0045" line each.
- `CONTEXT.md` needs no new term.

## Allowed paths

`src/diff.rs`, `src/check.rs` (visibility of the variants parser only), `src/contradict.rs` (a parameterised seam only;
no change to the judging), `src/main.rs` (the help page), `tests/**`, `docs/adopting.md`,
`docs/adr/0045-req-diff-names-the-contradictions-a-change-introduces.md` (the implementation note),
`docs/adr/0026-…`, `docs/adr/0043-…` (one line each), `examples/**` (only if the CI snippet's `--fail-on` list is pinned by
a test and must name the new flag).

Forbidden: `src/canon.rs`, `src/compare.rs`, `RULES_V`, `bench/**`, `claims/**`, `baselines/**`, `decisions/**`,
`aliases/**`, `.req/**` of this repository, `Cargo.toml`, `Cargo.lock`, `.github/**`.
