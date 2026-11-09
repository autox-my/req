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
    /// Members left an `IN` set (or joined a forbidden one): fewer values comply.
    MembershipNarrowing,
    /// Members joined an `IN` set (or left a forbidden one): more values comply.
    MembershipWidening,
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
            Direction::MembershipNarrowing => "membership_narrowing",
            Direction::MembershipWidening => "membership_widening",
            Direction::ScopeNarrowing => "scope_narrowing",
            Direction::ScopeWidening => "scope_widening",
            Direction::ScopeDivergence => "scope_divergence",
            Direction::Unrelated => "unrelated",
            Direction::Unclassifiable => "unclassifiable",
        }
    }

    /// Parse a corpus label. `None` for anything unrecognised, so the caller can fail
    /// loudly: a misspelled label must not silently disable a check.
    pub fn parse(label: &str) -> Option<Direction> {
        [
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
            Direction::Unrelated,
        ]
        .into_iter()
        .find(|d| d.as_str() == label)
    }

    /// True when the change permits strictly more than before.
    pub fn is_loosening(&self) -> bool {
        matches!(
            self,
            Direction::ModalityWeakening
                | Direction::NumericLoosening
                | Direction::MembershipWidening
                | Direction::ScopeWidening
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
            Direction::MembershipNarrowing => Some(Direction::MembershipWidening),
            Direction::MembershipWidening => Some(Direction::MembershipNarrowing),
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

    if a.operator != b.operator || a.value != b.value || a.members != b.members {
        if a.operator == Some(Comp::In) || b.operator == Some(Comp::In) {
            return membership_direction(a, b);
        }
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

/// Which way a set moved (ADR 0030). A removed member means fewer values comply: a narrowing. For
/// a prohibition the set is what is *forbidden*, so the senses swap: a member added to a forbidden
/// set narrows what may be done. Members both added and removed, or a set against a non-set, is
/// not a direction — `scope_divergence`, never "no change".
fn membership_direction(a: &CanonicalForm, b: &CanonicalForm) -> Direction {
    use Direction::*;

    if a.operator != Some(Comp::In) || b.operator != Some(Comp::In) {
        return ScopeDivergence;
    }
    let forbidden = prohibition(a.modality);
    if b.members.is_subset(&a.members) {
        return if forbidden {
            MembershipWidening
        } else {
            MembershipNarrowing
        };
    }
    if a.members.is_subset(&b.members) {
        return if forbidden {
            MembershipNarrowing
        } else {
            MembershipWidening
        };
    }
    ScopeDivergence
}

/// Which way a bound moved. The sign is the thing that is easy to get backwards: for an
/// upper bound (`≤`, `<`) a larger number permits more; for a lower bound (`≥`, `>`) a
/// larger number demands more.
fn numeric_direction(a: &CanonicalForm, b: &CanonicalForm) -> Direction {
    use Direction::*;

    let (Some(op_a), Some(op_b)) = (a.operator, b.operator) else {
        // A comparator appeared or disappeared: a different requirement, not a direction.
        return ScopeDivergence;
    };
    // Values are optional, and bailing out on a missing one was a real defect: "within a
    // single window" vs "in fewer than a single window" carries no number, yet LE→LT is a
    // tightening. The direction gold set found it (7 pairs read as ScopeDivergence).
    let values = match (a.value, b.value) {
        (Some(v_a), Some(v_b)) => Some((v_a, v_b)),
        (None, None) => None,
        // A bound that appears or disappears is a different requirement.
        _ => return ScopeDivergence,
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
    let value_tightens = match values {
        None => None,
        Some((v_a, v_b)) => match v_b.cmp(&v_a) {
            Ordering::Equal => None,
            Ordering::Greater => Some(lower),
            Ordering::Less => Some(!lower),
        },
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

/// `req compare <a.yml> <b.yml>` — what kind of change is this?
///
/// Exit codes: **0** classified, **3** UNVERIFIABLE (a claim that will not load — unparsable, or
/// declaring a file format this build does not write, whose parse is therefore NOT JUDGED — or a
/// pair with nothing structurally to compare). A classification is not an approval: the answer is
/// for a human to ratify.
pub fn run(path_a: &str, path_b: &str, root: &std::path::Path) -> i32 {
    let registry = match crate::canon::load_registry(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let (a, b) = match (load_claim(path_a), load_claim(path_b)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let (va, vb) = (
        crate::canon::derive(&a, &registry),
        crate::canon::derive(&b, &registry),
    );
    let direction = compare(&va.form, &vb.form);
    println!("{} -> {}: {}", a.claim_id, b.claim_id, direction.as_str());
    if direction.is_loosening() {
        println!("  this change permits strictly more than before");
    }
    if direction == Direction::Unclassifiable {
        println!("  one side has no structured form, so there is nothing to compare");
        return 3;
    }
    let (flags_a, flags_b) = (va.flagged_pins().count(), vb.flagged_pins().count());
    if flags_a > 0 || flags_b > 0 {
        println!(
            "  note: a pin disagrees with its claim's parse ({flags_a} / {flags_b} flagged) — \
             run `req canon` before trusting the classification"
        );
    }
    let (addr_a, addr_b) = (va.address, vb.address);
    println!("  {} -> {}", &addr_a[..12], &addr_b[..12]);
    0
}

fn load_claim(path: &str) -> Result<crate::model::ClaimFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    // A side declaring a format this build does not write is a claim that will not load, exactly
    // as the docstring above says: classifying it would report a direction over a parse nothing
    // judged, and the loader (which this path does not go through) refuses the same file. Same
    // class, same home: `canon::parse_claim`, never a second opinion.
    crate::canon::parse_claim(&text).map_err(|e| format!("{path}: {e}"))
}

fn rank(m: Option<Modality>) -> Option<u8> {
    match m {
        Some(Modality::Shall) => Some(3),
        Some(Modality::Should) => Some(2),
        Some(Modality::May) => Some(1),
        _ => None,
    }
}
