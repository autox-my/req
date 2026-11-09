# 0024. Test evidence is a set of reports, and a command's exit can be one

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-04, former Forgejo PR #111); issue #64
(opened from the Sedia evaluation). Date proposed: 2026-11-04. **Amends [ADR 0019](0019-claims-carry-authored-test-links.md) §6**
("`cargo test` is replaced by nextest: one run, one truth"). Nothing else in 0019 moves: the id is still
`classname::name`, a skipped test is still not a pass, and `tests_linked` still has four outcomes.

## Context

ADR 0019 made one JUnit report the evidence and nextest its only producer. A mixed-stack adopter breaks
both halves of that. Sedia's proofs are Rust tests, Python self-tests, and shell checks that exit 0 / 1 /
3; three tools, three places a result lives, and only the first writes JUnit. Today `req gate
--test-report` can see the Rust half and answers UNVERIFIABLE for every claim the other two prove.

Measured, not assumed (2026-11-04, pytest 9.2.0, `tests/fixtures/junit/README.md`): `src/junit.rs`, which
calls itself a reader for nextest's subset, **already reads pytest's `--junitxml` output**: the `<testsuites>`
root, `classname`/`name` ids (`tests.test_probe::test_passes`, `test_param[a b]`, `Class` methods), `&amp;` /
`&lt;` / `&quot;` in names, `<properties>`, `xfail` as `<skipped>`, a strict XPASS and a setup error as
failures. One real case it refuses: a **collection error**, which pytest writes as a `<testcase
classname="">` and which also interrupts the run, so no other test ran. Refusing the report is right; its
message does not say why.

## Decision

1. **pytest's JUnit is supported, and the support is tested.** Fixtures from real pytest runs are
   committed (a pass, a failure, a skip, an xfail, a strict XPASS, parametrized ids, a setup error, a
   collection error), and the reader accepts them unchanged. Other producers stay unpromised until a real
   fixture exists. A collection error stays a refusal of the whole report (never read partially), and its
   message names the cause.
2. **`--test-report` may be repeated.** `req gate` and `req check` read every named report and judge the
   **union**. A test id present in two reports with the same outcome dedupes; with different outcomes the
   evidence is unusable (the reader's existing rule, extended across files). Any report that cannot be
   read or parsed makes `tests_linked` UNVERIFIABLE, once, naming that file, because a test absent from the
   union might be in the one that failed to load. The union is empty → "carries no test cases", as for one
   report. A mixed-stack project passes one report per stack; ids from different stacks do not collide
   because `classname` differs.
3. **A command can be a test case.** `tools/junit_case.py` (stdlib Python, copy-and-adjust like
   `tools/adopt.py`; not part of the `req` binary, which still does not run anything) runs one command and
   records one `<testcase>` in a report it maintains: exit **0 → passed**, **1 → `<failure>`**, **3 →
   `<skipped>`**, anything else (including a signal, a missing executable or a timeout) → **`<error>`**.
   It exits with the command's own status, so wrapping a check never hides its failure. It does not touch
   the command's output. Re-running a case replaces its previous row; an existing report that cannot be
   parsed is refused, never overwritten.
4. **The cost of mapping 3 to skipped, stated.** ADR 0019 §5 reads a claim whose linked tests are all
   skipped as FAIL (exit 1), so a check that said "cannot confirm" (3) is a *defect-class* red in
   `tests_linked`, not exit 3. It is never a pass, which is the property that matters, and the case's
   message says `UNVERIFIABLE (exit 3)`. Changing §5 so skipped-only is UNVERIFIABLE would also move how
   an `#[ignore]`d proving test reads, a policy change this ADR does not make.
5. **libtest is a named gap.** `cargo test` writes no report and its JSON format is unstable. Two routes
   are supported and documented: adopt nextest (ADR 0019 §6, unchanged), or wrap the suite in the adapter
   as **one** case, which links a claim to "the whole suite passed" and says so.

## What stays outside req

req still runs no test, writes no report and signs nothing. The adapter is a tool in `tools/`, not a
subcommand; the reader gains no network and no dependency (`Cargo.lock` unchanged). The report is still
unsigned and the run unattested (ADR 0019 §8 is unchanged and now applies to each report).

## Consequences

- A mixed-stack project can make `tests_linked` see every stack; the CI template's single `--test-report`
  becomes one per report, and `req check` forwards the list to every gate step.
- `Report::load` gains a sibling that merges; no address, `RULES_V` or Baseline moves (`tests_hash` hashes
  the links and the evidence kind, not the reports).
- A wrong link is as costly as before. A shell check wrapped as a case proves only what the script
  checks; linking a claim to it is a claim about evidence, reviewed like any link.

## Rejected

- **Merge the reports in shell or in the adapter.** The merge rule (conflicting duplicate ids are
  unusable) belongs where the reader's other refusals live, and a merge tool would be a second reader.
- **A `req run` subcommand that executes a check.** req would start running things; ADR 0023 keeps it out.
- **3 → absent from the report.** It would give exit 3, but the gate's message would call a deliberate
  "cannot confirm" a "renamed or deleted" test.
