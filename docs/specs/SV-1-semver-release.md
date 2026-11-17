# SV-1: implement ADR 0055 (a release carries a semantic version beside its rev)

Task id: `SV-1`. Base: the commit that adds this spec, on top of `aa375ea` (ADR 0055 merged, PR #96, ratified 2026-11-16).
**The contract is ADR 0055.** Read it whole, together with ADRs 0025, 0029, 0031, 0032, 0035 and 0037, and
`docs/releasing.md`. This spec does not restate the ADR's decisions. It names the work, the order, the tests and the
allowed paths.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

## What this unit delivers

The mechanism only. **It does not bump the version.** `Cargo.toml` stays `0.1.0`, and the 0.2.0 bump is a separate pull
request after this one merges (ADR 0055 decision 6). After this unit, a dispatch at today's `main` must fail closed,
because the version is not greater than the previous release's. Prove that with a test, not a live dispatch.

1. **`scripts/release-notes.sh`** (decisions 3 and 4):
   - The previous release is the nearest `v*` or `rev-*` tag strictly before the rev. The search from `<rev>^` stays.
   - The notes open with the version, the rev and the bump class.
   - A new machine-readable output, beside the notes: the previous release's tag, its version (a `rev-*` release counts as
     `0.1.0`; a `v*` release is read from `<tag>:Cargo.toml`), and identity as one of `moved`, `unmoved` or `unknown`.
   - Choose the interface (a `--facts <file>` flag writing `key=value` lines is fine), document it in the script's header,
     and keep its exit contract.
   - The identity computation stays in this script, its one home.
2. **`.github/workflows/release.yml`, the `build` job** (decision 4):
   - Read `<VERSION>` with `git show <rev>:Cargo.toml`. Never check the rev out.
   - Read the facts file.
   - Fail closed, before any upload and naming the reason, in four cases:
     - the version is not greater than the previous release's;
     - identity moved and the required number did not rise (below 1.0 the minor; from 1.0 the major);
     - identity is `unknown` and only the patch number rose;
     - the bump is a major bump below 1.0.
   - Hand the version to the `publish` job as an output.
   - Put the comparison logic in a small script under `scripts/` that tests can drive (for example
     `scripts/release-version-check.sh`), not inline in YAML. AGENTS.md: "no check logic in workflow YAML".
3. **`.github/workflows/release.yml`, the `publish` job** (decision 5):
   - The tag is `v<VERSION>`; asset names are unchanged.
   - If the tag exists, its commit, read with `gh api repos/<repo>/commits/v<VERSION> --jq .sha`, must equal the rev, or
     the run fails.
   - If the release exists, it must not be a draft and must hold none of the six asset names.
   - If the release is absent, create it at the rev.
   - It still checks out no code.
   - Its final lines name `released v<VERSION>` and one `pin (<target>): VERSION=… REV=… SHA256=… TARGET=…` per target.
4. **`examples/install-req.sh`** (decision 7):
   - `VERSION` joins the pin table and builds the URL `…/download/v<VERSION>/req-<REV>-<TARGET>.tar.gz`.
   - The SHA-256 and `built from <REV>` checks are unchanged.
   - An empty or malformed `VERSION` is refused before any request, like `REV`.
   - Keep the CI snippet in step with it; `tests/ci_template.rs` pins it three ways.
5. **The `Cargo.lock` exception test** (decision 6): a test that, given two lock files, passes when only req's own package
   entry differs and fails when any other entry differs. Use fixtures; it must not depend on git history.
6. **Docs.**
   - `docs/releasing.md`: the runbook for a version bump PR and a `v<VERSION>` dispatch. Its Status records:
     - the 2026-11-16 dispatch of `rev-3294a46`, the first real run of the workflow's own macOS job and release notes;
     - a duplicate dispatch for the same rev, which exercised the no-overwrite refusal (run 43868208448, publish refused,
       release unchanged);
     - that a first dispatch can sit queued for about half an hour.
   - `docs/adopting.md` § *Install req*: name `rev-3294a46` as the first release that reads `req.claim/3`, so the
     prebuilt path works for today's corpus, and describe the `VERSION` pin. Make README's install note agree with it.
   - Make the prose that quotes `0.1.0` version-agnostic: `README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
     `docs/safety-manual.md` and the comment in `build.rs`.
   - ADR 0025's "req's `0.1.0` is the same on every commit by design" becomes "one version covers many commits".
   - ADR 0055's Status becomes accepted (ratified by the owner's merge of PR #96, 2026-11-16).
   - Drop "(proposed)" from the 0055 amendment notes in the headers of ADRs 0025, 0029 and 0031.

## Tests (red first)

Extend the existing tests and keep their style. Each new assertion is proven red at the base before the change.
- `tests/release_notes.rs`:
  - a `v*` tag now counts as a release (today `:213` asserts the opposite: change it, and say so);
  - the previous release is strictly before the rev;
  - the facts output: previous tag, version and the three identity states, using the existing fixture repositories.
- The version check script, table-driven: the four refusals, accept cases (patch with identity unmoved; minor with identity
  moved; minor with identity unknown), and `rev-*` as 0.1.0.
- `tests/release_workflow.rs`, `tests/release_publish.rs`, `tests/release_gate.rs` as they apply. The fake `gh` gains:
  - a tag at another commit, which fails;
  - an existing empty release at the rev, which continues;
  - a draft, which fails;
  - an existing asset, which fails;
  - no release, which is created at the rev.
- `tests/install_req.rs`: the `/v<VERSION>/` path and the four-key pin table (today it pins `/rev-<REV>/` and three keys:
  change those assertions, and say so); a malformed `VERSION` is refused.
- `tests/ci_template.rs`: as the snippet changes.
- The `Cargo.lock` exception test (item 5).
- A test that today's `main` (version 0.1.0, previous release `rev-3294a46`) is refused by the version check.

## Allowed paths

- `scripts/release-notes.sh`
- `scripts/release-version-check.sh` (new), or another new script under `scripts/` with the same purpose
- `.github/workflows/release.yml`
- `examples/install-req.sh`, `examples/` CI snippets that embed it
- `tests/release_notes.rs`, `tests/release_workflow.rs`, `tests/release_publish.rs`, `tests/release_gate.rs`,
  `tests/release_bundle.rs`, `tests/install_req.rs`, `tests/ci_template.rs`
- one new test file for the version check and the `Cargo.lock` exception, plus its fixtures under `tests/fixtures/`
- `docs/releasing.md`, `docs/adopting.md`, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `docs/safety-manual.md`,
  `build.rs` (its comment only)
- `docs/adr/0025-prebuilt-binary-per-pinned-rev.md`, `docs/adr/0029-req-is-published-on-github-under-apache-2.0.md`,
  `docs/adr/0031-release-notes-are-made-from-history-and-say-when-identity-moves.md`,
  `docs/adr/0055-a-release-carries-a-semantic-version-beside-its-rev.md`: the header and status edits named above only

Not allowed: `Cargo.toml`, `Cargo.lock`, `docs/sbom.cdx.json` (the bump is the next PR), `src/`, `claims/`, `baselines/`.

## Done when

- The full `scripts/check.sh` is green with no SKIP.
- Every new test was red at the base.
- `git diff <base>...HEAD --name-only` stays inside the allowed paths.
- The PR body lists each ADR 0055 decision and where it is implemented, and states that no release was dispatched.
