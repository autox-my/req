//! `tools/adopt.py` — the draft path: a document in, **legal** claims out.
//!
//! The seam is the CLI, not the extractor's functions: what the tool promises is a file set
//! that `req canon`, `req canon --fill` and `req canon --verify` accept, so every check here
//! drives the script and then the compiled binary over the corpus it wrote. A unit-tested
//! string builder would prove nothing about that.
//!
//! The oracle for "the drafts are unfilled" is `req canon --fill`'s own count: a draft that
//! carried a derived `form` or `address` would be *already current* to `--fill`, and `filled 2`
//! is the measurement that the tool derived nothing. The draft's shape is checked directly too
//! (`form: ""`, `address: null`) — the two assertions fail differently: one catches a derived
//! value, the other catches a template that drifted from `src/init.rs`'s.
//!
//! `python3` is required, and its absence is a **failure named here** rather than a skipped
//! check (`require_minisign` in `tests/baseline.rs` is the precedent): a skipped check is never
//! a pass.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

mod common;

/// The absolute path of the compiled binary, so the test does not depend on `$PATH`.
fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// The extractor under test, from the tree this test run was built out of.
fn adopt_py() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/adopt.py")
}

/// `python3` resolved from the test process's own `PATH`, as an absolute path: the child's
/// `PATH` is rewritten below (to put the built `req` on it), and a program name resolved
/// through the rewritten value would be a second, unintended dependency on it.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "python3 is required for tools/adopt.py — install it first \
         (macOS: `xcode-select --install`; Debian: `apt-get install python3`)"
    );
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn assert_code(&self, expected: i32, what: &str) {
        assert_eq!(
            self.code, expected,
            "{what}: expected exit {expected}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout, self.stderr
        );
    }
}

/// Distinguishes fixtures within one process; the process id extends that across processes.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A base directory the suite controls: `TMPDIR` when it is outside any checkout, else `/tmp`,
/// else `/var/tmp`. A sandboxed CI may set `TMPDIR` inside the workspace, and a fixture under it
/// would inherit this repository's `claims/`/`.git` and stop testing what it says.
fn fixture_base() -> PathBuf {
    let mut candidates = vec![std::env::temp_dir()];
    candidates.push(PathBuf::from("/tmp"));
    candidates.push(PathBuf::from("/var/tmp"));
    for base in candidates {
        if !base.is_dir() {
            continue;
        }
        let clean = !base
            .ancestors()
            .any(|dir| req::root::is_corpus_root(dir) || dir.join(".git").exists());
        if clean {
            return base;
        }
    }
    panic!(
        "no fixture base outside a checkout: TMPDIR, /tmp and /var/tmp all sit under a          directory holding claims/ or .git"
    );
}

/// A directory with **no `claims/` and no `.git` ancestor**: the extractor's root resolution
/// (`--root`, else `REQ_ROOT`, else the working directory) and its `--date` fallback (the
/// source's last commit) both walk outwards, so a fixture that inherited either would check
/// something other than what it says.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = fixture_base().join(format!("{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the fixture directory");
        if let Some(found) = dir.ancestors().find(|d| req::root::is_corpus_root(d)) {
            panic!(
                "the fixture assumes no ancestor holds claims/, but {} does",
                found.display()
            );
        }
        if let Some(found) = dir.ancestors().find(|d| d.join(".git").exists()) {
            panic!(
                "the fixture assumes it is outside a git repository (the `--date` fallback \
                 shells out to `git log`), but {} holds .git",
                found.display()
            );
        }
        Fixture(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Write a file, creating its parents.
    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("creates the fixture's parent directory");
        }
        fs::write(&path, text).unwrap_or_else(|e| panic!("writes {}: {e}", path.display()));
    }

    fn read(&self, rel: &str) -> String {
        let path = self.0.join(rel);
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reads {}: {e}", path.display()))
    }

    fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).is_file()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The compiled `req`, run in the fixture.
fn req(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `tools/adopt.py`, run in the fixture, with the built `req` first on `PATH` — the tool takes
/// `rules_v` and the claim schema from `req --version`, never from a literal.
fn adopt(cwd: &Path, args: &[&str]) -> Outcome {
    let mut path_value = req_bin()
        .parent()
        .expect("the binary has a parent")
        .as_os_str()
        .to_os_string();
    if let Some(existing) = std::env::var_os("PATH") {
        path_value.push(":");
        path_value.push(existing);
    }
    let output = Command::new(python3())
        .arg(adopt_py())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .env("PATH", path_value)
        .output()
        .expect("runs tools/adopt.py");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `tools/adopt.py` with `bin_dir` first on `PATH` — the seam a fake `req` needs.
#[cfg(unix)]
fn adopt_with_bin_dir(cwd: &Path, args: &[&str], bin_dir: &Path) -> Outcome {
    let mut path_value = bin_dir.as_os_str().to_os_string();
    if let Some(existing) = std::env::var_os("PATH") {
        path_value.push(":");
        path_value.push(existing);
    }
    let output = Command::new(python3())
        .arg(adopt_py())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .env("PATH", path_value)
        .output()
        .expect("runs tools/adopt.py");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `tools/adopt.py` with `REQ_ROOT` naming the corpus (the other root-resolution branch).
fn adopt_with_req_root(cwd: &Path, args: &[&str], root: &Path) -> Outcome {
    let mut path_value = req_bin()
        .parent()
        .expect("the binary has a parent")
        .as_os_str()
        .to_os_string();
    if let Some(existing) = std::env::var_os("PATH") {
        path_value.push(":");
        path_value.push(existing);
    }
    let output = Command::new(python3())
        .arg(adopt_py())
        .args(args)
        .current_dir(cwd)
        .env("REQ_ROOT", root)
        .env("PATH", path_value)
        .output()
        .expect("runs tools/adopt.py");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `req --version`'s `rules_v` and `claim_schema`, the values a draft must carry. Read from the
/// binary rather than restated, because a literal here would be a second copy of the axis that
/// decides what a stored address means.
fn version_axes() -> (String, String) {
    let out = req(Path::new("."), &["--version"]);
    out.assert_code(0, "req --version");
    let field = |label: &str| -> String {
        out.stdout
            .lines()
            .find_map(|line| {
                let rest = line.trim().strip_prefix(label)?;
                Some(rest.trim().to_string())
            })
            .unwrap_or_else(|| panic!("`{label}` missing from req --version:\n{}", out.stdout))
    };
    (field("rules_v"), field("claim_schema"))
}

/// The document the main loop uses: two statements, one empty item that must be skipped and
/// named. Nothing here is a requirement id — `bullets` takes the item's text verbatim.
const CAPABILITIES: &str = "\
# Capabilities

A paragraph that is not a requirement is not a candidate and is not reported.

- The copilot reads each submitted document before judging it.
- The copilot cites every source it quotes.
- \x20

```text
- A fenced sample is code, not a requirement.
```
";

/// One claim file's field, by exact whole-line prefix. The template is argued over byte for
/// byte, so this is a lookup rather than a `contains` that a comment could satisfy.
fn field<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines().find(|line| line.starts_with(prefix))
}

#[test]
fn the_draft_path_is_green_end_to_end() {
    let fx = Fixture::new("req-adopt-loop");
    fx.write("docs/capabilities.md", CAPABILITIES);
    let init = req(fx.path(), &["init"]);
    init.assert_code(0, "req init");

    // The dry run is the default: it prints, writes nothing, and names the skipped line.
    let dry = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    dry.assert_code(0, "the dry run");
    assert!(
        dry.stdout.contains("drafted: 2"),
        "the dry run must count the two drafts:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("skipped: 1"),
        "the dry run must count the skipped line:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("claims/REQ-0002.yml") && dry.stdout.contains("claims/REQ-0003.yml"),
        "the dry run must name the files it would write:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("empty text") && dry.stdout.contains("#L7"),
        "the skipped line must be named with its line number and reason:\n{}",
        dry.stdout
    );
    assert!(
        !fx.exists("claims/REQ-0002.yml"),
        "a dry run writes nothing"
    );

    let write = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    write.assert_code(0, "the write run");
    assert!(
        write.stdout.contains("wrote: 2"),
        "the write run must report what it wrote:\n{}",
        write.stdout
    );

    // The drafts are legal, unfilled, and carry no derived field: `form` is the placeholder
    // `--fill` rewrites and `address` is `null`. `--fill` itself is the independent oracle
    // below — `filled 2` is only possible if the tool derived nothing.
    let (rules_v, schema) = version_axes();
    for (path, statement, source_line) in [
        (
            "claims/REQ-0002.yml",
            "The copilot reads each submitted document before judging it.",
            5,
        ),
        (
            "claims/REQ-0003.yml",
            "The copilot cites every source it quotes.",
            6,
        ),
    ] {
        let text = fx.read(path);
        assert_eq!(
            field(&text, "schema: "),
            Some(format!("schema: {schema}").as_str()),
            "{path} must declare the schema this build writes:\n{text}"
        );
        assert_eq!(
            field(&text, "  rules_v: "),
            Some(format!("  rules_v: {rules_v}").as_str()),
            "{path} must carry the rules version `req --version` reports:\n{text}"
        );
        assert_eq!(
            field(&text, "modality: "),
            Some("modality: UNSTRUCTURED"),
            "{path}: an unstructured claim is the only legal draft:\n{text}"
        );
        assert_eq!(
            field(&text, "unstructured: "),
            Some("unstructured: true"),
            "{path}:\n{text}"
        );
        assert_eq!(
            field(&text, "  form: "),
            Some("  form: \"\""),
            "{path} must leave the form as the placeholder `--fill` rewrites:\n{text}"
        );
        assert_eq!(
            field(&text, "address: "),
            Some("address: null"),
            "{path} must never carry an address:\n{text}"
        );
        assert!(
            text.contains(statement),
            "{path} must carry its statement from the document:\n{text}"
        );
        assert!(
            text.contains(&format!("source: \"docs/capabilities.md#L{source_line}\"")),
            "{path} must carry the document and the exact line it came from (marker line, not \
             the joined item's last line):\n{text}"
        );
        assert!(
            text.contains("date: 2026-10-21"),
            "{path} must carry the authored date, not today's:\n{text}"
        );
    }

    // The skipped line produced no claim: it is not in the corpus in any form.
    for path in ["claims/REQ-0004.yml", "claims/REQ-0005.yml"] {
        assert!(!fx.exists(path), "the skipped line must not become a claim");
    }

    // The loop the draft path hands to the human: `canon` reports, `--fill` derives the two
    // fields, `--verify` proves them.
    let canon = req(fx.path(), &["canon"]);
    canon.assert_code(0, "req canon over the drafts");
    assert!(
        canon.stdout.contains("discrepancy flags: 0"),
        "the drafts are legal as written:\n{}",
        canon.stdout
    );

    let fill = req(fx.path(), &["canon", "--fill"]);
    fill.assert_code(0, "req canon --fill");
    assert!(
        fill.stdout.contains("filled 2"),
        "both drafts must have been unfilled — a drafted form or address would make one \
         already current:\n{}",
        fill.stdout
    );

    let verify = req(fx.path(), &["canon", "--verify"]);
    verify.assert_code(0, "req canon --verify");
    // Three claims: `init`'s starter claim and the two drafts. `--fill`'s `filled 2` above is
    // the load-bearing count (the starter was already current); this one proves the filled
    // corpus verifies end to end.
    assert!(
        verify.stdout.contains("verified 3"),
        "the drafts must verify after one fill:\n{}",
        verify.stdout
    );
}

#[test]
fn a_second_write_over_the_same_document_refuses_and_overwrites_nothing() {
    let fx = Fixture::new("req-adopt-second");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let args = [
        "--from",
        "docs/capabilities.md",
        "--subject",
        "system",
        "--date",
        "2026-10-21",
        "--write",
    ];
    adopt(fx.path(), &args).assert_code(0, "the first write");

    // Every existing claim file, byte for byte: an overwrite is visible as a changed file, and
    // a fresh draft is visible as a new one.
    let mut before = std::collections::BTreeMap::new();
    for entry in fs::read_dir(fx.path().join("claims")).expect("the claims directory") {
        let entry = entry.expect("a claim entry");
        if entry.path().extension().is_some_and(|e| e == "yml") {
            before.insert(
                entry.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(entry.path()).expect("reads a claim"),
            );
        }
    }

    let second = adopt(fx.path(), &args);
    second.assert_code(1, "a second --write over the same document");
    assert!(
        second.stderr.contains("already drafted"),
        "the refusal must say the document is already drafted, and name the claims:\n{}",
        second.stderr
    );
    assert!(
        second.stderr.contains("REQ-0002"),
        "the refusal must name the existing claims:\n{}",
        second.stderr
    );

    let mut after = std::collections::BTreeMap::new();
    for entry in fs::read_dir(fx.path().join("claims")).expect("the claims directory") {
        let entry = entry.expect("a claim entry");
        if entry.path().extension().is_some_and(|e| e == "yml") {
            after.insert(
                entry.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(entry.path()).expect("reads a claim"),
            );
        }
    }
    assert_eq!(
        before, after,
        "a refused run must write nothing and overwrite nothing"
    );
}

#[test]
fn no_corpus_is_unverifiable_and_names_init() {
    let fx = Fixture::new("req-adopt-nocorpus");
    fx.write("docs/capabilities.md", CAPABILITIES);
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    out.assert_code(3, "no corpus");
    assert!(
        out.stderr.contains("claims/") && out.stderr.contains("req init"),
        "the remedy for an unreadable corpus is named, and names the directory that is \
         missing (the registry's message is a different defect):\n{}",
        out.stderr
    );
    assert!(
        !fx.exists("claims/REQ-0001.yml"),
        "the extractor never creates a corpus"
    );
}

#[test]
fn an_unreadable_document_is_unverifiable() {
    let fx = Fixture::new("req-adopt-nodoc");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/absent.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    out.assert_code(3, "a document that is not there");
    assert!(
        out.stderr.contains("cannot read the document")
            && out.stderr.contains("docs/absent.md")
            && !out.stderr.contains("internal error"),
        "the missing file must be named, not reported as an internal error:\n{}",
        out.stderr
    );
}

#[test]
fn a_missing_subject_is_a_usage_error() {
    let fx = Fixture::new("req-adopt-nosubject");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let out = adopt(
        fx.path(),
        &["--from", "docs/capabilities.md", "--date", "2026-10-21"],
    );
    out.assert_code(2, "a missing --subject");
    assert!(
        out.stderr.contains("--subject"),
        "the usage error must name the missing flag:\n{}",
        out.stderr
    );
}

#[test]
fn a_date_absent_from_the_source_and_from_git_is_a_usage_error() {
    let fx = Fixture::new("req-adopt-nodate");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let out = adopt(
        fx.path(),
        &["--from", "docs/capabilities.md", "--subject", "system"],
    );
    out.assert_code(2, "no --date and no commit date to borrow");
    assert!(
        out.stderr.contains("--date"),
        "the remedy is the flag that supplies a date:\n{}",
        out.stderr
    );
}

/// A throwaway repository **inside the fixture**, with identity and dates pinned through the
/// environment: this repository's history is never touched, and the fixture's goes with it.
/// Global git config is isolated so a developer's `commit.gpgsign` or `core.hooksPath` cannot
/// fail the test, and so the pinned dates are the only thing that decides the outcome.
fn git_commit_all(fx: &Fixture, message: &str) {
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec!["commit", "-q", "-m", message],
    ] {
        let output = Command::new("git")
            .args(&args)
            .current_dir(fx.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "req adopt test")
            .env("GIT_AUTHOR_EMAIL", "adopt@example.invalid")
            .env("GIT_COMMITTER_NAME", "req adopt test")
            .env("GIT_COMMITTER_EMAIL", "adopt@example.invalid")
            .env("GIT_AUTHOR_DATE", "2020-01-02T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2020-01-02T00:00:00Z")
            .output()
            .expect("runs git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// The date is **authored**: `--date` or the source document's last commit, never "today".
#[test]
fn the_date_defaults_to_the_sources_last_commit_and_never_to_today() {
    let fx = Fixture::new("req-adopt-gitdate");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    git_commit_all(&fx, "the source document");

    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--write",
        ],
    );
    out.assert_code(0, "the date comes from the document's last commit");
    let draft = fx.read("claims/REQ-0002.yml");
    assert!(
        draft.contains("date: 2020-01-02"),
        "the draft must carry the source's commit date, not today's:\n{draft}"
    );
}

/// A document changed since its last commit has no commit that describes its text: borrowing
/// that date would name a revision the drafted text does not exist in.
#[test]
fn a_tracked_document_with_uncommitted_changes_has_no_date_to_borrow() {
    let fx = Fixture::new("req-adopt-dirty");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    git_commit_all(&fx, "the source document");
    fx.write(
        "docs/capabilities.md",
        &format!("{CAPABILITIES}\n- A bullet added after the commit.\n"),
    );

    let blocked = adopt(
        fx.path(),
        &["--from", "docs/capabilities.md", "--subject", "system"],
    );
    blocked.assert_code(2, "a changed document with no --date");
    assert!(
        blocked.stderr.contains("--date") && blocked.stderr.contains("uncommitted"),
        "the remedy is the flag that supplies a date:\n{}",
        blocked.stderr
    );

    // The authored date is the remedy, and the run then succeeds.
    let dated = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    dated.assert_code(0, "the same document with an authored date");
}

/// `req init`'s starter claim names `docs/requirements.md` as a pencil sketch, not as an
/// adoption: treating that as an earlier draft would refuse the document an adopter is most
/// likely to have.
#[test]
fn the_init_starter_placeholder_does_not_block_adoption_from_that_document() {
    let fx = Fixture::new("req-adopt-starter");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "docs/requirements.md",
        "# Requirements\n\n- The adopter's own first requirement.\n",
    );
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/requirements.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    out.assert_code(0, "adopting from the document the starter claim names");
    assert!(
        fx.exists("claims/REQ-0002.yml"),
        "the starter claim is not evidence of an earlier draft:\n{}\n{}",
        out.stdout,
        out.stderr
    );
    assert!(
        fx.read("claims/REQ-0001.yml")
            .contains("claim_id: REQ-0001"),
        "the starter claim is left alone"
    );
}

/// A draft is legal as written only when the canonicalizer leaves its statement unstructured:
/// a `shall` sentence parses as a structured claim, contradicts `unstructured: true`, and
/// `req canon --fill` refuses it. The tool must report such a candidate and never write it.
#[test]
fn a_structured_statement_is_reported_and_never_drafted() {
    let fx = Fixture::new("req-adopt-structured");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "docs/mixed.md",
        "# Mixed\n\n- The system shall log every decision.\n- The copilot cites every source it quotes.\n",
    );
    let dry = adopt(
        fx.path(),
        &[
            "--from",
            "docs/mixed.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    dry.assert_code(0, "one prose candidate drafts, the `shall` one is skipped");
    assert!(
        dry.stdout.contains("drafted: 1") && dry.stdout.contains("skipped: 1"),
        "the counts name both outcomes:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("parses as a structured claim") && dry.stdout.contains("#L3"),
        "the skipped statement is named with its reason and line:\n{}",
        dry.stdout
    );

    let write = adopt(
        fx.path(),
        &[
            "--from",
            "docs/mixed.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    write.assert_code(0, "the write run");
    let draft = fx.read("claims/REQ-0002.yml");
    assert!(
        draft.contains("statement: \"The copilot cites every source it quotes.\""),
        "only the prose candidate is drafted:\n{draft}"
    );
    assert!(
        !draft.contains("shall log every decision"),
        "the structured candidate is not written:\n{draft}"
    );
    req(fx.path(), &["canon"]).assert_code(0, "the written drafts are legal as written");

    // Every candidate structured: a definite problem, nothing written.
    fx.write(
        "docs/shall-only.md",
        "# Shall\n\n- The system shall log every decision.\n",
    );
    let none = adopt(
        fx.path(),
        &[
            "--from",
            "docs/shall-only.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    none.assert_code(1, "no candidate is a prose-only claim");
    assert!(
        none.stderr.contains("nothing drafted")
            && none.stderr.contains("parses as a structured claim"),
        "the finding names why nothing was drafted:\n{}",
        none.stderr
    );
    assert!(
        !fx.exists("claims/REQ-0003.yml"),
        "nothing is written when no candidate is draftable"
    );
}

/// Two identical statements would become two claims with one address — a duplicate under a
/// second id. The second occurrence is named as skipped, never drafted.
#[test]
fn a_duplicate_candidate_is_named_and_not_drafted_twice() {
    let fx = Fixture::new("req-adopt-duplicate");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "docs/capabilities.md",
        "# Capabilities\n\n- The copilot cites every source it quotes.\n- The copilot cites every source it quotes.\n",
    );
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    out.assert_code(0, "the duplicate is skipped, the first is drafted");
    assert!(
        out.stdout.contains("drafted: 1")
            && out.stdout.contains("skipped: 1")
            && out.stdout.contains("duplicate of #L3"),
        "the repeat is named with the line it duplicates:\n{}",
        out.stdout
    );
    assert!(
        fx.exists("claims/REQ-0002.yml"),
        "the first occurrence drafts"
    );
    assert!(
        !fx.exists("claims/REQ-0003.yml"),
        "the duplicate is not written as a second claim"
    );
    req(fx.path(), &["canon"]).assert_code(0, "the written draft is legal");
}

/// `req canon --fill` exits 3 both for a refusal *and* for a corpus it could not read, printing
/// only stderr in the latter case. A candidate check with no `refused <n>` summary judged
/// nothing, and treating that as "every candidate is legal" is the false success the check
/// exists to prevent: it must be UNVERIFIABLE, and nothing may be written.
#[cfg(unix)]
#[test]
fn a_candidate_check_with_no_verdict_is_unverifiable_not_a_pass() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new("req-adopt-nosummary");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write("docs/capabilities.md", CAPABILITIES);

    // A `req` that answers `--version` like this build but fails the candidate check the way
    // `--fill` does when it cannot read the corpus: exit 3, a message on stderr, no summary.
    let fake_dir = fx.path().join("fake-bin");
    fs::create_dir_all(&fake_dir).expect("creates the fake bin directory");
    let fake = fake_dir.join("req");
    fs::write(
        &fake,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
         \x20 printf 'req 0.1.0\\n  rules_v            5\\n  claim_schema       req.claim/1\\n'\n\
         \x20 exit 0\n\
         fi\n\
         echo 'cannot read the corpus registry' >&2\n\
         exit 3\n",
    )
    .expect("writes the fake req");
    let mut permissions = fs::metadata(&fake)
        .expect("the fake req exists")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).expect("makes the fake req executable");

    let out = adopt_with_bin_dir(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
        &fake_dir,
    );
    out.assert_code(3, "a candidate check that judged nothing");
    assert!(
        out.stderr.contains("no verdict") || out.stderr.contains("no `refused"),
        "the unknown is reported as UNVERIFIABLE, naming what was missing:\n{}",
        out.stderr
    );
    assert!(
        !fx.exists("claims/REQ-0002.yml"),
        "nothing is written when the candidates were not judged"
    );
}

/// A subject id that YAML would otherwise resolve to a non-string (`null`, `true`, `1.5`) must
/// still arrive as a string, or the draft is a file the strict `ClaimFile` cannot deserialize
/// while the tool reports success.
#[test]
fn a_yaml_shaped_subject_is_still_a_string_in_the_claim() {
    let fx = Fixture::new("req-adopt-subject");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "null",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    out.assert_code(0, "a subject YAML would otherwise resolve to null");
    let draft = fx.read("claims/REQ-0002.yml");
    assert!(
        draft.contains("subject: \"null\""),
        "the subject is a quoted string, not the YAML null scalar:\n{draft}"
    );
    req(fx.path(), &["canon"]).assert_code(0, "the draft parses");
}

/// A directory in `claims/` makes the corpus one `req` refuses (the corpus is flat). Drafting
/// into it would leave a tree that is red and that this tool reported writing successfully.
#[test]
fn a_directory_in_claims_is_a_corpus_error_not_a_silent_skip() {
    let fx = Fixture::new("req-adopt-claimsdir");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fs::create_dir_all(fx.path().join("claims/archive")).expect("creates a directory in claims/");
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    out.assert_code(3, "a corpus `req` refuses is not a corpus to draft into");
    assert!(
        out.stderr.contains("claims/"),
        "the error names the directory that is not a claim:\n{}",
        out.stderr
    );
    assert!(
        !fx.exists("claims/REQ-0002.yml"),
        "nothing is written into a corpus that is already red"
    );
}

/// A document that is not UTF-8 is UNVERIFIABLE naming the file — never an "internal error"
/// from a decode that slipped past the I/O handler.
#[test]
fn a_non_utf8_document_is_unverifiable_and_named() {
    let fx = Fixture::new("req-adopt-nonutf8");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fs::create_dir_all(fx.path().join("docs")).expect("creates the docs directory");
    fs::write(
        fx.path().join("docs/capabilities.md"),
        b"# Capabilities\n\n- caf\xe9\n",
    )
    .expect("writes raw bytes");
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    out.assert_code(3, "a document that is not UTF-8");
    assert!(
        out.stderr.contains("cannot read the document"),
        "the file is named, not reported as an internal error:\n{}",
        out.stderr
    );
}

/// A run from a subdirectory: `--root`/`REQ_ROOT` name the corpus, a relative `--from` is
/// resolved against it, and the provenance is recorded root-relative — one basis for writing
/// and for reading back, so the guard still refuses a second run from the same place.
#[test]
fn a_run_from_a_subdirectory_uses_the_named_root_and_one_provenance_basis() {
    let fx = Fixture::new("req-adopt-subdir");
    fx.write("docs/capabilities.md", CAPABILITIES);
    fx.write(
        "docs/more.md",
        "# More\n\n- A second document's requirement.\n",
    );
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fs::create_dir_all(fx.path().join("sub")).expect("creates the subdirectory");
    let sub = fx.path().join("sub");

    let args = [
        "--root",
        "..",
        "--from",
        "docs/capabilities.md",
        "--subject",
        "system",
        "--date",
        "2026-10-21",
        "--write",
    ];
    let out = adopt(&sub, &args);
    out.assert_code(0, "a run from a subdirectory with --root");
    assert!(
        fx.exists("claims/REQ-0002.yml"),
        "the drafts land in the named root:\n{}\n{}",
        out.stdout,
        out.stderr
    );
    let draft = fx.read("claims/REQ-0002.yml");
    assert!(
        draft.contains("source: \"docs/capabilities.md#L5\""),
        "the provenance is root-relative, so writing and reading share one basis:\n{draft}"
    );

    let second = adopt(&sub, &args);
    second.assert_code(1, "a second --write from the same subdirectory");
    assert!(
        second.stderr.contains("already drafted"),
        "the guard must read the same basis the write recorded:\n{}",
        second.stderr
    );

    // $REQ_ROOT names the root just as --root does, and a relative --from resolves against it.
    fs::create_dir_all(fx.path().join("sub2")).expect("creates a second subdirectory");
    let env_run = adopt_with_req_root(
        &fx.path().join("sub2"),
        &[
            "--from",
            "docs/more.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
        fx.path(),
    );
    env_run.assert_code(0, "REQ_ROOT resolves the document against the corpus root");
    assert!(
        env_run.stdout.contains("drafted: 1"),
        "the second document drafts from the named root:\n{}",
        env_run.stdout
    );
}

#[test]
fn req_lines_ignores_the_sources_ids_and_names_every_skipped_line() {
    let fx = Fixture::new("req-adopt-reqlines");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "docs/backlog.md",
        "# Backlog\n\n- REQ-7: The system logs every decision.\n- REQ-8: The system cites its sources.\n- A bullet with no id at all.\n",
    );
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/backlog.md",
            "--convention",
            "req-lines",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    out.assert_code(0, "the req-lines convention");
    assert!(
        out.stdout.contains("drafted: 2") && out.stdout.contains("skipped: 1"),
        "two req lines are drafted and the id-less bullet is skipped:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("ids are ignored"),
        "the report must say the source's ids are ignored:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("no id"),
        "the skipped bullet's reason is named:\n{}",
        out.stdout
    );

    // The source's ids do not become claim ids, and do not ride in the statements.
    let draft = fx.read("claims/REQ-0002.yml");
    assert!(
        draft.contains("claim_id: REQ-0002"),
        "req allocates the claim id:\n{draft}"
    );
    assert!(
        draft.contains("statement: \"The system logs every decision.\""),
        "the statement is the text after the colon:\n{draft}"
    );
    assert!(
        !draft.contains("REQ-7"),
        "the source's id must not reach the draft:\n{draft}"
    );
    req(fx.path(), &["canon"]).assert_code(0, "the drafts are legal as written");
}

#[test]
fn the_heading_scope_is_respected_and_everything_outside_it_is_named() {
    let fx = Fixture::new("req-adopt-under");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "docs/capabilities.md",
        "# Capabilities\n\n- In scope, first.\n\n## Detail\n\n- In scope, in a subsection.\n\n# Elsewhere\n\n- Out of scope.\n",
    );
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--under",
            "Capabilities",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
        ],
    );
    out.assert_code(0, "the heading scope");
    assert!(
        out.stdout.contains("drafted: 2"),
        "a heading's subsections are under it:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("skipped: 1") && out.stdout.contains("outside the heading"),
        "the bullet outside the heading is named with its reason:\n{}",
        out.stdout
    );
}

#[test]
fn nothing_extracted_is_a_definite_problem_and_names_why() {
    let fx = Fixture::new("req-adopt-nothing");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    // No candidates at all is a definite problem, not an empty draft list.
    fx.write(
        "docs/capabilities.md",
        "# Capabilities\n\nNo list items at all.\n",
    );
    let out = adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    out.assert_code(1, "a document with nothing to draft");
    assert!(
        !fx.exists("claims/REQ-0002.yml"),
        "nothing extracted writes nothing"
    );

    // Candidates that were all skipped are still named with their reasons: the finding is
    // "nothing extracted", and why each line was skipped is its remedy.
    fx.write("docs/skips-only.md", "# Capabilities\n\n- \n");
    let skipped = adopt(
        fx.path(),
        &[
            "--from",
            "docs/skips-only.md",
            "--subject",
            "system",
            "--date",
            "2026-10-21",
            "--write",
        ],
    );
    skipped.assert_code(1, "a document whose only candidate is skipped");
    assert!(
        skipped.stderr.contains("empty text") && skipped.stderr.contains("#L3"),
        "the skipped line must be named even when nothing is drafted:\n{}",
        skipped.stderr
    );
    assert!(
        !fx.exists("claims/REQ-0002.yml"),
        "nothing extracted writes nothing"
    );
}

#[test]
fn the_self_check_passes_and_is_wired_into_the_gate() {
    // The table itself.
    let out = Command::new(python3())
        .arg(adopt_py())
        .arg("--self-check")
        .output()
        .expect("runs tools/adopt.py --self-check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "--self-check must pass with no corpus, no git and no req:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The table is the test: an empty or gutted one must redden this, so its size and its
    // summary are pinned, not only the exit code.
    let cases: usize = stdout
        .split("adopt --self-check: ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("the self-check summary is missing:\n{stdout}"));
    assert!(
        cases >= 30,
        "the self-check table has lost rows ({cases} case(s)):\n{stdout}"
    );
    assert!(
        stdout.contains("0 failure(s)"),
        "every row must pass:\n{stdout}"
    );

    // The gate's plan prints the default run's commands from the same call sites and executes
    // none (`tests/baseline.rs` drives the same seam), so removing the wiring reddens this.
    for args in [vec!["plan"], vec!["--fast", "plan"]] {
        let plan = check_sh(&args);
        let stdout = String::from_utf8_lossy(&plan.stdout);
        assert_eq!(
            plan.status.code(),
            Some(3),
            "a plan is never a pass:\n{stdout}"
        );
        assert!(
            normalized(&stdout)
                .iter()
                .any(|line| line.contains("tools/adopt.py --self-check")),
            "the gate must self-check the extractor in {args:?}:\n{stdout}"
        );
    }
}

/// `scripts/check.sh` from the tree under test, with `HOME` pointed at the toolchain the gate
/// is meant to use (the same seam `tests/baseline.rs` and `tests/decision.rs` share).
fn check_sh(args: &[&str]) -> std::process::Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new("bash");
    command
        .arg(root.join("scripts/check.sh"))
        .args(args)
        .current_dir(&root);
    common::point_home_at_toolchain(&mut command);
    command.output().expect("runs scripts/check.sh")
}

fn normalized(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}
