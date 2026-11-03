# M2-2 — move the Tier-2 funnel out of the core; retire REQ-0011, re-scope REQ-0004

Task id: `M2-2`. Run **after M2-1 is merged** (both edit `AGENTS.md` and `scripts/check.sh`); the
code baseline is whatever `main` is then. Owner ruling of 2026-11-03 on F5 of the architecture
review: *retire REQ-0011 and re-scope REQ-0004*; keep a tiny `propose` seam in the core and move
the machinery to `experiments/`.

## Why

`src/funnel.rs` (1,035 lines), its two test files, `tools/tier2/*`, `tools/req-d1.sh`,
`tools/corpus/build_tier2.py`, ten Tier-2 corpora and captures and a 281-line protocol document are
research with an offline default (`NoProvider`) that always answers UNVERIFIABLE. They set the core
crate's surface and review load but ship nothing. Two dogfood claims were implemented and proven
only by that research:

- **REQ-0011** ("…report a tier 2 provider's … answer flip at a rate of at most 2 percent") is a
  statement about an experiment's instrument. It is **retired**.
- **REQ-0004** ("The req tool shall emit proposals for reviewer ratification.") states the
  human-disposes principle (ADR 0004). It stays, **re-scoped**: what the core guarantees is a
  seam that turns an external provider's answer into an *unratified proposal* and never into an
  edit; the corpus-wide funnel is no longer the evidence.

## What to do

1. **Decision record.** Add `docs/adr/0021-tier-2-leaves-the-core.md`, `**Status: accepted**` — the
   owner's ruling of 2026-11-03 — recording: the funnel and its instruments move to
   `experiments/tier2/`; REQ-0011 is retired and why; REQ-0004 is re-scoped and how; the research
   results stay in `bench/results/` (append-only history, untouched) and in the protocol. It states
   that ADR 0011 (Tier-2 budget) and ADR 0004 are unchanged in principle. Add a one-line *Amended*
   note to ADR 0011 pointing at 0021.
2. **The core seam (`req propose`, ≤ 80 lines in the core, own module).** It takes **one explicit
   pair** — `req propose <a.yml> <b.yml> [--provider-cmd <cmd>]` — with no blocking, no corpus walk,
   no gold set. With no provider it answers UNVERIFIABLE (exit 3), never "unrelated" (the offline
   rule). With `--provider-cmd` it speaks the existing line protocol to the command (`ID:`, `A:`,
   `B:`, `DIFF:`, `CHOOSE:`; the answer is a label or a JSON object with `choice`/`confidence`) and
   prints the answer as **`proposal (unratified): <label>`** with the provider command recorded
   verbatim. It never writes a file and never changes an address. Exit: 0 a proposal was produced,
   3 no provider / provider declined or failed, 2 usage. Its `--help` page states that contract.
   If the existing protocol code can be reused unchanged, keep it in the core and say so; if not,
   write the smallest version, and say which.
3. **Move to `experiments/tier2/`** (git renames, so history follows): `src/funnel.rs` (the rest of
   it: blocking, gold-set scoring, `--max-pairs`, `--min-confidence`, flip rate), `tests/funnel.rs`,
   `tests/funnel_flags.rs`, `tools/tier2/**`, `tools/req-d1.sh`, `tools/corpus/build_tier2.py`,
   `docs/tier2-sample-protocol.md`, `bench/corpus/tier2-*` (corpora and capture directories). It is a
   **standalone crate with its own `Cargo.lock`, depending on `req` by path**; it is **not** a
   member of the root workspace. **The root `Cargo.toml` and `Cargo.lock` must not change** (they are
   identity-path pins: a change there is a rules bump, which this unit is not); show
   `git diff --stat -- Cargo.toml Cargo.lock` empty in the PR. Add `experiments/tier2/README.md`: what it
   is, that it is research with honest negative results, how to run it, and that nothing in it
   gates a release. Fix every path the moved files carry (scripts, `evaluate.py`'s corpus
   resolution, docs) so `--self-check` of the Python instruments still passes from the new place.
4. **The gate keeps it compiling, not gating.** `scripts/check.sh` gains one step: `cargo check
   --manifest-path experiments/tier2/Cargo.toml --all-targets` and the crate's tests (full run
   only; `--fast` prints a `FAST` line for them). Neither feeds the JUnit report the release
   gate reads. Measure the added wall time and report it; if it exceeds 20 s, say so and move the
   tests (not the check) to a documented manual command, stating that choice in the PR.
5. **Retire REQ-0011.** Delete `claims/REQ-0011.yml`; no other claim relates *to* it (verify with
   grep and `req trace`). Update `tests/loader.rs`'s pinned id list in this commit, as its comment
   demands for a deliberate retirement. Do not renumber.
6. **Re-scope REQ-0004 without moving its address.** The statement stays; `rationale.assumptions`
   gains the scope sentence (the claim is satisfied by the single-pair seam; the funnel is research);
   its linked test becomes a **new core test** that fails if `req propose` ever writes a file or
   reports a provider answer as anything but an unratified proposal (through the CLI seam, like
   `tests/evidence.rs`'s tests). `tests/linked-test-ids.txt`: drop the two funnel ids, add the new
   one. `req canon --verify` must report no address moved for REQ-0004 (rationale and tests are
   outside identity).
7. **Corpus-size statements and counts.** The corpus goes from 22 to 21 claims. Update, from the
   tool's own output (never to make a test pass): `tests/parity_annotations.rs` and
   `tests/release_gate.rs` pinned counts, `docs/claim-format.md`'s corpus paragraph, `AGENTS.md`'s
   "(N applying, M undecided)" phrase, and regenerate `docs/spec.md` (`req render --out`, then
   `--check` exits 0). State each old → new.
8. **Docs.** `README.md` (the command table row, the Tier-2 status paragraph and its paths),
   `docs/design.md` and `docs/canonical-hashing.md` where they describe the funnel as part of the
   core: point at `experiments/tier2/` and ADR 0021; do not rewrite history sections. `AGENTS.md`:
   replace the Tier-2 funnel bullet and the `bench/results`/`tools/tier2` descriptions with a
   short "experiments/tier2" entry; keep `propose`'s one line.
9. **The signed Baseline is the owner's step.** REQ-0011 leaves the corpus and REQ-0004's test link
   changes (`tests_hash` is attested), so `baselines/air-gapped.json` must be re-emitted (`req
   baseline --at <now, RFC 3339 UTC> deployment_mode=air_gapped`) and committed **unsigned**; signing
   (`minisign -Sm`) is the owner's act and the `.minisig` is not yours. The Baseline verify step and
   the tests that verify the committed signature are the only expected red; report exactly that.

## Red tests

(a) the new core test of item 6 (`req propose` writes nothing, labels its output an unratified
proposal, UNVERIFIABLE with no provider), failing at the base because the single-pair form does
not exist; (b) the `tests/loader.rs` id list without REQ-0011, failing at the base.

## Mutation checks (envelope `mutations`)

- Make the seam write the proposal into a claim file in a scratch corpus → the item-6 test fails.
- Make it print the provider's label without the `proposal (unratified)` marker → the item-6 test
  fails.
- Re-add `claims/REQ-0011.yml` to a scratch copy → the loader id-list test fails.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run. Expected non-green: **only** the
Baseline verify and the two tests that verify the committed signature. Every `bench/results`
record must come out unchanged and the benches (parity, external-v1..v4, direction) pass. Markdown
is outside the OCR selection: say which docs were read by hand.

## Allowed paths

`src/funnel.rs` (moved), `src/lib.rs`, `src/main.rs` (the `propose` wiring and help), a new core
module for the seam, `tests/funnel*.rs` (moved), the new core test file (or `tests/evidence.rs`),
`tests/loader.rs`, `tests/parity_annotations.rs`, `tests/release_gate.rs`, `tests/linked-test-ids.txt`,
`experiments/tier2/**` (new), `tools/tier2/**`, `tools/req-d1.sh`, `tools/corpus/build_tier2.py`,
`docs/tier2-sample-protocol.md` (all moved), `bench/corpus/tier2-*` (moved), `claims/REQ-0004.yml`,
`claims/REQ-0011.yml` (deleted), `docs/adr/0021-tier-2-leaves-the-core.md`, `docs/adr/0011-…md`
(Amended note), `docs/spec.md`, `docs/claim-format.md`, `docs/design.md`, `docs/canonical-hashing.md`,
`README.md`, `AGENTS.md`, `scripts/check.sh`, `baselines/air-gapped.json`.

Forbidden: root `Cargo.toml` and `Cargo.lock`, `bench/results/**` (history; nothing is rewritten
or moved), `aliases/**`, `decisions/**`, `baselines/trust.yml`, `baselines/*.minisig`, `review/**`,
`.forgejo/**`, `tools/ocr/**`, every other `claims/**` file.
