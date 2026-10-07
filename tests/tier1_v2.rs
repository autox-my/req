//! Tier-1 v2: the rules that real external data and our own corpus forced.
//!
//! Each test names the evidence that produced it.

use req::canon::{canonical_json, canonicalize_statement};
use req::model::Registry;

fn lints_of(text: &str) -> (bool, Vec<String>) {
    let (form, lints) = canonicalize_statement(text, &Registry::empty());
    (
        form.unstructured,
        lints.into_iter().map(|l| l.code).collect(),
    )
}

/// Evidence: an external corpus pair split because the source statement carried a
/// second obligation ("…shall allow… NOTE: …shall include…") whose wording lives in
/// the unparsed tail. claim-format's authoring rule says "shall … and shall …" is two
/// claims, so Tier-1 must say so rather than hash the first obligation and hide the
/// rest.
#[test]
fn multiple_obligations_are_flagged_not_silently_truncated() {
    let (unstructured, codes) =
        lints_of("The req tool shall emit proposals and shall never rewrite a claim.");
    assert!(unstructured, "a two-obligation statement is not one claim");
    assert!(
        codes.contains(&"multiple_obligations".to_string()),
        "{codes:?}"
    );
}

#[test]
fn a_single_obligation_stays_structured() {
    let (unstructured, codes) = lints_of("The system shall log each command.");
    assert!(!unstructured);
    assert!(!codes.contains(&"multiple_obligations".to_string()));
}

/// Evidence: 16 false-positive lints on external prose, all of the form "IEC 61508"
/// — a standard's name before a number is not a comparator attempt. The lint must
/// fire only for words that could plausibly be comparators.
#[test]
fn standard_names_do_not_trip_the_comparator_lint() {
    let (_unstructured, codes) = lints_of("The tool shall comply with IEC 61508.");
    assert!(
        !codes.contains(&"untestable_comparator".to_string()),
        "{codes:?}"
    );
}

#[test]
fn ambiguous_comparator_words_still_lint() {
    let (_unstructured, codes) = lints_of("The system shall respond under 200 ms.");
    assert!(
        codes.contains(&"untestable_comparator".to_string()),
        "{codes:?}"
    );
}

/// A qualitative predicate cannot be canonicalized into an operator and a value, so
/// it belongs in the unstructured fallback with a lint — that is what *derives*
/// REQ-0007's authored `unstructured: true` instead of merely trusting it.
#[test]
fn qualitative_predicates_are_unstructured_and_linted() {
    let (unstructured, codes) = lints_of("Authoring shall be friction-free.");
    assert!(unstructured);
    assert!(
        codes.contains(&"untestable_qualifier".to_string()),
        "{codes:?}"
    );
}

#[test]
fn measurable_predicates_stay_structured() {
    let (unstructured, codes) = lints_of("The system shall respond within 200 ms.");
    assert!(!unstructured, "{codes:?}");
}

/// CRITICAL, found while implementing the qualitative rule: an unbounded claim's
/// predicate must be part of its identity. Without it, two statements that share a
/// subject and modality collapse no matter what they require — a silent false merge,
/// the one failure this design exists to prevent.
#[test]
fn different_predicates_never_collapse() {
    let (a, _) = canonicalize_statement("The system shall log each command.", &Registry::empty());
    let (b, _) =
        canonicalize_statement("The system shall delete each command.", &Registry::empty());
    assert_ne!(canonical_json(&a), canonical_json(&b));
}

/// "shall never X" is a prohibition, not an obligation to X. Reading it as SHALL
/// would put the wrong modality into the audit artifact.
#[test]
fn never_is_a_prohibition() {
    let (form, _) = canonicalize_statement(
        "The req tool shall never rewrite a claim.",
        &Registry::empty(),
    );
    assert!(!form.unstructured);
    assert_eq!(form.modality_str(), "SHALL-NOT");
}
