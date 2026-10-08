//! Views: a selection compiled into something a person reads (design step 9).
//!
//! `render` is the spec view — the 150% model with its selectors visible — and `trace` is
//! the traceability matrix. Both are **projections**: they read claims, they never write
//! one, and selecting changes no address.
//!
//! Three rules carry over from the layers below, and each is why this module exists:
//!
//! * an **undecided** claim is shown, in its own section. A view that drops it is exactly
//!   the silent exclusion ADR 0003 and REQ-0016 forbid, one layer up;
//! * **incoming relations are derived** here, because REQ-0020 stores each relation once
//!   on the outgoing side;
//! * every **edge that leaves the selection** is named — to a claim the variant excludes
//!   (legitimate, reported) or to one that is undecided (a gap needing a decision). A
//!   trace that quietly loses the parent of an applicable claim is worse than no trace.
//!
//! The view is deterministic: rows are sorted by claim id, and nothing here iterates a
//! hash map.

use crate::canon::{address_hex, canonical_json, evaluate_claim};
use crate::select::{Applies, Context, applies, parse_predicate};

/// One claim, as the view needs it: its identity, what it says, and how it selected.
#[derive(Debug, Clone)]
pub struct Row {
    pub claim_id: String,
    pub statement: String,
    pub modality: String,
    pub address: String,
    pub applies: Applies,
    pub selectors: Vec<String>,
    /// `(kind, to)` — the outgoing side only (REQ-0020).
    pub relations: Vec<(String, String)>,
    /// What the *stored* derived fields say, checked against a fresh parse: a release gate
    /// has to know whether the address on disk is the one the current rules produce.
    pub stored: crate::fill::StoredState,
    /// Provenance, for the audit policy and the view.
    pub rationale_source: String,
    pub rationale_date: String,
}

/// What a trace could not settle. Both are outcomes, not warnings: `dangling` is
/// UNVERIFIABLE, `undecided_edges` needs a decision from whoever owns the variant.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TraceFindings {
    pub dangling: Vec<String>,
    pub undecided_edges: Vec<String>,
}

/// The spec view: what applies, what is undecided, and how much is outside this variant.
pub fn render(rows: &[Row], context: &Context) -> String {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));

    let count = |state: Applies| sorted.iter().filter(|r| r.applies == state).count();
    let mut out = String::new();
    out.push_str(&format!("# Selection: {}\n", describe(context)));
    out.push_str(&format!(
        "applies {}, undecided {}, outside the selection {} \
         (an undecided claim is never reported as excluded)\n",
        count(Applies::Yes),
        count(Applies::Unknown),
        count(Applies::No),
    ));

    for (state, title) in [(Applies::Yes, "Applies"), (Applies::Unknown, "Undecided")] {
        let matching: Vec<&&Row> = sorted.iter().filter(|r| r.applies == state).collect();
        out.push_str(&format!("\n## {title} ({})\n", matching.len()));
        for row in matching {
            out.push_str(&format!(
                "- {}  {}  {}  {}{}\n",
                row.claim_id,
                short(&row.address),
                row.modality,
                row.statement,
                stored_marker(&row.stored)
            ));
            if row.selectors.is_empty() {
                out.push_str("    selector: (none — applies to every variant)\n");
            } else {
                out.push_str(&format!("    selector: {}\n", row.selectors.join("; ")));
            }
            for (kind, to) in &row.relations {
                out.push_str(&format!("    -> {kind} {to}\n"));
            }
        }
    }

    let outside: Vec<&&Row> = sorted.iter().filter(|r| r.applies == Applies::No).collect();
    out.push_str(&format!("\n## Outside the selection ({})\n", outside.len()));
    if outside.is_empty() {
        out.push_str("(none)\n");
    } else {
        let ids: Vec<&str> = outside.iter().map(|r| r.claim_id.as_str()).collect();
        out.push_str(&format!("{}\n", ids.join(", ")));
    }
    out
}

/// The traceability matrix, selection-scoped. Returns the view and what it could not
/// settle.
pub fn trace(rows: &[Row], context: &Context) -> (String, TraceFindings) {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    let known: std::collections::BTreeSet<&str> =
        sorted.iter().map(|r| r.claim_id.as_str()).collect();
    let state_of = |id: &str| sorted.iter().find(|r| r.claim_id == id).map(|r| r.applies);

    let mut findings = TraceFindings::default();
    let mut out = String::new();
    out.push_str(&format!("# Traceability: {}\n", describe(context)));
    out.push_str("edges are stored once, on the outgoing side (REQ-0020); the incoming\n");
    out.push_str("view below is derived by traversal, never stored\n");

    out.push_str("\n## Edges\n");
    let mut edges = 0usize;
    for row in &sorted {
        for (kind, to) in &row.relations {
            edges += 1;
            if !known.contains(to.as_str()) {
                findings.dangling.push(format!(
                    "{} -> {to} (referenced by {}, not in the corpus)",
                    kind, row.claim_id
                ));
                out.push_str(&format!(
                    "- {} -{kind}-> {to}  UNVERIFIABLE: no such claim\n",
                    row.claim_id
                ));
                continue;
            }
            match state_of(to) {
                Some(Applies::Unknown) | Some(Applies::No) => {
                    let target = state_of(to).expect("known");
                    if target == Applies::Unknown {
                        findings.undecided_edges.push(format!(
                            "{} -{kind}-> {to} (target undecided)",
                            row.claim_id
                        ));
                        out.push_str(&format!(
                            "- {} -{kind}-> {to}  [target undecided]\n",
                            row.claim_id
                        ));
                    } else {
                        out.push_str(&format!(
                            "- {} -{kind}-> {to}  [target outside the selection]\n",
                            row.claim_id
                        ));
                    }
                }
                _ => out.push_str(&format!("- {} -{kind}-> {to}\n", row.claim_id)),
            }
        }
    }
    if edges == 0 {
        out.push_str("(none)\n");
    }

    out.push_str("\n## Incoming (derived)\n");
    let mut any_incoming = false;
    for row in &sorted {
        let incoming: Vec<String> = sorted
            .iter()
            .flat_map(|other| {
                other
                    .relations
                    .iter()
                    .filter(|(_, to)| to == &row.claim_id)
                    .map(move |(kind, _)| format!("{kind}: {}", other.claim_id))
            })
            .collect();
        if !incoming.is_empty() {
            any_incoming = true;
            out.push_str(&format!("- {} <- {}\n", row.claim_id, incoming.join(", ")));
        }
    }
    if !any_incoming {
        out.push_str("(none)\n");
    }

    out.push_str("\n## Findings\n");
    if findings.dangling.is_empty() && findings.undecided_edges.is_empty() {
        out.push_str("(none)\n");
    }
    for line in &findings.dangling {
        out.push_str(&format!("- UNVERIFIABLE: {line}\n"));
    }
    for line in &findings.undecided_edges {
        out.push_str(&format!("- undecided edge: {line}\n"));
    }
    (out, findings)
}

fn describe(context: &Context) -> String {
    if context.is_empty() {
        return "(no context: nothing is decided by the caller)".to_string();
    }
    context
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn short(address: &str) -> String {
    address.chars().take(12).collect()
}

/// A view that showed a stale or tampered address as if it were current would be worse than
/// no view: the stored state is part of what the reader needs to see.
fn stored_marker(stored: &crate::fill::StoredState) -> String {
    match stored {
        crate::fill::StoredState::Verified => String::new(),
        crate::fill::StoredState::Candidate(why) => format!("  [candidate: {why}]"),
        crate::fill::StoredState::Mismatch(why) => format!("  [MISMATCH: {why}]"),
    }
}

/// `req render key=value ...`
pub fn run_render(args: &[String]) -> i32 {
    let context = match context_from(args) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let rows = match load_rows(&context) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    print!("{}", render(&rows, &context));
    if rows.iter().any(|r| r.applies == Applies::Unknown) {
        3
    } else {
        0
    }
}

/// `req trace key=value ...`
pub fn run_trace(args: &[String]) -> i32 {
    let context = match context_from(args) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let rows = match load_rows(&context) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let (view, findings) = trace(&rows, &context);
    print!("{view}");
    if !findings.dangling.is_empty() || !findings.undecided_edges.is_empty() {
        3
    } else {
        0
    }
}

/// Same `key=value` argv shape as `req select`, and the same grammar: the two must not
/// drift, so the *behaviour* on an uncovered form is pinned by a test in both.
fn context_from(args: &[String]) -> Result<Context, i32> {
    let mut context = Context::new();
    for arg in args {
        match arg.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                context.insert(key.to_string(), value.to_string());
            }
            _ => {
                eprintln!("usage: req render|trace [key=value ...]");
                return Err(2);
            }
        }
    }
    Ok(context)
}

pub fn load_rows(context: &Context) -> Result<Vec<Row>, String> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let registry = crate::canon::load_registry(&root)?;
    let files = crate::canon::claim_files(&root)?;
    let mut rows = Vec::new();
    for path in files.iter() {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let claim: crate::model::ClaimFile =
            yaml_serde::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
        let verdict = evaluate_claim(&claim, &registry);
        let form_json = canonical_json(&verdict.form);
        let address = address_hex(&form_json);
        let predicates: Vec<_> = claim
            .applicability
            .iter()
            .map(|p| parse_predicate(p))
            .collect();
        rows.push(Row {
            claim_id: claim.claim_id.clone(),
            statement: claim.statement.replace('\n', " "),
            modality: claim.modality.clone(),
            address,
            applies: applies(&predicates, context),
            selectors: claim.applicability.clone(),
            relations: claim
                .relations
                .iter()
                .map(|r| (r.kind.clone(), r.to.clone()))
                .collect(),
            // The same check `req canon --verify` runs, so the view and the gate cannot
            // disagree about whether a stored address is current.
            stored: crate::fill::verify_text(&text, &form_json, crate::canon::RULES_V),
            rationale_source: claim.rationale.source.clone(),
            rationale_date: claim.rationale.date.clone(),
        });
    }
    Ok(rows)
}
