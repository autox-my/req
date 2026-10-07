---
status: superseded by docs/adr/0009-core-language-rust.md
---

# The core is Python; the PM porcelain is planned as TypeScript

The canonicalizer, engine, CLI, and benchmark live in **Python 3**; the PM web UI is
the TypeScript/React stack, planned for build step 10. This choice was not obvious, so
it is recorded: TypeScript-everywhere would give one engine implementation across CLI,
IDE, and browser, but its determinism cost lands at the bottom of the stack — exact
decimals and Unicode precision need pinned third-party libraries plus actively banned
`Intl`/ICU paths — while Python's `Decimal` and `unicodedata` are exact and stable in
the stdlib. A Rust core was rejected for now on timing, not merit: the rules are
small, pure, and data-driven, and compressing the corpus/rules/benchmark feedback loop
matters more than the single-binary posture it buys. Go was rejected on the fondest
of its claims: a single static binary and infra credibility — because its stdlib has no
exact decimal and its map-iteration nondeterminism is a genuine hashing-core footgun.

## Considered Options

- **Python core + TS porcelain later (chosen)** — each layer gets its best language;
  the seam is the L1 file contract (ADR 0002), which already defines how surfaces
  exchange with the engine.
- **TypeScript everywhere** — the unification bonus is real; its cost lives at the
  bottom where it hurts determinism the most.
- **Rust core + TS porcelain** — the mature end-state; revisit if either flip
  criterion below triggers.
- **Go core** — weakest for exact decimals and canonicalization determinism.

## Consequences

- **The rules are data + property tests.** This is what keeps a future port cheap and
  is mandatory from the first module, not a maybe.
- **Interpreter pinned; Unicode version audited.** `unicodedata` is carried by the
  CPython minor version — the build pins the interpreter and documents which Unicode
  revision it implies; a rules/Unicode bump is treated like any Tier-1 rules bump
  (candidates needing re-ratified claims, per the hashing spec's freeze policy).
- **Banned-in-canonical-path rules are lintable.** `float(`, locale-dependent
  formatting, and `toLocaleLowerCase`-class calls never enter the Tier-1 module.
- **Porcelain language is deferred, not undecided** — TS/React is the plan; deciding
  it early would have its real cost at the bottom of the stack, and this ADR keeps
  the unification temptation from being rationally inferred rather than a chosen
  deferral.

## Flip criteria (when this decision should be revisited)

- A **static, single-binary, air-gapped distribution** becomes a hard requirement —
  the core then moves to Rust (or Go if the talent pool demands it), with rules ported
  cheaply because they are data with property tests.
- The scale requirement grows past the stated 100k-claim target into a regime where
  the Python interpreter itself becomes the bottleneck.
- The team's fluency profile contradicts the one used in this decision
  (`docs/design.md` says what was assumed here).
