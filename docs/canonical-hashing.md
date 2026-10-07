# Canonical hashing of claims — spec v0

Status: proposed. The falsifiable bet. Implements docs/adr/0001-the-unit-is-a-claim.md,
docs/adr/0004-model-proposes-human-disposes.md, and docs/adr/0007-collapse-tiers.md.

## 1. The falsifiable claim

Deterministic normalization of natural-language claims produces **near-zero false merges
and substantial false splits**. Everything else in the design can be built on that; if
the measured false-merge rate is not zero, the canonicalizer is *unsafe* and nothing may
be built on it — a false merge silently deletes a requirement, which is the one failure
this design exists to prevent (see docs/adr/0004-model-proposes-human-disposes.md).

## 2. What enters the address and what does not

| Enters identity | Does NOT enter identity |
|---|---|
| Modality (SHALL / SHALL-NOT / MAY / SHOULD) | Claim ID |
| Subject (via the registry) | Rationale (source, dated alternatives, assumptions) |
| Attribute / verb-object | Evidence links |
| Value(s) + unit(s), exact | Decisions, approvals, dissent |
| Comparator {LT, LE, EQ, GE, GT, NE} | Author, timestamps, status |
| Conditions[] / exceptions / temporal / quantifier | Variant *selections* (queries) |

The claim's own applicability predicate **enters** identity (it is part of what is
asserted); the *variant's selection* over claims does not — that is a query (ADR 0003).

## 3. Tier-1: rules that collapse (provably meaning-preserving)

Each rule is **decidable**: it applies provably or it is skipped and the residue goes to
lint or Tier-2. A rule that *might* change meaning is not a Tier-1 rule.

- **T1.0 Encoding** — NFC normalization; typographic quotes/dashes → ASCII; strip BOM
  and zero-width characters.
- **T1.1 Casing** — lowercase prose tokens. Preserve underscore/identifiers
  (``brake_type``, ``REQ-402``) and unit symbols until T1.5 has run.
- **T1.2 Whitespace/punctuation** — collapse internal runs; trim; drop the terminal
  period; numeral grouping (``1,000`` → ``1000``). **T1.2b: strip Markdown emphasis
  delimiters** (``**``, backticks) — added after real external prose (Doorstop writes
  ``**shall**``) showed the modality being missed entirely and the claim falling to the
  unstructured fallback. Only delimiter pairs are stripped: a lone ``*`` may be a
  wildcard and ``_`` is meaningful inside identifiers (T1.1).
- **T1.3 Numbers** — decimal parse; **never round, never truncate**; strip trailing
  zeros after the point only (``0.150`` ≡ ``0.15``); scientific notation → expanded
  pure decimal; equality is by value, not by presentation.
- **T1.4 Units → base SI** — curated *power-of-ten* exact-factor table
  (``ms``→``s`` ×10⁻³, ``kHz``→``Hz`` ×10³, ``kN``→``N`` ×10³, ``%``→÷100) plus the
  exact ``°C``→``K`` offset (+273.15). Factors that are **not** exact powers of ten
  (``km/h``→``m/s`` is ×5/18, a repeating decimal) are **NOT** Tier-1 — such units
  stay as their own canonical symbols and Tier-2 may propose equivalence. Anything
  under-specified (bare ``dB``, no reference) is **NOT normalized** → lint
  `untestable_quantity`.
- **T1.5 Comparators** — closed table:
  - ``within`` / ``shall not exceed`` / ``no later than`` / ``at most`` → **LE**
  - ``less than`` → **LT**  ← *the classic trap: LE and LT are different claims*
  - ``at least`` / ``no less than`` → **GE** − ``greater than`` / ``exceeds`` → **GT**
  - bare ``is`` between bound and value → **EQ**. Ambiguous comparators ("fast",
    "as soon as possible") → never guessed → lint `untestable_comparator`. The lint
    fires only for a word that could plausibly *be* a comparator (a curated list): a
    standard's name before a number ("IEC 61508") is not one, and linting it produced
    16 false positives on real external prose. Precision costs nothing here — an
    unresolved comparator is never guessed anyway, so the claim cannot collapse either
    way; only the lint's credibility was at stake.
  - a **qualitative predicate** ("friction-free", "seamless") cannot be canonicalized
    into an operator and a value → lint `untestable_qualifier`, unstructured. This
    *derives* what claim authors previously had to declare by hand (REQ-0007's
    authored `unstructured: true` is now produced by the rules).
  - The phrase *after* the value+unit — e.g. "of standby current" — is an identity
    field (``qualifier``); dropping it is how a standby-current claim silently
    becomes a surge-current claim. This is a false-merge hazard caught in
    implementation, not visible in prose.
- **T1.6 Modality** — ``shall``/``must``/``is required to`` → ``SHALL``;
  ``shall not``/``must not`` → ``SHALL-NOT``; ``shall never``/``must never`` →
  ``SHALL-NOT`` (a prohibition, not an obligation to the verb); ``may`` → ``MAY``;
  ``should`` → ``SHOULD``. The **earliest** match by position wins, and at the same
  position the longest phrase wins: table order must never decide position, or a later
  ``shall never`` shadows an earlier ``shall``. ``will`` is deontic only contextually
  → never guessed → lint `ambiguous_modality`. Modality is never cross-ran
  (``SHALL`` ≠ ``MAY``).
- **T1.6b Obligation and predicate integrity** (rules v2) —
  - the **predicate span** — from the end of the modal phrase (or of the whole
    absorbed ``<modal> <negator> <stem>`` phrase) to the comparator, else to the value
    token, else to the end of the statement — is part of identity. Emitting it only
    when a comparator existed made two unbounded obligations with the same subject and
    modality collapse: **58 false merges** on the external corpus's predicate-swap
    class, the single failure this design exists to prevent. It was invisible in the
    seed parity corpus, whose pairs never differed by verb alone.
  - a **second obligation** in one statement is not one claim (claim-format: "shall …
    and shall …" is two claims, split before filing) → lint `multiple_obligations`,
    unstructured. This is the finding the external corpus produced, and it immediately
    caught one of our own seed claims (REQ-0004, since split).
- **T1.7 Subject registry** — project-scoped YAML mapping surface forms → canonical
  subject id (``the system``/``the device``/``the product`` → ``SYSTEM``). The registry
  is a reviewed, versioned artifact; unregistered synonyms are **not** collapsed
  (Tier-2 may propose).
- **T1.8 Conditions** — guard clauses (``when``/``while``/``if``/``unless``/``during``)
  extract to predicate slots; ``unless`` negates the guard; conditions get **sorted**
  for assembly. A claim that *gains* a condition is a different claim (a trap case).
- **T1.9 Assembly + digest** — fixed schema
  ``{action, conditions, exception, modality, operator, qualifier, raw, subject,
  temporal, unstructured, unit, value}``: every key always present (null when unset),
  **valid JSON with sorted keys**, no insignificant whitespace, canonical number format
  → canonical form bytes →
  ``address = sha256("req.claim.v1\0" || canonical_form_bytes)``.
  - **`quantifier` is deliberately NOT in the form yet.** The parser does not derive
    one, and an always-null field would advertise coverage the rules cannot deliver; it
    stays authored-only and unverified (the diagnostic footer says so). The hazard today
    is nil — a quantifier word remains in the action text, so two statements differing
    there still differ — but if an extraction rule ever lands it **must** enter the form
    in that same change, or claims differing only in quantifier would collide. That is a
    rules bump and a re-canonicalization event.
  - The bytes are **parsed by consumers** (porcelain, audit tooling, any
    re-implementation), so validity is a contract, not a description: a test asserts the
    stored form parses as JSON with exactly the documented keys, in sorted order.

**Example that must collapse** — ``The system shall respond within 200 ms`` ≡
``The system shall respond no later than 200 ms`` → subject ``the system``, action
``respond``, operator LE, value 0.2 s, qualifier empty → same address. The inverse
trap: LE vs LT — ``within`` vs ``in fewer than`` — never collapse.

### 5.1 Writing and verifying the stored fields

`req canon --fill` is the only writer of `canonical.form`/`address`, and it **refuses**
a claim whose parse disagrees with its authored slots: an address over a parse that
contradicts the claim would enshrine the contradiction in the audit artifact. A refusal
is reported and exits 3 — never folded into success.

`req canon --verify` is in the gate, and it checks three things per claim: the stored
form equals what the current parse produces, the stored address recomputes from the
**stored bytes**, and the rules version matches. Exit codes are the three states:
**0** verified, **1** mismatch (a hand-edited digest, or a stale form), **3**
candidate (unfilled, or filled under other rules). A candidate is what a rules bump
produces by design — re-canonicalization candidates requiring re-ratification, never a
silent rehash — so a rules bump without a re-fill stops at the gate rather than
shipping a corpus whose addresses no longer mean what the rules say.

**Example that must NOT collapse** — ``The system shall not exceed 2.5 A of
standby current`` vs ``… of surge current``: the qualifier differs → different
addresses, no merge.

**Example that must NOT collapse** — ``… within 200 ms`` (LE) vs ``… in fewer than
200 ms`` (LT): different operators, different addresses.

**Example that must NOT collapse** — ``shall`` vs ``may``: `MODALITY_WEAKENING`.

## 4. Unstructured fallback

A statement that cannot parse into the schema keeps a **raw canonical form** (post-T1.0
– T1.2 text) with an explicit ``unstructured: true`` marker and its own address.
Unstructured claims participate in Tier-2 proposals and in lint; they never feed
structural merge-direction analysis, because guessable structure is exactly what a false
merge requires.

## 5. Freeze and upgrade policy

The canonical-form bytes are **stored on the claim object**, so every address is
recomputable from stored bytes: `sha256(stored canonical form) == stored address`.
Rules carry a version; rule upgrades do **not** rewrite ratified history. A claim
ratified in a signed Baseline keeps its (canonical form, address) pair; re-normalization
under new rules yields a *candidate* that requires re-ratification, never a silent
mutation — an address that changes between runs would make every Baseline unauditable.

## 6. Benchmark — the gate that turns the thesis into a number

### 6.1 Corpora

1. **Seed parity set** (`bench/corpus/parity.yml`) — equivalent pairs, the adversarial
   trap set (minimal meaning-changing edits: SHALL↔MAY, ``0.2 s`` vs ``0.25 s``,
   ``within``↔``fewer than``, an added condition, a swapped subject, ambiguous
   ``under``, a false-vocabulary pair) and this project's own dogfood claims. Labels
   were written from the spec, never from the tool.
2. **External corpus v1** (`bench/corpus/external-v1.yml`, built by
   `tools/corpus/build_external.py`) — real requirement prose written by *other*
   projects: StrictDoc's own L1 requirements and Doorstop's own requirements, pinned by
   commit SHA. Pairs are labelled **by construction** (the transformation applied),
   never by the canonicalizer, so the labels are an independent oracle. Tier-1-scoped:
   equivalence uses meaning-preserving normalizations only; paraphrase, voice and word
   order are Tier-2 and out of scope here.
3. **Natural duplicate run** — whole real documents; measures collapse of true
   duplicates vs near-duplicates. Not yet implemented.
4. **Not yet sourced** — Sphinx-Needs' bundled examples are test fixtures without prose
   (and its ``.. need::`` examples were not reachable in the restructured docs); the
   public PROMISE NFR set is not imported yet.

### 6.2 Labeling

Minimum two independent labelers, blind to tool output; disagreements adjudicated by a
third. Labels are versioned YAML in the repository.

### 6.3 Metrics and gates

| Metric | Definition | Gate |
|---|---|---|
| FMR (false-merge rate) | collapsed / all Not-equivalent pairs | **= 0.0 — hard gate** |
| Structural coverage (Not-equivalent) | pairs where *both* sides parsed to a structured form | reported per corpus; a pair that cannot be structurally checked is **never** counted as verified |
| FSR (false-split rate) | split / all Equivalent pairs | reported; v0 soft target ≤ 0.30 |
| Trap-class FMR | per adversarial class (LE/LT, modality, unit, condition, subject) | = 0.0 per class |
| Direction accuracy | classification on a direction gold set | ≥ 0.95 on structural cases |
| Tier-2 stability | provider output flip rate over ≥ 20 identical runs | ≤ 2% or the provider is not review-trustworthy |
| Tier-2 precision/recall | proposals vs labels | reported, never auto-applied |

The harness reports **PASS / FAIL / UNVERIFIABLE** as three distinct outcomes — a broken
corpus or harness is never accidentally a pass (the same principle as ADR 0005).

#### Recorded runs (append-only)

- **Rules v2** (parity, external-v1, external-v2) — all PASS. The v2 rule work was
  driven by the v1 findings: prohibition via "never", multi-obligation detection,
  qualitative-predicate classification, and a precise comparator lint.
  - **external-v2** (405 pairs; adds the predicate-swap class) — **FMR 0.0 over 227
    meaning-changing mutations**, FSR 0.0, 18 structurally unverifiable.
    *Before the predicate-span fix, this same corpus reported **58 false merges**
    (25.6%)* — a silent-merge bug the seed parity corpus and external-v1 both missed
    because neither varied the verb alone. The corpus was extended, the bug appeared,
    the bug was fixed, and the number went to zero. That is the falsification loop
    working as designed.
  - **external-v1** — FMR 0.0, FSR 0.0 with rules v2 (was FSR 0.58% at rules v1: the
    multi-clause pair is now correctly an out-of-scope statement carrying a
    `multiple_obligations` lint, not a silent split).
  - **parity** — FMR 0.0, FSR 0.0; 1 structurally unverifiable pair (`N-007`, whose
    both sides are unstructured by construction — reported, not hidden).

#### Recorded runs (append-only)

- **Seed parity corpus**, rules v1 — FMR 0.0 (7 traps), FSR 0.0 (5 structured
  equivalents), 3 Tier-2 candidates:
  `bench/results/parity-rules-v1.json`.
- **External corpus v1**, rules v1 — 338 pairs from other projects' real requirements,
  labelled by construction: **FMR 0.0 across 160 meaning-changing mutations, with 0
  structurally unverifiable pairs** (the pass is not vacuous) and **FSR 1/173 =
  0.58%**: `bench/results/external-v1-rules-v1.json`. Two findings, recorded rather
  than papered over:
  1. the single split is a **multi-clause** statement ("…shall allow… NOTE: …shall
     include…"). The transform changed the first modal, but the second obligation sits
     in the unparsed tail, so a meaning-preserving change there splits. Tier-1 parses
     one obligation per claim and does not yet **decompose clauses** — the next rule
     candidate, and a Tier-1 decomposition rather than a Tier-2 proposal.
  2. 16 `untestable_comparator` lints are **false positives on standard names**
     ("IEC 61508"): the word before the number is a name, not a comparator. This is the
     safe direction (a lint is noise, never a merge), but it erodes trust in the lint;
     the rule should require a comparator-*shaped* word, not merely a non-filler one.

### 6.4 Tier-2 provider funnel (if/when the gate passes)

Deterministic content addressing is O(1) per claim; semantic *proposals* are pairwise and
must be funneled, not fused:

```
Tier-1 exact-duplicate pass (free)
  → deterministic blocking (shared subject/attribute/value-class)
  → embedding shortlist (local, offline-capable)
  → provider adjudication on shortlist only
  → human ratification
```

Provider contract (external, pluggable, optional — e.g., a hosted calibrated model such
as TypeSafe's Jev, or local open weights for air-gapped deployments):

- **Equivalence** — ``Choice(equivalent | related | unrelated)`` with `confidence`
  (Choice carries confidence; a bare binary probability does not, so prefer Choice).
- **Contradiction** — binary proposal with confidence.
- **Direction** — ``Choice(modality_weakening | numeric_loosening | scope_divergence |
  cosmetic)``.
- **Achieveability lint** — ``Score`` over a testability rubric.
- **Provenance** — the provider's model/version string recorded on every proposal edge
  (the service returns one, e.g. ``model: jev-1.13.0`` — store it verbatim).
- **Air-gap rule** — the provider is a *plugin*; cloud providers are strictly optional
  and never required for any claim operation; local models must suffice offline.

Cost note: 5000 claims → ~12.5M naive pairs; the funnel must keep provider calls to
shortlisted candidates only (low thousands), or the design is economically dead.

## 7. Implementation constraints

- The core language is **Rust** (ADR 0009, superseding the Python decision, ADR 0008).
- **Tier-1 is stdlib-shaped with a small pinned crate set**: exact decimals via
  `rust_decimal` (pure Rust, verified current 2026-09); hashing via the stdlib's
  `sha2` equivalents or stdlib-facilitated hash equivalents as the lockfile decides;
  reading the claim files uses a *maintained* YAML crate pinned at first lockfile —
  `serde_yaml` is explicitly deprecated (verified: newest release is `0.9.34+deprecated`,
  2024-03-25). The two successor candidates named 2026-10-07 were evaluated:
  **`yaml_serde`** — the YAML organization's own serde_yaml continuation — is the
  *primary*: org-maintained (published by Ingy döt Net), `indexmap` insertion-order
  maps (order-preserving, which our deterministic re-writes lean on), small surface
  (serde + indexmap only, ~72 KB), MSRV 1.82, 2.86M downloads with 2.18M in the last
  90 days — the de facto ecosystem successor. **`serde-saphyr`** is the *fallback*:
  9.3M downloads (6.9M recent), pushed 2026-10-06, `trustpub_only` verified CI
  publishing, panic-free parsing with comment/tag capture — but a heavier
  dependency tree (num-traits, annotate-snippets, granit-parser, smallvec,
  encoding_rs_io) and an opt-in `include` fs feature that must stay disabled.
  MSRV turned out near-moot as a differentiator: `gix` 0.88 floors the workspace at
  rustc 1.88 regardless. Final pick = round-trip determinism test on the seed corpus
  at first lockfile, gated by `cargo-deny`/`cargo-vet`. Git access goes via
  **`gix`** (pure Rust — keeps libgit2 FFI out of the qualified surface; 50.5M
  downloads, 12.1M recent, verified 2026-10-07); `git2` + `vendored-openssl` is the
  documented fallback if HTTPS/SSH transport needs force us there.
- **Tier isolation is type-enforced**: `Canonical` has no constructor reachable from
  Tier-2 output — ADR 0007's asymmetry is guaranteed by the compiler, not by review.
- **Property tests are the rules harness**: the corpus lives as data
  (`bench/corpus/parity.yml`), and `req bench` runs the parity + adversarial trap sets
  as property tests (cf. `rust_decimal`'s own `proptest` feature).
- **Banned paths in the Tier-1 module**: no `f32`/`f64` math; no `HashMap` iteration
  where the address is computed — sort keys or use `BTreeMap` (the same footgun I
  earlier named only Go's; Rust's `std::HashMap` uses randomized seeds too); no
  locale-dependent formatting anywhere in the canonical path.
- **Toolchain/Unicode pin replaces the interpreter pin** (ADR 0008→0009): MSRV +
  lockfile + the `unicode-normalization` crate revision are part of rules' provenance;
  a bump is a Tier-1 rules bump (candidates needing re-ratification, never a silent
  rehash).
- **WASM target for the porcelain's browser surface**: the same canonicalizer core via
  `wasm-bindgen`, so no second Tier-1 implementation ever gets written for the PM UI.
- **Stateless and offline**: no model call anywhere in the canonicalizer crate;
  providers stay a *pluggable process boundary* (ADR 0009).
- Deterministic output across runs, platforms, locales — bit-for-bit.
- Layout: `src/canon/`, `bench/corpus/`, `bench/results/`, command: `req bench`.
- **Toolchain installed 2026-10-07**: rustup minimal profile; `cargo`/`rustc` 1.99.0
  (channel stable, 2026-09-28), canary compile verified exit 0. The workspace MSRV pin
  follows the real dependencies' floors (`gix` 0.88 → rust 1.88); set `rust-version` in
  Cargo.toml at scaffold time, per the dependency-pin record above.
