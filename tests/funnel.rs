//! The Tier-2 funnel: blocking recall, its cost, and the provider seam.
//!
//! The recall property is measured against the corpus's own labels — the same independent
//! oracle the identity gates use, and the only reason the number means anything.

use req::funnel::{
    Adjudication, Candidate, MAX_PAIRS_PER_RUN, MIN_SHARED_TERMS, NO_CAP, NoProvider, Proposed,
    Provider, blocking_misses, candidate_pairs, content_terms, naive_pairs, propose, shared_terms,
};
use req::model::{Corpus, Rationale, rationale_digest, relations_digest};
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
        // Real digests over the fixture's own content: a placeholder hash beside empty
        // relations is a Row state `load_rows` can never produce.
        relations_hash: relations_digest(&[]),
        rationale_hash: rationale_digest(&Rationale {
            source: "docs/adr/0004-model-proposes-human-disposes.md".to_string(),
            date: "2026-10-07".to_string(),
            rejected_alternatives: vec![],
            assumptions: vec![],
        }),
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
    let Proposed {
        proposals,
        already_identical,
        deferred,
    } = propose(&rows, &NoProvider, MIN_SHARED_TERMS, NO_CAP);
    assert_eq!(deferred, 0, "no cap, so nothing is deferred");
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
    let Proposed {
        proposals,
        already_identical,
        deferred,
    } = propose(&rows, &NoProvider, MIN_SHARED_TERMS, NO_CAP);
    assert_eq!(already_identical, 1);
    assert_eq!(
        deferred, 0,
        "an already-identical pair is resolved, not capped"
    );
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
    let Proposed {
        proposals,
        deferred,
        ..
    } = propose(&rows, &Stub, MIN_SHARED_TERMS, NO_CAP);
    assert_eq!(deferred, 0);
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].provider, "stub-1.2.3");
    assert_eq!(
        proposals[0].adjudication,
        Adjudication::Equivalent { confidence: 80 }
    );
}

/// **The cap protects the reviewer's attention, not the machine's** (ADR 0011). It must bind
/// *before* the provider is called: a cap applied after adjudication spends exactly the money it
/// was meant to save, and the deferred pairs must be counted rather than dropped — a silently
/// shortened list is the failure mode this whole project runs on.
#[test]
fn the_cap_binds_before_the_provider_is_called() {
    use std::cell::Cell;
    struct Counting(Cell<usize>);
    impl Provider for Counting {
        fn id(&self) -> String {
            "counting".to_string()
        }
        fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
            self.0.set(self.0.get() + 1);
            Adjudication::Unverifiable {
                reason: "counting only".to_string(),
            }
        }
    }
    // Five claims whose every pair shares three content terms and whose addresses all differ:
    // ten candidates, none of them already identical. The count is hand-derived, not recomputed.
    let rows: Vec<Row> = (0..5)
        .map(|i| {
            row(
                &format!("REQ-{i}"),
                "The system shall log each command.",
                &format!("addr-{i}"),
            )
        })
        .collect();
    assert_eq!(candidate_pairs(&rows, MIN_SHARED_TERMS).len(), 10);

    let provider = Counting(Cell::new(0));
    let run = propose(&rows, &provider, MIN_SHARED_TERMS, 3);
    assert_eq!(run.proposals.len(), 3, "the cap is the number of proposals");
    assert_eq!(
        provider.0.get(),
        3,
        "the provider is asked at most max_pairs times — a cap that calls it first saves nothing"
    );
    assert_eq!(
        run.deferred, 7,
        "every pair the cap did not reach is deferred, never dropped"
    );
    assert_eq!(run.already_identical, 0);
}

/// A deferred pair is a **third outcome**, not a fourth answer: it was never adjudicated, so it
/// can never be counted as `unverifiable` — and a cap large enough to reach every candidate
/// defers nothing at all.
#[test]
fn deferred_pairs_are_neither_adjudicated_nor_unverifiable() {
    let rows: Vec<Row> = (0..3)
        .map(|i| {
            row(
                &format!("REQ-{i}"),
                "The system shall log each command.",
                &format!("addr-{i}"),
            )
        })
        .collect();
    let whole = propose(&rows, &NoProvider, MIN_SHARED_TERMS, NO_CAP);
    assert_eq!(whole.proposals.len(), 3);
    assert_eq!(whole.deferred, 0, "no cap, nothing deferred");
    let unverifiable = |run: &Proposed| {
        run.proposals
            .iter()
            .filter(|p| matches!(p.adjudication, Adjudication::Unverifiable { .. }))
            .count()
    };
    assert_eq!(
        unverifiable(&whole),
        3,
        "the offline default refuses every pair it is asked about"
    );

    let capped = propose(&rows, &NoProvider, MIN_SHARED_TERMS, 1);
    assert_eq!(capped.proposals.len(), 1);
    assert_eq!(capped.deferred, 2);
    assert_eq!(
        unverifiable(&capped),
        1,
        "the deferred pair was never asked, so it is not an unverifiable answer"
    );
    assert_eq!(
        capped.proposals.len() + capped.deferred,
        whole.proposals.len(),
        "the three outcomes partition the candidates"
    );
}

/// A deferred pair and an unadjudicated pair are **different facts**, and the claims verdict names
/// each rather than summing them. `PASS` is a claim that both counts are zero, and a capped run is
/// never allowed to read as "everything was asked about".
#[test]
fn the_claims_verdict_names_deferral_separately_from_no_answer() {
    let (code, sentence) = req::funnel::claims_verdict(0, 0, MAX_PAIRS_PER_RUN);
    assert_eq!(code, 0, "{sentence}");
    assert!(sentence.starts_with("PASS"), "{sentence}");

    let (code, sentence) = req::funnel::claims_verdict(0, 7, 3);
    assert_eq!(code, 3, "{sentence}");
    assert!(
        sentence.contains("7") && sentence.contains("deferred"),
        "{sentence}"
    );
    assert!(
        !sentence.contains("unadjudicated"),
        "a deferred pair was never put to the provider, so it is not an answer: {sentence}"
    );

    let (code, sentence) = req::funnel::claims_verdict(5, 0, MAX_PAIRS_PER_RUN);
    assert_eq!(code, 3, "{sentence}");
    assert!(sentence.contains("unadjudicated"), "{sentence}");
    assert!(!sentence.contains("deferred"), "{sentence}");

    // Both at once: both are named, because reporting one would hide the other.
    let (_, sentence) = req::funnel::claims_verdict(5, 7, 3);
    assert!(
        sentence.contains("deferred") && sentence.contains("unadjudicated"),
        "{sentence}"
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
    let score = req::funnel::score_gold(&corpus, &AlwaysEquivalent, MIN_SHARED_TERMS, 0);
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
    let score = req::funnel::score_gold(&corpus, &NoProvider, MIN_SHARED_TERMS, 0);
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
    // Assert the parts, not the whole string: the contract gains fields, and a `starts_with` over
    // the entire format made this test fail on a rewording rather than on a broken contract.
    let request = req::funnel::provider_request("T2-E-001", "a", "b", "module|system");
    for part in [
        "ID: T2-E-001",
        "A: a",
        "B: b",
        "DIFF: module|system",
        "CHOOSE:",
    ] {
        assert!(
            request.contains(part),
            "the contract must carry {part}: {request}"
        );
    }
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

/// The provider contract widened rather than changed: a JSON object carries the confidence, and
/// the bare word still works — so every existing stub keeps answering and the threshold sweep has
/// a number to threshold on.
#[test]
fn a_provider_may_answer_with_json_and_the_confidence_survives() {
    assert_eq!(
        req::funnel::parse_adjudication(r#"{"choice": "equivalent", "confidence": 87}"#),
        Adjudication::Equivalent { confidence: 87 }
    );
    assert_eq!(
        req::funnel::parse_adjudication(r#"{"choice":"related","confidence":12}"#),
        Adjudication::Related { confidence: 12 }
    );
    // An option we did not define is not a guess: it is unverifiable.
    assert!(matches!(
        req::funnel::parse_adjudication(r#"{"choice":"maybe"}"#),
        Adjudication::Unverifiable { .. }
    ));
    // A missing confidence is "not stated" (0), which the type documents.
    assert_eq!(
        req::funnel::parse_adjudication(r#"{"choice":"unrelated"}"#),
        Adjudication::Unrelated { confidence: 0 }
    );
    // ...and the old forms are untouched.
    assert_eq!(
        req::funnel::parse_adjudication("related 40"),
        Adjudication::Related { confidence: 40 }
    );
    assert_eq!(
        req::funnel::parse_adjudication("UNRELATED"),
        Adjudication::Unrelated { confidence: 0 }
    );
    // Malformed JSON falls back to the word scan rather than failing the pair.
    assert_eq!(
        req::funnel::parse_adjudication(r#"{"choice": broken, unrelated"#),
        Adjudication::Unrelated { confidence: 0 }
    );
}

/// A count is not actionable; the ids are. Each named false positive must really be a pair the gold
/// set labels non-equivalent — otherwise the list would send the next fix after the wrong pairs.
#[test]
fn the_score_names_the_pairs_it_got_wrong() {
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
    let score = req::funnel::score_gold(&corpus, &AlwaysEquivalent, MIN_SHARED_TERMS, 0);
    assert_eq!(score.false_positive_ids.len(), score.false_positives);
    assert!(!score.false_positive_ids.is_empty());
    for id in &score.false_positive_ids {
        let pair = corpus
            .pairs
            .iter()
            .find(|pair| pair.id == *id)
            .expect("a named pair exists");
        assert_ne!(
            pair.label,
            req::funnel::Tier2Label::Equivalent,
            "{id} is labelled equivalent, so it cannot be a false merge"
        );
    }
    assert!(score.false_negative_ids.is_empty());
    // An abstaining provider gets no error ids either: no answer is not a wrong answer.
    let abstained = req::funnel::score_gold(&corpus, &NoProvider, MIN_SHARED_TERMS, 0);
    assert!(abstained.false_positive_ids.is_empty());
    assert!(abstained.false_negative_ids.is_empty());
}

/// A provider that crashes must be reported with its own words. This exact case cost a full run: the
/// wrapper lacked its executable bit, exited 126 for every pair, and the harness reported 68
/// `unverifiable` — blaming the model for a shell refusing to run a file.
#[test]
fn a_provider_that_crashes_reports_why_instead_of_going_quiet() {
    let provider = req::funnel::CommandProvider::new(
        "echo 'boom: cannot reach the model' >&2; exit 3".to_string(),
    );
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
    match verdict {
        Adjudication::Unverifiable { reason } => {
            assert!(
                reason.contains("boom"),
                "the provider's own words must survive: {reason}"
            );
            // Which of the two failure paths fires is a *race* — a command that exits before
            // reading its input can produce either — so what is pinned is the invariant, not the
            // path: the provider's own words survive into the reason, and the reason says how it
            // failed. The outcome (Unverifiable) is what a caller reads; the reason is for a human.
            assert!(
                reason.contains("closed its input") || reason.contains("exited"),
                "the reason must describe how it failed: {reason}"
            );
        }
        other => panic!("a crashed provider is unverifiable, not {other:?}"),
    }
}

/// The floor is the measured remedy: the model splits equivalent/related 0.478 to 0.4385, so below
/// a threshold its answer is a coin flip. A declined `equivalent` becomes `related` — the
/// asymmetric direction, a missed duplicate rather than a silent merge — and it is counted.
#[test]
fn a_confidence_floor_declines_a_coin_flip_instead_of_merging_on_it() {
    struct Fixed(u8);
    impl Provider for Fixed {
        fn id(&self) -> String {
            format!("fixed-{}", self.0)
        }
        fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
            Adjudication::Equivalent { confidence: self.0 }
        }
    }
    let corpus = tier2_gold();
    let unfloored = req::funnel::score_gold(&corpus, &Fixed(50), MIN_SHARED_TERMS, 0);
    let floored = req::funnel::score_gold(&corpus, &Fixed(50), MIN_SHARED_TERMS, 80);
    assert!(unfloored.false_positives > 0);
    assert_eq!(
        floored.false_positives, 0,
        "a declined merge is not a merge"
    );
    assert_eq!(
        floored.below_floor, unfloored.scored,
        "every answer was declined"
    );
    assert_eq!(floored.true_positives, 0);
    assert!(
        floored.false_negatives > 0,
        "the cost of the floor is missed duplicates, which is the affordable direction"
    );
    // A provider above the floor is untouched by it.
    let confident = req::funnel::score_gold(&corpus, &Fixed(95), MIN_SHARED_TERMS, 80);
    assert_eq!(confident.false_positives, unfloored.false_positives);
    assert_eq!(confident.below_floor, 0);
}

/// One run cannot measure stability, and asking twice for a comparison that will not happen both
/// inflates the cost and invents a `0%` that reads like a result.
#[test]
fn a_single_run_is_not_a_stability_measurement() {
    use std::cell::Cell;
    struct Counting(Cell<usize>);
    impl Provider for Counting {
        fn id(&self) -> String {
            "counting".to_string()
        }
        fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
            self.0.set(self.0.get() + 1);
            Adjudication::Unverifiable {
                reason: "counting only".to_string(),
            }
        }
    }
    let corpus = tier2_gold();
    let provider = Counting(Cell::new(0));
    let (flipped, measured) = req::funnel::flip_count(&corpus, &provider, MIN_SHARED_TERMS, 1);
    assert_eq!((flipped, measured), (0, 0), "nothing was compared");
    assert_eq!(
        provider.0.get(),
        0,
        "a run that cannot compare must not ask the provider twice"
    );
}

/// A PASS is a claim of three things at once, and this pins the rule that they must all hold —
/// including the one that is easy to fake: stability has to be *measured*.
#[test]
fn a_pass_requires_a_measured_flip_rate_and_no_false_merge() {
    let (code, sentence) = req::funnel::gold_verdict(0, 0, 0, 68);
    assert_eq!(code, 0, "{sentence}");
    assert!(sentence.starts_with("PASS"), "{sentence}");

    // The same perfect score with stability unmeasured is not a pass: REQ-0011 is not satisfied by
    // a number nobody took.
    let (code, sentence) = req::funnel::gold_verdict(0, 0, 0, 0);
    assert_eq!(code, 3, "{sentence}");
    assert!(sentence.contains("not measured"), "{sentence}");

    // A false merge fails even when the flip rate is unmeasured — a defect outranks an open question.
    assert_eq!(req::funnel::gold_verdict(0, 1, 0, 0).0, 1);
    // An equivalent pair blocking can never reach fails too.
    assert_eq!(req::funnel::gold_verdict(2, 0, 0, 68).0, 1);
    // And a measured flip rate above 2% fails a run that is otherwise perfect.
    assert_eq!(req::funnel::gold_verdict(0, 0, 3, 68).0, 1);
}

/// The diff is the change under test, so it must name exactly the terms that differ — sorted,
/// deduplicated, argument-order independent, and empty when nothing differs.
#[test]
fn the_diff_names_exactly_what_differs() {
    assert_eq!(
        req::funnel::differing_terms(
            "The system shall log each command.",
            "The module shall log each command."
        ),
        "module|system"
    );
    assert_eq!(
        req::funnel::differing_terms("The vendor shall log.", "The vendor shall log."),
        ""
    );
    assert_eq!(
        req::funnel::differing_terms("The system shall log.", "The module shall store."),
        req::funnel::differing_terms("The module shall store.", "The system shall log.")
    );
}
