# 0021. Tier-2 leaves the core; REQ-0011 is retired and REQ-0004 re-scoped

**Status: accepted** — the owner's ruling of 2026-11-03 on finding F5 of the architecture review of
2026-11-02. Date accepted: 2026-11-03. [ADR 0004](0004-model-proposes-human-disposes.md) (the model
proposes, a human disposes) and [ADR 0011](0011-tier-2-budget.md) (Tier-2's budget) are unchanged in
principle; this ADR moves where the machinery lives and what the dogfood corpus claims about it.

## Context

`src/funnel.rs` (1,035 lines), its two test files, the evaluation instruments in `tools/tier2/`, the
provider wrapper `tools/req-d1.sh`, the corpus builder `tools/corpus/build_tier2.py`, the Tier-2
corpora and their capture directory, and a 281-line sample protocol are research. Their default provider
(`NoProvider`) answers UNVERIFIABLE on every pair, because no local model exists in this environment
and REQ-0012 forbids requiring a network call. They set the core crate's surface and review load and
ship nothing a release depends on.

Two dogfood claims were implemented and proven only by that research:

- **REQ-0011** — the benchmark harness shall report a Tier-2 provider's repeated-identical-input
  answer flip at a rate of at most 2 percent. It is a statement about an experiment's *instrument*
  (a flip rate over a provider that is not shipped), not an obligation on the product.
- **REQ-0004** — the req tool shall emit proposals for reviewer ratification. It states the
  human-disposes principle (ADR 0004), which is a product obligation, but its only evidence was the
  corpus-wide funnel.

## Decision

1. **The funnel moves to `experiments/tier2/`.** Blocking, the gold-set score, `--max-pairs`,
   `--min-confidence` and the flip rate, with their tests, instruments, corpora, captures and the
   protocol, become a standalone crate (`req-tier2`) with its own `Cargo.lock`, depending on `req` by
   path. It is **not** a member of the root workspace: the root `Cargo.toml` and `Cargo.lock` are
   identity-path pins and do not change. The gate keeps the crate compiling and its tests green
   (`scripts/check.sh`), but nothing in it gates a release and none of it feeds the JUnit report the
   release gate reads.
2. **The core keeps one seam: `req propose <a.yml> <b.yml> [--provider-cmd <cmd>]`.** One explicit
   pair; no blocking, no corpus walk, no gold set. It speaks the funnel's line protocol to the
   provider command and prints the answer as `proposal (unratified): <label>`, naming the provider
   command verbatim. It never writes a file and never changes an address. With no provider the answer
   is UNVERIFIABLE (exit 3) — never "unrelated", the offline rule stands. The core's version of the
   protocol is the smallest one (a plain symmetric difference of lowercase words for `DIFF:`; the
   label is a bare word or a JSON `choice` with an optional `confidence`); the research crate keeps
   its fuller tokenizer, stopwords and failure diagnostics.
3. **REQ-0011 is retired.** The claim file is deleted and the id is not reused or renumbered. No
   other claim related to it (checked with `grep` and `req trace`). Consequences: the dogfood corpus
   goes from 22 to 21 claims; it no longer carries a `runs >= N` predicate, so the `runs=25` entry of
   `.req/variants` decided nothing and is removed, and the `Ge`-exemption rows of
   `tests/release_gate.rs` run on a fixture corpus instead. The flip-rate threshold itself survives
   as the research crate's own reporting rule (`docs/canonical-hashing.md` §6.3 still names it).
4. **REQ-0004 is re-scoped, not reworded.** Its statement, subject, address and `claim_id` are
   unchanged — `rationale` and `tests` sit outside identity, so no address moves. What changes: the
   rationale's assumptions record that the claim is satisfied by the single-pair seam, and its linked
   test is a core test that fails if `req propose` ever writes a file or reports a provider answer as
   anything but an unratified proposal.
5. **Research results stay where they are.** `bench/results/` is append-only history and is not
   touched; records that cite the old corpus paths remain true to the tree they described. The
   protocol, with its honest negative results, moves with the experiment.

## Consequences

- The committed Baseline attests each claim's `tests_hash` (envelope v4), and REQ-0011 leaves the
  corpus while REQ-0004's links change. `baselines/air-gapped.json` is re-emitted unsigned; signing is
  the owner's act, and the Baseline verification (and the tests that verify the committed signature)
  stay red until the owner signs.
- The Tier-2 research is reproducible from `experiments/tier2/README.md`; it is no longer exercised by
  `req bench` or any release gate, which it never was in substance.
- Nothing here weakens the asymmetric cost: a false merge still cannot be produced by the core, and the
  one seam it keeps cannot produce any merge at all.
