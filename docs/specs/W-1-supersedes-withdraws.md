# W-1 — a superseded claim is withdrawn where its successor applies (ADR 0049)

Task id: `W-1`. Base: the commit that adds this spec and ADR 0049 (proposed; the PR that carries this work ratifies it).
Owner go-ahead of 2026-11-12 ("go", answering the proposal to decide `supersedes`).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0049-a-superseded-claim-is-withdrawn-where-its-successor-applies.md` (decisions 1–7 and its
*Falsification* list). Where this spec and the ADR disagree, the ADR wins; stop and report.

## Choices fixed here

1. **One evaluation, in the row loader.** Withdrawal is computed once, where a selection's rows are built
   (`view::load_rows`), as a second pass after every row's own `applies` is known. Every caller (gate, check, baseline,
   audit, render, trace, impact, select, diff's contradiction pass, completeness) gets it from there. Do not re-implement
   it in any consumer. If a command builds its rows some other way, route it through the same pass and say so in the
   envelope.
2. **The fold** (ADR decision 2), over each claim's **successors**, meaning the rows whose outgoing relations include
   `(supersedes, <this id>)`:
   - fold with each successor's **own** `applies` (computed before the pass), never a folded one;
   - any `Yes` → set the claim's `applies` to `No` and record the successor ids in `superseded_by`;
   - else any `Unknown`, and the claim's own answer is not `No` → `Unknown`, with `superseded_by` empty;
   - else unchanged.
   Use a `BTreeMap`/sorted vectors. No `HashMap` iteration decides an outcome or an order.
3. **Cycles** (ADR decision 5). Find every `supersedes` cycle over the whole corpus, self-loops included. Members of a cycle
   are excluded from the fold, as successors and as targets, and keep their own answer. `relations_resolve` gains a
   **failed** list naming each cycle once, rotated to start at its lowest id
   (`supersedes cycle: REQ-A -> REQ-B -> REQ-A`). It is FAIL in every selection, whether or not the members apply.
4. **Dangling** (ADR decision 6). The existing UNVERIFIABLE entry for an applying successor stays as it is. A successor
   that does not apply but names a missing `to` is also listed as UNVERIFIABLE under `relations_resolve`, for
   `supersedes` only (other relation kinds keep today's applying-only rule).
5. **Row shape.** `Row` gains `superseded_by: Vec<String>`. Serialise it in a view's JSON row as `superseded_by` (sorted,
   `[]` when none). Every `req.*/1` document that serialises rows gains the key additively. `tests/json_format.rs` will
   need it.
6. **Text** (ADR decision 7). Wherever a text view already names claims outside the selection (`render`'s
   outside-the-selection list, `select`'s per-claim line, `impact`'s state), a withdrawn claim reads
   `does not apply (superseded by REQ-X[, REQ-Y])` instead of its plain wording. The audit packet gets a *Superseded*
   section per Baseline listing each withdrawn claim and its successors, **only when non-empty**. With no `supersedes`
   relation anywhere, every text and JSON output other than the new `superseded_by: []` key is byte-identical. The
   committed `docs/spec.md` and `docs/trace.md` must pass `--check` unchanged. A text projection's header and body do not
   carry the JSON key, so they do not move.
7. **Baselines.** The applying set comes from the rows, so a withdrawn claim is not in a manifest's claims. No envelope
   change. This repository declares no `supersedes`, so the three committed manifests stay VERIFIED with no re-sign. If
   they do not, stop with status `blocked`.
8. **The #64 accuracy fix rides along** (owner, 2026-11-12). In `docs/adopting.md`, the Swift Testing paragraph says the
   JUnit `name` "keeps its trailing `()` and its XML escaping". `src/junit.rs` decodes the entities, so it should say the
   name keeps its trailing `()` and that req decodes XML entities, so a `tests:` entry holds the decoded name. One phrase,
   nothing else in that section.

## Tests first (envelope `red`)

Use public seams: the CLI over a scratch corpus in the style of the existing `tests/gate*.rs`/`tests/select*.rs`, and
`req::view::load_rows` if it is public. Oracles come from the ADR, never from a run. At minimum, one test per ADR
*Falsification* item (1–6), plus:

- the motivating case: REQ-B (`at most 2`) supersedes REQ-A (`at least 3`), both unconditional. The gate's contradictions
  policy PASSes and names neither, and without the relation the same corpus FAILs;
- a recorded `conflicts-with` between a claim and its applying successor no longer makes the policy UNVERIFIABLE;
- a successor scoped to `variant == x` withdraws its predecessor under `variant=x` only, and under `variant=y` the
  predecessor applies;
- the chain A ← B ← C with all three applying withdraws A and B, and C applies;
- a successor whose predicate the context does not decide makes an otherwise-applying predecessor undecided, so the gate
  is UNVERIFIABLE and names it;
- a self-loop and a two-claim cycle each FAIL `relations_resolve` with the cycle named once, and no member is withdrawn;
- a dangling `supersedes` on a non-applying successor is UNVERIFIABLE;
- `req baseline` emit on a corpus with a withdrawn claim omits it from the manifest's claims;
- `--format json` of `render` and `select` carry `superseded_by`;
- `req diff`'s contradiction pass lists the motivating pair as **resolved** when the successor's relation is added at head.

Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Fold with the successor's folded state instead of its own → the chain test fails.
- Withdraw regardless of the successor's applicability → the variant-scoped test fails.
- Treat an undecided successor as not applying → falsification 3 fails.
- Withdraw within a cycle → the cycle test fails.
- Compute withdrawal only in the gate, not in the loader → a view-agreement test (falsification 5) fails.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home): exit 0, no SKIP other than one stated, tree clean after the
run, the three committed Baselines VERIFIED, `docs/spec.md` and `docs/trace.md` current with no regeneration.

## Docs, one home each

- Help pages: `req select`, `req render` and `req gate` each get one sentence. `req gate`'s sentence goes on
  `relations_resolve`, naming the cycle FAIL.
- `docs/claim-format.md`: one short paragraph beside the relations table, on what `supersedes` does and its direction.
- `docs/adopting.md`: one paragraph beside the existing *Contradictions and Conflicts* paragraph, on replacing a
  requirement by supersession. Plus choice 8's phrase.
- `CONTEXT.md`: add **Withdrawn** ("a claim that does not apply in a selection because a successor that supersedes it
  applies there; kept in the corpus") if no existing term covers it.
- ADR 0049: a dated implementation note. ADR 0043: one "Extended by ADR 0049" line.

## Allowed paths

`src/view.rs`, `src/select.rs`, `src/gate.rs`, `src/audit.rs`, `src/impact.rs`, `src/format.rs`, `src/main.rs` (help
pages), `src/completeness.rs` (only if it builds rows outside `load_rows`), `tests/**`, `docs/claim-format.md`,
`docs/adopting.md`, `CONTEXT.md`, `docs/adr/0049-a-superseded-claim-is-withdrawn-where-its-successor-applies.md` (the
implementation note), `docs/adr/0043-…` (one line).

Forbidden: `src/canon.rs`, `src/compare.rs`, `src/contradict.rs`, `src/satisfy.rs`, `src/baseline.rs`, `src/diff.rs`,
`RULES_V`, `bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`, `docs/spec.md`,
`docs/trace.md`, `Cargo.toml`, `Cargo.lock`, `.github/**`. If a forbidden file must change, stop and report.
