#!/usr/bin/env bash
# scripts/build-release.sh — build one rev of req, prove it, and bundle it (ADR 0025 §1-2).
#
# Usage: scripts/build-release.sh --rev <rev> --out <dir> [--repo <dir>] [--bin <path>]
#   --rev <rev>    the commit to release (anything `git rev-parse` resolves in --repo); required.
#   --out <dir>    where the three assets go (created if absent); required.
#   --repo <dir>   the repository holding <rev> (default: this script's repository).
#   --bin <path>   package this binary instead of building one, so the packaging logic is testable
#                  without a compile. EVERY check below applies to it exactly as to a built binary.
#
# What it does, in this order, and why the order matters:
#   1. A clean copy of the tree at <rev> in a temp directory (`git clone --no-hardlinks`, then a
#      detached checkout). Nothing is ever built or checked in the caller's working tree, so an
#      uncommitted edit cannot reach the artifact.
#   2. Reads the target from `rustc -vV`'s host line: `x86_64-unknown-linux-gnu` or
#      `aarch64-apple-darwin`, else exit 2 naming the host. (A `--bin` built for another target is
#      the caller's problem; the script cannot know it and records the target it assumed in
#      `build-info`.)
#   3. Builds there with `cargo build --release --locked` (the clone brings the pinned
#      `rust-toolchain.toml`; RUSTFLAGS and CARGO_BUILD_TARGET are cleared so the environment cannot
#      change the codegen of a released binary). Without --bin only.
#   4. PROVES the binary before anything is written, running it from the clean copy:
#        built-from       `req --version`'s `built from` is a bare SHA (no `-dirty`, `unknown` or
#                         `-cleanliness-unknown`) and a prefix of the full SHA <rev> resolved to.
#                         That value is <REV> in every name below.
#        baseline-verify  `req baseline --verify baselines/air-gapped.json` exits 0 and prints
#                         `[VERIFIED]`: the 18 pinned addresses are the ones this platform computes.
#        bench-gate       `req bench` (seed parity) exits 0 and its JSON `fmr.hard_gate` is `PASS`.
#                         The `direction` block is UNVERIFIABLE by design on this corpus and is not
#                         judged here.
#      The checks run on a private copy of the binary, and that copy is what is packaged: the bytes
#      that passed are the bytes that ship.
#   5. Writes to <dir>: `req-<REV>-<TARGET>.tar.gz` (one member, `req`, mode 0755, at the root),
#      `req-<REV>-<TARGET>.tar.gz.sha256` (`<hex>  <asset name>`) and `req-<REV>-<TARGET>.build-info`
#      (informational: rev, full sha, target, `rustc -V`, `cargo -V`, the builder's libc line, UTC
#      time). Each file goes to a temporary name in <dir> and is moved into place; a name that
#      already exists is an error, never an overwrite (a checksum that was pinned must stay true).
#      Prints the three paths and a line for an adopter's CI file:
#        pin: REV=<REV> SHA256=<hex> TARGET=<target>
#
# Exit codes (three-state, AGENTS.md; a check that cannot run is never a pass):
#   0  released
#   1  a check failed (built-from, baseline-verify, bench-gate), a build or git step failed, or an
#      output file already exists
#   2  usage, an unresolvable <rev>, an unsupported host, or a tool the script needs is missing
#   3  UNVERIFIABLE: `minisign` is not on PATH (the Baseline step has no SKIP)
# On any non-zero exit nothing is left in <dir> (it is not even created) and no temp directory is
# left behind.
#
# Publishing is NOT done here: uploading a release is the owner's act (ADR 0025 §5).
set -eu

PROG='build-release'
USAGE='usage: scripts/build-release.sh --rev <rev> --out <dir> [--repo <dir>] [--bin <path>]'

work=''
out=''
made_out=0
made_tar=''
made_sha=''
made_info=''
tmp_suffix=".tmp.$$"

# Everything a failed run must take back, then the private temp directory. Runs on every exit
# (including a signal, via the traps below), so an interrupted run leaves no half-written asset.
cleanup() {
  rc=$?
  trap - EXIT
  if [ "$rc" -ne 0 ] && [ -n "$out" ] && [ -d "$out" ]; then
    for f in "$made_tar" "$made_sha" "$made_info"; do
      [ -z "$f" ] || rm -f "$f"
    done
    rm -f "$out"/.req-*"$tmp_suffix"
    if [ "$made_out" -eq 1 ]; then
      rmdir "$out" 2>/dev/null || true
    fi
  fi
  if [ -n "$work" ] && [ -d "$work" ]; then
    rm -rf "$work"
  fi
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

say() { printf '%s: %s\n' "$PROG" "$*" >&2; }
die() {
  code=$1
  shift
  say "$*"
  exit "$code"
}
usage_error() {
  say "$*"
  printf '%s\n' "$USAGE" >&2
  exit 2
}
# Show a captured stream under a failed check, indented, so the cause is in the log.
show() {
  [ ! -s "$2" ] || { say "  $1:"; sed 's/^/    /' "$2" >&2; }
}

# --- arguments -----------------------------------------------------------------------------
rev=''
repo=''
bin=''
while [ $# -gt 0 ]; do
  case $1 in
    --rev | --out | --repo | --bin)
      flag=$1
      [ $# -ge 2 ] || usage_error "$flag needs a value"
      value=$2
      case $value in
        '' | -*) usage_error "$flag needs a value, not '$value'" ;;
      esac
      case $flag in
        --rev) rev=$value ;;
        --out) out=$value ;;
        --repo) repo=$value ;;
        --bin) bin=$value ;;
      esac
      shift 2
      ;;
    -*) usage_error "unknown flag: $1" ;;
    *) usage_error "unexpected argument: $1" ;;
  esac
done
[ -n "$rev" ] || usage_error '--rev is required'
[ -n "$out" ] || usage_error '--out is required'

if [ -z "$repo" ]; then
  repo=$(cd "$(dirname "$0")/.." && pwd -P)
fi
# The checks run from another directory, so every path is made absolute now.
case $out in /*) ;; *) out=$PWD/$out ;; esac
while [ "$out" != / ] && [ "${out%/}" != "$out" ]; do out=${out%/}; done
case $repo in /*) ;; *) repo=$PWD/$repo ;; esac
if [ -n "$bin" ]; then
  case $bin in /*) ;; *) bin=$PWD/$bin ;; esac
  { [ -f "$bin" ] && [ -x "$bin" ]; } || die 2 "--bin is not an executable file: $bin"
fi

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES
# The clean copy is the commit's bytes and nothing else: no user or system git configuration (an
# `autocrlf`, a filter or a hook would rewrite files before the proofs run) and no template
# directory. And the proofs must look at the clean copy's corpus, never at a REQ_ROOT the caller
# happens to have exported: `req` resolves its corpus from REQ_ROOT before the working directory.
unset GIT_TEMPLATE_DIR GIT_CONFIG GIT_CONFIG_COUNT REQ_ROOT
GIT_CONFIG_GLOBAL=/dev/null
GIT_CONFIG_SYSTEM=/dev/null
GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL GIT_CONFIG_SYSTEM GIT_CONFIG_NOSYSTEM

git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 || die 2 "--repo is not a git repository: $repo"
full=$(git -C "$repo" rev-parse --verify --quiet "$rev^{commit}") ||
  die 2 "cannot resolve --rev '$rev' to a commit in $repo"

for tool in git tar gzip mktemp awk sed grep date uname rustc cargo; do
  command -v "$tool" >/dev/null 2>&1 || die 2 "needed tool not on PATH: $tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() { sha256sum "$1" | awk '{ print $1 }'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() { shasum -a 256 "$1" | awk '{ print $1 }'; }
else
  die 2 'needed tool not on PATH: sha256sum or shasum'
fi

# The Baseline step has no SKIP: with no verifier, `req baseline --verify` is UNVERIFIABLE (exit 3),
# and "we could not tell" is never a pass. Said here, before a compile is spent on it.
command -v minisign >/dev/null 2>&1 ||
  die 3 'UNVERIFIABLE minisign: not on PATH (brew install minisign / apt-get install minisign); nothing written'

# --- a clean copy of exactly <rev> ----------------------------------------------------------
work=$(mktemp -d "${TMPDIR:-/tmp}/req-release.XXXXXX") || die 1 'cannot make a temporary directory'
src=$work/src
say "clean copy of $full"
git clone --quiet --no-checkout --no-hardlinks --template= "$repo" "$src" >"$work/git.log" 2>&1 ||
  { show 'git clone' "$work/git.log"; die 1 "git clone of $repo failed"; }
# The clone holds the branches and tags; this adds every other ref, so a rev that is only on a
# remote-tracking branch resolves too.
git -C "$src" fetch --quiet "$repo" '+refs/*:refs/req-release/*' >"$work/git.log" 2>&1 ||
  { show 'git fetch' "$work/git.log"; die 1 "git fetch of $repo failed"; }
git -C "$src" checkout --quiet --detach "$full" >"$work/git.log" 2>&1 ||
  { show 'git checkout' "$work/git.log"; die 1 "cannot check out $full in the clean copy"; }
[ "$(git -C "$src" rev-parse HEAD)" = "$full" ] || die 1 "the clean copy is not at $full"

# --- the target ------------------------------------------------------------------------------
rustc_vv=$(cd "$src" && rustc -vV) || die 2 'rustc -vV failed'
host=$(printf '%s\n' "$rustc_vv" | sed -n 's/^host: //p')
case $host in
  x86_64-unknown-linux-gnu | aarch64-apple-darwin) ;;
  *) die 2 "unsupported host '$host': releases are built for x86_64-unknown-linux-gnu and aarch64-apple-darwin only" ;;
esac
# The one clock an artifact may carry: the commit's own (SOURCE_DATE_EPOCH's convention). The archive's
# member time and any stamp the build makes come from it, never from the builder's clock.
epoch=$(git -C "$src" log -1 --format=%ct "$full") || die 1 'cannot read the commit time'
case $epoch in
  '' | *[!0-9]*) die 1 "the commit time '$epoch' is not a number of seconds" ;;
esac
rustc_v=$(cd "$src" && rustc -V) || die 2 'rustc -V failed'
cargo_v=$(cd "$src" && cargo -V) || die 2 'cargo -V failed'

# --- the binary ------------------------------------------------------------------------------
if [ -z "$bin" ]; then
  say "cargo build --release --locked ($host)"
  unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS CARGO_BUILD_TARGET
  # The builder's paths must not reach the binary (ADR 0035): panic messages and debug paths carry
  # the source path of every file, and a dependency's lives under the builder's CARGO_HOME. Both are
  # mapped to fixed names. The encoded form (flags joined by 0x1f) keeps a path with a space whole.
  # These are the only flags: nothing here changes what the code does, only what it says about where
  # it was built.
  sep=$(printf '\037')
  cargo_home=${CARGO_HOME:-${HOME:-/nonexistent}/.cargo}
  CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$cargo_home=/cargo${sep}--remap-path-prefix=$src=/req"
  export CARGO_ENCODED_RUSTFLAGS
  # The convention for "the time of this source": the commit's own time, for any tool that stamps one.
  SOURCE_DATE_EPOCH=$epoch
  export SOURCE_DATE_EPOCH
  (cd "$src" && CARGO_TARGET_DIR="$src/target" cargo build --release --locked) >&2 ||
    die 1 'cargo build --release --locked failed'
  unset CARGO_ENCODED_RUSTFLAGS SOURCE_DATE_EPOCH
  bin=$src/target/release/req
  { [ -f "$bin" ] && [ -x "$bin" ]; } || die 1 "the build produced no executable at $bin"
fi
# Prove, and package, one private copy: the bytes that pass the checks are the bytes that ship.
stage=$work/stage
mkdir "$stage"
cp "$bin" "$stage/req"
chmod 755 "$stage/req"

# --- the proofs, from the clean copy ---------------------------------------------------------
say 'check built-from'
rc=0
(cd "$src" && "$stage/req" --version) >"$work/version.out" 2>"$work/version.err" || rc=$?
if [ "$rc" -ne 0 ]; then
  show 'req --version' "$work/version.err"
  die 1 "FAIL built-from: req --version exited $rc"
fi
built=$(awk '$1 == "built" && $2 == "from" { print $3; exit }' "$work/version.out")
case $built in
  '' | *[!0-9a-f]*)
    show 'req --version' "$work/version.out"
    die 1 "FAIL built-from: '$built' is not a bare SHA (a -dirty, unknown or -cleanliness-unknown binary is never released)"
    ;;
esac
if [ "${#built}" -lt 7 ] || [ "${#built}" -gt 40 ]; then
  die 1 "FAIL built-from: '$built' is not a 7-40 character SHA"
fi
case $full in
  "$built"*) ;;
  *) die 1 "FAIL built-from: the binary says '$built', which is not a prefix of $full" ;;
esac

say 'check baseline-verify'
rc=0
(cd "$src" && "$stage/req" baseline --verify baselines/air-gapped.json) \
  >"$work/verify.out" 2>"$work/verify.err" || rc=$?
if [ "$rc" -ne 0 ]; then
  show 'stdout' "$work/verify.out"
  show 'stderr' "$work/verify.err"
  die 1 "FAIL baseline-verify: req baseline --verify exited $rc"
fi
if ! grep -qx '\[VERIFIED\]' "$work/verify.out"; then
  show 'stdout' "$work/verify.out"
  die 1 'FAIL baseline-verify: the Baseline did not print [VERIFIED]'
fi

say 'check bench-gate'
rc=0
(cd "$src" && "$stage/req" bench) >"$work/bench.out" 2>"$work/bench.err" || rc=$?
if [ "$rc" -ne 0 ]; then
  show 'stderr' "$work/bench.err"
  die 1 "FAIL bench-gate: req bench exited $rc"
fi
# The top-level `fmr` block's own `hard_gate`; the `direction` block has one too and is not judged.
fmr_gate=$(awk '
  /^  "fmr": \{/ { in_fmr = 1; next }
  in_fmr && /^  \}/ { in_fmr = 0 }
  in_fmr && /^    "hard_gate": "/ { v = $0; sub(/^    "hard_gate": "/, "", v); sub(/".*/, "", v); print v; exit }
' "$work/bench.out")
if [ "$fmr_gate" != PASS ]; then
  say "  fmr.hard_gate: '${fmr_gate:-not found}'"
  die 1 'FAIL bench-gate: fmr.hard_gate is not PASS'
fi

# --- the assets ------------------------------------------------------------------------------
asset="req-$built-$host"
tarball="$asset.tar.gz"
sumfile="$asset.tar.gz.sha256"
infofile="$asset.build-info"
for name in "$tarball" "$sumfile" "$infofile"; do
  if [ -e "$out/$name" ] || [ -L "$out/$name" ]; then
    die 1 "refusing to overwrite $out/$name (a checksum that was pinned must stay true)"
  fi
done

# macOS tar would add AppleDouble `._req` members for extended attributes; the archive is `req` only.
COPYFILE_DISABLE=1
export COPYFILE_DISABLE
# Nothing of the builder in the member: owner 0:0 with no name, the commit's time, ustar (no per-run
# extended header), so one binary at one rev is one archive whoever packs it and whenever (ADR 0035).
if tar --version 2>/dev/null | grep -q 'GNU tar'; then
  tar --format=ustar --owner=0 --group=0 --numeric-owner --mtime="@$epoch" \
    -cf "$work/req.tar" -C "$stage" req || die 1 'tar failed'
else
  # bsdtar has no --mtime: the staged file carries the time instead.
  stamp=$(date -u -r "$epoch" +%Y%m%d%H%M.%S 2>/dev/null || date -u -d "@$epoch" +%Y%m%d%H%M.%S) ||
    die 1 "cannot format the commit time $epoch"
  TZ=UTC touch -t "$stamp" "$stage/req" || die 1 'cannot set the staged binary time'
  tar --format ustar --uid 0 --gid 0 --uname '' --gname '' \
    -cf "$work/req.tar" -C "$stage" req || die 1 'tar failed'
fi
gzip -n -c "$work/req.tar" >"$work/$tarball" || die 1 'gzip failed'
members=$(tar -tzf "$work/$tarball") || die 1 'cannot list the archive just written'
[ "$members" = req ] || die 1 "the archive must hold exactly one member 'req', it holds: $members"

digest=$(sha256_of "$work/$tarball")
case $digest in
  *[!0-9a-f]* | '') die 1 "sha256 tool gave '$digest', not 64 hex digits" ;;
esac
[ "${#digest}" -eq 64 ] || die 1 "sha256 tool gave '$digest', not 64 hex digits"
printf '%s  %s\n' "$digest" "$tarball" >"$work/$sumfile"

if [ "$(uname -s)" = Linux ]; then
  libc_line=$(ldd --version 2>&1 | head -n 1) || libc_line=''
  [ -n "$libc_line" ] || libc_line='unknown (ldd gave no version)'
elif [ "$(uname -s)" = Darwin ] && command -v sw_vers >/dev/null 2>&1; then
  libc_line="macOS $(sw_vers -productVersion) (libSystem)"
else
  libc_line=$(uname -sr)
fi
{
  printf 'rev=%s\n' "$built"
  printf 'sha=%s\n' "$full"
  printf 'target=%s\n' "$host"
  printf 'rustc=%s\n' "$rustc_v"
  printf 'cargo=%s\n' "$cargo_v"
  printf 'libc=%s\n' "$libc_line"
  printf 'built_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >"$work/$infofile"

# Nothing above touched <out>. From here each file is written under a temporary name and moved.
if [ ! -d "$out" ]; then
  mkdir -p "$out" || die 1 "cannot create $out"
  made_out=1
fi
publish() {
  cp "$work/$1" "$out/.req-$1$tmp_suffix" || die 1 "cannot write into $out"
  # A hard link fails if the name exists, where `mv` would replace it: two runs racing for the same
  # --out cannot both succeed, and neither can replace an asset whose checksum was already printed.
  ln "$out/.req-$1$tmp_suffix" "$out/$1" || die 1 "cannot link $1 into place (it may have just been written by another run)"
  rm -f "$out/.req-$1$tmp_suffix"
}
publish "$tarball"
made_tar=$out/$tarball
publish "$sumfile"
made_sha=$out/$sumfile
publish "$infofile"
made_info=$out/$infofile

# What was written is what was hashed.
[ "$(sha256_of "$out/$tarball")" = "$digest" ] || die 1 "$out/$tarball does not hash to $digest after the move"

printf '%s\n' "$out/$tarball" "$out/$sumfile" "$out/$infofile"
printf 'pin: REV=%s SHA256=%s TARGET=%s\n' "$built" "$digest" "$host"
