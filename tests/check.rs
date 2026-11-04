//! `req check` (#63, ADR 0023): one command that folds the repository's release lists.
//!
//! The public seam is the compiled `req` against fixture corpora built in a temp dir. The oracles
//! are independent of `check`'s own output: the standalone `req gate` / `req render --check` exits
//! run in the same test (a step's exit is the command's exit, ADR 0023 §4), and every fixture is
//! hand-written. Items (a)–(n) are the spec's (`docs/specs/63-req-check.md`, unit 63-1).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// A fresh corpus in a temp dir. It starts as a `req init` corpus (the registry and the claims
/// directory) with the starter claim replaced by a hand-written one; claims and the two lists are
/// then added by the test.
struct Scratch(PathBuf);

/// `(milliseconds, the pin's exact decimal string)`: two fixtures whose pins the parse holds.
const MS_200: (&str, &str) = ("200", "0.2");
const MS_300: (&str, &str) = ("300", "0.3");
const MS_400: (&str, &str) = ("400", "0.4");
const MS_500: (&str, &str) = ("500", "0.5");

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-check-{}-{}-{name}",
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

    /// The corpus every "decided" fixture shares: `REQ-0001` always applies, `REQ-0002` applies
    /// in `deployment_mode=air_gapped`, `REQ-0003` in `deployment_mode=connected`. All carry
    /// process evidence, so no test report is needed and each line below is a complete release.
    fn decided() -> impl Fn(&str) -> Self {
        |name| {
            let s = Scratch::new(name);
            s.claim("REQ-0001", None, MS_200, None, None);
            s.claim(
                "REQ-0002",
                Some("deployment_mode == air_gapped"),
                MS_300,
                None,
                None,
            );
            s.claim(
                "REQ-0003",
                Some("deployment_mode == connected"),
                MS_400,
                None,
                None,
            );
            s
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `claims/<id>.yml`. `tests` links machine evidence (a test id the report names);
    /// `None` declares process evidence. `pinned` overrides the pin's value, which is how a
    /// flagged pin is made.
    fn claim(
        &self,
        id: &str,
        applicability: Option<&str>,
        (ms, value): (&str, &str),
        pinned: Option<&str>,
        tests: Option<&str>,
    ) {
        let applicability = match applicability {
            Some(predicate) => format!("applicability:\n  - {predicate}"),
            None => "applicability: []".to_string(),
        };
        let evidence = match tests {
            Some(test) => format!("tests:\n  - \"{test}\""),
            None => "evidence: process".to_string(),
        };
        let value = pinned.unwrap_or(value);
        let text = format!(
            "schema: req.claim/2\n\
             claim_id: {id}\n\
             statement: \"The {id} service shall respond to a health check within {ms} ms.\"\n\
             {applicability}\n\
             relations: []\n\
             {evidence}\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-04\n  rejected_alternatives: []\n  assumptions: []\n\
             pins:\n  operator: LE\n  value: \"{value}\"\n  unit: s\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    /// Writes a file beside the corpus, creating its directory.
    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, bytes).expect("writes the file");
    }

    fn variants(&self, text: &str) {
        self.write(".req/variants", text.as_bytes());
    }

    fn projections(&self, text: &str) {
        self.write(".req/projections", text.as_bytes());
    }

    fn req(&self, args: &[&str]) -> Out {
        run_in(&self.0, args)
    }

    /// A JUnit report in the shape nextest writes: `probe::passes` is the only test.
    fn report(&self, name: &str, outcome: Outcome) -> String {
        let case = match outcome {
            Outcome::Passed => r#"<testcase name="passes" classname="probe"/>"#,
            Outcome::Failed => {
                r#"<testcase name="passes" classname="probe"><failure message="boom"/></testcase>"#
            }
        };
        let text = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"nextest-run\" tests=\"1\">\n<testsuite name=\"probe\" tests=\"1\">\n{case}\n</testsuite>\n</testsuites>\n"
        );
        self.write(name, text.as_bytes());
        name.to_string()
    }
}

enum Outcome {
    Passed,
    Failed,
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

impl Out {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// The step lines (`[PASS] gate …`, `[FAIL] render …`): what a run actually examined.
    fn step_lines(&self) -> Vec<&str> {
        self.stdout
            .lines()
            .filter(|line| {
                ["[PASS] ", "[FAIL] ", "[UNVERIFIABLE] "]
                    .iter()
                    .any(|prefix| line.starts_with(prefix))
            })
            .collect()
    }
}

fn run_in(dir: &Path, args: &[&str]) -> Out {
    let out = Command::new(req_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("runs req");
    Out {
        code: out.status.code().expect("req exits, never a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// `req gate [--test-report f] <line tokens>` — the standalone oracle for a variant step.
fn gate(s: &Scratch, report: Option<&str>, line: &str) -> Out {
    let mut args: Vec<&str> = vec!["gate"];
    if let Some(report) = report {
        args.extend(["--test-report", report]);
    }
    args.extend(line.split_whitespace());
    s.req(&args)
}

fn check_with(s: &Scratch, report: Option<&str>) -> Out {
    match report {
        Some(report) => s.req(&["check", "--test-report", report]),
        None => s.req(&["check"]),
    }
}

// ---------------------------------------------------------------------------
// (a)–(e): the fold over variant lines
// ---------------------------------------------------------------------------

/// (a) A line whose gate finds a defect (a flagged pin) is exit 1.
#[test]
fn a_variant_line_that_fails_is_exit_1() {
    let s = Scratch::decided()("a-fail");
    s.claim(
        "REQ-0003",
        Some("deployment_mode == connected"),
        MS_400,
        Some("999"),
        None,
    );
    s.variants("deployment_mode=air_gapped\ndeployment_mode=connected\n");

    let oracle = gate(&s, None, "deployment_mode=connected");
    assert_eq!(oracle.code, 1, "setup: the standalone gate fails the line");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout.contains("verdict: FAIL"),
        "the verdict line says FAIL:\n{}",
        out.stdout
    );
}

/// (b) A line that leaves a claim undecided is exit 3 — the gate's contract, not a pass.
#[test]
fn a_variant_left_undecided_is_exit_3() {
    let s = Scratch::decided()("b-undecided");
    s.claim("REQ-0004", Some("region == eu"), MS_500, None, None);
    s.variants("deployment_mode=connected\n");

    let oracle = gate(&s, None, "deployment_mode=connected");
    assert_eq!(oracle.code, 3, "setup: the standalone gate is UNVERIFIABLE");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.stdout.contains("verdict: UNVERIFIABLE"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("undecided"),
        "the step says what was left open, so it is actionable without a rerun:\n{}",
        out.stdout
    );
}

/// (c) A deleted `.req/variants` on a corpus carrying a selector is the default variant, which
/// the gate answers 3: a deleted list cannot turn green, and the output names the file.
#[test]
fn a_missing_variants_file_on_a_corpus_with_a_selector_is_exit_3_and_named() {
    let s = Scratch::decided()("c-missing");
    assert!(!s.path().join(".req/variants").exists());

    let oracle = gate(&s, None, "");
    assert_eq!(oracle.code, 3, "setup: a bare gate with a selector is 3");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.stdout
            .contains("(no .req/variants: the default variant)"),
        "the absence is stated on its own line:\n{}",
        out.stdout
    );
}

/// (d) Every step ran and passed: exit 0 and `verdict: PASS`.
#[test]
fn all_steps_passing_is_exit_0_and_verdict_pass() {
    let s = Scratch::decided()("d-pass");
    s.variants("deployment_mode=air_gapped\ndeployment_mode=connected\n");

    for line in ["deployment_mode=air_gapped", "deployment_mode=connected"] {
        assert_eq!(gate(&s, None, line).code, 0, "setup: {line} passes");
    }
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .contains("verdict: PASS — every declared step ran and passed"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .starts_with("# req check: 2 variant step(s), 0 projection(s)"),
        "the first line counts the steps:\n{}",
        out.stdout
    );
    assert_eq!(out.step_lines().len(), 2, "{}", out.stdout);
    assert!(
        out.step_lines()
            .iter()
            .all(|l| l.starts_with("[PASS] gate "))
    );
}

/// (e) FAIL outranks UNVERIFIABLE whichever comes last: swapping the order changes nothing.
#[test]
fn a_failure_outranks_an_undecided_line_in_either_order() {
    let s = Scratch::decided()("e-order");
    s.claim(
        "REQ-0002",
        Some("deployment_mode == air_gapped"),
        MS_300,
        Some("999"),
        None,
    );
    let failing = "deployment_mode=air_gapped";
    let undecided = "bogus=1";
    assert_eq!(gate(&s, None, failing).code, 1, "setup: fails");
    assert_eq!(gate(&s, None, undecided).code, 3, "setup: undecided");

    s.variants(&format!("{failing}\n{undecided}\n"));
    let fail_first = s.req(&["check"]);
    s.variants(&format!("{undecided}\n{failing}\n"));
    let fail_last = s.req(&["check"]);
    assert_eq!(fail_first.code, 1, "{}", fail_first.all());
    assert_eq!(fail_last.code, 1, "{}", fail_last.all());
}

// ---------------------------------------------------------------------------
// (f): a step's exit is the standalone command's exit
// ---------------------------------------------------------------------------

/// (f) For a single-line list, `check`'s exit equals `req gate --test-report … <line>` for a pass,
/// a fail, an undecided line, and a report that is absent or unreadable. The expected value is
/// also written down, so equality with a command that is wrong the same way cannot pass.
#[test]
fn every_steps_exit_equals_the_standalone_gate() {
    // Machine-evidenced claims: only the report can answer.
    let s = Scratch::new("f-equal");
    for (id, predicate, ms) in [
        ("REQ-0001", None, MS_200),
        ("REQ-0002", Some("deployment_mode == air_gapped"), MS_300),
        ("REQ-0003", Some("deployment_mode == connected"), MS_400),
    ] {
        s.claim(id, predicate, ms, None, Some("probe::passes"));
    }
    let line = "deployment_mode=connected";
    s.variants(&format!("{line}\n"));
    let passing = s.report("passing.xml", Outcome::Passed);
    let failing = s.report("failing.xml", Outcome::Failed);

    let cases: [(&str, Option<&str>, i32); 4] = [
        ("pass", Some(passing.as_str()), 0),
        ("fail", Some(failing.as_str()), 1),
        ("no report supplied", None, 3),
        ("unreadable report", Some("does-not-exist.xml"), 3),
    ];
    for (name, report, expected) in cases {
        let oracle = gate(&s, report, line);
        let out = check_with(&s, report);
        assert_eq!(oracle.code, expected, "{name}: setup oracle");
        assert_eq!(out.code, expected, "{name}: check\n{}", out.all());
        assert_eq!(
            out.code, oracle.code,
            "{name}: a step is the command's exit"
        );
    }

    // An undecided line (the same corpus, a context that decides nothing).
    s.variants("bogus=1\n");
    let oracle = gate(&s, Some(&passing), "bogus=1");
    let out = check_with(&s, Some(&passing));
    assert_eq!(oracle.code, 3, "setup oracle");
    assert_eq!(out.code, oracle.code, "{}", out.all());
}

// ---------------------------------------------------------------------------
// (g): the list grammar, decided as scripts/check.sh decides it
// ---------------------------------------------------------------------------

/// (g) Comments, blank lines, CRLF, and a final line with no newline: both releases are gated.
#[test]
fn comments_blank_lines_crlf_and_a_last_line_without_a_newline_are_read() {
    let s = Scratch::decided()("g-grammar");
    s.write(
        ".req/variants",
        b"# the releases\r\n\r\n  deployment_mode=air_gapped  # trailing comment\r\n\r\ndeployment_mode=connected",
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .starts_with("# req check: 2 variant step(s), 0 projection(s)"),
        "the last line without a newline counts, the comment and blank lines do not:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains('\r'),
        "a CR is stripped, never carried into a value or a label:\n{}",
        out.stdout
    );
}

/// (g) A tab separates tokens, exactly as a space does.
#[test]
fn a_tab_separated_line_is_split() {
    let s = Scratch::decided()("g-tab");
    s.claim("REQ-0004", Some("region == eu"), MS_500, None, None);
    s.write(
        ".req/variants",
        b"deployment_mode=connected\tregion=eu\ndeployment_mode=air_gapped region=eu\n",
    );
    assert_eq!(
        gate(&s, None, "deployment_mode=connected region=eu").code,
        0,
        "setup: the two-key release is complete"
    );
    let out = s.req(&["check"]);
    assert_eq!(
        out.code,
        0,
        "an unsplit tab would make one value `connected\\tregion=eu`, which decides nothing:\n{}",
        out.all()
    );
}

/// (g) A `*` in a value is data: not globbed against the working directory.
#[test]
fn a_star_in_a_value_is_not_expanded() {
    let s = Scratch::decided()("g-star");
    s.claim("REQ-0004", Some("region == eu"), MS_500, None, None);
    // A file the glob would match: the value must stay `*`.
    s.write("region=eu", b"");
    s.variants("deployment_mode=connected region=*\n");
    let oracle = gate(&s, None, "deployment_mode=connected region=*");
    assert_eq!(oracle.code, 3, "setup: `*` is not a declared value");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.stdout.contains("region=*"),
        "the step names the line as written:\n{}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// (h): validation happens before anything runs
// ---------------------------------------------------------------------------

/// (h) A variants line with an option-shaped token, a token with no `=`, or an empty key is a
/// malformed list: exit 2, naming file and line, and nothing was gated — not even the valid line
/// before it.
#[test]
fn a_malformed_variants_line_is_exit_2_and_nothing_runs() {
    for (name, bad) in [
        ("option", "--test-report=x"),
        ("short-option", "-x"),
        ("no-equals", "connected"),
        ("empty-key", "=connected"),
        ("option-after-pair", "deployment_mode=connected --format"),
    ] {
        let s = Scratch::decided()(&format!("h-{name}"));
        s.variants(&format!("deployment_mode=air_gapped\n{bad}\n"));
        let out = s.req(&["check"]);
        assert_eq!(out.code, 2, "{name}: {}", out.all());
        assert!(
            out.stderr.contains(".req/variants:2"),
            "{name}: the line is named as file:line:\n{}",
            out.stderr
        );
        assert!(
            out.step_lines().is_empty(),
            "{name}: nothing ran, so no step line and no [PASS]:\n{}",
            out.stdout
        );
        assert!(!out.stdout.contains("[PASS]"), "{name}: {}", out.stdout);
    }
}

/// (h) A projections line with two words is exit 2 naming the line, and the variants did not run.
#[test]
fn a_two_word_projections_line_is_exit_2_and_nothing_runs() {
    let s = Scratch::decided()("h-two-words");
    s.variants("deployment_mode=air_gapped\ndeployment_mode=connected\n");
    s.projections("docs/a.md\ndocs/b.md docs/c.md\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.stderr.contains(".req/projections:2"), "{}", out.stderr);
    assert!(!out.stdout.contains("[PASS]"), "{}", out.stdout);
}

// ---------------------------------------------------------------------------
// (i): a list that exists and cannot be read is 3, never "absent"
// ---------------------------------------------------------------------------

/// (i) `.req/variants` as a directory, and as a dangling symlink: exit 3 before any step, and the
/// output says why.
#[test]
fn a_variants_path_that_is_a_directory_or_a_dangling_link_is_exit_3() {
    let s = Scratch::decided()("i-directory");
    fs::create_dir_all(s.path().join(".req/variants")).expect("makes a directory");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains(".req/variants"), "{}", out.all());
    assert!(out.step_lines().iter().all(|l| !l.starts_with("[PASS]")));

    let s = Scratch::decided()("i-dangling");
    fs::create_dir_all(s.path().join(".req")).expect("makes .req");
    std::os::unix::fs::symlink("nowhere-at-all", s.path().join(".req/variants"))
        .expect("makes a dangling link");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains(".req/variants"), "{}", out.all());
}

/// (i) A FIFO is a path that exists and is not a regular file: exit 3, and the command must not
/// open it (an open of a FIFO with no writer blocks forever), so the run is bounded.
#[test]
fn a_variants_fifo_is_exit_3_and_is_never_opened() {
    let s = Scratch::decided()("i-fifo");
    fs::create_dir_all(s.path().join(".req")).expect("makes .req");
    let made = Command::new("mkfifo")
        .arg(s.path().join(".req/variants"))
        .status();
    let Ok(status) = made else {
        return; // no mkfifo on this host: the other two shapes still cover the class
    };
    assert!(status.success(), "mkfifo");
    let mut child = Command::new(req_bin())
        .arg("check")
        .current_dir(s.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawns req");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("polls the child") {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            panic!(
                "`req check` blocked on a FIFO: it must not open a path it has not shown to be a regular file"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(3));
}

// ---------------------------------------------------------------------------
// (j): projections
// ---------------------------------------------------------------------------

fn with_projection(name: &str) -> Scratch {
    let s = Scratch::decided()(name);
    s.variants("deployment_mode=air_gapped\ndeployment_mode=connected\n");
    fs::create_dir_all(s.path().join("docs")).expect("makes docs/");
    let wrote = s.req(&["render", "--out", "docs/spec.md"]);
    assert_eq!(
        wrote.code,
        0,
        "setup: renders a projection\n{}",
        wrote.all()
    );
    s.projections("# pinned\ndocs/spec.md\n");
    s
}

/// (j) A current projection is a PASS step whose exit is `render --check`'s.
#[test]
fn a_current_projection_is_a_passing_render_step() {
    let s = with_projection("j-current");
    assert_eq!(
        s.req(&["render", "--check", "docs/spec.md"]).code,
        0,
        "setup"
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout.contains("[PASS] render docs/spec.md (exit 0)"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .starts_with("# req check: 2 variant step(s), 1 projection(s)"),
        "{}",
        out.stdout
    );
}

/// (j) No `.req/projections`: nothing to check, and it says so.
#[test]
fn an_absent_projections_list_says_none_pinned() {
    let s = Scratch::decided()("j-absent");
    s.variants("deployment_mode=air_gapped\ndeployment_mode=connected\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .contains("projections: none pinned (.req/projections absent; nothing to check)"),
        "{}",
        out.stdout
    );
}

/// (j) A stale projection is exit 1, as `render --check` answers.
#[test]
fn a_stale_projection_is_exit_1() {
    let s = with_projection("j-stale");
    let mut text = fs::read_to_string(s.path().join("docs/spec.md")).expect("reads it");
    text.push_str("a hand edit\n");
    s.write("docs/spec.md", text.as_bytes());
    assert_eq!(
        s.req(&["render", "--check", "docs/spec.md"]).code,
        1,
        "setup"
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout.contains("[FAIL] render docs/spec.md (exit 1)"),
        "{}",
        out.stdout
    );
}

/// (j) A listed path that does not exist is a vanished projection: exit 1, never a pass.
#[test]
fn a_missing_projection_path_is_exit_1() {
    let s = with_projection("j-missing");
    s.projections("docs/spec.md\ndocs/gone.md\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.all().contains("missing"), "{}", out.all());
    assert!(out.stdout.contains("docs/gone.md"), "{}", out.stdout);
}

/// (j) A list that yields no path examined nothing: exit 1.
#[test]
fn a_comment_only_projections_list_is_exit_1() {
    let s = with_projection("j-comment-only");
    s.projections("# nothing pinned yet\n\n   \n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.all()
            .contains("pins no projection: nothing was checked"),
        "{}",
        out.all()
    );
}

/// (j) A `trace` projection is not `render`'s to judge: `render --check` answers 3, so the step
/// is 3 and nothing is green.
#[test]
fn a_trace_projection_listed_is_exit_3_as_render_check_answers() {
    let s = with_projection("j-trace");
    let wrote = s.req(&["trace", "--out", "docs/trace.md"]);
    assert_eq!(wrote.code, 0, "setup: {}", wrote.all());
    s.projections("docs/trace.md\n");
    assert_eq!(
        s.req(&["render", "--check", "docs/trace.md"]).code,
        3,
        "setup: the standalone oracle"
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
}

/// (j) The projection paths are the corpus's, not the working directory's: a check run from a
/// subdirectory reads the same list and finds the same file.
#[test]
fn projection_paths_are_relative_to_the_corpus_root() {
    let s = with_projection("j-subdir");
    fs::create_dir_all(s.path().join("sub")).expect("makes a subdirectory");
    let out = run_in(&s.path().join("sub"), &["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
}

// ---------------------------------------------------------------------------
// (k): the JSON view
// ---------------------------------------------------------------------------

fn parse(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}):\n{text}"))
}

/// (k) `--format json` is `req.check/1`; its verdict and the process exit agree; a gate step
/// embeds exactly the document `req gate --format json` prints for that line.
#[test]
fn the_json_view_is_one_document_that_agrees_with_the_exit() {
    let s = Scratch::decided()("k-document");
    s.claim(
        "REQ-0002",
        Some("deployment_mode == air_gapped"),
        MS_300,
        Some("999"),
        None,
    );
    let variants = "deployment_mode=air_gapped\nbogus=1\ndeployment_mode=connected\n";
    s.variants(variants);
    s.projections("docs/spec.md\n");
    s.write("docs/spec.md", b"not a projection\n");

    let out = s.req(&["check", "--format", "json"]);
    let doc = parse(&out.stdout);
    assert_eq!(doc["schema"], "req.check/1");
    assert_eq!(doc["generator"], "check");
    assert!(doc["rules_v"].is_u64(), "rules_v is data");
    // Steps: gate 1 (flagged pin), gate 3 (undecided), gate 0, render 3 (not a projection).
    assert_eq!(out.code, 1, "{}", out.all());
    assert_eq!(doc["verdict"], "fail", "the document and the process agree");

    let steps = doc["steps"].as_array().expect("steps is an array");
    let gate_steps: Vec<&serde_json::Value> =
        steps.iter().filter(|s| s["kind"] == "gate").collect();
    assert_eq!(gate_steps.len(), 3);
    for (step, line) in gate_steps.iter().zip(variants.lines()) {
        assert_eq!(step["line"], line);
        let (code, standalone) = gate_json(&s, line);
        assert_eq!(step["exit"].as_i64(), Some(i64::from(code)), "{line}");
        assert_eq!(
            step["verdict"], standalone["verdict"],
            "{line}: a step's verdict is the standalone document's"
        );
        assert_eq!(
            step["gate"], standalone,
            "{line}: the embedded document is the one `req gate --format json` prints"
        );
    }
    let exits: Vec<i64> = gate_steps
        .iter()
        .map(|s| s["exit"].as_i64().expect("exit is a number"))
        .collect();
    assert_eq!(exits, [1, 3, 0], "hand-written expectation per line");

    let render = steps
        .iter()
        .find(|s| s["kind"] == "render")
        .expect("a render step");
    assert_eq!(render["path"], "docs/spec.md");
    assert_eq!(render["exit"], 3);
    assert_eq!(render["verdict"], "unverifiable");
}

/// (k) An undecided-only run is `unverifiable`, exit 3, in the document.
#[test]
fn the_json_verdict_of_an_undecided_run_is_unverifiable() {
    let s = Scratch::decided()("k-unverifiable");
    s.variants("bogus=1\n");
    let out = s.req(&["check", "--format", "json"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert_eq!(parse(&out.stdout)["verdict"], "unverifiable");
}

fn gate_json(s: &Scratch, line: &str) -> (i32, serde_json::Value) {
    let mut args = vec!["gate", "--format", "json"];
    args.extend(line.split_whitespace());
    let out = s.req(&args);
    (out.code, parse(&out.stdout))
}

/// (k) The JSON of an all-pass run is a pass with exit 0.
#[test]
fn the_json_verdict_of_a_passing_run_is_pass() {
    let s = Scratch::decided()("k-all-pass");
    s.variants("deployment_mode=connected\n");
    let out = s.req(&["check", "--format=json"]);
    // `--format=json` is not the shared parser's spelling: it is a usage error, as for `gate`.
    assert_eq!(out.code, 2, "{}", out.all());

    let out = s.req(&["check", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let doc = parse(&out.stdout);
    assert_eq!(doc["verdict"], "pass");
    assert!(
        !out.stdout.contains("verdict: PASS"),
        "the JSON view carries no text verdict line"
    );
}

/// (k) `--out` / `--check` beside `--format json` is misuse: there is no flag that writes, and
/// none that narrows. Exit 2 with usage.
#[test]
fn out_beside_format_json_is_exit_2() {
    let s = Scratch::decided()("k-out");
    s.variants("deployment_mode=connected\n");
    for args in [
        &["check", "--format", "json", "--out", "x.json"][..],
        &["check", "--out", "x.json"][..],
        &["check", "--check", "docs/spec.md"][..],
        &["check", "--format", "xml"][..],
        &["check", "--format"][..],
    ] {
        let out = s.req(args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.all());
        assert!(
            out.stderr
                .contains("usage: req check [--test-report <file>] [--format text|json]"),
            "{args:?}: {}",
            out.stderr
        );
        assert!(
            !s.path().join("x.json").exists(),
            "{args:?}: nothing written"
        );
    }
}

// ---------------------------------------------------------------------------
// (l): arguments
// ---------------------------------------------------------------------------

/// (l) `--test-report` with no value or an empty `=` value, an unknown flag, and an extra
/// positional (a `key=value` included: there is no flag or argument that narrows the steps) are
/// exit 2 with the usage line.
#[test]
fn argument_misuse_is_exit_2_with_usage() {
    let s = Scratch::decided()("l-args");
    s.variants("deployment_mode=connected\n");
    for args in [
        &["check", "--test-report"][..],
        &["check", "--test-report="][..],
        &["check", "--bogus"][..],
        &["check", "extra"][..],
        &["check", "deployment_mode=connected"][..],
        &["check", "--only", "gate"][..],
        &["check", "--skip", "render"][..],
    ] {
        let out = s.req(args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.all());
        assert!(
            out.stderr
                .contains("usage: req check [--test-report <file>] [--format text|json]"),
            "{args:?}: {}",
            out.stderr
        );
        assert!(out.step_lines().is_empty(), "{args:?}: nothing ran");
    }
}

/// (l) `--test-report=<file>` is the same flag as `--test-report <file>`.
#[test]
fn the_equals_spelling_of_test_report_is_the_same_flag() {
    let s = Scratch::new("l-equals");
    s.claim("REQ-0001", None, MS_200, None, Some("probe::passes"));
    s.variants("# only the default variant\n");
    let report = s.report("passing.xml", Outcome::Passed);
    let spaced = s.req(&["check", "--test-report", &report]);
    let equals = s.req(&["check", &format!("--test-report={report}")]);
    assert_eq!(spaced.code, 0, "{}", spaced.all());
    assert_eq!(equals.code, 0, "{}", equals.all());
}

// ---------------------------------------------------------------------------
// (m), (n)
// ---------------------------------------------------------------------------

/// (m) A comment-only variants file is the default variant, and says so.
#[test]
fn a_comment_only_variants_file_is_the_default_variant_and_says_so() {
    let s = Scratch::new("m-comment-only");
    s.claim("REQ-0001", None, MS_200, None, None);
    s.variants("# no release is declared here\n\n");
    assert_eq!(
        gate(&s, None, "").code,
        0,
        "setup: a bare gate passes this corpus"
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .contains("(.req/variants declares no release: the default variant)"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .starts_with("# req check: 1 variant step(s), 0 projection(s)"),
        "{}",
        out.stdout
    );
}

/// (n) No run that is not a PASS prints `verdict: PASS`, on either stream; a run that reaches a
/// verdict prints the right one (so the absence of PASS is not the absence of any output).
#[test]
fn no_non_pass_run_prints_a_pass_verdict() {
    let mut runs: Vec<(Out, Option<&str>)> = Vec::new();

    let s = Scratch::decided()("n-undecided");
    s.variants("bogus=1\n");
    runs.push((s.req(&["check"]), Some("verdict: UNVERIFIABLE")));

    let s = Scratch::decided()("n-missing-list");
    runs.push((s.req(&["check"]), Some("verdict: UNVERIFIABLE")));

    let s = Scratch::decided()("n-fail");
    s.claim(
        "REQ-0003",
        Some("deployment_mode == connected"),
        MS_400,
        Some("9"),
        None,
    );
    s.variants("deployment_mode=connected\ndeployment_mode=air_gapped\n");
    runs.push((s.req(&["check"]), Some("verdict: FAIL")));

    let s = Scratch::decided()("n-malformed");
    s.variants("deployment_mode=connected\n--bad\n");
    runs.push((s.req(&["check"]), None));

    let s = with_projection("n-stale");
    s.write("docs/spec.md", b"stale\n");
    runs.push((s.req(&["check"]), Some("verdict: UNVERIFIABLE")));

    let s = Scratch::decided()("n-usage");
    runs.push((s.req(&["check", "--bogus"]), None));

    for (out, verdict) in runs {
        assert_ne!(
            out.code,
            0,
            "setup: every run here is a non-pass\n{}",
            out.all()
        );
        assert!(
            !out.all().contains("verdict: PASS"),
            "exit {}: a non-pass must never print a PASS verdict:\n{}",
            out.code,
            out.all()
        );
        if let Some(verdict) = verdict {
            assert!(
                out.stdout.contains(verdict),
                "expected `{verdict}`:\n{}",
                out.stdout
            );
        }
    }
}
