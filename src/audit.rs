//! The audit packet: what was ratified, by whom, and whether it still is.
//!
//! This is the last compiled view — the one the README promises alongside the spec and the
//! traceability matrix. It answers a reader's question rather than the author's: *which of
//! these claims is covered by a signature, which has drifted since, and what is the provenance
//! of each?*
//!
//! Two rules carry over from the layers beneath, and neither is negotiable here:
//!
//! * an **undecided** claim is shown, never dropped (REQ-0016 — an audit that quietly omits
//!   the undecided claims is an audit of a different system);
//! * coverage is **derived from `baseline::drift`**, not recomputed. One comparison rule, one
//!   place, so the packet and `req baseline --verify` can never disagree about whether a claim
//!   still matches what was signed.
//!
//! With no Baseline at all the packet still prints — that is the point of an audit view — but
//! it states that **nothing here is ratified**, and exits UNVERIFIABLE rather than reporting a
//! clean audit of an unsigned model.

use crate::baseline::{self, Manifest};
use crate::select::{Applies, Context};
use crate::view::Row;
use std::collections::BTreeSet;

/// Which claims a signed Baseline covers, and how well. Derived from [`baseline::drift`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    /// In the Baseline and still matching — address *and* the attested audit surface.
    pub signed: Vec<String>,
    /// In the Baseline but changed: the address, or the attested audit surface
    /// (relations/rationale, ADR 0013) — edited after signing.
    pub drifted: Vec<String>,
    /// Applying now and not in the Baseline.
    pub uncovered: Vec<String>,
    /// In the Baseline and no longer applying — the variant moved after signing.
    pub missing: Vec<String>,
}

pub fn coverage(rows: &[Row], manifest: &Manifest) -> Coverage {
    let drift = baseline::drift(rows, manifest);

    // `signed` asks membership against all three drift lists for every claim in the manifest.
    // The id sets are built once, so that is three set lookups per claim rather than three
    // linear scans of the same lists. They are scoped to this block so their borrows of `drift`
    // provably end before its fields are moved out below: a set kept alive to the end of the
    // function is the shape that reads as a borrow error to a reviewer even though the compiler
    // accepts it, and the returned ids are built first for the same reason.
    let signed: Vec<String> = {
        let changed: BTreeSet<&str> = drift.changed.iter().map(String::as_str).collect();
        let attested: BTreeSet<&str> = drift.attested_changed.iter().map(String::as_str).collect();
        let missing: BTreeSet<&str> = drift.missing.iter().map(String::as_str).collect();
        manifest
            .claims
            .iter()
            // Filter on the borrowed id first, so a claim that turns out drifted, attested or
            // missing is never cloned only to be dropped.
            .filter(|claim| {
                !changed.contains(claim.claim_id.as_str())
                    && !attested.contains(claim.claim_id.as_str())
                    && !missing.contains(claim.claim_id.as_str())
            })
            .map(|claim| claim.claim_id.clone())
            .collect()
    };

    // `changed ++ attested_changed` is exactly `drifted`: the merge takes the ids whose
    // address or attested surface moved, and `signed` above has already excluded them all.
    // Sorted after the merge: each half is in manifest-claims order, but their concatenation
    // is not, and the packet's findings are read in id order.
    let mut drifted = drift.changed;
    drifted.extend(drift.attested_changed);
    drifted.sort();
    Coverage {
        signed,
        drifted,
        uncovered: drift.added,
        missing: drift.missing,
    }
}

/// Display-only truncation of an address.
fn short(address: &str) -> &str {
    &address[..address.len().min(12)]
}

/// The packet, as text a human reads and an auditor files. Deterministic: claims in id order,
/// no hash-map iteration anywhere.
pub fn packet(rows: &[Row], context: &Context, baseline: Option<&Manifest>) -> String {
    // Sorted here rather than assumed: the packet is read by someone else, and its order must
    // not depend on the order the caller happened to load claims in. (`view::load_rows` returns
    // sorted claims today, which is exactly why an unsorted packet would hide until a different
    // caller passed a differently-ordered slice.)
    let mut applying: Vec<&Row> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .collect();
    applying.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    let mut undecided: Vec<&Row> = rows
        .iter()
        .filter(|row| row.applies == Applies::Unknown)
        .collect();
    undecided.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    let coverage = baseline.map(|manifest| coverage(rows, manifest));

    let selection = if context.is_empty() {
        "(none)".to_string()
    } else {
        context
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let mut out = String::new();
    out.push_str("# Audit packet\n");
    out.push_str(&format!("selection: {selection}\n"));
    out.push_str(&format!(
        "claims: {} applying, {} undecided, {} excluded\n",
        applying.len(),
        undecided.len(),
        rows.len() - applying.len() - undecided.len()
    ));
    match (baseline, &coverage) {
        (Some(manifest), Some(coverage)) => out.push_str(&format!(
            "baseline: {} claim(s), rules v{} — {} covered, {} drifted, {} uncovered, {} gone\n",
            manifest.claims.len(),
            manifest.rules_v,
            coverage.signed.len(),
            coverage.drifted.len(),
            coverage.uncovered.len(),
            coverage.missing.len()
        )),
        _ => out.push_str(
            "baseline: none supplied — nothing in this packet is ratified, and this is a \
             manifest of intent, not an audit of authority\n",
        ),
    }

    if baseline.is_some() {
        out.push_str(
            "coverage: which claims this Baseline's manifest contains and still matches. Whether the\n  \
             manifest's signature is valid is `req baseline --verify`'s answer, not this packet's —\n  \
             this packet reports coverage, and never asserts authority it did not check.\n",
        );
    }

    out.push_str("\n## Applying claims\n");
    if applying.is_empty() {
        out.push_str("(none)\n");
    }
    for row in &applying {
        let status = match &coverage {
            None => "unratified",
            Some(coverage) if coverage.signed.contains(&row.claim_id) => "covered",
            Some(coverage) if coverage.drifted.contains(&row.claim_id) => "DRIFTED",
            Some(_) => "uncovered",
        };
        out.push_str(&format!(
            "- {}  {}  [{}]  {}  {}\n",
            row.claim_id,
            short(&row.address),
            status,
            row.modality,
            row.statement
        ));
        out.push_str(&format!(
            "    rationale: {} ({})\n",
            if row.rationale_source.is_empty() {
                "MISSING"
            } else {
                &row.rationale_source
            },
            if row.rationale_date.is_empty() {
                "no date"
            } else {
                &row.rationale_date
            }
        ));
        for (kind, to) in &row.relations {
            out.push_str(&format!("    {kind} -> {to}\n"));
        }
    }

    if !undecided.is_empty() {
        out.push_str("\n## Undecided (never treated as absent)\n");
        for row in &undecided {
            out.push_str(&format!(
                "- {}  selector: {}  {}\n",
                row.claim_id,
                if row.selectors.is_empty() {
                    "(none)".to_string()
                } else {
                    row.selectors.join("; ")
                },
                row.statement
            ));
        }
    }

    let (failed, open) = findings(rows, baseline, coverage.as_ref());
    if !failed.is_empty() || !open.is_empty() {
        out.push_str("\n## Findings\n");
        for reason in &failed {
            out.push_str(&format!("- FAIL: {reason}\n"));
        }
        for reason in &open {
            out.push_str(&format!("- UNVERIFIABLE: {reason}\n"));
        }
    }
    out
}

/// The two lists a verdict is made of. A known defect outranks an open question, as in
/// `req gate` and `req baseline`: "we found a problem" is stronger than "we could not tell".
pub fn findings(
    rows: &[Row],
    baseline: Option<&Manifest>,
    coverage: Option<&Coverage>,
) -> (Vec<String>, Vec<String>) {
    let mut failed = Vec::new();
    let mut open = Vec::new();

    let mut applying: Vec<&Row> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .collect();
    applying.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    for row in applying {
        if row.rationale_source.trim().is_empty() || row.rationale_date.trim().is_empty() {
            failed.push(format!(
                "{}: no provenance — an audit cannot cite what a claim rests on",
                row.claim_id
            ));
        }
    }

    match (baseline, coverage) {
        (None, _) => open.push(
            "no Baseline supplied: every applying claim is unratified (sign one, then audit \
             against it)"
                .to_string(),
        ),
        (Some(_), Some(coverage)) => {
            for id in &coverage.drifted {
                // `drifted` merges the two drift classes (the address, and the attested
                // relations/rationale), so the wording covers both: for an attested-only edit
                // the ratified *text* is untouched — what no longer recomputes is the claim as
                // it was ratified.
                failed.push(format!(
                    "{id}: edited after signing — the ratified claim no longer recomputes"
                ));
            }
            for id in &coverage.missing {
                failed.push(format!(
                    "{id}: in the Baseline but no longer applies in this selection"
                ));
            }
            for id in &coverage.uncovered {
                open.push(format!("{id}: applies now but no Baseline covers it"));
            }
        }
        (Some(_), None) => {}
    }
    (failed, open)
}

/// `req audit [--baseline <file>] [key=value ...]`
///
/// Exit: **0** every applying claim signed and provable, **1** a defect (drift, or no
/// provenance), **3** nothing ratified or something uncovered, **2** usage.
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
    let mut context = Context::new();
    let mut baseline_path: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].clone();
        index += 1;
        if arg == "--baseline" {
            match args.get(index) {
                Some(path) => {
                    baseline_path = Some(path.clone());
                    index += 1;
                }
                None => {
                    eprintln!("--baseline needs a file");
                    return 2;
                }
            }
            continue;
        }
        match arg.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                context.insert(key.to_string(), value.to_string());
            }
            _ => {
                eprintln!("usage: req audit [--baseline <file>] [key=value ...]");
                return 2;
            }
        }
    }

    let manifest = match &baseline_path {
        None => None,
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => match baseline::from_json(&text) {
                Ok(manifest) => Some(manifest),
                Err(e) => {
                    eprintln!("[UNVERIFIABLE] {e}");
                    return 3;
                }
            },
            Err(e) => {
                eprintln!("[UNVERIFIABLE] cannot read {path}: {e}");
                return 3;
            }
        },
    };

    // The Baseline defines the variant being audited; see `baseline::resolve_selection`.
    let effective = match &manifest {
        None => context.clone(),
        Some(manifest) => match baseline::resolve_selection(&context, manifest) {
            Ok(selection) => selection,
            Err(message) => {
                eprintln!("{message}");
                return 2;
            }
        },
    };
    let rows = match crate::view::load_rows(&effective, root) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let coverage = manifest.as_ref().map(|manifest| coverage(&rows, manifest));
    print!("{}", packet(&rows, &effective, manifest.as_ref()));
    let (failed, open) = findings(&rows, manifest.as_ref(), coverage.as_ref());
    let code = if !failed.is_empty() {
        1
    } else if !open.is_empty() {
        3
    } else {
        0
    };
    println!(
        "\nverdict: {}",
        match code {
            0 => "VERIFIED — every applying claim is covered by the Baseline and still matches",
            1 => "FAIL — a defect in what this packet reports",
            _ => "UNVERIFIABLE — nothing here is ratified, or something ratified is uncovered",
        }
    );
    code
}
