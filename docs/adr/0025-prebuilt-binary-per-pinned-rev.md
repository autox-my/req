# 0025. A prebuilt, checksummed binary per pinned rev; the install downloads and fails closed

Amended by ADR 0029 §4–§5 and ADR 0031 (the release notes).

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-05, former Forgejo PR #116); issue #65
(opened from the Sedia evaluation). Date proposed: 2026-11-05. **Adds an install path; removes none.**
`cargo install --git … --rev <PIN> --locked` stays, documented, as the source path and the air-gapped path
(ADR 0009's toolchain, REQ-0012). "The rev is the provenance" (`docs/adopting.md`) is unchanged: it now has
two ways to be honoured.

## Context

Every adopter job installs req with `cargo install --git ssh://… --rev <PIN> --locked`. That needs a Rust
toolchain on the runner, an SSH read key for the private host (the template manages a deploy key for it) and a
compile from source per job. A Python-first adopter pays all three for a tool whose claim operations need no
network, and a person who wants the same binary on a laptop pays the compile too. The project has paid this cost
once already and fixed it the same way: nextest is fetched as a prebuilt tarball with a pinned SHA-256 (ADR 0019;
"8+ minutes → seconds"), and a swapped artefact fails the step that fetched it.

What makes req's case different from nextest's: req's own identity is its rev. `req --version` prints
`built from <sha>` (`build.rs`: bare = clean, `-dirty` = modified, `unknown` = not a checkout), because semver
cannot say which commit a binary is. A downloaded binary is only worth having if that line survives the trip.

## Decision

1. **One release per pinned rev, with one tarball per supported target.** The tag is `rev-<REV>`, where
   `<REV>` is the string the binary prints after `built from` — a bare SHA, never `-dirty`, `unknown` or
   `-cleanliness-unknown`. Assets, per target:
   - `req-<REV>-<TARGET>.tar.gz` — exactly one member, `req` (mode 0755), at the archive root;
   - `req-<REV>-<TARGET>.tar.gz.sha256` — one line, `<64 hex>  <asset name>`;
   - `req-<REV>-<TARGET>.build-info` — text, informational only (rev, target, `rustc -V`, `cargo -V`, the
     builder's libc, UTC build time). Nothing verifies against it.

   Supported targets are `x86_64-unknown-linux-gnu` (the CI runner) and `aarch64-apple-darwin` (the owner's
   machine, so a local, non-CI project uses the same binary). Another target is not "skipped": the install says
   it is unsupported and exits non-zero (§4). The Linux floor is the builder's glibc, stated in `build-info`.
2. **A binary is built from a clean checkout of exactly `<REV>`, and refuses to be released unless it proves it.**
   One script builds it (`--locked`, the pinned `rust-toolchain.toml`, so the fingerprint-path pins of
   AGENTS.md are the same pins), and before it writes any tarball it checks that `req --version` says
   `built from <REV>` (bare), then runs the built binary over this repository's own corpus:
   `req baseline --verify baselines/air-gapped.json` must be `[VERIFIED]` and `req bench` (seed parity) must
   pass its FMR gate. The Baseline pins 18 addresses, so a platform that computed any of them differently would
   fail here, before an adopter ever sees the binary: cross-platform determinism is *checked per artifact*, not
   assumed from the source. A failed check writes no tarball and no checksum.
3. **The pin lives in the adopter's repository; the release is only where the bytes are.** The adopter's CI
   file names the rev and the SHA-256 of each target's tarball (as the nextest step does). The install step
   downloads, verifies the tarball against that pin, unpacks, and then asserts `req --version` prints
   `built from <REV>` (bare) and exits 0. The `.sha256` sidecar on the release is for the human who pins the
   value the first time — trust on first use of the maintainer's release, stated as such — and is **not** read
   by the install step: a checksum fetched from the place that could be tampered with would prove transfer
   integrity, never authenticity.
4. **The install fails closed, and does not fall back.** Each of these is a non-zero exit with a message naming
   the cause: an unsupported platform (`uname -sm`), a download that is not HTTP 2xx (an unknown rev is a 404;
   a missing or wrong token on the private host is a 401/404), a SHA-256 that differs from the pin, an archive
   whose members are not exactly `req`, a binary that does not run, a `built from` that is not `<REV>`. A failure
   never retries a different path: **no automatic fallback to the source install**, because falling back on a
   mismatch would hide the very event the check exists to surface. The source install is a separate, documented
   path the adopter chooses.
5. **Publishing is a human act, once per rev, never overwriting.** A release is created for a rev only when the
   owner asks (no release on every merge). An asset name that already exists on the release is an error, not a
   replacement: a checksum that has been pinned somewhere must stay true. The Linux tarball is built by a
   dispatch-only workflow on the CI runner; the macOS tarball by the same script on the owner's machine. The
   upload credential is the owner's (and one repository secret for the workflow, the owner's to provision):
   req the engine never signs and never uploads.
6. **Reading a private release needs a token.** The host is private, so the download carries a read token in an
   `Authorization` header. This replaces the SSH deploy key and the toolchain, not the need for a credential;
   the docs say so rather than imply "no secret". `--proto '=https'` is on every `curl`.

## What stays outside req

The `req` binary gains no command, no network client and no flag: this ADR is about how the bytes arrive. The
engine's claim operations stay offline (REQ-0012), the Baseline is still signed by a human with minisign
(ADR 0005, 0010), and no `RULES_V` changes (no fingerprint-path code moves).

## Consequences

- An adopter's CI job drops the compile of req and the SSH key for req's repository; the install needs only `curl`,
  `tar` and a SHA tool. It drops the Rust toolchain only if the project has no other use for it: the template's
  nextest step (`cargo install cargo-nextest`, ADR 0019) still compiles, and moving it to the prebuilt tarball
  req's own CI already uses is a separate change. The measured before/after of req's install is recorded in
  `docs/releasing.md` by the last unit, not asserted here (the cost per job was never measured; issue #65).
- The release process is one more thing that can be wrong, so §2 puts the evidence in the script and §3 puts the
  authority in the adopter's own repository; a compromised release can fail an adopter's CI but cannot make it
  run a different binary silently.
- A prebuilt binary is a second route to a different byte sequence for the same rev (another toolchain patch,
  another libc). We do not claim reproducible builds. The claim is narrower and checkable: this tarball, with this
  hash, reports this rev and verifies the signed Baseline on this platform.
- `scripts/check.sh` and req's own CI keep building from source; nothing about this repository's gate changes.

## Rejected

- **Publish to npm, PyPI or crates.io.** Rejected for #65, not forever: it needs a licence and a visibility
  decision for a private, unlicensed repository, a unique version per publish (req's `0.1.0` is the same on every
  commit by design), and a name (`req` is taken on npm and PyPI). A package wrapper over these same tarballs is the
  natural later step when the TypeScript porcelain of ADR 0009 arrives; it would be its own ADR.
- **Fall back to the source build when the download fails.** See §4.
- **Read the checksum from the release in CI.** See §3.
- **A GitHub-style `latest` URL.** An install that moves with the host is not pinned.
- **One universal install script fetched from req's repository.** The adopter template is copied into the
  adopter's repository and reviewed there; a script fetched at job time is code that changes outside review.
- **musl static binaries now.** It would remove the glibc floor, but it adds a target to the pinned toolchain and a
  different libc under the fingerprint path. If an adopter's glibc is older than the floor, that is a new ADR.
- **Cross-compiling macOS on the Linux runner.** No such toolchain is pinned; the owner's machine is already the
  macOS builder.
