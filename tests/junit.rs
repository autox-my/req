//! The JUnit reader: the test report is evidence, and evidence has three states.
//!
//! The fixtures mirror what `cargo-nextest 0.9.149` actually writes with
//! `[profile.ci.junit] report-skipped = "ignored"` — a `<testsuites>` root, one
//! `<testsuite>` per binary, self-closing `<testcase>` elements for passes, a
//! `<failure>` child (with `<system-out>`/`<system-err>` siblings) for a failure, and a
//! `<skipped>` child for an ignored test. The malformed fixtures are the shapes the
//! reader must refuse rather than read as "nothing failed".
//!
//! These tests go through the public surface only: a claim links a test by the id the
//! report names, so an id that is not exactly `classname::name` is a link that silently
//! stops resolving.

use req::junit::{Report, TestOutcome};
use std::path::PathBuf;

/// The measured all-pass shape, copied from a real nextest run.
const ALL_PASS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" skipped="0" failures="0" errors="0" uuid="f4d6cdc3-0c8e-47fe-994f-7fa25f931208" timestamp="2026-10-22T17:03:39.177+08:00" time="0.047">
    <testsuite name="req::gate" tests="3" skipped="0" errors="0" failures="0">
        <testcase name="exit_code_maps_the_three_outcomes" classname="req::gate" timestamp="2026-10-22T17:03:39.179+08:00" time="0.018"/>
        <testcase name="fmr_gate_truth_table" classname="req::gate" timestamp="2026-10-22T17:03:39.180+08:00" time="0.020"/>
        <testcase name="a_case_from_another_binary" classname="req::other" timestamp="2026-10-22T17:03:39.181+08:00" time="0.021"/>
    </testsuite>
</testsuites>
"#;

/// The measured mixed shape: a pass, a failure with its captured output, a process error,
/// and a skipped test (which nextest emits only when `report-skipped` opts in).
const MIXED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="4" skipped="1" failures="1" errors="1" uuid="b5fd4ce1-af31-4d0d-bb36-b8c9f5ae6f5a" timestamp="2026-10-22T17:06:53.793+08:00" time="0.016">
    <testsuite name="probe::probe" tests="4" skipped="1" errors="1" failures="1">
        <testcase name="passes" classname="probe::probe" timestamp="2026-10-22T17:06:53.794+08:00" time="0.013"/>
        <testcase name="fails" classname="probe::probe" timestamp="2026-10-22T17:06:53.794+08:00" time="0.013">
            <failure message="thread &apos;fails&apos; (14140694) panicked at tests/probe.rs:5:14" type="test failure with exit code 101">assertion `left == right` failed: deliberate failure &amp; &lt;tag&gt;</failure>
            <system-out>
running 1 test
test fails ... FAILED
</system-out>
            <system-err>
thread &apos;fails&apos; panicked at tests/probe.rs:5:14
</system-err>
        </testcase>
        <testcase name="errors" classname="probe::probe" timestamp="2026-10-22T17:06:53.795+08:00" time="0.001">
            <error message="process exited with signal: 11 (SIGSEGV)"/>
        </testcase>
        <testcase name="skipped" classname="probe::probe" time="0.000">
            <skipped message="Skipped: test does not match the run-ignored option"/>
        </testcase>
    </testsuite>
</testsuites>
"#;

fn parse_ok(input: &str) -> Report {
    Report::parse(input).unwrap_or_else(|reason| panic!("must parse: {reason}\n{input}"))
}

fn parse_err(input: &str) -> String {
    match Report::parse(input) {
        Ok(report) => panic!(
            "must be rejected, got {} test(s): {input}",
            report.test_count()
        ),
        Err(reason) => reason,
    }
}

#[test]
fn an_all_pass_report_marks_every_case_passed() {
    let report = parse_ok(ALL_PASS);
    assert_eq!(report.test_count(), 3);
    assert!(!report.is_empty());
    assert_eq!(
        report.outcome("req::gate::exit_code_maps_the_three_outcomes"),
        Some(TestOutcome::Passed)
    );
    assert_eq!(
        report.outcome("req::gate::fmr_gate_truth_table"),
        Some(TestOutcome::Passed)
    );
    assert_eq!(
        report.outcome("req::other::a_case_from_another_binary"),
        Some(TestOutcome::Passed)
    );
    // A test the report does not name is absent — the caller's UNVERIFIABLE, not a pass.
    assert_eq!(report.outcome("req::gate::not_in_this_report"), None);
}

#[test]
fn a_failure_child_is_failed_and_the_others_are_passed() {
    let report = parse_ok(MIXED);
    assert_eq!(
        report.outcome("probe::probe::fails"),
        Some(TestOutcome::Failed)
    );
    assert_eq!(
        report.outcome("probe::probe::passes"),
        Some(TestOutcome::Passed)
    );
}

#[test]
fn an_error_child_is_failed_too() {
    let report = parse_ok(MIXED);
    assert_eq!(
        report.outcome("probe::probe::errors"),
        Some(TestOutcome::Failed),
        "a <error> is a failure of the test, not a missing row"
    );
}

/// The reader must never lose the distinction: a skipped test is not a pass. What it means
/// for a requirement is the caller's judgement, but folding it here would take that away.
#[test]
fn a_skipped_case_is_skipped_and_never_folded_into_pass() {
    let report = parse_ok(MIXED);
    assert_eq!(
        report.outcome("probe::probe::skipped"),
        Some(TestOutcome::Skipped)
    );
    assert_ne!(
        report.outcome("probe::probe::skipped"),
        Some(TestOutcome::Passed)
    );
}

/// A zero-test run is a state, not an error and not a pass: the gate reports it
/// UNVERIFIABLE, and that is a different sentence from "the report is unreadable".
#[test]
fn a_report_with_no_testcases_is_ok_and_empty_not_an_error() {
    // The exact shape a filtered nextest run writes: a root with no children at all.
    let measured = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="0" skipped="0" failures="0" errors="0" uuid="457647fd-cebc-4ac5-9fd1-308eee0ea665" timestamp="2026-10-22T17:04:20.229+08:00" time="0.000">
</testsuites>
"#;
    for input in [
        measured,
        "<testsuites/>",
        r#"<testsuites name="nextest-run"></testsuites>"#,
        r#"<testsuites><testsuite name="s" tests="0"/></testsuites>"#,
    ] {
        let report = parse_ok(input);
        assert!(report.is_empty(), "must be empty: {input}");
        assert_eq!(report.test_count(), 0);
        assert_eq!(report.outcome("anything::at_all"), None);
    }
}

#[test]
fn a_single_testsuite_root_is_accepted() {
    let report = parse_ok(
        r#"<testsuite name="s"><testcase name="a" classname="s" time="0.1"/></testsuite>"#,
    );
    assert_eq!(report.outcome("s::a"), Some(TestOutcome::Passed));
}

#[test]
fn an_unclosed_testcase_is_an_error_not_a_partial_report() {
    let reason = parse_err(r#"<testsuites><testsuite name="s"><testcase name="a" classname="s">"#);
    assert!(reason.contains("testcase"), "{reason}");
}

#[test]
fn an_unterminated_attribute_quote_is_an_error() {
    let reason = parse_err(r#"<testsuites name="oops>"#);
    assert!(reason.contains("unterminated"), "{reason}");
}

#[test]
fn a_testcase_without_a_name_is_an_error() {
    let reason = parse_err(
        r#"<testsuites><testsuite name="s"><testcase classname="s" time="0.1"/></testsuite></testsuites>"#,
    );
    assert!(reason.contains("name"), "{reason}");
}

#[test]
fn a_testcase_without_a_classname_is_an_error() {
    // The id is `classname::name`; without a classname there is no stable identity to
    // look a claim's link up by, and inventing one would silently break every link.
    let reason = parse_err(
        r#"<testsuites><testsuite name="s"><testcase name="a" time="0.1"/></testsuite></testsuites>"#,
    );
    assert!(reason.contains("classname"), "{reason}");
}

#[test]
fn text_after_the_root_with_no_tag_is_an_error() {
    // A corrupt tail used to be consumed silently: `<testsuites/>garbage` parsed as a valid
    // (empty) report. A report whose tail cannot be parsed is not a result.
    let reason = parse_err(r#"<testsuites/>garbage"#);
    assert!(reason.contains("outside any element"), "{reason}");
}

#[test]
fn a_repeated_attribute_is_an_error_not_the_first_value() {
    // Keeping the first value would silently choose one of two identities.
    let reason = parse_err(
        r#"<testsuites><testsuite name="s"><testcase name="old" name="new" classname="s"/></testsuite></testsuites>"#,
    );
    assert!(reason.contains("twice"), "{reason}");
}

#[test]
fn an_empty_name_or_classname_is_an_error_not_an_id() {
    // Presence is not identity: `<testcase name="" classname="s"/>` would join the id space as
    // `s::`, a string no claim could link to intentionally. A report entry with no writable id is
    // not an entry.
    for xml in [
        r#"<testsuites><testsuite name="s"><testcase name="" classname="s"/></testsuite></testsuites>"#,
        r#"<testsuites><testsuite name="s"><testcase name="a" classname=""/></testsuite></testsuites>"#,
    ] {
        let reason = parse_err(xml);
        assert!(reason.contains("empty"), "{reason}");
    }
}

#[test]
fn junk_with_no_root_element_is_an_error() {
    for input in ["this is not a test report", "", "<testsuite", "<"] {
        let reason = parse_err(input);
        assert!(!reason.is_empty(), "the reader must say why: {input:?}");
    }
    // A root element outside the subset is refused, not silently emptied.
    let reason = parse_err(r#"<report><testsuite name="s"/></report>"#);
    assert!(
        reason.contains("root") || reason.contains("report"),
        "{reason}"
    );
}

#[test]
fn an_unterminated_comment_is_an_error() {
    let reason = parse_err("<!-- nothing closes this");
    assert!(reason.contains("comment"), "{reason}");
}

#[test]
fn identical_duplicate_rows_dedupe() {
    let report = parse_ok(
        r#"<testsuites><testsuite name="s">
            <testcase name="same" classname="s" time="0.1"/>
            <testcase name="same" classname="s" time="0.2"/>
        </testsuite></testsuites>"#,
    );
    assert_eq!(report.test_count(), 1);
    assert_eq!(report.outcome("s::same"), Some(TestOutcome::Passed));
}

/// Never silently pick a winner: a rename or a flake must not read as a chosen result.
#[test]
fn the_same_id_with_different_outcomes_is_an_error_naming_the_ambiguity() {
    let reason = parse_err(
        r#"<testsuites><testsuite name="s">
            <testcase name="flake" classname="s" time="0.1"/>
            <testcase name="flake" classname="s" time="0.2"><failure message="boom"/></testcase>
        </testsuite></testsuites>"#,
    );
    assert!(reason.contains("s::flake"), "{reason}");
    assert!(reason.contains("conflicting"), "{reason}");

    // Skipped-vs-passed is a conflict too: folding it would let a skipped test read as a
    // pass exactly where the gate looks for one.
    let reason = parse_err(
        r#"<testsuites><testsuite name="s">
            <testcase name="half" classname="s"/>
            <testcase name="half" classname="s"><skipped message="ignored"/></testcase>
        </testsuite></testsuites>"#,
    );
    assert!(reason.contains("s::half"), "{reason}");
}

#[test]
fn the_five_predefined_entities_and_numeric_references_decode_in_the_id() {
    let report = parse_ok(
        r#"<testsuites><testsuite name="entity">
            <testcase name="a&amp;b&lt;c&gt;d&quot;e&apos;f" classname="x&#65;y&#x41;z" time="0.1"/>
        </testsuite></testsuites>"#,
    );
    // classname "x&#65;y&#x41;z" -> xAyAz; name decodes the five predefined entities.
    assert_eq!(
        report.outcome("xAyAz::a&b<c>d\"e'f"),
        Some(TestOutcome::Passed),
        "the id must be the decoded text"
    );
    assert_eq!(report.test_count(), 1);
}

#[test]
fn an_unknown_entity_is_an_error_not_a_literal() {
    let reason = parse_err(
        r#"<testsuites><testsuite name="s"><testcase name="bad &bogus; entity" classname="s"/></testsuite></testsuites>"#,
    );
    assert!(reason.contains("bogus"), "{reason}");

    let reason = parse_err(
        r#"<testsuites><testsuite name="s"><testcase name="bad &#x110000; codepoint" classname="s"/></testsuite></testsuites>"#,
    );
    assert!(!reason.is_empty(), "{reason}");
}

/// Attribute values are read by their quotes, not by splitting on `>`: a parameterised
/// test name may contain one, and nextest's `failure message` is arbitrary text.
#[test]
fn an_attribute_value_may_contain_a_greater_than_sign() {
    let report = parse_ok(
        r#"<testsuites><testsuite name="params">
            <testcase name="case[value > 0]" classname="suite" time="0.1"/>
            <testcase classname="suite" name="name after > here" time="0.1"/>
        </testsuite></testsuites>"#,
    );
    assert_eq!(
        report.outcome("suite::case[value > 0]"),
        Some(TestOutcome::Passed)
    );
    assert_eq!(
        report.outcome("suite::name after > here"),
        Some(TestOutcome::Passed)
    );
    assert_eq!(report.test_count(), 2);
}

/// Captured output is not structure. Text that looks like markup — escaped or nested —
/// must never become a test row.
#[test]
fn system_out_and_system_err_are_ignored_even_when_they_look_like_xml() {
    let report = parse_ok(
        r#"<testsuites><testsuite name="s">
            <testcase name="real" classname="s">
                <system-out>&lt;testcase name="fake-escaped" classname="fake"/&gt;
raw-looking text: &lt;failure/&gt;</system-out>
                <system-err><testsuite name="ghost"><testcase name="fake-nested" classname="fake"/></testsuite></system-err>
            </testcase>
            <testcase name="real2" classname="s"><system-out/></testcase>
        </testsuite></testsuites>"#,
    );
    assert_eq!(report.test_count(), 2);
    assert_eq!(report.outcome("s::real"), Some(TestOutcome::Passed));
    assert_eq!(report.outcome("s::real2"), Some(TestOutcome::Passed));
    assert_eq!(report.outcome("fake::fake-escaped"), None);
    assert_eq!(report.outcome("fake::fake-nested"), None);
}

#[test]
fn comments_and_processing_instructions_are_tolerated() {
    let report = parse_ok(concat!(
        "\u{feff}",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- <testsuite name="ghost"><testcase name="fake" classname="fake"/></testsuite> -->
<testsuites>
    <!-- between elements -->
    <?nextest-extra value="x"?>
    <testsuite name="s">
        <!-- inside: <testcase name="fake2" classname="fake"/> -->
        <testcase name="real" classname="s" time="0.1"/>
    </testsuite>
</testsuites>
<!-- trailing -->
"#
    ));
    assert_eq!(report.test_count(), 1);
    assert_eq!(report.outcome("s::real"), Some(TestOutcome::Passed));
    assert_eq!(report.outcome("fake::fake"), None);
    assert_eq!(report.outcome("fake::fake2"), None);
}

/// The parser is total on `&str`: a truncated report is an `Err`, never a panic from an
/// out-of-bounds slice. Every byte boundary of every fixture is a cut point.
#[test]
fn truncating_any_fixture_is_an_error_not_a_panic() {
    for input in [ALL_PASS, MIXED] {
        for end in 0..=input.len() {
            if input.is_char_boundary(end) {
                let _ = Report::parse(&input[..end]);
            }
        }
    }
    for junk in [
        "",
        "<",
        "<!",
        "<!--",
        "&amp;",
        "\u{0}\u{feff}",
        "<testsuites name=",
        "<testcase",
        "</",
        "<!DOCTYPE testsuites>",
        "<testsuites><testsuite></testsuites>",
    ] {
        let _ = Report::parse(junk);
    }
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("req-junit-{}-{label}.xml", std::process::id()))
}

#[test]
fn load_reads_and_parses_a_report() {
    let path = temp_path("ok");
    std::fs::write(&path, ALL_PASS).expect("write the fixture");
    let report = Report::load(&path).expect("the fixture must load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(report.test_count(), 3);
    assert_eq!(
        report.outcome("req::gate::fmr_gate_truth_table"),
        Some(TestOutcome::Passed)
    );
}

#[test]
fn load_names_the_path_when_the_file_cannot_be_read() {
    let path = temp_path("missing");
    let _ = std::fs::remove_file(&path);
    let reason = Report::load(&path).expect_err("a missing report must be an error");
    assert!(reason.contains(&path.display().to_string()), "{reason}");
}

// ---------------------------------------------------------------------------
// pytest's `--junitxml` (issue #64, ADR 0024 §1). The fixtures are real pytest 9.2.0 output
// (`tests/fixtures/junit/README.md`); every expectation below is pytest's documented meaning of the
// case, written by hand, never read back from `req`.
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/junit")
        .join(name)
}

/// (a) `pytest-mixed.xml`: the ten cases, each with the outcome pytest documents for it — a pass,
/// the three parametrized ids (one with a space), a class method; a failure, a strict XPASS (a
/// failure by `xfail_strict`) and a setup error (an error is a failure here); `@skip` and a
/// non-strict `xfail` (both a skip).
#[test]
fn every_pytest_case_in_the_mixed_fixture_has_the_outcome_pytest_documents() {
    let report = Report::load(&fixture("pytest-mixed.xml")).expect("pytest's own output must load");
    let expected = [
        ("tests.test_probe::test_passes", TestOutcome::Passed),
        ("tests.test_probe::test_param[1]", TestOutcome::Passed),
        ("tests.test_probe::test_param[2]", TestOutcome::Passed),
        ("tests.test_probe::test_param[a b]", TestOutcome::Passed),
        (
            "tests.test_probe.TestGroup::test_in_class",
            TestOutcome::Passed,
        ),
        ("tests.test_probe::test_fails", TestOutcome::Failed),
        ("tests.test_probe::test_xpass_strict", TestOutcome::Failed),
        ("tests.test_probe::test_setup_error", TestOutcome::Failed),
        ("tests.test_probe::test_skipped", TestOutcome::Skipped),
        ("tests.test_probe::test_xfail", TestOutcome::Skipped),
    ];
    assert_eq!(
        report.test_count(),
        expected.len(),
        "no case more, none less"
    );
    for (id, outcome) in expected {
        assert_eq!(report.outcome(id), Some(outcome), "{id}");
    }
}

/// (a) `pytest-param-escapes.xml`: `&amp;`, `&lt;` and `&quot;` in a `name` attribute decode to the
/// characters a claim's `tests:` link would write.
#[test]
fn pytest_escapes_in_a_parametrized_name_decode_to_the_written_id() {
    let report =
        Report::load(&fixture("pytest-param-escapes.xml")).expect("pytest's own output must load");
    assert_eq!(report.test_count(), 3);
    for id in [
        "coll.test_param::test_p[a&b]",
        "coll.test_param::test_p[x<y]",
        "coll.test_param::test_p[q\"r]",
    ] {
        assert_eq!(report.outcome(id), Some(TestOutcome::Passed), "{id}");
    }
}

/// (b) pytest writes a collection error as a `<testcase classname="">` and the run was interrupted,
/// so nothing else is evidence: the whole report is refused (never read partially), the existing
/// wording stays, and the message names the cause.
#[test]
fn a_collection_error_report_is_refused_whole_and_the_message_says_why() {
    let path = fixture("pytest-collection-error.xml");
    let reason = Report::load(&path).expect_err("a collection error is not evidence");
    assert!(
        reason.contains("empty classname attribute"),
        "the existing refusal stays: {reason}"
    );
    assert!(
        reason.contains("collection error"),
        "the message names pytest's collection error as the likely cause: {reason}"
    );
    assert!(
        reason.contains("interrupted"),
        "…and that the run was interrupted, so nothing else is evidence: {reason}"
    );
    assert!(
        reason.contains(&path.display().to_string()),
        "the file is named: {reason}"
    );
}

// ---------------------------------------------------------------------------
// Several reports: the union (ADR 0024 §2)
// ---------------------------------------------------------------------------

fn one_case(classname: &str, name: &str, outcome: TestOutcome) -> String {
    let body = match outcome {
        TestOutcome::Passed => String::new(),
        TestOutcome::Failed => "<failure message=\"x\"/>".to_string(),
        TestOutcome::Skipped => "<skipped message=\"x\"/>".to_string(),
    };
    format!(
        "<testsuites><testsuite name=\"s\"><testcase classname=\"{classname}\" name=\"{name}\">{body}</testcase></testsuite></testsuites>"
    )
}

fn write_temp(label: &str, text: &str) -> PathBuf {
    let path = temp_path(label);
    std::fs::write(&path, text).expect("writes a fixture report");
    path
}

#[test]
fn merge_is_the_union_by_id() {
    let a = parse_ok(&one_case("a", "x", TestOutcome::Passed));
    let b = parse_ok(&one_case("b", "y", TestOutcome::Failed));
    let union = a.merge(b).expect("disjoint ids merge");
    assert_eq!(union.test_count(), 2);
    assert_eq!(union.outcome("a::x"), Some(TestOutcome::Passed));
    assert_eq!(union.outcome("b::y"), Some(TestOutcome::Failed));
}

#[test]
fn merge_dedupes_an_identical_outcome_and_refuses_a_different_one() {
    let passed = parse_ok(&one_case("a", "x", TestOutcome::Passed));
    let again = parse_ok(&one_case("a", "x", TestOutcome::Passed));
    let failed = parse_ok(&one_case("a", "x", TestOutcome::Failed));
    assert_eq!(
        passed
            .clone()
            .merge(again)
            .expect("identical dedupes")
            .test_count(),
        1
    );
    let reason = passed
        .merge(failed)
        .expect_err("a different outcome for one id is unusable");
    assert!(reason.contains("a::x"), "the id is named: {reason}");
}

#[test]
fn merging_an_empty_report_changes_nothing() {
    let a = parse_ok(&one_case("a", "x", TestOutcome::Passed));
    let empty = parse_ok("<testsuites/>");
    assert_eq!(a.clone().merge(empty.clone()).expect("merges"), a);
    assert_eq!(empty.clone().merge(a.clone()).expect("merges"), a);
    assert!(empty.clone().merge(empty).expect("merges").is_empty());
}

#[test]
fn load_many_reads_every_file_and_judges_the_union() {
    let a = write_temp("many-a", &one_case("a", "x", TestOutcome::Passed));
    let b = write_temp("many-b", &one_case("b", "y", TestOutcome::Skipped));
    let union = Report::load_many(&[a.as_path(), b.as_path()]).expect("the union loads");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    assert_eq!(union.test_count(), 2);
    assert_eq!(union.outcome("b::y"), Some(TestOutcome::Skipped));
}

#[test]
fn load_many_names_both_files_and_the_id_on_a_conflict() {
    let a = write_temp("conflict-a", &one_case("a", "x", TestOutcome::Passed));
    let b = write_temp("conflict-b", &one_case("a", "x", TestOutcome::Failed));
    let reason = Report::load_many(&[a.as_path(), b.as_path()])
        .expect_err("one id with two outcomes is unusable");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    assert!(reason.contains("a::x"), "{reason}");
    assert!(reason.contains(&a.display().to_string()), "{reason}");
    assert!(reason.contains(&b.display().to_string()), "{reason}");
}

#[test]
fn load_many_names_the_first_file_of_a_conflict_that_is_not_the_previous_one() {
    // The conflict is between the third file and the FIRST, with a clean file between them: the
    // union does not forget where an id came from.
    let a = write_temp("three-a", &one_case("a", "x", TestOutcome::Passed));
    let b = write_temp("three-b", &one_case("b", "y", TestOutcome::Passed));
    let c = write_temp("three-c", &one_case("a", "x", TestOutcome::Skipped));
    let reason = Report::load_many(&[a.as_path(), b.as_path(), c.as_path()])
        .expect_err("a conflict across files");
    for path in [&a, &b, &c] {
        let _ = std::fs::remove_file(path);
    }
    assert!(reason.contains(&a.display().to_string()), "{reason}");
    assert!(reason.contains(&c.display().to_string()), "{reason}");
    assert!(
        !reason.contains(&b.display().to_string()),
        "the innocent file is not blamed: {reason}"
    );
}

#[test]
fn load_many_refuses_the_whole_set_when_one_member_is_unreadable_and_names_it() {
    let good = write_temp("member-good", &one_case("a", "x", TestOutcome::Passed));
    let missing = temp_path("member-missing");
    let _ = std::fs::remove_file(&missing);
    let bad = write_temp("member-bad", "<testsuites><testcase");
    for order in [
        [good.as_path(), missing.as_path()],
        [missing.as_path(), good.as_path()],
    ] {
        let reason = Report::load_many(&order).expect_err("one member cannot be read");
        assert!(reason.contains(&missing.display().to_string()), "{reason}");
    }
    let reason = Report::load_many(&[good.as_path(), bad.as_path()]).expect_err("unparsable");
    let _ = std::fs::remove_file(&good);
    let _ = std::fs::remove_file(&bad);
    assert!(reason.contains(&bad.display().to_string()), "{reason}");
}

#[test]
fn load_many_accepts_an_empty_member_the_same_path_twice_and_an_all_empty_set() {
    let cases = write_temp("shape-cases", &one_case("a", "x", TestOutcome::Passed));
    let empty = write_temp("shape-empty", "<testsuites/>");
    let union = Report::load_many(&[empty.as_path(), cases.as_path()]).expect("empty + cases");
    assert_eq!(union.test_count(), 1);
    let twice = Report::load_many(&[cases.as_path(), cases.as_path()]).expect("same path twice");
    assert_eq!(twice.test_count(), 1);
    let none = Report::load_many(&[empty.as_path(), empty.as_path()]).expect("both empty");
    assert!(none.is_empty(), "an empty union is still an empty report");
    let _ = std::fs::remove_file(&cases);
    let _ = std::fs::remove_file(&empty);
}

#[test]
fn load_many_of_nothing_is_an_empty_report() {
    let none = Report::load_many(&[]).expect("no path, no report to refuse");
    assert!(none.is_empty());
}

#[test]
fn a_directory_is_not_a_report_and_is_named() {
    let dir = std::env::temp_dir();
    let reason = Report::load(&dir).expect_err("a directory is not a report");
    assert!(reason.contains(&dir.display().to_string()), "{reason}");
}
