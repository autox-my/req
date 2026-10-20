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
//! * **Pass** — every policy settled in the affirmative **and** every claim in the corpus was
//!   decided; a selection that leaves a claim undecided is Unverifiable, never Pass.
//!
//! One policy judges the **invocation** rather than a claim: what the `key=value` context
//! selected, and whether the corpus's own selector predicates confirm it.
//!
//! The verdict joins that answer to the selection: a run that leaves **any claim undecided**
//! is UNVERIFIABLE, because the invocation did not decide what it claims to release — the
//! claims it skipped are named, with the remedy, rather than merely counted. `Fail` still
//! outranks that ("we found a problem" is stronger than "we could not tell"), and an undecided
//! claim outranks a pass (ADR 0015). A bare `gate` is therefore UNVERIFIABLE on any corpus
//! carrying a selector, and the per-variant invocation is the one that can be green.

use crate::fill::StoredState;
use crate::select::{Applies, Context, Predicate, parse_predicate};
use crate::view::Row;
use rust_decimal::Decimal;
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

/// What the corpus declares: the values its `Eq` predicates enumerate, plus every selector
/// outside the tiny grammar, **kept verbatim**.
///
/// A key *present with an empty set* is mentioned only by `Ge` predicates, which decide a value
/// by comparison rather than enumerating legal ones. A key *absent from the map* is mentioned by
/// no `Eq` or `Ge` predicate — but an unparsed selector may still name it, so a message about
/// such a key must quote those rather than assert an absence the gate cannot see.
///
/// Collected over **every** row, not only the applying ones: a claim this variant excludes
/// still declares a legal value for the key, and the question answered here is what the
/// *corpus* describes, not what this selection happened to include.
#[derive(Default)]
struct Vocabulary {
    /// Key → the values its `Eq` predicates declare; an empty set means `Ge`-only.
    eq_values: BTreeMap<String, BTreeSet<String>>,
    /// Selectors the grammar does not cover, verbatim, deduplicated and ordered (`BTreeSet`).
    /// They decide nothing, and `select.rs` shows their text rather than guessing at it.
    unparsed: BTreeSet<String>,
}

/// The corpus's selector vocabulary, keyed by predicate key.
fn declared_vocabulary(rows: &[Row]) -> Vocabulary {
    let mut vocabulary = Vocabulary::default();
    for row in rows {
        for text in &row.selectors {
            match parse_predicate(text) {
                Predicate::Eq { key, value } => {
                    vocabulary.eq_values.entry(key).or_default().insert(value);
                }
                // Mentioned, so the key enters the vocabulary with nothing enumerated for it.
                Predicate::Ge { key, .. } => {
                    vocabulary.eq_values.entry(key).or_default();
                }
                Predicate::Unsupported { text } => {
                    vocabulary.unparsed.insert(text);
                }
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
        // A key no readable predicate mentions decided nothing at all: it selected nothing and
        // excluded nothing. Naming it is honest — but a selector outside the grammar is not
        // read, so its keys are unknown, and the two cases are worded differently rather than
        // asserting an absence the gate cannot check.
        let Some(declared) = vocabulary.eq_values.get(key) else {
            if vocabulary.unparsed.is_empty() {
                unverifiable.push(format!(
                    "{key}={value}: nothing in the corpus decides `{key}`, so this entry decided nothing"
                ));
            } else {
                let unparsed: Vec<&str> = vocabulary.unparsed.iter().map(String::as_str).collect();
                unverifiable.push(format!(
                    "{key}={value}: nothing in the corpus decides `{key}`; it also carries \
                     selector(s) outside the `==`/`>=` grammar, whose keys are not read — {} — so a \
                     selector naming `{key}` cannot be ruled out",
                    unparsed.join("; ")
                ));
            }
            continue;
        };
        // Mentioned only by `Ge` predicates: the value is decided by comparison, so there is no
        // enumerated vocabulary to check it against and nothing to flag. `runs=100` is legitimate
        // here; `runs=abc` parses as no decimal, and that honest `Unknown` shows up in the
        // undecided count rather than as a second finding.
        if declared.is_empty() {
            continue;
        }
        // The `Eq` predicates enumerate a **closed** vocabulary for the key, so a value outside
        // it was never declared and the entry silently narrows the selection: `field=val` drops
        // REQ-0018 from the variant. A `Ge` predicate on the same key does not rescue such a
        // value — letting the comparison decide legality would re-open the silent narrowing for
        // exactly the keys that carry both. No key in the dogfood corpus is mixed;
        // `tests/release_gate.rs` pins the choice with a fixture.
        if !declared.contains(value) {
            let declared_values: Vec<String> = declared
                .iter()
                .map(|declared| format!("{key} == {declared}"))
                .collect();
            unverifiable.push(format!(
                "{key}={value}: the corpus does not confirm this value for `{key}` — it declares {}",
                declared_values.join(", ")
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

/// The overall verdict of a set of policy results **and the selection they were made over**, as
/// an exit code.
///
/// The precedence is the ADR's, not a detail: **Fail (1) > undecided-or-Unverifiable (3) > Pass
/// (0)**. A definite defect outranks an open question — "we found a problem" is the stronger
/// statement — while an undecided claim outranks a pass, because a variant that did not decide
/// what it claims to release has not examined it (ADR 0015). The count is passed in rather than
/// derived from `results`: no policy over the *applying* claims can see what was not applying.
pub fn exit_code(results: &[PolicyResult], undecided: usize) -> i32 {
    if results.iter().any(|r| r.outcome == Outcome::Fail) {
        1
    } else if undecided > 0 || results.iter().any(|r| r.outcome == Outcome::Unverifiable) {
        3
    } else {
        0
    }
}

/// `req gate [key=value ...]`
///
/// Exit: **0** every policy passed **and** every claim in the corpus was decided, **1** a policy
/// failed, **3** a policy could not be settled *or* the selection left a claim undecided (the
/// claims are named, with the remedy), **2** usage. Fail outranks unverifiable, and an undecided
/// claim outranks a pass.
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
    let undecided: Vec<&str> = rows
        .iter()
        .filter(|r| r.applies == Applies::Unknown)
        .map(|r| r.claim_id.as_str())
        .collect();
    println!(
        "# Release gate: {applying} applying claim(s), {} undecided \
         (an undecided claim is never treated as absent)",
        undecided.len()
    );
    // Not every undecided claim is waiting for a context: a predicate the grammar cannot read is
    // decided by no context at all, so "pass the variant" would send the caller looking for a value
    // that does not exist. The two cases are named apart, each with its own remedy.
    let unparsable: Vec<(String, String)> = rows
        .iter()
        .filter(|r| r.applies == Applies::Unknown)
        .filter_map(|r| {
            let bad: Vec<String> = r
                .selectors
                .iter()
                .map(|text| parse_predicate(text))
                .filter_map(|predicate| match predicate {
                    Predicate::Unsupported { text } => Some(text),
                    // A `Ge` whose bound is not a decimal parses (the grammar sees
                    // `key >= value`) but no evaluation can compare it — `runs >= twenty` is
                    // unparsable in effect, and "pass the variant context" would be the wrong
                    // advice for it. The same parse `evaluate` uses decides the bucket.
                    Predicate::Ge { key, value } if value.parse::<Decimal>().is_err() => {
                        Some(format!("{key} >= {value}"))
                    }
                    _ => None,
                })
                .collect();
            if bad.is_empty() {
                None
            } else {
                Some((r.claim_id.clone(), bad.join("; ")))
            }
        })
        .collect();
    if !undecided.is_empty() {
        // The disclosure is not the signal — the exit code is — so the claims it skipped are
        // named beside it, with the one remedy that turns them into examined claims. Without
        // the names, a caller knows a release is incomplete and not which decision is missing.
        println!(
            "  undecided (not examined by this invocation): {}",
            undecided.join(", ")
        );
        for (claim, text) in &unparsable {
            println!(
                "  - {claim}: `{text}` is not in the selector grammar (`key == value`, \
                 `key >= number`), so no context can decide it — rewrite the predicate"
            );
        }
        if unparsable.is_empty() {
            println!(
                "  remedy: pass the variant context that decides them — a complete release context, \
                 as `.req/variants` declares — and gate each declared variant"
            );
        } else {
            println!(
                "  remedy: rewrite the predicate above, then pass the variant context that decides \
                 the rest — a complete release context, as `.req/variants` declares"
            );
        }
    }
    let results = evaluate(&rows, &context);
    for result in &results {
        println!("\n[{}] {}", result.outcome.as_str(), result.name);
        println!("  {}", result.detail);
        for offender in &result.offenders {
            println!("  - {offender}");
        }
    }
    let code = exit_code(&results, undecided.len());
    println!(
        "\nverdict: {}",
        match code {
            0 => "PASS — every policy settled in the affirmative, and every claim was decided",
            1 => "FAIL — a defect in a claim this variant includes",
            _ => "UNVERIFIABLE — nothing is wrong, but something cannot be confirmed",
        }
    );
    code
}
