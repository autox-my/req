//! Qualitative predicates: the words that keep a claim out of the structured form.
//!
//! Rules v5 widened the list to inflections and hyphenated heads, because a claim does
//! not become measurable by appending "-ly" or "-to-use". The failure directions are
//! asymmetric in our favour: a false hit costs a split (noise), a miss costs nothing
//! worse than before.

use req::canon::{canonicalize_statement, evaluate_claim};
use req::model::Registry;

fn unstructured(text: &str) -> bool {
    canonicalize_statement(text, &Registry::empty())
        .0
        .unstructured
}

#[test]
fn inflections_of_a_qualitative_word_are_still_qualitative() {
    assert!(unstructured("The system shall respond easily."));
    assert!(unstructured("The system shall respond quickly."));
    assert!(unstructured("The system shall behave robustly."));
    assert!(unstructured("The system shall be seamless."));
}

#[test]
fn a_hyphenated_compound_is_read_by_its_head() {
    assert!(unstructured("The system shall be easy-to-use."));
    assert!(unstructured("The system shall be friction-free."));
}

#[test]
fn a_measurable_predicate_is_not_swept_up() {
    // The list is curated and narrow: "-ly" alone is not evidence, and a bound keeps a
    // claim structured whatever else it says.
    assert!(!unstructured("The system shall respond within 200 ms."));
    // "exactly once" is not *qualitative* — no untestable_qualifier lint — but since rules v6
    // (T1.10) it is not structured either: `once` is a word no slot holds (it is not a number
    // word), and "exactly once" vs "exactly twice" must not share an address. The reason is the
    // coverage lint, not the qualitative one.
    let (form, lints) = canonicalize_statement(
        "The system shall report the count exactly once.",
        &Registry::empty(),
    );
    assert!(
        !lints.iter().any(|l| l.code == "untestable_qualifier"),
        "{lints:?}"
    );
    assert!(form.unstructured);
    assert!(
        lints
            .iter()
            .any(|l| l.code == "uncovered_content" && l.detail == "once"),
        "{lints:?}"
    );
}

#[test]
fn the_lint_names_the_word_it_rejected() {
    let (form, lints) =
        canonicalize_statement("The system shall be easy-to-use.", &Registry::empty());
    assert!(form.unstructured);
    let qualifier = lints
        .iter()
        .find(|l| l.code == "untestable_qualifier")
        .expect("a lint names the reason");
    assert!(
        qualifier.detail.contains("easy"),
        "detail should name the word: {}",
        qualifier.detail
    );
    // And a claim whose authored slots disagree with that classification is refused a
    // fill, not silently filled.
    let v = evaluate_claim(
        &yaml_serde::from_str(
            "schema: req.claim/1\nclaim_id: REQ-Q\nstatement: The system shall be easy-to-use.\nmodality: SHALL\nsubject: SYSTEM\napplicability: []\nunstructured: false\ncanonical:\n  rules_v: null\n  subject_registry: 1\n  form: null\naddress: null\nrationale:\n  source: x\n  date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\nrelations: []\n",
        )
        .expect("claim parses"),
        &Registry::empty(),
    );
    assert!(
        v.flags() >= 1,
        "authored `unstructured: false` disagrees with the rules"
    );
}
