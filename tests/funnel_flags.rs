//! The `propose` flag parser, at the CLI seam.
//!
//! `--pairs` is documented as a **bare** flag — `propose [--pairs]` in `USAGE`, in the module doc and in
//! the command's page — but the parse loop consumed the argument after it (issue #48). The consequence
//! was silent: a selector in first position vanished, the funnel fell back to the default context, and a
//! *cost measurement* reported one fewer applying claim than the corpus selection held. With another flag
//! after it, the same bug turned a valid invocation into a usage error.
//!
//! These are the regression tests: the same selection must count the same with and without the flag, and
//! the flag must work wherever it sits.

use std::process::Command;

fn req(args: &[&str]) -> (i32, String) {
    let bin = std::env::var_os("CARGO_BIN_EXE_req")
        .expect("integration tests run with CARGO_BIN_EXE_req set");
    let output = Command::new(bin)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("runs the binary");
    (
        output.status.code().expect("a real exit code"),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// The funnel header's applying count: `# Tier-2 funnel: 16 applying claim(s), …`.
fn applying(stdout: &str) -> usize {
    let line = stdout
        .lines()
        .find(|line| line.contains("applying claim(s)"))
        .unwrap_or_else(|| panic!("no funnel header in:\n{stdout}"));
    line.split(':')
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("cannot read the applying count from: {line}"))
}

/// **The silent-short case.** `--pairs` in first position used to eat the selector, so the measurement
/// covered the default context instead of the one asked for. The count must not depend on where the flag
/// sits.
#[test]
fn pairs_in_first_position_does_not_swallow_the_selector() {
    let (without_code, without) = req(&["propose", "corpus_set=parity_and_adversarial"]);
    let (with_code, with) = req(&["propose", "--pairs", "corpus_set=parity_and_adversarial"]);
    assert_ne!(without_code, 2, "the baseline invocation is valid");
    assert_ne!(
        with_code, 2,
        "--pairs must not turn a valid invocation into usage"
    );
    assert_eq!(
        applying(&with),
        applying(&without),
        "--pairs changed the measured selection: {} applying with the flag, {} without.\n\
         with:\n{with}\nwithout:\n{without}",
        applying(&with),
        applying(&without)
    );
}

/// **The usage-error case.** `--pairs --gold <file>` used to read `--gold` as the flag's "value", leaving
/// the path as a stray positional and exiting 2.
#[test]
fn pairs_before_another_flag_is_not_a_usage_error() {
    let (code, stdout) = req(&["propose", "--pairs", "--gold", "bench/corpus/tier2-v1.yml"]);
    assert_ne!(
        code, 2,
        "--pairs followed by --gold is a valid invocation:\n{stdout}"
    );
    assert!(
        stdout.contains("Tier-2 funnel"),
        "the funnel ran:\n{stdout}"
    );
}

/// The accidental workaround that worked before the fix — kept so a future change cannot silently break
/// the one arrangement that used to be usable.
#[test]
fn pairs_as_the_last_argument_still_works() {
    let (code, stdout) = req(&["propose", "--gold", "bench/corpus/tier2-v1.yml", "--pairs"]);
    assert_ne!(
        code, 2,
        "--pairs as the last argument is a valid invocation:\n{stdout}"
    );
    assert!(
        stdout.contains("Tier-2 funnel"),
        "the funnel ran:\n{stdout}"
    );
}
