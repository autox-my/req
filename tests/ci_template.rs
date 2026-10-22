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

/// The one **fenced YAML block** in a markdown document, without the fence. Panics when the
/// document has none or more than one: "the first one" is exactly the assumption that lets a
/// test compare the wrong block and pass, so a second block is a failure, not a tie-break.
fn the_only_yaml_block<'a>(markdown: &'a str, source: &str) -> &'a str {
    const OPEN: &str = "```yaml\n";
    let mut blocks = markdown.match_indices(OPEN);
    let first = blocks
        .next()
        .unwrap_or_else(|| panic!("{source} has no ```yaml block; there is nothing to compare"));
    assert!(
        blocks.next().is_none(),
        "{source} has more than one ```yaml block; this test compares *the* CI snippet block, \
         and a second one would let it compare the wrong text and pass"
    );
    let after = &markdown[first.0 + OPEN.len()..];
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
    let doc_block = the_only_yaml_block(&doc, "docs/adopting.md");

    let example_path = repo_root().join("examples/github-workflows/req.yml");
    let example = read_reporting(&example_path, "copy-ready example");

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
    // Logically implied by the two above, but the pair a human edits without a compiler gets
    // its own message: a red run should not send the reader through `src/init.rs` first.
    assert_same_bytes(
        doc_block,
        "docs/adopting.md's ```yaml block",
        &example,
        "examples/github-workflows/req.yml",
    );

    // The snippet must still *be* the snippet. Byte equality would keep three copies of a
    // gutted text honest with each other; each fragment below is a failure this repository
    // has already paid for, so its disappearance is named rather than averaged away.
    for (fragment, why) in [
        (
            "req canon --verify",
            "the variant-independent address check: without it a hand-edited digest is not caught",
        ),
        (
            "[ -r .req/variants ]",
            "the readability assertion: a failed input redirection does not stop `set -e`, so \
             every declared variant would be skipped and the step would go green",
        ),
        (
            "exit $rc",
            "the sticky exit fold: a swallowed non-zero is a green gate over a red run",
        ),
        (
            "--format json",
            "the agent-facing documents (pack C): an agent reads `req.gate/1`, not scraped text",
        ),
        // The artifact step is the pack's new surface, and it is *not* executed by
        // `tests/init.rs` (that target runs the const's gate step). Without these fragments a
        // step gutted down to `view render` would satisfy every test in this file.
        (
            "view render",
            "the spec view document: one of the four an agent reads instead of scraping text",
        ),
        (
            "view trace",
            "the traceability document: dropped, the artifact silently loses every edge",
        ),
        (
            "view audit",
            "the audit packet document: dropped, coverage and provenance vanish from the artifact",
        ),
        (
            "req gate \"${release[@]}\" --format json",
            "the per-variant gate document: a bare `req gate` is UNVERIFIABLE on a \
             selector-carrying corpus (ADR 0015), so the artifact would say \"cannot tell\" \
             beside a release that passed",
        ),
        (
            "actions/upload-artifact@v4",
            "the upload: without it the documents die with the runner and no reviewing agent \
             ever reads them",
        ),
    ] {
        assert!(
            printed.contains(fragment),
            "req init's CI snippet no longer carries {fragment:?} — {why}"
        );
    }
}
