# 0037. A release carries a record of the proofs its build ran, and `req selftest` is one of them

Amends ADR 0025 (§1, the `build-info` asset: "text, informational only") and ADR 0034 (the selftest was a check an adopter runs; it is now
also one the release runs before it writes an asset).

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-10, GitHub PR #39); asked for by the owner on 2026-11-10
(item 8 of the qualification-readiness package; no issue). Date proposed: 2026-11-10. Changes what a release contains, not what the
engine computes: no `RULES_V` change, no address moves, no change to the engine.

## Context

A release runs three proofs before it writes anything (ADR 0025 §2): the binary names the bare rev, the signed Baseline verifies and the FMR
gate passes. They leave no trace: a published release shows the tarball, a checksum and a `build-info` with the toolchain. Someone who
asks "what was checked about *these bytes*, by what" has the workflow log, which expires and is not attached to the release. An assessor
building a validation file (the tool-qualification question this package has been working on) wants a per-release statement they can
cite, tied to the revision and to the binary's hash.

Two things stand in the way of calling that a validation record: the `req selftest` known-answer checks (ADR 0034) were never part of the
release, and the proofs the script runs are not written down anywhere an asset carries.

## Decision

1. `scripts/build-release.sh` runs a fourth proof, `req selftest`, on the staged binary (the bytes that ship), outside the clean copy because
   it reads no corpus. Exit 0 and the line `selftest: PASS (N checks)` are both required; anything else is exit 1 naming `selftest`, and
   nothing is written. It sits beside the other three; the order (before the assets, after the bench gate) does not matter for the result.
2. The `build-info` asset stays one text file with the same name and the same `key=value` lines, and gains a record: `record=req.release-record/1`,
   `binary_sha256` (of the member `req`, the thing the checksum of the archive does not name by itself), `rules_v`, `claim_schema`, one
   `proof_<name>=pass` line per proof that ran (`built_from`, `baseline_verify`, `bench_fmr`, `selftest`), the Baseline line the verifier
   printed, the corpus hash the bench printed, the selftest's own last line, and
   `attestation=self: …not independent evidence and not signed`. The values are read from the binary's own output; a value that cannot be
   read fails the release, so the record never says what the script did not see.
3. **The asset set does not change.** Still three assets per target, still in the same publish order, still no overwrite. A new asset would
   have meant changes to the publishing job, the six-name check and the ordering rule that tell an adopter a visible tarball is complete,
   for a file that fits where `build-info` already is. The existing `sha=` line the publishing job reads is unchanged.
4. The record names **only what the build job ran**. The gate (`scripts/check.sh`), the test suite, the mutation run and the reproduction check
   are not in it: the first two run on pull requests (CI), the third is a research run (`docs/research/mutation-testing.md`), the fourth is a
   job step whose result is the job's own success. A test pins the absence of those names, so a later edit cannot make the record claim them
   without changing the test, and this ADR, on purpose.

## Consequences

- **It is self-attestation.** The project's runner ran the project's script on the project's binary and wrote down that it passed. It is not
  signed (req never signs; the Baseline is the signed thing and the record quotes its verification), it is not independent, and a compromised
  runner writes the same lines. Its value is as a **record bound to the revision and the binary hash**, and as raw material for an assessor,
  who can re-run `req selftest` and `req baseline --verify` on the downloaded binary and compare. `docs/safety-manual.md` says so.
- A binary that fails `req selftest` is no longer released. Before this, a build that passed the three proofs could in principle ship with a
  failing selftest; no release has done so (the selftest postdates both).
- Releases made before it (`rev-ec87808`, `rev-6b74f51`) have no record and are not changed: their `build-info` is the old, shorter file.
- The `build-info` of a reproduction (`scripts/reproduce.sh`) gains the same lines; the toolchain comparison it prints reads only `rustc`,
  `cargo`, `libc` and `target`, so it is unaffected.
- The line `built_utc=` is still the builder's clock. It is not part of the tarball, so the reproducibility of the archive (ADR 0035) is not
  affected.

## Rejected

- **A separate `validation.json` asset.** Cleaner to parse, but the cost above for the first dispatch after a long run of release-workflow
  changes. It can be split out later by an ADR if an assessor's tooling asks for JSON.
- **Signing the record.** req does not sign (ADR 0010) and a signature by the runner would add a key to guard for no independence.
- **Putting the gate and test results in the record.** They did not run in the build job. Re-running the suite there (two more minutes, a
  second implementation of the gate's logic) would make the record longer and the claim no stronger.
