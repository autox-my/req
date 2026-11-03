# M1-3 — make `external-v4` regenerable on its own record, and count what the generator drops

Task id: `M1-3`. Code baseline: `c1bcd8f` (rules v6, PR #73 and #75 merged). The generator-only unit:
no `src/**` change, no claim, no address moves, and the signed Baseline stays `[VERIFIED]`.

## Why

Two *low* findings from the M1-1 review, both in `tools/corpus/build_external.py`:

1. **The corpus does not record everything it was built from.** `t_possessive_registered` reads the
   live `aliases/subjects.yml` (`registry_aliases()`), and `_pick(pair_id, len(aliases), "alias")`
   indexes into that list in file order. The emitted `rules:` header records the three source-commit
   pins and nothing about the registry, so regenerating after a registry edit changes pairs under the
   same ids with no record of why. "Regenerates byte-identically" holds only while the registry is
   unchanged.
2. **A mutation whose shape does not match returns `None` and the pair vanishes silently.** There is
   no per-class counter, so a regression that drops half of a class (or a registry edit that does)
   would shrink the FMR denominator without a trace. The same finding applies to the v1–v3 classes
   (`fn(text)` returning `None` or an unchanged string).

Neither can move a claim address. Fixing (1) changes the bytes of `bench/corpus/external-v4.yml`
(its header only), hence a new `corpus_sha256`, hence a re-run of the bench for that corpus.

## What to do

1. **Record the registry in the header.** For `--version 4` (and only 4: v1–v3 stay as they are,
   the v2 precedent), append to the `rules:` text the digest of the alias list the generator used:
   `sha256` over the alias surface forms **in file order, newline-joined (no trailing newline)**,
   written `registry-aliases sha256:<64 hex> (N aliases, aliases/subjects.yml)`. The order is part of
   the input because `_pick` indexes by position, so a reorder moves pairs and the digest must move
   with it. Hash only the alias list `registry_aliases()` already returns, not the whole YAML file:
   an unrelated edit to the registry must not look like a change to the corpus input.
2. **Count every class.** In `main()`, keep, per class code (`EM MS CP NG US MW PF PS AP AF PH PR`),
   `attempted` (statements offered) and `emitted` (pairs written); `dropped = attempted - emitted`.
   - Print a per-class table to **stderr** for every version (stderr never enters the corpus bytes).
   - For `--version 4`, also append `per-class emitted/attempted: AP 275/…, …` to the header, in the
     fixed order above, so the corpus carries its own denominators. Do not add counters to v1–v3
     headers.
3. **Pair set unchanged.** The ids, `a`, `b`, `source` and `note` of every pair in the regenerated
   `external-v4.yml` must be byte-identical to the current file; only the `rules:` scalar differs.
   Prove it by diffing the two files with the `rules:` line removed. If any pair moved, that is a
   defect in the change, not an expected result.
4. **`--self-check`.** Add `python3 tools/corpus/build_external.py --self-check`, table-driving the
   pure half without the network: the registry digest (order-sensitive: swapping two aliases changes
   it; a file with the same aliases and other YAML keys does not), the counter arithmetic, and the
   header sentence. Wire it into `scripts/check.sh` beside the other `--self-check` steps (a missing
   `python3` is a `skip`, never a pass). To keep `main()` testable without the network, factor the
   header/counter assembly into a pure function; do not change what the network half fetches.
5. **Regenerate and re-measure.** Regenerate `bench/corpus/external-v4.yml` with
   `python3 tools/corpus/build_external.py --version 4 > …` (network required; if it is unavailable,
   report `blocked`, never hand-edit a generated corpus). Then `cargo run -q -- bench
   bench/corpus/external-v4.yml`. `req bench` names its record `<stem>-rules-v<RULES_V>.json`, so this
   **overwrites** `bench/results/external-v4-rules-v6.json` — a re-issue of the same corpus name at
   the same rules version, not a new rules version. Treat it as such: in the commit message and PR
   description state the old and the new `corpus_sha256`, and report that every other field of the
   record (FMR, FSR, pair counts, structurally unverifiable ids) is identical to the previous record
   (`git diff` it). The old record stays in git history. Do not touch any other file under
   `bench/results/`.

   **This overwrite is a one-time, stated exception, not a precedent.** `review/rule.json` tells the
   CI reviewer that `bench/results/*.json` are append-only ("never rewrite or delete an existing
   record — a changed number arrives as a new file"), and the owner chose, knowingly, to break that
   letter here (decision of 2026-11-03): the header gains provenance, no measured number changes,
   and the filename `<stem>-rules-v<N>.json` cannot carry a corpus-bytes change at the same rules
   version. Consequences the unit must honour:
   - The commit message and the PR description each carry a section headed **"Append-only
     exception"** that names the file, the old and the new `corpus_sha256`, the reason, and that
     every other field is identical. A reader of the PR must not have to discover the overwrite.
   - Do **not** work around the rule: no renamed or duplicated record to dodge the reviewer, no edit
     to `review/rule.json` or `src/bench.rs`, and no weakening of the reviewer's wording. Both are
     forbidden paths.
   - Expect the OCR reviewer to flag the overwrite. If it reports it as critical/high, say so in the
     envelope `summary` and leave it: the owner merges as an admin over a stated, understood finding,
     the same remedy `AGENTS.md` names for a known review false positive.
   - The exception covers `external-v4-rules-v6.json` only. Any later change to a corpus's bytes
     at an unchanged rules version needs its own decision (or the `src/bench.rs` filename change,
     which is a separate unit).
6. **Docs.** `docs/canonical-hashing.md` line 299 (the `external-v4` bullet) is the only prose that
   describes the corpus; add one sentence saying the header records the registry digest and the
   per-class denominators. If you cannot find a sentence that needs to change, say so rather than
   editing.

## Red test

The `--self-check` cases from item 4, written first and shown failing against the baseline (the
functions do not exist yet, so the red is the missing digest/counter behaviour: assert the specific
digest value computed by hand from a fixed alias list, not one derived by the code under test).

## Mutation checks (envelope `mutations`)

- Make the digest ignore alias order (sort before hashing) → the order-sensitivity self-check fails.
- Hash the whole `aliases/subjects.yml` instead of the alias list → the "other YAML keys do not
  move it" self-check fails.
- Make `dropped` always 0 → the counter self-check fails.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP; and
`req baseline --verify baselines/air-gapped.json` stays `[VERIFIED]` (a corpus header moves no
address).

## Allowed paths

`tools/corpus/build_external.py`, `bench/corpus/external-v4.yml`,
`bench/results/external-v4-rules-v6.json` (overwritten, as item 5 explains), `scripts/check.sh`
(the one self-check step), `docs/canonical-hashing.md` (the one sentence of item 6).

Forbidden: `src/**`, `claims/**`, `aliases/**`, `baselines/**`, `bench/corpus/` other than
`external-v4.yml`, every other `bench/results/` file, `AGENTS.md`, `docs/adr/**`.
