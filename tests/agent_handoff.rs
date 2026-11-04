//! The pointers an adopter's coding agent follows to find the right `req`, kept honest.
//!
//! `examples/agents-snippet.md` is pasted into another project's `AGENTS.md`, and
//! `examples/handoff-brief.md` is the template a step of `docs/adopting.md` § *For an agent* is
//! handed over in. Both name a precheck (`req --version` must show this build's `claim_schema` and
//! `rules_v`) and the commands to run. A pointer to a binary that has moved on is the failure these
//! tests exist to catch: the check reads the *built* `req`, so the text cannot drift behind it, and
//! every `req <command>` the two files name is run with `--help` so a renamed or removed command
//! fails here, not in someone else's repository.

use std::path::{Path, PathBuf};
use std::process::Command;

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

/// The value on a `req --version` line such as `  claim_schema       req.claim/2`.
fn version_field(version: &str, key: &str) -> String {
    version
        .lines()
        .find_map(|line| line.trim().strip_prefix(key))
        .map(|rest| rest.trim().to_string())
        .unwrap_or_else(|| panic!("`req --version` has no `{key}` line:\n{version}"))
}

fn run(args: &[&str]) -> (i32, String) {
    let output = Command::new(req_bin())
        .args(args)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// The `req <command>` words a text opens a code span with (`req --version` and `req <command>`
/// name no command and are skipped).
fn named_commands(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices("`req ") {
        let word: String = text[at + 5..]
            .chars()
            .take_while(char::is_ascii_lowercase)
            .collect();
        if !word.is_empty() && !found.contains(&word) {
            found.push(word);
        }
    }
    found
}

#[test]
fn the_snippet_tells_an_agent_how_to_know_it_has_the_right_req() {
    let (code, version) = run(&["--version"]);
    assert_eq!(code, 0, "{version}");
    let snippet = read("examples/agents-snippet.md");
    for needle in [
        "req --version".to_string(),
        version_field(&version, "claim_schema"),
        format!("rules_v {}", version_field(&version, "rules_v")),
        "req --help".to_string(),
        "req <command> --help".to_string(),
        "docs/adopting.md".to_string(),
    ] {
        assert!(
            snippet.contains(&needle),
            "examples/agents-snippet.md must name `{needle}` so an agent can find and trust its req"
        );
    }
}

#[test]
fn the_handoff_brief_names_one_step_its_precheck_and_its_report() {
    let brief = read("examples/handoff-brief.md");
    for field in [
        "STEP ",
        "REQ PIN",
        "INPUT",
        "DONE WHEN",
        "MAY NOT",
        "REPORT",
        "req --version",
        "examples/agents-snippet.md",
        "docs/adopting.md",
    ] {
        assert!(
            brief.contains(field),
            "examples/handoff-brief.md must contain `{field}`"
        );
    }
    let (_, version) = run(&["--version"]);
    assert!(
        brief.contains(&version_field(&version, "claim_schema")),
        "the brief's precheck must name this build's claim_schema"
    );
}

#[test]
fn every_req_command_the_two_files_name_exists() {
    let mut commands = named_commands(&read("examples/agents-snippet.md"));
    commands.extend(named_commands(&read("examples/handoff-brief.md")));
    assert!(
        commands.len() >= 4,
        "the files name too few commands to be a pointer: {commands:?}"
    );
    for command in commands {
        let (code, _) = run(&[&command, "--help"]);
        assert_eq!(
            code, 0,
            "`req {command} --help` must exit 0: the command is named in an adopter-facing file"
        );
    }
}
