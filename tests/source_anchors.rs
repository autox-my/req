//! Issue #19: a `rationale.source` that carries a line anchor (`docs/plan.md#L390`) is checked
//! against the file it names, so a plan that was edited since the claim was written cannot leave an
//! anchor pointing past its end, at a deleted file, or at a line that was never there.
//!
//! What is judged, and what deliberately is not:
//!   * judged: a source that is a repository path with a **line anchor** (`#L<n>`, `#L<n>-L<m>`,
//!     `#L<n>-<m>`): the file must exist, be a file, and have that many lines. A definite defect,
//!     so `rationale_present` fails (exit 1);
//!   * a file that exists but cannot be read is "cannot be told": UNVERIFIABLE (exit 3);
//!   * not judged: prose (it has whitespace), URLs, absolute paths, paths that leave the repository
//!     (`..`), a heading anchor, and a path with no anchor. Whether such a source still says what
//!     the claim was written from is not something a line count can tell, and a guess here would be
//!     a false FAIL on someone's release gate.
//!
//! Only applying claims are judged, like every other policy of the gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn req(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(
        std::env::var_os("CARGO_BIN_EXE_req")
            .map(PathBuf::from)
            .expect("integration tests run with CARGO_BIN_EXE_req set"),
    )
    .args(args)
    .current_dir(cwd)
    .env_remove("REQ_ROOT")
    .output()
    .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A corpus at `root` (a fresh temp directory, or a subdirectory of one).
struct World {
    top: PathBuf,
    root: PathBuf,
}

impl World {
    fn new(name: &str) -> Self {
        let top = std::env::temp_dir().join(format!(
            "req-source-anchors-{name}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&top);
        fs::create_dir_all(&top).expect("creates the fixture directory");
        let world = World {
            root: top.clone(),
            top,
        };
        world.init();
        world
    }

    /// The corpus lives in `<top>/sub`, and `<top>` is the repository (it holds a `.git`).
    fn in_subdirectory(name: &str) -> Self {
        let top = std::env::temp_dir().join(format!(
            "req-source-anchors-{name}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&top);
        let root = top.join("sub");
        fs::create_dir_all(&root).expect("creates the fixture directory");
        fs::create_dir_all(top.join(".git")).expect("marks the repository root");
        let world = World { top, root };
        world.init();
        world
    }

    fn init(&self) {
        let (code, out, err) = req(&self.root, &["init"]);
        assert_eq!(code, 0, "init: {out}{err}");
        fs::remove_file(self.root.join("claims/REQ-0001.yml")).expect("removes the starter claim");
    }

    /// A process-evidenced claim (so `tests_linked` is settled and only provenance is in question).
    fn claim(&self, id: &str, source: &str, applicability: Option<&str>) {
        let applicability = match applicability {
            Some(predicate) => format!("applicability:\n  - {predicate}\n"),
            None => "applicability: []\n".to_string(),
        };
        let text = format!(
            "schema: req.claim/2\nclaim_id: {id}\n\
             statement: \"The {id} service shall respond within 200 ms.\"\n\
             {applicability}relations: []\nevidence: process\n\
             rationale:\n  source: {source:?}\n  date: 2026-11-10\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.root.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    fn file(&self, rel: &str, content: &str) {
        let path = self.top.join(rel);
        fs::create_dir_all(path.parent().expect("a parent")).expect("creates the directory");
        fs::write(path, content).expect("writes the file");
    }

    fn gate(&self, extra: &[&str]) -> (i32, Value, String) {
        let mut args = vec!["gate", "--format", "json"];
        args.extend_from_slice(extra);
        let (code, out, err) = req(&self.root, &args);
        let document: Value =
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}\n{err}"));
        (code, document, out)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.top);
    }
}

fn rationale(document: &Value) -> &Value {
    document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == "rationale_present")
        .expect("the rationale_present policy")
}

fn details(document: &Value) -> Vec<String> {
    rationale(document)["details"]
        .as_array()
        .expect("details")
        .iter()
        .map(|d| d.as_str().expect("a string").to_string())
        .collect()
}

const FIVE_LINES: &str = "one\ntwo\nthree\nfour\nfive\n";

#[test]
fn an_anchor_inside_the_file_passes_and_is_counted() {
    let w = World::new("inside");
    w.file("docs/plan.md", FIVE_LINES);
    w.claim("REQ-0001", "docs/plan.md#L5", None);
    w.claim("REQ-0002", "docs/plan.md#L1-L5", None);
    w.claim("REQ-0003", "docs/plan.md#L2-4", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(rationale(&document)["outcome"], "pass");
    // Three sources were judged; the count is the proof that a pass is not a pass over nothing.
    assert_eq!(rationale(&document)["checked"], 3, "{out}");
}

#[test]
fn an_anchor_past_the_last_line_is_a_definite_finding_naming_the_claim_and_the_length() {
    let w = World::new("beyond");
    w.file("docs/plan.md", FIVE_LINES);
    w.claim("REQ-0001", "docs/plan.md#L6", None);
    w.claim("REQ-0002", "docs/plan.md#L3-L9", None);
    w.claim("REQ-0003", "docs/plan.md#L5", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(rationale(&document)["outcome"], "fail");
    assert_eq!(
        details(&document),
        vec![
            "REQ-0001: rationale.source docs/plan.md#L6 is beyond the last line of docs/plan.md (it has 5)",
            "REQ-0002: rationale.source docs/plan.md#L3-L9 is beyond the last line of docs/plan.md (it has 5)",
        ]
    );
    assert_eq!(rationale(&document)["checked"], 3);
}

#[test]
fn the_last_line_counts_without_a_trailing_newline_and_an_empty_file_has_none() {
    let w = World::new("eof");
    w.file("docs/two.md", "a\nb");
    w.file("docs/empty.md", "");
    w.claim("REQ-0001", "docs/two.md#L2", None);
    w.claim("REQ-0002", "docs/two.md#L3", None);
    w.claim("REQ-0003", "docs/empty.md#L1", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        details(&document),
        vec![
            "REQ-0002: rationale.source docs/two.md#L3 is beyond the last line of docs/two.md (it has 2)",
            "REQ-0003: rationale.source docs/empty.md#L1 is beyond the last line of docs/empty.md (it has 0)",
        ]
    );
}

#[test]
fn an_anchor_into_a_file_that_does_not_exist_or_is_a_directory_is_a_finding() {
    let w = World::new("missing");
    fs::create_dir_all(w.top.join("docs/adr")).expect("creates a directory");
    w.claim("REQ-0001", "docs/gone.md#L3", None);
    w.claim("REQ-0002", "docs/adr#L1", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        details(&document),
        vec![
            "REQ-0001: rationale.source docs/gone.md#L3 names a file that does not exist",
            "REQ-0002: rationale.source docs/adr#L1 names a directory, and a line anchor needs a file",
        ]
    );
}

#[test]
fn line_zero_and_a_backwards_range_are_not_lines() {
    let w = World::new("zero");
    w.file("docs/plan.md", FIVE_LINES);
    w.claim("REQ-0001", "docs/plan.md#L0", None);
    w.claim("REQ-0002", "docs/plan.md#L4-L2", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(
        details(&document),
        vec![
            "REQ-0001: rationale.source docs/plan.md#L0 is not a line (lines start at 1)",
            "REQ-0002: rationale.source docs/plan.md#L4-L2 is not a range (it ends before it starts)",
        ]
    );
}

#[test]
fn what_a_line_count_cannot_judge_is_not_judged() {
    let w = World::new("not-judged");
    // None of these has a file behind it, and none is judged: prose, a URL, an absolute path, a path
    // that leaves the repository, a heading anchor, and a path with no anchor at all.
    w.claim("REQ-0001", "the plan, section 3 (plan.md#L400)", None);
    w.claim("REQ-0002", "https://example.com/plan.md#L400", None);
    w.claim("REQ-0003", "/etc/plan.md#L400", None);
    w.claim("REQ-0004", "../plan.md#L400", None);
    w.claim("REQ-0005", "docs/plan.md#imaging", None);
    w.claim("REQ-0006", "docs/plan.md", None);
    w.claim("REQ-0007", "REQ-0003", None);
    w.claim("REQ-0008", "docs/plan.md#L", None);
    w.claim("REQ-0009", "docs/plan.md#L1x", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        rationale(&document)["checked"],
        0,
        "a pass over zero sources says so: {out}"
    );
}

#[test]
fn only_applying_claims_are_judged() {
    let w = World::new("applying");
    w.file("docs/plan.md", FIVE_LINES);
    w.claim("REQ-0001", "docs/plan.md#L2", Some("mode == a"));
    w.claim("REQ-0002", "docs/plan.md#L99", Some("mode == b"));
    let (code, document, out) = w.gate(&["mode=a"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(rationale(&document)["checked"], 1);
    let (code, document, out) = w.gate(&["mode=b"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(details(&document).len(), 1);
}

#[test]
fn a_repository_path_is_resolved_from_the_repository_root_as_well_as_the_corpus_root() {
    // The corpus is in `sub/`, the repository is its parent: `docs/plan.md` is the parent's file.
    let w = World::in_subdirectory("subdir");
    w.file("docs/plan.md", FIVE_LINES);
    w.claim("REQ-0001", "docs/plan.md#L4", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(rationale(&document)["checked"], 1);
    // A corpus-relative file is found too.
    fs::write(w.root.join("local.md"), "x\ny\n").expect("writes a corpus file");
    w.claim("REQ-0002", "local.md#L2", None);
    let (code, _, out) = w.gate(&[]);
    assert_eq!(code, 0, "{out}");
}

#[cfg(unix)]
#[test]
fn a_file_that_exists_but_cannot_be_read_is_unverifiable_not_a_defect() {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new("unreadable");
    w.file("docs/plan.md", FIVE_LINES);
    let path = w.top.join("docs/plan.md");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::read(&path).is_ok() {
        // Running as a user who can read anything (root): the condition cannot be made here.
        return;
    }
    w.claim("REQ-0001", "docs/plan.md#L2", None);
    let (code, document, out) = w.gate(&[]);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod back");
    assert_eq!(code, 3, "{out}");
    assert_eq!(rationale(&document)["outcome"], "unverifiable");
    let d = details(&document);
    assert_eq!(d.len(), 1);
    assert!(
        d[0].starts_with(
            "REQ-0001: rationale.source docs/plan.md#L2 cannot be read, so its length is unknown ("
        ),
        "{d:?}"
    );
}

#[test]
fn a_missing_source_or_date_is_still_the_old_finding_and_outranks_nothing_it_did_not_before() {
    // The pre-existing rule, unchanged: an empty source is named by claim id alone.
    let w = World::new("empty");
    w.claim("REQ-0001", "", None);
    let (code, document, out) = w.gate(&[]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(details(&document), vec!["REQ-0001"]);
}

#[test]
fn the_text_report_says_how_many_sources_were_checked() {
    let w = World::new("text");
    w.file("docs/plan.md", FIVE_LINES);
    w.claim("REQ-0001", "docs/plan.md#L2", None);
    let (_, out, _) = req(&w.root, &["gate"]);
    assert!(
        out.contains(
            "every applying claim carries provenance (rationale source and date); \
             1 source line anchor(s) checked against their files"
        ),
        "{out}"
    );
}
