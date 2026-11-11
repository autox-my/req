//! False carries (ADR 0040 decision 5, task S-1): a weakening class may carry a satisfaction link
//! only if no pair labelled a strengthening, a tightening, a polarity flip or a scope change is
//! classified into it — and only if that zero has *support*, because zero out of five is not
//! evidence. `req bench` reports both per weakening class; admission is ADR 0040's input, not a
//! bench verdict.
//!
//! The labels in these corpora are written here, from the mutation, never from the classifier.

use req::bench::{FalseCarry, evaluate};
use req::model::{Corpus, Pair, Registry};

const WEAKENING: [&str; 3] = [
    "modality_weakening",
    "numeric_loosening",
    "membership_widening",
];

fn pair(id: &str, a: &str, b: &str, direction: &str) -> Pair {
    Pair {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        source: "tests/false_carry.rs".to_string(),
        note: None,
        direction: Some(direction.to_string()),
    }
}

fn corpus(not_equivalent: Vec<Pair>) -> Corpus {
    Corpus {
        schema: "req.bench.corpus/0".to_string(),
        version: 2,
        rules: None,
        equivalent: vec![],
        not_equivalent,
    }
}

/// `n` correctly classified mirrors: "may" -> "shall" is a strengthening, labelled as one.
fn strengthenings(n: usize) -> Vec<Pair> {
    (0..n)
        .map(|i| {
            pair(
                &format!("MS-{i:04}"),
                "The system may log each command.",
                "The system shall log each command.",
                "modality_strengthening",
            )
        })
        .collect()
}

fn class<'a>(carries: &'a [FalseCarry], name: &str) -> &'a FalseCarry {
    carries
        .iter()
        .find(|c| c.class == name)
        .unwrap_or_else(|| panic!("false_carry always names {name}: {carries:?}"))
}

#[test]
fn a_strengthening_read_as_a_weakening_is_a_false_carry_under_the_reported_class() {
    // Labelled a strengthening (the oracle), but the text goes shall -> may: the classifier says
    // weakening. That is the pair that would have let a weakened design satisfy the claim.
    let mut pairs = strengthenings(3);
    pairs.push(pair(
        "X-001",
        "The system shall log each command.",
        "The system may log each command.",
        "modality_strengthening",
    ));
    let ev = evaluate(&corpus(pairs), &Registry::empty());
    let c = class(&ev.false_carry, "modality_weakening");
    assert_eq!(c.count, 1, "{c:?}");
    assert_eq!(c.pairs.len(), 1);
    assert!(c.pairs[0].contains("X-001"), "{:?}", c.pairs);
    assert!(
        c.pairs[0].contains("modality_strengthening") && c.pairs[0].contains("modality_weakening"),
        "named with expected and reported: {:?}",
        c.pairs
    );
    // ...and under no other class.
    assert_eq!(class(&ev.false_carry, "numeric_loosening").count, 0);
    assert_eq!(class(&ev.false_carry, "membership_widening").count, 0);
}

#[test]
fn a_numeric_tightening_read_as_a_loosening_is_a_false_carry_under_numeric_loosening() {
    let ev = evaluate(
        &corpus(vec![pair(
            "X-002",
            "The system shall respond within 100 ms.",
            "The system shall respond within 200 ms.",
            "numeric_tightening",
        )]),
        &Registry::empty(),
    );
    let c = class(&ev.false_carry, "numeric_loosening");
    assert_eq!(c.count, 1, "{c:?}");
    assert!(c.pairs[0].contains("X-002"), "{:?}", c.pairs);
    assert_eq!(class(&ev.false_carry, "modality_weakening").count, 0);
}

#[test]
fn a_polarity_flip_or_scope_change_read_as_a_weakening_is_a_false_carry_too() {
    // The ADR names them all: strengthening, tightening, polarity flip, scope change.
    let ev = evaluate(
        &corpus(vec![
            pair(
                "X-003",
                "The system shall log each command.",
                "The system may log each command.",
                "polarity_flip",
            ),
            pair(
                "X-004",
                "The system shall log each command.",
                "The system may log each command.",
                "scope_narrowing",
            ),
        ]),
        &Registry::empty(),
    );
    assert_eq!(class(&ev.false_carry, "modality_weakening").count, 2);
}

#[test]
fn a_correctly_classified_mirror_is_no_false_carry_and_counts_as_support() {
    let ev = evaluate(&corpus(strengthenings(7)), &Registry::empty());
    let c = class(&ev.false_carry, "modality_weakening");
    assert_eq!(c.count, 0, "{c:?}");
    assert!(c.pairs.is_empty());
    assert_eq!(c.support, 7);
}

#[test]
fn a_true_weakening_is_neither_a_carry_nor_support() {
    let ev = evaluate(
        &corpus(vec![pair(
            "W-001",
            "The system shall log each command.",
            "The system may log each command.",
            "modality_weakening",
        )]),
        &Registry::empty(),
    );
    let c = class(&ev.false_carry, "modality_weakening");
    assert_eq!((c.count, c.support), (0, 0), "{c:?}");
}

#[test]
fn admission_needs_support_of_100_and_no_carry() {
    let admitted =
        |ev: &req::bench::Evaluation| class(&ev.false_carry, "modality_weakening").admitted;

    let ev99 = evaluate(&corpus(strengthenings(99)), &Registry::empty());
    assert_eq!(class(&ev99.false_carry, "modality_weakening").support, 99);
    assert!(!admitted(&ev99), "zero carries on 99 is not enough");

    let ev100 = evaluate(&corpus(strengthenings(100)), &Registry::empty());
    assert!(admitted(&ev100), "zero carries on 100 admits");

    let mut pairs = strengthenings(500);
    pairs.push(pair(
        "X-005",
        "The system shall log each command.",
        "The system may log each command.",
        "modality_strengthening",
    ));
    let ev_carry = evaluate(&corpus(pairs), &Registry::empty());
    let c = class(&ev_carry.false_carry, "modality_weakening");
    assert_eq!(c.count, 1);
    assert!(c.support >= 500);
    assert!(!c.admitted, "one carry on any support is not admitted");
}

#[test]
fn a_class_with_no_pairs_is_present_unsupported_and_not_admitted() {
    let ev = evaluate(&corpus(vec![]), &Registry::empty());
    assert_eq!(ev.false_carry.len(), WEAKENING.len());
    for name in WEAKENING {
        let c = class(&ev.false_carry, name);
        assert_eq!((c.count, c.support), (0, 0), "{name}: {c:?}");
        assert!(!c.admitted, "{name}: no support is never admitted");
    }
}

#[test]
fn an_unclassifiable_pair_is_neither_a_carry_nor_support() {
    // A side with no statement the parser can hold: nothing to compare structurally.
    let ev = evaluate(
        &corpus(vec![pair(
            "U-001",
            "Gibberish without a modal phrase.",
            "More gibberish, still no obligation.",
            "modality_strengthening",
        )]),
        &Registry::empty(),
    );
    assert_eq!(ev.direction_unclassifiable, vec!["U-001".to_string()]);
    for name in WEAKENING {
        let c = class(&ev.false_carry, name);
        assert_eq!((c.count, c.support), (0, 0), "{name}: {c:?}");
    }
}
