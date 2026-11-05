# #65 — a prebuilt, checksummed binary per pinned rev

Task ids: `65-1` (the build-and-bundle script), `65-2` (the install step, the template and the docs), `65-3`
(the release workflow, the runbook and the timing harness). `65-1` and `65-2` touch disjoint paths and may run
in parallel; `65-3` is dispatched after both have merged. Code baseline: current `main`. Issue #65; the decision
is **[ADR 0025](../adr/0025-prebuilt-binary-per-pinned-rev.md)** (proposed; dispatched only after it merges).
Read the ADR first: the contract is there and is not restated here.

**Model:** `claude-sonnet-5-5` (high) for all three. `65-1` and `65-2` are each reviewed read-only on a different
family (`65-1-review-…`, `65-2-review-…`): the first decides what a released binary has proven, the second is the
fail-closed path an adopter's CI trusts.

## Why

An adopter compiles req from source in every job and holds an SSH key to do it. The fix is the nextest pattern
(pin the bytes by SHA-256 in the adopter's own file), made to carry what is specific to req: the rev the binary
reports must be the rev that was pinned, and a platform that computes a different address must be caught before
release. No `src/` change anywhere in this issue.

## Unit 65-1 — `scripts/build-release.sh`

1. **The script** (`bash`, `set -eu`, runs on macOS and Linux; no GNU-only flags). Usage:
   `scripts/build-release.sh --rev <rev> --out <dir> [--repo <dir>] [--bin <path>]`.
   - Resolve `--rev` in `--repo` (default: the script's repository) to a full SHA. Make a **clean copy** of the
     tree at that commit in a temp directory (`git clone --no-hardlinks` then `checkout`, or `git worktree`
     into the temp dir; never build in the caller's working tree). Without `--bin`, build there with
     `cargo build --release --locked`; the pinned `rust-toolchain.toml` comes with the clone. With `--bin <path>`,
     package that binary instead (so the packaging logic is testable without a compile); **every check below
     applies equally to a `--bin`**, which is why the flag is safe.
   - Read the target from `rustc -vV`'s host line; it must be `x86_64-unknown-linux-gnu` or
     `aarch64-apple-darwin`, otherwise exit 2 naming the host (a `--bin` for another target is the caller's
     problem; the script cannot know it, and says the target it assumed in `build-info`).
   - **Prove before writing.** Run `<bin> --version`; its `built from` value must be a bare SHA (not `-dirty`,
     `unknown`, `-cleanliness-unknown`) and a prefix of the resolved full SHA. That value is `<REV>` in every
     name below. Then, with the clean copy as the working directory: `<bin> baseline --verify
     baselines/air-gapped.json` must print `[VERIFIED]` (exit 0), and `<bin> bench` (seed parity) must exit 0
     and its JSON `fmr.hard_gate` must be `PASS` (the parity corpus's `direction` block is `UNVERIFIABLE` by design
     and is not judged here). Any failed check is exit 1 naming the check; `minisign` absent is exit 3
     (the Baseline step has no SKIP, `AGENTS.md`); in both cases **nothing is written to `--out`**.
   - **Write** to `--out` (created if absent; a pre-existing file of the same name is exit 1, never an
     overwrite): `req-<REV>-<TARGET>.tar.gz` with exactly one member `req` at the root (mode 0755, no directory
     prefix), `req-<REV>-<TARGET>.tar.gz.sha256` (one line, `<hex>  <asset name>`, computed by
     `sha256sum` or `shasum -a 256`, whichever exists), and `req-<REV>-<TARGET>.build-info` (rev, full SHA,
     target, `rustc -V`, `cargo -V`, the builder's libc line, UTC time as `date -u +%Y-%m-%dT%H:%M:%SZ`). Print
     the three paths and one line to paste into an adopter's CI file: `pin: REV=<REV> SHA256=<hex> TARGET=<t>`.
     Write each file to a temp name and `mv` it, so an interrupted run leaves no half-written asset.
2. **Tests** (`tests/release_bundle.rs`, new; public seam = the script run as a process against a temp git
   repository whose commit holds a tiny corpus and a stub `req` shell script as `--bin`; expectations from
   independent oracles — `tar -tf`, `shasum`, a hand-written expected file list — never the script's own output).
   Red first: (a) happy path: three files, the tarball lists exactly `req`, the `.sha256` line equals an
   independently computed hash and names the asset, `pin:` line matches; (b) a stub that prints `-dirty`,
   `unknown`, or a SHA that is not a prefix of the resolved rev → exit 1, `--out` empty; (c) stub whose
   `baseline --verify` prints anything but `[VERIFIED]`, or exits non-zero → exit 1, `--out` empty; (d) stub
   whose `bench` fails its gate → exit 1, `--out` empty; (e) no `minisign` on `PATH` → exit 3, `--out` empty
   (use a PATH holding only what the script needs); (f) an existing output file → exit 1, the file untouched;
   (g) usage errors (no `--rev`, unknown flag, extra positional, `--out` missing) → exit 2; (h) the build runs in a
   clean copy: a tracked file modified in the caller's checkout does not appear in the artifact (stub `req`
   copied from the clone, or a marker file in the clean copy, as the child designs it).
3. **One real run, recorded, not a test.** Build once on this machine without `--bin` from a pushed commit of the
   branch and report in the envelope's `notes` the `pin:` line and `req --version` of the unpacked binary
   (`aarch64-apple-darwin`). Do not upload or publish it: publishing is the owner's act (ADR 0025 §5).

### Mutation checks (envelope `mutations`)

- Skip the `built from` check → (b) fails. Skip `baseline --verify` → (c) fails. Skip `bench` → (d) fails.
- Write the tarball before the checks (and leave it on failure) → the "`--out` empty" assertions fail.
- Overwrite on a name collision → (f) fails. Build in the caller's tree → (h) fails.

### Allowed paths (65-1)

`scripts/build-release.sh` (new), `tests/release_bundle.rs` (new). Forbidden: everything else, notably `src/**`,
`Cargo.toml`/`Cargo.lock`, `scripts/check.sh`, `.forgejo/**`, `claims/**`, `baselines/**`, `bench/**`, docs.

## Unit 65-2 — the install step, the template and the docs

1. **The install script** (`examples/install-req.sh`, new; copy-ready, `bash`, `set -eu`, macOS and Linux).
   Its first lines are the pin table, the only thing an adopter edits:
   `REV=<rev>`, `SHA256_x86_64_unknown_linux_gnu=<hex>`, `SHA256_aarch64_apple_darwin=<hex>`.
   - Detect the target from `uname -sm` (`Linux x86_64`, `Darwin arm64`); anything else, or a target whose pin
     is empty, is a non-zero exit saying so (ADR 0025 §4); never a skip.
   - Download `<base>/rev-<REV>/req-<REV>-<TARGET>.tar.gz` with `curl --proto '=https' --tlsv1.2 -fsSL`, base
     `https://git.xantu-barb.ts.net/fdx/req/releases/download`, the token from `REQ_RELEASE_TOKEN` sent as an
     `Authorization: token …` header **not on the command line and never printed** (no `set -x`; use a header
     file or `-H @-`). A missing `REQ_RELEASE_TOKEN` is a non-zero exit saying which variable to set.
   - Verify the tarball against the **pinned** hash (`sha256sum -c` or `shasum -a 256 -c`); never read the
     release's `.sha256` asset. Check the archive lists exactly one member `req` (a directory prefix, a second
     member or an absolute/`..` path is a refusal) before extracting; install `req` mode 0755 into
     `${REQ_INSTALL_DIR:-$HOME/.local/bin}`; run it: `req --version` must exit 0 and its `built from` must equal
     `REV` exactly. Print the two `--version` lines that decide the binary (`rules_v`, `built from`).
   - Two test-only overrides, documented in the script header and **never set by any adopter-facing YAML** (the 65-3 timing harness sets them, to reach the runner's own server URL):
     `REQ_RELEASE_BASE` and `REQ_CURL_PROTO` (default `=https`).
2. **The template and the docs.**
   - `examples/github-workflows/req.yml`, `docs/adopting.md`'s `## CI` block and `req::init::CI_SNIPPET`:
     replace the `cargo install --git ssh://… --rev` step and its deploy-key plumbing **for req** with a step
     that is `examples/install-req.sh`'s body verbatim, with `REQ_RELEASE_TOKEN` taken from a secret. Keep
     anything the deploy key does for another purpose (read the template; say in the envelope what you kept and
     why). Do not touch the nextest step (its pin and compile are ADR 0019's; a separate change). The three
     copies stay byte-identical (`tests/ci_template.rs`).
   - `docs/adopting.md` § *Install req*: the prebuilt install first (what it needs: `curl`, `tar`, a SHA tool and a
     read token; where the pin comes from — the release's `.sha256` assets, trust on first use of the maintainer's
     release; the macOS and Linux targets; that there is **no automatic fallback**), then the existing
     `cargo install` form as "from source" and "air-gapped" unchanged. State once that the token replaces the SSH
     key and the toolchain for *req*, not the need for a credential. The README keeps linking, not restating.
   - `src/init.rs` next-steps text and `examples/agents-snippet.md` / `examples/handoff-brief.md` /
     `tests/agent_handoff.rs` only where they say how req is installed.
3. **Tests.** `tests/install_req.rs` (new; public seam = the script run as a process against a local HTTP server the
   test starts on `127.0.0.1` serving a fixture tarball built in the test, overrides set only here; oracles =
   hashes computed independently in the test and hand-written expected messages). Red first: (a) happy path: exit
   0, `req` installed 0755, output has the `built from` line; (b) wrong pin hash → non-zero, **nothing installed**;
   (c) unknown rev (server 404) → non-zero; (d) 401 → non-zero naming the token; (e) archive with a directory
   prefix, with two members, with a `..` member → non-zero, nothing extracted; (f) a tarball whose binary prints a
   different `built from` (or `-dirty`) → non-zero; (g) unsupported platform (the test supplies a `uname` shim
   first on `PATH`) → non-zero; (h) a binary that cannot run (not executable content) → non-zero; (i) no token
   → non-zero; (j) the token never appears in the script's stdout or stderr. `tests/ci_template.rs`: the
   three-way equality still holds; the install step's body equals `examples/install-req.sh`'s (modulo YAML
   indentation); no copy contains `cargo install --git`, `REQ_RELEASE_BASE` or `REQ_CURL_PROTO`; the step has no
   `continue-on-error`, no `|| true`, no pipe into `tee`. Update `tests/init.rs` only where it names the old text.
4. **Do not** change the exit contract of any req command, the pin grammar of the lists, or the nextest step.

### Mutation checks (envelope `mutations`)

- Flip one hex digit of the pinned hash in the happy-path fixture → (b) fails. Drop the `built from` assertion →
  (f) fails. Read the checksum from the server instead of the pin → (b) fails (serve a matching tampered pair).
- Add `|| true` to the YAML install step in one copy → the `ci_template` pins fail.
- Remove the single-member check → (e) fails.

### Allowed paths (65-2)

`examples/install-req.sh` (new), `examples/github-workflows/req.yml`, `examples/agents-snippet.md`,
`examples/handoff-brief.md`, `docs/adopting.md`, `src/init.rs` (snippet and next-steps text only),
`tests/install_req.rs` (new), `tests/ci_template.rs`, `tests/init.rs`, `tests/agent_handoff.rs`, `README.md`
(only a stale install line). Forbidden: everything `65-1` forbids except `src/init.rs`, plus
`scripts/build-release.sh` and `tests/release_bundle.rs`.

## Unit 65-3 — the release workflow, the runbook and the timing harness (after 65-1 and 65-2 merge)

1. `.forgejo/workflows/release.yml` (new): **`workflow_dispatch` only** (input `rev`; no `push`, no
   `pull_request`, no `schedule`). It checks out `rev`, installs the pinned toolchain and verifies that minisign
   is present, as `ci.yml` does, runs `scripts/build-release.sh --rev <rev> --out …`, creates the release `rev-<REV>` if absent and
   uploads the three assets with the Forgejo API (`POST /repos/fdx/req/releases`, `POST …/releases/{id}/assets`),
   **failing** if an asset of that name already exists. Credential: one repository secret (`RELEASE_TOKEN`,
   `write:repository`), the owner's to provision; the workflow never prints it. No `continue-on-error`.
2. `.forgejo/workflows/install-timing.yml` (new): `workflow_dispatch` only, inputs `rev` and `sha256`; it times
   (with `date +%s` around each) the source install (`cargo install --git … --rev … --locked`) and the
   prebuilt install (`examples/install-req.sh`) on the runner, and prints two lines, `INSTALL source: <n>s` and
   `INSTALL prebuilt: <n>s`. **No child writes a timing into a document**: the owner dispatches it and the
   orchestrator copies the two numbers (and the run URL) into `docs/releasing.md`'s "Measured" section.
3. `docs/releasing.md` (new, one page): the owner's runbook — when to cut a release (on request, not every
   merge), the Linux path (dispatch `release.yml`), the macOS path (`scripts/build-release.sh` then attach with
   the API call shown, refusing an existing name), what to pin in an adopter after (the `pin:` line), what a
   release must never do (replace an asset, release a `-dirty` binary), the "Measured" section (empty until
   filled as above), and the token scopes (`read:repository` for adopters, `write:repository` for the secret).
4. `tests/release_workflow.rs` (new): string pins on both workflows — dispatch-only triggers; no
   `continue-on-error`; the upload step cannot overwrite; `release.yml` calls `scripts/build-release.sh`;
   `docs/releasing.md` exists and names both scripts. Mutation-check each pin.

### Allowed paths (65-3)

`.forgejo/workflows/release.yml` (new), `.forgejo/workflows/install-timing.yml` (new), `docs/releasing.md` (new),
`tests/release_workflow.rs` (new), `AGENTS.md` (one line under "Where things live"). Forbidden: everything
`65-1` forbids except `.forgejo/**` (and then only the two new files), plus `ci.yml`, `review.yml`.

## Verify (all units)

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, **read `$?` before committing**, exit 0, no
SKIP, Baseline `[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline,
`bench/results` or `src/` change (except `65-2`'s snippet text in `src/init.rs`). Publishing a release is not
part of any unit; it is the owner's act after `65-3`, and then `65-2`'s install is run once for real by the
orchestrator against that release.
