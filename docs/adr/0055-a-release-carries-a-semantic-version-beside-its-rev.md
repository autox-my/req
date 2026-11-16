# 0055. A release carries a semantic version beside its rev

Amends ADR 0025 decision 1 (the release's tag) and decision 5 (a re-dispatch after a failed publish); ADR 0031 decision 2
(how the previous release is found, and what opens the notes); and AGENTS.md's rule that any `Cargo.lock` change is a
Tier-1 rules bump, for req's own package entry only. AGENTS.md says so in this commit. The commit stays the identity:
`built from`, the pinned rev and the pinned SHA-256 are unchanged, and nothing here moves an address. No `RULES_V` bump, no
claim-schema change, no Baseline envelope change.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Asked for by the owner on
2026-11-16, after the fourth release (`rev-3294a46`): "`--version` always shows 0.1.0 … we should also use the versioning
like other people." Date proposed: 2026-11-16; revised the same day after review (PR #96).

## Context

`req --version` prints `req 0.1.0` on its first line, from `Cargo.toml`'s `version`, and nothing has ever changed it. All
four releases (`rev-ec87808`, `rev-6b74f51`, `rev-aeaf987`, `rev-3294a46`) print 0.1.0. ADR 0025 named releases by commit
because "semver cannot say which commit a binary is", and that is still true: the `built from <sha>` line is what an
adopter's install step verifies. But the number on the first line says something false: that nothing has changed since
the first build. And a release's title is a commit hash, so the list of releases does not tell a fix from a change that
asks something of the adopter. Only the body says that.

Two facts shape the decision:
- `scripts/release-notes.sh` already computes, per release, **whether identity moved** since the previous one: `RULES_V`,
  the claim schema, the Baseline envelope (ADR 0031). An identity move is the line between "drop in the new binary" and
  "read the notes: re-sign your Baselines, or migrate your claims". Telling those apart at a glance is what a version
  number is for.
- Bumping req's own `version` rewrites req's own package entry in `Cargo.lock`, and the SBOM `docs/sbom.cdx.json`, which
  records that version and a serial number derived from the lock file (`tools/sbom.py`). AGENTS.md treats `Cargo.lock` as a
  fingerprint-path pin. Read literally, every release would then be a Tier-1 rules bump.

## Decision

1. **req has a version, `MAJOR.MINOR.PATCH`, and `Cargo.toml` is its one home.** `req --version` keeps printing it on its
   first line, and keeps printing `built from <sha>`. The version names a release; the commit stays the identity.
2. **What moves which number.** While the major number is 0:
   - **Minor:** identity moved (`RULES_V`, the claim schema or the Baseline envelope, as `release-notes.sh` computes it);
     or a command, a flag or an exit code was added or changed.
   - **Patch:** anything else, such as a fix, a document or a performance change, with identity unmoved.

   From 1.0 on, an identity move is a **major** bump, because it may ask the adopter to re-sign or migrate. When 1.0 comes
   is the owner's decision, recorded in a later ADR. It is not decided here.
3. **The release is tagged `v<VERSION>`.** The assets keep their names, `req-<REV>-<TARGET>.*`, so the tarball, its checksum
   and its `build-info` still name the commit they were built from (ADR 0025 decision 1). The notes open with the version,
   the rev and the bump class, followed by ADR 0031's sections in its order. The previous release is the nearest tag,
   `v*` or `rev-*`, that is an ancestor of the rev, so the first `v` release compares against `rev-3294a46`.
4. **The workflow checks what history can decide, and the bump pull request declares the rest.** The `build` job, which
   checks out the rev, reads `Cargo.toml` there and fails closed before anything is uploaded:
   - when `<VERSION>` is not greater than the previous release's version (a `rev-*` release counts as 0.1.0);
   - when identity moved but only the patch number rose.

   Whether a command, a flag or an exit code changed cannot be read from history by the workflow. The bump pull request
   says which class it is and why, and the owner who merges it is the check. A minor bump with identity unmoved is
   therefore the owner's declaration, not something the workflow verifies.
5. **A re-dispatch for the same version may finish a release, never replace one.** ADR 0025's rule stands: an existing
   asset name is refused, never replaced. The publish job carries it over to `v<VERSION>`: if the release exists and holds
   none of the six asset names, the run continues, so a publish that died before the upload can be retried. If it holds
   any of them, the run fails, as it does today.
6. **The version bump is its own pull request.** It changes `Cargo.toml`'s `version`, req's own package entry in
   `Cargo.lock` and the regenerated `docs/sbom.cdx.json`, and nothing else. **req's own entry is not a dependency pin.**
   The version is compiled into the binary, since `req --version` prints it. But it reaches no address, no digest, no
   Baseline manifest and no projection header (the address is computed over the statement's canonical form; the Baseline
   envelope carries `rules_v`, the selection and the claims). So changing it is not a Tier-1 rules bump. A test pins the
   exception: a version-bump commit whose `Cargo.lock` diff touches any other package fails it. The exception stays one
   entry wide.
7. **The installer pins the version for the download and the rev and SHA-256 for the proof.** `examples/install-req.sh`
   gains `VERSION` in its pin table, used only to build the download URL (`…/download/v<VERSION>/req-<REV>-<TARGET>.tar.gz`).
   The SHA-256 and `built from <REV>` checks are unchanged, so a moved tag cannot change what is installed.
8. **The existing `rev-*` releases stay as they are.** They are not renamed and not re-tagged, because pins in adopters'
   repositories point at them. The first release under this ADR is **0.2.0**: every earlier binary printed 0.1.0, so 0.1.0
   names all of them and cannot be reused.

## Consequences

- An adopter reads the release list as everyone reads one. A minor bump below 1.0 says "read the notes: you may re-sign
  or migrate". A patch bump says "drop it in".
- The `built from` check is unchanged. The version is a name for a reader, never a proof.
- Each release costs one more small pull request (the bump) before the dispatch.
- The implementing commit changes:
  - `.github/workflows/release.yml`, `scripts/release-notes.sh`, `examples/install-req.sh` and its CI snippet (pinned three
    ways by `tests/ci_template.rs`);
  - the tests `tests/release_workflow.rs`, `tests/release_publish.rs`, `tests/release_notes.rs`, which asserts today that a
    `v`-prefixed tag is not a release, and `tests/install_req.rs`, which pins the `/rev-<REV>/` path and a pin table of
    exactly three keys;
  - `docs/releasing.md` and `docs/adopting.md` § *Install req*;
  - the places that quote `0.1.0`: `README.md` (*Status* and install note), `CONTRIBUTING.md`, `SECURITY.md`,
    `docs/safety-manual.md` and the comment in `build.rs`.
- Adopters pinned to a `rev-*` release keep working. A new pin uses the `v<VERSION>` URL.

## Rejected

- **Keep `rev-<REV>` as the tag and add only a `v<VERSION>` git tag beside it.** The Releases page would still list
  commits, which is the thing the owner asked to change, and two tags per release could disagree.
- **Put the version in the asset names.** The commit in the asset name is what binds a tarball to the `built from` it must
  print (ADR 0025 decisions 1 and 3); a version there would be a second, weaker name.
- **Derive the version from the commit count or the date.** Neither says whether identity moved, which is the one thing an
  adopter needs from the number.
- **Bump `RULES_V` on every release because `Cargo.lock` changed.** It would move every address for a change that touches
  no fingerprint-path code. That is a false identity move, the very noise this ADR exists to remove.
- **Refuse any existing `v<VERSION>` release.** A publish that died before uploading would then need a release deleted by
  hand, or a version known to be wrong.
- **Start at 1.0.0.** 1.0 promises that only a major bump moves identity. That is the owner's call, made when the bet
  has more than seed evidence (AGENTS.md, "Do not overclaim").

## Falsification

This decision is wrong if any of these is observed:
1. A release whose version says patch while `release-notes.sh` reports identity moved.
2. A version-bump commit that changes any `Cargo.lock` entry other than req's own and still passes the gate.
3. An install that succeeds although the downloaded binary's `built from` differs from the pinned rev.
4. A version bump that moves an address, a Baseline manifest or a projection header.
