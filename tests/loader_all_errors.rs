//! Q-1 item 3 — the claim loader reports every broken file, not the first one.
//!
//! A corpus with three broken files used to name one, and the author fixed it, re-ran, and met
//! the next. The loader now collects the per-file problems and reports all of them, in path
//! order, as one UNVERIFIABLE (exit 3). A corpus with a single problem reads exactly as before.
//! No caller may take a partial corpus for a whole one: a load that found any problem returns
//! no claims at all.

use std::path::{Path, PathBuf};

use req::canon::load_claims;

const REGISTRY: &str = "\
schema: req.subject_registry/0
version: 1
subjects:
  REQ_TOOL:
    label: The req tool
    aliases:
      - the req tool
";

fn claim_text(claim_id: &str) -> String {
    let schema = req::canon::claim_schema();
    format!(
        "\
schema: {schema}
claim_id: {claim_id}
statement: >-
  The req tool shall treat a single claim as the unit of requirements authorship,
  review, and merge.
applicability: []
rationale:
  source: docs/adr/0001-the-unit-is-a-claim.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
"
    )
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("claims")).expect("claims dir");
        std::fs::create_dir_all(root.join("aliases")).expect("aliases dir");
        std::fs::write(root.join("aliases/subjects.yml"), REGISTRY).expect("registry");
        Fixture { root }
    }

    fn claim(self, file: &str, text: &str) -> Self {
        std::fs::write(self.root.join("claims").join(file), text).expect("claim file");
        self
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn req(root: &Path, args: &[&str]) -> (i32, String, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_req"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Three files broken three different ways, plus one good claim between them.
fn three_broken(name: &str) -> Fixture {
    Fixture::new(name)
        // a misspelled key: does not parse as a claim
        .claim(
            "REQ-0001.yml",
            &claim_text("REQ-0001").replace("relations:", "relatoins:"),
        )
        // a good claim, which must not hide the others
        .claim("REQ-0002.yml", &claim_text("REQ-0002"))
        // the stem is not the claim_id it declares
        .claim("REQ-0003.yml", &claim_text("REQ-0030"))
        // a claim-shaped name that is not `REQ-<digits>.yml`
        .claim("REQ-0004.yaml", &claim_text("REQ-0004"))
}

#[test]
fn a_corpus_with_three_broken_files_names_all_three() {
    let f = three_broken("q1-three-broken-lib");

    let err =
        load_claims(f.path()).expect_err("a broken corpus loads no claims, not a partial one");
    for name in ["REQ-0001.yml", "REQ-0003.yml", "REQ-0004.yaml"] {
        assert!(err.contains(name), "{name} is missing from: {err}");
    }
    assert!(
        !err.contains("REQ-0002.yml"),
        "the good file is not a problem: {err}"
    );
    // by path: REQ-0001 before REQ-0003 before REQ-0004, whatever order the filesystem listed
    let at = |name: &str| err.find(name).unwrap_or_else(|| panic!("{name} in {err}"));
    assert!(at("REQ-0001.yml") < at("REQ-0003.yml"), "{err}");
    assert!(at("REQ-0003.yml") < at("REQ-0004.yaml"), "{err}");
}

#[test]
fn the_cli_prints_every_problem_and_exits_three_once() {
    let f = three_broken("q1-three-broken-cli");

    for command in ["canon", "select", "render"] {
        let (code, stdout, stderr) = req(f.path(), &[command]);
        assert_eq!(
            code, 3,
            "{command}: a broken corpus is UNVERIFIABLE\n{stderr}"
        );
        assert_eq!(
            stdout, "",
            "{command}: nothing is reported over a broken corpus"
        );
        for name in ["REQ-0001.yml", "REQ-0003.yml", "REQ-0004.yaml"] {
            assert!(
                stderr.contains(name),
                "{command}: {name} is missing from: {stderr}"
            );
        }
    }
}

#[test]
fn a_corpus_with_one_broken_file_reads_as_it_always_did() {
    let f = Fixture::new("q1-one-broken")
        .claim("REQ-0001.yml", &claim_text("REQ-0001"))
        .claim("REQ-0009.yml", &claim_text("REQ-0010"));
    let path = f.path().join("claims/REQ-0009.yml");

    let err = load_claims(f.path()).expect_err("a misfiled claim must not load");
    assert_eq!(
        err,
        format!(
            "{}: claim_id REQ-0010 does not match the filename stem REQ-0009 \
             (claim-format.md: filename = claim_id)",
            path.display()
        )
    );
    let (code, _, stderr) = req(f.path(), &["canon"]);
    assert_eq!(code, 3);
    assert_eq!(
        stderr,
        format!(
            "corpus root: {}\n[UNVERIFIABLE] {err}\n",
            f.path().display()
        )
    );
}

#[test]
fn an_empty_corpus_is_still_reported_on_its_own() {
    let f = Fixture::new("q1-empty");
    let err = load_claims(f.path()).expect_err("an empty corpus must not load");
    assert!(err.contains("no claim files"), "{err}");
    assert_eq!(err.lines().count(), 1, "{err}");
}

#[test]
fn a_duplicate_is_the_duplicate_and_not_also_a_filename_complaint() {
    let text = claim_text("REQ-0007");
    let f = Fixture::new("q1-dup")
        .claim("REQ-0007.yml", &text)
        .claim("REQ-0007.yaml", &text);

    let err = load_claims(f.path()).expect_err("a duplicate id must not load");
    assert!(err.contains("identical content"), "{err}");
    assert!(!err.contains("not a claim filename"), "{err}");
}
