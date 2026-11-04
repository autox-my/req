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
    // Exit 2 is deliberately absent: `src/main.rs` reserves it for CLI usage errors
    // (missing or unrecognized subcommand, bad arguments) and `gate` follows the same
    // convention, so no bench outcome may be renumbered onto it.
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
}

/// The measurable boundary, on its own: one comparable pair is enough to measure, and
/// a denominator made entirely of unverifiable pairs (or none at all) is not.
#[test]
fn fmr_measurable_needs_one_comparable_pair() {
    assert!(!req::bench::fmr_measurable(1, 1));
    assert!(!req::bench::fmr_measurable(0, 0));
    assert!(req::bench::fmr_measurable(1, 0));
    assert!(req::bench::fmr_measurable(12, 11));
}

/// The FMR gate's truth table, the value `evaluate`'s verdict and the record's
/// `hard_gate` both read: a collapse is FAIL, an unmeasured denominator is
/// UNVERIFIABLE, and no collapse over a measured denominator is PASS.
#[test]
fn fmr_gate_truth_table() {
    assert_eq!(req::bench::fmr_gate(1, 5, 0), Verdict::Fail);
    assert_eq!(req::bench::fmr_gate(0, 0, 0), Verdict::Unverifiable);
    assert_eq!(req::bench::fmr_gate(0, 3, 3), Verdict::Unverifiable);
    assert_eq!(req::bench::fmr_gate(0, 3, 1), Verdict::Pass);
}

/// The `/1` value space for the rates: reported only when they were measured, for the
/// FMR rate, the direction accuracy and the FSR rate alike. `/0` wrote `0.0` there,
/// which a consumer reads as a clean measurement.
#[test]
fn reported_rate_only_when_measured() {
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

// ---------------------------------------------------------------------------
// #64 (ADR 0024): the CLI gate over several reports and over pytest's own output.
// ---------------------------------------------------------------------------

mod test_evidence {
    //! `req gate --test-report` over **several reports** and over **pytest's own output** (#64, ADR 0024).
    //!
    //! The public seam is the compiled `req` against a corpus built in a temp dir. The oracles are
    //! independent of `req`: the pytest fixtures in `tests/fixtures/junit/` are real pytest 9.2.0 output
    //! (`README.md` there), and every expected exit comes from the documented meaning of the case —
    //! a pass confirms a claim, a failure fails it, a skip is not a pass (ADR 0019 §5, ADR 0024 §4), an
    //! id the evidence does not carry cannot be confirmed. Items (c), (d) and the gate half of (f) are
    //! the spec's (`docs/specs/64-test-evidence.md`, unit 64-1).

    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    fn req_bin() -> PathBuf {
        std::env::var_os("CARGO_BIN_EXE_req")
            .map(PathBuf::from)
            .expect("integration tests run with CARGO_BIN_EXE_req set")
    }

    fn fixture(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/junit")
            .join(name)
            .display()
            .to_string()
    }

    const MIXED: &str = "pytest-mixed.xml";
    const ESCAPES: &str = "pytest-param-escapes.xml";

    /// A fresh corpus: `req init`, the starter claim removed, claims added by the test. No claim
    /// carries a selector, so a bare `req gate` decides every claim and only the evidence is in play.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "req-gate-evidence-{}-{}-{name}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("creates the scratch dir");
            let init = Command::new(req_bin())
                .arg("init")
                .current_dir(&dir)
                .output()
                .expect("runs req init");
            assert_eq!(init.status.code(), Some(0), "req init must succeed");
            fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
            Scratch(dir)
        }

        /// A corpus of machine-evidenced claims, one per `(claim id, linked test id)`.
        fn linking(name: &str, links: &[(&str, &str)]) -> Self {
            let s = Scratch::new(name);
            for (index, (id, test)) in links.iter().enumerate() {
                let ms = 200 + index * 100;
                let value = format!("0.{}", ms / 100);
                let test = test.replace('"', "\\\"");
                let text = format!(
                    "schema: req.claim/2\n\
                 claim_id: {id}\n\
                 statement: \"The {id} service shall respond to a health check within {ms} ms.\"\n\
                 applicability: []\n\
                 relations: []\n\
                 tests:\n  - \"{test}\"\n\
                 rationale:\n  source: docs/requirements.md\n  date: 2026-11-04\n  rejected_alternatives: []\n  assumptions: []\n\
                 pins:\n  operator: LE\n  value: \"{value}\"\n  unit: s\n"
                );
                fs::write(s.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
            }
            s
        }

        fn write(&self, relative: &str, text: &str) -> String {
            let path = self.0.join(relative);
            fs::write(&path, text).expect("writes a file");
            path.display().to_string()
        }

        fn gate(&self, args: &[&str]) -> Out {
            let mut full = vec!["gate"];
            full.extend_from_slice(args);
            let out = Command::new(req_bin())
                .args(&full)
                .current_dir(&self.0)
                .output()
                .expect("runs req");
            Out::from(out)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct Out {
        code: i32,
        stdout: String,
        stderr: String,
    }

    impl Out {
        fn from(out: std::process::Output) -> Self {
            Out {
                code: out.status.code().expect("req exits, never a signal"),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            }
        }

        fn all(&self) -> String {
            format!("{}{}", self.stdout, self.stderr)
        }
    }

    /// A one-case JUnit report in the shape pytest writes: `outcome` is `pass`, `fail` or `skip`.
    fn pytest_case(classname: &str, name: &str, outcome: &str) -> String {
        let body = match outcome {
            "pass" => String::new(),
            "fail" => "<failure message=\"assert False\"/>".to_string(),
            "skip" => "<skipped type=\"pytest.skip\" message=\"not today\"/>".to_string(),
            other => panic!("unknown outcome {other}"),
        };
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><testsuites name=\"pytest tests\"><testsuite name=\"pytest\" tests=\"1\"><testcase classname=\"{classname}\" name=\"{name}\" time=\"0.000\">{body}</testcase></testsuite></testsuites>"
        )
    }

    // ---------------------------------------------------------------------------
    // (c) pytest's own output through the gate
    // ---------------------------------------------------------------------------

    /// (c) A claim linked to a test pytest reports as passed is confirmed (exit 0) — for a plain id, a
    /// class method, a parametrized id with a space, and the escaped ids of the other fixture.
    #[test]
    fn a_claim_linked_to_a_passing_pytest_case_passes() {
        for (id, test, report) in [
            ("REQ-0001", "tests.test_probe::test_passes", MIXED),
            (
                "REQ-0002",
                "tests.test_probe.TestGroup::test_in_class",
                MIXED,
            ),
            ("REQ-0003", "tests.test_probe::test_param[a b]", MIXED),
            ("REQ-0004", "coll.test_param::test_p[a&b]", ESCAPES),
            ("REQ-0005", "coll.test_param::test_p[x<y]", ESCAPES),
            ("REQ-0006", "coll.test_param::test_p[q\"r]", ESCAPES),
        ] {
            let s = Scratch::linking("c-pass", &[(id, test)]);
            let out = s.gate(&["--test-report", &fixture(report)]);
            assert_eq!(out.code, 0, "{test}: {}", out.all());
        }
    }

    /// (c) A failed test, a strict XPASS and a setup error are failures of the claim: exit 1.
    #[test]
    fn a_claim_linked_to_a_failing_pytest_case_fails() {
        for test in [
            "tests.test_probe::test_fails",
            "tests.test_probe::test_xpass_strict",
            "tests.test_probe::test_setup_error",
        ] {
            let s = Scratch::linking("c-fail", &[("REQ-0001", test)]);
            let out = s.gate(&["--test-report", &fixture(MIXED)]);
            assert_eq!(out.code, 1, "{test}: {}", out.all());
            assert!(
                out.stdout.contains(test),
                "the test is named: {}",
                out.stdout
            );
        }
    }

    /// (c) ADR 0024 §4, asserted so the consequence is pinned: a claim whose only linked test is
    /// skipped (`@skip`, or an `xfail`) is exit 1 — never a pass, and a defect-class red rather than 3.
    #[test]
    fn a_claim_linked_only_to_a_skipped_pytest_case_is_exit_1_never_a_pass() {
        for test in [
            "tests.test_probe::test_skipped",
            "tests.test_probe::test_xfail",
        ] {
            let s = Scratch::linking("c-skip", &[("REQ-0001", test)]);
            let out = s.gate(&["--test-report", &fixture(MIXED)]);
            assert_eq!(out.code, 1, "{test}: {}", out.all());
            assert!(
                out.stdout.contains("a skipped test is not a pass"),
                "{}",
                out.stdout
            );
        }
    }

    /// (c) An id the evidence does not carry cannot be confirmed: exit 3.
    #[test]
    fn a_claim_linked_to_an_id_pytest_did_not_report_is_exit_3() {
        let s = Scratch::linking(
            "c-absent",
            &[("REQ-0001", "tests.test_probe::test_renamed")],
        );
        let out = s.gate(&["--test-report", &fixture(MIXED)]);
        assert_eq!(out.code, 3, "{}", out.all());
        assert!(out.stdout.contains("not in the report"), "{}", out.stdout);
    }

    /// (b) at the seam: a collection-error report makes `tests_linked` UNVERIFIABLE (exit 3), once,
    /// naming the file and the likely cause.
    #[test]
    fn a_collection_error_report_is_unverifiable_and_says_why() {
        let s = Scratch::linking(
            "b-collection",
            &[
                ("REQ-0001", "tests.test_probe::test_passes"),
                ("REQ-0002", "tests.test_probe::test_param[1]"),
            ],
        );
        let report = fixture("pytest-collection-error.xml");
        let out = s.gate(&["--test-report", &report]);
        assert_eq!(out.code, 3, "{}", out.all());
        assert!(
            out.stdout.contains(&report),
            "the file is named: {}",
            out.stdout
        );
        assert!(out.stdout.contains("collection error"), "{}", out.stdout);
        assert_eq!(
            out.stdout.matches("cannot be read").count(),
            1,
            "judged once, not per claim: {}",
            out.stdout
        );
    }

    // ---------------------------------------------------------------------------
    // (d) several reports: the union
    // ---------------------------------------------------------------------------

    /// (d) Two reports with disjoint ids are a union: a claim linked into each passes, and the same
    /// corpus over either report alone cannot be confirmed (the other claim's test is absent).
    #[test]
    fn two_reports_with_disjoint_ids_are_a_union() {
        let s = Scratch::linking(
            "d-union",
            &[
                ("REQ-0001", "tests.test_probe::test_passes"),
                ("REQ-0002", "coll.test_param::test_p[a&b]"),
            ],
        );
        let (mixed, escapes) = (fixture(MIXED), fixture(ESCAPES));
        let both = s.gate(&["--test-report", &mixed, "--test-report", &escapes]);
        assert_eq!(both.code, 0, "{}", both.all());
        let reversed = s.gate(&["--test-report", &escapes, "--test-report", &mixed]);
        assert_eq!(
            reversed.code,
            0,
            "order does not matter: {}",
            reversed.all()
        );
        let mixed_form = s.gate(&["--test-report", &mixed, &format!("--test-report={escapes}")]);
        assert_eq!(mixed_form.code, 0, "both spellings: {}", mixed_form.all());
        for alone in [&mixed, &escapes] {
            let out = s.gate(&["--test-report", alone]);
            assert_eq!(
                out.code,
                3,
                "one report alone lacks the other's test: {}",
                out.all()
            );
        }
    }

    /// (d) The same id with the same outcome in two reports dedupes.
    #[test]
    fn the_same_id_with_the_same_outcome_dedupes() {
        let s = Scratch::linking("d-dedupe", &[("REQ-0001", "tests.test_probe::test_passes")]);
        let again = s.write(
            "again.xml",
            &pytest_case("tests.test_probe", "test_passes", "pass"),
        );
        let out = s.gate(&["--test-report", &fixture(MIXED), "--test-report", &again]);
        assert_eq!(out.code, 0, "{}", out.all());
    }

    /// (d) The same id with different outcomes is unusable evidence: UNVERIFIABLE, **once** (not per
    /// claim), naming the id and **both files** — whichever outcome would have been "the right one".
    #[test]
    fn the_same_id_with_different_outcomes_is_unverifiable_once_naming_both_files() {
        let s = Scratch::linking(
            "d-conflict",
            &[
                ("REQ-0001", "tests.test_probe::test_passes"),
                ("REQ-0002", "tests.test_probe::test_param[1]"),
            ],
        );
        let other = s.write(
            "other.xml",
            &pytest_case("tests.test_probe", "test_passes", "fail"),
        );
        let mixed = fixture(MIXED);
        let out = s.gate(&["--test-report", &mixed, "--test-report", &other]);
        assert_eq!(out.code, 3, "{}", out.all());
        assert!(
            out.stdout.contains("tests.test_probe::test_passes"),
            "{}",
            out.stdout
        );
        assert!(
            out.stdout.contains(&mixed),
            "the first file: {}",
            out.stdout
        );
        assert!(
            out.stdout.contains(&other),
            "the second file: {}",
            out.stdout
        );
        assert_eq!(
            out.stdout.matches("cannot be read").count(),
            1,
            "judged once, not per claim: {}",
            out.stdout
        );
        // Neither order lets one outcome win.
        let swapped = s.gate(&["--test-report", &other, "--test-report", &mixed]);
        assert_eq!(swapped.code, 3, "{}", swapped.all());
    }

    /// (d) One unreadable report makes the evidence UNVERIFIABLE once, naming the file — even though
    /// the other report alone would have passed: a test absent from the union might be in the one that
    /// failed to load.
    #[test]
    fn one_unreadable_report_is_unverifiable_even_if_the_other_would_pass() {
        let s = Scratch::linking(
            "d-unreadable",
            &[
                ("REQ-0001", "tests.test_probe::test_passes"),
                ("REQ-0002", "tests.test_probe::test_param[1]"),
            ],
        );
        let mixed = fixture(MIXED);
        assert_eq!(
            s.gate(&["--test-report", &mixed]).code,
            0,
            "setup: the readable report alone passes"
        );
        let missing = s.0.join("not-there.xml").display().to_string();
        let broken = s.write("broken.xml", "<testsuites><testcase");
        for bad in [&missing, &broken] {
            for args in [
                ["--test-report", &mixed, "--test-report", bad],
                ["--test-report", bad, "--test-report", &mixed],
            ] {
                let out = s.gate(&args);
                assert_eq!(out.code, 3, "{args:?}: {}", out.all());
                assert!(
                    out.stdout.contains(bad.as_str()),
                    "{args:?}: names it: {}",
                    out.stdout
                );
                assert_eq!(
                    out.stdout.matches("cannot be read").count(),
                    1,
                    "{args:?}: once, not per claim: {}",
                    out.stdout
                );
            }
        }
    }

    /// (d) An empty report beside one with cases is accepted; two empty reports are "carries no test
    /// cases"; the same path twice is accepted.
    #[test]
    fn an_empty_member_is_accepted_two_empty_ones_carry_no_cases_and_a_path_twice_is_harmless() {
        let s = Scratch::linking("d-empty", &[("REQ-0001", "tests.test_probe::test_passes")]);
        let empty_a = s.write("empty-a.xml", "<testsuites/>");
        let empty_b = s.write("empty-b.xml", "<testsuites name=\"pytest tests\"/>");
        let mixed = fixture(MIXED);

        let one_empty = s.gate(&["--test-report", &empty_a, "--test-report", &mixed]);
        assert_eq!(one_empty.code, 0, "{}", one_empty.all());

        let both_empty = s.gate(&["--test-report", &empty_a, "--test-report", &empty_b]);
        assert_eq!(both_empty.code, 3, "{}", both_empty.all());
        assert!(
            both_empty.stdout.contains("carries no test cases"),
            "{}",
            both_empty.stdout
        );

        let twice = s.gate(&["--test-report", &mixed, "--test-report", &mixed]);
        assert_eq!(twice.code, 0, "{}", twice.all());
    }

    /// A report path is the operator's own argument and is read as given, so a report may arrive on a
    /// pipe — `cat report.xml | req gate --test-report /dev/stdin` — as it did before several reports
    /// were allowed; a directory is still a named unreadable report (exit 3).
    #[cfg(unix)]
    #[test]
    fn a_report_may_arrive_on_a_pipe_and_a_directory_is_named_unreadable() {
        let s = Scratch::linking("d-pipe", &[("REQ-0001", "tests.test_probe::test_passes")]);
        let report = std::fs::read(fixture(MIXED)).expect("reads the pytest fixture");
        let mut child = Command::new(req_bin())
            .args(["gate", "--test-report", "/dev/stdin"])
            .current_dir(&s.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawns req");
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("has stdin");
            stdin.write_all(&report).expect("writes the report");
        }
        let out = child.wait_with_output().expect("collects the output");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(out.status.code(), Some(0), "{text}");

        let dir = s.0.display().to_string();
        let out = s.gate(&["--test-report", &dir]);
        assert_eq!(out.code, 3, "{}", out.all());
        assert!(out.stdout.contains(&dir), "{}", out.stdout);
    }

    // ---------------------------------------------------------------------------
    // (f) usage
    // ---------------------------------------------------------------------------

    /// (f) `--test-report` with no value, an empty value or an option-shaped value is exit 2 with the
    /// usage line, in either spelling and wherever the repeat falls — before any report is read.
    #[test]
    fn test_report_usage_errors_stay_exit_2() {
        let s = Scratch::linking("f-usage", &[("REQ-0001", "tests.test_probe::test_passes")]);
        let mixed = fixture(MIXED);
        for args in [
            vec!["--test-report"],
            vec!["--test-report="],
            vec!["--test-report", ""],
            vec!["--test-report", "-x"],
            vec!["--test-report=-x"],
            vec!["--test-report", &mixed, "--test-report"],
            vec!["--test-report", &mixed, "--test-report="],
            vec!["--test-report", &mixed, "--test-report", ""],
            vec!["--test-report", &mixed, "--test-report=--x"],
        ] {
            let out = s.gate(&args);
            assert_eq!(out.code, 2, "{args:?}: {}", out.all());
            assert!(
                out.stderr.contains("usage: req gate"),
                "{args:?}: {}",
                out.stderr
            );
            assert!(
                out.stdout.is_empty(),
                "{args:?}: nothing was judged: {}",
                out.stdout
            );
        }
    }
}
