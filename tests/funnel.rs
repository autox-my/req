//! The Tier-2 funnel: blocking recall, its cost, and the provider seam.
//!
//! The recall property is measured against the corpus's own labels — the same independent
//! oracle the identity gates use, and the only reason the number means anything.

use req::funnel::{
    Adjudication, Candidate, MIN_SHARED_TERMS, NoProvider, Provider, blocking_misses,
    candidate_pairs, content_terms, naive_pairs, propose, shared_terms,
};
use req::model::Corpus;
use req::select::Applies;
use req::view::Row;

fn row(id: &str, statement: &str, address: &str) -> Row {
    Row {
        claim_id: id.to_string(),
        statement: statement.to_string(),
        modality: "SHALL".to_string(),
        address: address.to_string(),
        applies: Applies::Yes,
        selectors: Vec::new(),
        relations: Vec::new(),
        stored: req::fill::StoredState::Verified,
        rationale_source: "docs/adr/0004-model-proposes-human-disposes.md".to_string(),
        rationale_date: "2026-10-07".to_string(),
    }
}

fn parity() -> Corpus {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Corpus::load(root.join("bench/corpus/parity.yml").to_str().unwrap()).expect("corpus loads")
}

/// **The safety property.** A pair whose sides share too few terms is never proposed, which is
/// a duplicate silently dropped — so every equivalent pair the corpus knows about must survive
/// blocking. Measured on the labels, not on the implementation.
#[test]
fn blocking_proposes_every_equivalent_pair_in_the_corpus() {
    let misses = blocking_misses(&parity(), MIN_SHARED_TERMS);
    assert!(
        misses.is_empty(),
        "blocking would never propose these labelled-equivalent pairs: {misses:?}"
    );
}

/// The three pairs Tier-1 deliberately cannot collapse are paraphrases and a passive voice —
/// exactly what blocking must still reach, including the side that has no structured form.
#[test]
fn the_tier_two_candidates_survive_blocking() {
    for (a, b) in [
        (
            "Requirements are managed as claims, not as documents.",
            "The unit of requirements work is a claim, not a document.",
        ),
        (
            "Requirements are decisions, not documents.",
            "The tool should model decisions rather than documents.",
        ),
        (
            "The system shall apply at least 12 kN to the pedal.",
            "At least 12 kN shall be applied to the pedal.",
        ),
    ] {
        assert!(
            shared_terms(a, b) >= MIN_SHARED_TERMS,
            "{a:?} / {b:?} share only {} term(s)",
            shared_terms(a, b)
        );
    }
}

#[test]
fn terms_fold_plurals_drop_stopwords_and_stay_deterministic() {
    let terms = content_terms("The documents are managed as claims, not as documents.");
    assert!(terms.contains("document"), "{terms:?}");
    assert!(terms.contains("claim"), "{terms:?}");
    assert!(!terms.contains("the"), "stopwords are dropped: {terms:?}");
    assert_eq!(
        terms,
        content_terms("The documents are managed as claims, not as documents.")
    );
    // Numbers survive, and a trailing "s" on a short token is not stripped blindly.
    assert!(content_terms("At least 12 kN").contains("12"));
    assert!(content_terms("gas").contains("gas"));
}

/// Blocking is a cost control, so it must actually cut the pair count — otherwise the funnel
/// buys nothing over the 12.5M-pair naive worst case the design calls economically dead.
#[test]
fn blocking_is_cheaper_than_every_pair() {
    let rows = vec![
        row("REQ-A", "The system shall log each command.", "a"),
        row("REQ-B", "The system shall delete each command.", "b"),
        row("REQ-C", "The vendor shall document the protocol.", "c"),
        row(
            "REQ-D",
            "The vendor shall document the protocol version.",
            "d",
        ),
    ];
    let candidates = candidate_pairs(&rows, MIN_SHARED_TERMS);
    let naive = naive_pairs(rows.len());
    assert_eq!(naive, 6);
    assert!(
        candidates.len() < naive,
        "blocking should drop unrelated pairs: {candidates:?}"
    );
    // The two unrelated claims (A/B vs C/D) never meet.
    assert!(
        !candidates.contains(&("REQ-A".to_string(), "REQ-C".to_string())),
        "{candidates:?}"
    );
}

/// The offline default refuses to guess: every candidate comes back unadjudicated, with the
/// provider's identity recorded, and never as "unrelated".
#[test]
fn the_offline_default_never_guesses() {
    let rows = vec![
        row("REQ-A", "Requirements are decisions, not documents.", "a"),
        row(
            "REQ-B",
            "The tool should model decisions rather than documents.",
            "b",
        ),
    ];
    let (proposals, already_identical) = propose(&rows, &NoProvider, MIN_SHARED_TERMS);
    assert_eq!(already_identical, 0);
    assert_eq!(proposals.len(), 1, "{proposals:?}");
    let proposal = &proposals[0];
    assert_eq!(proposal.provider, "none");
    assert!(
        matches!(proposal.adjudication, Adjudication::Unverifiable { .. }),
        "{:?}",
        proposal.adjudication
    );
    assert!(!matches!(
        proposal.adjudication,
        Adjudication::Unrelated { .. }
    ));
}

/// Two claims at one address are one requirement: a provider has nothing to decide, so the
/// pair is counted and never sent.
#[test]
fn a_shared_address_is_counted_not_adjudicated() {
    let rows = vec![
        row(
            "REQ-A",
            "The system shall log each command.",
            "same-address",
        ),
        row(
            "REQ-B",
            "The system shall log each command.",
            "same-address",
        ),
    ];
    let (proposals, already_identical) = propose(&rows, &NoProvider, MIN_SHARED_TERMS);
    assert_eq!(already_identical, 1);
    assert!(proposals.is_empty(), "{proposals:?}");
}

/// A provider that *does* answer is recorded verbatim, and its verdict is what lands — the
/// seam is usable, not decorative.
#[test]
fn a_provider_that_answers_is_recorded_with_its_verdict() {
    struct Stub;
    impl Provider for Stub {
        fn id(&self) -> String {
            "stub-1.2.3".to_string()
        }
        fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
            Adjudication::Equivalent { confidence: 80 }
        }
    }
    let rows = vec![
        row("REQ-A", "Requirements are decisions, not documents.", "a"),
        row(
            "REQ-B",
            "The tool should model decisions rather than documents.",
            "b",
        ),
    ];
    let (proposals, _) = propose(&rows, &Stub, MIN_SHARED_TERMS);
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].provider, "stub-1.2.3");
    assert_eq!(
        proposals[0].adjudication,
        Adjudication::Equivalent { confidence: 80 }
    );
}

/// One shared term is not evidence: an index built on a single common word would make a block
/// out of half the corpus and defeat the point.
#[test]
fn a_single_shared_term_is_not_a_candidate() {
    let rows = vec![
        row("REQ-A", "The vendor shall document the protocol.", "a"),
        row("REQ-B", "The vendor shall certify the harness.", "b"),
    ];
    assert_eq!(shared_terms(&rows[0].statement, &rows[1].statement), 1);
    assert!(candidate_pairs(&rows, MIN_SHARED_TERMS).is_empty());
}

/// A full stop is punctuation, not part of a word. `.` has to stay *inside* a token so
/// decimals survive ("0.15" is one term), which means the ends need trimming explicitly — or
/// "documents." and "document" become different terms and blocking silently loses recall.
#[test]
fn trailing_punctuation_does_not_change_a_term() {
    let with_stop = content_terms("The system shall log the documents.");
    let without = content_terms("The system shall log the document");
    assert!(
        with_stop.contains("document"),
        "a trailing period must not defeat the plural fold: {with_stop:?}"
    );
    assert_eq!(with_stop, without, "{with_stop:?} vs {without:?}");
    assert!(content_terms("Respond within 0.15 seconds.").contains("0.15"));
}
