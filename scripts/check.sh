#!/usr/bin/env bash
# scripts/check.sh — req's single local gate.
#
# Runs every check whose tooling exists. Missing tooling prints SKIP, never PASS:
# a check that cannot run is not a passing check, and the commit message must say
# so. The first non-zero status fails the gate.
#
# Usage: scripts/check.sh [baseline [baseline.json]]
#   no argument        every check (the default; CI uses this)
#   baseline [file]    only the Baseline step (file defaults to the committed one).
#                      The step exists so its failure modes are testable —
#                      tests/baseline.rs drives it against tampered and unsigned
#                      fixtures — and so one check can be re-run alone. It must
#                      never recurse into the full gate, which runs `cargo test`.
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
say()  { printf '\n== %s\n' "$*"; }
skip() { printf 'SKIP  %s\n' "$1"; }
run()  {
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
  printf 'usage: scripts/check.sh [baseline [baseline.json]]\n' >&2
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
    fi
    ;;
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
# tests/baseline.rs sets REQ_GATE_BIN to the binary the test run already built, so the
# step is exercised without nesting a second cargo invocation — and a second build-dir
# lock — inside `cargo test`. Ordinary and CI invocations never set it.
verify_baseline() {
  if [ -n "${REQ_GATE_BIN:-}" ]; then
    run "$REQ_GATE_BIN" baseline --verify "$1"
  else
    run cargo run -q -- baseline --verify "$1"
  fi
}

finish() {
  printf '\n----------------------------------------\n'
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
# is genuinely missing, and never count a SKIP as a pass.
if command -v rustfmt >/dev/null 2>&1; then
  run cargo fmt --all -- --check
else
  skip "rustfmt not installed (rustup component add rustfmt)"
fi

if command -v cargo-clippy >/dev/null 2>&1; then
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

# OCR is a review tool, not a gate: report the pin so version drift is visible.
say "ocr (review tool, not a gate)"
if command -v ocr >/dev/null 2>&1; then
  ocr --version | head -1
  pin="$(grep -o '"@alibaba-group/open-code-review": *"[^"]*"' \
    "$CACHE_ROOT/tools/ocr/package.json" 2>/dev/null | cut -d'"' -f4)"
  printf 'pinned: %s\n' "${pin:-unknown}"
  # The rule file is load-bearing for review, and this CLI ignores unknown keys and
  # silently falls back for a glob that matches nothing — so its *effect* is checked,
  # never its exit status.
  if [ -f "$PWD/review/rule.json" ] && command -v python3 >/dev/null 2>&1; then
    # $PWD, not $CACHE_ROOT: this is repo content, and a worktree's copy is the one
    # under test. $CACHE_ROOT is for tool *state* (cargo home, node_modules) — a script
    # resolved there does not exist until the branch is merged, which the gate caught
    # the first time it ran.
    run python3 "$PWD/tools/review/check_rules.py"
  else
    skip "python3 or review/rule.json missing: cannot verify rule globs"
  fi
else
  skip "ocr not installed: npm --prefix tools/ocr install --cache .npm-cache"
fi

finish
