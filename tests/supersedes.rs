//! `supersedes` withdraws a claim where its successor applies (ADR 0049, unit W-1).
//!
//! Public seams only: [`req::view::load_rows`] over a scratch corpus and the compiled `req` (gate JSON,
//! baseline emit, the views). Every oracle is the ADR's decision or one of its six *Falsification*
//! items, written before the code and never read back from a run. The relation is authored on the
//! **successor** and points at the claim it replaces (decision 1): "REQ-0002 supersedes REQ-0001" is
//! written in REQ-0002's file.

use req::select::{Applies, Context};
use req::view::Row;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// At most 100 ms, and at least 200 ms: no value satisfies both (ADR 0043 decision 2), and the
/// subject/action/qualifier slots agree, so Tier 1 compares them.
const SHORT: &str = "The API shall respond in no more than 100 ms.";
const LONG: &str = "The API shall respond in at least 200 ms.";

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

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-supersedes-{}-{}-{name}",
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

    /// One claim: the statement, the applicability predicates and the outgoing relations.
    fn claim(&self, id: &str, statement: &str, applicability: &[&str], relations: &[(&str, &str)]) {
        let applicability = if applicability.is_empty() {
            "applicability: []".to_string()
        } else {
            let lines: Vec<String> = applicability.iter().map(|p| format!("  - {p}")).collect();
            format!("applicability:\n{}", lines.join("\n"))
        };
        let relations = if relations.is_empty() {
            "relations: []".to_string()
        } else {
            let lines: Vec<String> = relations
                .iter()
                .map(|(kind, to)| format!("  - type: {kind}\n    to: {to}"))
                .collect();
            format!("relations:\n{}", lines.join("\n"))
        };
        let text = format!(
            "schema: req.claim/2\n\
             claim_id: {id}\n\
             statement: \"{statement}\"\n\
             {applicability}\n\
             {relations}\n\
             evidence: process\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-12\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    fn req(&self, args: &[&str]) -> Out {
        let out = Command::new(req_bin())
            .args(args)
            .current_dir(&self.0)
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs req");
        Out {
            code: out.status.code().expect("req exits, never a signal"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn json(&self, args: &[&str]) -> (Out, serde_json::Value) {
        let mut all = args.to_vec();
        all.extend_from_slice(&["--format", "json"]);
        let out = self.req(&all);
        let document = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|e| panic!("{args:?} JSON: {e}\n{}{}", out.stdout, out.stderr));
        (out, document)
    }

    fn gate_json(&self, extra: &[&str]) -> (Out, serde_json::Value) {
        let mut args = vec!["gate"];
        args.extend(extra);
        self.json(&args)
    }

    fn rows(&self, pairs: &[(&str, &str)]) -> Vec<Row> {
        let context: Context = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        req::view::load_rows(&context, &self.0).expect("the scratch corpus loads")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn policy<'a>(document: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == name)
        .unwrap_or_else(|| panic!("the document carries a `{name}` policy: {document}"))
}

fn row<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter()
        .find(|row| row.claim_id == id)
        .unwrap_or_else(|| panic!("{id} is in the corpus"))
}

/// The row's withdrawn successors, by the field the view carries.
fn superseded_by(rows: &[Row], id: &str) -> Vec<String> {
    row(rows, id).superseded_by.clone()
}

/// The motivating pair: REQ-0001 (at least 200 ms) is replaced by REQ-0002 (at most 100 ms).
fn motivating(s: &Scratch) {
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("supersedes", "REQ-0001")]);
}

// ---------------------------------------------------------------------------
// decision 4 and the motivating case: supersession resolves a Conflict
// ---------------------------------------------------------------------------

#[test]
fn a_successor_resolves_the_contradiction_it_replaces_and_without_it_the_gate_fails() {
    let s = Scratch::new("motivating");
    motivating(&s);
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 0, "{}", out.all());
    let entry = policy(&document, "contradictions");
    assert_eq!(entry["outcome"], "pass", "{entry}");
    let details = entry["details"].to_string();
    assert!(
        !details.contains("REQ-0001") && !details.contains("REQ-0002"),
        "the policy names neither claim: {details}"
    );

    // The same two claims without the relation: the unrecorded contradiction FAILs.
    s.claim("REQ-0002", SHORT, &[], &[]);
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert_eq!(policy(&document, "contradictions")["outcome"], "fail");
}

#[test]
fn a_recorded_conflict_with_the_applying_successor_no_longer_makes_the_policy_unverifiable() {
    let s = Scratch::new("recorded-conflict");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim(
        "REQ-0002",
        SHORT,
        &[],
        &[("supersedes", "REQ-0001"), ("conflicts-with", "REQ-0001")],
    );
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert_eq!(policy(&document, "contradictions")["outcome"], "pass");

    // Contrast: the same recorded Conflict without the supersession is UNVERIFIABLE (ADR 0043).
    s.claim("REQ-0002", SHORT, &[], &[("conflicts-with", "REQ-0001")]);
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert_eq!(
        policy(&document, "contradictions")["outcome"],
        "unverifiable"
    );
}

// ---------------------------------------------------------------------------
// decision 2: the fold, per selection, over the successor's own answer
// ---------------------------------------------------------------------------

/// Falsification 1: a claim whose successor applies is not applying there.
#[test]
fn a_claim_whose_successor_applies_is_withdrawn_and_names_it() {
    let s = Scratch::new("withdrawn");
    motivating(&s);
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::No);
    assert_eq!(superseded_by(&rows, "REQ-0001"), ["REQ-0002"]);
    assert_eq!(row(&rows, "REQ-0002").applies, Applies::Yes);
    assert!(superseded_by(&rows, "REQ-0002").is_empty());
}

/// Falsification 2, and ADR 0003: a successor scoped to one variant supersedes only there.
#[test]
fn a_successor_scoped_to_a_variant_withdraws_its_predecessor_in_that_variant_only() {
    let s = Scratch::new("variant-scoped");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim(
        "REQ-0002",
        SHORT,
        &["variant == x"],
        &[("supersedes", "REQ-0001")],
    );

    let under_x = s.rows(&[("variant", "x")]);
    assert_eq!(row(&under_x, "REQ-0001").applies, Applies::No);
    assert_eq!(superseded_by(&under_x, "REQ-0001"), ["REQ-0002"]);

    let under_y = s.rows(&[("variant", "y")]);
    assert_eq!(
        row(&under_y, "REQ-0001").applies,
        Applies::Yes,
        "the successor does not apply under variant=y, so nothing is withdrawn there"
    );
    assert!(superseded_by(&under_y, "REQ-0001").is_empty());
    assert_eq!(row(&under_y, "REQ-0002").applies, Applies::No);

    // And the gate agrees per selection: the contradiction exists only where both apply.
    let (out, document) = s.gate_json(&["variant=x"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert_eq!(policy(&document, "contradictions")["outcome"], "pass");
}

/// ADR 0049 decision 2, *Rejected*: the fold reads each successor's **own** answer, so a chain works
/// without recursion and a withdrawn B does not bring A back.
#[test]
fn a_chain_withdraws_every_claim_that_has_an_applying_successor() {
    let s = Scratch::new("chain");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("supersedes", "REQ-0001")]);
    s.claim(
        "REQ-0003",
        "The API shall respond in no more than 50 ms.",
        &[],
        &[("supersedes", "REQ-0002")],
    );
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::No);
    assert_eq!(superseded_by(&rows, "REQ-0001"), ["REQ-0002"]);
    assert_eq!(row(&rows, "REQ-0002").applies, Applies::No);
    assert_eq!(superseded_by(&rows, "REQ-0002"), ["REQ-0003"]);
    assert_eq!(row(&rows, "REQ-0003").applies, Applies::Yes);
}

/// Falsification 3: an undecided successor leaves the claim undecided, never applying and never excluded.
#[test]
fn an_undecided_successor_makes_an_otherwise_applying_claim_undecided_and_the_gate_names_it() {
    let s = Scratch::new("undecided-successor");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim(
        "REQ-0002",
        SHORT,
        &["variant == x"],
        &[("supersedes", "REQ-0001")],
    );
    // The context decides nothing about `variant`.
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::Unknown);
    assert!(
        superseded_by(&rows, "REQ-0001").is_empty(),
        "undecided is not withdrawn"
    );

    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 3, "{}", out.all());
    let undecided: Vec<&str> = document["undecided"]
        .as_array()
        .expect("undecided[]")
        .iter()
        .map(|u| u["claim_id"].as_str().expect("a claim id"))
        .collect();
    assert!(undecided.contains(&"REQ-0001"), "{document}");
    let text = s.req(&["gate"]);
    assert_eq!(text.code, 3, "{}", text.all());
    assert!(
        text.stdout.contains("REQ-0001"),
        "the text report names it: {}",
        text.stdout
    );
}

/// The same clause, its other half: when the claim's own answer is *does not apply*, an undecided
/// successor changes nothing.
#[test]
fn an_undecided_successor_does_not_revive_a_claim_that_does_not_apply() {
    let s = Scratch::new("undecided-over-no");
    s.claim("REQ-0001", LONG, &["mode == a"], &[]);
    s.claim(
        "REQ-0002",
        SHORT,
        &["variant == x"],
        &[("supersedes", "REQ-0001")],
    );
    let rows = s.rows(&[("mode", "b")]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::No);
    assert!(superseded_by(&rows, "REQ-0001").is_empty());
}

/// Any applying successor wins over an undecided one (ADR decision 2, first bullet).
#[test]
fn an_applying_successor_outranks_an_undecided_one() {
    let s = Scratch::new("yes-over-unknown");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("supersedes", "REQ-0001")]);
    s.claim(
        "REQ-0003",
        "The API shall respond in no more than 50 ms.",
        &["variant == x"],
        &[("supersedes", "REQ-0001")],
    );
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::No);
    assert_eq!(
        superseded_by(&rows, "REQ-0001"),
        ["REQ-0002"],
        "only the successors that apply are named"
    );
}

#[test]
fn a_relation_listed_twice_names_the_successor_once_and_several_successors_come_sorted() {
    let s = Scratch::new("sorted");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim(
        "REQ-0003",
        SHORT,
        &[],
        &[("supersedes", "REQ-0001"), ("supersedes", "REQ-0001")],
    );
    s.claim(
        "REQ-0002",
        "The API shall respond in no more than 50 ms.",
        &[],
        &[("supersedes", "REQ-0001")],
    );
    let rows = s.rows(&[]);
    assert_eq!(superseded_by(&rows, "REQ-0001"), ["REQ-0002", "REQ-0003"]);
}

// ---------------------------------------------------------------------------
// decision 5 and 6: cycles and dangling edges
// ---------------------------------------------------------------------------

/// Falsification 4: a cycle never passes, and withdraws nobody.
#[test]
fn a_self_loop_fails_relations_resolve_naming_the_cycle_and_withdraws_nothing() {
    let s = Scratch::new("self-loop");
    s.claim("REQ-0001", LONG, &[], &[("supersedes", "REQ-0001")]);
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::Yes);
    assert!(superseded_by(&rows, "REQ-0001").is_empty());

    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 1, "{}", out.all());
    let entry = policy(&document, "relations_resolve");
    assert_eq!(entry["outcome"], "fail", "{entry}");
    let details = entry["details"].to_string();
    assert_eq!(
        details
            .matches("supersedes cycle: REQ-0001 -> REQ-0001")
            .count(),
        1,
        "named once: {details}"
    );
}

#[test]
fn a_two_claim_cycle_fails_in_every_selection_is_named_once_and_withdraws_no_member() {
    let s = Scratch::new("two-cycle");
    // Written from the higher id first, so a name that follows file order and not id order fails.
    s.claim(
        "REQ-0002",
        SHORT,
        &["variant == x"],
        &[("supersedes", "REQ-0001")],
    );
    s.claim(
        "REQ-0001",
        LONG,
        &["variant == x"],
        &[("supersedes", "REQ-0002")],
    );
    let under_x = s.rows(&[("variant", "x")]);
    for id in ["REQ-0001", "REQ-0002"] {
        assert_eq!(row(&under_x, id).applies, Applies::Yes, "{id}");
        assert!(superseded_by(&under_x, id).is_empty(), "{id}");
    }
    // FAIL whether or not the members apply: under variant=y neither does.
    for context in ["variant=x", "variant=y"] {
        let (out, document) = s.gate_json(&[context]);
        assert_eq!(out.code, 1, "{context}: {}", out.all());
        let entry = policy(&document, "relations_resolve");
        assert_eq!(entry["outcome"], "fail", "{context}: {entry}");
        let details = entry["details"].to_string();
        assert_eq!(
            details
                .matches("supersedes cycle: REQ-0001 -> REQ-0002 -> REQ-0001")
                .count(),
            1,
            "{context}: named once, rotated to its lowest id: {details}"
        );
    }
}

/// A claim outside the cycle that supersedes a member does not withdraw it: a member keeps its own
/// answer as a target as well as a successor (ADR decision 5).
#[test]
fn a_cycle_member_is_not_withdrawn_by_a_successor_outside_the_cycle() {
    let s = Scratch::new("cycle-target");
    s.claim("REQ-0001", LONG, &[], &[("supersedes", "REQ-0002")]);
    s.claim("REQ-0002", SHORT, &[], &[("supersedes", "REQ-0001")]);
    s.claim(
        "REQ-0003",
        "The API shall respond in no more than 50 ms.",
        &[],
        &[("supersedes", "REQ-0001")],
    );
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::Yes);
    assert_eq!(row(&rows, "REQ-0002").applies, Applies::Yes);
    assert!(superseded_by(&rows, "REQ-0001").is_empty());
}

/// And the converse: a cycle member does not withdraw a claim outside the cycle.
#[test]
fn a_cycle_member_does_not_withdraw_a_claim_outside_the_cycle_as_a_successor() {
    let s = Scratch::new("cycle-successor");
    s.claim("REQ-0001", LONG, &[], &[("supersedes", "REQ-0002")]);
    s.claim(
        "REQ-0002",
        SHORT,
        &[],
        &[("supersedes", "REQ-0001"), ("supersedes", "REQ-0003")],
    );
    s.claim(
        "REQ-0003",
        "The API shall respond in no more than 50 ms.",
        &[],
        &[],
    );
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0003").applies, Applies::Yes);
    assert!(superseded_by(&rows, "REQ-0003").is_empty());
}

/// Decision 6: an edge that silently names nothing is decay, so it is reported even when its
/// successor does not apply here.
#[test]
fn a_dangling_supersedes_on_a_non_applying_successor_is_unverifiable() {
    let s = Scratch::new("dangling");
    s.claim(
        "REQ-0001",
        LONG,
        &["variant == x"],
        &[("supersedes", "REQ-9999")],
    );
    let (out, document) = s.gate_json(&["variant=y"]);
    assert_eq!(out.code, 3, "{}", out.all());
    let entry = policy(&document, "relations_resolve");
    assert_eq!(entry["outcome"], "unverifiable", "{entry}");
    assert!(
        entry["details"].to_string().contains("REQ-9999"),
        "the missing claim is named: {entry}"
    );
}

/// The existing rule is unchanged for every other kind: a relation of a claim that does not apply
/// is not examined.
#[test]
fn a_dangling_relation_of_another_kind_on_a_non_applying_claim_still_passes() {
    let s = Scratch::new("dangling-other-kind");
    s.claim(
        "REQ-0001",
        LONG,
        &["variant == x"],
        &[("refines", "REQ-9999")],
    );
    let (_, document) = s.gate_json(&["variant=y"]);
    assert_eq!(policy(&document, "relations_resolve")["outcome"], "pass");
}

// ---------------------------------------------------------------------------
// decision 3 and 7: every view reads the one evaluation, additively
// ---------------------------------------------------------------------------

fn withdrawn_corpus(name: &str) -> Scratch {
    let s = Scratch::new(name);
    motivating(&s);
    s
}

#[test]
fn render_json_carries_superseded_by_on_every_row_and_the_text_view_names_the_successor() {
    let s = withdrawn_corpus("render");
    let (out, document) = s.json(&["render"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let outside = document["outside"].as_array().expect("outside[]");
    assert_eq!(outside.len(), 1, "{document}");
    assert_eq!(outside[0]["claim_id"], "REQ-0001");
    assert_eq!(outside[0]["applies"], "does_not_apply");
    assert_eq!(outside[0]["superseded_by"], serde_json::json!(["REQ-0002"]));
    let applies = document["applies"].as_array().expect("applies[]");
    assert_eq!(applies[0]["claim_id"], "REQ-0002");
    assert_eq!(
        applies[0]["superseded_by"],
        serde_json::json!([]),
        "the key is on every row, empty when none"
    );
    assert_eq!(
        document["schema"], "req.render/1",
        "additive: the schema stays"
    );

    let text = s.req(&["render"]);
    assert_eq!(text.code, 0, "{}", text.all());
    assert!(
        text.stdout
            .contains("REQ-0001  does not apply (superseded by REQ-0002)"),
        "{}",
        text.stdout
    );
}

#[test]
fn select_names_a_withdrawn_claim_and_its_successors() {
    let s = withdrawn_corpus("select");
    let out = s.req(&["select"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .contains("REQ-0001  does not apply (superseded by REQ-0002)"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("REQ-0002  applies"), "{}", out.stdout);
    // The summary line keeps its shape: a withdrawn claim is counted as not applying.
    assert!(
        out.stdout
            .contains("applies 1, does not apply 1, unknown 0"),
        "{}",
        out.stdout
    );
}

#[test]
fn trace_and_impact_name_the_successor_of_a_withdrawn_claim() {
    let s = Scratch::new("trace-impact");
    s.claim("REQ-0004", LONG, &[], &[]);
    s.claim("REQ-0003", SHORT, &[], &[("refines", "REQ-0004")]);
    s.claim(
        "REQ-0005",
        "The API shall respond in no more than 50 ms.",
        &[],
        &[("supersedes", "REQ-0003")],
    );
    let impact = s.req(&["impact", "REQ-0004"]);
    assert!(
        impact
            .stdout
            .contains("REQ-0003  distance 1  does not apply (superseded by REQ-0005)"),
        "{}{}",
        impact.stdout,
        impact.stderr
    );
    let trace = s.req(&["trace"]);
    assert!(
        trace.stdout.contains(
            "REQ-0005 -supersedes-> REQ-0003  [target does not apply (superseded by REQ-0005)]"
        ),
        "{}{}",
        trace.stdout,
        trace.stderr
    );
}

#[test]
fn the_audit_packet_lists_the_withdrawn_claim_in_a_superseded_section_only_when_one_exists() {
    let s = withdrawn_corpus("audit");
    let out = s.req(&["audit"]);
    assert!(
        out.stdout.contains("## Superseded"),
        "{}{}",
        out.stdout,
        out.stderr
    );
    let section: String = out
        .stdout
        .split("## Superseded")
        .nth(1)
        .expect("the section")
        .lines()
        .skip(1)
        .take_while(|line| !line.starts_with("##") && !line.starts_with("verdict"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        section.contains("REQ-0001") && section.contains("REQ-0002"),
        "the withdrawn claim and its successor: {section}"
    );
    // The withdrawn claim is not an applying claim of the packet.
    assert!(
        out.stdout
            .contains("claims: 1 applying, 0 undecided, 1 excluded"),
        "{}",
        out.stdout
    );

    // No `supersedes` anywhere: no new section, no new word, in any text view.
    let plain = Scratch::new("audit-plain");
    plain.claim("REQ-0001", LONG, &[], &[]);
    plain.claim(
        "REQ-0002",
        "The API shall respond in no more than 300 ms.",
        &[],
        &[],
    );
    for args in [["audit"], ["render"], ["select"]] {
        let out = plain.req(&args);
        assert!(
            !out.stdout.to_lowercase().contains("superseded"),
            "{args:?} must be byte-identical without a supersedes relation: {}",
            out.stdout
        );
    }
}

/// Falsification 5: two views of one selection never disagree about whether a claim is withdrawn.
/// The row loader is the one place the answer is made, so a computation that lived in the gate alone
/// would leave the others calling REQ-0001 an applying claim.
#[test]
fn every_view_agrees_on_what_is_withdrawn() {
    let s = withdrawn_corpus("agreement");
    s.claim("REQ-0003", "The worker shall log each command.", &[], &[]);

    // The loader.
    let rows = s.rows(&[]);
    assert_eq!(row(&rows, "REQ-0001").applies, Applies::No);

    // render (text and JSON), select, gate, audit and baseline each say the same.
    let (_, rendered) = s.json(&["render"]);
    let applying: Vec<&str> = rendered["applies"]
        .as_array()
        .expect("applies[]")
        .iter()
        .map(|r| r["claim_id"].as_str().expect("id"))
        .collect();
    assert_eq!(applying, ["REQ-0002", "REQ-0003"]);

    let select = s.req(&["select"]);
    assert!(
        !select.stdout.contains("REQ-0001  applies"),
        "{}",
        select.stdout
    );

    let gate = s.req(&["gate"]);
    assert!(
        gate.stdout.contains("# Release gate: 2 applying claim(s)"),
        "{}",
        gate.stdout
    );

    let audit = s.req(&["audit"]);
    assert!(
        audit
            .stdout
            .contains("claims: 2 applying, 0 undecided, 1 excluded"),
        "{}",
        audit.stdout
    );

    let manifest = emit_manifest(&s);
    assert_eq!(manifest_claim_ids(&manifest), ["REQ-0002", "REQ-0003"]);
}

// ---------------------------------------------------------------------------
// decision 3: the Baseline's applying set
// ---------------------------------------------------------------------------

fn emit_manifest(s: &Scratch) -> serde_json::Value {
    // The starter decision log: an empty one refuses to emit, and the log is not what this test is about.
    let decisions = s.path().join("decisions");
    fs::create_dir_all(&decisions).expect("creates decisions/");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("decisions/DEC-0001.yml"),
        decisions.join("DEC-0001.yml"),
    )
    .expect("copies the committed decision record");
    let out = s.req(&["baseline", "--at", "2026-11-12T00:00:00Z"]);
    assert_eq!(out.code, 0, "{}", out.all());
    serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("the manifest is JSON ({e}): {}", out.stdout))
}

fn manifest_claim_ids(manifest: &serde_json::Value) -> Vec<String> {
    manifest["claims"]
        .as_array()
        .expect("claims[]")
        .iter()
        .map(|claim| claim["claim_id"].as_str().expect("an id").to_string())
        .collect()
}

#[test]
fn a_baseline_emitted_over_a_withdrawn_claim_omits_it_from_the_manifest() {
    let s = withdrawn_corpus("baseline");
    let manifest = emit_manifest(&s);
    assert_eq!(manifest_claim_ids(&manifest), ["REQ-0002"], "{manifest}");
}

// ---------------------------------------------------------------------------
// the committed shapes
// ---------------------------------------------------------------------------

/// Falsification 6, on this repository: it declares no `supersedes`, so its committed projections are
/// current byte for byte and need no regeneration.
#[test]
fn this_repositorys_committed_projections_are_current_without_regeneration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (command, file) in [("render", "docs/spec.md"), ("trace", "docs/trace.md")] {
        let out = Command::new(req_bin())
            .args(["--root"])
            .arg(root)
            .args([command, "--check", file])
            .current_dir(root)
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs req");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{command} --check {file}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
