//! The audit packet: coverage of a signed Baseline, provenance, and the undecided.

mod common;

use req::audit::{coverage, findings, packet};
use req::baseline::{canonical_bytes, from_json, manifest};
use req::model::Relation;
use req::select::{Applies, Context};
use req::view::Row;

/// The date these fixtures carry when they have provenance at all; a claim with no source is
/// the packet's MISSING-provenance case, and it has no date either.
const FIXTURE_DATE: &str = "2026-10-07";

fn row(id: &str, address: &str, applies: Applies, rationale: &str) -> Row {
    // One idiom with every other row fixture (`common::rationale_columns`): the displayed
    // provenance and its attested digest are derived together, so the two cannot be edited
    // apart. A claim with no source is the packet's MISSING-provenance case, and it has no
    // date either.
    let (rationale_source, rationale_date, rationale_hash) = common::rationale_columns(
        rationale,
        if rationale.is_empty() {
            ""
        } else {
            FIXTURE_DATE
        },
    );
    // One idiom with every other row fixture (`common::relation_columns`): the columns and
    // their digest are derived together, so the empty case is a single expression, not two
    // that must be kept in step.
    let (relations, relations_hash) = common::relation_columns(Vec::new());
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
        relations,
        relations_hash,
        rationale_hash,
        stored: req::fill::StoredState::Verified,
        rationale_source,
        rationale_date,
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

/// ADR 0013 at the audit layer: a claim whose address (meaning) still matches but whose
/// attested audit surface moved — here, a relation added after signing — is drifted, not
/// signed, and fails. `tests/baseline.rs` covers this at the `verify`/`Drift::reasons` level;
/// this is `coverage`'s own merge of `attested_changed` into `drifted` and its exclusion from
/// `signed`, which exist only here.
#[test]
fn an_edited_attested_surface_is_reported_drifted_and_fails() {
    let signed_rows = vec![row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md")];
    let manifest = baseline_for(&signed_rows);

    // Same address — the statement's meaning is untouched — with a relation added, so the
    // attested surface is what moved. A real digest over that relation — the state `load_rows`
    // produces from the edited file, not a placeholder — derived together with the columns by
    // the shared helper, the same idiom the fixtures use.
    let (relations, relations_hash) = common::relation_columns(vec![Relation {
        kind: "refines".to_string(),
        to: "REQ-B".to_string(),
    }]);
    let mut edited = row("REQ-A", "aaa", Applies::Yes, "docs/adr/0001.md");
    edited.relations = relations;
    edited.relations_hash = relations_hash;
    let edited = vec![edited];

    let coverage = coverage(&edited, &manifest);
    assert_eq!(coverage.drifted, vec!["REQ-A".to_string()]);
    assert!(
        coverage.signed.is_empty(),
        "an attested change is not coverage: {coverage:?}"
    );

    let text = packet(&edited, &ctx(), Some(&manifest));
    assert!(text.contains("DRIFTED"), "{text}");
    let (failed, open) = findings(&edited, Some(&manifest), Some(&coverage));
    assert_eq!(failed.len(), 1, "{failed:?}");
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
