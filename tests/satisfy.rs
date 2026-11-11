//! Satisfaction links (ADR 0040, task S-2): a design element declares the claims it satisfies, a
//! human records the meaning each was judged against, and a later change of that claim is judged by
//! its *direction*. The public seam is the compiled `req` against hand-written fixture corpora in a
//! temp dir; every oracle (which edit is cosmetic, which strengthens, which loosens) is written here
//! from the ADR and the mutation, never read back from the implementation.
//!
//! One test per ADR *Falsification* item (1–6) is marked `// Falsification N`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// The listing line for one link: the one that names both the element and the claim.
    fn line_for(&self, element: &str, claim: &str) -> String {
        self.stdout
            .lines()
            .find(|line| line.contains(element) && line.contains(claim))
            .unwrap_or_else(|| panic!("no line names {element} and {claim}:\n{}", self.all()))
            .to_string()
    }

    /// The policy block of the gate's text report: its header line and the indented lines after it.
    fn policy_block(&self, policy: &str) -> String {
        let mut block = String::new();
        let mut inside = false;
        for line in self.stdout.lines() {
            if line.starts_with("\n[") || line.starts_with('[') {
                inside = line.ends_with(&format!("] {policy}"));
            } else if !line.starts_with("  ") {
                inside = false;
            }
            if inside {
                block.push_str(line);
                block.push('\n');
            }
        }
        block
    }
}

const BASE: &str = "The pump service shall respond to a health check within 100 ms.";
const ELEMENT: &str = "design/pump.md";

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-satisfy-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        let init = Command::new(req_bin())
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("runs req init");
        assert_eq!(init.status.code(), Some(0), "req init must succeed");
        fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, text).expect("writes the file");
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.0.join(relative)).expect("reads the file")
    }

    /// A process-evidenced claim (no test report needed), applying under `applicability`.
    fn claim(&self, id: &str, statement: &str, applicability: Option<&str>) {
        let applicability = match applicability {
            Some(predicate) => format!("applicability:\n  - {predicate}"),
            None => "applicability: []".to_string(),
        };
        let text = format!(
            "schema: req.claim/2\n\
             claim_id: {id}\n\
             statement: \"{statement}\"\n\
             {applicability}\n\
             relations: []\n\
             evidence: process\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-11\n  rejected_alternatives: []\n  assumptions: []\n"
        );
        self.write(&format!("claims/{id}.yml"), &text);
    }

    /// A design root `design` and one element in it declaring `ids`.
    fn element(&self, relative: &str, ids: &[&str]) {
        self.write(".req/design", "# design roots\ndesign\n");
        let mut text = String::from("# Pump design\n");
        for id in ids {
            text.push_str(&format!("<!-- req-satisfies: {id} -->\n"));
        }
        self.write(relative, &text);
    }

    fn req(&self, args: &[&str]) -> Out {
        let out = Command::new(req_bin())
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("runs req");
        Out {
            code: out.status.code().expect("req exits, never a signal"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// One claim (`REQ-0001`, `BASE`), one element declaring it, and the link recorded.
    fn recorded(name: &str) -> Self {
        let s = Scratch::new(name);
        s.claim("REQ-0001", BASE, None);
        s.element(ELEMENT, &["REQ-0001"]);
        let out = s.req(&["satisfy", "--record", ELEMENT, "REQ-0001"]);
        assert_eq!(out.code, 0, "recording succeeds: {}", out.all());
        s
    }
}

// ---- Falsification 1 -------------------------------------------------------------------------

#[test]
fn a_cosmetic_edit_leaves_a_recorded_link_holding() {
    // Falsification 1. Trailing full stop dropped, a doubled space, and `ms` spelled out: none
    // moves the meaning fingerprint, so none may make the link suspect.
    for edited in [
        "The pump service shall respond to a health check within 100 ms",
        "The pump service shall  respond to a health check within 100 ms.",
        "The pump service shall respond to a health check within 100 milliseconds.",
    ] {
        let s = Scratch::recorded("cosmetic");
        s.claim("REQ-0001", edited, None);
        let out = s.req(&["satisfy"]);
        assert_eq!(out.code, 0, "{edited}: {}", out.all());
        let line = out.line_for(ELEMENT, "REQ-0001");
        assert!(line.contains("HOLDS"), "{edited}: {line}");
        assert!(!line.contains("weakened"), "{edited}: {line}");
    }
}

// ---- Falsification 2 -------------------------------------------------------------------------

#[test]
fn a_strengthening_tightening_flip_or_scope_change_is_suspect_and_names_the_direction() {
    // Falsification 2. Each row is a change a design that met `BASE` may not survive.
    let cases: [(&str, &str, &str); 5] = [
        (
            "The pump service should respond to a health check within 100 ms.",
            "The pump service shall respond to a health check within 100 ms.",
            "modality_strengthening",
        ),
        (
            BASE,
            "The pump service shall respond to a health check within 50 ms.",
            "numeric_tightening",
        ),
        (
            BASE,
            "The pump service shall not respond to a health check within 100 ms.",
            "polarity_flip",
        ),
        (
            BASE,
            "The pump service shall respond to a health check within 100 ms when the tank is full.",
            "scope_narrowing",
        ),
        (
            "The pump service shall respond to a health check within 100 ms when the tank is full.",
            BASE,
            "scope_widening",
        ),
    ];
    for (recorded, changed, direction) in cases {
        let s = Scratch::new("suspect");
        s.claim("REQ-0001", recorded, None);
        s.element(ELEMENT, &["REQ-0001"]);
        assert_eq!(
            s.req(&["satisfy", "--record", ELEMENT, "REQ-0001"]).code,
            0,
            "records"
        );
        s.claim("REQ-0001", changed, None);
        let out = s.req(&["satisfy"]);
        assert_eq!(out.code, 3, "{direction}: {}", out.all());
        let line = out.line_for(ELEMENT, "REQ-0001");
        assert!(line.contains("SUSPECT"), "{direction}: {line}");
        assert!(line.contains(direction), "names the direction: {line}");
    }
}

// ---- Falsification 3 -------------------------------------------------------------------------

#[test]
fn an_admitted_weakening_holds_and_is_named_while_an_unadmitted_loosening_is_suspect() {
    // Falsification 3, both halves. `modality_weakening` is admitted (support 177, zero false
    // carries): the link holds and the report says the requirement loosened. `numeric_loosening` is
    // not (support 9 < 100): the same kind of loosening is suspect, so an unmeasured class never
    // carries a link.
    let s = Scratch::recorded("weakened");
    s.claim(
        "REQ-0001",
        "The pump service should respond to a health check within 100 ms.",
        None,
    );
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let line = out.line_for(ELEMENT, "REQ-0001");
    assert!(
        line.contains("HOLDS") && line.contains("weakened") && line.contains("modality_weakening"),
        "{line}"
    );

    let s = Scratch::recorded("loosened");
    s.claim(
        "REQ-0001",
        "The pump service shall respond to a health check within 200 ms.",
        None,
    );
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let line = out.line_for(ELEMENT, "REQ-0001");
    assert!(
        line.contains("SUSPECT") && line.contains("numeric_loosening"),
        "{line}"
    );
}

// ---- Falsification 4 -------------------------------------------------------------------------

#[test]
fn the_record_stores_the_statement_not_an_address_so_a_rules_bump_cannot_move_it() {
    // Falsification 4. A `RULES_V` bump re-derives the address of the same text; a record that
    // held the address would go stale on it. The record holds the statement verbatim and nothing
    // derived, so it needs no re-recording — checked here on the bytes, and by judging a record
    // written by hand with only the fields the format names.
    let s = Scratch::recorded("rules-bump");
    let record = s.read(".req/satisfied");
    assert!(record.contains("schema: req.satisfied/1"), "{record}");
    assert!(record.contains(BASE), "the statement verbatim: {record}");
    assert!(
        !record.contains("sha256")
            && !record.contains("address")
            && !record.contains("rules_v")
            && !record.contains("date"),
        "no address, rules version or date is stored: {record}"
    );
    assert!(
        record
            .split(|c: char| !c.is_ascii_hexdigit())
            .all(|run| run.len() < 32),
        "no digest of any kind: {record}"
    );

    let by_hand = Scratch::new("by-hand");
    by_hand.claim("REQ-0001", BASE, None);
    by_hand.element(ELEMENT, &["REQ-0001"]);
    by_hand.write(
        ".req/satisfied",
        &format!(
            "schema: req.satisfied/1\nlinks:\n- element: {ELEMENT}\n  claim_id: REQ-0001\n  statement: \"{BASE}\"\n"
        ),
    );
    let out = by_hand.req(&["satisfy"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.line_for(ELEMENT, "REQ-0001").contains("HOLDS"));
}

// ---- Falsification 5 -------------------------------------------------------------------------

#[test]
fn a_link_with_no_record_is_unratified_and_a_link_to_an_unknown_claim_is_unverifiable() {
    // Falsification 5.
    let s = Scratch::new("unratified");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0001"]);
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let line = out.line_for(ELEMENT, "REQ-0001");
    assert!(
        line.contains("UNRATIFIED") && !line.contains("HOLDS"),
        "{line}"
    );

    let s = Scratch::new("unknown-claim");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0099"]);
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let line = out.line_for(ELEMENT, "REQ-0099");
    assert!(
        line.contains("UNKNOWN") && !line.contains("HOLDS"),
        "{line}"
    );
}

#[test]
fn a_token_that_is_not_a_claim_id_is_never_ignored() {
    let s = Scratch::new("bad-token");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["not-an-id"]);
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains("not-an-id"), "{}", out.all());
}

// ---- Falsification 6 -------------------------------------------------------------------------

#[test]
fn req_writes_the_record_only_on_the_explicit_record_act() {
    // Falsification 6. Every command that reads links leaves `.req/satisfied` absent; only
    // `--record` creates it.
    let s = Scratch::new("never-writes");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0001"]);
    for args in [
        vec!["satisfy"],
        vec!["satisfy", "--format", "json"],
        vec!["gate"],
        vec!["check"],
        vec!["audit"],
    ] {
        let _ = s.req(&args);
        assert!(
            !s.path().join(".req/satisfied").exists(),
            "{args:?} wrote .req/satisfied"
        );
    }
    let out = s.req(&["satisfy", "--record", ELEMENT, "REQ-0001"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(s.path().join(".req/satisfied").is_file());
}

// ---- the record, the roots, the grammar ------------------------------------------------------

#[test]
fn record_refuses_an_undeclared_link_and_an_unknown_claim_with_exit_2_and_writes_nothing() {
    let s = Scratch::new("refuse");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0001"]);
    s.write("design/other.md", "no declaration here\n");

    for (args, named) in [
        (
            ["satisfy", "--record", "design/other.md", "REQ-0001"],
            "design/other.md",
        ),
        (["satisfy", "--record", ELEMENT, "REQ-0002"], "REQ-0002"),
    ] {
        let out = s.req(&args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.all());
        assert!(out.stderr.contains(named), "names {named}: {}", out.all());
        assert!(!s.path().join(".req/satisfied").exists());
    }
    // A claim id the corpus does not hold, though declared: also refused.
    s.element("design/ghost.md", &["REQ-0099"]);
    let out = s.req(&["satisfy", "--record", "design/ghost.md", "REQ-0099"]);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.stderr.contains("REQ-0099"), "{}", out.all());
    assert!(!s.path().join(".req/satisfied").exists());
}

#[test]
fn recording_again_replaces_the_one_record_and_keeps_the_list_sorted() {
    let s = Scratch::new("replace");
    s.claim("REQ-0001", BASE, None);
    s.claim("REQ-0002", BASE, None);
    s.write(".req/design", "design\n");
    s.write(
        "design/b.md",
        "req-satisfies: REQ-0002\nreq-satisfies: REQ-0001\n",
    );
    s.write("design/a.md", "req-satisfies: REQ-0001\n");
    for (element, id) in [
        ("design/b.md", "REQ-0002"),
        ("design/b.md", "REQ-0001"),
        ("design/a.md", "REQ-0001"),
    ] {
        assert_eq!(s.req(&["satisfy", "--record", element, id]).code, 0);
    }
    let tightened = "The pump service shall respond to a health check within 50 ms.";
    s.claim("REQ-0001", tightened, None);
    assert_eq!(s.req(&["satisfy"]).code, 3, "the change is suspect");
    assert_eq!(
        s.req(&["satisfy", "--record", "design/a.md", "REQ-0001"])
            .code,
        0
    );
    let record = s.read(".req/satisfied");
    assert_eq!(
        record.matches("element: design/a.md").count(),
        1,
        "{record}"
    );
    assert!(record.contains(tightened), "re-recorded: {record}");
    let order: Vec<usize> = ["design/a.md", "design/b.md"]
        .iter()
        .map(|e| record.find(&format!("element: {e}")).expect("present"))
        .collect();
    assert!(order[0] < order[1], "sorted by element: {record}");
    let out = s.req(&["satisfy"]);
    // design/b.md's REQ-0001 record still holds the old statement: suspect, until re-recorded.
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.line_for("design/a.md", "REQ-0001").contains("HOLDS"));
    assert!(out.line_for("design/b.md", "REQ-0001").contains("SUSPECT"));
    assert!(out.line_for("design/b.md", "REQ-0002").contains("HOLDS"));
}

#[test]
fn an_orphaned_record_is_listed_and_is_not_a_finding() {
    let s = Scratch::recorded("orphan");
    s.write(ELEMENT, "# the declaration was removed\n");
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 0, "an orphan is not a finding: {}", out.all());
    let line = out.line_for(ELEMENT, "REQ-0001");
    assert!(line.contains("ORPHANED"), "{line}");
}

#[test]
fn no_links_at_all_exits_0_and_says_so() {
    let s = Scratch::new("none");
    s.claim("REQ-0001", BASE, None);
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("no satisfaction link"), "{}", out.all());
}

#[test]
fn a_malformed_design_line_is_exit_2_naming_the_line() {
    for (body, bad) in [
        ("design\n/etc\n", ".req/design:2"),
        ("# c\n\n../outside\n", ".req/design:3"),
        ("design/../../x\n", ".req/design:1"),
    ] {
        let s = Scratch::new("malformed");
        s.claim("REQ-0001", BASE, None);
        s.write(".req/design", body);
        let out = s.req(&["satisfy"]);
        assert_eq!(out.code, 2, "{body:?}: {}", out.all());
        assert!(out.all().contains(bad), "{body:?}: {}", out.all());
    }
}

#[test]
fn a_file_under_a_root_that_is_not_utf8_is_unverifiable_and_named() {
    let s = Scratch::new("binary");
    s.claim("REQ-0001", BASE, None);
    s.write(".req/design", "design\n");
    fs::create_dir_all(s.path().join("design")).expect("makes the dir");
    fs::write(
        s.path().join("design/blob.bin"),
        [0xff, 0xfe, 0x00, 0xc3, 0x28],
    )
    .expect("writes the file");
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains("design/blob.bin"), "{}", out.all());
}

#[test]
fn roots_may_be_files_and_directories_are_read_recursively() {
    let s = Scratch::new("roots");
    s.claim("REQ-0001", BASE, None);
    s.write(".req/design", "design/one.md\nlib\n");
    s.write("design/one.md", "req-satisfies: REQ-0001\n");
    s.write("design/two.md", "req-satisfies: REQ-0001\n");
    s.write("lib/deep/er/pump.rs", "// req-satisfies: REQ-0001\n");
    let out = s.req(&["satisfy"]);
    // Both roots are read; `design/two.md` is outside them.
    assert!(out.stdout.contains("design/one.md"), "{}", out.all());
    assert!(out.stdout.contains("lib/deep/er/pump.rs"), "{}", out.all());
    assert!(!out.stdout.contains("design/two.md"), "{}", out.all());
}

#[test]
fn an_unknown_key_in_the_record_is_a_load_error_naming_it() {
    let s = Scratch::recorded("unknown-key");
    let record = s
        .read(".req/satisfied")
        .replace("links:", "address: sha256:abc\nlinks:");
    s.write(".req/satisfied", &record);
    let out = s.req(&["satisfy"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains("address"), "{}", out.all());
}

#[test]
fn the_json_listing_carries_each_link_and_its_outcome() {
    let s = Scratch::recorded("json");
    s.claim(
        "REQ-0001",
        "The pump service should respond to a health check within 100 ms.",
        None,
    );
    let out = s.req(&["satisfy", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("a JSON document");
    assert_eq!(doc["generator"], "satisfy");
    let link = &doc["links"][0];
    assert_eq!(link["element"], ELEMENT);
    assert_eq!(link["claim_id"], "REQ-0001");
    assert_eq!(link["outcome"], "holds_weakened");
    assert_eq!(link["direction"], "modality_weakening");
}

#[test]
fn the_help_page_names_its_exit_codes() {
    let s = Scratch::new("help");
    let out = s.req(&["satisfy", "--help"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("exit:"), "{}", out.stdout);
    assert!(out.stdout.contains("--record"), "{}", out.stdout);
}

// ---- the gate policy -------------------------------------------------------------------------

#[test]
fn the_gate_passes_over_zero_links_and_says_so() {
    let s = Scratch::new("gate-none");
    s.claim("REQ-0001", BASE, None);
    let out = s.req(&["gate"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let block = out.policy_block("design_satisfied");
    assert!(block.starts_with("[PASS] design_satisfied"), "{block}");
    assert!(block.contains("zero"), "says it judged nothing: {block}");
}

#[test]
fn the_gate_names_its_counts_and_passes_on_holding_and_weakened_links() {
    let s = Scratch::new("gate-pass");
    s.claim("REQ-0001", BASE, None);
    s.claim("REQ-0002", BASE, None);
    s.claim("REQ-0003", BASE, None);
    s.write(".req/design", "design\n");
    s.write(
        "design/pump.md",
        "req-satisfies: REQ-0001\nreq-satisfies: REQ-0002\nreq-satisfies: REQ-0003\n",
    );
    for id in ["REQ-0001", "REQ-0002", "REQ-0003"] {
        assert_eq!(s.req(&["satisfy", "--record", ELEMENT, id]).code, 0);
    }
    s.claim(
        "REQ-0002",
        "The pump service should respond to a health check within 100 ms.",
        None,
    );
    s.claim(
        "REQ-0003",
        "The pump service shall respond to a health check within 100 milliseconds.",
        None,
    );
    let out = s.req(&["gate"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let block = out.policy_block("design_satisfied");
    assert!(block.starts_with("[PASS] design_satisfied"), "{block}");
    assert!(
        block.contains("3 link(s) judged")
            && block.contains("2 holding")
            && block.contains("1 weakened"),
        "{block}"
    );
}

#[test]
fn the_gate_is_unverifiable_never_pass_on_an_unratified_link() {
    let s = Scratch::new("gate-unratified");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0001"]);
    let out = s.req(&["gate"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let block = out.policy_block("design_satisfied");
    assert!(
        block.starts_with("[UNVERIFIABLE] design_satisfied"),
        "{block}"
    );
    assert!(
        block.contains(ELEMENT) && block.contains("REQ-0001"),
        "{block}"
    );
}

#[test]
fn the_gate_is_unverifiable_on_a_suspect_link_and_never_fail() {
    let s = Scratch::recorded("gate-suspect");
    s.claim(
        "REQ-0001",
        "The pump service shall respond to a health check within 50 ms.",
        None,
    );
    let out = s.req(&["gate"]);
    assert_eq!(
        out.code,
        3,
        "a suspect link may still comply: {}",
        out.all()
    );
    let block = out.policy_block("design_satisfied");
    assert!(
        block.starts_with("[UNVERIFIABLE] design_satisfied"),
        "{block}"
    );
    assert!(block.contains("numeric_tightening"), "{block}");
}

#[test]
fn the_gate_is_unverifiable_on_a_link_to_an_unknown_claim() {
    let s = Scratch::new("gate-unknown");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0099"]);
    let out = s.req(&["gate"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let block = out.policy_block("design_satisfied");
    assert!(block.contains("REQ-0099"), "{block}");
}

#[test]
fn the_gate_judges_only_the_links_of_applying_claims_and_the_json_names_the_policy() {
    // REQ-0002 does not apply in `mode=a`: its unratified link is not this variant's problem.
    let s = Scratch::new("gate-applying");
    s.claim("REQ-0001", BASE, None);
    s.claim("REQ-0002", BASE, Some("mode == b"));
    // Declares `mode == a` too, so the corpus describes both values the two runs below pass.
    s.claim("REQ-0003", BASE, Some("mode == a"));
    s.write(".req/design", "design\n");
    s.write("design/pump.md", "req-satisfies: REQ-0001\n");
    s.write("design/valve.md", "req-satisfies: REQ-0002\n");
    assert_eq!(s.req(&["satisfy", "--record", ELEMENT, "REQ-0001"]).code, 0);
    let out = s.req(&["gate", "--format", "json", "mode=a"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("a JSON document");
    let policy = doc["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == "design_satisfied")
        .expect("the gate document names design_satisfied");
    assert_eq!(policy["outcome"], "pass");
    assert_eq!(policy["checked"], 1);
    let out = s.req(&["gate", "mode=b"]);
    assert_eq!(out.code, 3, "REQ-0002 applies in b: {}", out.all());
}

#[test]
fn check_inherits_the_policy_per_release_line() {
    let s = Scratch::new("check");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0001"]);
    s.write(".req/variants", "mode=a\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let out = s.req(&["check", "--format", "json"]);
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("a JSON document");
    let gate = doc["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|step| step["kind"] == "gate")
        .expect("a gate step");
    let policy = gate["gate"]["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == "design_satisfied")
        .expect("the gate step carries design_satisfied");
    assert_eq!(policy["outcome"], "unverifiable");
}

#[test]
fn a_malformed_design_list_stops_check_with_exit_2() {
    let s = Scratch::new("check-malformed");
    s.claim("REQ-0001", BASE, None);
    s.write(".req/design", "/absolute\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.all().contains(".req/design:1"), "{}", out.all());
}

// ---- `need design` ---------------------------------------------------------------------------

/// `REQ-0001` always applies, `REQ-0002` applies in `mode=a`, `REQ-0003` only in `mode=b` (declared
/// by no release line here, so it is excluded). One element satisfies `REQ-0001` only.
fn need_design(name: &str) -> Scratch {
    let s = Scratch::new(name);
    s.claim("REQ-0001", BASE, None);
    s.claim("REQ-0002", BASE, Some("mode == a"));
    s.claim("REQ-0003", BASE, Some("mode == b"));
    s.element(ELEMENT, &["REQ-0001"]);
    s.write(".req/variants", "mode=a\n");
    s.write(".req/completeness", "need design\n");
    s
}

fn completeness_block(out: &Out) -> String {
    let mut block = String::new();
    let mut inside = false;
    for line in out.stdout.lines() {
        if line.starts_with('[') {
            inside = line.contains("] completeness ");
        } else if !line.starts_with("    ") {
            inside = false;
        }
        if inside {
            block.push_str(line);
            block.push('\n');
        }
    }
    block
}

#[test]
fn need_design_names_an_applying_claim_no_element_satisfies_and_leaves_an_excluded_one_alone() {
    let s = need_design("need-design");
    let out = s.req(&["check"]);
    let block = completeness_block(&out);
    assert!(
        block.starts_with("[FAIL] completeness "),
        "REQ-0003 is in no release (FAIL), the unsatisfied one is UNVERIFIABLE: {block}"
    );
    let line = block
        .lines()
        .find(|l| l.contains("unsatisfied"))
        .unwrap_or_else(|| panic!("an unsatisfied note: {block}"));
    assert!(line.contains("REQ-0002"), "{line}");
    assert!(!line.contains("REQ-0001"), "REQ-0001 is satisfied: {line}");
    assert!(
        !line.contains("REQ-0003"),
        "REQ-0003 is excluded from the declared release: {line}"
    );
}

#[test]
fn need_design_alone_is_unverifiable_exit_3_and_json_carries_the_list() {
    let s = need_design("need-design-3");
    // Make REQ-0003 a legitimate release member so only `unsatisfied` is left to say.
    s.write(".req/variants", "mode=a\nmode=b\n");
    s.element("design/more.md", &["REQ-0002"]);
    let out = s.req(&["check"]);
    let block = completeness_block(&out);
    let line = block
        .lines()
        .find(|l| l.contains("unsatisfied"))
        .unwrap_or_else(|| panic!("an unsatisfied note: {block}"));
    assert!(line.contains("REQ-0003"), "{line}");
    assert!(
        !line.contains("REQ-0002"),
        "REQ-0002 has an element: {line}"
    );

    let out = s.req(&["check", "--format", "json"]);
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("a JSON document");
    let step = doc["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|step| step["kind"] == "completeness")
        .expect("a completeness step");
    assert_eq!(step["completeness"]["unsatisfied"][0], "REQ-0003");
}

#[test]
fn without_need_design_no_claim_is_called_unsatisfied() {
    let s = need_design("no-need");
    s.write(".req/completeness", "# nothing\n");
    let out = s.req(&["check"]);
    assert!(!out.stdout.contains("unsatisfied"), "{}", out.all());
}

#[test]
fn a_satisfied_corpus_has_nothing_unsatisfied() {
    let s = Scratch::new("all-satisfied");
    s.claim("REQ-0001", BASE, None);
    s.element(ELEMENT, &["REQ-0001"]);
    s.write(".req/completeness", "need design\n");
    let out = s.req(&["check"]);
    let block = completeness_block(&out);
    assert!(!block.contains("unsatisfied"), "{block}");
}

#[test]
fn the_audit_lists_unsatisfied_claims_under_structural_omissions_when_need_design_is_set() {
    let s = need_design("audit");
    let out = s.req(&["audit", "mode=a"]);
    let section = out
        .stdout
        .split("## Structural omissions")
        .nth(1)
        .unwrap_or_else(|| panic!("a Structural omissions section: {}", out.all()));
    let line = section
        .lines()
        .find(|l| l.contains("unsatisfied"))
        .unwrap_or_else(|| panic!("an unsatisfied line: {section}"));
    assert!(line.contains("REQ-0002"), "{line}");
    assert!(!line.contains("REQ-0001"), "{line}");
}

#[test]
fn an_unknown_word_after_need_is_exit_2_naming_the_line() {
    let s = Scratch::new("need-bad");
    s.claim("REQ-0001", BASE, None);
    s.write(".req/completeness", "need tests\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.all().contains(".req/completeness:1"), "{}", out.all());
}
