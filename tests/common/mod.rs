//! A `Row` fixture shared by the view, impact and gate tests.
//!
//! Separate test targets cannot share code except through a module like this one, and
//! three copies of the same fixture would drift apart — which is the failure this whole
//! project is about.

// This module is compiled into *every* test target that declares `mod common;`, so a
// helper one target does not use is not dead code — it is in use by another.
#![allow(dead_code)]

use req::fill::StoredState;
use req::select::Applies;
use req::view::Row;

/// A settled, verified, fully-provenanced claim with the given outgoing relations.
pub fn row(id: &str, applies: Applies, rels: &[(&str, &str)]) -> Row {
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: format!("{id:0>12}"),
        applies,
        selectors: Vec::new(),
        relations: rels
            .iter()
            .map(|(k, t)| (k.to_string(), t.to_string()))
            .collect(),
        stored: StoredState::Verified,
        rationale_source: "docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md"
            .to_string(),
        rationale_date: "2026-10-07".to_string(),
    }
}

pub fn stored(mut row: Row, state: StoredState) -> Row {
    row.stored = state;
    row
}

pub fn rationale(mut row: Row, source: &str) -> Row {
    row.rationale_source = source.to_string();
    row
}
