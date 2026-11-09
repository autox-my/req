//! The `IN` operator (ADR 0030, issue #17): "one of {A, B, C}" is a **set**, not a quantity.
//!
//! Through rules v6 a membership statement parsed `value "1"` (the word-number rule read "one"
//! as a bound) and kept the members as free text in the qualifier. So reordering the members
//! moved the address (a false split) and dropping one was only `scope_divergence`, never the
//! narrowing it is. Expected values here are written from the spec, not produced by the parser.

use req::canon::{address_hex, canonical_json, canonicalize_statement};
use req::compare::compare;
use req::model::Registry;

fn form(text: &str) -> (req::canon::CanonicalForm, Vec<String>) {
    let (f, lints) = canonicalize_statement(text, &Registry::empty());
    (f, lints.into_iter().map(|l| l.code).collect())
}

fn json(text: &str) -> String {
    canonical_json(&form(text).0)
}

fn direction(a: &str, b: &str) -> &'static str {
    compare(&form(a).0, &form(b).0).as_str()
}

const BASE: &str = "The report shall cite one of the four standards: ISO 9001, \
                    ISO 14001 (environment only), IEC 61508 or IEC 62304.";

#[test]
fn a_membership_statement_derives_the_in_operator_with_a_sorted_set() {
    let j = json(BASE);
    assert!(j.contains("\"operator\":\"IN\""), "operator: {j}");
    assert!(
        j.contains(
            "\"value\":[\"iec 61508\",\"iec 62304\",\"iso 14001 (environment only)\",\"iso 9001\"]"
        ),
        "the members are a sorted list in the value slot: {j}"
    );
    assert!(j.contains("\"action\":\"cite\""), "action: {j}");
    assert!(j.contains("\"unit\":null"), "a set carries no unit: {j}");
}

#[test]
fn one_of_in_a_membership_statement_is_not_a_quantity() {
    let j = json(BASE);
    assert!(
        !j.contains("\"value\":\"1\""),
        "the word-number rule must not read \"one of\" as a bound of 1: {j}"
    );
    assert!(
        !j.contains("of the four standards"),
        "the members are no longer free text in the qualifier: {j}"
    );
}

#[test]
fn reordering_the_members_keeps_the_address() {
    let reordered = "The report shall cite one of the four standards: IEC 62304, \
                     ISO 9001, IEC 61508 or ISO 14001 (environment only).";
    assert_eq!(json(BASE), json(reordered));
    assert_eq!(address_hex(&json(BASE)), address_hex(&json(reordered)));
}

#[test]
fn a_changed_member_moves_the_address() {
    let changed = BASE.replace("ISO 9001", "ISO 9002");
    assert_ne!(address_hex(&json(BASE)), address_hex(&json(&changed)));
}

#[test]
fn the_noun_the_list_is_about_is_identity_but_the_count_and_article_are_not() {
    // "standards" vs "sources" names what is chosen from: swapping it must move the address.
    let sources = BASE.replace("standards", "sources");
    assert_ne!(address_hex(&json(BASE)), address_hex(&json(&sources)));
    // The count is checked against the members and then dropped (the set already says it),
    // and the article is a stopword.
    assert!(
        json(BASE).contains("\"qualifier\":\"standards\""),
        "{}",
        json(BASE)
    );
}

#[test]
fn a_parenthetical_with_a_comma_stays_one_member() {
    let s = "The report shall cite one of the three standards: ISO 9001, \
             ISO 14001 (environment, health), or IEC 61508.";
    let (f, lints) = form(s);
    assert!(!f.unstructured, "lints: {lints:?}");
    assert!(
        json(s).contains("\"iso 14001 (environment, health)\""),
        "{}",
        json(s)
    );
}

#[test]
fn the_other_enumeration_shapes_derive_the_same_set() {
    let braces = "The report shall cite one of {ISO 9001, IEC 61508}.";
    let colon = "The report shall cite one of the following: ISO 9001, IEC 61508.";
    let header_comma = "The report shall cite one of the two standards, ISO 9001 or IEC 61508.";
    let bare = "The report shall cite one of ISO 9001, IEC 61508 or IEC 62304.";
    let want_two = "\"value\":[\"iec 61508\",\"iso 9001\"]";
    for s in [braces, colon, header_comma] {
        assert!(json(s).contains(want_two), "{s}\n{}", json(s));
        assert!(json(s).contains("\"operator\":\"IN\""), "{s}");
    }
    assert!(
        json(bare).contains("\"value\":[\"iec 61508\",\"iec 62304\",\"iso 9001\"]"),
        "{}",
        json(bare)
    );
}

#[test]
fn a_leading_action_before_one_of_stays_in_the_action_slot() {
    let s = "The copilot shall record what it observed for each clause, naming the clause, \
             as one of three observations: `located`, `not_located` or `unassessed`.";
    let j = json(s);
    assert!(
        j.contains("\"action\":\"record what it observed for each clause, naming the clause, as\""),
        "{j}"
    );
    assert!(
        j.contains("\"value\":[\"located\",\"not_located\",\"unassessed\"]"),
        "{j}"
    );
}

#[test]
fn a_trailing_clause_after_the_list_is_kept_as_the_qualifier() {
    let closed = format!("{} and no other standard", BASE.trim_end_matches('.'))
        .replace("IEC 62304", "IEC 62304,");
    let (f, _) = form(&closed);
    assert!(!f.unstructured, "{closed}");
    assert!(
        json(&closed).contains("\"qualifier\":\"standards | and no other standard\""),
        "the header noun and the closing clause are both identity: {}",
        json(&closed)
    );
    assert_ne!(
        address_hex(&json(&closed)),
        address_hex(&json(BASE)),
        "closing the set is a different obligation"
    );
}

#[test]
fn removing_a_member_narrows_and_adding_one_widens() {
    let fewer = "The report shall cite one of the three standards: ISO 9001, \
                 ISO 14001 (environment only) or IEC 61508.";
    let more = "The report shall cite one of the five standards: ISO 9001, \
                ISO 14001 (environment only), IEC 61508, IEC 62304 or IEC 60601.";
    assert_eq!(direction(BASE, fewer), "membership_narrowing");
    assert_eq!(direction(BASE, more), "membership_widening");
    assert_eq!(direction(fewer, BASE), "membership_widening");
}

#[test]
fn a_swapped_member_is_not_called_no_change_or_a_direction() {
    let swapped = BASE.replace("IEC 62304", "IEC 60601");
    assert_eq!(direction(BASE, &swapped), "scope_divergence");
}

#[test]
fn the_same_set_in_another_order_is_identical() {
    let reordered = "The report shall cite one of the four standards: IEC 62304, \
                     ISO 9001, IEC 61508 or ISO 14001 (environment only).";
    assert_eq!(direction(BASE, reordered), "identical");
}

#[test]
fn for_a_prohibition_a_larger_set_forbids_more() {
    let two = "The report shall not cite one of the two standards: ISO 9001 or IEC 61508.";
    let three =
        "The report shall not cite one of the three standards: ISO 9001, IEC 61508 or IEC 62304.";
    assert_eq!(direction(two, three), "membership_narrowing");
    assert_eq!(direction(three, two), "membership_widening");
}

#[test]
fn a_set_against_a_non_set_is_a_divergence() {
    let numeric = "The report shall cite at least 4 standards.";
    assert_eq!(direction(BASE, numeric), "scope_divergence");
    let exact = "The report shall cite exactly 4.";
    assert_eq!(direction(BASE, exact), "scope_divergence");
}

#[test]
fn a_count_that_disagrees_with_the_members_is_refused_not_guessed() {
    let s = "The report shall cite one of the four standards: ISO 9001, IEC 61508 or IEC 62304.";
    let (f, lints) = form(s);
    assert!(f.unstructured, "a wrong count must not be structured");
    assert!(
        lints.contains(&"membership_count_mismatch".to_string()),
        "{lints:?}"
    );
}

#[test]
fn a_last_member_that_carries_a_conjunction_is_a_clause_and_refused() {
    // No second modal here, so only the connective rule can refuse it: "and its revision" is
    // not part of a name, and reading it as one would put a clause in the set.
    let clause = "The report shall cite one of ISO 9001, IEC 61508 or IEC 62304 and its revision.";
    let (f, lints) = form(clause);
    assert!(f.unstructured, "{}", json(clause));
    assert!(
        lints.contains(&"membership_unclear".to_string()),
        "{lints:?}"
    );
}

#[test]
fn a_member_written_twice_is_refused_not_collapsed_to_a_smaller_set() {
    // No stated count, so only the duplicate rule can refuse it: silently deduplicating would
    // turn a typo into a two-member set and hide that a third was meant.
    let dup = "The report shall cite one of ISO 9001, IEC 61508 or ISO 9001.";
    let (f, lints) = form(dup);
    assert!(f.unstructured, "{}", json(dup));
    assert!(
        lints.contains(&"membership_unclear".to_string()),
        "{lints:?}"
    );
}

#[test]
fn a_one_of_with_no_enumeration_is_exactly_what_rules_v6_derived() {
    // The set is defined elsewhere ("the target market countries"); nothing here is a list.
    // These bytes are what v6 produced for the statement, and they must not move: the near-miss
    // corpora hold dozens of statements of this shape.
    let s = "The product shall allow the user to select a chosen language from one of the target market countries.";
    assert_eq!(
        json(s),
        "{\"action\":\"allow the user to select a chosen language from\",\"applicability\":[],\"conditions\":[],\"exception\":null,\"modality\":\"SHALL\",\"operator\":\"none\",\"qualifier\":\"of the target market countries\",\"raw\":\"\",\"subject\":\"the product\",\"temporal\":null,\"unstructured\":false,\"unit\":null,\"value\":\"1\"}"
    );
}

#[test]
fn a_two_item_or_without_a_delimiter_is_not_read_as_a_list() {
    // "countries or regions" is prose, not an enumeration: no comma, colon or bracket.
    let s = "The product shall allow selecting one of the target market countries or regions.";
    assert!(
        json(s).contains("\"operator\":\"none\""),
        "no delimiter, no set: {}",
        json(s)
    );
}

#[test]
fn a_bare_comma_list_with_no_or_is_not_read_as_a_set() {
    // Nothing ends the list: "one of a, b, c" could be followed by prose the parser cannot
    // see the edge of. Without a bracket, a colon, a counted header or an `or`, it stays as
    // rules v6 derived it rather than guessing where the set stops.
    let s = "The report shall cite one of ISO 9001, IEC 61508, IEC 62304.";
    assert!(json(s).contains("\"operator\":\"none\""), "{}", json(s));
    assert!(json(s).contains("\"value\":\"1\""), "{}", json(s));
}
