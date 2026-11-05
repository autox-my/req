//! `tools/needs_import.py` — the Sphinx-Needs reader (issue #124, ADR 0027): a `needs.json` in,
//! **legal, unratified** claim drafts out, and a report that accounts for every need in the file.
//!
//! The seam is the CLI, as in `tests/adopt.rs`: the script is driven as a subprocess and then the
//! compiled `req` reads what it wrote. The fixtures are the two committed excerpts of real exports
//! in `tests/fixtures/needs/` (read-only); a test that needs another shape writes a modified copy
//! into its own temporary corpus and says what it changed.
//!
//! The oracle for "what is drafted, what is skipped and why, what is not selected" is **written
//! here, from the fixture JSON** (`oracle`), in another language and with its own scanner for the
//! reStructuredText cases. It is never read back from the script's report: a report compared with
//! itself would prove nothing. The statements are read back through `req render`, so the check
//! that a draft carries its statement does not go through the script's own quoting either.
//!
//! `python3` is required, and its absence is a **failure named here**, never a skip.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value, json};

const ORIGIN: &str = "https://example.test/export/needs.json";
const DATE: &str = "2026-11-05";
const SCORE_TYPES: [&str; 4] = ["stkh_req", "feat_req", "comp_req", "aou_req"];
const DEMO_TYPES: [&str; 4] = ["req", "sysreq", "fsr", "swreq"];
const SCORE: &str = "score-excerpt.needs.json";
const DEMO: &str = "sphinx-needs-demo-excerpt.needs.json";

// --- the harness (the same shape as tests/adopt.rs) ---------------------------------------------

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/needs_import.py")
}

/// `python3` resolved from the test process's own `PATH`, as an absolute path: the child's `PATH`
/// is rewritten (to put the built `req` on it), and a program name resolved through the rewritten
/// value would be a second, unintended dependency on it.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "python3 is required for tools/needs_import.py — install it first \
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

static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A base directory the suite controls, outside any checkout (see `tests/adopt.rs::fixture_base`).
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
        "no fixture base outside a checkout: TMPDIR, /tmp and /var/tmp all sit under claims/ or .git"
    );
}

/// A directory with no `claims/` and no `.git` ancestor: the script's root resolution walks
/// outwards from the working directory only to refuse, and a fixture that inherited a corpus
/// would check something other than what it says.
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
                "the fixture assumes it is outside a git repository, but {} holds .git",
                found.display()
            );
        }
        Fixture(dir)
    }

    /// A fixture with `req init` run in it and `needs.json` holding `export`.
    fn with_export(name: &str, export: &str) -> Self {
        let fx = Fixture::new(name);
        req(fx.path(), &["init"]).assert_code(0, "req init");
        fx.write("needs.json", export);
        fx
    }

    fn path(&self) -> &Path {
        &self.0
    }

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

    /// Every `claims/*.yml`, by name, with its exact bytes: a write is a new key, an overwrite a
    /// changed value, a refusal an equal map.
    fn claims(&self) -> BTreeMap<String, String> {
        let mut found = BTreeMap::new();
        for entry in fs::read_dir(self.0.join("claims")).expect("the claims directory") {
            let entry = entry.expect("a claim entry");
            if entry.path().extension().is_some_and(|e| e == "yml") {
                found.insert(
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read_to_string(entry.path()).expect("reads a claim"),
                );
            }
        }
        found
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn req(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    outcome(&output)
}

fn outcome(output: &std::process::Output) -> Outcome {
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// How one run of the script is set up beyond its arguments.
#[derive(Default)]
struct Env<'a> {
    /// `REQ_ROOT`, when the run names its corpus that way.
    req_root: Option<&'a Path>,
    /// A directory placed **before** the built `req` on `PATH` (a fake `req`).
    path_first: Option<&'a Path>,
}

/// `tools/needs_import.py`, run in `cwd` with the built `req` on `PATH`: the script takes the
/// claim schema and `rules_v` from `req --version`, never from a literal.
fn import_in(cwd: &Path, args: &[String], env: &Env) -> Outcome {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(first) = env.path_first {
        dirs.push(first.to_path_buf());
    }
    dirs.push(
        req_bin()
            .parent()
            .expect("the binary has a parent")
            .to_path_buf(),
    );
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = Command::new(python3());
    command
        .arg(script())
        .args(args)
        .current_dir(cwd)
        .env("PATH", std::env::join_paths(dirs).expect("a PATH"));
    match env.req_root {
        Some(root) => command.env("REQ_ROOT", root),
        None => command.env_remove("REQ_ROOT"),
    };
    outcome(&command.output().expect("runs tools/needs_import.py"))
}

fn import(cwd: &Path, args: &[String]) -> Outcome {
    import_in(cwd, args, &Env::default())
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

/// The required arguments, with `--type` for each of `types`.
fn base(types: &[&str]) -> Vec<String> {
    let mut args = strings(&["--from", "needs.json", "--origin", ORIGIN, "--date", DATE]);
    for ty in types {
        args.push("--type".to_string());
        args.push((*ty).to_string());
    }
    args
}

fn plus(mut args: Vec<String>, more: &[&str]) -> Vec<String> {
    args.extend(strings(more));
    args
}

fn fixture_text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/needs")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("reads {}: {e}", path.display()))
}

fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture_text(name)).expect("the fixture is JSON")
}

// --- the oracle: computed here, from the fixture JSON --------------------------------------------

fn needs_of(export: &Value) -> &Map<String, Value> {
    let current = export["current_version"].as_str().expect("current_version");
    export["versions"][current]["needs"]
        .as_object()
        .expect("a needs mapping")
}

fn needs_mut(export: &mut Value) -> &mut Map<String, Value> {
    let current = export["current_version"]
        .as_str()
        .expect("current_version")
        .to_string();
    export["versions"][current.as_str()]["needs"]
        .as_object_mut()
        .expect("a needs mapping")
}

/// Whether `text` holds an inline role: `:name:` immediately followed by a backtick. Written as a
/// scan over characters, not a pattern, so it shares nothing with the script's expression.
fn has_role(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for (at, ch) in chars.iter().enumerate() {
        // The backtick, preceded by the role's closing colon.
        if *ch != '`' || at < 2 || chars[at - 1] != ':' {
            continue;
        }
        // Walk back over the name; it must be non-empty, start with a letter, and be opened by a colon.
        let mut start = at - 1;
        while start > 0 && (chars[start - 1].is_alphanumeric() || "_.+-".contains(chars[start - 1]))
        {
            start -= 1;
        }
        if start > 0 && start < at - 1 && chars[start - 1] == ':' && chars[start].is_alphabetic() {
            return true;
        }
    }
    false
}

enum Verdict {
    NotSelected(String),
    Skip(&'static str),
    Draft(String),
}

fn judge(need: &Value, types: &[&str], statuses: &[&str]) -> Verdict {
    let ty = need["type"].as_str().unwrap_or("(none)");
    let status_ok = statuses.is_empty()
        || need["status"]
            .as_str()
            .is_some_and(|s| statuses.contains(&s));
    if !types.contains(&ty) || !status_ok {
        return Verdict::NotSelected(ty.to_string());
    }
    if need["is_external"] == Value::Bool(true) {
        return Verdict::Skip("external");
    }
    let content = need["content"].as_str().unwrap_or("").trim().to_string();
    if content.is_empty() {
        return Verdict::Skip("no content");
    }
    let lines: Vec<&str> = content.split('\n').collect();
    if lines.iter().any(|line| line.trim().starts_with("..")) {
        return Verdict::Skip("rst directive");
    }
    if lines.iter().any(|line| line.trim().is_empty()) {
        return Verdict::Skip("multi-paragraph");
    }
    if has_role(&content) {
        return Verdict::Skip("rst role");
    }
    Verdict::Draft(
        lines
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

struct Expected {
    total: usize,
    drafted: BTreeMap<String, String>,
    skipped: BTreeMap<String, &'static str>,
    not_selected: BTreeMap<String, usize>,
}

impl Expected {
    fn not_selected_total(&self) -> usize {
        self.not_selected.values().sum()
    }
}

fn oracle(export: &Value, types: &[&str], statuses: &[&str]) -> Expected {
    let needs = needs_of(export);
    let mut expected = Expected {
        total: needs.len(),
        drafted: BTreeMap::new(),
        skipped: BTreeMap::new(),
        not_selected: BTreeMap::new(),
    };
    for (id, need) in needs {
        match judge(need, types, statuses) {
            Verdict::NotSelected(ty) => *expected.not_selected.entry(ty).or_insert(0) += 1,
            Verdict::Skip(reason) => {
                expected.skipped.insert(id.clone(), reason);
            }
            Verdict::Draft(statement) => {
                expected.drafted.insert(id.clone(), statement);
            }
        }
    }
    expected
}

/// The need ids of a pristine, pretty-printed fixture in the order the file lists them.
fn file_order(raw: &str, ids: &BTreeSet<String>) -> Vec<String> {
    let mut found: Vec<(usize, String)> = ids
        .iter()
        .map(|id| {
            let key = format!("\n        \"{id}\": {{");
            (
                raw.find(&key)
                    .unwrap_or_else(|| panic!("{id} is a key of the fixture")),
                id.clone(),
            )
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, id)| id).collect()
}

// --- reading the report ----------------------------------------------------------------------------

struct Draft {
    claim: String,
    need: String,
    location: String,
}

struct Recon {
    needs: usize,
    drafted: usize,
    skipped: usize,
    not_selected: usize,
    by_type: BTreeMap<String, usize>,
}

struct Report {
    drafts: Vec<Draft>,
    skips: Vec<(String, String)>,
    recon: Recon,
    not_carried: Option<String>,
    last: String,
}

fn number(text: &str) -> usize {
    text.trim()
        .parse()
        .unwrap_or_else(|_| panic!("a count, got {text:?}"))
}

fn parse_recon(line: &str) -> Recon {
    let rest = line
        .strip_prefix("needs ")
        .unwrap_or_else(|| panic!("the last line is the reconciling line, got {line:?}"));
    let (needs, rest) = rest.split_once(" = drafted ").expect("`= drafted`");
    let (drafted, rest) = rest.split_once(" + skipped ").expect("`+ skipped`");
    let (skipped, rest) = rest
        .split_once(" + not selected ")
        .expect("`+ not selected`");
    let (not_selected, tail) = match rest.split_once(" (by type: ") {
        Some((count, tail)) => (
            count,
            Some(tail.strip_suffix(')').expect("a closing parenthesis")),
        ),
        None => (rest, None),
    };
    let mut by_type = BTreeMap::new();
    for entry in tail.into_iter().flat_map(|tail| tail.split(", ")) {
        let (name, count) = entry.rsplit_once(' ').expect("`<type> <count>`");
        by_type.insert(name.to_string(), number(count));
    }
    Recon {
        needs: number(needs),
        drafted: number(drafted),
        skipped: number(skipped),
        not_selected: number(not_selected),
        by_type,
    }
}

fn parse_report(stdout: &str) -> Report {
    let mut drafts = Vec::new();
    let mut skips = Vec::new();
    let mut not_carried = None;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("draft ") {
            let parts: Vec<&str> = rest.splitn(3, "  ").collect();
            assert_eq!(
                parts.len(),
                3,
                "a draft line is `draft REQ-NNNN  <need id>  <docname>:<lineno>`: {line:?}"
            );
            drafts.push(Draft {
                claim: parts[0].to_string(),
                need: parts[1].to_string(),
                location: parts[2].to_string(),
            });
        } else if let Some(rest) = line.strip_prefix("skip ") {
            let parts: Vec<&str> = rest.splitn(2, "  ").collect();
            assert_eq!(
                parts.len(),
                2,
                "a skip line is `skip <need id>  <reason>`: {line:?}"
            );
            skips.push((parts[0].to_string(), parts[1].to_string()));
        } else if line.starts_with("not carried") {
            not_carried = Some(line.to_string());
        }
    }
    let last = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .expect("a report")
        .to_string();
    Report {
        drafts,
        skips,
        recon: parse_recon(&last),
        not_carried,
        last,
    }
}

/// The rows `req render` prints: claim id → (address, statement).
fn render_rows(stdout: &str) -> BTreeMap<String, (String, String)> {
    let mut rows = BTreeMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("- REQ-") else {
            continue;
        };
        let parts: Vec<&str> = rest.splitn(4, "  ").collect();
        assert_eq!(
            parts.len(),
            4,
            "a render row is `- <id>  <address>  <modality>  <statement>`: {line:?}"
        );
        rows.insert(
            format!("REQ-{}", parts[0]),
            (parts[1].to_string(), parts[3].to_string()),
        );
    }
    rows
}

/// One claim file's field, by exact whole-line prefix.
fn field<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines().find(|line| line.starts_with(prefix))
}

/// The report agrees with the oracle: same drafts, same skips with the same reasons, a
/// reconciling line that adds up and breaks the remainder down by type as the oracle does.
fn assert_report_matches(report: &Report, expected: &Expected, what: &str) {
    let drafted: BTreeSet<&String> = report.drafts.iter().map(|d| &d.need).collect();
    assert_eq!(
        drafted,
        expected.drafted.keys().collect::<BTreeSet<_>>(),
        "{what}: the drafted needs"
    );
    assert_eq!(
        report.drafts.len(),
        expected.drafted.len(),
        "{what}: one draft line per draft"
    );
    let skips: BTreeMap<&str, &str> = report
        .skips
        .iter()
        .map(|(id, why)| (id.as_str(), why.as_str()))
        .collect();
    let wanted: BTreeMap<&str, &str> = expected
        .skipped
        .iter()
        .map(|(id, why)| (id.as_str(), *why))
        .collect();
    assert_eq!(skips, wanted, "{what}: each skip is named with its reason");
    assert_eq!(
        report.skips.len(),
        expected.skipped.len(),
        "{what}: one skip line per skip"
    );
    let recon = &report.recon;
    assert_eq!(
        recon.needs, expected.total,
        "{what}: needs counts the file's mapping"
    );
    assert_eq!(recon.drafted, expected.drafted.len(), "{what}: drafted");
    assert_eq!(recon.skipped, expected.skipped.len(), "{what}: skipped");
    assert_eq!(
        recon.not_selected,
        expected.not_selected_total(),
        "{what}: not selected"
    );
    assert_eq!(
        recon.needs,
        recon.drafted + recon.skipped + recon.not_selected,
        "{what}: the last line must add up"
    );
    assert_eq!(
        recon.by_type, expected.not_selected,
        "{what}: not selected, by type"
    );
}

/// Write the export and run end to end: `--write`, then `req canon` and `req render` over the
/// corpus. Returns the fixture, the report and the rendered rows.
fn end_to_end(
    export_name: &str,
    types: &[&str],
) -> (Fixture, Report, BTreeMap<String, (String, String)>) {
    let raw = fixture_text(export_name);
    let expected = oracle(&fixture_json(export_name), types, &[]);
    let fx = Fixture::with_export("req-needs-e2e", &raw);
    let out = import(fx.path(), &plus(base(types), &["--write"]));
    out.assert_code(0, &format!("{export_name} --write"));
    let report = parse_report(&out.stdout);
    assert_report_matches(&report, &expected, export_name);

    // Every drafted need is a claim whose statement `req` itself reads back as the oracle's.
    let canon = req(fx.path(), &["canon"]);
    canon.assert_code(0, "req canon over the drafts");
    assert!(
        canon
            .stdout
            .contains(&format!("claims: {} (", expected.drafted.len() + 1))
            && canon.stdout.contains(", 0 flagged"),
        "the starter plus {} drafts load under the strict loader, and no pin is flagged:\n{}",
        expected.drafted.len(),
        canon.stdout
    );
    let render = req(fx.path(), &["render"]);
    render.assert_code(0, "req render over the drafts");
    let rows = render_rows(&render.stdout);
    for draft in &report.drafts {
        let (_address, statement) = rows
            .get(&draft.claim)
            .unwrap_or_else(|| panic!("{} renders:\n{}", draft.claim, render.stdout));
        assert_eq!(
            statement, &expected.drafted[&draft.need],
            "{} ({}) carries the statement the oracle derives from `content`",
            draft.claim, draft.need
        );
    }
    (fx, report, rows)
}

// --- (a) S-CORE end to end -----------------------------------------------------------------------------

#[test]
fn s_core_excerpt_end_to_end() {
    let (_fx, report, _rows) = end_to_end(SCORE, &SCORE_TYPES);
    // The oracle is itself held against the fixture: these facts were read off the excerpt by hand.
    let expected = oracle(&fixture_json(SCORE), &SCORE_TYPES, &[]);
    assert_eq!(
        expected.drafted.len(),
        5,
        "five plain-paragraph requirements in the excerpt"
    );
    assert_eq!(expected.skipped["aou_req__os__nonroot"], "multi-paragraph");
    assert_eq!(
        expected.skipped["aou_req__feo__on_target_crates"],
        "multi-paragraph"
    );
    assert_eq!(
        expected.skipped["stkh_req__dependability__firewall"],
        "no content"
    );
    assert_eq!(expected.not_selected.get("document"), Some(&1));
    assert_eq!(expected.not_selected.get("mod_view_sta"), Some(&1));
    assert_eq!(report.recon.needs, 10, "ten needs in the excerpt");

    // The draft line carries `docname:lineno`, read from the need.
    let communication = report
        .drafts
        .iter()
        .find(|d| d.need == "aou_req__communication__1")
        .expect("aou_req__communication__1 is drafted");
    assert_eq!(
        communication.location,
        "modules/communication/docs/requirements/aou_req:26"
    );
}

// --- (b) the Sphinx-Needs demo end to end -------------------------------------------------------------

#[test]
fn demo_excerpt_end_to_end() {
    let (_fx, report, rows) = end_to_end(DEMO, &DEMO_TYPES);
    let skips: BTreeMap<&str, &str> = report
        .skips
        .iter()
        .map(|(id, why)| (id.as_str(), why.as_str()))
        .collect();
    assert_eq!(skips.get("EX_REQ_001"), Some(&"rst directive"));
    assert_eq!(skips.get("GH_ISSUE_12"), Some(&"rst directive"));
    assert_eq!(skips.get("REQ_1_1_imp"), Some(&"no content"));
    assert_eq!(
        report.recon.by_type.get("arch"),
        Some(&1),
        "ARCH_001 is not a named type"
    );

    let sysreq = report
        .drafts
        .iter()
        .find(|d| d.need == "SYSREQ_BRAKE_01")
        .expect("SYSREQ_BRAKE_01 is drafted");
    let (_addr, statement) = &rows[&sysreq.claim];
    assert_eq!(
        statement,
        "The brake system shall implement X-split dual hydraulic circuits (FL-RR, FR-RL) with independent \
         pressure generation and control. Each circuit shall be capable of achieving minimum 0.3g \
         deceleration independently. Electronic control units are dual-redundant with independent power \
         supplies.",
        "the wrapped lines are joined with one space"
    );
}

// --- (c) dry run is the default; claim ids are req's ----------------------------------------------------

#[test]
fn a_dry_run_writes_nothing_and_a_write_creates_exactly_the_reported_files() {
    let raw = fixture_text(SCORE);
    let fx = Fixture::with_export("req-needs-dry", &raw);
    let before = fx.claims();

    let dry = import(fx.path(), &base(&SCORE_TYPES));
    dry.assert_code(0, "the dry run");
    assert_eq!(fx.claims(), before, "a dry run writes nothing");
    assert!(
        dry.stdout.contains("dry run"),
        "the report says it was a dry run:\n{}",
        dry.stdout
    );
    let dry_report = parse_report(&dry.stdout);

    let wet = import(fx.path(), &plus(base(&SCORE_TYPES), &["--write"]));
    wet.assert_code(0, "the write run");
    let report = parse_report(&wet.stdout);

    // Exactly the reported files, no others: the starter plus one file per draft line.
    let mut wanted: BTreeSet<String> = report
        .drafts
        .iter()
        .map(|d| format!("{}.yml", d.claim))
        .collect();
    wanted.insert("REQ-0001.yml".to_string());
    let after = fx.claims();
    assert_eq!(
        after.keys().cloned().collect::<BTreeSet<_>>(),
        wanted,
        "the write created exactly the reported files"
    );

    // The ids are the next free `REQ-NNNN`, handed out in file order, and the dry run named the same.
    let drafted: BTreeSet<String> = oracle(&fixture_json(SCORE), &SCORE_TYPES, &[])
        .drafted
        .into_keys()
        .collect();
    let order = file_order(&raw, &drafted);
    let allocated: Vec<(String, String)> = report
        .drafts
        .iter()
        .map(|d| (d.claim.clone(), d.need.clone()))
        .collect();
    let numbered: Vec<(String, String)> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (format!("REQ-{:04}", i + 2), id.clone()))
        .collect();
    assert_eq!(
        allocated, numbered,
        "ids follow the corpus's next free number, in the file's order"
    );
    let promised: Vec<(String, String)> = dry_report
        .drafts
        .iter()
        .map(|d| (d.claim.clone(), d.need.clone()))
        .collect();
    assert_eq!(
        promised, allocated,
        "the dry run names the ids the write then uses"
    );
}

#[test]
fn the_claim_id_is_never_derived_from_the_need_id() {
    // A modified export: needs whose own ids look like claim ids (`REQ-0099`, and `REQ-0001`, which
    // the starter claim already holds), in a corpus whose highest number is 7.
    let export = json!({
        "current_version": "1",
        "versions": {"1": {"needs": {
            "REQ-0001": {"type": "req", "content": "The first one shall hold.", "docname": "a", "lineno": 1},
            "REQ-0099": {"type": "req", "content": "The second one shall hold.", "docname": "a", "lineno": 2},
        }}}
    })
    .to_string();
    let fx = Fixture::with_export("req-needs-ids", &export);
    fx.write(
        "claims/REQ-0007.yml",
        &format!(
            "schema: {}\nclaim_id: REQ-0007\nstatement: An existing claim.\napplicability: []\nrelations: []\n\
             rationale:\n  source: x\n  date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\n",
            req::canon::claim_schema()
        ),
    );
    import(fx.path(), &plus(base(&["req"]), &["--write"])).assert_code(0, "the write run");
    let claims = fx.claims();
    let new: Vec<&String> = claims
        .keys()
        .filter(|name| !["REQ-0001.yml", "REQ-0007.yml"].contains(&name.as_str()))
        .collect();
    assert_eq!(
        new,
        ["REQ-0008.yml", "REQ-0009.yml"],
        "one past the highest number in use"
    );
    assert!(
        claims["REQ-0008.yml"].contains("claim_id: REQ-0008"),
        "the claim id is req's:\n{}",
        claims["REQ-0008.yml"]
    );
    assert!(
        claims["REQ-0008.yml"].contains(&format!("source: \"{ORIGIN}#REQ-0001\"")),
        "the need id is the provenance, not the id:\n{}",
        claims["REQ-0008.yml"]
    );
    assert!(claims["REQ-0009.yml"].contains(&format!("source: \"{ORIGIN}#REQ-0099\"")));
    // The starter claim was not touched by a need that happens to be called REQ-0001.
    assert!(
        claims["REQ-0001.yml"].contains("The API shall respond"),
        "REQ-0001 is still the starter"
    );
}

// --- (d) provenance ---------------------------------------------------------------------------------------------

#[test]
fn provenance_is_origin_and_need_id_and_a_second_write_over_the_same_origin_refuses() {
    let (fx, report, _rows) = end_to_end(SCORE, &SCORE_TYPES);
    for draft in &report.drafts {
        let text = fx.read(&format!("claims/{}.yml", draft.claim));
        assert_eq!(
            field(&text, "  source: "),
            Some(format!("  source: \"{ORIGIN}#{}\"", draft.need).as_str()),
            "{} records `<origin>#<need id>`:\n{text}",
            draft.claim
        );
        assert!(
            text.contains(&format!("date: {DATE}")),
            "{} carries the authored date:\n{text}",
            draft.claim
        );
        let header = text
            .lines()
            .take_while(|line| line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            header.contains(&draft.location) && header.contains("needs.json"),
            "{}: the header comment names the export and `docname:lineno` ({}):\n{header}",
            draft.claim,
            draft.location
        );
    }

    let before = fx.claims();
    for extra in [&["--write"][..], &[][..]] {
        let again = import(fx.path(), &plus(base(&SCORE_TYPES), extra));
        again.assert_code(1, "the same origin again (dry run or write)");
        assert!(
            again.stderr.contains("already drafted")
                && again.stderr.contains(&report.drafts[0].claim),
            "the refusal says so and names the claims:\n{}",
            again.stderr
        );
        assert_eq!(
            fx.claims(),
            before,
            "a refused run writes and overwrites nothing"
        );
    }

    // The same file under a different origin is a different provenance: allowed.
    let mirror = "https://example.test/mirror/needs.json";
    let mut args = base(&SCORE_TYPES);
    args[3] = mirror.to_string();
    let other = import(fx.path(), &plus(args, &["--write"]));
    other.assert_code(0, "the same file under another origin");
    assert_eq!(
        fx.claims().len(),
        before.len() + report.drafts.len(),
        "a second set of drafts"
    );
    let second = parse_report(&other.stdout);
    let text = fx.read(&format!("claims/{}.yml", second.drafts[0].claim));
    assert!(
        text.contains(&format!("source: \"{mirror}#{}\"", second.drafts[0].need)),
        "{text}"
    );
}

#[test]
fn a_need_already_drafted_is_refused_but_a_skipped_one_never_is() {
    // Only a selected, draftable need counts: a claim whose source is a need the script would
    // skip anyway (here `no content`) is not an earlier draft of anything.
    let export = json!({
        "current_version": "1",
        "versions": {"1": {"needs": {
            "N_EMPTY": {"type": "req", "content": "", "docname": "a", "lineno": 1},
            "N_PLAIN": {"type": "req", "content": "The plain one shall hold.", "docname": "a", "lineno": 2},
        }}}
    })
    .to_string();
    let fx = Fixture::with_export("req-needs-skipped-source", &export);
    fx.write(
        "claims/REQ-0002.yml",
        &format!(
            "schema: {}\nclaim_id: REQ-0002\nstatement: A claim drafted earlier.\napplicability: []\nrelations: []\n\
             rationale:\n  source: \"{ORIGIN}#N_EMPTY\"\n  date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\n",
            req::canon::claim_schema()
        ),
    );
    import(fx.path(), &plus(base(&["req"]), &["--write"]))
        .assert_code(0, "N_EMPTY was skipped, so it is no obstacle");
    let again = import(fx.path(), &base(&["req"]));
    again.assert_code(1, "N_PLAIN is now already drafted");
    assert!(
        again.stderr.contains("N_PLAIN"),
        "the refusal names the need:\n{}",
        again.stderr
    );
}

// --- (e) reconciliation --------------------------------------------------------------------------------------------

#[test]
fn the_last_line_adds_up_on_both_excerpts_and_on_a_copy_with_a_need_removed() {
    for (name, types) in [(SCORE, &SCORE_TYPES), (DEMO, &DEMO_TYPES)] {
        let fx = Fixture::with_export("req-needs-recon", &fixture_text(name));
        let out = import(fx.path(), &base(types));
        out.assert_code(0, name);
        let report = parse_report(&out.stdout);
        assert_report_matches(&report, &oracle(&fixture_json(name), types, &[]), name);
        assert!(
            out.stdout.trim_end().lines().last() == Some(report.last.as_str()),
            "the reconciling line is the report's last line"
        );
    }

    // Not selected is broken down by type.
    let fx = Fixture::with_export("req-needs-recon-types", &fixture_text(SCORE));
    let report = parse_report(&import(fx.path(), &base(&SCORE_TYPES)).stdout);
    assert_eq!(
        report.recon.by_type,
        BTreeMap::from([("document".to_string(), 1), ("mod_view_sta".to_string(), 1)])
    );
    assert!(
        report.last.contains("document 1") && report.last.contains("mod_view_sta 1"),
        "{}",
        report.last
    );

    // A copy with a need removed, and with count keys that no longer tell the truth (the reader
    // counts the mapping and never trusts `needs_amount`).
    let mut export = fixture_json(SCORE);
    needs_mut(&mut export).remove("aou_req__communication__1");
    export["needs_amount"] = json!(999);
    export["versions"]["0.1"]["needs_amount"] = json!(10);
    export["versions"]["0.1"]["filters_amount"] = json!(3);
    let fx = Fixture::with_export("req-needs-recon-removed", &export.to_string());
    let out = import(fx.path(), &base(&SCORE_TYPES));
    out.assert_code(0, "a copy with a need removed and stale counts");
    let report = parse_report(&out.stdout);
    let expected = oracle(&export, &SCORE_TYPES, &[]);
    assert_eq!(expected.total, 9, "one need fewer than the excerpt");
    assert_report_matches(&report, &expected, "the copy with a need removed");
    assert_eq!(report.recon.needs, 9, "needs_amount is not trusted");
}

// --- (f) no default type ---------------------------------------------------------------------------------------------

#[test]
fn there_is_no_default_type_and_a_type_the_file_lacks_lists_the_ones_it_has() {
    let fx = Fixture::with_export("req-needs-types", &fixture_text(SCORE));
    let before = fx.claims();

    let none = import(fx.path(), &base(&[]));
    none.assert_code(2, "no --type");
    assert!(
        none.stderr.contains("--type"),
        "the usage error names the flag:\n{}",
        none.stderr
    );
    let empty = import(fx.path(), &plus(base(&[]), &["--type", ""]));
    empty.assert_code(2, "an empty --type");

    let unknown = import(
        fx.path(),
        &plus(base(&["stkh_req"]), &["--type", "bogus_req", "--write"]),
    );
    unknown.assert_code(1, "a --type the file does not have");
    for name in [
        "bogus_req",
        "stkh_req",
        "feat_req",
        "comp_req",
        "aou_req",
        "document",
        "mod_view_sta",
    ] {
        assert!(
            unknown.stderr.contains(name),
            "the message names the absent type and lists the file's types ({name}):\n{}",
            unknown.stderr
        );
    }
    assert_eq!(fx.claims(), before, "nothing is written");
}

// --- (g) skips are named ----------------------------------------------------------------------------------------------

#[test]
fn every_skip_reason_is_named_counted_and_checked_in_order() {
    let mut export = fixture_json(SCORE);
    {
        let needs = needs_mut(&mut export);
        let set = |needs: &mut Map<String, Value>, id: &str, key: &str, value: Value| {
            needs
                .get_mut(id)
                .unwrap_or_else(|| panic!("{id} is in the excerpt"))
                .as_object_mut()
                .expect("a need")
                .insert(key.to_string(), value);
        };
        // CHANGED: three kinds of blank content (empty, null, whitespace only) ...
        set(
            needs,
            "stkh_req__ai_platform__agent_execution",
            "content",
            json!(""),
        );
        set(
            needs,
            "feat_req__ai_platform__agent_execution",
            "content",
            Value::Null,
        );
        set(
            needs,
            "comp_req__feo__act_alloc_error",
            "content",
            json!("  \n\t "),
        );
        // ... an inline role ...
        set(
            needs,
            "aou_req__communication__1",
            "content",
            json!("The platform shall use :need:`doc__os_requirements` as its base."),
        );
        // ... and an external need (its content is fine).
        set(
            needs,
            "feat_req__feo__act_alloc_error",
            "is_external",
            json!(true),
        );
        // ADDED: order checks (the first reason that applies is the one named) and a control.
        needs.insert("x_ext_empty".into(), json!({"type": "feat_req", "is_external": true, "content": "", "docname": "x", "lineno": 1}));
        needs.insert(
            "x_empty_dir".into(),
            json!({"type": "feat_req", "content": "  ", "docname": "x", "lineno": 2}),
        );
        needs.insert("x_dir_multi".into(), json!({"type": "feat_req", "content": ".. note::\n\n   body", "docname": "x", "lineno": 3}));
        needs.insert("x_multi_role".into(), json!({"type": "feat_req", "content": "A statement.\n\nSee :ref:`x`.", "docname": "x", "lineno": 4}));
        needs.insert("x_indented_dir".into(), json!({"type": "feat_req", "content": "A statement.\n   .. note:: indented", "docname": "x", "lineno": 5}));
        needs.insert("x_ticks".into(), json!({"type": "feat_req", "content": "The tool shall write to the `log` file: and `more`.", "docname": "x", "lineno": 6}));
        needs.insert("x_domain_role".into(), json!({"type": "feat_req", "content": "The tool shall call :py:func:`run`.", "docname": "x", "lineno": 7}));
    }
    let fx = Fixture::with_export("req-needs-skips", &export.to_string());
    let out = import(fx.path(), &base(&SCORE_TYPES));
    out.assert_code(0, "the modified copy");
    let report = parse_report(&out.stdout);
    let expected = oracle(&export, &SCORE_TYPES, &[]);
    assert_report_matches(&report, &expected, "the modified copy");

    let reasons: BTreeMap<&str, &str> = report
        .skips
        .iter()
        .map(|(id, why)| (id.as_str(), why.as_str()))
        .collect();
    for (id, reason) in [
        ("stkh_req__ai_platform__agent_execution", "no content"),
        ("feat_req__ai_platform__agent_execution", "no content"),
        ("comp_req__feo__act_alloc_error", "no content"),
        ("stkh_req__dependability__firewall", "no content"),
        ("aou_req__os__nonroot", "multi-paragraph"),
        ("aou_req__communication__1", "rst role"),
        ("feat_req__feo__act_alloc_error", "external"),
        ("x_ext_empty", "external"),
        ("x_empty_dir", "no content"),
        ("x_dir_multi", "rst directive"),
        ("x_multi_role", "multi-paragraph"),
        ("x_indented_dir", "rst directive"),
        ("x_domain_role", "rst role"),
    ] {
        assert_eq!(reasons.get(id), Some(&reason), "{id}");
    }
    assert!(
        !reasons.contains_key("x_ticks"),
        "a backtick that is not preceded by a role is not a skip"
    );
    assert!(
        report.drafts.iter().any(|d| d.need == "x_ticks"),
        "x_ticks is drafted"
    );

    // `rst directive` from the demo, as published.
    let demo = Fixture::with_export("req-needs-skips-demo", &fixture_text(DEMO));
    let demo_report = parse_report(&import(demo.path(), &base(&DEMO_TYPES)).stdout);
    assert!(
        demo_report
            .skips
            .contains(&("EX_REQ_001".to_string(), "rst directive".to_string()))
    );
}

// --- (h) equal text is not collapsed -----------------------------------------------------------------------------------

#[test]
fn equal_statements_are_two_drafts_with_one_address() {
    let (fx, report, rows) = end_to_end(SCORE, &SCORE_TYPES);
    let claim_of = |need: &str| {
        report
            .drafts
            .iter()
            .find(|d| d.need == need)
            .unwrap_or_else(|| panic!("{need} is drafted"))
            .claim
            .clone()
    };
    let comp = claim_of("comp_req__feo__act_alloc_error");
    let feat = claim_of("feat_req__feo__act_alloc_error");
    assert_ne!(comp, feat, "two claims, not one");
    assert_eq!(
        rows[&comp], rows[&feat],
        "the same statement derives the same address"
    );
    assert!(fx.exists(&format!("claims/{comp}.yml")) && fx.exists(&format!("claims/{feat}.yml")));

    // `req canon` prints the same parse for both (it does not print the address itself).
    let canon = req(fx.path(), &["canon"]);
    canon.assert_code(0, "req canon");
    let parse_of = |claim: &str| {
        canon
            .stdout
            .lines()
            .find(|line| line.starts_with(&format!("{claim} ")))
            .unwrap_or_else(|| panic!("canon prints {claim}:\n{}", canon.stdout))
            .trim_start_matches(claim)
            .to_string()
    };
    assert_eq!(
        parse_of(&comp),
        parse_of(&feat),
        "canon shows the two with the same parse"
    );
}

// --- (i) not carried -------------------------------------------------------------------------------------------------------

#[test]
fn status_tags_safety_security_reqtype_and_links_are_in_no_claim_and_the_report_says_so() {
    let (fx, report, _rows) = end_to_end(SCORE, &SCORE_TYPES);
    let export = fixture_json(SCORE);
    let needs = needs_of(&export);
    for draft in &report.drafts {
        let text = fx.read(&format!("claims/{}.yml", draft.claim));
        assert_eq!(
            field(&text, "relations: "),
            Some("relations: []"),
            "{} carries no relation",
            draft.claim
        );
        // Everything but the statement line (the author's own words): no source field name, and no
        // value the source carried (`valid`, `ASIL_B`, `Non-Functional`, a link target).
        let metadata: String = text
            .lines()
            .filter(|line| !line.starts_with("statement:"))
            .collect::<Vec<_>>()
            .join("\n");
        let need = &needs[&draft.need];
        let mut forbidden: Vec<String> = [
            "status",
            "tags",
            "safety",
            "security",
            "reqtype",
            "derived_from",
            "satisfied_by",
            "links",
        ]
        .iter()
        .map(|word| (*word).to_string())
        .collect();
        for key in ["status", "safety", "security", "reqtype"] {
            // Short values (`QM`, `NO`) would match any word; the longer ones are the evidence.
            if let Some(value) = need[key].as_str().filter(|value| value.len() >= 5) {
                forbidden.push(value.to_string());
            }
        }
        for word in forbidden {
            assert!(
                !metadata.contains(&word),
                "{} must not carry `{word}`:\n{metadata}",
                draft.claim
            );
        }
    }

    // The report names what it did not carry ...
    let line = report
        .not_carried
        .expect("the report has a `not carried` line");
    for name in ["status", "tags", "safety", "security", "reqtype"] {
        assert!(line.contains(name), "`not carried` names {name}: {line}");
    }
    // ... and counts the drafted needs that had typed links, naming the link fields. Read off the
    // excerpt: a link type is a field with a `_back` partner somewhere in the file, here
    // `derived_from`/`derived_from_back`; four of the five drafted needs have one of them.
    let with_links = report
        .drafts
        .iter()
        .filter(|d| {
            let need = &needs[&d.need];
            need.get("derived_from").is_some_and(|v| !v.is_null())
                || need.get("derived_from_back").is_some_and(|v| !v.is_null())
        })
        .count();
    assert_eq!(
        (with_links, report.drafts.len()),
        (4, 5),
        "the oracle's own reading of the excerpt"
    );
    assert!(
        line.contains(&format!(
            "{with_links} of {} drafted needs",
            report.drafts.len()
        )),
        "`not carried` counts the drafted needs with links: {line}"
    );
    assert!(
        line.contains("derived_from") && line.contains("derived_from_back"),
        "and names the link fields: {line}"
    );
    assert!(
        line.contains("satisfied_by"),
        "a field it did not classify is still named, never dropped: {line}"
    );

    // The demo: `implements` has its `_back` partner in the file; `links_back` is on a need that is not drafted.
    let demo = Fixture::with_export("req-needs-links-demo", &fixture_text(DEMO));
    let demo_report = parse_report(&import(demo.path(), &base(&DEMO_TYPES)).stdout);
    let demo_line = demo_report.not_carried.expect("a `not carried` line");
    assert!(
        demo_line.contains("2 of 2 drafted needs") && demo_line.contains("implements"),
        "{demo_line}"
    );
}

// --- (j) --status -----------------------------------------------------------------------------------------------------------

#[test]
fn status_narrows_the_selection_and_is_never_written() {
    let export = fixture_json(SCORE);
    let fx = Fixture::with_export("req-needs-status", &fixture_text(SCORE));
    let out = import(
        fx.path(),
        &plus(base(&SCORE_TYPES), &["--status", "valid", "--write"]),
    );
    out.assert_code(0, "--status valid");
    let report = parse_report(&out.stdout);
    let expected = oracle(&export, &SCORE_TYPES, &["valid"]);
    assert_report_matches(&report, &expected, "--status valid");
    assert!(
        expected.not_selected.get("aou_req") == Some(&1),
        "the `invalid` aou_req counts as not selected, under its type"
    );
    assert!(
        !report
            .skips
            .iter()
            .any(|(id, _)| id == "aou_req__feo__on_target_crates"),
        "it is not a skip either"
    );
    for (name, text) in fx.claims() {
        if name != "REQ-0001.yml" {
            assert!(
                field(&text, "status").is_none() && !text.contains("valid"),
                "{name} carries no status:\n{text}"
            );
        }
    }

    // Repeatable: two statuses, one of which nothing has. Only the `invalid` aou_req is selected, and
    // it is a multi-paragraph need, so nothing is draftable: a definite problem, named.
    let two = import(
        fx.path(),
        &plus(
            base(&SCORE_TYPES),
            &["--status", "invalid", "--status", "nonesuch"],
        ),
    );
    two.assert_code(1, "the only selected need is a skip");
    assert!(
        two.stderr
            .contains("skip aou_req__feo__on_target_crates  multi-paragraph"),
        "{}",
        two.stderr
    );
}

// --- (k) dates -----------------------------------------------------------------------------------------------------------------

#[test]
fn the_date_is_authored_and_the_script_never_reads_the_clock_or_a_commit() {
    let fx = Fixture::with_export("req-needs-date", &fixture_text(SCORE));
    let without: Vec<String> = base(&SCORE_TYPES)
        .into_iter()
        .scan(false, |skip_next, arg| {
            Some(if *skip_next {
                *skip_next = false;
                None
            } else if arg == "--date" {
                *skip_next = true;
                None
            } else {
                Some(arg)
            })
        })
        .flatten()
        .collect();
    let missing = import(fx.path(), &without);
    missing.assert_code(2, "no --date");
    assert!(missing.stderr.contains("--date"), "{}", missing.stderr);

    for bad in [
        "2026-13-40",
        "2026-1-2",
        "2026-02-30",
        "yesterday",
        "2026-11-05T00:00:00Z",
    ] {
        let mut args = base(&SCORE_TYPES);
        args[5] = bad.to_string();
        let out = import(fx.path(), &args);
        out.assert_code(2, &format!("--date {bad}"));
        assert!(out.stderr.contains("--date"), "{}", out.stderr);
    }
    assert_eq!(fx.claims().len(), 1, "nothing was written");

    // The export committed in a repository with a pinned past commit date: still no --date, still
    // exit 2 (the sibling `adopt.py` would borrow that date; this reader has none to borrow).
    git_commit_all(&fx, "the export");
    import(fx.path(), &without).assert_code(2, "a committed export with no --date");
    import(fx.path(), &plus(base(&SCORE_TYPES), &["--write"])).assert_code(0, "with --date");
    for (name, text) in fx.claims() {
        if name != "REQ-0001.yml" {
            assert!(
                text.contains(&format!("date: {DATE}\n")),
                "{name} carries the date it was given:\n{text}"
            );
            assert!(
                !text.contains("2020-01-02"),
                "{name} must not borrow a commit date"
            );
        }
    }

    // The source never asks for the time or for a commit.
    let source = fs::read_to_string(script()).expect("reads the script");
    for forbidden in [
        "date.today",
        ".now(",
        "utcnow",
        "time.time",
        "strftime",
        "\"git\"",
        "'git'",
        "git log",
        "git -C",
    ] {
        assert!(
            !source.contains(forbidden),
            "tools/needs_import.py must not contain `{forbidden}`"
        );
    }
}

/// A throwaway repository inside the fixture, identity and dates pinned through the environment.
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
            .env("GIT_AUTHOR_NAME", "req needs test")
            .env("GIT_AUTHOR_EMAIL", "needs@example.invalid")
            .env("GIT_COMMITTER_NAME", "req needs test")
            .env("GIT_COMMITTER_EMAIL", "needs@example.invalid")
            .env("GIT_AUTHOR_DATE", "2020-01-02T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2020-01-02T00:00:00Z")
            .output()
            .expect("runs git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// --- usage ------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_malformed_invocation_is_a_usage_error() {
    let fx = Fixture::with_export("req-needs-usage", &fixture_text(SCORE));
    let drop_flag = |flag: &str| {
        let mut args = base(&SCORE_TYPES);
        let at = args
            .iter()
            .position(|a| a == flag)
            .expect("the flag is in the base");
        args.drain(at..at + 2);
        args
    };
    for (what, args) in [
        ("no --from", drop_flag("--from")),
        ("no --origin", drop_flag("--origin")),
        (
            "an empty --origin",
            plus(drop_flag("--origin"), &["--origin", ""]),
        ),
        (
            "a blank --origin",
            plus(drop_flag("--origin"), &["--origin", "   "]),
        ),
        (
            "a value that begins with `-` for --origin",
            plus(drop_flag("--origin"), &["--origin=-5"]),
        ),
        (
            "a value that begins with `-` for --type",
            plus(base(&["req"]), &["--type=-5"]),
        ),
        (
            "a flag where a value is expected",
            plus(base(&["req"]), &["--status", "--write"]),
        ),
        ("an unknown flag", plus(base(&["req"]), &["--subject", "x"])),
    ] {
        let out = import(fx.path(), &args);
        out.assert_code(2, what);
        assert!(
            out.stderr.contains("usage"),
            "{what}: the script itself answers with its usage:\n{}",
            out.stderr
        );
    }
    assert_eq!(fx.claims().len(), 1, "nothing was written");
}

#[test]
fn a_file_with_nothing_draftable_is_a_definite_problem_and_names_why() {
    let export = json!({
        "current_version": "1",
        "versions": {"1": {"needs": {
            "A": {"type": "req", "content": "", "docname": "a", "lineno": 1},
            "B": {"type": "req", "content": "One.\n\nTwo.", "docname": "a", "lineno": 2},
            "C": {"type": "other", "content": "Not selected.", "docname": "a", "lineno": 3},
        }}}
    })
    .to_string();
    let fx = Fixture::with_export("req-needs-nothing", &export);
    let out = import(fx.path(), &plus(base(&["req"]), &["--write"]));
    out.assert_code(1, "nothing drafted");
    for expect in [
        "skip A  no content",
        "skip B  multi-paragraph",
        "needs 3 = drafted 0 + skipped 2 + not selected 1",
    ] {
        assert!(
            out.stderr.contains(expect),
            "the finding names why ({expect}):\n{}",
            out.stderr
        );
    }
    assert_eq!(fx.claims().len(), 1, "nothing is written");
}

// --- (l) UNVERIFIABLE ---------------------------------------------------------------------------------------------------------------

#[test]
fn a_file_or_corpus_that_cannot_be_read_honestly_is_unverifiable_and_writes_nothing() {
    let good_need =
        json!({"type": "req", "content": "The one shall hold.", "docname": "a", "lineno": 1});
    let cases: Vec<(&str, String, Vec<&str>)> = vec![
        ("not json", "this is not json".to_string(), vec![]),
        ("a json array", "[1, 2, 3]".to_string(), vec![]),
        ("no versions", json!({"current_version": "1", "needs": {}}).to_string(), vec!["versions"]),
        (
            "two versions and no current_version",
            json!({"versions": {"0.1": {"needs": {"A": good_need}}, "0.2": {"needs": {"A": good_need}}}}).to_string(),
            vec!["0.1", "0.2", "current_version"],
        ),
        (
            "a current_version the file does not have",
            json!({"current_version": "9", "versions": {"1": {"needs": {"A": good_need}}}}).to_string(),
            vec!["9"],
        ),
        (
            "a version without a needs mapping",
            json!({"current_version": "1", "versions": {"1": {"needs": [1, 2]}}}).to_string(),
            vec!["needs"],
        ),
        (
            "a need that is not an object",
            json!({"current_version": "1", "versions": {"1": {"needs": {"A": "text"}}}}).to_string(),
            vec!["A"],
        ),
        (
            "content that is not text",
            json!({"current_version": "1", "versions": {"1": {"needs": {"A": {"type": "req", "content": ["x"]}}}}}).to_string(),
            vec!["A", "content"],
        ),
    ];
    for (what, text, names) in cases {
        let fx = Fixture::with_export("req-needs-unverifiable", &text);
        let out = import(fx.path(), &plus(base(&["req"]), &["--write"]));
        out.assert_code(3, what);
        assert!(
            out.stderr.starts_with("[UNVERIFIABLE]"),
            "{what}: tri-state, not a crash:\n{}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("internal error"),
            "{what}: named, not an internal error:\n{}",
            out.stderr
        );
        for name in names {
            assert!(
                out.stderr.contains(name),
                "{what}: the message names `{name}`:\n{}",
                out.stderr
            );
        }
        assert_eq!(fx.claims().len(), 1, "{what}: nothing written");
    }

    // An unknown --version-key names the versions the file has; a known one is read.
    let two = json!({"versions": {
        "0.1": {"needs": {"A": {"type": "req", "content": "The old one shall hold.", "docname": "a", "lineno": 1}}},
        "0.2": {"needs": {"B": {"type": "req", "content": "The new one shall hold.", "docname": "a", "lineno": 1}}},
    }})
    .to_string();
    let fx = Fixture::with_export("req-needs-version-key", &two);
    let unknown = import(fx.path(), &plus(base(&["req"]), &["--version-key", "0.3"]));
    unknown.assert_code(3, "an unknown --version-key");
    assert!(
        unknown.stderr.contains("0.3")
            && unknown.stderr.contains("0.1")
            && unknown.stderr.contains("0.2"),
        "{}",
        unknown.stderr
    );
    let chosen = import(
        fx.path(),
        &plus(base(&["req"]), &["--version-key", "0.2", "--write"]),
    );
    chosen.assert_code(0, "a named --version-key reads that version");
    assert_eq!(parse_report(&chosen.stdout).drafts[0].need, "B");

    // An unreadable file, and no corpus.
    let fx = Fixture::with_export("req-needs-missing", "{}");
    let mut args = base(&["req"]);
    args[1] = "absent.json".to_string();
    let out = import(fx.path(), &args);
    out.assert_code(3, "a file that is not there");
    assert!(
        out.stderr.contains("absent.json") && !out.stderr.contains("internal error"),
        "{}",
        out.stderr
    );
    let bare = Fixture::new("req-needs-nocorpus");
    bare.write("needs.json", &fixture_text(SCORE));
    let out = import(bare.path(), &base(&SCORE_TYPES));
    out.assert_code(3, "no corpus");
    assert!(
        out.stderr.contains("claims/") && out.stderr.contains("req init"),
        "{}",
        out.stderr
    );
    assert!(
        !bare.exists("claims/REQ-0001.yml"),
        "the reader never creates a corpus"
    );
}

#[test]
fn a_corpus_the_loader_rejects_is_unverifiable_and_nothing_is_drafted() {
    let current = req::canon::claim_schema();
    let valid = |schema_line: &str, extra: &str| {
        format!(
            "{schema_line}\nclaim_id: REQ-0009\nstatement: An existing claim.\napplicability: []\nrelations: []\n\
             rationale:\n  source: x\n  date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\n{extra}"
        )
    };
    for (name, text) in [
        (
            "removed-key",
            valid(&format!("schema: {current}"), "modality: SHALL\n"),
        ),
        ("no-schema", valid("# no schema here", "")),
        (
            "bad-pin",
            valid(&format!("schema: {current}"), "pins:\n  value: \"0.30\"\n"),
        ),
    ] {
        let fx = Fixture::with_export("req-needs-reject", &fixture_text(SCORE));
        fx.write("claims/REQ-0009.yml", &text);
        let before = fx.claims();
        let out = import(fx.path(), &plus(base(&SCORE_TYPES), &["--write"]));
        out.assert_code(3, &format!("{name}: a corpus the loader rejects"));
        assert!(
            out.stderr.contains("req canon"),
            "{name}: the loader is named:\n{}",
            out.stderr
        );
        assert_eq!(fx.claims(), before, "{name}: nothing is drafted into it");
    }
}

#[cfg(unix)]
#[test]
fn a_req_reporting_another_claim_schema_is_unverifiable_and_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::with_export("req-needs-schema", &fixture_text(SCORE));
    let bin = fx.path().join("fake-bin");
    fs::create_dir_all(&bin).expect("fake bin dir");
    let fake = bin.join("req");
    fs::write(&fake, "#!/bin/sh\nprintf 'req 0.1.0\\n  rules_v            6\\n  claim_schema       req.claim/3\\n'\n")
        .expect("fake req");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("chmod");
    let before = fx.claims();
    let out = import_in(
        fx.path(),
        &plus(base(&SCORE_TYPES), &["--write"]),
        &Env {
            path_first: Some(&bin),
            ..Env::default()
        },
    );
    out.assert_code(3, "a req that judges another claim schema");
    assert!(
        out.stderr.contains("req.claim/3") && out.stderr.contains("req.claim/2"),
        "the message names both schemas:\n{}",
        out.stderr
    );
    assert_eq!(
        fx.claims(),
        before,
        "nothing is drafted against a schema the reader does not write"
    );
}

// --- (m) nothing derived ------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_draft_carries_nothing_derived_and_the_strict_loader_accepts_it() {
    let (fx, report, _rows) = end_to_end(SCORE, &SCORE_TYPES);
    let schema = req::canon::claim_schema();
    for draft in &report.drafts {
        let text = fx.read(&format!("claims/{}.yml", draft.claim));
        assert_eq!(
            field(&text, "schema: "),
            Some(format!("schema: {schema}").as_str()),
            "{text}"
        );
        for key in [
            "modality:",
            "subject:",
            "attribute:",
            "operator:",
            "value:",
            "unit:",
            "conditions:",
            "exception:",
            "temporal:",
            "quantifier:",
            "unstructured:",
            "canonical:",
            "address:",
            "pins:",
        ] {
            assert_eq!(
                field(&text, key),
                None,
                "{} carries nothing derived and no pin (`{key}`):\n{text}",
                draft.claim
            );
        }
        assert_eq!(field(&text, "applicability: "), Some("applicability: []"));
        assert_eq!(field(&text, "relations: "), Some("relations: []"));
        assert!(
            text.contains("rejected_alternatives: []") && text.contains("assumptions: []"),
            "{text}"
        );
    }
    // `end_to_end` already ran `req canon` (exit 0, 0 flagged); the gate sees them too.
    let canon = req(fx.path(), &["canon", "--fail-on-flag"]);
    canon.assert_code(0, "no pin is flagged");
}

#[test]
fn a_hostile_need_id_origin_and_text_are_quoted_not_executed() {
    // A need id, a docname and a statement full of YAML-significant characters: the draft must
    // load and `req` must read the statement back unchanged.
    let statement = "The tool shall print \"a: b # c\" and a back\\slash.";
    let export = json!({
        "current_version": "1",
        "versions": {"1": {"needs": {
            "id: with #odd\"chars": {"type": "req", "content": statement, "docname": "d o\nc", "lineno": 7},
        }}}
    })
    .to_string();
    let fx = Fixture::with_export("req-needs-hostile", &export);
    let mut args = base(&["req"]);
    args[3] = "path with spaces/needs: x.json".to_string();
    import(fx.path(), &plus(args, &["--write"])).assert_code(0, "the hostile export");
    let render = req(fx.path(), &["render"]);
    render.assert_code(0, "req render");
    assert_eq!(render_rows(&render.stdout)["REQ-0002"].1, statement);
    req(fx.path(), &["canon"]).assert_code(0, "req canon");
}

// --- (n) --self-check ------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_self_check_stands_alone_and_refuses_company() {
    // No document, no corpus, no `req` on PATH.
    let bare = Fixture::new("req-needs-selfcheck");
    let out = Command::new(python3())
        .arg(script())
        .arg("--self-check")
        .current_dir(bare.path())
        .env("PATH", "")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/needs_import.py --self-check");
    let out = outcome(&out);
    out.assert_code(0, "--self-check with nothing around it");
    let cases: usize = out
        .stdout
        .split("needs_import --self-check: ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("the self-check summary is missing:\n{}", out.stdout));
    assert!(
        cases >= 25,
        "the self-check table has lost rows ({cases}):\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("0 failure(s)"),
        "every row passes:\n{}",
        out.stdout
    );

    // Beside any other argument it is a usage error, not a quiet run of two things.
    for extra in [
        vec!["--write"],
        vec!["--from", "needs.json"],
        vec!["--type", "req"],
    ] {
        let mut args = vec!["--self-check"];
        args.extend(extra);
        let out = import(bare.path(), &strings(&args));
        out.assert_code(2, &format!("--self-check beside {args:?}"));
    }
}

// --- (o) the corpus root ----------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_corpus_root_is_named_or_the_working_directory_never_an_ancestor() {
    let fx = Fixture::with_export("req-needs-root", &fixture_text(SCORE));
    let sub = fx.path().join("sub");
    fs::create_dir_all(&sub).expect("creates the subdirectory");
    let before = fx.claims();

    // From a subdirectory with no root named: the working directory is the root, and it has no
    // corpus, so the run is UNVERIFIABLE and drafts nothing into the parent that holds one.
    let from_sub = import(&sub, &plus(base(&SCORE_TYPES), &["--write"]));
    from_sub.assert_code(
        3,
        "a subdirectory is not a corpus, and an ancestor is never searched",
    );
    assert!(
        from_sub.stderr.contains("claims/") && from_sub.stderr.contains("sub"),
        "{}",
        from_sub.stderr
    );
    assert_eq!(fx.claims(), before, "nothing was drafted into the ancestor");

    // --root, with `--from` resolved against it and the provenance independent of the cwd.
    let rooted = import(
        &sub,
        &plus(base(&SCORE_TYPES), &["--root", "..", "--write"]),
    );
    rooted.assert_code(0, "--root ..");
    assert!(
        fx.claims().len() > before.len(),
        "the drafts land in the named root"
    );

    // REQ_ROOT is the next rule; --root beats it.
    let other = Fixture::with_export("req-needs-root-env", &fixture_text(DEMO));
    let env_run = import_in(
        &sub,
        &plus(base(&DEMO_TYPES), &["--write"]),
        &Env {
            req_root: Some(other.path()),
            ..Env::default()
        },
    );
    env_run.assert_code(0, "REQ_ROOT names the corpus");
    assert_eq!(
        other.claims().len(),
        3,
        "the starter plus the demo's two drafts landed in REQ_ROOT"
    );
    let beaten = import_in(
        &sub,
        &plus(
            base(&SCORE_TYPES),
            &[
                "--root",
                fx.path().to_str().expect("a UTF-8 path"),
                "--origin",
                "https://example.test/third.json",
            ],
        ),
        &Env {
            req_root: Some(other.path()),
            ..Env::default()
        },
    );
    beaten.assert_code(0, "--root beats REQ_ROOT");
    assert!(
        beaten
            .stdout
            .contains(fx.path().to_str().expect("a UTF-8 path")),
        "the report names the chosen root:\n{}",
        beaten.stdout
    );
}
