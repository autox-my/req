//! The direction gate: `req bench` scores a corpus's expected change direction.
//!
//! Two properties, and their asymmetry is the point:
//!   * **a dangerous confusion is the hard gate** — a loosening reported as no change, or
//!     as the opposite direction. That is the direction analogue of a false merge: it is
//!     how a loosened requirement gets ratified.
//!   * **accuracy is reported, not gated** — misreading a tightening as a widening (say)
//!     is wrong but it does not silently delete an obligation, exactly as FSR is noise
//!     where FMR is fatal.
//!
//! An unknown `direction:` label fails the gate rather than being ignored: a misspelled
//! label would otherwise switch the whole check off while every number looked fine.

use req::bench::{Verdict, evaluate};
use req::model::{Corpus, Pair, Registry};

fn pair(id: &str, a: &str, b: &str, direction: Option<&str>) -> Pair {
    Pair {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        source: "tests/direction_gate.rs".to_string(),
        note: None,
        direction: direction.map(|d| d.to_string()),
    }
}

fn corpus(not_equivalent: Vec<Pair>) -> Corpus {
    Corpus {
        schema: "req.bench.corpus/0".to_string(),
        version: 1,
        rules: None,
        equivalent: vec![],
        not_equivalent,
    }
}

#[test]
fn a_correct_direction_is_counted_and_not_dangerous() {
    let ev = evaluate(
        &corpus(vec![pair(
            "D-001",
            "The system shall log each command.",
            "The system may log each command.",
            Some("modality_weakening"),
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.direction_checked, 1);
    assert_eq!(ev.direction_correct, 1);
    assert!(ev.dangerous.is_empty());
    assert!(ev.direction_label_errors.is_empty());
    assert_eq!(ev.verdict, Verdict::Pass);
}

#[test]
fn a_loosening_reported_as_no_change_fails_the_gate() {
    // The dangerous pair: identical statements labelled as a weakening. The classifier
    // says "identical", which is exactly how a relaxed obligation gets waved through.
    let ev = evaluate(
        &corpus(vec![pair(
            "D-002",
            "The system shall log each command.",
            "The system shall log each command.",
            Some("modality_weakening"),
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.dangerous.len(), 1, "{:?}", ev.dangerous);
    assert!(ev.dangerous[0].contains("modality_weakening"));
    assert_eq!(ev.verdict, Verdict::Fail);
}

#[test]
fn a_loosening_reported_backwards_fails_the_gate() {
    let ev = evaluate(
        &corpus(vec![pair(
            "D-003",
            "The system shall respond within 200 ms.",
            "The system shall respond within 250 ms.",
            Some("numeric_tightening"), // wrong on purpose: it is a loosening
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.dangerous.len(), 1, "{:?}", ev.dangerous);
    assert_eq!(ev.verdict, Verdict::Fail);
}

#[test]
fn a_harmless_misread_fails_nothing_and_lowers_accuracy() {
    // A tightening labelled as a widening is wrong, but nothing was loosened: reported.
    let ev = evaluate(
        &corpus(vec![pair(
            "D-004",
            "The system shall respond within 200 ms.",
            "The system shall respond within 150 ms.",
            Some("scope_narrowing"),
        )]),
        &Registry::empty(),
    );
    assert!(ev.dangerous.is_empty());
    assert_eq!(ev.direction_checked, 1);
    assert_eq!(ev.direction_correct, 0);
    assert_eq!(ev.direction_mismatches.len(), 1, "named, not just counted");
    assert_eq!(ev.verdict, Verdict::Pass);
}

#[test]
fn an_unknown_label_fails_rather_than_switching_the_check_off() {
    let ev = evaluate(
        &corpus(vec![pair(
            "D-005",
            "The system shall log each command.",
            "The system may log each command.",
            Some("weakening"), // misspelled: the real label is modality_weakening
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.direction_label_errors.len(), 1);
    assert_eq!(ev.direction_checked, 0);
    assert_eq!(ev.verdict, Verdict::Fail);
}

#[test]
fn an_unparseable_side_is_coverage_not_a_correct_answer() {
    let ev = evaluate(
        &corpus(vec![pair(
            "D-006",
            "Authoring shall be friction-free.",
            "Authoring shall be robust.",
            Some("scope_divergence"),
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.direction_unclassifiable, vec!["D-006".to_string()]);
    assert_eq!(ev.direction_checked, 0);
    assert_eq!(ev.direction_correct, 0);
    assert_eq!(ev.verdict, Verdict::Pass, "no merge and no dangerous read");
}

#[test]
fn pairs_without_a_direction_label_are_left_alone() {
    let ev = evaluate(
        &corpus(vec![pair(
            "D-007",
            "The system shall log each command.",
            "The system may log each command.",
            None,
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.direction_checked, 0);
    assert!(ev.direction_label_errors.is_empty());
    assert_eq!(ev.verdict, Verdict::Pass);
}
