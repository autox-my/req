# 0040. A design satisfies a claim at a meaning, and a change of meaning is judged by its direction

Extends ADR 0019 (claims carry authored test links), ADR 0020 (the address is a meaning fingerprint) and ADR 0039 (structural
omissions). Nothing is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-11, GitHub PR #49). Asked for by the owner on 2026-11-11
("give me suggestion for design satisfaction. what is standard practice out there, if there is none, we should come up with
innovative solution"). Date proposed: 2026-11-11. This ADR decides the contract; the code follows in its own pull request, test
first, and only after the measurement in decision 5 exists.

## Context

req verifies a claim against code only through linked tests (ADR 0019). There is no design element and no *satisfies* link, so
nothing answers "does the architecture still meet this requirement after it changed?" — `docs/design.md` §4 pain 26, §6 item 7.

**Standard practice, read at source on 2026-11-11.**

- **OpenFastTrace** (`doc/user_guide/reference/`): an item declares the artefact types that must cover it (`Needs: dsn, utest`);
  a covering item names the covered one with an **authored revision** (`Covers: req~authentication~1`, a code tag
  `[impl->dsn~validate-authentication-request~1]`). A cover that names an old revision is `COVERING_WRONG_VERSION`. The revision
  moves only when an author remembers to bump it.
- **Doorstop** (`docs/reference/item.md`, `docs/cli/validation.md`): a link stores the parent's **fingerprint** — SHA-256 over
  the UID, `text`, `ref` and the link UIDs — and validation warns "suspect link" when it changes, until `doorstop clear`. Any edit
  to the text, a typo fix included, makes every child link suspect.

*From memory, not re-read for this ADR (leads, not findings):* DO-178C asks for trace data from high-level to low-level
requirements to source to tests, with design compliance shown by review; ASPICE SWE.2/SWE.3 ask for bidirectional traceability
and consistency between requirements and architecture; SysML v2 has a `satisfy` relationship; commercial ALM tools mark links
"suspect" when the upstream item changes.

**The shared weakness.** Both mechanisms read a change as *any* change. A text hash fires on cosmetic edits, so reviewers learn
to clear suspect links unread — the alarm decays into noise. An authored revision fires only when someone remembers, so a real
change can go unflagged. Neither can tell a change that keeps a design compliant from one that breaks it.

**What req already has.** The meaning fingerprint (ADR 0020) does not move on a cosmetic edit, and `req compare` classifies a
change by direction (`src/compare.rs`). A design that meets "within 100 ms" meets "within 200 ms"; one that meets a SHALL meets
the same SHOULD; one that picks from `{A, B}` still picks from `{A, B, C}`. Those carry-overs are logic, not judgement — provided
the classifier is right about the direction, which is a measured property, not an assumed one.

## Decision

1. **A satisfaction link is authored where the design is.** A design element — any file under a root listed in a new optional
   `.req/design` (grammar as the other `.req/` lists) — declares the claims it satisfies with a line `req-satisfies: <claim-id>`
   (one per line, in a comment or front matter; req reads the token, not the file format). The element's id is its repository
   path. A link to a claim the corpus does not hold is UNVERIFIABLE, never ignored.
2. **Ratifying a link records the meaning it was judged against.** `req satisfy --record <element> <claim-id>` appends to
   `.req/satisfied` the element, the claim id and the claim's **statement text verbatim** at that moment. Recording is the human
   act (ADR 0004): req never records a link on its own. The statement, not the address, is stored, so the record survives a
   `RULES_V` bump (it is re-canonicalized under the current rules) and needs no git history (a shallow CI clone, an air-gapped
   checkout: REQ-0012).
3. **A link is judged by the direction of the claim's change since it was recorded**, re-deriving both statements under the
   current rules:
   - **holds** — same address (a cosmetic edit, or none);
   - **holds (weakened)** — the change is one of `modality_weakening`, `numeric_loosening`, `membership_widening`, and decision
     5's measurement admits that class. Reported by name, so a reviewer sees that the requirement loosened, but the link is not
     re-opened: what met the stronger obligation meets the weaker one;
   - **suspect** — every other direction: a strengthening or tightening, a polarity flip, scope narrowing **and** widening,
     divergence, unrelated, unclassifiable, or a weakening class decision 5 has not admitted. Suspect is UNVERIFIABLE (the design
     may still comply; nobody has shown it) and names the element, the claim and the direction, until a human re-records it;
   - **unratified** — a declared link with no record: UNVERIFIABLE.
4. **Where it is enforced.** A new gate policy `design_satisfied` judges the links of every applying claim with the outcomes
   above (no link at all is not this policy's finding). ADR 0039's file gains `need design`: with it, an applying claim no design
   element satisfies is a structural omission (`unsatisfied`, UNVERIFIABLE) — OpenFastTrace's `Needs: dsn`, as a repository
   policy instead of a per-claim field, so no claim-schema change.
5. **Carry-over is earned by measurement, as the fingerprint is.** A weakening class carries a link only if, on the direction gold
   set (`bench/corpus/direction-v1.yml`, labels from the mutation, never from the classifier), **no pair whose label is a
   strengthening, a tightening, a polarity flip or a scope change is classified into that class** — a *false carry*, the
   satisfaction analogue of a false merge, with the same zero bound. `req bench` reports the false-carry count per class; a class
   with one is not admitted, and its links are suspect. Until the measurement exists, no class is admitted and every change of
   address is suspect — strictly safer than Doorstop, and never noisier: cosmetic edits still hold.
6. **Scope classes are never carried**, whatever the measurement says. Adding a guard (`when the tank is full`) makes an obligation
   apply less often; removing one makes it apply more often. Which of those a design survives depends on the design, so both are
   suspect. (Writing this ADR found that `req diff` and `req compare` read *removing* a condition as the loosening — see
   *Consequences*.)

## Not decided here

- **Deep coverage** — requiring a design element's own tests to pass before its links count (OpenFastTrace's deep status).
- **Code-level elements**, i.e. tags in source files, which this decision admits only as files under a `.req/design` root.
- **Who may record** — a role on the record ties into ADR 0005's unbuilt roles and `docs/design.md` §6 item 8.

## Consequences

- A requirement edit no longer forces a blind re-review of every linked design: a cosmetic edit leaves links holding, an admitted
  weakening holds and is reported, and only the rest re-open. The noise that trains reviewers to clear alarms unread is the
  failure this removes; the asymmetric cost (ADR 0004) is kept because any doubt is suspect.
- `.req/satisfied` is a reviewed file like `.req/variants`; a hand-edited statement in it is caught only by review and git
  history, as a pin is (ADR 0022's named loss). Attesting it in the Baseline is a follow-up if links become release-bearing.
- **A finding outside this ADR's scope, recorded so it is not lost.** `src/compare.rs` classes a removed condition as
  `scope_widening` and `Direction::is_loosening` counts it as "permits strictly more than before"; `req diff` maps it to
  `weakened` and an *added* condition to `tightened` (ADR 0026 §2). For guard conditions the reverse holds: "The pump shall stop
  within 2 s" → "… when the tank is full" obligates *less*, yet is reported `tightened` — the dangerous direction, a weakening
  shown as a tightening. Probed on 2026-11-11 with `req compare` (rules v8). It needs its own decision (an ADR 0026 amendment,
  with the gold-set labels re-read against the spec); decision 6 keeps this ADR independent of it.
  *Decided by ADR 0041: a guard added to an obligation now reads as permitting more.*

## Implementation note — 2026-11-11 (S-1: the false-carry measurement)

Not an edit of the decisions above; what decision 5 now has behind it.

- **The measurement set is `bench/corpus/direction-v2.yml`**: v1 plus the mirrors of the modality/numeric pairs (labelled with
  the opposite class — the mutation read backwards). v1 held 5 `modality_strengthening`, 9 `numeric_tightening` and no
  `membership_*` pairs, so its zero false carries was vacuous. The pinned sources hold no enumerated `one of {…}` statement, so
  v2 has **no membership pairs**; none was synthesised.
- **The support minimum is 100** scored pairs labelled the class's opposite (owner go-ahead, 2026-11-11). `req bench` reports
  `direction.false_carry.<class> = {count, support, admitted, pairs}` for all three weakening classes; `admitted` is `count == 0`
  and `support >= 100`. It is an input to this ADR's satisfaction links, not a bench verdict.
- **Measured at rules v8** (`bench/results/direction-v2-rules-v8.json`; docs/canonical-hashing.md §6.3): zero false carries in
  all three classes, and
  - `modality_weakening` — support 177: **admitted today**;
  - `numeric_loosening` — support 9 (of 17 labelled tightenings, 8 are unclassifiable and 4 read as `scope_divergence`):
    **not admitted**, support below 100;
  - `membership_widening` — support 0: **not admitted**, no pair exists to measure it on.

  So today only a `modality_weakening` change may carry a link; a numeric or membership loosening does not, until the gold set
  has the support (a source with such statements, or classifier coverage of the unclassifiable ones).

## Implementation note — 2026-11-11 (S-2: the code)

Not an edit of the decisions above; where they now live.

- **The module is `src/satisfy.rs`**; `req satisfy [--format text|json]` lists every declared link and `req satisfy
  --record <element> <claim-id>` is the human act. `.req/design` is the optional list of roots (grammar of the other `.req/`
  lists; directories are read recursively, symlinks and `.git` are not followed, a file that is not UTF-8 or a root that does
  not exist is UNVERIFIABLE and named). A declaration is any line holding `req-satisfies:` followed by a claim id; the
  element id is the repo-relative, `/`-separated path.
- **The file format.** `.req/satisfied` is `schema: req.satisfied/1` and `links:`, a list of `{element, claim_id,
  statement}` sorted by `(element, claim_id)`; `statement` is the claim's statement verbatim. No address, no rules version, no
  date. An unknown key, another schema or a repeated link is a load error (UNVERIFIABLE; `--record` refuses to overwrite it).
- **The admitted set** is `ADMITTED_CARRIES` in that module: `modality_weakening` alone, the one class
  `bench/results/direction-v2-rules-v8.json` reports `admitted: true` (S-1). `tests/satisfy_admission.rs` reads the record
  for the current `RULES_V` and asserts the constant equals it, so a rules bump or a re-measurement that changes admission
  fails CI until the constant is changed on purpose; the record is never read at runtime. `carries()` refuses a scope class
  whatever the constant says (decision 6).
- **Judging** re-derives the recorded and the current statement under the current rules, with the claim's current
  applicability on both sides (a selector change is not a statement change, and the record holds none): same address holds,
  `compare` in the admitted set holds weakened, any other direction is suspect.
- **Enforcement.** The gate's seventh policy `design_satisfied` (PASS with counts, or UNVERIFIABLE; never FAIL; PASS over zero
  links with no `.req/design`, said so) is folded in by `gate::evaluate_selection`, so `req check` inherits it per release line.
  `need design` in `.req/completeness` adds the `unsatisfied` omission to the completeness step and to `req audit`'s
  *Structural omissions* section. A malformed `.req/design` line is exit 2 in `satisfy` and `check`.

- **2026-11-12 (G-1: PURE; corrected by G-1-fix).** `direction-v4` (v3 plus pairs from the numeric statements of PURE, Zenodo 1414117, CC BY 4.0, with the 7 domain-comparator statements found by review excluded by hand) measures `numeric_loosening` at **0 false carries over a support of 142** (v3: 17, v2: 9; 154 before the exclusions), and `modality_weakening` at 0 over 273; its record reports both `admitted: true`. The support is pairs, not statements (about 45 PURE statements the parser reads with a value slot, each giving several pairs), and `ADMITTED_CARRIES` is **unchanged** and still pinned to the v2 record, so only `modality_weakening` may carry a link today; moving the measurement to v4 and admitting `numeric_loosening` is a separate owner decision. docs/canonical-hashing.md, *Recorded runs*.

## Falsification

This decision is wrong if any of these is observed:

1. a cosmetic edit to a claim makes a recorded link suspect;
2. a strengthening, tightening, polarity flip or scope change leaves a recorded link holding;
3. a weakening class with a non-zero false-carry count on the gold set carries a link;
4. a `RULES_V` bump alone makes a recorded link suspect when the statement text is unchanged;
5. a link with no record, or to a claim the corpus does not hold, reads as holding;
6. req writes `.req/satisfied` without the explicit `--record` act.
