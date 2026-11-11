# S-1 — the false-carry measurement (ADR 0040 decision 5)

Task id: `S-1`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-11 ("go"), on the orchestrator's proposal
of a minimum of **100** opposing pairs per class.

**Model:** writer `claude-sonnet-5-5` (claudeAgent); reviewer `deepseek-v4.1-flash` (opencode-go), read-only.

## Why

ADR 0040 decision 5: a weakening class (`modality_weakening`, `numeric_loosening`, `membership_widening`) may carry a
satisfaction link only if, on the direction gold set, **no pair labelled a strengthening, a tightening, a polarity flip or a
scope change is classified into that class** — a *false carry*, held to zero like a false merge. Today's count is zero, but
vacuously: `bench/corpus/direction-v1.yml` holds **5** `modality_strengthening`, **9** `numeric_tightening`, **0**
`membership_*` pairs (measured on `main` at rules v8). Zero out of five is not evidence. This unit makes the zero mean
something, and reports it; it does **not** build satisfaction links (ADR 0040's code is a later unit).

## What to do

1. **A new corpus, `bench/corpus/direction-v2.yml`, from the same generator** (`tools/corpus/build_direction.py`, a
   `--version 2` switch; with no switch, or `--version 1`, the output must stay **byte-identical** to the committed
   `direction-v1.yml` — prove it by regenerating v1 and diffing). v2 = every v1 pair, plus:
   - **mirrors**: for every v1 pair labelled `modality_weakening`, `numeric_loosening` or `numeric_tightening` (and the `LT`
     family), the pair with `a` and `b` swapped, labelled with the **opposite** class (`modality_strengthening`,
     `numeric_tightening`, `numeric_loosening`). The label is still the mutation, read backwards — never the classifier's
     output. Ids get a distinct suffix (e.g. `-MWr`), and the `note` says it is a mirror;
   - **membership**: mutations on source statements that carry an enumeration the rules-v8 parser reads as `IN` (ADR 0030:
     "one of {A, B, C}" shapes): one member added (`membership_widening`) and one removed (`membership_narrowing`), plus
     their mirrors. If the pinned sources yield few or none, **say so** in the header line and the PR — do not synthesise
     statements to reach a number;
   - the `rules:` header names v2, its families and the counts, as v1's does.
   Sources and pins are unchanged (the generator fetches them at the pinned commits; network for the generator is fine — it
   is a dev tool, not the core).
2. **`req bench` reports false carries.** In `src/bench.rs`, for every scored pair (not unclassifiable): if the label is a
   non-weakening class (`modality_strengthening`, `numeric_tightening`, `membership_narrowing`, `polarity_flip`,
   `scope_narrowing`, `scope_widening`, `scope_divergence`) and the reported class is one of the three weakening classes,
   count it as a false carry **under the reported class**, named (pair id, expected, reported). Per weakening class also
   report its **support**: the number of scored pairs labelled its opposite (`modality_strengthening` for
   `modality_weakening`, `numeric_tightening` for `numeric_loosening`, `membership_narrowing` for `membership_widening`),
   and **`admitted`**: `true` exactly when its false-carry count is 0 **and** its support is ≥ 100. Emit it in the bench
   JSON under the direction block as `false_carry: {<class>: {count, support, admitted, pairs: [...]}}` for all three
   classes, always present (a class with no support is `admitted: false`, never omitted). It is **reported, not a gate
   verdict**: the existing direction gate already fails a mirrored strengthening read as its weakening (a dangerous
   confusion), and admission is ADR 0040's input, not a bench failure.
3. **Record and wire.** Run `req bench bench/corpus/direction-v2.yml` and commit its record
   `bench/results/direction-v2-rules-v8.json` (append-only: a new file; the existing records gain only the new block — see *Allowed paths*). Add the v2 bench to the full
   `scripts/check.sh` run beside v1 (skipped by `--fast`, as v1 is). A new `check.sh` step must be stubbed in the fixtures
   that drive `check.sh` (`tests/baseline.rs`, `tests/projection.rs`; find any other the same way) or CI fails where local
   does not.
4. **Docs, each in its one home.** `docs/canonical-hashing.md` §6.1 item 3 names v2 and what it adds, and its *Recorded
   runs* gain the v2 numbers; ADR 0040 gets a dated *Implementation note* (not an edit of its decisions) recording: the
   measurement set is v2, the support minimum is 100 (owner go-ahead 2026-11-11), and the measured `false_carry` block —
   which classes are admitted today and which are not, with the reason. `req bench --help` (in `src/main.rs`) gains one
   sentence on `false_carry`.

## Tests first (envelope `red`)

Public seam: `req::bench::evaluate` on small in-memory corpora, independent oracles (labels written in the test).
- a `modality_strengthening`-labelled pair the classifier reads as `modality_weakening` → one false carry under
  `modality_weakening`, named;
- a correctly classified mirror → no false carry, support counted;
- support 99 with zero carries → `admitted: false`; 100 → `true`; one carry with support 500 → `false`;
- a class with no pairs at all → present, `support: 0`, `admitted: false`;
- an unclassifiable pair is neither a carry nor support.
Prove them red at the base (compile failure counts only if the field is genuinely absent), then green.

## Mutation checks (envelope `mutations`)

- Drop the support condition from `admitted` → a test fails.
- Count carries under the *expected* class instead of the reported one → a test fails.
- Remove the mirror family from the generator → the v2 support for `modality_weakening` falls below 100 (show the number).

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP; v1 regenerates byte-identical; the
v1 bench numbers (1016 checked, 1012 correct, 0 dangerous at rules v8) are unchanged.

## Allowed paths

`tools/corpus/build_direction.py`, `bench/corpus/direction-v2.yml` (new), `bench/results/direction-v2-rules-v8.json`
(new), the six existing `bench/results/*-rules-v8.json` records **additively** (amended 2026-11-11 after review
S-1-review-1: `req bench` rewrites a record in place, so the always-present `false_carry` block must be committed once;
every existing number unchanged, +20/-0 each), `src/bench.rs`, `src/main.rs` (the bench help sentence only), `tests/**` (new tests; check.sh fixture stubs),
`scripts/check.sh` (the v2 bench step only), `docs/canonical-hashing.md`, `docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md`
(the implementation note only).

Forbidden: `bench/corpus/direction-v1.yml` and every other existing corpus, any change to an existing number in a result record, `src/canon.rs`,
`src/compare.rs` (the classifier is what is measured — if it is wrong, report it, do not fix it here), `RULES_V`,
`claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`.
