# 0029. req is published on GitHub under Apache-2.0; GitHub becomes the canonical home

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-08, former Forgejo PR #139); issue #138. Date
proposed: 2026-11-08. **Amends [ADR 0025](0025-prebuilt-binary-per-pinned-rev.md) §5 and §6** (where the release
lives and what the install needs); nothing else in 0025 changes. ADRs 0001–0009 are untouched.

## Context

req has lived on a private Forgejo instance (`git.xantu-barb.ts.net`, reachable over a tailnet). The owner
publishes it at `https://github.com/autox-my/req`, beside `disktriage`, which already adopts it. The scan done
before this decision (2026-11-08) found no secret in any commit reachable from `main`, no customer content, and
only permissive dependency licences. It found three things that a public repository changes:

- the private host is written into the install path, the issue links, the CI snippets and the tests that pin them, so a public
  reader could neither install req nor report a bug;
- the tree has no licence, and ADR 0025 describes the repository as "private, unlicensed";
- CI, release and review run on Forgejo (`.forgejo/workflows/`), which GitHub does not read, and two of those
  workflows need the owner's private LLM endpoint and the Forgejo API.

## Decision

1. **Licence: Apache-2.0**, as `disktriage`. `LICENSE` is the standard text; `NOTICE` names the copyright holder
   and lists the third-party data the repository keeps (the external benchmark corpora and the fixtures), each
   under its own licence. The PROMISE data is credited to its authors (CC-BY-4.0 on the Zenodo record); the
   licence of the rows the 969-row expanded copy adds to it is **not verified**, and `NOTICE` says so.
2. **GitHub is canonical.** The repository is `autox-my/req`. Pull requests, required checks and releases happen
   there. The Forgejo instance stops being a place the project's own instructions send anyone to.
3. **CI is `.github/workflows/ci.yml`**, the same job as before: it invokes `scripts/check.sh` and implements no
   check of its own. It runs on `pull_request` and `push` to `main` with read-only permissions and no secret, so
   a pull request from a fork cannot reach one. The trust anchor is still read from `origin/main`, not from the
   pull request (unchanged; it needs the full-history checkout the workflow already has).
4. **The release is `.github/workflows/release.yml`, dispatch-only**, with the checks of ADR 0025 §2 unchanged.
   It publishes a GitHub release `rev-<REV>` with the three assets of §1 using the workflow's own
   `GITHUB_TOKEN` (`contents: write` on the publishing job only). **No repository secret is needed.** An asset
   name that already exists on the release is an error, never a replacement (§5 stands: a checksum that has been
   pinned must stay true). The macOS tarball is still built by the owner's machine with the same script and
   attached by hand.
5. **The install needs no credential for a public release.** The default download base becomes
   `https://github.com/autox-my/req/releases/download`; a token is sent only when the adopter sets one (a fork
   or mirror that is private). The pin, the SHA-256 check, the `built from <REV>` assertion and the fail-closed
   rules of §3 and §4 are unchanged. The source path becomes `cargo install --git https://github.com/autox-my/req.git
   --rev <REV> --locked`; the SSH deploy key the old template managed is gone.
6. **`main` history is published as it is.** No commit is rewritten, so every SHA, every `rev-<REV>` tag and every
   adopter pin stays valid. Consequences the owner accepted on 2026-11-08: the author addresses in those commits
   (`fadhli.dasril@gmail.com`, already public through `disktriage`, and the local hostname address
   `fd@fd-macbook.local`) become public; older revisions still contain the private host name, the runner
   inventory that `AGENTS.md` used to hold, `.forgejo/` and the PROMISE-derived corpora. A `.mailmap` folds the two
   author names into one for `git log` and `git shortlog`; it changes no commit. Removing content from the
   current tree does not remove it from those older revisions.
7. **Not published.** The review workflows (`review.yml`, `review-measure.yml`) and `install-timing.yml` leave the
   tree: they depend on the owner's private LLM endpoint and the Forgejo API, and a workflow that holds an LLM
   credential is the wrong thing to run on pull requests from strangers. `tools/review/`, `review/rule.json` and
   `tools/ocr/` stay, so a maintainer can run the review by hand. **This ends "CI reviews" (owner ruling
   2026-11-03): until a review workflow is chosen for GitHub, a human reads every pull request, and `AGENTS.md`
   says so.**
8. **Old numbers are history.** Issue and pull-request numbers in ADRs, specs and commit messages (`#44`, `#137`)
   are the Forgejo numbers. They do not resolve on GitHub, whose numbering starts again at 1.

## What stays outside req

The `req` binary changes only strings in `req init` (the issues URL and the install hints). No claim, address,
canonicalizer rule or Baseline moves, so there is no `RULES_V` bump and no re-signing.

## Consequences

- First-time readers can install, build and file an issue with nothing but a browser and `curl`.
- The first real run of the ported workflows happens on GitHub, after the push. Tests pin their text and shape;
  they cannot show a green run. The owner's first-run checklist is in `docs/specs/138-github-publication.md`.
- Branch protection on GitHub (a required `gate` check, no force-push to `main`) is the owner's setting; the
  repository cannot configure it.
- Any adopter still on the Forgejo URL keeps working only while that host stays up; their pins do not change.

## Rejected

- **Rewrite history (`git filter-repo`) to a noreply address or to a fresh root commit.** It would remove the
  addresses and the old infrastructure detail from the past, at the price of every SHA, the `rev-` tags and every
  pinned adopter. Declined by the owner; the address is already public through `disktriage`.
- **Keep Forgejo canonical and mirror `main`.** `.forgejo/` could not stay out of the public tree, so the two
  trees would diverge on every sync.
- **Port `review.yml` with the LLM credential as a repository secret.** A review that spends the owner's key on
  strangers' pull requests needs its own decision; it is not bundled into a move.
- **Drop the PROMISE-derived pairs from the tree.** They are about two-thirds of `external-v4`, so the FMR
  evidence would have to be regenerated and re-measured; and the data would stay in history regardless.
