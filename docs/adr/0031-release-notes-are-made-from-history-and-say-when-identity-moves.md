# 0031. Release notes are made from history and say when identity moves

**Status: proposed** — the owner ratifies it by merging the pull request that carries it; asked for by the owner on
2026-11-09 (no issue). Date proposed: 2026-11-09. **Amends [ADR 0025](0025-prebuilt-binary-per-pinned-rev.md) §5** (what
the publishing step writes into a release); nothing else in 0025 changes: the assets, their names, the no-overwrite rule and the
split of permissions between the two jobs are exactly as before.

## Context

A release of req is a pin: an adopter copies a rev and a SHA-256 into their own repository and rebuilds trust from there.
Through `rev-6b74f51` every release carried the same one-line body ("Prebuilt req for the pinned rev … pin the .sha256").
It said nothing about what had changed since the previous release, and that is the question an adopter asks before moving a
pin. For req it has a sharp edge: some changes move **identity**. A bump of `RULES_V` re-derives addresses and invalidates a
signed Baseline until the owner re-signs it (that is what the `IN` operator, ADR 0030, did between `rev-ec87808` and
`rev-6b74f51`); a new claim schema stops older claim files being judged; a new Baseline envelope changes what an old manifest
still verifies. A changelog that lists pull-request titles and buries those three facts has failed the one reader it exists for.

## Decision

1. **Every release gets generated notes**, written by `scripts/release-notes.sh` in the workflow's **build** job (the job that
   holds `contents: read` and no token), from git history alone: no network, no model, no build. It runs before the build
   script, so notes that cannot be written fail the release before anything is built.
2. **They say, in this order:** the previous release (the nearest `rev-*` tag that is an ancestor of the rev, and a compare link;
   or "first release"); whether **identity** moved, by reading `RULES_V` and `CLAIM_SCHEMA_V` (`src/canon.rs`) and
   `BASELINE_VERSION` (`src/baseline.rs`) at both commits; and the pull requests merged since, newest first, grouped by their
   conventional-commit prefix (features, fixes, documentation, other), with the number.
3. **A change in any of the three is named and says what the adopter does.** `rules_v 6 → 7 — changed: re-emit and re-sign your
   Baseline with a req built from this rev; `req baseline --verify` is UNVERIFIABLE until you do`. A value that cannot be read at
   either commit is **"could not be determined"** and the notes send the reader to the compare link; it is never reported as
   unchanged. "No identity change" appears only when all three were read and are equal.
4. **Every title is hostile text.** The notes become the body of a public release, and a title is whatever the merged pull request
   was called. Control bytes and `<` `>` are removed, `@` and `://` and `](` are defused so a title cannot notify a person or
   become a live link, and a title is cut at 200 bytes. This is not a security boundary against the owner (who merges every
   pull request, ADR 0029 §7) but a refusal to let a title do more than be read.
5. **The publishing job only reads a file.** The notes travel as a second artifact (`release-notes`), apart from the assets whose
   names and hashes the publishing job re-checks. The job refuses to publish without a non-empty notes file under 60 000 bytes,
   passes it to `gh release create --notes-file`, runs no script and still **never edits a release**: a release that already
   exists keeps the body it has, so a re-dispatch cannot rewrite what an adopter has read.
6. **Not a `CHANGELOG.md`, and not `--generate-notes`.** The notes are an artifact of a release, tested by
   `tests/release_notes.rs`; `tests/release_workflow.rs` pins the wiring and forbids the generator.

## Consequences

- `rev-6b74f51` keeps its old one-line body. The owner may add notes to it by hand; the workflow will not.
- A rev whose predecessor tag cannot be found, or whose three constants moved file, is reported honestly rather than guessed; a
  future move of a constant must update the script and its test in the same change, or the notes say "could not be determined".
- The macOS tarball (attached by hand today) is not described in the notes: the notes describe the rev, not the asset list.

## Rejected

- **`gh release create --generate-notes`.** It lists pull-request titles unsanitised, knows nothing of identity, and the
  tests could not pin its output.
- **A hand-kept `CHANGELOG.md`.** A second thing a one-person project forgets, and it would need its own staleness check.
- **Computing the notes in the publishing job.** That job holds the token and runs none of the rev's, or main's, scripts.
- **Failing the release when a constant cannot be read.** The release is still correct; the honest output is the sentence that
  says the comparison was not made.
