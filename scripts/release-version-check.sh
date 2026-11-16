#!/usr/bin/env bash
# scripts/release-version-check.sh -- the version rules of ADR 0055, in one place (AGENTS.md: no check logic in
# workflow YAML). The release workflow's build job calls `check` before anything is built or uploaded; the rest are
# the pieces `check` and scripts/release-notes.sh share, and the Cargo.lock exception a version-bump commit is held to.
#
# Usage:
#   release-version-check.sh check (--version <X.Y.Z> | --rev <rev>) --facts <file>
#       Judge whether a release may carry that version. `--rev` reads it from `git show <rev>:Cargo.toml` (the rev is
#       never checked out); `--version` takes it as given (the tests). `<file>` is what `release-notes.sh --facts`
#       wrote: `previous_tag=`, `previous_version=` and `identity=` (moved | unmoved | unknown), one `key=value` per
#       line, each exactly once. On acceptance it prints `version=<X.Y.Z>` and `bump=<class>` on stdout, so a workflow
#       step can append them to its outputs; on refusal stdout stays empty and the reason is on stderr.
#   release-version-check.sh read <rev>
#       Print the `version` of the `[package]` table of `git show <rev>:Cargo.toml`, plain MAJOR.MINOR.PATCH only.
#   release-version-check.sh classify <previous> <new>
#       Print one of: major, minor, patch, none (equal), lower, `not comparable` (either side is not plain semver).
#   release-version-check.sh lock-exception <old Cargo.lock> <new Cargo.lock>
#       ADR 0055 decision 6: pass only if the two lock files agree on everything except req's own package entry (the
#       `[[package]]` named req that has no `source`). The exception stays one entry wide. Reads files, never git
#       history; the bump pull request's runner is `git show origin/main:Cargo.lock > old` and this.
#
# The rules, from the ADR (decisions 2 and 4). The previous release's version is a `rev-*` release's 0.1.0 or a `v*`
# release's own Cargo.toml. Below 1.0 (the previous major is 0) an identity move is a MINOR bump; from 1.0 on it is a
# MAJOR bump. `check` refuses, naming the reason, when:
#   - the version is not greater than the previous release's;
#   - the bump is a major bump below 1.0 (the ADR that decides 1.0 is the owner's, so the workflow never takes it);
#   - identity moved and the required number did not rise;
#   - identity is unknown and only the patch number rose (an unknown is never an unmoved identity).
# A minor bump with identity unmoved is the owner's declaration (a changed command, flag or exit code cannot be read
# from history), so it passes. With no previous release there is nothing to be greater than: a first release passes.
#
# Exit: 0 accepted (or the answer printed); 1 refused by a rule, the reason on stderr; 2 usage, or an input it cannot
# judge (a version that is not plain MAJOR.MINOR.PATCH, a facts file that is malformed or contradicts itself, a lock
# file without exactly one entry of req's own). Every non-zero exit prints nothing on stdout.
set -euo pipefail
export LC_ALL=C

die() { # die <status> <message>
  printf 'release-version-check: %s\n' "$2" >&2
  exit "$1"
}

usage() {
  die 2 "usage: check (--version <X.Y.Z> | --rev <rev>) --facts <file> | read <rev> | classify <previous> <new> | lock-exception <old> <new>"
}

# --- plain semantic versions: MAJOR.MINOR.PATCH, no pre-release, no build metadata, no leading zeros -------------
v_re='^(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$'
V_MAJOR=0 V_MINOR=0 V_PATCH=0
parse_version() { # parse_version <text> -> 0 and V_MAJOR/V_MINOR/V_PATCH set, or 1
  case $1 in '' | *[!0-9.]*) return 1 ;; esac
  [[ $1 =~ $v_re ]] || return 1
  V_MAJOR=${BASH_REMATCH[1]} V_MINOR=${BASH_REMATCH[2]} V_PATCH=${BASH_REMATCH[3]}
}

# --- the version of a rev -----------------------------------------------------------------------------------------
read_version() { # read_version <rev> -> prints the version, or exits 2
  local rev=$1 text found
  case $rev in
    '' | *[!0-9A-Za-z._/-]* | -*) die 2 "'$rev' is not a SHA or a ref name" ;;
  esac
  text=$(git show "$rev:Cargo.toml" 2>/dev/null) || die 2 "cannot read Cargo.toml at '$rev'"
  # The first `version = "…"` of the [package] table, and nothing else: a dependency table has versions of its own.
  found=$(printf '%s\n' "$text" | awk '
    /^[[:space:]]*\[/ { in_pkg = ($0 ~ /^[[:space:]]*\[package\][[:space:]]*(#.*)?$/); next }
    in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
      line = $0
      sub(/^[[:space:]]*version[[:space:]]*=[[:space:]]*/, "", line)
      if (match(line, /^"[^"]*"/)) { print substr(line, 2, RLENGTH - 2) }
      exit
    }')
  [ -n "$found" ] || die 2 "Cargo.toml at '$rev' has no string version in [package]"
  parse_version "$found" || die 2 "the version '$found' at '$rev' is not plain MAJOR.MINOR.PATCH"
  printf '%s\n' "$found"
}

# --- the class of a bump ------------------------------------------------------------------------------------------
classify() { # classify <previous> <new> -> prints the class
  local pm pn pp
  if ! parse_version "$1"; then printf 'not comparable\n'; return 0; fi
  pm=$V_MAJOR pn=$V_MINOR pp=$V_PATCH
  if ! parse_version "$2"; then printf 'not comparable\n'; return 0; fi
  if [ "$V_MAJOR" -eq "$pm" ] && [ "$V_MINOR" -eq "$pn" ] && [ "$V_PATCH" -eq "$pp" ]; then
    printf 'none\n'
  elif [ "$V_MAJOR" -lt "$pm" ] ||
    { [ "$V_MAJOR" -eq "$pm" ] && [ "$V_MINOR" -lt "$pn" ]; } ||
    { [ "$V_MAJOR" -eq "$pm" ] && [ "$V_MINOR" -eq "$pn" ] && [ "$V_PATCH" -lt "$pp" ]; }; then
    printf 'lower\n'
  elif [ "$V_MAJOR" -gt "$pm" ]; then
    printf 'major\n'
  elif [ "$V_MINOR" -gt "$pn" ]; then
    printf 'minor\n'
  else
    printf 'patch\n'
  fi
}

# --- check --------------------------------------------------------------------------------------------------------
cmd_check() {
  local version= rev= facts= have_version=0
  while [ $# -gt 0 ]; do
    case $1 in
      --version) [ $# -ge 2 ] || usage; version=$2; have_version=1; shift 2 ;;
      --rev) [ $# -ge 2 ] || usage; rev=$2; shift 2 ;;
      --facts) [ $# -ge 2 ] || usage; facts=$2; shift 2 ;;
      *) usage ;;
    esac
  done
  [ -n "$facts" ] || usage
  if [ "$have_version" -eq 1 ] && [ -n "$rev" ]; then usage; fi
  if [ "$have_version" -eq 0 ] && [ -z "$rev" ]; then usage; fi
  if [ -n "$rev" ]; then version=$(read_version "$rev"); fi
  parse_version "$version" || die 2 "the version '$version' is not plain MAJOR.MINOR.PATCH"

  [ -f "$facts" ] && [ -r "$facts" ] || die 2 "cannot read the facts file '$facts'"
  local prev_tag= prev_version= identity= seen_tag=0 seen_version=0 seen_identity=0 line key value
  while IFS= read -r line || [ -n "$line" ]; do
    case $line in
      *=*) key=${line%%=*} value=${line#*=} ;;
      *) die 2 "the facts file has a line that is not key=value" ;;
    esac
    case $key in
      previous_tag) [ "$seen_tag" -eq 0 ] || die 2 "the facts file repeats previous_tag"; seen_tag=1; prev_tag=$value ;;
      previous_version) [ "$seen_version" -eq 0 ] || die 2 "the facts file repeats previous_version"; seen_version=1; prev_version=$value ;;
      identity) [ "$seen_identity" -eq 0 ] || die 2 "the facts file repeats identity"; seen_identity=1; identity=$value ;;
      *) die 2 "the facts file has an unknown key '$key'" ;;
    esac
  done <"$facts"
  [ "$seen_tag" -eq 1 ] && [ "$seen_version" -eq 1 ] && [ "$seen_identity" -eq 1 ] ||
    die 2 "the facts file must hold previous_tag, previous_version and identity"
  case $identity in
    moved | unmoved | unknown) ;;
    *) die 2 "identity is '$identity', not one of moved, unmoved, unknown" ;;
  esac

  # The previous release, and its version: a rev-* release counts as 0.1.0 and the facts may not say otherwise.
  local rev_tag_re='^rev-[0-9a-f]{7,40}$'
  if [ -z "$prev_tag" ]; then
    [ -z "$prev_version" ] || die 2 "the facts name a previous version but no previous release"
    printf 'version=%s\nbump=first release\n' "$version"
    return 0
  elif [[ $prev_tag =~ $rev_tag_re ]]; then
    [ "$prev_version" = 0.1.0 ] || die 2 "the previous release $prev_tag is a rev release, which counts as 0.1.0, but the facts say '$prev_version'"
  elif [[ $prev_tag =~ ^v[0-9] ]] && parse_version "${prev_tag#v}"; then
    if [ -z "$prev_version" ]; then
      printf 'release-version-check: refused: the previous release %s has no readable version (its Cargo.toml could not be read), so the previous version is unknown\n' "$prev_tag" >&2
      exit 1
    fi
    parse_version "$prev_version" || die 2 "the previous version '$prev_version' is not plain MAJOR.MINOR.PATCH"
    [ "$prev_version" = "${prev_tag#v}" ] || die 2 "the tag $prev_tag says ${prev_tag#v}, but its Cargo.toml says $prev_version"
  else
    die 2 "the previous release '$prev_tag' is neither a rev-<sha> nor a v<X.Y.Z> tag"
  fi
  parse_version "$prev_version" || die 2 "the previous version '$prev_version' is not plain MAJOR.MINOR.PATCH"
  local prev_major=$V_MAJOR
  local class
  class=$(classify "$prev_version" "$version")

  # 1. greater than the previous release's
  case $class in
    none | lower)
      printf 'release-version-check: refused: version %s is not greater than the previous release %s (%s)\n' "$version" "$prev_version" "$prev_tag" >&2
      exit 1
      ;;
  esac
  # 4. a major bump below 1.0 is the owner's decision, made in an ADR, never the workflow's
  if [ "$class" = major ] && [ "$prev_major" -eq 0 ]; then
    printf 'release-version-check: refused: %s -> %s is a major bump below 1.0, which is refused until the ADR that decides 1.0 exists\n' "$prev_version" "$version" >&2
    exit 1
  fi
  # 2. identity moved, and the number that rose is not the one the ADR requires
  if [ "$identity" = moved ]; then
    if [ "$prev_major" -eq 0 ] && [ "$class" != minor ]; then
      printf 'release-version-check: refused: identity moved since %s, so below 1.0 the minor number must rise; %s -> %s is a %s bump\n' "$prev_tag" "$prev_version" "$version" "$class" >&2
      exit 1
    fi
    if [ "$prev_major" -ge 1 ] && [ "$class" != major ]; then
      printf 'release-version-check: refused: identity moved since %s, so from 1.0 the major number must rise; %s -> %s is a %s bump\n' "$prev_tag" "$prev_version" "$version" "$class" >&2
      exit 1
    fi
  fi
  # 3. identity unknown, and only the patch number rose
  if [ "$identity" = unknown ] && [ "$class" = patch ]; then
    printf 'release-version-check: refused: identity could not be compared with %s and only the patch number rose (%s -> %s); an unknown is not an unmoved identity\n' "$prev_tag" "$prev_version" "$version" >&2
    exit 1
  fi
  printf 'version=%s\nbump=%s\n' "$version" "$class"
}

# --- lock-exception -----------------------------------------------------------------------------------------------
# Splits a Cargo.lock into the one entry of req's own (a `[[package]]` named req with no `source`) and the rest. The
# rest is written to <out>; the number of own entries is printed.
split_lock() { # split_lock <lock> <out>
  awk -v out="$2" '
    function flush() {
      if (!inblock) return
      if (name_req && !has_source) { own++ } else { printf "%s", buf > out }
      buf = ""; inblock = 0; name_req = 0; has_source = 0
    }
    /^\[\[package\]\][[:space:]]*$/ { flush(); inblock = 1; buf = $0 "\n"; next }
    /^\[/ { flush(); print $0 > out; next }
    {
      if (inblock) {
        buf = buf $0 "\n"
        if ($0 ~ /^name = "req"[[:space:]]*$/) name_req = 1
        if ($0 ~ /^source = /) has_source = 1
      } else {
        print $0 > out
      }
    }
    END { flush(); close(out); print own + 0 }' "$1"
}

cmd_lock_exception() {
  [ $# -eq 2 ] || usage
  local old=$1 new=$2 f n_old n_new
  for f in "$old" "$new"; do
    [ -f "$f" ] && [ -r "$f" ] || die 2 "cannot read the lock file '$f'"
  done
  # Global, not local: the EXIT trap runs after this function has returned.
  work=$(mktemp -d) || die 2 "cannot make a scratch directory"
  trap 'rm -rf "$work"' EXIT
  : >"$work/old" && : >"$work/new"
  n_old=$(split_lock "$old" "$work/old")
  n_new=$(split_lock "$new" "$work/new")
  [ "$n_old" -eq 1 ] || die 2 "$old holds $n_old entries of req's own (a package named req with no source); expected exactly one"
  [ "$n_new" -eq 1 ] || die 2 "$new holds $n_new entries of req's own (a package named req with no source); expected exactly one"
  if ! cmp -s "$work/old" "$work/new"; then
    printf 'release-version-check: refused: the lock files differ in an entry other than req'"'"'s own (ADR 0055 decision 6: the exception is one entry wide):\n' >&2
    diff "$work/old" "$work/new" | head -n 12 >&2 || true
    exit 1
  fi
}

[ $# -ge 1 ] || usage
sub=$1
shift
case $sub in
  check) cmd_check "$@" ;;
  read) [ $# -eq 1 ] || usage; read_version "$1" ;;
  classify) [ $# -eq 2 ] || usage; classify "$1" "$2" ;;
  lock-exception) cmd_lock_exception "$@" ;;
  *) usage ;;
esac
