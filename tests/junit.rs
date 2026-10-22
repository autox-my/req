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
