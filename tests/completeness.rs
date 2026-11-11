//! Structural omissions (ADR 0039): `req check`'s `completeness` step and the audit packet's section.
//!
//! The public seam is the compiled `req` against hand-written fixture corpora in a temp dir. The oracle
//! is the fixture itself: which claim applies in which declared release, which manifest names which
//! claim, and which claim carries a relation are all written here, never read back from req.
//!
//! The three findings and their answers:
//! * `in_no_release`  — applies in no `.req/variants` line → FAIL (1);
//! * `in_no_baseline` — applies in a line, named by no signed manifest → UNVERIFIABLE (3);
//! * `unrelated`      — no relation in or out → reported only, FAIL under `strict unrelated`.

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

    /// The completeness step's line and the indented notes under it.
    fn completeness_block(&self) -> String {
        let mut block = String::new();
        let mut inside = false;
        for line in self.stdout.lines() {
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
}

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-completeness-{}-{}-{name}",
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

    /// A process-evidenced claim (no test report needed), applying under `applicability` (always when
    /// `None`), with outgoing relations `relations` (`(kind, to)`).
    fn claim(&self, id: &str, applicability: Option<&str>, relations: &[(&str, &str)]) {
        let applicability = match applicability {
            Some(predicate) => format!("applicability:\n  - {predicate}"),
            None => "applicability: []".to_string(),
        };
        let relations = if relations.is_empty() {
            "relations: []".to_string()
        } else {
            let mut text = "relations:".to_string();
            for (kind, to) in relations {
                text.push_str(&format!("\n  - type: {kind}\n    to: {to}"));
            }
            text
        };
        let text = format!(
            "schema: req.claim/2\n\
             claim_id: {id}\n\
             statement: \"The {id} service shall respond to a health check within 200 ms.\"\n\
             {applicability}\n\
             {relations}\n\
             evidence: process\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-11\n  rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, text).expect("writes the file");
    }

    /// A v4 manifest naming `ids`, hand-written: the step reads which claims a Baseline names, and
    /// whether its hashes still match is `req baseline --verify`'s question, not this one.
    fn manifest(&self, name: &str, ids: &[&str], signed: bool) {
        let claims: Vec<String> = ids
            .iter()
            .map(|id| {
                format!(
                    "{{\"claim_id\":\"{id}\",\"address\":\"{h}\",\"relations_hash\":\"{h}\",\
                     \"rationale_hash\":\"{h}\",\"tests_hash\":\"{h}\"}}",
                    h = "0".repeat(64)
                )
            })
            .collect();
        let text = format!(
            "{{\"version\":4,\"signed_at\":\"2026-11-11T00:00:00Z\",\"authority\":{{\"head\":\"{h}\",\
             \"entries\":1}},\"rules_v\":8,\"selection\":{{}},\"claims\":[{}]}}",
            claims.join(","),
            h = "0".repeat(64)
        );
        self.write(&format!("baselines/{name}"), &text);
        if signed {
            self.write(
                &format!("baselines/{name}.minisig"),
                "untrusted comment: fixture\n",
            );
        }
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
}

/// Two releases, `mode=a` and `mode=b`. `REQ-0001` always applies and is the root everything refines;
/// `REQ-0002` applies in `a`, `REQ-0003` in `b`. Every claim is related and every claim is signed, so
/// the step passes; each test then breaks exactly one thing.
fn clean(name: &str) -> Scratch {
    let s = Scratch::new(name);
    s.claim("REQ-0001", None, &[]);
    s.claim("REQ-0002", Some("mode == a"), &[("refines", "REQ-0001")]);
    s.claim("REQ-0003", Some("mode == b"), &[("refines", "REQ-0001")]);
    s.write(".req/variants", "mode=a\nmode=b\n");
    s.manifest("release.json", &["REQ-0001", "REQ-0002", "REQ-0003"], true);
    s
}

#[test]
fn a_clean_repository_passes_and_says_what_it_checked() {
    let s = clean("clean");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[PASS] completeness "), "{block}");
    assert!(block.contains("no structural omission found"), "{block}");
}

#[test]
fn a_claim_that_applies_in_no_release_is_exit_1_and_named() {
    let s = clean("no-release");
    s.claim("REQ-0004", Some("mode == c"), &[("refines", "REQ-0001")]);
    s.manifest(
        "release.json",
        &["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"],
        true,
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[FAIL] completeness "), "{block}");
    assert!(
        block
            .lines()
            .any(|l| l.contains("in_no_release") && l.contains("REQ-0004")),
        "{block}"
    );
    assert!(
        !block.contains("REQ-0002"),
        "a claim some line examines is not named: {block}"
    );
}

#[test]
fn a_claim_in_a_release_but_in_no_baseline_is_exit_3_and_named() {
    let s = clean("no-baseline");
    s.manifest("release.json", &["REQ-0001", "REQ-0002"], true);
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[UNVERIFIABLE] completeness "), "{block}");
    assert!(
        block
            .lines()
            .any(|l| l.contains("in_no_baseline") && l.contains("REQ-0003")),
        "{block}"
    );
    assert!(
        !block
            .lines()
            .any(|l| l.contains("in_no_baseline") && l.contains("REQ-0002")),
        "{block}"
    );
}

#[test]
fn no_baseline_at_all_is_exit_3_and_says_nothing_is_ratified() {
    let s = clean("none-at-all");
    fs::remove_dir_all(s.path().join("baselines")).expect("removes the baselines");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.contains("no signed Baseline"), "{block}");
    for id in ["REQ-0001", "REQ-0002", "REQ-0003"] {
        assert!(block.contains(id), "{id} is named: {block}");
    }
}

#[test]
fn a_manifest_with_no_signature_beside_it_covers_nothing() {
    let s = clean("unsigned");
    fs::remove_file(s.path().join("baselines/release.json.minisig"))
        .expect("removes the signature");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let block = out.completeness_block();
    assert!(
        block.contains("baselines/release.json") && block.contains("no signature"),
        "the unsigned manifest is named: {block}"
    );
    assert!(
        block
            .lines()
            .any(|l| l.contains("in_no_baseline") && l.contains("REQ-0001")),
        "{block}"
    );
}

/// Review A-1-review-1 (low): a DIRECTORY named like a party signature is not a signature, so the
/// manifest beside it is not counted as signed.
#[test]
fn a_directory_named_like_a_signature_does_not_count() {
    let s = clean("keyed-dir");
    fs::remove_file(s.path().join("baselines/release.json.minisig")).expect("removes the default");
    fs::create_dir_all(s.path().join("baselines/release.json.oem-ro.minisig"))
        .expect("makes the directory");
    let out = s.req(&["check"]);
    let block = out.completeness_block();
    assert!(
        block.starts_with("[UNVERIFIABLE] completeness "),
        "a directory is not a signature: {block}"
    );
}

#[test]
fn a_manifest_signed_only_by_a_keyed_party_signature_counts() {
    // ADR 0042: `<manifest>.<key-id>.minisig` is a signature too. Whether it verifies is
    // `req baseline --verify`'s question; this step reads which claims a signed manifest names.
    let s = clean("keyed-only");
    fs::remove_file(s.path().join("baselines/release.json.minisig")).expect("removes the default");
    s.write(
        "baselines/release.json.oem-ro.minisig",
        "untrusted comment: fixture\n",
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[PASS] completeness "), "{block}");
}

#[test]
fn an_unsigned_manifest_note_names_both_signature_spellings() {
    let s = clean("note-spellings");
    fs::remove_file(s.path().join("baselines/release.json.minisig")).expect("removes the default");
    let out = s.req(&["check"]);
    let block = out.completeness_block();
    assert!(
        block.contains("release.json.minisig") && block.contains("release.json.<key-id>.minisig"),
        "{block}"
    );
}

#[test]
fn a_manifest_that_cannot_be_read_is_exit_3_and_named() {
    let s = clean("corrupt");
    s.write("baselines/broken.json", "{not json");
    s.write(
        "baselines/broken.json.minisig",
        "untrusted comment: fixture\n",
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.completeness_block().contains("baselines/broken.json"),
        "{}",
        out.all()
    );
}

#[test]
fn a_failure_outranks_an_open_question_inside_the_step() {
    let s = clean("both");
    s.claim("REQ-0004", Some("mode == c"), &[("refines", "REQ-0001")]);
    s.manifest("release.json", &["REQ-0001", "REQ-0002"], true);
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    let block = out.completeness_block();
    assert!(
        block.contains("in_no_release") && block.contains("in_no_baseline"),
        "{block}"
    );
}

#[test]
fn an_unrelated_claim_is_reported_and_does_not_change_the_verdict() {
    let s = clean("unrelated");
    s.claim("REQ-0004", None, &[]);
    s.manifest(
        "release.json",
        &["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"],
        true,
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let block = out.completeness_block();
    assert!(
        block
            .lines()
            .any(|l| l.contains("unrelated") && l.contains("REQ-0004")),
        "{block}"
    );
    // REQ-0001 has no outgoing relation, but two claims refine it: an incoming edge counts.
    assert!(
        !block
            .lines()
            .any(|l| l.contains("unrelated") && l.contains("REQ-0001")),
        "{block}"
    );
}

#[test]
fn strict_unrelated_fails_an_unlisted_claim_and_spares_a_declared_root() {
    let s = clean("strict");
    s.claim("REQ-0004", None, &[]);
    s.claim("REQ-0005", None, &[]);
    s.manifest(
        "release.json",
        &["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004", "REQ-0005"],
        true,
    );
    s.write(".req/completeness", "# roots\nREQ-0005\nstrict unrelated\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    let block = out.completeness_block();
    assert!(
        block
            .lines()
            .any(|l| l.contains("unrelated") && l.contains("REQ-0004")),
        "{block}"
    );
    assert!(
        !block.contains("REQ-0005"),
        "a declared root is not reported: {block}"
    );
}

#[test]
fn a_declared_root_alone_is_not_reported_and_nothing_is_promoted() {
    let s = clean("root-only");
    s.claim("REQ-0004", None, &[]);
    s.manifest(
        "release.json",
        &["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"],
        true,
    );
    s.write(".req/completeness", "REQ-0004\r\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        !out.completeness_block().contains("REQ-0004"),
        "{}",
        out.all()
    );
}

#[test]
fn an_unknown_word_or_id_in_the_completeness_list_is_exit_2_and_nothing_runs() {
    for (body, named) in [
        ("strict everything\n", ".req/completeness:1"),
        ("REQ-0001\nREQ-0099\n", ".req/completeness:2"),
        ("REQ-0001 REQ-0002\n", ".req/completeness:1"),
    ] {
        let s = clean("bad-list");
        s.write(".req/completeness", body);
        let out = s.req(&["check"]);
        assert_eq!(out.code, 2, "{body:?}: {}", out.all());
        assert!(out.stderr.contains(named), "{body:?}: {}", out.stderr);
        assert!(
            !out.stdout.contains("] gate "),
            "nothing ran: {}",
            out.stdout
        );
    }
}

#[test]
fn no_output_calls_the_requirements_complete() {
    for name in ["clean", "no-baseline"] {
        let s = clean(name);
        if name == "no-baseline" {
            s.manifest("release.json", &["REQ-0001"], true);
        }
        let out = s.req(&["check"]);
        for word in out
            .all()
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        {
            assert_ne!(word.to_ascii_lowercase(), "complete", "{}", out.all());
        }
    }
}

#[test]
fn the_json_step_carries_the_three_lists() {
    let s = clean("json");
    s.claim("REQ-0004", Some("mode == c"), &[]);
    let out = s.req(&["check", "--format", "json"]);
    assert_eq!(out.code, 1, "{}", out.all());
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("one JSON document");
    let step = doc["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|step| step["kind"] == "completeness")
        .expect("a completeness step");
    assert_eq!(step["exit"], 1);
    assert_eq!(
        step["completeness"]["in_no_release"],
        serde_json::json!(["REQ-0004"])
    );
    assert_eq!(
        step["completeness"]["in_no_baseline"],
        serde_json::json!([])
    );
    assert_eq!(
        step["completeness"]["unrelated"],
        serde_json::json!(["REQ-0004"])
    );
    assert!(
        step.get("path").is_none() && step.get("line").is_none(),
        "{step}"
    );
}

#[test]
fn the_default_variant_is_the_release_when_no_list_declares_one() {
    let s = Scratch::new("default");
    s.claim("REQ-0001", None, &[]);
    s.manifest("release.json", &["REQ-0001"], true);
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[PASS] completeness "), "{block}");
    assert!(
        block
            .lines()
            .any(|l| l.contains("unrelated") && l.contains("REQ-0001")),
        "{block}"
    );
}

#[test]
fn the_audit_packet_lists_unrelated_claims_in_its_selection() {
    let s = clean("audit");
    s.claim("REQ-0004", Some("mode == a"), &[]);
    s.claim("REQ-0005", Some("mode == b"), &[]);
    let out = s.req(&["audit", "mode=a"]);
    let section = out
        .stdout
        .split("## Structural omissions")
        .nth(1)
        .unwrap_or_else(|| panic!("a section of its own: {}", out.stdout));
    assert!(section.contains("REQ-0004"), "{section}");
    assert!(
        !section.contains("REQ-0005"),
        "a claim outside the selection is not listed: {section}"
    );
    assert!(
        !section.contains("REQ-0002"),
        "a related claim is not listed: {section}"
    );
}

#[test]
fn a_claim_a_line_leaves_undecided_is_the_gates_open_question_not_a_defect() {
    let s = clean("undecided");
    s.claim("REQ-0004", Some("region == eu"), &[("refines", "REQ-0001")]);
    s.manifest(
        "release.json",
        &["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"],
        true,
    );
    let out = s.req(&["check"]);
    assert_eq!(
        out.code,
        3,
        "the gate's UNVERIFIABLE, never outranked by a FAIL: {}",
        out.all()
    );
    let block = out.completeness_block();
    assert!(block.starts_with("[PASS] completeness "), "{block}");
    assert!(!block.contains("in_no_release"), "{block}");
}
