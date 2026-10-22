# req

A semantic, git-native requirements engine.

**Requirements are decisions, not documents.** In req the unit of work is a *claim*: one
claim per file in git, written in a small, reviewable format. A claim's identity is not its
path and not its history, but a content address over its *canonical meaning* — a
deterministic pipeline parses the statement, normalises it against a subject registry, and
hashes the result. Two claims with the same address are the same requirement, however
differently they are worded. Two claims that differ in a bound, an actor, or a modality are
different requirements, however similar their prose.

The spec, the traceability matrix, and the audit packet are compiled views over a signed
baseline. Nobody edits them by hand; they are regenerated, and their freshness is verified
against what was signed.

## The bet, stated so it can fail

> Deterministic canonicalization yields **zero false merges**: no rewording, however loose,
> collapses two different requirements into one.

A false merge silently deletes a requirement. A false split is noise. That asymmetry shapes
everything: the gate is zero false merges, it runs on every commit, and it has never been
relaxed to let a change land.

## What it does

| command | the question it answers |
| --- | --- |
| `req init` | start a corpus here — a starter claim already filled and verified, its registry, and the authoring README |
| `req canon` | what each claim parses to, and whether the parse still agrees with the author's slots |
| `req bench` | does identity hold — zero false merges over the corpora; exit non-zero and the build is red |
| `req select` | which claims apply in a variant — selectors, not branches — with UNKNOWN as a first-class answer |
| `req render`, `req trace` | the compiled spec view, and the traceability matrix with every edge named |
| `req impact`, `req gate` | what a change touches, nearest first, and the release policies |
| `req baseline`, `req audit` | the canonical manifest a human signs, drift verification, and the audit packet |
| `req decision` | the decision log and its hash chain — TAMPERED is a finding, a truncated history is unanswerable |
| `req propose` | the Tier-2 funnel: deterministic blocking, then a model proposes and a human disposes |

Tri-state is a discipline here, not a garnish: PASS, FAIL and UNVERIFIABLE are kept apart,
because "we could not tell" is a different answer from both "it passed" and "it is broken".

The **corpus root** is resolved at *runtime* (ADR 0012), so req manages any repository's
claims: `--root <dir>`, then `REQ_ROOT`, then the nearest ancestor of the working directory
holding a `claims/` directory. Whichever root was chosen is printed to stderr — never stdout,
which carries the exact bytes a human signs — and a root that yields no corpus is
UNVERIFIABLE (exit 3), never an empty answer. `req init` is the one exception: it *creates* a
root in the current directory, so it resolves none and takes no `--root`.

## The two differentiators

1. **Semantic content-addressing.** Identity over canonical *meaning*: a rewording keeps its
   address, and an edit that changes an obligation gets a new one — with the direction of the
   change classified, so a weakening is never reported as no change.
2. **Variability as selectors.** A variant is a query over one model of claims, not a branch
   or a fork: variants compose, applicability is computed per context, and a claim a context
   cannot decide is surfaced rather than dropped.

Git-native storage is table stakes, not the moat.

## Status

- **Tier-1 is built and measured.** `RULES_V 5`, 21 dogfood claims — req's own requirements,
  written in req's own format — and seven corpus families, including three of real
  requirements prose from other projects, pinned by commit. Zero false merges on all of them.
- **Tier-2 is measured and deliberately conservative.** Five providers evaluated against a
  human-labelled sample; one has a usable confidence ordering at a measured floor; the honest
  default is `NoProvider` until a provider survives held-out validation on a grown,
  blind-labelled corpus. Instruments in `tools/tier2/`, protocol in
  `docs/tier2-sample-protocol.md`.

## Development

```bash
cargo build --release
cargo run -q -- bench     # the gate: zero false merges, or the build is red
./scripts/check.sh        # fmt, clippy, tests, canon + Baseline verification, every corpus
```

Changes go through a feature branch and a pull request. CI runs the same gate that runs
locally, and an AI review reads the diff and blocks on critical or high findings. The
agent-facing instructions are in `AGENTS.md`, the glossary in `CONTEXT.md`, the design of
record in `docs/design.md`, and the decisions in `docs/adr/`.

Adopting req in a project — as a human or as an agent — starts at `docs/agentic.md`: the
five-minute path, the CI snippet in `examples/github-workflows/req.yml`, and a block to paste
into a project's own `AGENTS.md`. `tools/quickstart.sh` rehearses that path offline and prints
`QUICKSTART <n>s`; the page is rehearsed by the script, not asserted by it.
