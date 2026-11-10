//! Issue #20: `req gate --format json` carries machine-readable counts for `tests_linked`, the text
//! report prints the same one-line summary. (`req canon`'s last line already carries the claim and
//! unstructured totals, and is pinned by `tools/quickstart.sh` and `docs/adopting.md`; it is unchanged.)
//!
//! The counts exist so a consumer never has to parse the prose (`no linked test` lines) and count
//! files to know how much of a corpus is evidenced. Every number below is derived by hand from the
//! fixture's claim list, never read back from the binary.
//!
//! Definitions the tests pin (and `req gate --help` states):
//!   applying           the claims the invocation's context leaves applying;
//!   process_evidenced  of those, the ones declaring `evidence: process`;
//!   linked / unlinked  of the rest (machine-evidenced), with and without a `tests:` link, so
//!                      `applying = linked + unlinked + process_evidenced`;
//!   passed / failed / not_in_report
//!                      the linked claims by verdict (`passed + failed + not_in_report = linked`),
//!                      or `null` when no usable report was supplied: a claim that was not judged
//!                      is not counted as judged.

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

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "req-gate-counts-{name}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the fixture directory");
        let (code, out, err) = req(&dir, &["init"]);
        assert_eq!(code, 0, "init: {out}{err}");
        // The starter claim would add one more evidenced-by-nothing claim to every count.
        fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        Dir(dir)
    }

    /// One claim: `tests` links, `evidence` mode, and an applicability predicate (or none).
    fn claim(&self, id: &str, tests: &[&str], process: bool, applicability: Option<&str>) {
        let tests_block = if tests.is_empty() {
            String::new()
        } else {
            let mut block = String::from("tests:\n");
            for test in tests {
                block.push_str(&format!("  - \"{test}\"\n"));
            }
            block
        };
        let evidence = if process { "evidence: process\n" } else { "" };
        let applicability = match applicability {
            Some(predicate) => format!("applicability:\n  - {predicate}\n"),
            None => "applicability: []\n".to_string(),
        };
        let text = format!(
            "schema: req.claim/2\nclaim_id: {id}\n\
             statement: \"The {id} service shall respond within 200 ms.\"\n\
             {applicability}relations: []\n{tests_block}{evidence}\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-10\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    fn report(&self, cases: &[(&str, &str)]) -> String {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"t\">\n<testsuite name=\"t\">\n",
        );
        for (id, state) in cases {
            let (class, name) = id.rsplit_once("::").expect("an id with ::");
            let child = match *state {
                "pass" => "",
                "fail" => "<failure message=\"x\"/>",
                "skip" => "<skipped/>",
                other => panic!("unknown state {other}"),
            };
            xml.push_str(&format!(
                "<testcase name=\"{name}\" classname=\"{class}\">{child}</testcase>\n"
            ));
        }
        xml.push_str("</testsuite>\n</testsuites>\n");
        let path = self.0.join("junit.xml");
        fs::write(&path, xml).expect("writes the report");
        path.to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Eight applying claims, one excluded by the context. By hand:
///   0001 links a passing test                          linked, passed
///   0002 links a failing test                          linked, failed
///   0003 links a test the report marks skipped         linked, failed (a skip is not a pass)
///   0004 links a test the report does not carry        linked, not in the report
///   0005 links nothing                                 unlinked
///   0006 declares `evidence: process`                  process-evidenced
///   0007 links a passing and an absent test            linked, not in the report (absent outranks)
///   0008 applies only to `internet` (context: air_gapped)   not counted at all
///   0009 applies to `air_gapped`, links a passing test linked, passed
fn mixed_corpus() -> Dir {
    let d = Dir::new("mixed");
    d.claim("REQ-0001", &["t::pass_a"], false, None);
    d.claim("REQ-0002", &["t::fail_b"], false, None);
    d.claim("REQ-0003", &["t::skip_c"], false, None);
    d.claim("REQ-0004", &["t::gone_d"], false, None);
    d.claim("REQ-0005", &[], false, None);
    d.claim("REQ-0006", &[], true, None);
    d.claim("REQ-0007", &["t::pass_a", "t::gone_e"], false, None);
    d.claim(
        "REQ-0008",
        &["t::pass_a"],
        false,
        Some("deployment_mode == internet"),
    );
    d.claim(
        "REQ-0009",
        &["t::pass_a"],
        false,
        Some("deployment_mode == air_gapped"),
    );
    d
}

fn tests_linked(document: &Value) -> &Value {
    document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == "tests_linked")
        .expect("the tests_linked policy")
}

fn gate_json(d: &Dir, with_report: bool) -> Value {
    let mut args = vec!["gate", "--format", "json"];
    let report;
    if with_report {
        report = d.report(&[
            ("t::pass_a", "pass"),
            ("t::fail_b", "fail"),
            ("t::skip_c", "skip"),
        ]);
        args.push("--test-report");
        args.push(&report);
    }
    args.push("deployment_mode=air_gapped");
    let (_, out, err) = req(&d.0, &args);
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}\n{err}"))
}

#[test]
fn the_counts_are_the_claims_by_verdict_and_add_up() {
    let d = mixed_corpus();
    let document = gate_json(&d, true);
    let counts = &tests_linked(&document)["counts"];
    assert_eq!(
        *counts,
        serde_json::json!({
            "applying": 8,
            "linked": 6,
            "unlinked": 1,
            "process_evidenced": 1,
            "passed": 2,
            "failed": 2,
            "not_in_report": 2,
        }),
        "{counts}"
    );
    let n = |key: &str| counts[key].as_u64().expect(key);
    assert_eq!(
        n("applying"),
        n("linked") + n("unlinked") + n("process_evidenced")
    );
    assert_eq!(n("linked"), n("passed") + n("failed") + n("not_in_report"));
}

#[test]
fn without_a_report_the_verdict_counts_are_null_not_zero() {
    let d = mixed_corpus();
    let document = gate_json(&d, false);
    let counts = &tests_linked(&document)["counts"];
    // What needs no report is still known.
    assert_eq!(counts["applying"], 8);
    assert_eq!(counts["linked"], 6);
    assert_eq!(counts["unlinked"], 1);
    assert_eq!(counts["process_evidenced"], 1);
    // What needs one was not judged: zero would say "none failed".
    for key in ["passed", "failed", "not_in_report"] {
        assert!(counts[key].is_null(), "{key} must be null: {counts}");
    }
}

#[test]
fn a_corpus_of_process_claims_alone_needs_no_report_and_judges_zero_machine_claims() {
    let d = Dir::new("process-only");
    d.claim("REQ-0001", &[], true, None);
    d.claim("REQ-0002", &[], true, None);
    let (code, out, err) = req(&d.0, &["gate", "--format", "json"]);
    assert_eq!(code, 0, "{out}{err}");
    let document: Value = serde_json::from_str(&out).expect("JSON");
    assert_eq!(
        tests_linked(&document)["counts"],
        serde_json::json!({
            "applying": 2, "linked": 0, "unlinked": 0, "process_evidenced": 2,
            "passed": 0, "failed": 0, "not_in_report": 0,
        })
    );
}

#[test]
fn only_tests_linked_carries_counts() {
    let d = mixed_corpus();
    let document = gate_json(&d, true);
    for policy in document["policies"].as_array().expect("policies") {
        if policy["policy"] != "tests_linked" {
            assert!(
                policy.get("counts").is_none(),
                "{} must not carry counts",
                policy["policy"]
            );
        }
    }
}

#[test]
fn the_text_report_prints_the_same_summary_line() {
    let d = mixed_corpus();
    let report = d.report(&[
        ("t::pass_a", "pass"),
        ("t::fail_b", "fail"),
        ("t::skip_c", "skip"),
    ]);
    let (_, out, _) = req(
        &d.0,
        &[
            "gate",
            "--test-report",
            &report,
            "deployment_mode=air_gapped",
        ],
    );
    assert!(
        out.lines().any(|l| l.trim()
            == "counts: 8 applying · 6 linked · 1 unlinked · 1 process-evidenced · 2 passed · 2 failed · 2 not in the report"),
        "{out}"
    );
    // Without a report the verdict counts are said not to be judged.
    let (_, out, _) = req(&d.0, &["gate", "deployment_mode=air_gapped"]);
    assert!(
        out.lines().any(|l| l.trim()
            == "counts: 8 applying · 6 linked · 1 unlinked · 1 process-evidenced · passed/failed/not in the report: not judged (no usable report)"),
        "{out}"
    );
}

#[test]
fn json_and_text_agree_on_the_verdict_with_the_counts_added() {
    // The counts are additive: same verdict, same exit code in both renderings.
    let d = mixed_corpus();
    let report = d.report(&[
        ("t::pass_a", "pass"),
        ("t::fail_b", "fail"),
        ("t::skip_c", "skip"),
    ]);
    let (text_code, _, _) = req(
        &d.0,
        &[
            "gate",
            "--test-report",
            &report,
            "deployment_mode=air_gapped",
        ],
    );
    let (json_code, _, _) = req(
        &d.0,
        &[
            "gate",
            "--format",
            "json",
            "--test-report",
            &report,
            "deployment_mode=air_gapped",
        ],
    );
    assert_eq!(text_code, 1, "a failing linked test fails the gate");
    assert_eq!(text_code, json_code);
}

#[test]
fn the_text_line_names_process_evidence_only_when_there_is_some() {
    // The report's words "process-evidenced" mean a claim declares it (tests/release_gate.rs pins
    // that an empty `tests:` list never produces them); a zero is carried by the JSON instead.
    let d = Dir::new("no-process");
    d.claim("REQ-0001", &["t::pass_a"], false, None);
    d.claim("REQ-0002", &[], false, None);
    let report = d.report(&[("t::pass_a", "pass")]);
    let (_, out, _) = req(&d.0, &["gate", "--test-report", &report]);
    let line = out
        .lines()
        .find(|l| l.trim_start().starts_with("counts:"))
        .unwrap_or_else(|| panic!("no counts line: {out}"));
    assert_eq!(
        line.trim(),
        "counts: 2 applying · 1 linked · 1 unlinked · 1 passed · 0 failed · 0 not in the report"
    );
    assert!(!out.contains("process-evidenced"), "{out}");
}
