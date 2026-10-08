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

fn tier2_gold() -> req::funnel::Tier2Corpus {
    req::funnel::Tier2Corpus::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/bench/corpus/tier2-v1.yml"
    ))
    .expect("the gold set loads")
}

/// The gold set is the measuring instrument for every Tier-2 claim. It must keep all three labels,
/// stay large enough to mean something, and hold no degenerate pair — a corpus of identical or
/// mislabelled pairs would silently make a provider look better than it is.
#[test]
fn the_gold_set_holds_three_labels_and_no_degenerate_pairs() {
    let corpus = tier2_gold();
    assert!(
        corpus.pairs.len() >= 70,
        "grow the gold set, never shrink it: {} pairs",
        corpus.pairs.len()
    );
    for label in [
        req::funnel::Tier2Label::Equivalent,
        req::funnel::Tier2Label::Related,
        req::funnel::Tier2Label::Unrelated,
    ] {
        assert!(
            corpus.pairs.iter().any(|pair| pair.label == label),
            "no {label:?} pairs"
        );
    }
    for pair in &corpus.pairs {
        assert_ne!(pair.a, pair.b, "{}: identical sides", pair.id);
        assert!(
            !pair.transform.is_empty(),
            "{}: unrecorded transform",
            pair.id
        );
        assert!(
            !pair.a.trim().is_empty() && !pair.b.trim().is_empty(),
            "{}: empty side",
            pair.id
        );
    }
}

/// **The property the gold set exists to measure.** Every pair it labels equivalent must be
/// reachable by blocking, or the funnel can never propose it and the provider never sees it.
#[test]
fn the_gold_set_is_fully_reachable_by_blocking() {
    let misses = req::funnel::tier2_blocking_misses(&tier2_gold(), MIN_SHARED_TERMS);
    assert!(misses.is_empty(), "unreachable equivalents: {misses:?}");
}

/// A miss must only ever be an equivalent: related and unrelated pairs are not duplicates, so an
/// unreachable negative is allowed and must not be reported as a recall failure.
#[test]
fn only_equivalents_are_ever_reported_as_misses() {
    let corpus = tier2_gold();
    for miss in req::funnel::tier2_blocking_misses(&corpus, MIN_SHARED_TERMS) {
        let id = miss.split(':').next().unwrap_or_default();
        let pair = corpus
            .pairs
            .iter()
            .find(|pair| pair.id == id)
            .expect("a miss names a known pair");
        assert_eq!(pair.label, req::funnel::Tier2Label::Equivalent, "{miss}");
    }
}

/// A provider that calls everything equivalent: the oracle for the scoring maths, in-process and
/// deterministic. It is also the honest worst case — every non-equivalent it sees becomes a
/// **false merge**, the one error whose cost is asymmetric.
#[test]
fn a_provider_that_calls_everything_equivalent_earns_false_merges() {
    struct AlwaysEquivalent;
    impl Provider for AlwaysEquivalent {
        fn id(&self) -> String {
            "always-equivalent".to_string()
        }
        fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
            Adjudication::Equivalent { confidence: 100 }
        }
    }
    let corpus = tier2_gold();
    let proposed_equivalents = corpus
        .pairs
        .iter()
        .filter(|p| p.label == req::funnel::Tier2Label::Equivalent)
        .filter(|p| shared_terms(&p.a, &p.b) >= MIN_SHARED_TERMS)
        .count();
    let score = req::funnel::score_gold(&corpus, &AlwaysEquivalent, MIN_SHARED_TERMS);
    assert_eq!(score.true_positives, proposed_equivalents);
    assert_eq!(score.false_negatives, 0, "it never says anything else");
    assert!(score.false_positives > 0, "it must be caught: {score:?}");
    assert_eq!(score.recall(), Some(100));
    assert!(score.precision().expect("defined") < 100, "{score:?}");
    assert_eq!(score.not_proposed + score.scored, corpus.pairs.len());
}

/// Abstention earns no credit and no blame: no answer is `Unverifiable`, not a wrong answer, and
/// precision and recall are **undefined** rather than zero — a distinction that keeps an unmeasured
/// provider from looking measured.
#[test]
fn an_abstaining_provider_is_unverifiable_not_wrong() {
    let corpus = tier2_gold();
    let score = req::funnel::score_gold(&corpus, &NoProvider, MIN_SHARED_TERMS);
    assert_eq!(score.true_positives, 0);
    assert_eq!(score.false_positives, 0);
    assert_eq!(score.false_negatives, 0);
    assert!(score.unverifiable > 0);
    assert_eq!(score.precision(), None);
    assert_eq!(score.recall(), None);
}

/// REQ-0011 is measured, not assumed: a provider whose answer changes on the second look must be
/// counted, however deterministic its architecture claims to be.
#[test]
fn an_unstable_provider_is_measured_as_flipping() {
    use std::cell::Cell;
    struct EveryOther {
        calls: Cell<usize>,
    }
    impl Provider for EveryOther {
        fn id(&self) -> String {
            "every-other".to_string()
        }
        fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
            let n = self.calls.get();
            self.calls.set(n + 1);
            if n.is_multiple_of(2) {
                Adjudication::Equivalent { confidence: 100 }
            } else {
                Adjudication::Related { confidence: 100 }
            }
        }
    }
    let corpus = tier2_gold();
    let flaky = EveryOther {
        calls: Cell::new(0),
    };
    let proposed = corpus
        .pairs
        .iter()
        .filter(|p| shared_terms(&p.a, &p.b) >= MIN_SHARED_TERMS)
        .count();
    let (flipped, measured) = req::funnel::flip_count(&corpus, &flaky, MIN_SHARED_TERMS, 2);
    assert_eq!(flipped, measured, "every proposed pair flips");
    // Measured over exactly the pairs blocking proposes: a pair it never hands over cannot flip.
    assert_eq!(measured, proposed);
    let (stable, measured) = req::funnel::flip_count(&corpus, &NoProvider, MIN_SHARED_TERMS, 3);
    assert_eq!(stable, 0);
    assert!(measured > 0);
}

/// The command provider is a process boundary with a documented contract — two lines in, one word
/// out — verified with `sh` standing in for a model, so the protocol is tested without one.
#[test]
fn the_command_provider_speaks_the_documented_protocol() {
    let command = "read -r a; read -r b; echo 'equivalent 87'".to_string();
    let provider = req::funnel::CommandProvider::new(command.clone());
    let verdict = provider.adjudicate(
        &Candidate {
            claim_id: "T2-E-001",
            statement: "one",
        },
        &Candidate {
            claim_id: "T2-E-001",
            statement: "two",
        },
    );
    assert_eq!(
        verdict,
        Adjudication::Equivalent { confidence: 87 },
        "{verdict:?}"
    );
    assert_eq!(provider.id(), format!("cmd:{command}"));
    assert!(req::funnel::provider_request("a", "b").starts_with("A: a\nB: b\nCHOOSE:"));
}

/// `unrelated` contains `related`. Reading the substring first would turn every abstention into a
/// confident `related`, so the longer word is checked first — and this pins it.
#[test]
fn the_answer_parser_prefers_unrelated_over_its_own_substring() {
    assert_eq!(
        req::funnel::parse_adjudication("UNRELATED"),
        Adjudication::Unrelated { confidence: 0 }
    );
    assert_eq!(
        req::funnel::parse_adjudication("related 40"),
        Adjudication::Related { confidence: 40 }
    );
    assert!(matches!(
        req::funnel::parse_adjudication("I cannot say"),
        Adjudication::Unverifiable { .. }
    ));
}
