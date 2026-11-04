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

#[test]
fn the_overview_page_is_current_with_the_tool() {
    let mut entries = vec![req_bin().parent().expect("a parent dir").to_path_buf()];
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(entries).expect("valid PATH");
    let output = Command::new("python3")
        .args(["tools/overview/build.py", "--check", "docs/overview.html"])
        .current_dir(root())
        .env("PATH", path)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/overview/build.py");
    assert_eq!(
        output.status.code(),
        Some(0),
        "docs/overview.html is stale or its scenario broke (regenerate with `python3 \
         tools/overview/build.py --out docs/overview.html`):\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_overview_page_carries_no_invented_metric() {
    let page = read("docs/overview.html");
    for forbidden in [
        "% faster",
        "x faster",
        "hours saved",
        "time saved",
        "reduction in",
    ] {
        assert!(
            !page.to_lowercase().contains(forbidden),
            "the page must not assert a metric the tool did not print: `{forbidden}`"
        );
    }
    assert!(
        page.contains("not yet been tested against outside corpora"),
        "the page must say the bet is measured on req's own corpora only"
    );
}

#[test]
fn how_it_works_names_only_commands_and_files_that_exist() {
    let text = read("docs/how-it-works.md");
    let mut commands: Vec<String> = Vec::new();
    for word in [
        "canon", "select", "render", "trace", "impact", "compare", "gate", "baseline", "audit",
        "decision", "propose",
    ] {
        assert!(text.contains(word), "the doc should mention `{word}`");
        commands.push(word.to_string());
    }
    for command in commands {
        let status = Command::new(req_bin())
            .args([command.as_str(), "--help"])
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs req")
            .status;
        assert_eq!(status.code(), Some(0), "`req {command} --help` must exit 0");
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
