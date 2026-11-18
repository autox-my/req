//! `req canon --format json`, the `req.canon/1` document (task C-1; the contract is `src/format.rs`'s module
//! doc and `req canon --help`).
//!
//! The seam is the compiled `req` over this repository and over scratch corpora, plus
//! `req::canon::load_claims` for the T1.9 canonical bytes. The oracles are those bytes, the text output
//! and `req trace --format json`, never a run of the printer under test. Nothing here depends on where the
//! temp directory is.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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
            .unwrap_or_else(|e| panic!("stdout is a JSON document ({e}):\n{}", self.all()))
    }
}

fn run(dir: &Path, args: &[&str]) -> Out {
    let out = Command::new(req_bin())
        .args(args)
        .current_dir(dir)
        // An inherited root would make a scratch corpus check something other than what it says.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req");
    Out {
        code: out.status.code().expect("req exits, never a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// A corpus made by the real bootstrap, its starter claim removed.
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-canon-json-{}-{}-{name}",
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

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, text).expect("writes the file");
    }

    fn claim(&self, id: &str, schema: &str, statement: &str, extra: &str) {
        self.write(
            &format!("claims/{id}.yml"),
            &format!(
                "schema: {schema}\n\
                 claim_id: {id}\n\
                 statement: \"{statement}\"\n\
                 applicability: []\n\
                 relations: []\n\
                 {extra}\
                 rationale:\n  source: docs/requirements.md\n  date: 2026-11-18\n  rejected_alternatives: []\n  assumptions: []\n"
            ),
        );
    }

    fn req(&self, args: &[&str]) -> Out {
        run(&self.0, args)
    }
}

const HEALTH: &str = "The API shall respond to a health check within 200 ms.";
const PROSE: &str = "Hello there, this is only prose.";

/// The one claim object for `id`.
fn claim_of<'a>(doc: &'a Value, id: &str) -> &'a Value {
    doc["claims"]
        .as_array()
        .expect("`claims` is an array")
        .iter()
        .find(|c| c["claim_id"] == id)
        .unwrap_or_else(|| panic!("{id} is in the document"))
}

fn keys(object: &Value) -> Vec<&str> {
    object
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

// --- this repository ---------------------------------------------------------------------------------

#[test]
fn the_document_over_this_repository_has_the_envelope_and_one_claim_per_claim_by_id() {
    let out = run(repo(), &["canon", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout.starts_with('{') && out.stdout.ends_with("}\n") && !out.stdout.ends_with("\n\n"),
        "one JSON document and one trailing newline"
    );
    let doc = out.json();
    assert_eq!(doc["schema"], "req.canon/1");
    assert_eq!(doc["generator"], "canon");
    assert_eq!(doc["rules_v"], 9);

    let loaded = req::canon::load_claims(repo()).expect("this repository loads");
    let ids: Vec<&str> = doc["claims"]
        .as_array()
        .expect("claims is an array")
        .iter()
        .map(|c| c["claim_id"].as_str().expect("an id"))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "ordered by claim_id");
    assert_eq!(ids.len(), loaded.len(), "as long as the corpus");
    assert!(!ids.is_empty(), "the corpus is not vacuously empty");
}

#[test]
fn each_claim_object_has_exactly_the_contracted_keys_in_order() {
    let out = run(repo(), &["canon", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    // The document is compact JSON in the contract's key order: read the first claim's keys out of the bytes,
    // because a parsed object sorts its keys.
    let first = out.stdout.find("\"claims\":[{").expect("claims opens") + "\"claims\":[{".len();
    let rest = &out.stdout[first..];
    let order: Vec<usize> = [
        "\"claim_id\":",
        "\"address\":",
        "\"structured\":",
        "\"form\":",
        "\"lints\":",
        "\"pins\":",
        "\"area\":",
        "\"kind\":",
        "\"fields\":",
    ]
    .iter()
    .map(|key| rest.find(key).unwrap_or_else(|| panic!("{key} is written")))
    .collect();
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "keys in the contract's order: {order:?}"
    );
    let doc = out.json();
    let first = &doc["claims"][0];
    let mut got = keys(first);
    got.sort_unstable();
    let mut want = vec![
        "address",
        "area",
        "claim_id",
        "fields",
        "form",
        "kind",
        "lints",
        "pins",
        "structured",
    ];
    want.sort_unstable();
    assert_eq!(got, want, "nothing else goes in");
}

#[test]
fn each_address_equals_the_one_req_trace_reports() {
    let canon = run(repo(), &["canon", "--format", "json"]).json();
    let trace = run(repo(), &["trace", "--format", "json"]).json();
    let mut trace_address = BTreeMap::new();
    for node in trace["nodes"].as_array().expect("nodes") {
        trace_address.insert(
            node["claim_id"].as_str().expect("id").to_string(),
            node["address"].as_str().expect("address").to_string(),
        );
    }
    let claims = canon["claims"].as_array().expect("claims");
    assert_eq!(claims.len(), trace_address.len());
    assert!(!claims.is_empty(), "a positive count, not a vacuous loop");
    for claim in claims {
        let id = claim["claim_id"].as_str().expect("id");
        assert_eq!(
            claim["address"].as_str(),
            trace_address.get(id).map(String::as_str),
            "{id}"
        );
    }
}

#[test]
fn form_is_the_claims_canonical_bytes_parsed_and_verbatim() {
    let out = run(repo(), &["canon", "--format", "json"]);
    let doc = out.json();
    let raw = out.stdout.as_str();
    let loaded = req::canon::load_claims(repo()).expect("this repository loads");
    assert!(!loaded.is_empty());
    let mut checked = 0;
    for claim in &loaded {
        let id = claim.claim.claim_id.as_str();
        let form = &claim_of(&doc, id)["form"];
        let bytes: Value =
            serde_json::from_str(&claim.derived.canonical).expect("T1.9 bytes are JSON");
        assert_eq!(
            form, &bytes,
            "{id}: the form is the canonical bytes, parsed"
        );
        // And in their own spelling, key order included: the document embeds the frozen bytes, not a
        // re-sorted rendering of them.
        let embedded = format!("\"form\":{}", claim.derived.canonical);
        assert!(
            raw.contains(&embedded),
            "{id}: the document carries the T1.9 bytes verbatim"
        );
        checked += 1;
    }
    assert_eq!(checked, loaded.len());
}

/// One text line of `req canon`, read back as the five derived slots and the lint codes.
struct TextRow {
    structured: bool,
    modality: String,
    subject: String,
    operator: String,
    value: String,
    unit: String,
    lints: Vec<String>,
    pins: Vec<(String, String, String)>,
}

fn text_rows(stdout: &str) -> BTreeMap<String, TextRow> {
    let mut rows: BTreeMap<String, TextRow> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("  pin ") {
            // `pin <key> <pinned>: agrees|FLAG contradiction — the parse says <v>|FLAG not carried …`
            let (head, verdict) = rest.split_once(": ").expect("a pin line");
            let (key, pinned) = head.split_once(' ').expect("key and value");
            let kind = if verdict == "agrees" {
                "agrees"
            } else if verdict.starts_with("FLAG contradiction") {
                "contradiction"
            } else if verdict.starts_with("FLAG not carried") {
                "not_carried"
            } else {
                panic!("unknown verdict {verdict:?}")
            };
            let derived = verdict
                .split_once("the parse says ")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            rows.get_mut(current.as_deref().expect("a claim line first"))
                .expect("the row")
                .pins
                .push((format!("{key} {pinned}"), kind.to_string(), derived));
            continue;
        }
        if !line.starts_with("REQ-") {
            continue;
        }
        let id = line.split_whitespace().next().expect("an id").to_string();
        let rest = line[id.len()..].trim_start();
        let (structure, rest) = rest.split_once(' ').expect("structure");
        let rest = rest.trim_start();
        let (modality, rest) = rest.split_once(' ').expect("modality");
        let (subject, rest) = rest.split_once(" op ").expect("op marker");
        let (operator, rest) = rest.split_once(" value ").expect("value marker");
        let (value, rest) = rest.split_once(" unit ").expect("unit marker");
        let (unit, rest) = rest.split_once(" lints ").expect("lints marker");
        let lints = rest.split_whitespace().next().expect("the lints token");
        rows.insert(
            id.clone(),
            TextRow {
                structured: structure == "structured",
                modality: modality.to_string(),
                subject: subject.trim().to_string(),
                operator: operator.trim().to_string(),
                value: value.trim().to_string(),
                unit: unit.trim().to_string(),
                lints: if lints == "-" {
                    Vec::new()
                } else {
                    lints.split(',').map(str::to_string).collect()
                },
                pins: Vec::new(),
            },
        );
        current = Some(id);
    }
    rows
}

/// The value slot the way the text writes it: the decimal, or `{a|b}` for a set, `-` for none.
fn value_as_text(form: &Value) -> String {
    match &form["value"] {
        Value::Null => "-".to_string(),
        Value::String(s) => s.clone(),
        Value::Array(members) => format!(
            "{{{}}}",
            members
                .iter()
                .map(|m| m.as_str().expect("a member"))
                .collect::<Vec<_>>()
                .join("|")
        ),
        other => panic!("a value is a string, a set or null: {other}"),
    }
}

fn dash(s: &str) -> String {
    if s.is_empty() || s == "none" {
        "-".to_string()
    } else {
        s.to_string()
    }
}

/// Every claim in `corpus`: the JSON's modality, subject, operator, value, unit, lints and pin verdicts equal
/// what the text line says.
fn assert_text_agrees(dir: &Path) -> usize {
    let text = run(dir, &["canon"]);
    assert_eq!(text.code, 0, "{}", text.all());
    let json = run(dir, &["canon", "--format", "json"]);
    assert_eq!(json.code, 0, "{}", json.all());
    let doc = json.json();
    let rows = text_rows(&text.stdout);
    let claims = doc["claims"].as_array().expect("claims");
    assert_eq!(claims.len(), rows.len(), "one text line per claim");
    for claim in claims {
        let id = claim["claim_id"].as_str().expect("id");
        let row = rows
            .get(id)
            .unwrap_or_else(|| panic!("{id} has a text line"));
        let form = &claim["form"];
        assert_eq!(claim["structured"], row.structured, "{id} structured");
        if row.structured {
            assert_eq!(form["modality"], row.modality.as_str(), "{id} modality");
            assert_eq!(
                dash(form["subject"].as_str().unwrap_or("")),
                row.subject,
                "{id} subject"
            );
            assert_eq!(
                dash(form["operator"].as_str().unwrap_or("")),
                row.operator,
                "{id} operator"
            );
            assert_eq!(value_as_text(form), row.value, "{id} value");
            assert_eq!(
                dash(form["unit"].as_str().unwrap_or("")),
                row.unit,
                "{id} unit"
            );
        } else {
            for slot in [
                &row.modality,
                &row.subject,
                &row.operator,
                &row.value,
                &row.unit,
            ] {
                assert_eq!(slot, "-", "{id}: an unstructured line shows dashes");
            }
        }
        let lints: Vec<&str> = claim["lints"]
            .as_array()
            .expect("lints")
            .iter()
            .map(|l| l["code"].as_str().expect("a code"))
            .collect();
        assert_eq!(lints, row.lints, "{id} lints, in the order raised");
        let pins = claim["pins"].as_array().expect("pins");
        assert_eq!(pins.len(), row.pins.len(), "{id} pin count");
        for (pin, (said, verdict, derived)) in pins.iter().zip(&row.pins) {
            let key = pin["key"].as_str().expect("key");
            let pinned = pin["pinned"].as_str().expect("pinned");
            assert_eq!(&format!("{key} {pinned}"), said, "{id}");
            assert_eq!(
                pin["verdict"].as_str(),
                Some(verdict.as_str()),
                "{id} {key}"
            );
            if verdict == "contradiction" {
                assert_eq!(
                    pin["derived"].as_str(),
                    Some(derived.as_str()),
                    "{id} {key}"
                );
            } else {
                assert!(pin["derived"].is_null(), "{id} {key}: derived is null");
            }
        }
    }
    claims.len()
}

#[test]
fn the_json_says_what_the_text_says_over_this_repository() {
    let count = assert_text_agrees(repo());
    assert!(count > 0, "a positive count of claims was compared");
    // The repository exercises lints and pins, so the comparison above is not vacuous on them.
    let doc = run(repo(), &["canon", "--format", "json"]).json();
    let lints: usize = doc["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .map(|c| c["lints"].as_array().expect("lints").len())
        .sum();
    assert!(lints > 0, "this repository has lints to compare");
}

#[test]
fn the_json_says_what_the_text_says_over_a_corpus_with_pins_lints_and_prose() {
    let s = Scratch::new("agree");
    s.claim(
        "REQ-0001",
        "req.claim/4",
        HEALTH,
        "pins:\n  operator: LE\n  value: \"0.2\"\n  unit: s\n",
    );
    s.claim("REQ-0002", "req.claim/4", PROSE, "pins:\n  value: \"3\"\n");
    s.claim(
        "REQ-0003",
        "req.claim/4",
        "The API shall respond to a health check within 200 ms.",
        "pins:\n  value: \"999\"\n",
    );
    assert_eq!(assert_text_agrees(&s.0), 3);
}

// --- pins, structure, kind and fields ----------------------------------------------------------------

#[test]
fn pins_carry_a_verdict_and_derived_only_for_a_contradiction() {
    let s = Scratch::new("pins");
    s.claim(
        "REQ-0001",
        "req.claim/4",
        HEALTH,
        "pins:\n  operator: LE\n  value: \"999\"\n  unit: s\n",
    );
    s.claim("REQ-0002", "req.claim/4", PROSE, "pins:\n  value: \"3\"\n");
    s.claim("REQ-0003", "req.claim/4", HEALTH, "");
    let doc = s.req(&["canon", "--format", "json"]).json();

    let pins = claim_of(&doc, "REQ-0001")["pins"]
        .as_array()
        .expect("pins")
        .clone();
    assert_eq!(pins.len(), 3);
    let by_key = |key: &str| {
        pins.iter()
            .find(|p| p["key"] == key)
            .unwrap_or_else(|| panic!("pin {key}"))
            .clone()
    };
    let value = by_key("value");
    assert_eq!(value["pinned"], "999");
    assert_eq!(value["verdict"], "contradiction");
    assert_eq!(value["derived"], "0.2", "the parsed value, named");
    let operator = by_key("operator");
    assert_eq!(operator["verdict"], "agrees");
    assert!(
        operator["derived"].is_null(),
        "an agreeing pin has no derived"
    );
    assert_eq!(
        keys(&operator),
        ["derived", "key", "pinned", "verdict"],
        "the pin object's keys"
    );

    let prose = claim_of(&doc, "REQ-0002");
    assert_eq!(prose["structured"], false);
    assert_eq!(prose["pins"][0]["verdict"], "not_carried");
    assert!(prose["pins"][0]["derived"].is_null());

    assert_eq!(claim_of(&doc, "REQ-0003")["pins"], serde_json::json!([]));
}

#[test]
fn an_unstructured_statement_is_not_structured_and_its_lints_match_the_text() {
    let s = Scratch::new("prose");
    s.claim("REQ-0001", "req.claim/4", PROSE, "");
    let doc = s.req(&["canon", "--format", "json"]).json();
    let claim = claim_of(&doc, "REQ-0001");
    assert_eq!(claim["structured"], false);
    assert_eq!(claim["form"]["unstructured"], true);

    let text = s.req(&["canon"]);
    let rows = text_rows(&text.stdout);
    let row = rows.get("REQ-0001").expect("its text line");
    assert!(!row.structured);
    let lints: Vec<&str> = claim["lints"]
        .as_array()
        .expect("lints")
        .iter()
        .map(|l| l["code"].as_str().expect("code"))
        .collect();
    assert_eq!(lints, row.lints);
    // Every lint carries its detail beside its code, and the text prints the same detail.
    for lint in claim["lints"].as_array().expect("lints") {
        assert_eq!(keys(lint), ["code", "detail"]);
        let line = format!(
            "  lint {}: {}",
            lint["code"].as_str().expect("code"),
            lint["detail"].as_str().expect("detail")
        );
        assert!(text.stdout.contains(&line), "the text holds `{line}`");
    }
}

const KIND_ALL: &str = "\
schema: req.kind/1
label: Every field type that renders differently
fields:
  count:
    type: integer
  ratio:
    type: decimal
  reviewed:
    type: bool
  tags:
    type: list
    of: string
  note:
    type: string
";

#[test]
fn kind_and_fields_equal_the_req_trace_node() {
    let s = Scratch::new("kind");
    s.write(".req/kinds/audit.yml", KIND_ALL);
    s.claim(
        "REQ-0001",
        "req.claim/4",
        HEALTH,
        "area: diagnostics/startup\nkind: audit\nfields:\n  tags: [zeta, alpha]\n  reviewed: true\n  count: \"7\"\n  ratio: \"0.3\"\n  note: hello\n",
    );
    s.claim("REQ-0002", "req.claim/4", HEALTH, "");
    s.claim("REQ-0003", "req.claim/3", HEALTH, "");
    let canon = s.req(&["canon", "--format", "json"]);
    assert_eq!(canon.code, 0, "{}", canon.all());
    let canon = canon.json();
    let trace = s.req(&["trace", "--format", "json"]).json();
    let mut compared = 0;
    for node in trace["nodes"].as_array().expect("nodes") {
        let id = node["claim_id"].as_str().expect("id");
        let claim = claim_of(&canon, id);
        assert_eq!(claim["kind"], node["kind"], "{id} kind");
        assert_eq!(claim["fields"], node["fields"], "{id} fields");
        assert_eq!(claim["area"], node["area"], "{id} area");
        compared += 1;
    }
    assert_eq!(compared, 3);

    let kinded = claim_of(&canon, "REQ-0001");
    assert_eq!(kinded["kind"], "audit");
    assert_eq!(kinded["area"], "diagnostics/startup");
    assert_eq!(
        kinded["fields"]["reviewed"], true,
        "a bool is a JSON boolean"
    );
    assert_eq!(kinded["fields"]["count"], "7", "an integer is a string");
    assert_eq!(
        kinded["fields"]["ratio"], "0.3",
        "a decimal is its canonical string"
    );
    assert_eq!(
        kinded["fields"]["tags"],
        serde_json::json!(["alpha", "zeta"]),
        "a list is sorted by byte"
    );
    assert_eq!(
        keys(&kinded["fields"]),
        ["count", "note", "ratio", "reviewed", "tags"],
        "keys sorted by byte"
    );

    let bare = claim_of(&canon, "REQ-0002");
    assert!(bare["kind"].is_null());
    assert_eq!(bare["fields"], serde_json::json!({}));
    assert!(bare["area"].is_null());
}

// --- flags and refusals ------------------------------------------------------------------------------

#[test]
fn fail_on_flag_with_json_exits_1_and_still_prints_the_whole_document() {
    let s = Scratch::new("flagged");
    s.claim(
        "REQ-0001",
        "req.claim/4",
        HEALTH,
        "pins:\n  value: \"999\"\n",
    );
    let plain = s.req(&["canon", "--format", "json"]);
    assert_eq!(plain.code, 0, "{}", plain.all());
    let strict = s.req(&["canon", "--format", "json", "--fail-on-flag"]);
    assert_eq!(strict.code, 1, "{}", strict.all());
    assert_eq!(strict.stdout, plain.stdout, "the same full document");
    assert!(strict.stderr.contains("REQ-0001"), "{}", strict.stderr);
    let text = s.req(&["canon", "--fail-on-flag"]);
    assert_eq!(text.code, 1, "the same exit under both formats");

    s.claim(
        "REQ-0001",
        "req.claim/4",
        HEALTH,
        "pins:\n  value: \"0.2\"\n",
    );
    let clean = s.req(&["canon", "--fail-on-flag", "--format", "json"]);
    assert_eq!(clean.code, 0, "{}", clean.all());
}

#[test]
fn json_with_by_area_is_a_usage_error_naming_the_combination() {
    let s = Scratch::new("by-area");
    s.claim("REQ-0001", "req.claim/4", HEALTH, "");
    for args in [
        ["canon", "--format", "json", "--by-area"],
        ["canon", "--by-area", "--format", "json"],
    ] {
        let out = s.req(&args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.all());
        assert!(out.stdout.is_empty(), "no document on stdout");
        assert!(
            out.stderr.contains("--by-area") && out.stderr.contains("--format json"),
            "names the combination: {}",
            out.stderr
        );
    }
    // The text summary is unchanged beside it.
    assert_eq!(s.req(&["canon", "--by-area", "--format", "text"]).code, 0);
}

#[test]
fn a_bad_format_value_is_a_usage_error() {
    let s = Scratch::new("bad-format");
    s.claim("REQ-0001", "req.claim/4", HEALTH, "");
    for (args, said) in [
        (vec!["canon", "--format", "yaml"], "unknown format"),
        (vec!["canon", "--format"], "needs a value"),
    ] {
        let out = s.req(&args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.all());
        assert!(out.stdout.is_empty());
        assert!(out.stderr.contains(said), "{args:?}: {}", out.stderr);
        assert!(
            out.stderr.contains("req canon") && out.stderr.contains("--format"),
            "the usage line names the verb and the flag: {}",
            out.stderr
        );
    }
}

#[test]
fn a_corpus_that_cannot_load_is_exit_3_with_the_texts_message_and_no_stdout() {
    let s = Scratch::new("broken");
    s.claim("REQ-0001", "req.claim/4", HEALTH, "");
    s.write("claims/REQ-0002.yml", "this: is not\n  a claim: [\n");
    let json = s.req(&["canon", "--format", "json"]);
    let text = s.req(&["canon"]);
    assert_eq!(json.code, 3, "{}", json.all());
    assert_eq!(text.code, 3);
    assert!(
        json.stdout.is_empty(),
        "no JSON on stdout: {:?}",
        json.stdout
    );
    assert!(json.stderr.contains("[UNVERIFIABLE]"), "{}", json.stderr);
    assert!(json.stderr.contains("REQ-0002"), "{}", json.stderr);
    assert_eq!(
        json.stderr.replace(&s.0.display().to_string(), "<root>"),
        text.stderr.replace(&s.0.display().to_string(), "<root>"),
        "the same message as text"
    );
}

#[test]
fn a_malformed_kind_declaration_is_exit_2_and_prints_no_document() {
    let s = Scratch::new("bad-kind");
    s.claim("REQ-0001", "req.claim/4", HEALTH, "");
    s.write(
        ".req/kinds/colour.yml",
        "schema: req.kind/1\ncolour: red\nfields: {}\n",
    );
    let json = s.req(&["canon", "--format", "json"]);
    assert_eq!(json.code, 2, "{}", json.all());
    assert!(json.stdout.is_empty(), "no JSON on stdout");
    assert!(
        json.stderr.contains("colour"),
        "the declaration's own refusal, naming the key: {}",
        json.stderr
    );
    assert_eq!(s.req(&["canon"]).code, 2, "the same exit as text");
}

// --- determinism and the text default ----------------------------------------------------------------

#[test]
fn two_runs_are_byte_identical() {
    let a = run(repo(), &["canon", "--format", "json"]);
    let b = run(repo(), &["canon", "--format", "json"]);
    assert_eq!(a.code, 0);
    assert!(!a.stdout.is_empty());
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn format_text_is_the_bare_command_byte_for_byte() {
    let dir = repo();
    let bare = run(dir, &["canon"]);
    let text = run(dir, &["canon", "--format", "text"]);
    assert_eq!(bare.code, 0);
    assert!(!bare.stdout.is_empty());
    assert_eq!((bare.code, &bare.stdout), (text.code, &text.stdout));
    let by_area = run(dir, &["canon", "--by-area"]);
    let by_area_text = run(dir, &["canon", "--format", "text", "--by-area"]);
    assert_eq!(by_area.stdout, by_area_text.stdout);
}

#[test]
fn claims_are_ordered_by_id_whatever_order_the_files_were_written_in() {
    let s = Scratch::new("order");
    // Written highest first; the loader reads by path, the document orders by id.
    for id in ["REQ-0010", "REQ-0009", "REQ-0002", "REQ-0001"] {
        s.claim(id, "req.claim/4", HEALTH, "");
    }
    let doc = s.req(&["canon", "--format", "json"]).json();
    let ids: Vec<&str> = doc["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .map(|c| c["claim_id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["REQ-0001", "REQ-0002", "REQ-0009", "REQ-0010"]);
}

#[test]
fn the_form_is_the_canonical_bytes_even_where_a_json_library_would_respell_them() {
    let s = Scratch::new("escapes");
    // A backspace (`\u{8}`) is spelled `\u0008` in the T1.9 bytes and `\b` by a re-serialiser; `é` and a
    // quote must pass through unchanged.
    s.claim(
        "REQ-0001",
        "req.claim/4",
        "Caf\\u00e9 \\b \\\"quoted\\\" prose",
        "",
    );
    let out = s.req(&["canon", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let loaded = req::canon::load_claims(&s.0).expect("the scratch corpus loads");
    assert_eq!(loaded.len(), 1);
    let canonical = &loaded[0].derived.canonical;
    assert!(
        canonical.contains("\\u0008"),
        "the fixture reaches the escape: {canonical}"
    );
    assert!(
        out.stdout.contains(&format!("\"form\":{canonical},")),
        "the T1.9 bytes verbatim:\n{}",
        out.stdout
    );
    out.json();
}
