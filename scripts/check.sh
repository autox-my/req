#!/usr/bin/env bash
# scripts/check.sh — req's single local gate.
#
# Runs every check whose tooling exists. Missing tooling prints SKIP, never PASS:
# a check that cannot run is not a passing check, and the commit message must say
# so. The first non-zero status fails the gate.
#
# Usage: scripts/check.sh [all] | [baseline [file]] | [plan]
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
export PATH="$TC:$CACHE_ROOT/tools/ocr/node_modules/.bin:$PATH"
export CARGO_HOME="$CACHE_ROOT/.cargo-home"
export OCR_NO_UPDATE=1

fail=0
dry=0
say()  { printf '\n== %s\n' "$*"; }
skip() { printf 'SKIP  %s\n' "$1"; }
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
  printf 'usage: scripts/check.sh [all] | [baseline [file]] | [plan]\n' >&2
}

# One home for the committed Baseline the gate verifies: the full run and the standalone
# step both default to it, so the two can never drift apart.
baseline_default="baselines/air-gapped.json"
baseline_file="$baseline_default"
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

finish() {
  printf '\n----------------------------------------\n'
  if [ "$dry" -eq 1 ]; then
    # No check was executed, so nothing was checked: exit 3, and never a pass.
    printf 'GATE: UNVERIFIABLE (plan only: no check was executed)\n'
    exit 3
  fi
  if [ "$step" = baseline ]; then
    # One check ran, not the gate. The banner says which, so a caller keying on the
    # full gate's verdict cannot mistake this for one.
    if [ $fail -eq 0 ]; then
      printf 'BASELINE STEP: PASS (one check; the gate did not run)\n'
    else
      printf 'BASELINE STEP: FAIL\n'
    fi
    exit $fail
  fi
  if [ $fail -eq 0 ]; then
    printf 'GATE: PASS (skips above are not passes)\n'
  else
    printf 'GATE: FAIL\n'
  fi
  exit $fail
}

if [ "$step" = baseline ]; then
  verify_baseline "$baseline_file"
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

if [ "$dry" -eq 1 ] || command -v cargo-clippy >/dev/null 2>&1; then
  run cargo clippy --all-targets -- -D warnings
else
  skip "clippy not installed (rustup component add clippy)"
fi

run cargo test
run cargo run -q -- canon   # exit 3 = UNVERIFIABLE (never a pass)
# The stored addresses must match the current rules and recompute from the stored
# bytes: a rules bump without a re-fill exits 3 (candidates), a hand-edited digest
# exits 1 (mismatch). This is REQ-0017 enforced, not asserted.
run cargo run -q -- canon --verify
# The committed Baseline (F5): the gate re-checks what a human signed. See
# verify_baseline above for what each exit means — 1 and 3 both block.
verify_baseline "$baseline_file"
run cargo run -q -- bench   # seed parity corpus: 0 PASS / 1 FAIL / 3 UNVERIFIABLE
# External corpora (other projects' real requirement prose, labelled by
# construction). Same hard gate; see docs/canonical-hashing.md 6.3. v2 adds the
# predicate-swap class, which caught a silent false merge during rules v2.
run cargo run -q -- bench bench/corpus/external-v1.yml
run cargo run -q -- bench bench/corpus/external-v2.yml
# v3 widens the prose: the PROMISE NFR set (deterministic sample of 969 statements
# across 47 project ids) plus StrictDoc and Doorstop, and adds the digit-grouping and
# exact unit-scale equivalence classes. It caught the "may not" false merge (rules v4).
run cargo run -q -- bench bench/corpus/external-v3.yml
# The direction gold set: same hard FMR gate plus the direction gate — a loosening
# reported as no change (or as the opposite direction) is a dangerous confusion and
# fails, exactly as a false merge does. See docs/canonical-hashing.md 6.3.
run cargo run -q -- bench bench/corpus/direction-v1.yml

# OCR is a review tool, not a gate: report the pin so version drift is visible. A plan
# reports the rule-check it would run, but probes nothing — so `plan` executes no command
# at all beyond the environment setup above.
if [ "$dry" -eq 1 ] || command -v ocr >/dev/null 2>&1; then
  if [ "$dry" -eq 0 ]; then
    say "ocr (review tool, not a gate)"
    ocr --version | head -1
    # $PWD, not $CACHE_ROOT: the pin is repo content, and from a worktree $CACHE_ROOT is
    # the main checkout — the 1.13.0 pin printed its own `ocr --version` 1.13.0 beside the
    # main tree's `pinned: 1.12.12`. $CACHE_ROOT is for tool *state* (PATH above, cargo home).
    pin="$(grep -o '"@alibaba-group/open-code-review": *"[^"]*"' \
      "$PWD/tools/ocr/package.json" 2>/dev/null | cut -d'"' -f4)"
    printf 'pinned: %s\n' "${pin:-unknown}"
  fi
  # The rule file is load-bearing for review, and this CLI ignores unknown keys and
  # silently falls back for a glob that matches nothing — so its *effect* is checked,
  # never its exit status.
  if [ "$dry" -eq 1 ] || { [ -f "$PWD/review/rule.json" ] && command -v python3 >/dev/null 2>&1; }; then
    # $PWD, not $CACHE_ROOT: this is repo content, and a worktree's copy is the one
    # under test. $CACHE_ROOT is for tool *state* (cargo home, node_modules) — a script
    # resolved there does not exist until the branch is merged, which the gate caught
    # the first time it ran.
    run python3 "$PWD/tools/review/check_rules.py"
    # A rule that matches a path is not a rule that reaches the reviewer: the CLI also
    # decides *which* files are selected, and review.yml's --exclude is the single string
    # that wires review/rule.json's .forgejo/** checklist to the merge-blocking path. A
    # checklist whose paths are filtered out upstream looks exactly like one that fired,
    # so the workflow's own exclude is driven here and what reached the model is asserted.
    run python3 "$PWD/tools/review/check_review_scope.py"
    # The same check reads review/rule.json by hand, and `rule` is optional to JSON: a
    # matched entry carrying no usable text must be *reported*, never raised into a
    # traceback and an exit code nobody chose. Table-driven, and it runs no review.
    run python3 "$PWD/tools/review/check_review_scope.py" --self-check
  else
    skip "python3 or review/rule.json missing: cannot verify rule globs"
  fi
else
  skip "ocr not installed: npm --prefix tools/ocr install --cache .npm-cache"
fi

finish
