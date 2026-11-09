//! String pins on the release machinery of ADR 0025 as amended by ADR 0029 (unit 138-1 of
//! `docs/specs/138-github-publication.md`): `.github/workflows/release.yml`, `.github/workflows/ci.yml`'s
//! trigger block and `docs/releasing.md`.
//!
//! The workflow cannot be run from a test (no runner, and publishing a release is the owner's act),
//! so what is checked is what a text can show and what a careless edit would silently lose: the
//! **trigger** (a release is cut on request, never by a push, a pull request, a schedule or another
//! workflow), the **permissions** (`contents: write` on the publishing job only, `contents: read`
//! everywhere else), the absence of any secret but the run's own `GITHUB_TOKEN`, the absence of anything
//! that turns a failure into a pass, the **no-overwrite** rule of §5 (a checksum that has been pinned
//! must stay true: no `--clobber`, no delete, no edit), that the Linux tarball comes out of
//! `scripts/build-release.sh` (the script that proves a binary before it writes one) and that the
//! token never reaches a command line.
//!
//! Pins read the file with whole-line comments removed, so a comment may say "no `continue-on-error`"
//! without tripping the pin on `continue-on-error`. The pins are text, not a parse of YAML, and say
//! so: they catch the edits this repository has already had to take back, not every possible one.
//! Each one has been mutation-checked (the change that must turn it red is in the unit's envelope).
//! **None of this shows a green GitHub run**: that is the owner's first-run checklist in the spec.

use std::path::{Path, PathBuf};

const RELEASE: &str = ".github/workflows/release.yml";
const CI: &str = ".github/workflows/ci.yml";
const RUNBOOK: &str = "docs/releasing.md";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// A whole file, or a panic that names the file: "it is missing" is the failure these pins exist for.
fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{rel} must exist and be readable ({}): {error}",
            path.display()
        )
    })
}

/// The file without its whole-line comments (a line whose first non-blank character is `#`).
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The keys directly under the top-level `on:` mapping, in order. A key is an indented
/// `name:` line at the first indentation level inside `on:`.
fn trigger_keys(rel: &str) -> Vec<String> {
    let text = code(rel);
    let mut inside = false;
    let mut keys = Vec::new();
    for line in text.lines() {
        if line.starts_with("on:") {
            assert!(
                line.trim_end() == "on:",
                "{rel}: the `on:` mapping must be written as a block, one trigger per line (found `{line}`)"
            );
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        // The next top-level key ends the block.
        if !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if let Some(rest) = line.strip_prefix("  ")
            && !rest.starts_with(' ')
            && !rest.trim().is_empty()
        {
            let key = rest.split(':').next().unwrap_or("").trim();
            keys.push(key.to_owned());
        }
    }
    keys
}

/// The lines of the `inputs:` block of `workflow_dispatch`, as the input names it declares.
fn input_names(rel: &str) -> Vec<String> {
    let text = code(rel);
    let mut in_inputs = false;
    let mut indent = 0;
    let mut names = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let lead = line.len() - trimmed.len();
        if trimmed == "inputs:" {
            in_inputs = true;
            indent = lead;
            continue;
        }
        if in_inputs {
            if trimmed.is_empty() {
                continue;
            }
            if lead <= indent {
                break;
            }
            if lead == indent + 2 {
                names.push(trimmed.split(':').next().unwrap_or("").to_owned());
            }
        }
    }
    names
}

fn require(rel: &str, text: &str, needle: &str, why: &str) {
    assert!(
        text.contains(needle),
        "{rel} must contain `{needle}` — {why}"
    );
}

fn forbid(rel: &str, text: &str, needle: &str, why: &str) {
    assert!(
        !text.contains(needle),
        "{rel} must not contain `{needle}` — {why}"
    );
}

/// The block of one job: the lines under `  <name>:` up to the next line indented two spaces or less.
fn job_block(text: &str, name: &str) -> String {
    let header = format!("  {name}:");
    let mut inside = false;
    let mut block = Vec::new();
    for line in text.lines() {
        if line == header {
            inside = true;
            continue;
        }
        if inside {
            let indent = line.len() - line.trim_start().len();
            if !line.trim().is_empty() && indent <= 2 {
                break;
            }
            block.push(line);
        }
    }
    assert!(inside, "a job named `{name}` must exist");
    block.join("\n")
}

/// The top-level `permissions:` block, as its `key: value` lines (a job's own sits deeper).
fn top_level_permissions(text: &str) -> Vec<String> {
    let mut inside = false;
    let mut entries = Vec::new();
    for line in text.lines() {
        if line.trim_end() == "permissions:" {
            inside = true;
            continue;
        }
        if inside {
            if !line.is_empty() && !line.starts_with(' ') {
                break;
            }
            if !line.trim().is_empty() {
                entries.push(line.trim().to_owned());
            }
        }
    }
    entries
}

#[test]
fn the_release_is_dispatch_only() {
    assert_eq!(
        trigger_keys(RELEASE),
        vec!["workflow_dispatch".to_owned()],
        "{RELEASE}: a release is cut on request — `workflow_dispatch` is the only trigger \
         (no push, pull_request, schedule, release or workflow_run)"
    );
}

#[test]
fn ci_runs_on_pull_request_push_and_dispatch_and_never_on_pull_request_target() {
    // `pull_request_target` runs the base repository's workflow with its secrets against code the
    // pull request supplies; the CI job must never be reachable that way (ADR 0029 §3).
    assert_eq!(
        trigger_keys(CI),
        vec![
            "pull_request".to_owned(),
            "push".to_owned(),
            "workflow_dispatch".to_owned()
        ],
        "{CI}: the `on:` block holds exactly pull_request, push and workflow_dispatch"
    );
    for rel in [CI, RELEASE] {
        forbid(
            rel,
            &code(rel),
            "pull_request_target",
            "it would hand the repository's token to a fork's code",
        );
    }
}

#[test]
fn ci_cancels_a_superseded_pull_request_run_and_never_a_run_on_main() {
    // A new push to a pull request makes the older run of that pull request pointless, so it is cancelled.
    // A run on `main` (or a manual dispatch) is a verdict on a commit and always completes: the post-merge
    // run is the only proof that `main` itself is green.
    let text = code(CI);
    let group = text
        .lines()
        .find(|line| line.trim_start().starts_with("group:"))
        .unwrap_or_else(|| panic!("{CI}: a top-level `concurrency:` block with a `group:`"));
    assert!(
        group.contains("github.workflow")
            && group.contains("github.event.pull_request.number")
            && group.contains("github.run_id"),
        "{CI}: the group is per workflow and per pull request, and a non-pull-request run gets its own group \
         (github.run_id) so a run on main is never cancelled by another: {group}"
    );
    let cancel = text
        .lines()
        .find(|line| line.trim_start().starts_with("cancel-in-progress:"))
        .unwrap_or_else(|| panic!("{CI}: `cancel-in-progress:` is stated"));
    assert_eq!(
        cancel.trim(),
        "cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
        "{CI}: only a pull_request run may be cancelled"
    );
}

#[test]
fn the_release_takes_exactly_one_input_the_rev() {
    assert_eq!(
        input_names(RELEASE),
        vec!["rev".to_owned()],
        "{RELEASE}: exactly one input, `rev`"
    );
}

#[test]
fn write_permission_is_the_publishing_jobs_alone() {
    let text = code(RELEASE);
    assert_eq!(
        top_level_permissions(&text),
        vec!["contents: read".to_owned()],
        "{RELEASE}: the workflow's default is `contents: read` and nothing else"
    );
    let build = job_block(&text, "build");
    let publish = job_block(&text, "publish");
    assert!(
        build.contains("contents: read") && !build.contains(": write"),
        "{RELEASE}: the build job runs the rev's code and holds `contents: read`, never write:\n{build}"
    );
    assert!(
        publish.contains("contents: write"),
        "{RELEASE}: the publishing job holds `contents: write`:\n{publish}"
    );
    assert_eq!(
        text.matches(": write").count(),
        1,
        "{RELEASE}: exactly one write permission in the whole workflow, the publishing job's"
    );
    for broader in [
        "write-all",
        "id-token",
        "packages:",
        "actions:",
        "pull-requests:",
    ] {
        forbid(
            RELEASE,
            &text,
            broader,
            "the publishing job needs `contents: write` and no other scope",
        );
    }
    // The job that holds the token runs none of the rev's code.
    forbid(
        RELEASE,
        &publish,
        "actions/checkout",
        "the publishing job checks out nothing: it must not be able to run the rev's code",
    );
    forbid(
        RELEASE,
        &publish,
        "scripts/",
        "the publishing job runs no script from the tree",
    );
    forbid(
        RELEASE,
        &build,
        "GITHUB_TOKEN",
        "the job that runs the rev's code does not receive the token",
    );
    forbid(
        RELEASE,
        &build,
        "GH_TOKEN",
        "the job that runs the rev's code does not receive the token",
    );
}

#[test]
fn ci_is_read_only_and_references_no_secret() {
    let text = code(CI);
    assert_eq!(
        top_level_permissions(&text),
        vec!["contents: read".to_owned()],
        "{CI}: `permissions: contents: read` at the top level, nothing broader"
    );
    forbid(CI, &text, ": write", "CI holds no write permission");
    forbid(
        CI,
        &text,
        "secrets.",
        "CI references no secret: a fork's pull request must not reach one",
    );
    forbid(CI, &text, "GITHUB_TOKEN", "CI does not use the token");
    assert!(
        text.lines().any(|line| line.trim() == "gate:"),
        "{CI}: the job id stays `gate`, the name branch protection requires"
    );
}

#[test]
fn every_download_of_a_tool_in_ci_retries_a_transient_error_and_still_checks_the_hash() {
    let text = code(CI);
    for line in text.lines().filter(|line| line.contains("curl ")) {
        assert!(
            line.contains("--retry 3 --retry-all-errors"),
            "{CI}: a download retries a transient server error (a 504 failed the first run once):\n{line}"
        );
    }
    assert!(
        text.matches("curl ").count() >= 2,
        "{CI}: the rustup installer and the nextest tarball are both downloaded with curl"
    );
    // The hash is compared after the download, and a mismatch is never retried around.
    require(
        CI,
        &text,
        "sha256sum -c -",
        "the nextest tarball is verified against its pinned SHA-256",
    );
    require(
        CI,
        &text,
        "SHA256=32d1d8c0567b7a80bee2bad8bb09125c846da96bc009b2de11ec08557290c770",
        "the pin of the nextest tarball is unchanged by the move",
    );
    forbid(
        CI,
        &text,
        "|| true",
        "it would swallow the failure it follows",
    );
}

#[test]
fn the_nextest_download_falls_back_to_the_api_asset_route_and_is_still_hash_checked() {
    // 2026-11-09: `github.com/.../releases/download/...` answered 504 for this asset on every try
    // (reproduced from outside CI) while GitHub's status page was green, and the same asset came
    // down intact through the API asset route. The fallback is the same file, so it changes where
    // the bytes come from and nothing about what is accepted: the pin below still decides.
    let text = code(CI);
    require(
        CI,
        &text,
        "https://api.github.com/repos/nextest-rs/nextest/releases/assets/660814555",
        "the fallback names the pinned asset by id, so a name cannot be re-pointed",
    );
    require(
        CI,
        &text,
        "Accept: application/octet-stream",
        "the API asset route serves the bytes only for this Accept header",
    );
    let primary = text
        .find("releases/download/cargo-nextest")
        .expect("primary url");
    let fallback = text
        .find("releases/assets/660814555")
        .expect("fallback url");
    let check = text.find("sha256sum -c -").expect("hash check");
    assert!(
        primary < fallback && fallback < check,
        "{CI}: the primary is tried first and the hash is checked after BOTH routes, never between"
    );
    forbid(
        CI,
        &text,
        "Authorization",
        "the fallback is anonymous: CI does not use the token (ci_is_read_only_and_references_no_secret)",
    );
}

#[test]
fn no_step_turns_a_failure_into_a_pass() {
    for rel in [RELEASE, CI] {
        let text = code(rel);
        forbid(
            rel,
            &text,
            "continue-on-error",
            "a release or a gate that fails must fail the run",
        );
        forbid(
            rel,
            &text,
            "|| true",
            "it would swallow the failure it follows",
        );
        forbid(
            rel,
            &text,
            "||true",
            "it would swallow the failure it follows",
        );
        forbid(rel, &text, "set +e", "it would let a failed command pass");
    }
    // The release has no pipe into `tee`: the pipeline's status is tee's, not the command's
    // (AGENTS.md). ci.yml uses one on purpose and reads PIPESTATUS, which the release does not need.
    forbid(
        RELEASE,
        &code(RELEASE),
        "| tee",
        "the pipeline's status is tee's, not the command's (AGENTS.md)",
    );
}

#[test]
fn neither_workflow_names_the_old_host() {
    for rel in [RELEASE, CI] {
        let text = read(rel);
        for old in [
            "xantu-barb",
            ".ts.net",
            "FORGEJO",
            "forgejo",
            "Forgejo",
            "RELEASE_TOKEN",
        ] {
            forbid(
                rel,
                &text,
                old,
                "the public workflows know nothing of the private host or its credentials (ADR 0029)",
            );
        }
    }
}

#[test]
fn the_release_builds_with_the_script_that_proves_the_binary() {
    let text = code(RELEASE);
    require(
        RELEASE,
        &text,
        "scripts/build-release.sh",
        "the binary is built, proved and bundled by one script (ADR 0025 §2), not by steps of the workflow",
    );
    require(
        RELEASE,
        &text,
        "--rev",
        "the script is told which rev to release",
    );
    require(
        RELEASE,
        &text,
        "--out",
        "the script is told where the assets go",
    );
    require(
        RELEASE,
        &text,
        "!= refs/heads/main",
        "the tooling that proves the binary is main's: a dispatch from another ref is refused",
    );
    require(
        RELEASE,
        &text,
        "merge-base --is-ancestor",
        "only a rev on main is released (the script proves the Baseline with the trust anchor of the rev it builds)",
    );
    require(
        RELEASE,
        &text,
        "case $REV_INPUT in",
        "the rev input is a name, never a command: its alphabet is checked before it reaches git",
    );
    require(
        RELEASE,
        &text,
        "\"$full\" != \"$REL_FULL\"",
        "what is published is the rev that was checked to be on main",
    );
    require(
        RELEASE,
        &text,
        "sha256sum \"$RELEASE_OUT/$tarball\"",
        "the sidecar is re-checked against the tarball after the artifact hand-over",
    );
    forbid(
        RELEASE,
        &text,
        "cargo build",
        "a build here would bypass the proofs the script runs before writing a tarball",
    );
    forbid(
        RELEASE,
        &text,
        "tar -c",
        "a tarball made here would not be the one that passed the proofs",
    );
}

#[test]
fn the_release_keeps_the_runner_prerequisites_of_ci() {
    let text = code(RELEASE);
    require(
        RELEASE,
        &text,
        "command -v cc",
        "a rustup toolchain is not sufficient: build scripts need a linker (ci.yml)",
    );
    require(
        RELEASE,
        &text,
        "command -v minisign",
        "the Baseline proof has no SKIP: no verifier is UNVERIFIABLE and blocks (ci.yml)",
    );
    require(
        RELEASE,
        &text,
        "rustup toolchain install",
        "the pinned toolchain must be present before the script builds",
    );
    require(
        RELEASE,
        &text,
        "GITHUB_PATH",
        "a step's `export` does not reach the next step; the toolchain is published through the runner",
    );
    require(
        RELEASE,
        &text,
        "fetch-depth: 0",
        "the rev to release is resolved in the checkout's history",
    );
    for line in text.lines().filter(|line| line.contains("curl ")) {
        assert!(
            line.contains("--retry 3 --retry-all-errors"),
            "{RELEASE}: the rustup download retries a transient error:\n{line}"
        );
    }
}

#[test]
fn the_upload_cannot_overwrite_an_asset() {
    let text = code(RELEASE);
    // It looks before it writes, and a name that is already there is an error, not a replacement.
    require(
        RELEASE,
        &text,
        "--json assets",
        "the release's existing assets are listed before anything is uploaded",
    );
    require(
        RELEASE,
        &text,
        "already exists on release $tag — refusing to replace it",
        "an asset name that already exists is an error naming the rule (ADR 0025 §5)",
    );
    // And the refusal comes first: the check is before the first byte is uploaded, not after.
    let refusal = text.find("already exists on release").unwrap_or(usize::MAX);
    require(
        RELEASE,
        &text,
        "gh release upload \"$tag\"",
        "the upload the ordering below is measured against",
    );
    let first_upload = text
        .find("gh release upload \"$tag\"")
        .unwrap_or(usize::MAX);
    assert!(
        refusal < first_upload,
        "{RELEASE}: the existing-asset check must come before the first upload, not after it"
    );
    // The tarball goes last: a visible tarball means the other two assets are there.
    require(
        RELEASE,
        &text,
        "names=\"req-$rev-$target.build-info req-$rev-$target.tar.gz.sha256 $tarball\"",
        "the tarball is uploaded last, so it is the signal that the release is complete",
    );
    // And the count afterwards, which is what a racing second run shows up in.
    require(
        RELEASE,
        &text,
        "after the upload; expected exactly once",
        "each name exactly once after the upload",
    );
    // A draft release is refused, not published into or edited.
    require(
        RELEASE,
        &text,
        "exists as a draft",
        "an asset uploaded into a draft is invisible to adopters",
    );
    for spelling in [
        // replacing, deleting or editing what is published
        "--clobber",
        "gh release delete",
        "gh release delete-asset",
        "gh release edit",
        "gh release create --draft",
        // raw API access, which could do any of the above under another name
        "gh api",
        "-X PUT",
        "-X PATCH",
        "-X DELETE",
        "--method",
        "--request",
        "--upload-file",
        "-T ",
        // ending a step green after a failure, or printing the credential
        "|| :",
        "|| exit 0",
        "set -ex",
        "set -x",
        "set -eux",
        "set -euxo",
        "GH_DEBUG",
        "--verbose",
        "--trace",
    ] {
        forbid(
            RELEASE,
            &text,
            spelling,
            "a spelling that deletes or replaces an asset, ends the step green after a failure, or prints the credential",
        );
    }
}

#[test]
fn the_only_credential_is_the_runs_own_token_and_it_is_never_on_a_command_line() {
    let text = code(RELEASE);
    // Every `secrets.` reference is the token the run mints for itself: no repository secret
    // (no RELEASE_TOKEN, no REQ_RELEASE_TOKEN) is read, so there is nothing for the owner to provision.
    let mut seen = 0;
    for (at, _) in text.match_indices("secrets.") {
        seen += 1;
        assert!(
            text[at..].starts_with("secrets.GITHUB_TOKEN"),
            "{RELEASE}: the only secrets reference allowed is secrets.GITHUB_TOKEN, found `{}`",
            text[at..].lines().next().unwrap_or("")
        );
    }
    assert!(
        seen >= 1,
        "{RELEASE}: the publishing job authenticates with secrets.GITHUB_TOKEN"
    );
    forbid(
        RELEASE,
        &text,
        "RELEASE_TOKEN",
        "no repository secret is read by the release",
    );
    // The token is mapped into one environment variable `gh` reads, and into no argument list.
    for line in text.lines().filter(|line| line.contains("GH_TOKEN")) {
        assert!(
            line.trim_start() == "GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}",
            "{RELEASE}: GH_TOKEN appears only as the env mapping of the token, found `{line}`"
        );
    }
    for line in text.lines().filter(|line| line.contains("GITHUB_TOKEN")) {
        assert!(
            line.trim_start() == "GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}",
            "{RELEASE}: GITHUB_TOKEN appears only in that mapping, found `{line}`"
        );
    }
    forbid(RELEASE, &text, "xtrace", "a trace would print the token");
    forbid(
        RELEASE,
        &text,
        "gh auth",
        "the token stays in the environment, never written by a login",
    );
    forbid(
        RELEASE,
        &text,
        "curl -v",
        "a verbose curl prints request headers",
    );
}

#[test]
fn the_runbook_is_for_github_and_names_the_machinery_and_the_first_run() {
    let text = read(RUNBOOK);
    for name in [
        "scripts/build-release.sh",
        "examples/install-req.sh",
        ".github/workflows/release.yml",
        "gh release upload",
        "GITHUB_TOKEN",
        "contents: write",
        "REQ_RELEASE_TOKEN",
        "glibc",
        "-dirty",
        "## First run on GitHub",
        "## Install timing",
        "https://github.com/autox-my/req",
    ] {
        require(
            RUNBOOK,
            &text,
            name,
            "the runbook names every part of the release machinery and the owner's first-run checklist",
        );
    }
    for old in [
        "xantu-barb",
        ".ts.net",
        ".forgejo",
        "Forgejo",
        "install-timing.yml",
        "write:repository",
        "read:repository",
    ] {
        forbid(
            RUNBOOK,
            &text,
            old,
            "the runbook is for the GitHub repository; the old host is history (ADR 0029)",
        );
    }
    // The only token variable it names is the install's optional one, never a `RELEASE_TOKEN` secret.
    for (at, _) in text.match_indices("RELEASE_TOKEN") {
        assert!(
            text[..at].ends_with("REQ_"),
            "{RUNBOOK}: a bare RELEASE_TOKEN secret no longer exists (GITHUB_TOKEN is the credential)"
        );
    }
}

#[test]
fn agents_md_points_at_the_runbook() {
    let text = read("AGENTS.md");
    let line = text
        .lines()
        .find(|l| l.contains("docs/releasing.md"))
        .unwrap_or_else(|| {
            panic!("AGENTS.md must name docs/releasing.md under \"Where things live\"")
        });
    assert!(
        line.trim_start().starts_with("- "),
        "AGENTS.md: the pointer to docs/releasing.md is one bullet line: `{line}`"
    );
}
