//! Which direction of change may carry a satisfaction link (ADR 0040 decisions 3, 5 and 6), through
//! the library seam: the judging function and the compiled-in admitted set.
//!
//! The admitted set is *pinned to the measurement*: the measurement's record (`req bench` writes
//! `bench/results/direction-v2-rules-v<RULES_V>.json`) says which weakening classes had zero false
//! carries with enough support, and the constant must agree with it. A rules bump, or a
//! re-measurement that changes admission, fails here until the constant is changed on purpose. The
//! record is read by this test only — never at runtime, where an adopter does not have it.

use req::canon::RULES_V;
use req::compare::Direction;
use req::model::Registry;
use req::satisfy::{ADMITTED_CARRIES, Judgement, carries, judge};

const WEAKENING: [Direction; 3] = [
    Direction::ModalityWeakening,
    Direction::NumericLoosening,
    Direction::MembershipWidening,
];

const SCOPE: [Direction; 3] = [
    Direction::ScopeNarrowing,
    Direction::ScopeWidening,
    Direction::ScopeDivergence,
];

fn names(directions: &[Direction]) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = directions.iter().map(Direction::as_str).collect();
    names.sort_unstable();
    names
}

#[test]
fn the_admitted_set_is_exactly_what_the_measurement_record_admits() {
    let path = format!(
        "{}/bench/results/direction-v2-rules-v{RULES_V}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the measurement for rules v{RULES_V} must exist ({path}): {e} — a rules bump needs the \
             false-carry measurement re-run before satisfaction links may carry anything"
        )
    });
    let record: serde_json::Value = serde_json::from_str(&text).expect("the record is JSON");
    let false_carry = &record["direction"]["false_carry"];
    let admitted_by_record: Vec<Direction> = WEAKENING
        .iter()
        .copied()
        .filter(|class| {
            false_carry[class.as_str()]["admitted"]
                .as_bool()
                .unwrap_or_else(|| panic!("the record reports {} admitted or not", class.as_str()))
        })
        .collect();
    assert_eq!(
        names(ADMITTED_CARRIES),
        names(&admitted_by_record),
        "ADMITTED_CARRIES must equal the classes {path} reports `admitted: true`"
    );
}

#[test]
fn only_a_weakening_class_is_ever_admitted_and_a_scope_class_never_is() {
    for admitted in ADMITTED_CARRIES {
        assert!(
            WEAKENING.contains(admitted),
            "{} is not a weakening class",
            admitted.as_str()
        );
    }
    for class in SCOPE {
        assert!(
            !ADMITTED_CARRIES.contains(&class),
            "scope classes are never carried (ADR 0040 decision 6): {}",
            class.as_str()
        );
        assert!(
            !carries(class),
            "carries() refuses a scope class whatever the set says: {}",
            class.as_str()
        );
    }
    for class in [
        Direction::ModalityStrengthening,
        Direction::NumericTightening,
        Direction::MembershipNarrowing,
        Direction::PolarityFlip,
        Direction::Unrelated,
        Direction::Unclassifiable,
        Direction::Identical,
    ] {
        assert!(!carries(class), "{} carries nothing", class.as_str());
    }
    for class in ADMITTED_CARRIES {
        assert!(carries(*class), "{}", class.as_str());
    }
}

fn judged(recorded: &str, current: &str) -> Judgement {
    judge(recorded, current, &[], &Registry::empty())
}

#[test]
fn judging_reads_the_direction_of_the_change_since_the_recorded_statement() {
    let base = "The pump service shall respond to a health check within 100 ms.";
    assert_eq!(judged(base, base), Judgement::Holds);
    assert_eq!(
        judged(
            base,
            "The pump service shall respond to a health check within 100 ms"
        ),
        Judgement::Holds,
        "a cosmetic edit"
    );
    assert_eq!(
        judged(
            base,
            "The pump service should respond to a health check within 100 ms."
        ),
        Judgement::HoldsWeakened(Direction::ModalityWeakening)
    );
    assert_eq!(
        judged(
            base,
            "The pump service shall respond to a health check within 200 ms."
        ),
        Judgement::Suspect(Direction::NumericLoosening),
        "numeric_loosening is a weakening class the measurement has not admitted"
    );
    assert_eq!(
        judged(
            base,
            "The pump service shall respond to a health check within 50 ms."
        ),
        Judgement::Suspect(Direction::NumericTightening)
    );
}

#[test]
fn an_unparseable_statement_is_suspect_unless_it_is_the_same_text() {
    let prose = "Pumps are nice.";
    assert_eq!(judged(prose, prose), Judgement::Holds);
    assert_eq!(
        judged(prose, "Pumps are lovely."),
        Judgement::Suspect(Direction::Unclassifiable)
    );
}
