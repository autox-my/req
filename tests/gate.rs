//! Integration tests at req's public seams (see AGENTS.md): the canonicalizer's
//! identity rules, and the gate's evaluation.
//!
//! TDD order: this file was written before `bench::evaluate` existed — the
//! compile error was the red state.

use req::bench::evaluate;
use req::canon::{canonical_json, canonicalize_statement};
use req::model::{Corpus, Pair, Registry};

fn pair(id: &str, a: &str, b: &str) -> Pair {
    Pair {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        source: "tests/gate.rs".to_string(),
        note: None,
    }
}

fn corpus(equivalent: Vec<Pair>, not_equivalent: Vec<Pair>) -> Corpus {
    Corpus {
        schema: "req.bench.corpus/0".to_string(),
        version: 1,
        rules: None,
        equivalent,
        not_equivalent,
    }
}

/// Real-world variance: Doorstop writes `**shall**` in Markdown. Emphasis markers
/// are typographic, so stripping them is a meaning-preserving Tier-1 rule — without
/// it the modality is missed entirely and the claim falls to the unstructured
/// fallback (a false split against its own plain-text twin).
#[test]
fn emphasis_markers_do_not_change_identity() {
    let r = Registry::empty();
    let (marked, _) = canonicalize_statement(
        "Doorstop **shall** support the storage of external requirements assets.",
        &r,
    );
    let (plain, _) = canonicalize_statement(
        "Doorstop shall support the storage of external requirements assets.",
        &r,
    );
    assert!(
        !marked.unstructured,
        "the modal must be found despite Markdown emphasis"
    );
    assert_eq!(canonical_json(&marked), canonical_json(&plain));
}

/// The gate on a clean pair set: nothing collapses that should not, nothing splits
/// that should not.
#[test]
fn gate_passes_on_a_clean_pair_set() {
    let ev = evaluate(
        &corpus(
            // shall / MUST are the same modality (T1.6) — must collapse.
            vec![pair(
                "E-001",
                "The system shall log each command.",
                "The system MUST log each command.",
            )],
            // shall / may are different modalities (T1.6) — must not collapse.
            vec![pair(
                "N-001",
                "The system shall log each command.",
                "The system may log each command.",
            )],
        ),
        &Registry::empty(),
    );
    assert!(ev.fmr_hits.is_empty(), "FMR hits: {:?}", ev.fmr_hits);
    assert!(ev.fsr_hits.is_empty(), "FSR hits: {:?}", ev.fsr_hits);
    assert_eq!(ev.verdict, "PASS");
}

/// A not-equivalent pair with an unstructured side cannot merge, so the FMR
/// property holds — but it was NOT structurally verified. It must be counted, not
/// silently ignored (an unverified pair is not a verified pair).
#[test]
fn unstructured_not_equivalent_pairs_are_counted_not_hidden() {
    let ev = evaluate(
        &corpus(
            vec![],
            vec![pair(
                "U-001",
                "The system shall log each command.",
                "Logging each command.",
            )],
        ),
        &Registry::empty(),
    );
    assert_eq!(ev.unverifiable_not_equivalent, 1);
    assert!(ev.fmr_hits.is_empty());
    assert_eq!(ev.verdict, "PASS");
}

/// Mutation-check for the gate itself: a pair labelled not-equivalent that *does*
/// collapse must be reported as an FMR hit and FAIL the gate. If this test can
/// pass with the FMR accounting removed, the gate is inert.
#[test]
fn gate_fails_on_a_degenerate_not_equivalent_pair() {
    let ev = evaluate(
        &corpus(
            vec![],
            vec![pair(
                "M-001",
                "The system shall log each command.",
                "The system shall log each command.",
            )],
        ),
        &Registry::empty(),
    );
    assert!(ev.fmr_hits.contains(&"M-001".to_string()));
    assert_eq!(ev.verdict, "FAIL");
}
