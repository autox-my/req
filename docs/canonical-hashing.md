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
| Comparator {LT, LE, EQ, GE, GT, NE, IN} | Author, timestamps, status |
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
  - ``within`` / ``not exceed`` / ``no later than`` / ``at most`` / ``up to`` → **LE**;
    and, since rules v9 (R-1), the negated upper bounds ``no more than`` / ``not more than`` /
    ``no greater than`` / ``not greater than`` → **LE**, and ``not fewer than`` → **GE** (the mirrors of the lower-bound pair
    below; the bare ``more than`` / ``greater than`` matched *inside* them and read an upper
    bound as a lower one, so a loosening was reported as a tightening). The longest phrase wins.
  - ``less than`` / ``fewer than`` / ``in less than`` / ``in fewer than`` → **LT**  ← *the classic trap: LE and LT are different claims*
  - ``at least`` / ``no less than`` / ``not less than`` / ``no fewer than`` / ``not fewer than`` → **GE** −
    ``greater than`` / ``more than`` / ``exceeds`` → **GT**
  - ``equal to`` / ``exactly``, or bare ``is`` between bound and value → **EQ**. Ambiguous comparators ("fast",
    "as soon as possible") → never guessed → lint `untestable_comparator`. The lint
    fires only for a word that could plausibly *be* a comparator (a curated list): a
    standard's name before a number ("IEC 61508") is not one, and linting it produced
    16 false positives on real external prose. Precision costs nothing here — an
    unresolved comparator is never guessed anyway, so the claim cannot collapse either
    way; only the lint's credibility was at stake.
  - **`IN`** (rules v7, ADR 0030) — an **enumerated** "one of …" is a set, not a bound.
    The members are lower-cased and sorted, and ride in the `value` slot as a JSON array, so
    no other claim's bytes move. Four shapes read, and only these: a bracketed list
    (`one of {a, b, c}`), a colon header (`one of the following: a, b or c`), a counted
    header (`one of the four sources, a, b, c or d`) and a bare list with its `or`
    (`one of a, b or c`). A stated count must equal the members, or the claim is unstructured
    with `membership_count_mismatch` (a wrong count is a dropped member). A member that holds a
    conjunction, a duplicate member, or fewer than two members is `membership_unclear` and
    unstructured. The noun the list is about ("standards") and any clause after the last member
    ("and no other standard") are identity, in `qualifier`, joined with ` | `; the article, the
    count and "following" are not. A "one of" with no enumeration (`one of the target market
    countries`) parses exactly as before — no delimiter, no set — and still derives the
    quantity `1`, which is a known wrinkle left alone because the near-miss corpora hold dozens
    of that shape and it has never merged anything. Direction: a member removed narrows, added
    widens (swapped, or a set against a non-set, is `scope_divergence`); for a prohibition the
    set is what is forbidden, so the senses swap.
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
  **A leading condition** (rules v8, ADR 0038) — a sentence that *starts* with a guard word
  (``when``/``while``/``if``/``unless``/``during``, and ``where`` in this position only) reads
  ``<guard> <condition>, <subject> <modal> …``: the condition is what lies between the guard word and
  the **last comma before the subject**, the subject is the noun phrase after that comma, and the
  form is the one the same obligation has with its condition last. Only an unambiguous sentence is
  read: with no comma separating the condition from the subject, or an empty condition or subject,
  the statement parses as it did before (the whole clause in the subject), not guessed.
- **T1.9 Assembly + digest** — fixed schema
  ``{action, applicability, conditions, exception, modality, operator, qualifier, raw,
  subject, temporal, unstructured, unit, value}``: every key always present (null when
  unset), **valid JSON with sorted keys**, no insignificant whitespace, canonical number
  format (``value`` is a decimal string, or — for the operator ``IN`` only — a sorted array of
  member strings) → canonical form bytes →
  ``address = sha256("req.claim.v1\0" || canonical_form_bytes)``.
  - **`applicability` is an input, not a derivation.** The statement carries no formal
    selector syntax yet (ADR 0003 defers it), so the form takes the sorted authored list.
    Rules v5 — before that the form had **no applicability field at all**, which meant two
    claims differing only in their selector hashed to the same address.
  - **A slot the rules cannot carry is refused, never ignored.** `quantifier`, `exception`
    and `temporal` have no derivation, and since `req.claim/2` (ADR 0022) no claim key either:
    authoring one is a parse error that names it — the honest answer to "the rules cannot
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

### 5.1 Deriving at load, and checking what is load-bearing

Nothing derived is stored in a claim file (`req.claim/2`, ADR 0022): `canon::load_claims`
derives each claim's form, canonical bytes and address from its `statement` and authored
`applicability` every time the corpus is read, so no command writes a `canonical.form` or an
`address` and none can be stale or hand-written. What an author wants held is stated as a
**pin** (`pins:` — modality, subject, operator, value, unit), judged against the derived form:
it agrees, or it is a flag (a contradiction, or a value the parse does not carry). `req canon`
reports each verdict and exits 0; the release gate's `pins_hold` fails on a flagged pin.

A rules bump therefore moves derived addresses at the next load rather than leaving stored ones
stale. The check that catches it is **drift against the signed manifest**: `req baseline
--verify` re-derives every attested claim's address and fails on any that moved, which is the
re-canonicalization candidate of §5 made visible per claim — never a silent rehash.

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

The canonical-form bytes are **derived from the claim** at load (ADR 0022), so every address
is recomputable from the claim alone: `sha256("req.claim.v1\0" ‖ canonical bytes) == address`,
and the address a signed Baseline **stores** per claim is what a later derivation is compared
against. Rules carry a version; rule upgrades do **not** rewrite ratified history. A claim
ratified in a signed Baseline keeps its attested address; re-normalization under new rules
yields a *candidate* — drift against that manifest — that requires re-ratification, never a
silent mutation: an address that changed unnoticed between runs would make every Baseline
unauditable.

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
   **Version 2** (`bench/corpus/direction-v2.yml`, `build_direction.py --version 2`; S-1, ADR 0040
   decision 5) is every v1 pair plus the **mirrors** of the modality/numeric pairs (`a` and `b`
   swapped, labelled with the opposite class — the mutation read backwards, ids suffixed `r`) and the
   membership mutations on enumerated `one of` sets. It exists to measure **false carries**: v1 holds 5
   `modality_strengthening` and 9 `numeric_tightening` pairs, so its zero false carries is vacuous. The
   pinned sources hold no enumerated `one of {…}` statement, so v2 carries **0** membership pairs
   (none was synthesised); v1 regenerates byte-identical from the same generator.
   **Version 3** (`bench/corpus/direction-v3.yml`, `build_direction.py --version 3`; R-2, ADR 0044
   decision 7) is every v2 pair plus two families whose pairs carry more than the single mutation v1 and v2
   have: **compound** pairs (two mutations on one statement: modality × bound, modality × guard, guard ×
   bound; each also swapped), labelled by the rule of ADR 0044 decisions 3–4 from the two mutations' own
   labels — a modality change beside a guard has no direction, otherwise a direction only when both axes
   agree — and **numeric mutations on a prohibition**, labelled with the swapped sense (decision 5). The
   sources hold no `shall not` statement with one bound, so the prohibition base is a source SHALL statement
   flipped to SHALL NOT (the base is synthesised; the label is the numeric mutation's, swapped). Only
   single-sentence statements with one quantity and no other negation enter the new families. v1 and v2
   regenerate byte-identical, and their records are unchanged.
   **Version 4** (`bench/corpus/direction-v4.yml`, `build_direction.py --version 4 --pure-zip requirements.zip`; G-1)
   is every v3 pair plus pairs from the numeric statements of **PURE** — A. Ferrari, G. O. Spagnolo, S. Gnesi, "PURE: A
   Dataset of Public Requirements Documents", RE 2017 (Zenodo record 1414117, DOI `10.5281/zenodo.1414117`, version 1.0,
   licence **CC BY 4.0**, per the record's `license` field; the corpus header names the authors, title, DOI and licence, as
   the licence requires). The 62 PDFs of `requirements.zip` (33,623,628 bytes, md5 `bc319fe28619f6290badff328ca159dd`; the
   header also records the sha256) are read through **`pdftotext` (poppler) 26.11.0** in its default reading order. The
   archive and the extractor are **generator provenance**, as the OCR pin is review provenance: the generator refuses any
   other `pdftotext` version or any other archive, naming both, before it fetches or runs anything; the core never runs it
   and `scripts/check.sh` never regenerates. The pool is every sentence that says "shall", is 300 characters or fewer after
   whitespace normalisation and carries a comparator phrase followed by a number (the generator's 17 comparator
   phrases, not `up to`, `exceed`, `maximum`, `minimum` or `or less`: 36 further short SHALL statements carry one, mostly
   capacities, which are domain comparators; the task's earlier 124 came from a broader regex whose remainder is not
   accounted for by name). Extraction noise is dropped by named, counted rules (a replacement character; a sentence
   holding a line that recurs on three or more pages of its document, i.e. page furniture; a hyphenated line end, which is
   not rejoined because that needs a dictionary; and two interleaved-table-column sentences from a list read by hand), an
   identical repeat of an earlier statement is kept once, and **every statement of the pool was read by hand** and its
   comparator classified as the obligation's own bound or as a bound on the *domain* (what is detected, handled or
   configured; a bound in the condition clause; a clause the SHALL does not govern). The second kind is excluded, never
   relabelled, in the generator's ordered `DOMAIN_COMPARATOR` list (7 statements), because there raising the number can
   oblige more and a "numeric loosening" label would be wrong; the hand-read list, not a rule, is the authority. The
   header counts each. No sentence is ever edited, and a PURE `a` is a sentence as extracted, not a verified
   requirement. The families are the existing ones — numeric loosening and tightening (and the LE-to-LT
   tightening) with mirrors, modality, compound, and numeric mutations on a **native** `shall not` statement (none is
   flipped) — with the bound found by its comparator phrase rather than by the sources' unit list, because PURE's units
   ("hours", "days") are not theirs and widening `NUMBER_UNIT` would change v1–v3. Labels are the mutation, never req's
   answer. PURE holds no `one of` enumeration, so `membership_widening` is unmoved and none is synthesised; the polarity,
   scope, unrelated and cosmetic families are not run on it. v1, v2 and v3 regenerate byte-identical.
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
| False contradiction | a pair that must never contradict (labelled equivalent, or a nested-set `numeric_*` / `membership_*` / `scope_*` direction pair) reported by the structural contradiction detector (ADR 0043) | **= 0 — hard bound**; the detector judges only once it holds on every corpus (`contradict::DETECTOR_ADMITTED`) |
| Contradiction recall | `polarity_flip` direction pairs whose earlier claim is a `SHALL`, found by the detector; the pairs it could not compare are counted | reported, never gated |
| Tier-2 stability | provider output flip rate over ≥ 20 identical runs | ≤ 2% or the provider is not review-trustworthy (measured by `experiments/tier2/`, not a core gate: REQ-0011 is retired, ADR 0021) |
| Tier-2 precision/recall | proposals vs labels | reported, never auto-applied |

**Why direction gates on the dangerous confusion, not on accuracy.** The earlier
definition made "≥ 0.95 on structural cases" the gate, which prices every misread the
same. It does not cost the same. Reading a tightening as a widening is wrong but nothing
was silently relaxed; reading a **loosening** as *no change* is how a weakened
requirement gets ratified — the direction analogue of a false merge. So the hard gate is
**dangerous confusions = 0**, and accuracy is reported against a 0.95 soft target,
exactly as FMR is hard and FSR is soft.

**False contradictions, beside FMR (ADR 0043).** The contradiction detector (`src/contradict.rs`) is the same slot
equality the fingerprint rests on, so it inherits the same asymmetry: a **false contradiction** — two statements of the
same meaning, or a nested-set direction pair (a bound moved, a member added, a condition added), reported as a pair no
value satisfies — is a defect nobody should have to look at twice, and the bound is **0**, measured over every pair labelled
equivalent and every `numeric_*` / `membership_*` / `scope_*` direction pair, with labels from the corpus and never from
the detector. Each record carries a `contradiction` block (`false_contradictions`, the comparable and not-comparable pairs
behind the zero, and `recall` over the `polarity_flip` pairs). The gate's `contradictions` policy FAILs an unrecorded
contradiction only while `DETECTOR_ADMITTED` is true, which is pinned to the records by `tests/contradict.rs`: a rules bump
or a corpus that breaks the bound fails CI until the constant is changed on purpose.

The harness reports **PASS / FAIL / UNVERIFIABLE** as three distinct outcomes — a broken
corpus or harness is never accidentally a pass (the same principle as ADR 0005). A
denominator that is empty is UNVERIFIABLE, not PASS: with no not-equivalent pairs the FMR
is **0/0**, which measures nothing, so a generator regression that dropped every mutation
cannot read as a clean gate. A definite defect still outranks the open question, exactly
as `req gate` lets Fail outrank Unverifiable.

#### Recorded runs (append-only)

- **Direction gold set v4, rules v9** (G-1, corrected by G-1-fix; `bench/results/direction-v4-rules-v9.json`; PURE, §6.1) —
  2679 pairs (v3's 1827 plus 852 from a pool of 63 PURE statements, 53 of them with exactly one numbered comparator
  phrase): FMR 0/2400 (308 structurally unverifiable), FSR 0/199, hard gate PASS, **0 dangerous confusions**, accuracy
  0.9772 over 2371 classified pairs (2317 correct), 0 false contradictions over 833 comparable pairs. No `RULES_V` bump;
  v1–v3 and every earlier record are byte-identical. Of the 31,682 sentences PURE's 62 PDFs yield, 4,487 say "shall",
  4,178 of those are 300 characters or fewer, 82 carry a numbered comparator phrase (a narrower reading than the task's
  broader 124: 36 more carry `up to`, `exceed`, `maximum`, `minimum` or `or less`, not taken). Of the 82, 9 were page
  furniture and 2 interleaved table columns (0 replacement characters, 0 hyphenated line ends), 1 an identical repeat
  (`2004 - rlcs` #589 of #377) and 7 **domain comparators** excluded by hand (`1999 - tcs` #487; `2002 - evla back` #162;
  `2004 - rlcs` #409, #410, #416; `2007 - eirene fun 7` #507; `2007 - ertms` #229), leaving 63; 5 of those yield no pair.
  The first run of this corpus (G-1, 972 pairs, support 154) had labelled `tcs` #487 ("detecting any deviations greater
  than 10%") and `evla back` #162 ("lag frames of less than 128") `numeric_loosening` for a change that obliges more:
  review found it, and the fix excludes rather than relabels.
  - **PURE pairs by family**: single mutations 188 (`MW` 56, `NL` 50, `NT` 50, `LT` 32, `MS` 0) + 188 mirrors; compound 234
    + 234 mirrors; numeric mutations on a native `shall not` 4 + 4 mirrors, from **2** statements (`1999 - tcs` #423,
    `2007 - eirene fun 7` #189), both read correctly in both senses. Of the 852, 754 are correct, 68 unclassifiable and 30
    mismatched, all harmless: three statements (`2007 - eirene fun 7` #667 and #675; `2008 - caiso` #83) whose scaled number
    is not the parser's value slot read `scope_divergence` (or `unrelated` for the LT pair), the same parse-coverage fact as
    v1's PROMISE pairs. **Never a loosening reported as no change or as the opposite.**
  - **false carry** (`direction.false_carry`; reported, `admitted` = count 0 and support ≥ 100):

    | weakening class | false carries | support (scored opposing pairs) | `admitted` in this record |
    | --- | --- | --- | --- |
    | `modality_weakening` | 0 | 273 (v3: 181) | yes |
    | `numeric_loosening` | 0 | **142** (v3: 17; v2: 9) | **yes** |
    | `membership_widening` | 0 | 0 | no — no pair exists |

    **`numeric_loosening` still reaches the floor on v4 after the exclusions: 0 false carries over 142 scored numeric
    tightenings** (154 before them). Read it for what it is. The 125 pairs PURE adds are not 125 samples: they are
    mutations of the number in about 45 statements the parser reads with a value slot (43 of the 50 obligations with one
    bound, plus the 2 native prohibitions; 4 further statements are unclassifiable and 3 read `scope_divergence`), each
    yielding several pairs (`NT`, `LT`, the mirrored `NL`, …), and the 63-statement pool comes from 12 documents, 26 of
    its statements from one. Every pair scales a bound by 1.25 or 0.8 over the comparator phrases PURE happens to use, the
    hand classification of "own bound" against "domain" is one reviewer's reading, and the measurement does not cover
    phrases or units that no source has. It is a public-corpus measurement of a rule req already has, not the
    external-corpora falsification of the bet.
  - **Admission is not changed by this run.** `ADMITTED_CARRIES` stays pinned to `direction-v2-rules-v<RULES_V>.json`
    (S-2): `numeric_loosening` is `admitted: false` there (support 9). Moving the measurement to v4 and admitting the class
    is an owner decision for a later unit.

- **Contradiction measurement, rules v9** (C-1; the `contradiction` block added to every `bench/results/*-rules-v9.json`;
  ADR 0043 decision 6) — **false contradictions 0 on every corpus**: parity 0 of 12 scored pairs (9 comparable, 3 not),
  external-v1 0/178 (150, 28), external-v2 0/178 (150, 28), external-v3 0/499 (383, 116), external-v4 0/499 (383, 116),
  direction-v1 0/533 (392, 141), direction-v2 0/548 (401, 147), direction-v3 0/992 (433, 559). **Recall** on the
  `polarity_flip` pairs whose earlier claim is a `SHALL`: 176 of 176 found, 0 not comparable, on each direction corpus
  (reported, not gated). The detector is admitted (`DETECTOR_ADMITTED = true`). Support is thin where it matters
  least to the claim: the direction corpora hold at most 46 `numeric_*` pairs (23 loosening, 23 tightening in v3) and no `membership_*` pair, so the
  nested-set bound is carried mostly by the equivalent and `scope_*` pairs. No `RULES_V` bump; the diff of each
  `*-rules-v9.json` is the new key and nothing else.

- **Direction gold set v3, rules v9** (R-2; `bench/results/direction-v3-rules-v9.json`; ADR 0044) — 1827 pairs
  (v2's plus 436 compound and 16 prohibition-bound pairs, each also swapped): FMR 0/1548 (240 structurally
  unverifiable), FSR 0/199, hard gate PASS, **0 dangerous confusions**, accuracy 0.9849 over 1587 classified
  pairs (1563 correct, v2's 1193 plus 370 new). No `RULES_V` bump: the classifier is not on the address path, and
  the v1 and v2 records are byte-identical before and after the change (every single-axis pair keeps its class).
  - **compound** (436 pairs: modality × bound 16 — all four sense combinations, the strengthened ones through
    the swapped mirrors, because no bounded source statement says "may" or "should" (11 say "should", none with a single bound; measured by review R-2-review-1) — modality × guard 404, guard ×
    bound 16): 370 scored, 362 correct, 66 unclassifiable. The 8 mismatches are the two PROMISE statements
    `P8:r375` and `P8:r380`, whose scaled number is not the value slot (the same parse-coverage fact as their
    v1 `numeric_*` pairs: reported `scope_divergence`, never a direction).
  - **prohibition bound** (16 pairs from 4 bases, **0 of them native `shall not` statements**: the sources hold
    none with one bound, so the bases are flipped SHALLs): 8 correct (the two bases whose number is the value
    slot, `P14:r600` and `P14:r605`, in both senses and both directions), 8 read `scope_divergence` (the same
    `P8` statements as above). The swapped sense is therefore measured on **two statements**, and by
    `tests/compare.rs`; do not read 8 correct as a population.
  - **false carry**: not recomputed as an admission input — it stays v2's (ADR 0040's note; v3's support is
    not single-axis).

- **Rules v9** — a negated upper bound is an upper bound (T1.5; R-1). Found while checking parser coverage for
  the numeric direction pairs: `no more than 3 seconds` derived `op GT`, because the table had `no less than`
  → GE but not its mirrors and the bare `more than` matched inside. `req compare` then called "no more than
  3 s" → "no more than 4 s" a `numeric_tightening`, a loosening reported as a tightening. Addresses did not
  falsely merge (the stray "no" stayed in the action). The direction gold set did not catch it because its
  generator had the same substring bug and labelled the same way, so the labels agreed with the parser's wrong
  reading: that was the oracle failing, not the classifier passing. Per corpus, never pooled, against the
  recorded v8 numbers:
  - **parity** — 31 pairs (the two new ones are hand-written from the spec: `E-012` "no more than" ≡ "at most",
    `N-019` "no more than" ≠ "more than"): FMR 0/19, FSR 0/9, 2 structurally unverifiable (was 29 pairs, 0/18, 0/8).
  - **external-v1/v2/v3/v4** — every count is identical to v8 (FMR 0/160, 0/227, 0/625, 0/1595; FSR 0/150,
    0/150, 0/389, 0/389), and **no pair's verdict moved**: the records differ from v8 only in `rules_v`. No external
    pair is named because none moved; those corpora are not evidence about this rule, the parity pairs and
    `tests/negated_upper_comparators.rs` are.
  - **direction gold set** (the generator was fixed: the longest matching phrase decides, the negated forms are
    listed, and a number after a backslash is not a quantity). v1: 1157 pairs (was 1159), FMR 0/878, FSR 0/199,
    hard gate PASS, 0 dangerous confusions, accuracy 0.9961 over 1016 classified pairs (1012 correct; 141
    unclassifiable, was 143). v2: 1375 pairs (was 1379), FMR 0/1096, hard gate PASS, 0 dangerous confusions,
    accuracy 0.9933 over 1201 (1193 correct; 174 unclassifiable, was 178). The regenerated files differ from the
    committed v8 ones in exactly two source statements and their mirrors: `D-PR-P6:r310-NL`/`-NT` (the
    `\92s` escape read as 92 seconds: no bound changes, dropped with their v2 mirrors) and `D-PR-P11:r545-NL`/`-NT`
    ("no more than 3 seconds": the two labels had been mutated the wrong way round, so each pair's `b` now
    holds the other's, and the mirrors likewise).
  - **false carry** (`direction-v2`): `numeric_loosening` 0 false carries, support **9** (unchanged from v8;
    admitted: no, support below 100). The fix does not move it: the two repaired pairs (`D-PR-P11:r545-NL/-NT`) and the
    two dropped ones were unclassifiable at v8 and stay so, so none of them ever fed the support. More support needs more numeric source statements, a different unit.
    `modality_weakening` stays 0 over 177; `membership_widening` 0 over 0.
  - **dogfood** — no claim in this repository holds `no more than`, `not more than`, `no greater than` or
    `not greater than` (`req canon`: 40 claims, 0 flags), so there is **no re-canonicalization candidate**; only
    `rules_v` 8 → 9 differs in the re-emitted manifests, which need the owner's re-signature and nothing else.
- **Direction gold set v2, rules v8** (S-1; `bench/results/direction-v2-rules-v8.json`) — 1379 pairs
  (v1's 1159 plus 220 mirrors, 0 membership): FMR 0/1100 (178 structurally unverifiable), FSR 0/199. Hard gate PASS,
  **0 dangerous confusions**, accuracy 0.9933 over 1201 classified pairs (178 unclassifiable); the 8
  mismatches are the 4 v1 `numeric_* → scope_divergence` coverage facts (two PROMISE statements whose
  scaled number is not the value slot) and their 4 mirrors. v1 is unchanged (1016 checked, 1012
  correct, 0 dangerous). The measured `false_carry` block (reported, not a verdict; `admitted` =
  count 0 **and** support ≥ 100):

  | weakening class | false carries | support (scored opposing pairs) | admitted |
  | --- | --- | --- | --- |
  | `modality_weakening` | 0 | 177 (`modality_strengthening`; was 1 scored on v1) | yes |
  | `numeric_loosening` | 0 | 9 (`numeric_tightening`; was 5 on v1) | no — support below 100 |
  | `membership_widening` | 0 | 0 (`membership_narrowing`) | no — no pair at all |

  Without the mirror family the `modality_weakening` support is 1 (mutation check). The numeric
  class cannot reach 100 from these sources: 17 labelled tightenings, 8 of which the classifier
  cannot parse (unclassifiable) and 4 read as `scope_divergence`. Seed and other-projects' prose only.
- **Rules v8** — a leading condition (T1.8, ADR 0038; issue #18). Found on an adopter's second
  pilot: "When a document is sent to a third-party provider, the provider shall be recorded …"
  read the whole clause as the subject, so rewording only the trigger moved the subject and
  `req compare` called the pair `unrelated`. Per corpus, never pooled, against the recorded v7 numbers:
  - **parity** — 29 pairs (the four new ones are hand-written from the spec: one equivalent,
    three not-equivalent): FMR 0/18, FSR 0/8, 2 structurally unverifiable (was 25 pairs, 0/15, 0/7).
  - **external-v1/v2/v3/v4** and the **direction gold set** — every count is identical to v7
    (FMR 0/160, 0/227, 0/625, 0/1595; direction hard gate PASS). 66 statements in v3, v4 and the
    direction set start with a guard word, but **none of them has a comma before its subject**
    ("When a game is ended the product shall …"), so none is read as a leading condition and no pair
    moved. The new rule has teeth only in the parity pairs and `tests/leading_condition.rs`; it is not
    an external result, and the common no-comma spelling is **not** covered by it (see ADR 0038).
  - **dogfood** — no claim starts with a guard word: `req canon` 0 flags, only `rules_v` 7 → 8
    differs in the re-emitted manifest. The Baseline therefore needs the owner's re-signature and
    nothing else (no re-canonicalization candidate).
- **Rules v7** — the `IN` operator (T1.5, ADR 0030; issue #17). Found on an adopter's statements
  ("one of the four sources, …"), which parsed `value "1"` with the members as free text in the
  qualifier: reordering them moved the address and dropping one was only `scope_divergence`.
  Per corpus, never pooled, against the recorded v6 numbers:
  - **parity** — 25 pairs (the eight new ones are hand-written from the spec: two equivalent,
    six not-equivalent): FMR 0/15, FSR 0/7, 2 structurally unverifiable (was 17 pairs, 0/9, 0/5).
  - **external-v1/v2/v3/v4** and the **direction gold set** — every count is identical to v6
    (FMR 0/160, 0/227, 0/625, 0/1595; FSR 0/150, 0/150, 0/389, 0/389; direction hard gate PASS,
    0 dangerous confusions, 1012 of 1016 classified correct): none of those corpora holds an
    enumerated "one of", so no pair moved. The new rule therefore has teeth only in the parity
    pairs and `tests/membership.rs`; it is not an external result.
  - **dogfood** — no claim holds an enumerated "one of": `req canon` 0 flags, all 18 attested
    hashes of the re-emitted manifest equal the signed ones; only `rules_v` 6 → 7 differs. The
    Baseline therefore needs the owner's re-signature and nothing else (no re-canonicalization
    candidate).

- **Dogfood split (M4-1), rules v6** — no rule moved; REQ-0007's second obligation was split
  into the new REQ-0023 (owner ruling 2026-11-04) and REQ-0015's rationale assumption was
  re-worded, so only manifest fields moved (**re-canonicalization candidates**, §5): REQ-0007
  address ``7689bfe6d984`` → ``d5f74f33838e`` and ``tests_hash`` ``7c2829a1a0f8`` →
  ``f632fd83e5bd`` (its ``rationale_hash`` and ``relations_hash`` did not move);
  REQ-0015 ``rationale_hash`` ``b330399ee6ed`` → ``a52e24f7a684`` (its address and
  ``tests_hash`` did not); REQ-0023 is new (address ``f33145e56369``). The other fifteen manifest
  entries are byte-identical to the signed ones, and no Decision names REQ-0007's old
  address (``DEC-0001`` ratifies REQ-0016). REQ-0007 is still derived unstructured; REQ-0023 is
  derived *structured* (`SHALL` on the subject "the decision boundary", no lint). The corpus
  is 22 claims. The Baseline is re-emitted unsigned and awaits the owner's signature.

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
- **Dogfood re-wording (M1-4), rules v6** — no rule moved; three claims were re-worded and
  one split, so only claim addresses moved (**re-canonicalization candidates**, §5):
  REQ-0004 ``f545c40177ba`` → ``ddc6992dd05b`` and REQ-0021 ``47dafd1eaad1`` →
  ``fc5e269b978e`` (the head "semantic layer" dropped from the statement; the subject is the
  registered ``REQ_TOOL`` again), REQ-0020 ``1ffac8e3e4e2`` → ``b455b84714c2`` (structured
  again: "exactly once" moved to the new REQ-0022, ``01e9dc781ad7``). `req canon` 0 flags,
  `--verify` 22 verified / 0 candidates / 0 mismatches; the other eighteen did not move, and
  no bench pair moved either (every `bench/results` record is byte-identical). No Decision
  names any of the three addresses (`DEC-0001` ratifies REQ-0016), so none is orphaned.
  The Baseline `baselines/air-gapped.json` is re-emitted and awaits the owner's signature.
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

*Status (ADR 0021, 2026-11-03): the funnel below is research and lives in `experiments/tier2/`; the core
keeps only the single-pair `req propose <a.yml> <b.yml>` seam, whose answer is an unratified proposal.
The design that follows is unchanged.*

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
