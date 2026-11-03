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
mod common;

use common::row;
use req::view::{render, trace};

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

/// REQ-0022: a relation is serialized exactly once. The committed corpus is the oracle: each
/// edge is written in its origin claim's file exactly once, and a second home for it (a mirrored
/// reverse key on the target) is refused by the loader rather than accepted.
///
/// The strict bar (would a failure reject the requirement?): a claim file that gained a second
/// home for an edge — a repeated entry, or a mirrored reverse key the loader accepts — is the
/// requirement broken, and each assertion below fails on exactly that.
#[test]
fn a_relation_is_serialized_exactly_once_on_its_origin_claim() {
    use std::collections::BTreeMap;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let claims = req::canon::load_claims(&root).expect("the committed corpus loads");

    // (origin, type, target) -> how many times the corpus writes that edge down.
    let mut written: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for loaded in &claims {
        for relation in &loaded.claim.relations {
            *written
                .entry((
                    loaded.claim.claim_id.clone(),
                    relation.kind.clone(),
                    relation.to.clone(),
                ))
                .or_default() += 1;
        }
    }
    // REQ-0022 is itself the sibling the split made: its edge to REQ-0020 must be in the set,
    // which also keeps this test from passing over an empty relation map.
    assert_eq!(
        written.get(&(
            "REQ-0022".to_string(),
            "refines".to_string(),
            "REQ-0020".to_string()
        )),
        Some(&1),
        "REQ-0022 must refine REQ-0020, written once: {written:?}"
    );
    let repeated: Vec<_> = written.iter().filter(|(_, n)| **n != 1).collect();
    assert!(
        repeated.is_empty(),
        "an edge is written twice: {repeated:?}"
    );

    // A mirrored key on the target side is refused by the strict loader rather than accepted as
    // a second copy of the edge.
    let mirrored = std::fs::read_to_string(root.join("claims/REQ-0022.yml"))
        .expect("REQ-0022 exists")
        .replace(
            "relations:",
            "referenced_by:\n  - type: refines\n    to: REQ-0020\nrelations:",
        );
    let dir = std::env::temp_dir().join(format!("req-0022-mirror-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("claims")).expect("scratch corpus");
    std::fs::write(dir.join("claims/REQ-0022.yml"), mirrored).expect("scratch claim");
    let err = req::canon::load_claims(&dir).expect_err("a reverse-edge key must not load");
    std::fs::remove_dir_all(&dir).ok();
    assert!(err.contains("referenced_by"), "{err}");
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
