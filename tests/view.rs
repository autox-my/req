//! Views: a selection compiled into something a person reads (design step 9).
//!
//! Three properties have to survive into the view, and each is a rule the rest of req
//! already committed to:
//!   * an **undecided** claim appears, in its own section — never silently dropped
//!     (ADR 0003, REQ-0016, now at the view layer);
//!   * **incoming relations are derived**, because REQ-0020 stores each relation exactly
//!     once, on the outgoing side, and a matrix that cannot reconstruct the reverse edge
//!     is not a matrix;
//!   * every edge that **leaves the selection** is named — a trace that quietly loses the
//!     parent of an applicable claim is worse than no trace at all.

use req::select::{Applies, Context};
use req::view::{Row, render, trace};

fn row(id: &str, applies: Applies, rels: &[(&str, &str)]) -> Row {
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: format!("{id:0<64}").replace(id, &format!("{id:0>12}")),
        applies,
        selectors: vec![],
        relations: rels
            .iter()
            .map(|(k, t)| (k.to_string(), t.to_string()))
            .collect(),
    }
}

fn ctx() -> Context {
    Context::new()
}

#[test]
fn the_view_keeps_undecided_claims_in_their_own_section() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[]),
        row("REQ-B", Applies::Unknown, &[]),
        row("REQ-C", Applies::No, &[]),
    ];
    let out = render(&rows, &ctx());
    assert!(out.contains("## Applies (1)"), "{out}");
    assert!(out.contains("## Undecided (1)"), "{out}");
    assert!(
        out.contains("never reported as excluded"),
        "the view must say what unknown means: {out}"
    );
    // The undecided claim is named in the undecided section...
    let undecided = out.split("## Undecided").nth(1).expect("a section");
    assert!(undecided.contains("REQ-B"), "{out}");
    // ...and the excluded one is only counted, never listed as if it applied.
    assert!(out.contains("## Outside the selection (1)"), "{out}");
}

#[test]
fn the_view_is_deterministic_whatever_order_it_is_handed() {
    let forward = vec![
        row("REQ-A", Applies::Yes, &[]),
        row("REQ-B", Applies::Yes, &[]),
    ];
    let backward = vec![
        row("REQ-B", Applies::Yes, &[]),
        row("REQ-A", Applies::Yes, &[]),
    ];
    assert_eq!(render(&forward, &ctx()), render(&backward, &ctx()));
}

/// REQ-0020: relations are stored once, on the outgoing side. The reverse edge is the
/// view's job.
#[test]
fn incoming_relations_are_derived_from_the_outgoing_side() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[]),
    ];
    let (out, findings) = trace(&rows, &ctx());
    assert!(
        out.contains("REQ-B <- refines: REQ-A"),
        "the incoming edge must be reconstructed: {out}"
    );
    assert!(findings.dangling.is_empty());
    assert!(findings.undecided_edges.is_empty());
}

#[test]
fn an_edge_that_leaves_the_selection_is_named() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("derives-from", "REQ-B")]),
        row("REQ-B", Applies::Unknown, &[]),
    ];
    let (out, findings) = trace(&rows, &ctx());
    assert!(
        out.contains("REQ-A -derives-from-> REQ-B"),
        "the edge must be shown: {out}"
    );
    assert_eq!(findings.undecided_edges.len(), 1, "{findings:?}");
    assert!(findings.undecided_edges[0].contains("REQ-B"));
    // An edge into a claim the variant excludes is legitimate — reported, not a finding.
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::No, &[]),
    ];
    let (out, findings) = trace(&rows, &ctx());
    assert!(findings.undecided_edges.is_empty());
    assert!(out.contains("outside the selection"), "{out}");
}

/// A relation pointing at a claim that is not in the corpus is UNVERIFIABLE: the matrix
/// cannot be completed, and saying nothing about it would be the worst answer.
#[test]
fn a_dangling_relation_is_unverifiable_not_ignored() {
    let rows = vec![row("REQ-A", Applies::Yes, &[("refines", "REQ-GONE")])];
    let (out, findings) = trace(&rows, &ctx());
    assert_eq!(findings.dangling.len(), 1, "{findings:?}");
    assert!(findings.dangling[0].contains("REQ-GONE"));
    assert!(out.contains("UNVERIFIABLE"), "{out}");
}
