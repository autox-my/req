//! `tools/junit_case.py` — a command as a test case (ADR 0024 §3, unit 64-2 of `docs/specs/64-test-evidence.md`).
//!
//! The public seam is the script itself (`python3 tools/junit_case.py …`, exactly what an adopter's
//! shell check runs) and the compiled `req`, which reads what the adapter wrote. The oracles are
//! independent of the adapter: the mapping (0 passed, 1 failure, 3 skipped, anything else an
//! error) is ADR 0024 §3's, the adapter's exit is the command's own, and the report is judged by
//! `req`'s own reader and gate rather than by the adapter's idea of its output.
//!
//! The failure paths use real commands, never mocks: a missing executable, a file without the
//! execute bit, a command that outlives its timeout (and one whose *grandchild* does), a command
//! that kills itself with a signal, and an adapter that is itself sent SIGTERM.
//!
//! `python3` is a hard prerequisite of this file, like `minisign` and `bash` for the Baseline gate
//! tests: a test that returned early would be recorded as **passed** in the JUnit report `req gate`
//! reads, which is the absence-reads-as-pass failure this repository exists to prevent. A machine
//! without it fails the suite loudly (`scripts/check.sh` skips only the adapter's own self-check
//! step, and says SKIP there).
#![cfg(unix)]

mod common;

use req::junit::{Report, TestOutcome};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long any one adapter run may take before the test gives up on it (a hang is a failure).
const DEADLINE: Duration = Duration::from_secs(60);

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn adapter_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/junit_case.py")
}

fn pytest_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/junit")
        .join(name)
}

struct Run {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Run {
    fn err(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    fn all(&self) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&self.stdout),
            String::from_utf8_lossy(&self.stderr)
        )
    }
}

/// Run a prepared command to completion, killing it and failing the test if it outlives `DEADLINE`.
fn run_with_deadline(mut command: Command, stdin: Option<&[u8]>) -> Run {
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawns the process");
    let pid = child.id();
    if let Some(bytes) = stdin {
        let mut pipe = child.stdin.take().expect("stdin was piped");
        let bytes = bytes.to_vec();
        std::thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        });
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(DEADLINE) {
        Ok(output) => {
            let output = output.expect("collects the output");
            Run {
                code: output.status.code().unwrap_or(-1),
                stdout: output.stdout,
                stderr: output.stderr,
            }
        }
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            panic!("the process hung past {DEADLINE:?}: that is the failure");
        }
    }
}

/// A scratch directory holding a report and the cases' side files, removed on drop.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    /// Always `Some`: `python3` missing is a panic (see the module doc), never an early return.
    fn new(name: &str) -> Option<Scratch> {
        let python_ok = Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        assert!(
            python_ok,
            "junit_case::{name}: python3 is not on PATH, so the adapter cannot run (a hard prerequisite)"
        );
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-junit-case-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        Some(Scratch { dir })
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    /// The default report: in a directory that does not exist yet, so the adapter must create it.
    fn report(&self) -> PathBuf {
        self.path("out/junit.xml")
    }

    /// Run the adapter with exactly these arguments.
    fn adapter(&self, args: &[&str]) -> Run {
        let mut command = Command::new("python3");
        command
            .arg(adapter_script())
            .args(args)
            .current_dir(&self.dir);
        run_with_deadline(command, None)
    }

    /// Run one named case of the default report: `--classname checks --name <name> -- <command…>`.
    fn case(&self, name: &str, command: &[&str]) -> Run {
        self.case_into(&self.report(), name, command, &[])
    }

    fn case_into(&self, report: &Path, name: &str, command: &[&str], extra: &[&str]) -> Run {
        let report = report.display().to_string();
        let mut args = vec!["--report", &report, "--classname", "checks", "--name", name];
        args.extend_from_slice(extra);
        args.push("--");
        args.extend_from_slice(command);
        self.adapter(&args)
    }

    fn load(&self) -> Report {
        self.load_from(&self.report())
    }

    fn load_from(&self, report: &Path) -> Report {
        Report::load(report)
            .unwrap_or_else(|reason| panic!("req must read the adapter's report: {reason}"))
    }

    fn xml(&self) -> String {
        fs::read_to_string(self.report()).expect("the report exists")
    }

    /// Everything in the report's directory, so a leaked temp file shows.
    fn listing(&self, relative: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.path(relative))
            .expect("lists the directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// ---------------------------------------------------------------------------
// the exit mapping (ADR 0024 §3)
// ---------------------------------------------------------------------------

/// 0 → passed, 1 → `<failure>`, 3 → `<skipped>` naming UNVERIFIABLE, any other exit → `<error>`; the
/// adapter's own exit is the command's. `req`'s reader is the oracle for the outcome, the raw XML for
/// which element carries it (the reader folds `<error>` into failed).
#[test]
fn the_exit_mapping_is_zero_passed_one_failure_three_skipped_else_error() {
    struct Row {
        code: &'static str,
        outcome: TestOutcome,
        element: Option<&'static str>,
        message: &'static str,
    }
    let rows = [
        Row {
            code: "0",
            outcome: TestOutcome::Passed,
            element: None,
            message: "",
        },
        Row {
            code: "1",
            outcome: TestOutcome::Failed,
            element: Some("failure"),
            message: "exit 1: sh -c 'exit 1'",
        },
        Row {
            code: "3",
            outcome: TestOutcome::Skipped,
            element: Some("skipped"),
            message: "UNVERIFIABLE (exit 3): sh -c 'exit 3'",
        },
        Row {
            code: "2",
            outcome: TestOutcome::Failed,
            element: Some("error"),
            message: "exit 2: sh -c 'exit 2'",
        },
        Row {
            code: "7",
            outcome: TestOutcome::Failed,
            element: Some("error"),
            message: "exit 7: sh -c 'exit 7'",
        },
    ];
    for row in rows {
        let Some(s) = Scratch::new("mapping") else {
            return;
        };
        let script = format!("exit {}", row.code);
        let run = s.case("c", &["sh", "-c", &script]);
        assert_eq!(
            run.code.to_string(),
            row.code,
            "the adapter exits with the command's own status ({})",
            run.err()
        );
        assert_eq!(
            s.load().outcome("checks::c"),
            Some(row.outcome),
            "exit {} must map to {:?}",
            row.code,
            row.outcome
        );
        let xml = s.xml();
        for element in ["failure", "skipped", "error"] {
            let has = xml.contains(&format!("<{element} "));
            assert_eq!(
                has,
                row.element == Some(element),
                "exit {}: `<{element}>` presence in\n{xml}",
                row.code
            );
        }
        if let Some(element) = row.element {
            assert!(
                xml.contains(&format!("<{element} message=\"{}\"", row.message)),
                "exit {}: the message must carry the exit and the command in\n{xml}",
                row.code
            );
        }
    }
}

/// The report's shape is the one ADR 0024 §3 names: `<testsuites>` holding `<testsuite name="junit_case">`.
#[test]
fn the_report_root_is_testsuites_holding_the_junit_case_suite() {
    let Some(s) = Scratch::new("shape") else {
        return;
    };
    assert_eq!(s.case("c", &["true"]).code, 0);
    let xml = s.xml();
    assert!(xml.contains("<testsuites>"), "{xml}");
    assert!(xml.contains("<testsuite name=\"junit_case\""), "{xml}");
    assert!(
        xml.contains("<testcase classname=\"checks\" name=\"c\""),
        "{xml}"
    );
}

/// The adapter never hides a command's status: every code from 0 to 255 that a shell can produce is
/// passed through unchanged.
#[test]
fn the_adapters_exit_equals_the_commands() {
    let Some(s) = Scratch::new("exit-equality") else {
        return;
    };
    for code in [0, 1, 2, 3, 4, 42, 64, 126, 255] {
        let script = format!("exit {code}");
        let run = s.case("c", &["sh", "-c", &script]);
        assert_eq!(run.code, code, "command exit {code}: {}", run.err());
    }
}

/// A missing executable is `<error>` and the shell's 127; a file that is not executable is 126.
#[test]
fn a_missing_executable_is_an_error_and_exit_127() {
    let Some(s) = Scratch::new("missing") else {
        return;
    };
    let run = s.case("c", &["req-no-such-command-for-the-adapter"]);
    assert_eq!(run.code, 127, "{}", run.err());
    let xml = s.xml();
    assert!(
        xml.contains("<error message=\"cannot start: req-no-such-command-for-the-adapter"),
        "{xml}"
    );
    assert_eq!(s.load().outcome("checks::c"), Some(TestOutcome::Failed));
}

#[test]
fn a_file_without_the_execute_bit_is_an_error_and_exit_126() {
    let Some(s) = Scratch::new("noexec") else {
        return;
    };
    let script = s.path("not-executable.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").expect("writes the script");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).expect("chmod");
    let run = s.case("c", &[&script.display().to_string()]);
    assert_eq!(run.code, 126, "{}", run.err());
    assert!(
        s.xml().contains("<error message=\"cannot start: "),
        "{}",
        s.xml()
    );
}

/// A command that outlives `--timeout` is killed, recorded as `<error>`, and the adapter answers 124.
#[test]
fn a_timeout_kills_the_command_records_an_error_and_exits_124() {
    let Some(s) = Scratch::new("timeout") else {
        return;
    };
    let started = Instant::now();
    let run = s.case_into(
        &s.report(),
        "c",
        &["sh", "-c", "exec sleep 30"],
        &["--timeout", "0.5"],
    );
    assert_eq!(run.code, 124, "{}", run.err());
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the command was killed, not waited for"
    );
    let xml = s.xml();
    assert!(
        xml.contains("<error message=\"timed out after 0.5s: "),
        "{xml}"
    );
    assert_eq!(s.load().outcome("checks::c"), Some(TestOutcome::Failed));
}

/// The timeout kills the command's whole process group: a grandchild left running would be a check
/// that keeps working after the gate has moved on.
#[test]
fn a_timeout_kills_the_grandchild_too() {
    let Some(s) = Scratch::new("grandchild") else {
        return;
    };
    let pidfile = s.path("grandchild.pid");
    let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());
    let run = s.case_into(
        &s.report(),
        "c",
        &["sh", "-c", &script],
        &["--timeout", "2"],
    );
    assert_eq!(run.code, 124, "{}", run.err());
    let pid = fs::read_to_string(&pidfile)
        .expect("the grandchild recorded its pid")
        .trim()
        .to_string();
    assert_gone(&pid, "grandchild");
}

/// True while `pid` is a live process. A zombie (killed, not yet reaped by whatever inherited it —
/// a plain container's PID 1 may never reap) counts as dead: `kill -0` succeeds on a zombie, so the
/// process state is read instead.
fn is_alive(pid: &str) -> bool {
    let out = Command::new("ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .expect("runs ps");
    let state = String::from_utf8_lossy(&out.stdout);
    let state = state.trim();
    !state.is_empty() && !state.starts_with('Z')
}

fn assert_gone(pid: &str, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while is_alive(pid) {
        assert!(Instant::now() < deadline, "{what} {pid} is still running");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A command killed by a signal is `<error>` naming the signal; the adapter answers 128 + the signal,
/// as a shell would.
#[test]
fn a_signalled_command_is_an_error_and_exit_128_plus_the_signal() {
    for (script, code, fragment) in [
        ("kill -9 $$", 137, "killed by signal 9 (SIGKILL)"),
        ("kill -15 $$", 143, "killed by signal 15 (SIGTERM)"),
    ] {
        let Some(s) = Scratch::new("signal") else {
            return;
        };
        let run = s.case("c", &["sh", "-c", script]);
        assert_eq!(run.code, code, "{}", run.err());
        let xml = s.xml();
        assert!(
            xml.contains(&format!("<error message=\"{fragment}: ")),
            "{xml}"
        );
    }
}

/// The adapter itself sent SIGTERM: the command is stopped, the case is recorded as an error (a stale
/// earlier pass must not survive an interrupted run) and the adapter answers 128 + 15.
#[test]
fn an_interrupted_adapter_stops_the_command_and_records_an_error() {
    let Some(s) = Scratch::new("interrupted") else {
        return;
    };
    // The earlier run passed; the interrupted one must overwrite that pass.
    assert_eq!(s.case("c", &["true"]).code, 0);
    let started = s.path("started");
    let pidfile = s.path("command.pid");
    let script = format!(
        "echo $$ > '{}'; touch '{}'; exec sleep 30",
        pidfile.display(),
        started.display()
    );
    let report = s.report().display().to_string();
    let mut child = Command::new("python3")
        .arg(adapter_script())
        .args([
            "--report",
            &report,
            "--classname",
            "checks",
            "--name",
            "c",
            "--",
            "sh",
            "-c",
            &script,
        ])
        .current_dir(&s.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawns the adapter");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !started.exists() {
        assert!(Instant::now() < deadline, "the command never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("signals the adapter");
    let status = loop {
        if let Some(status) = child.try_wait().expect("polls the adapter") {
            break status;
        }
        assert!(Instant::now() < deadline, "the adapter did not stop");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(143));
    let pid = fs::read_to_string(&pidfile)
        .expect("the command recorded its pid")
        .trim()
        .to_string();
    assert_gone(&pid, "the interrupted adapter's command");
    let xml = s.xml();
    assert!(
        xml.contains("<error message=\"interrupted by SIGTERM: "),
        "{xml}"
    );
    assert_eq!(s.load().test_count(), 1);
}

// ---------------------------------------------------------------------------
// the report: replace, accumulate, refuse, fresh
// ---------------------------------------------------------------------------

#[test]
fn a_rerun_replaces_the_previous_row() {
    let Some(s) = Scratch::new("replace") else {
        return;
    };
    assert_eq!(s.case("c", &["sh", "-c", "exit 1"]).code, 1);
    assert_eq!(s.load().outcome("checks::c"), Some(TestOutcome::Failed));
    assert_eq!(s.case("c", &["true"]).code, 0);
    let report = s.load();
    assert_eq!(report.test_count(), 1, "one case, one row:\n{}", s.xml());
    assert_eq!(report.outcome("checks::c"), Some(TestOutcome::Passed));
}

#[test]
fn two_cases_accumulate_and_a_rerun_keeps_the_other() {
    let Some(s) = Scratch::new("accumulate") else {
        return;
    };
    assert_eq!(s.case("a", &["true"]).code, 0);
    assert_eq!(s.case("b", &["sh", "-c", "exit 3"]).code, 3);
    let report = s.load();
    assert_eq!(report.test_count(), 2);
    assert_eq!(report.outcome("checks::a"), Some(TestOutcome::Passed));
    assert_eq!(report.outcome("checks::b"), Some(TestOutcome::Skipped));
    assert_eq!(s.case("a", &["sh", "-c", "exit 1"]).code, 1);
    let report = s.load();
    assert_eq!(report.test_count(), 2);
    assert_eq!(report.outcome("checks::a"), Some(TestOutcome::Failed));
    assert_eq!(report.outcome("checks::b"), Some(TestOutcome::Skipped));
}

/// The id is `classname::name`; a name with the five XML-special characters, a quote and a space
/// round-trips through the adapter's own re-read and `req`'s reader, and a rerun still replaces it.
#[test]
fn an_awkward_name_and_command_round_trip_and_replace() {
    let Some(s) = Scratch::new("escapes") else {
        return;
    };
    let name = "p[a&b <c> \"d\" 'e']";
    let report = s.report();
    let first = s.case_into(
        &report,
        name,
        &["sh", "-c", "exit 1", "arg with <&> \"quotes\" and\nnewline"],
        &[],
    );
    assert_eq!(first.code, 1, "{}", first.err());
    assert_eq!(
        s.load().outcome(&format!("checks::{name}")),
        Some(TestOutcome::Failed)
    );
    let second = s.case_into(&report, name, &["true"], &[]);
    assert_eq!(second.code, 0, "{}", second.err());
    let loaded = s.load();
    assert_eq!(loaded.test_count(), 1, "{}", s.xml());
    assert_eq!(
        loaded.outcome(&format!("checks::{name}")),
        Some(TestOutcome::Passed)
    );
}

/// A report that is not the adapter's own is refused whole: nothing is written, the file is byte-identical
/// and — because the refusal comes before the run — the command never runs.
#[test]
fn an_existing_report_that_is_not_this_shape_is_refused_and_left_byte_identical() {
    let valid_then_edited =
        "<testsuites><testsuite name=\"junit_case\"><testcase classname=\"c\" name=\"n\"/>";
    let pytest = fs::read(pytest_fixture("pytest-mixed.xml")).expect("reads the fixture");
    let samples: Vec<(&str, Vec<u8>)> = vec![
        ("garbage", b"this is not xml at all".to_vec()),
        ("truncated", valid_then_edited.as_bytes().to_vec()),
        ("empty file", Vec::new()),
        ("another producer's report", pytest),
        (
            "a doctype",
            b"<!DOCTYPE testsuites [<!ENTITY x \"y\">]><testsuites><testsuite name=\"junit_case\"/></testsuites>".to_vec(),
        ),
        (
            "wrong suite name",
            b"<testsuites><testsuite name=\"other\"/></testsuites>".to_vec(),
        ),
        (
            "an unknown child",
            b"<testsuites><testsuite name=\"junit_case\"><note/></testsuite></testsuites>".to_vec(),
        ),
    ];
    for (label, bytes) in samples {
        let Some(s) = Scratch::new("refusal") else {
            return;
        };
        let report = s.path("report.xml");
        fs::write(&report, &bytes).expect("writes the existing report");
        let marker = s.path("ran");
        let script = format!("touch '{}'", marker.display());
        let run = s.case_into(&report, "c", &["sh", "-c", &script], &[]);
        assert_eq!(
            run.code,
            2,
            "{label}: refused as a usage-class error: {}",
            run.all()
        );
        assert!(
            run.err().contains("junit_case:") && run.err().contains("--fresh"),
            "{label}: the refusal names itself and the remedy: {}",
            run.err()
        );
        assert_eq!(
            fs::read(&report).expect("reads"),
            bytes,
            "{label}: byte-identical"
        );
        assert!(
            !marker.exists(),
            "{label}: the command must not run before a refusal"
        );
        assert_eq!(
            s.listing("."),
            vec!["report.xml"],
            "{label}: no stray temp file"
        );
    }
}

#[test]
fn fresh_starts_a_new_report_over_one_it_would_refuse() {
    let Some(s) = Scratch::new("fresh") else {
        return;
    };
    let report = s.path("report.xml");
    fs::write(&report, "not xml").expect("writes");
    let run = s.case_into(&report, "c", &["sh", "-c", "exit 3"], &["--fresh"]);
    assert_eq!(run.code, 3, "{}", run.err());
    let loaded = s.load_from(&report);
    assert_eq!(loaded.test_count(), 1);
    assert_eq!(loaded.outcome("checks::c"), Some(TestOutcome::Skipped));
    // --fresh also drops rows of a valid report: it starts over, it does not merge.
    assert_eq!(s.case_into(&report, "other", &["true"], &[]).code, 0);
    assert_eq!(s.load_from(&report).test_count(), 2);
    assert_eq!(
        s.case_into(&report, "third", &["true"], &["--fresh"]).code,
        0
    );
    let loaded = s.load_from(&report);
    assert_eq!(loaded.test_count(), 1);
    assert_eq!(loaded.outcome("checks::third"), Some(TestOutcome::Passed));
}

/// Exit 2 *from the adapter*: its own `junit_case:` message is on stderr, not an interpreter error.
fn assert_refused(label: &str, run: &Run) {
    assert_eq!(run.code, 2, "{label} is refused: {}", run.all());
    assert!(
        run.err().contains("junit_case:"),
        "{label}: the refusal is the adapter's own: {}",
        run.err()
    );
}

/// A report path that is not a regular file is never read: a FIFO would block forever, a symlink would
/// be replaced by `os.replace`, a directory cannot be one. Each is refused without touching it.
#[test]
fn a_report_path_that_is_not_a_regular_file_is_refused_without_being_opened() {
    let Some(s) = Scratch::new("not-regular") else {
        return;
    };
    let fifo = s.path("fifo.xml");
    let made = Command::new("mkfifo").arg(&fifo).status();
    if made.is_ok_and(|status| status.success()) {
        for extra in [&[][..], &["--fresh"][..]] {
            let run = s.case_into(&fifo, "c", &["true"], extra);
            assert_refused("a FIFO (never opened)", &run);
        }
    } else {
        eprintln!("SKIP the FIFO half: mkfifo is not available");
    }

    let target = s.path("target.xml");
    fs::write(&target, "keep me").expect("writes the target");
    let link = s.path("link.xml");
    std::os::unix::fs::symlink(&target, &link).expect("symlinks");
    let run = s.case_into(&link, "c", &["true"], &["--fresh"]);
    assert_refused("a symlink", &run);
    assert_eq!(fs::read_to_string(&target).expect("reads"), "keep me");
    assert!(
        fs::symlink_metadata(&link)
            .expect("stat")
            .file_type()
            .is_symlink()
    );

    let dir = s.path("a-directory");
    fs::create_dir(&dir).expect("mkdir");
    let run = s.case_into(&dir, "c", &["true"], &["--fresh"]);
    assert_refused("a directory", &run);
}

/// The write is atomic and in the report's directory: after any run the directory holds the report and
/// nothing else, and a report written into a directory that did not exist is readable.
#[test]
fn the_write_leaves_no_temp_file_behind() {
    let Some(s) = Scratch::new("atomic") else {
        return;
    };
    assert_eq!(s.case("a", &["true"]).code, 0);
    assert_eq!(s.case("b", &["sh", "-c", "exit 1"]).code, 1);
    assert_eq!(
        s.case("c", &["req-no-such-command-for-the-adapter"]).code,
        127
    );
    assert_eq!(s.listing("out"), vec!["junit.xml"]);
    assert_eq!(s.load().test_count(), 3);
}

/// Atomic means the report is replaced, not rewritten in place: a temp file renamed over it gives the
/// path a new inode, while `open(report, "w")` would keep the old one and expose a truncated file to
/// a concurrent `req gate` or a crash.
#[test]
fn a_rewrite_replaces_the_file_instead_of_truncating_it() {
    use std::os::unix::fs::MetadataExt;
    let Some(s) = Scratch::new("inode") else {
        return;
    };
    assert_eq!(s.case("a", &["true"]).code, 0);
    let before = fs::metadata(s.report()).expect("stats the report").ino();
    assert_eq!(s.case("b", &["true"]).code, 0);
    let after = fs::metadata(s.report()).expect("stats the report").ino();
    assert_ne!(
        before, after,
        "the report was rewritten in place, not replaced"
    );
}

/// Two different `(classname, name)` pairs can join to one id when a part contains `::`. `req` would
/// read them as one test, so the second must not silently replace the first (a pass overwriting an
/// unrelated failure), and the refusal comes before the command runs.
#[test]
fn a_colliding_pair_is_refused_before_the_command_runs() {
    let Some(s) = Scratch::new("collision") else {
        return;
    };
    let report = s.report().display().to_string();
    let first = Command::new("python3")
        .arg(adapter_script())
        .args([
            "--report",
            &report,
            "--classname",
            "a::b",
            "--name",
            "c",
            "--",
            "sh",
            "-c",
            "exit 1",
        ])
        .output()
        .expect("runs the adapter");
    assert_eq!(first.status.code(), Some(1));
    let before = fs::read(s.report()).expect("reads the report");
    let marker = s.path("ran");
    let touch = format!("touch '{}'", marker.display());
    let second = Command::new("python3")
        .arg(adapter_script())
        .args([
            "--report",
            &report,
            "--classname",
            "a",
            "--name",
            "b::c",
            "--",
            "sh",
            "-c",
            &touch,
        ])
        .output()
        .expect("runs the adapter");
    assert_eq!(
        second.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(!marker.exists(), "the command must not have run");
    assert_eq!(fs::read(s.report()).expect("reads the report"), before);
}

/// A DTD hidden in UTF-16 would pass a byte scan and still be decoded by the XML parser: any
/// non-UTF-8 existing report is refused and left byte-identical.
#[test]
fn a_utf16_report_with_a_dtd_is_refused_and_left_alone() {
    let Some(s) = Scratch::new("utf16") else {
        return;
    };
    let text = "<?xml version=\"1.0\" encoding=\"utf-16\"?><!DOCTYPE a [<!ENTITY x \"y\">]><testsuites><testsuite name=\"junit_case\"/></testsuites>";
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::create_dir_all(s.report().parent().expect("has a directory")).expect("makes the directory");
    fs::write(s.report(), &bytes).expect("writes the report");
    let run = s.case("c", &["true"]);
    assert_eq!(run.code, 2, "{}", run.err());
    assert_eq!(fs::read(s.report()).expect("reads the report"), bytes);
}

/// The gate runs the adapter's self-check: deleting the step from `scripts/check.sh` reddens this, as
/// it does for the other tools' tables (a plan executes nothing and is never a pass).
#[test]
fn the_gate_plan_names_the_adapter_self_check() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for args in [vec!["plan"], vec!["--fast", "plan"]] {
        let mut command = Command::new("bash");
        command
            .arg(root.join("scripts/check.sh"))
            .args(&args)
            .current_dir(&root);
        common::point_home_at_toolchain(&mut command);
        let plan = command.output().expect("runs scripts/check.sh");
        let stdout = String::from_utf8_lossy(&plan.stdout);
        assert_eq!(
            plan.status.code(),
            Some(3),
            "a plan is never a pass:\n{stdout}"
        );
        assert!(
            stdout.lines().any(|line| line
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("tools/junit_case.py --self-check")),
            "the gate must self-check the adapter in {args:?}:\n{stdout}"
        );
    }
}

// ---------------------------------------------------------------------------
// usage: validate before touching any file
// ---------------------------------------------------------------------------

/// Every malformed invocation is exit 2, runs no command, and leaves an existing report byte-identical
/// (or absent, if there was none).
#[test]
fn a_usage_error_is_exit_2_and_touches_nothing() {
    let Some(s) = Scratch::new("usage") else {
        return;
    };
    assert_eq!(s.case("keep", &["true"]).code, 0);
    let before = fs::read(s.report()).expect("reads the report");
    let marker = s.path("ran");
    let touch = format!("touch '{}'", marker.display());
    let report = s.report().display().to_string();
    let absent = s.path("never/created.xml").display().to_string();

    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("no arguments", vec![]),
        (
            "no report",
            vec!["--classname", "c", "--name", "n", "--", "sh", "-c", &touch],
        ),
        (
            "no classname",
            vec!["--report", &report, "--name", "n", "--", "sh", "-c", &touch],
        ),
        (
            "no name",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "no separator",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "no command",
            vec!["--report", &report, "--classname", "c", "--name", "n", "--"],
        ),
        (
            "empty classname",
            vec![
                "--report",
                &report,
                "--classname",
                "",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "empty name",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "blank name",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "   ",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "newline in name",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "a\nb",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "tab in classname",
            vec![
                "--report",
                &report,
                "--classname",
                "a\tb",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "escape in name",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "a\u{1b}b",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "empty report",
            vec![
                "--report",
                "",
                "--classname",
                "c",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "option-shaped report",
            vec![
                "--report",
                "-x",
                "--classname",
                "c",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "stdin-shaped report",
            vec![
                "--report",
                "-",
                "--classname",
                "c",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "unknown option",
            vec![
                "--bogus",
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "repeated option",
            vec![
                "--report",
                &report,
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "report without value",
            vec!["--classname", "c", "--name", "n", "--report"],
        ),
        (
            "zero timeout",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--timeout",
                "0",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "negative timeout",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--timeout",
                "-1",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "nan timeout",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--timeout",
                "nan",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "infinite timeout",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--timeout",
                "inf",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "word timeout",
            vec![
                "--report",
                &report,
                "--classname",
                "c",
                "--name",
                "n",
                "--timeout",
                "soon",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
        (
            "self-check with other arguments",
            vec!["--self-check", "--report", &report],
        ),
        (
            "usage error never creates a directory",
            vec![
                "--report",
                &absent,
                "--classname",
                "",
                "--name",
                "n",
                "--",
                "sh",
                "-c",
                &touch,
            ],
        ),
    ];
    for (label, args) in cases {
        let run = s.adapter(&args);
        assert_eq!(run.code, 2, "{label}: {}", run.all());
        assert!(
            run.err().contains("junit_case:"),
            "{label}: the message names the tool: {}",
            run.err()
        );
        assert_eq!(
            fs::read(s.report()).expect("reads"),
            before,
            "{label}: report untouched"
        );
        assert!(!marker.exists(), "{label}: no command ran");
        assert!(!s.path("never").exists(), "{label}: nothing was created");
    }
    assert_eq!(s.listing("out"), vec!["junit.xml"]);
}

/// `--help` prints the usage and exits 0 without touching anything.
#[test]
fn help_prints_the_usage() {
    let Some(s) = Scratch::new("help") else {
        return;
    };
    let run = s.adapter(&["--help"]);
    assert_eq!(run.code, 0, "{}", run.err());
    assert!(String::from_utf8_lossy(&run.stdout).contains("--classname"));
}

// ---------------------------------------------------------------------------
// the command's own streams are not the adapter's business
// ---------------------------------------------------------------------------

/// Stdin reaches the command, and its stdout and stderr come back byte-for-byte — including bytes that
/// are not UTF-8 — so wrapping a check never changes what the check prints.
#[test]
fn the_commands_streams_pass_through_untouched() {
    let Some(s) = Scratch::new("streams") else {
        return;
    };
    let report = s.report().display().to_string();
    let mut command = Command::new("python3");
    command
        .arg(adapter_script())
        .args([
            "--report",
            &report,
            "--classname",
            "checks",
            "--name",
            "c",
            "--",
            "sh",
            "-c",
            "cat; printf 'out\\377\\000end'; printf 'err\\377' >&2",
        ])
        .current_dir(&s.dir);
    let run = run_with_deadline(command, Some(b"in\xffput"));
    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, b"in\xffputout\xff\0end");
    assert_eq!(run.stderr, b"err\xff");
}

/// A command that reads stdin to the end gets exactly what the caller sent: the adapter never reads it.
#[test]
fn the_adapter_does_not_consume_stdin() {
    let Some(s) = Scratch::new("stdin") else {
        return;
    };
    let report = s.report().display().to_string();
    let mut command = Command::new("python3");
    command
        .arg(adapter_script())
        .args([
            "--report",
            &report,
            "--classname",
            "checks",
            "--name",
            "c",
            "--",
            "wc",
            "-c",
        ])
        .current_dir(&s.dir);
    let payload = vec![b'x'; 200_000];
    let run = run_with_deadline(command, Some(&payload));
    assert_eq!(run.code, 0);
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "200000");
}

// ---------------------------------------------------------------------------
// the self-check
// ---------------------------------------------------------------------------

#[test]
fn the_self_check_passes() {
    let Some(s) = Scratch::new("self-check") else {
        return;
    };
    let run = s.adapter(&["--self-check"]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("0 failure(s)"),
        "{}",
        run.all()
    );
}

// ---------------------------------------------------------------------------
// acceptance: through req gate
// ---------------------------------------------------------------------------

mod through_the_gate {
    use super::*;

    struct Out {
        code: i32,
        text: String,
    }

    /// A corpus of machine-evidenced claims, one per `(claim id, linked test id)`; no selector, so a
    /// bare `req gate` decides every claim and only the evidence is in play.
    fn corpus(s: &Scratch, links: &[(&str, &str)]) {
        let init = Command::new(req_bin())
            .arg("init")
            .current_dir(&s.dir)
            .output()
            .expect("runs req init");
        assert_eq!(init.status.code(), Some(0), "req init must succeed");
        fs::remove_file(s.path("claims/REQ-0001.yml")).expect("removes the starter claim");
        for (index, (id, test)) in links.iter().enumerate() {
            let ms = 200 + index * 100;
            let value = format!("0.{}", ms / 100);
            let test = test.replace('"', "\\\"");
            let text = format!(
                "schema: req.claim/2\n\
                 claim_id: {id}\n\
                 statement: \"The {id} service shall respond to a health check within {ms} ms.\"\n\
                 applicability: []\n\
                 relations: []\n\
                 tests:\n  - \"{test}\"\n\
                 rationale:\n  source: docs/requirements.md\n  date: 2026-11-04\n  rejected_alternatives: []\n  assumptions: []\n\
                 pins:\n  operator: LE\n  value: \"{value}\"\n  unit: s\n"
            );
            fs::write(s.path(&format!("claims/{id}.yml")), text).expect("writes a claim");
        }
    }

    fn gate(s: &Scratch, reports: &[&Path]) -> Out {
        let mut command = Command::new(req_bin());
        command.arg("gate").current_dir(&s.dir);
        for report in reports {
            command.arg("--test-report").arg(report);
        }
        let run = run_with_deadline(command, None);
        Out {
            code: run.code,
            text: run.all(),
        }
    }

    /// A claim linked to a case whose command exited 0 passes.
    #[test]
    fn a_claim_linked_to_a_case_that_exited_zero_passes() {
        let Some(s) = Scratch::new("accept-pass") else {
            return;
        };
        corpus(&s, &[("REQ-0001", "checks::proof")]);
        assert_eq!(s.case("proof", &["true"]).code, 0);
        let out = gate(&s, &[&s.report()]);
        assert_eq!(out.code, 0, "{}", out.text);
    }

    /// ADR 0024 §4, pinned: a case whose command exited 3 is `<skipped>`, and ADR 0019 §5 reads a claim
    /// whose linked tests are all skipped as FAIL — exit 1, naming the id. It is never a pass and, by
    /// this ADR's own account, not exit 3.
    #[test]
    fn a_claim_linked_to_a_case_that_exited_three_is_not_a_pass_it_is_exit_one() {
        let Some(s) = Scratch::new("accept-three") else {
            return;
        };
        corpus(&s, &[("REQ-0001", "checks::proof")]);
        assert_eq!(s.case("proof", &["sh", "-c", "exit 3"]).code, 3);
        let out = gate(&s, &[&s.report()]);
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(
            out.text.contains("checks::proof"),
            "the id is named: {}",
            out.text
        );
        assert!(
            out.text.contains("REQ-0001"),
            "the claim is named: {}",
            out.text
        );
    }

    /// Exit 1 and any other exit are not passes either.
    #[test]
    fn a_failed_or_errored_case_fails_the_claim() {
        for script in ["exit 1", "exit 2", "kill -9 $$"] {
            let Some(s) = Scratch::new("accept-fail") else {
                return;
            };
            corpus(&s, &[("REQ-0001", "checks::proof")]);
            assert_ne!(s.case("proof", &["sh", "-c", script]).code, 0);
            let out = gate(&s, &[&s.report()]);
            assert_eq!(out.code, 1, "{script}: {}", out.text);
        }
    }

    /// The adapter's report and a real pytest report together, through `--test-report` twice: each
    /// claim is confirmed by its own stack's evidence.
    #[test]
    fn the_adapters_report_and_pytests_together_are_the_union() {
        let Some(s) = Scratch::new("accept-union") else {
            return;
        };
        corpus(
            &s,
            &[
                ("REQ-0001", "checks::proof"),
                ("REQ-0002", "tests.test_probe::test_passes"),
            ],
        );
        assert_eq!(s.case("proof", &["true"]).code, 0);
        let pytest = pytest_fixture("pytest-mixed.xml");
        let report = s.report();
        let out = gate(&s, &[&report, &pytest]);
        assert_eq!(out.code, 0, "{}", out.text);

        // One report alone cannot confirm the other stack's claim: absent from it → not decided.
        let alone = gate(&s, &[&report]);
        assert_eq!(alone.code, 3, "{}", alone.text);

        // A failing adapter case still fails its claim in the union.
        assert_eq!(s.case("proof", &["sh", "-c", "exit 1"]).code, 1);
        let failing = gate(&s, &[&report, &pytest]);
        assert_eq!(failing.code, 1, "{}", failing.text);
    }
}
