//! Reporting a field change, the JSON views and the two findings (ADR 0056 decisions 4, 5.1, 8 and 9, task F-3).
//!
//! The seam is the compiled `req` over throwaway corpora, most of them scratch git repositories with
//! `.req/kinds/`. Every expectation comes from the ADR and its *Falsification* list, never from a run.
//!
//! * 6 — `missing_field` folded into PASS, or an absent value reaching a consumer as `null`;
//! * 7 — `req diff` reports a field-only change as `unchanged`, or exits 0 under `--fail-on fields-changed`;
//!   `compare` prints a field edit as a direction;
//! * 9 — a gate policy, a selector, a projection or an exit code reads a field;
//! * 11 — an added and a removed claim that differ only in fields are reported without saying so;
//! * 12 — an edit to `.req/kinds/` between two revisions is not reported by `req diff`;
//! * L5, L10 — the JSON views gain `kind` and `fields` after `area` and nothing moves; a reordered list is
//!   not a change.
//!
//! Nothing here asserts a path: the corpora live under the OS temp dir and no output is compared against a
//! location.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

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

    /// The line of stdout that starts with `prefix`.
    fn line(&self, prefix: &str) -> Option<&str> {
        self.stdout.lines().find(|l| l.starts_with(prefix))
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
            "req-field-reporting-{}-{}-{name}",
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

    fn remove(&self, relative: &str) {
        fs::remove_file(self.0.join(relative)).expect("removes the file");
    }

    fn claim(&self, id: &str, extra: &str) {
        self.write(
            &format!("claims/{id}.yml"),
            &claim_text(id, &own_statement(id), extra),
        );
    }

    fn claim_saying(&self, id: &str, statement: &str, extra: &str) {
        self.write(
            &format!("claims/{id}.yml"),
            &claim_text(id, statement, extra),
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

fn own_statement(id: &str) -> String {
    format!("The {id} service shall respond to a health check within 200 ms.")
}

fn claim_text(id: &str, statement: &str, extra: &str) -> String {
    format!(
        "schema: req.claim/4\n\
         claim_id: {id}\n\
         statement: \"{statement}\"\n\
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

/// A second kind that shares `asil`, for a kind change that moves no field.
const SECURITY: &str = "\
schema: req.kind/1
fields:
  asil:
    type: enum
    values: [QM, A, B, C, D]
  mitigated_by:
    type: list
    of: ref
  review_due:
    type: date
";

/// One kind that uses every scalar type, and the list, for the JSON views.
const TYPED: &str = "\
schema: req.kind/1
fields:
  count:
    type: integer
  ratio:
    type: decimal
  flag:
    type: bool
  tags:
    type: list
    of: string
  title:
    type: string
  parent:
    type: ref
  due:
    type: date
  level:
    type: enum
    values: [low, high]
";

const TYPED_CLAIM: &str = "\
kind: typed
fields:
  count: \"3\"
  ratio: \"0.3\"
  flag: true
  tags: [b, a]
  title: Hello
  parent: REQ-0002
  due: 2027-03-01
  level: high
";

/// A kinded claim whose every `ref` names a claim the `filed` corpus holds (or none), so the only finding a test
/// meets is the one it adds.
fn plain(asil: &str) -> String {
    format!("kind: safety\nfields:\n  asil: {asil}\n")
}

fn kinded(asil: &str) -> String {
    format!("kind: safety\nfields:\n  asil: {asil}\n  mitigated_by: [REQ-0002, REQ-0007]\n")
}

// --- git --------------------------------------------------------------------------------------------

fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "req field reporting test")
        .env("GIT_AUTHOR_EMAIL", "fields@example.invalid")
        .env("GIT_COMMITTER_NAME", "req field reporting test")
        .env("GIT_COMMITTER_EMAIL", "fields@example.invalid")
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

/// A repository whose `base` commit holds the `safety` kind and one kinded claim.
fn repo(name: &str) -> Scratch {
    let s = Scratch::new(name);
    git_in(s.path(), &["init", "-q", "-b", "main"]);
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim("REQ-0001", &kinded("C"));
    commit(&s, "base");
    s
}

fn diff(s: &Scratch, extra: &[&str]) -> Out {
    let mut args = vec!["diff", "--base", "base", "--head", "head"];
    args.extend_from_slice(extra);
    s.req(&args)
}

// --- falsification 7: `req diff` reports a field edit ----------------------------------------------------

#[test]
fn a_claim_whose_only_change_is_a_field_is_metadata_and_carries_the_flag() {
    let s = repo("field-only");
    s.claim("REQ-0001", &kinded("D"));
    commit(&s, "head");
    let out = diff(&s, &[]);
    assert_eq!(out.code, 0, "{}", out.all());
    let line = out.line("metadata REQ-0001").unwrap_or_else(|| {
        panic!(
            "the claim is listed as metadata, not unchanged:\n{}",
            out.stdout
        )
    });
    assert!(
        line.ends_with(" fields-changed asil"),
        "the flag names the field: {line}"
    );
    assert!(
        out.stdout.contains("metadata 1") && out.stdout.contains("unchanged 0"),
        "{}",
        out.stdout
    );
    assert_eq!(
        line, "metadata REQ-0001 fields-changed asil",
        "a field edit has no direction word"
    );

    let json = diff(&s, &["--format", "json"]).json();
    let claim = &json["claims"][0];
    assert_eq!(claim["class"], "metadata");
    assert_eq!(claim["flags"], json!(["fields-changed asil"]));
    assert_eq!(claim["direction"], Value::Null);
}

#[test]
fn fail_on_fields_changed_exits_1_on_a_field_edit_and_0_without_one() {
    let s = repo("fail-on");
    s.claim("REQ-0001", &kinded("D"));
    commit(&s, "head");
    let out = diff(&s, &["--fail-on", "fields-changed"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout.contains("fail-on: fields-changed — 1 triggered"),
        "{}",
        out.stdout
    );
    let json = diff(&s, &["--fail-on", "fields-changed", "--format", "json"]);
    assert_eq!(json.code, 1);
    assert_eq!(json.json()["triggered"], json!(["fields-changed"]));

    // Opt-in: nothing is on by default, so the same edit exits 0 with no `--fail-on`.
    assert_eq!(diff(&s, &[]).code, 0);

    // A head that edits no field: exit 0 under the same flag.
    let t = repo("fail-on-none");
    t.claim("REQ-0003", "");
    commit(&t, "head");
    let out = diff(&t, &["--fail-on", "fields-changed"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .contains("fail-on: fields-changed — none triggered"),
        "{}",
        out.stdout
    );
}

#[test]
fn a_field_edit_beside_a_reworded_or_changed_statement_still_carries_the_flag() {
    // Reworded: the text changed and the meaning fingerprint did not.
    let s = repo("beside-reworded");
    s.claim_saying(
        "REQ-0001",
        "The REQ-0001 service shall respond to a health check within 200 milliseconds.",
        &kinded("D"),
    );
    commit(&s, "head");
    let out = diff(&s, &[]);
    let line = out
        .line("reworded REQ-0001")
        .unwrap_or_else(|| panic!("the claim is reworded:\n{}", out.stdout));
    assert!(line.contains("fields-changed asil"), "{line}");
    assert_eq!(
        diff(&s, &["--fail-on", "fields-changed"]).code,
        1,
        "the flag fails under any class"
    );

    // Weakened: the meaning moved too.
    let s = repo("beside-weakened");
    s.claim_saying(
        "REQ-0001",
        "The REQ-0001 service shall respond to a health check within 300 ms.",
        &kinded("D"),
    );
    commit(&s, "head");
    let out = diff(&s, &[]);
    let line = out
        .line("weakened REQ-0001")
        .unwrap_or_else(|| panic!("the claim is weakened:\n{}", out.stdout));
    assert!(line.contains("fields-changed asil"), "{line}");
}

#[test]
fn the_flag_names_every_changed_field_and_the_kind_sorted_by_byte() {
    // Added, removed and changed values, and a kind change, in one claim.
    let s = repo("names");
    s.write(".req/kinds/security.yml", SECURITY);
    s.claim(
        "REQ-0001",
        "kind: security\nfields:\n  asil: C\n  review_due: 2027-03-01\n",
    );
    commit(&s, "head");
    let out = diff(&s, &[]);
    let line = out
        .line("metadata REQ-0001")
        .unwrap_or_else(|| panic!("listed:\n{}", out.stdout));
    // `mitigated_by` was removed, `review_due` added, the kind changed, `asil` is the same.
    assert!(
        line.ends_with(" fields-changed kind,mitigated_by,review_due"),
        "{line}"
    );

    // Dropping the kind and every field: `asil` and `mitigated_by` are removed and the kind changed.
    let s = repo("names-dropped");
    s.claim("REQ-0001", "");
    commit(&s, "head");
    let line = diff(&s, &[])
        .line("metadata REQ-0001")
        .map(str::to_string)
        .expect("listed");
    assert!(
        line.ends_with(" fields-changed asil,kind,mitigated_by"),
        "{line}"
    );
}

#[test]
fn a_reordered_list_value_is_not_a_change() {
    // L10: a list is a set, and the digest sorts it.
    let s = repo("reordered");
    s.claim(
        "REQ-0001",
        "kind: safety\nfields:\n  asil: C\n  mitigated_by: [REQ-0007, REQ-0002]\n",
    );
    commit(&s, "head");
    let out = diff(&s, &["--fail-on", "fields-changed"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        !out.stdout.contains("REQ-0001") && !out.stdout.contains("fields-changed REQ"),
        "an unchanged claim is counted, never listed:\n{}",
        out.stdout
    );
    assert!(out.stdout.contains("unchanged 1"), "{}", out.stdout);
}

// --- falsification 11 (L11): a copy with another level is not a bare renumber ----------------------------

#[test]
fn an_added_and_a_removed_claim_with_one_address_and_another_level_say_so() {
    let statement = "The REQ-0001 service shall respond to a health check within 200 ms.";
    let s = repo("same-address-fields");
    s.remove("claims/REQ-0001.yml");
    s.claim_saying("REQ-0002", statement, &kinded("D"));
    commit(&s, "head");
    let out = diff(&s, &[]);
    let line = out
        .line("added REQ-0002")
        .unwrap_or_else(|| panic!("the added claim is listed:\n{}", out.stdout));
    assert!(
        line.ends_with("same-address-as REQ-0001 (fields differ: asil)"),
        "{line}"
    );

    // The kind is named too, with the fields, sorted by byte.
    let s = repo("same-address-kind");
    s.remove("claims/REQ-0001.yml");
    s.claim_saying("REQ-0002", statement, "");
    commit(&s, "head");
    let line = diff(&s, &[])
        .line("added REQ-0002")
        .map(str::to_string)
        .expect("listed");
    assert!(
        line.ends_with("same-address-as REQ-0001 (fields differ: asil,kind,mitigated_by)"),
        "{line}"
    );
    let json = diff(&s, &["--format", "json"]).json();
    let flags = json["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["claim_id"] == "REQ-0002")
        .expect("the added claim")["flags"]
        .clone();
    assert_eq!(
        flags,
        json!(["same-address-as REQ-0001 (fields differ: asil,kind,mitigated_by)"])
    );
}

#[test]
fn a_bare_renumber_keeps_todays_spelling_exactly() {
    let statement = "The REQ-0001 service shall respond to a health check within 200 ms.";
    let s = repo("bare-renumber");
    s.remove("claims/REQ-0001.yml");
    s.claim_saying("REQ-0002", statement, &kinded("C"));
    commit(&s, "head");
    let out = diff(&s, &[]);
    assert!(
        out.stdout
            .lines()
            .any(|l| l == "added REQ-0002 same-address-as REQ-0001"),
        "{}",
        out.stdout
    );

    // And with no kinds anywhere, as before this unit.
    let t = Scratch::new("bare-renumber-no-kinds");
    git_in(t.path(), &["init", "-q", "-b", "main"]);
    t.claim_saying("REQ-0001", statement, "");
    commit(&t, "base");
    t.remove("claims/REQ-0001.yml");
    t.claim_saying("REQ-0002", statement, "");
    commit(&t, "head");
    let out = diff(&t, &[]);
    assert!(
        out.stdout
            .lines()
            .any(|l| l == "added REQ-0002 same-address-as REQ-0001"),
        "{}",
        out.stdout
    );
}

// --- falsification 12: the declarations are reported once ----------------------------------------------

#[test]
fn an_edit_to_a_kind_file_prints_kinds_changed_and_sets_the_json_key() {
    let s = repo("kinds-changed");
    s.write(
        ".req/kinds/safety.yml",
        &SAFETY.replace("    required: true\n", ""),
    );
    commit(&s, "head");
    let out = diff(&s, &[]);
    assert_eq!(out.code, 0, "{}", out.all());
    let lines: Vec<&str> = out.stdout.lines().collect();
    let registry = lines
        .iter()
        .position(|l| *l == "registry: unchanged")
        .expect("the registry line");
    assert_eq!(
        lines.get(registry + 1),
        Some(&"kinds: changed"),
        "printed once, beside `registry:`:\n{}",
        out.stdout
    );
    assert_eq!(
        out.stdout.matches("kinds: changed").count(),
        1,
        "{}",
        out.stdout
    );
    let json = diff(&s, &["--format", "json"]).json();
    assert_eq!(json["kinds_changed"], true);
    assert_eq!(json["registry_changed"], false);
    assert_eq!(json["schema"], "req.diff/1", "additive: the schema stays");

    // A declaration added or removed is a change too.
    let s = repo("kinds-added");
    s.write(".req/kinds/security.yml", SECURITY);
    commit(&s, "head");
    assert_eq!(
        diff(&s, &["--format", "json"]).json()["kinds_changed"],
        true
    );
}

#[test]
fn no_edit_to_the_declarations_gives_false_and_no_line() {
    let s = repo("kinds-unchanged");
    s.claim("REQ-0001", &kinded("D"));
    commit(&s, "head");
    let out = diff(&s, &[]);
    assert!(!out.stdout.contains("kinds:"), "{}", out.stdout);
    assert_eq!(
        diff(&s, &["--format", "json"]).json()["kinds_changed"],
        false
    );

    // With no `.req/kinds/` on either side the key is still present, and false.
    let t = Scratch::new("kinds-none");
    git_in(t.path(), &["init", "-q", "-b", "main"]);
    t.claim("REQ-0001", "");
    commit(&t, "base");
    t.claim("REQ-0002", "");
    commit(&t, "head");
    let out = diff(&t, &[]);
    assert!(!out.stdout.contains("kinds:"), "{}", out.stdout);
    let doc = diff(&t, &["--format", "json"]).json();
    assert_eq!(doc["kinds_changed"], false);
}

// --- `req compare` --------------------------------------------------------------------------------------

#[test]
fn compare_prints_the_fields_line_with_no_direction_and_keeps_its_exit() {
    let s = Scratch::new("compare");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.write(
        "a.yml",
        &claim_text("REQ-0001", &own_statement("REQ-0001"), &kinded("C")),
    );
    s.write(
        "b.yml",
        &claim_text("REQ-0001", &own_statement("REQ-0001"), &kinded("QM")),
    );
    s.write(
        "same.yml",
        &claim_text("REQ-0001", &own_statement("REQ-0001"), &kinded("C")),
    );
    let out = s.req(&["compare", "a.yml", "b.yml"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout.contains("REQ-0001 -> REQ-0001: identical"),
        "the verdict stays `identical`: {}",
        out.stdout
    );
    let fields = out
        .line("fields:")
        .unwrap_or_else(|| panic!("the fields line is printed:\n{}", out.stdout));
    assert_eq!(
        fields,
        "fields: changed (asil) — not part of meaning; see req diff"
    );
    for word in ["weakened", "tightened", "strengthen", "lower", "higher"] {
        assert!(
            !out.stdout.contains(word),
            "no direction for a field: `{word}` in {}",
            out.stdout
        );
    }
    // The exit code is the one of a pair that differs in no field.
    let plain = s.req(&["compare", "a.yml", "same.yml"]);
    assert_eq!(plain.code, out.code);
    assert!(!plain.stdout.contains("fields:"), "{}", plain.stdout);
}

#[test]
fn compare_names_the_kind_first_and_the_fields_sorted() {
    let s = Scratch::new("compare-names");
    s.write(".req/kinds/safety.yml", SAFETY);
    s.write(".req/kinds/security.yml", SECURITY);
    s.write(
        "a.yml",
        &claim_text("REQ-0001", &own_statement("REQ-0001"), &kinded("C")),
    );
    s.write(
        "b.yml",
        &claim_text(
            "REQ-0001",
            &own_statement("REQ-0001"),
            "kind: security\nfields:\n  asil: D\n  review_due: 2027-03-01\n",
        ),
    );
    let out = s.req(&["compare", "a.yml", "b.yml"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert_eq!(
        out.line("fields:"),
        Some(
            "fields: changed (kind, asil, mitigated_by, review_due) — not part of meaning; see req diff"
        ),
        "{}",
        out.stdout
    );

    // A list reordered is not a change; a field added is.
    s.write(
        "c.yml",
        &claim_text(
            "REQ-0001",
            &own_statement("REQ-0001"),
            "kind: safety\nfields:\n  asil: C\n  mitigated_by: [REQ-0007, REQ-0002]\n",
        ),
    );
    let out = s.req(&["compare", "a.yml", "c.yml"]);
    assert!(!out.stdout.contains("fields:"), "{}", out.stdout);
}

// --- L5: the JSON views -----------------------------------------------------------------------------------

/// The keys of the object whose first key is `"claim_id": "<id>"`, in file order (the writer's order: a
/// JSON object read into `serde_json::Value` would be sorted).
fn keys_of_row(text: &str, id: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| {
            l.trim_start()
                .starts_with(&format!("\"claim_id\": \"{id}\""))
        })
        .unwrap_or_else(|| panic!("no row for {id}:\n{text}"));
    let indent = lines[start].len() - lines[start].trim_start().len();
    let mut keys = Vec::new();
    for line in &lines[start..] {
        let here = line.len() - line.trim_start().len();
        if here < indent {
            break;
        }
        if here == indent
            && let Some(rest) = line.trim_start().strip_prefix('"')
            && let Some((key, _)) = rest.split_once("\": ")
        {
            keys.push(key.to_string());
        }
    }
    keys
}

fn typed_corpus(name: &str) -> Scratch {
    let s = Scratch::new(name);
    s.write(".req/kinds/typed.yml", TYPED);
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim("REQ-0001", TYPED_CLAIM);
    s.claim("REQ-0002", "kind: safety\nfields:\n  asil: C\n");
    s.claim("REQ-0003", "");
    s
}

#[test]
fn render_rows_carry_kind_and_fields_right_after_area_and_nothing_moves() {
    let s = typed_corpus("render-json");
    let out = s.req(&["render", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let before = [
        "claim_id",
        "statement",
        "modality",
        "address",
        "applies",
        "selectors",
        "relations",
        "rationale",
        "superseded_by",
        "area",
    ];
    for id in ["REQ-0001", "REQ-0002", "REQ-0003"] {
        let keys = keys_of_row(&out.stdout, id);
        let expected: Vec<String> = before
            .iter()
            .map(|k| (*k).to_string())
            .chain(["kind".to_string(), "fields".to_string()])
            .collect();
        assert_eq!(
            keys, expected,
            "{id}: every old key keeps its position, `kind` and `fields` follow `area`"
        );
    }
    let doc = out.json();
    let row = |id: &str| -> Value {
        doc["applies"]
            .as_array()
            .expect("applies")
            .iter()
            .find(|r| r["claim_id"] == id)
            .unwrap_or_else(|| panic!("no row {id}"))
            .clone()
    };
    // Typed values: integers and decimals as strings, a boolean as a boolean, a list sorted.
    assert_eq!(row("REQ-0001")["kind"], "typed");
    assert_eq!(
        row("REQ-0001")["fields"],
        json!({
            "count": "3",
            "due": "2027-03-01",
            "flag": true,
            "level": "high",
            "parent": "REQ-0002",
            "ratio": "0.3",
            "tags": ["a", "b"],
            "title": "Hello",
        })
    );
    // The writer's order of the field names is sorted by byte.
    let block = &out.stdout[out.stdout.find("\"count\"").expect("count")..];
    let mut last = 0;
    for name in [
        "\"count\"",
        "\"due\"",
        "\"flag\"",
        "\"level\"",
        "\"parent\"",
        "\"ratio\"",
        "\"tags\"",
        "\"title\"",
    ] {
        let at = block.find(name).unwrap_or_else(|| panic!("{name} missing"));
        assert!(at >= last, "{name} is out of order:\n{block}");
        last = at;
    }
    // An absent field is absent, never `null`; no kind is `null`, and `fields` is then an empty object.
    assert_eq!(row("REQ-0002")["fields"], json!({"asil": "C"}));
    assert!(row("REQ-0002")["fields"].get("mitigated_by").is_none());
    assert_eq!(row("REQ-0003")["kind"], Value::Null);
    assert_eq!(row("REQ-0003")["fields"], json!({}));
}

#[test]
fn trace_nodes_carry_kind_and_fields_right_after_area() {
    let s = typed_corpus("trace-json");
    let out = s.req(&["trace", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    for id in ["REQ-0001", "REQ-0003"] {
        let keys = keys_of_row(&out.stdout, id);
        assert_eq!(
            keys,
            [
                "claim_id",
                "statement",
                "modality",
                "address",
                "applies",
                "superseded_by",
                "area",
                "kind",
                "fields",
                "source",
                "evidence",
                "tests"
            ],
            "{id}: `kind` and `fields` follow `area`, the rest keeps its order"
        );
    }
    let doc = out.json();
    let node = |id: &str| -> Value {
        doc["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|n| n["claim_id"] == id)
            .unwrap_or_else(|| panic!("no node {id}"))
            .clone()
    };
    assert_eq!(node("REQ-0001")["kind"], "typed");
    assert_eq!(node("REQ-0001")["fields"]["tags"], json!(["a", "b"]));
    assert_eq!(node("REQ-0001")["fields"]["flag"], true);
    assert_eq!(node("REQ-0003")["kind"], Value::Null);
    assert_eq!(node("REQ-0003")["fields"], json!({}));
    // A `ref` field adds no edge (decision 4): REQ-0001 names REQ-0002 in `parent` and relates to nothing.
    assert_eq!(doc["edges"], json!([]), "{}", out.stdout);
    assert_eq!(doc["derived_incoming"], json!([]));
}

#[test]
fn the_text_projections_do_not_change_with_a_kind_or_a_field() {
    // L4: no text projection prints a field. The same corpus with the kind and fields removed renders and traces
    // byte-identically.
    let with = typed_corpus("projection-with");
    let without = Scratch::new("projection-without");
    for id in ["REQ-0001", "REQ-0002", "REQ-0003"] {
        without.claim(id, "");
    }
    for args in [["render"], ["trace"]] {
        let a = with.req(&args);
        let b = without.req(&args);
        assert_eq!(a.code, b.code, "{args:?}");
        assert_eq!(a.stdout, b.stdout, "{args:?} prints no field");
        assert!(!a.stdout.is_empty());
    }
    // The committed projections of this repository pass `--check` unchanged.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let spec = run(repo, &["render", "--check", "docs/spec.md"]);
    assert_eq!(spec.code, 0, "docs/spec.md is current:\n{}", spec.all());
    let trace_args: Vec<&str> = std::iter::once("trace")
        .chain(["--check", "docs/trace.md"])
        .chain(
            "corpus_set=parity_and_adversarial set=direction_gold_structural deployment_mode=air_gapped field=value"
                .split(' '),
        )
        .collect();
    let trace = run(repo, &trace_args);
    assert_eq!(trace.code, 0, "docs/trace.md is current:\n{}", trace.all());
}

// --- falsification 6: the two findings ----------------------------------------------------------------------

/// Two related claims, each named by a signed-looking Baseline, so the completeness step's only open
/// questions are the ones a test adds.
fn filed(name: &str, one: &str, two: &str) -> Scratch {
    let s = Scratch::new(name);
    s.write(".req/kinds/safety.yml", SAFETY);
    s.claim("REQ-0001", one);
    s.write(
        "claims/REQ-0002.yml",
        &claim_text("REQ-0002", &own_statement("REQ-0002"), two).replace(
            "relations: []",
            "relations:\n  - type: refines\n    to: REQ-0001",
        ),
    );
    let h = "0".repeat(64);
    let claims: Vec<String> = ["REQ-0001", "REQ-0002"]
        .iter()
        .map(|id| {
            format!(
                "{{\"claim_id\":\"{id}\",\"address\":\"{h}\",\"relations_hash\":\"{h}\",\
                 \"rationale_hash\":\"{h}\",\"tests_hash\":\"{h}\"}}"
            )
        })
        .collect();
    s.write(
        "baselines/release.json",
        &format!(
            "{{\"version\":4,\"signed_at\":\"2026-11-11T00:00:00Z\",\"authority\":{{\"head\":\"{h}\",\
             \"entries\":1}},\"rules_v\":8,\"selection\":{{}},\"claims\":[{}]}}",
            claims.join(",")
        ),
    );
    s.write(
        "baselines/release.json.minisig",
        "untrusted comment: fixture\n",
    );
    s
}

fn completeness_json(out: &Out) -> Value {
    out.json()["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|st| st["kind"] == "completeness")
        .expect("a completeness step")["completeness"]
        .clone()
}

#[test]
fn a_missing_required_field_is_unverifiable_never_pass_and_fail_under_strict_fields() {
    let s = filed(
        "missing-field",
        &plain("C"),
        "kind: safety\nfields:\n  review_due: 2027-03-01\n",
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "UNVERIFIABLE, never PASS:\n{}", out.all());
    let block = out.completeness_block();
    assert!(block.starts_with("[UNVERIFIABLE] completeness "), "{block}");
    let note = block
        .lines()
        .find(|l| l.contains("missing_field"))
        .unwrap_or_else(|| panic!("the finding is named:\n{block}"));
    assert!(
        note.contains("REQ-0002") && note.contains("safety") && note.contains("asil"),
        "names the claim, the kind and the field: {note}"
    );
    assert!(
        !note.contains("REQ-0001"),
        "a claim with the field is no finding: {note}"
    );

    let json = s.req(&["check", "--format", "json"]);
    let found = completeness_json(&json)["missing_field"].clone();
    assert_eq!(
        found,
        json!([{"claim_id": "REQ-0002", "kind": "safety", "field": "asil"}])
    );

    // `strict fields` promotes it to FAIL.
    s.write(".req/completeness", "strict fields\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.completeness_block().starts_with("[FAIL] completeness "),
        "{}",
        out.stdout
    );
    assert_eq!(
        completeness_json(&s.req(&["check", "--format", "json"]))["missing_field_judged"],
        true
    );

    // Supplied, there is no finding, and the step passes under the strict word.
    s.write(
        "claims/REQ-0002.yml",
        &claim_text("REQ-0002", &own_statement("REQ-0002"), &plain("A")).replace(
            "relations: []",
            "relations:\n  - type: refines\n    to: REQ-0001",
        ),
    );
    let out = s.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains("missing_field"), "{}", out.all());
}

#[test]
fn a_dangling_field_ref_is_unverifiable_and_names_the_claim_the_field_and_the_id() {
    let s = filed(
        "dangling",
        "kind: safety\nfields:\n  asil: C\n  mitigated_by: [REQ-0002, REQ-9999]\n",
        &kinded("A"),
    );
    // REQ-0002 names REQ-0007, which is not in the corpus either.
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let block = out.completeness_block();
    let notes: Vec<&str> = block
        .lines()
        .filter(|l| l.contains("dangling_field_ref"))
        .collect();
    assert_eq!(notes.len(), 1, "one note:\n{block}");
    let note = notes[0];
    assert!(
        note.contains("REQ-0001")
            && note.contains("mitigated_by")
            && note.contains("REQ-9999")
            && note.contains("REQ-0007"),
        "{note}"
    );
    let found =
        completeness_json(&s.req(&["check", "--format", "json"]))["dangling_field_ref"].clone();
    assert_eq!(
        found,
        json!([
            {"claim_id": "REQ-0001", "field": "mitigated_by", "ref": "REQ-9999"},
            {"claim_id": "REQ-0002", "field": "mitigated_by", "ref": "REQ-0007"},
        ])
    );

    // `strict fields` does not promote it: it is the class of a dangling relation.
    s.write(".req/completeness", "strict fields\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 3, "{}", out.all());

    // A `ref` that names a claim the corpus holds is no finding, and a `ref` field adds no edge.
    let ok = filed(
        "not-dangling",
        "kind: safety\nfields:\n  asil: C\n  mitigated_by: [REQ-0002]\n",
        "kind: safety\nfields:\n  asil: A\n  mitigated_by: [REQ-0001]\n",
    );
    let out = ok.req(&["check"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(!out.all().contains("dangling_field_ref"), "{}", out.all());
}

#[test]
fn a_corpus_with_no_kinds_produces_neither_finding_and_no_new_line() {
    let s = Scratch::new("no-kinds");
    s.claim("REQ-0001", "");
    let text = s.req(&["check"]);
    let json = s.req(&["check", "--format", "json"]);
    for out in [&text, &json] {
        assert!(!out.all().contains("missing_field"), "{}", out.all());
        assert!(!out.all().contains("dangling_field_ref"), "{}", out.all());
        assert!(!out.all().contains("strict fields"), "{}", out.all());
    }
    assert!(
        text.completeness_block().contains("(3 checks)"),
        "the step's label does not move:\n{}",
        text.stdout
    );
    // `strict fields` with no kinds is read, and judges nothing.
    s.write(".req/completeness", "strict fields\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, text.code, "{}", out.all());
    assert!(!out.all().contains("missing_field"), "{}", out.all());
}

#[test]
fn strict_fields_is_a_word_of_the_completeness_grammar_and_the_error_names_it() {
    let s = filed("grammar", &plain("C"), &plain("A"));
    s.write(".req/completeness", "strict fields\n");
    assert_eq!(s.req(&["check"]).code, 0);
    s.write(".req/completeness", "strict nonsense\n");
    let out = s.req(&["check"]);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.stderr.contains("strict fields"), "{}", out.stderr);
}

// --- the audit packet ------------------------------------------------------------------------------------------

#[test]
fn the_audit_packet_names_both_findings_and_prints_no_field_value() {
    let s = Scratch::new("audit");
    s.write(
        ".req/kinds/safety.yml",
        &format!("{SAFETY}  note:\n    type: string\n"),
    );
    s.claim(
        "REQ-0001",
        "kind: safety\nfields:\n  note: zzq-distinctive-value-7\n  mitigated_by: [REQ-7777]\n",
    );
    s.claim("REQ-0002", &kinded("QM"));
    let out = s.req(&["audit"]);
    let packet = out.stdout.clone();
    let line = |word: &str| -> String {
        packet
            .lines()
            .find(|l| l.contains(word))
            .unwrap_or_else(|| panic!("the packet names `{word}`:\n{packet}"))
            .to_string()
    };
    let missing = line("missing_field");
    assert!(
        missing.contains("REQ-0001") && missing.contains("safety") && missing.contains("asil"),
        "{missing}"
    );
    let dangling = line("dangling_field_ref");
    assert!(
        dangling.contains("REQ-0001") && dangling.contains("mitigated_by"),
        "{dangling}"
    );
    for value in ["zzq-distinctive-value-7", "REQ-7777"] {
        assert!(
            !out.all().contains(value),
            "no field value in the packet: `{value}`\n{}",
            out.all()
        );
    }
    // The same in the JSON document.
    let json = s.req(&["audit", "--format", "json"]);
    assert!(
        !json.all().contains("zzq-distinctive-value-7") && !json.all().contains("REQ-7777"),
        "{}",
        json.all()
    );
    assert!(json.stdout.contains("missing_field"), "{}", json.stdout);

    // A corpus with no kinds prints neither.
    let plain = Scratch::new("audit-plain");
    plain.claim("REQ-0001", "");
    let out = plain.req(&["audit"]);
    assert!(!out.all().contains("missing_field"), "{}", out.all());
    assert!(!out.all().contains("dangling_field_ref"), "{}", out.all());
}

// --- falsification 9: the readers are closed ----------------------------------------------------------------

/// The code of a source file, comments removed, as the text a scan reads.
fn code_of(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every place `code` could read a claim's kind or a field's value. Returns what matched.
fn field_reads(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (index, line) in code.lines().enumerate() {
        let member = [".fields", ".kind"].iter().any(|token| {
            line.match_indices(token).any(|(at, _)| {
                !line[at + token.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
            })
        });
        let named = [
            "fields_hash",
            "JudgedField",
            "FieldValue",
            "crate::kinds",
            "kinds::",
            "Declarations",
            "judge_fields",
        ]
        .iter()
        .any(|token| line.contains(token));
        if member || named {
            found.push(format!("line {}: {}", index + 1, line.trim()));
        }
    }
    found
}

#[test]
fn the_scan_finds_a_read_when_there_is_one() {
    // The detector's own control: a scan that cannot fire passes vacuously.
    assert!(!field_reads("let x = row.fields.len();").is_empty());
    assert!(!field_reads("let k = claim.kind.as_deref();").is_empty());
    assert!(!field_reads("use crate::kinds::FieldValue;").is_empty());
    assert!(field_reads("let r = relation.kind_name;").is_empty());
    assert!(field_reads("let x = row.fields_other;").is_empty());
}

#[test]
fn no_gate_policy_selector_contradiction_check_or_text_projection_reads_a_field() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |file: &str| {
        fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file} is readable: {e}"))
    };
    // The text renderers of `src/view.rs`: from `render` to the JSON documents' section. Anchors, so a moved
    // function cannot leave the scan reading nothing.
    let view = read("src/view.rs");
    let start = view.find("pub fn render(").expect("`render` is in view.rs");
    let end = view
        .find("// JSON documents (pack C)")
        .expect("the JSON section marker is in view.rs");
    assert!(
        start < end,
        "the text renderers come before the JSON documents"
    );
    let renderers = view[start..end].to_string();

    let scanned: [(&str, String, &str); 4] = [
        ("src/gate.rs", read("src/gate.rs"), "pub fn exit_code("),
        ("src/select.rs", read("src/select.rs"), "pub fn applies("),
        (
            "src/contradict.rs",
            read("src/contradict.rs"),
            "pub fn find(",
        ),
        ("src/view.rs (text renderers)", renderers, "pub fn render("),
    ];
    let mut lines_scanned = 0;
    for (file, text, anchor) in &scanned {
        let code = code_of(text);
        let count = code.lines().count();
        assert!(
            count > 40,
            "{file}: the scan must read real code, read {count} lines"
        );
        assert!(
            text.contains(*anchor),
            "{file}: the anchor `{anchor}` is the proof the right file was read"
        );
        lines_scanned += count;
        let reads = field_reads(&code);
        assert!(
            reads.is_empty(),
            "{file} reads a claim's kind or a field (ADR 0056 decision 9: no gate policy, selector, projection or \
             exit code reads a field's value):\n{}",
            reads.join("\n")
        );
    }
    assert!(lines_scanned > 400, "scanned {lines_scanned} lines");
}

#[test]
fn two_corpora_that_differ_only_in_a_field_give_identical_gate_select_and_view_answers() {
    let build = |name: &str, asil: &str, note: &str| {
        let s = Scratch::new(name);
        s.write(
            ".req/kinds/safety.yml",
            &format!("{SAFETY}  note:\n    type: string\n"),
        );
        s.claim(
            "REQ-0001",
            &format!("kind: safety\nfields:\n  asil: {asil}\n  note: {note}\n"),
        );
        s.claim("REQ-0002", &kinded(asil));
        s
    };
    let a = build("closed-a", "C", "alpha");
    let b = build("closed-b", "QM", "omega");
    let commands: [&[&str]; 8] = [
        &["gate"],
        &["select"],
        &["render"],
        &["trace"],
        &["canon"],
        &["gate", "--format", "json"],
        &["select", "mode=a"],
        &["impact", "REQ-0001"],
    ];
    let mut compared = 0;
    for args in commands {
        let (x, y) = (a.req(args), b.req(args));
        assert_eq!(
            x.code,
            y.code,
            "{args:?}: one exit code\n{}\n{}",
            x.all(),
            y.all()
        );
        if args[0] == "canon" {
            // `canon` prints the fields as diagnostics (decision 8); only its reading of identity must agree.
            let strip = |t: &str| {
                t.lines()
                    .map(|l| l.split(" kind ").next().unwrap_or(l))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            assert_eq!(strip(&x.stdout), strip(&y.stdout), "{args:?}");
        } else {
            assert_eq!(x.stdout, y.stdout, "{args:?}: byte-identical output");
        }
        // stderr names the corpus root, which is each scratch directory's own location: compare all else.
        let quiet = |t: &str| {
            t.lines()
                .filter(|l| !l.starts_with("corpus root:"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(quiet(&x.stderr), quiet(&y.stderr), "{args:?}");
        compared += 1;
    }
    assert_eq!(compared, commands.len());
}

// --- `req pin` -------------------------------------------------------------------------------------------------

fn pin_file(s: &Scratch) -> String {
    let rev = git_in(s.path(), &["rev-parse", "base"]);
    s.write("corpus.pin", &format!("rev: {rev}\n"));
    s.path().join("corpus.pin").to_string_lossy().into_owned()
}

#[test]
fn pin_verify_fails_on_an_uncommitted_change_under_kinds_and_names_the_file() {
    let s = repo("pin-verify");
    let pin = pin_file(&s);
    let clean = s.req(&["pin", "--verify", &pin]);
    assert_eq!(clean.code, 0, "{}", clean.all());

    // Edited, uncommitted: a gate would judge declarations other than the pin's.
    s.write(
        ".req/kinds/safety.yml",
        &SAFETY.replace("    required: true\n", ""),
    );
    let out = s.req(&["pin", "--verify", &pin]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.stdout.contains("[FAIL]"), "{}", out.stdout);
    assert!(
        out.stdout.contains(".req/kinds/safety.yml"),
        "names the file: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("claims/")
            && out.stdout.contains("aliases/")
            && out.stdout.contains(".req/kinds/"),
        "the message names all three directories: {}",
        out.stdout
    );

    // An untracked kind file is a change too.
    let s = repo("pin-verify-untracked");
    let pin = pin_file(&s);
    s.write(".req/kinds/security.yml", SECURITY);
    let out = s.req(&["pin", "--verify", &pin]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout.contains(".req/kinds/security.yml"),
        "{}",
        out.stdout
    );
}

#[test]
fn pin_drift_reports_kinds_changed() {
    let s = repo("pin-drift");
    let pin = pin_file(&s);
    s.write(
        ".req/kinds/safety.yml",
        &SAFETY.replace("    required: true\n", ""),
    );
    commit(&s, "head");
    let out = s.req(&["pin", "--drift", &pin, "--against", "head"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout.lines().any(|l| l == "kinds: changed"),
        "{}",
        out.stdout
    );
}

// --- the help pages (one home per rule) ----------------------------------------------------------------------

#[test]
fn the_help_pages_state_the_new_surface() {
    let help = |command: &str| {
        let out = run(Path::new("."), &[command, "--help"]);
        assert_eq!(out.code, 0, "{command}: {}", out.all());
        out.stdout
    };
    let diff = help("diff");
    for word in ["fields-changed", "kinds: changed", "kinds_changed"] {
        assert!(diff.contains(word), "diff --help names `{word}`");
    }
    assert!(help("compare").contains("fields: changed"));
    let check = help("check");
    for word in ["missing_field", "dangling_field_ref", "strict fields"] {
        assert!(check.contains(word), "check --help names `{word}`");
    }
    assert!(
        help("trace").contains("kind"),
        "trace --help names the node keys"
    );
    assert!(
        help("pin").contains(".req/kinds"),
        "pin --help names the pathspec"
    );
    let audit = help("audit");
    assert!(
        audit.contains("missing_field") && audit.contains("dangling_field_ref"),
        "audit --help lists the findings"
    );
}
