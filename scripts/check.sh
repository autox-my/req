#!/usr/bin/env bash
# scripts/check.sh — req's single local gate.
#
# Runs every check whose tooling exists. Missing tooling prints SKIP, never PASS:
# a check that cannot run is not a passing check, and the commit message must say
# so. The first non-zero status fails the gate.
#
# Usage: scripts/check.sh [--fast] [all|plan] | [baseline [file]] | [decision]
#   no argument, or `all`
#                      every check (the default; CI uses the no-argument form)
#   baseline [file]    only the Baseline step (file defaults to
#                      baselines/air-gapped.json).
#                      The step exists so its failure modes are testable —
#                      tests/baseline.rs drives it against tampered and unsigned
#                      fixtures — and so one check can be re-run alone. It must
#                      never recurse into the full gate, which runs `cargo test`,
#                      and it reports its own verdict (`BASELINE STEP: PASS/FAIL`),
#                      never the gate's banner: one check is not the gate.
#   decision           only the decision-log step.
#                      The same shape as `baseline`: tests/decision.rs drives it
#                      against a tampered and an empty log, and it reports
#                      `DECISION STEP: PASS/FAIL`, never the gate's banner.
#   --fast             run the full gate without the checks that cost the most to *run*
#                      and tell you the least mid-iteration: `clippy`, and the external
#                      corpora (v1-v3) with the direction gold set. **Iteration only.**
#                      The banner reads `GATE: PASS (FAST …)`, never the plain one, and a
#                      commit still requires the full run — a fast green is not a merge
#                      verdict. Rejected beside `baseline`/`decision` (one check is not the
#                      gate, and neither is fast) and unknown flags stay usage errors.
#   plan               print the command of every check the default run would execute,
#                      from the same call sites, and execute no check. The probes that
#                      gate a check are not consulted, so the plan lists a check whose
#                      tooling is missing now. Its verdict is UNVERIFIABLE (exit 3): a
#                      plan is not a pass. tests/baseline.rs uses it to pin that the
#                      default run still verifies the Baseline — the step branch returns
#                      before that call site.
#
# Never read a status through a pipeline (`cmd | tail` reports tail's status, not
# the command's). The sandbox denies writes to $HOME, so all tool state lives in
# the main checkout; a feature worktree shares it through git's common dir.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

# Shared state root = the main checkout (worktrees resolve to it too).
COMMON_DIR="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"
CACHE_ROOT="$(dirname "${COMMON_DIR:-$PWD/.git}")"

# Toolchain: rust-toolchain.toml pins the channel, and the components the gate
# needs (rustfmt, clippy) live in that toolchain — not in `stable`. Resolve the
# directory instead of hardcoding one; fail loudly if there is no cargo at all.
CHANNEL="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml 2>/dev/null)"
TC=""
for d in "$HOME/.rustup/toolchains/${CHANNEL}-"*/bin "$HOME/.rustup/toolchains/"*/bin; do
  if [ -x "$d/cargo" ]; then
    TC="$d"
    break
  fi
done
if [ -z "$TC" ]; then
  printf 'FAIL  no rust toolchain with cargo under $HOME/.rustup/toolchains\n'
  exit 2
fi
# `.cargo-home/bin` first: the sandbox denies writes to $HOME, so the pinned test runner is
# installed into the workspace's cargo home (see below), and a worktree shares it through the
# git common dir. The ambient PATH is kept last, so CI's `~/.cargo/bin` (where the workflow
# installs the same pin) still resolves when the workspace copy is absent.
export PATH="$TC:$CACHE_ROOT/tools/ocr/node_modules/.bin:$CACHE_ROOT/.cargo-home/bin:$PATH"
export CARGO_HOME="$CACHE_ROOT/.cargo-home"
export OCR_NO_UPDATE=1

# The **nextest pin** (ADR 0019): the gate's test runner is adopted for its JUnit artefact, not
# for speed, and an unpinned runner would make the artefact's shape a thing that drifts.
# `cargo install cargo-nextest --version 0.9.149 --locked` (CI does exactly this); verified at pin
# time, never bumped opportunistically, and NOT a Cargo.lock change — it is tool provenance, like
# the pinned `ocr` CLI.
NEXTEST_PIN="0.9.149"
# The artefact the release gate reads. Removed before the run so a *stale* report can never answer
# for a run that failed to produce one: a report nobody generated is UNVERIFIABLE, never a pass.
#  relocates the build tree (CI does); nextest writes the report under it. The
# default stays **relative**, so the planned and executed lines are the same text wherever the run
# starts from.
JUNIT_REPORT="${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml"

fail=0
dry=0
say()  { printf '\n== %s\n' "$*"; }
skip() { printf 'SKIP  %s\n' "$1"; }
fastskip() {
  # A check `--fast` deliberately did not run. Not a SKIP — the tooling is there — and
  # never a pass: the banner says the run was fast, so a scrollback cannot mistake it
  # for a merge verdict.
  printf 'FAST  %s — skipped: iteration only (run scripts/check.sh before a commit)\n' "$1"
}
run()  {
  # A plan prints what the default run would execute and executes nothing, so the
  # Baseline step's *default-run* call site is testable without running the gate.
  if [ "$dry" -eq 1 ]; then
    printf 'PLAN  %s\n' "$*"
    return 0
  fi
  say "$*"
  "$@"
  rc=$?
  if [ $rc -ne 0 ]; then
    printf 'FAIL  %s (exit %d)\n' "$*" "$rc"
    fail=1
  fi
  return 0
}

# Which checks run: everything by default; one step by name for tests and re-runs.
# `baseline [file]` runs only the Baseline step, so its failure modes are testable —
# tests/baseline.rs drives it against tampered and unsigned fixtures. A stray argument
# is a usage error (exit 2), never a quiet run of a different scope: an unknown step
# name, an extra positional, or a second file are all rejected.
usage() {
  printf 'usage: scripts/check.sh [--fast] [all|plan] | [baseline [file]] | [decision]\n' >&2
}

# One home for the committed Baseline the gate verifies: the full run and the standalone
# step both default to it, so the two can never drift apart.
baseline_default="baselines/air-gapped.json"
baseline_file="$baseline_default"
fast=0
if [ "${1:-}" = "--fast" ]; then
  fast=1
  shift
fi
step="all"
if [ $# -gt 0 ]; then
  step="$1"
  shift
fi
case $step in
  all) ;;
  baseline)
    if [ $# -gt 0 ]; then
      baseline_file="$1"
      shift
      # A path the step cannot mean is a usage error here, where it can be named, rather
      # than a confusing downstream failure: an empty value, or one option-shaped enough
      # that req would read it as its own flag.
      case $baseline_file in
        ""|-*)
          printf 'baseline file must be a path, not %s\n' "'$baseline_file'" >&2
          usage
          exit 2
          ;;
      esac
    fi
    ;;
  plan) dry=1 ;;
  decision) ;;
  *)
    usage
    exit 2
    ;;
esac
if [ $# -gt 0 ]; then
  printf 'unexpected argument: %s\n' "$1" >&2
  usage
  exit 2
fi
# `--fast` scopes the full run (and its plan). A single-step invocation is already one
# check, so the flag would be a no-op there — and a no-op flag is how a caller comes to
# believe they ran something they did not.
if [ "$fast" -eq 1 ] && [ "$step" != all ] && [ "$step" != plan ]; then
  printf -- '--fast applies to the full run or its plan, not to the %s step\n' "$step" >&2
  usage
  exit 2
fi

# [F5] The committed Baseline is part of the release, so the gate re-verifies it on
# every run: a claim edited after signing fails here, not in an auditor's terminal.
# The exit contract is req's, and what the gate does with it is the decision this
# wiring records:
#   0  VERIFIED      pass
#   1  FAIL          blocks — drift, a signature that does not verify, a revoked key
#   3  UNVERIFIABLE  blocks — no verifier installed, an unknown key, no signature
# There is deliberately no SKIP on this step, and a missing minisign is a hard
# requirement, not missing tooling: with no verifier req answers UNVERIFIABLE, and
# "we could not tell" must never be a pass — a skipped check is never a pass. A dev
# machine without it installs it first: `brew install minisign` (Debian:
# `apt-get install minisign`; the CI runner image already ships minisign 0.12).
#
# The step drives the req binary: normally `cargo run -q --` like every other step here.
# The *standalone* step alone also honors REQ_GATE_BIN — the seam tests/baseline.rs uses to
# exercise it against the binary the test run already built, with no second cargo invocation
# (and no second build-dir lock) inside `cargo test`. The full gate never honors it, and an
# override that is not an executable file is refused by name: a stray environment value must
# not be able to redirect the verification the gate exists to perform.
verify_baseline() {
  if [ "$step" = baseline ] && [ -n "${REQ_GATE_BIN:-}" ]; then
    # A regular executable: `-x` alone is also true for a directory, which would fail later
    # as a generic gate failure instead of the usage error promised here.
    if [ ! -f "$REQ_GATE_BIN" ] || [ ! -x "$REQ_GATE_BIN" ]; then
      printf 'REQ_GATE_BIN is not an executable file: %s\n' "$REQ_GATE_BIN" >&2
      exit 2
    fi
    run "$REQ_GATE_BIN" baseline --verify "$1"
  else
    run cargo run -q -- baseline --verify "$1"
  fi
}

# The recorded decisions are part of the record, so the gate re-verifies the log on
# every run: a decision record edited after it was recorded fails here, not in an
# auditor's terminal. The whole point of `--fill`'s refusal (its "recompute over an
# edit would launder it" refusal) is that this check exists. The exit contract is
# req's, and what the gate does with it is the decision this wiring records:
#   0  VALID          pass
#   1  TAMPERED       blocks — a stored hash that does not recompute, or a `prev_hash`
#                     that disagrees with the entry before it: a definite finding
#   3  UNVERIFIABLE   blocks — an unfilled hash, an unparsable file, an empty log, or a
#                     log that does not begin at the genesis anchor
# There is deliberately no SKIP on this step, and a missing `decisions/` directory is
# that last case rather than an error: an empty log verifies nothing, and "we could not
# tell" must never be a pass — a skipped check is never a pass. A consumer adopting this
# wiring needs one ratified Decision first, or the gate stays red until it has one.
#
# The step drives the req binary: normally `cargo run -q --` like every other step here.
# The *standalone* step alone also honors REQ_GATE_BIN — the seam tests/decision.rs uses to
# exercise it against the binary the test run already built, with no second cargo invocation
# (and no second build-dir lock) inside `cargo test`. The full gate never honors it, and an
# override that is not an executable file is refused by name: a stray environment value must
# not be able to redirect the verification the gate exists to perform.
verify_decision() {
  if [ "$step" = decision ] && [ -n "${REQ_GATE_BIN:-}" ]; then
    # A regular executable: `-x` alone is also true for a directory, which would fail later
    # as a generic gate failure instead of the usage error promised here.
    if [ ! -f "$REQ_GATE_BIN" ] || [ ! -x "$REQ_GATE_BIN" ]; then
      printf 'REQ_GATE_BIN is not an executable file: %s\n' "$REQ_GATE_BIN" >&2
      exit 2
    fi
    run "$REQ_GATE_BIN" decision --verify
  else
    run cargo run -q -- decision --verify
  fi
}

finish() {
  printf '\n----------------------------------------\n'
  if [ "$dry" -eq 1 ]; then
    # No check was executed, so nothing was checked: exit 3, and never a pass.
    if [ "$fast" -eq 1 ]; then
      printf 'GATE: UNVERIFIABLE (fast plan only: no check was executed)\n'
    else
      printf 'GATE: UNVERIFIABLE (plan only: no check was executed)\n'
    fi
    exit 3
  fi
  if [ "$step" = baseline ] || [ "$step" = decision ]; then
    # One check ran, not the gate. The banner says which, so a caller keying on the
    # full gate's verdict cannot mistake this for one.
    step_label="$(printf '%s' "$step" | tr '[:lower:]' '[:upper:]')"
    if [ $fail -eq 0 ]; then
      printf '%s STEP: PASS (one check; the gate did not run)\n' "$step_label"
    else
      printf '%s STEP: FAIL\n' "$step_label"
    fi
    exit $fail
  fi
  if [ $fail -eq 0 ]; then
    if [ "$fast" -eq 1 ]; then
      # Not the plain banner: a fast run did not check clippy, the external corpora or the
      # direction gold set, so it is an iteration verdict and must not be quoted as a merge one.
      printf 'GATE: PASS (FAST: iteration only — clippy, the external corpora and direction bench did not run; a full scripts/check.sh is required before a commit)\n'
    else
      printf 'GATE: PASS (skips above are not passes)\n'
    fi
  else
    printf 'GATE: FAIL\n'
  fi
  exit $fail
}

if [ "$step" = baseline ]; then
  verify_baseline "$baseline_file"
  finish
fi

if [ "$step" = decision ]; then
  verify_decision
  finish
fi

# Both components live in the pinned toolchain (resolved above); SKIP only if one
# is genuinely missing, and never count a SKIP as a pass. A plan lists the check
# regardless: the probe answers "can this run now", not "would the gate run it".
if [ "$dry" -eq 1 ] || command -v rustfmt >/dev/null 2>&1; then
  run cargo fmt --all -- --check
else
  skip "rustfmt not installed (rustup component add rustfmt)"
fi

if [ "$fast" -eq 1 ]; then
  fastskip "clippy --all-targets -- -D warnings"
elif [ "$dry" -eq 1 ] || command -v cargo-clippy >/dev/null 2>&1; then
  run cargo clippy --all-targets -- -D warnings
else
  skip "clippy not installed (rustup component add clippy)"
fi

# The suite — and its evidence — in **one run** (ADR 0019): `cargo test` is replaced, not kept
# beside, because one run is one truth. The `ci` profile is the reviewed `.config/nextest.toml`
# (`fail-fast = false`, and the JUnit report under `[profile.ci.junit]` with
# `report-skipped = "ignored"`), so this run writes the report the release gate below reads.
#
# nextest does **not run doctests**. The repository has none today; the gap is named here rather
# than discovered: a future `#[doc]` example needs its own step, or it will be silently unrun.
#
# A *stale* report is deleted first: if this step fails to produce one, the gate must read absence
# (UNVERIFIABLE), never yesterday's pass. `--fast` keeps this step — it is the suite, not a
# runtime-heavy extra — so it is never a `FAST` skip.
if [ "$dry" -eq 0 ]; then
  rm -f "$JUNIT_REPORT"
fi
if [ "$dry" -eq 1 ] || command -v cargo-nextest >/dev/null 2>&1; then
  # Verified at pin time: a different installed version would write a different artefact shape,
  # so it is a FAIL naming both versions, never a silent substitution. The install command is the
  # remedy, exactly as the ocr pin prints its own.
  if [ "$dry" -eq 0 ]; then
    installed="$(cargo nextest --version 2>/dev/null | sed -n 's/^cargo-nextest \([0-9][^ ]*\).*/\1/p' | head -1)"
    if [ "$installed" != "$NEXTEST_PIN" ]; then
      printf 'FAIL  cargo-nextest %s is not the pinned %s: install it with `cargo install cargo-nextest --version %s --locked`\n' "${installed:-<none>}" "$NEXTEST_PIN" "$NEXTEST_PIN"
      fail=1
    fi
    printf 'nextest pinned: %s (installed: %s)\n' "$NEXTEST_PIN" "${installed:-unknown}"
  fi
  run cargo nextest run --profile ci
else
  # A skipped check is never a pass. CI installs the pin, so this SKIP makes the job red there.
  skip "cargo-nextest not installed (cargo install cargo-nextest --version $NEXTEST_PIN --locked)"
fi
# The Tier-2 research crate (`experiments/tier2/`, ADR 0021) is kept COMPILING and its own tests
# green, never made a gate: it is a standalone crate with its own Cargo.lock (`--locked`: a stale
# lock is a failure here, not a quiet rewrite), it is not in the root workspace, and neither of
# these commands feeds the JUnit report the release gate reads — plain `cargo test`, not nextest,
# precisely so the evidence of the core suite stays the core suite's. Full run only: `--fast` prints
# a `FAST` line, because a fast green is an iteration verdict and the crate's build is the cost.
if [ "$fast" -eq 1 ]; then
  fastskip "experiments/tier2: cargo check --all-targets and the crate's tests"
else
  run cargo check --locked --manifest-path experiments/tier2/Cargo.toml --all-targets
  run cargo test --locked --manifest-path experiments/tier2/Cargo.toml
fi
# The corpus loads under the strict req.claim/2 loader and every claim's parse is derived
# (exit 3 = UNVERIFIABLE, never a pass). Nothing derived is stored in a claim file (ADR 0022),
# so there is no `canon --verify` step: the pins are held by each release's `req gate`
# (`pins_hold`), and every address is re-derived against the signed manifest just below.
run cargo run -q -- canon
# The committed Baseline (F5): the gate re-checks what a human signed. See
# verify_baseline above for what each exit means — 1 and 3 both block.
verify_baseline "$baseline_file"
# The decision log: the gate re-checks the recorded decisions. See verify_decision above
# for what each exit means — 1 and 3 both block.
verify_decision

# `--format json` (pack C): each of the four commands the pack scopes must emit a document a
# consumer can parse. The step reads stdout and parses it; it deliberately **ignores the
# command's exit code**, which the steps above already judge — a bare `gate` exits 3 on this
# corpus and an unsigned `audit` exits 3 by design. This step's claim is "the document parses",
# not "the verdict is green", so an open verdict is not a failure here while an absent or
# unparseable document is. python3 missing is SKIP, never PASS.
json_smoke() {
  if [ "$dry" -eq 1 ]; then
    printf 'PLAN  cargo run -q -- %s --format json | python3 -c json.load\n' "$*"
    return 0
  fi
  # stderr carries the corpus-root line, never part of the document.
  json_out="$(cargo run -q -- "$@" --format json 2>/dev/null)"
  if [ -z "$json_out" ]; then
    printf 'FAIL  --format json emitted no document for `%s`\n' "$*"
    fail=1
    return 0
  fi
  if printf '%s' "$json_out" | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null; then
    printf 'PASS  --format json parses: req %s\n' "$*"
  else
    printf 'FAIL  --format json is not parseable JSON: req %s\n' "$*"
    fail=1
  fi
}

if [ "$dry" -eq 1 ] || command -v python3 >/dev/null 2>&1; then
  say "the --format json documents parse (render, trace, gate, audit)"
  json_smoke render
  json_smoke trace
  json_smoke gate
  json_smoke audit
else
  skip "python3 missing: cannot parse-check the --format json documents"
fi

run cargo run -q -- bench   # seed parity corpus: 0 PASS / 1 FAIL / 3 UNVERIFIABLE
# External corpora (other projects' real requirement prose, labelled by
# construction). Same hard gate; see docs/canonical-hashing.md 6.3. v2 adds the
# predicate-swap class, which caught a silent false merge during rules v2.
  # `--fast` skips these four here, and only here: they are the gate's largest *runtime*
  # cost (~19s of an 85s run, measured), and their corpora do not move mid-iteration.
  # The seed parity bench above is not skipped — it is the gate that matters, and it is 1s.
  if [ "$fast" -eq 1 ]; then
    fastskip "the external corpora (v1-v3) and the direction gold set"
  else
  run cargo run -q -- bench bench/corpus/external-v1.yml
  run cargo run -q -- bench bench/corpus/external-v2.yml
  # v3 widens the prose: the PROMISE NFR set (deterministic sample of 969 statements
  # across 47 project ids) plus StrictDoc and Doorstop, and adds the digit-grouping and
  # exact unit-scale equivalence classes. It caught the "may not" false merge (rules v4).
  run cargo run -q -- bench bench/corpus/external-v3.yml
  # v4 (rules v6) adds the two classes the dropped-word false merges slipped through: any-position
  # substitution and the possessive head (tools/corpus/build_external.py --version 4).
  run cargo run -q -- bench bench/corpus/external-v4.yml
  # The direction gold set: same hard FMR gate plus the direction gate — a loosening
  # reported as no change (or as the opposite direction) is a dangerous confusion and
  # fails, exactly as a false merge does. See docs/canonical-hashing.md 6.3.
  run cargo run -q -- bench bench/corpus/direction-v1.yml
  fi

# The release gate over this repository's own corpus. `req gate` was the one subcommand the gate
# never invoked, so the six variant-scoped claims in `claims/` were never examined by CI while a
# bare `gate` printed PASS (`15 applying claim(s), 6 undecided`, measured). `.req/variants` is this
# repository's own list — one `key=value ...` release per line, and the reader is this script,
# because req knows nothing about the file.
#
# Each line is a **complete** release context, and since ADR 0015 a green line means exactly that:
# an undecided claim exits 3, so a dropped `set`, `deployment_mode` or `field` reddens this
# step instead of passing silently — and the gate names the claim it cost. What no single line can
# see is the property *across* lines — a claim that applies in no declared release is examined by
# none of them — and that is what `tests/release_gate.rs` adds with
# `every_claim_applies_in_at_least_one_declared_release`. Its per-line twin asserts exit 0 and
# `, 0 undecided` for the declared lines — which proves today's lines are complete, not that
# `exit_code` honours the count: a `gate` that stopped counting undecided would leave these lines
# green. What fails in that case is the fixture tests that assert exit 3 for an undecided corpus:
# `tests/release_gate.rs::a_bare_gate_that_leaves_a_claim_undecided_is_unverifiable` and
# `::a_ge_with_a_non_decimal_bound_is_named_not_left_to_a_context`.
if [ -r "$PWD/.req/variants" ]; then
  variants_gated=0
  # `|| [ -n ... ]`: `read` returns non-zero on a final line with no trailing newline while still
  # assigning it, and a release appended without one would otherwise be silently dropped.
  while IFS= read -r variant_line || [ -n "${variant_line}" ]; do
    # The tests' reader strips a trailing CR (`str::lines` plus `trim`); `read -r` keeps it, so a
    # CRLF checkout would gate a `\r`-suffixed value the tests never see.
    variant_line="${variant_line%$'\r'}"
    # `read -a` splits on IFS and does **not** run pathname expansion: a bare expansion would
    # glob-expand a value carrying `*`, `?` or `[` against the working directory.
    read -r -a release <<< "${variant_line%%#*}"
    [ "${#release[@]}" -gt 0 ] || continue
    # </dev/null: nothing in the body may consume the remaining releases from this file's stdin.
    # The evidence the policy reads: the report the test step just wrote. `req gate` without it
    # answers UNVERIFIABLE (a policy that cannot be exercised is not a pass), and a report the
    # run did not produce is absence, not "no failures found".
    run cargo run -q -- gate --test-report "$JUNIT_REPORT" "${release[@]}" < /dev/null
    variants_gated=$((variants_gated + 1))
  done < "$PWD/.req/variants"
  # "Nothing was examined" is not a pass. A file of only comments iterates zero times, and an
  # unreadable file makes the redirection at `done` fail — the loop then does not execute, its
  # non-zero status is discarded (`set -e` is off here), and without this the banner would print
  # PASS with the release gate never run.
  if [ "$variants_gated" -eq 0 ]; then
    printf 'FAIL  .req/variants declares no release: nothing was gated, and a skipped check is never a pass\n'
    fail=1
  fi
else
  # Not a SKIP. The file is tracked, so its absence is a defect — and a skipped check is never a
  # pass, which is the invariant this script exists to keep.
  printf 'FAIL  .req/variants is missing or unreadable: the release gate cannot run, and a skipped check is never a pass\n'
  fail=1
fi

# Committed projections (ADR 0018): a document generated from the corpus is checked by
# regeneration, never trusted about itself, and never hand-edited in place — a difference is
# exit 1, with the first differing line named. `.req/projections` is this repository's pinned
# list — one repo-relative path per line, `#` comments, blank lines skipped, exactly the
# `.req/variants` grammar above — and the reader is this script, because req knows nothing about
# the file. The list is deliberately small (ADR 0018 §4): a projection is committed where it is a
# *deliverable*, not as a cache of every view, and adding a path is a decision.
#
# Conditional on purpose: a project that commits no projection has nothing to check, so an absent
# `.req/projections` is not a failed check. But "no list" and "a list this script cannot read" are
# different states: a file that exists and is not readable (mode 000, a dangling link, a bad mount)
# is a **pinned list that became unusable**, and it must be red — the `.req/variants` step makes the
# same distinction beside it, and `[ -r ]` alone would fold the two into the tolerated one. Once the
# list is readable the step is the gate's, and two conditions make "nothing was checked" red
# instead of green — the release-gate step's lesson: a listed path that no longer exists (a
# projection that vanished is silent decay), and a list that yielded no path at all (a comment-only
# file examined nothing). The step prints how many projections it checked, so neither case can read
# as a pass.
projections_file="$PWD/.req/projections"
if [ ! -e "$projections_file" ] && [ ! -L "$projections_file" ]; then
  printf 'projections: none pinned (.req/projections absent; nothing to check)\n'
elif [ ! -f "$projections_file" ] || [ ! -r "$projections_file" ]; then
  # Present but unusable is not absent: a pinned list that cannot be examined must be red, or the
  # gate reports "nothing pinned" about a path that is right there. `-L` covers a dangling link
  # (`-e` cannot see through it), and `-f` refuses a directory, FIFO or device — a directory would
  # otherwise fail the loop's redirection with a raw bash error and be blamed as "pins no
  # projection", and a FIFO would block the gate forever.
  printf 'FAIL  .req/projections is present but is not a readable regular file: the pinned list cannot be examined, and a skipped check is never a pass\n'
  fail=1
else
  projections_checked=0
  projection_entries=0
  while IFS= read -r projection_line || [ -n "${projection_line}" ]; do
    # A CRLF checkout would otherwise ask for a path ending in `\r`.
    projection_line="${projection_line%$'\r'}"
    # One path per line: `read` trims the surrounding whitespace and takes the first word. More
    # than one word is named rather than half-checked — a path containing whitespace would be
    # silently truncated to its first word, and the step would check a file nobody listed.
    read -r projection_path projection_rest <<< "${projection_line%%#*}"
    [ -n "$projection_path" ] || continue
    projection_entries=$((projection_entries + 1))
    if [ -n "$projection_rest" ]; then
      printf 'FAIL  .req/projections line names more than one word (one repo-relative path per line): %s\n' "$projection_line"
      fail=1
      continue
    fi
    if [ ! -f "$PWD/$projection_path" ]; then
      printf 'FAIL  .req/projections lists a projection that is missing: %s\n' "$projection_path"
      fail=1
      continue
    fi
    # </dev/null: nothing in the body may consume the remaining paths from this file's stdin.
    run cargo run -q -- render --check "$projection_path" < /dev/null
    projections_checked=$((projections_checked + 1))
  done < "$projections_file"
  if [ "$dry" -eq 1 ]; then
    # `plan` executes nothing, so it must not report a count that reads as checks performed.
    printf 'projections planned: %d (nothing executed)\n' "$projections_checked"
  else
    printf 'projections checked: %d\n' "$projections_checked"
  fi
  # "Nothing was examined" is not a pass, and the reason is the *list*, not a missing file: a
  # comment-only list iterates zero times. A list whose every entry was reported missing already
  # failed above, and saying "pins no projection" there would name the wrong reason twice.
  if [ "$projection_entries" -eq 0 ]; then
    printf 'FAIL  .req/projections pins no projection: nothing was checked, and a skipped check is never a pass\n'
    fail=1
  fi
fi

# The document extractor's pure half (tools/adopt.py): the two conventions, the heading scope,
# the skip reasons, the id-ignored rule, the next-free-number scan and the draft template. It is
# table-driven and needs no filesystem, no git and no `req`, so `--fast` keeps it — the contract
# it encodes is the claim format itself, and its filesystem half is `tests/adopt.rs`. A missing
# python3 or script prints SKIP, never PASS.
if [ "$dry" -eq 1 ] || { [ -f "$PWD/tools/adopt.py" ] && command -v python3 >/dev/null 2>&1; }; then
  run python3 "$PWD/tools/adopt.py" --self-check
else
  skip "python3 or tools/adopt.py missing: cannot self-check the document extractor"
fi

# The claim migrator's table (tools/migrate_claims.py, ADR 0022): what a req.claim/1 file keeps,
# which authored slots become pins, what it drops and what it refuses. No `req` and no repository
# writes (its `run()` cases work in a cleaned-up temp dir), so `--fast` keeps it. A missing python3 or script prints SKIP, never PASS.
if [ "$dry" -eq 1 ] || { [ -f "$PWD/tools/migrate_claims.py" ] && command -v python3 >/dev/null 2>&1; }; then
  run python3 "$PWD/tools/migrate_claims.py" --self-check
else
  skip "python3 or tools/migrate_claims.py missing: cannot self-check the claim migrator"
fi

# The task-envelope validator's table (tools/orchestration/validate_envelope.py): the report
# contract every delegated worker answers with (AGENTS.md, Orchestration). Pure, no filesystem
# and no `req`, so `--fast` keeps it. A missing python3 or script prints SKIP, never PASS.
if [ "$dry" -eq 1 ] || { [ -f "$PWD/tools/orchestration/validate_envelope.py" ] && command -v python3 >/dev/null 2>&1; }; then
  run python3 "$PWD/tools/orchestration/validate_envelope.py" --self-check
else
  skip "python3 or tools/orchestration/validate_envelope.py missing: cannot self-check the envelope validator"
fi

# The external-corpus generator's pure half (tools/corpus/build_external.py): the registry-alias
# digest (order-sensitive, alias-list only), the per-class counter arithmetic and the header
# sentence. No network, no corpus, no `req`, so `--fast` keeps it. A missing python3 or script
# prints SKIP, never PASS.
if [ "$dry" -eq 1 ] || { [ -f "$PWD/tools/corpus/build_external.py" ] && command -v python3 >/dev/null 2>&1; }; then
  run python3 "$PWD/tools/corpus/build_external.py" --self-check
else
  skip "python3 or tools/corpus/build_external.py missing: cannot self-check the corpus generator"
fi

# The paraphrase probe (experiments/paraphrase, P-1): a standalone research crate that depends on
# `req` by path and has its own Cargo.lock, so it can never move the root pins. Its `--self-check`
# (the Wilson interval, the difference-shape bucketing, the judge filter, the sampler, the
# validators and the 3-statement fixture end to end) is measured at ~0.3s warm and ~5s for the
# crate's first build in a target dir, so `--fast` keeps it. It never feeds the release gate: the
# experiment's *results* are not a check, only its instrument is. `--locked`: a lockfile the build
# would have to rewrite is a failure, not a silent update. The build tree is shared with the root
# crate (CARGO_TARGET_DIR, else ./target). Missing rustfmt/clippy print SKIP, never PASS.
paraphrase_manifest="experiments/paraphrase/Cargo.toml"
if [ "$dry" -eq 1 ] || [ -f "$PWD/$paraphrase_manifest" ]; then
  paraphrase_target="${CARGO_TARGET_DIR:-$PWD/target}"
  if [ "$dry" -eq 1 ] || command -v rustfmt >/dev/null 2>&1; then
    run cargo fmt --manifest-path "$paraphrase_manifest" -- --check
  else
    skip "rustfmt not installed: cannot format-check experiments/paraphrase"
  fi
  if [ "$fast" -eq 1 ]; then
    fastskip "clippy for experiments/paraphrase"
  elif [ "$dry" -eq 1 ] || command -v cargo-clippy >/dev/null 2>&1; then
    run env CARGO_TARGET_DIR="$paraphrase_target" cargo clippy --locked --manifest-path "$paraphrase_manifest" --all-targets -- -D warnings
  else
    skip "clippy not installed: cannot lint experiments/paraphrase"
  fi
  run env CARGO_TARGET_DIR="$paraphrase_target" cargo run -q --locked --manifest-path "$paraphrase_manifest" -- --self-check
else
  # The same shape as the tools' self-checks above: a tree without the experiment (a fixture
  # repo the gate's own tests build) prints SKIP, which is never a pass.
  skip "$paraphrase_manifest missing: cannot self-check the paraphrase probe"
fi

# The five-minute path (`docs/adopting.md`, "For an agent"), rehearsed for real against the binary this run just
# built. `tools/quickstart.sh` does in a `mktemp -d` what the page tells an adopter to do — `req
# init`, the extractor's dry run and `--write`, an authored applicability, `canon` showing the
# derived parse, the declared variant's `gate`, `render --out` then `--check` — offline and with no
# $HOME write, and prints the one line the page's claim is measured by: `QUICKSTART <n>s`. The
# page says "under five minutes", so the gate carries the clock rather than trusting the prose:
# the script fails at n >= 300 and on any step whose wrong-but-zero answer its assertions catch.
#
# Kept under `--fast` too: it is a ~1s step, and a docs claim that quietly stopped being true
# would otherwise wait for a full run. The fast banner still says what did not run, so a fast
# green is never quoted as the gate's verdict.
#
# The binary is bound to this tree's debug build when there is one, so the rehearsal judges what
# the gate just built rather than a stale release. It is NOT a condition for running: a caller
# who redirects CARGO_TARGET_DIR — this script deliberately does not set it — or builds release
# only would otherwise turn the rehearsal into a SKIP inside a green run, and a docs claim that
# goes unchecked is exactly the hole the SKIP/`FAST` distinction exists to show. With no debug
# binary the script's own resolution runs, and failing to find one is a red, never a pass.
if [ "$dry" -eq 1 ] || { [ -f "$PWD/tools/quickstart.sh" ] && command -v python3 >/dev/null 2>&1; }; then
  # `plan` prints this same choice, from this same call site: `-x` picks the command, it does
  # not gate the check, so consulting it in a plan is what makes the plan show the binding the
  # default run uses.
  #
  # The search honours a redirected build dir, the same resolution the rehearsal itself uses: a
  # caller with CARGO_TARGET_DIR set has the binary there, and `$PWD/target` would bind a path
  # that does not exist — turning the binding into a refusal inside the script. The fallback
  # unsets a caller's REQ_BIN: it exists to let the script resolve the tree's own build, and a
  # stray exported value would make the gate rehearse an arbitrary binary (the rule the
  # standalone Baseline and decision steps apply to REQ_GATE_BIN).
  quickstart_target="${CARGO_TARGET_DIR:-$PWD/target}"
  case "$quickstart_target" in
    /*) : ;;
    *) quickstart_target="$PWD/$quickstart_target" ;;
  esac
  if [ -x "$quickstart_target/debug/req" ]; then
    run env REQ_BIN="$quickstart_target/debug/req" bash "$PWD/tools/quickstart.sh"
  else
    run env -u REQ_BIN bash "$PWD/tools/quickstart.sh"
  fi
else
  skip "tools/quickstart.sh or python3 missing: cannot rehearse the quickstart"
fi

finish
