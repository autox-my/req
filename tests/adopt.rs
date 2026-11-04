//! `tools/adopt.py` — the draft path: a document in, **legal** claims out.
//!
//! The seam is the CLI, not the extractor's functions: what the tool promises is a file set
//! that `req canon`, `req render` and `req gate` accept, so every check here drives the script
//! and then the compiled binary over the corpus it wrote. A unit-tested string builder would
//! prove nothing about that.
//!
//! The oracle for "the drafts carry nothing derived" is twofold and fails two ways: the draft's
//! bytes carry none of the `req.claim/1` slots or stored fields (a template that drifted), and
//! the strict `req.claim/2` loader accepts them (a removed key is a parse error naming it, ADR
//! 0022) — so `req canon` exiting 0 over the drafts is itself the measurement.
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
        &["--from", "docs/capabilities.md", "--date", "2026-10-21"],
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

    // The drafts are legal and carry nothing derived (ADR 0022): no parse, no address, no pin.
    let (_rules_v, schema) = version_axes();
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
        for key in [
            "modality:",
            "subject:",
            "attribute:",
            "operator:",
            "value:",
            "unit:",
            "conditions:",
            "unstructured:",
            "canonical:",
            "address:",
            "pins:",
        ] {
            assert_eq!(
                field(&text, key),
                None,
                "{path} carries nothing derived and no pin it could only guess (`{key}`):\n{text}"
            );
        }
        assert_eq!(field(&text, "applicability: "), Some("applicability: []"));
        assert_eq!(field(&text, "relations: "), Some("relations: []"));
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

    // The loop the draft path hands to the human: `canon` shows the derived parse of all three
    // claims (`init`'s starter and the two drafts) and nothing is flagged; the corpus renders.
    let canon = req(fx.path(), &["canon"]);
    canon.assert_code(0, "req canon over the drafts");
    assert!(
        canon.stdout.contains("claims: 3 (") && canon.stdout.contains(", 0 flagged"),
        "the drafts load under the strict v2 loader, and no pin is flagged:\n{}",
        canon.stdout
    );
    let render = req(fx.path(), &["render"]);
    render.assert_code(0, "req render over the drafts");
    for id in ["REQ-0001", "REQ-0002", "REQ-0003"] {
        assert!(
            render.stdout.contains(id),
            "{id} renders:\n{}",
            render.stdout
        );
    }
}

#[test]
fn a_second_write_over_the_same_document_refuses_and_overwrites_nothing() {
    let fx = Fixture::new("req-adopt-second");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let args = [
        "--from",
        "docs/capabilities.md",
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

/// The tool drafts the schema its template writes. A `req` that reports another one cannot load
/// the draft, so the tool fails loud (UNVERIFIABLE, exit 3) naming both schemas, instead of
/// writing an unloadable corpus and exiting 0. A fake `req` first on `PATH` is the seam: the tool
/// asks `req --version` and nothing else before drafting.
#[cfg(unix)]
#[test]
fn a_req_reporting_another_claim_schema_is_unverifiable_and_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new("req-adopt-schema-mismatch");
    fx.write("docs/capabilities.md", CAPABILITIES);
    let init = req(fx.path(), &["init"]);
    init.assert_code(0, "req init");

    let bin = fx.path().join("fake-bin");
    std::fs::create_dir_all(&bin).expect("fake bin dir");
    let fake = bin.join("req");
    std::fs::write(
        &fake,
        "#!/bin/sh\nprintf 'req 0.1.0\\n  rules_v            6\\n  claim_schema       req.claim/3\\n'\n",
    )
    .expect("fake req");
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let mut path_value = bin.as_os_str().to_os_string();
    path_value.push(":");
    path_value.push(std::env::var_os("PATH").unwrap_or_default());
    let output = Command::new(python3())
        .arg(adopt_py())
        .args([
            "--from",
            "docs/capabilities.md",
            "--date",
            "2026-10-21",
            "--write",
        ])
        .current_dir(fx.path())
        .env_remove("REQ_ROOT")
        .env("PATH", path_value)
        .output()
        .expect("runs tools/adopt.py");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "stderr:\n{stderr}");
    assert!(
        stderr.contains("req.claim/3") && stderr.contains("req.claim/2"),
        "the message names the schema the binary reports and the one the tool drafts:\n{stderr}"
    );
    assert!(
        !fx.exists("claims/REQ-0002.yml"),
        "nothing is drafted against a schema the tool does not write"
    );
}

#[test]
fn no_corpus_is_unverifiable_and_names_init() {
    let fx = Fixture::new("req-adopt-nocorpus");
    fx.write("docs/capabilities.md", CAPABILITIES);
    let out = adopt(
        fx.path(),
        &["--from", "docs/capabilities.md", "--date", "2026-10-21"],
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
        &["--from", "docs/absent.md", "--date", "2026-10-21"],
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

/// `--subject` is gone with `req.claim/2`'s subject slot (ADR 0022): the subject is derived from
/// the statement. An adopter's script still passing it is told so — a usage error naming the
/// flag — rather than having it silently ignored.
/// A corpus the loader does not accept is not drafted into: the tool runs the corpus plus the
/// drafts through `req canon` before writing anything, so every shape the loader rejects — an
/// older schema (also with a comment or a block scalar on the line), a missing schema, a removed
/// v1 key, an invalid pin — turns the run away (exit 3) and writes nothing. The older-schema
/// message names the migration tool. A control: a valid file with a quoted schema is no obstacle.
#[test]
fn a_corpus_the_loader_rejects_is_unverifiable_and_nothing_is_drafted() {
    let valid = |schema_line: &str, extra: &str| {
        format!(
            "{schema_line}\nclaim_id: REQ-0009\nstatement: An existing claim.\napplicability: []\n\
             relations: []\nrationale:\n  source: x\n  date: 2026-10-07\n  \
             rejected_alternatives: []\n  assumptions: []\n{extra}"
        )
    };
    let current = req::canon::claim_schema();
    let older = format!("{}1", req::canon::CLAIM_SCHEMA_PREFIX);
    for (name, text, migrator) in [
        ("older", valid(&format!("schema: {older}"), ""), true),
        (
            "older-comment",
            valid(&format!("schema: {older}  # old"), ""),
            true,
        ),
        ("older-block", valid("schema: >-\n  req.claim/1", ""), true),
        ("no-schema", valid("# no schema here", ""), false),
        (
            "removed-key",
            valid(&format!("schema: {current}"), "modality: SHALL\n"),
            false,
        ),
        (
            "bad-pin",
            valid(&format!("schema: {current}"), "pins:\n  value: \"0.30\"\n"),
            false,
        ),
    ] {
        let fx = Fixture::new(&format!("req-adopt-reject-{name}"));
        fx.write("docs/capabilities.md", CAPABILITIES);
        req(fx.path(), &["init"]).assert_code(0, "req init");
        fx.write("claims/REQ-0009.yml", &text);
        let out = adopt(
            fx.path(),
            &[
                "--from",
                "docs/capabilities.md",
                "--date",
                "2026-10-21",
                "--write",
            ],
        );
        out.assert_code(3, &format!("{name}: a corpus the loader rejects"));
        assert!(
            out.stderr.contains("predates") == migrator,
            "{name}: only an older schema is reported as predating the format (and the loader \
             names the migration tool there):\n{}",
            out.stderr
        );
        assert!(
            !fx.exists("claims/REQ-0010.yml"),
            "{name}: nothing is drafted into it"
        );
    }

    let fx = Fixture::new("req-adopt-reject-control");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "claims/REQ-0009.yml",
        &valid(&format!("schema: \"{current}\""), ""),
    );
    adopt(
        fx.path(),
        &[
            "--from",
            "docs/capabilities.md",
            "--date",
            "2026-10-21",
            "--write",
        ],
    )
    .assert_code(0, "a quoted current schema is a valid corpus");
}

#[test]
fn a_subject_flag_is_a_usage_error_since_the_subject_is_derived() {
    let fx = Fixture::new("req-adopt-nosubject");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
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
    out.assert_code(2, "a removed --subject");
    assert!(
        out.stderr.contains("--subject"),
        "the usage error must name the flag:\n{}",
        out.stderr
    );
    assert!(!fx.exists("claims/REQ-0002.yml"), "nothing is written");
}

#[test]
fn a_date_absent_from_the_source_and_from_git_is_a_usage_error() {
    let fx = Fixture::new("req-adopt-nodate");
    fx.write("docs/capabilities.md", CAPABILITIES);
    req(fx.path(), &["init"]).assert_code(0, "req init");
    let out = adopt(fx.path(), &["--from", "docs/capabilities.md"]);
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

    let out = adopt(fx.path(), &["--from", "docs/capabilities.md", "--write"]);
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

    let blocked = adopt(fx.path(), &["--from", "docs/capabilities.md"]);
    blocked.assert_code(2, "a changed document with no --date");
    assert!(
        blocked.stderr.contains("--date") && blocked.stderr.contains("uncommitted"),
        "the remedy is the flag that supplies a date:\n{}",
        blocked.stderr
    );

    // The authored date is the remedy, and the run then succeeds.
    let dated = adopt(
        fx.path(),
        &["--from", "docs/capabilities.md", "--date", "2026-10-21"],
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

/// Since `req.claim/2` a structured statement is drafted like any other: its parse is derived at
/// load, so there is no authored slot for the draft to contradict. What the tool must not do is
/// guess the pins — the draft carries none, and `req canon` shows the derived parse the human pins.
#[test]
fn a_structured_statement_is_drafted_with_its_parse_derived_and_no_pin_guessed() {
    let fx = Fixture::new("req-adopt-structured");
    req(fx.path(), &["init"]).assert_code(0, "req init");
    fx.write(
        "docs/mixed.md",
        "# Mixed\n\n- The system shall log every decision within 200 ms.\n- The copilot cites every source it quotes.\n",
    );
    let write = adopt(
        fx.path(),
        &["--from", "docs/mixed.md", "--date", "2026-10-21", "--write"],
    );
    write.assert_code(0, "both candidates draft");
    assert!(
        write.stdout.contains("drafted: 2") && write.stdout.contains("skipped: 0"),
        "{}",
        write.stdout
    );
    let structured = fx.read("claims/REQ-0002.yml");
    assert!(
        structured.contains("statement: \"The system shall log every decision within 200 ms.\""),
        "{structured}"
    );
    assert!(
        field(&structured, "pins:").is_none(),
        "a pin is the author's statement, never the tool's guess:\n{structured}"
    );
    let canon = req(fx.path(), &["canon"]);
    canon.assert_code(0, "the drafts load");
    let row = canon
        .stdout
        .lines()
        .find(|line| line.starts_with("REQ-0002 "))
        .unwrap_or_else(|| panic!("no REQ-0002 row:\n{}", canon.stdout));
    assert!(
        row.contains("structured") && row.contains("op LE") && row.contains("value 0.2"),
        "the parse is derived, and shown for the human to pin:\n{row}"
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
        &["--from", "docs/capabilities.md", "--date", "2026-10-21"],
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
        &["--from", "docs/more.md", "--date", "2026-10-21"],
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
