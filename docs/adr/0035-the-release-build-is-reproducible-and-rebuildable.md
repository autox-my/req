# 0035. The release build is reproducible on one builder, and anyone can rebuild a release and compare

Amends ADR 0025 (*Consequences*: "We do not claim reproducible builds").

**Status: proposed** — the owner ratifies it by merging the pull request that carries it; asked for by the owner on 2026-11-10
(item 5 of the qualification-readiness package; no issue). Date proposed: 2026-11-10. Changes how a release is built, not what it
computes: no `RULES_V` change, no address moves, no change to the engine.

## Context

ADR 0025 ships a prebuilt binary and says, honestly, that two builds of one rev can differ: the pinned checksum is the authority, and a
release is "verified by its checksum, never by rebuilding it" (`docs/safety-manual.md`). That held for `rev-ec87808` and `rev-6b74f51`: two
builds of one rev gave different hashes.

Measured on 2026-11-10 (macOS arm64, rev `01ff401`, two clean builds in different temporary directories): the **binary was already
byte-identical**; the **archive was not**. The difference was in the tarball's member header (modification time, owner and group of the
builder, and on macOS a pax extended header with a sub-second clock). A second source would appear on another machine: a dependency's
source path, which sits under the builder's `CARGO_HOME`, is written into the binary by panic messages (`/Users/…/.cargo/registry/src/…`
here, `/home/runner/.cargo/…` on the CI runner). Both are properties of how the release is built, not of the code.

For an adopter who must justify a tool, "I can derive the binary from the source and get the same bytes" is a stronger statement than
"I trust the checksum the maintainer published", and it costs little here: six direct dependencies, one binary, a pinned toolchain.

## Decision

1. **The archive carries nothing of the builder.** `scripts/build-release.sh` writes the tarball with owner `0:0` and no names, mode 0755,
   format `ustar` (no per-run extended header), and the member's modification time set to **the commit's own time** (`git log -1 --format=%ct`,
   the convention of `SOURCE_DATE_EPOCH`), never the builder's clock. `gzip -n` already wrote no name and no time. One binary at one rev is one
   tarball, whoever packs it and whenever.
2. **The binary carries no path of the builder.** The build runs with `--remap-path-prefix` for the clean copy (to `/req`) and for the
   builder's `CARGO_HOME` (to `/cargo`), passed as `CARGO_ENCODED_RUSTFLAGS` so a path with a space survives. These are the only flags; they
   change what the binary *says about where it was built*, not what it computes. An exported `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`,
   `CARGO_BUILD_RUSTFLAGS` or `CARGO_BUILD_TARGET` still cannot reach the build. `SOURCE_DATE_EPOCH` is set to the commit time for the build.
3. **`scripts/reproduce.sh`**, a check and an instrument, in two modes.
   - `--rev R` builds `R` twice with `build-release.sh`, in different temporary directories (different path and length) and two seconds apart,
     and requires the two tarballs to be byte-identical. It proves the build depends on the rev and the pinned toolchain, not on where or when it
     ran. It does **not** prove another machine gets the same bytes.
   - `--rev R --against <release tarball>` builds `R` once and compares the **binary inside** (`req`, by SHA-256) with the one in the given
     tarball. This is what an adopter or an assessor runs. The archive's hash is reported and does not decide, because the archive also depends on the
     `gzip` of whoever packed it; the binary's bytes do not.
   - Verdicts are `REPRODUCED` (exit 0) and `DIFFERS` (exit 1, with both hashes and, for `--against`, this machine's toolchain line so the
     reader compares it with the release's `build-info`). No `minisign` is UNVERIFIABLE (exit 3), as for `build-release.sh`, because the
     build's own proofs could not run.
4. **The gate runs the first mode on every full run** (`scripts/check.sh`, after the Tier-2 crate; `--fast` prints a `FAST` line, no
   `minisign` is a SKIP and the Baseline step fails the gate on its own). It builds `HEAD`, the last commit: before a commit it checks the
   release process and the pinned toolchain; in CI, where `HEAD` is the merge, it checks the change itself. Cost: about 30 seconds on an
   Apple-silicon laptop.
5. **What is and is not claimed.** *Claimed and checked every run:* on one builder, two builds of a rev give identical bytes. *Claimed, to be
   checked by whoever cares:* a rebuild on another machine of the **same target** with the pinned toolchain gives the same binary; run
   `reproduce.sh --against`. *Not claimed:* the same bytes across targets (a different compiler backend target is a different binary), **on macOS with the pinned toolchain
   installed at another path** (measured on 2026-11-10: the same rev built with the same toolchain version from two install locations differs in
   48 bytes, the Mach-O `LC_UUID` and the code-signature hash that covers it; everything else is identical, and `reproduce.sh` reports the count of
   differing bytes so that case is recognisable), or on a Linux
   builder whose linker or C library differ from the release's (a dynamically linked binary records what it was linked with; `build-info`
   names the builder's toolchain and libc, and a `DIFFERS` is reported, never forced to a pass). Releases made before this ADR
   (`rev-ec87808`, `rev-6b74f51`) were built the old way and are **not** reproducible; nothing is rewritten, because a pinned checksum must
   stay true (ADR 0025).

## Consequences

- A release's checksum changes for a given rev compared with what the old script would have produced; that is a property of the new script,
  and it applies only to releases made after it merges. No existing pin moves.
- The new gate step is two clean release builds: a full run is about thirty seconds longer, and CI longer on a cold runner. That is the price of
  the claim being re-checked on every change rather than asserted once.
- `reproduce.sh --against` is the first piece of evidence an assessor can produce without trusting the maintainer. It is **evidence of the
  build process, not of the correctness of the rules**: a reproducible binary that canonicalizes wrongly reproduces perfectly. The correctness
  evidence is `req selftest`, the corpora and the signed Baseline (ADR 0034, ADR 0010).
- The release workflow runs `reproduce.sh --against` on each runner, after `build-release.sh` and before the hand-over to the publishing job
  (added after the owner asked for it, in a follow-up to the first change). It compares the tarball about to be uploaded with one more build
  of the same rev made in another directory and at another time, on the same runner. A `DIFFERS` fails the job and nothing is published. It
  adds one build per target to a dispatch (a few minutes) and adds no trust: it is the same script the gate runs on every pull request, and
  it cannot show that another machine would get the same bytes. The first dispatch after it merges is its first run on a macOS runner, so a
  red run there is read as information about the build (the `DIFFERS` output counts the differing bytes) before it is read as a workflow fault.

## Rejected

- **Bit-for-bit across targets or toolchains.** Not possible and not wanted: the target and the toolchain are inputs.
- **A hermetic build environment (Nix, Bazel, a container).** Would make the Linux claim stronger. It adds a second toolchain and a second
  thing to trust to a project whose appeal is that `cargo build --locked` is the whole story; the cost was not argued for by any adopter.
- **Dropping `gzip` for a tarball hash that does not depend on it.** The adopter's pin is the tarball's hash (ADR 0025); changing the asset format
  would move every pin. The binary hash decides in `--against` instead.
