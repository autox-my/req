#!/usr/bin/env bash
# scripts/reproduce.sh — is a release build of one rev reproducible? (ADR 0035)
#
# Usage: scripts/reproduce.sh --rev <rev> [--repo <dir>] [--against <tarball>] [--bin <path>]
#   --rev <rev>        the commit to build (anything `git rev-parse` resolves in --repo); required.
#   --repo <dir>       the repository holding <rev> (default: this script's repository).
#   --against <file>   compare ONE fresh build with this release tarball (`req-<REV>-<TARGET>.tar.gz`),
#                      instead of comparing two fresh builds with each other.
#   --bin <path>       passed to scripts/build-release.sh (the packaging logic is testable without a compile).
#
# Without --against it runs scripts/build-release.sh twice, on purpose in different conditions: another
# TMPDIR (so the clean copy lives at another path, of another length), another output directory and a
# clock two seconds later. The two tarballs must be byte-identical. This proves the build and the archive
# depend on the rev and the pinned toolchain, not on where or when they ran. It does NOT prove another
# machine gets the same bytes: that is --against.
#
# With --against it builds the rev once and compares the binary inside (`req`, by SHA-256) with the one
# in the given tarball, for an adopter or an assessor who wants to derive the released binary themselves.
# The archive hash is reported but does not decide: the archive's bytes also depend on the `gzip` of
# whoever packed it, the binary's do not. The binary's bytes DO depend on the target, the toolchain the
# rev pins (`rust-toolchain.toml`) and, on Linux, the linker and C library the builder has; on macOS the
# toolchain's install path too (the linker's UUID, 16 bytes, and the signature that covers it, 32). A mismatch is
# reported with the toolchain line and how many bytes differ, never forced to a pass, and never read as a defect of
# the release without that comparison. It is evidence either way.
#
# Output: one verdict line (`REPRODUCED` or `DIFFERS`) and the hashes it compared.
# Exit codes (three-state, AGENTS.md):
#   0  REPRODUCED
#   1  DIFFERS, or a build or a check of scripts/build-release.sh failed
#   2  usage, an unresolvable rev, an unsupported host, or a missing tool
#   3  UNVERIFIABLE: scripts/build-release.sh could not run its Baseline proof (no `minisign`)
# Nothing is written outside a private temp directory, which is removed on every exit.
set -eu

PROG='reproduce'
USAGE='usage: scripts/reproduce.sh --rev <rev> [--repo <dir>] [--against <tarball>] [--bin <path>]'

work=''
cleanup() {
  rc=$?
  trap - EXIT
  [ -z "$work" ] || [ ! -d "$work" ] || rm -rf "$work"
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

rev=''
repo=''
against=''
bin=''
while [ $# -gt 0 ]; do
  case $1 in
    --rev | --repo | --against | --bin)
      flag=$1
      [ $# -ge 2 ] || usage_error "$flag needs a value"
      value=$2
      case $value in
        '' | -*) usage_error "$flag needs a value, not '$value'" ;;
      esac
      case $flag in
        --rev) rev=$value ;;
        --repo) repo=$value ;;
        --against) against=$value ;;
        --bin) bin=$value ;;
      esac
      shift 2
      ;;
    -*) usage_error "unknown flag: $1" ;;
    *) usage_error "unexpected argument: $1" ;;
  esac
done
[ -n "$rev" ] || usage_error '--rev is required'

here=$(cd "$(dirname "$0")" && pwd -P)
[ -n "$repo" ] || repo=$(cd "$here/.." && pwd -P)
case $repo in /*) ;; *) repo=$PWD/$repo ;; esac
if [ -n "$against" ]; then
  case $against in /*) ;; *) against=$PWD/$against ;; esac
  [ -f "$against" ] || die 2 "--against is not a file: $against"
fi
if [ -n "$bin" ]; then
  case $bin in /*) ;; *) bin=$PWD/$bin ;; esac
fi

for tool in tar gzip mktemp awk sed grep sleep; do
  command -v "$tool" >/dev/null 2>&1 || die 2 "needed tool not on PATH: $tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() { sha256sum "$1" | awk '{ print $1 }'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() { shasum -a 256 "$1" | awk '{ print $1 }'; }
else
  die 2 'needed tool not on PATH: sha256sum or shasum'
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/req-reproduce.XXXXXX") || die 1 'cannot make a temporary directory'

# One build, in conditions of its own. $1 names the condition (a, b); the TMPDIR differs in path and
# length between the two so the clean copy of the rev lives somewhere else each time.
build() {
  tag=$1
  case $tag in
    a) tmp=$work/a ;;
    *) tmp=$work/b-a-deliberately-longer-temporary-directory-name/t ;;
  esac
  mkdir -p "$tmp"
  set -- --rev "$rev" --repo "$repo" --out "$work/out-$tag"
  [ -z "$bin" ] || set -- "$@" --bin "$bin"
  rc=0
  TMPDIR=$tmp bash "$here/build-release.sh" "$@" >"$work/build-$tag.out" 2>"$work/build-$tag.err" || rc=$?
  if [ "$rc" -ne 0 ]; then
    sed 's/^/  /' "$work/build-$tag.err" >&2
    case $rc in
      3) die 3 "UNVERIFIABLE: build $tag could not run its proofs (see above); nothing was compared" ;;
      2) die 2 "build $tag: usage or environment error (see above)" ;;
      *) die 1 "FAIL: build $tag failed (see above); nothing was compared" ;;
    esac
  fi
}
asset_of() { # $1 = a|b, $2 = suffix
  ls "$work/out-$1"/req-*"$2" 2>/dev/null | head -n 1
}
member_sha() { # $1 = tarball, $2 = scratch dir name
  mkdir -p "$work/$2"
  tar -xzf "$1" -C "$work/$2" req || die 1 "cannot unpack the member 'req' of $1"
  sha256_of "$work/$2/req"
}
info_line() { # the toolchain lines of a build-info, without the clock
  grep -E '^(rustc|cargo|libc|target)=' "$1" 2>/dev/null | tr '\n' ' '
}

build a
tar_a=$(asset_of a .tar.gz)
[ -n "$tar_a" ] || die 1 'the build wrote no tarball'
bin_a=$(member_sha "$tar_a" ua)
arc_a=$(sha256_of "$tar_a")

if [ -n "$against" ]; then
  bin_b=$(member_sha "$against" ub)
  arc_b=$(sha256_of "$against")
  if [ "$bin_a" = "$bin_b" ]; then
    note='identical'
    [ "$arc_a" = "$arc_b" ] || note='differs (the packer'"'"'s gzip may differ; the binary is what decides)'
    printf 'REPRODUCED  the binary in %s is the one built from %s\n' "$against" "$rev"
    printf '  binary   %s\n  archive  built %s, given %s: %s\n' "$bin_a" "$arc_a" "$arc_b" "$note"
    exit 0
  fi
  printf 'DIFFERS  the binary in %s is not the one this machine builds from %s\n' "$against" "$rev"
  printf '  binary   built %s\n  binary   given %s\n' "$bin_a" "$bin_b"
  # How far apart: a handful of bytes is a stamp (on macOS the linker writes a UUID derived in part from the
  # toolchain's install path, and the code signature that covers it); thousands is a different build.
  size_a=$(wc -c <"$work/ua/req" | tr -d ' ')
  size_b=$(wc -c <"$work/ub/req" | tr -d ' ')
  apart=$(cmp -l "$work/ua/req" "$work/ub/req" 2>/dev/null | wc -l | tr -d ' ')
  printf '  size     built %s bytes, given %s bytes; %s bytes differ in the common prefix\n' "$size_a" "$size_b" "$apart"
  printf '  built here:  %s\n' "$(info_line "$(asset_of a .build-info)")"
  printf '  The release'"'"'s own build-info names the toolchain and libc it was built with; compare them first.\n'
  exit 1
fi

sleep 2
build b
tar_b=$(asset_of b .tar.gz)
[ -n "$tar_b" ] || die 1 'the second build wrote no tarball'
bin_b=$(member_sha "$tar_b" ub)
arc_b=$(sha256_of "$tar_b")

if [ "$arc_a" = "$arc_b" ] && [ "$bin_a" = "$bin_b" ]; then
  printf 'REPRODUCED  two builds of %s, in different directories and two seconds apart, are byte-identical\n' "$rev"
  printf '  binary   %s\n  archive  %s\n' "$bin_a" "$arc_a"
  exit 0
fi
printf 'DIFFERS  two builds of %s are not byte-identical\n' "$rev"
printf '  binary   %s\n  binary   %s\n' "$bin_a" "$bin_b"
printf '  archive  %s\n  archive  %s\n' "$arc_a" "$arc_b"
if [ "$bin_a" = "$bin_b" ]; then
  printf '  the binaries agree: the archive (member header or compression) is what varies\n'
fi
exit 1
