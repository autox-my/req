# #138 — publish req on GitHub: hosting port

Task id: `138-1` (one unit). Code baseline: current `main` after the pull request that carries this file merges.
Issue #138; the decision is **[ADR 0029](../adr/0029-req-is-published-on-github-under-apache-2.0.md)** (proposed;
dispatched only after it merges). Read the ADR first: the contract is there and is not restated here. Also read
[ADR 0025](../adr/0025-prebuilt-binary-per-pinned-rev.md) (the release and install contract this unit moves),
`docs/releasing.md`, `tests/release_workflow.rs`, `tests/ci_template.rs`, `tests/install_req.rs` and the *Install*
parts of `docs/adopting.md`.

**Model:** `claude-sonnet-5-5` (high). Reviewed read-only on a different family (`138-1-review-…`): it decides
whether anything the public tree should not say is left, and whether a workflow can be misused from a fork.

## Why

The licence, `NOTICE`, `.mailmap`, the README licence section and the Cargo metadata are already in `main` (the
pull request that carries this spec). What is left is that every instruction in the tree sends a reader to a
private Forgejo host, and CI, release and review are Forgejo workflows that GitHub does not read.

## Unit 138-1 — the hosting port

1. **CI.** `.github/workflows/ci.yml`: the current `.forgejo/workflows/ci.yml` moved and re-worded for GitHub
   (comments say GitHub, not Forgejo). Same steps, same `scripts/check.sh` invocation, same
   "no `SKIP` line is a pass" step, same trust anchor read from `origin/main` with `fetch-depth: 0`. Add
   `permissions: contents: read` at the top level. Triggers `pull_request`, `push` to `main`, `workflow_dispatch`
   (never `pull_request_target`). The job id stays `gate`. No secret is referenced.
2. **Release.** `.github/workflows/release.yml`, `workflow_dispatch` only, input the rev as now. The build and
   the §2 proofs of ADR 0025 are unchanged. The publish half is rewritten for GitHub: create release `rev-<REV>`
   if absent, upload the three assets, using the workflow's `GITHUB_TOKEN`
   (`permissions: contents: write` on the **publishing job only**; the build job stays `contents: read`). An asset
   name that already exists is a non-zero exit, never a replacement (no `--clobber`, no delete-and-upload). No
   `RELEASE_TOKEN` and no `REQ_RELEASE_TOKEN` is read by the workflow.
3. **Delete `.forgejo/` entirely** (`ci`, `release`, `install-timing`, `review`, `review-measure`). Nothing in
   the tree may mention `.forgejo/workflows/` as a place that exists, except history-bearing text (below).
4. **The install path.**
   - `examples/install-req.sh`, the copies of it in `examples/github-workflows/req.yml`,
     `examples/github-workflows/req-diff.yml` and `src/init.rs`, and the install text of `docs/adopting.md`:
     default base `https://github.com/autox-my/req/releases/download`. The `Authorization` header is sent **only**
     when `REQ_RELEASE_TOKEN` is set and non-empty (a private fork). Every other rule of ADR 0025 §3–§4 is
     unchanged: `--proto '=https'`, pinned SHA-256, exactly one member `req`, `built from <REV>`, no fallback.
   - The source path is `cargo install --git https://github.com/autox-my/req.git --rev <REV> --locked`. The SSH
     deploy-key steps, `CARGO_NET_GIT_FETCH_WITH_CLI` and `StrictHostKeyChecking` handling of the old template go.
   - `src/init.rs`: `ISSUES_URL` is `https://github.com/autox-my/req/issues`; the next-steps text names only
     `.github/workflows/req.yml` (no Forgejo variant).
   - Issue and report links in `docs/adopting.md` and the rest of the live docs point at GitHub.
5. **Docs.** `docs/releasing.md` rewritten for GitHub (the dispatch, the owner attaching the macOS tarball with
   `gh release upload`, the first-run checklist of the evidence section below, the install timing measured once
   by hand if the owner wants it, since `install-timing.yml` is gone). `AGENTS.md`: remove the runner inventory
   (the CT/host/user paths and `pct exec`), "main is protected on Forgejo", the Forgejo job-log paragraph; state
   what is true: CI is `ci.yml`; **there is no automated review workflow in this repository, a human reads every
   pull request** (ADR 0029 §7); the `tools/review/` and `review/` files remain for a maintainer to run by hand.
   `README.md`: only if it names the old host or the removed workflows. `docs/adr/0025-…md`: add one status line
   "Amended by ADR 0029 §4–§5" — no other edit to any ADR, spec or handoff (they are history; their old URLs and
   issue numbers stay).
6. **Tests** (update the existing ones; they must assert the *new* contract, never be loosened to pass):
   - `tests/release_workflow.rs`, `tests/ci_template.rs`, `tests/install_req.rs`, `tests/init.rs` follow the new
     paths, host and rules above. The release test pins: dispatch-only trigger; `contents: write` on the publish
     job only and `contents: read` elsewhere; the absence of `--clobber`; the absence of any `secrets.` reference
     other than `GITHUB_TOKEN`; the existing-asset error.
   - **`tests/public_tree.rs`** (new), independent of the others, over `git ls-files`: no tracked path starts
     with `.forgejo/`; no tracked file outside `docs/adr/`, `docs/specs/`, `docs/handoffs/`, `bench/results/`,
     `experiments/`, `prompts/`, `tests/` (tests assert the absence of these strings, so they name them) and
     `bench/corpus/` contains `xantu-barb`, `.ts.net`,
     `pct exec`, `/Users/`, a `100.x.y.z` address in `100.64.0.0/10`, or `FORGEJO`; `LICENSE` starts with the
     Apache-2.0 title and `NOTICE` exists and names `PROMISE`, `StrictDoc` and `Doorstop`; `Cargo.toml` says
     `license = "Apache-2.0"`; `.mailmap` exists. (The oracle is the list of strings above, written in the test.)
   - A test that the `ci.yml` `on:` block holds no `pull_request_target`.
7. **Mutation checks** (envelope `mutations`):
   - Restore one `xantu-barb` URL in `docs/adopting.md` → `public_tree` fails. Restore `.forgejo/workflows/ci.yml`
     → `public_tree` fails.
   - Add `--clobber` to the upload → the release test fails. Give the build job `contents: write` → it fails.
   - Always send the `Authorization` header in `install-req.sh` (even with no token) → the install test fails.
   - Change a trigger to `pull_request_target` → the ci test fails.
8. **Allowed paths:** `.github/workflows/ci.yml`, `.github/workflows/release.yml` (new), `.forgejo/**` (deleted),
   `examples/**`, `scripts/build-release.sh` (only if it names the forge), `src/init.rs`, `tests/release_workflow.rs`,
   `tests/ci_template.rs`, `tests/install_req.rs`, `tests/init.rs`, `tests/public_tree.rs` (new), `docs/adopting.md`,
   `docs/releasing.md`, `docs/adr/0025-prebuilt-binary-per-pinned-rev.md` (the one status line), `AGENTS.md`,
   `README.md`, `docs/overview.html` (only if it names the old host).
   **Forbidden:** every other `src/**` file, `build.rs`, `claims/**`, `baselines/**`, `bench/**` (the corpora and
   the append-only results stay byte-identical), `aliases/**`, `decisions/**`, `review/**`, `tools/**`,
   `tests/fixtures/**`, every other ADR/spec/handoff, `Cargo.toml`, `Cargo.lock`, `LICENSE`, `NOTICE`, `.mailmap`,
   any canonicalizer / `RULES_V` / address change, any re-signing of a Baseline.

## Not verifiable by this unit (the owner, after the push; stated, not claimed)

The tests pin the text and shape of the workflows. They cannot show a green GitHub run. Checklist for the owner:
1. Push `main` (and the `rev-a8fcd2a` tag only if wanted; **never** `--mirror` or `--all`, which would publish the
   `refs/t3/…` checkpoint refs with local paths and a tailnet address).
2. Settings: Actions enabled; branch protection on `main` requiring the `gate` check, no force-push; "Keep my
   email addresses private" and "Block command line pushes that expose my email" on.
3. Run `CI` by `workflow_dispatch`; read the log (the toolchain install, the nextest download and its SHA-256,
   the gate, no `SKIP`).
4. Run `release` for a rev; install from the result with the template's step on a scratch repository.
5. Update `disktriage` README ("not publicly available yet") and its install instructions.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, **read `$?` before committing**, exit 0, no
SKIP, Baseline `[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline,
corpus or `bench/results` change; `git diff --stat` shows none of them.
