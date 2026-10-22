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
            1,
            "the variant-independent address check: without it a hand-edited digest is not caught",
        ),
        (
            "[ -r .req/variants ]",
            2,
            "the readability assertion, in the gate step and again in the artifact step: a failed \
             input redirection does not stop `set -e`, so every declared variant would be skipped \
             and the step would go green",
        ),
        (
            "exit $rc",
            1,
            "the sticky exit fold: a swallowed non-zero is a green gate over a red run",
        ),
        (
            "the deploy key the private install needs, when the runner has none",
            1,
            "the credential the private install needs: a copy-ready file that cannot reach req's \
             host on a fresh runner is not copy-ready",
        ),
        (
            "secrets.REQ_DEPLOY_KEY",
            1,
            "the key comes from a secret and is skippable, so a runner that already has access is \
             not forced to carry one",
        ),
        (
            "REQ_DEPLOY_KEY is not set",
            1,
            "the skip is announced: a silent step would make a keyless runner look provisioned",
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
            "capture \"gate-${releases}\" gate \"${release[@]}\" < /dev/null",
            1,
            "the per-variant gate document, with the `/dev/null` stdin the loop comment calls \
             load-bearing (a `req` that read stdin would shorten the release list and end green)",
        ),
        (
            "[ \"$variants_read\" -eq 1 ]",
            2,
            "the bare gate document is written only for a variants file this step actually read: a \
             missing or unreadable one is the gate step's red, and `gate.json` must not say \
             `pass` beside it",
        ),
        (
            "if: always()",
            3,
            "the capture and the upload must run on a red build — the run an agent most needs to \
             read",
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
             empty directory, so one lost variant document would ship silently",
        ),
        (
            "export PATH=\"$HOME/.local/bin:$PATH\"",
            1,
            "`$GITHUB_PATH` applies to later steps only; this step's own `req --version` needs the \
             installed binary on PATH now, or the copy-ready file is red on its first run",
        ),
        (
            "umask 077",
            2,
            "the key is never briefly world-readable — and the reason says so, so a later edit \
             that drops the umask is visible",
        ),
        (
            "remove the deploy key this job wrote",
            1,
            "the key is removed under `if: always()`: a private key left in `$HOME` is readable by \
             every later job on a self-hosted runner",
        ),
        (
            "RUNNER_TEMP",
            3,
            "the key lives in the per-job temp dir (written, offered to the install, removed), not \
             in a path that survives the job",
        ),
        (
            "release line carries an option",
            3,
            "a release line is a `key=value` context, never an option: `req gate --help` exits 0 \
             having gated nothing, so an option-shaped line would leave the gate step green and \
             the capture would move help text into the artifact",
        ),
        (
            "req_deploy_key",
            3,
            "the deploy key gets a path of its own: overwriting `$HOME/.ssh/id_ed25519` would take \
             a self-hosted runner's identity away from every other job",
        ),
        (
            "IdentitiesOnly=yes",
            1,
            "only that key is offered to the private host — never the runner's default identity",
        ),
        (
            "[ \"$rejected\" -eq 0 ]",
            3,
            "a variants file whose every line was rejected is not the default variant: no bare \
             `gate.json` is written beside the gate step's red",
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
