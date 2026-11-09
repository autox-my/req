# 0032. The macOS tarball is built by the release workflow, and a release has both targets or none

**Status: proposed** — the owner ratifies it by merging the pull request that carries it; asked for by the owner on
2026-11-09 (no issue). Date proposed: 2026-11-09. **Amends [ADR 0025](0025-prebuilt-binary-per-pinned-rev.md) §1, §2 and §5**
(who builds the macOS tarball, and what a release is); the assets, their names, the proofs of §2 and the no-overwrite rule of §5
are exactly as before.

## Context

ADR 0025 had the macOS tarball built by the same script on the owner's machine and attached by hand. That worked once
(`rev-6b74f51`) and showed what it costs: the bytes depend on one laptop, nothing but the owner's say-so ties them to the commit
(the script's proofs run, but a reader cannot re-run them on the laptop), the archive carried the local account name as the
owner of the file, the binary kept its debug info because the toolchain on that machine could not strip it, and every release
needs the owner at a keyboard after the workflow has finished. The Linux tarball has none of those problems because a runner
builds it, with a job that holds no token (ADR 0025 §5 as amended by ADR 0029).

GitHub hosts Apple-silicon runners (`macos-14`), so the reason ADR 0025 gave for the owner's machine (no pinned way to build
macOS on the Linux runner; cross-compiling was rejected) no longer applies: this is a native build, not a cross-compile.

## Decision

1. **A second build job, `build-macos`, on `macos-14`**, runs `scripts/build-release.sh` for `aarch64-apple-darwin`, the only macOS
   target ADR 0025 supports. It holds `contents: read` and no token or secret, exactly like the Linux job: it runs the rev's
   code. It runs only when dispatched from `main`, takes the full SHA the Linux job resolved (so a ref given as the input cannot
   move between the two builds), and re-checks that the SHA is an ancestor of `origin/main`.
2. **The proofs are the script's, unchanged**: `built from` is the bare rev, `req baseline --verify` is `[VERIFIED]`, the FMR gate
   passes, and a missing `minisign` is UNVERIFIABLE (exit 3) and blocks. The macOS runner gets `minisign` from Homebrew when
   it is absent.
3. **A release has both targets or none.** The publishing job needs both builds, receives both artifacts, validates each target
   the same way (three assets, a sidecar that matches its tarball, a `.build-info` that names the resolved commit, the same
   rev for both) and refuses unless **all six names are free on the release**, before it uploads the first. It uploads Linux then
   macOS, each target's tarball last. A failure of either build, including a macOS runner outage, means no release and a second
   dispatch; nothing is half-published by this path.
4. **The publishing job is otherwise unchanged**: it checks out nothing, runs no script of the repository, never edits a release and
   never replaces an asset (§5). It prints one `pin (<target>): REV=… SHA256=… TARGET=…` line per target.
5. **The by-hand macOS path stays as a fallback**, documented in `docs/releasing.md`, for a rev released before this (which has no
   macOS asset, or one made by hand) and for a publish that died between the Linux tarball and the macOS assets: the workflow then
   refuses a second dispatch because the Linux names are taken, and the owner attaches the macOS assets as before.

## Consequences

- The macOS asset of a new release is built by GitHub, from main's script, on a runner anyone can read the log of. It is still
  **unsigned and not notarized**; a browser download may be quarantined by Gatekeeper, while the installer's `curl` is not.
- The release is slower (the macOS build follows the Linux job) and has one more thing that can fail. That is the price of "both or none";
  the alternative, a release that sometimes lacks macOS, would make every adopter check which targets a release has.
- **The bytes are not reproducible.** Two builds of the same rev on one machine gave tarballs with different hashes (observed
  2026-11-09 on a Mac; probably the temporary build directory embedded in the binary). The pin is the hash of the *published*
  bytes, so this does not weaken ADR 0025, but it means "rebuild it and compare" is not a way to verify a release.
- Intel Macs and Windows are still not built (`docs/adopting.md`).
- **This job has not run on a runner yet**: its first dispatch is its first real run. The publishing script is run against a fake
  `gh` and real asset shapes by `tests/release_publish.rs`, and `actionlint` is clean, but a runner without `brew`, or whose Rust
  differs, would show up only then. A failed first run costs a red run and no release.

## Rejected

- **A matrix on the existing build job.** Each leg would resolve the input itself, and a moving ref could give two commits; the
  publishing job's equality check would catch it, but only after both builds.
- **Publishing Linux when macOS fails** (`if: always()`). A release then has a silent hole and the workflow would need the
  `continue-on-error` its tests forbid.
- **Cross-compiling macOS on the Linux runner.** Still rejected by ADR 0025: no pinned toolchain, and the proofs run the binary.
- **Signing and notarizing now.** Needs an Apple developer identity and a secret in the repository; a decision of its own.
