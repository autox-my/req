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
    "audit", "baseline", "bench", "canon", "check", "compare", "decision", "diff", "gate",
    "impact", "init", "next-id", "pin", "propose", "render", "satisfy", "select", "selftest",
    "trace",
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
    // The marker belongs on stderr today; if it moved to (or were copied onto) stdout, the
    // page text would be polluted for the reader who pipes it, and the caller must see that.
    assert!(
        !out.stdout.contains("corpus root:"),
        "`req {command} --help` must not print a root line on stdout either:\n{}",
        out.stdout
    );
    out.stdout
}

/// The verbs named in the `USAGE` text's verb column, scraped from the text **as it is** —
/// never a second hand-maintained list, and never by changing `USAGE`'s layout to suit this
/// parser. Within the column, `|` separates alternatives and the verb list ends at the first
/// flag, placeholder or `k=v` argument.
///
/// The description column is **derived from the text**, not hardcoded: it is the commonest
/// offset at which a run of two or more spaces ends among the indented lines, which is where
/// the rows put their prose. A line is a verb row only when its description starts there, or
/// when it is an `a | b` alternatives row (which carries no description). Prose that wraps
/// back into the margin is therefore never parsed as verbs — a wrapped word is not at the
/// description column — and a deliberate relayout moves the anchor with the text instead of
/// silently misreading the column. This is the oracle behind
/// `every_verb_named_in_usage_has_a_page`, so its failure mode must be "no verbs found",
/// never "prose demanded pages".
fn usage_verbs(usage: &str) -> Vec<String> {
    let indented: Vec<&str> = usage
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .collect();

    // Where the rows put their descriptions: the strict-majority end of a 2+ space run. A
    // tie means the text has no single description column, and picking one would let prose
    // choose the column (and then reject the legitimate rows) — a loud failure is the only
    // honest answer, because a guessed column can hide a verb.
    let mut offsets: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    for line in &indented {
        if let Some(offset) = description_offset(line) {
            *offsets.entry(offset).or_default() += 1;
        }
    }
    let described: usize = offsets.values().sum();
    let (column, count) = offsets
        .into_iter()
        .max_by_key(|&(offset, count)| (count, offset))
        .unwrap_or((0, 0));
    assert!(
        described > 0 && count * 2 > described,
        "USAGE has no single description column ({count} line(s) at offset {column}, \
         {described} described line(s)) — fix the layout or teach the scraper"
    );
    let description_column = Some(column);

    let mut unclassifiable: Vec<&str> = Vec::new();
    let mut verbs: Vec<String> = Vec::new();
    for line in &indented {
        let column = match description_column {
            Some(column) if description_offset(line) == Some(column) => line[..column].trim_end(),
            // An alternatives row has no description column; its whole line is the column.
            _ if line.contains(" | ") && description_offset(line).is_none() => line.trim_end(),
            _ => {
                // An indented, non-empty line this oracle cannot place is an ambiguity it
                // must not swallow: a misaligned row, or a bare verb row with no description
                // at all, would otherwise vanish from the scrape and the set-equality guard
                // would agree with the omission. Fail loudly instead, naming the line.
                if !line.trim().is_empty() {
                    unclassifiable.push(line);
                }
                continue;
            }
        };
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
    assert!(
        unclassifiable.is_empty(),
        "the USAGE scrape cannot classify these indented lines — fix their layout or teach the \
         scraper (a silent skip could hide a verb):\n{unclassifiable:#?}"
    );
    verbs
}

/// Where a line's description starts (the index in `line` of the first non-space after the
/// first run of two or more spaces), or `None` when the line has no such run.
fn description_offset(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let start = bytes.windows(2).position(|pair| pair == b"  ".as_slice())?;
    let after_spaces = bytes[start..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|index| start + index)?;
    Some(after_spaces)
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
fn the_usage_line_check_enforces_the_command_boundary_and_the_empty_synopsis() {
    // The strip_prefix rule is load-bearing: without the boundary, `render` would accept
    // `usage: req rendering …` and the `render|trace` line. The empty synopsis is `init`'s.
    assert!(first_line_names_command(
        "usage: req render [k=v ...]\n",
        "render"
    ));
    assert!(first_line_names_command("usage: req init\n", "init"));
    assert!(
        !first_line_names_command("usage: req rendering [k=v ...]\n", "render"),
        "a longer verb sharing the prefix is not this command"
    );
    assert!(
        !first_line_names_command("usage: req render|trace [k=v ...]\n", "render"),
        "a combined usage line is not either command's page"
    );
}

#[test]
#[should_panic(expected = "cannot classify")]
fn the_usage_scrape_names_a_bare_verb_row_it_cannot_place() {
    // A row with no description and no alternatives is still a row: skipping it silently could
    // hide an advertised verb, which is exactly what this oracle exists to catch.
    let usage = "\
usage: req x

  bench [corpus.yml]              the FMR gate
  canon [--fill | --verify]       parse diagnostics
  compare <a.yml> <b.yml>         what kind of change
  newverb

The corpus root is resolved at runtime.";
    let _ = usage_verbs(usage);
}

#[test]
fn the_usage_scrape_reads_the_verb_column_and_never_prose() {
    // A description column is derived from the text (every row's prose starts at the same
    // offset). These are the shapes that made a naive "split on two spaces" parser read prose
    // as verbs: a wrapped description re-entering the margin, one wrapped with its own
    // double-space run, and the `a | b` alternatives row that carries no description at all.
    let usage = "\
usage: req [--root <dir>] <command> [args]

  bench [corpus.yml]              the FMR gate
  select | render | trace | gate k=v [...]
                                  the variant projections and the release gate
  init                            create a corpus
  --version | -V                  the versions
  --help | -h                     this text

The corpus root is resolved at runtime.";
    assert_eq!(
        usage_verbs(usage),
        ["bench", "select", "render", "trace", "gate", "init"]
    );
}

#[test]
#[should_panic(expected = "no single description column")]
fn the_usage_scrape_refuses_text_without_a_description_column() {
    // One row and one prose wrap are a tie: there is no column to anchor on, and picking one
    // would let the prose choose it and reject the legitimate row. Fail loudly instead.
    let usage = "\
usage: req x

  bench [corpus.yml]              the FMR gate
  the variant  projections and the release gate

The corpus root is resolved at runtime.";
    let _ = usage_verbs(usage);
}

#[test]
#[should_panic(expected = "cannot classify")]
fn the_usage_scrape_names_a_misaligned_row_it_cannot_place() {
    // With a clear majority column, a row whose description sits elsewhere is a layout bug the
    // oracle must name rather than skip: a silent skip could hide a verb from the scrape.
    let usage = "\
usage: req x

  bench [corpus.yml]              the FMR gate
  canon [--fill | --verify]       parse diagnostics
  compare <a.yml> <b.yml>         what kind of change
  reject [--force]  reject a claim

The corpus root is resolved at runtime.";
    let _ = usage_verbs(usage);
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
fn render_and_trace_usage_errors_name_the_invoked_verb() {
    // The pages made `render` and `trace` two verbs; the shared usage error must not keep
    // naming a pair that exists nowhere else. It is still a usage error, not a page. Since ADR
    // 0018 the synopsis carries the two committed-projection flags, and since pack C it carries
    // `--format`, so the error must name the invoked verb *and* the shape it accepts.
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"));
    for command in ["render", "trace"] {
        let out = run_at(corpus, &[command, "--bogus"]);
        assert_eq!(
            out.code, 2,
            "`{command} --bogus` is a usage error:\n{}",
            out.stderr
        );
        assert!(
            out.stderr.contains(&format!(
                "usage: req {command} [--out <path>] [--check <path>] \
                 [--format text|json] [key=value ...]"
            )),
            "the error names the invoked verb and its accepted shape, not the pair:\n{}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("render|trace"),
            "the combined line is gone from the error path:\n{}",
            out.stderr
        );
    }
}

#[test]
fn a_help_flag_wins_in_a_non_leading_position() {
    // AGENTS.md states the contract: among a command's arguments a help flag wins wherever it
    // appears. A regression to `args.first().map(is_help_flag)` would keep every other test in
    // this file green, so one non-leading case is pinned here.
    let fx = NoCorpus::new("req-help-non-leading");
    let out = run_at(fx.path(), &["propose", "--gold", "--help"]);
    assert_eq!(
        out.code, 0,
        "a help flag after an option value is still help:\n{}",
        out.stderr
    );
    assert!(
        first_line_names_command(&out.stdout, "propose"),
        "the page for the invoked verb is printed:\n{}",
        out.stdout
    );
    assert!(!out.stderr.contains("corpus root:"), "{}", out.stderr);
}

#[test]
fn a_help_request_for_an_unknown_verb_stays_a_usage_error() {
    // The failure direction of the lookup: help must not turn a typo into an answer. A
    // regression that printed some page for any first argument would pass every test above.
    let fx = NoCorpus::new("req-unknown-verb-help");
    let out = run_at(fx.path(), &["not-a-command", "--help"]);
    assert_eq!(
        out.code, 2,
        "an unknown verb is a usage error even with --help:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        !out.stdout.contains("usage: req not-a-command"),
        "an unknown verb must not receive a page:\n{}",
        out.stdout
    );
    assert!(
        out.stderr
            .contains("unrecognized subcommand: not-a-command"),
        "the usage error names the unknown verb:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("corpus root:"),
        "the lookup fails before the root is resolved:\n{}",
        out.stderr
    );
}

#[test]
fn the_short_help_flag_is_the_same_page_for_every_command() {
    let fx = NoCorpus::new("req-no-corpus-per-command-short-help");
    for command in ALL_COMMANDS {
        let long = help_page(fx.path(), command);
        let short = run_at(fx.path(), &[command, "-h"]);
        assert_eq!(short.code, 0, "`req {command} -h`:\n{}", short.stderr);
        assert!(
            !short.stderr.contains("corpus root:"),
            "`req {command} -h` resolves no root, exactly like the long form:\n{}",
            short.stderr
        );
        assert_eq!(
            short.stdout, long,
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
fn init_help_does_not_swallow_init_s_usage_errors() {
    // The page says `init` takes no arguments and no `--root` (exit 2), and AGENTS.md says the
    // same. A help flag must not turn either into a page: a user who got the invocation wrong
    // is exactly the one who needs the error.
    let fx = NoCorpus::new("req-init-help-usage");
    let root = fx.path().to_str().expect("fixture path is UTF-8");
    let with_root = run_at(fx.path(), &["init", "--root", root, "--help"]);
    assert_eq!(
        with_root.code, 2,
        "`init --root … --help` is still a usage error:\n{}\n{}",
        with_root.stdout, with_root.stderr
    );
    assert!(
        with_root.stderr.contains("takes no --root"),
        "the error names the offending flag:\n{}",
        with_root.stderr
    );

    let with_argument = run_at(fx.path(), &["init", "junk", "--help"]);
    assert_eq!(
        with_argument.code, 2,
        "`init junk --help` is still a usage error:\n{}\n{}",
        with_argument.stdout, with_argument.stderr
    );
    assert!(
        with_argument.stderr.contains("unexpected argument: junk"),
        "the error names the offending argument:\n{}",
        with_argument.stderr
    );
    let help_first = run_at(fx.path(), &["init", "--help", "junk"]);
    assert_eq!(
        help_first.code, 2,
        "`init --help junk` is still a usage error:\n{}\n{}",
        help_first.stdout, help_first.stderr
    );
    assert!(
        help_first.stderr.contains("unexpected argument: junk"),
        "the error names the actual bad argument, not the help flag:\n{}",
        help_first.stderr
    );

    assert!(
        !fx.path().join("claims").exists(),
        "a refused help request creates nothing"
    );
}

#[test]
fn every_page_starts_with_usage_its_own_name_where_the_root_would_resolve() {
    // R5's "one line each" row. The no-corpus loop above proves help needs no root; this arm
    // runs where the root *would* resolve, which is the failure the no-corpus fixture cannot
    // see: a help path that resolved the corpus first would print `corpus root:` and still
    // exit 0. `help_page` asserts that line is absent, so this is coverage, not a duplicate.
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        req::root::is_corpus_root(corpus),
        "this arm needs a directory where root resolution would succeed: {}",
        corpus.display()
    );
    for command in ALL_COMMANDS {
        let page = help_page(corpus, command);
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
        let exit = page
            .lines()
            .find(|line| line.starts_with("exit:"))
            .unwrap_or_else(|| {
                panic!("`req {command} --help` must carry an `exit:` line:\n{page}")
            });
        let first_code = exit
            .strip_prefix("exit: ")
            .and_then(|codes| codes.split_whitespace().next());
        assert_eq!(
            first_code,
            Some("0"),
            "`req {command} --help`'s `exit:` line must carry the codes, starting with 0:\n{exit}"
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

// ---------------------------------------------------------------------------
// M2-3: the help pages are the command reference
// ---------------------------------------------------------------------------
//
// `AGENTS.md` used to hold a second copy of every command's contract, and each page's footer
// pointed into it. The contract now lives on the page, where the code and these tests are, and
// the footer points at the one document that is true after the cut. A page that loses a
// contract below has lost its only home for it, so each phrase is a pin on a rule, not on
// wording for its own sake.

/// A page with its line wrapping removed, so a pinned phrase may straddle the page's own line
/// breaks without the test depending on where they fall.
fn squashed(page: &str) -> String {
    page.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_page_ends_by_pointing_at_the_adopting_page_and_none_points_into_agents_md() {
    // The old footer sent a reader into `AGENTS.md` §Commands, the copy that drifts. The new one
    // names the loop's home only; the command reference is the page the reader is already on.
    let fx = NoCorpus::new("req-page-footer");
    for command in ALL_COMMANDS {
        let page = help_page(fx.path(), command);
        assert_eq!(
            page.trim_end().lines().last(),
            Some("`docs/adopting.md` for the loop."),
            "`req {command} --help` must end with the adopting-page pointer:\n{page}"
        );
        assert!(
            !page.contains("AGENTS.md"),
            "`req {command} --help` must not point into AGENTS.md — the page is the reference:\n{page}"
        );
    }
    let usage = run_at(fx.path(), &["--help"]);
    assert!(
        !usage.stdout.contains("AGENTS.md"),
        "`req --help` must not point into AGENTS.md either:\n{}",
        usage.stdout
    );
}

#[test]
fn the_top_level_help_carries_the_root_help_and_exit_contract_it_used_to_borrow() {
    let fx = NoCorpus::new("req-usage-contract");
    let usage = squashed(&run_at(fx.path(), &["--help"]).stdout);
    for phrase in [
        // The root: how it is chosen is already there; that it is announced once on stderr, and
        // that no corpus is UNVERIFIABLE rather than an empty answer, was only in AGENTS.md.
        "printed once to stderr",
        "[UNVERIFIABLE] and exit 3",
        // The help contract: wherever it appears, before the root, and what stays a usage error.
        "A help flag wins wherever it appears",
        "answered before any root is resolved",
        "an unknown subcommand and an unknown flag are exit 2",
        // The exit contract common to every command.
        "tri-state",
        "3 UNVERIFIABLE",
    ] {
        assert!(
            usage.contains(phrase),
            "`req --help` must carry {phrase:?}:\n{usage}"
        );
    }
}

/// Each command's page, and the contract phrases that moved onto it from `AGENTS.md`.
const MOVED_CONTRACTS: &[(&str, &[&str])] = &[
    (
        "bench",
        &[
            "append-only",
            "dangerous confusions = 0",
            "0.95",
            "misspelled `direction:` label fails the gate",
        ],
    ),
    (
        "canon",
        &[
            "Nothing derived is stored in a claim file",
            "`--fill` and `--verify` are removed",
            "a *contradiction*",
            "*not carried*",
            "There is no third, non-flagging category",
            "A flagged pin moves no address",
            "NOT JUDGED",
            "migration candidate",
            "`tools/migrate_claims.py`",
            "a parse error that names the key",
        ],
    ),
    ("select", &["`key == value` and `key >= number`"]),
    (
        "render",
        &[
            "complete even when the plain command's verdict is open",
            "first differing line",
            "second question",
        ],
    ),
    (
        "trace",
        &[
            "complete even when the plain command's verdict is open",
            "first differing line",
            "second question",
        ],
    ),
    (
        "gate",
        &[
            "`pins_hold`",
            "it ran over zero pins, it was not skipped",
            "`selectors_confirmed`",
            "refused on purpose",
            "a skipped test is not a pass",
            "linked test not in the report",
            "evidence: process",
            "`process_evidenced`",
        ],
    ),
    (
        "baseline",
        &[
            "usage error (exit 2)",
            "chain head",
            "tests_hash",
            "prospective",
            "signer-asserted",
            "`valid_from` is recorded but not enforced",
            "fails closed on every envelope",
        ],
    ),
    (
        "check",
        &[
            "ONE verdict",
            "A step's exit is the command's exit",
            "a defect (1) outranks",
            "Absence is not silence",
            "a deleted list cannot turn green",
            "validated before anything runs",
            "no `--only`",
            "no flag that narrows",
            "`req.check/1`",
            "2 usage or a malformed list",
        ],
    ),
    (
        "diff",
        &[
            "Claims are matched by claim_id",
            "`req.diff/1`",
            "a description, not a verdict",
            "never says PASS or FAIL",
            "`fetch-depth: 0`",
            "1 only with `--fail-on`",
            "a candidate renumber, never inferred as a rename",
        ],
    ),
    ("audit", &["nothing is ratified"]),
    ("decision", &["a log is history", "writes nothing"]),
    ("init", &["prints the next steps"]),
];

#[test]
fn each_contract_that_left_agents_md_is_on_the_page_of_the_command_it_governs() {
    let fx = NoCorpus::new("req-moved-contracts");
    for (command, phrases) in MOVED_CONTRACTS {
        let page = squashed(&help_page(fx.path(), command));
        for phrase in *phrases {
            assert!(
                page.contains(phrase),
                "`req {command} --help` must carry {phrase:?} — it is that rule's only home:\n{page}"
            );
        }
    }
}

#[test]
fn the_moved_contract_table_names_only_commands_that_have_a_page() {
    // A typo in the table would turn a pin into a vacuous loop over nothing: every name must be
    // one the tool advertises.
    for (command, phrases) in MOVED_CONTRACTS {
        assert!(
            ALL_COMMANDS.contains(command),
            "MOVED_CONTRACTS names {command:?}, which is not a command"
        );
        assert!(!phrases.is_empty(), "{command} pins nothing");
    }
}
