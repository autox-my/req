#!/usr/bin/env bash
# scripts/check.sh — req's single local gate.
#
# Runs every check whose tooling exists. Missing tooling prints SKIP, never PASS:
# a check that cannot run is not a passing check, and the commit message must say
# so. The first non-zero status fails the gate.
#
# Never read a status through a pipeline (`cmd | tail` reports tail's status, not
# the command's). The sandbox denies writes to $HOME, so all tool state lives in
# the main checkout; a feature worktree shares it through git's common dir.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

# Shared state root = the main checkout (worktrees resolve to it too).
COMMON_DIR="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"
CACHE_ROOT="$(dirname "${COMMON_DIR:-$PWD/.git}")"

TC="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"
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

# rustfmt and clippy are NOT part of the minimal rustup profile.
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
run cargo run -q -- bench   # 0 PASS / 1 FAIL / 3 UNVERIFIABLE

# OCR is a review tool, not a gate: report the pin so version drift is visible.
say "ocr (review tool, not a gate)"
if command -v ocr >/dev/null 2>&1; then
  ocr --version | head -1
  pin="$(grep -o '"@alibaba-group/open-code-review": *"[^"]*"' \
    "$CACHE_ROOT/tools/ocr/package.json" 2>/dev/null | cut -d'"' -f4)"
  printf 'pinned: %s\n' "${pin:-unknown}"
else
  skip "ocr not installed: npm --prefix tools/ocr install --cache .npm-cache"
fi

printf '\n----------------------------------------\n'
if [ $fail -eq 0 ]; then
  printf 'GATE: PASS (skips above are not passes)\n'
else
  printf 'GATE: FAIL\n'
fi
exit $fail
