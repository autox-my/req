# R-3 — "in" before any comparator belongs to the comparator (rules v10)

> **Dropped 2026-11-12 by the owner. Rules stay at v9.** See *Why it was dropped* at the end. Nothing in this spec
> is built.

Task id: `R-3`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-12 (after PR #60 merged: "widen the
parser" first).

**Model:** writer `claude-sonnet-5-5` (the Tier-1 path); reviewer `deepseek-v4.1-flash`, read-only.

## The defect (measured on `main` at rules v9, 2026-11-12)

T1.5's closed table (`COMPARATOR_PHRASES`, `src/canon.rs`) absorbs a leading "in" for exactly two phrases, `in less than`
and `in fewer than`. Before every other comparator the "in" stays in the **action**:

| statement | action | comparator |
| --- | --- | --- |
| "The API shall respond within 100 ms." | `respond` | `LE 0.1 s` |
| "The API shall respond in less than 200 ms." | `respond` | `LT 0.2 s` |
| "The API shall respond in at least 200 ms." | `respond in` | `GE 0.2 s` |
| "The API shall respond in no more than 100 ms." | `respond in` | `LE 0.1 s` |

So "respond within 100 ms" and "respond in no more than 100 ms" (the same obligation) get **different addresses** (a false
split), and ADR 0043's own opening example — "within 100 ms" against "in at least 200 ms" — is **not comparable**, so the
contradiction detector cannot see it (recorded in ADR 0043's implementation note, PR #60). `req compare` between them
reports a divergence of the action instead of the bound.

## What to do

1. **Tests first** (envelope `red`), public seams (`canonicalize_statement`, `req::compare::compare`,
   `req::contradict::contradicts`), oracles from T1.5 and English, never from a run:
   - "respond in at least 200 ms", "respond in no more than 100 ms", "respond in at most 100 ms", "respond in exactly
     100 ms", "respond in more than 100 ms" all derive action `respond` with the comparator, value and unit unchanged;
     "respond in less than 200 ms" and "respond in fewer than 3 steps" still derive `LT` exactly as at v9;
   - "respond within 100 ms" and "respond in no more than 100 ms" have **equal addresses** (E); "respond within 100 ms" and
     "respond in less than 100 ms" still **differ** (N — LE vs LT, the classic trap N-002 must still hold);
   - `contradicts`: "The API shall respond within 100 ms." against "The API shall respond in at least 200 ms." is a
     contradiction (slot `bounds`) — ADR 0043's example;
   - `compare`: "respond in no more than 100 ms" → "respond in no more than 200 ms" is `numeric_loosening` (unchanged) and
     "respond within 100 ms" → "respond in at least 100 ms" is a bound change, not `scope_divergence` on the action;
   - add two hand-written parity pairs in the existing style: an `E-` ("within" vs "in no more than") and an `N-`
     ("within" vs "in at least").
2. **The rule.** One general rule replaces the two special entries: when the word immediately before a matched
   comparator phrase is `in`, that word is part of the comparator's span (removed from the action, as `in less than`
   always was). Delete `in less than` / `in fewer than` from the table; their derivations must not change (a test proves
   it). Only `in`: no other preposition is absorbed (`for at least 30 days` keeps `for` in the action — say so in a test,
   so a later widening is a deliberate rules change). Update T1.5 in `docs/canonical-hashing.md` (the closed table) in the
   same commit, and its two "within vs in fewer than" examples if their wording names the old entries.
3. **Rules-change protocol** (AGENTS.md, `docs/canonical-hashing.md` §5): `RULES_V` 9 → 10; regenerate `req selftest`'s
   goldens deliberately (add one known answer for "in at least"); record the re-canonicalization candidates — claims in
   this repository whose statement has "in" before a comparator (expected: `REQ-0022`, "in exactly one claim file"; say
   whether its slots read better or worse and do not edit it); new `bench/results/*-rules-v10.json` records for every
   corpus the gate benches (the v9 records stay as history); the *Recorded runs* section gains the v10 numbers, naming
   every external-corpus pair whose verdict moved and every direction pair whose class moved. **FMR must stay 0 on every
   corpus; if any pair becomes a false merge, stop with status `blocked`** — do not narrow the rule to fit the corpus
   without saying so.
4. **What hangs off `RULES_V`.** `ADMITTED_CARRIES` (`src/satisfy.rs`) and `DETECTOR_ADMITTED` (`src/contradict.rs`) are
   pinned by tests to the `-rules-v<RULES_V>` records: the new v10 records must keep both tests green without changing
   either constant; if the measurement changes admission, stop and report (the constant is changed only by an owner
   decision). Report the contradiction block's v10 numbers (comparable, recall) beside v9.
5. **The generator.** `tools/corpus/build_direction.py` labels from the mutation; if its reading of a bound relies on the
   action text, check v1–v3 regenerate **byte-identically** (expected: they do — say so). No corpus file changes otherwise.
6. **Re-sign is the owner's.** The three Baseline manifests record `rules_v`: re-emit them (`req baseline --at <now> --out
   baselines/<file> <selection>`, selections as in `.req/variants` and `deployment_mode=air_gapped`), regenerate
   `docs/spec.md` and `docs/trace.md`, sync every other doc that states `rules_v` (R-1 found `docs/safety-manual.md`),
   commit the re-emitted manifests in the feature commit (the R-1 precedent, f59d3ab), and **stop with status `blocked`
   and blocker "owner must sign three manifests"** once everything else is green. Do not push until the orchestrator
   says the signatures are in.
7. **Docs.** ADR 0043's implementation note gains one dated line: the example pair is comparable from rules v10. Nothing
   else in `docs/adr/`.

## Mutation checks (envelope `mutations`)

- Remove the general "in" rule → the E- parity pair and the ADR 0043 contradiction test fail.
- Absorb any preposition, not only `in` → the `for at least` test fails.
- Restore `in less than` as a separate table entry and drop the general rule → the `in at least` derivation test fails.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home) green except the three Baseline signature checks, which must
fail **only** with "signature does not verify" until the owner signs; `req selftest` passes; the tree is clean after a run.

## Allowed paths

`src/canon.rs` (the comparator matching and the table only), `src/selftest.rs` (goldens), `tests/**`,
`bench/corpus/parity.yml` (the two pairs), `bench/results/*-rules-v10.json` (new), `docs/canonical-hashing.md`,
`docs/spec.md`, `docs/trace.md` (regenerated only), `docs/safety-manual.md` (`rules_v` only), `baselines/air-gapped.json`, `baselines/release-parity.json`,
`baselines/release-parity-and-adversarial.json` (re-emitted only),
`docs/adr/0043-contradictions-are-found-structurally-and-recorded-as-conflicts.md` (one line).

Forbidden: every `bench/results/*-rules-v9.json` and older, `bench/corpus/direction-*.yml`, `bench/corpus/external-*.yml`,
`baselines/*.minisig`, `baselines/trust.yml`, `claims/**`, `decisions/**`, `aliases/**`, `src/compare.rs`,
`src/contradict.rs`, `src/satisfy.rs`, `tools/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`.

## Why it was dropped (2026-11-12)

The writer built the rule (local commit `4d0a433`, never pushed). It held FMR = 0 on all eight corpora, and no external
or direction pair moved. But absorbing "in" before **any** comparator read "The user shall log in at least 3 times." as
action `log`. That shares an address with "The user shall log at least 3 times.", which is a different obligation.

The orchestrator then narrowed the rule to bounds whose unit is a duration (`62b94bc`, never pushed). Review R-3-review-1
(deepseek-v4.1-flash) confirmed that the shape survives exactly where these verbs occur: "The user shall log in within
2 s." equals "The system shall log within 2 s.", and "check in within 5 min" equals "check within 5 min". At rules v9 each
pair differs, so v10 would **create** these false merges. No corpus pair trips them, which is why FMR stayed 0.

No narrower rule removes the shape:

- "In at most 2 s" after a particle verb is ambiguous in English.
- A closed list of particle verbs (log, sign, check, …) silently misses every verb it does not list.

A false merge is always a failure (AGENTS.md), while the defect this spec fixes is a false split, which is noise. The
owner chose to keep rules v9.

**What stays as it is:**

- "within 100 ms" and "in no more than 100 ms" keep different addresses (a false split).
- ADR 0043's opening example ("within 100 ms" against "in at least 200 ms") stays **not comparable**, and the
  contradiction policy's PASS line counts it among the pairs it could not compare.
- An author who wants such a pair compared writes both claims with the same preposition ("within" / "in at least", or
  "in no more than" / "in at least"), as ADR 0043's implementation note says.

**Do not retry this without new evidence.** A future widening must first add the trap pairs "log in within 2 s" vs
"log within 2 s" and "log in at least 3 times" vs "log at least 3 times" to the parity corpus, labelled N, and must keep
them apart.
