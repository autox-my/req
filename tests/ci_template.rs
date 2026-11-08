//! The CI snippet is **one text in three places**, and this target is what keeps it one text.
//!
//! 1. what `req init` prints — `req::init::CI_SNIPPET`, the string an adopter's runner runs;
//! 2. the fenced ```` ```yaml ```` block in `docs/adopting.md`, the page a human reads;
//! 3. `examples/github-workflows/req.yml`, the file an adopter `cp`s.
//!
//! Three copies of the same advice is exactly the drift this repository exists to prevent, so
//! the equality is **asserted** rather than left to review. `cargo test` is the gate that runs
//! it — no new script — and the failure names the first differing byte, because "they differ"
//! is not actionable while "line 41, `--rev <PIN>` vs `--rev <SHA>`" is.
//!
//! The files are read at test time, not `include_str!`-ed: a missing copy must fail this test
//! **with its reason** (a check that cannot fail is not a check), and a compile error would
//! report it as a build problem instead.
//!
//! The load-bearing fragments are pinned too. Byte equality alone would keep three copies of a
//! *gutted* snippet honest with each other; these are the parts an adopter cannot afford to
//! lose, from the failures this repository has already paid for.

use std::path::{Path, PathBuf};

/// The worktree (or checkout) this test binary was compiled from.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Read a file, naming the path when the read fails: "the copy is missing" is a failure of
/// the three-way equality, and it must say which file it could not find.
fn read_reporting(path: &Path, what: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "the {what} must exist and be readable ({path}) — it is one of the three copies \
             of the CI snippet this test exists to keep identical: {error}",
            path = path.display()
        )
    })
}

/// The one **fenced YAML block** in the adopting page's `## CI` section, without the fence.
///
/// Scoped to that section on purpose: the guide is long and may legitimately grow another yaml
/// example later, which is not this test's business. Inside the section, "more than one" is
/// still a failure — this test compares *the* CI snippet block, and a second one would let it
/// compare the wrong text and pass.
fn the_ci_yaml_block<'a>(markdown: &'a str, source: &str) -> &'a str {
    const HEADING: &str = "\n## CI\n";
    const OPEN: &str = "```yaml\n";
    let after_heading = markdown
        .find(HEADING)
        .unwrap_or_else(|| panic!("{source} has no `## CI` section; there is nothing to compare"))
        + HEADING.len();
    let rest = &markdown[after_heading..];
    let section = &rest[..rest.find("\n## ").unwrap_or(rest.len())];
    let mut blocks = section.match_indices(OPEN);
    let first = blocks.next().unwrap_or_else(|| {
        panic!("{source}'s `## CI` section has no ```yaml block; there is nothing to compare")
    });
    assert!(
        blocks.next().is_none(),
        "{source}'s `## CI` section has more than one ```yaml block; this test compares *the* \
         CI snippet block, and a second one would let it compare the wrong text and pass"
    );
    let after = &section[first.0 + OPEN.len()..];
    let close = after
        .find("\n```")
        .unwrap_or_else(|| panic!("{source}'s ```yaml block is not closed with ```"));
    // `close + 1` keeps the newline before the closing fence: the snippet's own body ends with
    // one, and dropping it would make an identical text look different.
    &after[..close + 1]
}

/// Fail with the first differing byte, its line, and the two lines — the comparison a human
/// can act on. Byte equality is the claim; the *reason* is what a red run must produce.
fn assert_same_bytes(left: &str, left_name: &str, right: &str, right_name: &str) {
    if left == right {
        return;
    }
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let shorter = left_bytes.len().min(right_bytes.len());
    let at = (0..shorter)
        .find(|&i| left_bytes[i] != right_bytes[i])
        .unwrap_or(shorter);
    // The byte offset may fall inside a multi-byte character; counting newlines over the byte
    // prefix keeps the line number right without slicing at a non-char boundary.
    let line = left_bytes[..at].iter().filter(|&&b| b == b'\n').count() + 1;
    let line_of = |text: &str| {
        text.lines()
            .nth(line - 1)
            .unwrap_or("<line absent>")
            .to_string()
    };
    if at == shorter {
        panic!(
            "{left_name} and {right_name} are not byte-identical: one ends at byte {at} \
             ({left_len} vs {right_len} bytes, first difference on line {line})\n  \
             {left_name} line:  {:?}\n  {right_name} line: {:?}",
            line_of(left),
            line_of(right),
            left_len = left_bytes.len(),
            right_len = right_bytes.len(),
        );
    }
    panic!(
        "{left_name} and {right_name} are not byte-identical: first difference at byte {at} \
         (line {line})\n  {left_name} line:  {:?}\n  {right_name} line: {:?}",
        line_of(left),
        line_of(right),
    );
}

/// The three copies are one text. See the module doc for why each leg is a separate assertion:
/// the two files a human can edit without a compiler are the ones most likely to drift.
#[test]
fn the_ci_snippet_has_one_text_in_three_places() {
    let printed = req::init::CI_SNIPPET;

    let doc_path = repo_root().join("docs/adopting.md");
    let doc = read_reporting(&doc_path, "adopting doc");
    let doc_block = the_ci_yaml_block(&doc, "docs/adopting.md");

    let example_path = repo_root().join("examples/github-workflows/req.yml");
    let example = read_reporting(&example_path, "copy-ready example");

    // Order matters: the two files a human edits without a compiler are compared FIRST, so a
    // drift between them names those two paths. Asserted last it could never fire — the two
    // comparisons with `printed` would already have failed — which is a check that cannot fail.
    assert_same_bytes(
        doc_block,
        "docs/adopting.md's ```yaml block",
        &example,
        "examples/github-workflows/req.yml",
    );
    assert_same_bytes(
        printed,
        "req init's printed snippet",
        doc_block,
        "docs/adopting.md's ```yaml block",
    );
    assert_same_bytes(
        printed,
        "req init's printed snippet",
        &example,
        "examples/github-workflows/req.yml",
    );

    // The snippet must still *be* the snippet. Byte equality would keep three copies of a
    // gutted text honest with each other, so each fragment below is pinned; each is a failure
    // this repository has already paid for. Fragments that legitimately occur more than once
    // are pinned by **count**: a `contains` check on `if: always()` survives deleting one of
    // the two, which is exactly the mutation the pin exists to catch.
    for (fragment, occurrences, why) in [
        (
            "req canon --verify",
            0,
            "removed in req.claim/2 (ADR 0022; the template must not even mention it, by design: \
             the pin is on the words): nothing derived is stored, so there is nothing to \
             verify — a step still running it exits 2 and reds every adopter's CI; the pins it \
             gave way to are `req gate`'s `pins_hold`",
        ),
        (
            "if req check --test-report \"${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml\" > \"$out\" 2>&1; then",
            1,
            "the release step IS `req check` (ADR 0023): one command, the report it reads, its \
             output to a file, and no pipe — a pipe would report the last command's status, and \
             an exit 3 would go green",
        ),
        (
            "exit \"$code\"",
            1,
            "the step's status is `req check`'s own status: the step ends in the code it captured, \
             so an UNVERIFIABLE (3) is a red step and never a green one",
        ),
        (
            "else\n            code=$?\n          fi",
            1,
            "the status is captured from the `if` condition itself (`$?` in the `else` branch is \
             the condition's status), not from a later command that would overwrite it",
        ),
        // The shell this step used to carry, now req's (ADR 0023): each of these coming back
        // means a copy re-implements the loop or the fold, which `req check` exists to prevent.
        (
            "[ -r .req/variants ]",
            0,
            "the readability assertion belongs to `req check` now (an unreadable list is exit 3 \
             there, tested once); a copy that asserts it again is re-implementing the reader",
        ),
        (
            "exit $rc",
            0,
            "the sticky exit fold belongs to `req check` now (`fold` in src/check.rs)",
        ),
        (
            "read -r -a",
            0,
            "a shell loop over the release list: the list grammar is `req check`'s now",
        ),
        (
            "done < .req/variants",
            0,
            "a shell loop over `.req/variants`: `req check` reads it, validated before anything \
             runs",
        ),
        (
            "variants_read",
            0,
            "the old artifact step's list bookkeeping: there is one `req check` document now",
        ),
        // The install (ADR 0025, #65): the prebuilt, checksummed binary, pinned in the adopter's own file.
        (
            "req, installed from the prebuilt binary pinned below",
            1,
            "the install step by name: the pinned rev and the pinned SHA-256 per target are the \
             adopter's authority (ADR 0025 §3), and the step that checks them is this one",
        ),
        (
            "secrets.REQ_RELEASE_TOKEN",
            1,
            "the optional read token of a private fork comes from a secret, in this step's `env:` and \
             nowhere on a command line (an unset secret is an empty value, and sends no header)",
        ),
        (
            "REQ_RELEASE_TOKEN is not set",
            0,
            "the refusal for a missing token is retired (ADR 0029 §5): a public release needs no \
             credential, and a template that demanded one would fail every adopter's fork PR",
        ),
        (
            "if [ -n \"${REQ_RELEASE_TOKEN:-}\" ]; then printf 'Authorization: token %s\\n' \"$REQ_RELEASE_TOKEN\"; fi",
            1,
            "the Authorization header exists only when a token is set and non-empty: an always-sent \
             header would hand a public host a credential-shaped string for nothing",
        ),
        (
            "https://github.com/autox-my/req/releases/download",
            1,
            "the default download base is the public GitHub release (ADR 0029 §5)",
        ),
        (
            "xantu-barb",
            0,
            "the private host is not written into an adopter's file (ADR 0029)",
        ),
        (
            "FORGEJO",
            0,
            "no Forgejo variable: the summary target is GitHub's",
        ),
        ("forgejo", 0, "no Forgejo workflow directory is offered"),
        (
            "SHA-256 mismatch",
            1,
            "the tarball is verified against the PIN: a mismatch is a named, non-zero refusal",
        ),
        (
            "exactly one member",
            1,
            "the archive is checked to hold exactly one member, `req`, before anything is extracted",
        ),
        (
            "no automatic fallback",
            1,
            "the refusal says there is no fallback to the source install: falling back on a \
             mismatch would hide the event the check exists to surface (ADR 0025 §4)",
        ),
        (
            "--tlsv1.2",
            1,
            "the download is https-only, TLS 1.2 or later (ADR 0025 §6)",
        ),
        (
            "-H @-",
            1,
            "the Authorization header is read from stdin: the token is never an argument `ps` shows",
        ),
        (
            "echo \"${REQ_INSTALL_DIR:-$HOME/.local/bin}\" >> \"$GITHUB_PATH\"",
            1,
            "the PATH step adds the directory the install used (REQ_INSTALL_DIR, else ~/.local/bin), not a hardcoded one",
        ),
        // What the old source install carried and the new one must not: the deploy key, the ssh
        // options, the compile, and the two test-only overrides of the install script.
        (
            "REQ_DEPLOY_KEY",
            0,
            "the SSH deploy key was for req's source install only; the token replaces it",
        ),
        (
            "req_deploy_key",
            0,
            "no key file is written, so there is nothing to remove under `if: always()`",
        ),
        (
            "IdentitiesOnly",
            0,
            "no ssh identity is offered to anything",
        ),
        ("umask 077", 0, "no secret is written to a file"),
        (
            "cargo install --git",
            0,
            "req is not compiled from source in the template (that path stays in docs/adopting.md)",
        ),
        (
            "--rev <PIN>",
            0,
            "the source install's pin grammar is not in the CI template",
        ),
        (
            "REQ_RELEASE_BASE",
            0,
            "a test-only override of the install script: no shipped YAML sets it",
        ),
        (
            "REQ_CURL_PROTO",
            0,
            "a test-only override of the install script: no shipped YAML sets it",
        ),
        (
            "set -x",
            0,
            "a trace would print the header the token rides in",
        ),
        (
            "req \"$@\" --format json",
            1,
            "the one capture helper: every view goes through the same redirect-and-record path, so \
             an empty document can never be written",
        ),
        ("capture render render", 1, "the spec view document"),
        ("capture trace trace", 1, "the traceability document"),
        ("capture audit audit", 1, "the audit packet document"),
        (
            "capture check check --test-report \"${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml\"",
            1,
            "the ONE `req.check/1` document (the folded verdict and every step's `req.gate/2`), \
             reading the same report as the check step: without it the document says \
             `unverifiable` beside a green check",
        ),
        (
            "does not declare [profile.ci], [profile.ci.junit] and path",
            1,
            "the template verifies the nextest profile it depends on instead of assuming it: an \
             adopter's own config without the profile makes the gate read a missing report",
        ),
        (
            "cargo nextest run --profile ci",
            1,
            "the suite whose JUnit report the gate reads: without it `--test-report` has nothing \
             to read and the gate is UNVERIFIABLE, not green",
        ),
        (
            "cargo install cargo-nextest --version 0.9.149 --locked",
            1,
            "the pinned test runner (ADR 0019): an unpinned install would let the artefact's shape \
             drift, exactly as an unpinned `ocr` would let the review's",
        ),
        (
            "report-skipped = \"ignored\"",
            1,
            "a skipped test is present in the artefact — it is not a pass, and a report that \
             silently omits it would read as one",
        ),
        (
            "--test-report \"${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml\"",
            2,
            "the check step and the `req.check/1` capture read the same artefact: a document \
             captured without it was the review's finding — `unverifiable` beside a green check",
        ),
        (
            "expected=4",
            1,
            "the view set is render, trace, audit and check: the step knows how many documents it \
             expects, so a lost one is red",
        ),
        (
            "if: always()",
            4,
            "every step that must run on a red build: the invalidation of a stale report (before the \
             run, so a skipped step cannot leave yesterday's), the capture and the upload — plus the \
             comment that names the property",
        ),
        (
            "if-no-files-found: error",
            2,
            "an upload that finds nothing must be red: a green step with no artifact is the \
             silent-loss case this pack exists to remove",
        ),
        (
            "actions/upload-artifact@v4",
            1,
            "the upload: without it the documents die with the runner and no reviewing agent ever \
             reads them",
        ),
        (
            "incomplete view set",
            1,
            "a PARTIAL document set must be red: `if-no-files-found` only notices a completely \
             empty directory, so one lost document would ship silently",
        ),
    ] {
        let found = printed.matches(fragment).count();
        assert!(
            found == occurrences,
            "req init's CI snippet carries {fragment:?} {found} time(s), expected {occurrences} \
             — {why}"
        );
    }
}

// ---------------------------------------------------------------------------
// The release step is `req check` (ADR 0023): unpiped, unmasked, and the fold is not re-derived
// ---------------------------------------------------------------------------

/// The step blocks of the snippet: `(name line, body)` per `- name:` item, the body being every
/// following line up to the next item at the same indent. Positional, so a step cannot be read
/// out of a comment that merely mentions it.
fn the_steps(snippet: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = snippet.lines().collect();
    let mut steps = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim_start().starts_with("- name:") || line.trim_start().starts_with("- uses:") {
            let indent = line.len() - line.trim_start().len();
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j];
                let next_indent = next.len() - next.trim_start().len();
                let comment = next.trim_start().starts_with('#');
                // The next list item, or a comment block that introduces it, ends this step.
                if !next.trim().is_empty()
                    && next_indent <= indent
                    && (comment || next.trim_start().starts_with("- "))
                {
                    break;
                }
                j += 1;
            }
            steps.push((line.trim().to_string(), lines[i..j].join("\n")));
            i = j;
        } else {
            i += 1;
        }
    }
    steps
}

/// The one step that runs `req check`, whole (its keys and its `run:` body).
fn the_check_step(snippet: &str) -> String {
    let found: Vec<String> = the_steps(snippet)
        .into_iter()
        .filter(|(_, body)| body.contains("if req check --test-report"))
        .map(|(_, body)| body)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one step runs `req check` (the release verdict); found {}:\n{snippet}",
        found.len()
    );
    found.into_iter().next().unwrap_or_default()
}

/// The shell body (`run: |`) of the step that contains `anchor`, dedented — the bytes an
/// adopter's runner hands to bash.
fn the_script_containing(snippet: &str, anchor: &str) -> String {
    let lines: Vec<&str> = snippet.lines().collect();
    let anchor_at = lines
        .iter()
        .position(|line| line.contains(anchor))
        .unwrap_or_else(|| panic!("the snippet has no line containing {anchor:?}"));
    let at = lines[..anchor_at]
        .iter()
        .rposition(|line| line.trim() == "run: |")
        .expect("the anchor lives inside a `run: |` block");
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

/// The `req check` step's status is `req check`'s status. Each assertion is a way an exit 3 (or
/// an exit 1) turns into a green step: a pipe reports the last command, `|| true` and `continue-on-error`
/// discard it, `set +e` and a trailing command overwrite it. The probe below *runs* the step; this
/// pins the shape so the cheaper mutations fail here first, by name.
#[test]
fn the_check_step_is_unpiped_and_unmasked() {
    let snippet = req::init::CI_SNIPPET;
    let step = the_check_step(snippet);

    let command: Vec<&str> = step
        .lines()
        .filter(|line| line.trim_start().starts_with("if req check --test-report"))
        .collect();
    assert_eq!(command.len(), 1, "one `req check` command line:\n{step}");
    for masked in ["|", "tee", "true", "&&", ";;"] {
        let line = command[0].trim().trim_end_matches("; then");
        assert!(
            !line.contains(masked),
            "the `req check` line must not carry {masked:?} — its status is the step's:\n{line}"
        );
    }
    for masked in [
        "continue-on-error",
        "|| true",
        "set +e",
        "pipefail",
        "exit 0",
        ":-0}",
    ] {
        assert!(
            !step.contains(masked),
            "the check step must not carry {masked:?}: it would turn a red into a green:\n{step}"
        );
    }

    let script = the_script_containing(snippet, "if req check --test-report");
    let last = script
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    assert_eq!(
        last, "exit \"$code\"",
        "the step's last command is `exit \"$code\"` — nothing after `req check` may set the status"
    );
    assert!(
        script.contains("else\n  code=$?\nfi"),
        "`code` is `req check`'s own status, taken from the `if` condition:\n{script}"
    );
    assert!(
        !script
            .lines()
            .any(|line| line.trim_start().starts_with("code=")
                && !line.contains("code=0")
                && !line.contains("code=$?")),
        "`code` is assigned only 0 (the command passed) or `$?` (its status):\n{script}"
    );

    // No `if:` on this step: a skipped release check is a green build that checked nothing.
    assert!(
        !step
            .lines()
            .any(|line| line.trim_start().starts_with("if:")),
        "the check step runs on every run; an `if:` could skip the release verdict:\n{step}"
    );

    // The stale report is removed *before* the check reads it, and under `if: always()`.
    let steps = the_steps(snippet);
    let position = |needle: &str| {
        steps
            .iter()
            .position(|(_, body)| body.contains(needle))
            .unwrap_or_else(|| panic!("no step contains {needle:?}"))
    };
    let invalidate = position("rm -f \"${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml\"");
    assert!(
        steps[invalidate].1.contains("if: always()"),
        "the stale-report invalidation runs on a red build too:\n{}",
        steps[invalidate].1
    );
    assert!(
        invalidate < position("if req check --test-report"),
        "the stale report is invalidated before `req check` reads it"
    );
    assert!(
        position("cargo nextest run --profile ci") < position("if req check --test-report"),
        "the suite that writes the report runs before the check that reads it"
    );
}

/// The shell this template used to carry — the loop over `.req/variants`, the `read -a` split, the
/// sticky `rc`, the per-line option check — is `req check` now (ADR 0023, tested once in Rust).
/// A copy that grows it back is the drift the command removed. Only *code* lines count: a comment
/// may name the files.
#[test]
fn no_copy_reimplements_the_loop_or_the_fold() {
    let doc = read_reporting(&repo_root().join("docs/adopting.md"), "adopting doc");
    let example = read_reporting(
        &repo_root().join("examples/github-workflows/req.yml"),
        "copy-ready example",
    );
    for (name, text) in [
        ("req init's printed snippet", req::init::CI_SNIPPET),
        (
            "docs/adopting.md's ```yaml block",
            the_ci_yaml_block(&doc, "docs/adopting.md"),
        ),
        ("examples/github-workflows/req.yml", example.as_str()),
    ] {
        for line in text.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            for forbidden in [
                ".req/variants",
                ".req/projections",
                "req gate",
                "gate --test-report",
                "read -r -a",
                "read -a",
                "rc=",
                "IFS=",
                "while ",
            ] {
                assert!(
                    !line.contains(forbidden),
                    "{name} carries {forbidden:?} in code — the loop and the fold are `req check`'s, \
                     not a shell's:\n{line}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The install step (ADR 0025, #65): `examples/install-req.sh`'s body, verbatim, in all three copies
// ---------------------------------------------------------------------------

/// The step's name: how the three copies, and this test, find it.
const INSTALL_STEP_NAME: &str = "req, installed from the prebuilt binary pinned below";
/// The script's own lines that no CI copy carries: its documentation and the two test-only
/// overrides. Everything outside them is the step's body, byte for byte.
const NOT_COPIED_BEGIN: &str = "# >>> not copied into the CI step";
const NOT_COPIED_END: &str = "# <<< end of the part not copied into the CI step";

/// The script as the CI step carries it: no shebang (a `run:` block has its own shell) and none of
/// the lines between the two markers. The markers are the contract; a script without them has
/// nothing to compare.
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

/// The three copies of the snippet, named, for the checks that apply to each.
fn the_three_copies() -> Vec<(&'static str, String)> {
    let doc = read_reporting(&repo_root().join("docs/adopting.md"), "adopting doc");
    let example = read_reporting(
        &repo_root().join("examples/github-workflows/req.yml"),
        "copy-ready example",
    );
    vec![
        (
            "req init's printed snippet",
            req::init::CI_SNIPPET.to_string(),
        ),
        (
            "docs/adopting.md's ```yaml block",
            the_ci_yaml_block(&doc, "docs/adopting.md").to_string(),
        ),
        ("examples/github-workflows/req.yml", example),
    ]
}

/// The install step is the install script — not a paraphrase of it. An adopter reviews one file
/// (`examples/install-req.sh`, which the tests in `tests/install_req.rs` run against a server) and
/// the step is that file; a step that drifted from it would be untested shell.
#[test]
fn the_install_step_is_the_install_scripts_body_verbatim() {
    let script = read_reporting(
        &repo_root().join("examples/install-req.sh"),
        "install script",
    );
    let expected = install_script_as_ci_body(&script);
    for (name, copy) in the_three_copies() {
        let body = the_script_containing(&copy, "SHA256_x86_64_unknown_linux_gnu=");
        assert_same_bytes(
            &body,
            &format!("{name}'s install step"),
            &expected,
            "examples/install-req.sh (shebang and the not-copied block removed)",
        );
    }
}

/// The install step fails the job when the install fails. Every way a red turns green is named:
/// the step-level switch, the shell's `|| true`, a pipe into `tee` (whose status replaces the
/// command's), a conditional that skips the step, and a shell that does not stop on error.
#[test]
fn the_install_step_cannot_mask_a_failure_and_takes_its_optional_token_from_a_secret() {
    for (name, copy) in the_three_copies() {
        let steps = the_steps(&copy);
        let found: Vec<&(String, String)> = steps
            .iter()
            .filter(|(line, _)| line.contains(INSTALL_STEP_NAME))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "{name}: exactly one install step, named {INSTALL_STEP_NAME:?}"
        );
        let step = &found[0].1;
        for masked in [
            "continue-on-error",
            "|| true",
            "||true",
            "| tee",
            "|tee",
            "tee ",
            "set +e",
            "pipefail",
            "exit 0",
        ] {
            assert!(
                !step.contains(masked),
                "{name}: the install step must not carry {masked:?} — it would turn a failed or \
                 refused install into a green step:\n{step}"
            );
        }
        assert!(
            !step
                .lines()
                .any(|line| line.trim_start().starts_with("if:")),
            "{name}: the install step runs on every run; an `if:` could skip it:\n{step}"
        );
        assert!(
            step.contains("REQ_RELEASE_TOKEN: ${{ secrets.REQ_RELEASE_TOKEN }}"),
            "{name}: the optional token comes from the REQ_RELEASE_TOKEN secret, in the step's `env:`:\n{step}"
        );
        assert!(
            step.lines().any(|line| line.trim() == "set -eu"),
            "{name}: the install step's shell stops on the first error and on an unset variable"
        );
    }
}

/// No copy compiles req, offers an ssh identity, or knows the install script's test-only
/// overrides. These are the old step's words and the new script's test hooks; neither belongs in
/// what an adopter copies.
#[test]
fn no_copy_installs_req_from_source_or_knows_the_test_overrides() {
    for (name, copy) in the_three_copies() {
        for forbidden in [
            "cargo install --git",
            "cargo install --path",
            "REQ_RELEASE_BASE",
            "REQ_CURL_PROTO",
            "REQ_DEPLOY_KEY",
            "GIT_SSH_COMMAND",
            "CARGO_NET_GIT_FETCH_WITH_CLI",
            "ssh://",
        ] {
            assert!(
                !copy.contains(forbidden),
                "{name} carries {forbidden:?} — the CI install is the prebuilt, checksummed binary \
                 (ADR 0025); the source install is documented in docs/adopting.md, not copied"
            );
        }
    }
}

/// Order is the property: req is installed before any step runs it, and the PATH step sits
/// between the install and the first use.
#[test]
fn the_install_comes_before_every_step_that_runs_req() {
    for (name, copy) in the_three_copies() {
        let steps = the_steps(&copy);
        let position = |needle: &str| {
            steps
                .iter()
                .position(|(_, body)| body.contains(needle))
                .unwrap_or_else(|| panic!("{name}: no step contains {needle:?}"))
        };
        let install = position(INSTALL_STEP_NAME);
        let on_path = position(">> \"$GITHUB_PATH\"");
        assert!(install < on_path, "{name}: PATH is set after the install");
        assert!(
            on_path < position("if req check --test-report"),
            "{name}: `req` is on PATH before the release check runs it"
        );
        assert!(
            on_path < position("capture check check --test-report"),
            "{name}: `req` is on PATH before the JSON views run it"
        );
    }
}

/// The adopting page leads with the prebuilt install and says what it does not do. The existing
/// `cargo install` form stays — as "from source" and "air-gapped" — and `tests/init.rs` pins its
/// words; this pins the order and the honesty (a public release needs no credential, there is no fallback).
#[test]
fn the_adoption_doc_leads_with_the_prebuilt_install_and_says_what_it_does_not_do() {
    let doc = read_reporting(&repo_root().join("docs/adopting.md"), "adopting doc");
    let start = doc
        .find("\n## Install req\n")
        .expect("docs/adopting.md has an `## Install req` section");
    let rest = &doc[start + 1..];
    let section = &rest[..rest[3..].find("\n## ").map_or(rest.len(), |end| end + 3)];
    let at = |needle: &str| {
        section.find(needle).unwrap_or_else(|| {
            panic!("docs/adopting.md § Install req must say {needle:?}:\n{section}")
        })
    };
    assert!(
        at("examples/install-req.sh") < at("cargo install"),
        "the prebuilt install comes first; the source install is the second way"
    );
    for needle in [
        "REQ_RELEASE_TOKEN",
        "private fork",
        "sent **only** when",
        "needs no credential",
        "https://github.com/autox-my/req.git",
        "curl",
        "tar",
        "SHA-256",
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "trust on first use",
        "no automatic fallback",
        "from source",
        "Air-gapped",
        "replaces the SSH key and the toolchain",
    ] {
        at(needle);
    }
    for gone in [
        "read:repository",
        "git@",
        "ssh://",
        "GIT_SSH_COMMAND",
        "CARGO_NET_GIT_FETCH_WITH_CLI",
    ] {
        assert!(
            !section.contains(gone),
            "the retired SSH/Forgejo install ({gone:?}) is gone from § Install req:\n{section}"
        );
    }
    // The pin table's placeholders are named as placeholders, not presented as values.
    assert!(
        section.contains("placeholder"),
        "the pin table's placeholders are named as such:\n{section}"
    );
}

/// The directory a probe corpus lives in: `req init`, the starter claim replaced by one that
/// applies only in `deployment_mode=air_gapped`, so `.req/variants` decides what the gate can say.
struct Probe(PathBuf);

impl Probe {
    fn new(name: &str, pinned_value: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("req-ci-template-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the probe dir");
        let init = std::process::Command::new(env!("CARGO_BIN_EXE_req"))
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("runs req init");
        assert_eq!(init.status.code(), Some(0), "req init must succeed");
        std::fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        std::fs::write(
            dir.join("claims/REQ-0001.yml"),
            format!(
                "schema: req.claim/2\n\
                 claim_id: REQ-0001\n\
                 statement: \"The REQ-0001 service shall respond to a health check within 200 ms.\"\n\
                 applicability:\n  - deployment_mode == air_gapped\n\
                 relations: []\n\
                 evidence: process\n\
                 rationale:\n  source: docs/requirements.md\n  date: 2026-11-04\n  rejected_alternatives: []\n  assumptions: []\n\
                 pins:\n  operator: LE\n  value: \"{pinned_value}\"\n  unit: s\n"
            ),
        )
        .expect("writes the probe claim");
        Probe(dir)
    }

    fn variants(&self, text: &str) {
        std::fs::create_dir_all(self.0.join(".req")).expect("makes .req/");
        std::fs::write(self.0.join(".req/variants"), text).expect("writes .req/variants");
    }

    /// Run the template's `req check` step, as a runner would (`bash -c <the step's script>`),
    /// with the freshly built `req` first on PATH. Returns the exit code, the combined output and
    /// the job summary the step wrote.
    fn run_step(&self, with_req: bool) -> (Option<i32>, String, String) {
        let script = the_script_containing(req::init::CI_SNIPPET, "if req check --test-report");
        let summary = self.0.join("step-summary.md");
        let _ = std::fs::remove_file(&summary);
        let bin_dir = Path::new(env!("CARGO_BIN_EXE_req"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        // Without `req` the PATH is the system's alone: the ambient one may carry an installed
        // `req`, and "not found" must be the case under test, not whatever the machine has.
        let path = if with_req {
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            )
        } else {
            "/usr/bin:/bin".to_string()
        };
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(script)
            .current_dir(&self.0)
            .env("PATH", path)
            .env("RUNNER_TEMP", &self.0)
            .env("GITHUB_STEP_SUMMARY", &summary)
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs the check step");
        (
            out.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
            std::fs::read_to_string(&summary).unwrap_or_default(),
        )
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The probe that matters: the template's own `req check` command line, run for real against
/// corpora whose verdict is known, must make the **step** exit with that verdict — 3 included.
/// A step that turned an UNVERIFIABLE into success (a pipe, `|| true`, a trailing command) would
/// pass every shape pin above and fail here.
#[cfg(unix)]
#[test]
fn the_check_step_exits_with_req_checks_exit() {
    // 3: the variants line leaves the claim's selector undecided — the case the old template's
    // sticky-`rc` shell existed to keep from going green.
    let undecided = Probe::new("undecided", "0.2");
    undecided.variants("field=value\n");
    let (code, text, summary) = undecided.run_step(true);
    assert_eq!(
        code,
        Some(3),
        "an undecided release is UNVERIFIABLE, and the step is red:\n{text}"
    );
    assert!(text.contains("verdict: UNVERIFIABLE"), "{text}");
    assert!(
        !text.contains("verdict: PASS") && !summary.contains("PASS"),
        "a non-PASS verdict never prints PASS, in the log or the summary:\n{text}\n{summary}"
    );
    assert!(
        summary.contains("| 3 | UNVERIFIABLE"),
        "the job summary names the exit and the verdict:\n{summary}"
    );

    // 1: a pin the statement does not hold fails the gate.
    let defect = Probe::new("defect", "0.9");
    defect.variants("deployment_mode=air_gapped\n");
    let (code, text, _) = defect.run_step(true);
    assert_eq!(code, Some(1), "a failing gate is exit 1:\n{text}");

    // FAIL outranks UNVERIFIABLE through the step, in either order.
    for lines in [
        "deployment_mode=air_gapped\nfield=value\n",
        "field=value\ndeployment_mode=air_gapped\n",
    ] {
        let both = Probe::new("both", "0.9");
        both.variants(lines);
        let (code, text, _) = both.run_step(true);
        assert_eq!(
            code,
            Some(1),
            "FAIL outranks UNVERIFIABLE ({lines:?}):\n{text}"
        );
    }

    // 2: a malformed list is a usage-class error and nothing was gated.
    let malformed = Probe::new("malformed", "0.2");
    malformed.variants("--help\n");
    let (code, text, _) = malformed.run_step(true);
    assert_eq!(
        code,
        Some(2),
        "an option-shaped release line is exit 2:\n{text}"
    );
    assert!(!text.contains("[PASS]"), "nothing ran:\n{text}");

    // 0: every declared step ran and passed.
    let healthy = Probe::new("healthy", "0.2");
    healthy.variants("deployment_mode=air_gapped\n");
    let (code, text, summary) = healthy.run_step(true);
    assert_eq!(code, Some(0), "a passing release is a green step:\n{text}");
    assert!(text.contains("verdict: PASS"), "{text}");
    assert!(summary.contains("| 0 | PASS"), "{summary}");

    // `req` not installed: the step is red (127), never a quiet success.
    let missing = Probe::new("missing-req", "0.2");
    missing.variants("deployment_mode=air_gapped\n");
    let (code, text, _) = missing.run_step(false);
    assert!(
        matches!(code, Some(c) if c != 0),
        "a runner without `req` must not report success:\n{text}"
    );
}
