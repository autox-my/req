# Design of record — req

Status: v0 (2026-10-07); implementation state recorded 2026-11-11 (§4 *State* column, §5 status marks,
§6 items 6–8). The design itself is unchanged by that revision: it records what is built against it.
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

(*Wording, 2026-11-02:* "content-address" here names the **meaning fingerprint** — a derived
comparison primitive for drift and duplicate hints; a claim's identity is its `claim_id`, per
[ADR 0020](adr/0020-the-address-is-a-meaning-fingerprint.md), accepted.)

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
  (*State, 2026-11-11:* built as separate commands over one selection — `req render` (spec),
  `req trace` (matrix), `req audit` (packet against a Baseline) — each with `--out`/`--check` where it is
  a committed projection (ADR 0018). `--audience`, `--jurisdiction`, the V-model evidence graph and the
  roadmap view are not built.)
- **L5 Protocol** — open object model; ReqIF interop (incumbents become clients);
  federated libraries for shared/regulatory claims.

## 4. Pain → mechanism (consolidated)

| # | Pain | Mechanism | State (2026-11-11) |
|---|---|---|---|
| 1 | Centralized bottleneck, no offline | Git-native serialization; full graph local; L0 works offline | built — REQ-0012; air-gapped is the release variant |
| 2 | Spec/code desync | Same repo, same branch; `checkout` moves both; atomic PR | built — projections checked by `req check` (ADR 0018/0023) |
| 3 | Ticket state global, branch state local | Claim state is branch- and selector-scoped | built — selectors (ADR 0003); branch scope is git's |
| 4 | Non-engineers can't use git | PM UI on the claim model; edits are Proposals in a Working set; one commit per ratified logical Decision — *never* continuous two-way sync | **not built** — no PM UI, no Working set |
| 5 | Concurrent-edit data loss | Claim-level granularity + explicit proposal state; no last-write-wins | partial — one claim per file, `req next-id --reserve`; no explicit proposal state |
| 6 | Textual merge on prose | Structural merge + semantic proposals, human-ratified | **not built** — §5 step 6 |
| 7 | Duplicate requirements | Semantic content-addressing (the technical bet) | built — the meaning fingerprint, FMR = 0 measured by `req bench` (drift signal, not a paraphrase collapser: README *Status*) |
| 8 | Contradictions found in the field | Graph contradiction queries; Conflict is a first-class object | **not built** — no contradiction query, no Conflict object |
| 9 | Ambiguous / untestable claims | Testability lint: unmeasurable bound, vague verbs, unquantified ``shall`` | built — `req canon` lints (`untestable_comparator`, `untestable_qualifier`, `ambiguous_modality`, `uncovered_content`) |
| 10 | Rationale rot | Rationale is an object: source, date, rejected alternatives, assumptions | built — `rationale_present`, line anchors checked |
| 11 | Stale traceability matrices | Trace is a query; never stale by construction | built — `req trace`; `docs/trace.md` checked (ADR 0018 amendment) |
| 12 | Trace skipped under pressure | CI Gate — scoped to traceable targets, demands evidence not ID presence | built — `req gate` `tests_linked` over JUnit reports (ADR 0019/0024); claim→test only |
| 13 | Audit archaeology | Signed Baseline; `render --audience auditor` compiles the packet | built — signed Baseline + `req audit` (not `render --audience`) |
| 14 | E-signature / 21 CFR Part 11 | Signed Decisions: identity, authority, meaning, timestamp | partial — Baselines signed (ADR 0010/0016); Decisions hash-chained, not individually signed; `role` is free text (§9) |
| 15 | Rewritable history | Append-only refs + signatures + optional external anchoring | partial — Decision hash chain, attested by the Baseline (ADR 0016); no external anchoring |
| 16 | Variant explosion | Applicability selectors over the 150% model (the commercial bet) | built — tri-state selectors, `in` operator (ADR 0030) |
| 17 | Cross-product shared/regulatory claims | Federated claim libraries; ReqIF interop; claims as dependencies | partial — ReqIF and needs.json readers draft claims (ADR 0027/0028); external corpus pinned (ADR 0033); no federation, no writer |
| 18 | Impact guesswork | `req impact` traverses relations + evidence + selectors | partial — `req impact` over relations, each dependent's state in the selection; evidence not traversed |
| 19 | Air-gap / ITAR | File-native, self-hosted, offline by default | built — no network in the core |
| 20 | Lock-in | Open object model; the repo is the migration artifact | built — plain YAML, open schema |
| 21 | Auditor unfamiliarity | DO-330 tool qualification + safety manual as first-class deliverable (deferred: §9) | partial — safety manual draft, `req selftest`, reproducible build, release record (ADR 0034/0035/0037); no qualification |
| 22 | Scale (100k+ claims) | Indexed graph; specs are projections | **not built** — measured linear, trigger recorded (§9.1) |
| 23 | AI accelerates prose rot | AI as quality gate + semantic engine — never author of record | built — `req propose`, one pair, never author of record (ADR 0021) |
| 24 | Roadmap vs spec | Views compiled over the same graph | **not built** |
| 25 | Incomplete requirements, silent omissions | Report what req can prove structurally: a claim no release examines, no Baseline covers, or no relation joins; never assert "complete" | built — `req check`'s `completeness` step and the audit's *Structural omissions* (ADR 0039) |
| 26 | Design satisfaction unverified | A design or code element states which claims it satisfies; the link is checked against meaning drift | **not built** — §6 item 7 |
| 27 | Multi-party co-development (OEM ↔ supplier) | Signed, pinned snapshots exchanged between repositories; sign-off by named parties | **not built** — §6 item 8; ADR 0033 is the one building block |

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
   *External run, 2026-10-07 (rules v1): 338 pairs built from other projects' real
   requirements — StrictDoc's own L1 requirements and Doorstop's own requirements,
   labelled by construction: **FMR = 0.0 across 160 meaning-changing mutations, with 0
   structurally unverifiable pairs** (the pass is not vacuous); FSR = 1/173 = 0.58%.
   The headline holds on external prose.* Two findings recorded
   ([canonical-hashing.md §6.3](canonical-hashing.md)): the single split is a
   multi-clause statement whose second obligation sits in the unparsed tail (clause
   decomposition is the next rule candidate), and 16 `untestable_comparator` lints are
   false positives on standard names ("IEC 61508"). Results:
   [bench/results/external-v1-rules-v1.json](../bench/results/external-v1-rules-v1.json),
   [bench/results/parity-rules-v1.json](../bench/results/parity-rules-v1.json).
   *Rules v2, 2026-10-07: the v1 findings became rules — prohibition via "never",
   multi-obligation detection, qualitative-predicate classification, a precise
   comparator lint — and the extended corpus (external-v2, predicate-swap class)
   immediately found a **critical** defect: an unbounded claim's predicate was not part
   of its identity, so **58 of 227 mutations (25.6%) collapsed silently**. Fixed: FMR
   0.0 over 227 mutations, FSR 0.0, all three corpora PASS. The seed corpus had missed
   it because no pair differed by verb alone. Our own corpus is clean too: 21 claims, 0
   discrepancy flags — REQ-0007's unstructured classification is now derived rather
   than declared, and REQ-0004 was split into REQ-0004 + REQ-0021 because v2 flags
   multi-obligation statements (claim-format requires one obligation per claim).*
   *Rules v3/v4, 2026-10-07: the derived fields are written (`req canon --fill`, 21
   claims address-addressed) and enforced (`req canon --verify` in the gate; both
   removed by ADR 0022 — since `req.claim/2` the fields are derived at load and stored nowhere). Reading
   the bytes it wrote exposed an invalid-JSON canonical form and a schema field
   (`quantifier`) that was never in identity. Then the external source set was widened —
   the **PROMISE NFR** set (deterministic sample, 969 statements, 47 project ids) plus
   StrictDoc and Doorstop — and the widened corpus (external-v3, 1124 pairs) found a
   false merge within minutes of existing: `may not` was mapped to `SHALL-NOT`, which
   T1.6 never sanctioned, so a `shall`→`may` mutation collapsed. Fixed at rules v4
   (ambiguous negation scope is linted, never guessed): **FMR 0.0 over 625 mutations,
   FSR 0.0, four corpora PASS**.*
5. **Semantic proposals** — pluggable provider behind a stable interface; Tier-2 only. *(Since
   ADR 0021 the core keeps the single-pair `req propose` seam; the corpus-wide funnel is research in
   `experiments/tier2/`.)*
6. **Structural merge + conflict classification** (needs direction, hence the canonical
   layer). *Not built:* direction of change exists (`req compare`, `req diff`); the 3-way merge does not.
7. **Selectors + tri-state scope** (ADR 0003) — the commercial differentiator. *Built.*
8. **Decision log + hash chain + verify** (`VALID` / `TAMPERED` / `UNVERIFIABLE` as three
   distinct outcomes). *Built* (`req decision`).
9. **`render` / `trace` / `impact` / `gate`.** *Built*, with `req check` folding them (ADR 0023).
10. CLI, then the porcelain split. *CLI built*; the porcelain is not started.

## 6. Open problems, mapped to their tension

1. Canonicalization accuracy → tension 6. False merges are catastrophic; FMR=0 is the gate.
2. Non-engineer adoption → tension 1. UX, not storage; no architecture solves it.
3. Concurrency on legally-weighted prose → tension 5. Name the policy; defend it to an auditor.
4. Tool qualification (DO-330 / tool-confidence) → §9; deferred deliberately, not denied.
5. Buyer ≠ sufferer → bottom-up via the code link; top-down via the audit packet.
6. Completeness → no oracle in the repository. req proves structural omissions and claims no more
   (ADR 0039: `req check`'s `completeness` step).
7. Design satisfaction → verification reaches code only through linked tests; no design element or
   *satisfies* link exists. Standard practice (OpenFastTrace `Covers:`/`Needs:` with an authored revision;
   Doorstop links that store the parent's text fingerprint and turn *suspect* on any edit) is the baseline to
   beat; req's candidate edge is a link stamped with the meaning fingerprint and judged by direction of
   change. Needs its own ADR.
8. Multi-party collaboration → one repository, one signing key, a free-text `role`. ADR 0005's "the tool must
   model roles and authority" is not yet met. Access control stays with the git host; what req owes is
   sign-off by named parties and exchange of signed, pinned snapshots.

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

### Beside the table: Scion, at a scale where req's target stops

Anthropic, *Redesigning source control at Anthropic*, published 2026-10-13 (updated 2026-10-15):
<https://www.anthropic.com/engineering/redesigning-source-control>. It is their account of
**Scion**, an internal version control system they built in-house and migrated their monorepo onto
in five months with seven people. Scion is a VCS, not a requirements tool, so it is not a row in
the table above; it sits beside it as the adjacent system whose scale regime is where req's design
target *stops* rather than starts.

- **What it is, at what scale.** Their own telemetry: ~2,500 code changes merged per day in May
  2026, over 30,000/day by September; a main-branch history approaching 100 GB, whose compressed
  working tree exceeds 25 GB; ≈25 million CI checkouts a day. Their problem is *data movement and
  materialisation*. req reads a few dozen small YAML files (39 claims on 2026-11-11). The regimes do not overlap, and none of their
  numbers is a target req grows into.
- **An inverted assumption — a divergence, not a lag.** Scion is designed for clients that are
  always online, and the article says that git's offline-distributed strengths had become part
  of the problem at their scale. req inverts a different assumption on purpose: REQ-0012
  requires no network access for any claim operation in an offline deployment, which is what
  makes req usable in the air-gapped, auditor-facing settings it names. The article is evidence
  that one assumption can be inverted profitably; it is not evidence about which assumption req
  should invert.
- **The shared premise.** Their move and ADR 0002 are the same thought: identity belongs where
  the data's semantics are, not where a VCS's primitives happen to put it.

Their own caveat, kept because a prior-art entry that omits it is selling rather than recording:
they state that Scion is not a fit replacement for git in ordinary use, and that its ideas suit
only monorepos at extreme scale. It corroborates a premise. It does not validate req's bet,
which is measured on `bench/` corpora — an unrelated system's success falsifies nothing here.

## 9. Deferred deliberately

- Tool qualification per standard (DO-330 for airborne; tool-confidence per ISO 26262-8;
  software-tool validation per IEC 62304) — deferred, not denied. A draft `docs/safety-manual.md`
  (intended use, guarantees and their evidence, limits, failure modes, operating rules) exists as the
  raw material for that dossier; it is not a qualification. Sequencing: wedge
  revenue first, co-funded qualification with paying design partners when req-generated
  evidence starts feeding *their* audits. The propose-only architecture (ADR 0004) lowers
  the tool's claimed automation impact, which lowers the qualification burden.
- Per-Decision signatures. Baselines are signed with minisign over a committed trust file (ADR 0010),
  and the signed Baseline attests the decision chain's head and length (ADR 0016); an individual
  Decision record is hash-chained but not itself signed, and its `actor` is an asserted name.
- ReqIF interop, federation, PM web UI — after the plumbing gates pass. (*2026-11-11:* the ReqIF and
  needs.json **readers** exist as drafting tools, ADR 0027/0028; a writer, federation and the PM UI remain
  deferred.)

The Scion reading in §8 re-opened two deferrals and made a third question answerable. They are
recorded here with the trigger — or, for the index, the conditions — that would make each current
work, because a deferral without one is a note.

### 1. Corpus-scale cost — measured, with a trigger

`gate` walks the whole corpus, so its cost is **O(corpus) per invocation, not O(change size)**: a
one-claim change pays what a full-corpus gate pays. Measured in this tree — macOS/aarch64, 8 cores,
release build (`cargo build --release`), one wall-clock `gate` per size, best of two after a
warm-up run, on a shared host:

| claims | `gate` wall | ms/claim |
|---|---|---|
| 21 (the real corpus) | 0.009 s | 0.42 |
| 210 | 0.034 s | 0.16 |
| 2,100 | 0.282 s | 0.13 |
| 10,500 | 1.605 s | 0.15 |

Linear with a stable constant: **~0.09–0.17 ms/claim → ≈9–17 s per `gate` at 100k claims**. The
band is the honest width, and it is a band because a per-host absolute is a **sample**, not a
property: the table above is one sample (0.16 / 0.13 / 0.15 ms/claim at n≥210); a second independent
run, on corpora proved md5-identical to these, gave 0.0060 / 0.0227 / 0.1951 / 1.0578 s (a repeat
0.0053 / 0.0213 / 0.1846 / 1.0309 s) — ~1.5× faster throughout, 0.108 / 0.093 / 0.101 ms/claim and
0.101 / 0.088 / 0.098 on the repeat, which points at host load rather than method; and a third
independent run was *slower* than the table, 0.171 / 0.147 / 0.169 ms/claim with 1.774 s against the
table's 1.605 s at 10,500. Four runs at n≥210 span 0.088–0.171 ms/claim, so the band above is that
range rounded outward; the claim is the **shape** and the **trigger**, not the third digit of the
constant. The constant is the **marginal** cost over n≥210 — the 21-claim row's ms/claim sits
above it because ~5.8 ms of its ~9 ms is fixed process start-up, not corpus work. A debug build,
measured the same way, is ~6× slower: ≈0.6–0.9 ms/claim → ≈60–90 s at 100k claims (the ratio, not
the absolute, is what an independent debug run reproduces: three samples span 5.6–6.4×). That is
the same shape of problem Scion solves, arriving at a much smaller scale.
**Trigger to act:** a real-corpus invocation above ~1 s, or a corpus past ~10k claims. The remedy
direction is change-scoping or derived data, not a faster parser — the constant is already small —
and it is deliberately not prescribed here.

The measurement's own discipline is what makes the table a result rather than a number: the
21-claim row is the real corpus copied verbatim, while each larger corpus replicated
`claims/REQ-0001.yml` — which carries no applicability predicate and `relations: []`, so every copy
applies and every edge resolves — with distinct `claim_id`s **keeping its `canonical:` block** (the
strict loader rejects a claim without one, and the copied bytes keep the stored address current —
a `req.claim/1` measurement; under `req.claim/2` a claim carries no such block, ADR 0022).
Each corpus root also needs `aliases/subjects.yml`, without which `gate` exits 3 on a registry load
failure. The gate's own head was asserted to report the expected load at every size — the real
corpus as 15 applying + 6 undecided = its 21 files, and exactly n applying / 0 undecided at 210,
2,100 and 10,500. An earlier attempt that stripped the `canonical:` block was refused by the loader,
after which `gate` returned a flat, tiny time at every size — single-digit-to-tens of milliseconds,
no release-gate header, exit 3 — a clean-looking scaling result that measured nothing.

### 2. A persisted index, if one is ever proposed — the tripwire

The cost above makes a derived index over `claims/` the obvious remedy at scale, which is exactly
why its conditions belong here **before** anyone proposes it. This is not a new rule but the
extension of existing house style: REQ-0020 stores each relation **once**, on the outgoing side,
and derives the incoming edges (the `impact` and `trace` paths already read them that way), and
ADR 0013 hashes `relations` and `rationale` *beside* the address. An index over `claims/` must:

1. be **reconstructible from `claims/` alone**, and **never** influence an address or any digest
   — identity is meaning-only (ADR 0007);
2. record **`rules_v` and a corpus digest**, or staleness is undetectable (a `RULES_V` bump moves
   every address — the same coupling the Baseline `rules_v` guard already enforces);
3. **fail loudly** when missing, corrupt, or built under other rules, and never fall back quietly
   to a fresh derivation. The precedent is the review CLI, which silently falls back to its
   built-in rubric for a glob matching nothing while every exit code stays zero (a script that
   checked the *effect* rather than the status existed until 2026-11-03, when the owner retired
   it and accepted the risk — see ADR 0018's note). An index must be verified **used**, not
   merely loaded;
4. and, if it ever becomes **an input to a decision** or an auditor relies on it, carry its own
   attested hash with a NUL-delimited domain tag and an ADR — the ADR 0013 pattern, which exists
   for exactly this: derived, relied upon, therefore attested separately from identity.

### 3. Concurrency — a recorded divergence, not a gap

Anthropic abandoned the industry-standard global merge-queue lock at agent scale: hundreds of
changes land while the test suite is still running, so their investment goes into testing after the
merge and before deploy, and they rely on agents to fix conflicts fast. req resolves the same
tension differently, and deliberately — §2's tension 5 gives claim-level granularity, an explicit
Working set, *one commit per ratified logical Decision*, never last-write-wins, with an auditable
policy, and ADR 0002 merges in the graph, never with `git merge` over requirement text. The reason
is the asymmetric cost: they repair forward, while a false merge here **silently deletes a
requirement**. A future reader who notices the missing merge queue should read the absence as this
decision, not as an oversight.
