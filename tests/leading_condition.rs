//! Issue #18: a leading condition (`When X, Y shall Z`) is a condition, not part of the subject.
//!
//! Through rules v7 a statement that opens with a guard word had the whole clause read into the
//! **subject** (`when a document is sent to a third-party provider, the provider`), so rewording only the
//! trigger moved the subject and `req compare` called the pair `unrelated` (two different requirements)
//! instead of a change to the obligation's condition. A trailing `when` clause was already a condition
//! (T1.8); this makes the leading position agree with it.
//!
//! Expected values come from the rule, not from a run of the parser: the condition is what sits between
//! the guard word and the comma that precedes the subject; the subject is the words between that comma
//! and the modal phrase; the form is the one the same obligation has with its condition last.

use req::canon::{CanonicalForm, address_hex, canonical_json, canonicalize_statement};
use req::compare::compare;
use req::model::Registry;

fn form(text: &str) -> CanonicalForm {
    canonicalize_statement(text, &Registry::empty()).0
}

fn json(text: &str) -> String {
    canonical_json(&form(text))
}

fn conditions(text: &str) -> Vec<String> {
    form(text).conditions.into_iter().collect()
}

fn direction(a: &str, b: &str) -> &'static str {
    compare(&form(a), &form(b)).as_str()
}

const LEADING: &str = "When a document is sent to a third-party provider, the provider shall be recorded in the findings file.";
const TRAILING: &str = "The provider shall be recorded in the findings file when a document is sent to a third-party provider.";

#[test]
fn a_leading_condition_leaves_the_subject_a_noun_phrase() {
    let f = form(LEADING);
    assert!(!f.unstructured);
    assert_eq!(f.subject, "the provider");
    assert_eq!(
        f.conditions.into_iter().collect::<Vec<_>>(),
        ["when: a document is sent to a third-party provider"]
    );
}

#[test]
fn the_leading_and_the_trailing_form_of_one_obligation_are_one_claim() {
    // Same obligation, same words, the condition moved: the form and the address agree.
    assert_eq!(json(LEADING), json(TRAILING));
    assert_eq!(address_hex(&json(LEADING)), address_hex(&json(TRAILING)));
}

#[test]
fn rewording_only_the_trigger_is_a_change_to_the_condition_not_a_new_subject() {
    let reworded = LEADING.replace("is sent", "is transmitted");
    assert_ne!(address_hex(&json(LEADING)), address_hex(&json(&reworded)));
    assert_eq!(form(LEADING).subject, form(&reworded).subject);
    // The obligation stayed; its trigger was replaced: a divergence, never "unrelated".
    assert_eq!(direction(LEADING, &reworded), "scope_divergence");
}

#[test]
fn adding_or_removing_a_leading_condition_is_a_narrowing_or_a_widening() {
    let plain = "The provider shall be recorded in the findings file.";
    assert_eq!(direction(plain, LEADING), "scope_narrowing");
    assert_eq!(direction(LEADING, plain), "scope_widening");
}

#[test]
fn every_guard_word_reads_in_the_leading_position() {
    for (guard, expected) in [
        ("When", "when: the valve is open"),
        ("If", "if: the valve is open"),
        ("While", "while: the valve is open"),
        ("Unless", "unless: the valve is open"),
        ("During", "during: the valve is open"),
        // `where` is a guard only at the start of a sentence: a trailing "where" is a relative
        // pronoun or a place far more often than a condition (see the next test).
        ("Where", "where: the valve is open"),
    ] {
        let text = format!("{guard} the valve is open, the pump shall stop.");
        let f = form(&text);
        assert_eq!(f.subject, "the pump", "{text}");
        assert_eq!(
            f.conditions.into_iter().collect::<Vec<_>>(),
            [expected],
            "{text}"
        );
    }
}

#[test]
fn a_trailing_where_is_still_not_a_guard() {
    // Unchanged from rules v7: nothing about the trailing position moves.
    assert!(conditions("The system shall log the change where it is configured.").is_empty());
}

#[test]
fn commas_inside_the_condition_do_not_end_it() {
    let f = form("When the door, the lock or the alarm fails, the pump shall stop.");
    assert_eq!(f.subject, "the pump");
    assert_eq!(
        f.conditions.into_iter().collect::<Vec<_>>(),
        ["when: the door, the lock or the alarm fails"]
    );
}

#[test]
fn a_leading_and_a_trailing_condition_are_both_conditions() {
    let f = form("When the door is open, the pump shall stop if the tank is empty.");
    assert_eq!(f.subject, "the pump");
    assert_eq!(
        f.conditions.into_iter().collect::<Vec<_>>(),
        ["if: the tank is empty", "when: the door is open"]
    );
}

#[test]
fn a_guard_word_inside_a_leading_condition_stays_inside_it() {
    let f = form("When the valve is open while the tank is idle, the pump shall stop.");
    assert_eq!(f.subject, "the pump");
    assert_eq!(
        f.conditions.into_iter().collect::<Vec<_>>(),
        ["when: the valve is open while the tank is idle"]
    );
}

#[test]
fn without_a_comma_before_the_subject_nothing_is_guessed() {
    // No comma separates the condition from the subject: the statement parses exactly as before.
    for text in [
        "When the valve is open the pump shall stop.",
        "During startup the pump shall stop.",
    ] {
        assert!(conditions(text).is_empty(), "{text}");
    }
}

#[test]
fn an_empty_condition_or_an_empty_subject_is_not_a_split() {
    for text in [
        // The only comma is right after the guard word (the guard is then "when,", not a guard word).
        "When, in practice, the pump shall stop.",
        // A stray comma where the condition should be: nothing between the guard and the comma.
        "When , the pump shall stop.",
        // The only comma is right before the modal: no subject.
        "When the valve is open, shall stop.",
    ] {
        assert!(conditions(text).is_empty(), "{text}");
        // And it parses as rules v7 did: structured, the clause up to the modal as the subject. A split
        // that left an empty slot would make it unstructured instead.
        let f = form(text);
        assert!(!f.unstructured, "{text}");
        assert!(f.subject.starts_with("when"), "{text}: {}", f.subject);
    }
}

#[test]
fn a_statement_that_does_not_start_with_a_guard_word_is_untouched() {
    for text in [
        "The pump shall stop.",
        "The pump, which is old, shall stop.",
        "Whenever the valve is open, the pump shall stop.",
        "Wherever the valve is, the pump shall stop.",
        "Ifrit, the pump, shall stop.",
    ] {
        assert!(conditions(text).is_empty(), "{text}");
    }
}

#[test]
fn a_leading_condition_adds_no_word_that_no_slot_holds() {
    // T1.10: every word lands in a slot, so the claim is structured and nothing was dropped.
    let (f, lints) = canonicalize_statement(LEADING, &Registry::empty());
    assert!(!f.unstructured);
    assert!(
        lints.iter().all(|l| l.code != "unstructured_coverage"),
        "{:?}",
        lints.iter().map(|l| &l.code).collect::<Vec<_>>()
    );
}
