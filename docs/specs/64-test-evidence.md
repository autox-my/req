# #64 — test evidence beyond nextest

Task ids: `64-1` (the reader and the repeated flag), `64-2` (the adapter and the docs; dispatched only after
`64-1` has merged). Code baseline: current `main`. Issue #64; the decision is **[ADR
0024](../adr/0024-test-evidence-is-a-set-of-reports.md)** (proposed; dispatched only after it merges). Read
the ADR first: the contract is there and is not restated here.

**Model:** `claude-sonnet-5-5` (high) for both units. `64-1` is reviewed read-only on a different family
(`64-1-review-…`): the union rule is evidence handling.

## Unit 64-1 — the reader and `--test-report` repeated

1. **Fixtures are already committed** (`tests/fixtures/junit/`, with `README.md`: real pytest 9.2.0 output).
   Do not edit them; if a test needs another shape, add a new fixture beside them and say how it was made.
2. **`src/junit.rs`:** (a) the empty-`classname` refusal names the likely cause: keep the refusal and its
   current wording, and append that pytest writes an empty `classname` for a **collection error** (the run
   was interrupted, so nothing else is evidence); (b) `Report::merge(self, other) -> Result<Report, String>`
   and `Report::load_many(paths: &[&Path]) -> Result<Report, String>`: union by id, identical outcomes
   dedupe, different outcomes are `Err` naming the id and **both files**; an unreadable or unparsable file is
   `Err` naming that file. No new dependency, `Cargo.lock` unchanged.
3. **`src/gate.rs` and `src/check.rs`:** `--test-report <file>` (and `--test-report=<file>`) may be given
   more than once, in `gate` and `check` alike; the value rules stay (empty or option-shaped value is exit
   2). Duplicates of the same path are harmless (identical outcomes dedupe). `check` forwards the whole list
   to every gate step. The report is loaded **once** per command and shared, as today. An evidence error is
   the existing `TestEvidence::Unreadable(reason)`, so the gate's "judged once, not per claim" holds.
   `src/main.rs`: both help pages (`usage:` lines, one sentence on repetition, the union rule, and that one
   unreadable report makes `tests_linked` UNVERIFIABLE).
4. **Tests** (`tests/junit.rs`, `tests/gate.rs` or `tests/evidence.rs`, `tests/check.rs`; the fixtures are
   the oracle, labels from pytest's documented meaning, never from `req`'s output):
   (a) every fixture parses: `pytest-mixed.xml` ids and outcomes are exactly — passed: `tests.test_probe::test_passes`,
   `…::test_param[1]`, `…::test_param[2]`, `…::test_param[a b]`, `tests.test_probe.TestGroup::test_in_class`;
   failed: `…::test_fails`, `…::test_xpass_strict`, `…::test_setup_error`; skipped: `…::test_skipped`,
   `…::test_xfail`; the escapes fixture yields ids `coll.test_param::test_p[a&b]`, `[x<y]`, `[q"r]`;
   (b) the collection-error fixture is refused whole, the message names the file's cause (collection error);
   (c) `req gate` over a fixture corpus linking pytest ids: pass → exit 0, failed → 1, skipped-only → 1
   (ADR 0024 §4, asserted so the consequence is pinned), absent id → 3;
   (d) two reports, disjoint ids → union, a claim linked into each passes; same id same outcome dedupes;
   same id different outcome → UNVERIFIABLE once naming the id and both files; one unreadable file →
   UNVERIFIABLE once naming it, even though the other would have passed; one empty file plus one with cases
   → accepted; both empty → "carries no test cases"; the same path twice → accepted;
   (e) `req check --test-report a --test-report b` gives each gate step the union, and its exit equals the
   standalone `req gate` with the same two flags;
   (f) usage: `--test-report` with no value / empty / option-shaped stays exit 2, in both commands.
5. **Docs in this unit:** only the two help pages; `docs/adopting.md` and the ADR amendment line belong to 64-2.

### Mutation checks (envelope `mutations`)

- Make `merge` keep the first outcome on a conflict → (d) fails.
- Make a failed member load be skipped (merge the rest) → (d) fails.
- Make `check` pass only the first report to its gates → (e) fails.
- Remove the collection-error sentence → (b) fails.

### Allowed paths (64-1)

`src/junit.rs`, `src/gate.rs`, `src/check.rs`, `src/main.rs`, `tests/junit.rs`, `tests/gate.rs`,
`tests/evidence.rs`, `tests/check.rs`, and new files under `tests/fixtures/junit/` (additions only).
Forbidden: `claims/**`, `baselines/**`, `bench/**`, `aliases/**`, `decisions/**`, `review/**`, `.forgejo/**`,
`examples/**`, `docs/**`, `scripts/check.sh`, `src/init.rs`, `tools/**`, root `Cargo.toml` and `Cargo.lock`,
and any change to the canonicalizer, `RULES_V` or an address. No Baseline re-signing.

## Unit 64-2 — the adapter and the docs (after 64-1 merges)

1. **`tools/junit_case.py`** (Python 3 stdlib only, no third-party import; header docstring is the usage and
   the contract). Interface: `junit_case.py --report <file> --classname <C> --name <N> [--timeout <s>]
   [--fresh] -- <command> [args...]`, and `--self-check` (no filesystem outside a temp dir, no network,
   exits 0/1; a missing python3 is the caller's SKIP). Behaviour per ADR 0024 §3: exit 0 → passed; 1 →
   `<failure message="exit 1: <command>">`; 3 → `<skipped message="UNVERIFIABLE (exit 3): …">`; any other
   exit, a negative return code (signal), a missing executable and a timeout → `<error>` with the reason;
   the adapter exits with the command's own status (for a signal or a missing executable: 127 / 128+signal
   as a shell would, and say so in the docstring); output is not captured or altered; the report root is
   `<testsuites><testsuite name="junit_case">…`; the report is rewritten atomically (temp file in the same
   directory, `os.replace`); a case with the same `classname::name` replaces the old row; an existing report
   that is not parseable XML or not this shape is refused (exit 2, nothing written) unless `--fresh`
   (start a new report); empty or control-character `--classname`/`--name` is exit 2. Never mutate a
   report on a usage error.
2. **`scripts/check.sh`:** one step `python3 tools/junit_case.py --self-check`, SKIP (never PASS) if
   python3 or the script is missing, kept by `--fast` like `adopt.py --self-check`.
3. **Tests** (`tests/junit_case.rs`, public seam: `python3 tools/junit_case.py` and the compiled `req`;
   skip with a printed reason if python3 is missing, never pass silently): the exit mapping for 0 / 1 / 3 /
   2 / a missing executable / a timeout / a signal; the adapter's exit equals the command's; replace on a
   re-run; two cases accumulate; an unparsable existing report is refused and left byte-identical;
   `--fresh`; **acceptance:** a claim linked to a case whose command exited 3 is not PASS in `req gate`
   (exit 1 per ADR 0024 §4, message names the id), and linked to a case that exited 0 it passes; the
   adapter's report and `tests/fixtures/junit/pytest-mixed.xml` together through `--test-report` twice
   (needs 64-1).
4. **Docs.** `docs/adopting.md`, section "Link each claim to its proving test": pytest
   (`pytest --junitxml=target/pytest/junit.xml`, what the ids look like, which outcomes map where, the
   collection-error refusal), the adapter with a worked example (a `scripts/check.sh`-style shell check, the
   0/1/3 mapping and the §4 consequence in one sentence), several reports (`--test-report` repeated, the
   union rule, the unreadable-member rule), the libtest named gap and its two routes (ADR 0024 §5); the CI
   section's `--test-report` line gets a sentence saying to repeat it per report. `examples/agents-snippet.md`
   only if it names `--test-report`. `docs/adr/0019-claims-carry-authored-test-links.md` gets a one-line
   "Amended by ADR 0024 (§6)" beside §6. **Do not** change `examples/github-workflows/req.yml`,
   `docs/adopting.md`'s `## CI` yaml block or `src/init.rs`'s snippet (the three-way pin): the single-report
   template stays valid.

### Mutation checks (envelope `mutations`)

- Map exit 3 to passed → the acceptance test fails.
- Make the adapter always exit 0 → the exit-equality test fails.
- Overwrite an unparsable report instead of refusing → the refusal test fails.

### Allowed paths (64-2)

`tools/junit_case.py` (new), `tests/junit_case.rs` (new), `scripts/check.sh` (the one step),
`docs/adopting.md` (the evidence section and one sentence in `## CI` prose outside the yaml block),
`docs/adr/0019-claims-carry-authored-test-links.md` (the one amendment line), `examples/agents-snippet.md`.
Forbidden: everything `64-1` forbids, plus `src/**` and `tests/ci_template.rs`.

## Verify (both units)

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP (python3 is present),
Baseline `[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline or
`bench/results` change.
