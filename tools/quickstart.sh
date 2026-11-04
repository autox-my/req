#!/usr/bin/env bash
# tools/quickstart.sh — rehearse the five-minute adoption path, for real, offline, and time it.
#
# What this proves (all of it observed, none of it asserted from memory):
#   * a fresh project reaches a gated, rendered corpus from a `spec.md` with the
#     commands docs/adopting.md ("For an agent") prints, in a scratch directory, with no network;
#   * the extractor drafts every candidate as a `req.claim/2` claim carrying nothing derived
#     (ADR 0022), names a skipped line with its reason, and writes nothing on a dry run;
#   * an authored `applicability` and the statement are all the address needs: `req canon`
#     shows the parse each claim derives at load, with nothing to fill and nothing to verify;
#   * a corpus carrying a selector makes bare `gate` UNVERIFIABLE (exit 3, ADR 0015)
#     while the declared variant gates PASS, and every claim is decided;
#   * the release gate's test-evidence policy (ADR 0019) answers honestly before any link
#     exists ("no linked test", exit 3), and the declared variant gates PASS once each
#     claim is linked to a test the supplied report carries as passing;
#   * `render --out` writes a projection, `render --check` proves it current, and an
#     edited projection is STALE (exit 1) — the alarm can fail, so it is a check.
#
# What it deliberately does NOT do: install req. The networked
# `cargo install --git … --rev … --locked` is the *adopter's* step and belongs in
# prose (docs/adopting.md, "Install req"), not in this script. It also does not
# sign a Baseline (that is a human act with a key req never sees), does not write a
# decision log, and does not run the repository gate.
#
# Properties held by construction:
#   * offline — no network call, no `cargo install`; a prebuilt binary is used when
#     one exists and `cargo build -q --offline` is only the last resort;
#   * no $HOME writes — no command here sets HOME, and every corpus command runs in
#     $WORK with REQ_ROOT unset, so the root is resolved by walking up from $WORK.
#     (The build fallback is the one exception to "nothing else": cargo writes
#     CARGO_HOME, which this script takes from <repo>/.cargo-home when that in-repo
#     dir exists and otherwise points at $WORK/cargo-home; its build output goes to
#     $WORK/target, never the checkout or the user's ~/.cargo.)
#     Python would be the other: the system interpreter caches compiled stdlib
#     modules under $HOME/Library/Caches/com.apple.python (measured: 54 files on a
#     fresh HOME), so bytecode writing is switched off for the whole run below.
#     With that, nothing writes outside $WORK;
#   * idempotent — a fresh `mktemp -d` per run, removed on success, kept and named
#     on failure, on SIGINT/SIGTERM (or with QUICKSTART_KEEP=1);
#   * stdout carries EXACTLY one line, `QUICKSTART <n>s`: progress, step timings and
#     every failure diagnostic go to stderr. The measured window starts at the first
#     adoption step; preflight (binary resolution, a possible offline build, the
#     scratch dir) is timed separately and logged. Exit 0 only when every step passed
#     and n < 300; otherwise a `QUICKSTART: FAIL …` diagnostic goes to stderr and the
#     exit code is non-zero.
#   * the 300s budget is a **post-hoc verdict**, not a deadline: this script does not
#     kill a wedged command (that is the caller's timeout, CI's job timeout), it
#     reports that a completed rehearsal exceeded five minutes.
#
# Usage:
#   tools/quickstart.sh
#   REQ_BIN=/path/to/req tools/quickstart.sh      # a binary that is not the repo build
#   QUICKSTART_KEEP=1 tools/quickstart.sh         # keep the scratch corpus on success
#
# The repository root is resolved from this script's own path, so it runs from any cwd.

set -u -o pipefail

START="$(date +%s)"
WORK_DIR=""
STEP_INDEX=0
STEP_NAME="(preflight)"
STEP_T0="$START"
OUT=""
RC=0

log() { printf '%s\n' "$*" >&2; }


# fail <message>: report, name the scratch dir (so the failure can be inspected), and
# exit non-zero — but still print the one measured stdout line, so a failed run is timed
# too (a slow failure is a finding, not a missing measurement).
fail() {
  local elapsed=$(( $(date +%s) - START ))
  printf 'QUICKSTART: FAIL — %s\n' "$*" >&2
  if [ -n "$WORK_DIR" ]; then
    printf 'QUICKSTART: scratch kept for inspection: %s\n' "$WORK_DIR" >&2
  fi
  printf 'QUICKSTART %ss\n' "$elapsed"
  exit 1
}

# A signal must not leave an anonymous scratch dir: the header promises one that is
# removed or named, and an interrupted run in CI (a cancelled job) is the normal case.
on_signal() {
  printf 'QUICKSTART: interrupted — scratch kept for inspection: %s\n' "${WORK_DIR:-<none>}" >&2
  printf 'QUICKSTART %ss\n' "$(( $(date +%s) - START ))"
  exit 130
}

# Installed after `on_signal` exists: a signal in the window before the definition would run an
# undefined command, and the trap would return non-zero instead of exiting 130.
trap on_signal INT TERM

step_start() {
  STEP_INDEX=$((STEP_INDEX + 1))
  STEP_NAME=$1
  STEP_T0="$(date +%s)"
  log ""
  log "==> [${STEP_INDEX}] ${STEP_NAME}"
}

step_ok() {
  local now dt
  now="$(date +%s)"
  dt=$((now - STEP_T0))
  log "    ok  ${STEP_NAME}  (${dt}s)"
}

# run_step <name> <allowed-exit> <display-command> <cmd...>
# Runs one command, captures stdout+stderr, and on any exit but the allowed one prints
# the step, the command, the exit code and the whole output to stderr, then fails.
run_step() {
  local name=$1 want=$2 display=$3
  shift 3
  step_start "$name"
  OUT="$("$@" 2>&1)"
  RC=$?
  if [ "$RC" -ne "$want" ]; then
    printf -- '--- step failed: %s\n    command: %s\n    cwd: %s\n    exit: %s (allowed: %s)\n    output:\n%s\n' \
      "$STEP_NAME" "$display" "$PWD" "$RC" "$want" "$OUT" >&2
    fail "$STEP_NAME (exit $RC, expected $want): $display"
  fi
  step_ok
}

# require_out <needle>: the step's captured output must contain it. A zero exit is not
# enough when a wrong-but-zero answer is possible.
require_out() {
  case "$OUT" in
    *"$1"*) : ;;
    *)
      printf -- '--- assertion failed: %s\n    expected output to contain: %s\n    output:\n%s\n' \
        "$STEP_NAME" "$1" "$OUT" >&2
      fail "$STEP_NAME: output did not contain: $1"
      ;;
  esac
}

require_file() {
  [ -f "$1" ] || fail "$STEP_NAME: expected file $1 (cwd $PWD)"
}

require_absent() {
  [ ! -e "$1" ] || fail "$STEP_NAME: unexpected file $1 (cwd $PWD)"
}

require_out_re() { # <extended-regex>: the captured output must match it, not merely contain it
  # A here-string, not `printf | grep -q`: under `pipefail`, `grep -q` closes the pipe on its
  # first match and a large `$OUT` makes `printf` die of SIGPIPE, failing the pipeline even
  # though the pattern matched.
  if grep -Eq "$1" <<< "$OUT"; then
    :
  else
    printf -- '--- assertion failed: %s\n    expected output to match: %s\n    output:\n%s\n' \
      "$STEP_NAME" "$1" "$OUT" >&2
    fail "$STEP_NAME: output did not match: $1"
  fi
}

require_grep() { # <pattern> <file> <why>
  grep -Eq "$1" "$2" || fail "$STEP_NAME: $3 ($2 does not match /$1/)"
}

require_no_grep() { # <pattern> <file> <why>
  # grep answers 0 match, 1 no match, 2 could not read: only 1 is the answer asked for, so an
  # unreadable file fails the step rather than passing as "no match".
  local status=0
  grep -Eq "$1" "$2" || status=$?
  case "$status" in
    0) fail "$STEP_NAME: $3 ($2 matches /$1/)" ;;
    1) ;;
    *) fail "$STEP_NAME: could not read $2 to check that $3 (grep exit $status)" ;;
  esac
}

claim_count() {
  local n=0 f
  for f in "$WORK_DIR"/claims/REQ-*.yml; do
    [ -e "$f" ] && n=$((n + 1))
  done
  printf '%s' "$n"
}

digest_of() { # extract the sha256:… token from a render line
  printf '%s\n' "$1" | sed -n 's/.*\(sha256:[0-9a-f]\{64\}\).*/\1/p' | head -1
}

# --- preflight: resolve the repo, the binary and the scratch dir --------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"

TMP_BASE="${TMPDIR:-/tmp}"
TMP_BASE="${TMP_BASE%/}"
WORK_DIR="$(mktemp -d "${TMP_BASE}/req-quickstart.XXXXXX")" || {
  printf 'QUICKSTART: FAIL — mktemp -d failed under %s\n' "$TMP_BASE" >&2
  printf 'QUICKSTART %ss\n' "$(( $(date +%s) - START ))"
  exit 1
}
# `mktemp -d` returns a RELATIVE path when TMPDIR is relative. Every later use of $WORK_DIR happens
# after `cd "$WORK_DIR"`, so a relative one would resolve against the scratch dir itself (a count
# of 0, a build landing outside $WORK, an `rm -rf` of the wrong path). Make it absolute now.
if ! RESOLVED_WORK_DIR="$(cd "$WORK_DIR" && pwd -P)"; then
  fail "cannot resolve the scratch dir to an absolute path"
fi
WORK_DIR="$RESOLVED_WORK_DIR"
# The same resolution for the base: on macOS `/var` is a symlink to `/private/var`, so the
# cleanup guard below must compare the RESOLVED directory with the RESOLVED scratch path.
if ! TMP_BASE="$(cd "$TMP_BASE" && pwd -P)"; then
  fail "cannot resolve the temp base to an absolute path"
fi

log "req quickstart: repo $REPO_ROOT"
log "req quickstart: scratch $WORK_DIR"

# A caller-supplied REQ_BIN must be an executable regular file; otherwise name the
# offending value and refuse (a directory, a script without +x or a typo would fail
# later, inside a Python child process, with a far worse message).
if [ -n "${REQ_BIN:-}" ]; then
  REQ_BIN_GIVEN="$REQ_BIN"
  case "$REQ_BIN_GIVEN" in
    */*) REQ_CAND="$REQ_BIN_GIVEN" ;;
    *) REQ_CAND="$(command -v "$REQ_BIN_GIVEN" 2>/dev/null || true)" ;;
  esac
  if [ -z "${REQ_CAND:-}" ] || [ ! -f "$REQ_CAND" ] || [ ! -x "$REQ_CAND" ]; then
    fail "REQ_BIN is not an executable regular file: $REQ_BIN_GIVEN"
  fi
  REQ_BIN="$(cd "$(dirname "$REQ_CAND")" && pwd -P)/$(basename "$REQ_CAND")"
else
  REQ_BIN=""
  # A caller may redirect the build directory (CARGO_TARGET_DIR — `scripts/check.sh` deliberately
  # does not set it), and then the binary is there, not under `$REPO_ROOT/target`.
  TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
  case "$TARGET_DIR" in
    /*) : ;;
    *) TARGET_DIR="$REPO_ROOT/$TARGET_DIR" ;;
  esac
  for REQ_CAND in "$TARGET_DIR/debug/req" "$TARGET_DIR/release/req"; do
    if [ -f "$REQ_CAND" ] && [ -x "$REQ_CAND" ]; then
      REQ_BIN="$(cd "$(dirname "$REQ_CAND")" && pwd -P)/$(basename "$REQ_CAND")"
      break
    fi
  done
  if [ -z "$REQ_BIN" ]; then
    log "no prebuilt binary under $TARGET_DIR — building offline into the scratch dir"
    if [ -z "${CARGO_HOME:-}" ]; then
      if [ -d "$REPO_ROOT/.cargo-home" ]; then
        CARGO_HOME="$REPO_ROOT/.cargo-home"
      else
        # Never the user's `~/.cargo`: this script's property is that nothing is written outside
        # $WORK. On a fresh checkout there is no in-repo cargo home to borrow, and the offline
        # build then fails loudly — the honest outcome, since the fallback is for a checkout
        # that has built at least once.
        CARGO_HOME="$WORK_DIR/cargo-home"
        mkdir -p "$CARGO_HOME"
      fi
      export CARGO_HOME
    fi
    command -v cargo >/dev/null 2>&1 \
      || fail "no prebuilt req binary and cargo is not on PATH"
    # Build INTO $WORK: the rehearsal must not write into the checkout, which is the property
    # the header states (and which a redirected CARGO_TARGET_DIR would otherwise dodge while
    # still writing `target/`). `--locked` keeps the committed Cargo.lock from moving.
    ( cd "$REPO_ROOT" && CARGO_TARGET_DIR="$WORK_DIR/target" cargo build -q --offline --locked ) \
      || fail "cargo build -q --offline --locked failed in $REPO_ROOT"
    REQ_CAND="$WORK_DIR/target/debug/req"
    [ -f "$REQ_CAND" ] && [ -x "$REQ_CAND" ] \
      || fail "cargo build produced no executable at $REQ_CAND"
    REQ_BIN="$(cd "$(dirname "$REQ_CAND")" && pwd -P)/$(basename "$REQ_CAND")"
  fi
fi
export REQ_BIN

# tools/adopt.py shells out to the `req` on PATH, so make the chosen binary the one a
# child process finds. The shim also makes a custom-named REQ_BIN work; the binary's own
# directory goes on PATH as well, which is the plain case when it is already called `req`.
mkdir -p "$WORK_DIR/bin"
ln -s "$REQ_BIN" "$WORK_DIR/bin/req" || fail "cannot link $REQ_BIN as $WORK_DIR/bin/req"
PATH="$WORK_DIR/bin:$(dirname "$REQ_BIN"):$PATH"
export PATH
[ "$(command -v req)" = "$WORK_DIR/bin/req" ] \
  || fail "could not put the resolved req on PATH (found: $(command -v req || true))"

unset REQ_ROOT
# The system Python (measured: /usr/bin/python3 on macOS) caches compiled stdlib modules under
# $HOME/Library/Caches/com.apple.python, which would falsify the "no $HOME writes" property in
# the header on the maintainer's own platform. Bytecode writing is what creates them, so it is
# off for every interpreter this script runs; `python3 -B` would only cover the one call site.
PYTHONDONTWRITEBYTECODE=1
export PYTHONDONTWRITEBYTECODE
command -v python3 >/dev/null 2>&1 || fail "python3 is not on PATH (tools/adopt.py needs it)"
[ -f "$REPO_ROOT/tools/adopt.py" ] || fail "missing extractor: $REPO_ROOT/tools/adopt.py"

cd "$WORK_DIR" || fail "cannot cd into the scratch dir: $WORK_DIR"

log ""
log "req quickstart: binary $REQ_BIN"
"$REQ_BIN" --version >&2 || fail "the resolved binary does not answer --version: $REQ_BIN"

# The measured window starts here: `QUICKSTART <n>s` is the adoption path the page
# describes, not this script's own setup. Preflight is timed and logged on its own.
PREFLIGHT_ELAPSED=$(( $(date +%s) - START ))
START="$(date +%s)"
log "req quickstart: preflight ${PREFLIGHT_ELAPSED}s (not in the measured line)"

# --- the five-minute path ---------------------------------------------------------

step_start "init a fresh corpus (req init)"
OUT="$(req init 2>&1)"
RC=$?
if [ "$RC" -ne 0 ]; then
  printf -- '--- step failed: %s\n    command: req init\n    cwd: %s\n    exit: %s (allowed: 0)\n    output:\n%s\n' \
    "$STEP_NAME" "$PWD" "$RC" "$OUT" >&2
  fail "$STEP_NAME (exit $RC): req init"
fi
require_out "claims/REQ-0001.yml"
require_out "aliases/subjects.yml"
require_file claims/REQ-0001.yml
require_file aliases/subjects.yml
step_ok

step_start "write the adopter's spec document (docs/capabilities.md)"
mkdir -p docs || fail "$STEP_NAME: cannot create docs/"
cat > docs/capabilities.md <<'SPEC'
# Capabilities

- The system shall complete within 200 ms.
- Operators can review every recorded decision.
- Operators can review every recorded decision.
SPEC
# One structured `shall`, one prose bullet and a repeat of it: both statements draft (their
# parse is derived, never authored), and the repeat is skipped with its reason.
[ -s docs/capabilities.md ] || fail "$STEP_NAME: docs/capabilities.md is empty"
step_ok

TODAY="$(date +%Y-%m-%d)"
CLAIMS_BEFORE="$(claim_count)"
[ "$CLAIMS_BEFORE" -eq 1 ] || fail "preflight: expected 1 claim after init, found $CLAIMS_BEFORE"

run_step "extract a draft, dry run (nothing may be written)" 0 \
  "python3 $REPO_ROOT/tools/adopt.py --from docs/capabilities.md --date $TODAY (dry run)" \
  python3 "$REPO_ROOT/tools/adopt.py" --from docs/capabilities.md --date "$TODAY"
require_out "draft  REQ-0002  claims/REQ-0002.yml  docs/capabilities.md#L3"
require_out "draft  REQ-0003  claims/REQ-0003.yml  docs/capabilities.md#L4"
require_out "skip   docs/capabilities.md#L5  duplicate of #L4"
require_out "dry run: nothing written"
# The property the dry run exists for: it wrote no claim at all.
require_absent claims/REQ-0002.yml
[ "$(claim_count)" -eq "$CLAIMS_BEFORE" ] \
  || fail "$STEP_NAME: the dry run changed the claim count ($CLAIMS_BEFORE -> $(claim_count))"

run_step "write the drafts (the extractor's --write)" 0 \
  "python3 $REPO_ROOT/tools/adopt.py --from docs/capabilities.md --date $TODAY --write" \
  python3 "$REPO_ROOT/tools/adopt.py" --from docs/capabilities.md --date "$TODAY" --write
require_out "wrote: 2 claim(s)"
require_file claims/REQ-0002.yml
require_file claims/REQ-0003.yml
# A draft carries NOTHING derived (req.claim/2): no address, no canonical form, no guessed pin.
for draft in claims/REQ-0002.yml claims/REQ-0003.yml; do
  require_no_grep '^(address|canonical|pins):' "$draft" "the draft carries a derived field or a pin"
  require_grep '^applicability: \[\]$' "$draft" "the draft has no empty applicability anchor"
done
[ "$(claim_count)" -eq $((CLAIMS_BEFORE + 2)) ] \
  || fail "$STEP_NAME: expected $((CLAIMS_BEFORE + 2)) claims, found $(claim_count)"

derived_address() { # <claim id> — the address `req render` derives for it, now
  # The whole document is read before the script answers: exiting at the first match would close
  # the pipe on `req render` and fail the pipeline under `pipefail` (as `require_out` notes).
  # `req render` exits 3 when a claim is undecided (the case after the edit): the document is
  # still complete, so 0 and 3 are both fine here.
  { req render --format json 2>/dev/null || [ $? -eq 3 ]; } | python3 -c '
import json, sys
want = sys.argv[1]
found = []
def walk(node):
    if isinstance(node, dict):
        if node.get("claim_id") == want and "address" in node:
            found.append(node["address"])
        for value in node.values():
            walk(value)
    elif isinstance(node, list):
        for value in node:
            walk(value)
walk(json.load(sys.stdin))
if len(found) != 1:
    raise SystemExit(1)
print(found[0])
' "$1"
}

step_start "author ONE applicability predicate into a draft (the address carries it)"
ADDRESS_BEFORE="$(derived_address REQ-0002)" || fail "$STEP_NAME: no derived address for REQ-0002 before the edit"
AUTHOR_OUT="$(python3 - "$WORK_DIR/claims/REQ-0002.yml" <<'PY' 2>&1
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
anchor = "applicability: []"
if text.count(anchor) != 1:
    raise SystemExit("expected exactly one 'applicability: []' line in %s" % path)
path.write_text(text.replace(anchor, "applicability:\n  - deployment_mode == air_gapped"))
print("authored applicability: deployment_mode == air_gapped", file=sys.stderr)
PY
)"
RC=$?
if [ "$RC" -ne 0 ]; then
  printf -- '--- step failed: %s\n    command: python3 author applicability in claims/REQ-0002.yml\n    exit: %s (allowed: 0)\n    output:\n%s\n' \
    "$STEP_NAME" "$RC" "$AUTHOR_OUT" >&2
  fail "$STEP_NAME (exit $RC)"
fi
require_grep '^  - deployment_mode == air_gapped$' claims/REQ-0002.yml \
  "the authored applicability predicate is not in the draft"
# The claim in the step's title: the applicability is part of the derived address, so it moved.
ADDRESS_AFTER="$(derived_address REQ-0002)" || fail "$STEP_NAME: no derived address for REQ-0002 after the edit"
[ "$ADDRESS_BEFORE" != "$ADDRESS_AFTER" ] \
  || fail "$STEP_NAME: authoring an applicability predicate did not move REQ-0002's address ($ADDRESS_BEFORE)"
step_ok

run_step "req canon shows the derived parse (exit 0 by contract; no pin flagged)" 0 \
  "req canon" req canon
require_out "claims: 3 (1 unstructured); pins: 3 on 1 claim(s), 0 flagged"
require_out_re '^REQ-0002 +structured +SHALL '

step_start "declare the release variant (.req/variants)"
mkdir -p .req || fail "$STEP_NAME: cannot create .req/"
printf 'deployment_mode=air_gapped\n' > .req/variants \
  || fail "$STEP_NAME: cannot write .req/variants"
[ -s .req/variants ] || fail "$STEP_NAME: .req/variants is empty"
step_ok

run_step "bare req gate is UNVERIFIABLE on a selector-carrying corpus (ADR 0015)" 3 \
  "req gate" req gate
require_out "verdict: UNVERIFIABLE"

# The release gate now also asks the flagship question — does a test that proves each
# applying claim pass? — and a claim with no link is a finding, not a pass. With no report
# supplied the policy cannot be exercised at all (exit 3); that is the honest first answer,
# and the next two steps are the remedy a real project applies.
run_step "gate with no test evidence is UNVERIFIABLE (a policy that cannot be exercised)" 3 \
  "req gate deployment_mode=air_gapped" req gate deployment_mode=air_gapped
require_out "no --test-report"

step_start "link each claim to its proving test and write the report the gate reads"
python3 - "$WORK_DIR/claims/REQ-0001.yml" "$WORK_DIR/claims/REQ-0002.yml" "$WORK_DIR/claims/REQ-0003.yml" <<'PY'
import pathlib
import sys

for path in sys.argv[1:]:
    claim = pathlib.Path(path)
    text = claim.read_text()
    # Every req.claim/2 claim carries exactly one `relations:` key at the top level; the link
    # goes right before it (an authored field, like everything in the file).
    if text.count("\nrelations:") != 1:
        raise SystemExit("expected exactly one `relations:` line in %s" % claim)
    anchor = "\nrelations:"
    claim_id = claim.stem
    test_id = "quickstart::%s" % claim_id.lower().replace("-", "_")
    claim.write_text(text.replace(anchor, '\ntests:\n  - "%s"' % test_id + anchor))
    print("linked %s -> %s" % (claim_id, test_id), file=sys.stderr)
PY
RC=$?
if [ "$RC" -ne 0 ]; then
  printf -- '--- step failed: %s\n    exit: %s (allowed: 0)\n' "$STEP_NAME" "$RC" >&2
  fail "$STEP_NAME (exit $RC)"
fi
# A stand-in for the adopter's own nextest run: the artefact nextest writes, carrying the three
# linked tests as passed. The gate is a reader, and what it can prove is what the report says.
printf '%s\n' \
  '<?xml version="1.0" encoding="UTF-8"?>' \
  '<testsuites name="quickstart" tests="3" skipped="0" failures="0" errors="0">' \
  '    <testsuite name="quickstart">' \
  '        <testcase name="req_0001" classname="quickstart"/>' \
  '        <testcase name="req_0002" classname="quickstart"/>' \
  '        <testcase name="req_0003" classname="quickstart"/>' \
  '    </testsuite>' \
  '</testsuites>' > quickstart-junit.xml \
  || fail "$STEP_NAME: cannot write the JUnit report"
step_ok

run_step "req gate deployment_mode=air_gapped (the release gate, with evidence)" 0 \
  "req gate --test-report quickstart-junit.xml deployment_mode=air_gapped" \
  req gate --test-report quickstart-junit.xml deployment_mode=air_gapped
# A boundary, not a substring: `0 undecided` also occurs inside `10 undecided`.
require_out_re '(^|[^0-9])0 undecided' 
require_out "verdict: PASS"

run_step "req render --out docs/spec.md writes the projection" 0 \
  "req render --out docs/spec.md" req render --out docs/spec.md
require_out "[WROTE] docs/spec.md"
require_file docs/spec.md
RENDER_DIGEST="$(digest_of "$OUT")"
[ -n "$RENDER_DIGEST" ] || fail "$STEP_NAME: the render line carried no sha256 identity"
# The header names what generated it; the body keeps an undecided claim visible rather
# than dropping it from the selection.
require_grep '^<!-- req projection: generator=req render' docs/spec.md \
  "docs/spec.md has no projection header"
require_grep 'identity=sha256:[0-9a-f]+' docs/spec.md \
  "docs/spec.md's header carries no identity"
require_grep 'undecided 1' docs/spec.md \
  "the projection dropped the undecided claim instead of showing it"

run_step "req render --check docs/spec.md is CURRENT (the drift alarm)" 0 \
  "req render --check docs/spec.md" req render --check docs/spec.md
require_out "[CURRENT] docs/spec.md"

step_start "the projection check is an alarm: an edited projection is STALE"
printf '\nhand edit outside the corpus\n' >> docs/spec.md \
  || fail "$STEP_NAME: cannot edit docs/spec.md"
OUT="$(req render --check docs/spec.md 2>&1)"
RC=$?
if [ "$RC" -ne 1 ]; then
  printf -- '--- step failed: %s\n    command: req render --check docs/spec.md (after a hand edit)\n    exit: %s (allowed: 1)\n    output:\n%s\n' \
    "$STEP_NAME" "$RC" "$OUT" >&2
  fail "$STEP_NAME (exit $RC, expected 1): a hand-edited projection must be STALE"
fi
require_out "[STALE] docs/spec.md"
step_ok

run_step "regenerate the projection after the alarm" 0 \
  "req render --out docs/spec.md" req render --out docs/spec.md
require_out "[WROTE] docs/spec.md"
[ "$(digest_of "$OUT")" = "$RENDER_DIGEST" ] \
  || fail "$STEP_NAME: regeneration produced a different identity ($RENDER_DIGEST -> $(digest_of "$OUT"))"

run_step "req render --check docs/spec.md is CURRENT again" 0 \
  "req render --check docs/spec.md" req render --check docs/spec.md
require_out "[CURRENT] docs/spec.md"

# --- the measurement --------------------------------------------------------------

ELAPSED=$(( $(date +%s) - START ))
log ""
log "req quickstart: ${STEP_INDEX} steps passed in ${ELAPSED}s (budget: < 300s)"

if [ "$ELAPSED" -ge 300 ]; then
  fail "the path passed but took ${ELAPSED}s, at or over the 300s budget"
fi

if [ "${QUICKSTART_KEEP:-0}" = "1" ]; then
  log "req quickstart: kept $WORK_DIR (QUICKSTART_KEEP=1)"
else
  # Remove exactly the directory mktemp created for this run, and nothing else.
  case "$WORK_DIR" in
    "${TMP_BASE}"/req-quickstart.*) rm -rf "$WORK_DIR" \
      || fail "could not remove the scratch dir: $WORK_DIR" ;;
    *) fail "refusing to remove an unexpected scratch path: $WORK_DIR" ;;
  esac
fi

printf 'QUICKSTART %ss\n' "$ELAPSED"
exit 0
