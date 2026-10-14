//! A `Row` fixture shared by the view, impact and gate tests.
//!
//! Separate test targets cannot share code except through a module like this one, and
//! three copies of the same fixture would drift apart — which is the failure this whole
//! project is about.

// This module is compiled into *every* test target that declares `mod common;`, so a
// helper one target does not use is not dead code — it is in use by another.
#![allow(dead_code)]

use req::fill::StoredState;
use req::model::{Rationale, Relation, rationale_digest, relations_digest};
use req::select::Applies;
use req::view::Row;

/// The full rationale a row's provenance fields stand for. The attested digest is over all
/// four fields, and these fixtures carry none of the optional ones — this is the single place
/// the fixture's rationale shape is written, so the hash and the displayed provenance cannot
/// drift apart. Public so every row-fixture target shares one shape instead of keeping its own
/// copy: this suite's stated failure mode is fixture copies drifting apart.
pub fn rationale_of(source: &str, date: &str) -> Rationale {
    Rationale {
        source: source.to_string(),
        date: date.to_string(),
        rejected_alternatives: vec![],
        assumptions: vec![],
    }
}

/// A settled, verified, fully-provenanced claim with the given outgoing relations.
pub fn row(id: &str, applies: Applies, rels: &[(&str, &str)]) -> Row {
    // Real digests over this row's own content: a placeholder hash beside real relations
    // is a Row state `load_rows` can never produce, and a fixture that could not exist
    // proves nothing about the layers above it.
    let relations: Vec<Relation> = rels
        .iter()
        .map(|(kind, to)| Relation {
            kind: kind.to_string(),
            to: to.to_string(),
        })
        .collect();
    // Bound once and read for both the digest and the displayed provenance, so the two cannot
    // be edited apart.
    let provenance = rationale_of(
        "docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md",
        "2026-10-07",
    );
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: format!("{id:0>12}"),
        applies,
        selectors: Vec::new(),
        relations: relations
            .iter()
            .map(|r| (r.kind.clone(), r.to.clone()))
            .collect(),
        relations_hash: relations_digest(&relations),
        rationale_hash: rationale_digest(&provenance),
        stored: StoredState::Verified,
        rationale_source: provenance.source.clone(),
        rationale_date: provenance.date.clone(),
    }
}

pub fn stored(mut row: Row, state: StoredState) -> Row {
    row.stored = state;
    row
}

pub fn rationale(mut row: Row, source: &str) -> Row {
    row.rationale_source = source.to_string();
    // Re-derive the attested hash from the provenance the row now displays: a changed
    // source with the old hash would recreate the unproducible row this module exists
    // to avoid.
    row.rationale_hash = rationale_digest(&rationale_of(source, &row.rationale_date));
    row
}
