# 0014. An authored slot that contradicts the parse is a defect, whatever its shape

**Status: accepted** — proposed by the agent, ratified by the maintainer on 2026-10-20. The decision answers review finding [F2]. Date proposed: 2026-10-20.
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
