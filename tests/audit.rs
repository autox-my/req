//! The audit packet: coverage of a signed Baseline, provenance, and the undecided.

use req::audit::{coverage, findings, packet};
use req::baseline::{canonical_bytes, from_json, manifest};
use req::model::{Rationale, rationale_digest, relations_digest};
use req::select::{Applies, Context};
use req::view::Row;

/// The full rationale a row's provenance fields stand for: the attested digest is over all
/// four fields, so the hash and the displayed provenance are computed from one shape.
fn fixture_rationale(source: &str) -> Rationale {
    Rationale {
        source: source.to_string(),
        date: if source.is_empty() {
            String::new()
        } else {
            "2026-10-07".to_string()
        },
        rejected_alternatives: vec![],
        assumptions: vec![],
    }
}

fn row(id: &str, address: &str, applies: Applies, rationale: &str) -> Row {
    let provenance = fixture_rationale(rationale);
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: address.to_string(),
        applies,
        selectors: if applies == Applies::Unknown {
            vec!["deployment_mode == air_gapped".to_string()]
        } else {
            Vec::new()
        },
        relations: Vec::new(),
        // Real digests over this row's own content — a placeholder hash beside empty
        // relations is a Row state `load_rows` can never produce.
        relations_hash: relations_digest(&[]),
        rationale_hash: rationale_digest(&provenance),
        stored: req::fill::StoredState::Verified,
        rationale_source: provenance.source.clone(),
        rationale_date: provenance.date.clone(),
    }
}

fn ctx() -> Context {
    let mut context = Context::new();
    context.insert("deployment_mode".to_string(), "air_gapped".to_string());
    context
}

fn baseline_for(rows: &[Row]) -> req::baseline::Manifest {
    manifest(rows, 5, &ctx())
}

/// With no Baseline the packet still prints — but it says nothing is ratified, and the verdict
/// is UNVERIFIABLE. An audit that passed on an unsigned model would be worse than no audit.
#[test]
fn without_a_baseline_nothing_is_ratified_and_the_verdict_is_unverifiable() {
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md"),
        row("REQ-B", "bbb", Applies::Unknown, "docs/adr/0001.md"),
    ];
    let text = packet(&rows, &ctx(), None);
    assert!(text.contains("none supplied"), "{text}");
    assert!(text.contains("unratified"), "{text}");
    assert!(
        text.contains("manifest of intent, not an audit of authority"),
        "{text}"
    );
    // The undecided claim is present, in its own section.
    assert!(text.contains("## Undecided"), "{text}");
    assert!(text.contains("REQ-B"), "{text}");
    let (failed, open) = findings(&rows, None, None);
    assert!(failed.is_empty());
    assert_eq!(open.len(), 1);
}

#[test]
fn a_fully_covered_selection_reports_every_claim_covered() {
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md"),
        row("REQ-B", "bbb", Applies::Yes, "docs/adr/0007.md"),
        row("REQ-C", "ccc", Applies::No, "docs/adr/0007.md"),
    ];
    let manifest = baseline_for(&rows);
    let coverage = coverage(&rows, &manifest);
    assert_eq!(
        coverage.signed,
        vec!["REQ-A".to_string(), "REQ-B".to_string()]
    );
    assert!(coverage.uncovered.is_empty() && coverage.drifted.is_empty());

    let text = packet(&rows, &ctx(), Some(&manifest));
    assert!(!text.contains("unratified"), "{text}");
    assert!(text.contains("[covered]"), "{text}");
    // The excluded claim is not audited as if it were in the selection.
    assert!(!text.contains("REQ-C"), "{text}");
    let (failed, open) = findings(&rows, Some(&manifest), Some(&coverage));
    assert!(failed.is_empty() && open.is_empty(), "{failed:?} {open:?}");
}

/// The property the packet exists for: a claim edited after signing is reported as drifted, not
/// as signed — and the same answer `req baseline --verify` gives, because it derives from it.
#[test]
fn an_edited_claim_is_reported_drifted_and_fails() {
    let signed_rows = vec![row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md")];
    let manifest = baseline_for(&signed_rows);
    let edited = vec![row(
        "REQ-A",
        "aaa-changed",
        Applies::Yes,
        "docs/adr/0001.md",
    )];
    let coverage = coverage(&edited, &manifest);
    assert_eq!(coverage.drifted, vec!["REQ-A".to_string()]);
    assert!(coverage.signed.is_empty());

    let text = packet(&edited, &ctx(), Some(&manifest));
    assert!(text.contains("DRIFTED"), "{text}");
    let (failed, open) = findings(&edited, Some(&manifest), Some(&coverage));
    assert_eq!(failed.len(), 1);
    assert!(open.is_empty(), "{open:?}");
}

#[test]
fn a_claim_added_after_signing_is_uncovered_and_unverifiable() {
    let signed_rows = vec![row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md")];
    let manifest = baseline_for(&signed_rows);
    let mut rows = signed_rows;
    rows.push(row("REQ-D", "ddd", Applies::Yes, "docs/adr/0001.md"));
    let coverage = coverage(&rows, &manifest);
    assert_eq!(coverage.uncovered, vec!["REQ-D".to_string()]);
    let (failed, open) = findings(&rows, Some(&manifest), Some(&coverage));
    assert!(failed.is_empty());
    assert_eq!(open.len(), 1);
    assert!(open[0].contains("REQ-D"));
}

/// Provenance is the packet's whole point: a claim with no source cannot be cited.
#[test]
fn a_claim_without_provenance_fails_the_audit() {
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes, ""),
        row("REQ-B", "bbb", Applies::Yes, "docs/adr/0001.md"),
    ];
    let manifest = baseline_for(&rows);
    let coverage = coverage(&rows, &manifest);
    let text = packet(&rows, &ctx(), Some(&manifest));
    assert!(text.contains("MISSING"), "{text}");
    let (failed, _) = findings(&rows, Some(&manifest), Some(&coverage));
    assert_eq!(failed.len(), 1);
    assert!(failed[0].contains("REQ-A"));
}

/// An audit document is read by someone else: the same tree must produce the same packet.
#[test]
fn the_packet_is_deterministic_and_in_id_order() {
    let rows = vec![
        row("REQ-B", "bbb", Applies::Yes, "docs/adr/0001.md"),
        row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md"),
    ];
    let manifest = baseline_for(&rows);
    let first = packet(&rows, &ctx(), Some(&manifest));
    let reordered = packet(&[rows[1].clone(), rows[0].clone()], &ctx(), Some(&manifest));
    assert_eq!(first, reordered);
    let a = first.find("REQ-A").expect("present");
    let b = first.find("REQ-B").expect("present");
    assert!(a < b, "claims appear in id order:\n{first}");
}

/// The packet and the manifest must agree on the bytes they cover: a manifest re-parsed from
/// its own canonical form yields the same coverage, so signatures cover what the audit reads.
#[test]
fn coverage_survives_the_manifest_round_trip() {
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md"),
        row("REQ-B", "bbb", Applies::Yes, "docs/adr/0007.md"),
    ];
    let manifest = baseline_for(&rows);
    let reparsed = from_json(&canonical_bytes(&manifest)).expect("round trip");
    assert_eq!(coverage(&rows, &reparsed), coverage(&rows, &manifest));
    assert_eq!(coverage(&rows, &reparsed).signed.len(), 2);
}

/// The audit takes its selection from the Baseline too: a Baseline defines the variant, so
/// auditing it under an empty context would report the variant's own claims as "no longer
/// applies" — a mistake in the invocation dressed up as a defect in the requirements.
#[test]
fn audit_takes_its_selection_from_the_baseline() {
    let rows = vec![row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md")];
    let manifest = baseline_for(&rows);
    let resolved = req::baseline::resolve_selection(&Context::new(), &manifest).expect("resolves");
    assert_eq!(resolved, manifest.selection);
    // Under the Baseline's own selection the coverage is complete; the claim is not "missing".
    let coverage = coverage(&rows, &manifest);
    assert!(coverage.missing.is_empty(), "{coverage:?}");
    assert_eq!(coverage.signed.len(), 1);
}
