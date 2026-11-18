//! `req.claim/4`, kind declarations and the loader that judges fields (ADR 0056, task F-1).
//!
//! The seam is the compiled `req` over throwaway corpora, plus `req::canon::parse_claim` (the file's
//! own shape). Every expectation comes from the ADR — decisions 1 to 5, decision 8's paragraph on
//! `req diff` reading each revision's declarations, decision 9 — and its *Falsification* list, never
//! from a run. The library seam (`req::kinds`) is `tests/kinds_seam.rs`.
//!
//! * 1 — a kind or a field moves an address, a selftest answer, `bench`, an applicability or a parse;
//! * 4 — `/3` or `/2` carrying `kind` or `fields` loads, a `/4` build refuses or rewrites `/3` or `/2`;
//! * 6 — an unknown kind, an unknown field or a wrong type loads; an absent value reaches a consumer
//!   as a default;
//! * 12 — `req diff` over a kinded corpus exits 3 for want of a revision's declarations;
//! * 13 — the release notes (`tests/release_notes.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use req::canon::{canonical_json, load_claims, parse_claim};

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
            "req-kinds-{}-{}-{name}",
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

    fn claim(&self, id: &str, schema: &str, extra: &str) {
        self.write(&format!("claims/{id}.yml"), &claim_text(id, schema, extra));
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

fn claim_text(id: &str, schema: &str, extra: &str) -> String {
    format!(
        "schema: {schema}\n\
         claim_id: {id}\n\
         statement: \"The {id} service shall respond to a health check within 200 ms.\"\n\
         applicability: []\n\
         relations: []\n\
         evidence: process\n\
         {extra}\
         rationale:\n  source: docs/requirements.md\n  date: 2026-11-18\n  rejected_alternatives: []\n  assumptions: []\n"
    )
}

/// The `safety` kind of ADR 0056 decision 2.
const SAFETY: &str = "\
schema: req.kind/1
label: Safety-related requirement
fields:
  asil:
    type: enum
    values: [QM, A, B, C, D]
    ordered: true
    required: true
  mitigated_by:
    type: list
    of: ref
  review_due:
    type: date
";

const KINDED: &str = "\
kind: safety
fields:
  asil: C
  mitigated_by: [REQ-0002, REQ-0007]
  review_due: 2027-03-01
";

/// The address of each claim `req render --format json` lists, by claim id.
fn addresses(s: &Scratch) -> Vec<(String, String)> {
    let out = s.req(&["render", "--format", "json"]);
    assert_eq!(out.code, 0, "render loads the corpus:\n{}", out.all());
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("a JSON document");
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

// --- the format (decision 1) -----------------------------------------------------------------------

#[test]
fn this_build_writes_claim_schema_4_and_rules_v_stays_9() {
    let version = run(Path::new("."), &["--version"]);
    assert_eq!(version.code, 0, "{}", version.all());
    let line = |key: &str| {
        version
            .stdout
            .lines()
            .find(|l| l.trim().starts_with(key))
            .map(|l| l.split_whitespace().last().unwrap_or_default().to_string())
    };
    assert_eq!(line("claim_schema").as_deref(), Some("req.claim/4"));
    assert_eq!(line("rules_v").as_deref(), Some("9"), "no rules bump");
    assert_eq!(req::canon::claim_schema(), "req.claim/4");
    assert_eq!(req::canon::RULES_V, 9);
    assert_eq!(req::canon::READS_AS_IS, &[2, 3]);
}

#[test]
fn req_init_writes_claim_schema_4_and_needs_no_kind() {
    let fresh = std::env::temp_dir().join(format!("req-kinds-init-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fresh);
    fs::create_dir_all(&fresh).expect("creates the dir");
    let init = run(&fresh, &["init"]);
    assert_eq!(init.code, 0, "{}", init.all());
    let starter = fs::read_to_string(fresh.join("claims/REQ-0001.yml")).expect("starter claim");
    assert!(
        starter.lines().any(|l| l == "schema: req.claim/4"),
        "{starter}"
    );
    assert!(
        !starter.contains("kind:") && !starter.contains("fields:"),
        "the starter files no kind:\n{starter}"
    );
    assert!(!fresh.join(".req/kinds").exists(), "init writes no kind");
    let canon = run(&fresh, &["canon"]);
    assert_eq!(canon.code, 0, "{}", canon.all());
    let _ = fs::remove_dir_all(&fresh);
}

#[test]
fn a_v3_claim_and_the_same_claim_declared_v4_with_no_kind_have_one_address_and_no_migration_note() {
    let three = Scratch::new("addr-v3");
    three.claim("REQ-0001", "req.claim/3", "");
    let four = Scratch::new("addr-v4");
    four.claim("REQ-0001", "req.claim/4", "");
    assert_eq!(addresses(&three), addresses(&four));
    for s in [&three, &four] {
        let canon = s.req(&["canon"]);
        assert_eq!(canon.code, 0, "{}", canon.all());
        assert!(
            !canon.all().contains("migration candidate") && !canon.all().contains("NOT JUDGED"),
            "neither is reported as a migration candidate:\n{}",
            canon.all()
        );
    }
    // Falsification 4: a /4 build neither refuses nor rewrites a /3 or /2 file.
    for declared in ["req.claim/2", "req.claim/3"] {
        let s = Scratch::new("loads-as-is");
        s.claim("REQ-0001", declared, "");
        let before = fs::read_to_string(s.path().join("claims/REQ-0001.yml")).expect("reads");
        let canon = s.req(&["canon"]);
        assert_eq!(canon.code, 0, "{declared}: {}", canon.all());
        let _ = s.req(&["render"]);
        let after = fs::read_to_string(s.path().join("claims/REQ-0001.yml")).expect("reads");
        assert_eq!(before, after, "{declared} is never rewritten");
    }
}

#[test]
fn a_newer_claim_format_is_not_judged() {
    let s = Scratch::new("v5");
    s.claim("REQ-0001", "req.claim/5", "");
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("NOT JUDGED") && out.all().contains("newer"),
        "{}",
        out.all()
    );
}

#[test]
fn a_v3_or_v2_file_carrying_kind_or_fields_is_a_parse_error_naming_the_key_and_the_format() {
    for schema in ["req.claim/3", "req.claim/2"] {
        for (key, extra) in [
            ("kind", "kind: safety\n"),
            ("fields", "fields:\n  asil: C\n"),
            // Judged on the KEY, as `area` is: a null value does not un-carry it.
            ("kind", "kind: ~\n"),
            ("fields", "fields: {}\n"),
        ] {
            let text = claim_text("REQ-0001", schema, extra);
            let error =
                parse_claim(&text).expect_err(&format!("{schema} carrying `{key}` must not load"));
            assert!(
                error.contains(&format!("`{key}`")),
                "{schema}: names the key `{key}`: {error}"
            );
            assert!(
                error.contains("req.claim/4"),
                "{schema}: names the format it needs: {error}"
            );

            let s = Scratch::new("old-with-kind");
            s.write("claims/REQ-0001.yml", &text);
            let out = s.req(&["canon"]);
            assert_eq!(out.code, 3, "{schema} {key}: {}", out.all());
            assert!(
                out.all().contains(&format!("`{key}`")) && out.all().contains("req.claim/4"),
                "{}",
                out.all()
            );
        }
    }
}

#[test]
fn fields_without_a_kind_is_a_parse_error() {
    let text = claim_text("REQ-0001", "req.claim/4", "fields:\n  asil: C\n");
    let error = parse_claim(&text).expect_err("a value has no type until a kind declares it");
    assert!(
        error.contains("`fields`") && error.contains("`kind`"),
        "names both keys: {error}"
    );
    let s = Scratch::new("fields-no-kind");
    s.write("claims/REQ-0001.yml", &text);
    s.write(".req/kinds/safety.yml", SAFETY);
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
}

#[test]
fn kind_and_field_names_are_lower_case_ascii_identifiers() {
    for (extra, named) in [
        ("kind: Safety\nfields:\n  asil: C\n", "Safety"),
        ("kind: safe-ty\nfields:\n  asil: C\n", "safe-ty"),
        ("kind: 1safety\nfields:\n  asil: C\n", "1safety"),
        ("kind: safety\nfields:\n  Asil: C\n", "Asil"),
        ("kind: safety\nfields:\n  as-il: C\n", "as-il"),
        ("kind: safety\nfields:\n  _asil: C\n", "_asil"),
    ] {
        let error = parse_claim(&claim_text("REQ-0001", "req.claim/4", extra))
            .expect_err(&format!("{named} is not a name"));
        assert!(error.contains(named), "{named}: {error}");
        assert!(error.contains("[a-z][a-z0-9_]*"), "{named}: {error}");
    }
    for ok in ["kind: safety\n", "kind: s2_x\n", "kind: a\n"] {
        parse_claim(&claim_text("REQ-0001", "req.claim/4", ok)).expect("a valid name parses");
    }
}

#[test]
fn area_stays_valid_in_v3_and_v4() {
    for schema in ["req.claim/3", "req.claim/4"] {
        parse_claim(&claim_text(
            "REQ-0001",
            schema,
            "area: diagnostics/startup\n",
        ))
        .unwrap_or_else(|e| panic!("{schema} carries area: {e}"));
    }
}

// --- outside identity (decision 5.1: L1, L2, L3, L4, L12) --------------------------------------------

#[test]
fn a_kind_and_three_fields_move_no_address_and_a_value_change_moves_none() {
    // L1: the same claim without a kind, with one, and with another value has one address and one
    // `canonical_json`.
    let plain = Scratch::new("l1-plain");
    plain.claim("REQ-0001", "req.claim/4", "");
    let kinded = Scratch::new("l1-kinded");
    kinded.write(".req/kinds/safety.yml", SAFETY);
    kinded.claim("REQ-0001", "req.claim/4", KINDED);
    let other = Scratch::new("l1-other");
    other.write(".req/kinds/safety.yml", SAFETY);
    other.claim(
        "REQ-0001",
        "req.claim/4",
        &KINDED.replace("asil: C", "asil: D"),
    );
    assert_eq!(addresses(&plain), addresses(&kinded));
    assert_eq!(addresses(&kinded), addresses(&other));

    let derive = |s: &Scratch| {
        let loaded = load_claims(s.path()).expect("loads");
        let d = &loaded[0].derived;
        (d.address.clone(), canonical_json(&d.form))
    };
    assert_eq!(derive(&plain), derive(&kinded));
    assert_eq!(derive(&kinded), derive(&other));
}

#[test]
fn a_field_changes_neither_the_applicability_verdict_nor_the_derived_parse() {
    // L3: the claim's row in every view, and the derived parse `req canon` prints, are the same with and
    // without a kind. L4 follows: no text projection prints a field.
    let plain = Scratch::new("l3-plain");
    plain.claim("REQ-0001", "req.claim/4", "");
    let kinded = Scratch::new("l3-kinded");
    kinded.write(".req/kinds/safety.yml", SAFETY);
    kinded.claim("REQ-0001", "req.claim/4", KINDED);
    for args in [
        &["render"][..],
        &["trace"][..],
        &["select"][..],
        &["render", "--format", "json"][..],
    ] {
        let (a, b) = (plain.req(args), kinded.req(args));
        assert_eq!(a.code, b.code, "{args:?}");
        let (a_out, b_out) = (a.stdout, b.stdout);
        if args.contains(&"json") {
            // L5 (`tests/field_reporting.rs`): the JSON views gain `kind` and `fields`. Here only the address and
            // the applicability sections are compared.
            let (da, db): (serde_json::Value, serde_json::Value) = (
                serde_json::from_str(&a_out).expect("json"),
                serde_json::from_str(&b_out).expect("json"),
            );
            for section in ["applies", "undecided", "outside"] {
                assert_eq!(da[section][0]["address"], db[section][0]["address"]);
                assert_eq!(
                    da[section].as_array().map(Vec::len),
                    db[section].as_array().map(Vec::len)
                );
            }
        } else {
            assert_eq!(a_out, b_out, "no text projection prints a field: {args:?}");
        }
        if !args.contains(&"json") {
            // The JSON views display `kind` and `fields` (L5, `tests/field_reporting.rs`); no text view does.
            assert!(
                !b_out.contains("asil")
                    && !b_out.contains("safety")
                    && !b_out.contains("2027-03-01"),
                "{args:?}:\n{b_out}"
            );
        }
    }
    let strip = |text: &str| {
        text.lines()
            .map(|l| l.split(" kind safety").next().unwrap_or(l).to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let (a, b) = (plain.req(&["canon"]), kinded.req(&["canon"]));
    assert_eq!(a.code, 0, "{}", a.all());
    assert_eq!(b.code, 0, "{}", b.all());
    assert_eq!(
        strip(&a.stdout),
        strip(&b.stdout),
        "the derived parse is the same"
    );
}

#[test]
fn selftest_and_bench_answer_as_before_with_a_kinded_corpus_beside_them() {
    // L2: `req selftest` reads no corpus and `req bench` reads statements, so neither moves.
    let s = Scratch::new("l2");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim("REQ-0001", "req.claim/4", KINDED);
    let selftest = run(Path::new("."), &["selftest"]);
    assert_eq!(selftest.code, 0, "{}", selftest.all());
    let bench = run(Path::new(env!("CARGO_MANIFEST_DIR")), &["bench"]);
    assert_eq!(bench.code, 0, "FMR = 0 holds:\n{}", bench.all());
}

#[test]
fn two_files_of_one_id_that_differ_only_in_a_field_are_a_duplicate_id() {
    // L12.
    let s = Scratch::new("l12");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim("REQ-0001", "req.claim/4", KINDED);
    s.write(
        "claims/REQ-0001.yaml",
        &claim_text(
            "REQ-0001",
            "req.claim/4",
            &KINDED.replace("asil: C", "asil: D"),
        ),
    );
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("duplicate claim_id REQ-0001"),
        "{}",
        out.all()
    );
}

#[test]
fn req_canon_prints_the_kind_and_the_fields_sorted_after_the_area() {
    let s = Scratch::new("canon-display");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim(
        "REQ-0001",
        "req.claim/4",
        "area: diagnostics/startup\nkind: safety\nfields:\n  review_due: 2027-03-01\n  mitigated_by: [REQ-0002, REQ-0007]\n  asil: C\n",
    );
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let line = out
        .stdout
        .lines()
        .find(|l| l.starts_with("REQ-0001"))
        .expect("the claim's line");
    assert!(
        line.ends_with(
            " area diagnostics/startup kind safety fields asil=C mitigated_by=[REQ-0002,REQ-0007] review_due=2027-03-01"
        ),
        "kind and fields follow the area, sorted by field name:\n{line}"
    );
}

#[test]
fn an_absent_field_is_absent_to_canon_never_a_default() {
    // Absent is UNKNOWN (decision 4): a required field a claim does not carry loads, and nothing
    // shows a value for it.
    let s = Scratch::new("absent");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: safety\nfields:\n  review_due: 2027-03-01\n",
    );
    s.claim("REQ-0002", "req.claim/4", "kind: safety\n");
    let out = s.req(&["canon"]);
    assert_eq!(
        out.code,
        0,
        "a missing required field loads:\n{}",
        out.all()
    );
    assert!(
        !out.all().contains("asil"),
        "no value, no default, no null:\n{}",
        out.all()
    );
    assert!(
        !out.all().contains("null") && !out.all().contains("QM"),
        "{}",
        out.all()
    );
    let line2 = out
        .stdout
        .lines()
        .find(|l| l.starts_with("REQ-0002"))
        .expect("line");
    assert!(
        line2.contains(" kind safety") && !line2.contains("fields"),
        "{line2}"
    );
}

#[test]
fn a_ref_that_points_at_no_claim_loads() {
    let s = Scratch::new("dangling-ref");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: safety\nfields:\n  asil: C\n  mitigated_by: [REQ-0999]\n",
    );
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 0, "the finding is F-3's:\n{}", out.all());
}

// --- judging a value (decisions 3 and 4) --------------------------------------------------------------

/// One declared field `level` of kind `safety`, one claim carrying `level: <value>`; `canon`'s answer.
struct Probe {
    scratch: Scratch,
}

impl Probe {
    fn new(declaration: &str) -> Probe {
        let scratch = Scratch::new("probe");
        scratch.write(
            ".req/kinds/safety.yml",
            &format!("schema: req.kind/1\nfields:\n  level:\n    {declaration}\n"),
        );
        Probe { scratch }
    }

    fn value(&self, value: &str) -> Out {
        self.scratch.claim(
            "REQ-0001",
            "req.claim/4",
            &format!("kind: safety\nfields:\n  level: {value}\n"),
        );
        self.scratch.req(&["canon"])
    }

    fn accepts(&self, value: &str, shown: Option<&str>) {
        let out = self.value(value);
        assert_eq!(out.code, 0, "`{value}` is accepted:\n{}", out.all());
        if let Some(shown) = shown {
            assert!(
                out.stdout.contains(&format!("fields level={shown}")),
                "`{value}` is shown as `{shown}`:\n{}",
                out.stdout
            );
        }
    }

    fn refuses(&self, value: &str, alternatives: &[&str]) {
        let out = self.value(value);
        assert_eq!(
            out.code,
            3,
            "`{value}` is refused, NOT JUDGED:\n{}",
            out.all()
        );
        let text = out.all();
        for named in ["REQ-0001", "safety", "level"].iter().chain(alternatives) {
            assert!(
                text.contains(named),
                "`{value}`: the refusal names `{named}`:\n{text}"
            );
        }
        assert!(
            out.stdout.is_empty() || !out.stdout.contains("REQ-0001 "),
            "nothing is printed for a claim that did not load:\n{}",
            out.stdout
        );
    }
}

#[test]
fn a_string_is_one_line_of_text_a_plain_typed_scalar_must_be_quoted() {
    let p = Probe::new("type: string");
    for ok in ["hello", "\"3\"", "\"true\"", "yes", "\"null\""] {
        p.accepts(ok, None);
    }
    for refused in [
        "\"\"",
        "\"a\\nb\"",
        "\"a\\tb\"",
        "\" lead\"",
        "\"trail \"",
        "true",
        "3",
        "1.5",
        "null",
        "~",
    ] {
        p.refuses(refused, &["string"]);
    }
}

#[test]
fn an_integer_is_a_quoted_canonical_integer_within_i64() {
    let p = Probe::new("type: integer");
    p.accepts("\"3\"", Some("3"));
    p.accepts("\"-12\"", Some("-12"));
    p.accepts("\"9223372036854775807\"", Some("9223372036854775807"));
    for refused in [
        "3",
        "\"007\"",
        "\"+1\"",
        "\"-0\"",
        "\"9223372036854775808\"",
        "\"1.0\"",
        "\"\"",
        "null",
        "true",
    ] {
        p.refuses(refused, &["integer"]);
    }
}

#[test]
fn a_decimal_is_a_quoted_canonical_exact_decimal() {
    let p = Probe::new("type: decimal");
    p.accepts("\"0.3\"", Some("0.3"));
    p.accepts("\"-0.5\"", Some("-0.5"));
    p.accepts("\"3\"", Some("3"));
    for refused in [
        "0.3", "\"0.30\"", "\".3\"", "\"3e-1\"", "\"1E2\"", "null", "true",
    ] {
        p.refuses(refused, &["decimal"]);
    }
}

#[test]
fn a_bool_is_a_scalar_the_loader_delivers_as_a_boolean_and_its_spellings_are_one_value() {
    let p = Probe::new("type: bool");
    for ok in ["true", "True", "TRUE"] {
        p.accepts(ok, Some("true"));
    }
    for ok in ["false", "False", "FALSE"] {
        p.accepts(ok, Some("false"));
    }
    for refused in ["yes", "on", "\"true\"", "1", "null"] {
        p.refuses(refused, &["bool"]);
    }
}

#[test]
fn an_enum_is_one_of_the_declared_values_compared_exactly() {
    let p = Probe::new("type: enum\n    values: [QM, A, B, C, D]");
    p.accepts("C", Some("C"));
    for refused in ["E", "c", "\"C \"", "3", "null", "[C]"] {
        p.refuses(refused, &["QM", "A", "B", "C", "D"]);
    }
    // A token YAML would type is quoted in the claim, as a `string` is.
    let q = Probe::new("type: enum\n    values: [\"true\", \"3\", x.y-z_1]");
    q.accepts("\"true\"", Some("true"));
    q.accepts("\"3\"", Some("3"));
    q.accepts("x.y-z_1", Some("x.y-z_1"));
    q.refuses("true", &["true", "3", "x.y-z_1"]);
    q.refuses("3", &["true", "3", "x.y-z_1"]);
}

#[test]
fn a_ref_is_a_claim_id_in_the_id_grammar() {
    let p = Probe::new("type: ref");
    p.accepts("REQ-0001", Some("REQ-0001"));
    p.accepts("REQ-0999", Some("REQ-0999"));
    for refused in [
        "\"REQ-\"",
        "req-0001",
        "\"0001\"",
        "REQ-00x1",
        "1",
        "null",
        "\"REQ-0001 \"",
    ] {
        p.refuses(refused, &["ref"]);
    }
}

#[test]
fn a_date_is_text_calendar_valid_with_no_clock_and_no_zone() {
    let p = Probe::new("type: date");
    for ok in [
        "2027-03-01",
        "\"2027-03-01\"",
        "\"2028-02-29\"",
        "\"2000-02-29\"",
        "\"2027-12-31\"",
    ] {
        p.accepts(ok, None);
    }
    p.accepts("2027-03-01", Some("2027-03-01"));
    for refused in [
        "\"2027-02-29\"",
        "\"2100-02-29\"",
        "\"2027-13-01\"",
        "\"2027-00-10\"",
        "\"2027-04-31\"",
        "\"2027-03-00\"",
        "\"2027-1-1\"",
        "\"27-03-01\"",
        "\"2027-03-01T00:00:00Z\"",
        "\"2027-03-01 \"",
        "20270301",
        "null",
    ] {
        p.refuses(refused, &["date"]);
    }
}

#[test]
fn a_list_is_a_set_of_one_element_type_and_a_duplicate_is_refused() {
    let strings = Probe::new("type: list\n    of: string");
    strings.accepts("[a, b]", Some("[a,b]"));
    strings.accepts("[]", Some("[]"));
    for refused in [
        "[a, a]", "[1]", "[null]", "[[a]]", "[{a: b}]", "a", "null", "{a: b}",
    ] {
        strings.refuses(refused, &["list", "string"]);
    }
    let integers = Probe::new("type: list\n    of: integer");
    integers.accepts("[\"1\", \"2\"]", Some("[1,2]"));
    integers.refuses("[1]", &["list", "integer"]);
    integers.refuses("[\"1\", \"1\"]", &["list", "integer"]);
    let enums = Probe::new("type: list\n    of: enum\n    values: [A, B]");
    enums.accepts("[B, A]", None);
    enums.refuses("[A, C]", &["A", "B"]);
    enums.refuses("[A, A]", &["A", "B"]);
    let refs = Probe::new("type: list\n    of: ref");
    refs.accepts("[REQ-0002, REQ-0007]", Some("[REQ-0002,REQ-0007]"));
    refs.refuses("[REQ-0002, nope]", &["ref"]);
    let dates = Probe::new("type: list\n    of: date");
    dates.accepts("[2027-03-01, 2027-04-01]", None);
    dates.refuses("[\"2027-02-29\"]", &["date"]);
    let decimals = Probe::new("type: list\n    of: decimal");
    decimals.accepts("[\"0.3\", \"1\"]", Some("[0.3,1]"));
    decimals.refuses("[0.3]", &["decimal"]);
}

#[test]
fn a_kind_with_no_declaration_a_field_not_declared_and_a_null_are_refused() {
    // Every kind when `.req/kinds/` does not exist: no claim is accepted untyped.
    let none = Scratch::new("no-kinds-dir");
    none.claim("REQ-0001", "req.claim/4", KINDED);
    let out = none.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let text = out.all();
    assert!(
        text.contains("REQ-0001") && text.contains("safety") && text.contains(".req/kinds"),
        "names the claim, the kind and where the declaration should be:\n{text}"
    );
    // A kind with no file, beside another kind that has one.
    let other = Scratch::new("other-kind");
    other.write(".req/kinds/safety.yml", SAFETY);
    other.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: security\nfields:\n  asil: C\n",
    );
    let out = other.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("security") && out.all().contains("REQ-0001"),
        "{}",
        out.all()
    );
    // A field the kind does not declare names the ones it does.
    let undeclared = Scratch::new("undeclared-field");
    undeclared.write(".req/kinds/safety.yml", SAFETY);
    undeclared.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: safety\nfields:\n  asill: C\n",
    );
    let out = undeclared.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    for named in [
        "REQ-0001",
        "safety",
        "asill",
        "asil",
        "mitigated_by",
        "review_due",
    ] {
        assert!(out.all().contains(named), "names `{named}`:\n{}", out.all());
    }
    // `null` is neither absent nor a value.
    let null = Scratch::new("null-field");
    null.write(".req/kinds/safety.yml", SAFETY);
    null.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: safety\nfields:\n  asil: null\n",
    );
    let out = null.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("asil") && out.all().contains("null"),
        "{}",
        out.all()
    );
}

#[test]
fn every_bad_claim_is_named_in_one_run_in_path_order() {
    let s = Scratch::new("collected");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim(
        "REQ-0003",
        "req.claim/4",
        "kind: safety\nfields:\n  asil: E\n",
    );
    s.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: safety\nfields:\n  asil: Z\n",
    );
    s.claim(
        "REQ-0002",
        "req.claim/4",
        "kind: safety\nfields:\n  asil: C\n",
    );
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let text = out.all();
    let (one, three) = (text.find("REQ-0001"), text.find("REQ-0003"));
    assert!(
        one.is_some() && three.is_some() && one < three,
        "path order:\n{text}"
    );
    assert!(
        !text.contains("REQ-0002.yml"),
        "the good claim is not named:\n{text}"
    );
}

#[test]
fn a_wrong_value_is_exit_3_in_every_command_that_loads_claims() {
    let s = Scratch::new("wrong-everywhere");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim(
        "REQ-0001",
        "req.claim/4",
        "kind: safety\nfields:\n  asil: E\n",
    );
    for args in [
        &["canon"][..],
        &["render"][..],
        &["trace"][..],
        &["select"][..],
        &["impact", "REQ-0001"][..],
        &["gate"][..],
        &["check"][..],
        &["audit"][..],
        &["baseline", "--at", "2026-11-18T00:00:00Z"][..],
    ] {
        let out = s.req(args);
        assert_eq!(out.code, 3, "{args:?} is NOT JUDGED:\n{}", out.all());
        assert!(out.all().contains("asil"), "{args:?}:\n{}", out.all());
    }
}

// --- the declarations (decision 2) ---------------------------------------------------------------------

#[test]
fn a_malformed_declaration_is_exit_2_naming_every_bad_file_and_key_in_one_run() {
    let s = Scratch::new("malformed");
    s.claim("REQ-0001", "req.claim/4", "");
    s.write(
        ".req/kinds/colour.yml",
        "schema: req.kind/1\ncolour: red\nfields: {}\n",
    );
    s.write(
        ".req/kinds/defaulted.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: string\n    default: x\n",
    );
    s.write(
        ".req/kinds/Bad-Stem.yml",
        "schema: req.kind/1\nfields: {}\n",
    );
    s.write(".req/kinds/notes.txt", "a stray file\n");
    fs::create_dir_all(s.path().join(".req/kinds/sub")).expect("makes a subdirectory");
    s.write(
        ".req/kinds/ordered_string.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: string\n    ordered: true\n",
    );
    s.write(
        ".req/kinds/dup_enum.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: enum\n    values: [A, B, A]\n",
    );
    s.write(
        ".req/kinds/empty_enum.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: enum\n    values: []\n",
    );
    s.write(
        ".req/kinds/bad_token.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: enum\n    values: [\"a b\"]\n",
    );
    s.write(
        ".req/kinds/values_on_string.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: string\n    values: [a]\n",
    );
    s.write(
        ".req/kinds/of_on_string.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: string\n    of: string\n",
    );
    s.write(
        ".req/kinds/list_of_bool.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: list\n    of: bool\n",
    );
    s.write(
        ".req/kinds/list_of_list.yml",
        "schema: req.kind/1\nfields:\n  level:\n    type: list\n    of: list\n",
    );
    s.write(
        ".req/kinds/bad_field_name.yml",
        "schema: req.kind/1\nfields:\n  Level:\n    type: string\n",
    );
    let out = s.req(&["canon"]);
    assert_eq!(
        out.code,
        2,
        "a malformed declaration is exit 2, never 3:\n{}",
        out.all()
    );
    let text = out.all();
    for (file, key) in [
        ("colour.yml", "colour"),
        ("defaulted.yml", "default"),
        ("Bad-Stem.yml", "Bad-Stem"),
        ("notes.txt", "notes.txt"),
        ("sub", "sub"),
        ("ordered_string.yml", "ordered"),
        ("dup_enum.yml", "values"),
        ("empty_enum.yml", "values"),
        ("bad_token.yml", "values"),
        ("values_on_string.yml", "values"),
        ("of_on_string.yml", "of"),
        ("list_of_bool.yml", "bool"),
        ("list_of_list.yml", "list"),
        ("bad_field_name.yml", "Level"),
    ] {
        let line = text
            .lines()
            .find(|l| l.contains(file))
            .unwrap_or_else(|| panic!("`{file}` is named:\n{text}"));
        assert!(line.contains(key), "`{file}`'s line names `{key}`:\n{line}");
    }
}

#[test]
fn every_command_that_loads_claims_reads_a_malformed_declaration_as_exit_2() {
    // The class is the same whichever command meets it: none folds it into 3.
    let s = Scratch::new("malformed-everywhere");
    s.claim("REQ-0001", "req.claim/4", "");
    s.write(
        ".req/kinds/safety.yml",
        "schema: req.kind/1\ndefault: x\nfields: {}\n",
    );
    s.write("a.yml", &claim_text("REQ-0001", "req.claim/4", ""));
    s.write("b.yml", &claim_text("REQ-0001", "req.claim/4", ""));
    for args in [
        &["canon"][..],
        &["render"][..],
        &["trace"][..],
        &["trace", "--format", "json"][..],
        &["select"][..],
        &["impact", "REQ-0001"][..],
        &["gate"][..],
        &["check"][..],
        &["audit"][..],
        &["satisfy"][..],
        &["baseline", "--at", "2026-11-18T00:00:00Z"][..],
        &["compare", "a.yml", "b.yml"][..],
    ] {
        let out = s.req(args);
        assert_eq!(out.code, 2, "{args:?}:\n{}", out.all());
        assert!(
            out.all().contains("safety.yml"),
            "{args:?} names the file:\n{}",
            out.all()
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_under_the_declarations_is_a_stray_file() {
    let s = Scratch::new("symlink");
    s.claim("REQ-0001", "req.claim/4", "");
    fs::create_dir_all(s.path().join(".req/kinds")).expect("makes the dir");
    s.write("elsewhere.yml", SAFETY);
    std::os::unix::fs::symlink(
        s.path().join("elsewhere.yml"),
        s.path().join(".req/kinds/safety.yml"),
    )
    .expect("links");
    let out = s.req(&["canon"]);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.all().contains("safety.yml"), "{}", out.all());
}

#[test]
fn a_newer_declaration_format_is_not_judged_and_an_unreadable_one_is_exit_3() {
    let newer = Scratch::new("kind-v2");
    newer.claim("REQ-0001", "req.claim/4", "");
    newer.write(
        ".req/kinds/safety.yml",
        &SAFETY.replace("req.kind/1", "req.kind/2"),
    );
    let out = newer.req(&["canon"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("req.kind/2")
            && out.all().contains("NOT JUDGED")
            && out.all().contains("newer"),
        "{}",
        out.all()
    );

    let unreadable = Scratch::new("kind-unreadable");
    unreadable.claim("REQ-0001", "req.claim/4", "");
    fs::create_dir_all(unreadable.path().join(".req/kinds")).expect("makes the dir");
    fs::write(
        unreadable.path().join(".req/kinds/safety.yml"),
        [0xff_u8, 0xfe, 0x00, 0x9f],
    )
    .expect("writes bytes that are not UTF-8");
    let out = unreadable.req(&["canon"]);
    assert_eq!(
        out.code,
        3,
        "an unreadable declaration is exit 3:\n{}",
        out.all()
    );
    assert!(out.all().contains("safety.yml"), "{}", out.all());
}

#[test]
fn a_corpus_with_no_declarations_and_no_kind_loads_exactly_as_today() {
    let s = Scratch::new("as-today");
    s.claim("REQ-0001", "req.claim/3", "");
    s.claim("REQ-0002", "req.claim/4", "");
    for args in [&["canon"][..], &["render"][..], &["trace"][..]] {
        let out = s.req(args);
        assert_eq!(out.code, 0, "{args:?}: {}", out.all());
    }
    let canon = s.req(&["canon"]);
    assert!(
        !canon.stdout.contains(" kind ") && !canon.stdout.contains(" fields "),
        "no kind, no display:\n{}",
        canon.stdout
    );
}

// --- `req compare` (decision 4: both files are judged by the working tree's declarations) ---------

#[test]
fn compare_judges_a_v4_files_fields_by_the_working_trees_declarations() {
    let s = Scratch::new("compare");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.write("a.yml", &claim_text("REQ-0001", "req.claim/4", KINDED));
    s.write(
        "b.yml",
        &claim_text(
            "REQ-0001",
            "req.claim/4",
            &KINDED.replace("asil: C", "asil: D"),
        ),
    );
    let out = s.req(&["compare", "a.yml", "b.yml"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout.contains("REQ-0001 -> REQ-0001: identical"),
        "the verdict is about meaning and stays identical:\n{}",
        out.stdout
    );
    // A wrong value is exit 3, as at load.
    s.write(
        "c.yml",
        &claim_text(
            "REQ-0001",
            "req.claim/4",
            &KINDED.replace("asil: C", "asil: E"),
        ),
    );
    let out = s.req(&["compare", "a.yml", "c.yml"]);
    assert_eq!(out.code, 3, "{}", out.all());
    for named in ["c.yml", "safety", "asil", "QM"] {
        assert!(out.all().contains(named), "names `{named}`:\n{}", out.all());
    }
    // A file the declarations do not know is refused, side b or side a.
    s.write(
        "d.yml",
        &claim_text("REQ-0001", "req.claim/4", "kind: security\n"),
    );
    for args in [["compare", "a.yml", "d.yml"], ["compare", "d.yml", "a.yml"]] {
        let out = s.req(&args);
        assert_eq!(out.code, 3, "{args:?}: {}", out.all());
        assert!(out.all().contains("security"), "{}", out.all());
    }
    // A file with no kind keeps its verdict and exit code.
    s.write("e.yml", &claim_text("REQ-0001", "req.claim/4", ""));
    let out = s.req(&["compare", "a.yml", "e.yml"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("identical"), "{}", out.stdout);
}

// --- `req diff` stages the declarations (decision 8) -------------------------------------------------

fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "req kinds test")
        .env("GIT_AUTHOR_EMAIL", "kinds@example.invalid")
        .env("GIT_COMMITTER_NAME", "req kinds test")
        .env("GIT_COMMITTER_EMAIL", "kinds@example.invalid")
        .stdin(Stdio::null())
        .output()
        .expect("runs git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit(s: &Scratch, tag: &str) {
    git_in(s.path(), &["add", "-A"]);
    git_in(s.path(), &["commit", "-q", "--allow-empty", "-m", tag]);
    git_in(s.path(), &["tag", tag]);
}

fn kinded_repo(name: &str) -> Scratch {
    let s = Scratch::new(name);
    git_in(s.path(), &["init", "-q", "-b", "main"]);
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim("REQ-0001", "req.claim/4", KINDED);
    commit(&s, "base");
    s
}

#[test]
fn diff_over_a_kinded_corpus_loads_both_sides_with_their_declarations() {
    // Falsification 12, the non-reporting half: `req diff` over a kinded corpus does not exit 3 for want of
    // declarations. Each side is judged by its own.
    let s = kinded_repo("diff-kinded");
    s.claim(
        "REQ-0001",
        "req.claim/4",
        &KINDED.replace("asil: C", "asil: D"),
    );
    commit(&s, "head");
    let out = s.req(&["diff", "--base", "base", "--head", "head"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains("UNVERIFIABLE"), "{}", out.all());

    // A side is judged by ITS declarations: the head's kind gains a value, and the claim uses it.
    let s = kinded_repo("diff-own-declarations");
    s.write(
        ".req/kinds/safety.yml",
        &SAFETY.replace("[QM, A, B, C, D]", "[QM, A, B, C, D, E]"),
    );
    s.claim(
        "REQ-0001",
        "req.claim/4",
        &KINDED.replace("asil: C", "asil: E"),
    );
    commit(&s, "head");
    let out = s.req(&["diff", "--base", "base", "--head", "head"]);
    assert_eq!(
        out.code,
        0,
        "the head's value is valid under the head's declarations:\n{}",
        out.all()
    );
}

#[test]
fn a_malformed_declaration_at_a_staged_revision_is_exit_3_naming_the_file_and_the_revision() {
    let s = kinded_repo("diff-malformed");
    s.write(".req/kinds/safety.yml", &format!("{SAFETY}colour: red\n"));
    commit(&s, "head");
    // The working tree is not read by `diff`; the staged `head` is.
    fs::write(s.path().join(".req/kinds/safety.yml"), SAFETY).expect("repairs the working tree");
    let out = s.req(&["diff", "--base", "base", "--head", "head"]);
    assert_eq!(
        out.code,
        3,
        "a staged side that cannot load is exit 3, ADR 0026 §3:\n{}",
        out.all()
    );
    assert!(
        out.all().contains("head:.req/kinds/safety.yml"),
        "names the file and the revision:\n{}",
        out.all()
    );
    // The staging directory is `req-baseline-<pid>-…` (`baseline::staging_dir_name`). The scratch repository's own
    // root is printed on the `corpus root:` line and may sit under any temporary directory, so the test names the
    // staging directory and never the location of `$TMPDIR`.
    assert!(
        !out.all().contains("req-baseline-"),
        "never the staging path:\n{}",
        out.all()
    );

    // And a revision that dropped the declarations while a claim still names the kind.
    let s = kinded_repo("diff-missing");
    fs::remove_file(s.path().join(".req/kinds/safety.yml")).expect("removes the declaration");
    commit(&s, "head");
    let out = s.req(&["diff", "--base", "base", "--head", "head"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("safety") && out.all().contains("head"),
        "{}",
        out.all()
    );
}

#[test]
fn a_malformed_declaration_in_the_working_tree_is_exit_2_for_diff_and_a_staged_one_stays_3() {
    // ADR 0056 decision 8: exit 3 is for a staged revision; decision 2's exit 2 is for the working tree a command
    // reads directly. `req diff` with no `--head` reads the working tree as its head side.
    let s = kinded_repo("diff-working-tree");
    s.write(".req/kinds/safety.yml", &format!("{SAFETY}colour: red\n"));
    let out = s.req(&["diff", "--base", "base"]);
    assert_eq!(
        out.code,
        2,
        "the working tree's bad declaration: {}",
        out.all()
    );
    assert!(
        out.all().contains("safety.yml") && out.all().contains("colour"),
        "{}",
        out.all()
    );

    // The same file, staged at a revision, is the side failing to load.
    commit(&s, "head");
    fs::write(s.path().join(".req/kinds/safety.yml"), SAFETY).expect("repairs the working tree");
    let out = s.req(&["diff", "--base", "base", "--head", "head"]);
    assert_eq!(out.code, 3, "{}", out.all());
    // And a repaired working tree against a clean base is not refused.
    let out = s.req(&["diff", "--base", "base"]);
    assert_eq!(out.code, 0, "{}", out.all());

    // A newer declaration in the working tree is NOT JUDGED, exit 3, as it is everywhere else.
    s.write(
        ".req/kinds/safety.yml",
        &SAFETY.replace("req.kind/1", "req.kind/2"),
    );
    let out = s.req(&["diff", "--base", "base"]);
    assert_eq!(out.code, 3, "{}", out.all());
}

#[test]
fn req_init_reads_a_malformed_declaration_as_exit_2_and_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("req-kinds-init-malformed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".req/kinds")).expect("makes the dir");
    fs::write(
        dir.join(".req/kinds/safety.yml"),
        "schema: req.kind/1\ndefault: x\nfields: {}\n",
    )
    .expect("writes a malformed declaration");
    let out = run(&dir, &["init"]);
    assert_eq!(
        out.code,
        2,
        "init loads claims, so it holds the same class as canon:\n{}",
        out.all()
    );
    assert!(
        out.all().contains("safety.yml") && out.all().contains("default"),
        "{}",
        out.all()
    );
    assert!(
        !dir.join("claims").exists() && !dir.join("aliases").exists(),
        "nothing was written"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The subcommand names in `src/main.rs`'s command table, in table order.
fn table_commands() -> Vec<String> {
    let main = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("reads main.rs");
    main.lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("name: \"")
                .and_then(|rest| rest.strip_suffix("\","))
                .map(str::to_string)
        })
        .collect()
}

/// The minimal arguments that get each subcommand past its own usage check, so that it runs. **A subcommand missing
/// here fails the guard below**: a new command must be classified, here, by someone who has read what it needs. The
/// guard needs no list of which commands load claims: it runs every one.
fn probe_args(command: &str) -> Option<Vec<&'static str>> {
    Some(match command {
        "impact" => vec!["impact", "REQ-0001"],
        "baseline" => vec!["baseline", "--at", "2026-11-18T00:00:00Z"],
        "compare" => vec!["compare", "a.yml", "b.yml"],
        "diff" => vec!["diff", "--base", "HEAD"],
        "propose" => vec!["propose", "a.yml", "b.yml"],
        "bench" => vec!["bench"],
        "select" => vec!["select"],
        "render" => vec!["render"],
        "trace" => vec!["trace"],
        "satisfy" => vec!["satisfy"],
        "gate" => vec!["gate"],
        "check" => vec!["check"],
        "next-id" => vec!["next-id"],
        "audit" => vec!["audit"],
        "decision" => vec!["decision"],
        "pin" => vec!["pin"],
        "canon" => vec!["canon"],
        _ => return None,
    })
}

#[test]
fn no_subcommand_that_reads_a_malformed_declaration_folds_it_into_exit_3() {
    // ADR 0056 decision 2 / spec F-1 choice 2: no caller folds the exit-2 class into 3. The guard runs EVERY subcommand
    // of the command table against a corpus whose `.req/kinds/` holds a malformed declaration, and requires that any
    // run whose output names the declaration exits 2. A command that loads claims and was not given the class (a new
    // `loads_claims: false` entry that calls the loader) names the file with exit 3 and fails here, with no hand list
    // of which commands load claims to forget to extend.
    let commands = table_commands();
    assert!(
        commands.len() >= 15,
        "the command table was found and enumerated: {commands:?}"
    );
    for known in [
        "canon", "gate", "render", "trace", "check", "baseline", "diff",
    ] {
        assert!(
            commands.iter().any(|c| c == known),
            "`{known}` is in the table: {commands:?}"
        );
    }
    let s = Scratch::new("every-subcommand");
    s.claim("REQ-0001", "req.claim/4", "");
    s.write("a.yml", &claim_text("REQ-0001", "req.claim/4", ""));
    s.write("b.yml", &claim_text("REQ-0001", "req.claim/4", ""));
    s.write(
        ".req/kinds/safety.yml",
        "schema: req.kind/1\ndefault: x\nfields: {}\n",
    );
    let mut named = Vec::new();
    for command in &commands {
        let args = probe_args(command).unwrap_or_else(|| {
            panic!("`{command}` is a subcommand with no entry in probe_args: classify it (give it the arguments that make it run)")
        });
        let out = s.req(&args);
        if out.all().contains("safety.yml") {
            named.push(command.clone());
            assert_eq!(
                out.code,
                2,
                "`req {}` reads `.req/kinds/` and must hold the exit-2 class, never 3:\n{}",
                args.join(" "),
                out.all()
            );
        }
    }
    // The guard cannot pass vacuously: the commands every release depends on read the declarations.
    for loader in [
        "canon", "gate", "render", "trace", "select", "check", "audit", "baseline", "impact",
        "compare", "diff",
    ] {
        assert!(
            named.iter().any(|c| c == loader),
            "`{loader}` should have named the malformed declaration (named: {named:?})"
        );
    }
}

/// The modules of `src/` that call the loader (`load_claims` or `load_rows`) directly: a new caller must be added
/// here, and the guard above is what then judges the command that reaches it.
const LOADER_CALLERS: &[&str] = &[
    "audit.rs",
    "baseline.rs",
    "canon.rs",
    "completeness.rs",
    "contradict.rs",
    "diff.rs",
    "gate.rs",
    "impact.rs",
    "init.rs",
    "satisfy.rs",
    "select.rs",
    "view.rs",
];

#[test]
fn a_module_that_calls_the_claim_loader_is_classified() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut callers = Vec::new();
    for entry in fs::read_dir(&src).expect("reads src/") {
        let path = entry.expect("an entry").path();
        let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };
        if file == "kinds.rs" || path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("reads a module");
        // A call, not a definition or a doc mention.
        let calls = text.lines().any(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
                && !t.starts_with("pub fn load_")
                && (t.contains("load_claims(") || t.contains("load_rows("))
        });
        if calls {
            callers.push(file.to_string());
        }
    }
    // Positive count: a renamed loader leaves the scan with nothing to say, and that must fail, not pass.
    assert!(
        callers.len() >= 8,
        "the scan found the loader's callers (found {callers:?}); did `load_claims` or `load_rows` change name?"
    );
    for file in &callers {
        assert!(
            LOADER_CALLERS.contains(&file.as_str()),
            "src/{file} calls the claim loader: add it to LOADER_CALLERS (and give the subcommand that reaches it \
             the exit-2 class: `no_subcommand_that_reads_a_malformed_declaration_folds_it_into_exit_3` runs it)"
        );
    }
}

#[test]
fn the_help_pages_state_the_declarations_and_the_exit_classes() {
    // The help page is the contract (AGENTS.md): each page states what the code does with `.req/kinds/`.
    let page = |command: &str| run(Path::new("."), &[command, "--help"]).stdout;
    let diff = page("diff");
    assert!(
        diff.contains("`.req/kinds/`"),
        "diff stages the declarations:\n{diff}"
    );
    assert!(
        diff.contains("working tree") && diff.contains("staged revision"),
        "diff names both classes of declaration failure:\n{diff}"
    );
    let compare = page("compare");
    assert!(
        compare.contains("`.req/kinds/`") && compare.contains("malformed"),
        "compare states its declaration contract and its exit 2:\n{compare}"
    );
    let init = page("init");
    assert!(
        init.contains("req.kind/2") && init.contains("malformed `.req/kinds/`"),
        "init names the 3 and the 2 of an unusable declaration:\n{init}"
    );
    for command in [
        "select", "render", "trace", "gate", "check", "satisfy", "impact", "baseline", "audit",
        "canon",
    ] {
        let text = page(command);
        assert!(
            text.contains(".req/kinds/") && text.contains("exit 2"),
            "`req {command} --help` states the malformed-declaration exit:\n{text}"
        );
    }
}
