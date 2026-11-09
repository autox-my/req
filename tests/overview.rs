//! The two explanatory views stay true to the tool.
//!
//! `docs/overview.html` is generated from the real `req` by `tools/overview/build.py`, so it is
//! checked the way a projection is: regenerated and compared, never hand-edited. `docs/how-it-works.md`
//! is drawn by hand, so what it names is checked against the binary: every `req <command>` in it must
//! answer `--help`, and every source file or doc it points at must exist. A diagram that names a
//! command that has been renamed fails here, not in a reader's head.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

/// `python3` as an absolute path, resolved before the child's `PATH` is rewritten, so a missing
/// interpreter is a named environment problem and not a failing check.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/overview/build.py - install it first");
}

#[test]
fn the_overview_page_is_current_with_the_tool() {
    let interpreter = python3();
    let mut entries = vec![req_bin().parent().expect("a parent dir").to_path_buf()];
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(entries).expect("valid PATH");
    let output = Command::new(interpreter)
        .args(["tools/overview/build.py", "--check", "docs/overview.html"])
        .current_dir(root())
        .env("PATH", path)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/overview/build.py");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    match output.status.code() {
        Some(0) => {}
        Some(3) => panic!("UNVERIFIABLE, not stale: the generator found no `req` on PATH:\n{said}"),
        Some(1) => panic!(
            "docs/overview.html is stale or its scenario broke (regenerate with `python3 \
             tools/overview/build.py --out docs/overview.html`):\n{said}"
        ),
        other => panic!("tools/overview/build.py ended abnormally ({other:?}):\n{said}"),
    }
}

/// The page's hand-written prose is the only place an invented metric can come from. Outside the
/// generated terminal blocks it may carry no percentage, no multiplier and none of the words a
/// performance claim needs; the numbers it does carry (200 ms, 500 ms) are the scenario's own.
#[test]
fn the_overview_page_carries_no_invented_metric() {
    let page = read("docs/overview.html");
    let mut prose = String::new();
    let mut rest = page.as_str();
    while let Some(open) = rest.find("<pre") {
        prose.push_str(&rest[..open]);
        let close = rest[open..].find("</pre>").expect("a <pre> closes");
        rest = &rest[open + close + "</pre>".len()..];
    }
    prose.push_str(rest);
    let prose = prose.replace("&nbsp;", " ").to_lowercase();
    for forbidden in [
        "%",
        "\u{d7}",
        "faster",
        "slower",
        "saved",
        "saves",
        "less toil",
        "cuts ",
        "reduc",
        "roi",
        "productiv",
    ] {
        assert!(
            !prose.contains(forbidden),
            "the page must not assert a metric the tool did not print: found `{forbidden}`"
        );
    }
    assert!(
        page.contains("independent human-written paraphrase is not yet measured"),
        "the page must state the bound of the measurement (README \"The bet\")"
    );
    assert!(
        !page.contains("not yet been tested against outside corpora"),
        "the external families (external-v1 to external-v4) exist: the page must not deny them"
    );
}

/// The set of `req <command>` code spans the doc opens (flags and placeholders are skipped).
fn named_commands(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices("`req ") {
        let word: String = text[at + 5..]
            .chars()
            .take_while(char::is_ascii_lowercase)
            .collect();
        if !word.is_empty() && !found.contains(&word) {
            found.push(word);
        }
    }
    found
}

#[test]
fn how_it_works_names_only_commands_and_files_that_exist() {
    let text = read("docs/how-it-works.md");
    let commands = named_commands(&text);
    for expected in [
        "canon", "select", "render", "trace", "impact", "compare", "gate", "baseline", "audit",
        "decision", "propose",
    ] {
        assert!(
            commands.iter().any(|c| c == expected),
            "the doc must name `req {expected}` as a code span (found {commands:?})"
        );
    }
    for command in &commands {
        let status = Command::new(req_bin())
            .args([command.as_str(), "--help"])
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs req")
            .status;
        assert_eq!(
            status.code(),
            Some(0),
            "`req {command} --help` must exit 0: the doc names it"
        );
    }
    for referenced in [
        "docs/overview.html",
        "docs/adopting.md",
        "docs/design.md",
        "docs/claim-format.md",
        "docs/canonical-hashing.md",
        "tools/adopt.py",
        "examples/handoff-brief.md",
        "examples/agents-snippet.md",
    ] {
        assert!(
            text.contains(referenced),
            "the doc should point at `{referenced}`"
        );
        assert!(
            root().join(referenced).exists(),
            "`{referenced}` is named in docs/how-it-works.md and must exist"
        );
    }
    assert_eq!(
        text.matches("```mermaid").count(),
        3,
        "three diagrams: lifecycle, authored/derived/signed, change to release"
    );
}

/// GitHub Pages serves `docs/` as is (`docs/.nojekyll`), so `https://autox-my.github.io/req/` is
/// `docs/index.html` or a 404. The landing page must exist, reach the overview by a relative link
/// (it is served beside it) and name no file the repository does not have.
#[test]
fn the_pages_root_is_a_landing_page_that_links_the_overview() {
    let page = read("docs/index.html");
    assert!(
        page.contains("href=\"overview.html\""),
        "docs/index.html must link the overview page relatively: it is served beside it"
    );
    for linked in ["docs/how-it-works.md", "docs/adopting.md", "README.md"] {
        assert!(
            page.contains(&format!("/blob/main/{linked}")),
            "docs/index.html should link `{linked}` on GitHub"
        );
        assert!(root().join(linked).is_file(), "`{linked}` must exist");
    }
}
