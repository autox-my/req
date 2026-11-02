//! `tools/orchestration/validate_envelope.py` — the report contract of delegated work.
//!
//! Process tooling, not the product: nothing here touches an address. It is tested because the
//! orchestrator trusts a worker's report only after this script accepts it, so a gap in the
//! validator is a gap in what "done" means for a fix unit (AGENTS.md, "Orchestration").
//!
//! Two seams. The command line: an envelope in, an exit code out (0 valid, 1 invalid, 2 cannot
//! read) — the contract the orchestrator calls, so the blocking rule (a fix unit carries both
//! `red` and `mutations`) is pinned through it and not only through the script's own table.
//! And the gate: the self-check is a step of `scripts/check.sh`, and a wiring nobody pins can
//! be deleted with every exit code staying zero.
//!
//! `python3` is required, and its absence is a failure named here rather than a skipped check.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

mod common;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn validator() -> PathBuf {
    root().join("tools/orchestration/validate_envelope.py")
}

/// `python3` resolved from the test process's own `PATH`, as an absolute path.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "python3 is required for tools/orchestration/validate_envelope.py — install it first \
         (macOS: `xcode-select --install`; Debian: `apt-get install python3`)"
    );
}

/// A scratch file holding `bytes`, unique per call so parallel tests never share one.
fn scratch(bytes: &[u8]) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "req-orchestration-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("envelope.json");
    fs::write(&path, bytes).expect("write scratch envelope");
    path
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(python3())
        .arg("-I")
        .arg(validator())
        .args(args)
        .output()
        .expect("runs validate_envelope.py")
}

fn validate(json: &str) -> Output {
    let path = scratch(json.as_bytes());
    run(&[path.as_os_str()])
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A complete fix-unit envelope. The `{extra}` slot takes the evidence fields under test.
fn fix_envelope(extra: &str) -> String {
    format!(
        r#"{{
  "task_id": "M1-1-fix",
  "status": "done",
  "branch": "feature/m1-identity-coverage",
  "base_commit": "b56bca3",
  "files_changed": ["src/canon.rs"],
  "tests": "scripts/check.sh exit 0 (full run)",
  "diff_stat": "+10/-2 across 1 file",
  "committed": true,
  "blockers": null,
  "summary": "one line"{extra}
}}"#
    )
}

const RED: &str = r#","red": {"test": "canon::t", "base_commit": "b56bca3", "failure": "assertion failed"}"#;
const MUTATIONS: &str =
    r#","mutations": [{"change": "delete the rule", "test_that_failed": "canon::t"}]"#;

#[test]
fn a_fix_unit_with_red_and_mutations_is_valid() {
    let out = validate(&fix_envelope(&format!("{RED}{MUTATIONS}")));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn a_fix_unit_without_mutations_is_invalid() {
    // The blocking review finding: `red` alone used to be enough, so a fix unit could claim
    // its test had teeth without naming one mutation that turns it red.
    let out = validate(&fix_envelope(RED));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("requires mutations"),
        "the refusal must name the missing evidence:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_fix_unit_without_red_is_invalid() {
    let out = validate(&fix_envelope(MUTATIONS));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("requires red"), "{}", stderr(&out));
}

#[test]
fn an_empty_identifier_is_invalid() {
    // The schema declares `task_id` and `branch` `minLength: 1`; an envelope it rejects must
    // not pass the validator the orchestrator actually runs.
    let json = fix_envelope(&format!("{RED}{MUTATIONS}")).replace("M1-1-fix", "");
    let out = validate(&json);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("task_id"), "{}", stderr(&out));
}

#[test]
fn a_file_that_is_not_utf8_is_invalid_not_a_traceback() {
    // Exit 1 means "not a valid envelope", and bytes that are not text are not one. A
    // traceback also exits 1, which is how the two were once indistinguishable to a caller.
    let path = scratch(&[0xff, 0xfe, 0x00, b'{']);
    let out = run(&[path.as_os_str()]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("Traceback"),
        "a read error must be reported, not raised:\n{}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("INVALID"), "{}", stderr(&out));
}

#[test]
fn a_missing_file_is_an_io_error() {
    let out = run(&[std::ffi::OsStr::new("/nonexistent/req-envelope.json")]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

#[test]
fn the_self_check_passes_and_is_wired_into_the_gate() {
    // The table itself, including the comparison against the published schema.
    let out = run(&[std::ffi::OsStr::new("--self-check")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "--self-check must pass with no corpus, no git and no req:\n{stdout}\n{}",
        stderr(&out)
    );
    // The table is the test: a gutted one must redden this, so its size is pinned too.
    let checks: usize = stdout
        .split("validate_envelope self-check: ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("the self-check summary is missing:\n{stdout}"));
    assert!(
        checks >= 30,
        "the self-check table has lost rows ({checks} check(s)):\n{stdout}"
    );

    // The gate's plan prints the default run's commands from the same call sites and executes
    // none, so removing the wiring reddens this (the precedent is `tests/adopt.rs`).
    for args in [vec!["plan"], vec!["--fast", "plan"]] {
        let mut command = Command::new("bash");
        command
            .arg(root().join("scripts/check.sh"))
            .args(&args)
            .current_dir(root());
        common::point_home_at_toolchain(&mut command);
        let plan = command.output().expect("runs scripts/check.sh");
        let plan_out = String::from_utf8_lossy(&plan.stdout);
        assert_eq!(
            plan.status.code(),
            Some(3),
            "a plan is never a pass:\n{plan_out}"
        );
        assert!(
            plan_out
                .lines()
                .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
                .any(|line| line.contains("tools/orchestration/validate_envelope.py --self-check")),
            "the gate must self-check the envelope validator in {args:?}:\n{plan_out}"
        );
    }
}
