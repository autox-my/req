#!/usr/bin/env bash
# scripts/release-notes.sh -- the release notes of one rev of req (ADR 0031, amending ADR 0025 section 5).
#
# Usage: scripts/release-notes.sh --rev <rev> --repo <owner/name> --out <file>
#
# Reads git history only; it builds nothing and calls no network service. It writes, in this order:
#   1. which release came before (the nearest `rev-*` tag that is an ancestor of the rev) and a compare link;
#   2. whether req's *identity* moved since then -- RULES_V (src/canon.rs), the claim schema
#      (CLAIM_SCHEMA_V, src/canon.rs) and the Baseline envelope (BASELINE_VERSION, src/baseline.rs) --
#      and what an adopter must do if it did. A value that cannot be read is "could not be determined",
#      never "unchanged";
#   3. the pull requests merged since, newest first, grouped by their conventional-commit prefix.
# Every title is treated as hostile text (see `clean`): the notes become the body of a public release.
#
# Exit: 0 written; 1 --out already exists (never overwritten); 2 usage, an unresolvable rev, or a
# repository name outside owner/name. In every non-zero case --out is left as it was.
set -euo pipefail

die() { # die <status> <message>
  printf 'release-notes: %s\n' "$2" >&2
  exit "$1"
}

rev_in= repo= out=
while [ $# -gt 0 ]; do
  case $1 in
    --rev) [ $# -ge 2 ] || die 2 "--rev needs a value"; rev_in=$2; shift 2 ;;
    --repo) [ $# -ge 2 ] || die 2 "--repo needs a value"; repo=$2; shift 2 ;;
    --out) [ $# -ge 2 ] || die 2 "--out needs a value"; out=$2; shift 2 ;;
    *) die 2 "unknown argument '$1' (usage: --rev <rev> --repo <owner/name> --out <file>)" ;;
  esac
done
[ -n "$rev_in" ] && [ -n "$repo" ] && [ -n "$out" ] || die 2 "usage: --rev <rev> --repo <owner/name> --out <file>"

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

# --- the previous release -----------------------------------------------------------------------------
prev=$(git describe --tags --match 'rev-*' --abbrev=0 "${full}^" 2>/dev/null) || prev=

# --- the three constants, at a commit; empty when they cannot be read --------------------------------
const_at() { # const_at <commit> <file> <NAME>
  # A missing file or constant is an empty answer, not a failure: the caller reports it as undetermined.
  { git show "$1:$2" 2>/dev/null | sed -n "s/^pub const $3: u32 = \([0-9][0-9]*\);.*/\1/p" | head -n 1; } || true
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
trap 'rm -f "$tmp"' EXIT

{
  printf 'Prebuilt req for the pinned rev `%s` (ADR 0025). Pin the `.sha256` of the tarball in your own repository.\n\n' "$short"
  printf '## Identity\n\n'

  n_rules=$(const_at "$full" src/canon.rs RULES_V)
  n_schema=$(const_at "$full" src/canon.rs CLAIM_SCHEMA_V)
  n_env=$(const_at "$full" src/baseline.rs BASELINE_VERSION)

  if [ -z "$prev" ]; then
    printf 'First release: no earlier `rev-*` tag is an ancestor of this rev, so nothing is compared and no history is listed.\n\n'
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
      printf -- '- claim schema req.claim/%s → req.claim/%s — **changed**: a claim file declaring the old schema is NOT JUDGED by this build; see `docs/claim-format.md`.\n' \
        "$p_schema" "$n_schema"; changed=1
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

# Publish the finished file; a name that appeared meanwhile is still never replaced.
if ! ln "$tmp" "$out" 2>/dev/null; then
  die 1 "$out already exists: refusing to overwrite it"
fi
