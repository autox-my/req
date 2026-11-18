//! The public seam of `req::kinds` (ADR 0056 decisions 2 to 4, task F-1): the one function that judges
//! a claim's `kind` and `fields` against the declarations, which the corpus loader and `req compare`
//! both call, and the declarations' own reader.
//!
//! The oracles are the decision 3 table and decision 2's rules, written here by hand.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use req::canon::parse_claim;
use req::kinds::{Declarations, FieldValue, judge_fields};
use yaml_serde::Value;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-kinds-seam-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".req/kinds")).expect("makes the dir");
        Dir(dir)
    }

    fn kind(&self, name: &str, text: &str) {
        fs::write(self.0.join(format!(".req/kinds/{name}.yml")), text).expect("writes");
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const SAFETY: &str = "\
schema: req.kind/1
label: Safety
fields:
  asil:
    type: enum
    values: [QM, A, B, C, D]
    ordered: true
    required: true
  mitigated_by:
    type: list
    of: ref
  reviewed:
    type: bool
  count:
    type: integer
  ratio:
    type: decimal
  due:
    type: date
  note:
    type: string
";

fn fields_of(text: &str) -> BTreeMap<String, yaml_serde::Value> {
    let claim = format!(
        "schema: req.claim/4\nclaim_id: REQ-0001\nstatement: \"The system shall log each command.\"\napplicability: []\nrelations: []\nkind: safety\nfields:\n{text}rationale:\n  source: s\n  date: 2026-11-18\n  rejected_alternatives: []\n  assumptions: []\n"
    );
    parse_claim(&claim)
        .expect("the file's own shape parses")
        .fields
}

#[test]
fn with_no_declarations_a_claim_with_no_kind_and_no_fields_judges_to_nothing() {
    let dir = Dir::new("absent");
    fs::remove_dir_all(dir.0.join(".req/kinds")).expect("removes the dir");
    let decls = Declarations::read(&dir.0)
        .expect("an absent `.req/kinds/` is no declaration, not an error");
    let judged =
        judge_fields("REQ-0001", None, &BTreeMap::new(), &decls).expect("nothing to judge");
    assert!(judged.is_empty());
    // Every kind is refused when the directory is absent: no claim is accepted untyped.
    let errors = judge_fields("REQ-0001", Some("safety"), &BTreeMap::new(), &decls)
        .expect_err("a kind with no declaration is refused");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("REQ-0001") && e.contains("safety")),
        "{errors:?}"
    );
}

#[test]
fn a_judged_value_is_its_declared_type_and_its_canonical_text() {
    let dir = Dir::new("judged");
    dir.kind("safety", SAFETY);
    let decls = Declarations::read(&dir.0).expect("reads");
    let fields = fields_of(
        "  asil: C\n  reviewed: TRUE\n  count: \"-12\"\n  ratio: \"0.3\"\n  due: 2027-03-01\n  note: hello\n  mitigated_by: [REQ-0007, REQ-0002]\n",
    );
    let judged = judge_fields("REQ-0001", Some("safety"), &fields, &decls).expect("judges");
    let by_name: BTreeMap<&str, _> = judged.iter().map(|f| (f.name.as_str(), f)).collect();
    assert_eq!(
        judged.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        [
            "asil",
            "count",
            "due",
            "mitigated_by",
            "note",
            "ratio",
            "reviewed"
        ],
        "sorted by name, one entry per carried field and none for an absent one"
    );
    assert_eq!(by_name["asil"].type_name, "enum");
    assert_eq!(by_name["asil"].value, FieldValue::Text("C".to_string()));
    assert_eq!(by_name["reviewed"].type_name, "bool");
    assert_eq!(
        by_name["reviewed"].value,
        FieldValue::Bool(true),
        "TRUE is the boolean true"
    );
    assert_eq!(by_name["count"].value, FieldValue::Text("-12".to_string()));
    assert_eq!(by_name["ratio"].value, FieldValue::Text("0.3".to_string()));
    assert_eq!(
        by_name["due"].value,
        FieldValue::Text("2027-03-01".to_string())
    );
    assert_eq!(by_name["note"].value, FieldValue::Text("hello".to_string()));
    assert_eq!(by_name["mitigated_by"].type_name, "list");
    assert_eq!(by_name["mitigated_by"].of, Some("ref"));
    assert_eq!(
        by_name["mitigated_by"].value,
        FieldValue::List(vec!["REQ-0007".to_string(), "REQ-0002".to_string()]),
        "a list keeps its elements as authored; sorting for a digest is F-2's"
    );
}

#[test]
fn every_problem_of_a_claim_is_named_with_the_claim_the_kind_the_field_and_the_alternatives() {
    let dir = Dir::new("collected");
    dir.kind("safety", SAFETY);
    let decls = Declarations::read(&dir.0).expect("reads");
    let fields =
        fields_of("  asil: E\n  count: 3\n  nope: x\n  mitigated_by: [REQ-0001, REQ-0001]\n");
    let errors =
        judge_fields("REQ-0042", Some("safety"), &fields, &decls).expect_err("four problems");
    assert_eq!(errors.len(), 4, "one per field, none lost: {errors:?}");
    for error in &errors {
        assert!(
            error.contains("REQ-0042") && error.contains("safety"),
            "names the claim and the kind: {error}"
        );
    }
    let joined = errors.join("\n");
    assert!(
        joined.contains("asil") && joined.contains("QM, A, B, C, D"),
        "{joined}"
    );
    assert!(
        joined.contains("count") && joined.contains("integer"),
        "{joined}"
    );
    assert!(
        joined.contains("nope") && joined.contains("asil") && joined.contains("note"),
        "{joined}"
    );
    assert!(
        joined.contains("mitigated_by") && joined.contains("duplicate"),
        "{joined}"
    );
}

#[test]
fn a_malformed_declaration_is_class_2_and_a_newer_or_unreadable_one_is_class_3() {
    let dir = Dir::new("classes");
    dir.kind("a", "schema: req.kind/1\nfields: {}\ncolour: red\n");
    dir.kind(
        "b",
        "schema: req.kind/1\nfields:\n  x:\n    type: string\n    default: y\n",
    );
    let error = Declarations::read(&dir.0).expect_err("malformed");
    assert_eq!(error.exit_code(), 2);
    let text = error.messages().join("\n");
    assert!(text.contains("a.yml") && text.contains("colour"), "{text}");
    assert!(text.contains("b.yml") && text.contains("default"), "{text}");

    let newer = Dir::new("newer");
    newer.kind("a", "schema: req.kind/2\nfields: {}\n");
    let error = Declarations::read(&newer.0).expect_err("newer");
    assert_eq!(error.exit_code(), 3);
    assert!(error.messages().join("\n").contains("NOT JUDGED"));

    // Malformed outranks: an author must fix it before anything else can be judged.
    let both = Dir::new("both");
    both.kind("a", "schema: req.kind/2\nfields: {}\n");
    both.kind("b", "schema: req.kind/1\nfields: {}\ncolour: red\n");
    assert_eq!(
        Declarations::read(&both.0).expect_err("both").exit_code(),
        2
    );
}

#[test]
fn the_loader_keeps_the_judged_fields_on_the_loaded_claim() {
    let dir = Dir::new("loaded");
    dir.kind("safety", SAFETY);
    fs::create_dir_all(dir.0.join("claims")).expect("makes claims/");
    fs::create_dir_all(dir.0.join("aliases")).expect("makes aliases/");
    fs::write(
        dir.0.join("aliases/subjects.yml"),
        "schema: req.subject_registry/0\nversion: 1\nsubjects: {}\n",
    )
    .expect("writes the registry");
    fs::write(
        dir.0.join("claims/REQ-0001.yml"),
        "schema: req.claim/4\nclaim_id: REQ-0001\nstatement: \"The system shall log each command.\"\napplicability: []\nrelations: []\nkind: safety\nfields:\n  asil: B\nrationale:\n  source: s\n  date: 2026-11-18\n  rejected_alternatives: []\n  assumptions: []\n",
    )
    .expect("writes the claim");
    let claims = req::canon::load_claims(&dir.0).expect("loads");
    assert_eq!(claims[0].fields.len(), 1);
    assert_eq!(claims[0].fields[0].name, "asil");
    assert_eq!(claims[0].fields[0].value, FieldValue::Text("B".to_string()));
    assert_eq!(claims[0].claim.kind.as_deref(), Some("safety"));
}

// --- what the loader delivers (decision 3: "a test pins the loader's behaviour for each") ---

fn claim_text(extra: &str) -> String {
    format!(
        "schema: req.claim/4\nclaim_id: REQ-0001\nstatement: \"The system shall log each command.\"\napplicability: []\nrelations: []\n{extra}rationale:\n  source: s\n  date: 2026-11-18\n  rejected_alternatives: []\n  assumptions: []\n"
    )
}

/// What `yaml_serde` delivers for a value spelling, as a short description (ADR 0056 decision 3: "a test
/// pins the loader's behaviour for each").
fn delivered(spelling: &str) -> String {
    let text = claim_text(&format!("kind: safety\nfields:\n  f: {spelling}\n"));
    let claim = parse_claim(&text).unwrap_or_else(|e| panic!("{spelling}: {e}"));
    match claim.fields.get("f").expect("the field is carried") {
        Value::Bool(b) => format!("bool {b}"),
        Value::Number(n) => format!("number {n}"),
        Value::String(s) => format!("string {s}"),
        Value::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

#[test]
fn the_loader_delivers_each_spelling_the_decision_3_table_names() {
    for (spelling, expected) in [
        ("true", "bool true"),
        ("True", "bool true"),
        ("TRUE", "bool true"),
        ("false", "bool false"),
        ("yes", "string yes"),
        ("on", "string on"),
        ("\"true\"", "string true"),
        ("3", "number 3"),
        ("\"3\"", "string 3"),
        ("0.3", "number 0.3"),
        ("\"0.30\"", "string 0.30"),
        ("null", "null"),
        ("~", "null"),
        ("2027-03-01", "string 2027-03-01"),
        ("\"2027-02-29\"", "string 2027-02-29"),
        ("\"2028-02-29\"", "string 2028-02-29"),
    ] {
        assert_eq!(delivered(spelling), expected, "spelling {spelling}");
    }
}
