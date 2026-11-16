//! `scripts/release-version-check.sh` — the version rules of ADR 0055, run as a process (SV-1).
//!
//! The release workflow's build job calls this script to decide, before anything is built or uploaded, whether a
//! release may carry the version its rev declares. Its facts come from `scripts/release-notes.sh --facts` (tested
//! in `tests/release_notes.rs`); this file feeds it facts by hand, so every row below is an independent oracle
//! typed from the ADR, never read back from the script.
//!
//! Also here, because they belong to the same decision:
//! - the **`Cargo.lock` exception** (decision 6): a version bump rewrites req's own package entry and no other, and
//!   a lock diff that touches any other entry fails. Tested on fixtures (`tests/fixtures/cargo-lock/`), never on git
//!   history;
//! - the **refusal of today's `main`** (decision 4 and the unit's "does not bump" rule): `Cargo.toml` says 0.1.0 and
//!   the previous release `rev-3294a46` counts as 0.1.0, so a dispatch now must fail closed;
//! - the documents that quoted `0.1.0` as a literal no longer do (decision 6), and ADR 0055 is accepted.
//!
//! Exit contract of the script: 0 accepted (or the answer printed); 1 refused by a rule, the reason on stderr; 2
//! usage, or an input it cannot judge (a malformed version or facts file).
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn script() -> PathBuf {
    root().join("scripts/release-version-check.sh")
}

/// A scratch directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "req-release-version-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).expect("writes");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str], cwd: &Path) -> Output {
    Command::new("bash")
        .arg(script())
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("runs the script")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The facts file `release-notes.sh --facts` writes, typed by hand.
fn facts(previous_tag: &str, previous_version: &str, identity: &str) -> String {
    format!(
        "previous_tag={previous_tag}\nprevious_version={previous_version}\nidentity={identity}\n"
    )
}

// ---------------------------------------------------------------------------
// The verdict: four refusals, the accepts, and the rev-* rule
// ---------------------------------------------------------------------------

/// One row: (previous tag, previous version, identity, the rev's version, accepted?, a phrase of the reason).
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
    &'static str,
);

#[test]
fn the_version_check_table() {
    let rows: &[Row] = &[
        // 1. The version must be greater than the previous release's. A `rev-*` release counts as 0.1.0.
        (
            "rev-3294a46",
            "0.1.0",
            "unmoved",
            "0.1.0",
            false,
            "not greater",
        ),
        (
            "rev-3294a46",
            "0.1.0",
            "moved",
            "0.1.0",
            false,
            "not greater",
        ),
        (
            "rev-3294a46",
            "0.1.0",
            "unknown",
            "0.1.0",
            false,
            "not greater",
        ),
        (
            "rev-3294a46",
            "0.1.0",
            "unmoved",
            "0.0.9",
            false,
            "not greater",
        ),
        ("v0.2.0", "0.2.0", "unmoved", "0.2.0", false, "not greater"),
        ("v0.3.0", "0.3.0", "unmoved", "0.2.9", false, "not greater"),
        // accepts: a patch with identity unmoved; a minor with identity moved; a minor with identity unknown
        ("rev-3294a46", "0.1.0", "unmoved", "0.1.1", true, ""),
        ("rev-3294a46", "0.1.0", "moved", "0.2.0", true, ""),
        ("rev-3294a46", "0.1.0", "unknown", "0.2.0", true, ""),
        // a minor with identity unmoved is the owner's declaration, not something history can refuse
        ("rev-3294a46", "0.1.0", "unmoved", "0.2.0", true, ""),
        ("v0.2.0", "0.2.0", "moved", "0.3.0", true, ""),
        ("v0.2.7", "0.2.7", "unmoved", "0.2.8", true, ""),
        ("v0.2.7", "0.2.7", "unmoved", "0.3.0", true, ""),
        // 2. Identity moved, and the number that rose is not the one required (below 1.0, the minor).
        (
            "rev-3294a46",
            "0.1.0",
            "moved",
            "0.1.1",
            false,
            "identity moved",
        ),
        ("v0.2.0", "0.2.0", "moved", "0.2.1", false, "identity moved"),
        // 3. Identity unknown, and only the patch number rose: an unknown is never an unmoved.
        (
            "rev-3294a46",
            "0.1.0",
            "unknown",
            "0.1.1",
            false,
            "could not be compared",
        ),
        (
            "v0.4.2",
            "0.4.2",
            "unknown",
            "0.4.3",
            false,
            "could not be compared",
        ),
        // 4. A major bump below 1.0 is the owner's decision, in an ADR, never the workflow's.
        (
            "rev-3294a46",
            "0.1.0",
            "unmoved",
            "1.0.0",
            false,
            "major bump below 1.0",
        ),
        (
            "rev-3294a46",
            "0.1.0",
            "moved",
            "1.0.0",
            false,
            "major bump below 1.0",
        ),
        (
            "v0.9.4",
            "0.9.4",
            "unknown",
            "1.0.0",
            false,
            "major bump below 1.0",
        ),
        (
            "v0.9.4",
            "0.9.4",
            "unmoved",
            "2.0.0",
            false,
            "major bump below 1.0",
        ),
        // From 1.0 on an identity move is a major bump.
        ("v1.2.3", "1.2.3", "moved", "1.3.0", false, "identity moved"),
        ("v1.2.3", "1.2.3", "moved", "1.2.4", false, "identity moved"),
        ("v1.2.3", "1.2.3", "moved", "2.0.0", true, ""),
        (
            "v1.2.3",
            "1.2.3",
            "unknown",
            "1.2.4",
            false,
            "could not be compared",
        ),
        ("v1.2.3", "1.2.3", "unknown", "1.3.0", true, ""),
        ("v1.2.3", "1.2.3", "unmoved", "1.2.4", true, ""),
        ("v1.2.3", "1.2.3", "unmoved", "2.0.0", true, ""),
        // Numbers compare as numbers, not as text.
        ("v0.9.0", "0.9.0", "unmoved", "0.10.0", true, ""),
        ("v0.2.9", "0.2.9", "unmoved", "0.2.10", true, ""),
        (
            "v0.10.0",
            "0.10.0",
            "unmoved",
            "0.9.0",
            false,
            "not greater",
        ),
    ];
    for (prev_tag, prev_version, identity, version, accepted, reason) in rows {
        let scratch = Scratch::new();
        let facts_file = scratch.write("facts", &facts(prev_tag, prev_version, identity));
        let out = run(
            &[
                "check",
                "--version",
                version,
                "--facts",
                facts_file.to_str().expect("utf-8"),
            ],
            &scratch.0,
        );
        let row = format!("{prev_tag} ({prev_version}), identity {identity}, version {version}");
        if *accepted {
            assert_eq!(code(&out), 0, "{row} must be accepted: {}", stderr(&out));
            assert!(
                stdout(&out)
                    .lines()
                    .any(|l| l == format!("version={version}")),
                "{row}: an accepted check prints `version={version}` for the workflow's output: {}",
                stdout(&out)
            );
        } else {
            assert_eq!(code(&out), 1, "{row} must be refused: {}", stderr(&out));
            assert!(
                stderr(&out).contains(reason),
                "{row}: the refusal names its reason `{reason}`: {}",
                stderr(&out)
            );
            assert!(
                stdout(&out).is_empty(),
                "{row}: a refusal prints nothing on stdout (the workflow reads stdout as the version): {}",
                stdout(&out)
            );
        }
    }
}

#[test]
fn a_rev_release_counts_as_0_1_0_and_the_facts_cannot_say_otherwise() {
    let scratch = Scratch::new();
    // A rev-* previous release with a version other than 0.1.0 is a facts file that contradicts the rule: refused.
    let bad = scratch.write("bad", &facts("rev-3294a46", "0.7.0", "unmoved"));
    let out = run(
        &[
            "check",
            "--version",
            "0.8.0",
            "--facts",
            bad.to_str().expect("utf-8"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 2, "{}", stderr(&out));
    assert!(stderr(&out).contains("0.1.0"), "{}", stderr(&out));
}

#[test]
fn a_previous_version_that_could_not_be_read_fails_closed() {
    let scratch = Scratch::new();
    let f = scratch.write("facts", &facts("v0.2.0", "", "unmoved"));
    let out = run(
        &[
            "check",
            "--version",
            "0.3.0",
            "--facts",
            f.to_str().expect("utf-8"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 1, "{}", stderr(&out));
    assert!(
        stderr(&out).contains("previous version"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_first_release_has_nothing_to_be_greater_than() {
    let scratch = Scratch::new();
    let f = scratch.write("facts", &facts("", "", "unknown"));
    let out = run(
        &[
            "check",
            "--version",
            "0.1.0",
            "--facts",
            f.to_str().expect("utf-8"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert!(stdout(&out).contains("version=0.1.0"), "{}", stdout(&out));
}

#[test]
fn a_version_that_is_not_plain_semver_is_unusable() {
    let scratch = Scratch::new();
    let f = scratch.write("facts", &facts("rev-3294a46", "0.1.0", "unmoved"));
    for bad in [
        "",
        "0.2",
        "0.2.0.1",
        "v0.2.0",
        "0.2.0-rc1",
        "0.2.0+build",
        "01.2.3",
        "0.02.0",
        "-1.0.0",
        "0.2.x",
        " 0.2.0",
        "0.2.0 ",
        "0.2.0\n",
        "99999999999.0.0",
        "0.2.0;echo",
    ] {
        let out = run(
            &[
                "check",
                "--version",
                bad,
                "--facts",
                f.to_str().expect("utf-8"),
            ],
            &scratch.0,
        );
        assert_eq!(code(&out), 2, "{bad:?} must be unusable: {}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{bad:?}: {}", stdout(&out));
    }
}

#[test]
fn a_facts_file_that_cannot_be_judged_is_unusable() {
    let scratch = Scratch::new();
    for (label, text) in [
        (
            "no identity",
            "previous_tag=rev-3294a46\nprevious_version=0.1.0\n",
        ),
        (
            "no previous_tag",
            "previous_version=0.1.0\nidentity=unmoved\n",
        ),
        (
            "no previous_version",
            "previous_tag=rev-3294a46\nidentity=unmoved\n",
        ),
        ("a fourth identity", &facts("rev-3294a46", "0.1.0", "maybe")),
        (
            "an upper-case identity",
            &facts("rev-3294a46", "0.1.0", "MOVED"),
        ),
        (
            "a repeated key",
            "previous_tag=rev-3294a46\nprevious_version=0.1.0\nidentity=moved\nidentity=unmoved\n",
        ),
        (
            "an unknown key",
            "previous_tag=rev-3294a46\nprevious_version=0.1.0\nidentity=moved\nbump=patch\n",
        ),
        (
            "a line with no `=`",
            "previous_tag=rev-3294a46\nprevious_version=0.1.0\nidentity\n",
        ),
        ("an empty file", ""),
        (
            "a tag that is neither a rev tag nor a version tag",
            &facts("nightly", "0.1.0", "unmoved"),
        ),
        (
            "a v tag with a malformed version",
            &facts("v0.2.0", "0.2", "unmoved"),
        ),
        (
            "a v tag that disagrees with its own Cargo.toml",
            &facts("v0.2.0", "0.3.0", "unmoved"),
        ),
        (
            "a previous version without a previous release",
            &facts("", "0.1.0", "unknown"),
        ),
    ] {
        let f = scratch.write("facts", text);
        let out = run(
            &[
                "check",
                "--version",
                "0.9.0",
                "--facts",
                f.to_str().expect("utf-8"),
            ],
            &scratch.0,
        );
        assert_eq!(code(&out), 2, "{label}: {}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{label}: {}", stdout(&out));
    }
    // A facts file that does not exist is the same: nothing to judge by.
    let out = run(
        &["check", "--version", "0.9.0", "--facts", "no-such-file"],
        &scratch.0,
    );
    assert_eq!(code(&out), 2, "{}", stderr(&out));
}

#[test]
fn usage_errors_are_exit_2() {
    let scratch = Scratch::new();
    let f = scratch.write("facts", &facts("rev-3294a46", "0.1.0", "unmoved"));
    let f = f.to_str().expect("utf-8");
    for (label, args) in [
        ("no subcommand", vec![]),
        ("an unknown subcommand", vec!["bump"]),
        ("check with no version source", vec!["check", "--facts", f]),
        (
            "check with both version sources",
            vec!["check", "--version", "0.2.0", "--rev", "HEAD", "--facts", f],
        ),
        ("check with no facts", vec!["check", "--version", "0.2.0"]),
        (
            "check with an unknown flag",
            vec!["check", "--version", "0.2.0", "--facts", f, "--force"],
        ),
        ("classify with one argument", vec!["classify", "0.1.0"]),
        ("read with no rev", vec!["read"]),
        ("lock-exception with one file", vec!["lock-exception", "x"]),
    ] {
        let out = run(&args, &scratch.0);
        assert_eq!(code(&out), 2, "{label}: {}", stderr(&out));
    }
}

#[test]
fn todays_main_is_refused() {
    // Cargo.toml says 0.1.0 and the previous release is rev-3294a46, which counts as 0.1.0: whatever identity says,
    // a dispatch at today's main fails closed, because 0.1.0 is not greater than 0.1.0. The bump is the next pull request.
    let scratch = Scratch::new();
    for identity in ["unmoved", "moved", "unknown"] {
        let f = scratch.write("facts", &facts("rev-3294a46", "0.1.0", identity));
        let out = run(
            &[
                "check",
                "--version",
                "0.1.0",
                "--facts",
                f.to_str().expect("utf-8"),
            ],
            &scratch.0,
        );
        assert_eq!(code(&out), 1, "identity {identity}: {}", stderr(&out));
        assert!(stderr(&out).contains("not greater"), "{}", stderr(&out));
    }
}

#[test]
fn the_repositorys_own_cargo_toml_has_a_version_the_check_can_read() {
    // Valid before and after the bump pull request: the value is not pinned here, only that it reads as plain semver.
    let out = run(&["read", "HEAD"], &root());
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let version = stdout(&out).trim().to_owned();
    let parts: Vec<&str> = version.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "`read HEAD` prints MAJOR.MINOR.PATCH, got {version:?}"
    );
}

// ---------------------------------------------------------------------------
// Reading the version from a rev, never checking it out
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .expect("runs git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_owned()
}

/// A repository whose Cargo.toml lists a dependency table, with its own `version`, before `[package]`, and again after.
fn repo_with_cargo_toml(cargo_toml: Option<&str>) -> (Scratch, String) {
    let scratch = Scratch::new();
    git(&scratch.0, &["init", "-q", "-b", "main"]);
    match cargo_toml {
        Some(text) => {
            scratch.write("Cargo.toml", text);
        }
        None => {
            scratch.write("README.md", "x\n");
        }
    }
    git(&scratch.0, &["add", "-A"]);
    git(&scratch.0, &["commit", "-q", "-m", "c"]);
    let rev = git(&scratch.0, &["rev-parse", "HEAD"]);
    (scratch, rev)
}

#[test]
fn read_takes_the_version_of_the_package_table_from_the_rev_alone() {
    let (scratch, rev) = repo_with_cargo_toml(Some(
        "[dependencies.foo]\nversion = \"9.9.9\"\n\n[package]\nname = \"req\"\nedition = \"2024\"\nversion = \"0.2.0\"\n\n[dependencies.bar]\nversion = \"7.7.7\"\n",
    ));
    // The working tree changes after the commit: only the rev's own file counts.
    scratch.write(
        "Cargo.toml",
        "[package]\nname = \"req\"\nversion = \"5.5.5\"\n",
    );
    let out = run(&["read", &rev], &scratch.0);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stdout(&out), "0.2.0\n");
}

#[test]
fn read_refuses_a_rev_it_cannot_take_a_plain_version_from() {
    for (label, cargo) in [
        ("no Cargo.toml", None),
        ("no version", Some("[package]\nname = \"req\"\n")),
        (
            "a version outside [package]",
            Some("[dependencies.foo]\nversion = \"1.2.3\"\n"),
        ),
        (
            "a pre-release version",
            Some("[package]\nversion = \"0.2.0-rc1\"\n"),
        ),
        (
            "a version that is not a string",
            Some("[package]\nversion.workspace = true\n"),
        ),
    ] {
        let (scratch, rev) = repo_with_cargo_toml(cargo);
        let out = run(&["read", &rev], &scratch.0);
        assert_eq!(code(&out), 2, "{label}: {}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{label}: {}", stdout(&out));
    }
    let (scratch, _) = repo_with_cargo_toml(Some("[package]\nversion = \"0.2.0\"\n"));
    let out = run(
        &["read", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"],
        &scratch.0,
    );
    assert_eq!(code(&out), 2, "an unresolvable rev: {}", stderr(&out));
    let out = run(&["read", "-x"], &scratch.0);
    assert_eq!(code(&out), 2, "a rev that is an option: {}", stderr(&out));
}

#[test]
fn check_with_a_rev_reads_the_version_from_that_rev() {
    let (scratch, rev) =
        repo_with_cargo_toml(Some("[package]\nname = \"req\"\nversion = \"0.2.0\"\n"));
    let f = scratch.write("facts", &facts("rev-3294a46", "0.1.0", "moved"));
    let out = run(
        &[
            "check",
            "--rev",
            &rev,
            "--facts",
            f.to_str().expect("utf-8"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert!(
        stdout(&out).lines().any(|l| l == "version=0.2.0"),
        "{}",
        stdout(&out)
    );
    // The same rev against a previous release of 0.2.0 is not greater.
    let f = scratch.write("facts2", &facts("v0.2.0", "0.2.0", "unmoved"));
    let out = run(
        &[
            "check",
            "--rev",
            &rev,
            "--facts",
            f.to_str().expect("utf-8"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 1, "{}", stderr(&out));
    // A rev with no readable version is unusable, not accepted.
    let (scratch, rev) = repo_with_cargo_toml(None);
    let f = scratch.write("facts", &facts("rev-3294a46", "0.1.0", "moved"));
    let out = run(
        &[
            "check",
            "--rev",
            &rev,
            "--facts",
            f.to_str().expect("utf-8"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 2, "{}", stderr(&out));
}

#[test]
fn classify_names_the_class_of_a_bump() {
    let scratch = Scratch::new();
    for (previous, new, class) in [
        ("0.1.0", "0.1.1", "patch"),
        ("0.1.0", "0.2.0", "minor"),
        ("0.1.5", "0.2.0", "minor"),
        ("0.9.0", "1.0.0", "major"),
        ("1.2.3", "2.0.0", "major"),
        ("1.2.3", "1.3.0", "minor"),
        ("0.1.0", "0.1.0", "none"),
        ("0.2.0", "0.1.9", "lower"),
        ("0.9.0", "0.10.0", "minor"),
        ("0.1.0", "unreadable", "not comparable"),
        ("", "0.1.0", "not comparable"),
    ] {
        let out = run(&["classify", previous, new], &scratch.0);
        assert_eq!(code(&out), 0, "{previous} -> {new}: {}", stderr(&out));
        assert_eq!(stdout(&out), format!("{class}\n"), "{previous} -> {new}");
    }
}

// ---------------------------------------------------------------------------
// The Cargo.lock exception (ADR 0055 decision 6)
// ---------------------------------------------------------------------------

fn lock(name: &str) -> String {
    root()
        .join("tests/fixtures/cargo-lock")
        .join(name)
        .to_str()
        .expect("utf-8")
        .to_owned()
}

#[test]
fn a_lock_diff_that_touches_only_reqs_own_entry_passes() {
    let scratch = Scratch::new();
    for new in [
        "req-bumped.lock",
        "req-and-dependency-list.lock",
        "base.lock",
    ] {
        let out = run(
            &["lock-exception", &lock("base.lock"), &lock(new)],
            &scratch.0,
        );
        assert_eq!(code(&out), 0, "{new}: {}", stderr(&out));
    }
}

#[test]
fn a_lock_diff_that_touches_any_other_entry_fails() {
    let scratch = Scratch::new();
    for new in [
        "dependency-bumped.lock",
        "req-and-dependency-bumped.lock",
        "dependency-added.lock",
        "dependency-removed.lock",
        "decoy-bumped.lock",
        "preamble-changed.lock",
    ] {
        let out = run(
            &["lock-exception", &lock("base.lock"), &lock(new)],
            &scratch.0,
        );
        assert_eq!(code(&out), 1, "{new} must fail: {}", stderr(&out));
        assert!(
            stderr(&out).contains("other than req's own"),
            "{new}: the refusal says what differed: {}",
            stderr(&out)
        );
    }
    // And in the other direction: the exception is symmetric, it is about what differs.
    let out = run(
        &[
            "lock-exception",
            &lock("dependency-bumped.lock"),
            &lock("base.lock"),
        ],
        &scratch.0,
    );
    assert_eq!(code(&out), 1, "{}", stderr(&out));
}

#[test]
fn a_lock_file_without_exactly_one_own_entry_cannot_be_judged() {
    let scratch = Scratch::new();
    for (old, new) in [
        ("base.lock", "no-own-entry.lock"),
        ("no-own-entry.lock", "base.lock"),
        ("base.lock", "two-own-entries.lock"),
        ("base.lock", "no-such-file.lock"),
    ] {
        let out = run(&["lock-exception", &lock(old), &lock(new)], &scratch.0);
        assert_eq!(code(&out), 2, "{old} vs {new}: {}", stderr(&out));
    }
}

#[test]
fn the_real_lock_file_is_read_by_the_exception_check() {
    // The script must understand the file this repository actually has: one own entry, and a lock is equal to itself.
    let scratch = Scratch::new();
    let real = root().join("Cargo.lock");
    let real = real.to_str().expect("utf-8");
    let out = run(&["lock-exception", real, real], &scratch.0);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    // A copy with req's own version edited passes; a copy with another package's version edited does not.
    let text = fs::read_to_string(real).expect("reads Cargo.lock");
    let own = text
        .find("name = \"req\"\nversion = \"")
        .expect("req's own entry");
    let bumped = text.replacen(
        &text[own..own + text[own..].find("dependencies").expect("its dependencies")],
        "name = \"req\"\nversion = \"9.9.9\"\n",
        1,
    );
    assert_ne!(bumped, text);
    let a = scratch.write("bumped.lock", &bumped);
    let out = run(
        &["lock-exception", real, a.to_str().expect("utf-8")],
        &scratch.0,
    );
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let other = text.replacen(
        "name = \"serde\"\nversion = \"",
        "name = \"serde\"\nversion = \"9.",
        1,
    );
    assert_ne!(other, text);
    let b = scratch.write("other.lock", &other);
    let out = run(
        &["lock-exception", real, b.to_str().expect("utf-8")],
        &scratch.0,
    );
    assert_eq!(code(&out), 1, "{}", stderr(&out));
}

// ---------------------------------------------------------------------------
// The prose no longer carries the version as a literal; ADR 0055 is accepted
// ---------------------------------------------------------------------------

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

#[test]
fn no_prose_document_carries_the_version_as_a_literal() {
    // ADR 0055 decision 6: the bump pull request changes Cargo.toml, Cargo.lock and the SBOM and nothing else, so no
    // document may quote the number it would then have to edit.
    for rel in [
        "README.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "docs/safety-manual.md",
        "build.rs",
    ] {
        let text = read(rel);
        assert!(
            !text.contains("0.1.0"),
            "{rel} still quotes 0.1.0 as a literal"
        );
    }
}

#[test]
fn adr_0025_says_one_version_covers_many_commits() {
    let text = read("docs/adr/0025-prebuilt-binary-per-pinned-rev.md");
    assert!(
        text.contains("one version covers many commits"),
        "ADR 0025's Rejected item no longer says a `0.1.0` is the same on every commit by design"
    );
    assert!(
        !text.contains("is the same on every\n  commit by design"),
        "the old wording is gone"
    );
}

#[test]
fn adr_0055_is_accepted_and_the_amendment_notes_are_no_longer_proposed() {
    let adr = read("docs/adr/0055-a-release-carries-a-semantic-version-beside-its-rev.md");
    assert!(
        adr.contains("**Status: accepted**") && !adr.contains("**Status: proposed.**"),
        "ADR 0055's Status is accepted"
    );
    assert!(
        adr.contains("PR #96") && adr.contains("2026-11-16"),
        "the Status names the merge that ratified it"
    );
    for rel in [
        "docs/adr/0025-prebuilt-binary-per-pinned-rev.md",
        "docs/adr/0029-req-is-published-on-github-under-apache-2.0.md",
        "docs/adr/0031-release-notes-are-made-from-history-and-say-when-identity-moves.md",
    ] {
        let text = read(rel);
        assert!(
            text.contains("ADR 0055"),
            "{rel} still carries the amendment note"
        );
        assert!(
            !text.contains("ADR 0055 (proposed"),
            "{rel}: the amendment note still says (proposed)"
        );
    }
}
