//! Selectors: does a claim apply in a variant? (ADR 0003, REQ-0016)
//!
//! The property that matters is the third state. "We cannot tell" must never become
//! "does not apply": that would drop a safety claim out of a variant without anyone
//! noticing — the failure ADR 0003 exists to prevent, and what REQ-0016 forbids.

use req::model::ClaimFile;
use req::select::{Applies, Context, applies, parse_predicate};

fn ctx(pairs: &[(&str, &str)]) -> Context {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_decided_predicate_resolves_both_ways() {
    let c = ctx(&[("deployment_mode", "air_gapped")]);
    assert_eq!(
        applies(&[parse_predicate("deployment_mode == air_gapped")], &c),
        Applies::Yes
    );
    assert_eq!(
        applies(&[parse_predicate("deployment_mode == connected")], &c),
        Applies::No
    );
}

/// REQ-0016: the applicability of an *undecided* feature resolves UNKNOWN, never FALSE.
#[test]
fn an_undecided_key_is_unknown_never_false() {
    let c = ctx(&[("deployment_mode", "air_gapped")]);
    assert_eq!(
        applies(&[parse_predicate("build_target == embedded")], &c),
        Applies::Unknown
    );
}

#[test]
fn numeric_thresholds_compare_exactly() {
    assert_eq!(
        applies(&[parse_predicate("runs >= 20")], &ctx(&[("runs", "20")])),
        Applies::Yes
    );
    assert_eq!(
        applies(&[parse_predicate("runs >= 21")], &ctx(&[("runs", "20")])),
        Applies::No
    );
    // Not a number on either side: never guessed, and never FALSE.
    assert_eq!(
        applies(&[parse_predicate("runs >= many")], &ctx(&[("runs", "20")])),
        Applies::Unknown
    );
    assert_eq!(
        applies(
            &[parse_predicate("runs >= 20")],
            &ctx(&[("runs", "twenty")])
        ),
        Applies::Unknown
    );
}

#[test]
fn a_predicate_the_grammar_does_not_cover_is_unknown() {
    let c = ctx(&[("deployment_mode", "air_gapped")]);
    assert_eq!(
        applies(&[parse_predicate("applies when it is cold")], &c),
        Applies::Unknown
    );
    assert_eq!(applies(&[parse_predicate("")], &c), Applies::Unknown);
    assert_eq!(
        applies(&[parse_predicate("== no key")], &c),
        Applies::Unknown
    );
}

/// A claim with no selector applies everywhere: the 150% model needs no annotation until
/// a variant actually differs.
#[test]
fn no_selector_applies_everywhere() {
    assert_eq!(applies(&[], &Context::new()), Applies::Yes);
}

/// A decisive No outranks an Unknown — the claim is excluded whatever else is undecided —
/// while an Unknown alongside a Yes leaves the claim undecided.
#[test]
fn a_no_is_decisive_and_an_unknown_is_not() {
    let c = ctx(&[("deployment_mode", "connected")]);
    assert_eq!(
        applies(
            &[
                parse_predicate("deployment_mode == air_gapped"), // No
                parse_predicate("build_target == embedded"),      // Unknown
            ],
            &c
        ),
        Applies::No
    );
    assert_eq!(
        applies(
            &[
                parse_predicate("deployment_mode == connected"), // Yes
                parse_predicate("build_target == embedded"),     // Unknown
            ],
            &c
        ),
        Applies::Unknown
    );
}

/// The wiring from the claim format: `applicability` is the selector slot, and it is part
/// of identity since rules v5.
#[test]
fn a_claims_applicability_is_its_selector() {
    let claim: ClaimFile = yaml_serde::from_str(
        "schema: req.claim/1\nclaim_id: REQ-X\nstatement: The system shall log each command.\n\
         modality: SHALL\nsubject: SYSTEM\nunstructured: false\ncanonical:\n  rules_v: null\n  \
         subject_registry: 1\n  form: null\naddress: null\nrationale:\n  source: x\n  \
         date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\napplicability:\n  - deployment_mode == air_gapped\nrelations: []\n",
    )
    .expect("claim parses");
    let predicates: Vec<_> = claim
        .applicability
        .iter()
        .map(|p| parse_predicate(p))
        .collect();
    assert_eq!(
        applies(&predicates, &ctx(&[("deployment_mode", "air_gapped")])),
        Applies::Yes
    );
    assert_eq!(
        applies(&predicates, &ctx(&[("deployment_mode", "connected")])),
        Applies::No
    );
    assert_eq!(applies(&predicates, &ctx(&[])), Applies::Unknown);
}
