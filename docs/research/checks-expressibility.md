# Can ten realistic rules be written as repository checks without a fork?

Task X-1, experiment 1 (`docs/specs/X-1-checks-and-fields-experiments.md`). Run 2026-11-17 with `req 0.5.0`, built from
`30f85a5` with `cargo build --release` (`req --version`: `rules_v 9`, `baseline_envelope 4`, `claim_schema req.claim/3`).
Every command below was run on this checkout and its output is taken from that run. Every example rule is invented and
generic; the only claims quoted are req's own `claims/`.

**Question.** Could a small, declarative, three-valued check language over a claim and its derived data express the rules
an organisation imposes, or would each rule need a new built-in in Rust?

**Method.** I wrote ten rules and a syntax sketch, named for each rule the exact source that would supply its data, ran the
queries that source implies on this repository, and classified each rule. **No evaluator was written.** The numbers under
"Measured here" are queries (`jq`, `ruby`, `req ...`) over the outputs and files a check would read, so they show that the
data is or is not there. They do not show a check language working.

## 1. What data req offers today

The spec lists four candidate sources. Checked against the binary:

| source | exists? | evidence |
| --- | --- | --- |
| `req canon --format json` | **No.** `canon` is text only. | `req canon --format json` prints `unrecognized or repeated flag: --format`, exit 2. `req canon --help` documents `--fail-on-flag` and `--by-area` only. |
| `req trace --format json` (`req.trace/1`) | yes | node keys: `address applies area claim_id evidence modality source statement superseded_by tests`; edge keys: `from relation to`; `findings`: `dangling`, `undecided_edges` |
| `req satisfy --format json` (`req.satisfy/1`) | yes | link keys `claim_id element line outcome` (+ `direction` on a changed claim); `verdict`, `links`, `mentions`, `unreadable` |
| `req gate --format json` (`req.gate/2`) | yes, **per policy, not per claim** | `policies[]` carry `outcome`, `details` (text), `checked`/`counts`; there is no per-claim outcome field |
| a claim's authored keys | yes | `claims/*.yml`; strict key list in `src/model.rs:314` |

Three more surfaces turned out to matter, none named by the spec:

- `req render --format json` rows add `selectors` (the `applicability` predicates), `relations` and `rationale{source,date}`,
  and carry no `tests`, `evidence`, `source` flattening or lints.
- `req diff --base <rev> --format json` (`req.diff/1`): per changed claim `class`, `direction`, `axes`, `flags`,
  `base_address`, `head_address`.
- `req audit --baseline <file> --format json`: `coverage.{signed,drifted,uncovered,missing,not_judged}`.

**Gaps that decide classifications below.**

- **Derived facts that `Derived` computes (`src/canon.rs:1742`) but no JSON document carries:** `lints`, and the derived
  `operator`/`value`/`unit`. They appear only in `req canon` text. `UNSTRUCTURED` is the one exception: it is the `modality`
  label of a node (`Derived::modality_label`), and `jq` finds it: `REQ-0007 area=authoring`.
- **`rationale.rejected_alternatives`, `rationale.assumptions`, `rationale.date` and the pins are in no JSON document**
  (`render` carries `source` and `date` only; `trace` carries `source` only). They exist in the authored file and in
  `rationale_hash`.
- **No command computes "this claim has an accepting Decision at its current address".** `audit.rs`, `gate.rs`, `view.rs`
  and `check.rs` do not read the decision log per claim (`grep -n "decision::" src/audit.rs src/gate.rs src/view.rs
  src/check.rs` finds nothing); `req decision` verifies the hash chain and lists records. The pieces exist: `decisions/DEC-*.yml`
  carries `kind`, `claim_id`, `address`, `role`.
- **Per-variant applicability is one `trace` call per `.req/variants` line.** Values: `applies`, `does_not_apply`, `unknown`.
- **A "released variant" is not a datum a document names.** It is the set of lines of `.req/variants` (two here).

**What "today" means below.** An engine living *inside* req could read `LoadedClaim` (`src/canon.rs`: `claim` + `derived`)
directly, and then lints and values are in memory. I classify against the **published machine surfaces** (JSON documents
and authored files), because a check file would name its data, and the names it uses become a contract the way
`req.trace/1` is one (ADR 0046). An in-process engine would not remove the need for that contract; it would move it from
"new JSON" to "new datum names".

## 2. The syntax sketch

Repository YAML style, one file (`.req/checks`; a sketch, nothing in req reads it):

```yaml
schema: req.checks/0
checks:
  - id: CHK-001
    for: claim                # subject kind: claim | area | variant | link | decision
    in: released              # each line of .req/variants (default), or: corpus
    when: <expr>              # optional: which subjects the rule is about
    unverifiable_when: <expr> # optional: an explicit UNKNOWN (added by rules R7 and R9, see below)
    require: <expr>
    severity: error           # error | warn
    message: "<text with {claim.id} placeholders>"
```

**Core expression vocabulary** (everything else is a built-in): field paths; `== != < <= > >=`; `in [..]`; `and or not`;
`count(list)`; `any/all(list, pred)`; `list where pred`; `is_empty`; `known(x)`; lookup of a row by key (`claim_of(id)`),
which is a join.

**Three-valued semantics.**

- A value is true, false or **unknown**. A path that resolves to no datum (an absent field, a source that did not load, an
  unreadable report) is unknown, never null and never false.
- `and`/`or`/`not` are strong Kleene. `false and unknown` is false (FAIL outranks UNVERIFIABLE, as in ADR 0023);
  `true or unknown` is true, which is sound but surprising, so the sketch names it.
- `when`: false skips the subject; true evaluates it; **unknown makes the subject UNVERIFIABLE**, because it cannot be said
  whether the rule applies (this is the undecided-selector case, ADR 0006, one level up).
- A subject is PASS when `require` is true, violated when it is false, UNKNOWN when it is unknown or `unverifiable_when` is
  true.
- A rule is FAIL if any subject is violated under `severity: error`; else UNVERIFIABLE if any is UNKNOWN; else PASS. It
  says the number of subjects it judged, and says "over zero subjects" when `when` selected none (`pins_hold` does the same).
- **`severity` governs the FAIL branch only.** `warn` reports a violation without changing the verdict, as the completeness
  findings do (ADR 0039); it never downgrades an UNKNOWN. A severity that could turn UNVERIFIABLE into a warning would
  fold UNVERIFIABLE into PASS, which `AGENTS.md` forbids.
- Results fold with the gate's by the ADR 0023 rule: any FAIL, else any UNVERIFIABLE, else PASS.

**Two additions the rules forced.** The four keys the spec names (`when`, `require`, severity, message) were not enough:

1. `unverifiable_when` (R7, R9). The sources themselves carry verdict-shaped values: `satisfy` says `suspect`, which the
   gate treats as UNVERIFIABLE because "a suspect link may still comply; nobody has shown it". With only `require`, `suspect`
   would be false and a check would FAIL where the built-in says UNVERIFIABLE: a check stronger than the gate it sits beside.
2. A subject kind other than the claim (R5 wants the variant list, R6's area list, R9's decision, and the built-in
   `design_satisfied` judges *links*). A claim-only language cannot state them.

## 3. The ten rules

Each rule: the YAML, then the data it reads, its class, and what makes it UNVERIFIABLE rather than PASS. "ASIL" is the
generic automotive-style integrity level `QM < A < B < C < D`, used only as an invented example of a typed field.

### R1. Typed field: an ASIL C or D requirement needs a linked test

```yaml
- id: CHK-001
  for: claim
  when: claim.fields.asil in [C, D]
  require: count(claim.tests) >= 1
  severity: error
  message: "{claim.id}: ASIL {claim.fields.asil} needs at least one linked test"
```

- **Data.** `claim.tests`: `req trace --format json` node `tests` (exists). `claim.fields.asil`: **does not exist**; the loader
  refuses `fields:` (the real refusal is quoted in `docs/research/fields-separation.md`, §1).
- **Class.** Expressible once typed fields exist.
- **UNVERIFIABLE when.** The claim has no `asil` value (`when` is unknown); the value is outside the declared set. A claim
  declaring `evidence: process` has no tests and so FAILs this rule by design.
- **Measured here.** 0 of 40 claims can carry the field. With "absent is unknown" every claim of an existing corpus is
  UNVERIFIABLE the day the rule is added; with "absent means QM" the rule silently judges zero claims. The typed-fields ADR
  has to pick one, and only the first agrees with `AGENTS.md`.

### R2. Typed field: a claim shall not carry a lower ASIL than the claim it refines

```yaml
- id: CHK-002
  for: claim
  require: all(claim.edges where relation == "refines",
               claim.fields.asil >= claim_of(to).fields.asil)
  severity: error
  message: "{claim.id}: ASIL lower than the claim it refines"
```

- **Data.** `edges[]` of the trace document (exists; `refines` is one of the two relation types the corpus uses today:
  `grep -h "type:" claims/*.yml | sort | uniq -c` gives 24 `derives-from`, 16 `refines`). `fields.asil` on both ends: does not exist.
  `>=` on `asil` needs the field's **declared order**, so the type declaration (an ordered enum) is part of the ADR.
- **Class.** Expressible once typed fields exist; also needs the join `claim_of(to)`.
- **UNVERIFIABLE when.** Either end has no value; `to` is dangling (`req trace` lists it under `findings.dangling`, exit 3);
  the type has no declared order.

### R3. Rationale provenance: a SHALL claim records at least one rejected alternative and a source

```yaml
- id: CHK-003
  for: claim
  when: claim.modality == "SHALL"
  require: count(claim.rationale.rejected_alternatives) >= 1 and not is_empty(claim.rationale.source)
  severity: warn
  message: "{claim.id}: rationale records no rejected alternative"
```

- **Data.** `modality`: trace node (exists). `rationale.rejected_alternatives` and `rationale.source`: **authored keys** in
  `claims/*.yml`; `source` is also a trace node field, `rejected_alternatives` is in no JSON document.
- **Class.** Expressible on today's data, by reading the authored key. An engine outside req would re-parse the claim files
  and so repeat the loader's strictness (`deny_unknown_fields`, `src/model.rs:22`).
- **UNVERIFIABLE when.** The claim file does not load (`req canon` exit 3, NOT JUDGED); a `req.claim/<n>` schema this build does not
  write.
- **Measured here.** `ruby -ryaml -rdate -rjson ... Dir["claims/REQ-*.yml"]` finds `rationale.rejected_alternatives` empty on
  **4 of 40** claims (REQ-0006, REQ-0009, REQ-0010, REQ-0014); of the 34 claims whose trace `modality` is `SHALL` under the first
  declared variant, 3 are among them.

### R4. Relation cardinality: allocated to at most one component

```yaml
- id: CHK-004
  for: claim
  require: count(claim.edges where relation == "allocated-to") <= 1
  severity: error
  message: "{claim.id}: allocated to more than one component"
```

- **Data.** `edges[]` of the trace document. The loader does not restrict relation types: `Relation.kind` is a `String`
  (`src/model.rs:34`), and only `supersedes` and `conflicts-with` carry meaning.
- **Class.** Expressible on today's data **with a modelling caveat**, which I tested. Each relation's `to` must be a claim.
  On a temporary copy I added `allocated-to` to REQ-0001 twice, once to `COMP-ENGINE` (not a claim) and once to REQ-0002.
  `req trace --format json` prints both edges, but `findings.dangling` is `["allocated-to -> COMP-ENGINE (referenced by REQ-0001,
  not in the corpus)"]`, `req trace` exits 3 and `req gate` reports `[UNVERIFIABLE] relations_resolve` naming it. So this rule
  is expressible only if components are claims. If components stay outside the corpus, it moves to "needs typed fields" (a
  `component` field). I count it under today's data and say so in the verdict.
- **UNVERIFIABLE when.** The `to` is dangling; the edge target is undecided in the selection (`findings.undecided_edges`).
- **Measured here.** 0 of 40 claim files contain `allocated-to` (`grep -c allocated-to claims/*.yml`).

### R5. Variants: every claim applies in at least one declared variant

```yaml
- id: CHK-005
  for: claim
  in: corpus
  require: any_variant(claim.applies == "applies")
  severity: error
  message: "{claim.id} applies in no declared variant"
```

- **Data.** One `req trace --format json <line>` per line of `.req/variants`; `nodes[].applies` (exists).
- **Class.** **Needs a new built-in:** `any_variant(pred)`, a quantifier over the declared variants that runs the selection
  per line. Today the same property is a Rust test, `tests/release_gate.rs::every_claim_applies_in_at_least_one_declared_release`
  (named in `.req/variants`).
- **UNVERIFIABLE when.** A claim is `unknown` in every variant where it does not `apply` (an undecided selector is not a
  FAIL); `.req/variants` is absent or unreadable (`req check` exit 3 on a corpus carrying a selector, ADR 0023 §3).
- **Measured here.** `bash` loop over `.req/variants`: variant 1 `applies 39, does_not_apply 1` (REQ-0010), variant 2 `applies 39,
  does_not_apply 1` (REQ-0008); the union of claims that apply in at least one line is **40 of 40** (`jq -rs '[.[]|.nodes[]|
  select(.applies=="applies")|.claim_id]|unique|length'`).

### R6. Areas: no claim filed under `<subtree>/` is unstructured

```yaml
- id: CHK-006
  for: claim
  when: under(claim.area, "safety")
  require: claim.modality != "UNSTRUCTURED"
  severity: error
  message: "{claim.id}: filed under safety/ but its statement has no deterministic parse"
```

- **Data.** `area` and `modality` on the trace node (exist). `UNSTRUCTURED` is `Derived::modality_label()` (`src/canon.rs`).
- **Class.** **Needs a new built-in:** `under(path, prefix)`. It cannot be string `startswith`, because the area grammar
  (`src/model.rs:361`, ADR 0051) is segment-based: `safety-x` is not under `safety`.
- **UNVERIFIABLE when.** The claim has no `area` (`null`): `when` is unknown, not false; `.req/areas` malformed (trace exit 2).
- **Measured here.** This repository has no `safety/` area, so the same rule with prefix `authoring`: `jq` finds one
  `UNSTRUCTURED` node, `REQ-0007 area=authoring`, so the rule would FAIL on it.

### R7. Direction: a tightened claim needs a re-recorded satisfaction link

```yaml
- id: CHK-007
  for: claim
  when: diff(claim).class == "tightened"
  unverifiable_when: diff(claim).class == "not-judged"
                     or any(satisfy.links where claim_id == claim.id, outcome in [suspect, unratified])
  require: count(satisfy.links where claim_id == claim.id) >= 1
           and all(satisfy.links where claim_id == claim.id, outcome in [holds, holds_weakened])
  severity: error
  message: "{claim.id} tightened since {base}: record the satisfaction link again"
```

- **Data.** `req diff --base <rev> --format json` (`class`, exists) joined by `claim_id` to `req satisfy --format json`
  (`links[].outcome`, exists).
- **Class.** Expressible on today's data. It needs a join across two documents, and a **base revision that is not a repository
  datum**: the Baseline records no git revision, so `<rev>` is a parameter. With none, or in a shallow clone, `req diff` exits 3 and
  the rule is UNVERIFIABLE.
- **UNVERIFIABLE when.** No resolvable base; the claim is `not-judged` (no structured form on a side, so no direction);
  a link is `suspect`, `unratified` or `unknown_claim`; the `.req/design` roots are unreadable (`satisfy.unreadable`).
- **Measured here.** On the temporary copy I changed REQ-0009 from "at least 0.95" to "at least 0.97". `req diff --base HEAD
  --format json` gives `{"claim_id":"REQ-0009","class":"tightened","direction":"numeric_tightening","flags":["pin-flagged"],...}`;
  `req satisfy --format json` gives `{"claim_id":"REQ-0009","element":"src/bench.rs","outcome":"suspect","direction":"numeric_tightening"}`,
  verdict `unverifiable`, exit 3. The join is possible. The rule agrees with the built-in `design_satisfied` here (§5), so as
  written it adds only the "since base" scoping.

### R8. Lints: no `untestable_qualifier` on a claim that applies in a released variant

```yaml
- id: CHK-008
  for: claim
  when: claim.applies == "applies"
  require: not any(claim.lints, code == "untestable_qualifier")
  severity: error
  message: "{claim.id}: statement carries an untestable qualifier"
```

- **Data.** `claim.lints[]`: computed (`Derived::lints`, `src/canon.rs:1750`, six codes: `ambiguous_modality`,
  `multiple_obligations`, `uncovered_content`, `untestable_comparator`, `untestable_qualifier`, `untestable_quantity`), but
  printed only in `req canon` text (a `lints <codes>` column).
- **Class.** **Needs a new derived datum on a machine surface:** per-claim lints in a JSON document. The first
  line of the text, `REQ-0007  unstructured -  -  op - value -  unit -  lints untestable_qualifier area authoring`, is parseable,
  but a check that parses a diagnostics column breaks silently when the column moves.
- **UNVERIFIABLE when.** The lint list is not available (`req canon` exit 3: a claim file NOT JUDGED); the claim is `unknown`
  in the variant (its `when` is unknown).
- **Measured here.** `req canon | grep '^REQ-' | grep -v 'lints - '` lists two claims with lints: REQ-0003
  (`untestable_comparator`) and REQ-0007 (`untestable_qualifier`). REQ-0007 applies in both declared variants, so the rule
  would FAIL on it.

### R9. Decisions: an applying claim has an `accept` Decision at its current address

```yaml
- id: CHK-009
  for: claim
  when: claim.applies == "applies"
  unverifiable_when: decision_log.verdict != "valid"
  require: any(decisions where claim_id == claim.id and address == claim.address and kind == "accept")
  severity: error
  message: "{claim.id}: no accept Decision ratifies its current address"
```

- **Data.** `decisions/DEC-*.yml` authored keys (`kind`, `claim_id`, `address`, `role`, `actor`, `date`: real file
  `decisions/DEC-0001.yml`); `claim.address` from the trace node; the chain's verdict from `req decision --verify` (exit 0 VALID,
  1 TAMPERED, 3 UNVERIFIABLE; text only, no JSON).
- **Class.** **Needs a new derived datum:** per-claim *ratified at the current address*. No req command computes it (§1); the
  join is cheap, but it is a new computation with a new meaning (what an older address's Decision says about a reworded claim:
  superseded, not tampered, `req decision --help`).
- **UNVERIFIABLE when.** The log is unreadable, empty or unfilled; a signed Decision whose signature cannot be judged (a
  missing verifier); a TAMPERED log is FAIL, not UNKNOWN.
- **Measured here.** `req decision` lists one record, `DEC-0001  accept REQ-0016`, and its `address` equals REQ-0016's current
  trace address (`04836a98...fddf8bda`). Of the 39 claims that apply in variant 1, **1** has a ratifying Decision at its current
  address; the rule would FAIL on 38.

### R10 (my choice). Pins: a claim with a derived numeric bound carries a pin

```yaml
- id: CHK-010
  for: claim
  when: known(claim.derived.value)
  require: not is_empty(claim.pins)
  severity: warn
  message: "{claim.id}: a quantity claim should pin its bound (docs/claim-format.md, authoring rules)"
```

- **Data.** `claim.derived.value`: computed, printed only in `req canon` text. `claim.pins`: an authored key.
- **Class.** **Needs a new derived datum on a machine surface** (the same canon JSON as R8). I chose it because it is a rule req's
  own `docs/claim-format.md` states ("Pin the bound of a quantity claim") and no gate enforces.
- **UNVERIFIABLE when.** The derivation is unavailable; the claim is unstructured (no derived value, so the `when` is false, not
  unknown: a statement the parser could not read has no bound for the rule to hold).
- **Measured here.** `req canon` shows a derived `value` on 9 claims (REQ-0003, 0005, 0008, 0009, 0010, 0014, 0021, 0022, 0040);
  its summary line says `pins: 9 on 5 claim(s)` (REQ-0008, 0009, 0010, 0022, 0040), so **4** (REQ-0003, 0005, 0014, 0021) carry a bound
  and no pin. This count parses a text column.

## 4. Classification

| # | rule | class | new thing it needs |
| --- | --- | --- | --- |
| R1 | typed field: ASIL C/D needs a linked test | typed fields | `claim.fields.asil`; "absent" semantics |
| R2 | typed field: no lower ASIL than the refined claim | typed fields | ordered enum type; `claim_of(to)` join |
| R3 | rationale: rejected alternative and source | **today** (authored key) | none |
| R4 | relation: at most one `allocated-to` | **today**, if components are claims | none (caveat above) |
| R5 | variants: applies somewhere | new built-in | `any_variant(pred)` |
| R6 | areas: subtree has no unstructured claim | new built-in | `under(path, prefix)` |
| R7 | direction: tightened needs a re-recorded link | **today** (join of two JSON documents) | a base revision as parameter |
| R8 | lints: no `untestable_qualifier` | new derived datum | per-claim lints in a JSON document |
| R9 | decisions: ratified at the current address | new derived datum | per-claim ratification state |
| R10 | pins: a bounded claim carries a pin | new derived datum | derived `value`/`operator`/`unit` in a JSON document |

No rule landed in "cannot be expressed three-valued". That is not evidence the class is empty: I wrote the rules knowing the
data and the syntax.

**What makes each UNVERIFIABLE rather than PASS** is in the rule entries. They fall into four kinds: an **absent datum** (R1, R2, R6: a
claim with no `asil`, no `area`); an **undecided selector** (R5, R8, R9: `applies` is `unknown`); an **unreadable or unverified
source** (R3, R7, R9: a claim that does not load, no base revision, a log that is not VALID); and a **verdict-shaped input**
(R7: `suspect`, `unratified`).

## 5. Can the language write the built-in policies?

A language that cannot write the gate's own policies is too small to replace them; one that can would show what a check has
to be able to say. Three attempts:

**`design_satisfied`** (judges the `req satisfy` links):

```yaml
- id: BUILTIN-design-satisfied
  for: link
  unverifiable_when: link.outcome in [suspect, unratified, unknown_claim]
  require: link.outcome in [holds, holds_weakened]
  severity: error
```

Writable. It works because `satisfy` already did the judging (the direction of the change, ADR 0040); the language
**consumes a verdict, it does not derive one**. Two gaps remain: the gate never FAILs this policy (the sketch's `severity: error`
would, so the policy needs a way to say "UNKNOWN only"), and it also judges links naming a claim the corpus does not hold, so the
subject kind is the link, not the claim.

**`rationale_present`**:

```yaml
- id: BUILTIN-rationale-present
  for: claim
  when: claim.applies == "applies"
  require: not is_empty(claim.rationale.source) and source_anchor_ok(claim.rationale.source)
```

The first half is writable. The second needs a built-in, `source_anchor_ok(path#Lnn)` (ADR 0048): a repository path with a line
anchor must exist, be a file and have that many lines; one that exists and cannot be read is UNVERIFIABLE; prose, URLs, absolute or
`..` paths, heading anchors and a path without an anchor are **not judged**; and paths are looked up from the corpus root and from the
checkout root when the corpus sits in a subdirectory. That needs the filesystem and a path grammar, which are not core.

**`tests_linked`**:

```yaml
- id: BUILTIN-tests-linked
  for: claim
  when: claim.applies == "applies" and claim.evidence == "machine"
  require: any(claim.tests, test_outcome(.) == "passed")
```

Not writable as the policy means it. It needs `test_outcome(id)` over the **union** of one or more JUnit reports (`src/junit.rs`):
the same id with different outcomes in two reports is unusable and names both files; a skipped test is not a pass; a failed one
outranks a passing one on the same claim; a renamed test is "linked test not in the report" and never a pass; an absent, unparsable
or zero-testcase report is UNVERIFIABLE **once, not per claim**. It also reports `process_evidenced` claims as PASS-as-declared in a
named list. A per-subject language has no "once per run" outcome and no "pass, listed" annotation. Moreover `req gate --format json`
does not expose per-claim outcomes to build on: with a fixture report of three testcases (one passed, one failed, one skipped) over
the 39 applying claims of variant 2, `tests_linked` printed `counts: {applying 39, linked 37, process_evidenced 2, passed 1, failed
2, not_in_report 34}` and `details` named only the 2 failures (fail outranks unverifiable). The 34 `not_in_report` claims are a
count, not a list.

**Result.** One of three policies is writable without a new built-in, and that one only by delegating its judgement; one needs one
built-in; one needs a run-level aggregation the language lacks. The language is **too small to replace the gate's policies**, and
should not try. A check file would be a ninth source of verdicts beside them.

## 6. Built-ins and data a check file would need

New **built-in functions**: `any_variant(pred)` (R5), `under(path, prefix)` (R6); and for the built-in policies `source_anchor_ok(...)`,
`test_outcome(id)`.

New **derived data** on a machine surface: per-claim lints, and derived `operator`/`value`/`unit` (R8, R10), both already in
`Derived`; the per-claim ratification state (R9), which is a new computation; typed `fields` (R1, R2) with a declared type
including order; the list of declared variants as data (R5).

New **language structure**: subject kinds beyond the claim, `unverifiable_when`, a join (`claim_of`, two-document joins in R7, R9).

A file `.req/checks` would also contradict one sentence of ADR 0023 §2 ("`req check` adds no key, no section and no third file") and
needs its own ADR; I do not draft it here.

## 7. Verdict

Of the ten rules:

- **3 fit on today's data** (R3, R4, R7), with two caveats: R4 holds only if components are modelled as claims (otherwise it
  moves to typed fields, making the typed-field count 3), and R7 needs a base revision supplied from outside.
- **2 need typed fields** (R1, R2), and R2 needs a declared order on the field's type.
- **3 need a derived datum** that req computes but does not expose (R8, R10) or does not compute at all (R9).
- **2 need a new built-in** (R5, R6).
- **0 cannot be expressed three-valued.**

So **7 of 10 need a change to req beyond the check language itself**, and of those only R5 and R6 are a function in the language; the
other five are data or schema work that the typed-fields and canon-JSON decisions would do anyway. Three-valuedness itself was never
the obstacle: every rule had a natural UNKNOWN. The obstacle was that the published surfaces do not yet carry the facts that
organisations' rules are about.

A reading the evidence also supports: R3, R5, R6 and R7 can already be written today as a `jq`/`ruby` script over the JSON documents
(I measured them that way). What a repository-defined language would add over a script is not data; it is the **fold**: a rule that
returns UNVERIFIABLE instead of silently skipping a claim, and one `req check` verdict. That is the part ADR 0023 says belongs in req,
and it is the part this experiment did not build.

## 8. What this does not show

- It does not show a check language working: no evaluator was written, so every "would FAIL on N" is a query on the data, not a run
  of a rule.
- Ten rules chosen by the author of the syntax are a thin sample. Zero rules "cannot be expressed three-valued" is a property of this
  sample. Rules over time (a review within twelve months, which needs a clock), over free text (a statement that "reads as
  testable"), or over a risk analysis outside the repository were not tried.
- The counts describe req's own 40 claims, which carry no typed field and one Decision. They say nothing about an adopter's corpus.
- Nothing here measures authoring cost, error rates or whether adopters want the language. Nothing compares it with a CI script.
- The two built-in-policy checks that the language cannot write were read from `--help` and `src/junit.rs`'s contract and one fixture
  report; I did not enumerate every `tests_linked` branch.
- No external corpus was used and no external number is quoted. The Tier-1 bet and `FMR = 0` are untouched: a check reads data and
  moves no address.

## Appendix: commands run

All from the repository root unless noted; `R=./target/release/req`. Variant 1 is the first line of `.req/variants`.

```
$ git rev-parse --short HEAD ; ./target/release/req --version
30f85a5  /  req 0.5.0, rules_v 9, baseline_envelope 4, decision_chain 1, claim_schema req.claim/3, built from 30f85a5
$ $R canon --format json ; echo $?                       -> unrecognized or repeated flag: --format   (exit 2)
$ $R trace --format json <variant 1>                      -> exit 0; keys: schema generator rules_v selection nodes areas edges derived_incoming findings
$ $R trace --format json                                  -> exit 3 (5 claims unknown with no context); JSON still printed
$ $R gate --format json <variant 1>                       -> exit 3, verdict unverifiable: tests_linked has no --test-report; the other seven policies pass
$ $R satisfy --format json                                -> exit 0, verdict pass, 29 links
$ $R audit --baseline baselines/air-gapped.json --format json -> exit 0; coverage.signed 36 claims, drifted/uncovered/missing empty
$ $R canon --by-area | tail                               -> claims: 40 (1 unstructured); pins: 9 on 5 claim(s), 0 flagged; lints on 2 claim(s)
$ bash m3.sh   (trace --format json per .req/variants line)
variant 1: applies 39, does_not_apply 1 ; variant 2: applies 39, does_not_apply 1 ; union applying: 40
$ $R decision                                              -> - DEC-0001  accept REQ-0016  by fdx (maintainer)  2026-10-15 ... [VALID]
$ cd <temporary copy>; add allocated-to to COMP-ENGINE and REQ-0002 on REQ-0001
$ $R trace --format json deployment_mode=air_gapped | jq -c .findings
{"dangling":["allocated-to -> COMP-ENGINE (referenced by REQ-0001, not in the corpus)"],"undecided_edges":[...]}   (trace exit 3)
$ $R gate deployment_mode=air_gapped                       -> [UNVERIFIABLE] relations_resolve ... - REQ-0001 -allocated-to-> COMP-ENGINE
$ cd <temporary copy>; REQ-0009 "0.95" -> "0.97"
$ $R diff --base HEAD --format json | jq -c '.claims[]|select(.claim_id=="REQ-0009")'
{"axes":["numeric_tightening"],"claim_id":"REQ-0009","class":"tightened","direction":"numeric_tightening","flags":["pin-flagged"],...}
$ $R satisfy --format json | jq -c '{verdict,links:[.links[]|select(.claim_id=="REQ-0009")]}'
{"verdict":"unverifiable","links":[{"element":"src/bench.rs","claim_id":"REQ-0009","outcome":"suspect","direction":"numeric_tightening","line":11}]}   (exit 3)
$ $R gate --format json <variant 1> | jq policies   (same copy)
pins_hold fail "REQ-0009: pin value 0.95 contradicts the parse (0.97)" ; design_satisfied unverifiable "src/bench.rs -> REQ-0009: suspect — ... numeric_tightening ..."
$ $R gate --format json --test-report <fixture, 3 testcases> <variant 2> | jq tests_linked
{"outcome":"fail","counts":{"applying":39,"linked":37,"unlinked":0,"process_evidenced":2,"passed":1,"failed":2,"not_in_report":34},"n_details":2,...}
```

The fixture report and the temporary copy lived in a scratch directory and were deleted afterwards; the fixture is a hand-written
JUnit file of three `testcase` elements, not a project artefact.
