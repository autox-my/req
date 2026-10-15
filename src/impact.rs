//! Impact: if this claim changes, what does it touch? (design step 9)
//!
//! Reverse traversal over relations that are stored **once, on the outgoing side**
//! (REQ-0020) — so the edge set is derived here, exactly as in `trace`.
//!
//! Two properties beyond "walk backwards":
//!
//! * the **selection state of every dependent is carried through**, because a dependent
//!   outside the variant is not something this variant has to re-decide — and an
//!   *undecided* dependent is a gap, never a silent zero;
//! * a relation whose target no claim answers to makes the answer **incomplete**, so it is
//!   reported as UNVERIFIABLE rather than quietly shortening the list. An impact analysis
//!   that understates its own result is worse than one that admits it cannot finish.
//!
//! Cycles terminate: a corpus is authored data, and a graph we did not write must not be
//! able to hang this.

use crate::select::{Applies, Context};
use crate::view::Row;
use std::collections::{BTreeMap, BTreeSet};

/// One claim that depends on the target, and how far away it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependent {
    pub claim_id: String,
    pub applies: Applies,
    pub distance: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImpactFindings {
    /// The claim asked about does not exist in the corpus.
    pub missing_target: bool,
    /// The target is undecided in this variant.
    pub unknown_target: bool,
    /// A dependent that is undecided in this variant.
    pub unknown_dependents: Vec<String>,
    /// Relations no claim answers to: the answer cannot be complete.
    pub dangling: Vec<String>,
}

/// Everything that depends on `target`, nearest first, with the target's own state.
pub fn dependents(rows: &[Row], target: &str) -> (Vec<Dependent>, ImpactFindings) {
    let known: BTreeSet<&str> = rows.iter().map(|r| r.claim_id.as_str()).collect();
    let mut findings = ImpactFindings {
        missing_target: !known.contains(target),
        unknown_target: rows
            .iter()
            .any(|r| r.claim_id == target && r.applies == Applies::Unknown),
        ..ImpactFindings::default()
    };

    // Derived incoming adjacency. A BTreeMap keyed by target, values in id order, so the
    // traversal order never depends on hash iteration.
    let mut incoming: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for row in rows {
        for (kind, to) in &row.relations {
            if !known.contains(to.as_str()) {
                findings
                    .dangling
                    .push(format!("{} -{kind}-> {to}", row.claim_id));
                continue;
            }
            incoming
                .entry(to.as_str())
                .or_default()
                .insert(row.claim_id.as_str());
        }
    }
    if findings.missing_target {
        return (Vec::new(), findings);
    }

    // Breadth-first, so "nearest first" is a property of the answer and not of the walk.
    let mut queue: Vec<(&str, usize)> = vec![(target, 0)];
    let mut seen: BTreeSet<&str> = BTreeSet::from([target]);
    let mut index = 0;
    let mut out: Vec<Dependent> = Vec::new();
    while index < queue.len() {
        let (current, distance) = queue[index];
        index += 1;
        if let Some(parents) = incoming.get(current) {
            for parent in parents {
                if seen.insert(parent) {
                    queue.push((parent, distance + 1));
                    let applies = rows
                        .iter()
                        .find(|r| r.claim_id == *parent)
                        .map(|r| r.applies)
                        .unwrap_or(Applies::Unknown);
                    if applies == Applies::Unknown {
                        findings.unknown_dependents.push(parent.to_string());
                    }
                    out.push(Dependent {
                        claim_id: parent.to_string(),
                        applies,
                        distance: distance + 1,
                    });
                }
            }
        }
    }
    (out, findings)
}

/// `req impact <claim-id> [key=value ...]`
///
/// Exit: **0** complete and settled, **2** usage, **3** UNVERIFIABLE (missing target, an
/// undecided target or dependent, or a relation that does not resolve).
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
    let mut target: Option<String> = None;
    let mut context = Context::new();
    for arg in args {
        match arg.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                context.insert(key.to_string(), value.to_string());
            }
            _ if target.is_none() => target = Some(arg.clone()),
            _ => {
                eprintln!("usage: req impact <claim-id> [key=value ...]");
                return 2;
            }
        }
    }
    let Some(target) = target else {
        eprintln!("usage: req impact <claim-id> [key=value ...]");
        return 2;
    };
    let rows = match crate::view::load_rows(&context, root) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let (dependents, findings) = dependents(&rows, &target);
    println!("# Impact: {target}");
    if findings.missing_target {
        println!("UNVERIFIABLE: no claim with that id is in the corpus");
        return 3;
    }
    if dependents.is_empty() {
        println!("nothing depends on it (no incoming relations)");
    } else {
        println!("{} dependent(s), nearest first:", dependents.len());
        for dependent in &dependents {
            let state = match dependent.applies {
                Applies::Yes => "applies",
                Applies::No => "outside the selection",
                Applies::Unknown => "UNDECIDED",
            };
            println!(
                "- {}  distance {}  {state}",
                dependent.claim_id, dependent.distance
            );
        }
    }
    if findings.unknown_target
        || !findings.unknown_dependents.is_empty()
        || !findings.dangling.is_empty()
    {
        println!("\n## Findings");
        if findings.unknown_target {
            println!("- the target itself is undecided in this variant");
        }
        for id in &findings.unknown_dependents {
            println!("- dependent undecided: {id}");
        }
        for line in &findings.dangling {
            println!("- UNVERIFIABLE: {line} (no such claim) — the answer may be incomplete");
        }
        return 3;
    }
    0
}
