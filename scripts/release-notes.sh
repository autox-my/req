#!/usr/bin/env bash
# scripts/release-notes.sh -- the release notes of one rev of req (ADR 0031, amending ADR 0025 section 5; ADR 0055).
#
# Usage: scripts/release-notes.sh --rev <rev> --repo <owner/name> --out <file> [--facts <file>]
#
# Reads git history only; it builds nothing and calls no network service. It writes, in this order:
#   0. one opening line: the version the rev declares (`git show <rev>:Cargo.toml`), the rev and the bump class
#      (major, minor, patch, none, lower, `first release`, or `not comparable`), as
#      `Version 0.2.0 · rev `abc1234` · bump: minor`;
#   1. which release came before (the nearest release tag strictly before the rev: a `rev-*` or any `v*` tag, so a
#      stray `v` tag is found and the version check refuses it; the search starts at `<rev>^`, so a release's own tag is never its predecessor) and a compare link;
#   2. whether req's *identity* moved since then -- RULES_V (src/canon.rs), the claim schema
#      (CLAIM_SCHEMA_V, src/canon.rs) and the Baseline envelope (BASELINE_VERSION, src/baseline.rs) --
#      and what an adopter must do if it did. A value that cannot be read is "could not be determined",
#      never "unchanged";
#   3. the pull requests merged since, newest first, grouped by their conventional-commit prefix.
# Every title is treated as hostile text (see `clean`): the notes become the body of a public release.
#
# --facts <file> writes, beside the notes, what the release workflow decides on (ADR 0055 decision 4), the one home
# of these facts: three `key=value` lines, in this order.
#   previous_tag=      the previous release's tag, or empty for a first release
#   previous_version=  its version: `0.1.0` for a `rev-*` release; for a `v*` release, the `[package]` version of
#                      `<tag>:Cargo.toml`, empty when that cannot be read (the check then refuses, fail closed)
#   identity=          `moved` when RULES_V, the claim schema or the Baseline envelope changed since the previous
#                      release; `unmoved` when all three could be compared and are equal; `unknown` otherwise (a value
#                      that cannot be read, or no previous release). A known move outranks an unreadable value.
# scripts/release-version-check.sh reads that file; the workflow never computes any of it again.
#
# Exit: 0 written; 1 --out or --facts already exists (never overwritten); 2 usage, an unresolvable rev, or a
# repository name outside owner/name. In every non-zero case --out and --facts are left as they were.
set -euo pipefail

die() { # die <status> <message>
  printf 'release-notes: %s\n' "$2" >&2
  exit "$1"
}

rev_in= repo= out= facts=
while [ $# -gt 0 ]; do
  case $1 in
    --rev) [ $# -ge 2 ] || die 2 "--rev needs a value"; rev_in=$2; shift 2 ;;
    --repo) [ $# -ge 2 ] || die 2 "--repo needs a value"; repo=$2; shift 2 ;;
    --out) [ $# -ge 2 ] || die 2 "--out needs a value"; out=$2; shift 2 ;;
    --facts) [ $# -ge 2 ] || die 2 "--facts needs a value"; facts=$2; shift 2 ;;
    *) die 2 "unknown argument '$1' (usage: --rev <rev> --repo <owner/name> --out <file> [--facts <file>])" ;;
  esac
done
[ -n "$rev_in" ] && [ -n "$repo" ] && [ -n "$out" ] || die 2 "usage: --rev <rev> --repo <owner/name> --out <file> [--facts <file>]"
[ "$facts" != "$out" ] || die 2 "--facts and --out are the same file"

# The repository name goes into a link, so it is held to GitHub's alphabet and to exactly two parts.
case $repo in
  */*/* | */ | /* | *[!A-Za-z0-9._/-]*) die 2 "--repo '$repo' is not owner/name" ;;
  */*) ;;
  *) die 2 "--repo '$repo' is not owner/name" ;;
esac
owner=${repo%%/*} name=${repo#*/}
case $owner in . | ..) die 2 "--repo '$repo' is not owner/name" ;; esac
case $name in . | ..) die 2 "--repo '$repo' is not owner/name" ;; esac

case $rev_in in
  '' | *[!0-9A-Za-z._/-]* | -*) die 2 "--rev is not a SHA or a ref name" ;;
esac
full=$(git rev-parse --verify --quiet "${rev_in}^{commit}") || die 2 "cannot resolve '$rev_in' to a commit"
short=${full:0:7}

[ ! -e "$out" ] || die 1 "$out already exists: refusing to overwrite it"
[ -z "$facts" ] || [ ! -e "$facts" ] || die 1 "$facts already exists: refusing to overwrite it"

# --- the previous release -----------------------------------------------------------------------------
# The nearest release tag strictly before the rev (the search starts at the rev's parent): `rev-*` (ADR 0025) or a
# version tag `v*` (ADR 0055 decision 3). A `v` tag that is not a plain `v<N>.<N>.<N>` is still found, and its version
# then cannot be read, so the version check refuses: a stray tag fails closed, never passes as "first release".
prev=$(git describe --tags --match 'rev-*' --match 'v*' --abbrev=0 "${full}^" 2>/dev/null) || prev=

# --- the versions: the one reader and the one classifier are scripts/release-version-check.sh ---------------------
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
version=$(bash "$here/release-version-check.sh" read "$full" 2>/dev/null) || version=
prev_version=
if [ -n "$prev" ]; then
  case $prev in
    rev-*) prev_version=0.1.0 ;; # every release before the first version tag printed 0.1.0 (ADR 0055 decision 8)
    *) prev_version=$(bash "$here/release-version-check.sh" read "$prev" 2>/dev/null) || prev_version= ;;
  esac
  bump=$(bash "$here/release-version-check.sh" classify "$prev_version" "$version")
else
  bump='first release'
fi
identity=unknown

# --- the three constants, at a commit; empty when they cannot be read --------------------------------
const_at() { # const_at <commit> <file> <NAME>
  # A missing file or constant is an empty answer, not a failure: the caller reports it as undetermined.
  { git show "$1:$2" 2>/dev/null | sed -n "s/^pub const $3: u32 = \([0-9][0-9]*\);.*/\1/p" | head -n 1; } || true
}

# `READS_AS_IS` is a list, `pub const READS_AS_IS: &[u32] = &[2, 3];`, so `const_at`'s `u32` pattern cannot read it
# (ADR 0056 *Consequences*). Prints the numbers one per line, or nothing: an absent constant and an empty list
# are told apart by `list_at` below, which prints `-` for a list it found.
list_at() { # list_at <commit> <file> <NAME> -> `-` then each number, or nothing when the constant is not there
  { git show "$1:$2" 2>/dev/null | sed -n "s/^pub const $3: &\[u32\] = &\[\([0-9, ]*\)\];.*/-,\1/p" | head -n 1 | tr ',' '\n' | tr -d ' ' | sed '/^$/d'; } || true
}

# --- a title as safe text ----------------------------------------------------------------------------
zw=$(printf '\342\200\213') # U+200B, which breaks an @mention or a URL without deleting a character
clean() { # clean <title> -> one line, no control bytes, no HTML, no live link, no mention, at most 200 bytes
  local s=$1
  s=$(printf '%s' "$s" | tr '\n\r\t' '   ' | LC_ALL=C tr -d '\000-\010\013\014\016-\037\177<>')
  s=${s//@/@$zw}
  s=${s//](/] (}
  s=${s//:\/\//:/$zw/}
  if [ "$(printf '%s' "$s" | wc -c)" -gt 200 ]; then
    s=$(printf '%s' "$s" | head -c 200 | iconv -f UTF-8 -t UTF-8 -c 2>/dev/null)…
  fi
  printf '%s' "$s"
}

tmp=$(mktemp "${out}.XXXXXX") || die 2 "cannot create a file next to $out"
ftmp=
trap 'rm -f "$tmp"; [ -z "$ftmp" ] || rm -f "$ftmp"' EXIT

{
  printf 'Version %s · rev `%s` · bump: %s\n\n' "${version:-unknown}" "$short" "$bump"
  printf 'Prebuilt req for the pinned rev `%s` (ADR 0025). Pin the `.sha256` of the tarball in your own repository.\n\n' "$short"
  printf '## Identity\n\n'

  n_rules=$(const_at "$full" src/canon.rs RULES_V)
  n_schema=$(const_at "$full" src/canon.rs CLAIM_SCHEMA_V)
  n_env=$(const_at "$full" src/baseline.rs BASELINE_VERSION)

  if [ -z "$prev" ]; then
    printf 'First release: no earlier release tag (`rev-*` or `v<version>`) is an ancestor of this rev, so nothing is compared and no history is listed.\n\n'
    printf 'This rev: rules_v %s, claim schema req.claim/%s, baseline envelope %s.\n' \
      "${n_rules:-?}" "${n_schema:-?}" "${n_env:-?}"
  else
    printf 'Previous release: `%s` ([compare](https://github.com/%s/compare/%s...%s)).\n\n' "$prev" "$repo" "$prev" "$short"
    p_rules=$(const_at "$prev" src/canon.rs RULES_V)
    p_schema=$(const_at "$prev" src/canon.rs CLAIM_SCHEMA_V)
    p_env=$(const_at "$prev" src/baseline.rs BASELINE_VERSION)
    changed=0 unknown=0
    if [ -z "$p_rules" ] || [ -z "$n_rules" ]; then
      printf -- '- rules_v could not be determined (previous `%s`, this rev `%s`).\n' "${p_rules:-?}" "${n_rules:-?}"; unknown=1
    elif [ "$p_rules" != "$n_rules" ]; then
      printf -- '- rules_v %s → %s — **changed**: addresses derived under %s are re-canonicalization candidates, never silently rewritten. Re-emit and re-sign your Baseline with a req built from this rev (`req baseline --verify` is UNVERIFIABLE until you do), then re-pin.\n' \
        "$p_rules" "$n_rules" "$p_rules"; changed=1
    fi
    if [ -z "$p_schema" ] || [ -z "$n_schema" ]; then
      printf -- '- the claim schema could not be determined (previous `%s`, this rev `%s`).\n' "${p_schema:-?}" "${n_schema:-?}"; unknown=1
    elif [ "$p_schema" != "$n_schema" ]; then
      # What the old schema means to THIS build is decided by the released rev's own `READS_AS_IS` (`$full`), never by
      # the previous rev's list, whose contents are what would reprint the false line (ADR 0056 falsification 13).
      reads=$(list_at "$full" src/canon.rs READS_AS_IS)
      if [ -z "$reads" ]; then
        printf -- '- claim schema req.claim/%s → req.claim/%s — **changed**: whether a claim file declaring req.claim/%s is loaded as it is could not be determined from `READS_AS_IS` at this rev; see `docs/claim-format.md`.\n' \
          "$p_schema" "$n_schema" "$p_schema"
      elif printf '%s\n' "$reads" | grep -qx "$p_schema"; then
        printf -- '- claim schema req.claim/%s → req.claim/%s — **changed**: a claim file declaring req.claim/%s is loaded as it is by this build — no migration, no message, no address moved; see `docs/claim-format.md`.\n' \
          "$p_schema" "$n_schema" "$p_schema"
      else
        printf -- '- claim schema req.claim/%s → req.claim/%s — **changed**: a claim file declaring the old schema is NOT JUDGED by this build; see `docs/claim-format.md`.\n' \
          "$p_schema" "$n_schema"
      fi
      changed=1
    fi
    if [ -z "$p_env" ] || [ -z "$n_env" ]; then
      printf -- '- the baseline envelope could not be determined (previous `%s`, this rev `%s`).\n' "${p_env:-?}" "${n_env:-?}"; unknown=1
    elif [ "$p_env" != "$n_env" ]; then
      printf -- '- baseline envelope %s → %s — **changed**: read `req baseline --help` for what a manifest of the old envelope still verifies.\n' \
        "$p_env" "$n_env"; changed=1
    fi
    if [ "$unknown" -eq 1 ]; then
      printf '\nIdentity could not be compared in full: read the compare link above before you rely on an unchanged address.\n'
    fi
    if [ "$changed" -eq 0 ] && [ "$unknown" -eq 0 ]; then
      printf 'No identity change: rules_v, the claim schema and the Baseline envelope are unchanged from `%s` (rules_v %s, req.claim/%s, envelope %s).\n' \
        "$prev" "$n_rules" "$n_schema" "$n_env"
    fi
    # What the release workflow decides on (ADR 0055): a known move outranks a value that could not be read.
    if [ "$changed" -eq 1 ]; then identity=moved
    elif [ "$unknown" -eq 1 ]; then identity=unknown
    else identity=unmoved
    fi

    # --- merged since ----------------------------------------------------------------------------
    printf '\n## Merged since `%s`\n\n' "$prev"
    feat= fixes= docs= other=
    count=0
    while IFS= read -r commit; do
      [ -n "$commit" ] || continue
      subject=$(git log -1 --format=%s "$commit")
      body=$(git log -1 --format=%b "$commit" | sed -n '/./{p;q;}')
      number=
      case $subject in
        "Merge pull request '"*) number=$(printf '%s' "$subject" | sed -n "s/.*' (#\([0-9][0-9]*\)).*/\1/p") ;;
        "Merge pull request #"*) number=$(printf '%s' "$subject" | sed -n 's/^Merge pull request #\([0-9][0-9]*\) .*/\1/p') ;;
      esac
      title=$body
      if [ -z "$title" ]; then
        case $subject in
          "Merge pull request '"*) title=$(printf '%s' "$subject" | sed -n "s/^Merge pull request '\(.*\)' (#[0-9][0-9]*).*/\1/p") ;;
        esac
      fi
      [ -n "$title" ] || title=$subject
      line="- $(clean "$title")"
      [ -z "$number" ] || line="$line (#$number)"
      case $title in
        feat | feat[\(:!]*) feat="$feat$line"$'\n' ;;
        fix | fix[\(:!]*) fixes="$fixes$line"$'\n' ;;
        docs | docs[\(:!]*) docs="$docs$line"$'\n' ;;
        *) other="$other$line"$'\n' ;;
      esac
      count=$((count + 1))
    done < <(git log --first-parent --merges --format=%H "${prev}..${full}")
    if [ "$count" -eq 0 ]; then
      printf 'No pull request was merged between the two revs.\n'
    else
      [ -z "$feat" ] || printf '### Features\n\n%s\n' "$feat"
      [ -z "$fixes" ] || printf '### Fixes\n\n%s\n' "$fixes"
      [ -z "$docs" ] || printf '### Documentation\n\n%s\n' "$docs"
      [ -z "$other" ] || printf '### Other\n\n%s\n' "$other"
    fi
  fi
} >"$tmp"

# Publish the finished files; a name that appeared meanwhile is still never replaced. The facts go first, so a
# failure to publish the notes can take them back and leave neither behind.
if [ -n "$facts" ]; then
  ftmp=$(mktemp "${facts}.XXXXXX") || die 2 "cannot create a file next to $facts"
  printf 'previous_tag=%s\nprevious_version=%s\nidentity=%s\n' "$prev" "$prev_version" "$identity" >"$ftmp"
  if ! ln "$ftmp" "$facts" 2>/dev/null; then
    die 1 "$facts already exists: refusing to overwrite it"
  fi
fi
if ! ln "$tmp" "$out" 2>/dev/null; then
  [ -z "$facts" ] || rm -f "$facts"
  die 1 "$out already exists: refusing to overwrite it"
fi
