//! `docs/architecture.html` and the files behind it stay true to the tool (ADR 0052 decision 7, step 3, unit O-3).
//!
//! The diagram (`docs/architecture/req.archify.json`) is an architect's reading of the code and is reviewed like a
//! document. `overlay.json` and `matrix.md` are computed from it, from `req satisfy --format json` and from `req trace
//! --format json` of this repository, by `tools/graph/build_architecture.py`; they are checked the way a projection
//! is: regenerated and compared, never hand-edited. The page itself is rendered by archify (Node and a browser), which
//! the gate cannot run, so what a test can read back of it is the part archify writes as attributes: every component's
//! id, label and tag, the subtitle and every source link (ADR 0052's note names the rest as a gap).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// `python3` as an absolute path, resolved before the child's `PATH` is rewritten, so a missing interpreter is a named
/// environment problem and not a failing check.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/graph/build_architecture.py - install it first");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

/// A scratch copy of the parts of the repository the build reads, never this checkout.
struct Copy(PathBuf);

impl Copy {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-architecture-page-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        for tree in [
            "claims",
            "aliases",
            ".req",
            "src",
            "decisions",
            "baselines",
            "docs/architecture",
            "tools/graph",
        ] {
            copy_tree(&root().join(tree), &dir.join(tree));
        }
        // Every file the diagram cites, wherever it lives.
        let diagram: Value =
            serde_json::from_str(&read(&root().join("docs/architecture/req.archify.json")))
                .expect("the diagram is JSON");
        for component in diagram["components"].as_array().expect("components") {
            for source in component["sources"].as_array().into_iter().flatten() {
                let path = source["path"].as_str().expect("a source path");
                let target = dir.join(path);
                if !target.exists() {
                    fs::create_dir_all(target.parent().expect("a parent")).expect("makes the dir");
                    fs::copy(root().join(path), &target).expect("copies a cited file");
                }
            }
        }
        Copy(dir)
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("makes the dir");
    for entry in fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let entry = entry.expect("a dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copies a file");
        }
    }
}

struct Run {
    code: Option<i32>,
    said: String,
}

/// `python3 <repo>/tools/graph/build_architecture.py <args>` with `req` first on `PATH`, run from `repo` (the script
/// reads the repository it lives in, so a copy checks itself).
fn build(repo: &Path, args: &[&str]) -> Run {
    let interpreter = python3();
    let mut entries = vec![req_bin().parent().expect("a parent dir").to_path_buf()];
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(entries).expect("valid PATH");
    let output = Command::new(interpreter)
        .arg("tools/graph/build_architecture.py")
        .args(args)
        .current_dir(repo)
        .env("PATH", path)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/graph/build_architecture.py");
    Run {
        code: output.status.code(),
        said: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

#[test]
fn the_architecture_files_are_current_with_the_tool() {
    let run = build(&root(), &["--check", "docs/architecture"]);
    match run.code {
        Some(0) => {}
        Some(3) => panic!(
            "UNVERIFIABLE, not stale: the build could not run `req`:\n{}",
            run.said
        ),
        Some(1) => panic!(
            "docs/architecture is stale (regenerate with `python3 tools/graph/build_architecture.py --out \
             docs/architecture`, then render docs/architecture.html again):\n{}",
            run.said
        ),
        other => panic!(
            "tools/graph/build_architecture.py ended abnormally ({other:?}):\n{}",
            run.said
        ),
    }
}

/// ADR 0052 falsification 3 as the page shows it: a linked claim whose meaning moved must turn its components SUSPECT,
/// so a committed overlay that still says HOLDS is stale and the check says which file.
#[test]
fn a_tightened_claim_makes_the_overlay_stale() {
    let copy = Copy::new("tightened");
    let claim = copy.path("claims/REQ-0028.yml");
    let text = read(&claim);
    let tightened = text.replace(
        "The req tool shall fail the verification",
        "The req tool shall fail within 200 ms the verification",
    );
    assert_ne!(
        text, tightened,
        "the statement REQ-0028 is expected to open with that sentence"
    );
    fs::write(&claim, tightened).expect("tightens the claim in the copy");
    let run = build(&copy.0, &["--check", "docs/architecture"]);
    assert_eq!(
        run.code,
        Some(1),
        "a moved claim must make the page stale:\n{}",
        run.said
    );
    assert!(
        run.said.contains("STALE") && run.said.contains("overlay.json"),
        "the stale file must be named:\n{}",
        run.said
    );
    let regenerated = build(&copy.0, &["--out", "docs/architecture"]);
    assert_eq!(regenerated.code, Some(0), "{}", regenerated.said);
    assert!(
        read(&copy.path("docs/architecture/overlay.json")).contains("SUSPECT"),
        "the baselines component must now be drawn SUSPECT"
    );
}

/// ADR 0052 decision 4: a cited file that is gone is a failure of the page, with the converter's reason, never a
/// quietly redrawn component.
#[test]
fn a_deleted_cited_file_fails_the_check_with_the_stale_reason() {
    let copy = Copy::new("deleted");
    fs::remove_file(copy.path("src/junit.rs")).expect("deletes a cited file in the copy");
    let run = build(&copy.0, &["--check", "docs/architecture"]);
    assert_eq!(run.code, Some(1), "{}", run.said);
    assert!(
        run.said.contains("cannot be made honestly") && run.said.contains("src/junit.rs"),
        "the converter's stale reason must be reported, naming the file:\n{}",
        run.said
    );
}

#[test]
fn the_check_is_unverifiable_without_req() {
    let interpreter = python3();
    let output = Command::new(interpreter)
        .args([
            "tools/graph/build_architecture.py",
            "--check",
            "docs/architecture",
        ])
        .current_dir(root())
        .env("PATH", "")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the script");
    assert_eq!(
        output.status.code(),
        Some(3),
        "no `req` is UNVERIFIABLE, not stale"
    );
    let usage = Command::new(python3())
        .args(["tools/graph/build_architecture.py"])
        .current_dir(root())
        .output()
        .expect("runs the script");
    assert_eq!(usage.status.code(), Some(2), "no mode is a usage error");
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The page is archify's rendering of `overlay.json`. archify does not embed the document itself (the page holds a
/// rendered SVG, not a JSON copy), so a test cannot compare documents; it can read back what archify writes as
/// attributes. A page rendered from an older overlay differs in a tag, a label, the subtitle or a source link.
#[test]
fn the_page_carries_what_the_overlay_says() {
    let page = read(&root().join("docs/architecture.html"));
    let overlay: Value =
        serde_json::from_str(&read(&root().join("docs/architecture/overlay.json")))
            .expect("overlay.json is JSON");
    let repository = &overlay["meta"]["repository"];
    let revision = repository["revision"].as_str().expect("a pinned revision");
    let subtitle = overlay["meta"]["subtitle"].as_str().expect("a subtitle");
    assert!(
        page.contains(&escape(subtitle)),
        "the page's subtitle must be the overlay's (revision, rules_v, archify):\n{subtitle}"
    );
    for component in overlay["components"].as_array().expect("components") {
        let id = component["id"].as_str().expect("an id");
        let label = component["label"].as_str().expect("a label");
        assert!(
            page.contains(&format!(
                "data-node-id=\"{id}\" data-node-label=\"{}\"",
                escape(label)
            )),
            "the page has no node {id} labelled {label}: render it again from overlay.json"
        );
        if let Some(tag) = component["tag"].as_str() {
            assert!(
                page.contains(&format!("data-node-tag=\"{}\"", escape(tag))),
                "the page does not carry the tag `{tag}` of {id}: render it again from overlay.json"
            );
        }
        for source in component["sources"].as_array().into_iter().flatten() {
            let path = source["path"].as_str().expect("a source path");
            let line = source["line"].as_u64().expect("a start line");
            let end = source["end_line"].as_u64().expect("an end line");
            let anchor = if line == end {
                format!("#L{line}")
            } else {
                format!("#L{line}-L{end}")
            };
            assert!(
                page.contains(&format!(
                    "https://github.com/autox-my/req/blob/{revision}/{path}{anchor}"
                )),
                "the page does not link {path}{anchor} at revision {revision}"
            );
        }
    }
}

/// The page is committed, standalone and offline: no script or stylesheet is fetched, and no path of the machine that
/// rendered it leaks into it.
#[test]
fn the_page_is_standalone_and_leaks_no_local_path() {
    let page = read(&root().join("docs/architecture.html"));
    for fetched in ["<script src=", "<link rel=\"stylesheet\"", "@import url("] {
        assert!(
            !page.contains(fetched),
            "the page must not fetch `{fetched}`"
        );
    }
    for local in ["/Users/", "/private/", "/home/", "C:\\Users"] {
        assert!(
            !page.contains(local),
            "the page leaks a local path: `{local}`"
        );
    }
}

/// The README's counts come from the generated matrix, so a re-recorded link or a new claim cannot leave the front page
/// quoting last month's numbers.
#[test]
fn the_readme_quotes_the_counts_the_matrix_shows() {
    let matrix = read(&root().join("docs/architecture/matrix.md"));
    let mut claims = 0;
    let mut linked = 0;
    let mut holding = 0;
    for row in matrix.lines().filter(|line| line.starts_with("| REQ-")) {
        claims += 1;
        let cells: Vec<&str> = row.split('|').skip(2).map(str::trim).collect();
        let filled: Vec<&&str> = cells.iter().filter(|cell| !cell.is_empty()).collect();
        if !filled.is_empty() {
            linked += 1;
        }
        holding += filled
            .iter()
            .filter(|cell| cell.starts_with("holds"))
            .count();
    }
    let unreached = claims - linked;
    let readme = read(&root().join("README.md"));
    let section = readme
        .split("## Requirements on the architecture")
        .nth(1)
        .expect("the README has the section")
        .split("\n## ")
        .next()
        .expect("a section body")
        .replace('\n', " ");
    for expected in [
        format!("{linked} of req's {claims} claims are linked"),
        format!("all {holding} claim–part pairs hold"),
        format!("the {unreached} claims no part reaches"),
    ] {
        assert!(
            section.contains(&expected),
            "the README section must say `{expected}` (the matrix shows it):\n{section}"
        );
    }
}
