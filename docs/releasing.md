# Releasing a prebuilt req

The owner's runbook for [ADR 0025](adr/0025-prebuilt-binary-per-pinned-rev.md), amended by
[ADR 0029](adr/0029-req-is-published-on-github-under-apache-2.0.md) §4–§5: one release per pinned rev, with a
checksummed tarball per supported target, on GitHub (<https://github.com/autox-my/req/releases>). The adopter's
side is `examples/install-req.sh` and [`docs/adopting.md`](adopting.md) § *Install req*; this page is the side that
cuts the release. Nothing here changes req: the binary gains no command and no network client, and nothing is
released until a person asks.

**Status.** `.github/workflows/release.yml` has been dispatched twice, on 2026-11-09: for `ec87808`
(<https://github.com/autox-my/req/actions/runs/42568766890>) and for `6b74f51`
(<https://github.com/autox-my/req/actions/runs/42622634900>). Both runs succeeded, and each release holds the three
Linux assets (the tarball's SHA-256 matches the `.sha256` asset). `rev-6b74f51` also holds the three macOS assets, built
with `scripts/build-release.sh` on the owner's machine and attached with the upload script of the macOS path below (its
first use). `examples/install-req.sh` was run against `rev-ec87808` on Ubuntu 22.04, 24.04 and latest runners (the floor
note below) and against `rev-6b74f51`'s macOS tarball on one macOS arm64 machine. Not yet exercised: the
no-overwrite refusal (a second dispatch for the same rev), a macOS binary on a second machine, and the workflow's own
macOS job and release notes (ADRs 0031 and 0032: their first real run is the next dispatch, which is also why the
publishing script is run against a fake `gh` in `tests/release_publish.rs`). Intel Mac and Windows
are not built; a Windows adopter runs the Linux tarball under WSL2 (`docs/adopting.md`, *Windows*). `tests/release_workflow.rs` pins the workflow's text and shape (the trigger, the
permissions, the no-overwrite rule). The workflow fails closed, so a misconfiguration is a red run, never a
half-published release you cannot tell from a good one.

## When to cut a release

On request, for a rev an adopter is going to pin. Not on every merge: a release is a promise that its
checksums stay true, so there is no reason to make more of them than there are pins. The rev must be on
`main`. `release.yml` refuses a rev that is not an ancestor of `origin/main`, because the Baseline proof
inside the build script uses the trust anchor of the commit it builds (ADR 0010), and a commit off `main`
could carry its own key.

## Dispatch `release.yml`: Linux and macOS together

1. Nothing to provision. The workflow publishes with its own `GITHUB_TOKEN`; no repository secret exists or is
   read. Actions must be enabled for the repository.
2. Actions, *Release*, *Run workflow*, **from `main`**, input `rev` = the commit (a SHA or a ref on `main`).
   The workflow and `scripts/build-release.sh` come from the dispatched ref; the rev is built from a clean
   copy of itself.
3. The **build** job (`contents: read`, it runs the rev's code) checks the runner (C linker, `minisign`,
   `rustup` with the toolchain the rev pins), runs `scripts/build-release.sh --rev <full sha> --out …` and hands
   the three assets to the next job as an artifact. The **build-macos** job (`contents: read`, a `macos-14`
   Apple-silicon runner) takes the SHA the Linux job resolved, re-checks that it is on `main`, and runs the same
   script for `aarch64-apple-darwin`; its three assets travel as a second artifact (ADR 0032).
4. The **publish** job (`contents: write`, the only job that holds the token, and it checks out and runs none
   of the rev's code) re-checks the assets of **both** targets, refuses unless all six names are free, creates the
   release `rev-<REV>` if it is absent and uploads, Linux then macOS, each target's `….build-info`, `….tar.gz.sha256`
   and `….tar.gz`, in that order, with `gh release upload`. A release has both targets or none.
   The release body is the notes `scripts/release-notes.sh` wrote in the build job (ADR 0031): the previous release and a
   compare link, **whether identity moved** (`RULES_V`, the claim schema, the Baseline envelope, and what an adopter does if
   one did) and the pull requests merged since, grouped by prefix. A release that already exists keeps its body: the
   workflow never edits one. To preview the notes before you dispatch, run
   `bash scripts/release-notes.sh --rev <rev> --repo autox-my/req --out /tmp/notes.md` in a clone with the tags fetched.
5. The last lines of the log are `released rev-<REV>` and one `pin (<target>): REV=… SHA256=… TARGET=…` line per
   target. Use those: the build job's log also carries the script's own `pin:` line, printed before the upload, which
   after a half-failed upload names a hash that was never published.

The script refuses to write anything unless the binary says `built from` the bare rev, `req baseline
--verify baselines/air-gapped.json` prints `[VERIFIED]` and `req bench` passes its FMR gate (exit 1 names
the check; exit 3 is `minisign` missing, which is UNVERIFIABLE and blocks). A failed proof leaves nothing
published. If a run dies **during the upload**, before the tarball is there, nothing is pinnable yet: open
the release, delete the leftover `.build-info`/`.sha256` assets of that rev, and dispatch again. (They are
deleted by hand on purpose: the workflow has no way to delete or replace an asset, and an asset name that
already exists on the release is an error, never a replacement.) A release whose tarball is present is
complete.

**The Linux floor is the runner's glibc, or lower.** `build-info` records the builder's libc line (`ldd --version`).
For `rev-ec87808` (built on glibc 2.43) the installed binary ran on Ubuntu 24.04 (2.39) cleanly and on 22.04
(2.35) with a loader warning on stderr (`weak version GLIBC_2.39 not found`); only `--version`, `req init` and
`req canon` were run there, and nothing older than 2.35 was tried. Read the `.build-info` line, and test a
distribution, before telling an adopter it is supported; a lower floor is a new ADR (musl was rejected in ADR 0025), not a flag.

## Rebuild a release and compare (ADR 0035)

A release made after ADR 0035 can be derived from its source by anyone with the repository, the pinned toolchain
(`rust-toolchain.toml`) and `minisign` (the build runs the same proofs as a release). From a checkout of the repository:

```bash
scripts/reproduce.sh --rev <rev> --against req-<REV>-<TARGET>.tar.gz
```

It builds the rev in a clean copy, unpacks the given tarball and compares the **binary** by SHA-256. `REPRODUCED` means the
bytes are the ones this rev builds. `DIFFERS` prints both hashes and this machine's toolchain line: compare it with the
release's `.build-info` (`rustc`, `cargo`, `libc`) before concluding anything, because the target, the toolchain and, on Linux, the
linker and C library are inputs, and on macOS so is the path the toolchain is installed at: it reaches the binary's `LC_UUID`,
so a rebuild with the same toolchain elsewhere differs by 48 bytes (the line `bytes differ in the common prefix` tells that case from a different build). A `DIFFERS` is a finding to report, not a failure to explain away. Releases made before ADR 0035
(`rev-ec87808`, `rev-6b74f51`) were built the old way and will not reproduce.

`scripts/reproduce.sh --rev <rev>` with no `--against` builds twice, in different directories and two seconds apart, and requires
the two tarballs to be byte-identical; `scripts/check.sh` runs it on `HEAD` in every full run.

## The macOS path by hand (a fallback, ADR 0032)

Since ADR 0032 the workflow builds the macOS tarball too. This path remains for two cases: a rev released before that
(`rev-ec87808` has no macOS asset at all; `rev-6b74f51`'s was made this way), and a publish that died after the Linux
tarball but before the macOS assets were there (the workflow then refuses a second dispatch, because the Linux names are taken).

```bash
scripts/build-release.sh --rev <rev> --out "$(mktemp -d)"
```

It builds in a clean copy, proves the binary exactly as above, writes
`req-<REV>-aarch64-apple-darwin.{tar.gz,tar.gz.sha256,build-info}` into the directory and prints the `pin:`
line. Attach the three files to the release `rev-<REV>` with the `gh` CLI (logged in as a person who may write to
the repository), **refusing a name that already exists**, tarball last:

```bash
set -euo pipefail   # save this as a file and run it with `bash`: a failed upload must stop it, not scroll past
REPO=autox-my/req
REV=<REV>; FULL=<full sha>; OUT=<the --out directory>; T=aarch64-apple-darwin

# The release for this rev: reuse it if the Linux dispatch made it, create it otherwise.
gh release view "rev-$REV" --repo "$REPO" >/dev/null 2>&1 ||
  gh release create "rev-$REV" --repo "$REPO" --target "$FULL" --title "rev-$REV" \
    --notes "Prebuilt req for the pinned rev $REV (ADR 0025). Pin the .sha256 of the tarball in your own repository."

# Refuse a name that is already there, for all three, before uploading any.
gh release view "rev-$REV" --repo "$REPO" --json assets --jq '.assets[].name' >"$OUT/.listed"
for n in req-$REV-$T.build-info req-$REV-$T.tar.gz.sha256 req-$REV-$T.tar.gz; do
  ! grep -Fxq -- "$n" "$OUT/.listed" || { echo "$n already exists: refusing to replace it" >&2; exit 1; }
done
for n in req-$REV-$T.build-info req-$REV-$T.tar.gz.sha256 req-$REV-$T.tar.gz; do
  gh release upload "rev-$REV" --repo "$REPO" "$OUT/$n" || { echo "uploading $n failed: the release is incomplete" >&2; exit 1; }
  echo "uploaded $n"
done
```

Dispatch the Linux workflow first when you can: the release it creates carries the generated notes (ADR 0031), whereas
the `gh release create` line above, used when no release exists yet, gives a one-line body, and nothing rewrites a body later.

There is no `--clobber` here, on purpose. If the Linux release for the same rev exists already, the release is
simply reused, and the macOS names differ from the Linux ones, so the two paths never contend for a name.

## What to pin in an adopter afterwards

The `pin:` line each path prints: `REV=<REV> SHA256=<hex> TARGET=<target>`. In the adopter's copy of
`examples/install-req.sh`, `REV=` takes the rev and `SHA256_<target with underscores>=` takes the hash for
that target. That first copy is trust on first use of the maintainer's release; from then on the pin in the
adopter's repository is the authority, and the install never reads the release's own `.sha256` asset. Run the
adopter's job once and check that `req --version` reports `built from <REV>`.

The release is public, so the install sends no credential. A private fork or mirror of the release sets
`REQ_RELEASE_TOKEN` (a token that can read it) in the adopter's CI secrets; the `Authorization` header is sent
only when that variable is set and non-empty.

## What a release must never do

- **Replace, edit or delete an asset.** A checksum that has been pinned somewhere must stay true. A wrong
  release is superseded by a new rev, not repaired. (The workflow uses no `--clobber`, no `gh release delete`
  and no `gh release edit`, and `tests/release_workflow.rs` fails if one appears.)
- **Release a `-dirty`, `unknown` or `-cleanliness-unknown` binary,** or one that does not print `built from`
  followed by the rev it is named for. The script refuses; do not route around it with a hand-made tarball.
- **Release on every merge,** or from a trigger other than a person's dispatch.
- **Give the job that runs the rev's code a write token.** `contents: write` belongs to the publishing job
  alone; the build job runs the rev's own code and holds `contents: read`.
- **Print the token.** No step traces (`set -x`), and the token reaches `gh` through its environment or not at all.
- **Fall back.** If a build, a proof or an upload fails, nothing else is substituted: the failure is the answer.

## First run on GitHub

What the tests cannot show, in the order to do it (the same list as `docs/specs/138-github-publication.md`):

1. Push `main` (and the `rev-a8fcd2a` tag only if wanted). **Never** `git push --mirror` or `--all`: they would
   publish the `refs/t3/…` checkpoint refs, which carry local paths and a private network address.
2. Settings: Actions enabled; branch protection on `main` requiring the `gate` check, with no force-push;
   *Keep my email addresses private* and *Block command line pushes that expose my email* on. Consider
   *Require approval for first-time contributors* under Actions, so a stranger's pull request does not start a
   run unseen.
3. Run `CI` by `workflow_dispatch`; read the log: the toolchain install, the nextest download and its SHA-256, the
   gate, no `SKIP` line.
4. Run `Release` for a rev from `main`; install from the result with the template's step on a scratch repository.
5. Update `disktriage`'s README (it says req is not publicly available yet) and its install instructions.

There is no automated review workflow in this repository: a human reads every pull request (ADR 0029 §7).

## Install timing

`install-timing` is no longer a workflow. If the owner wants the figure, measure it once by hand on a scratch
Linux machine with an empty `CARGO_HOME`, and write it here with the date and the rev:

```bash
# the source install, then the prebuilt one (examples/install-req.sh, pins filled in); each timed alone
time cargo install --git https://github.com/autox-my/req.git --rev <REV> --root "$(mktemp -d)" --locked req
time bash install-req.sh
```

The one figure on record is from the earlier, private setup: about 36 s for the source install and under a
second for the prebuilt one, on a LAN runner, one run. Read it with its limits:

- **The compile is cheap.** req has six direct dependencies, so a cold source install (empty `CARGO_HOME`,
  registry index and crates fetched, then compiled) took about half a minute. Unlike nextest's eight minutes
  (ADR 0019), the saving from the prebuilt binary is **not mainly the compile**: it is the Rust toolchain a
  project would otherwise need only for req.
- **The toolchain is outside both figures.** A runner with no Rust pays `rustup toolchain install` on top of the
  source figure; it was not measured.
- **One run.** Not an average; no variance is claimed, and a public download over the internet is not a LAN hop.

## Tokens

| where | what | for |
| --- | --- | --- |
| the workflow's own `GITHUB_TOKEN` | `contents: write`, on the publishing job only | `release.yml` creating a release and uploading its assets; minted per run, nothing to provision |
| the owner's `gh` login | write access to the repository | the macOS path above |
| an adopter's CI secret `REQ_RELEASE_TOKEN` | read access to a **private** fork's release | `examples/install-req.sh`; unused for the public release |

No long-lived credential is stored in the repository's settings for the release.
