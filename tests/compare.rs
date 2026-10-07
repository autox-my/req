//! `req compare` — *what kind of change is this?*
//!
//! Direction is a safety classification, not a description. Telling a human "this
//! edit is cosmetic" when it actually weakened an obligation is how a loosened
//! requirement gets ratified, so the classes below are the ones a reviewer must never
//! confuse. Every expectation is built by construction — the mutation applied to a
//! corpus statement — never read off the classifier.

use req::canon::canonicalize_statement;
use req::compare::{Direction, compare};
use req::model::Registry;

fn dir(a: &str, b: &str) -> Direction {
    let r = Registry::empty();
    let (fa, _) = canonicalize_statement(a, &r);
    let (fb, _) = canonicalize_statement(b, &r);
    compare(&fa, &fb)
}

#[test]
fn identical_statements_are_identical() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall log each command."
        ),
        Direction::Identical
    );
}

/// A cosmetic edit (case, whitespace, punctuation, unit spelling) canonicalizes to the
/// same form, so it lands here too — which is the audit answer: the text changed, the
/// meaning did not.
#[test]
fn cosmetic_edits_are_identical_not_a_change() {
    assert_eq!(
        dir(
            "The system shall not exceed 2.5 amps of standby current.",
            "The system shall not exceed 2.50 A of standby current."
        ),
        Direction::Identical
    );
}

#[test]
fn weakening_and_strengthening_are_told_apart() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system may log each command."
        ),
        Direction::ModalityWeakening
    );
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system should log each command."
        ),
        Direction::ModalityWeakening
    );
    assert_eq!(
        dir(
            "The system may log each command.",
            "The system shall log each command."
        ),
        Direction::ModalityStrengthening
    );
}

/// A polarity flip is not a strengthening: forbidding X is a different obligation, not
/// a stricter version of requiring it.
#[test]
fn polarity_flip_is_its_own_class() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall not log each command."
        ),
        Direction::PolarityFlip
    );
}

/// The sign that is easy to get backwards: for an upper bound a *larger* number allows
/// more (loosening); for a lower bound a larger number demands more (tightening).
#[test]
fn numeric_direction_follows_the_operator_polarity() {
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond within 250 ms."
        ),
        Direction::NumericLoosening
    );
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond within 150 ms."
        ),
        Direction::NumericTightening
    );
    assert_eq!(
        dir(
            "The module shall store at least 1,000 records.",
            "The module shall store at least 1200 records."
        ),
        Direction::NumericTightening
    );
    assert_eq!(
        dir(
            "The module shall store at least 1,200 records.",
            "The module shall store at least 1000 records."
        ),
        Direction::NumericLoosening
    );
}

/// LT is stricter than LE, so the LE-vs-LT trap is a *tightening* — and it must never
/// be reported as a loosening.
#[test]
fn fewer_than_is_tighter_than_within() {
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond in fewer than 200 ms."
        ),
        Direction::NumericTightening
    );
}

#[test]
fn conditions_narrow_and_widen_scope() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall log each command when auto-brake is enabled."
        ),
        Direction::ScopeNarrowing
    );
    assert_eq!(
        dir(
            "The system shall log each command when auto-brake is enabled.",
            "The system shall log each command."
        ),
        Direction::ScopeWidening
    );
}

/// The hazard the predicate-span bug taught us: same bound, different *thing* bounded.
#[test]
fn a_changed_qualifier_is_divergence_not_a_no_op() {
    assert_eq!(
        dir(
            "The system shall not exceed 2.5 A of standby current.",
            "The system shall not exceed 2.5 A of surge current."
        ),
        Direction::ScopeDivergence
    );
}

#[test]
fn different_predicates_are_unrelated() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall delete each command."
        ),
        Direction::Unrelated
    );
}

/// One unparseable side means there is nothing to compare structurally. That is its own
/// outcome — it must not be reported as "no change" (the dangerous direction) or forced
/// into a direction class.
#[test]
fn an_unparseable_side_is_unclassifiable_never_identical() {
    let r = Registry::empty();
    let (a, _) = canonicalize_statement("Authoring shall be friction-free.", &r);
    let (b, _) = canonicalize_statement("Authoring shall be robust.", &r);
    assert_eq!(compare(&a, &b), Direction::Unclassifiable);
}

/// This predicate is what the direction corpus will gate on (next slice), so pin the
/// definition of "dangerous" *before* it is used as a gate: a safety-relevant change
/// reported as no-change, or as the opposite direction, is dangerous; a correct answer,
/// an unrelated pair, or a false alarm on an unchanged pair is not.
#[test]
fn the_dangerous_confusion_gate_names_the_dangerous_pairs() {
    use req::compare::Direction::{
        Identical, ModalityStrengthening, ModalityWeakening, NumericLoosening, NumericTightening,
        PolarityFlip, ScopeNarrowing, ScopeWidening, Unrelated,
    };
    // A loosening waved through, or read backwards.
    assert!(Direction::is_dangerous_confusion(
        ModalityWeakening,
        Identical
    ));
    assert!(Direction::is_dangerous_confusion(
        ModalityWeakening,
        ModalityStrengthening
    ));
    assert!(Direction::is_dangerous_confusion(
        NumericLoosening,
        NumericTightening
    ));
    assert!(Direction::is_dangerous_confusion(
        ScopeWidening,
        ScopeNarrowing
    ));
    // A polarity flip waved through as no change.
    assert!(Direction::is_dangerous_confusion(PolarityFlip, Identical));
    // Correct answers are not dangerous.
    assert!(!Direction::is_dangerous_confusion(
        ModalityWeakening,
        ModalityWeakening
    ));
    assert!(!Direction::is_dangerous_confusion(
        NumericTightening,
        NumericTightening
    ));
    // Neither is a pair that claims no movement, nor a false alarm on an unchanged pair.
    assert!(!Direction::is_dangerous_confusion(Unrelated, Identical));
    assert!(!Direction::is_dangerous_confusion(
        Identical,
        NumericLoosening
    ));
    assert!(!Direction::is_dangerous_confusion(
        Direction::Unclassifiable,
        Identical
    ));
}
