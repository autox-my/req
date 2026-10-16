//! `req --version` / `--help`: the tool's own flags, answered where the subcommand lookup
//! happens — before the corpus root is resolved. They must therefore work **anywhere**,
//! including a directory with no `claims/` ancestor, and must never print a `corpus root:`
//! line: an answer that needed a corpus would be a different claim.
//!
//! The axes are asserted against the constants **by name** (`RULES_V`, `BASELINE_VERSION`,
//! `DECISION_VERSION`, `claim_schema()`), never against a re-typed literal: a deliberate bump
//! must not redden a test that is not about the bump. The build identity is asserted as a
//! *contract* — present, and either the honest `unknown` or a plausible short SHA — never as a
//! value: a specific sha would redden on every commit and be rewritten to whatever the code
//! prints, which is the one thing this test must not do.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// The absolute path of the compiled binary, so the test does not depend on `$PATH`.
fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_at(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        // The resolver reads `REQ_ROOT` before the walk-up, so an inherited value would name a
        // corpus and make the no-corpus fixture check nothing.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The value printed after the line label `label` (e.g. `built from`), or `None` when no line
/// carries it. The label must be followed by whitespace, so `rules_v` cannot be satisfied by a
/// line belonging to a longer label.
fn axis<'a>(stdout: &'a str, label: &str) -> Option<&'a str> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .find_map(|line| {
            let rest = line.strip_prefix(label)?;
            rest.starts_with(char::is_whitespace).then(|| rest.trim())
        })
}

fn assert_axis(stdout: &str, label: &str, expected: &str) {
    assert_eq!(
        axis(stdout, label),
        Some(expected),
        "the `{label}` axis in:\n{stdout}"
    );
}

/// The build identity as a contract: the line is **never omitted** (an absent line reads as
/// "not applicable", a different claim), and its value is either `unknown` — no git, no
/// `.git`, a tarball — or a plausible short SHA, optionally marked dirty.
fn assert_build_identity(stdout: &str) {
    let value = axis(stdout, "built from").unwrap_or_else(|| {
        panic!(
            "the `built from` line must be present, even when it cannot be established:\n{stdout}"
        )
    });
    if value == "unknown" {
        return;
    }
    let sha = value.strip_suffix("-dirty").unwrap_or(value);
    assert!(
        (7..=40).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "`built from` must be `unknown` or a short git SHA (optionally `-dirty`), not {value:?}"
    );
}

/// A directory with **no `claims/` ancestor**, and that fact *asserted* rather than assumed: if
/// some ancestor does hold `claims/` (a `TMPDIR` inside a checkout), the walk-up would resolve a
/// corpus by accident and the test would pass while checking nothing.
///
/// It lives under the system temp dir, not `CARGO_TARGET_TMPDIR`: the build tree is inside the
/// repository, whose own `claims/` is an ancestor of it.
///
/// The **name** is unique per process and per instance — the process id covers two processes
/// sharing a `TMPDIR` (parallel CI jobs, two checkouts), the counter covers two fixtures in one
/// process. A fixed name meant `new`'s re-create and `Drop`'s remove ran against a directory
/// another test was living in.
struct NoCorpus(PathBuf);

/// Distinguishes fixtures within one process. The process id is what extends that distinctness
/// across processes; this only has to make two instances in one run differ.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

impl NoCorpus {
    fn new(name: &str) -> Self {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("{name}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the fixture directory");
        if let Some(found) = dir.ancestors().find(|d| req::root::is_corpus_root(d)) {
            panic!(
                "the fixture assumes no ancestor holds claims/, but {} does",
                found.display()
            );
        }
        NoCorpus(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for NoCorpus {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Two fixtures must never share a directory — within one process or across two. `new` removes
/// and re-creates the path and `Drop` removes it, so a shared name is one test running inside a
/// directory another test is about to delete, and the failure lands on whatever assertion happens
/// to be reached next. The label is not the identity: the path must carry the process id as well
/// as a per-instance component.
#[test]
fn fixture_directories_are_unique_per_instance_and_carry_the_process_id() {
    let first = NoCorpus::new("req-fixture-uniqueness");
    let second = NoCorpus::new("req-fixture-uniqueness");

    assert_ne!(
        first.path(),
        second.path(),
        "two fixtures created with the same label are two directories, not one"
    );
    let pid = std::process::id().to_string();
    for fx in [&first, &second] {
        // The process id as its own `-`-separated token: present in the digits of an unrelated
        // number would not make distinctness hold across processes, which is the property.
        let name = fx
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        assert!(
            name.split('-').any(|part| part == pid),
            "the fixture name must carry the process id, so two processes sharing TMPDIR cannot \
             collide: {name:?}"
        );
        assert!(
            fx.path().is_dir(),
            "the fixture is a directory while it lives: {}",
            fx.path().display()
        );
    }
}

// ---------------------------------------------------------------------------
// The trap: no corpus
// ---------------------------------------------------------------------------

#[test]
fn version_works_with_no_corpus_and_prints_no_root_line() {
    let fx = NoCorpus::new("req-no-corpus-version");
    let out = run_at(fx.path(), &["--version"]);

    assert_eq!(
        out.code, 0,
        "`--version` must answer with no corpus at all:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        !out.stderr.contains("corpus root:"),
        "`--version` resolves no root and must not name one:\n{}",
        out.stderr
    );

    // The first line stays single-line and greppable, like any other tool's.
    let first = out.stdout.lines().next().unwrap_or_default();
    assert_eq!(first, format!("req {}", env!("CARGO_PKG_VERSION")));

    assert_axis(&out.stdout, "rules_v", &req::canon::RULES_V.to_string());
    assert_axis(
        &out.stdout,
        "baseline_envelope",
        &req::baseline::BASELINE_VERSION.to_string(),
    );
    assert_axis(
        &out.stdout,
        "decision_chain",
        &req::decision::DECISION_VERSION.to_string(),
    );
    assert_axis(&out.stdout, "claim_schema", &req::canon::claim_schema());
    assert_build_identity(&out.stdout);
}

#[test]
fn version_is_answered_before_the_root_is_resolved() {
    // The same command, run *inside* a corpus: the flag is answered in the subcommand-lookup
    // position, before `root::resolve_corpus_root`, so it resolves nothing even when it could.
    // A later move of that handling would still exit 0 here and be caught by this line.
    let out = run_at(Path::new(env!("CARGO_MANIFEST_DIR")), &["--version"]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert!(
        !out.stderr.contains("corpus root:"),
        "the root is resolved once, after dispatch, and `--version` never reaches it:\n{}",
        out.stderr
    );
}

#[test]
fn the_short_version_flag_is_the_same_answer() {
    let fx = NoCorpus::new("req-no-corpus-short-version");
    let long = run_at(fx.path(), &["--version"]);
    let short = run_at(fx.path(), &["-V"]);
    assert_eq!(short.code, 0, "{}\n{}", short.stdout, short.stderr);
    assert_eq!(
        short.stdout, long.stdout,
        "`-V` and `--version` are one answer, not two"
    );
}

// ---------------------------------------------------------------------------
// `--help` is help, not the no-command usage error
// ---------------------------------------------------------------------------

#[test]
fn help_works_with_no_corpus_and_exits_zero() {
    let fx = NoCorpus::new("req-no-corpus-help");
    let out = run_at(fx.path(), &["--help"]);

    assert_eq!(
        out.code, 0,
        "`--help` is a request that was answered, not a usage error:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        out.stdout.contains("usage: req"),
        "`--help` prints the usage:\n{}",
        out.stdout
    );
    assert!(
        !out.stderr.contains("corpus root:"),
        "`--help` resolves no root:\n{}",
        out.stderr
    );
    // `USAGE` documents the flags, so the help text and the tool cannot describe different
    // tools: a flag that exists only in one of them is the drift this pin is for.
    for documented in ["--version", "--help"] {
        assert!(
            out.stdout.contains(documented),
            "`USAGE` must document {documented}:\n{}",
            out.stdout
        );
    }
}

#[test]
fn the_short_help_flag_is_the_same_answer() {
    let fx = NoCorpus::new("req-no-corpus-short-help");
    let long = run_at(fx.path(), &["--help"]);
    let short = run_at(fx.path(), &["-h"]);
    assert_eq!(short.code, 0, "{}\n{}", short.stdout, short.stderr);
    assert_eq!(
        short.stdout, long.stdout,
        "`-h` and `--help` are one answer"
    );
}

// ---------------------------------------------------------------------------
// The usage errors stay usage errors
// ---------------------------------------------------------------------------

#[test]
fn bare_req_and_unknown_arguments_stay_usage_errors() {
    let fx = NoCorpus::new("req-no-corpus-usage");

    // `--help` must not be folded into this path: a request for help is answered; saying
    // nothing is a usage error.
    let bare = run_at(fx.path(), &[]);
    assert_eq!(
        bare.code, 2,
        "bare `req` is a usage error:\n{}",
        bare.stderr
    );
    assert!(bare.stderr.contains("usage: req"), "{}", bare.stderr);

    for argv in [&["--nope"][..], &["--ver"][..], &["canonn"][..]] {
        let out = run_at(fx.path(), argv);
        assert_eq!(
            out.code, 2,
            "{argv:?} is not a command and not a flag this tool knows:\n{}\n{}",
            out.stdout, out.stderr
        );
    }
}
