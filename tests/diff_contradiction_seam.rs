//! ADR 0045 (unit D-1): the pure seams under `req diff`'s `contradictions:` section.
//!
//! No git and no process: the delta between two sides' findings, which claims a delta flags and what
//! the block prints are decided by values this file writes down by hand. The admission is a
//! **parameter** of the seams that need it, because `contradict::DETECTOR_ADMITTED` is `true` today
//! and a test cannot flip a constant (ADR 0045 decision 4, falsification 3).

use req::contradict::{Contradiction, Findings};
use req::diff::{SelectionFindings, contradiction_delta, contradictions_block, flagged_claims};

fn contradiction(a: &str, b: &str, recorded: bool) -> Contradiction {
    Contradiction {
        a_id: a.to_string(),
        b_id: b.to_string(),
        slot: "bounds",
        a: "SHALL LE 100 ms".to_string(),
        b: "SHALL GE 200 ms".to_string(),
        recorded,
    }
}

fn selection(line: &str, found: &[Contradiction]) -> SelectionFindings {
    SelectionFindings {
        line: line.to_string(),
        findings: Findings {
            contradictions: found.to_vec(),
            ..Findings::default()
        },
    }
}

#[test]
fn a_pair_held_on_both_sides_under_one_line_is_neither_added_nor_resolved() {
    let held = contradiction("REQ-0001", "REQ-0002", false);
    let delta = contradiction_delta(
        &[selection("release=1", std::slice::from_ref(&held))],
        &[selection("release=1", &[held])],
    );
    assert!(
        delta.added.is_empty() && delta.resolved.is_empty(),
        "{delta:?}"
    );
}

#[test]
fn a_pair_only_at_head_is_added_and_one_only_at_base_is_resolved_each_naming_its_line() {
    let pair = contradiction("REQ-0001", "REQ-0002", false);
    let delta = contradiction_delta(
        &[selection("release=1", &[])],
        &[selection("release=1", std::slice::from_ref(&pair))],
    );
    assert_eq!(delta.added.len(), 1, "{delta:?}");
    assert!(delta.resolved.is_empty());
    let added = &delta.added[0];
    assert_eq!(
        (
            added.line.as_str(),
            added.contradiction.a_id.as_str(),
            added.contradiction.b_id.as_str(),
            added.contradiction.slot
        ),
        ("release=1", "REQ-0001", "REQ-0002", "bounds")
    );
    assert_eq!(
        (
            added.contradiction.a.as_str(),
            added.contradiction.b.as_str()
        ),
        ("SHALL LE 100 ms", "SHALL GE 200 ms")
    );

    let delta = contradiction_delta(
        &[selection("release=1", &[pair])],
        &[selection("release=1", &[])],
    );
    assert_eq!(delta.resolved.len(), 1, "{delta:?}");
    assert!(delta.added.is_empty());
}

#[test]
fn the_same_pair_under_another_line_is_a_different_pair() {
    let pair = contradiction("REQ-0001", "REQ-0002", false);
    let delta = contradiction_delta(
        &[
            selection("release=1", std::slice::from_ref(&pair)),
            selection("release=2", &[]),
        ],
        &[
            selection("release=1", std::slice::from_ref(&pair)),
            selection("release=2", &[pair]),
        ],
    );
    assert_eq!(delta.added.len(), 1, "{delta:?}");
    assert_eq!(delta.added[0].line, "release=2");
    assert!(delta.resolved.is_empty());
}

#[test]
fn a_pair_recorded_at_head_is_resolved_never_added() {
    // Falsification 4. Outstanding (unrecorded) at base, recorded at head: resolved.
    let open = contradiction("REQ-0001", "REQ-0002", false);
    let recorded = contradiction("REQ-0001", "REQ-0002", true);
    let delta = contradiction_delta(
        &[selection("release=1", &[open])],
        &[selection("release=1", std::slice::from_ref(&recorded))],
    );
    assert!(delta.added.is_empty(), "{delta:?}");
    assert_eq!(delta.resolved.len(), 1, "{delta:?}");
    // Absent at base and recorded at head: nothing was outstanding, so nothing resolved or added.
    let delta = contradiction_delta(
        &[selection("release=1", &[])],
        &[selection("release=1", &[recorded])],
    );
    assert!(
        delta.added.is_empty() && delta.resolved.is_empty(),
        "{delta:?}"
    );
}

#[test]
fn a_recorded_pair_on_both_sides_is_not_news_and_an_unrecorded_one_after_a_record_is_added() {
    let open = contradiction("REQ-0001", "REQ-0002", false);
    let recorded = contradiction("REQ-0001", "REQ-0002", true);
    let delta = contradiction_delta(
        &[selection("release=1", std::slice::from_ref(&recorded))],
        &[selection("release=1", std::slice::from_ref(&recorded))],
    );
    assert!(
        delta.added.is_empty() && delta.resolved.is_empty(),
        "{delta:?}"
    );
    // The Conflict record was deleted: the contradiction is outstanding again, and new to the gate.
    let delta = contradiction_delta(
        &[selection("release=1", &[recorded])],
        &[selection("release=1", &[open])],
    );
    assert_eq!(delta.added.len(), 1, "{delta:?}");
}

#[test]
fn pairs_are_sorted_by_the_lines_position_then_the_ids() {
    let delta = contradiction_delta(
        &[selection("zeta=1", &[]), selection("alpha=1", &[])],
        &[
            selection(
                "zeta=1",
                &[
                    contradiction("REQ-0003", "REQ-0004", false),
                    contradiction("REQ-0001", "REQ-0009", false),
                ],
            ),
            selection("alpha=1", &[contradiction("REQ-0002", "REQ-0005", false)]),
        ],
    );
    let order: Vec<(&str, &str)> = delta
        .added
        .iter()
        .map(|p| (p.line.as_str(), p.contradiction.a_id.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            ("zeta=1", "REQ-0001"),
            ("zeta=1", "REQ-0003"),
            ("alpha=1", "REQ-0002")
        ],
        "the list's own order, then the ids"
    );
}

/// Falsification 3: nothing is flagged while the detector is not admitted, and both ids are when it is.
#[test]
fn the_flag_is_raised_only_when_the_detector_is_admitted() {
    let delta = contradiction_delta(
        &[selection("release=1", &[])],
        &[selection(
            "release=1",
            &[contradiction("REQ-0001", "REQ-0002", false)],
        )],
    );
    assert_eq!(delta.added.len(), 1);
    assert!(
        flagged_claims(&delta, false).is_empty(),
        "an unadmitted detector raises no flag"
    );
    let flagged = flagged_claims(&delta, true);
    assert_eq!(flagged.len(), 2, "{flagged:?}");
    assert_eq!(
        flagged["REQ-0001"].iter().collect::<Vec<_>>(),
        ["REQ-0002"],
        "each side names the other"
    );
    assert_eq!(flagged["REQ-0002"].iter().collect::<Vec<_>>(), ["REQ-0001"]);
    // A resolved pair never flags.
    let resolved = contradiction_delta(
        &[selection(
            "release=1",
            &[contradiction("REQ-0001", "REQ-0002", false)],
        )],
        &[selection("release=1", &[])],
    );
    assert!(flagged_claims(&resolved, true).is_empty());
}

#[test]
fn the_block_says_reported_not_judged_exactly_while_the_detector_is_not_admitted() {
    let delta = contradiction_delta(
        &[selection("release=1", &[])],
        &[selection(
            "release=1",
            &[contradiction("REQ-0001", "REQ-0002", false)],
        )],
    );
    let admitted = contradictions_block(&delta, 1, true);
    let reported = contradictions_block(&delta, 1, false);
    assert!(!admitted.contains("reported, not judged"), "{admitted}");
    assert!(reported.contains("(reported, not judged)"), "{reported}");
    for block in [&admitted, &reported] {
        assert!(
            block.contains("added [release=1] REQ-0001 vs REQ-0002: bounds — SHALL LE 100 ms against SHALL GE 200 ms"),
            "{block}"
        );
        assert!(
            block.starts_with("contradictions: 1 added, 0 resolved across 1 selection(s)\n"),
            "{block}"
        );
    }
    let empty = contradictions_block(&contradiction_delta(&[], &[]), 3, true);
    assert_eq!(
        empty,
        "contradictions: none added or resolved across 3 selection(s)\n"
    );
}
