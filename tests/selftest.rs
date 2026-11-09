//! `req selftest` (ADR 0034): known-answer checks of the installed binary.
//!
//! Two seams. The compiled `req` is run in a directory that holds no corpus, which is the point of the
//! command: it answers anywhere, reads nothing, and is the thing a user runs after installing to learn
//! that this build, on this machine, derives what the project's reference build derives. And the
//! library is driven with a **tampered table**, because a check that cannot fail proves nothing: for each
//! kind of check one answer is made wrong and that check, and only it, must report FAIL.
//!
//! The expectations in the table come from the hashing spec's worked examples and from the parity
//! corpus's labels (written by hand, independent of the implementation); the address goldens are
//! regression pins and are said to be that.

use req::canon::RULES_V;
use req::selftest::{Check, KnownAnswers, known_answers, run_checks};
use std::path::{Path, PathBuf};
use std::process::Command;

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// A directory with no `claims/` in it or above it.
fn empty_dir() -> PathBuf {
    for base in [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ] {
        if base.is_dir()
            && !base
                .ancestors()
                .any(|d| req::root::is_corpus_root(d) || d.join(".git").exists())
        {
            let dir = base.join(format!("req-selftest-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("creates");
            return std::fs::canonicalize(dir).expect("resolves");
        }
    }
    panic!("no directory outside a checkout");
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn req(cwd: &Path, args: &[&str]) -> Run {
    let out = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn failures(checks: &[Check]) -> Vec<&Check> {
    checks.iter().filter(|c| !c.ok).collect()
}

// ---------------------------------------------------------------------------------------------
// the command
// ---------------------------------------------------------------------------------------------

#[test]
fn selftest_passes_in_a_directory_with_no_corpus_and_names_the_build() {
    let dir = empty_dir();
    let run = req(&dir, &["selftest"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(
        !run.stderr.contains("corpus root:"),
        "it reads no corpus and resolves no root: {}",
        run.stderr
    );
    assert!(run.stdout.contains("selftest: PASS"), "{}", run.stdout);
    assert!(
        run.stdout.contains(&format!("rules_v {RULES_V}")) && run.stdout.contains("built from"),
        "the build that was checked is named: {}",
        run.stdout
    );
}

#[test]
fn the_summary_counts_exactly_the_lines_it_reports_as_checks() {
    let run = req(&empty_dir(), &["selftest"]);
    let passes = run
        .stdout
        .lines()
        .filter(|l| l.starts_with("PASS "))
        .count();
    let summary = run
        .stdout
        .lines()
        .find(|l| l.starts_with("selftest: PASS"))
        .unwrap_or_else(|| panic!("no summary: {}", run.stdout));
    assert!(
        passes > 10,
        "a table this small proves little: {passes} checks"
    );
    assert!(
        summary.contains(&format!("{passes} checks")),
        "{summary} vs {passes} PASS lines"
    );
    assert!(
        run.stdout
            .lines()
            .filter(|l| l.starts_with("info:"))
            .all(|l| !l.starts_with("PASS")),
        "environment notes are not checks"
    );
}

#[test]
fn selftest_has_a_page_and_takes_no_arguments_and_no_root() {
    let dir = empty_dir();
    let page = req(&dir, &["selftest", "--help"]);
    assert_eq!(page.code, 0, "{}{}", page.stdout, page.stderr);
    for needle in [
        "usage: req selftest",
        "exit:",
        "known-answer",
        "not a defence",
    ] {
        assert!(
            page.stdout.contains(needle),
            "the page lacks `{needle}`: {}",
            page.stdout
        );
    }
    let extra = req(&dir, &["selftest", "extra"]);
    assert_eq!(extra.code, 2, "{}{}", extra.stdout, extra.stderr);
    assert!(
        extra.stderr.contains("usage: req selftest"),
        "{}",
        extra.stderr
    );
    let rooted = req(&dir, &["--root", dir.to_str().expect("utf-8"), "selftest"]);
    assert_eq!(
        rooted.code, 2,
        "a root has nothing to name here: {}{}",
        rooted.stdout, rooted.stderr
    );
}

// ---------------------------------------------------------------------------------------------
// the table, and that each kind of check can fail
// ---------------------------------------------------------------------------------------------

#[test]
fn every_check_passes_on_the_embedded_answers() {
    let checks = run_checks(&known_answers()).expect("the embedded data parses");
    assert!(failures(&checks).is_empty(), "{:?}", failures(&checks));
}

#[test]
fn the_table_covers_the_rule_classes_the_safety_case_leans_on() {
    let k = known_answers();
    let ops: Vec<&str> = k.fields.iter().map(|f| f.operator.as_str()).collect();
    for needed in ["LE", "LT", "IN", "none"] {
        assert!(
            ops.contains(&needed),
            "no field row with operator {needed}: {ops:?}"
        );
    }
    assert!(
        k.fields.iter().any(|f| f.modality == "SHALL-NOT"),
        "a prohibition"
    );
    assert!(
        k.goldens.iter().any(|(s, _)| s.contains("friction-free")),
        "an unstructured fallback"
    );
    assert!(
        !k.same.is_empty() && !k.different.is_empty(),
        "equal and different pairs"
    );
    assert!(
        k.different
            .iter()
            .any(|(a, b)| a.contains("less than") != b.contains("less than")),
        "the LE-versus-LT trap is a pair that must stay apart"
    );
    let labels: Vec<&str> = k.directions.iter().map(|d| d.expected.as_str()).collect();
    for needed in [
        "numeric_loosening",
        "numeric_tightening",
        "modality_weakening",
        "polarity_flip",
        "scope_narrowing",
        "membership_narrowing",
    ] {
        assert!(
            labels.contains(&needed),
            "no direction row labelled {needed}: {labels:?}"
        );
    }
}

fn only_failing(k: &KnownAnswers, kind: &str) -> usize {
    let checks = run_checks(k).expect("still parses");
    let bad = failures(&checks);
    assert!(!bad.is_empty(), "the tampered table must fail something");
    assert!(
        bad.iter().all(|c| c.kind == kind),
        "only `{kind}` checks may fail, got {:?}",
        bad.iter().map(|c| (&c.kind, &c.name)).collect::<Vec<_>>()
    );
    bad.len()
}

#[test]
fn a_wrong_digest_answer_fails_the_digest_check() {
    let mut k = known_answers();
    k.digest_expected = "0".repeat(64);
    assert_eq!(only_failing(&k, "digest"), 1);
}

#[test]
fn a_wrong_field_answer_fails_that_row() {
    let mut k = known_answers();
    let row = k
        .fields
        .iter_mut()
        .find(|f| f.operator == "LE")
        .expect("an LE row");
    row.operator = "LT".to_owned();
    assert_eq!(only_failing(&k, "fields"), 1);
}

#[test]
fn a_wrong_unit_or_member_set_fails_its_row() {
    let mut k = known_answers();
    k.fields
        .iter_mut()
        .find(|f| f.unit.is_some())
        .expect("a unit row")
        .unit = Some("ms".to_owned());
    assert_eq!(only_failing(&k, "fields"), 1);
    let mut k = known_answers();
    k.fields
        .iter_mut()
        .find(|f| !f.members.is_empty())
        .expect("a set row")
        .members
        .pop();
    assert_eq!(only_failing(&k, "fields"), 1);
}

#[test]
fn a_pair_that_must_be_equal_but_is_not_fails() {
    let mut k = known_answers();
    k.same.push((
        "The system shall respond within 200 ms.".to_owned(),
        "The system shall respond within 300 ms.".to_owned(),
    ));
    assert_eq!(only_failing(&k, "same"), 1);
}

#[test]
fn a_pair_that_must_stay_apart_but_collapses_fails() {
    let mut k = known_answers();
    k.different.push((
        "The system shall respond within 200 ms.".to_owned(),
        "The system shall respond no later than 0.2 seconds.".to_owned(),
    ));
    assert_eq!(only_failing(&k, "different"), 1);
}

#[test]
fn a_wrong_address_golden_fails_and_names_the_statement() {
    let mut k = known_answers();
    k.goldens[0].1 = "f".repeat(64);
    let checks = run_checks(&k).expect("parses");
    let bad = failures(&checks);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(bad[0].kind, "address");
    assert!(
        bad[0].detail.contains(&k.goldens[0].0) || bad[0].name.contains(&k.goldens[0].0),
        "{:?}",
        bad[0]
    );
}

#[test]
fn goldens_recorded_under_another_rules_version_are_refused_even_if_every_address_matches() {
    let mut k = known_answers();
    k.golden_rules_v = RULES_V - 1;
    let checks = run_checks(&k).expect("parses");
    let bad = failures(&checks);
    assert!(
        bad.iter()
            .any(|c| c.kind == "address" && c.detail.contains("rules_v")),
        "the goldens are for another rule set, which no address match can excuse: {bad:?}"
    );
}

#[test]
fn a_wrong_direction_label_fails_that_row() {
    let mut k = known_answers();
    k.directions[0].expected = "scope_widening".to_owned();
    assert_eq!(only_failing(&k, "direction"), 1);
}

#[test]
fn a_false_merge_in_the_embedded_parity_corpus_fails_the_benchmark_check() {
    let mut k = known_answers();
    // Two wordings of ONE obligation, labelled as two different requirements: the corpus now asks the
    // canonicalizer to keep apart what it correctly merges, i.e. it reads as a false merge.
    k.parity_yaml = k.parity_yaml.replacen(
        "\nnot_equivalent:\n",
        "\nnot_equivalent:\n  - id: T-001\n    a: The system shall respond within 200 ms.\n    b: The system shall respond no later than 0.2 seconds.\n    source: selftest tamper\n",
        1,
    );
    assert_eq!(only_failing(&k, "parity"), 1);
}

#[test]
fn embedded_data_that_does_not_parse_cannot_be_told_and_is_not_a_pass() {
    let mut k = known_answers();
    k.parity_yaml = "schema: [not a corpus".to_owned();
    assert!(
        run_checks(&k).is_err(),
        "unparsable embedded data is UNVERIFIABLE, never a pass"
    );
    let mut k = known_answers();
    k.registry_yaml = "subjects: [".to_owned();
    assert!(run_checks(&k).is_err());
}
