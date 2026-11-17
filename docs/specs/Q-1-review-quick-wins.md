# Q-1: four quick wins from the req-vs-TRLC/LOBSTER review

Task id: `Q-1`. Base: the commit that adds this spec, on top of `origin/main` at `9c659bb`. The owner accepted these four items
on 2026-11-17, from an external comparative review (a private document; it is not quoted, and none of its text belongs in the
repository). Each item was checked against today's `main` before this spec was written.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `AGENTS.md`, in particular: Tier-1 determinism; "Version pins are provenance" (no new dependency, because any
  `Cargo.lock` change other than req's own entry is a Tier-1 rules bump); three-state outcomes; "Docs are claims too";
  TDD with independent oracles.
- ADR 0022: nothing derived lives in a claim.
- ADR 0021: the `propose` seam.
- Each command's `--help` page, which is its contract.

## 1. `claims/README.md` names the schema the files use

`claims/README.md:36` says the format is `req.claim/2`, while every file in `claims/` is `req.claim/3`. Fix it, and fix any other
place that describes this repository's own claims as `/2` (`src/init.rs:26`'s doc comment says "`/2` or `/3`", which is true,
so leave it). Add a test that reads the schema line every file in `claims/` declares, and fails if `claims/README.md` names
another one. Do not type the version into the test as the expected answer; read it from the files.

## 2. Property tests for the canonicaliser, with no new dependency

Add a test file of invariant checks over `canonicalize_statement` (and the public seams it is reached through). **Do not add
`proptest`, `quickcheck` or any crate.** Write a small deterministic generator in the test: a fixed-seed integer PRNG and
integer arithmetic only, with no floats. It composes statements from the vocabularies the rules already know: subjects from
`aliases/subjects.yml`, modalities, comparators, values, units and conditions. Run a few thousand cases per invariant, inside
the gate's time budget. The invariants, each a named test:
- **No panic,** and a deterministic result: the same input gives the same canonical form and address on a second call.
- **Normalisation is idempotent:** canonicalising the rendered canonical text again gives the same address, where the code
  offers such a rendering. If it doesn't, say so and test the nearest true property.
- **The coverage invariant holds:** whenever the parse is structured, every word position is attributed to a slot.
- **Adding a condition always moves the address.**
- **LE against LT never collapses:** "at most N" and "less than N" never share an address. The same goes for GE against GT.
- **Unit conversion is exact:** equal quantities in convertible units (ms and s, for example) share an address, and unequal
  ones never do. All arithmetic is in integers or exact decimals.
- **Every generated case has a known-answer twin.** Where a generated case has a labelled corpus pair, the property and the
  corpus agree; do not regenerate corpus labels.

A property that turns out false on today's code is a finding, not a test to weaken. Report it in the envelope, with the
smallest failing input, and stop for the orchestrator. Do not change Tier-1 code in this unit.

## 3. The claim loader reports every broken file, not the first one

`load_claims` returns `Result<Vec<LoadedClaim>, String>`, so a corpus with three broken files names one.
- Collect the problems: every file that fails to load, each with its own message. A command still exits **3** once, after
  printing them all, in a deterministic order (by path).
- Keep every existing refusal and its wording: a bad name, a duplicate id, an empty corpus, a symlink, a subdirectory, an
  unknown key. A corpus-level problem that stops loading altogether (an unreadable registry, an empty corpus) is still
  reported on its own.
- `.req/areas` already reports every bad line, so follow its pattern.
- Change the signature in the smallest way the callers allow. No caller may treat a partial corpus as a whole one: a load
  that found any problem returns no claims.
- Tests:
  - a corpus with three broken files names all three in one run and exits 3;
  - a corpus with one broken file behaves as today, byte for byte, where existing tests pin that.

## 4. An example `propose` provider in `docs/adopting.md`

`req propose <a.yml> <b.yml> --provider-cmd <cmd>` has no example provider, so an adopter cannot see the seam working.
- Add a short subsection to `docs/adopting.md` with a minimal provider: a shell or Python script that reads what `propose`
  hands it and answers in the format `propose --help` documents. Put the script under `examples/` (for example
  `examples/propose-provider.sh`).
- It calls no model and no network: it is a deterministic stand-in that shows the contract. A real provider is the
  adopter's process boundary (ADR 0021). Say so.
- Add a test that runs `req propose` with the example provider on two claims, and checks that the output is the documented
  unratified-proposal line, with the provider command recorded verbatim.

## Allowed paths

- `claims/README.md`
- `src/canon.rs`, and the callers of `load_claims` under `src/` (item 3 only; no Tier-1 rule changes)
- `docs/adopting.md`, `examples/propose-provider.sh` (or `.py`)
- New test files under `tests/` for items 1–4, and existing tests that pin the loader's messages, where item 3 moves them
  (say which and why)

Not allowed: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `claims/*.yml`, `bench/`, `baselines/`, and any change to
`RULES_V` or to what a statement canonicalises to.

## Done when

- The full `scripts/check.sh` is green with no SKIP.
- Every new test was red at the base where it tests new behaviour (items 1, 3 and 4). Item 2's invariants pass on today's
  code, or are reported as findings.
- Mutation checks:
  - item 3: re-introduce the stop at the first error, and the three-file test fails;
  - item 2: swap LE and LT in the comparator table on a scratch copy, and the LE/LT property fails. Do not commit that.
- `git diff <base>...HEAD --name-only` stays inside the allowed paths.
