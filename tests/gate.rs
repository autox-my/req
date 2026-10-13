//! Integration tests at req's public seams (see AGENTS.md): the canonicalizer's
//! identity rules, and the gate's evaluation.
//!
//! TDD order: this file was written before `bench::evaluate` existed — the
//! compile error was the red state.

use req::bench::{Verdict, evaluate};
use req::canon::{canonical_json, canonicalize_statement};
use req::model::{Corpus, Pair, Registry};

fn pair(id: &str, a: &str, b: &str) -> Pair {
    Pair {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        source: "tests/gate.rs".to_string(),
        note: None,
        direction: None,
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
    assert_eq!(ev.verdict, Verdict::Pass);
}

/// A not-equivalent pair with an unstructured side cannot merge, so the FMR
/// property holds — but it was NOT structurally verified. It must be counted, not
/// silently ignored (an unverified pair is not a verified pair), and a denominator
/// made only of such pairs measured nothing, so the run is UNVERIFIABLE rather than a
/// clean PASS.
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
    assert_eq!(ev.unverifiable_pairs, vec!["U-001".to_string()]);
    assert!(ev.fmr_hits.is_empty());
    assert_eq!(
        ev.verdict,
        Verdict::Unverifiable,
        "counted, and never reported as a measured 0.0 rate"
    );
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
    assert_eq!(ev.verdict, Verdict::Fail);
}

/// Found by the *widened* external corpus (PROMISE NFR prose), rules v4: a
/// `shall`->`may` mutation collapsed, because the modality table mapped "may not" to
/// SHALL-NOT. T1.6 never sanctioned that, and "may not X" is genuinely ambiguous —
/// "is not permitted to X" vs "is permitted to not X". The scope of a negation is not
/// a Tier-1 question: lint it and stay unstructured, exactly like "will".
#[test]
fn may_not_is_ambiguous_and_never_guessed() {
    let (prohibition, _) = canonicalize_statement(
        "A surface computer shall not require a user to log in.",
        &Registry::empty(),
    );
    let (ambiguous, lints) = canonicalize_statement(
        "A surface computer may not require a user to log in.",
        &Registry::empty(),
    );
    assert_eq!(prohibition.modality_str(), "SHALL-NOT");
    assert!(
        ambiguous.unstructured,
        "an ambiguous negation scope must not be resolved"
    );
    assert!(
        lints.iter().any(|l| l.code == "ambiguous_modality"),
        "lints: {:?}",
        lints.iter().map(|l| &l.code).collect::<Vec<_>>()
    );
    assert_ne!(canonical_json(&prohibition), canonical_json(&ambiguous));
}

/// The absorbed form is different: there the negation belongs to the *comparison*
/// ("may not exceed 200" = permitted up to 200), which is what absorption records —
/// so it stays structured, and weaker than "shall not exceed".
#[test]
fn absorbed_may_not_exceed_stays_structured_and_weaker() {
    let (shall_not, _) =
        canonicalize_statement("The system shall not exceed 200 ms.", &Registry::empty());
    let (may_not, lints) =
        canonicalize_statement("The system may not exceed 200 ms.", &Registry::empty());
    assert!(!may_not.unstructured, "{:?}", lints.len());
    assert_eq!(shall_not.modality_str(), "SHALL");
    assert_eq!(may_not.modality_str(), "MAY");
    assert_ne!(canonical_json(&shall_not), canonical_json(&may_not));
}

/// Review [F7]: FMR is *collapsed / all not-equivalent pairs*, so a corpus with no
/// not-equivalent pairs scores 0/0. That is not a measurement — the gate has nothing
/// to falsify the bet on — and a generator regression that drops every mutation would
/// otherwise read as PASS. A vacuous denominator is UNVERIFIABLE, exit 3.
#[test]
fn a_corpus_with_no_not_equivalent_pairs_is_unverifiable_not_pass() {
    let ev = evaluate(&corpus(vec![], vec![]), &Registry::empty());
    assert_eq!(ev.not_equivalent_total, 0);
    assert_eq!(ev.verdict, Verdict::Unverifiable);
    assert_eq!(
        req::bench::exit_code(ev.verdict),
        3,
        "FMR 0/0 must block, not pass"
    );
}

/// The seam between a verdict and the process exit code, including the degraded
/// record: a run that could not be trusted must never exit 0. The enum carries the
/// outcome, so this test cannot drift from the string the report emits — the degraded
/// wording has exactly one home, `Verdict::as_str`.
#[test]
fn exit_code_maps_the_three_outcomes_and_the_degraded_record() {
    assert_eq!(req::bench::exit_code(Verdict::Pass), 0);
    assert_eq!(req::bench::exit_code(Verdict::Fail), 1);
    assert_eq!(req::bench::exit_code(Verdict::Unverifiable), 3);
    assert_eq!(req::bench::exit_code(Verdict::Degraded), 3);
    // Every wording the report emits goes through `as_str`, and those records are
    // append-only history: pin all four, not just the degraded one, so a silent
    // rewording of PASS/FAIL/UNVERIFIABLE fails here rather than in the archive.
    assert_eq!(Verdict::Pass.as_str(), "PASS");
    assert_eq!(Verdict::Fail.as_str(), "FAIL");
    assert_eq!(Verdict::Unverifiable.as_str(), "UNVERIFIABLE");
    assert_eq!(Verdict::Degraded.as_str(), "DEGRADED-PASS/FAIL-SEE-REPORT");
}

/// The FMR denominator is measurable only if at least one `not_equivalent` pair was
/// actually comparable. A corpus whose mutation sides all fell to the unstructured
/// fallback has a *non-empty* denominator that measured nothing — 0 hits over N
/// unverifiable pairs — which is the same 0/0 as the empty case, and it must not read
/// as PASS.
#[test]
fn a_denominator_of_nothing_but_unverifiable_pairs_is_not_a_pass() {
    let ev = evaluate(
        &corpus(
            vec![],
            vec![pair(
                "N-001",
                "The system shall respond quickly.",
                "The system shall be seamless.",
            )],
        ),
        &Registry::empty(),
    );
    assert_eq!(ev.not_equivalent_total, 1, "the denominator is not empty");
    assert_eq!(ev.unverifiable_pairs, vec!["N-001".to_string()]);
    assert!(ev.fmr_hits.is_empty());
    assert_eq!(ev.verdict, Verdict::Unverifiable);
    assert_eq!(req::bench::exit_code(ev.verdict), 3);
    assert!(!req::bench::fmr_measurable(1, 1));
    assert!(req::bench::fmr_measurable(1, 0));
    // And the gate they feed, spelled once for both the verdict and the record: a
    // collapse is FAIL, an unmeasured denominator is UNVERIFIABLE, no collapse over a
    // measured denominator is PASS.
    assert_eq!(req::bench::fmr_gate(1, 5, 0), Verdict::Fail);
    assert_eq!(req::bench::fmr_gate(0, 0, 0), Verdict::Unverifiable);
    assert_eq!(req::bench::fmr_gate(0, 3, 3), Verdict::Unverifiable);
    assert_eq!(req::bench::fmr_gate(0, 3, 1), Verdict::Pass);
    // The /1 value space: a rate is reported only when it was measured, for the FMR
    // rate and the direction accuracy alike. `/0` wrote 0.0 there, which a consumer
    // reads as a clean measurement.
    assert_eq!(req::bench::reported_rate(false, 0.0), None);
    assert_eq!(req::bench::reported_rate(false, 0.7), None);
    assert_eq!(req::bench::reported_rate(true, 0.0), Some(0.0));
}

/// The record's discriminator is part of the consumer contract, so it is pinned here
/// beside the rest of the value space rather than smuggled into a denominator test: a
/// deliberate bump is a migration step, and it should fail a test named for it.
#[test]
fn the_record_schema_names_its_value_space() {
    assert_eq!(req::bench::RECORD_SCHEMA, "req.bench.result/1");
}

/// The direction gate is a tri-state like the FMR gate, and for the same reason: a
/// corpus that carries no `direction:` labels (or whose sides cannot be parsed) has
/// `checked: 0`, and a `PASS` there would read as "the direction check came back
/// clean" when no direction check happened.
#[test]
fn the_direction_gate_is_not_a_vacuous_pass() {
    use req::bench::direction_gate;
    assert_eq!(
        direction_gate(0, false),
        Verdict::Unverifiable,
        "nothing measured is not a pass"
    );
    // One comparable pair is the boundary: measured, so the gate can pass.
    assert_eq!(direction_gate(1, false), Verdict::Pass);
    assert_eq!(direction_gate(2, true), Verdict::Fail);
    assert_eq!(
        direction_gate(0, true),
        Verdict::Fail,
        "a label error is a corpus defect even when nothing else was checked"
    );
}

/// Precedence at the run seam: a definite defect outranks the degraded record, the same
/// way it outranks the vacuous denominator inside `evaluate`. An empty registry makes
/// the run untrustworthy, but it does not un-find an FMR hit, a dangerous direction read
/// or a misspelled label — and demoting that to exit 3 is how a script keyed on exit 1
/// would miss it.
#[test]
fn a_defect_outranks_the_degraded_record() {
    use req::bench::reported_verdict;
    assert_eq!(reported_verdict(Verdict::Fail, true), Verdict::Fail);
    assert_eq!(
        req::bench::exit_code(reported_verdict(Verdict::Fail, true)),
        1
    );
    // With nothing found, a degraded run is its own outcome — never a PASS.
    assert_eq!(
        reported_verdict(Verdict::Pass, true),
        Verdict::Degraded,
        "a degraded run must never report the corpus as clean"
    );
    assert_eq!(
        reported_verdict(Verdict::Unverifiable, true),
        Verdict::Degraded
    );
    assert_eq!(
        req::bench::exit_code(reported_verdict(Verdict::Pass, true)),
        3
    );
    // Undegraded runs pass through untouched.
    assert_eq!(reported_verdict(Verdict::Pass, false), Verdict::Pass);
    assert_eq!(
        reported_verdict(Verdict::Unverifiable, false),
        Verdict::Unverifiable
    );
}

/// A definite defect still outranks the vacuous denominator, exactly as `req gate`
/// lets Fail outrank Unverifiable: a misspelled `direction:` label — which would
/// otherwise switch the direction check off — must not hide behind a corpus whose
/// FMR denominator is empty.
#[test]
fn a_direction_label_error_outranks_the_vacuous_denominator() {
    let mut labelled = pair(
        "E-001",
        "The system shall log each command.",
        "The system MUST log each command.",
    );
    labelled.direction = Some("modality_weaken".to_string()); // misspelled
    let ev = evaluate(&corpus(vec![labelled], vec![]), &Registry::empty());
    assert!(
        ev.direction_label_errors
            .iter()
            .any(|e| e.contains("modality_weaken")),
        "label errors: {:?}",
        ev.direction_label_errors
    );
    // The premise, pinned: this test is only about the precedence when the FMR
    // denominator is vacuous. If the lists were ever swapped, the pair would collapse
    // and count as an FMR hit — the test would still see FAIL and stop exercising the
    // label-error-outranks-vacuous path at all.
    assert_eq!(
        ev.not_equivalent_total, 0,
        "the denominator must be vacuous"
    );
    assert_eq!(ev.verdict, Verdict::Fail);
    assert_eq!(req::bench::exit_code(ev.verdict), 1);
}
