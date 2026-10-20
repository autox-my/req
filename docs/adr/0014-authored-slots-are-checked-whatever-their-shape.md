# 0014. An authored slot that contradicts the parse is a defect, whatever its shape

**Status: accepted** — proposed by the agent, ratified by the maintainer on 2026-10-20, and
**amended the same day** after prototyping the rule against the whole corpus (see *Amendment* at the
end). The decision answers review finding [F2]. Date proposed: 2026-10-20.
 The decision answers review
finding [F2]. Date proposed: 2026-10-20.

## Context

`req canon` re-parses each claim's `statement` and reports, per field, whether the authored slots agree
with the parse. That is the parity invariant, and it is the only thing standing between "the claim says
what its author meant" and "the claim's structured slots say something else".

It does not check every field. `src/canon.rs:1284-1300` gates the comparison on whether the **authored**
value looks numeric:

```rust
let authored_is_num =
    !authored_v.is_empty() && authored_v.chars().next().unwrap_or('x').is_ascii_digit();
let value_unit_ok = !authored_is_num || (v_parsed == authored_v && u_parsed == authored_u);
let op_checkable = authored_is_num;
let op_ok = !op_checkable || op_parsed == authored_op;
```

So for any claim whose authored `value` is not numeric, `value`, `unit` and `operator` are **never
compared with anything**. Measured on REQ-0015 (a prohibition): setting `value: "present"`, then
`unit: s`, then `operator: LT`, then `attribute: halal_submission` each left the report at
`ok ok ok ok ok ok`, `--fill` answered `refused 0`, and the address did not move. The claim is
`verified`; the gate passes.

That is not an obscure corner, it is the shape the field reference teaches. `docs/claim-format.md:99-101`
tells an author of a prohibition to write exactly that: "Negation rides on `modality: SHALL-NOT`
composed with the `attribute`/`value` slots (… operator EQ, value `absent`), never on reworded semantics." — a convention the tool ignores,
which two claims in our own corpus follow (REQ-0015, REQ-0021). Two consequences, both real:

1. **The documentation describes a rule the code does not keep.** That is a defect by this repository's
   own standard ("docs are claims too").
2. **An author can write anything in those slots and nothing notices** — including a value that
   contradicts the statement, which is exactly the contradiction the parity invariant exists to catch.
   The harm is asymmetric in the wrong direction: a prohibition is where a wrong slot is least visible
   and most consequential (sedia's AP-8/AP-9 firewalls are prohibitions).

`attribute` is a genuinely different case and must stay out of this rule: the lexicon is unverified by
design — the diagnostic says so in its own footnote — so comparing an authored attribute name with a
derived token would flag nearly every claim and teach authors to ignore the report.

## Decision

*(The rule below is refined by the Amendment at the end of this file: a slot the parse did not derive
is an **annotation** to be reported, not a contradiction to be flagged. Read both.)*

The parity check compares `value`, `unit` and `operator` against the parse for **every** claim, whatever
the authored value's shape. The numeric-ness of the authored value stops being the condition:

- an authored `value`/`unit` that the parse did not derive, or a derived one the author omitted, is a
  **flag**; so is an authored `operator` that differs from the derived one;
- `req canon --fill` therefore **refuses** (exit 3) a prohibition whose slots contradict its statement,
  exactly as it refuses a numeric contradiction today;
- `attribute` remains unverified, and both the diagnostic and the field reference say so explicitly;
- `docs/claim-format.md` stops instructing `value: absent`: a statement that carries no quantity leaves
  `value` and `unit` empty. The two claims that follow the retired convention are migrated in the same
  change.

## Consequences

- **No `RULES_V` bump and no address moves.** The canonical *form* is derived from the statement and is
  unchanged; only the checking of *authored* slots changes. Measured: emptying REQ-0015's `value` left
  its address at `4a15b28c…` and `canon --verify` at `verified 21, mismatches 0`.
- The migration is two files (REQ-0015, REQ-0021): empty the `value`, and align `operator` with what the
  parse derives — `req canon` names the derived value when it flags, so the migration is mechanical and
  checkable rather than guessed.
- A red-first test: the four mutations measured above must each produce a flag on a prohibition, and
  `--fill` must refuse; today all four pass silently. Mutation-check the rule afterwards by deleting the
  comparison and confirming the test fails.
- Worth knowing: `--fill`'s refusal message must say what to do ("a statement with no quantity leaves
  `value`/`unit` empty"), because "authored slots and parse disagree" is a true but unhelpful sentence
  for a prohibition.
- **Falsification:** author `value: present` on REQ-0015, and `operator: GE` on REQ-0021, in the
  committed corpus; `req canon` must flag both and `req canon --fill` must refuse both. If either passes,
  this ADR has not landed.

---

## Amendment (2026-10-20) — the rule is narrower than the Decision above says, and why

The Decision was written from four mutations on **one** claim (REQ-0015), then generalised to "every
claim, whatever its shape". Prototyped against the whole corpus before implementation, that rule flags
**31 slots across 17 of 21 claims**. The reason is a convention the ADR had not looked at:

```
REQ-0001  attribute: unit_of_work                       operator: EQ   value: claim
REQ-0002  attribute: decision_records                   operator: EQ   value: first_class_signed_objects
REQ-0015  attribute: continuous_bidirectional_file_sync operator: EQ   value: absent
```

The corpus states a **qualitative triple** — `attribute = value` — and the canonical form derives
`value` and `operator` **only for quantities**. For every qualitative claim the parsed side is empty, so
the literal rule calls an annotation a contradiction; applied as written, its "migration" would have
deleted the authored `value` and `operator` from 17 claims — deleting meaning that the identity never
carried, to satisfy a check about identity.

**The corrected rule separates the two cases — which is what the check was always for:**

| the parse derived a value/operator? | the author wrote one? | verdict |
|---|---|---|
| yes | yes, and they agree | ok |
| yes | yes, and they disagree | **flag** — a contradiction |
| yes | no | **flag** — an omission |
| no | yes | **annotation** — reported and counted, never a flag, never silent |
| no | no | ok |

- a **contradiction** and an **omission** are refused by `--fill`, exactly as today;
- an **annotation** is made visible in both places the exemption used to hide: the per-claim diagnostic
  cell is distinct from `ok` and from `FLAG`, and the summary line counts the class
  (`annotations: N`). F2's harm was *invisibility*, and this removes it even where nothing is wrong.

  **Measurement note (2026-10-20, at implementation).** The draft above said 17; the corpus measures
  **16**. The arithmetic is the oracle: 21 claims, less the four numeric ones (REQ-0008–0011), less the
  one `UNSTRUCTURED` claim whose slots are null by design (REQ-0007) = 16 qualitative triples. The 17
  was the naive prototype's count of *flagged* claims, and that count included REQ-0007 — the sentinel
  false positive this amendment exists to prevent, so it could not survive the corrected rule. The
  measurement changes no rule, and it is recorded rather than quietly restated;
- `docs/claim-format.md` documents the annotation convention and retires the `value: absent`
  instruction, which is one instance of the convention rather than a rule of its own;
- `attribute` stays unverified by design: its lexicon has no parser to agree with, so "unverified" is
  the honest verdict for it and a flag would teach authors to ignore the report.

**Two implementation details that decide whether this rule is applied as written.** `Form::operator_str()`
returns the **sentinel `"none"`**, never an empty string (`src/canon.rs:258-263`), and the unit is
defaulted the same way — so "the parse derived one" is `op_parsed != "none"`, not "is non-empty".
Measured consequence: a claim whose `operator`/`value` is `null` (REQ-0007, the `UNSTRUCTURED` one) must
read **ok**, not flag, because its authored side is empty *and* the parse derived nothing. The prototype
that flagged it did so by comparing the sentinel against emptiness — the exact mistake this paragraph
exists to prevent.

**No corpus migration.** Under the corrected rule REQ-0015 and REQ-0021 keep `value: absent` — it is a
faithful annotation, so the Consequences' "the migration is two files" paragraph is **superseded by this
Amendment**: the corpus needs no edit at all, only the diagnostic, its summary, the field reference and
their tests change.

**No `RULES_V` bump and no address moves** — unchanged from the Decision: the canonical form and the file
format are untouched. Only the diagnostic's judgement of authored slots changes, and how it reports the
ones that are neither a defect nor represented.

**Falsification** (unchanged, and still the acceptance test): `value: present` on REQ-0015 and
`operator: GE` on REQ-0021 must each be flagged, and `--fill` must refuse both. Plus the annotation
direction this amendment adds: with the corpus as it stands, `req canon` must report `discrepancy flags:
0` **and** count the annotations — a rule that fixed the flags by going silent would fail the second
half.
