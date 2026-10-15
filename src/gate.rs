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
//!   an unfilled claim, a relation that does not resolve, a dependency left undecided, a
//!   context the corpus cannot confirm.
//! * **Pass** — every policy settled in the affirmative.
//!
//! One policy judges the **invocation** rather than a claim: what the `key=value` context
//! selected, and whether the corpus's own selector predicates confirm it.

use crate::fill::StoredState;
use crate::select::{Applies, Context, Predicate, parse_predicate};
use crate::view::Row;
use std::collections::{BTreeMap, BTreeSet};

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

/// The corpus's selector vocabulary, keyed by predicate key: the values its `Eq` predicates
/// enumerate. A key **present with an empty set** is mentioned only by `Ge` predicates, which
/// decide a value by comparison rather than enumerating legal ones; a key absent entirely is
/// mentioned by no predicate, so a `key=value` entry for it decided nothing.
///
/// Collected over **every** row, not only the applying ones: a claim this variant excludes
/// still declares a legal value for the key, and the question answered here is what the
/// *corpus* describes, not what this selection happened to include.
type Vocabulary = BTreeMap<String, BTreeSet<String>>;

/// The corpus's selector vocabulary, keyed by predicate key.
fn declared_vocabulary(rows: &[Row]) -> Vocabulary {
    let mut vocabulary = Vocabulary::new();
    for row in rows {
        for text in &row.selectors {
            match parse_predicate(text) {
                Predicate::Eq { key, value } => {
                    vocabulary.entry(key).or_default().insert(value);
                }
                // Mentioned, so the key enters the vocabulary with nothing enumerated for it.
                Predicate::Ge { key, .. } => {
                    vocabulary.entry(key).or_default();
                }
                // Syntax outside the tiny grammar declares nothing: it decides nothing, and
                // `select.rs` shows its text verbatim rather than guessing at it.
                Predicate::Unsupported { .. } => {}
            }
        }
    }
    vocabulary
}

/// The invocation's own facts: did this variant select anything at all, and does the corpus
/// confirm the context the selection was made against?
///
/// Deliberately a policy like the rest, so `exit_code` and the report format need no new path —
/// but it judges the **invocation**, which is why it is named for a fact about the call rather
/// than a property of a claim.
fn selectors_confirmed(rows: &[Row], context: &Context) -> PolicyResult {
    let mut unverifiable = Vec::new();

    // A gate that clears a release over zero applying claims verified nothing, however
    // well-formed the invocation and however green the four claim policies look over an empty
    // set. Unreachable in the dogfood corpus — 15 of its 21 claims carry no predicate and so
    // always apply — which is why it is tested against a fixture.
    if rows.iter().all(|row| row.applies != Applies::Yes) {
        unverifiable.push(format!(
            "no claim applies in this variant ({} claim(s) in the corpus): the gate verified nothing",
            rows.len()
        ));
    }

    let vocabulary = declared_vocabulary(rows);
    for (key, value) in context {
        // A key no predicate anywhere mentions decided nothing at all: it selected nothing and
        // excluded nothing. Naming it is honest, and it cannot misfire — such an entry provably
        // changed no claim's applicability.
        if !vocabulary.contains_key(key) {
            unverifiable.push(format!(
                "{key}={value}: no applicability predicate in the corpus mentions `{key}`, so this entry decided nothing"
            ));
        }
    }

    PolicyResult::new(
        "selectors_confirmed",
        "every context entry is confirmed by the corpus's own selector predicates, and the variant selects at least one claim",
        Vec::new(),
        unverifiable,
    )
}

/// Evaluate every policy over a selection's applying claims, and the invocation that made it.
pub fn evaluate(rows: &[Row], context: &Context) -> Vec<PolicyResult> {
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

    // 5. The invocation itself. Appended rather than prepended so the four claim policies keep
    //    the order the docs and the tests already name; the verdict does not depend on position,
    //    and `Fail` still outranks `Unverifiable` across the whole list.
    let selectors = selectors_confirmed(rows, context);

    vec![addresses, relations, decided, rationale, selectors]
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
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
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
    let rows = match crate::view::load_rows(&context, root) {
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
    let results = evaluate(&rows, &context);
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
