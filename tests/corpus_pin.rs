//! `req pin` (ADR 0033, issue #16): a consumer repository pins the commit of an external corpus it
//! is gated against, and can see how far that corpus has moved.
//!
//! The public seam is the compiled `req`, run against **temporary git repositories this file builds
//! with real commits**: one repository is the external corpus (the "policy"), and the pin file lives
//! apart from it, as it would in the consumer's repository. Every expectation is written by hand from
//! the ADR and the issue; none is read back from `pin`'s own output.
//!
//! Exit contract under test, the project's three states: **0** the corpus is exactly the pinned
//! commit (or a drift report was produced); **1** FAIL, the corpus is not the pinned commit or has
//! uncommitted changes under `claims/` or `aliases/`; **3** UNVERIFIABLE, the pin cannot be read or
//! is not a commit, `git` is unusable, or the corpus is not in a git work tree; **2** usage.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A base directory outside any checkout: a fixture under this repository's own `.git` or `claims/`
/// would test the wrong repository.
fn fixture_base() -> PathBuf {
    for base in [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ] {
        if base.is_dir()
            && !base
                .ancestors()
                .any(|d| req::root::is_corpus_root(d) || d.join(".git").exists())
        {
            return fs::canonicalize(&base).expect("the fixture base resolves");
        }
    }
    panic!("no fixture base outside a checkout");
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = fixture_base().join(format!("req-pin-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the fixture directory");
        Dir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "req pin test")
        .env("GIT_AUTHOR_EMAIL", "pin@example.invalid")
        .env("GIT_COMMITTER_NAME", "req pin test")
        .env("GIT_COMMITTER_EMAIL", "pin@example.invalid")
        .env("GIT_AUTHOR_DATE", "2020-01-02T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2020-01-02T00:00:00Z")
        .output()
        .expect("runs git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

const REGISTRY: &str = "\
schema: req.subject_registry/0
version: 1
subjects:
  SYSTEM:
    label: The system
    aliases:
      - the system
";

fn claim_file(id: &str, statement: &str) -> String {
    let statement = serde_json::to_string(statement).expect("a JSON string");
    format!(
        "schema: req.claim/2\nclaim_id: {id}\nstatement: {statement}\napplicability: []\n\
         rationale:\n  source: tests/corpus_pin.rs\n  date: 2026-11-09\n  rejected_alternatives: []\n  assumptions: []\nrelations: []\n"
    )
}

/// The external corpus: a git repository (optionally with the corpus in a subdirectory).
struct Policy {
    dir: Dir,
    prefix: String,
}

impl Policy {
    fn new() -> Self {
        Self::with_prefix("")
    }

    fn with_prefix(prefix: &str) -> Self {
        let dir = Dir::new("policy");
        git_in(dir.path(), &["init", "-q", "-b", "main"]);
        let policy = Policy {
            dir,
            prefix: prefix.to_string(),
        };
        policy.write("aliases/subjects.yml", REGISTRY);
        policy.claim("REQ-0001", "The system shall respond within 200 ms.");
        policy.commit("v1");
        policy
    }

    fn repo(&self) -> &Path {
        self.dir.path()
    }

    fn root(&self) -> PathBuf {
        if self.prefix.is_empty() {
            self.repo().to_path_buf()
        } else {
            self.repo().join(&self.prefix)
        }
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.root().join(rel);
        fs::create_dir_all(path.parent().expect("a parent")).expect("creates the parent");
        fs::write(path, text).expect("writes the file");
    }

    fn claim(&self, id: &str, statement: &str) {
        self.write(&format!("claims/{id}.yml"), &claim_file(id, statement));
    }

    fn commit(&self, tag: &str) -> String {
        git_in(self.repo(), &["add", "-A"]);
        git_in(self.repo(), &["commit", "-q", "--allow-empty", "-m", tag]);
        git_in(self.repo(), &["tag", tag]);
        self.sha("HEAD")
    }

    fn sha(&self, rev: &str) -> String {
        git_in(self.repo(), &["rev-parse", rev])
    }
}

/// A pin file in a directory of the consumer's, outside the policy repository.
struct Consumer {
    dir: Dir,
}

impl Consumer {
    fn new() -> Self {
        Consumer {
            dir: Dir::new("consumer"),
        }
    }

    fn pin(&self, text: &str) -> PathBuf {
        let path = self.dir.path().join("corpus.pin");
        fs::write(&path, text).expect("writes the pin");
        path
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn req(cwd: &Path, root: Option<&Path>, args: &[&str]) -> Run {
    let mut command = Command::new(req_bin());
    if let Some(root) = root {
        command.arg("--root").arg(root);
    }
    let output = command
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn verify(consumer: &Consumer, policy: &Policy, pin: &Path) -> Run {
    req(
        consumer.dir.path(),
        Some(&policy.root()),
        &["pin", "--verify", pin.to_str().expect("utf-8 path")],
    )
}

const REPO_URL: &str = "https://example.invalid/policy";

fn pin_text(rev: &str) -> String {
    format!("# the policy this repository is gated against\nrepo: {REPO_URL}\nrev: {rev}\n")
}

// ---------------------------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------------------------

#[test]
fn a_corpus_at_the_pinned_commit_verifies_and_the_output_names_the_pin() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout
            .contains(&format!("corpus pinned: {REPO_URL} @ {}", policy.sha("v1"))),
        "the pin is the first thing a gate log shows: {}",
        run.stdout
    );
}

#[test]
fn a_pin_without_a_repo_line_still_verifies() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&format!("rev: {}\n", policy.sha("v1")));
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout
            .contains(&format!("corpus pinned: @ {}", policy.sha("v1"))),
        "{}",
        run.stdout
    );
}

#[test]
fn a_corpus_ahead_of_the_pin_fails_and_says_how_far() {
    let policy = Policy::new();
    let pinned = policy.sha("v1");
    policy.claim("REQ-0002", "The system shall log each command.");
    let head = policy.commit("v2");
    let consumer = Consumer::new();
    let run = verify(&consumer, &policy, &consumer.pin(&pin_text(&pinned)));
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("[FAIL]"), "{}", run.stdout);
    assert!(
        run.stdout.contains(&pinned[..12]) && run.stdout.contains(&head[..12]),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("1 commit(s) ahead"), "{}", run.stdout);
}

#[test]
fn a_corpus_behind_the_pin_fails_and_says_so() {
    let policy = Policy::new();
    policy.claim("REQ-0002", "The system shall log each command.");
    let newer = policy.commit("v2");
    git_in(policy.repo(), &["checkout", "-q", "v1"]);
    let consumer = Consumer::new();
    let run = verify(&consumer, &policy, &consumer.pin(&pin_text(&newer)));
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("1 commit(s) behind"), "{}", run.stdout);
}

#[test]
fn an_uncommitted_change_under_claims_fails_even_at_the_pinned_commit() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    policy.claim("REQ-0001", "The system shall respond within 100 ms.");
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("uncommitted"), "{}", run.stdout);
    assert!(
        run.stdout.contains("claims/REQ-0001.yml"),
        "the file is named: {}",
        run.stdout
    );
}

#[test]
fn an_untracked_claim_and_an_edited_alias_registry_fail_too() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    policy.claim("REQ-0009", "The system shall log each command.");
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(
        run.code, 1,
        "an untracked claim is a different corpus: {}",
        run.stdout
    );
    fs::remove_file(policy.root().join("claims/REQ-0009.yml")).expect("removes");
    policy.write(
        "aliases/subjects.yml",
        &format!("{REGISTRY}  OTHER:\n    label: Other\n    aliases:\n      - other\n"),
    );
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(
        run.code, 1,
        "a changed registry can move many addresses: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("aliases/subjects.yml"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_change_outside_claims_and_aliases_does_not_fail_the_pin() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    policy.write("notes.txt", "scratch\n");
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(
        run.code, 0,
        "only the files the gate reads are the pin's business: {}{}",
        run.stdout, run.stderr
    );
}

#[test]
fn a_pinned_commit_that_is_not_in_the_checkout_is_unverifiable_not_a_failure() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let run = verify(
        &consumer,
        &policy,
        &consumer.pin(&pin_text(&"ab".repeat(20))),
    );
    assert_eq!(run.code, 3, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stderr.contains("[UNVERIFIABLE]") && run.stderr.contains("not in this checkout"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_pin_that_names_a_tag_object_or_a_tree_is_not_a_commit() {
    let policy = Policy::new();
    git_in(policy.repo(), &["tag", "-a", "-m", "annotated", "ann"]);
    let tag_object = git_in(policy.repo(), &["rev-parse", "ann"]);
    let tree = git_in(policy.repo(), &["rev-parse", "HEAD^{tree}"]);
    let consumer = Consumer::new();
    for (label, oid) in [("tag object", tag_object), ("tree", tree)] {
        let run = verify(&consumer, &policy, &consumer.pin(&pin_text(&oid)));
        assert_eq!(run.code, 3, "{label}: {}{}", run.stdout, run.stderr);
        assert!(
            run.stderr.contains("not a commit"),
            "{label}: {}",
            run.stderr
        );
    }
}

#[test]
fn a_pin_that_cannot_be_read_is_unverifiable() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let v1 = policy.sha("v1");
    let cases: Vec<(&str, String, &str)> = vec![
        ("empty", String::new(), "no `rev:` line"),
        ("only comments", "# nothing\n".to_owned(), "no `rev:` line"),
        ("a short sha", format!("rev: {}\n", &v1[..12]), "40-digit"),
        (
            "upper-case hex",
            format!("rev: {}\n", v1.to_uppercase()),
            "40-digit",
        ),
        ("a branch name", "rev: main\n".to_owned(), "40-digit"),
        ("no rev", format!("repo: {REPO_URL}\n"), "no `rev:` line"),
        ("two revs", format!("rev: {v1}\nrev: {v1}\n"), "given twice"),
        (
            "an unknown key",
            format!("rev: {v1}\nbranch: main\n"),
            "unknown key",
        ),
        ("a line with no colon", format!("rev {v1}\n"), "expected"),
        (
            "two repos",
            format!("repo: a\nrepo: b\nrev: {v1}\n"),
            "given twice",
        ),
        ("an empty value", format!("repo:\nrev: {v1}\n"), "no value"),
    ];
    for (label, text, needle) in cases {
        let run = verify(&consumer, &policy, &consumer.pin(&text));
        assert_eq!(run.code, 3, "{label}: {}{}", run.stdout, run.stderr);
        assert!(
            run.stderr.contains("[UNVERIFIABLE]"),
            "{label}: {}",
            run.stderr
        );
        assert!(
            run.stderr.contains(needle),
            "{label}: expected `{needle}` in {}",
            run.stderr
        );
        assert!(
            run.stdout.is_empty(),
            "{label}: nothing on stdout when nothing is verified: {}",
            run.stdout
        );
    }
    let run = req(
        consumer.dir.path(),
        Some(&policy.root()),
        &["pin", "--verify", "does-not-exist.pin"],
    );
    assert_eq!(run.code, 3, "a missing file: {}{}", run.stdout, run.stderr);
}

#[test]
fn a_corpus_that_is_not_in_a_git_work_tree_is_unverifiable() {
    let dir = Dir::new("plain");
    fs::create_dir_all(dir.path().join("claims")).expect("creates");
    fs::write(
        dir.path().join("claims/REQ-0001.yml"),
        claim_file("REQ-0001", "The system shall respond within 200 ms."),
    )
    .expect("writes");
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&"ab".repeat(20)));
    let run = req(
        consumer.dir.path(),
        Some(dir.path()),
        &["pin", "--verify", pin.to_str().expect("utf-8")],
    );
    assert_eq!(run.code, 3, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stderr.contains("not inside a git work tree"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_corpus_in_a_subdirectory_of_its_repository_verifies() {
    let policy = Policy::with_prefix("policy");
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    assert_eq!(verify(&consumer, &policy, &pin).code, 0);
    policy.claim("REQ-0001", "The system shall respond within 100 ms.");
    let run = verify(&consumer, &policy, &pin);
    assert_eq!(
        run.code, 1,
        "a dirty claim under the sub-root is seen: {}",
        run.stdout
    );
}

#[test]
fn a_caller_exported_git_dir_cannot_redirect_which_repository_is_read() {
    let policy = Policy::new();
    let other = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    let mut command = Command::new(req_bin());
    let output = command
        .arg("--root")
        .arg(policy.root())
        .args(["pin", "--verify"])
        .arg(&pin)
        .current_dir(consumer.dir.path())
        .env("GIT_DIR", other.repo().join(".git"))
        .output()
        .expect("runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------------------------
// drift
// ---------------------------------------------------------------------------------------------

fn drift(consumer: &Consumer, policy: &Policy, pin: &Path, against: &str) -> Run {
    req(
        consumer.dir.path(),
        Some(&policy.root()),
        &[
            "pin",
            "--drift",
            pin.to_str().expect("utf-8"),
            "--against",
            against,
        ],
    )
}

#[test]
fn drift_names_each_changed_claim_with_its_direction() {
    let policy = Policy::new();
    let pinned = policy.sha("v1");
    policy.claim("REQ-0001", "The system shall respond within 100 ms.");
    policy.claim("REQ-0002", "The system shall log each command.");
    policy.commit("v2");
    let consumer = Consumer::new();
    let run = drift(&consumer, &policy, &consumer.pin(&pin_text(&pinned)), "v2");
    assert_eq!(
        run.code, 0,
        "a drift report is a description, never a verdict: {}{}",
        run.stdout, run.stderr
    );
    assert!(
        run.stdout.contains("pin drift:") && run.stdout.contains(&pinned[..12]),
        "{}",
        run.stdout
    );
    let line_of = |id: &str| {
        run.stdout
            .lines()
            .find(|l| l.contains(id))
            .unwrap_or_else(|| panic!("no line for {id}: {}", run.stdout))
            .to_owned()
    };
    assert!(
        line_of("REQ-0001").contains("tightened"),
        "200 ms to 100 ms is a tightening: {}",
        line_of("REQ-0001")
    );
    assert!(
        line_of("REQ-0002").contains("added"),
        "{}",
        line_of("REQ-0002")
    );
}

#[test]
fn drift_does_not_need_the_checkout_to_be_at_the_pin() {
    let policy = Policy::new();
    let pinned = policy.sha("v1");
    policy.claim("REQ-0002", "The system shall log each command.");
    policy.commit("v2");
    // The checkout is at v2, not at the pin: drift only reads revisions.
    let consumer = Consumer::new();
    let run = drift(
        &consumer,
        &policy,
        &consumer.pin(&pin_text(&pinned)),
        "HEAD",
    );
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
}

#[test]
fn no_drift_is_reported_as_no_listed_claim() {
    let policy = Policy::new();
    let pinned = policy.sha("v1");
    let consumer = Consumer::new();
    let run = drift(&consumer, &policy, &consumer.pin(&pin_text(&pinned)), "v1");
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(
        !run.stdout.contains("REQ-0001"),
        "an identical claim is counted, not listed: {}",
        run.stdout
    );
}

#[test]
fn drift_from_a_pinned_commit_that_is_not_in_the_checkout_is_unverifiable() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let run = drift(
        &consumer,
        &policy,
        &consumer.pin(&pin_text(&"cd".repeat(20))),
        "v1",
    );
    assert_eq!(run.code, 3, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stderr.contains("not in this checkout"),
        "{}",
        run.stderr
    );
    assert!(run.stdout.is_empty(), "{}", run.stdout);
}

#[test]
fn drift_from_a_pin_that_is_not_a_commit_is_unverifiable() {
    let policy = Policy::new();
    let tree = git_in(policy.repo(), &["rev-parse", "HEAD^{tree}"]);
    let consumer = Consumer::new();
    let run = drift(&consumer, &policy, &consumer.pin(&pin_text(&tree)), "v1");
    assert_eq!(run.code, 3, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("not a commit"), "{}", run.stderr);
}

#[test]
fn drift_against_an_unresolvable_revision_is_unverifiable() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let run = drift(
        &consumer,
        &policy,
        &consumer.pin(&pin_text(&policy.sha("v1"))),
        "no-such-branch",
    );
    assert_eq!(run.code, 3, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout.is_empty(),
        "a report that fails leaves stdout empty: {}",
        run.stdout
    );
}

// ---------------------------------------------------------------------------------------------
// usage and the help page
// ---------------------------------------------------------------------------------------------

#[test]
fn every_other_spelling_is_a_usage_error() {
    let policy = Policy::new();
    let consumer = Consumer::new();
    let pin = consumer.pin(&pin_text(&policy.sha("v1")));
    let p = pin.to_str().expect("utf-8");
    let cases: Vec<Vec<&str>> = vec![
        vec!["pin"],
        vec!["pin", "--verify"],
        vec!["pin", "--drift", p],
        vec!["pin", "--verify", p, "--against", "main"],
        vec!["pin", "--drift", p, "--against"],
        vec!["pin", "--drift", p, "--against", "-x"],
        vec!["pin", "--verify", p, "extra"],
        vec!["pin", "--frobnicate", p],
        vec!["pin", "--verify", "--drift", p],
    ];
    for args in cases {
        let run = req(consumer.dir.path(), Some(&policy.root()), &args);
        assert_eq!(run.code, 2, "{args:?}: {}{}", run.stdout, run.stderr);
        assert!(
            run.stderr.contains("usage: req pin"),
            "{args:?}: {}",
            run.stderr
        );
    }
}

#[test]
fn the_help_page_states_the_contract_and_works_anywhere() {
    let nowhere = Dir::new("nowhere");
    let run = req(nowhere.path(), None, &["pin", "--help"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    for needle in ["--verify", "--drift", "--against", "exit:", "never fetches"] {
        assert!(
            run.stdout.contains(needle),
            "help lacks `{needle}`: {}",
            run.stdout
        );
    }
}
