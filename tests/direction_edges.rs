//! Direction classification, edge by edge (the survivors of the first mutation run, `docs/research/mutation-testing.md`).
//!
//! Mutating `compare.rs` showed that the direction tests agree with the code on the common cases and say nothing about
//! the boundaries: that the opposite of a direction is its mirror, which classes count as loosening, that one unparseable
//! side is enough to refuse a comparison, that a qualifier or a unit alone makes a bound a different thing, that an
//! exact value is not a bound, that two families of bound are incomparable, and that a change which tightens one way
//! and loosens the other is no direction at all. Each expectation below is written from the definitions in
//! `compare.rs`'s own doc and `docs/canonical-hashing.md`, never read back from a run.

use req::canon::{CanonicalForm, canonicalize_statement};
use req::compare::{Direction, compare};
use req::model::Registry;

fn form(statement: &str) -> CanonicalForm {
    canonicalize_statement(statement, &Registry::empty()).0
}

fn dir(a: &str, b: &str) -> &'static str {
    compare(&form(a), &form(b)).as_str()
}

const ALL: [Direction; 11] = [
    Direction::Identical,
    Direction::ModalityWeakening,
    Direction::ModalityStrengthening,
    Direction::PolarityFlip,
    Direction::NumericLoosening,
    Direction::NumericTightening,
    Direction::MembershipNarrowing,
    Direction::MembershipWidening,
    Direction::ScopeNarrowing,
    Direction::ScopeWidening,
    Direction::ScopeDivergence,
];

#[test]
fn the_opposite_of_a_direction_is_its_mirror_and_only_axes_have_one() {
    use Direction::*;
    for (a, b) in [
        (ModalityWeakening, ModalityStrengthening),
        (NumericLoosening, NumericTightening),
        (MembershipNarrowing, MembershipWidening),
        (ScopeNarrowing, ScopeWidening),
    ] {
        assert_eq!(a.opposite(), Some(b), "{a:?}");
        assert_eq!(b.opposite(), Some(a), "{b:?}");
    }
    for none in [
        Identical,
        PolarityFlip,
        ScopeDivergence,
        Unrelated,
        Unclassifiable,
    ] {
        assert_eq!(none.opposite(), None, "{none:?} has no mirror");
    }
}

#[test]
fn exactly_four_directions_permit_more_than_before() {
    use Direction::*;
    let loosening: Vec<Direction> = ALL
        .iter()
        .chain([Unrelated, Unclassifiable].iter())
        .copied()
        .filter(Direction::is_loosening)
        .collect();
    assert_eq!(
        loosening,
        [
            ModalityWeakening,
            NumericLoosening,
            MembershipWidening,
            ScopeWidening
        ]
    );
}

#[test]
fn a_loosening_read_as_no_change_or_as_its_opposite_is_the_dangerous_confusion() {
    use Direction::*;
    // The gate's definition: a safety-relevant expectation reported as no change, or as the opposite direction.
    for (expected, opposite) in [
        (ModalityWeakening, ModalityStrengthening),
        (NumericLoosening, NumericTightening),
        (MembershipWidening, MembershipNarrowing),
        (ScopeWidening, ScopeNarrowing),
    ] {
        assert!(
            Direction::is_dangerous_confusion(expected, Identical),
            "{expected:?} read as no change"
        );
        assert!(
            Direction::is_dangerous_confusion(expected, opposite),
            "{expected:?} read as {opposite:?}"
        );
        assert!(
            !Direction::is_dangerous_confusion(expected, ScopeDivergence),
            "{expected:?} read as divergence is harmless"
        );
        assert!(
            !Direction::is_dangerous_confusion(expected, expected),
            "{expected:?} read correctly"
        );
    }
    assert!(
        !Direction::is_dangerous_confusion(Identical, ModalityWeakening),
        "no movement was claimed"
    );
    assert!(
        !Direction::is_dangerous_confusion(Unrelated, Identical),
        "an unrelated pair claims no movement"
    );
}

#[test]
fn one_unparseable_side_is_enough_to_refuse_a_comparison() {
    let structured = "The system shall respond within 200 ms.";
    let raw = "Authoring shall be friction-free.";
    assert_eq!(
        dir(structured, raw),
        "unclassifiable",
        "the second side has no structure"
    );
    assert_eq!(
        dir(raw, structured),
        "unclassifiable",
        "the first side has no structure"
    );
}

#[test]
fn a_different_subject_or_a_different_predicate_is_unrelated() {
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The charger shall respond within 200 ms."
        ),
        "unrelated"
    );
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall reset within 200 ms."
        ),
        "unrelated"
    );
}

#[test]
fn a_qualifier_alone_or_a_unit_alone_makes_the_bound_a_different_thing_even_when_the_number_moved_too()
 {
    // Same unit, different qualifier, and the number ALSO moved: still a different thing, never a tightening.
    assert_eq!(
        dir(
            "The system shall draw at most 5 A of standby current.",
            "The system shall draw at most 3 A of surge current."
        ),
        "scope_divergence"
    );
    // Same qualifier (none), different unit, and the number also moved: a frequency is not a time.
    assert_eq!(
        dir(
            "The system shall respond within 5 ms.",
            "The system shall respond within 3 hz."
        ),
        "scope_divergence"
    );
    // The number is the same and only the unit differs.
    assert_eq!(
        dir(
            "The system shall draw at most 5 m.",
            "The system shall draw at most 5 s."
        ),
        "scope_divergence"
    );
}

#[test]
fn the_strict_comparator_tightens_and_back_loosens_for_both_families() {
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond in less than 200 ms."
        ),
        "numeric_tightening"
    );
    assert_eq!(
        dir(
            "The system shall respond in less than 200 ms.",
            "The system shall respond within 200 ms."
        ),
        "numeric_loosening"
    );
    assert_eq!(
        dir(
            "The system shall store at least 10 records.",
            "The system shall store more than 10 records."
        ),
        "numeric_tightening"
    );
    assert_eq!(
        dir(
            "The system shall store more than 10 records.",
            "The system shall store at least 10 records."
        ),
        "numeric_loosening"
    );
}

#[test]
fn a_bound_that_moves_is_judged_by_the_family_of_its_comparator() {
    // An upper bound grows: more complies. A lower bound grows: fewer complies.
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond within 300 ms."
        ),
        "numeric_loosening"
    );
    assert_eq!(
        dir(
            "The system shall respond within 300 ms.",
            "The system shall respond within 200 ms."
        ),
        "numeric_tightening"
    );
    assert_eq!(
        dir(
            "The system shall store at least 10 records.",
            "The system shall store at least 20 records."
        ),
        "numeric_tightening"
    );
    assert_eq!(
        dir(
            "The system shall store at least 20 records.",
            "The system shall store at least 10 records."
        ),
        "numeric_loosening"
    );
}

/// A form with a comparator and no value. The coverage rule (T1.10) keeps statements like "within a single window" from
/// being structured today, so this state is reachable only by building the form: the arm still decides it, and the
/// direction gold set once found 7 pairs wrongly read as divergence here.
fn bound_without_number(op: req::canon::Comp) -> CanonicalForm {
    let mut f = form("The system shall respond within 200 ms.");
    f.operator = Some(op);
    f.value = None;
    f.unit = None;
    f
}

#[test]
fn a_bound_with_no_number_still_has_a_direction_from_its_comparator() {
    use req::canon::Comp::{Le, Lt};
    assert_eq!(
        compare(&bound_without_number(Le), &bound_without_number(Lt)),
        Direction::NumericTightening
    );
    assert_eq!(
        compare(&bound_without_number(Lt), &bound_without_number(Le)),
        Direction::NumericLoosening
    );
}

#[test]
fn an_exact_value_is_not_a_bound_on_either_side() {
    let exact = "The system shall store exactly 10 records.";
    assert_eq!(
        dir(exact, "The system shall store at most 10 records."),
        "scope_divergence",
        "exact against a bound"
    );
    assert_eq!(
        dir("The system shall store at most 10 records.", exact),
        "scope_divergence",
        "a bound against exact"
    );
    assert_eq!(
        dir(exact, "The system shall store exactly 12 records."),
        "scope_divergence",
        "changing an exact value changes what is required"
    );
}

#[test]
fn an_upper_and_a_lower_bound_are_incomparable_whatever_their_strictness() {
    let le = "The system shall store at most 10 records.";
    let lt = "The system shall store fewer than 10 records.";
    let ge = "The system shall store at least 10 records.";
    let gt = "The system shall store more than 10 records.";
    for (a, b) in [(le, ge), (le, gt), (lt, ge), (lt, gt), (ge, le), (gt, lt)] {
        assert_eq!(dir(a, b), "scope_divergence", "{a:?} -> {b:?}");
    }
}

#[test]
fn a_comparator_that_appears_or_a_bound_that_appears_is_a_different_requirement() {
    let bounded = form("The system shall store at most 10 records.");
    let mut bare = bounded.clone();
    bare.operator = None;
    bare.value = None;
    bare.unit = None;
    assert_eq!(
        compare(&bare, &bounded),
        Direction::ScopeDivergence,
        "a comparator appears"
    );
    assert_eq!(
        compare(&bounded, &bare),
        Direction::ScopeDivergence,
        "a comparator disappears"
    );
    // A comparator on both sides, but a number on one side only.
    let numberless = bound_without_number(req::canon::Comp::Le);
    let numbered = form("The system shall respond within 200 ms.");
    assert_eq!(
        compare(&numberless, &numbered),
        Direction::ScopeDivergence,
        "a number appears where there was none"
    );
    assert_eq!(
        compare(&numbered, &numberless),
        Direction::ScopeDivergence,
        "a number disappears"
    );
}

#[test]
fn a_change_that_tightens_one_way_and_loosens_the_other_is_no_direction() {
    // LE to LT tightens the comparator, while 200 to 300 loosens the value.
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond in less than 300 ms."
        ),
        "scope_divergence"
    );
    // LT to LE loosens the comparator, while 300 to 200 tightens the value.
    assert_eq!(
        dir(
            "The system shall respond in less than 300 ms.",
            "The system shall respond within 200 ms."
        ),
        "scope_divergence"
    );
    // Both move the same way: a direction.
    assert_eq!(
        dir(
            "The system shall respond within 300 ms.",
            "The system shall respond in less than 200 ms."
        ),
        "numeric_tightening"
    );
    assert_eq!(
        dir(
            "The system shall respond in less than 200 ms.",
            "The system shall respond within 300 ms."
        ),
        "numeric_loosening"
    );
}

#[test]
fn an_exception_or_a_temporal_slot_that_differs_alone_is_divergence_never_identical() {
    // The parser derives neither slot, so the only way to meet this arm is a form built by hand: the arm exists so
    // that a future slot cannot pass silently.
    let base = form("The system shall respond within 200 ms.");
    let mut with_exception = base.clone();
    with_exception.exception = Some("except in maintenance".to_owned());
    let mut with_temporal = base.clone();
    with_temporal.temporal = Some("every 5 s".to_owned());
    assert_eq!(compare(&base, &with_exception), Direction::ScopeDivergence);
    assert_eq!(compare(&with_exception, &base), Direction::ScopeDivergence);
    assert_eq!(compare(&base, &with_temporal), Direction::ScopeDivergence);
    assert_eq!(compare(&with_temporal, &base), Direction::ScopeDivergence);
    assert_eq!(
        compare(&with_exception, &with_exception),
        Direction::Identical
    );
}

// ---------------------------------------------------------------------------------------------
// `req compare`, the command: what it prints beside the class
// ---------------------------------------------------------------------------------------------

mod cli {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const REGISTRY: &str = "schema: req.subject_registry/0\nversion: 1\nsubjects:\n  SYSTEM:\n    label: The system\n    aliases:\n      - the system\n";

    fn claim(id: &str, statement: &str, pins: &str) -> String {
        let statement = serde_json::to_string(statement).expect("a JSON string");
        format!(
            "schema: req.claim/2\nclaim_id: {id}\nstatement: {statement}\napplicability: []\nrationale:\n  source: tests/direction_edges.rs\n  date: 2026-11-10\n  rejected_alternatives: []\n  assumptions: []\nrelations: []\n{pins}"
        )
    }

    struct Corpus(PathBuf);

    impl Corpus {
        fn new(name: &str) -> Corpus {
            let dir =
                std::env::temp_dir().join(format!("req-direction-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("claims")).expect("creates");
            fs::create_dir_all(dir.join("aliases")).expect("creates");
            fs::write(dir.join("aliases/subjects.yml"), REGISTRY).expect("writes");
            Corpus(dir)
        }

        fn write(&self, file: &str, text: &str) -> PathBuf {
            let path = self.0.join(file);
            fs::write(&path, text).expect("writes");
            path
        }

        fn compare(&self, a: &Path, b: &Path) -> (i32, String) {
            let out = Command::new(env!("CARGO_BIN_EXE_req"))
                .arg("--root")
                .arg(&self.0)
                .arg("compare")
                .arg(a)
                .arg(b)
                .env_remove("REQ_ROOT")
                .output()
                .expect("runs req");
            (
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stdout).into_owned(),
            )
        }
    }

    impl Drop for Corpus {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const TIGHT: &str = "The system shall respond within 200 ms.";
    const LOOSE: &str = "The system shall respond within 500 ms.";

    #[test]
    fn a_loosening_says_it_permits_strictly_more_and_a_tightening_does_not() {
        let c = Corpus::new("loosen");
        let tight = c.write("a.yml", &claim("REQ-9001", TIGHT, ""));
        let loose = c.write("b.yml", &claim("REQ-9001", LOOSE, ""));
        let (code, out) = c.compare(&tight, &loose);
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("numeric_loosening"), "{out}");
        assert!(
            out.contains("this change permits strictly more than before"),
            "{out}"
        );
        let (code, out) = c.compare(&loose, &tight);
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("numeric_tightening"), "{out}");
        assert!(
            !out.contains("permits strictly more"),
            "a tightening permits less: {out}"
        );
    }

    #[test]
    fn a_pin_that_disagrees_is_noted_beside_the_class_and_only_then() {
        let c = Corpus::new("pins");
        let clean = c.write("clean.yml", &claim("REQ-9001", TIGHT, ""));
        let agreeing = c.write(
            "agree.yml",
            &claim("REQ-9001", LOOSE, "pins:\n  operator: LE\n"),
        );
        let flagged = c.write(
            "flag.yml",
            &claim("REQ-9001", LOOSE, "pins:\n  operator: LT\n"),
        );
        let (_, out) = c.compare(&clean, &agreeing);
        assert!(
            !out.contains("note: a pin disagrees"),
            "no pin disagrees: {out}"
        );
        let (_, out) = c.compare(&clean, &flagged);
        assert!(
            out.contains("note: a pin disagrees with its claim's parse (0 / 1 flagged)"),
            "the second side is flagged: {out}"
        );
        let (_, out) = c.compare(&flagged, &clean);
        assert!(
            out.contains("(1 / 0 flagged)"),
            "the first side is flagged: {out}"
        );
    }

    #[test]
    fn an_unstructured_side_is_exit_3_and_says_there_is_nothing_to_compare() {
        let c = Corpus::new("raw");
        let structured = c.write("a.yml", &claim("REQ-9001", TIGHT, ""));
        let raw = c.write(
            "b.yml",
            &claim("REQ-9001", "Authoring shall be friction-free.", ""),
        );
        let (code, out) = c.compare(&structured, &raw);
        assert_eq!(code, 3, "{out}");
        assert!(out.contains("nothing to compare"), "{out}");
    }
}
