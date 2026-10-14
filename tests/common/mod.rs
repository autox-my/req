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

/// The date every fixture's provenance carries — one home, so the bare literal is not repeated
/// at every call site across the fixture files.
pub const FIXTURE_DATE: &str = "2026-10-07";

/// A Row's relation columns and the digest over the same relations, derived together so the two
/// cannot be edited apart. Takes the tuple form `row()` already receives, so the conversion to
/// the typed relations is written here — once — rather than kept in step at every call site.
pub fn relation_columns(rels: &[(&str, &str)]) -> (Vec<(String, String)>, String) {
    let relations: Vec<Relation> = rels
        .iter()
        .map(|(kind, to)| Relation {
            kind: kind.to_string(),
            to: to.to_string(),
        })
        .collect();
    let digest = relations_digest(&relations);
    let columns = relations.into_iter().map(|r| (r.kind, r.to)).collect();
    (columns, digest)
}

/// The provenance a row displays, with the attested digest over the same rationale. A named
/// struct, not a tuple: all three fields are `String`, so a transposed tuple binding would
/// compile silently and pair the digest with the wrong rationale — the exact drift this helper
/// exists to prevent (`relation_columns`' tuple is safe only because its element types differ).
pub struct Provenance {
    pub source: String,
    pub date: String,
    pub hash: String,
}

/// The rationale-side mirror of `relation_columns`: the displayed provenance and the attested
/// digest over the same rationale, derived together, so pairing the hash to the source and date
/// it covers rests on one definition rather than per-call-site discipline.
pub fn rationale_columns(source: &str, date: &str) -> Provenance {
    let rationale = rationale_of(source, date);
    Provenance {
        hash: rationale_digest(&rationale),
        source: rationale.source,
        date: rationale.date,
    }
}

/// A settled, verified, fully-provenanced claim with the given outgoing relations.
pub fn row(id: &str, applies: Applies, rels: &[(&str, &str)]) -> Row {
    // Real digests over this row's own content: a placeholder hash beside real relations
    // is a Row state `load_rows` can never produce, and a fixture that could not exist
    // proves nothing about the layers above it.
    let (relations, relations_hash) = relation_columns(rels);
    let provenance = rationale_columns(
        "docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md",
        FIXTURE_DATE,
    );
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: format!("{id:0>12}"),
        applies,
        selectors: Vec::new(),
        relations,
        relations_hash,
        rationale_hash: provenance.hash,
        stored: StoredState::Verified,
        rationale_source: provenance.source,
        rationale_date: provenance.date,
    }
}

pub fn stored(mut row: Row, state: StoredState) -> Row {
    row.stored = state;
    row
}

pub fn rationale(mut row: Row, source: &str) -> Row {
    // The provenance and its attested digest are derived together, so a changed source cannot
    // leave the old hash behind — the unproducible row this module exists to prevent.
    let provenance = rationale_columns(source, &row.rationale_date);
    row.rationale_source = provenance.source;
    row.rationale_date = provenance.date;
    row.rationale_hash = provenance.hash;
    row
}
