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
//!   still matches what was signed — including the case where neither may judge it at all: a
//!   manifest whose declared `rules_v` disagrees with this build's is NOT JUDGED by either,
//!   because drift is itself derived from that version.
//!
//! With no Baseline at all the packet still prints — that is the point of an audit view — but
//! it states that **nothing here is ratified**, and exits UNVERIFIABLE rather than reporting a
//! clean audit of an unsigned model.

use crate::baseline::{self, Manifest};
use crate::canon::RULES_V;
use crate::format::{self, Format, SCHEMA_AUDIT};
use crate::select::{Applies, Context};
use crate::view::Row;
use serde::Serialize;
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
    /// `Some(reason)` when the manifest declares a different canonical rules version than this
    /// build writes: **drift was not judged**, so every list above is empty because nothing was
    /// compared — never because everything matched. A reader must not mistake one for the other,
    /// and this field is what makes the difference visible.
    pub not_judged: Option<String>,
}

pub fn coverage(rows: &[Row], manifest: &Manifest) -> Coverage {
    // Drift is *derived from* the rules version, so a manifest declaring a different one has no
    // common basis with this build: the comparison is void, not inconclusive. Answer NOT JUDGED
    // rather than a packet in which every claim reads as changed — and do not recompute an
    // answer that `baseline --verify` refuses to give, which is why the guard is the shared
    // `baseline::rules_mismatch` rather than a second comparison here.
    if let Some(mismatch) = baseline::rules_mismatch(manifest) {
        return Coverage {
            not_judged: Some(mismatch.reason()),
            ..Coverage::default()
        };
    }

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
        // Reached only under a matching rules version: a mismatch returned NOT JUDGED above.
        not_judged: None,
    }
}

/// Display-only truncation of an address.
fn short(address: &str) -> &str {
    &address[..address.len().min(12)]
}

/// The packet, as text a human reads and an auditor files. Deterministic: claims in id order,
/// no hash-map iteration anywhere.
pub fn packet(rows: &[Row], context: &Context, baseline: Option<&Manifest>) -> String {
    packet_with_roots(rows, context, baseline, &BTreeSet::new())
}

/// [`packet`], with the roots `.req/completeness` declares spared from the `unrelated` list
/// (ADR 0039).
pub fn packet_with_roots(
    rows: &[Row],
    context: &Context,
    baseline: Option<&Manifest>,
    roots: &BTreeSet<String>,
) -> String {
    packet_with_omissions(rows, context, baseline, roots, &[])
}

/// [`packet_with_roots`], with the applying claims no design element declares a link for — the
/// `unsatisfied` omission of `need design` (ADR 0040), reported beside `unrelated`.
pub fn packet_with_omissions(
    rows: &[Row],
    context: &Context,
    baseline: Option<&Manifest>,
    roots: &BTreeSet<String>,
    unsatisfied: &[String],
) -> String {
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
        // A mismatched rules version is not a clean packet with zero of everything: the coverage
        // question was not answered, and the line says so instead of reading as "0 drifted".
        (Some(manifest), Some(coverage)) if coverage.not_judged.is_some() => {
            out.push_str(&format!(
                "baseline: {} claim(s), rules v{} — coverage NOT JUDGED: the manifest declares a \
             different canonical rules version than this build writes (rules v{})\n",
                manifest.claims.len(),
                manifest.rules_v,
                crate::canon::RULES_V
            ))
        }
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

    if coverage
        .as_ref()
        .is_some_and(|coverage| coverage.not_judged.is_some())
    {
        // The reason itself is printed once, in Findings, beside the same words
        // `baseline --verify` uses; repeating it here would give one fact three homes.
        out.push_str(
            "coverage: NOT JUDGED — this packet and `req baseline --verify` both decline to \
             compare a\n  manifest written under a different canonical rules version (see \
             Findings). No claim was\n  compared, so none is reported covered or drifted.\n",
        );
    } else if baseline.is_some() {
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
            // Neither covered nor uncovered: nothing was compared. Checked before the two states
            // that a comparison would produce, so an empty `signed` list cannot read as the
            // `uncovered` arm.
            Some(coverage) if coverage.not_judged.is_some() => "NOT-JUDGED",
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

    // ADR 0039 §4: the structural omission this selection can show. `in_no_baseline` is already
    // the coverage above (`uncovered`), and `in_no_release` is a property across release lines,
    // which is `req check`'s question; what remains per selection is an applying claim with no
    // relation. Reported, never part of the verdict: a top-level obligation is legitimate.
    let unrelated = unrelated_in_selection(rows, roots);
    if !unrelated.is_empty() || !unsatisfied.is_empty() {
        out.push_str("\n## Structural omissions (reported, not judged)\n");
    }
    if !unrelated.is_empty() {
        out.push_str(&format!(
            "- unrelated: {} — no relation in or out; add the missing relation, or list the claim \
             as a root in {}\n",
            unrelated.join(", "),
            crate::completeness::COMPLETENESS
        ));
    }
    if !unsatisfied.is_empty() {
        out.push_str(&format!(
            "- unsatisfied: {} — no design element declares `req-satisfies:` for it (`need design` in \
             {}; roots in {}); `req check` judges it UNVERIFIABLE\n",
            unsatisfied.join(", "),
            crate::completeness::COMPLETENESS,
            crate::satisfy::DESIGN
        ));
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

/// Applying claims with no relation in or out, minus the declared roots (ADR 0039).
fn unrelated_in_selection(rows: &[Row], roots: &BTreeSet<String>) -> Vec<String> {
    crate::completeness::unrelated(rows, roots)
        .into_iter()
        .filter(|id| {
            rows.iter()
                .any(|row| &row.claim_id == id && row.applies == Applies::Yes)
        })
        .collect()
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
        // The coverage question was not answered, so there is no drift to blame: listing the
        // ids of a comparison that never happened would read as "every claim drifted" — the
        // misdiagnosis this shares `baseline::rules_mismatch` to avoid. The provenance findings
        // above are independent of the rules version and still stand, so a definite defect still
        // outranks this open question.
        (Some(_), Some(coverage)) if coverage.not_judged.is_some() => {
            if let Some(reason) = &coverage.not_judged {
                open.push(reason.clone());
            }
        }
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

/// [`Coverage`] as the document carries it — the same five lists, `not_judged` as the reason
/// string when coverage could not be judged.
#[derive(Serialize)]
struct CoverageDocument<'a> {
    signed: &'a [String],
    drifted: &'a [String],
    uncovered: &'a [String],
    missing: &'a [String],
    not_judged: Option<&'a str>,
}

impl<'a> From<&'a Coverage> for CoverageDocument<'a> {
    fn from(coverage: &'a Coverage) -> Self {
        CoverageDocument {
            signed: &coverage.signed,
            drifted: &coverage.drifted,
            uncovered: &coverage.uncovered,
            missing: &coverage.missing,
            not_judged: coverage.not_judged.as_deref(),
        }
    }
}

/// `audit`'s document: whether anything is ratified, the coverage the packet derived, and the
/// packet's findings **in order**, each carrying the class the text packet prints ("FAIL: …" /
/// "UNVERIFIABLE: …") so the fold stays visible in the data.
#[derive(Serialize)]
struct AuditDocument<'a> {
    schema: &'static str,
    generator: &'static str,
    rules_v: u32,
    selection: &'a Context,
    ratified: bool,
    /// `None` when no Baseline was supplied: nothing was compared, which is not the same fact
    /// as a comparison with an empty result, and the document must not blur them.
    coverage: Option<CoverageDocument<'a>>,
    findings: Vec<String>,
    /// Applying claims with no relation in or out, minus `.req/completeness` roots (ADR 0039):
    /// reported, not part of the verdict.
    unrelated: Vec<String>,
    /// With `need design` (ADR 0040): applying claims no design element declares a link for.
    /// Reported, not part of the verdict; absent unless the policy asks.
    #[serde(skip_serializing_if = "Option::is_none")]
    unsatisfied: Option<Vec<String>>,
}

/// `req audit [--baseline <file>] [--format text|json] [key=value ...]`
///
/// Exit: **0** every applying claim signed and provable, **1** a defect (drift, or no
/// provenance), **2** usage; **3** when nothing is ratified, when something ratified is
/// uncovered, or when the Baseline's declared `rules_v` disagrees with this build's — the last
/// answered as coverage NOT JUDGED, because a comparison across two rule sets is void, and no
/// claim was compared rather than something found uncovered.
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
    let usage = "usage: req audit [--baseline <file>] [--format text|json] [key=value ...]";
    let (format, args) = match format::take_format(args, usage) {
        Ok(parsed) => parsed,
        Err(code) => return code,
    };
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
                eprintln!("{usage}");
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

    // `.req/completeness` names the roots `unrelated` spares; a malformed one is refused, as
    // `req check` refuses it, rather than read as "no roots".
    let policy = match std::fs::read_to_string(root.join(crate::completeness::COMPLETENESS)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::completeness::Policy::default()
        }
        Err(e) => {
            eprintln!(
                "[UNVERIFIABLE] {} cannot be read: {e}",
                crate::completeness::COMPLETENESS
            );
            return 3;
        }
        Ok(text) => {
            let known: BTreeSet<String> = rows.iter().map(|row| row.claim_id.clone()).collect();
            match crate::completeness::parse_policy(&text, &known) {
                Ok(policy) => policy,
                Err(errors) => {
                    for error in errors {
                        eprintln!("{error}");
                    }
                    return 2;
                }
            }
        }
    };

    let roots = policy.roots.clone();
    // `need design` (ADR 0040): the applying claims no element declares a link for. A malformed
    // `.req/design` is refused as `req check` refuses it; an unreadable design root leaves the
    // question open, which is UNVERIFIABLE and never an empty list.
    let unsatisfied = if policy.need_design {
        match crate::completeness::unsatisfied_in(&rows, root) {
            Ok(ids) => Some(ids),
            Err(error) => {
                for line in error.lines() {
                    eprintln!("{line}");
                }
                return error.exit();
            }
        }
    } else {
        None
    };
    let coverage = manifest.as_ref().map(|manifest| coverage(&rows, manifest));
    let (failed, open) = findings(&rows, manifest.as_ref(), coverage.as_ref());
    // Computed once, before either rendering: the verdict is the same fold in both, and the exit
    // code is returned from it in both.
    let code = if !failed.is_empty() {
        1
    } else if !open.is_empty() {
        3
    } else {
        0
    };
    if format == Format::Json {
        let document = AuditDocument {
            schema: SCHEMA_AUDIT,
            generator: "audit",
            rules_v: RULES_V,
            selection: &effective,
            ratified: manifest.is_some(),
            coverage: coverage.as_ref().map(CoverageDocument::from),
            // The packet prints these in this order (defects, then open questions); the JSON
            // carries the same strings and the same order, with the class kept as a prefix.
            findings: failed
                .iter()
                .map(|reason| format!("FAIL: {reason}"))
                .chain(open.iter().map(|reason| format!("UNVERIFIABLE: {reason}")))
                .collect(),
            unrelated: unrelated_in_selection(&rows, &roots),
            unsatisfied: unsatisfied.clone(),
        };
        println!("{}", format::document(&document));
        return code;
    }
    print!(
        "{}",
        packet_with_omissions(
            &rows,
            &effective,
            manifest.as_ref(),
            &roots,
            unsatisfied.as_deref().unwrap_or(&[])
        )
    );
    println!(
        "\nverdict: {}",
        match code {
            0 => "VERIFIED — every applying claim is covered by the Baseline and still matches",
            1 => "FAIL — a defect in what this packet reports",
            // The third state has two distinct reasons and they are not interchangeable: "we
            // could not judge coverage" is not "something is uncovered", and the packet must not
            // blur them any more than the Baseline's message may.
            _ if coverage
                .as_ref()
                .is_some_and(|coverage| coverage.not_judged.is_some()) =>
            {
                "UNVERIFIABLE — the Baseline's canonical rules version disagrees with this \
                 build's, so coverage was not judged"
            }
            _ => "UNVERIFIABLE — nothing here is ratified, or something ratified is uncovered",
        }
    );
    code
}
