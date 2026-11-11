//! R-1 — "no more than" is an upper bound (rules v9, `docs/specs/R-1-negated-upper-comparators.md`).
//!
//! T1.5's comparator table had `no less than` → GE but not its mirrors, so `more than` (GT) matched inside
//! "no more than" and a loosening was reported as a tightening. Every expectation below is written from T1.5 and
//! from English ("no more than 3" means at most 3), never read back from a run.

use req::canon::{Comp, address_hex, canonical_json, canonicalize_statement};
use req::compare::compare;
use req::model::Registry;

fn canon(text: &str) -> req::canon::CanonicalForm {
    canonicalize_statement(text, &Registry::empty()).0
}

fn address(text: &str) -> String {
    address_hex(&canonical_json(&canon(text)))
}

fn dir(a: &str, b: &str) -> &'static str {
    compare(&canon(a), &canon(b)).as_str()
}

/// The four negated spellings of an upper bound read LE, with the value and unit exactly as for "at most".
#[test]
fn negated_upper_phrases_derive_le() {
    for phrase in [
        "no more than",
        "not more than",
        "no greater than",
        "not greater than",
    ] {
        let f = canon(&format!("The system shall respond in {phrase} 3 seconds."));
        assert!(!f.unstructured, "{phrase}: {f:?}");
        assert_eq!(f.operator, Some(Comp::Le), "{phrase}: {f:?}");
        assert_eq!(
            f.value.map(|d| d.to_string()),
            Some("3".to_string()),
            "{phrase}: {f:?}"
        );
        assert_eq!(f.unit.as_deref(), Some("s"), "{phrase}: {f:?}");
    }
}

/// The bare comparatives are untouched: "more than" / "greater than" stay GT, and the negated lower bound stays GE.
#[test]
fn bare_comparatives_and_the_negated_lower_bound_are_unchanged() {
    for phrase in ["more than", "greater than"] {
        let f = canon(&format!("The system shall store {phrase} 3 records."));
        assert_eq!(f.operator, Some(Comp::Gt), "{phrase}: {f:?}");
    }
    for phrase in ["no less than", "not less than"] {
        let f = canon(&format!("The system shall store {phrase} 3 records."));
        assert_eq!(f.operator, Some(Comp::Ge), "{phrase}: {f:?}");
    }
}

/// A loosening reported as a tightening was the dangerous confusion: "no more than 4 s" permits more than "no more
/// than 3 s".
#[test]
fn raising_a_no_more_than_bound_loosens_and_lowering_it_tightens() {
    assert_eq!(
        dir(
            "The system shall respond in no more than 3 seconds.",
            "The system shall respond in no more than 4 seconds."
        ),
        "numeric_loosening"
    );
    assert_eq!(
        dir(
            "The system shall respond in no more than 4 seconds.",
            "The system shall respond in no more than 3 seconds."
        ),
        "numeric_tightening"
    );
}

/// E: "no more than 3 seconds" and "at most 3 seconds" are one claim (same subject, same action).
#[test]
fn no_more_than_and_at_most_share_an_address() {
    assert_eq!(
        address("The system shall complete the check no more than 3 seconds after the request."),
        address("The system shall complete the check at most 3 seconds after the request.")
    );
}

/// N: an upper bound and the strict lower bound on the same value are different claims.
#[test]
fn no_more_than_and_more_than_differ() {
    assert_ne!(
        address("The system shall complete the check no more than 3 seconds after the request."),
        address("The system shall complete the check more than 3 seconds after the request.")
    );
}

/// Review R-1-review-1 (MEDIUM): `not fewer than` was the last unmirrored negated form — the table
/// had `no fewer than` and `not less than` (GE) but not it, so `fewer than` (LT) matched inside it and a
/// raised floor read as a loosening. Oracle: English — "not fewer than 3" is "at least 3".
#[test]
fn not_fewer_than_is_a_lower_bound() {
    let form = canon("The system shall store not fewer than 3 records.");
    assert_eq!(form.operator, Some(req::canon::Comp::Ge), "{form:?}");
    assert_eq!(
        dir(
            "The system shall store not fewer than 3 records.",
            "The system shall store not fewer than 4 records."
        ),
        "numeric_tightening"
    );
    assert_eq!(
        address("The system shall store not fewer than 3 records."),
        address("The system shall store at least 3 records.")
    );
}
