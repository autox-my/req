# 0055. A release carries a semantic version beside its rev

Amends ADR 0025 decision 1 (the release's tag and name) and AGENTS.md's rule that any `Cargo.lock` change is a Tier-1
rules bump (for req's own package line only). The commit stays the identity: `built from`, the pinned rev and the pinned
SHA-256 are unchanged, and nothing here moves an address. No `RULES_V` bump, no claim-schema change, no Baseline envelope
change.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Asked for by the owner on
2026-11-16, after the fourth release (`rev-3294a46`): "`--version` always shows 0.1.0 … we should also use the versioning
like other people." Date proposed: 2026-11-16.

## Context

`req --version` prints `req 0.1.0` on its first line, from `Cargo.toml`'s `version`, and nothing has ever changed it: four
releases (`rev-ec87808`, `rev-6b74f51`, `rev-aeaf987`, `rev-3294a46`) all print 0.1.0. ADR 0025 named releases by commit
because "semver cannot say which commit a binary is", and that is still true: the `built from <sha>` line is what an
adopter's install step verifies. But the number on the first line says something false, that nothing has changed since the
first build, and the release list gives a reader no way to tell a fix from a change that makes them re-sign.

Two facts shape the decision:
- `scripts/release-notes.sh` already computes, per release, **whether identity moved** since the previous one: `RULES_V`,
  the claim schema, the Baseline envelope (ADR 0031). That is exactly the line between "re-sign your Baselines" and "drop
  in the new binary", which is what a version number is for.
- Bumping req's own `version` rewrites one entry of `Cargo.lock`: req's own package line. AGENTS.md treats `Cargo.lock` as a
  fingerprint-path pin, and read literally, every release would then be a Tier-1 rules bump.

## Decision

1. **req has a version, `MAJOR.MINOR.PATCH`, and `Cargo.toml` is its one home.** `req --version` keeps printing it on its
   first line, and keeps printing `built from <sha>`: the version names a release, and the commit stays the identity.
2. **What moves which number.** While the major number is 0:
   - **Minor:** identity moved (`RULES_V`, the claim schema or the Baseline envelope, as `release-notes.sh` computes it);
     or a command, a flag or an exit code was added or changed.
   - **Patch:** anything else, such as a fix, a document or a performance change, with identity unmoved.

   From 1.0 on, an identity move is a **major** bump, because it may make an adopter re-sign. When 1.0 comes is the
   owner's decision, recorded in a later ADR. It is not decided here.
3. **The release is tagged `v<VERSION>`.** The assets keep their names, `req-<REV>-<TARGET>.*`, so the tarball, its
   checksum and its `build-info` still name the commit they were built from (ADR 0025 decision 1). The release body opens
   with the version, the rev and the bump class, and gives the reason ("identity moved: RULES_V 9 → 10").
4. **The workflow refuses a version that does not fit.** It fails closed, before anything is uploaded:
   - when `v<VERSION>` already exists;
   - when `<VERSION>` is not greater than the previous release's;
   - when identity moved but only the patch number rose.

   Like ADR 0025's no-overwrite rule, a release that would mislead is a red run, never a published one.
5. **The version bump is its own pull request.** It changes `Cargo.toml`'s `version` and req's own package entry in
   `Cargo.lock`, and nothing else. **That entry is not a dependency pin**: it names the crate being built, not code
   compiled into it. Changing it is not a Tier-1 rules bump, and AGENTS.md says so in the commit that implements this ADR.
   A test pins the exception: a version-bump commit whose `Cargo.lock` diff touches any other package fails it. The
   exception therefore stays one line wide.
6. **The installer pins the version for the download and the rev and SHA-256 for the proof.** `examples/install-req.sh`
   gains `VERSION` in its pin table, used only to build the download URL (`…/download/v<VERSION>/req-<REV>-<TARGET>.tar.gz`).
   The SHA-256 and `built from <REV>` checks are unchanged, so a moved tag cannot change what is installed.
7. **The existing `rev-*` releases stay as they are.** They are not renamed and not re-tagged, because pins in adopters'
   repositories point at them. The first release under this ADR is **0.2.0**: every earlier binary printed 0.1.0, so 0.1.0
   names all of them and cannot be reused.

## Consequences

- An adopter reads the release list as everyone reads one: a minor bump below 1.0 says "read the notes, you may re-sign",
  and a patch bump says "drop it in".
- The `built from` check is unchanged. The version is a name for a reader, never a proof.
- Each release costs one more small pull request (the bump) before the dispatch.
- The workflow, `tests/release_workflow.rs`, `tests/release_publish.rs`, `scripts/release-notes.sh`,
  `examples/install-req.sh` and its CI snippet (pinned three ways by `tests/ci_template.rs`), `docs/releasing.md`,
  `docs/adopting.md` § *Install req* and AGENTS.md's `Cargo.lock` sentence all change in the commit that implements this.
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
- **Start at 1.0.0.** 1.0 promises that only a major bump moves identity. That is the owner's call, made when the bet
  has more than seed evidence (AGENTS.md, "Do not overclaim").

## Falsification

This decision is wrong if any of these is observed:
1. A release whose version says patch while `release-notes.sh` reports identity moved.
2. A version-bump commit that changes any `Cargo.lock` entry other than req's own and still passes the gate.
3. An install that succeeds although the downloaded binary's `built from` differs from the pinned rev.
