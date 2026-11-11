//! Structural contradictions and recorded Conflicts (ADR 0043, unit C-1).
//!
//! Public seams only: [`req::contradict::contradicts`] over canonical forms, [`req::contradict::find`]
//! over a selection's rows, and the compiled `req` (the gate's JSON and exit codes, `req check`).
//! Every oracle is the ADR's rule or arithmetic on exact decimals, written down before any run —
//! never a number read back from the detector. The ADR's five *Falsification* items are the
//! headings below.

mod common;

use req::canon::{CanonicalForm, RULES_V, canonicalize_statement};
use req::contradict::{DETECTOR_ADMITTED, Pair, contradicts, find};
use req::model::{Corpus, Registry};
use req::select::Applies;
use req::view::Row;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn form(statement: &str) -> CanonicalForm {
    let (form, _) = canonicalize_statement(statement, &Registry::empty());
    assert!(
        !form.unstructured,
        "setup: `{statement}` must parse structured"
    );
    form
}

fn verdict(a: &str, b: &str) -> Pair {
    contradicts(&form(a), &form(b))
}

fn is_contradiction(pair: &Pair) -> bool {
    matches!(pair, Pair::Contradiction { .. })
}

/// Both argument orders, so a rule that only holds one way round fails.
fn both_ways(a: &str, b: &str) -> (Pair, Pair) {
    (verdict(a, b), verdict(b, a))
}

// ---------------------------------------------------------------------------
// decision 2: a comparable pair contradicts exactly when no value satisfies both
// ---------------------------------------------------------------------------

#[test]
fn an_upper_bound_below_a_lower_bound_contradicts() {
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 100 ms.",
        "The API shall respond in at least 200 ms.",
    );
    assert!(is_contradiction(&ab), "{ab:?}");
    assert!(is_contradiction(&ba), "{ba:?}");
    if let Pair::Contradiction { slot, .. } = ab {
        assert_eq!(slot, "bounds");
    }
}

#[test]
fn strictness_at_the_meeting_point_contradicts_and_closed_ends_do_not() {
    // `≤ 100` and `> 100`: the meeting point is excluded by one side.
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 100 ms.",
        "The API shall respond in more than 100 ms.",
    );
    assert!(is_contradiction(&ab), "{ab:?}");
    assert!(is_contradiction(&ba), "{ba:?}");
    // `≤ 100` and `≥ 100`: exactly 100 satisfies both.
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 100 ms.",
        "The API shall respond in at least 100 ms.",
    );
    assert!(matches!(ab, Pair::Consistent), "{ab:?}");
    assert!(matches!(ba, Pair::Consistent), "{ba:?}");
}

#[test]
fn two_different_exact_values_contradict_and_an_exact_value_must_sit_inside_the_bound() {
    let (ab, ba) = both_ways(
        "The API shall respond in exactly 150 ms.",
        "The API shall respond in exactly 200 ms.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    // `EQ 150` against `≤ 100`: 150 is outside the bound.
    let (ab, ba) = both_ways(
        "The API shall respond in exactly 150 ms.",
        "The API shall respond in no more than 100 ms.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    // `EQ 100` against `≤ 100`: inside it.
    let (ab, ba) = both_ways(
        "The API shall respond in exactly 100 ms.",
        "The API shall respond in no more than 100 ms.",
    );
    assert!(matches!(ab, Pair::Consistent) && matches!(ba, Pair::Consistent));
    // The same value spelled in another unit is the same value (canonical conversion, exact).
    let (ab, _) = both_ways(
        "The API shall respond in exactly 100 ms.",
        "The API shall respond in exactly 0.1 seconds.",
    );
    assert!(matches!(ab, Pair::Consistent), "{ab:?}");
}

#[test]
fn bounds_are_decided_on_exact_decimals_never_floats() {
    // 0.3 s and 300 ms are the same decimal after conversion; as floats 0.1+0.2 would not be 0.3.
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 0.3 seconds.",
        "The API shall respond in at least 300 ms.",
    );
    assert!(matches!(ab, Pair::Consistent), "{ab:?}");
    assert!(matches!(ba, Pair::Consistent), "{ba:?}");
}

#[test]
fn disjoint_membership_sets_contradict_and_overlapping_ones_do_not() {
    let (ab, ba) = both_ways(
        "The API shall use one of {TLS1.2, TLS1.3}.",
        "The API shall use one of {SSL3, TLS1.0}.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    if let Pair::Contradiction { slot, .. } = ab {
        assert_eq!(slot, "membership");
    }
    let (ab, ba) = both_ways(
        "The API shall use one of {TLS1.2, TLS1.3}.",
        "The API shall use one of {TLS1.3, TLS1.0}.",
    );
    assert!(matches!(ab, Pair::Consistent) && matches!(ba, Pair::Consistent));
}

#[test]
fn shall_in_s_against_shall_not_in_t_contradicts_exactly_when_s_is_inside_t() {
    // S = {TLS1.2, TLS1.3} ⊆ T = {TLS1.2, TLS1.3, SSL3}: every allowed value is forbidden.
    let (ab, ba) = both_ways(
        "The API shall use one of {TLS1.2, TLS1.3}.",
        "The API shall not use one of {TLS1.2, TLS1.3, SSL3}.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    // S ⊄ T: TLS1.3 is allowed and not forbidden.
    let (ab, ba) = both_ways(
        "The API shall use one of {TLS1.2, TLS1.3}.",
        "The API shall not use one of {TLS1.2, SSL3}.",
    );
    assert!(matches!(ab, Pair::Consistent) && matches!(ba, Pair::Consistent));
}

#[test]
fn shall_against_shall_not_in_the_same_slot_contradicts() {
    let (ab, ba) = both_ways(
        "The API shall log each command.",
        "The API shall not log each command.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    if let Pair::Contradiction { slot, .. } = ab {
        assert_eq!(slot, "polarity");
    }
    let (ab, ba) = both_ways(
        "The API shall respond within 100 ms.",
        "The API shall not respond within 100 ms.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    // Two prohibitions never exclude each other.
    let (ab, _) = both_ways(
        "The API shall not log each command.",
        "The API shall not log each command.",
    );
    assert!(matches!(ab, Pair::Consistent), "{ab:?}");
}

#[test]
fn a_bare_action_contradicts_only_by_polarity() {
    let (ab, ba) = both_ways(
        "The API shall log each command.",
        "The API shall log each command within 100 ms.",
    );
    // Different slots here (the bound is part of the second claim's value/unit): never a guess.
    assert!(!is_contradiction(&ab) && !is_contradiction(&ba), "{ab:?}");
}

#[test]
fn a_recommendation_or_a_permission_never_contradicts_at_tier_1() {
    for weaker in [
        "The API should respond in at least 200 ms.",
        "The API may respond in at least 200 ms.",
    ] {
        let (ab, ba) = both_ways("The API shall respond in no more than 100 ms.", weaker);
        assert!(
            matches!(ab, Pair::NotComparable(ref reason) if reason.contains("SHOULD and MAY")),
            "{weaker}: {ab:?}"
        );
        assert!(matches!(ba, Pair::NotComparable(_)), "{weaker}: {ba:?}");
    }
}

#[test]
fn a_membership_against_a_bound_is_not_comparable() {
    let (ab, ba) = both_ways(
        "The API shall use one of {TLS1.2, TLS1.3}.",
        "The API shall respond in no more than 100 ms.",
    );
    // Different unit and operator slots: nothing to decide, and nothing guessed.
    assert!(!is_contradiction(&ab) && !is_contradiction(&ba), "{ab:?}");
}

#[test]
fn different_subject_action_qualifier_or_unit_is_not_comparable_and_names_the_slot() {
    let base = "The API shall respond in no more than 100 ms.";
    for (other, slot) in [
        ("The worker shall respond in at least 200 ms.", "subject"),
        ("The API shall acknowledge in at least 200 ms.", "action"),
        ("The API shall respond in at least 200 bytes.", "unit"),
        (
            "The API shall respond in at least 200 ms per request.",
            "qualifier",
        ),
    ] {
        let (ab, ba) = both_ways(base, other);
        for pair in [ab, ba] {
            assert!(
                matches!(&pair, Pair::NotComparable(reason) if reason.contains(slot)),
                "`{other}` differs from `{base}` in the {slot} slot, which the rule compares by equality: {pair:?}"
            );
        }
    }
}

#[test]
fn nested_conditions_are_comparable_and_non_nested_ones_are_not() {
    // `{}` ⊆ `{when the user is idle}`: a situation exists where both bind.
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 100 ms.",
        "When the user is idle, the API shall respond in at least 200 ms.",
    );
    assert!(is_contradiction(&ab) && is_contradiction(&ba), "{ab:?}");
    // Two different conditions, neither inside the other: never guessed.
    let (ab, ba) = both_ways(
        "When the user is idle, the API shall respond in no more than 100 ms.",
        "When the disk is full, the API shall respond in at least 200 ms.",
    );
    assert!(
        matches!(ab, Pair::NotComparable(ref reason) if reason.contains("conditions")),
        "{ab:?}"
    );
    assert!(
        matches!(ba, Pair::NotComparable(ref reason) if reason.contains("conditions")),
        "{ba:?}"
    );
    // Equal conditions are nested (each is a subset of the other).
    let (ab, _) = both_ways(
        "When the user is idle, the API shall respond in no more than 100 ms.",
        "When the user is idle, the API shall respond in at least 200 ms.",
    );
    assert!(is_contradiction(&ab), "{ab:?}");
}

#[test]
fn an_unstructured_claim_is_not_comparable() {
    let (unstructured, _) = canonicalize_statement("Make it feel fast.", &Registry::empty());
    assert!(
        unstructured.unstructured,
        "setup: this statement is the fallback"
    );
    let pair = contradicts(
        &unstructured,
        &form("The API shall respond in no more than 100 ms."),
    );
    assert!(matches!(pair, Pair::NotComparable(_)), "{pair:?}");
}

#[test]
fn the_report_names_the_slot_and_both_values() {
    let Pair::Contradiction { slot, a, b } = verdict(
        "The API shall respond in no more than 100 ms.",
        "The API shall respond in at least 200 ms.",
    ) else {
        panic!("setup: this pair contradicts");
    };
    assert_eq!(slot, "bounds");
    // Canonical values: 100 ms is 0.1 s, 200 ms is 0.2 s; the report prints what the parse holds.
    assert!(a.contains("LE") && a.contains("0.1"), "{a}");
    assert!(b.contains("GE") && b.contains("0.2"), "{b}");
}

// ---------------------------------------------------------------------------
// find: a selection's applying claims, bucketed; recorded Conflicts
// ---------------------------------------------------------------------------

fn forms_of(claims: &[(&str, &str)]) -> BTreeMap<String, CanonicalForm> {
    claims
        .iter()
        .map(|(id, statement)| ((*id).to_string(), form(statement)))
        .collect()
}

/// `(claim id, applies, outgoing relations)` per row.
type RowSpec<'a> = (&'a str, Applies, &'a [(&'a str, &'a str)]);

fn rows_of(spec: &[RowSpec<'_>]) -> Vec<Row> {
    spec.iter()
        .map(|(id, applies, relations)| common::row(id, *applies, relations))
        .collect()
}

const SHORT: &str = "The API shall respond in no more than 100 ms.";
const LONG: &str = "The API shall respond in at least 200 ms.";

#[test]
fn find_reports_each_contradiction_once_sorted_by_the_two_claim_ids() {
    let forms = forms_of(&[
        ("REQ-0003", LONG),
        ("REQ-0001", SHORT),
        ("REQ-0002", "The API shall respond in no more than 50 ms."),
    ]);
    let rows = rows_of(&[
        ("REQ-0003", Applies::Yes, &[]),
        ("REQ-0001", Applies::Yes, &[]),
        ("REQ-0002", Applies::Yes, &[]),
    ]);
    let findings = find(&rows, &forms);
    let pairs: Vec<(&str, &str)> = findings
        .contradictions
        .iter()
        .map(|c| (c.a_id.as_str(), c.b_id.as_str()))
        .collect();
    // 0001 ≤ 100 and 0002 ≤ 50 agree; 0003 ≥ 200 contradicts both.
    assert_eq!(pairs, [("REQ-0001", "REQ-0003"), ("REQ-0002", "REQ-0003")]);
    assert_eq!(findings.comparable, 3, "three claims, three bucket pairs");
    assert_eq!(findings.not_comparable, 0);
}

/// ADR 0043 Falsification 2: two claims that never apply in the same selection are not reported.
#[test]
fn claims_outside_the_selection_never_contradict() {
    let forms = forms_of(&[("REQ-0001", SHORT), ("REQ-0002", LONG)]);
    for other in [Applies::No, Applies::Unknown] {
        let rows = rows_of(&[("REQ-0001", Applies::Yes, &[]), ("REQ-0002", other, &[])]);
        let findings = find(&rows, &forms);
        assert!(
            findings.contradictions.is_empty(),
            "a claim the selection leaves {other:?} is not one of its applying claims"
        );
        assert_eq!(findings.comparable, 0);
    }
}

#[test]
fn pairs_in_a_bucket_that_cannot_be_compared_are_counted_not_guessed() {
    let forms = forms_of(&[
        ("REQ-0001", SHORT),
        ("REQ-0002", "The API should respond in at least 200 ms."),
        (
            "REQ-0003",
            "When the user is idle, the API shall respond in at least 200 ms.",
        ),
        (
            "REQ-0004",
            "When the disk is full, the API shall respond in at least 200 ms.",
        ),
    ]);
    let rows = rows_of(&[
        ("REQ-0001", Applies::Yes, &[]),
        ("REQ-0002", Applies::Yes, &[]),
        ("REQ-0003", Applies::Yes, &[]),
        ("REQ-0004", Applies::Yes, &[]),
    ]);
    let findings = find(&rows, &forms);
    // Six pairs in the one bucket. 0001–0003 and 0001–0004 are comparable (the empty condition
    // set is inside every set) and contradict; 0003–0004 are different, non-nested conditions;
    // the three pairs with 0002 involve a SHOULD.
    assert_eq!(findings.comparable, 2, "{findings:?}");
    assert_eq!(findings.not_comparable, 4, "{findings:?}");
    let ids: Vec<(&str, &str)> = findings
        .contradictions
        .iter()
        .map(|c| (c.a_id.as_str(), c.b_id.as_str()))
        .collect();
    assert_eq!(ids, [("REQ-0001", "REQ-0003"), ("REQ-0001", "REQ-0004")]);
}

#[test]
fn a_recorded_conflict_on_one_side_only_is_found_once_and_on_both_sides_is_still_one() {
    let forms = forms_of(&[("REQ-0001", SHORT), ("REQ-0002", LONG)]);
    let one_side = rows_of(&[
        ("REQ-0001", Applies::Yes, &[("conflicts-with", "REQ-0002")]),
        ("REQ-0002", Applies::Yes, &[]),
    ]);
    let found = find(&one_side, &forms);
    assert_eq!(
        found.conflicts,
        [("REQ-0001".to_string(), "REQ-0002".to_string())]
    );
    assert!(
        found.contradictions[0].recorded,
        "the contradiction is the recorded Conflict"
    );

    let both_sides = rows_of(&[
        ("REQ-0001", Applies::Yes, &[("conflicts-with", "REQ-0002")]),
        ("REQ-0002", Applies::Yes, &[("conflicts-with", "REQ-0001")]),
    ]);
    assert_eq!(find(&both_sides, &forms).conflicts.len(), 1);

    // Recorded on the other claim only: the same unordered pair.
    let other_side = rows_of(&[
        ("REQ-0001", Applies::Yes, &[]),
        ("REQ-0002", Applies::Yes, &[("conflicts-with", "REQ-0001")]),
    ]);
    assert_eq!(
        find(&other_side, &forms).conflicts,
        [("REQ-0001".to_string(), "REQ-0002".to_string())]
    );
}

#[test]
fn a_conflict_is_listed_only_when_both_claims_apply_and_a_dangling_one_is_named() {
    let forms = forms_of(&[("REQ-0001", SHORT), ("REQ-0002", LONG)]);
    let rows = rows_of(&[
        ("REQ-0001", Applies::Yes, &[("conflicts-with", "REQ-0002")]),
        ("REQ-0002", Applies::No, &[]),
    ]);
    assert!(find(&rows, &forms).conflicts.is_empty());

    let rows = rows_of(&[("REQ-0001", Applies::Yes, &[("conflicts-with", "REQ-9999")])]);
    let found = find(&rows, &forms_of(&[("REQ-0001", SHORT)]));
    assert_eq!(
        found.dangling,
        [("REQ-0001".to_string(), "REQ-9999".to_string())]
    );
    assert!(found.conflicts.is_empty());
}

#[test]
fn other_relation_kinds_record_no_conflict() {
    let forms = forms_of(&[("REQ-0001", SHORT), ("REQ-0002", LONG)]);
    let rows = rows_of(&[
        ("REQ-0001", Applies::Yes, &[("refines", "REQ-0002")]),
        ("REQ-0002", Applies::Yes, &[]),
    ]);
    let found = find(&rows, &forms);
    assert!(found.conflicts.is_empty() && found.dangling.is_empty());
    assert!(!found.contradictions[0].recorded);
}

// ---------------------------------------------------------------------------
// the compiled req: the gate policy, `req check`, the audit packet
// ---------------------------------------------------------------------------

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-contradict-{}-{}-{name}",
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

    fn claim(
        &self,
        id: &str,
        statement: &str,
        applicability: Option<&str>,
        relations: &[(&str, &str)],
    ) {
        let applicability = match applicability {
            Some(predicate) => format!("applicability:\n  - {predicate}"),
            None => "applicability: []".to_string(),
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
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-04\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, text).expect("writes the file");
    }

    /// Names every claim in a placeholder signed manifest, so the `completeness` step (ADR 0039)
    /// leaves a `req check` verdict to the policy under test.
    fn sign_every_claim(&self) {
        let mut ids: Vec<String> = fs::read_dir(self.0.join("claims"))
            .expect("lists the claims")
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .strip_suffix(".yml")
                    .map(str::to_string)
            })
            .collect();
        ids.sort();
        let hash = "0".repeat(64);
        let claims: Vec<String> = ids
            .iter()
            .map(|id| {
                format!(
                    "{{\"claim_id\":\"{id}\",\"address\":\"{hash}\",\"relations_hash\":\"{hash}\",\
                     \"rationale_hash\":\"{hash}\",\"tests_hash\":\"{hash}\"}}"
                )
            })
            .collect();
        let manifest = format!(
            "{{\"version\":4,\"signed_at\":\"2026-11-11T00:00:00Z\",\"authority\":{{\"head\":\"{hash}\",\
             \"entries\":1}},\"rules_v\":8,\"selection\":{{}},\"claims\":[{}]}}",
            claims.join(",")
        );
        self.write("baselines/fixture.json", &manifest);
        self.write(
            "baselines/fixture.json.minisig",
            "untrusted comment: fixture signature, never verified by this step\n",
        );
    }

    fn req(&self, args: &[&str]) -> Out {
        let out = Command::new(req_bin())
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("runs req");
        Out {
            code: out.status.code().expect("req exits, never a signal"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn gate_json(&self, extra: &[&str]) -> (Out, serde_json::Value) {
        let mut args = vec!["gate", "--format", "json"];
        args.extend(extra);
        let out = self.req(&args);
        let document = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|e| panic!("gate JSON: {e}\n{}{}", out.stdout, out.stderr));
        (out, document)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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

fn policy<'a>(document: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == name)
        .unwrap_or_else(|| panic!("the document carries a `{name}` policy: {document}"))
}

#[test]
fn the_gate_carries_eight_policies_and_contradictions_sits_before_the_invocation_policy() {
    let s = Scratch::new("order");
    s.claim("REQ-0001", SHORT, None, &[]);
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 0, "{}", out.all());
    let names: Vec<&str> = document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .map(|p| p["policy"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names,
        [
            "pins_hold",
            "relations_resolve",
            "dependencies_decided",
            "rationale_present",
            "tests_linked",
            "design_satisfied",
            "contradictions",
            "selectors_confirmed",
        ]
    );
}

/// ADR 0043 Falsification 4: the PASS line names the count of pairs it could not compare.
#[test]
fn the_pass_line_names_both_counts() {
    let s = Scratch::new("pass-counts");
    s.claim("REQ-0001", SHORT, None, &[]);
    s.claim(
        "REQ-0002",
        "The API shall respond in no more than 50 ms.",
        None,
        &[],
    );
    s.claim(
        "REQ-0003",
        "The API should respond in at least 200 ms.",
        None,
        &[],
    );
    let out = s.req(&["gate"]);
    assert_eq!(out.code, 0, "{}", out.all());
    // One bucket, three pairs: 0001–0002 agree (comparable); the two pairs with the SHOULD are not.
    assert!(
        out.stdout.contains(
            "no structural contradiction among 1 comparable pairs; 2 pairs not comparable"
        ),
        "the PASS line names both counts:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.to_lowercase().contains("consistent"),
        "the policy never says the corpus is consistent:\n{}",
        out.stdout
    );
    let (_, document) = s.gate_json(&[]);
    assert_eq!(policy(&document, "contradictions")["checked"], 1);
    assert_eq!(policy(&document, "contradictions")["outcome"], "pass");
}

#[test]
fn an_unrecorded_contradiction_fails_once_the_detector_is_admitted_and_is_reported_before() {
    let s = Scratch::new("unrecorded");
    s.claim("REQ-0001", SHORT, None, &[]);
    s.claim("REQ-0002", LONG, None, &[]);
    let (out, document) = s.gate_json(&[]);
    let entry = policy(&document, "contradictions");
    let details = entry["details"].to_string();
    // Named in both states: both ids, the slot and the two values.
    for needle in ["REQ-0001", "REQ-0002", "bounds", "LE", "GE"] {
        assert!(details.contains(needle), "`{needle}` is named: {details}");
    }
    if DETECTOR_ADMITTED {
        assert_eq!(out.code, 1, "{}", out.all());
        assert_eq!(entry["outcome"], "fail");
    } else {
        assert_eq!(out.code, 0, "{}", out.all());
        assert_eq!(entry["outcome"], "pass");
        assert!(
            details.contains("reported, not judged"),
            "until admitted the finding is listed and does not change the outcome: {details}"
        );
        assert!(
            out.stdout.is_empty() || !out.stdout.contains("consistent"),
            "{}",
            out.stdout
        );
    }
}

/// ADR 0043 Falsification 3: a recorded Conflict between two applying claims never lets the
/// policy PASS — judged from the first unit on, whatever the detector's admission.
#[test]
fn a_recorded_conflict_between_applying_claims_is_unverifiable_and_never_pass() {
    let s = Scratch::new("recorded");
    s.claim("REQ-0001", SHORT, None, &[("conflicts-with", "REQ-0002")]);
    s.claim("REQ-0002", LONG, None, &[]);
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 3, "{}", out.all());
    let entry = policy(&document, "contradictions");
    assert_eq!(entry["outcome"], "unverifiable");
    let details = entry["details"].to_string();
    assert!(
        details.contains("REQ-0001") && details.contains("REQ-0002"),
        "both ids are named: {details}"
    );
    // The same Conflict also makes the contradiction a *recorded* one: never a FAIL as well.
    let text = s.req(&["gate"]);
    assert_eq!(text.code, 3, "{}", text.all());
    assert!(
        text.stdout.contains("[UNVERIFIABLE] contradictions"),
        "{}",
        text.stdout
    );
}

#[test]
fn a_recorded_conflict_needs_no_detector_to_be_judged() {
    // Two claims the detector cannot compare (different subjects), recorded by a human.
    let s = Scratch::new("recorded-blind");
    s.claim("REQ-0001", SHORT, None, &[("conflicts-with", "REQ-0002")]);
    s.claim(
        "REQ-0002",
        "The worker shall drain the queue before exit.",
        None,
        &[],
    );
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert_eq!(
        policy(&document, "contradictions")["outcome"],
        "unverifiable"
    );
}

#[test]
fn a_conflict_whose_other_claim_is_outside_the_selection_binds_nothing() {
    let s = Scratch::new("recorded-outside");
    s.claim(
        "REQ-0001",
        SHORT,
        Some("deployment_mode == air_gapped"),
        &[("conflicts-with", "REQ-0002")],
    );
    s.claim("REQ-0002", LONG, Some("deployment_mode == connected"), &[]);
    let (out, document) = s.gate_json(&["deployment_mode=air_gapped"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert_eq!(policy(&document, "contradictions")["outcome"], "pass");
}

#[test]
fn a_dangling_conflict_is_unverifiable_and_named() {
    let s = Scratch::new("dangling");
    s.claim("REQ-0001", SHORT, None, &[("conflicts-with", "REQ-9999")]);
    let (out, document) = s.gate_json(&[]);
    assert_eq!(out.code, 3, "{}", out.all());
    let entry = policy(&document, "contradictions");
    assert_eq!(entry["outcome"], "unverifiable");
    assert!(
        entry["details"].to_string().contains("REQ-9999"),
        "the missing claim is named: {entry}"
    );
}

/// Contradiction is a property of a selection (ADR 0043, *Rejected*: the 150% model): two claims
/// that apply in different `.req/variants` lines are never compared with each other.
#[test]
fn claims_in_different_variant_lines_are_not_reported_by_check() {
    let s = Scratch::new("variants");
    s.claim(
        "REQ-0001",
        SHORT,
        Some("deployment_mode == air_gapped"),
        &[],
    );
    s.claim("REQ-0002", LONG, Some("deployment_mode == connected"), &[]);
    s.sign_every_claim();
    s.write(
        ".req/variants",
        "deployment_mode=air_gapped\ndeployment_mode=connected\n",
    );
    for line in ["deployment_mode=air_gapped", "deployment_mode=connected"] {
        let (out, document) = s.gate_json(&[line]);
        assert_eq!(out.code, 0, "{line}: {}", out.all());
        let entry = policy(&document, "contradictions");
        assert_eq!(entry["outcome"], "pass", "{line}: {entry}");
        assert_eq!(
            entry["checked"], 0,
            "{line}: nothing to compare in one line"
        );
    }
    let check = s.req(&["check"]);
    assert_eq!(check.code, 0, "{}", check.all());
    assert!(
        !check.stdout.contains("contradictions"),
        "no step mentions a contradiction:\n{}",
        check.stdout
    );
}

#[test]
fn check_inherits_the_policy_for_a_line_whose_claims_do_contradict() {
    let s = Scratch::new("check-inherits");
    s.claim("REQ-0001", SHORT, None, &[("conflicts-with", "REQ-0002")]);
    s.claim("REQ-0002", LONG, None, &[]);
    s.sign_every_claim();
    let check = s.req(&["check"]);
    assert_eq!(check.code, 3, "{}", check.all());
    assert!(
        check.stdout.contains("contradictions: UNVERIFIABLE"),
        "the step names the policy that was not a pass:\n{}",
        check.stdout
    );
}

#[test]
fn the_audit_packet_lists_contradictions_and_conflicts_per_baseline() {
    let s = Scratch::new("audit-none");
    s.claim("REQ-0001", SHORT, None, &[]);
    s.claim(
        "REQ-0002",
        "The API shall respond in no more than 50 ms.",
        None,
        &[],
    );
    let packet = s.req(&["audit"]);
    assert!(
        packet.stdout.contains("## Contradictions and Conflicts"),
        "{}",
        packet.stdout
    );
    assert!(
        packet
            .stdout
            .contains("none found among 1 comparable pairs, 0 not comparable"),
        "{}",
        packet.stdout
    );

    let s = Scratch::new("audit-some");
    s.claim("REQ-0001", SHORT, None, &[("conflicts-with", "REQ-0002")]);
    s.claim("REQ-0002", LONG, None, &[]);
    let packet = s.req(&["audit"]);
    let section = packet
        .stdout
        .split("## Contradictions and Conflicts")
        .nth(1)
        .unwrap_or_else(|| panic!("the section is present:\n{}", packet.stdout));
    assert!(
        section.contains("REQ-0001") && section.contains("REQ-0002"),
        "{section}"
    );
    assert!(
        section.contains("conflicts-with") || section.contains("Conflict"),
        "{section}"
    );
    if !DETECTOR_ADMITTED {
        assert!(section.contains("not judged"), "{section}");
    }
}

// ---------------------------------------------------------------------------
// Falsification 5: no part of the decision is made by a model
// ---------------------------------------------------------------------------

#[test]
fn neither_the_module_nor_the_gate_path_calls_a_provider() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in ["src/contradict.rs", "src/gate.rs"] {
        let text = fs::read_to_string(root.join(file)).expect("the source is readable");
        let code: String = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "propose",
            "provider",
            "std::process",
            "std::net",
            "reqwest",
            "TcpStream",
        ] {
            assert!(
                !code.contains(forbidden),
                "{file} must decide without a model or a process: found `{forbidden}`"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Falsification 1 and the admission: the bound is measured, never asserted
// ---------------------------------------------------------------------------

const CORPORA: [&str; 8] = [
    "parity",
    "external-v1",
    "external-v2",
    "external-v3",
    "external-v4",
    "direction-v1",
    "direction-v2",
    "direction-v3",
];

const NESTED_LABELS: [&str; 7] = [
    "numeric_loosening",
    "numeric_tightening",
    "membership_narrowing",
    "membership_widening",
    "scope_narrowing",
    "scope_widening",
    "scope_divergence",
];

fn corpus(name: &str) -> Corpus {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    Corpus::load(
        root.join(format!("bench/corpus/{name}.yml"))
            .to_str()
            .expect("a utf-8 path"),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn registry() -> Registry {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    Registry::load(
        root.join("aliases/subjects.yml")
            .to_str()
            .expect("a utf-8 path"),
    )
    .expect("the registry loads")
}

/// The independent re-measurement: the same rule the record states, recomputed here from the
/// corpora's own labels (`equivalent` pairs; the direction labels that name nested sets).
#[test]
fn no_equivalent_pair_and_no_nested_direction_pair_is_reported_as_a_contradiction() {
    let registry = registry();
    let mut examined = 0usize;
    for name in CORPORA {
        let corpus = corpus(name);
        let labelled = corpus
            .equivalent
            .iter()
            .map(|pair| (pair, true))
            .chain(corpus.not_equivalent.iter().map(|pair| (pair, false)));
        for (pair, equivalent) in labelled {
            let nested = pair
                .direction
                .as_deref()
                .is_some_and(|label| NESTED_LABELS.contains(&label));
            if !equivalent && !nested {
                continue;
            }
            let (a, _) = canonicalize_statement(&pair.a, &registry);
            let (b, _) = canonicalize_statement(&pair.b, &registry);
            examined += 1;
            let pair_verdict = contradicts(&a, &b);
            assert!(
                !is_contradiction(&pair_verdict),
                "{name} {}: a pair labelled equivalent (or a nested-set direction pair) is reported \
                 as a contradiction — ADR 0043 Falsification 1: {pair_verdict:?}",
                pair.id
            );
        }
    }
    assert!(
        examined > 500,
        "the measurement has support: {examined} pairs"
    );
}

fn records_with_the_block() -> Vec<(String, serde_json::Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/results");
    let suffix = format!("-rules-v{RULES_V}.json");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("bench/results exists")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(&suffix))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let text = fs::read_to_string(dir.join(&name)).expect("a record is readable");
            let record: serde_json::Value = serde_json::from_str(&text).expect("a record is JSON");
            record
                .get("contradiction")
                .is_some()
                .then_some((name, record))
        })
        .collect()
}

#[test]
fn every_current_corpus_record_carries_the_contradiction_block() {
    let with_block: Vec<String> = records_with_the_block()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for corpus in CORPORA {
        let expected = format!("{corpus}-rules-v{RULES_V}.json");
        assert!(
            with_block.contains(&expected),
            "{expected} carries the `contradiction` block — regenerate it with `req bench`: {with_block:?}"
        );
    }
}

/// The admission is compiled in and pinned to the measurement (the S-2 pattern): a rules bump, or a
/// corpus that breaks the bound, fails here until the constant is changed on purpose. The record is
/// read by this test only — never at runtime.
#[test]
fn the_detector_is_admitted_exactly_when_every_record_reports_zero_false_contradictions() {
    let records = records_with_the_block();
    assert!(
        !records.is_empty(),
        "the measurement for rules v{RULES_V} must exist: a rules bump needs `req bench` re-run \
         before the detector may judge"
    );
    let admitted_by_records = records.iter().all(|(name, record)| {
        record["contradiction"]["false_contradictions"]
            .as_u64()
            .unwrap_or_else(|| panic!("{name} reports false_contradictions as a count"))
            == 0
    });
    assert_eq!(
        DETECTOR_ADMITTED, admitted_by_records,
        "DETECTOR_ADMITTED must equal 'every rules-v{RULES_V} record reports false_contradictions: 0'"
    );
}

#[test]
fn the_record_block_has_the_decision_6_shape() {
    for (name, record) in records_with_the_block() {
        let block = &record["contradiction"];
        for key in [
            "false_contradictions",
            "comparable",
            "not_comparable",
            "pairs",
            "recall",
        ] {
            assert!(
                block.get(key).is_some(),
                "{name}: `{key}` is present: {block}"
            );
        }
        let recall = &block["recall"];
        for key in ["found", "of", "not_comparable"] {
            assert!(
                recall[key].is_u64(),
                "{name}: recall.{key} is a count: {recall}"
            );
        }
        assert_eq!(
            block["pairs"].as_array().map(Vec::len),
            block["false_contradictions"]
                .as_u64()
                .map(|n| usize::try_from(n).expect("small")),
            "{name}: the pairs list names each false contradiction"
        );
    }
}

// ---------------------------------------------------------------------------
// review C-1-review-1
// ---------------------------------------------------------------------------

/// Two prohibitions whose bounds leave disjoint values contradict: forbidding "more than 100 ms"
/// obliges at most 100, forbidding "less than 200 ms" obliges at least 200 (ADR 0043 decision 2: no
/// value satisfies both). Bare prohibitions stay consistent (abstaining satisfies both).
#[test]
fn two_prohibitions_whose_bounds_leave_no_common_value_contradict() {
    let (ab, ba) = both_ways(
        "The API shall not take more than 100 ms to respond.",
        "The API shall not take less than 200 ms to respond.",
    );
    assert!(
        is_contradiction(&ab) && is_contradiction(&ba),
        "{ab:?} / {ba:?}"
    );
    if let Pair::Contradiction { slot, a, b } = &ab {
        assert_eq!(*slot, "bounds");
        assert!(
            a.starts_with("SHALL-NOT") && b.starts_with("SHALL-NOT"),
            "printed as written: {a} / {b}"
        );
    }
    let (ab, ba) = both_ways(
        "The API shall not take more than 200 ms to respond.",
        "The API shall not take less than 100 ms to respond.",
    );
    assert!(
        matches!(ab, Pair::Consistent),
        "100..=200 satisfies both: {ab:?}"
    );
    assert!(matches!(ba, Pair::Consistent), "{ba:?}");
}

/// A `SHALL` bound against a `SHALL NOT` bound of a different operator: the prohibition leaves an
/// obligation, and the two are judged as two `SHALL`s.
#[test]
fn a_shall_bound_against_a_prohibited_bound_is_judged_on_what_the_prohibition_leaves() {
    // ≤ 100 against "not ≤ 150" (> 150): disjoint.
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 100 ms.",
        "The API shall not respond in no more than 150 ms.",
    );
    assert!(
        is_contradiction(&ab) && is_contradiction(&ba),
        "{ab:?} / {ba:?}"
    );
    // ≤ 100 against "not > 100" (≤ 100): the same obligation, consistent.
    let (ab, ba) = both_ways(
        "The API shall respond in no more than 100 ms.",
        "The API shall not respond in more than 100 ms.",
    );
    assert!(matches!(ab, Pair::Consistent), "{ab:?}");
    assert!(matches!(ba, Pair::Consistent), "{ba:?}");
}

/// ADR 0043 decision 2 and *Consequences*: pairs with a different slot, or an unstructured claim,
/// are not comparable and are counted as such — every applying pair is either comparable or counted.
#[test]
fn every_applying_pair_is_counted_comparable_or_not() {
    let forms = forms_of(&[
        ("REQ-0001", SHORT),
        (
            "REQ-0002",
            "The store shall keep each record for at least 30 days.",
        ),
        ("REQ-0003", LONG),
    ]);
    let rows = rows_of(&[
        ("REQ-0001", Applies::Yes, &[]),
        ("REQ-0002", Applies::Yes, &[]),
        ("REQ-0003", Applies::Yes, &[]),
    ]);
    let findings = find(&rows, &forms);
    assert_eq!(findings.comparable, 1, "{findings:?}");
    assert_eq!(
        findings.not_comparable, 2,
        "the two pairs with REQ-0002 have another subject: {findings:?}"
    );
}

/// A FAIL outranks a recorded Conflict, but the Conflict is still named in the entry (spec choice 4:
/// the entry carries the contradictions and the Conflicts).
#[test]
fn a_fail_still_names_the_recorded_conflicts_beside_it() {
    let s = Scratch::new("fail-and-conflict");
    s.claim("REQ-0001", SHORT, None, &[]);
    s.claim("REQ-0002", LONG, None, &[]);
    s.claim(
        "REQ-0003",
        "The store shall keep each record for at least 30 days.",
        None,
        &[("conflicts-with", "REQ-0004")],
    );
    s.claim(
        "REQ-0004",
        "The archive shall delete each record within 7 days.",
        None,
        &[],
    );
    let (out, document) = s.gate_json(&[]);
    let entry = policy(&document, "contradictions");
    if DETECTOR_ADMITTED {
        assert_eq!(out.code, 1, "{}", out.all());
        assert_eq!(entry["outcome"], "fail");
    }
    let details = entry["details"].to_string();
    assert!(
        details.contains("REQ-0003 conflicts-with REQ-0004"),
        "the recorded Conflict is named beside the FAIL: {details}"
    );
    assert!(details.contains("REQ-0001 vs REQ-0002"), "{details}");
}
