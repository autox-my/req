# #63 — `req check`: one command that folds the repository's release lists

Task ids: `63-1` (the command), `63-2` (the template, the docs and this repository's own gate; dispatched
only after `63-1` has merged). Code baseline: current `main`. Issue #63, re-scoped by the owner's comment
after ADR 0022; the decision is **[ADR 0023](../adr/0023-req-check-folds-the-repositorys-release-lists.md)**
(proposed; this spec is dispatched only after it merges). Read the ADR first: the contract is there and is
not restated here.

**Model:** `claude-sonnet-5-5` (high) for both units. Review of `63-1` on a different family, read-only
(`63-1-review-…`), because the fold is the thing this command exists to get right.

## Why

Four review rounds have found aggregation defects in the adopter shell (exit 3 folded to green, swallowed
exit, unrun loop after a failed redirect, last release line dropped). The fold is a three-line rule in
`src/gate.rs::exit_code`; it should be written once, in Rust, with a test of every combination.

## Unit 63-1 — `req check`

1. **The command** (`src/check.rs`, one module like the others; register it in `src/main.rs`'s `Command`
   table with a `CHECK_HELP` page whose `exit:` line states 0 / 1 / 3 / 2 as ADR 0023 decides).
   - Corpus root as every command (ADR 0012); the two lists are `<root>/.req/variants` and
     `<root>/.req/projections`. Arguments: `--test-report <file>` (and `--test-report=<file>`, empty value
     is exit 2), `--format text|json` (use `format::take_format` as `gate` does). Anything else is exit 2
     with `usage: req check [--test-report <file>] [--format text|json]`; there is **no** flag that narrows
     the steps.
   - **Validate both lists before running anything** (read each into lines, split as ADR 0023 §2): a
     variants line with a token starting `-` or lacking `=` (or with an empty key), or a projections line
     with more than one word, is exit 2 naming `<file>:<line>`; nothing runs. Do this on bytes read once;
     do not re-read between validation and use.
   - **Variants:** absent file → one step, the default variant (bare gate), labelled
     `(no .req/variants: the default variant)`. A path that exists but is not a readable regular file
     (check `metadata().is_file()` and a successful read; a dangling link, a directory and a FIFO — never
     open a FIFO — are all this case) → exit 3 before any step, stating why. A readable file with no line
     → the default variant, labelled `(.req/variants declares no release: the default variant)`.
   - **Gate steps** reuse `gate`'s code: refactor `gate::run`'s evaluation into a function that returns
     the results, the folded code, the undecided list and the `req.gate/2` document without printing, and
     have both `gate::run` and `check` call it. **Do not shell out** and do not copy a policy. The test
     report is loaded once and shared.
   - **Projection steps:** absent `.req/projections` → print `projections: none pinned (.req/projections
     absent; nothing to check)`, no step. Present: unreadable or not a regular file → exit 3; zero paths →
     exit 1 (`pins no projection: nothing was checked`); a path that does not exist → that step is exit 1
     (`missing`); otherwise the step's exit is exactly `req render --check <path>`'s, by calling the code
     `render --check` calls.
   - **Fold:** FAIL outranks UNVERIFIABLE outranks PASS over all steps. Implement it as one small pure
     function over step exits (`fold(&[i32]) -> i32`) with a unit test of every combination of 0/1/3 for
     lengths 0–3; an exit outside {0,1,3} from a step is a bug and is surfaced as exit 1 naming the step
     (never silently folded to anything else).
   - **Text output:** a first line `# req check: <v> variant step(s), <p> projection(s)`; one line per step
     `[PASS|FAIL|UNVERIFIABLE] gate <line> (exit <n>)` / `… render <path> (exit <n>)`, and under a non-pass
     step the policies that were not PASS (name and outcome, one line each) and the undecided count, so the
     line is actionable without rerunning; a last line `verdict: PASS — every declared step ran and passed`
     / `verdict: FAIL — …` / `verdict: UNVERIFIABLE — …` (the gate's wording style). PASS appears nowhere
     else when the verdict is not PASS.
   - **JSON output:** `req.check/1` as ADR 0023 §6 (`schema`, `generator: "check"`, `rules_v`, `verdict`,
     `steps[]` with `kind`, `line` or `path`, `exit`, `verdict`, and `gate` = the `req.gate/2` document for
     gate steps). Same fold, same exit as the text. Add `req.check/1` to wherever `req.gate/2` is declared
     (the schema constants and `docs/` page that lists the schemas, if one does).
2. **Tests** (`tests/check.rs`, new; public seam = the compiled `req` against fixture corpora built in a
   temp dir; expectations from independent oracles — the standalone `req gate` / `req render --check`
   exits run in the same test, and hand-written fixtures — never from `check`'s own output). Red first:
   (a) a variant line that fails (a flagged pin) → 1; (b) a variant left undecided → 3; (c) a missing
   `.req/variants` on a corpus with a selector → 3, and the output names the file; (d) all steps pass →
   0 and `verdict: PASS`; (e) FAIL outranks UNVERIFIABLE: one failing line plus one undecided line → 1,
   and swapping their order changes nothing; (f) every step's exit equals the standalone command's, for
   at least: pass, fail, undecided, and a `--test-report` that is absent/unreadable; (g) list grammar:
   comments, blank lines, CRLF, a final line without a newline, a tab-separated line, a `*` in a value
   (not globbed) — each decided as `scripts/check.sh` decides it; (h) an option-shaped variants line and a
   token without `=` → 2, nothing gated (the output holds no `[PASS]` line); a two-word projections line →
   2; (i) `.req/variants` is a directory / a dangling symlink → 3; (j) projections: absent → `none
   pinned`; a stale file → 1; a missing path → 1; a comment-only list → 1; a `trace` projection listed →
   3 (as `render --check` answers); (k) `--format json` is `req.check/1`, its verdict and the process exit
   agree, and a gate step embeds a document equal to the one `req gate --format json` prints for that
   line; `--format json --out` style misuse is exit 2; (l) `--test-report` with no value, `=` empty, an
   unknown flag, and an extra positional → 2; (m) a comment-only variants file is the default variant
   and says so; (n) no output on a non-PASS verdict contains the string `verdict: PASS`.
   Also extend `tests/cli_version.rs`'s command list and help-page expectations for `check`.
3. **Docs in this unit:** `AGENTS.md` command table row (`check` — may this repository ship: every declared
   variant and projection, folded) and the "Commands" sentence if it enumerates; the `req --help` command
   list; nothing else (63-2 owns the adopter pages).

### Mutation checks (envelope `mutations`)

- Make `fold` return the last step's exit → (e) fails.
- Make `fold` treat 3 as 0 → (b) fails.
- Skip validation (run lines before checking) → (h) fails.
- Treat a missing `.req/variants` as "no steps" → (c) fails.
- Drop the report sharing (load per step but ignore it) → (f) fails.

### Allowed paths (63-1)

`src/check.rs` (new), `src/main.rs`, `src/gate.rs` (the extraction only; no behaviour change — `tests/gate.rs`
and `tests/release_gate.rs` stay green and unedited), the module that owns `render --check` (extraction
only), `src/lib.rs` if modules are declared there, `tests/check.rs` (new), `tests/cli_version.rs`, `AGENTS.md`.
Forbidden: `claims/**`, `baselines/**`, `bench/**`, `aliases/**`, `decisions/**`, `review/**`,
`.forgejo/**`, `examples/**`, `docs/adopting.md`, `scripts/check.sh`, `src/init.rs`, root `Cargo.toml` and
`Cargo.lock`, and any change to the canonicalizer, `RULES_V` or an address. No Baseline re-signing.

## Unit 63-2 — the template, the docs and this repository's own gate (after 63-1 merges)

1. **The CI snippet shrinks.** In `examples/github-workflows/req.yml`, `docs/adopting.md`'s `## CI` block
   and `req::init::CI_SNIPPET`: replace the variants-gate step with one step that runs `req check
   --test-report "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml"` **unpiped**, with no `|| true`, no
   `tee`, no `continue-on-error`, so the step's status is `req check`'s status; keep the job summary (read the
   `verdict:` line from a captured copy as today, without masking the exit). Replace the JSON-views step's
   per-variant loop with one `req check … --format json` capture (`req-views/check.json`), keeping the
   `.tmp`-then-`mv` discipline (a view that wrote nothing leaves no file) and the expected-count check
   (`render`, `trace`, `audit`, `check` = 4). Keep the install, deploy-key, stale-report and nextest steps
   unchanged. The three copies must stay byte-identical (`tests/ci_template.rs`).
2. **`tests/ci_template.rs`** keeps the three-way equality and replaces the pinned fragments of the deleted
   shell with the ones that now matter: the `req check` step exists, is unpiped and unmasked; the stale-report
   invalidation still precedes it with `if: always()`; no copy re-implements the fold (no `rc=1`/`read -a`
   loop over `.req/variants`). **A probe that forces exit 3** (a fixture corpus with a selector and a variants
   file the gate cannot decide, run through the template's `req check` command line) must exit 3 and the test
   asserts the step's command would not turn that into success. Mutation-check the pins.
3. **`scripts/check.sh`** replaces its own `.req/variants` loop and `.req/projections` loop with `run cargo
   run -q -- check --test-report "$JUNIT_REPORT"` (keep `--fast` behaviour, the dry-run `plan` output and
   the `projections checked: N`-style visibility the script prints by reading the command's output, not
   by re-deriving it). One implementation, two invocations. `tests/release_gate.rs` and
   `tests/baseline.rs` assertions about the files stay as they are.
4. **Docs.** `docs/adopting.md`: the CI section prose (what `req check` does, its exit contract, "absent is
   not silent", the grammar of both lists as `req check` reads it), step 5 of the adopting loop, the
   `.req/variants` paragraph; `examples/agents-snippet.md` ("Run, before merging: `req canon`, and `req check`";
   keep the `--fail-on-flag` sentence); `docs/adr/0018-documents-are-projections.md` gets a one-line
   "Amended by ADR 0023: req reads `.req/projections` through `req check`" beside the sentence it amends;
   the header comments of `.req/variants` and `.req/projections` and the `src/init.rs` next-steps text stop
   saying req knows nothing about the files; `examples/handoff-brief.md` / `tests/agent_handoff.rs` if a
   command list there needs `check`.
5. **Do not** change the exit contract, the grammar or any step semantics decided in 63-1; a defect found
   there is a fix unit against 63-1, not an edit here.

### Mutation checks (envelope `mutations`)

- Pipe the template's `req check` into `tee` → the `ci_template` pin fails.
- Re-introduce a shell loop over `.req/variants` in one copy → the three-way equality fails.
- Make `scripts/check.sh` ignore `check`'s status → introduce a deliberately stale `docs/spec.md` in a
  scratch copy and confirm the full gate fails.

### Allowed paths (63-2)

`examples/github-workflows/req.yml`, `examples/agents-snippet.md`, `examples/handoff-brief.md`,
`docs/adopting.md`, `docs/adr/0018-documents-are-projections.md` (the one amendment line),
`src/init.rs` (the snippet and the next-steps text), `tests/ci_template.rs`, `tests/agent_handoff.rs`,
`scripts/check.sh`, `.req/variants` and `.req/projections` (comments only), `tests/init.rs` if its
expectations name the old text. Forbidden: everything `63-1` forbids, plus `src/check.rs`, `src/gate.rs`.

## Verify (both units)

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP, Baseline `[VERIFIED]`
(unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline or `bench/results` change.
