//! Structural contradictions and recorded Conflicts (ADR 0043).
//!
//! Contradiction in general is undecidable from prose; what Tier 1 *can* decide is narrow and exact:
//! two claims about the same subject, action, qualifier, unit, exception and temporal, under nested
//! conditions, whose obligations admit **no common value**. That is the slot equality the meaning
//! fingerprint already rests on, so the detector inherits its measured property rather than
//! inventing one (ADR 0043 decision 6: false contradictions = 0 on every pair labelled equivalent
//! and on every nested-set direction pair).
//!
//! Two seams, both pure:
//!
//! * [`contradicts`] — one pair of canonical forms: [`Pair::NotComparable`] (with the reason),
//!   [`Pair::Consistent`] or [`Pair::Contradiction`], decided on exact [`Decimal`]s, never floats.
//! * [`find`] — a selection's **applying** claims, bucketed by the six equality fields in a
//!   `BTreeMap` (no hash iteration), compared inside a bucket, plus the recorded Conflicts
//!   (`relations: [{type: conflicts-with, to: REQ-X}]`).
//!
//! Nothing here consults a model or a process: a provider may *propose* a Conflict through
//! `req propose`, and a human records it; the judgement below is arithmetic on the canonical form.

use crate::canon::{CanonicalForm, Comp, Modality};
use crate::select::Applies;
use crate::view::Row;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

/// The relation kind a human records a Conflict with (`CONTEXT.md`, *Conflict*).
pub const CONFLICTS_WITH: &str = "conflicts-with";

/// Whether the detector is **admitted**: whether an unrecorded contradiction is judged (FAIL) or only
/// reported (ADR 0043 decision 6, *the rule is measured before it gates*).
///
/// Compiled in and pinned to the measurement, never read at runtime: `true` exactly when every
/// `bench/results/*-rules-v<RULES_V>.json` record that carries the `contradiction` block reports
/// `false_contradictions: 0`. `tests/contradict.rs` reads the records and asserts equality, so a
/// rules bump, or a corpus that breaks the bound, fails CI until this constant is changed
/// deliberately.
pub const DETECTOR_ADMITTED: bool = true;

/// The verdict on one pair of canonical forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pair {
    /// Tier 1 cannot decide this pair, and the reason is named: a different slot, conditions that are
    /// not nested, a recommendation or a permission, an unstructured claim. Counted, never guessed.
    NotComparable(String),
    /// Comparable, and some value satisfies both obligations.
    Consistent,
    /// Comparable, and **no** value satisfies both. `slot` names the rule that fired (`polarity`,
    /// `bounds` or `membership`); `a` and `b` are the two obligations as the parse holds them.
    Contradiction {
        slot: &'static str,
        a: String,
        b: String,
    },
}

/// One contradiction found among a selection's applying claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contradiction {
    /// The lower claim id, then the higher: the pair is unordered, so it is reported once.
    pub a_id: String,
    pub b_id: String,
    pub slot: &'static str,
    /// The first claim's obligation, as [`Pair::Contradiction`] prints it.
    pub a: String,
    pub b: String,
    /// Whether a human recorded the pair as a Conflict (`conflicts-with`, on either claim).
    pub recorded: bool,
}

impl Contradiction {
    /// `REQ-A vs REQ-B: <slot> — <a> against <b>`: both ids, the slot and the two values.
    pub fn describe(&self) -> String {
        format!(
            "{} vs {}: {} — {} against {}",
            self.a_id, self.b_id, self.slot, self.a, self.b
        )
    }
}

/// What a selection's applying claims hold: the contradictions, how many pairs were comparable, how
/// many in a bucket were not, and the recorded Conflicts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Findings {
    /// Every Tier-1 contradiction (recorded or not), sorted by the two claim ids.
    pub contradictions: Vec<Contradiction>,
    /// Pairs in a bucket that were comparable (consistent or contradicting).
    pub comparable: usize,
    /// Pairs in a bucket that were not (non-nested conditions, `SHOULD`/`MAY`, an `IN` against a bound).
    pub not_comparable: usize,
    /// Recorded Conflicts whose two claims both apply, one per unordered pair (lower id first).
    pub conflicts: Vec<(String, String)>,
    /// `(claim, to)` for a recorded `conflicts-with` whose `to` the corpus does not hold: a dangling
    /// Conflict cannot be shown resolved.
    pub dangling: Vec<(String, String)>,
}

impl Findings {
    /// The contradictions no human recorded as a Conflict — the defects nobody has looked at.
    pub fn unrecorded(&self) -> impl Iterator<Item = &Contradiction> {
        self.contradictions.iter().filter(|c| !c.recorded)
    }
}

/// An obligation as the report prints it: `SHALL LE 0.1 s`, `SHALL-NOT IN {a|b}`, `SHALL (no bound)`.
fn obligation(form: &CanonicalForm) -> String {
    let mut out = form.modality_str().to_string();
    match form.value_display() {
        Some(value) => {
            out.push(' ');
            out.push_str(form.operator_str());
            out.push(' ');
            out.push_str(&value);
        }
        None => out.push_str(" (no bound)"),
    }
    if let Some(unit) = form.unit.as_deref().filter(|unit| *unit != "none") {
        out.push(' ');
        out.push_str(unit);
    }
    out
}

fn contradiction(slot: &'static str, a: &CanonicalForm, b: &CanonicalForm) -> Pair {
    Pair::Contradiction {
        slot,
        a: obligation(a),
        b: obligation(b),
    }
}

/// One end of an interval over the exact decimals: the value and whether the end is open.
type End = (Decimal, bool);

/// The interval an `SHALL` bound admits, or `None` for an operator that is not an interval (`NE`).
fn interval(form: &CanonicalForm) -> Option<(Option<End>, Option<End>)> {
    let value = form.value?;
    match form.operator? {
        Comp::Le => Some((None, Some((value, false)))),
        Comp::Lt => Some((None, Some((value, true)))),
        Comp::Ge => Some((Some((value, false)), None)),
        Comp::Gt => Some((Some((value, true)), None)),
        Comp::Eq => Some((Some((value, false)), Some((value, false)))),
        Comp::Ne | Comp::In => None,
    }
}

/// The tighter lower end: the greater value; at a tie, open wins.
fn tighter_lower(a: Option<End>, b: Option<End>) -> Option<End> {
    match (a, b) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => Some(if x.0 > y.0 {
            x
        } else if y.0 > x.0 {
            y
        } else {
            (x.0, x.1 || y.1)
        }),
    }
}

/// The tighter upper end: the smaller value; at a tie, open wins.
fn tighter_upper(a: Option<End>, b: Option<End>) -> Option<End> {
    match (a, b) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => Some(if x.0 < y.0 {
            x
        } else if y.0 < x.0 {
            y
        } else {
            (x.0, x.1 || y.1)
        }),
    }
}

/// Is the intersection of two intervals empty? An upper end below a lower end, or the two meeting at
/// one point that either end excludes.
fn disjoint(a: (Option<End>, Option<End>), b: (Option<End>, Option<End>)) -> bool {
    let lower = tighter_lower(a.0, b.0);
    let upper = tighter_upper(a.1, b.1);
    match (lower, upper) {
        (Some((low, low_open)), Some((high, high_open))) => {
            low > high || (low == high && (low_open || high_open))
        }
        _ => false,
    }
}

/// Decide one pair of canonical forms (ADR 0043 decisions 1 and 2).
///
/// Comparable when both are structured, the six slots `subject`, `action`, `qualifier`, `unit`,
/// `exception` and `temporal` are equal, one `conditions` set is a subset of the other (the empty set
/// is a subset of every set) and both modalities are `SHALL`/`SHALL NOT`. A contradiction is a pair
/// no value satisfies: polarity, disjoint bounds, or disjoint/forbidden membership. Anything Tier 1
/// has no rule for is [`Pair::NotComparable`] with its reason; it never guesses.
pub fn contradicts(a: &CanonicalForm, b: &CanonicalForm) -> Pair {
    if a.unstructured || b.unstructured {
        return Pair::NotComparable(
            "an unstructured claim has no slots to compare (ADR 0043)".to_string(),
        );
    }
    let slots: [(&str, bool); 6] = [
        ("subject", a.subject == b.subject),
        ("action", a.action == b.action),
        ("qualifier", a.qualifier == b.qualifier),
        ("unit", a.unit == b.unit),
        ("exception", a.exception == b.exception),
        ("temporal", a.temporal == b.temporal),
    ];
    if let Some((slot, _)) = slots.iter().find(|(_, same)| !same) {
        return Pair::NotComparable(format!("the {slot} differs"));
    }
    if !(a.conditions.is_subset(&b.conditions) || b.conditions.is_subset(&a.conditions)) {
        return Pair::NotComparable(
            "the conditions are not nested: no situation is known where both bind".to_string(),
        );
    }
    let (Some(ma), Some(mb)) = (a.modality, b.modality) else {
        return Pair::NotComparable("a claim carries no modality".to_string());
    };
    let binding = |m: Modality| matches!(m, Modality::Shall | Modality::ShallNot);
    if !binding(ma) || !binding(mb) {
        return Pair::NotComparable(
            "a recommendation or a permission does not exclude a value (SHOULD and MAY never \
             contradict at Tier 1)"
                .to_string(),
        );
    }
    match (ma, mb) {
        // Two prohibitions never exclude each other.
        (Modality::ShallNot, Modality::ShallNot) => Pair::Consistent,
        (Modality::Shall, Modality::ShallNot) => polarity(a, b, a, b),
        (Modality::ShallNot, Modality::Shall) => polarity(a, b, b, a),
        _ => both_shall(a, b),
    }
}

/// One `SHALL` (`pos`) against one `SHALL NOT` (`neg`); `a`/`b` keep the caller's order for printing.
fn polarity(
    a: &CanonicalForm,
    b: &CanonicalForm,
    pos: &CanonicalForm,
    neg: &CanonicalForm,
) -> Pair {
    if pos.operator == neg.operator && pos.value == neg.value && pos.members == neg.members {
        return contradiction("polarity", a, b);
    }
    if pos.operator == Some(Comp::In) && neg.operator == Some(Comp::In) {
        // Every allowed value is forbidden exactly when S ⊆ T.
        return if pos.members.is_subset(&neg.members) {
            contradiction("membership", a, b)
        } else {
            Pair::Consistent
        };
    }
    Pair::NotComparable(
        "a SHALL against a SHALL NOT whose operator or value differ: no Tier-1 rule decides it"
            .to_string(),
    )
}

fn both_shall(a: &CanonicalForm, b: &CanonicalForm) -> Pair {
    match (a.operator, b.operator) {
        // A bare action only contradicts by polarity.
        (None, _) | (_, None) => Pair::Consistent,
        (Some(Comp::In), Some(Comp::In)) => {
            if a.members.is_disjoint(&b.members) {
                contradiction("membership", a, b)
            } else {
                Pair::Consistent
            }
        }
        (Some(Comp::In), _) | (_, Some(Comp::In)) => Pair::NotComparable(
            "a membership against a bound: no Tier-1 rule compares them".to_string(),
        ),
        (Some(Comp::Ne), Some(Comp::Ne)) => Pair::Consistent,
        // `NE x` leaves everything but one point: only an exact `x` is excluded by it.
        (Some(Comp::Ne), Some(other)) | (Some(other), Some(Comp::Ne)) => {
            if other == Comp::Eq && a.value.is_some() && a.value == b.value {
                contradiction("bounds", a, b)
            } else if a.value.is_none() || b.value.is_none() {
                Pair::NotComparable("a bound without a value".to_string())
            } else {
                Pair::Consistent
            }
        }
        (Some(_), Some(_)) => match (interval(a), interval(b)) {
            (Some(ia), Some(ib)) if disjoint(ia, ib) => contradiction("bounds", a, b),
            (Some(_), Some(_)) => Pair::Consistent,
            _ => Pair::NotComparable("a bound without a value".to_string()),
        },
    }
}

/// The key two claims share when they may be compared: subject, action, qualifier, unit, exception
/// and temporal. `conditions` are compared inside a bucket (nested, not equal).
type BucketKey = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn bucket_key(form: &CanonicalForm) -> BucketKey {
    (
        form.subject.clone(),
        form.action.clone(),
        form.qualifier.clone(),
        form.unit.clone(),
        form.exception.clone(),
        form.temporal.clone(),
    )
}

fn ordered(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// The canonical form of every claim in the corpus, by claim id — what [`find`] reads, since a
/// [`Row`] carries the address but not the form it was hashed from.
pub fn forms_of(root: &std::path::Path) -> Result<BTreeMap<String, CanonicalForm>, String> {
    Ok(crate::canon::load_claims(root)?
        .into_iter()
        .map(|loaded| (loaded.claim.claim_id.clone(), loaded.derived.form))
        .collect())
}

/// Find the contradictions and recorded Conflicts among a selection's **applying** claims.
///
/// A claim the selection excludes, or leaves undecided, is not one of its applying claims: two claims
/// that never apply together are never compared (ADR 0043, *Rejected*: the 150% model). `forms` holds
/// every claim's canonical form by id. Deterministic: buckets are a `BTreeMap`, claims are visited in
/// id order and the contradictions are sorted by the two ids.
pub fn find(rows: &[Row], forms: &BTreeMap<String, CanonicalForm>) -> Findings {
    let applying: BTreeSet<&str> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| row.claim_id.as_str())
        .collect();
    let known: BTreeSet<&str> = rows.iter().map(|row| row.claim_id.as_str()).collect();

    // Recorded Conflicts: a relation on either claim records the unordered pair, so one recorded on
    // both sides is still one Conflict.
    let mut recorded: BTreeSet<(String, String)> = BTreeSet::new();
    let mut dangling: BTreeSet<(String, String)> = BTreeSet::new();
    for row in rows {
        for (kind, to) in &row.relations {
            if kind != CONFLICTS_WITH {
                continue;
            }
            if known.contains(to.as_str()) {
                recorded.insert(ordered(&row.claim_id, to));
            } else if applying.contains(row.claim_id.as_str()) {
                dangling.insert((row.claim_id.clone(), to.clone()));
            }
        }
    }
    let conflicts: Vec<(String, String)> = recorded
        .iter()
        .filter(|(x, y)| applying.contains(x.as_str()) && applying.contains(y.as_str()))
        .cloned()
        .collect();

    let mut buckets: BTreeMap<BucketKey, Vec<(&str, &CanonicalForm)>> = BTreeMap::new();
    for id in &applying {
        if let Some(form) = forms.get(*id).filter(|form| !form.unstructured) {
            buckets
                .entry(bucket_key(form))
                .or_default()
                .push((id, form));
        }
    }

    let mut findings = Findings {
        conflicts,
        dangling: dangling.into_iter().collect(),
        ..Findings::default()
    };
    for members in buckets.values() {
        for (index, (a_id, a)) in members.iter().enumerate() {
            for (b_id, b) in &members[index + 1..] {
                match contradicts(a, b) {
                    Pair::NotComparable(_) => findings.not_comparable += 1,
                    Pair::Consistent => findings.comparable += 1,
                    Pair::Contradiction { slot, a: va, b: vb } => {
                        findings.comparable += 1;
                        findings.contradictions.push(Contradiction {
                            a_id: (*a_id).to_string(),
                            b_id: (*b_id).to_string(),
                            slot,
                            a: va,
                            b: vb,
                            recorded: recorded.contains(&ordered(a_id, b_id)),
                        });
                    }
                }
            }
        }
    }
    findings
        .contradictions
        .sort_by(|x, y| (&x.a_id, &x.b_id).cmp(&(&y.a_id, &y.b_id)));
    findings
}
