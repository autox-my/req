//! Comparison: **what kind of change is this?**
//!
//! Direction is a *safety* classification, not a description. The canonical layer's
//! asymmetric cost is that a false merge silently deletes a requirement; the cost here
//! is that a loosened obligation is reported as a no-op — or as a tightening — and gets
//! ratified. Every class below exists so those two can never be confused, and a pair the
//! canonical layer cannot parse is [`Direction::Unclassifiable`]: its own outcome,
//! never folded into "identical".
//!
//! Deterministic and offline: two canonical forms in, one class out. Nothing here
//! reads the network, a model, or a clock, and no comparison goes through a float.

use crate::canon::{CanonicalForm, Comp, Modality};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Same canonical form. A text edit that lands here was cosmetic.
    Identical,
    /// The obligation was weakened (SHALL → SHOULD → MAY).
    ModalityWeakening,
    /// The obligation was strengthened.
    ModalityStrengthening,
    /// Obligation ↔ prohibition: a different obligation, not a stronger or weaker one.
    PolarityFlip,
    /// An upper bound grew, or a lower bound shrank: more values comply.
    NumericLoosening,
    /// The reverse: fewer values comply.
    NumericTightening,
    /// A condition was added: the claim applies less often.
    ScopeNarrowing,
    /// A condition was removed: it applies more often.
    ScopeWidening,
    /// Same subject and predicate, different thing bounded or different qualifier.
    ScopeDivergence,
    /// Different subject or predicate — not a change to this requirement.
    Unrelated,
    /// At least one side did not parse, so there is nothing to compare structurally.
    Unclassifiable,
}

impl Direction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::Identical => "identical",
            Direction::ModalityWeakening => "modality_weakening",
            Direction::ModalityStrengthening => "modality_strengthening",
            Direction::PolarityFlip => "polarity_flip",
            Direction::NumericLoosening => "numeric_loosening",
            Direction::NumericTightening => "numeric_tightening",
            Direction::ScopeNarrowing => "scope_narrowing",
            Direction::ScopeWidening => "scope_widening",
            Direction::ScopeDivergence => "scope_divergence",
            Direction::Unrelated => "unrelated",
            Direction::Unclassifiable => "unclassifiable",
        }
    }

    /// True when the change permits strictly more than before.
    pub fn is_loosening(&self) -> bool {
        matches!(
            self,
            Direction::ModalityWeakening | Direction::NumericLoosening | Direction::ScopeWidening
        )
    }

    /// Classes where a wrong answer misleads a ratifier about whether an obligation
    /// moved. "Identical" and "unrelated" are not among them — they claim no movement.
    pub fn is_safety_relevant(&self) -> bool {
        !matches!(
            self,
            Direction::Identical | Direction::Unrelated | Direction::Unclassifiable
        )
    }

    /// The opposite direction on the same axis, if there is one.
    pub fn opposite(&self) -> Option<Direction> {
        match self {
            Direction::ModalityWeakening => Some(Direction::ModalityStrengthening),
            Direction::ModalityStrengthening => Some(Direction::ModalityWeakening),
            Direction::NumericLoosening => Some(Direction::NumericTightening),
            Direction::NumericTightening => Some(Direction::NumericLoosening),
            Direction::ScopeNarrowing => Some(Direction::ScopeWidening),
            Direction::ScopeWidening => Some(Direction::ScopeNarrowing),
            _ => None,
        }
    }

    /// The gate for the direction gold set: a safety-relevant expectation reported as
    /// **no change**, or as a change in the **opposite** direction, is the dangerous
    /// confusion — the equivalent, for a reviewer, of a false merge for identity.
    pub fn is_dangerous_confusion(expected: Direction, reported: Direction) -> bool {
        if !expected.is_safety_relevant() {
            return false;
        }
        reported == Direction::Identical || expected.opposite() == Some(reported)
    }
}

/// Classify the change from `a` to `b`.
pub fn compare(a: &CanonicalForm, b: &CanonicalForm) -> Direction {
    use Direction::*;

    if crate::canon::canonical_json(a) == crate::canon::canonical_json(b) {
        return Identical;
    }
    // One unparseable side: no structural comparison is possible, and saying
    // "identical" or a direction here would be a guess about a claim we cannot read.
    if a.unstructured || b.unstructured {
        return Unclassifiable;
    }
    // A different subject or predicate is not a change to *this* requirement.
    if a.subject != b.subject || a.action != b.action {
        return Unrelated;
    }

    // Modality. Polarity first: forbidding X is a different obligation, and calling it a
    // "strengthening" would tell a reviewer the wrong thing.
    if prohibition(a.modality) != prohibition(b.modality) {
        return PolarityFlip;
    }
    match (rank(a.modality), rank(b.modality)) {
        (Some(x), Some(y)) if y < x => return ModalityWeakening,
        (Some(x), Some(y)) if y > x => return ModalityStrengthening,
        _ => {}
    }

    // Conditions: added narrows, removed widens, replaced diverges.
    if a.conditions != b.conditions {
        if b.conditions.is_superset(&a.conditions) {
            return ScopeNarrowing;
        }
        if a.conditions.is_superset(&b.conditions) {
            return ScopeWidening;
        }
        return ScopeDivergence;
    }

    // The qualifier is an identity field: "2.5 A of standby current" and "2.5 A of surge
    // current" are the same number bounding different things.
    if a.qualifier != b.qualifier || a.unit != b.unit {
        return ScopeDivergence;
    }

    if a.operator != b.operator || a.value != b.value {
        return numeric_direction(a, b);
    }

    if a.exception != b.exception || a.temporal != b.temporal {
        return ScopeDivergence;
    }
    // Same subject, predicate, modality, conditions, qualifier, bound — yet a different
    // canonical form. Something in identity is not accounted for above; divergence is
    // the honest answer, and this arm is what keeps a new field from silently passing.
    ScopeDivergence
}

/// Which way a bound moved. The sign is the thing that is easy to get backwards: for an
/// upper bound (`≤`, `<`) a larger number permits more; for a lower bound (`≥`, `>`) a
/// larger number demands more.
fn numeric_direction(a: &CanonicalForm, b: &CanonicalForm) -> Direction {
    use Direction::*;

    let (Some(op_a), Some(op_b)) = (a.operator, b.operator) else {
        // A bound appeared or disappeared: a different requirement, not a direction.
        return ScopeDivergence;
    };
    let (Some(v_a), Some(v_b)) = (a.value, b.value) else {
        return ScopeDivergence;
    };
    // An exact value is not a bound: changing it changes what is required.
    if op_a == Comp::Eq || op_b == Comp::Eq {
        return ScopeDivergence;
    }
    let same_family = |x: Comp, y: Comp| {
        let upper = |o: Comp| matches!(o, Comp::Le | Comp::Lt);
        let lower = |o: Comp| matches!(o, Comp::Ge | Comp::Gt);
        (upper(x) && upper(y)) || (lower(x) && lower(y))
    };
    if !same_family(op_a, op_b) {
        // Incomparable: "at most 200" and "at least 200" are not a direction apart.
        return ScopeDivergence;
    }

    let lower = matches!(op_a, Comp::Ge | Comp::Gt);
    let value_tightens = match v_b.cmp(&v_a) {
        Ordering::Equal => None,
        Ordering::Greater => Some(lower),
        Ordering::Less => Some(!lower),
    };
    // LT ⊂ LE and GT ⊂ GE, so moving to the strict comparator tightens.
    let op_tightens = match (op_a, op_b) {
        (Comp::Le, Comp::Lt) | (Comp::Ge, Comp::Gt) => Some(true),
        (Comp::Lt, Comp::Le) | (Comp::Gt, Comp::Ge) => Some(false),
        _ => None,
    };

    match (op_tightens, value_tightens) {
        (Some(true), None) | (None, Some(true)) => NumericTightening,
        (Some(false), None) | (None, Some(false)) => NumericLoosening,
        (Some(t), Some(v)) if t == v => {
            if t {
                NumericTightening
            } else {
                NumericLoosening
            }
        }
        // Both moved, in opposite senses: the change is not a direction.
        _ => ScopeDivergence,
    }
}

fn prohibition(m: Option<Modality>) -> bool {
    matches!(m, Some(Modality::ShallNot))
}

fn rank(m: Option<Modality>) -> Option<u8> {
    match m {
        Some(Modality::Shall) => Some(3),
        Some(Modality::Should) => Some(2),
        Some(Modality::May) => Some(1),
        _ => None,
    }
}
