# Releasing a prebuilt req

The owner's runbook for [ADR 0025](adr/0025-prebuilt-binary-per-pinned-rev.md), amended by
[ADR 0029](adr/0029-req-is-published-on-github-under-apache-2.0.md) §4–§5 and
[ADR 0055](adr/0055-a-release-carries-a-semantic-version-beside-its-rev.md): one release per version, tagged `v<VERSION>`,
for one pinned rev, with a checksummed tarball per supported target, on GitHub (<https://github.com/autox-my/req/releases>). The adopter's
side is `examples/install-req.sh` and [`docs/adopting.md`](adopting.md) § *Install req*; this page is the side that
cuts the release. Nothing here changes req: the binary gains no command and no network client, and nothing is
released until a person asks.

**Status.** Four releases exist: `rev-ec87808`, `rev-6b74f51`, `rev-aeaf987` and `rev-3294a46`.
`.github/workflows/release.yml` was dispatched on 2026-11-09 for `ec87808`
(<https://github.com/autox-my/req/actions/runs/42568766890>) and for `6b74f51`
(<https://github.com/autox-my/req/actions/runs/42622634900>), and on 2026-11-16 for `3294a46`. The
first two each hold the three Linux assets (the tarball's SHA-256 matches the `.sha256` asset); `rev-6b74f51` also holds the three
macOS assets, built with `scripts/build-release.sh` on the owner's machine and attached with the upload script of the macOS
path below (its first use). The **2026-11-16 dispatch of `rev-3294a46` was the first real run of the workflow's own macOS job and
of the generated release notes** (ADRs 0031 and 0032). A **duplicate dispatch for the same rev** the same day exercised the
no-overwrite refusal: the publish job refused, and the release was left unchanged (run 43868208448,
<https://github.com/autox-my/req/actions/runs/43868208448>). A first dispatch can also **sit queued for about half an hour**
before a runner takes it; wait, and do not dispatch again. `examples/install-req.sh` was run against `rev-ec87808` on Ubuntu 22.04, 24.04
and latest runners (the floor note below) and against `rev-6b74f51`'s macOS tarball on one macOS arm64 machine.

All four releases are tagged `rev-<REV>`. ADR 0055 (accepted 2026-11-16) changes the next one: it is tagged `v<VERSION>`, with
the version read from the rev's `Cargo.toml`, and the workflow refuses a version that does not follow from the previous
release. **That path has been run only against a fake `gh` and fixtures** (`tests/release_publish.rs`,
`tests/release_version.rs`), never against GitHub: no `v<VERSION>` release exists yet, and the pull request that
implements it does not bump the version, so a dispatch at that commit fails closed at the version check (0.1.0 is not greater than
the 0.1.0 that `rev-3294a46` counts as). The one call that touches GitHub was tried read-only on 2026-11-17: `gh api repos/autox-my/req/commits/<tag> --jq .sha`
answered the commit of an existing tag (`rev-3294a46`) and HTTP 422 "No commit found for SHA" for an absent one, which is what the
publish job's lookup expects. Not yet exercised on GitHub: a macOS binary on a second machine and finishing a release whose
publish died before the upload. Intel Mac and Windows
are not built; a Windows adopter runs the Linux tarball under WSL2 (`docs/adopting.md`, *Windows*). `tests/release_workflow.rs` pins the workflow's text and shape (the trigger, the
permissions, the no-overwrite rule). The workflow fails closed, so a misconfiguration is a red run, never a
half-published release you cannot tell from a good one.

## When to cut a release

On request, for a rev an adopter is going to pin. Not on every merge: a release is a promise that its
checksums stay true, so there is no reason to make more of them than there are pins. The rev must be on
`main`. `release.yml` refuses a rev that is not an ancestor of `origin/main`, because the Baseline proof
inside the build script uses the trust anchor of the commit it builds (ADR 0010), and a commit off `main`
could carry its own key.

## Bump the version

A release carries the version its rev declares in `Cargo.toml` (ADR 0055), so the version changes in a pull request of its own,
merged **before** the dispatch. That pull request changes `Cargo.toml`'s `version`, req's own package entry in `Cargo.lock`
and the regenerated `docs/sbom.cdx.json`, and nothing else; no prose document quotes the number, so none needs editing.

1. **Pick the class** (ADR 0055 decision 2). While the major number is 0, a **minor** bump when identity moved (`RULES_V`,
   the claim schema or the Baseline envelope) or a command, a flag or an exit code was added or changed; a **patch** bump
   for anything else. A major bump below 1.0 is refused by the workflow until the ADR that decides 1.0 exists. The class is
   your declaration for a changed command, flag or exit code: history cannot show that, so the pull request body says which
   class and why, and the owner who merges it is the check.
2. **Ask the script what it will say.** In a clone with the tags fetched (`git fetch --tags`), on the rev you mean to release:

   ```bash
   d=$(mktemp -d)        # the script never overwrites, so each run gets a fresh directory
   bash scripts/release-notes.sh --rev <rev> --repo autox-my/req --out "$d/notes.md" --facts "$d/facts"
   cat "$d/facts"        # previous_tag, previous_version, identity (moved | unmoved | unknown)
   bash scripts/release-version-check.sh check --version <NEW> --facts "$d/facts"
   ```

   Exit 0 prints `version=` and `bump=`; exit 1 prints the refusal the workflow would. A `rev-*` release counts as 0.1.0, and a
   `v*` release's version is read from its own `Cargo.toml`. Identity `unknown` (a value that could not be read) never lets a
   patch bump through.
3. **Make the change,** edit `Cargo.toml`, run `cargo build` so `Cargo.lock` follows, then `python3 tools/sbom.py --out
   docs/sbom.cdx.json`. Before you push, check the lock exception (ADR 0055 decision 6): a version bump touches req's own
   entry and no other, so it is not a rules bump.

   ```bash
   git show origin/main:Cargo.lock > /tmp/main.lock
   bash scripts/release-version-check.sh lock-exception /tmp/main.lock Cargo.lock   # exit 0, or the entry that differs
   ```

4. Run `scripts/check.sh` in full, open the pull request, and merge it. The rev to release is then on `main` with that version.

## Dispatch `release.yml`: Linux and macOS together

1. Nothing to provision. The workflow publishes with its own `GITHUB_TOKEN`; no repository secret exists or is
   read. Actions must be enabled for the repository.
2. Actions, *Release*, *Run workflow*, **from `main`**, input `rev` = the commit (a SHA or a ref on `main`).
   The workflow and `scripts/build-release.sh` come from the dispatched ref; the rev is built from a clean
   copy of itself.
3. The **build** job (`contents: read`, it runs the rev's code) first writes the release notes and, beside them, the facts of the
   previous release, then **reads the version from the rev with `git show <rev>:Cargo.toml`** (it never checks the rev out)
   and judges it with `scripts/release-version-check.sh`: it fails, before anything is built or uploaded and naming the
   reason, when the version is not greater than the previous release's, when identity moved and the required number did
   not rise, when identity is `unknown` and only the patch rose, or for a major bump below 1.0. Then it checks the runner (C linker, `minisign`,
   `rustup` with the toolchain the rev pins), runs `scripts/build-release.sh --rev <full sha> --out …`, then
   `scripts/reproduce.sh --rev <full sha> --against <the tarball it just wrote>` (ADR 0035: a binary the runner cannot
   derive again from the rev fails the job, and nothing is published), and hands the three assets to the next job as an
   artifact. The **build-macos** job (`contents: read`, a `macos-15`
   Apple-silicon runner) takes the SHA the Linux job resolved, re-checks that it is on `main`, and runs the same
   script for `aarch64-apple-darwin`; its three assets travel as a second artifact (ADR 0032).
4. The **publish** job (`contents: write`, the only job that holds the token, and it checks out and runs none
   of the rev's code) re-checks the assets of **both** targets and the version the build job handed over, and binds the tag
   `v<VERSION>` to the rev on every path: if that tag exists it must point at the rev being published (its commit, read with
   `gh api`), or the run fails. An existing release is finished only if it is not a draft and holds none of the six asset
   names (a publish that died before the upload can be retried); a draft, or an asset already there, fails the run. A new
   release is created at the rev. Then it refuses unless all six names are free and uploads, Linux then macOS, each target's
   `….build-info`, `….tar.gz.sha256` and `….tar.gz`, in that order, with `gh release upload`. A release has both targets or none.
   The asset names keep the rev (`req-<REV>-<TARGET>.*`): the commit stays the identity, the version is a name.
   The release body is the notes `scripts/release-notes.sh` wrote in the build job (ADR 0031): the version, the rev and the
   bump class, the previous release (the nearest `rev-*` or `v*` tag strictly before the rev; a `v` tag that is not a plain
   `v<X.Y.Z>` blocks the dispatch until it is deleted or renamed) and a
   compare link, **whether identity moved** (`RULES_V`, the claim schema, the Baseline envelope, and what an adopter does if
   one did) and the pull requests merged since, grouped by prefix. A release that already exists keeps its body: the
   workflow never edits one. To preview the notes before you dispatch, run
   `bash scripts/release-notes.sh --rev <rev> --repo autox-my/req --out "$(mktemp -d)/notes.md"` in a clone with the tags
   fetched.
5. The last lines of the log are `released v<VERSION>` and one `pin (<target>): VERSION=… REV=… SHA256=… TARGET=…` line per
   target. Use those: the build job's log also carries the script's own `pin:` line, printed before the upload, which
   after a half-failed upload names a hash that was never published.

**A second dispatch for the same version.** It may finish a release, never replace one (ADR 0055 decision 5). If the first
publish died before the upload (the release exists, is not a draft and holds none of the six names), dispatch again for the
same rev: the tag already points at it and the assets are uploaded. If the first run uploaded anything, or the release is a
draft, or the tag points at another commit, the second run fails and says which; that refusal is the point (the duplicate
dispatch of `rev-3294a46` was refused this way, with the release unchanged). Never move a tag by hand to make a run pass.

The script refuses to write anything unless the binary says `built from` the bare rev, `req baseline
--verify baselines/air-gapped.json` prints `[VERIFIED]`, `req bench` passes its FMR gate and `req selftest` passes (exit 1 names
the check; exit 3 is `minisign` missing, which is UNVERIFIABLE and blocks). A failed proof leaves nothing
published. If a run dies **during the upload**, before the tarball is there, nothing is pinnable yet: open
the release, delete the leftover `.build-info`/`.sha256` assets of that rev, and dispatch again. (They are
deleted by hand on purpose: the workflow has no way to delete or replace an asset, and an asset name that
already exists on the release is an error, never a replacement.) A release whose tarball is present is
complete.
A `DIFFERS` from the reproduction step (ADR 0035) also fails the job and publishes nothing: read its output, which names
both hashes and how many bytes differ. A handful of bytes is a stamp (on macOS the linker's UUID); thousands is a different
build. Either way the rev is not released from that run; do not work around it by editing the workflow.

**The record in `.build-info` (ADR 0037).** Besides the toolchain lines, each release's `.build-info` has `record=req.release-record/1`, the SHA-256 of
the binary (`binary_sha256`), `rules_v`, one `proof_…=pass` line per proof the script ran (built-from, baseline-verify, bench FMR gate, selftest)
and the Baseline, corpus and selftest lines they printed, ending in an `attestation=self…` line. It is a record of what the runner did, bound to
the rev and the bytes; it is not signed and not independent. The script now also refuses to release a binary whose `req selftest` fails
(exit 1 naming `selftest`). Releases made before ADR 0037 have the shorter file.

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
line. Attach the three files to the release with the `gh` CLI (logged in as a person who may write to
the repository), **refusing a name that already exists**, tarball last. The release is `v<VERSION>` for a version tag
and `rev-<REV>` for the four releases made before ADR 0055; set `TAG` to its name:

```bash
set -euo pipefail   # save this as a file and run it with `bash`: a failed upload must stop it, not scroll past
REPO=autox-my/req
REV=<REV>; FULL=<full sha>; OUT=<the --out directory>; T=aarch64-apple-darwin
TAG=<v<VERSION>, or rev-<REV> for a release made before ADR 0055>

# The release for this rev: reuse it if the Linux dispatch made it, create it otherwise.
gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1 ||
  gh release create "$TAG" --repo "$REPO" --target "$FULL" --title "$TAG" \
    --notes "Prebuilt req for the pinned rev $REV (ADR 0025). Pin the .sha256 of the tarball in your own repository."

# Refuse a name that is already there, for all three, before uploading any.
gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name' >"$OUT/.listed"
for n in req-$REV-$T.build-info req-$REV-$T.tar.gz.sha256 req-$REV-$T.tar.gz; do
  ! grep -Fxq -- "$n" "$OUT/.listed" || { echo "$n already exists: refusing to replace it" >&2; exit 1; }
done
for n in req-$REV-$T.build-info req-$REV-$T.tar.gz.sha256 req-$REV-$T.tar.gz; do
  gh release upload "$TAG" --repo "$REPO" "$OUT/$n" || { echo "uploading $n failed: the release is incomplete" >&2; exit 1; }
  echo "uploaded $n"
done
```

Dispatch the Linux workflow first when you can: the release it creates carries the generated notes (ADR 0031), whereas
the `gh release create` line above, used when no release exists yet, gives a one-line body, and nothing rewrites a body later.

There is no `--clobber` here, on purpose. If the Linux release for the same rev exists already, the release is
simply reused, and the macOS names differ from the Linux ones, so the two paths never contend for a name.

## What to pin in an adopter afterwards

The `pin (<target>):` line the workflow prints: `VERSION=<VERSION> REV=<REV> SHA256=<hex> TARGET=<target>` (the script's
own `pin:` line, which the macOS path prints, has no `VERSION`: it does not know the tag). In the adopter's copy of
`examples/install-req.sh`, `VERSION=` takes the version (it builds the download URL, `…/download/v<VERSION>/…`, and nothing
else), `REV=` takes the rev and `SHA256_<target with underscores>=` takes the hash for that target. A release made before
ADR 0055 is `rev-<REV>`, and its installer is the one at that rev, which has no `VERSION` line. That first copy is trust on first use of the maintainer's release; from then on the pin in the
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
- **Re-point or re-use a version tag.** `v<VERSION>` names one commit's tarballs for good; a version that was released is
  never released again with other bytes (the workflow fails if the tag points anywhere but the rev). A wrong release is
  superseded by a higher version.
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
