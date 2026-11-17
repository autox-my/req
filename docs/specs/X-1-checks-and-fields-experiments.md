# X-1: two experiments before the checks and typed-fields ADRs

Task id: `X-1`. Base: the commit that adds this spec, on top of `origin/main` at `7526008`. The owner accepted on 2026-11-17 an
evaluation of an external comparative review (a private document; it is not quoted, and none of its text belongs in the
repository). That evaluation recommended running two cheap experiments before committing to either large proposal:
- **repository-defined checks:** a rule language over claims;
- **typed claim fields:** for example ASIL, attested by the Baseline and kept out of the address.

This unit produces **evidence, not features**. Its output is two research records under `docs/research/`. It changes no
behaviour of req.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `AGENTS.md`: "Do not overclaim", "External material is read at named provenance, or as a lead", three-state outcomes,
  Tier-1 determinism.
- ADR 0022: nothing derived lives in a claim.
- ADR 0023: the fold of three outcomes.
- ADR 0005: a signed Decision is the authority.
- ADR 0013: provenance hashes.
- The Baseline envelope (`src/baseline.rs`).

## Experiment 1: can ten realistic rules be written as repository checks without a fork?

The question is whether a small, declarative, three-valued check language over a claim and its derived data can express
the rules an organisation actually imposes, or whether each rule would need a new built-in in Rust.

1. **Write ten rules** of the kind a safety-relevant project imposes on its requirements. **Invent them, generically.** Use
   no real customer, company or project material, and nothing from any private document. They must cover:
   - two rules that depend on a typed field (for example "an ASIL C or D requirement needs a linked test");
   - one on rationale provenance;
   - one on relation cardinality (for example "allocated to at most one component");
   - one on variants (for example "every claim applies in at least one declared variant");
   - one on areas;
   - one on direction or drift (for example "a tightened claim needs a re-recorded satisfaction link");
   - one on lints (for example "no `untestable_qualifier` in a released variant");
   - one on Decisions;
   - one of your choice.
2. **Sketch a minimal check syntax** in the repository's YAML style: `when`, `require`, a severity, and a message. Write
   each rule in it. Where a rule needs data, name the exact source req offers today, and say whether that source exists:
   - a field of `req canon --format json`, `req trace --format json`, `req satisfy --format json` or `req gate --format
     json`, read from each command's `--help` and real output on this repository;
   - or a claim's authored keys.
3. **Classify each rule** as one of:
   - expressible on today's data;
   - expressible once typed fields exist (Experiment 2);
   - needs a new derived datum req does not compute (name it);
   - needs a new built-in function (name it);
   - cannot be expressed three-valued (say why).

   For every rule, say what makes it UNVERIFIABLE rather than PASS: a missing field, an undecided selector, an unreadable
   report.
4. **Check the language against the built-in gate policies.** Can `tests_linked`, `rationale_present` and
   `design_satisfied` be written in the sketch? If one cannot, say what is missing. That shows whether the language is
   too small.
5. **Write `docs/research/checks-expressibility.md`.** Include the ten rules, the syntax sketch, a classification table,
   the data each rule reads, and the built-ins it would need. Close with a verdict that answers this:
   - how many of the ten fit on today's data;
   - how many need typed fields;
   - how many need something new.

   A result like "6 of 10 need a new built-in" is a valid answer, and the record must say so plainly.

## Experiment 2: would a typed field stay out of the address while the Baseline still catches a change to it?

The question is whether req's architecture already separates authored metadata from meaning in a way a `fields:` block
could reuse, and exactly where a field must never enter. **Do not implement `fields:`.** Measure the analogue that exists
today, and map the code.

1. **The analogue, run for real.** `rationale` and `tests` are authored, sit outside the canonical form, and are attested
   by `rationale_hash` and `tests_hash` in the Baseline envelope. On a temporary copy of this repository, never this
   checkout, with the built `req`:
   - Change one claim's `rationale.source`. Record the claim's address before and after (`req canon`, or the JSON view
     that carries it), plus `req baseline --verify` on the copy's `baselines/air-gapped.json`, `req diff`'s class for the
     change, and `req compare` for the pair.
   - Do the same for a `tests:` entry.

   Expected: the address is unchanged; verification fails on that hash; `diff` reports a non-meaning class. Record the
   actual output, and say plainly where the expectation was wrong. The signature check needs the trust anchor; if a step
   cannot run (for example minisign is missing), report it as UNVERIFIABLE.
2. **The code map.** From `src/canon.rs`, `src/model.rs` and `src/baseline.rs`, list:
   - every input to the address computation;
   - the strict-key list the loader enforces, which today refuses an unknown key such as `fields:`;
   - where in the envelope a new attested hash would go, and what a v5 envelope would change for existing signatures.

   Name the functions and the lines.
3. **The risks:** every place a `fields:` value could leak into identity or into a projection's identity digest by
   accident, and the test that would catch each one.
4. **Write `docs/research/fields-separation.md`.** Include the commands run, their real output (trimmed), the code map, the
   risks, and a verdict: is the separation already proven by the analogue, and what would the typed-fields ADR have to add?

## Rules for both records

- **Use no real customer data and quote no private document.** Any example requirement is invented, or taken from req's own
  `claims/`.
- **Every number has its source:** the command, or the file and line.
- **No recommendation stronger than the evidence.** Each record ends with "What this does not show".

## Allowed paths

- `docs/research/checks-expressibility.md` (new)
- `docs/research/fields-separation.md` (new)

Nothing else. Scratch work goes in a temporary directory and is deleted.

## Done when

- Both records exist, and every command in them was run, with output taken from the run.
- The full `scripts/check.sh` is green with no SKIP. It is docs only, but the gate reads `docs/`.
- `git diff <base>...HEAD --name-only` lists only the two records.
