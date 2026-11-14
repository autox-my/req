//! `req canon --by-area` (issue #88): the per-area work list.
//!
//! The expectations are built from claims this test writes: one area with a structured and an
//! unstructured claim (one lint), one with a structured claim, and the `req init` starter with no
//! area. Nothing is read back from the implementation's own formatting except the counts the
//! issue states.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-canon-by-area-{}-{}-{name}",
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

    fn claim(&self, id: &str, area: Option<&str>, statement: &str) {
        let area = area
            .map(|area| format!("area: {area}\n"))
            .unwrap_or_default();
        let text = format!(
            "schema: req.claim/3
claim_id: {id}
{area}statement: \"{statement}\"
applicability: []
relations: []
rationale:
  source: docs/x.md
  date: 2026-11-15
  rejected_alternatives: []
  assumptions: []
"
        );
        fs::write(self.0.join("claims").join(format!("{id}.yml")), text).expect("writes a claim");
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(req_bin())
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("runs req")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout is UTF-8")
}

#[test]
fn by_area_groups_claims_by_their_authored_area_and_leaves_plain_canon_unchanged() {
    let s = Scratch::new("counts");
    s.claim(
        "REQ-0002",
        Some("diagnostics/startup"),
        "The API shall respond within 200 ms.",
    );
    s.claim(
        "REQ-0003",
        Some("diagnostics/startup"),
        "The API shall be robust.",
    );
    s.claim(
        "REQ-0004",
        Some("interfaces/can-bus"),
        "The bus shall carry 8 bytes.",
    );

    let plain = s.run(&["canon"]);
    assert_eq!(plain.status.code(), Some(0));
    assert!(
        !stdout(&plain).contains("areas:"),
        "plain canon has no summary: {}",
        stdout(&plain)
    );

    let out = s.run(&["canon", "--by-area"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    let line = |area: &str| {
        text.lines()
            .find(|line| line.trim_start().starts_with(area))
            .unwrap_or_else(|| panic!("no `{area}` line in:\n{text}"))
    };
    assert!(text.contains("areas: 3"), "{text}");
    assert_eq!(
        line("diagnostics/startup"),
        "  diagnostics/startup     2 claim(s)     1 structured     1 unstructured     1 lint(s) on   1 claim(s)"
    );
    assert_eq!(
        line("interfaces/can-bus"),
        "  interfaces/can-bus      1 claim(s)     1 structured     0 unstructured     0 lint(s) on   0 claim(s)"
    );
    assert_eq!(
        line("(no area)"),
        "  (no area)               1 claim(s)     1 structured     0 unstructured     0 lint(s) on   0 claim(s)"
    );
    assert!(
        text.trim_end().ends_with("(no area)               1 claim(s)     1 structured     0 unstructured     0 lint(s) on   0 claim(s)"),
        "`(no area)` is last:\n{text}"
    );
}

#[test]
fn by_area_composes_with_fail_on_flag_and_a_repeated_flag_is_usage() {
    let s = Scratch::new("flags");
    let both = s.run(&["canon", "--by-area", "--fail-on-flag"]);
    assert_eq!(
        both.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&both.stderr)
    );
    assert!(stdout(&both).contains("areas:"), "{}", stdout(&both));

    for args in [
        &["canon", "--by-area", "--by-area"][..],
        &["canon", "--fail-on-flag", "--fail-on-flag"][..],
        &["canon", "--by-area", "extra"][..],
    ] {
        let out = s.run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: req canon"),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
