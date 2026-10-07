---
status: accepted (supersedes docs/adr/0008-core-language-python.md)
---

# The core is Rust; the porcelain is TypeScript

The three Tier-1 criteria in the previous language ADR were already set up to fire on
exactly the input that arrived. Flip criterion #3 ("the team's fluency profile
contradicts the assumption used here") was triggered by the developer's preference for
Rust; on re-checking, criterion #1 ("a static, single-binary, air-gapped distribution
becomes a hard requirement") is *not* a hypothetical for this product — it is the
position the design already argues (docs/design.md §2 tension 5, docs/design.md §8:
air-gap/ITAR posture as a differentiating architecture) — and the re-test found the
original rejection's remaining argument could not be substantiated. Superseding.

## What the rejection was, and what verification found

The earlier pick rejected a Rust core **"on timing"** — a slow claim that it would
comprise "the corpus/rules/benchmark iteration loop". That was an **unmeasured
assumption**, now retracted. Under this design's rules-as-data architecture (ADR 0007,
the hashing spec's Tier-1 tables), a rules edit is a *data* edit in either language;
the compile-time counterweight is a test cycle whose magnitude for a small, pure,
stdlib-like core is comparable to Python's, never measured in this environment, and —
given the type-enforced invariants Rust adds — plausibly cost-*negative* for the
correctness work that matters here. No measurement supports the "slower, therefore
rejected" claim, so the claim is withdrawn rather than re-argued.

Neither environment cost is a veto either: in this dev container **cargo/rustc and
`pip3` are equally absent** — both routes require a toolchain bootstrap step.

## Facts verified for this decision (measured in this turn, not assumed)

- **crates.io API, pulled 2026-10-07:**
  - `rust_decimal` 1.43.0 — 154.2M downloads (37.2M recent), pure Rust, MIT, updated
    2026-09-02, MSRV 1.67.1, ships a `proptest` feature (directly useful for the
    property-test harness this design already mandates).
  - `git2` 0.21.0 — 118.3M downloads (16.4M recent), **rust-lang maintained**, MIT /
    Apache-2.0, updated 2026-10-06, with `vendored-libgit2` and `vendored-openssl`
    features — the exact knobs a self-contained, air-gapped single binary needs.
  - `gix` 0.88.0 — 50.5M downloads (12.1M recent), pure Rust (no C FFI surface),
    MIT / Apache-2.0, updated 2026-09-25, MSRV 1.88, edition 2024, with `merge` and
    `sha256` features matching L1/L2 needs.
  - `serde_yaml` — 427.5M downloads *but* the newest release is literally named
    `0.9.34+deprecated` (last update 2024-03-25). This is the one soft spot found:
    the YAML reader crate must be pinned from a maintained successor (~saphyr /
    serde_yml; the pick itself is not verified here) at the first lockfile.
- **This dev container (measured):** `cargo`/`rustc`: absent; `go`/`pip3`: absent;
  `node` v24.21.0, `python3` 3.13.5, `git` 2.47.3, `pnpm` 11.7.0: present. The Rust
  route needs a `rustup` bootstrap with network + approval here; this is an
  environment fact, not a product vote.

## Why Rust now wins (the re-tested reasons)

1. **Tier isolation becomes a *compile-time* invariant**, not a lint/convention.
   ADR 0007's most dangerous decision — probabilistic output may never construct an
   address — is structurally enforced when `Canonical` has no constructor reachable
   from Tier-2 values. In Python that guarantee vanishes into convention and review.
2. **The air-gap posture is a product thesis we already adopted** (docs/design.md
   §2/§8). A static single binary — `git2`'s/`gix`'s vendored builds — plus a
   checksummed, reproducible artifact is a materially better DO-330/audit posture than
   any interpreter-pinned install. Criterion #1 was written for exactly this case.
3. **`wasm-bindgen` delivers the "one engine everywhere" property — better than the
   TypeScript-everywhere argument I credited.** The same core runs in the CLI, as the
   LSP/IDE extension process, and *in the browser* via WASM for the PM UI — without
   the determinism-at-the-bottom cost that made me reject TS for the engine.
4. **The L1 git integration is first-class** in this stack (git2/gix verified, above),
   so ADR 0002's "commits are signed Baselines" lands on mature, maintained bindings.

## Considered Options

- **Rust core + TS porcelain (chosen)** — flip criteria triggered; verified facts
  above.
- **Python core (ADR 0008)** — every remaining pro is either absorbed by rules-as-data
  (iteration speed), matched by a verified crate exact-decimal equivalent
  (`rust_decimal`), or weaker than the air-gap posture it would trade away. No
  measurable advantage survives the re-test.
- **TypeScript core** — the unification appeal survives, but only at the cost of
  banned-international-locale discipline at the bottom of the stack; Rust core with a
  wasm-*target* achieves the same unification without that hazard.
- **Go core** — unchanged: no stdlib exact decimal; the map-iteration nondeterminism
  argument applies to Rust's `std::HashMap` too (it uses randomized seeds), so the
  canonicalizer's rule is *sort everything / use `BTreeMap`* in either language — Go
  still loses on that point being *only* a weakened fix for an unqualified need.

## Consequences

- **Two toolchains, honestly counted**: rust (core) and TS porcelain, when it arrives —
  two CI gates, a wider DO-330 audit surface. This is the *real* trade that the timing
  claim gestured at; recorded here as the actual, verifiable cost.
- **Supply-chain audit surface is a maintained work item**: the pinned sets show
  `serde_yaml`'s deprecation; `cargo-deny`/`cargo-vet` (or `cargo-audit`) gate new
  crates from the first lockfile.
- **Toolchain/unicode-version pin replaces the interpreter pin**: MSRV, lockfile, and
  the `unicode-normalization` crate rev are part of the rules' provenance; a bump is a
  Tier-1 rules bump (candidates needing re-ratification, per the freeze policy — never
  a silent rehash).
- **Canonicalizer discipline (language-typical) to ban proactively**: no `f32`/`f64`
  math anywhere near the address; no `HashMap` iteration (sort keys / use `BTreeMap`)
  anywhere the address is computed; no locale-dependent formatting in the Tier-1 path.
  Go's map-iteration footgun, the same hazard in Rust's `RandomState`, and float paths
  are all *enumerated* here so the lints/canonicalizer tests cover them deliberately.
- **Bootstrap step for this dev environment**: `rustup` install (not present here) —
  this is the concrete prerequisite before step 3 of `docs/design.md`'s build order
  can run in this container; if network/approvers deny it, this fact blocks code —
  not the decision.
