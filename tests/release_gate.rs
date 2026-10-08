//! The release gate: three outcomes, and the scoping that makes it useful.

mod common;

use common::{rationale, row, stored};
use req::fill::StoredState;
use req::gate::{Outcome, evaluate, exit_code};
use req::select::Applies;

#[test]
fn a_clean_selection_passes_every_policy() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[]),
    ];
    let results = evaluate(&rows);
    assert_eq!(exit_code(&results), 0, "{results:?}");
    assert!(results.iter().all(|r| r.outcome == Outcome::Pass));
}

/// The variant-scoping property: a claim the selection excludes is not this variant's
/// problem, however broken it is.
#[test]
fn claims_outside_the_selection_do_not_fail_the_gate() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[]),
        stored(
            row("REQ-B", Applies::No, &[]),
            StoredState::Mismatch("tampered digest".to_string()),
        ),
        rationale(row("REQ-C", Applies::No, &[]), ""),
    ];
    let results = evaluate(&rows);
    assert_eq!(exit_code(&results), 0, "{results:?}");
}

#[test]
fn a_hand_edited_digest_fails_and_an_unfilled_claim_does_not_pass() {
    let tampered = vec![stored(
        row("REQ-A", Applies::Yes, &[]),
        StoredState::Mismatch("stored address does not recompute".to_string()),
    )];
    let results = evaluate(&tampered);
    assert_eq!(exit_code(&results), 1);
    assert_eq!(results[0].outcome, Outcome::Fail);
    assert!(results[0].offenders[0].contains("does not recompute"));

    let unfilled = vec![stored(
        row("REQ-A", Applies::Yes, &[]),
        StoredState::Candidate("unfilled".to_string()),
    )];
    let results = evaluate(&unfilled);
    assert_eq!(
        exit_code(&results),
        3,
        "not wrong, but not confirmed either"
    );
    assert_eq!(results[0].outcome, Outcome::Unverifiable);
}

#[test]
fn a_defect_outranks_an_open_question() {
    let rows = vec![
        rationale(row("REQ-A", Applies::Yes, &[]), ""),
        row("REQ-B", Applies::Yes, &[("refines", "REQ-GONE")]),
    ];
    let results = evaluate(&rows);
    assert_eq!(exit_code(&results), 1, "{results:?}");
}

#[test]
fn an_undecided_dependency_blocks_the_release() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Unknown, &[]),
    ];
    let results = evaluate(&rows);
    assert_eq!(exit_code(&results), 3);
    let decided = results
        .iter()
        .find(|r| r.name == "dependencies_decided")
        .expect("the policy exists");
    assert_eq!(decided.outcome, Outcome::Unverifiable);
    assert!(decided.offenders[0].contains("REQ-B"));
}
