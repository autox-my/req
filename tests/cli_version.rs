//! `req --version` / `--help` / `<cmd> --help`: the tool's own flags and every subcommand's
//! page, answered where the subcommand lookup happens — before the corpus root is resolved.
//! They must therefore work **anywhere**, including a directory with no `claims/` ancestor,
//! and must never print a `corpus root:` line: an answer that needed a corpus would be a
//! different claim (and a verb's help may be read to find out how to create one).
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
/// "not applicable", a different claim), and its value is one of exactly four states — `unknown`,
/// when the commit itself could not be resolved and so there is nothing to attribute, or a
/// resolved short SHA, suffixed `-dirty` when a tracked file is modified and
/// `-cleanliness-unknown` when the cleanliness probe could not answer at all. A bare `<sha>`
/// therefore always means *clean*: the round-1 defect was emitting exactly that for a probe which
/// had established nothing.
fn assert_build_identity(stdout: &str) {
    let value = axis(stdout, "built from").unwrap_or_else(|| {
        panic!(
            "the `built from` line must be present, even when it cannot be established:\n{stdout}"
        )
    });
    if value == "unknown" {
        return;
    }
    // Every suffix a resolved commit can carry, longest first, so the cleanliness marker is never
    // mistaken for part of the SHA it follows.
    let sha = value
        .strip_suffix("-cleanliness-unknown")
        .or_else(|| value.strip_suffix("-dirty"))
        .unwrap_or(value);
    assert!(
        (7..=40).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "`built from` must be `unknown` or a short git SHA (optionally `-dirty` or \
         `-cleanliness-unknown`), not {value:?}"
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

// ---------------------------------------------------------------------------
// Per-command help: `<cmd> --help` is answered where the verb is looked up
// ---------------------------------------------------------------------------

/// Every verb the tool has, written down once here so a loop can ask each one for its page.
///
/// This list is **not** the completeness check — the help table makes a verb without a page
/// fail to compile, and `every_verb_named_in_usage_has_a_page` scrapes `USAGE` for the verbs
/// the tool advertises. It exists because two tests need a concrete list (the mutation
/// `init` case is outside the table, and a loop that ran zero times would pass silently).
const ALL_COMMANDS: &[&str] = &[
    "audit", "baseline", "bench", "canon", "compare", "decision", "gate", "impact", "init",
    "propose", "render", "select", "trace",
];

/// The page `<command> --help` prints, or a failure that names the command and its output.
fn help_page(cwd: &Path, command: &str) -> String {
    let out = run_at(cwd, &[command, "--help"]);
    assert_eq!(
        out.code, 0,
        "`req {command} --help` must be a request that was answered, not a usage error or a \
         corpus lookup (exit {}):\nstdout:\n{}\nstderr:\n{}",
        out.code, out.stdout, out.stderr
    );
    assert!(
        !out.stderr.contains("corpus root:"),
        "`req {command} --help` resolves no root and must not name one:\n{}",
        out.stderr
    );
    out.stdout
}

/// The verbs named in the `USAGE` text's verb column, scraped from the text **as it is** —
/// never a second hand-maintained list, and never by changing `USAGE`'s layout to suit this
/// parser. The column is the part of a line before the description, and descriptions are
/// separated from it by a run of spaces; within the column, `|` separates alternatives and
/// the verb list ends at the first flag, placeholder or `k=v` argument.
///
/// A line is a verb row only when it is a verb column **followed by a description**, or an
/// `a | b` alternatives row. Prose that wraps into the margin is not a verb row: its first
/// words are lowercase and would otherwise read as verbs, and this function is the oracle
/// behind `every_verb_named_in_usage_has_a_page`.
fn usage_verbs(usage: &str) -> Vec<String> {
    let mut verbs: Vec<String> = Vec::new();
    for line in usage.lines() {
        let Some(indented) = line.strip_prefix("  ") else {
            continue;
        };
        // The verb column ends where the description column begins: two or more spaces.
        let column = indented.split("  ").next().unwrap_or("");
        let has_description = column.len() < indented.len();
        let has_alternatives = column.contains(" | ");
        if !has_description && !has_alternatives {
            continue;
        }
        for token in column.split_whitespace() {
            if token == "|" {
                continue;
            }
            let is_verb = token.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && token
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !is_verb {
                break;
            }
            if !verbs.iter().any(|verb| verb == token) {
                verbs.push(token.to_string());
            }
        }
    }
    verbs
}

/// Whether a page's first line is that command's usage line, with the command boundary
/// intact: `render` must not be satisfied by `usage: req rendering …`, and the synopsis may
/// be absent (a command taking no arguments) or separated by whitespace.
fn first_line_names_command(page: &str, command: &str) -> bool {
    let first = page.lines().next().unwrap_or_default();
    first
        .strip_prefix(&format!("usage: req {command}"))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

#[test]
fn every_commands_help_answers_with_no_corpus_and_prints_no_root_line() {
    // Fact 2's bug: `req canon --help` used to resolve the root first and answer
    // `[UNVERIFIABLE] no claims/ directory …` with exit 3. A help page must be readable in a
    // directory with no corpus — help is exactly what you read before creating one.
    let fx = NoCorpus::new("req-no-corpus-per-command-help");
    for command in ALL_COMMANDS {
        let page = help_page(fx.path(), command);
        assert!(
            first_line_names_command(&page, command),
            "`req {command} --help` must begin with its own usage line, not another verb's:\n{page}"
        );
    }
}

#[test]
fn the_short_help_flag_is_the_same_page_for_every_command() {
    let fx = NoCorpus::new("req-no-corpus-per-command-short-help");
    for command in ALL_COMMANDS {
        let long = run_at(fx.path(), &[command, "--help"]);
        let short = run_at(fx.path(), &[command, "-h"]);
        assert_eq!(short.code, 0, "`req {command} -h`:\n{}", short.stderr);
        assert_eq!(
            short.stdout, long.stdout,
            "`req {command} -h` and `req {command} --help` are one answer, not two"
        );
    }
}

#[test]
fn init_help_is_answered_before_the_init_handler_sees_it() {
    // Fact 5's bug: `init` is special-cased before the table, so `--help` reached it as a
    // command-line argument and answered `usage: req init` + `unexpected argument: --help`,
    // exit 2. Asking for help must never run the command, and here it also must not create a
    // corpus as a side effect of being asked.
    let fx = NoCorpus::new("req-no-corpus-init-help");
    let page = help_page(fx.path(), "init");
    assert!(
        first_line_names_command(&page, "init"),
        "`req init --help` must print the init page:\n{page}"
    );
    assert!(
        page.contains("exit:"),
        "the page carries its exit contract:\n{page}"
    );
    assert!(
        !fx.path().join("claims").exists() && !fx.path().join("aliases").exists(),
        "a help request must not create a corpus"
    );
}

#[test]
fn every_page_starts_with_usage_its_own_name() {
    // R5's "one line each" row, pinned separately from the no-corpus loop so a page that
    // started with a different verb's usage line is still caught when the corpus exists.
    let fx = NoCorpus::new("req-page-usage-line");
    for command in ALL_COMMANDS {
        let page = help_page(fx.path(), command);
        assert!(
            first_line_names_command(&page, command),
            "the first line of `req {command} --help` must be `usage: req {command} <synopsis>`:\n{}",
            page.lines().next().unwrap_or_default()
        );
    }
}

#[test]
fn every_page_carries_an_exit_line() {
    // The exit contract is the payload a bare usage line cannot carry — the thing issue #45
    // is actually about. Deleting one page's `exit:` line must redden this test.
    let fx = NoCorpus::new("req-page-exit-line");
    for command in ALL_COMMANDS {
        let page = help_page(fx.path(), command);
        assert!(
            page.lines().any(|line| line.starts_with("exit:")),
            "`req {command} --help` must carry an `exit:` line:\n{page}"
        );
    }
}

#[test]
fn render_and_trace_pages_differ_and_each_names_its_projection() {
    // Fact 4's bug: both printed `usage: req render|trace [key=value ...]`. Two different
    // projections need two different pages, and each must say which one it is.
    let fx = NoCorpus::new("req-render-trace-pages");
    let render = help_page(fx.path(), "render");
    let trace = help_page(fx.path(), "trace");

    assert_ne!(
        render, trace,
        "`render` and `trace` are different projections and must not share one page"
    );
    assert!(
        render.contains("spec view"),
        "render's page names the spec view it projects:\n{render}"
    );
    assert!(
        trace.contains("traceability") && trace.contains("incoming"),
        "trace's page names the traceability matrix and its derived incoming edges:\n{trace}"
    );
    for (command, page) in [("render", &render), ("trace", &trace)] {
        assert!(
            page.contains("undecided"),
            "{command}'s page names how an undecided claim (or an edge into one) is handled:\n{page}"
        );
        assert!(page.contains("projection"), "{command}'s page:\n{page}");
    }
}

#[test]
fn every_verb_named_in_usage_has_a_page() {
    // Fact 7: the advertised verb list is `USAGE`'s verb column. Scrape it from the text the
    // tool actually prints, so a verb that exists only in the table (or only in `USAGE`)
    // cannot hide. The table makes a *table* verb without a page a compile error; this test
    // is what catches a verb added to `USAGE` alone, and a scrape that silently found nothing.
    let fx = NoCorpus::new("req-usage-verbs");
    let usage = run_at(fx.path(), &["--help"]);
    assert_eq!(usage.code, 0, "{}", usage.stderr);

    // Set equality in both directions: a verb added to `USAGE` must have a page (the loop
    // below), and a verb added to the table and `USAGE` must be added here too — otherwise it
    // would escape the per-page loops above and could ship without an `exit:` line.
    let mut verbs = usage_verbs(&usage.stdout);
    let mut expected: Vec<String> = ALL_COMMANDS.iter().map(|c| c.to_string()).collect();
    verbs.sort();
    expected.sort();
    assert_eq!(
        verbs, expected,
        "USAGE's verb column and ALL_COMMANDS must name the same verbs — add a new advertised \n\
         verb to ALL_COMMANDS so every page test covers it:\n{}",
        usage.stdout
    );

    for verb in &verbs {
        let page = help_page(fx.path(), verb);
        assert!(
            first_line_names_command(&page, verb),
            "`USAGE` advertises `{verb}`, so it must have a page:\n{page}"
        );
    }
}
