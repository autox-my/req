//! The release gate: may this selection ship? (design step 9)
//!
//! Policies over the **applying** claims of a selection. A claim the variant excludes is
//! not this variant's problem; a claim that is *undecided* is never treated as absent
//! (REQ-0016), so anything it leaves open is UNVERIFIABLE rather than PASS.
//!
//! Three outcomes, and the middle one is why this module is not a boolean:
//!
//! * **Fail** — a defect in a claim the variant includes: an address that does not
//!   recompute, a digest that disagrees with the stored form, no provenance.
//! * **Unverifiable** — nothing is known to be wrong, but nothing can be confirmed either:
//!   an unfilled claim, a relation that does not resolve, a dependency left undecided.
//! * **Pass** — every policy settled in the affirmative.

use crate::fill::StoredState;
use crate::select::{Applies, Context};
use crate::view::Row;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Unverifiable,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail => "FAIL",
            Outcome::Unverifiable => "UNVERIFIABLE",
        }
    }
}

/// One policy's verdict, with the claims that decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyResult {
    pub name: &'static str,
    pub detail: &'static str,
    pub outcome: Outcome,
    pub offenders: Vec<String>,
}

impl PolicyResult {
    fn new(
        name: &'static str,
        detail: &'static str,
        failed: Vec<String>,
        unverifiable: Vec<String>,
    ) -> Self {
        // A known defect outranks an open question: "we found a problem" is a stronger
        // statement than "we could not tell".
        let (outcome, offenders) = if !failed.is_empty() {
            (Outcome::Fail, failed)
        } else if !unverifiable.is_empty() {
            (Outcome::Unverifiable, unverifiable)
        } else {
            (Outcome::Pass, Vec::new())
        };
        Self {
            name,
            detail,
            outcome,
            offenders,
        }
    }
}

/// Evaluate every policy over a selection's applying claims.
pub fn evaluate(rows: &[Row]) -> Vec<PolicyResult> {
    let known: BTreeSet<&str> = rows.iter().map(|r| r.claim_id.as_str()).collect();
    let state_of = |id: &str| rows.iter().find(|r| r.claim_id == id).map(|r| r.applies);
    let applying: Vec<&Row> = rows.iter().filter(|r| r.applies == Applies::Yes).collect();

    // 1. Every applying claim has an address for the *current* rules. A hand-edited digest
    //    is a defect; an unfilled or stale one is an open question, not a pass.
    let (mut failed, mut unverifiable) = (Vec::new(), Vec::new());
    for row in &applying {
        match &row.stored {
            StoredState::Verified => {}
            StoredState::Mismatch(why) => failed.push(format!("{}: {why}", row.claim_id)),
            StoredState::Candidate(why) => unverifiable.push(format!("{}: {why}", row.claim_id)),
        }
    }
    let addresses = PolicyResult::new(
        "addresses_current",
        "every applying claim stores an address that recomputes under the current rules",
        failed,
        unverifiable,
    );

    // 2. Every relation of an applying claim resolves inside the corpus. An edge to a
    //    claim that does not exist cannot be checked, so it is unverifiable — never
    //    silently dropped, which would understate the graph being released.
    let mut unverifiable = Vec::new();
    for row in &applying {
        for (kind, to) in &row.relations {
            if !known.contains(to.as_str()) {
                unverifiable.push(format!("{} -{kind}-> {to}", row.claim_id));
            }
        }
    }
    let relations = PolicyResult::new(
        "relations_resolve",
        "every relation of an applying claim resolves to a claim in the corpus",
        Vec::new(),
        unverifiable,
    );

    // 3. Every dependency of an applying claim is decided in this variant: a release cannot
    //    be signed while a claim it depends on is undecided.
    let mut unverifiable = Vec::new();
    for row in &applying {
        for (kind, to) in &row.relations {
            if state_of(to) == Some(Applies::Unknown) {
                unverifiable.push(format!("{} -{kind}-> {to}", row.claim_id));
            }
        }
    }
    let decided = PolicyResult::new(
        "dependencies_decided",
        "no applying claim depends on a claim this variant leaves undecided",
        Vec::new(),
        unverifiable,
    );

    // 4. Provenance: a requirement without a source and a date cannot be audited.
    let mut failed = Vec::new();
    for row in &applying {
        if row.rationale_source.trim().is_empty() || row.rationale_date.trim().is_empty() {
            failed.push(row.claim_id.clone());
        }
    }
    let rationale = PolicyResult::new(
        "rationale_present",
        "every applying claim carries provenance (rationale source and date)",
        failed,
        Vec::new(),
    );

    vec![addresses, relations, decided, rationale]
}

/// The overall verdict of a set of policy results, as an exit code.
pub fn exit_code(results: &[PolicyResult]) -> i32 {
    if results.iter().any(|r| r.outcome == Outcome::Fail) {
        1
    } else if results.iter().any(|r| r.outcome == Outcome::Unverifiable) {
        3
    } else {
        0
    }
}

/// `req gate [key=value ...]`
///
/// Exit: **0** every policy passed, **1** a policy failed, **3** a policy could not be
/// settled, **2** usage. Fail outranks unverifiable.
pub fn run(args: &[String]) -> i32 {
    let mut context = Context::new();
    for arg in args {
        match arg.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                context.insert(key.to_string(), value.to_string());
            }
            _ => {
                eprintln!("usage: req gate [key=value ...]");
                return 2;
            }
        }
    }
    let rows = match crate::view::load_rows(&context) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let applying = rows.iter().filter(|r| r.applies == Applies::Yes).count();
    let undecided = rows
        .iter()
        .filter(|r| r.applies == Applies::Unknown)
        .count();
    println!(
        "# Release gate: {applying} applying claim(s), {undecided} undecided \
         (an undecided claim is never treated as absent)"
    );
    let results = evaluate(&rows);
    for result in &results {
        println!("\n[{}] {}", result.outcome.as_str(), result.name);
        println!("  {}", result.detail);
        for offender in &result.offenders {
            println!("  - {offender}");
        }
    }
    let code = exit_code(&results);
    println!(
        "\nverdict: {}",
        match code {
            0 => "PASS — every policy settled in the affirmative",
            1 => "FAIL — a defect in a claim this variant includes",
            _ => "UNVERIFIABLE — nothing is wrong, but something cannot be confirmed",
        }
    );
    code
}
