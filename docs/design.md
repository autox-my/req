# Design of record — req

Status: v0 (2026-10-07).
This document exists because until now the full synthesis lived only in conversation,
which is not a system of record. Complements [CONTEXT.md](../CONTEXT.md) (the glossary)
and the ADRs — most paths (docs/adr/0001-the-unit-is-a-claim.md … 0006-friction-zones.md)
carry individual decisions.

## 0. Where this came from

Three proposals converged in discussion; this document pins the synthesis, not a fourth
opinion:

- **The decisions thesis** — model *claims*, content-addressed by canonical meaning;
  documents are compiled views, never the source of truth.
- **Git-native "Req"** — requirements as repository files, CI as the gate, air-gap by
  design, dual interface. Its real contributions survive (ADR 0002, ADR 0006); its
  storage-first thesis does not (docs/adr/0001-the-unit-is-a-claim.md,
  docs/adr/0002-git-is-transport-not-identity.md).
- **The reconciliation** — git for transport, the graph for identity, selectors for
  variants, signed decisions for authority.

## 1. Thesis

> Requirements are decisions, not documents. We content-address the decisions —
> semantically — and compile the spec, the traceability and the audit from a signed
> baseline. Git gives you the storage and the atomic coupling; the claim graph gives you
> the meaning; selectors give you 40 variants without 40 forks; signed decisions give you
> an audit that holds up.

Why now: LLMs make semantic operations on natural-language claims tractable;
content-addressing makes the audit reconstructible instead of archaeological; file-native
storage makes offline and air-gapped work free. None of these existed when the incumbent
suites were designed — which is why the incumbents cannot follow without rewriting their
data model.

The falsifiable center is canonical hashing ([docs/canonical-hashing.md](canonical-hashing.md)):
deterministic normalization must produce a measured false-merge rate of **zero** before
anything is built on it.

## 2. Tensions → resolutions (this *is* the design)

A perfect tool is defined by the tensions it resolves, not the features it lists.

| # | Tension | Resolution | ADR |
|---|---|---|---|
| 1 | Friction vs adoption | Two zones: authoring friction-free (produces Proposals); the decision boundary deliberately auditable (role, meaning, evidence) | 0006 |
| 2 | Determinism vs semantic power | Structural deterministic core; semantic providers *propose* reviewable edges; humans ratify | 0004 |
| 3 | Code coupling vs graph identity | Git is transport+serialization; the graph is identity+merge unit; files are a projection | 0002 |
| 4 | Generality vs wedge | General claim graph; one opinionated wedge — permanent variation (selectors) | 0003 |
| 5 | Concurrent editing vs legally-weighted prose | Claim-level granularity, explicit Working set, *one commit per ratified logical Decision*; never last-write-wins; policy must be auditable | — |
| 6 | False merge vs false split | Hash proposes `may_be_equivalent`; split always allowed; merge always ratified | 0004, 0007 |

## 3. Layer model

- **L0 Graph** — `Claim`, `Relation`, `Decision`, `Evidence`, `Baseline`, `Variant`;
  identity by canonical meaning; a `Decision` records authority *and* the dissent +
  road not taken (never discard the losing option).
- **L1 Storage** — git repo = deterministic serialization, one claim per file;
  commits are signed Baselines; `git checkout` moves spec+code atomically.
- **L2 Merge** — deterministic structural 3-way over claim IDs; conflict classification
  by *direction of change* (computable only via the canonical layer); provider-assisted
  proposals.
- **L3 Porcelain** — engineer CLI/IDE; PM web UI over the claim model with an explicit
  Working set (Gap: never a continuous file sync bridge).
- **L4 Views** — `render --audience X --jurisdiction Y --baseline vN`: spec, trace
  matrix, V-model evidence graph, audit packet, roadmap. Compiled, never maintained.
- **L5 Protocol** — open object model; ReqIF interop (incumbents become clients);
  federated libraries for shared/regulatory claims.

## 4. Pain → mechanism (consolidated)

| # | Pain | Mechanism |
|---|---|---|
| 1 | Centralized bottleneck, no offline | Git-native serialization; full graph local; L0 works offline |
| 2 | Spec/code desync | Same repo, same branch; `checkout` moves both; atomic PR |
| 3 | Ticket state global, branch state local | Claim state is branch- and selector-scoped |
| 4 | Non-engineers can't use git | PM UI on the claim model; edits are Proposals in a Working set; one commit per ratified logical Decision — *never* continuous two-way sync |
| 5 | Concurrent-edit data loss | Claim-level granularity + explicit proposal state; no last-write-wins |
| 6 | Textual merge on prose | Structural merge + semantic proposals, human-ratified |
| 7 | Duplicate requirements | Semantic content-addressing (the technical bet) |
| 8 | Contradictions found in the field | Graph contradiction queries; Conflict is a first-class object |
| 9 | Ambiguous / untestable claims | Testability lint: unmeasurable bound, vague verbs, unquantified ``shall`` |
| 10 | Rationale rot | Rationale is an object: source, date, rejected alternatives, assumptions |
| 11 | Stale traceability matrices | Trace is a query; never stale by construction |
| 12 | Trace skipped under pressure | CI Gate — scoped to traceable targets, demands evidence not ID presence |
| 13 | Audit archaeology | Signed Baseline; `render --audience auditor` compiles the packet |
| 14 | E-signature / 21 CFR Part 11 | Signed Decisions: identity, authority, meaning, timestamp |
| 15 | Rewritable history | Append-only refs + signatures + optional external anchoring |
| 16 | Variant explosion | Applicability selectors over the 150% model (the commercial bet) |
| 17 | Cross-product shared/regulatory claims | Federated claim libraries; ReqIF interop; claims as dependencies |
| 18 | Impact guesswork | `req impact` traverses relations + evidence + selectors |
| 19 | Air-gap / ITAR | File-native, self-hosted, offline by default |
| 20 | Lock-in | Open object model; the repo is the migration artifact |
| 21 | Auditor unfamiliarity | DO-330 tool qualification + safety manual as first-class deliverable (deferred: §9) |
| 22 | Scale (100k+ claims) | Indexed graph; specs are projections |
| 23 | AI accelerates prose rot | AI as quality gate + semantic engine — never author of record |
| 24 | Roadmap vs spec | Views compiled over the same graph |

## 5. Build order and gates

1. **Claim-file format v0** — the serialization contract for one claim per file —
   written: [docs/claim-format.md](claim-format.md).
2. **Dogfood seed corpus** — seeded: `claims/REQ-0001…REQ-0020`, the subject registry
   (`aliases/subjects.yml`), and the starter parity corpus
   ([bench/corpus/parity.yml](../bench/corpus/parity.yml)); indexed in
   [claims/README.md](../claims/README.md).
3. **Canonicalizer v0** — Tier-1 only; **Rust** (ADR 0009, supersedes ADR 0008); pinned
   crate set; offline; no model anywhere in the core. Toolchain installed here
   2026-10-07 (cargo/rustc 1.99.0, minimal profile), canary build verified exit 0.
4. **Benchmark** — parity + adversarial traps corpora; HARD GATE: false-merge rate must
   equal **0.0**. If it isn't, the canonicalizer is *unsafe* and nothing below this line
   proceeds ([docs/canonical-hashing.md](canonical-hashing.md)).
   *First run, seed corpus, 2026-10-07: FMR = 0.0 on all 7 adversarial traps (LE-vs-LT,
   modality, value, condition-add, ambiguous "under", subject-swap, false-vocabulary);
   FSR = 0.0 across the 5 structured equivalents (unit conversions collapse exactly);
   3 Tier-2 candidates (E-006/7/8 — voice, paraphrase — are the provider funnel's rule,
   by design); 1 lint (N-005's "under" is never guessed); verdict **PASS**. Results:
   [bench/results/rules-v0-seed-first-run.json](../bench/results/rules-v0-seed-first-run.json).
   This is the *seed-corpus* gate; the external corpora falsification (step 4's
   headline claim) is the next measured run.
5. **Semantic proposals** — pluggable provider behind a stable interface; Tier-2 only.
6. **Structural merge + conflict classification** (needs direction, hence the canonical
   layer).
7. **Selectors + tri-state scope** (ADR 0003) — the commercial differentiator.
8. **Decision log + hash chain + verify** (`VALID` / `TAMPERED` / `UNVERIFIABLE` as three
   distinct outcomes).
9. **`render` / `trace` / `impact` / `gate`.**
10. CLI, then the porcelain split.

## 6. Open problems, mapped to their tension

1. Canonicalization accuracy → tension 6. False merges are catastrophic; FMR=0 is the gate.
2. Non-engineer adoption → tension 1. UX, not storage; no architecture solves it.
3. Concurrency on legally-weighted prose → tension 5. Name the policy; defend it to an auditor.
4. Tool qualification (DO-330 / tool-confidence) → §9; deferred deliberately, not denied.
5. Buyer ≠ sufferer → bottom-up via the code link; top-down via the audit packet.

## 7. Dogfooding — req manages req

The tool's requirements are managed with the tool's own objects. The seed corpus already
exists in embryo: the glossary terms are the alias registry; the ADRs are Decisions with
the losing side recorded; the tension table (§2) converts into claims carrying
applicability conditions. Benefits beyond hygiene: the same machinery accrues the
tool's own DO-330 dossier over time, and this project's genuine paraphrases (the three
proposals restated identical decisions differently) are a free equivalence corpus for the
benchmark.

## 8. Prior art, verified 2026-10: the plain-text tools are alive - and one grew a competitor

| Tool | Stars | Created | Last push | Note |
|---|---|---|---|---|
| Doorstop | 674 | 2013-04 (the "2011" claim was wrong) | 2026-10-03 | maintained |
| StrictDoc | 408 | 2020-05 | 2026-09-25 | regulated usage claimed (DO-178C, ECSS, IEC 62443) |
| Sphinx-Needs | 309 | 2016-11 | 2026-10-07 | MIT; topics list DO-178C/ISO 26262/IEC 61508 |

The operative insight is not the numbers — it is that **Sphinx-Needs did not die; it grew
into a company**. useblocks GmbH (its maintainer) now sells "the intent layer for
compliant agentic engineering in safety-critical industries" with named automotive
customers (Bosch, BMW, VW/ZF, Continental, Cariad, VinFast, Denso, TTTech Auto), an agent
product, and x-as-code messaging: "audit-ready on day one, not reconstructed before every
release".

Implications:

- The niche-market thesis holds (sub-1k-star OSS after a decade), but it *grew a
  commercial wrapper aimed at our exact segment* — the market is validated and **a
  competitor with customers is already active**.
- Their identity model is still files/documents; no evidence of semantic content-addressing
  or selectors. The differentiation window is real but **closing** — the why-now argument
  is now also a wind-in-our-sails deadline.

## 9. Deferred deliberately

- Tool qualification per standard (DO-330 for airborne; tool-confidence per ISO 26262-8;
  software-tool validation per IEC 62304) — deferred, not denied. Sequencing: wedge
  revenue first, co-funded qualification with paying design partners when req-generated
  evidence starts feeding *their* audits. The propose-only architecture (ADR 0004) lowers
  the tool's claimed automation impact, which lowers the qualification burden.
- Production signatures (prototype: HMAC + hash chain; later: Ed25519/minisign).
- ReqIF interop, federation, PM web UI — after the plumbing gates pass.
