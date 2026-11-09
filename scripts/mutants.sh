#!/usr/bin/env bash
# Mutation testing of the Tier-1 canonicalizer and the direction classifier (docs/research/mutation-testing.md).
#
# NOT part of scripts/check.sh: a full run is ~2 hours. It is evidence about the test suite, not a gate.
# Needs cargo-mutants (run with 27.1.0: `cargo install --locked --version 27.1.0 cargo-mutants`) and cargo-nextest.
# This is a development tool, not a pin in Cargo.lock; the report says which version produced its numbers.
#
#   scripts/mutants.sh [OUT_DIR] [extra cargo-mutants args]     e.g. `--iterate` to rerun only what survived
#
# Exit code is cargo-mutants' own: 0 all caught, 2 some missed, 3 timeouts, 4 baseline fails.
# cargo-mutants copies the tree WITHOUT `.git`, so test binaries that shell out to git cannot run in it
# (baseline, decision, public_tree, projection, orchestration, junit_case, adopt, diff, corpus_pin). They are left
# out of the selected set and the loader functions only they exercise are excluded (see the report's limits).
set -euo pipefail
cd "$(dirname "$0")/.."

out="${1:-target/mutants-report}"
[ "$#" -gt 0 ] && shift

tests=(address_recompute canon_coverage claim_slots canonical_form compare direction_gate evidence gate membership
       pins qualitative tier1_v2 selftest claim_v2 loader release_gate json_format tier1_edges direction_edges)
args=(--cargo-test-arg=--lib)
for t in "${tests[@]}"; do args+=(--cargo-test-arg=--test "--cargo-test-arg=$t"); done

CARGO_PROFILE_DEV_DEBUG=0 exec cargo mutants \
  --test-tool nextest \
  -f src/canon.rs -f src/compare.rs \
  -E 'load_claims|run_claim_diagnostics|looks_like_claim|is_claim_stem|load_registry|parse_claim|classify_claim_schema|claim_schema' \
  "${args[@]}" -j 3 -o "$out" "$@"
