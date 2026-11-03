# Canonical hashing of claims — spec v0

Status: proposed. The falsifiable bet. Implements docs/adr/0001-the-unit-is-a-claim.md,
docs/adr/0004-model-proposes-human-disposes.md, and docs/adr/0007-collapse-tiers.md.

## 1. The falsifiable claim

Deterministic normalization of natural-language claims produces **near-zero false merges
and substantial false splits**. Everything else in the design can be built on that; if
the measured false-merge rate is not zero, the canonicalizer is *unsafe* and nothing may
be built on it — a false merge silently deletes a requirement, which is the one failure
this design exists to prevent (see docs/adr/0004-model-proposes-human-disposes.md).

(The "address" and "identity" below mean the **meaning fingerprint**, not the claim's identity, which
is its `claim_id`: docs/adr/0020-the-address-is-a-meaning-fingerprint.md, accepted.)

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
  ``should`` → ``SHOULD``. ``may not``/``may never`` are deliberately **not**
  resolved: the *scope* of the negation is ambiguous (``is not permitted to X`` vs
  ``is permitted to not X``), so they lint `ambiguous_modality` and stay unstructured
  (rules v4 — a ``shall``→``may`` mutation collapsed on the widened external corpus
  before this). The absorbed form is unaffected: ``may not exceed 200`` records the
  negation on the *comparison* → ``MAY`` with **LE**. The **earliest** match by
  position wins, and at the same position the longest phrase wins: table order must
  never decide position, or a later ``shall never`` shadows an earlier ``shall``.
  ``will`` is deontic only contextually → never guessed → lint `ambiguous_modality`.
  Modality is never cross-ran (``SHALL`` ≠ ``MAY``).
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
  (Tier-2 may propose). **An alias matches only when it is the whole subject span** (rules
  v6): the pre-modal words are resolved as one phrase, so ``the req tool`` is ``REQ_TOOL`` but
  ``the req tool's semantic layer`` (possessive stripped by T1.1: ``the req tool semantic
  layer``) is not a registered alias and stays a literal subject carrying every word. Through
  rules v5 the longest alias covering a *prefix* won and the leftover words fell in no slot —
  ``the req tool's semantic layer`` and ``the req tool's audit layer`` were both
  ``REQ_TOOL``, a false merge in this repository's own REQ-0004. An alias is a collapse
  decision for the registry's owner; the parser never extends one.
- **T1.8 Conditions** — guard clauses (``when``/``while``/``if``/``unless``/``during``)
  extract to predicate slots; ``unless`` negates the guard; conditions get **sorted**
  for assembly. A claim that *gains* a condition is a different claim (a trap case).
- **T1.9 Assembly + digest** — fixed schema
  ``{action, applicability, conditions, exception, modality, operator, qualifier, raw,
  subject, temporal, unstructured, unit, value}``: every key always present (null when
  unset), **valid JSON with sorted keys**, no insignificant whitespace, canonical number
  format → canonical form bytes →
  ``address = sha256("req.claim.v1\0" || canonical_form_bytes)``.
  - **`applicability` is an input, not a derivation.** The statement carries no formal
    selector syntax yet (ADR 0003 defers it), so the form takes the sorted authored list.
    Rules v5 — before that the form had **no applicability field at all**, which meant two
    claims differing only in their selector hashed to the same address.
  - **A slot the rules cannot carry is refused, never ignored.** Authoring `quantifier`,
    `exception` or `temporal` (no derivation *and* no field) is reported by `req canon`
    and makes `req canon --fill` refuse that claim: the honest answer to "the rules cannot
    represent this" is to say so, not to hash a form that drops what the author wrote. If
    a derivation ever lands, the field must arrive in the same change — a rules bump.
  - The bytes are **parsed by consumers** (porcelain, audit tooling, any
    re-implementation), so validity is a contract, not a description: a test asserts the
    stored form parses as JSON with exactly the documented keys, in sorted order.

- **T1.10 Coverage invariant** (rules v6) — after a structured parse, **every word
  position of the normalised statement must be attributed to a slot**: the subject (the
  whole pre-modal span), the modality phrase (with the absorbed negation and comparative
  of ``shall not exceed``), the comparator phrase, the value token (a digit run or a number
  word) and its unit, the guard clauses (T1.8), the action, or the qualifier — or be a
  **stopword**. Any unattributed position makes the claim **unstructured**: its ``raw`` is
  the normalised statement (§4, the way REQ-0007 is addressed) and the lint
  `uncovered_content` names the uncovered words in statement order
  (``uncovered_content: once, on, outgoing, side, of, its, origin, claim``).
  - **Positional, not token-set.** A word is covered because of the slot that holds *that
    position*, never because its spelling appears somewhere in the form: a dropped
    ``relation`` after ``serialize each relation exactly`` is uncovered although
    ``relation`` is in the action. A numeric token that did not resolve to a value
    (``200-300``), or whose attached suffix did not resolve to a unit (``5foobarbaz``),
    does not hold its position either: the text it carried left the form.
  - **Stopwords: ``a``, ``an``, ``the``, and nothing else.** Every stopword is a word that
    can be swapped or deleted without moving the address, so the list is a false-merge
    surface. Prepositions are not stopwords (``before``/``after``, ``on``/``off``). Widen
    the list only when a recorded mutation case shows that a substitution cannot change
    meaning.
  - **What it does not prove.** The invariant proves that no word is dropped. It does
    **not** prove word order is kept: slots that hold text (subject, action, qualifier,
    conditions) keep their words in order already, but two statements that put the same
    words in different slots is a question for the slots, not for coverage.
  - **Evidence.** REQ-0020 (``exactly`` taken as the ``EQ`` comparator, no value follows,
    every word after it in no slot: ``outgoing`` ≡ ``incoming``) and REQ-0004 (the prefix
    subject match above) were two false merges in this repository's own corpus that no
    benchmark mutation reached, because the v1–v3 mutations never touch a trailing clause
    or a subject's head. The seed traps ``N-008``/``N-009`` and the ``external-v4``
    any-position and possessive-head classes now do.

**Example that must collapse** — ``The system shall respond within 200 ms`` ≡
``The system shall respond no later than 200 ms`` → subject ``the system``, action
``respond``, operator LE, value 0.2 s, qualifier empty → same address. The inverse
trap: LE vs LT — ``within`` vs ``in fewer than`` — never collapse.

### 5.1 Writing and verifying the stored fields

`req canon --fill` is the only writer of `canonical.form`/`address` into an existing claim —
a new corpus's starter claim is derived by `req init` through this same path, never typed —
and it **refuses** a claim whose parse disagrees with its authored slots: an address over a
parse that contradicts the claim would enshrine the contradiction in the audit artifact. A
refusal is reported and exits 3 — never folded into success.

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
2. **External corpora v1–v3** (`bench/corpus/external-v{1,2,3}.yml`, built by
   `tools/corpus/build_external.py`) — real requirement prose written by *other*
   projects, pinned by commit SHA, extended only at the front (v1 and v2 stay, so their
   recorded runs remain reproducible). Pairs are labelled **by construction** (the
   transformation applied), never by the canonicalizer, so the labels are an
   independent oracle. Tier-1-scoped: equivalence uses meaning-preserving
   normalizations only (emphasis, modality synonym, case/punctuation, digit grouping,
   exact power-of-ten unit scaling); paraphrase, voice and word order are Tier-2 and
   out of scope here. The generator *implements* all five classes, but not every class
   *emits*: see the measured counts below. Sources:
   - **StrictDoc** — its own L1 requirements (67 statements).
   - **Doorstop** — its own requirements (18), which is where T1.2b came from: Doorstop
     writes ``**shall**``.
   - **PROMISE NFR** (v3) — the public natural-language NFR set: 969 statements across
     47 project ids, sampled **deterministically** (every 5th) because all of them ×
     five transforms would make the corpus and its review diff unreviewable. 24% of
     them carry numeric bounds, but **the unit-scale class emits no pair in any external
     corpus**: its pattern matches only ``N ms``, ``N kN`` and ``N kHz`` (the exact-factor
     units of T1.4), and no source statement is written that way — the unit-bearing ones
     say ``seconds`` or ``s``, which are already base units. Measured per class, as
     emitted/attempted, by ``external-v4``'s header (the same sources as v3): unit scale
     (US) 0/279, digit grouping (NG) 4/279, emphasis (EM) 13/279. The unit-factor rules
     therefore have **no external false-split evidence**; the only measurement of them is
     the hand-written parity pairs (``E-002``, ``E-005``). The class is not removed: it
     stays in the generator so a source that carries one is exercised, and its count is
     visible rather than assumed. Giving it input (for example ``N seconds`` →
     ``N000 ms``) would be a new equivalence class and a new corpus version.
3. **Direction gold set** (`bench/corpus/direction-v1.yml`, built by
   `tools/corpus/build_direction.py`) — the same three sources, each statement carried
   through one mutation per direction class, and the `direction:` label **is** the
   mutation. It answers "what kind of change is this?", and it is labelled by
   construction for the same reason the identity corpora are.
4. **Natural duplicate run** — whole real documents; measures collapse of true
   duplicates vs near-duplicates. Not yet implemented.
5. **Evaluated and excluded — Sphinx-Needs.** Its own documentation uses standard
   Sphinx directives (``versionadded``, ``note``, ``automodule``) rather than its own
   needs; its test roots contain no ``.. need::`` directives; and its demo project's
   29 ``swreq`` needs carry a title plus attributes but **no statement body** (e.g.
   ``.. swreq:: Lane Marking Detection Algorithm``). Those are titles, not requirement
   statements, so they would have made the corpus look wider while adding nothing a
   canonicalizer can parse. Recorded here as a negative result rather than silently
   dropped.

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
| Direction accuracy | correct / classified labelled pairs on the direction gold set | reported; soft target ≥ 0.95 |
| Dangerous direction confusion | a safety-relevant change reported as *no change*, or as the **opposite** direction | **= 0 — hard gate** (see below) |
| Tier-2 stability | provider output flip rate over ≥ 20 identical runs | ≤ 2% or the provider is not review-trustworthy |
| Tier-2 precision/recall | proposals vs labels | reported, never auto-applied |

**Why direction gates on the dangerous confusion, not on accuracy.** The earlier
definition made "≥ 0.95 on structural cases" the gate, which prices every misread the
same. It does not cost the same. Reading a tightening as a widening is wrong but nothing
was silently relaxed; reading a **loosening** as *no change* is how a weakened
requirement gets ratified — the direction analogue of a false merge. So the hard gate is
**dangerous confusions = 0**, and accuracy is reported against a 0.95 soft target,
exactly as FMR is hard and FSR is soft.

The harness reports **PASS / FAIL / UNVERIFIABLE** as three distinct outcomes — a broken
corpus or harness is never accidentally a pass (the same principle as ADR 0005). A
denominator that is empty is UNVERIFIABLE, not PASS: with no not-equivalent pairs the FMR
is **0/0**, which measures nothing, so a generator regression that dropped every mutation
cannot read as a clean gate. A definite defect still outranks the open question, exactly
as `req gate` lets Fail outrank Unverifiable.

#### Recorded runs (append-only)

- **Rules v6** — the coverage invariant (T1.10) and the whole-span subject (T1.7). Two
  false merges found by hand in the repository's own corpus (REQ-0020, REQ-0004), which
  every corpus then passed; per corpus, never pooled:
  - **parity** — 17 pairs (the two new traps ``N-008``/``N-009`` are hand-written from
    the spec): FMR 0/9, FSR 0/5, 2 structurally unverifiable.
  - **external-v1/v2/v3** — FMR 0/160, 0/227, 0/625; FSR 0/150, 0/150, 0/389. The price is
    visible and is not hidden: structurally unverifiable not-equivalent pairs rose
    12→22, 18→32, 68→92 and the Tier-2 candidates 17→28, 17→28, 89→110, because
    statements whose tail no slot held are now unstructured rather than silently
    structured (the structured-equivalent denominator fell 161→150, 161→150, 410→389).
  - **external-v4** (new; 2094 pairs, the v3 sources at the same pinned commits plus the
    any-position and possessive-head not-equivalent classes) — FMR 0/1595, FSR 0/389, 327
    structurally unverifiable, 110 Tier-2 candidates. It has teeth, measured by deleting the
    rule: without the coverage invariant it collapses 8 any-position pairs, and with the
    prefix subject match restored 178 possessive-head pairs. Those two numbers are
    mutation runs, not recorded results.
    Its header also records what the pinned commits do not: the digest of the registry alias
    list the possessive-registered class indexes into, and per-class emitted/attempted counts
    (the denominators; e.g. `US 0/279`, so a dropped class shows as a shrunken one).
  - **direction gold set** — hard gate PASS, 0 dangerous confusions, accuracy 0.9961
    over 1016 classified pairs (143 unclassifiable, was 109).
  - **dogfood** — all 21 claims re-filled under rules v6: `req canon` 0 discrepancy
    flags, `--verify` 21 verified / 0 candidates / 0 mismatches. Three addresses moved
    (**re-canonicalization candidates**, §5): REQ-0004 ``ddc6992dd05b`` →
    ``f545c40177ba`` and REQ-0021 ``fc5e269b978e`` → ``47dafd1eaad1`` (both now carry the
    literal subject ``the req tool semantic layer``), and REQ-0020 ``d04ccf3bb7cf`` →
    ``1ffac8e3e4e2``, which became **unstructured**. The other eighteen did not move.
  - These are **seed and self-corpus** results plus other projects' prose labelled by
    construction; the external falsification of the bet is still the headline measurement,
    and nothing here validates it.
- **Direction gold set, rules v5** — 1159 pairs (279 cosmetic + 880 mutations) from the
  three real sources: **accuracy 0.9962 over 1050 classified pairs, zero dangerous
  confusions**, FMR 0.0, FSR 0.0, 109 unclassifiable (coverage). Four mismatches remain,
  all `numeric_* → scope_divergence`, on two PROMISE statements whose scaled number is not
  the value slot — a **parser-coverage fact**, named in the report rather than smoothed
  over. The first run also found a real classifier defect: an operator-only move carrying
  no value ("within a single window" → "in fewer than a single window") was reported as
  divergence instead of a tightening, because the classifier bailed out whenever a value
  was absent. Fixed; the seven pairs it affected now classify correctly. Two of the
  generator's own labels were wrong too, and the gold set caught them: flipping the
  polarity of a statement that was already a prohibition (a doubled negation) and
  swapping a unit that was not the bound's.
- **Rules v5** — the authored-slot gaps closed (same class as the predicate-span bug):
  `applicability` entered identity, authored `conditions` are compared with the parse,
  and `quantifier`/`exception`/`temporal` are refused when authored. The new checks found
  **six authoring divergences in our own corpus**: five claims had put their *variant
  selector* in `conditions` (where the format wants a parsed guard), and REQ-0012's
  statement guard was unauthored. The selectors moved to `applicability`; the guard was
  written. All 21 claims re-filled under rules v5 — `req canon` 0 discrepancy flags,
  `--verify` 21 verified / 0 candidates / 0 mismatches, four corpora PASS.
- **Rules v3/v4** — derived fields written and enforced, then the corpus widened.
  - **external-v3** (1124 pairs; PROMISE NFR prose + StrictDoc + Doorstop; adds
    digit-grouping and unit-scale equivalence *transforms* — the unit-scale one emitted
    no pair, see §6.1 above) — at rules v3 it **FAILED**:
    FMR 2/625 (0.32%), both `shall`→`may` mutations that collapsed. The cause was a
    code-vs-spec divergence: the modality table mapped ``may not`` → ``SHALL-NOT``,
    which T1.6 never sanctioned, so a weakened obligation merged with a prohibition.
    At rules v4 (ambiguous negation scope is linted, never guessed) — **FMR 0.0 over
    625 mutations, FSR 0.0, 68 structurally unverifiable, 89 Tier-2 candidates**.
    Two of the false merges came from prose nobody here wrote, in a project nobody
    here has heard of, within minutes of the corpus existing. That is the whole point
    of widening the sources.
  - **`req canon --verify`** — 21 verified / 0 candidates / 0 mismatches (rules v4);
    the corpus was re-filled once per rules bump, and the gate would have stopped the
    merge otherwise.
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
