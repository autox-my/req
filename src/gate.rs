//! The release gate: may this selection ship? (design step 9)
//!
//! Policies over the **applying** claims of a selection. A claim the variant excludes is
//! not this variant's problem; a claim that is *undecided* is never treated as absent
//! (REQ-0016), so anything it leaves open is UNVERIFIABLE rather than PASS.
//!
//! Three outcomes, and the middle one is why this module is not a boolean:
//!
//! * **Fail** — a defect in a claim the variant includes: a pin the derived parse does not
//!   hold (ADR 0022), no provenance, a linked test that did not pass.
//! * **Unverifiable** — nothing is known to be wrong, but nothing can be confirmed either:
//!   a relation that does not resolve, a dependency left undecided, a context the corpus
//!   cannot confirm, a test report that cannot be read.
//! * **Pass** — every policy settled in the affirmative **and** every claim in the corpus was
//!   decided; a selection that leaves a claim undecided is Unverifiable, never Pass.
//!
//! One policy judges the **invocation** rather than a claim: what the `key=value` context
//! selected, and whether the corpus's own selector predicates confirm it. One policy, added by
//! ADR 0019, judges the **evidence**: does a test that proves each applying claim pass? (Its
//! process-evidence arm is a declared answer, not an exception: see [`TestEvidence`].)
//!
//! The verdict joins that answer to the selection: a run that leaves **any claim undecided**
//! is UNVERIFIABLE, because the invocation did not decide what it claims to release — the
//! claims it skipped are named, with the remedy, rather than merely counted. `Fail` still
//! outranks that ("we found a problem" is stronger than "we could not tell"), and an undecided
//! claim outranks a pass (ADR 0015). A bare `gate` is therefore UNVERIFIABLE on any corpus
//! carrying a selector, and the per-variant invocation is the one that can be green.
//!
//! One case is not fixable by any invocation, and the docs must not imply otherwise: a selector
//! outside the grammar (`field = value`) or a `Ge` whose bound is not a decimal (`runs >= twenty`)
//! is decided by no context at all — such a claim can be *excluded* by a context, never made to
//! apply — so a variant that does not exclude it stays UNVERIFIABLE until the predicate is
//! rewritten. An excluded claim is not the variant's problem, which is the one way a corpus
//! carrying such a predicate can go green without touching the grammar.

use crate::canon::PinVerdict;
use crate::canon::RULES_V;
use crate::format::{self, Format, SCHEMA_GATE};
use crate::junit::TestOutcome;
use crate::model::Evidence;
use crate::select::{Applies, Context, Predicate, evaluate as evaluate_predicate, parse_predicate};
use crate::view::Row;
use rust_decimal::Decimal;
use serde::Serialize;
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

    /// The lowercase spelling the JSON document uses. `as_str` stays uppercase because the text
    /// report's bytes are load-bearing; the document's `outcome` values are `pass`/`fail`/
    /// `unverifiable` by the pack's shape.
    pub fn as_json(&self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Fail => "fail",
            Outcome::Unverifiable => "unverifiable",
        }
    }
}

/// One policy's verdict, with the claims that decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyResult {
    pub name: &'static str,
    /// The policy's prose description, printed under its name. Owned, because one policy
    /// (`pins_hold`) states how many pins it held — a pass over zero pins says so.
    pub detail: String,
    pub outcome: Outcome,
    pub offenders: Vec<String>,
    /// Claims whose evidence is an explicit **process declaration** rather than a machine link.
    /// They do not decide the outcome (a declared process is a pass-as-declared), but they are
    /// **named and counted**, because a count that hides its subjects is how "declared" starts to
    /// read as "proven". Empty for every policy but `tests_linked`.
    pub process: Vec<String>,
    /// How many pins the policy judged, for `pins_hold` only (`None` for every other policy).
    /// It is in the JSON document too, so a pass over zero pins is distinguishable from a pass
    /// that held some for a consumer that never reads the text form.
    pub checked: Option<usize>,
}

impl PolicyResult {
    fn new(
        name: &'static str,
        detail: impl Into<String>,
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
            detail: detail.into(),
            outcome,
            offenders,
            process: Vec::new(),
            checked: None,
        }
    }

    /// Attach the process-evidenced claims the policy reports beside its outcome.
    fn with_process(mut self, process: Vec<String>) -> Self {
        self.process = process;
        self
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

/// The evidence the invocation supplied for the `tests_linked` policy (ADR 0019).
///
/// Three states, because a missing report and an unreadable one are different remedies but the
/// same class: neither is "no failures found". A **process declaration is not a report**: a claim
/// that declares `evidence: process` is answered by its declaration, and a claim that declares
/// nothing (the default, `machine`) is answered only by the report.
#[derive(Debug, Clone, Copy)]
pub enum TestEvidence<'a> {
    /// No `--test-report` was supplied.
    Absent,
    /// The supplied report could not be read or parsed; the reason is the reader's.
    Unreadable(&'a str),
    /// The parsed report. An empty one is a distinct state the policy reports (zero test cases
    /// verify nothing).
    Report(&'a crate::junit::Report),
}

/// The `tests_linked` policy: for every applying claim, does a test that proves it pass?
///
/// The four outcomes, and there is no fifth:
///
/// * **PASS** — the claim links at least one test and at least one linked test **passed**;
/// * **FAIL** — every linked test is present in the report and **none passed** (a `skipped` test
///   is not a pass); a linked test that **failed** outranks any unverifiable;
/// * **UNVERIFIABLE — no linked test** — the claim carries no `tests:` entry, the flagship answer,
///   named per claim;
/// * **UNVERIFIABLE — linked test not in the report** — a linked test id has no testcase (renamed
///   or deleted), never folded into PASS.
///
/// Plus two **report-level** unverifiables, reported **once** rather than once per claim: no
/// report was supplied, or the report cannot be read / carries zero test cases. A policy that
/// cannot be exercised is not a pass. A claim that declares `evidence: process` is PASS **as
/// declared**: named in [`PolicyResult::process`] and counted in the summary, never inferred from
/// an empty `tests:` list.
fn tests_linked(applying: &[&Row], evidence: TestEvidence<'_>) -> PolicyResult {
    const DETAIL: &str = "every applying claim's linked test passed, or the claim declares process \
                          evidence; a claim with no link, or a linked test the report does not \
                          carry, cannot be confirmed";
    let (process, machine): (Vec<&Row>, Vec<&Row>) = applying
        .iter()
        .partition(|row| row.evidence == Evidence::Process);
    let process_ids: Vec<String> = process.iter().map(|row| row.claim_id.clone()).collect();

    // Nothing machine-evidenced here: the declarations are the answer, and no report is needed.
    if machine.is_empty() {
        return PolicyResult::new("tests_linked", DETAIL, Vec::new(), Vec::new())
            .with_process(process_ids);
    }

    let mut failed: Vec<String> = Vec::new();
    let mut unverifiable: Vec<String> = Vec::new();
    match evidence {
        // A policy that cannot be exercised is not a pass — and the remedy is named.
        TestEvidence::Absent => unverifiable.push(format!(
            "no --test-report was supplied, so the {} machine-evidenced claim(s) cannot be judged",
            machine.len()
        )),
        TestEvidence::Unreadable(reason) => {
            unverifiable.push(format!("the test report cannot be read: {reason}"));
        }
        TestEvidence::Report(report) if report.is_empty() => unverifiable
            .push("the test report carries no test cases, so it verifies nothing".to_string()),
        TestEvidence::Report(report) => {
            for row in machine {
                if row.tests.is_empty() {
                    // The flagship answer. "No link" is never a pass, however green the suite is.
                    unverifiable.push(format!("{}: no linked test", row.claim_id));
                    continue;
                }
                let mut saw_pass = false;
                let mut absent: Vec<&str> = Vec::new();
                let mut failed_tests: Vec<&str> = Vec::new();
                for id in &row.tests {
                    match report.outcome(id) {
                        Some(TestOutcome::Passed) => saw_pass = true,
                        Some(TestOutcome::Failed) => failed_tests.push(id),
                        // Present, and deliberately not a pass. It only becomes a FAIL below when
                        // it is *all* there is; alone it cannot confirm the claim either.
                        Some(TestOutcome::Skipped) => {}
                        None => absent.push(id),
                    }
                }
                if !failed_tests.is_empty() {
                    // A failure outranks any unverifiable, whatever else is missing.
                    failed.push(format!(
                        "{}: linked test failed: {}",
                        row.claim_id,
                        failed_tests.join(", ")
                    ));
                } else if !absent.is_empty() {
                    // The rename defence: a linked test the report no longer carries is not a
                    // test that passed.
                    unverifiable.push(format!(
                        "{}: linked test not in the report: {}",
                        row.claim_id,
                        absent.join(", ")
                    ));
                } else if !saw_pass {
                    // Every linked test present and none passed: skipped is not a pass.
                    failed.push(format!(
                        "{}: every linked test is present and none passed — a skipped test is \
                         not a pass ({})",
                        row.claim_id,
                        row.tests.join(", ")
                    ));
                }
            }
        }
    }
    PolicyResult::new("tests_linked", DETAIL, failed, unverifiable).with_process(process_ids)
}

/// Evaluate every policy over a selection's applying claims, and the invocation that made it.
///
/// `evidence` is the test report the invocation supplied; [`TestEvidence::Absent`] is a real state
/// and not a no-op, because a release whose claims carry no machine evidence cannot be confirmed.
pub fn evaluate(rows: &[Row], context: &Context, evidence: TestEvidence<'_>) -> Vec<PolicyResult> {
    let mut results = evaluate_claims(rows, context);
    let applying: Vec<&Row> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .collect();
    // Inserted before the invocation policy, keeping the claim policies first and `tests_linked`
    // the sibling of `rationale_present` the docs name. Position does not affect the fold.
    results.insert(results.len() - 1, tests_linked(&applying, evidence));
    results
}

/// The five claim-and-invocation policies, without the test-evidence policy (`tests_linked` is
/// added by [`evaluate`], which is what the CLI calls).
///
/// A seam for the tests that judge one policy over hand-built rows: it is not an alternative
/// answer the CLI can give — [`evaluate`] is what `req gate` calls, so no invocation can skip
/// the evidence.
pub fn evaluate_claims(rows: &[Row], context: &Context) -> Vec<PolicyResult> {
    let known: BTreeSet<&str> = rows.iter().map(|r| r.claim_id.as_str()).collect();
    let state_of = |id: &str| rows.iter().find(|r| r.claim_id == id).map(|r| r.applies);
    let applying: Vec<&Row> = rows.iter().filter(|r| r.applies == Applies::Yes).collect();

    // 1. Every pin of every applying claim agrees with the parse derived at load (ADR 0022).
    //    A flagged pin — a contradiction, or a slot the parse does not carry — is a defect: the
    //    author said what the statement must parse to, and it does not. With no pin to hold the
    //    policy passes *and says so*: it ran over zero pins, it was not skipped.
    let mut failed = Vec::new();
    let mut held = 0usize;
    for row in &applying {
        for check in &row.pins {
            match &check.verdict {
                PinVerdict::Agrees => held += 1,
                PinVerdict::Contradiction(derived) => failed.push(format!(
                    "{}: pin {} {} contradicts the parse ({derived})",
                    row.claim_id, check.key, check.pinned
                )),
                PinVerdict::NotCarried => failed.push(format!(
                    "{}: pin {} {} is not carried by the parse (it holds no {})",
                    row.claim_id, check.key, check.pinned, check.key
                )),
            }
        }
    }
    let detail = if held + failed.len() == 0 {
        "every pin of every applying claim agrees with the derived parse — zero pins in this \
         selection: nothing to hold, so nothing failed (the policy ran over zero pins; it was \
         not skipped)"
            .to_string()
    } else {
        format!(
            "every pin of every applying claim agrees with the derived parse ({} pin(s), {held} \
             held)",
            held + failed.len()
        )
    };
    let checked = held + failed.len();
    let mut pins = PolicyResult::new("pins_hold", detail, failed, Vec::new());
    pins.checked = Some(checked);

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

    vec![pins, relations, decided, rationale, selectors]
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

/// One policy, as the JSON document carries it. `details` is the offenders list: the claims that
/// decided the outcome, never the policy's prose description.
#[derive(Serialize)]
struct PolicyDocument<'a> {
    policy: &'a str,
    outcome: &'a str,
    details: &'a [String],
    /// `pins_hold` only: the number of pins judged. Absent for every other policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<usize>,
}

/// One claim the invocation left undecided, and what a caller would have to supply to decide it.
#[derive(Serialize)]
struct UndecidedDocument {
    claim_id: String,
    missing: Vec<String>,
}

/// `gate`'s document: the folded verdict, the six policies, the process-evidenced claims and
/// the undecided claims. The
/// verdict is computed from the same `exit_code` the command returns, so the document and the
/// process cannot disagree.
#[derive(Serialize)]
struct GateDocument<'a> {
    schema: &'static str,
    generator: &'static str,
    rules_v: u32,
    selection: &'a Context,
    verdict: &'static str,
    policies: Vec<PolicyDocument<'a>>,
    /// Claims whose evidence is an explicit process declaration (ADR 0019), sorted. They pass as
    /// declared and are named here so a consumer cannot mistake the machine count for the whole
    /// answer.
    process_evidenced: &'a [String],
    undecided: Vec<UndecidedDocument>,
}

/// The word for a verdict code: one mapping, used for the document only — the text report keeps
/// its own longer explanation.
fn verdict_word(code: i32) -> &'static str {
    match code {
        0 => "pass",
        1 => "fail",
        _ => "unverifiable",
    }
}

/// The undecided claims, sorted by id, each with the context keys that would decide it.
///
/// `missing` is a **key** for an `Eq`/`Ge` predicate the context leaves open, and the predicate's
/// own text for a selector outside the grammar — no context can decide that one, so naming a key
/// would send the caller looking for a value that does not exist. Deduplicated and sorted
/// (`BTreeSet`), because a document is deterministic.
fn undecided_document(rows: &[Row], context: &Context) -> Vec<UndecidedDocument> {
    let mut sorted: Vec<&Row> = rows
        .iter()
        .filter(|row| row.applies == Applies::Unknown)
        .collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    sorted
        .into_iter()
        .map(|row| {
            let mut missing: BTreeSet<String> = BTreeSet::new();
            for text in &row.selectors {
                let predicate = parse_predicate(text);
                if evaluate_predicate(&predicate, context) != Applies::Unknown {
                    continue;
                }
                match predicate {
                    Predicate::Eq { key, .. } | Predicate::Ge { key, .. } => {
                        missing.insert(key);
                    }
                    Predicate::Unsupported { text } => {
                        missing.insert(text);
                    }
                }
            }
            UndecidedDocument {
                claim_id: row.claim_id.clone(),
                missing: missing.into_iter().collect(),
            }
        })
        .collect()
}

/// `req gate [--test-report <junit.xml>] [--format text|json] [key=value ...]`
///
/// Exit: **0** every policy passed **and** every claim in the corpus was decided, **1** a policy
/// failed, **3** a policy could not be settled *or* the selection left a claim undecided (the
/// claims are named, with the remedy), **2** usage. Fail outranks unverifiable, and an undecided
/// claim outranks a pass.
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
    let usage = "usage: req gate [--test-report <junit.xml>] [--format text|json] [key=value ...]";
    let (format, args) = match format::take_format(args, usage) {
        Ok(parsed) => parsed,
        Err(code) => return code,
    };
    let mut context = Context::new();
    let mut test_report: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--test-report" {
            index += 1;
            match args.get(index) {
                Some(path) => {
                    test_report = Some(path.clone());
                }
                None => {
                    eprintln!("--test-report needs a value: {usage}");
                    return 2;
                }
            }
        } else if let Some(path) = arg.strip_prefix("--test-report=") {
            // The `--flag=value` spelling most CI snippets and GNU habits produce. Without this arm it
            // fell through to the selector parser and became a *context entry* — a silent answer to a
            // question nobody asked, which is the one failure mode the gate must not have.
            if path.is_empty() {
                eprintln!("--test-report needs a value: {usage}");
                return 2;
            }
            test_report = Some(path.to_string());
        } else {
            match arg.split_once('=') {
                Some((key, value)) if !key.is_empty() => {
                    context.insert(key.to_string(), value.to_string());
                }
                _ => {
                    eprintln!("{usage}");
                    return 2;
                }
            }
        }
        index += 1;
    }
    // Read the report before the corpus: a usage error or an unreadable file is answered the same
    // way whether or not the corpus loads, and the reason is the reader's own.
    let loaded_report = test_report
        .as_deref()
        .map(|path| crate::junit::Report::load(std::path::Path::new(path)));
    let evidence = match &loaded_report {
        None => TestEvidence::Absent,
        Some(Err(reason)) => TestEvidence::Unreadable(reason),
        Some(Ok(report)) => TestEvidence::Report(report),
    };
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
    // Evaluated once, before either rendering: the verdict is the same fold in both, and the
    // exit code is returned from it in both.
    let results = evaluate(&rows, &context, evidence);
    let code = exit_code(&results, undecided.len());
    // The process-evidenced claims, sorted once: they are reported beside the outcome (never as
    // offenders, because a declaration is a pass-as-declared) and must be visible as a count.
    let mut process_evidenced: Vec<String> = results
        .iter()
        .flat_map(|result| result.process.iter().cloned())
        .collect();
    process_evidenced.sort();
    process_evidenced.dedup();
    if format == Format::Json {
        let document = GateDocument {
            schema: SCHEMA_GATE,
            generator: "gate",
            rules_v: RULES_V,
            selection: &context,
            verdict: verdict_word(code),
            policies: results
                .iter()
                .map(|result| PolicyDocument {
                    policy: result.name,
                    outcome: result.outcome.as_json(),
                    details: &result.offenders,
                    checked: result.checked,
                })
                .collect(),
            // A process declaration is an answer, so it is not an offender; it is still a fact a
            // consumer must see, and it is named rather than only counted (ADR 0019).
            process_evidenced: &process_evidenced,
            undecided: undecided_document(&rows, &context),
        };
        println!("{}", format::document(&document));
        return code;
    }
    println!(
        "# Release gate: {applying} applying claim(s), {} undecided \
         (an undecided claim is never treated as absent)",
        undecided.len()
    );
    // Not every undecided claim is waiting for a context: a predicate the grammar cannot read is
    // decided by no context at all, so "pass the variant" would send the caller looking for a value
    // that does not exist. The two cases are named apart, each with its own remedy.
    let unparsable: Vec<(String, String)> =
        rows.iter()
            .filter(|r| r.applies == Applies::Unknown)
            .filter_map(|r| {
                let bad: Vec<String> =
                    r.selectors
                        .iter()
                        .map(|text| parse_predicate(text))
                        // This bucket mirrors `select::evaluate`'s decidability, and `select` owns
                        // both halves of it: `parse_predicate` for the grammar and the `Decimal`
                        // comparison. Keep the match exhaustive — a `_` arm would default "a context
                        // can decide this", so a new `Predicate` variant must be classified here
                        // too or it silently joins the decidable side.
                        .filter_map(|predicate| match predicate {
                            Predicate::Unsupported { text } => Some(text),
                            // A `Ge` whose bound is not a decimal parses (the grammar sees
                            // `key >= value`) but no evaluation can compare it — `runs >= twenty` is
                            // unparsable in effect, and "pass the variant context" would be the wrong
                            // advice for it. The same `Decimal` parse `select::evaluate` uses decides
                            // the bucket.
                            Predicate::Ge { key, value } => (value.parse::<Decimal>().is_err())
                                .then(|| format!("{key} >= {value}")),
                            // Decidable by a context, in both directions.
                            Predicate::Eq { .. } => None,
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
                 `key >= number`), so no context can make it apply; a context can still exclude \
                 it (another selector on the claim returning `No`), and an excluded claim is not \
                 the variant's problem"
            );
        }
        if unparsable.is_empty() {
            println!(
                "  remedy: pass the variant context that decides them — one complete release \
                 context per line of the release list your own gate script reads — and gate each \
                 declared variant"
            );
        } else if unparsable.len() == undecided.len() {
            // Every undecided claim here is unreadable, so there is no "rest" for a context to
            // decide: the rewrite is the whole remedy.
            println!(
                "  remedy: rewrite the predicates above — no context can make these claims apply"
            );
        } else {
            println!(
                "  remedy: rewrite the predicate above, then pass the variant context that decides \
                 the rest — one complete release context per line of the release list your own gate \
                 script reads"
            );
        }
    }
    for result in &results {
        println!("\n[{}] {}", result.outcome.as_str(), result.name);
        println!("  {}", result.detail);
        for offender in &result.offenders {
            println!("  - {offender}");
        }
        if !result.process.is_empty() {
            // Named, not folded into the pass: a declaration is evidence of a different kind.
            println!(
                "  * process-evidenced (declared, not machine-proven): {}",
                result.process.join(", ")
            );
        }
    }
    if !process_evidenced.is_empty() {
        println!(
            "\nprocess-evidenced: {} — declared, not machine-proven: {}",
            process_evidenced.len(),
            process_evidenced.join(", ")
        );
    }
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
