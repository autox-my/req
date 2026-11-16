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
    // The summary is appended: every line plain canon printed comes first, byte for byte.
    assert!(
        text.starts_with(&stdout(&plain)),
        "--by-area must only append to plain canon's output:\n{text}"
    );
    let line = |area: &str| {
        text.lines()
            .find(|line| line.trim_start().starts_with(area))
            .unwrap_or_else(|| panic!("no `{area}` line in:\n{text}"))
    };
    // Two areas; the claims that name none are a group of their own, not an area.
    assert!(text.contains("areas: 2"), "{text}");
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

/// ADR 0051: the registry's line order is the reading order (decision 3), and a reader sees an area's label, never its
/// key (decision 2). Areas the registry does not declare follow, by path, under their key (they have no label), and
/// `(no area)` stays last.
#[test]
fn by_area_reads_the_registry_for_order_and_labels() {
    let s = Scratch::new("registry");
    fs::create_dir_all(s.0.join(".req")).expect("makes .req");
    fs::write(
        s.0.join(".req/areas"),
        "# reading order\ninterfaces/can-bus Bus interfaces\ndiagnostics/startup Start-up (診断)\nempty/area Nothing filed here\n",
    )
    .expect("writes the registry");
    s.claim(
        "REQ-0002",
        Some("diagnostics/startup"),
        "The API shall respond within 200 ms.",
    );
    s.claim(
        "REQ-0003",
        Some("zeta/extra"),
        "The bus shall carry 8 bytes.",
    );
    s.claim(
        "REQ-0004",
        Some("interfaces/can-bus"),
        "The bus shall carry 8 bytes.",
    );
    s.claim(
        "REQ-0005",
        Some("alpha/undeclared"),
        "The bus shall carry 8 bytes.",
    );

    let out = s.run(&["canon", "--by-area"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    let summary: Vec<&str> = text
        .lines()
        .skip_while(|line| !line.starts_with("areas: "))
        .collect();
    assert_eq!(summary.first().copied(), Some("areas: 4"), "{text}");
    let names: Vec<&str> = summary[1..]
        .iter()
        .map(|line| line.trim_start().split("  ").next().unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        [
            "Bus interfaces",
            "Start-up (診断)",
            "alpha/undeclared",
            "zeta/extra",
            "(no area)"
        ],
        "registry order with labels, then undeclared keys by path, then (no area); an area with no claim is not \
         listed:\n{text}"
    );
    // The name column pads by characters: the longest name here is `alpha/undeclared` (16); `Start-up (診断)` is 13.
    assert_eq!(
        summary[2],
        "  Start-up (診断)        1 claim(s)     1 structured     0 unstructured     0 lint(s) on   0 claim(s)",
        "{text}"
    );
    assert!(
        !text.contains("diagnostics/startup  "),
        "a declared area is shown by its label, never its key:\n{text}"
    );
}

/// A malformed registry is refused before anything is printed (as `trace --format json` refuses it), and the exit is
/// canon's 3 for a registry it cannot read. Plain `canon` does not read `.req/areas` and is unaffected.
#[test]
fn a_malformed_registry_is_refused_only_with_by_area() {
    let s = Scratch::new("malformed");
    fs::create_dir_all(s.0.join(".req")).expect("makes .req");
    fs::write(s.0.join(".req/areas"), "a A\na twice\n").expect("writes a bad registry");
    let out = s.run(&["canon", "--by-area"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout(&out), "", "nothing is printed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(".req/areas:2:"),
        "names the bad line: {stderr}"
    );
    assert_eq!(s.run(&["canon"]).status.code(), Some(0));
}
