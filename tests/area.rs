//! `req.claim/3` and the `area` key (ADR 0051, task A-1): a claim may name the place it is filed,
//! and the place is **outside identity** — no address, applicability or derived parse moves with it,
//! and no byte of a Baseline.
//!
//! The seam is the compiled `req` over throwaway corpora, plus the two public functions the loader
//! is built from (`req::canon::parse_claim`, `req::canon::classify_claim_schema[_at]`). Every
//! expectation comes from the ADR's decisions and its *Falsification* list, never from a run:
//!
//! * 1 — adding, changing or removing `area` moves an address or breaks a Baseline;
//! * 2 — a claim is shown in two groups (this unit has no grouped view; the format-level half is
//!   that a claim has **one** area, so a list of areas is refused);
//! * 3 — a claim naming an undeclared area fails to load, when it should be a finding;
//! * 4 — a `/3` build refuses or rewrites a `/2` file, or a `/2` build reading `/3` says anything
//!   but "newer";
//! * 5 — a real adopter's sections cannot be one path of stable keys per requirement;
//! * 6 — a claim's address, applicability or derived parse changes because of its area.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use req::canon::{ClaimSchema, classify_claim_schema, classify_claim_schema_at, parse_claim};
use serde_json::Value;

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout is a JSON document ({e}):\n{}", self.stdout))
    }

    /// The completeness step's line and the indented notes under it.
    fn completeness_block(&self) -> String {
        let mut block = String::new();
        let mut inside = false;
        for line in self.stdout.lines() {
            if line.starts_with('[') {
                inside = line.contains("] completeness ");
            } else if !line.starts_with("    ") {
                inside = false;
            }
            if inside {
                block.push_str(line);
                block.push('\n');
            }
        }
        block
    }
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// A corpus made by the real bootstrap, with its starter claim removed.
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-area-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        let init = Command::new(req_bin())
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("runs req init");
        assert_eq!(init.status.code(), Some(0), "req init must succeed");
        fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, text).expect("writes the file");
    }

    /// One claim file: `schema` as declared, `area` as written (a YAML scalar or flow value), `extra`
    /// lines appended.
    fn claim(&self, id: &str, schema: &str, area: Option<&str>, extra: &str) {
        self.write(
            &format!("claims/{id}.yml"),
            &claim_text(id, schema, area, extra),
        );
    }

    fn req(&self, args: &[&str]) -> Out {
        run(&self.0, args)
    }
}

fn run(dir: &Path, args: &[&str]) -> Out {
    let out = Command::new(req_bin())
        .args(args)
        .current_dir(dir)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req");
    Out {
        code: out.status.code().expect("req exits, never a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn claim_text(id: &str, schema: &str, area: Option<&str>, extra: &str) -> String {
    let area = area.map_or(String::new(), |a| format!("area: {a}\n"));
    format!(
        "schema: {schema}\n\
         claim_id: {id}\n\
         statement: \"The {id} service shall respond to a health check within 200 ms.\"\n\
         applicability: []\n\
         relations: []\n\
         evidence: process\n\
         {area}{extra}\
         rationale:\n  source: docs/requirements.md\n  date: 2026-11-13\n  rejected_alternatives: []\n  assumptions: []\n"
    )
}

/// The `address` of each claim `req render --format json` lists, by claim id.
fn addresses(s: &Scratch) -> Vec<(String, String)> {
    addresses_under(s, &[])
}

/// [`addresses`] under a selection (`key=value` words), for a corpus whose claims carry selectors.
fn addresses_under(s: &Scratch, selection: &[&str]) -> Vec<(String, String)> {
    let mut args = vec!["render", "--format", "json"];
    args.extend(selection);
    let out = s.req(&args);
    assert_eq!(out.code, 0, "render loads the corpus:\n{}", out.all());
    let doc = out.json();
    let mut found: Vec<(String, String)> = ["applies", "undecided", "outside"]
        .iter()
        .flat_map(|section| doc[*section].as_array().cloned().unwrap_or_default())
        .map(|row| {
            (
                row["claim_id"].as_str().expect("claim_id").to_string(),
                row["address"].as_str().expect("address").to_string(),
            )
        })
        .collect();
    found.sort();
    found
}

// --- the format (ADR 0051 decision 6) -------------------------------------------------------------

#[test]
fn this_build_writes_claim_schema_3() {
    let version = run(Path::new("."), &["--version"]);
    assert_eq!(version.code, 0, "{}", version.all());
    assert!(
        version
            .stdout
            .lines()
            .any(|l| l.trim().starts_with("claim_schema") && l.trim().ends_with("req.claim/3")),
        "`req --version` reports claim_schema req.claim/3:\n{}",
        version.stdout
    );
    assert_eq!(req::canon::claim_schema(), "req.claim/3");
}

#[test]
fn req_init_starts_a_corpus_at_claim_schema_3() {
    // A fresh init shows what the bootstrap writes.
    let fresh = std::env::temp_dir().join(format!("req-area-init-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fresh);
    fs::create_dir_all(&fresh).expect("creates the dir");
    let init = run(&fresh, &["init"]);
    assert_eq!(init.code, 0, "{}", init.all());
    let starter = fs::read_to_string(fresh.join("claims/REQ-0001.yml")).expect("starter claim");
    assert!(
        starter.lines().any(|l| l == "schema: req.claim/3"),
        "the starter claim declares the current format:\n{starter}"
    );
    let readme = fs::read_to_string(fresh.join("claims/README.md")).expect("claims README");
    assert!(
        readme.contains("req.claim/3") && !readme.contains("req.claim/2"),
        "the claims README names the current format, not the previous one:\n{readme}"
    );
    let _ = fs::remove_dir_all(&fresh);
}

#[test]
fn a_v2_file_loads_as_it_is_and_is_no_migration_candidate() {
    // The set of formats this build loads as they are is data, stated once.
    assert_eq!(
        classify_claim_schema("req.claim/2").reason("req.claim/2"),
        None,
        "req.claim/2 needs no migration and no message"
    );
    assert!(
        !matches!(classify_claim_schema("req.claim/2"), ClaimSchema::Older(_)),
        "req.claim/2 is not an older format to migrate"
    );
    let s = Scratch::new("v2-loads");
    s.claim("REQ-0001", "req.claim/2", None, "");
    let canon = s.req(&["canon"]);
    assert_eq!(canon.code, 0, "{}", canon.all());
    assert!(
        !canon.all().contains("migration candidate") && !canon.all().contains("NOT JUDGED"),
        "nothing is reported about a /2 file:\n{}",
        canon.all()
    );
    let before = fs::read_to_string(s.path().join("claims/REQ-0001.yml")).expect("reads");
    let _ = s.req(&["render"]);
    let after = fs::read_to_string(s.path().join("claims/REQ-0001.yml")).expect("reads");
    assert_eq!(
        before, after,
        "no command rewrites a /2 file (falsification 4)"
    );
}

#[test]
fn the_same_claim_declared_v2_or_v3_has_one_address() {
    let two = Scratch::new("addr-v2");
    two.claim("REQ-0001", "req.claim/2", None, "");
    let three = Scratch::new("addr-v3");
    three.claim("REQ-0001", "req.claim/3", None, "");
    assert_eq!(addresses(&two), addresses(&three));
}

#[test]
fn v1_stays_a_migration_candidate_and_v4_a_newer_format() {
    assert_eq!(classify_claim_schema("req.claim/1"), ClaimSchema::Older(1));
    let older = classify_claim_schema("req.claim/1")
        .reason("req.claim/1")
        .expect("a /1 file is reported");
    assert!(older.contains("migration candidate"), "{older}");
    assert!(older.contains("NOT JUDGED"), "{older}");

    assert_eq!(classify_claim_schema("req.claim/4"), ClaimSchema::Newer(4));
    let newer = classify_claim_schema("req.claim/4")
        .reason("req.claim/4")
        .expect("a /4 file is reported");
    assert!(newer.contains("newer"), "{newer}");

    // Falsification 4, the old-reader half: a /2 build reading a /3 file says "newer", nothing else.
    assert_eq!(
        classify_claim_schema_at("req.claim/3", 2),
        ClaimSchema::Newer(3)
    );

    for (declared, word) in [
        ("req.claim/1", "migration candidate"),
        ("req.claim/4", "newer"),
    ] {
        let s = Scratch::new("schema-judged");
        s.claim("REQ-0001", declared, None, "");
        let out = s.req(&["canon"]);
        assert_eq!(out.code, 3, "{declared} is NOT JUDGED:\n{}", out.all());
        assert!(out.all().contains(word), "{declared}: {}", out.all());
    }
}

#[test]
fn a_v2_file_carrying_area_is_a_parse_error_naming_the_key_and_the_format() {
    let text = claim_text("REQ-0001", "req.claim/2", Some("diagnostics/startup"), "");
    let error = parse_claim(&text).expect_err("area needs req.claim/3");
    assert!(error.contains("`area`"), "names the key: {error}");
    assert!(
        error.contains("req.claim/3"),
        "names the format it needs: {error}"
    );

    let s = Scratch::new("v2-area");
    s.claim("REQ-0001", "req.claim/2", Some("diagnostics/startup"), "");
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("`area`") && out.all().contains("req.claim/3"),
        "{}",
        out.all()
    );
}

#[test]
fn a_v2_file_carrying_the_key_is_refused_whatever_its_value() {
    // ADR 0051 decision 6 judges the KEY: an older /2 build rejects `area` as an unknown key whatever
    // follows it, so a /3 build must not load such a file silently as "no area". `area:`, `area: null`
    // and `area: ~` are all YAML null.
    for spelling in ["", "null", "~"] {
        let text = claim_text("REQ-0001", "req.claim/2", Some(spelling), "");
        let error = parse_claim(&text)
            .expect_err(&format!("`area: {spelling}` in a /2 file carries the key"));
        assert!(
            error.contains("`area`"),
            "{spelling:?}: names the key: {error}"
        );
        assert!(
            error.contains("req.claim/3"),
            "{spelling:?}: names the format it needs: {error}"
        );

        let s = Scratch::new("v2-null-area");
        s.claim("REQ-0001", "req.claim/2", Some(spelling), "");
        let out = s.req(&["canon"]);
        assert_eq!(out.code, 3, "{spelling:?}: {}", out.all());
    }
}

// --- the key (decisions 1, 2 and 4) ---------------------------------------------------------------

#[test]
fn a_v3_claim_may_name_an_area_as_a_path_of_stable_keys() {
    for area in [
        "diagnostics",
        "diagnostics/startup",
        "interfaces/can-bus/frames",
        "sys-1/sw-2/0x",
        "9/9",
    ] {
        let text = claim_text("REQ-0001", "req.claim/3", Some(area), "");
        let claim = parse_claim(&text).unwrap_or_else(|e| panic!("{area}: {e}"));
        // Read through the claim's serialisation, so this test compiles on a build without the field.
        let value = serde_json::to_value(&claim).expect("a claim serialises");
        assert_eq!(value["area"], Value::String(area.to_string()), "{area}");
    }
    let none = parse_claim(&claim_text("REQ-0001", "req.claim/3", None, "")).expect("parses");
    let value = serde_json::to_value(&none).expect("a claim serialises");
    assert_eq!(value["area"], Value::Null, "absent by default");
}

#[test]
fn a_malformed_area_is_a_load_error_naming_the_claim_and_the_key() {
    // The message text is pinned: the claim, the key, the value, the grammar and the remedy, each
    // separated by one space (a line continuation once left 14-space runs inside the literal).
    let error = parse_claim(&claim_text("REQ-0007", "req.claim/3", Some("Diag"), ""))
        .expect_err("`Diag` is not an area path");
    assert_eq!(
        error,
        "REQ-0007 has a malformed `area`: \"Diag\" is not an area path: segments joined by `/`, \
         each `[a-z0-9][a-z0-9-]*`, with no empty segment and no leading or trailing `/` (for \
         example `diagnostics/startup`). A segment is a stable key, not a heading or a section \
         number; its reader-facing name is the label in `.req/areas`"
    );
    for bad in [
        "Diag", "a//b", "/a", "a/", "\"a b\"", "-a", "\"\"", "a/-b", "\"a/B\"", "\"é\"", "a_b",
    ] {
        let s = Scratch::new("bad-area");
        s.claim("REQ-0007", "req.claim/3", Some(bad), "");
        let out = s.req(&["canon"]);
        assert_eq!(out.code, 3, "area {bad}: a load error:\n{}", out.all());
        assert!(
            out.all().contains("REQ-0007") && out.all().contains("`area`"),
            "area {bad}: names the claim and the key:\n{}",
            out.all()
        );
    }
}

#[test]
fn a_claim_has_one_area_so_a_list_is_refused() {
    let s = Scratch::new("two-areas");
    s.claim("REQ-0001", "req.claim/3", Some("[a, b]"), "");
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 3, "a claim is filed in one place:\n{}", out.all());
}

#[test]
fn an_undeclared_area_still_loads() {
    // Falsification 3: a finding, never a load error (ADR 0006: friction at the decision boundary).
    let s = Scratch::new("undeclared-loads");
    s.claim("REQ-0001", "req.claim/3", Some("never/declared"), "");
    s.write(".req/areas", "interfaces Interfaces\n");
    for command in [&["canon"][..], &["render"], &["trace"]] {
        let out = s.req(command);
        assert_ne!(
            out.code,
            3,
            "{command:?} must load the corpus:\n{}",
            out.all()
        );
    }
    assert_eq!(s.req(&["canon"]).code, 0);
}

#[test]
fn an_adopters_sections_are_paths_of_stable_keys() {
    // Falsification 5, as far as a fixture can say it: a three-level specification, its headings
    // carried by the source tool's stable object identifiers and not by section numbers.
    let s = Scratch::new("adopter");
    for (n, area) in [
        "system/interfaces",
        "system/interfaces/can-bus",
        "software/diagnostics/startup",
        "software/diagnostics/shutdown",
    ]
    .iter()
    .enumerate()
    {
        s.claim(&format!("REQ-000{}", n + 1), "req.claim/3", Some(area), "");
    }
    assert_eq!(s.req(&["canon"]).code, 0);
}

#[test]
fn canon_prints_a_claims_area_on_its_line() {
    let s = Scratch::new("canon-area");
    s.claim("REQ-0001", "req.claim/3", Some("diagnostics/startup"), "");
    s.claim("REQ-0002", "req.claim/3", None, "");
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let line = |id: &str| {
        out.stdout
            .lines()
            .find(|l| l.starts_with(id))
            .unwrap_or_else(|| panic!("a line for {id}:\n{}", out.stdout))
            .to_string()
    };
    assert!(
        line("REQ-0001").contains("diagnostics/startup"),
        "{}",
        line("REQ-0001")
    );
    assert!(!line("REQ-0002").contains("area"), "{}", line("REQ-0002"));
}

// --- outside identity and outside the Baseline (decision 4; falsifications 1 and 6) ---------------

#[test]
fn the_area_is_not_an_input_to_the_address() {
    let without = Scratch::new("addr-no-area");
    without.claim("REQ-0001", "req.claim/3", None, "");
    let with = Scratch::new("addr-area");
    with.claim("REQ-0001", "req.claim/3", Some("diagnostics/startup"), "");
    let moved = Scratch::new("addr-moved");
    moved.claim("REQ-0001", "req.claim/3", Some("interfaces"), "");
    assert_eq!(addresses(&without), addresses(&with));
    assert_eq!(addresses(&with), addresses(&moved));
}

/// The repository's own corpus, copied: a Baseline needs a decision log and claims that already
/// emit, and this is the one corpus here that does.
fn repo_copy(name: &str) -> Scratch {
    let s = Scratch::new(name);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::remove_dir_all(s.path().join("claims")).expect("clears claims");
    for dir in ["claims", "decisions", "aliases"] {
        copy_dir(&repo.join(dir), &s.path().join(dir));
    }
    s
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("creates the dir");
    for entry in fs::read_dir(from).expect("reads the dir") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copies the file");
        }
    }
}

const SELECTION: [&str; 4] = [
    "corpus_set=parity",
    "set=direction_gold_structural",
    "deployment_mode=air_gapped",
    "field=value",
];

fn emit(s: &Scratch) -> String {
    let mut args = vec!["baseline", "--at", "2026-11-13T00:00:00Z"];
    args.extend(SELECTION);
    let out = s.req(&args);
    assert_eq!(out.code, 0, "the Baseline emits:\n{}", out.all());
    out.stdout
}

#[test]
fn adding_changing_or_removing_an_area_moves_no_byte_of_the_baseline() {
    let plain = repo_copy("baseline-plain");
    let reference = emit(&plain);
    let addresses_before = addresses_under(&plain, &SELECTION);

    // The same claim, declared /3 with an area: nothing signed moves.
    let path = plain.path().join("claims/REQ-0001.yml");
    let original = fs::read_to_string(&path).expect("reads REQ-0001");
    assert!(
        original.contains("schema: req.claim/2"),
        "the repo's claims stay /2"
    );
    let filed = |area: &str| {
        original.replacen(
            "schema: req.claim/2\n",
            &format!("schema: req.claim/3\narea: {area}\n"),
            1,
        )
    };
    for area in ["diagnostics/startup", "interfaces"] {
        fs::write(&path, filed(area)).expect("files the claim");
        assert_eq!(emit(&plain), reference, "area {area} moved the Baseline");
        assert_eq!(
            addresses_under(&plain, &SELECTION),
            addresses_before,
            "area {area} moved an address"
        );
    }
    // Removed again (declared /3, no area): still the same bytes.
    fs::write(&path, original.replacen("req.claim/2", "req.claim/3", 1)).expect("rewrites");
    assert_eq!(emit(&plain), reference);
}

#[test]
fn an_area_changes_no_applicability_verdict_or_derived_parse() {
    // Falsification 6, on a claim a variant selects: applicability, the gate's verdict and the
    // derived parse are the same with and without an area.
    let build = |name: &str, area: Option<&str>| {
        let s = Scratch::new(name);
        s.claim(
            "REQ-0001",
            "req.claim/3",
            area,
            "pins:\n  operator: LE\n  value: \"0.2\"\n  unit: s\n",
        );
        s.write(
            "claims/REQ-0002.yml",
            &claim_text("REQ-0002", "req.claim/3", area, "")
                .replace("applicability: []", "applicability:\n  - mode == a"),
        );
        s
    };
    let bare = build("verdict-bare", None);
    let filed = build("verdict-filed", Some("diagnostics/startup"));

    for context in ["mode=a", "mode=b"] {
        let a = bare.req(&["gate", context]);
        let b = filed.req(&["gate", context]);
        assert_eq!(
            a.code,
            b.code,
            "{context}: gate exit\n{}\n{}",
            a.all(),
            b.all()
        );
        assert_eq!(a.stdout, b.stdout, "{context}: gate report");

        let strip = |s: &Scratch| -> Value {
            let mut doc = s.req(&["render", context, "--format", "json"]).json();
            for section in ["applies", "undecided", "outside"] {
                for row in doc[section].as_array_mut().expect("a section") {
                    row.as_object_mut().expect("a row").remove("area");
                }
            }
            doc
        };
        assert_eq!(
            strip(&bare),
            strip(&filed),
            "{context}: the rows, area aside"
        );
    }
    // The parse is the same line, area aside.
    let parse = |s: &Scratch| -> Vec<String> {
        s.req(&["canon"])
            .stdout
            .lines()
            .filter(|l| l.starts_with("REQ-"))
            .map(|l| l.replace(" area diagnostics/startup", ""))
            .collect()
    };
    assert_eq!(parse(&bare), parse(&filed));
}

#[test]
fn the_views_carry_area_and_null_when_absent() {
    let s = Scratch::new("views");
    s.claim("REQ-0001", "req.claim/3", Some("diagnostics/startup"), "");
    s.claim("REQ-0002", "req.claim/2", None, "");

    let render = s.req(&["render", "--format", "json"]);
    assert_eq!(render.code, 0, "{}", render.all());
    let doc = render.json();
    let rows = doc["applies"].as_array().expect("applies");
    let area_of = |id: &str| -> Value {
        rows.iter()
            .find(|r| r["claim_id"] == id)
            .unwrap_or_else(|| panic!("{id} applies"))["area"]
            .clone()
    };
    assert_eq!(
        area_of("REQ-0001"),
        Value::String("diagnostics/startup".into())
    );
    assert_eq!(
        area_of("REQ-0002"),
        Value::Null,
        "absent is null, never missing"
    );

    let trace = s.req(&["trace", "--format", "json"]);
    let doc = trace.json();
    let nodes = doc["nodes"].as_array().expect("nodes");
    let node = |id: &str| nodes.iter().find(|n| n["claim_id"] == id).expect("a node");
    assert_eq!(node("REQ-0001")["area"], "diagnostics/startup");
    assert_eq!(node("REQ-0002")["area"], Value::Null);
    // The field order is the document's contract: `area` follows `superseded_by`.
    let text = &trace.stdout;
    assert!(
        text.find("\"superseded_by\"").expect("superseded_by")
            < text.find("\"area\"").expect("area"),
        "area follows superseded_by:\n{text}"
    );
}

#[test]
fn text_projections_do_not_change_with_an_area() {
    let bare = Scratch::new("text-bare");
    bare.claim("REQ-0001", "req.claim/3", None, "");
    let filed = Scratch::new("text-filed");
    filed.claim("REQ-0001", "req.claim/3", Some("diagnostics/startup"), "");
    for command in [&["render"][..], &["trace"]] {
        assert_eq!(
            bare.req(command).stdout,
            filed.req(command).stdout,
            "{command:?}: the text projection is unchanged, so a committed one passes --check"
        );
    }
}

#[test]
fn this_repositorys_claims_stay_v2() {
    // ADR 0051 decision 6, live: /3 loads every /2 claim as it is, so nothing here is migrated.
    let claims = Path::new(env!("CARGO_MANIFEST_DIR")).join("claims");
    let mut seen = 0;
    for entry in fs::read_dir(&claims).expect("claims/") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|e| e == "yml") {
            let text = fs::read_to_string(&path).expect("reads a claim");
            assert!(
                text.lines().any(|l| l == "schema: req.claim/2"),
                "{} stays req.claim/2",
                path.display()
            );
            assert!(
                !text.lines().any(|l| l.starts_with("area:")),
                "{}",
                path.display()
            );
            seen += 1;
        }
    }
    assert!(seen > 0);
    let out = run(Path::new(env!("CARGO_MANIFEST_DIR")), &["canon"]);
    assert_eq!(
        out.code,
        0,
        "the /2 corpus loads under a /3 build:\n{}",
        out.all()
    );
}

// --- `.req/areas` (decision 3) ---------------------------------------------------------------------

/// Two releases and every claim related and signed, as `tests/completeness.rs`'s `clean`: the step
/// passes, and each test then adds exactly one area fact.
fn filed(name: &str, areas: &[(&str, Option<&str>)]) -> Scratch {
    let s = Scratch::new(name);
    for (id, area) in areas {
        s.write(
            &format!("claims/{id}.yml"),
            &claim_text(id, "req.claim/3", *area, "").replace(
                "relations: []",
                if *id == "REQ-0001" {
                    "relations: []"
                } else {
                    "relations:\n  - type: refines\n    to: REQ-0001"
                },
            ),
        );
    }
    let ids: Vec<&str> = areas.iter().map(|(id, _)| *id).collect();
    let claims: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                "{{\"claim_id\":\"{id}\",\"address\":\"{h}\",\"relations_hash\":\"{h}\",\
                 \"rationale_hash\":\"{h}\",\"tests_hash\":\"{h}\"}}",
                h = "0".repeat(64)
            )
        })
        .collect();
    s.write(
        "baselines/release.json",
        &format!(
            "{{\"version\":4,\"signed_at\":\"2026-11-11T00:00:00Z\",\"authority\":{{\"head\":\"{h}\",\
             \"entries\":1}},\"rules_v\":8,\"selection\":{{}},\"claims\":[{}]}}",
            claims.join(","),
            h = "0".repeat(64)
        ),
    );
    s.write(
        "baselines/release.json.minisig",
        "untrusted comment: fixture\n",
    );
    s
}

#[test]
fn check_names_an_undeclared_area_and_still_passes() {
    let s = filed(
        "undeclared",
        &[
            ("REQ-0001", Some("interfaces")),
            ("REQ-0002", Some("diagnostics/startup")),
            ("REQ-0003", None),
        ],
    );
    s.write(".req/areas", "interfaces Interfaces\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "reported, not judged:\n{}", out.all());
    let block = out.completeness_block();
    let line = block
        .lines()
        .find(|l| l.contains("undeclared_area"))
        .unwrap_or_else(|| panic!("the finding is named:\n{block}"));
    assert!(
        line.contains("REQ-0002") && line.contains("diagnostics/startup"),
        "names the claim and the area: {line}"
    );
    assert!(
        !line.contains("REQ-0001") && !line.contains("REQ-0003"),
        "a declared area and no area are not findings: {line}"
    );
    assert!(block.starts_with("[PASS] completeness "), "{block}");

    let json = s.req(&["check", "--format", "json"]);
    let doc = json.json();
    let step = doc["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|st| st["kind"] == "completeness")
        .expect("a completeness step");
    let found = step["completeness"]["undeclared_area"]
        .as_array()
        .expect("undeclared_area is a list");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["claim_id"], "REQ-0002");
    assert_eq!(found[0]["area"], "diagnostics/startup");
}

#[test]
fn strict_areas_makes_an_undeclared_area_a_failure() {
    let s = filed(
        "strict-areas",
        &[
            ("REQ-0001", Some("interfaces")),
            ("REQ-0002", Some("diagnostics")),
        ],
    );
    s.write(".req/areas", "interfaces Interfaces\n");
    assert_eq!(s.req(&["check"]).code, 0, "without the word, reported only");
    s.write(".req/completeness", "strict areas\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[FAIL] completeness "), "{block}");
    assert!(
        block.contains("undeclared_area") && block.contains("REQ-0002"),
        "{block}"
    );

    // Declared, it is no finding, and the strict policy passes.
    s.write(
        ".req/areas",
        "interfaces Interfaces\ndiagnostics Diagnostics\n",
    );
    let ok = s.req(&["check"]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    assert!(!ok.all().contains("undeclared_area"), "{}", ok.all());
}

#[test]
fn with_no_registry_there_is_no_finding() {
    let s = filed(
        "no-registry",
        &[
            ("REQ-0001", Some("anything/at-all")),
            ("REQ-0002", Some("x")),
        ],
    );
    s.write(".req/completeness", "strict areas\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains("undeclared_area"), "{}", out.all());
    let json = s.req(&["check", "--format", "json"]);
    assert!(!json.stdout.contains("undeclared_area"), "{}", json.stdout);
}

#[test]
fn a_parent_need_not_be_declared_and_a_label_is_any_text() {
    let s = filed(
        "registry-grammar",
        &[
            ("REQ-0001", Some("diagnostics/startup")),
            ("REQ-0002", Some("schnittstellen")),
        ],
    );
    s.write(
        ".req/areas",
        "# the reading order\r\n\ndiagnostics/startup   Start-up (診断)  \r\nschnittstellen Schnittstellen und Prüfung\n",
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains("undeclared_area"), "{}", out.all());
}

#[test]
fn a_malformed_registry_is_refused_naming_every_bad_line() {
    let s = filed("registry-bad", &[("REQ-0001", Some("interfaces"))]);
    s.write(
        ".req/areas",
        "interfaces Interfaces\n\
         # a comment, line 2\n\
         interfaces Interfaces again\n\
         diagnostics\n\
         Bad/Path A label\n\
         ok-one Fine\n\
         a//b Two slashes\n",
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 2, "{}", out.all());
    for expected in [
        ".req/areas:3:", // a path declared twice
        ".req/areas:4:", // an empty label
        ".req/areas:5:", // a malformed path
        ".req/areas:7:", // a malformed path
    ] {
        assert!(
            out.stderr.contains(expected),
            "every bad line is named in one run, missing {expected}:\n{}",
            out.stderr
        );
    }
    assert!(!out.stderr.contains(".req/areas:1:"), "{}", out.stderr);
    assert!(!out.stderr.contains(".req/areas:6:"), "{}", out.stderr);
    assert!(
        !out.stdout.contains("completeness"),
        "nothing ran: a malformed list is refused before any step:\n{}",
        out.stdout
    );
}

#[test]
fn an_unreadable_registry_is_unverifiable_not_a_pass() {
    let s = filed("registry-dir", &[("REQ-0001", Some("interfaces"))]);
    fs::create_dir_all(s.path().join(".req/areas")).expect("a directory where the list should be");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains(".req/areas"), "{}", out.all());
}

#[test]
fn a_registry_may_not_declare_a_path_the_loader_would_refuse() {
    // The same grammar as the claim key: one rule, one home.
    for line in ["-a Label", "a/ Label", "/a Label", "A Label", "a_b Label"] {
        let s = filed("registry-path", &[("REQ-0001", None)]);
        s.write(".req/areas", &format!("{line}\n"));
        let out = s.req(&["check"]);
        assert_eq!(out.code, 2, "{line}: {}", out.all());
        assert!(
            out.stderr.contains(".req/areas:1:"),
            "{line}: {}",
            out.stderr
        );
    }
}
