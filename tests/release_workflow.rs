//! String pins on the release machinery of ADR 0025 (unit 65-3 of `docs/specs/65-prebuilt-binary.md`):
//! `.forgejo/workflows/release.yml`, `.forgejo/workflows/install-timing.yml` and `docs/releasing.md`.
//!
//! Neither workflow can be run from a test (no runner, no secret, and publishing a release is the
//! owner's act), so what is checked is what a text can show and what a careless edit would silently
//! lose: the **triggers** (a release is cut on request, never by a push, a pull request, a schedule
//! or another workflow), the absence of anything that turns a failure into a pass, the **no-overwrite**
//! rule of §5 (a checksum that has been pinned must stay true), that the Linux tarball comes out of
//! `scripts/build-release.sh` (the script that proves a binary before it writes one) and that the
//! token never reaches a command line.
//!
//! Pins read the file with whole-line comments removed, so a comment may say "no `continue-on-error`"
//! without tripping the pin on `continue-on-error`. The pins are text, not a parse of YAML, and say
//! so: they catch the edits this repository has already had to take back, not every possible one.
//! Each one has been mutation-checked (the change that must turn it red is in the unit's envelope).

use std::path::{Path, PathBuf};

const RELEASE: &str = ".forgejo/workflows/release.yml";
const TIMING: &str = ".forgejo/workflows/install-timing.yml";
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

#[test]
fn both_workflows_are_dispatch_only() {
    for rel in [RELEASE, TIMING] {
        assert_eq!(
            trigger_keys(rel),
            vec!["workflow_dispatch".to_owned()],
            "{rel}: a release is cut and a timing is taken on request — `workflow_dispatch` is the only \
             trigger (no push, pull_request, schedule, release or workflow_run)"
        );
    }
}

#[test]
fn the_release_takes_a_rev_and_the_timing_takes_a_rev_and_a_hash() {
    assert_eq!(
        input_names(RELEASE),
        vec!["rev".to_owned()],
        "{RELEASE}: exactly one input, `rev`"
    );
    assert_eq!(
        input_names(TIMING),
        vec!["rev".to_owned(), "sha256".to_owned()],
        "{TIMING}: inputs `rev` and `sha256`, in that order"
    );
}

#[test]
fn no_step_turns_a_failure_into_a_pass() {
    for rel in [RELEASE, TIMING] {
        let text = code(rel);
        forbid(
            rel,
            &text,
            "continue-on-error",
            "a release or a measurement that fails must fail the run",
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
        forbid(
            rel,
            &text,
            "| tee",
            "the pipeline's status is tee's, not the command's (AGENTS.md)",
        );
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
    forbid(
        RELEASE,
        &text,
        "git.xantu-barb.ts.net",
        "the public tailnet name does not resolve on the runner: the API base is the runner's server URL",
    );
    require(
        RELEASE,
        &text,
        "${{ github.server_url }}/api/v1/repos/${{ github.repository }}",
        "the API base is the URL this runner reaches Forgejo at",
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
}

#[test]
fn the_upload_cannot_overwrite_an_asset() {
    let text = code(RELEASE);
    // It looks before it writes, and a name that is already there is an error, not a replacement.
    require(
        RELEASE,
        &text,
        "/assets\"",
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
        "-F \"attachment=",
        "the upload form the ordering below is measured against",
    );
    let first_upload = text.find("-F \"attachment=").unwrap_or(usize::MAX);
    assert!(
        refusal < first_upload,
        "{RELEASE}: the existing-asset check must come before the first upload, not after it"
    );
    require(
        RELEASE,
        &text,
        "-X POST",
        "assets are created, with the one verb that cannot replace a named one",
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
    for token in [
        "|| :",
        "set -ex",
        "--request=",
        "|| exit 0",
        "set -x",
        "set -eux",
        "set -euxo",
        "curl -v",
        "--verbose",
        "--trace",
    ] {
        forbid(
            RELEASE,
            &text,
            token,
            "a spelling that deletes or replaces an asset, ends the step green after a failure, or prints the Authorization header",
        );
    }
    for verb in [
        "-X PUT",
        "-X PATCH",
        "-X DELETE",
        "--request PUT",
        "--request PATCH",
        "--request DELETE",
    ] {
        forbid(
            RELEASE,
            &text,
            verb,
            "an asset that has been pinned somewhere must stay true: no replace, edit or delete",
        );
    }
    forbid(
        RELEASE,
        &text,
        "--upload-file",
        "that is a PUT; assets are created with a POST",
    );
    forbid(
        RELEASE,
        &text,
        "-T ",
        "that is a PUT; assets are created with a POST",
    );
}

#[test]
fn the_release_credential_is_one_secret_and_never_on_a_command_line() {
    let text = code(RELEASE);
    require(
        RELEASE,
        &text,
        "${{ secrets.RELEASE_TOKEN }}",
        "the credential is one repository secret, the owner's to provision",
    );
    require(
        RELEASE,
        &text,
        "-H @-",
        "the header is read by curl from stdin, so the token is not in an argument list",
    );
    forbid(RELEASE, &text, "set -x", "a trace would print the token");
    forbid(RELEASE, &text, "xtrace", "a trace would print the token");
    forbid(
        RELEASE,
        &text,
        "curl -v",
        "a verbose curl prints request headers",
    );
    forbid(
        RELEASE,
        &text,
        "--verbose",
        "a verbose curl prints request headers",
    );
    // Every line that mentions the token is the secret's mapping, the guard that it is set, or the
    // one `printf` that feeds the header to curl's stdin. Anything else is a way for it to leak.
    for line in text.lines().filter(|l| l.contains("RELEASE_TOKEN")) {
        let allowed = line
            .trim_start()
            .starts_with("RELEASE_TOKEN: ${{ secrets.RELEASE_TOKEN }}")
            || line.contains("-z \"${RELEASE_TOKEN:-}\"")
            || line.contains("printf 'Authorization: token %s\\n' \"$RELEASE_TOKEN\"")
            || line.contains("::error::RELEASE_TOKEN");
        assert!(
            allowed,
            "{RELEASE}: a line mentions the token outside the places that are safe: `{line}`"
        );
    }
}

#[test]
fn the_timing_measures_both_installs_and_prints_two_lines() {
    let text = code(TIMING);
    require(
        TIMING,
        &text,
        "date +%s",
        "each install is timed between two readings of the clock",
    );
    require(
        TIMING,
        &text,
        "cargo install --path",
        "the source install is one of the two things measured (after the authenticated clone)",
    );
    require(
        TIMING,
        &text,
        "!= refs/heads/main",
        "the read token goes only to main's install script, the one an adopter gets",
    );
    require(
        TIMING,
        &text,
        "[ -z \"$built\" ]",
        "an unestablished identity prints no figure: an empty `built from` must not match every rev",
    );
    require(
        TIMING,
        &text,
        "unset GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 REQ_RELEASE_TOKEN",
        "the read token is dropped from the environment before any build script of a fetched crate can see it",
    );
    require(
        TIMING,
        &text,
        "--locked",
        "the source install is the documented one, with the committed lockfile",
    );
    require(
        TIMING,
        &text,
        "examples/install-req.sh",
        "the prebuilt install is the copy-ready script, not a reimplementation of it",
    );
    require(
        TIMING,
        &text,
        "INSTALL source: ",
        "the first of the two lines the orchestrator copies",
    );
    require(
        TIMING,
        &text,
        "INSTALL prebuilt: ",
        "the second of the two lines the orchestrator copies",
    );
    require(
        TIMING,
        &text,
        "${{ secrets.REQ_RELEASE_TOKEN }}",
        "the prebuilt install reads its token from REQ_RELEASE_TOKEN, as an adopter's CI does",
    );
    // The install script's two test-only overrides are for tests; a measurement that set one would
    // not be measuring what an adopter runs.
    // The runner cannot resolve the public host, so the timing points the script at the runner's own
    // server URL through the two overrides; but only ever at that URL, never at a literal host.
    require(
        TIMING,
        &text,
        "REQ_RELEASE_BASE=\"${GITHUB_SERVER_URL%/}/${GITHUB_REPOSITORY}/releases/download\"",
        "the override is derived from the runner's own server URL, not typed",
    );
    forbid(
        TIMING,
        &text,
        "git.xantu-barb.ts.net",
        "the public tailnet name does not resolve on the runner: use the server URL",
    );
}

#[test]
fn the_runbook_exists_and_names_both_scripts_and_both_workflows() {
    let text = read(RUNBOOK);
    for name in [
        "scripts/build-release.sh",
        "examples/install-req.sh",
        ".forgejo/workflows/release.yml",
        ".forgejo/workflows/install-timing.yml",
    ] {
        require(
            RUNBOOK,
            &text,
            name,
            "the runbook names every part of the release machinery",
        );
    }
    require(
        RUNBOOK,
        &text,
        "## Measured",
        "the runbook has the section the timing is copied into",
    );
    for word in [
        "read:repository",
        "write:repository",
        "RELEASE_TOKEN",
        "glibc",
        "-dirty",
    ] {
        require(
            RUNBOOK,
            &text,
            word,
            "a rule or a scope the owner needs on the page",
        );
    }
}

#[test]
fn the_measured_section_holds_no_number_until_the_owner_dispatches_the_timing() {
    // No child writes a timing into a document (the spec): the orchestrator copies the two `INSTALL`
    // lines and the run URL after the owner dispatches `install-timing.yml`. Until then the section
    // is an instruction, and an instruction here has no number in it: no word starting with a digit
    // ("42s", "7", "1.5"). A digit inside a name (`sha256`) is not a number.
    let text = read(RUNBOOK);
    let start = text
        .find("\n## Measured")
        .unwrap_or_else(|| panic!("{RUNBOOK} has no `## Measured` section"))
        + 1;
    let rest = &text[start..];
    let section = &rest[..rest[3..].find("\n## ").map_or(rest.len(), |at| at + 3)];
    let numeric = section
        .split(|c: char| c.is_whitespace() || "`'\"(),:;<>[]".contains(c))
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()));
    assert!(
        numeric.is_none(),
        "the Measured section must hold no number until the timing has been dispatched and copied \
         (found `{}`):\n{section}",
        numeric.unwrap_or("")
    );
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
