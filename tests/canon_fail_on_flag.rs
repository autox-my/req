//! `req canon --fail-on-flag` (issue #106): a supported way to fail on a flagged pin before any test
//! evidence exists.
//!
//! The gap it closes: `req canon` reports a flagged pin and exits 0 (it is a diagnostic), and
//! `req gate` fails on it but can never be green until every applying claim has a passing linked
//! test. Between "pinned" and "fully test-linked" nothing held a pin. Expectations are independent of
//! the implementation: the exit codes the issue measured (0 clean / 1 flagged / 3 unreadable / 2
//! usage), and the plain `req canon` output as the oracle for "unchanged output".

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

impl Scratch {
    /// A fresh `req init` corpus: one starter claim, `REQ-0001`, whose pins (LE, 0.2, s) the
    /// statement "within 200 ms" holds.
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-canon-fail-on-flag-{}-{}-{name}",
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
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn starter(&self) -> PathBuf {
        self.0.join("claims/REQ-0001.yml")
    }

    fn edit_starter(&self, from: &str, to: &str) {
        let text = fs::read_to_string(self.starter()).expect("reads the starter claim");
        assert!(text.contains(from), "the starter claim holds `{from}`");
        fs::write(self.starter(), text.replace(from, to)).expect("writes the starter claim");
    }
}

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

fn req(scratch: &Scratch, args: &[&str]) -> Out {
    let out = Command::new(req_bin())
        .args(args)
        .current_dir(scratch.path())
        .output()
        .expect("runs req");
    Out {
        code: out.status.code().expect("req exits, never a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn a_corpus_with_a_flagged_pin_and_no_test_report_exits_1() {
    let s = Scratch::new("flagged");
    s.edit_starter("value: \"0.2\"", "value: \"999\"");

    let plain = req(&s, &["canon"]);
    assert_eq!(
        plain.code, 0,
        "canon alone is a diagnostic: {}",
        plain.stdout
    );

    let strict = req(&s, &["canon", "--fail-on-flag"]);
    assert_eq!(
        strict.code, 1,
        "a flagged pin fails: {}{}",
        strict.stdout, strict.stderr
    );
    assert!(
        strict.stdout.contains("FLAG contradiction"),
        "the flag is still reported on stdout: {}",
        strict.stdout
    );
    assert!(
        strict.stderr.contains("REQ-0001") && strict.stderr.contains("flagged"),
        "the failure names the claim on stderr: {}",
        strict.stderr
    );
}

#[test]
fn the_same_corpus_with_the_pin_fixed_exits_0() {
    let s = Scratch::new("fixed");
    s.edit_starter("value: \"0.2\"", "value: \"999\"");
    assert_eq!(req(&s, &["canon", "--fail-on-flag"]).code, 1, "red first");

    s.edit_starter("value: \"999\"", "value: \"0.2\"");
    let strict = req(&s, &["canon", "--fail-on-flag"]);
    assert_eq!(strict.code, 0, "{}{}", strict.stdout, strict.stderr);
}

#[test]
fn the_flag_changes_the_exit_code_and_nothing_on_stdout() {
    for (name, value) in [("clean", "0.2"), ("flagged", "999")] {
        let s = Scratch::new(name);
        s.edit_starter("value: \"0.2\"", &format!("value: \"{value}\""));
        let plain = req(&s, &["canon"]);
        let strict = req(&s, &["canon", "--fail-on-flag"]);
        assert_eq!(
            plain.stdout, strict.stdout,
            "{name}: `--fail-on-flag` must not alter the diagnostics"
        );
    }
}

#[test]
fn a_pin_the_parse_does_not_carry_is_a_flag_too() {
    // The second kind of flag in the pin contract: the statement holds no value at all.
    let s = Scratch::new("not-carried");
    s.edit_starter(
        "The API shall respond to a health check within 200 ms.",
        "The API shall respond to a health check.",
    );
    let plain = req(&s, &["canon"]);
    assert!(
        plain.stdout.contains("FLAG not carried"),
        "the setup must produce a not-carried flag: {}",
        plain.stdout
    );
    assert_eq!(req(&s, &["canon", "--fail-on-flag"]).code, 1);
}

#[test]
fn a_flagged_pin_on_a_claim_no_variant_applies_still_fails() {
    // `canon` judges every claim's pins; `req gate` judges the applying ones. Before test evidence
    // exists the question is "is any pin wrong", so a wrong pin behind a selector must not hide.
    let s = Scratch::new("selector");
    s.edit_starter(
        "applicability: []",
        "applicability:\n  - deployment_mode == air_gapped",
    );
    s.edit_starter("value: \"0.2\"", "value: \"999\"");
    assert_eq!(req(&s, &["canon", "--fail-on-flag"]).code, 1);
}

#[test]
fn an_unreadable_corpus_is_3_never_1_and_never_0() {
    let s = Scratch::new("unreadable");
    s.edit_starter("applicability:", "applicabilty:");
    let strict = req(&s, &["canon", "--fail-on-flag"]);
    assert_eq!(strict.code, 3, "{}{}", strict.stdout, strict.stderr);
}

#[test]
fn a_clean_green_does_not_read_as_a_release_pass() {
    let s = Scratch::new("green-note");
    let strict = req(&s, &["canon", "--fail-on-flag"]);
    assert_eq!(strict.code, 0);
    let note = strict.stderr.to_lowercase();
    assert!(
        note.contains("not a release") || note.contains("not a pass"),
        "a green says it is not a release verdict: {}",
        strict.stderr
    );
    assert!(
        !strict.stdout.contains("PASS") && !strict.stderr.contains("PASS"),
        "the word PASS is the gate's: {}{}",
        strict.stdout,
        strict.stderr
    );
}

#[test]
fn other_arguments_stay_usage_errors() {
    let s = Scratch::new("usage");
    for args in [
        &["canon", "--fail-on-flag", "--fail-on-flag"][..],
        &["canon", "--fail-on-flag", "extra"],
        &["canon", "extra", "--fail-on-flag"],
        &["canon", "--fail-on-flag=1"],
        &["canon", "--fail-on-flag", "--verify"],
        &["canon", "--fail-on-flags"],
    ] {
        let out = req(&s, args);
        assert_eq!(out.code, 2, "{args:?}: {}{}", out.stdout, out.stderr);
        assert!(
            out.stderr.contains("usage: req canon"),
            "{args:?}: {}",
            out.stderr
        );
    }
}

#[test]
fn the_help_page_states_the_flag_and_what_its_green_is_not() {
    let s = Scratch::new("help");
    let help = req(&s, &["canon", "--help"]);
    assert_eq!(help.code, 0);
    for needle in ["--fail-on-flag", "every claim", "not a release", "req gate"] {
        assert!(
            help.stdout.contains(needle),
            "help names `{needle}`: {}",
            help.stdout
        );
    }
}
