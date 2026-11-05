//! `examples/github-workflows/req-diff.yml` — the copy-ready pull-request workflow for `req diff`
//! (ADR 0026, #123) — and the adopter page section that explains it.
//!
//! This is a **separate file from `req.yml`**: it is not one of the three pinned copies of that
//! snippet (`tests/ci_template.rs`), so nothing here weakens that equality. What is pinned is what an
//! adopter cannot afford to lose, from the failures this repository has already paid for:
//!
//! - the trigger is `pull_request` and nothing that runs fork code with secrets;
//! - the checkout is `fetch-depth: 0`, because the base revision must be in the checkout;
//! - the `req diff` command is unpiped and unmasked: its exit is the step's exit, and the job
//!   summary is written from a captured copy, never from a pipe;
//! - the install step is `examples/install-req.sh`'s body (modulo indentation), with none of the
//!   test-only overrides;
//! - the adopting page carries one section, after `## CI`, that names every class and flag and the
//!   same starting `--fail-on` set the workflow uses.
//!
//! Files are read at test time, not `include_str!`-ed: a missing file must fail **with its reason**.

use std::path::{Path, PathBuf};

const WORKFLOW: &str = "examples/github-workflows/req-diff.yml";
const INSTALL_STEP_NAME: &str = "req, installed from the prebuilt binary pinned below";
const NOT_COPIED_BEGIN: &str = "# >>> not copied into the CI step";
const NOT_COPIED_END: &str = "# <<< end of the part not copied into the CI step";

/// The starting set ADR 0026 §4 and the spec recommend: the movements a team most plausibly wants
/// to block a merge on, before it has chosen its own.
const STARTING_SET: &str = "weakened,removed,pins-changed,tests-lost";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_reporting(relative: &str, what: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "the {what} must exist and be readable ({}): {error}",
            path.display()
        )
    })
}

fn workflow() -> String {
    read_reporting(WORKFLOW, "copy-ready pull-request workflow")
}

/// The lines of the file that are code: not blank, not a comment. A comment may name anything.
fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .collect()
}

/// The code of the file, as one string (comments removed), for the "must not contain" pins.
fn code(text: &str) -> String {
    code_lines(text).join("\n")
}

/// The shell body (`run: |`) containing `anchor`, dedented: the bytes the runner hands to bash.
fn the_script_containing(text: &str, anchor: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let anchor_at = lines
        .iter()
        .position(|line| line.contains(anchor))
        .unwrap_or_else(|| panic!("the workflow has no line containing {anchor:?}"));
    let at = lines[..anchor_at]
        .iter()
        .rposition(|line| line.trim() == "run: |")
        .unwrap_or_else(|| panic!("{anchor:?} must live inside a `run: |` block"));
    let run_indent = lines[at].len() - lines[at].trim_start().len();
    let mut body: Vec<&str> = Vec::new();
    for line in &lines[at + 1..] {
        let blank = line.trim().is_empty();
        let indent = line.len() - line.trim_start().len();
        if !blank && indent <= run_indent {
            break;
        }
        body.push(line);
    }
    let dedent = body
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .expect("the run block has a body");
    let mut script = String::new();
    for line in body {
        if line.len() > dedent {
            script.push_str(&line[dedent..]);
        }
        script.push('\n');
    }
    script
}

/// `examples/install-req.sh` as a CI step carries it: no shebang, none of the not-copied block.
fn install_script_as_ci_body(script: &str) -> String {
    let mut body = String::new();
    let mut skipping = false;
    let mut closed = false;
    for (index, line) in script.lines().enumerate() {
        if index == 0 && line.starts_with("#!") {
            continue;
        }
        if line.starts_with(NOT_COPIED_BEGIN) {
            skipping = true;
        }
        if !skipping {
            body.push_str(line);
            body.push('\n');
        }
        if skipping && line.starts_with(NOT_COPIED_END) {
            skipping = false;
            closed = true;
        }
    }
    assert!(
        closed && !skipping,
        "examples/install-req.sh must delimit the part CI does not carry with {NOT_COPIED_BEGIN:?} \
         and {NOT_COPIED_END:?}"
    );
    body
}

/// The first differing line of two texts, named: "they differ" is not actionable.
fn assert_same_lines(left: &str, left_name: &str, right: &str, right_name: &str) {
    if left == right {
        return;
    }
    let (l, r): (Vec<&str>, Vec<&str>) = (left.lines().collect(), right.lines().collect());
    let at = (0..l.len().max(r.len()))
        .find(|&i| l.get(i) != r.get(i))
        .unwrap_or(0);
    panic!(
        "{left_name} and {right_name} differ, first on line {}:\n  {left_name}:  {:?}\n  {right_name}: {:?}",
        at + 1,
        l.get(at).unwrap_or(&"<ends>"),
        r.get(at).unwrap_or(&"<ends>"),
    );
}

// ---------------------------------------------------------------------------
// The workflow
// ---------------------------------------------------------------------------

/// The trigger is `pull_request`, and only it. `pull_request_target` runs with the base
/// repository's secrets against code the pull request supplies; `push` would have no base to
/// compare against. The question this file answers is a pull request's.
#[test]
fn the_trigger_is_pull_request_and_nothing_that_hands_secrets_to_fork_code() {
    let text = workflow();
    let code = code(&text);
    assert!(
        code.starts_with("name: req-diff\non:\n  pull_request:\njobs:\n"),
        "the workflow is named `req-diff` (not `req`: a second workflow of that name would collide with the \
         release check) and is triggered by `pull_request` alone:\n{}",
        text.lines().take(8).collect::<Vec<_>>().join("\n")
    );
    for forbidden in [
        "pull_request_target",
        "push:",
        "workflow_run",
        "issue_comment",
    ] {
        assert!(
            !code.contains(forbidden),
            "the workflow must not be triggered by {forbidden:?}: only `pull_request` has a base branch to \
             compare against without handing secrets to code the pull request supplies"
        );
    }
}

/// The base revision must be in the checkout: a shallow clone has no `origin/<base>` history to
/// read, and `req diff` would be exit 3 (UNVERIFIABLE) on every run.
#[test]
fn the_checkout_has_the_full_history() {
    let text = workflow();
    let lines = code_lines(&text);
    let at = lines
        .iter()
        .position(|line| line.trim() == "- uses: actions/checkout@v4")
        .expect("the workflow checks the repository out with actions/checkout@v4");
    let next = &lines[at + 1..];
    assert_eq!(
        next.first().map(|line| line.trim()),
        Some("with:"),
        "the checkout step carries a `with:` block"
    );
    assert_eq!(
        next.get(1).map(|line| line.trim()),
        Some("fetch-depth: 0"),
        "the checkout is `fetch-depth: 0` (the base revision must be in the checkout)"
    );
}

/// The one `req diff` command line.
fn the_diff_command(text: &str) -> String {
    let found: Vec<&str> = code_lines(text)
        .into_iter()
        .filter(|line| line.trim_start().starts_with("if req diff "))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one line runs `req diff` as the `if` condition of the step:\n{text}"
    );
    found[0].trim().to_string()
}

/// `req diff`'s exit is the step's exit. Every way a red turns green is named: a pipe (whose
/// status is the last command's), `|| true`, `continue-on-error`, `set +e`, a trailing command or
/// a literal `exit 0` that overwrites the status.
#[test]
fn the_diff_command_is_unpiped_and_unmasked() {
    let text = workflow();
    let command = the_diff_command(&text);

    let after_flag = command
        .strip_prefix("if req diff --base \"origin/${{ github.base_ref }}\" --fail-on ")
        .unwrap_or_else(|| {
            panic!(
                "the command compares the pull request's base branch against the checkout, with `--fail-on` \
                 named:\n{command}"
            )
        });
    let rest = after_flag
        .strip_suffix("; then")
        .unwrap_or_else(|| panic!("the command is the condition of an `if`:\n{command}"));
    let (list, redirect) = rest.split_once(' ').unwrap_or_else(|| {
        panic!("the command redirects its stdout to a captured copy:\n{command}")
    });
    assert_eq!(
        list, STARTING_SET,
        "the workflow starts from the recommended `--fail-on` set"
    );
    assert!(
        redirect.starts_with("> \"$")
            && redirect.ends_with('"')
            && redirect.matches('>').count() == 1,
        "stdout goes to one captured copy by a plain redirect (a pipe's status would replace req's):\n{redirect}"
    );
    for masked in ["|", "tee", "true", "&&", ";", "2>&1"] {
        assert!(
            !redirect.contains(masked),
            "the `req diff` line must not carry {masked:?}: its status is the step's:\n{command}"
        );
    }

    // Anywhere in the file: the switches that discard a failed step's status.
    let code = code(&text);
    for masked in ["continue-on-error", "|| true", "set +e"] {
        assert!(
            !code.contains(masked),
            "the workflow must not carry {masked:?}: it would turn a red into a green:\n{code}"
        );
    }

    // In the `req diff` step itself (the install body has its own, legitimate `||`): nothing
    // that replaces or overrides the status of the command.
    let script = the_script_containing(&text, "if req diff ");
    for masked in ["||", "pipefail", "exit 0", ":-0}", "tee"] {
        assert!(
            !script.contains(masked),
            "the `req diff` step must not carry {masked:?}: it would turn a red into a green:\n{script}"
        );
    }
    let last = script
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    assert_eq!(
        last, "exit \"$code\"",
        "the step's last command is `exit \"$code\"`: nothing after `req diff` may set the status"
    );
    assert!(
        script.contains("then\n  code=0\nelse\n  code=$?\nfi"),
        "`code` is `req diff`'s own status, taken from the `if` condition:\n{script}"
    );
    assert!(
        !script
            .lines()
            .any(|line| line.trim_start().starts_with("code=")
                && !line.contains("code=0")
                && !line.contains("code=$?")),
        "`code` is assigned only 0 (the command passed) or `$?` (its status):\n{script}"
    );
}

/// The report is also written to the job summary, from the captured copy and before the exit.
#[test]
fn the_job_summary_is_written_from_the_captured_copy_before_the_exit() {
    let text = workflow();
    let script = the_script_containing(&text, "if req diff ");
    let at = |needle: &str| {
        script
            .find(needle)
            .unwrap_or_else(|| panic!("the step must contain {needle:?}:\n{script}"))
    };
    assert!(
        script.contains("out=$(mktemp)"),
        "the captured copy is a fresh temporary file:\n{script}"
    );
    assert!(
        at("GITHUB_STEP_SUMMARY") < at("exit \"$code\""),
        "the summary is written before the step exits with req's status"
    );
    assert!(
        at("if req diff ") < at("GITHUB_STEP_SUMMARY"),
        "the summary is written after `req diff` ran, from what it printed"
    );
    assert!(
        script.contains("cat \"$out\""),
        "the captured report is also printed to the log:\n{script}"
    );
    // Nothing after the `if ... fi` runs `req` again: the summary is written from the captured
    // copy, never by running `req diff` a second time (which would also be a second exit status).
    let after_fi = &script[at("\nfi\n")..];
    assert!(
        !after_fi.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("req ") || line.contains("$(req ") || line.contains("`req ")
        }),
        "the summary is written from the captured copy, never by running `req` a second time:\n{after_fi}"
    );
    // The step has no `if:` that could skip it.
    let step_condition = code(&text)
        .lines()
        .any(|line| line.trim_start().starts_with("if:"));
    assert!(
        !step_condition,
        "no step of this workflow has an `if:`: a skipped `req diff` is a green build that compared nothing"
    );
}

/// The install step is the install script: not a paraphrase of it. Same discipline as
/// `tests/ci_template.rs`, applied to this file.
#[test]
fn the_install_step_is_the_install_scripts_body_modulo_indentation() {
    let script = read_reporting("examples/install-req.sh", "install script");
    let expected = install_script_as_ci_body(&script);
    let text = workflow();
    let body = the_script_containing(&text, "SHA256_x86_64_unknown_linux_gnu=");
    assert_same_lines(
        &body,
        &format!("{WORKFLOW}'s install step"),
        &expected,
        "examples/install-req.sh (shebang and the not-copied block removed)",
    );
    assert!(
        text.contains(INSTALL_STEP_NAME),
        "the install step keeps its name, {INSTALL_STEP_NAME:?}"
    );
    assert!(
        text.contains("secrets.REQ_RELEASE_TOKEN"),
        "the release token comes from a secret, in the install step's `env:`"
    );
}

/// The install is a step that fails the job, and it comes before the step that runs `req`.
#[test]
fn the_install_comes_first_and_cannot_mask_a_failure() {
    let text = workflow();
    let code = code(&text);
    let position = |needle: &str| {
        code.find(needle)
            .unwrap_or_else(|| panic!("the workflow has no line containing {needle:?}"))
    };
    assert!(
        position(INSTALL_STEP_NAME) < position("if req diff "),
        "the install runs before the step that calls `req`"
    );
    assert!(
        position("GITHUB_PATH") < position("if req diff "),
        "the installed req is put on PATH before the step that calls it"
    );
    assert!(
        position("actions/checkout@v4") < position(INSTALL_STEP_NAME),
        "the checkout comes first"
    );
}

/// What the test-only overrides and the source install must not leak into an adopter's file.
#[test]
fn no_test_only_override_and_no_source_install() {
    let text = workflow();
    for forbidden in [
        "REQ_RELEASE_BASE",
        "REQ_CURL_PROTO",
        "set -x",
        "cargo install --git",
        "REQ_DEPLOY_KEY",
    ] {
        assert!(
            !text.contains(forbidden),
            "{WORKFLOW} must not contain {forbidden:?}: a test-only override or the retired source install"
        );
    }
}

// ---------------------------------------------------------------------------
// The adopter page
// ---------------------------------------------------------------------------

/// The section of `docs/adopting.md` headed `## On a pull request`, without the heading.
fn the_pull_request_section(markdown: &str) -> &str {
    const HEADING: &str = "\n## On a pull request\n";
    let start = markdown
        .find(HEADING)
        .unwrap_or_else(|| panic!("docs/adopting.md has no `## On a pull request` section"))
        + HEADING.len();
    let rest = &markdown[start..];
    &rest[..rest.find("\n## ").unwrap_or(rest.len())]
}

/// The section comes after `## CI` and before the next top-level section after it, and exists once.
#[test]
fn the_adopting_page_has_one_pull_request_section_after_ci() {
    let doc = read_reporting("docs/adopting.md", "adopting page");
    assert_eq!(
        doc.matches("\n## On a pull request\n").count(),
        1,
        "exactly one `## On a pull request` section"
    );
    let ci = doc
        .find("\n## CI\n")
        .expect("the page has a `## CI` section");
    let here = doc
        .find("\n## On a pull request\n")
        .expect("the page has the section");
    let coverage = doc
        .find("\n## The coverage rule\n")
        .expect("the page has `## The coverage rule`");
    assert!(
        ci < here && here < coverage,
        "the section sits after `## CI` and before `## The coverage rule`"
    );
}

/// The classes and flags ADR 0026 §2 decides, each named in the section as code. The list is
/// hand-written from the ADR, not read out of the command's own help.
#[test]
fn the_section_names_every_class_and_flag() {
    let doc = read_reporting("docs/adopting.md", "adopting page");
    let section = the_pull_request_section(&doc);
    for word in [
        "added",
        "removed",
        "weakened",
        "tightened",
        "reshaped",
        "not-judged",
        "reworded",
        "metadata",
        "pins-changed",
        "pin-flagged",
        "tests-lost",
        "tests-added",
        "same-address-as",
        "registry: changed",
    ] {
        assert!(
            section.contains(&format!("`{word}")),
            "the section must name `{word}` as code: it is a class or flag of `req diff` (ADR 0026 §2)"
        );
    }
}

/// What the section must say, each a way an adopter is otherwise misled.
#[test]
fn the_section_states_what_an_adopter_must_not_misread() {
    let doc = read_reporting("docs/adopting.md", "adopting page");
    let section = the_pull_request_section(&doc);
    let starting = format!("`{STARTING_SET}`");
    for (needle, why) in [
        (
            starting.as_str(),
            "the recommended starting `--fail-on` set as prose (the same one the workflow starts from)",
        ),
        (
            "examples/github-workflows/req-diff.yml",
            "the copy-ready workflow: the page points at it and does not carry a fourth copy",
        ),
        (
            "fetch-depth: 0",
            "the base revision must be in the checkout, so a shallow clone is UNVERIFIABLE",
        ),
        ("`git`", "the command runs the local `git` binary"),
        (
            "not a verdict",
            "the output is a description of a change, not a release verdict (`req check` is that)",
        ),
        (
            "prompt for a human",
            "`weakened` is a prompt for a human, never a rejection by the tool",
        ),
        (
            "exactly as good as `req compare`'s",
            "the direction quality is exactly `req compare`'s, in the same breath",
        ),
        (
            "aliases/subjects.yml",
            "the registry-change caveat: one edit there can move many addresses at once",
        ),
        (
            "Nothing is on by default",
            "no class blocks a merge until the team names it",
        ),
    ] {
        assert!(
            section.contains(needle),
            "the `## On a pull request` section must say {needle:?}: {why}"
        );
    }
    assert!(
        !section.contains("```yaml"),
        "the section points at the example file and carries no fenced copy of it: a fourth copy of a \
         workflow is drift waiting for a reader"
    );
}
