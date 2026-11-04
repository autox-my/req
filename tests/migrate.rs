//! `tools/migrate_claims.py` — the migration path proves its own output before it writes.
//!
//! The seam is the CLI: what the tool promises is a file set that the real loader accepts and
//! whose pins hold against the parse, so every check here drives the script and then the compiled
//! `req` over the corpus it wrote. The tool's own `--self-check` table asserts emitted *text*; it
//! cannot say whether that text loads, which is the defect class this file pins (M4-3).
//!
//! Oracles are independent of the tool: each expectation is stated from the statement's words
//! (`within 200 ms` is `LE 0.2 s` under T1.4/T1.5) and from the loader's own exit codes, never
//! read back from what the migrator printed.
//!
//! `python3` is required, and its absence is a **failure named here**, not a skipped check.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn migrate_py() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/migrate_claims.py")
}

/// `python3` as an absolute path, resolved before the child's `PATH` is rewritten.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/migrate_claims.py — install it first");
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

    fn both(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A scratch corpus: `aliases/subjects.yml` copied from this repository, an empty `claims/`.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("migrate-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("claims")).expect("creates the fixture");
        fs::create_dir_all(dir.join("aliases")).expect("creates the fixture aliases");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("aliases/subjects.yml"),
            dir.join("aliases/subjects.yml"),
        )
        .expect("copies the subject registry");
        Fixture(dir)
    }

    fn claims(&self) -> PathBuf {
        self.0.join("claims")
    }

    fn write(&self, rel: &str, text: &str) {
        fs::write(self.0.join(rel), text).expect("writes a fixture file");
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).expect("reads a fixture file")
    }

    /// Every file under `claims/`, name → bytes, so "nothing written" is a whole-directory fact
    /// (a stray `.migrating` temp file would show).
    fn snapshot(&self) -> Vec<(String, String)> {
        let mut found: Vec<(String, String)> = fs::read_dir(self.claims())
            .expect("lists claims/")
            .map(|entry| {
                let path = entry.expect("reads a claims/ entry").path();
                (
                    path.file_name()
                        .expect("has a name")
                        .to_string_lossy()
                        .into_owned(),
                    fs::read_to_string(&path).expect("reads a claim"),
                )
            })
            .collect();
        found.sort();
        found
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The migrator, run from `cwd` with `path_value` as its `PATH`.
fn migrate_with_path(cwd: &Path, args: &[&str], path_value: &std::ffi::OsStr) -> Outcome {
    let output = Command::new(python3())
        .arg(migrate_py())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .env("PATH", path_value)
        .output()
        .expect("runs tools/migrate_claims.py");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The migrator with the built `req` first on `PATH`.
fn migrate(cwd: &Path, args: &[&str]) -> Outcome {
    let mut path_value = req_bin()
        .parent()
        .expect("the binary has a parent")
        .as_os_str()
        .to_os_string();
    if let Some(existing) = std::env::var_os("PATH") {
        path_value.push(":");
        path_value.push(existing);
    }
    migrate_with_path(cwd, args, &path_value)
}

fn req(root: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A `req.claim/1` quantity claim. The statement says `within 200 ms`: `LE`, `0.2`, `s` (T1.4
/// rescales the unit, T1.5 reads `within` as `LE`), whatever the authored slots below claim.
fn v1_quantity(claim_id: &str, value: &str) -> String {
    format!(
        "schema: req.claim/1\n\
claim_id: {claim_id}\n\
statement: >-\n  The req tool shall respond to a health check within 200 ms.\n\
modality: SHALL\n\
subject: REQ_TOOL\n\
attribute: latency\n\
operator: LE\n\
value: \"{value}\"\n\
unit: s\n\
conditions: []\n\
applicability: []\n\
exception: null\n\
temporal: null\n\
quantifier: null\n\
unstructured: false\n\
tests: []\n\
evidence: process\n\
canonical:\n  rules_v: 6\n  subject_registry: 1\n  form: null\n\
address: null\n\
rationale:\n  source: docs/design.md\n  date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\n\
relations: []\n"
    )
}

/// The gate's `pins_hold` policy object, from `--format json` (a `corpus root:` line precedes
/// the document on stdout).
fn pins_hold(root: &Path) -> (String, u64, Vec<String>) {
    let gate = req(root, &["gate", "--format", "json"]);
    let start = gate
        .stdout
        .find('{')
        .expect("the gate prints a JSON document");
    let doc: serde_json::Value =
        serde_json::from_str(&gate.stdout[start..]).expect("the gate document parses");
    let policy = doc["policies"]
        .as_array()
        .expect("policies is an array")
        .iter()
        .find(|p| p["policy"] == "pins_hold")
        .expect("the gate carries a pins_hold policy")
        .clone();
    let details = policy["details"]
        .as_array()
        .expect("details is an array")
        .iter()
        .map(|d| d.as_str().expect("a detail is a string").to_string())
        .collect();
    (
        policy["outcome"]
            .as_str()
            .expect("outcome is a string")
            .to_string(),
        policy["checked"]
            .as_u64()
            .expect("pins_hold carries checked"),
        details,
    )
}

/// (a) A valid v1 corpus migrates, the loader accepts it and the three pins its slots implied
/// (value, operator, unit) hold against the parse.
#[test]
fn a_valid_corpus_migrates_loads_and_its_pins_hold() {
    let fixture = Fixture::new("valid");
    fixture.write("claims/REQ-0001.yml", &v1_quantity("REQ-0001", "0.2"));

    let done = migrate(&fixture.0, &["--write", "claims"]);
    done.assert_code(0, "a valid v1 corpus migrates");
    assert!(
        done.stdout.contains("migrated 1 file(s)"),
        "the final line says what was done: {}",
        done.stdout
    );

    let text = fixture.read("claims/REQ-0001.yml");
    assert!(
        text.starts_with("schema: req.claim/2\n"),
        "rewritten to v2: {text}"
    );
    req(&fixture.0, &["canon"]).assert_code(0, "the loader accepts the migrated corpus");
    let (outcome, checked, details) = pins_hold(&fixture.0);
    assert_eq!(outcome, "pass", "the pins hold: {details:?}");
    assert_eq!(checked, 3, "value, operator and unit were each judged");
}

/// (b) THE RED TEST. A v1 file whose authored value contradicts its statement (`0.5` against
/// `within 200 ms`) would be written as a pin the parse refutes. It is refused, nothing is
/// written, and the message names the pin.
#[test]
fn a_pin_the_statement_contradicts_is_refused_and_nothing_is_written() {
    let fixture = Fixture::new("contradiction");
    fixture.write("claims/REQ-0001.yml", &v1_quantity("REQ-0001", "0.5"));
    let before = fixture.snapshot();

    let done = migrate(&fixture.0, &["--write", "claims"]);
    done.assert_code(1, "a contradicted pin is a refusal");
    assert_eq!(
        fixture.snapshot(),
        before,
        "a refused migration writes nothing"
    );
    let said = done.both();
    assert!(said.contains("REQ-0001"), "names the claim: {said}");
    assert!(said.contains("pin value 0.5"), "names the pin: {said}");
    assert!(said.contains("0.2"), "names what the parse says: {said}");
    assert!(
        !said.contains("migrated 1 file"),
        "never claims a migration: {said}"
    );
}

/// (c) A file the migrator's own refusals do not catch but the loader rejects: `evidence:
/// process` beside linked `tests:` migrates byte for byte (both are kept blocks) and the loader
/// refuses naming both (exit 3). The migrator surfaces the loader's message and writes nothing.
#[test]
fn a_file_the_loader_rejects_is_refused_with_the_loaders_message() {
    let fixture = Fixture::new("loader");
    let text = v1_quantity("REQ-0001", "0.2").replace(
        "tests: []\nevidence: process\n",
        "tests:\n  - \"x::t\"\nevidence: process\n",
    );
    assert!(
        text.contains("- \"x::t\""),
        "the fixture really links a test"
    );
    fixture.write("claims/REQ-0001.yml", &text);
    let before = fixture.snapshot();

    // The oracle: the real loader rejects exactly this file once migrated by hand.
    let by_hand = Fixture::new("loader-by-hand");
    by_hand.write(
        "claims/REQ-0001.yml",
        "schema: req.claim/2\nclaim_id: REQ-0001\nstatement: >-\n  The req tool shall respond to a \
         health check within 200 ms.\napplicability: []\ntests:\n  - \"x::t\"\nevidence: process\n\
         rationale:\n  source: docs/design.md\n  date: 2026-10-07\n  rejected_alternatives: []\n  \
         assumptions: []\nrelations: []\n",
    );
    let loader = req(&by_hand.0, &["canon"]);
    assert_ne!(
        loader.code,
        0,
        "the premise: the loader rejects this file: {}",
        loader.both()
    );

    let done = migrate(&fixture.0, &["--write", "claims"]);
    done.assert_code(1, "a loader refusal is a refusal");
    assert_eq!(fixture.snapshot(), before, "nothing is written");
    // The loader's message past its path prefix (the path differs: the migrator judges a scratch copy).
    let loader_says = loader.both();
    let message = loader_says
        .split(".yml: ")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .expect("the loader's message follows the file path");
    assert!(
        message.contains("evidence: process"),
        "the premise: the loader names the contradiction: {loader_says}"
    );
    assert!(
        done.both().contains(message),
        "the loader's own message is printed:\nloader: {message}\nmigrator: {}",
        done.both()
    );
}

/// (d) No `req` on `PATH` → exit 3, never a silent pass; `--no-verify` migrates and says so.
#[test]
fn without_req_it_is_unverifiable_and_no_verify_says_not_verified() {
    let fixture = Fixture::new("noreq");
    fixture.write("claims/REQ-0001.yml", &v1_quantity("REQ-0001", "0.2"));
    let before = fixture.snapshot();
    let empty = Fixture::new("empty-path");
    let empty_path = empty.0.as_os_str();

    let blocked = migrate_with_path(&fixture.0, &["--write", "claims"], empty_path);
    blocked.assert_code(3, "no req on PATH is UNVERIFIABLE");
    assert_eq!(
        fixture.snapshot(),
        before,
        "nothing is written without the proof"
    );
    assert!(
        blocked.both().contains("--no-verify"),
        "says how to proceed: {}",
        blocked.both()
    );

    let waived = migrate_with_path(
        &fixture.0,
        &["--write", "--no-verify", "claims"],
        empty_path,
    );
    waived.assert_code(0, "--no-verify migrates without the check");
    assert!(
        waived.stdout.contains("NOT VERIFIED"),
        "says it was not verified: {}",
        waived.stdout
    );
    let last = waived
        .stdout
        .trim_end()
        .lines()
        .last()
        .expect("has a final line");
    assert!(
        last.contains("NOT VERIFIED"),
        "the final line says so: {last}"
    );
    assert!(
        fixture
            .read("claims/REQ-0001.yml")
            .starts_with("schema: req.claim/2\n")
    );
}

/// (e) A dry run writes nothing, and it also runs the check: the contradiction is refused there
/// too (exit 1), a valid corpus passes (exit 0) with files untouched.
#[test]
fn a_dry_run_writes_nothing_and_also_runs_the_check() {
    let bad = Fixture::new("dry-bad");
    bad.write("claims/REQ-0001.yml", &v1_quantity("REQ-0001", "0.5"));
    let before = bad.snapshot();
    let refused = migrate(&bad.0, &["claims"]);
    refused.assert_code(1, "a dry run runs the check");
    assert!(
        refused.both().contains("pin value 0.5"),
        "{}",
        refused.both()
    );
    assert_eq!(bad.snapshot(), before);

    let good = Fixture::new("dry-good");
    good.write("claims/REQ-0001.yml", &v1_quantity("REQ-0001", "0.2"));
    let before = good.snapshot();
    let ok = migrate(&good.0, &["claims"]);
    ok.assert_code(0, "a valid dry run passes");
    assert!(ok.stdout.contains("dry run"), "{}", ok.stdout);
    assert_eq!(good.snapshot(), before, "a dry run writes nothing");
}

/// A claim carrying a selector is undecided under a bare gate, so `pins_hold` would pass over
/// zero pins. The migrator must not let that be a pass: the contradiction is still refused.
#[test]
fn a_claim_with_an_applicability_selector_has_its_pins_judged_too() {
    let fixture = Fixture::new("selector");
    let text = v1_quantity("REQ-0001", "0.5")
        .replace("applicability: []", "applicability:\n  - set == one");
    fixture.write("claims/REQ-0001.yml", &text);
    let before = fixture.snapshot();
    let done = migrate(&fixture.0, &["--write", "claims"]);
    done.assert_code(1, "the contradiction is found despite the selector");
    assert_eq!(fixture.snapshot(), before);
}
