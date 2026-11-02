# M1-1 — no statement word leaves identity silently

Task id: `M1-1-fix`. One writer: everything here touches the Tier-1 path, so it is not split.
Code baseline: `b56bca3` (`RULES_V 5`).

## The defect (reproduced at the baseline, release build)

Two false merges in this repository's own corpus. Each pair below gets the **same address**,
`req canon --verify` exits 0 and `req compare` answers `identical`:

| claim | edit | address |
|---|---|---|
| REQ-0020 | "…exactly once, on the **outgoing** side of its origin claim" → "**incoming**" | `d04ccf3bb7cf` both |
| REQ-0004 | "The req tool's **semantic** layer shall emit proposals…" → "**audit** layer" | `ddc6992dd05b` both |

Mechanisms, read from the stored forms:

1. **Dropped tail.** In REQ-0020 "exactly" is taken as the `EQ` comparator, no value follows,
   and every word after it is in no slot: the form is `"action":"serialize each relation",
   "operator":"EQ","value":null,"qualifier":""`. The qualifier rule (hashing spec T1.5) only
   keeps the phrase after a *value*.
2. **Prefix subject match.** "The req tool's semantic layer" resolves to `REQ_TOOL` and
   "semantic layer" is in no slot. That violates T1.7 as written ("unregistered synonyms are
   not collapsed"): `the req tool's semantic layer` is not a registered alias.

Neither was caught: the benchmark's mutations (`tools/corpus/build_external.py`: shall→may,
shall→shall not, first-verb swap) never touch a trailing clause or a subject's head, and ADR
0014's annotation rule reports REQ-0020's authored `value: outgoing_side_once` as `ann` instead
of a contradiction.

## What to build

1. **The coverage invariant (new Tier-1 rule; give it the next T1 number in
   `docs/canonical-hashing.md`).** After a structured parse, every word position of the
   normalised statement must be attributed to a slot: subject (the whole matched alias span),
   modality phrase, comparator phrase, value (digits or a number word), unit, condition text,
   action, or qualifier — or to the stopword set. Any unattributed position makes the claim
   **unstructured** (its `raw` is the normalised statement, which is already how REQ-0007 is
   addressed) with a lint naming the uncovered words, e.g. `uncovered_content: once, on,
   outgoing, side, of, its, origin, claim`.
   - **Positional, not token-set.** Matching words against the form's text would pass a dropped
     word that also appears elsewhere in the statement. A test must pin that case.
   - **Stopwords: `a`, `an`, `the`, and nothing else** unless a recorded mutation case shows a
     substitution cannot change meaning. Every stopword is a word that can be swapped without
     moving the address, so the list is a false-merge surface. Prepositions are not stopwords
     ("before"/"after", "on"/"off").
   - The invariant proves that no word is dropped. It does not prove word order is kept; slots
     that hold text keep their order already. Say so in the spec text.
2. **Whole-span subject (T1.7 as written).** A registry alias matches only when it is the
   entire subject span. "the req tool's semantic layer" is unregistered, so it stays a literal
   subject (or, if the parser cannot carry a literal possessive subject, the claim becomes
   unstructured by rule 1). **Do not add aliases to `aliases/subjects.yml`**: an alias is a
   collapse decision for the owner.
3. **Regression tests (red first, at `b56bca3`).** At minimum: the REQ-0020 pair and the REQ-0004
   pair get different addresses (an independent oracle: the two statements differ in meaning by
   construction); the duplicate-word case from rule 1; and `req compare` no longer answers
   `identical` for either pair. Put the positive cases (`exactly zero`, `at most 2 percent`,
   `the benchmark harness`, `never`) in the same suite as **must stay structured**, so the
   invariant cannot pass by making everything unstructured.
4. **Seed traps.** Add the two shapes to `bench/corpus/parity.yml` as hand-written
   not-equivalent pairs, labelled from the spec, never from the canonicalizer's output.
5. **Mutation classes in the generator.** Extend `tools/corpus/build_external.py` with two
   not-equivalent mutations, labelled by construction: **any-position substitution** (replace
   one content word with a token absent from the statement, at a position chosen
   deterministically from `sha256(pair id)`, plus one in the final third of the statement) and
   **possessive head** (`<subject> shall` → `<subject>'s <component> shall`). Emit
   `bench/corpus/external-v4.yml` (v1–v3 stay as they are, the v2 precedent) and add it to
   `scripts/check.sh` beside v1–v3. Keep the size of the same order as v3.
6. **Rules bump, per AGENTS.md's protocol.** `RULES_V` 5 → 6. Then:
   - `req canon --fill` over `claims/`. A claim that becomes unstructured will carry authored
     slots that now contradict its parse, and `--fill` will refuse it. Resolve that by making
     the authored slots declare `unstructured: true` (REQ-0007's shape). **Do not re-word a
     statement**: re-wording a requirement is the owner's call. Propose re-wordings that would
     let a claim stay structured in the PR description instead.
   - Regenerate golden addresses deliberately; a golden that moves is listed in the commit
     message with the claim it belongs to.
   - New `bench/results/*-rules-v6.json` for every corpus. Never rewrite a v5 record.
   - `req render --out docs/spec.md`, then check that `req render --check docs/spec.md` exits 0.
   - Update every doc line that states `rules_v 5` (`README.md` status line only, not its bet
     section, which M1-2 owns; `docs/adopting.md`, `docs/agentic.md`, the `tests/adopt.rs`
     fixture if it asserts the build's value).
   - Record the re-canonicalisation candidates (claims whose address moved) per hashing spec §5.
7. **The signed Baseline is the owner's step.** `baselines/air-gapped.json` records `rules_v 5`, so
   after the bump `req baseline --verify` is UNVERIFIABLE and the gate's Baseline step blocks (it
   has no SKIP). Emit the replacement manifest with `req baseline --at <now, RFC 3339 UTC>
   deployment_mode=air_gapped` into `baselines/air-gapped.json`, commit it, and stop: signing is a
   human act (`minisign -Sm baselines/air-gapped.json`). Report the gate honestly: exit and the
   one blocking step, nothing folded into a pass.

## Mutation checks (envelope `mutations`)

- Delete the coverage invariant → the REQ-0020 test, the duplicate-word test and the
  any-position class of `external-v4` fail.
- Restore prefix subject matching → the REQ-0004 test and the possessive class fail.
- Add a preposition (`on`) to the stopword set → a named test fails.

## Report in the envelope summary

Which corpus produced which number, never pooled: FMR and FSR per corpus at rules v6, the
count of dogfood claims that became unstructured **with their ids**, and the address moves.

## Allowed paths

`src/**`, `tests/**`, `claims/REQ-*.yml`, `bench/corpus/parity.yml`, `bench/corpus/external-v4.yml`
(new), `tools/corpus/build_external.py`, `bench/results/*-rules-v6.json` (new files only),
`docs/spec.md`, `docs/canonical-hashing.md`, `docs/claim-format.md`, `docs/adopting.md`,
`docs/agentic.md`, `README.md` (status line only), `scripts/check.sh` (the external-v4 line),
`baselines/air-gapped.json`, `tests/linked-test-ids.txt`, `CONTEXT.md` (only for a new term).

Forbidden: `aliases/**`, `decisions/**`, `baselines/trust.yml`, `baselines/*.minisig`,
`review/**`, `.forgejo/**`, `bench/results/*-rules-v[1-5].json`, `docs/adr/**`.
