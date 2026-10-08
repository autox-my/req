//! Impact: what depends on a claim, and what that answer could not settle.

mod common;

use common::row;
use req::impact::dependents;
use req::select::Applies;

#[test]
fn dependents_are_found_transitively_and_nearest_first() {
    // A refines B refines C, so C's dependents are B (distance 1) then A (distance 2).
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[("refines", "REQ-C")]),
        row("REQ-C", Applies::Yes, &[]),
    ];
    let (found, findings) = dependents(&rows, "REQ-C");
    let ids: Vec<&str> = found.iter().map(|d| d.claim_id.as_str()).collect();
    assert_eq!(ids, vec!["REQ-B", "REQ-A"], "{found:?}");
    assert_eq!(found[0].distance, 1);
    assert_eq!(found[1].distance, 2);
    assert!(findings.dangling.is_empty());

    // Nothing depends on the root.
    let (nothing, _) = dependents(&rows, "REQ-A");
    assert!(nothing.is_empty());
}

/// A graph we did not author must not be able to hang the traversal.
#[test]
fn a_cycle_terminates_and_visits_each_claim_once() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[("refines", "REQ-A")]),
    ];
    let (found, _) = dependents(&rows, "REQ-A");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].claim_id, "REQ-B");
}

#[test]
fn an_undecided_dependent_is_a_finding_not_a_zero() {
    let rows = vec![
        row("REQ-A", Applies::Unknown, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[]),
    ];
    let (found, findings) = dependents(&rows, "REQ-B");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].applies, Applies::Unknown);
    assert_eq!(findings.unknown_dependents, vec!["REQ-A".to_string()]);
}

#[test]
fn a_missing_target_and_a_dangling_edge_are_both_unverifiable() {
    let rows = vec![row("REQ-A", Applies::Yes, &[("refines", "REQ-GONE")])];

    let (found, findings) = dependents(&rows, "REQ-NOWHERE");
    assert!(found.is_empty());
    assert!(findings.missing_target);

    // A broken edge elsewhere still means the answer may be incomplete: it is reported
    // rather than quietly shortening the list.
    let (_, findings) = dependents(&rows, "REQ-A");
    assert_eq!(findings.dangling.len(), 1, "{findings:?}");
    assert!(findings.dangling[0].contains("REQ-GONE"));
}
