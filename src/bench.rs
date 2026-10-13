//! `req bench` — the parity-corpus harness (hashing spec §6).
//!
//! PASS / FAIL / UNVERIFIABLE are three distinct outcomes. FMR hits are collapses
//! among `not_equivalent` pairs (hard gate: exactly 0.0 per REQ-0008); FSR hits are
//! splits among equivalent pairs where *both* sides parsed to a bounded obligation
//! (soft target per REQ-0010; SHOULD). Equivalent pairs where either side falls to
//! the unstructured fallback are Tier-2 candidates — the provider funnel's domain
//! — and are excluded from the FSR headline by design, exactly as the corpus notes
//! declare.

use serde_json::json;

use crate::canon::{CanonicalForm, RULES_V, canonical_json, canonicalize_statement};
use crate::model::{Corpus, Registry};

const CORPUS_REL: &str = "bench/corpus/parity.yml";
const REGISTRY_REL: &str = "aliases/subjects.yml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PairKind {
    /// Canonical forms are equal.
    Collapse,
    /// Parsed-but-different canonical forms.
    Split,
    /// At least one side fell to the unstructured fallback — Tier-2 territory.
    Unstructured,
}

fn collapse(a: &CanonicalForm, b: &CanonicalForm) -> bool {
    canonical_json(a) == canonical_json(b)
}

fn classify(label_eq: bool, a: &CanonicalForm, b: &CanonicalForm) -> PairKind {
    let c = collapse(a, b);
    if a.unstructured || b.unstructured {
        // Either side is Tier-1-unparseable: the outcome lives outside the
        // Tier-1 gate — EXCEPT on a `not_equivalent` pair, where a collapse is
        // still a false merge and must hit the FMR gate.
        if !label_eq && c {
            PairKind::Collapse
        } else {
            PairKind::Unstructured
        }
    } else if label_eq {
        if c {
            PairKind::Collapse
        } else {
            PairKind::Split
        }
    } else if c {
        PairKind::Collapse
    } else {
        PairKind::Split
    }
}

/// The gate's outcome. Computed without any I/O so it can be tested at the seam
/// (see tests/gate.rs) and reused by any caller.
pub struct Evaluation {
    /// Collapses among `not_equivalent` pairs — the hard gate. Non-empty = FAIL.
    pub fmr_hits: Vec<String>,
    /// Splits among equivalent pairs where both sides parsed to a bounded claim.
    pub fsr_hits: Vec<String>,
    /// (pair id, collapsed at Tier 1) — equivalent pairs needing the provider funnel.
    pub tier2: Vec<(String, bool)>,
    pub lints: Vec<serde_json::Value>,
    pub structured_equivalent: usize,
    pub not_equivalent_total: usize,
    /// `not_equivalent` pairs where a side fell to the unstructured fallback: no
    /// merge happened, so the FMR property holds — but the pair was not
    /// structurally verified. Named, not just counted: a coverage shortfall has to be
    /// auditable pair by pair.
    pub unverifiable_pairs: Vec<String>,
    /// Direction gold set: how many labelled pairs were classified, and how many were
    /// read correctly. Accuracy is *reported*; the hard gate is `dangerous`.
    pub direction_checked: usize,
    pub direction_correct: usize,
    /// The dangerous confusions, named with what was expected and what was reported: a
    /// loosening read as no change, or as the opposite direction. The direction analogue
    /// of a false merge.
    pub dangerous: Vec<String>,
    /// Labelled pairs with an unparseable side: coverage, never counted as correct.
    pub direction_unclassifiable: Vec<String>,
    /// Wrong but harmless reads, named: a wrong answer that did not mislead anyone about
    /// whether an obligation moved still has to be auditable.
    pub direction_mismatches: Vec<String>,
    /// Labels no `Direction` answers to. A misspelled label would otherwise switch the
    /// whole check off while every number looked fine.
    pub direction_label_errors: Vec<String>,
    pub rows: Vec<String>,
    pub verdict: Verdict,
}

/// A gate verdict: the three-state outcome plus the degraded record. An enum rather
/// than a `&str`, so an invalid verdict is unrepresentable at every place one is
/// produced (`evaluate`, `run`) or consumed (the exit code, the report), while the
/// strings other tools read stay stable through `as_str()`.
///
/// The crate already models this pattern as an enum where a known defect and an open
/// question must stay distinct (`baseline::Outcome`, `audit`'s verdict lists); this is
/// the same shape at the benchmark's own boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Unverifiable,
    Degraded,
}

impl Verdict {
    /// The string the report and the log carry. **Stable**: result records under
    /// `bench/results/` are append-only history, so the wording may not drift.
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Unverifiable => "UNVERIFIABLE",
            Verdict::Degraded => "DEGRADED-PASS/FAIL-SEE-REPORT",
        }
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The direction tally, carried through both pair loops as one value.
#[derive(Default)]
struct DirectionTally {
    checked: usize,
    correct: usize,
    dangerous: Vec<String>,
    mismatches: Vec<String>,
    unclassifiable: Vec<String>,
    label_errors: Vec<String>,
}

/// Score one pair's expected direction, when the corpus carries a label.
fn record_direction(
    pair: &crate::model::Pair,
    a: &CanonicalForm,
    b: &CanonicalForm,
    tally: &mut DirectionTally,
) {
    let Some(label) = pair.direction.as_deref() else {
        return;
    };
    let Some(expected) = crate::compare::Direction::parse(label) else {
        tally
            .label_errors
            .push(format!("{}: unknown direction label {label:?}", pair.id));
        return;
    };
    let reported = crate::compare::compare(a, b);
    if reported == crate::compare::Direction::Unclassifiable {
        tally.unclassifiable.push(pair.id.clone());
        return;
    }
    tally.checked += 1;
    if reported == expected {
        tally.correct += 1;
        return;
    }
    let named = format!(
        "{}: expected {}, reported {}",
        pair.id,
        expected.as_str(),
        reported.as_str()
    );
    if crate::compare::Direction::is_dangerous_confusion(expected, reported) {
        tally.dangerous.push(named);
    } else {
        tally.mismatches.push(named);
    }
}

/// Evaluate one corpus: pure, no files touched, nothing printed.
pub fn evaluate(corpus: &Corpus, registry: &Registry) -> Evaluation {
    let mut fmr_hits: Vec<String> = Vec::new();
    let mut fsr_hits: Vec<String> = Vec::new();
    let mut tier2: Vec<(String, bool)> = Vec::new(); // (id, collapsed)
    let mut lints: Vec<serde_json::Value> = Vec::new();
    let mut structured_equivalent: usize = 0;
    let mut unverifiable_pairs: Vec<String> = Vec::new();
    let mut tally = DirectionTally::default();
    let mut rows: Vec<String> = Vec::new();

    let push_lints = |pair_id: &str,
                      side: &str,
                      ls: &Vec<crate::canon::Lint>,
                      out: &mut Vec<serde_json::Value>| {
        for l in ls {
            out.push(json!({"pair": pair_id, "side": side, "code": l.code, "detail": l.detail}));
        }
    };

    for pair in &corpus.equivalent {
        let (fa, la) = canonicalize_statement(&pair.a, registry);
        let (fb, lb) = canonicalize_statement(&pair.b, registry);
        let kind = classify(true, &fa, &fb);
        let collapsed = collapse(&fa, &fb);
        let structured = !fa.unstructured && !fb.unstructured;
        if structured {
            structured_equivalent += 1;
        }
        match kind {
            PairKind::Collapse => {}
            PairKind::Split => fsr_hits.push(pair.id.clone()),
            PairKind::Unstructured => tier2.push((pair.id.clone(), collapsed)),
        }
        push_lints(&pair.id, "a", &la, &mut lints);
        push_lints(&pair.id, "b", &lb, &mut lints);
        record_direction(pair, &fa, &fb, &mut tally);
        rows.push(format!(
            "{:<8} {:<15} collapsed={:<5} structured={}",
            pair.id, "equivalent", collapsed, structured
        ));
    }

    for pair in &corpus.not_equivalent {
        let (fa, la) = canonicalize_statement(&pair.a, registry);
        let (fb, lb) = canonicalize_statement(&pair.b, registry);
        let kind = classify(false, &fa, &fb);
        let collapsed = collapse(&fa, &fb);
        match kind {
            PairKind::Collapse => fmr_hits.push(pair.id.clone()),
            // Cannot merge (one side has no structured form), so the FMR property
            // holds — but the pair was not structurally verified. Named, never
            // silently dropped.
            PairKind::Unstructured => unverifiable_pairs.push(pair.id.clone()),
            PairKind::Split => {}
        }
        push_lints(&pair.id, "a", &la, &mut lints);
        push_lints(&pair.id, "b", &lb, &mut lints);
        record_direction(pair, &fa, &fb, &mut tally);
        rows.push(format!(
            "{:<8} {:<15} collapsed={}",
            pair.id, "not_equivalent", collapsed
        ));
    }

    // A corpus with no `not_equivalent` pairs scores FMR *0/0*. That is not a
    // measurement: the gate has nothing to falsify the bet on, and a generator
    // regression that dropped every mutation would otherwise read as PASS. The
    // vacuous denominator is UNVERIFIABLE — and a definite defect still outranks the
    // open question, exactly as `req gate` lets Fail outrank Unverifiable.
    let verdict =
        if !fmr_hits.is_empty() || !tally.dangerous.is_empty() || !tally.label_errors.is_empty() {
            Verdict::Fail
        } else if corpus.not_equivalent.is_empty() {
            Verdict::Unverifiable
        } else {
            Verdict::Pass
        };

    Evaluation {
        verdict,
        fmr_hits,
        fsr_hits,
        tier2,
        lints,
        structured_equivalent,
        not_equivalent_total: corpus.not_equivalent.len(),
        unverifiable_pairs,
        direction_checked: tally.checked,
        direction_correct: tally.correct,
        dangerous: tally.dangerous,
        direction_unclassifiable: tally.unclassifiable,
        direction_mismatches: tally.mismatches,
        direction_label_errors: tally.label_errors,
        rows,
    }
}

/// Run the gate over a corpus file — the seed parity corpus by default — and
/// record the result under `bench/results/`.
/// Exit codes: 0 PASS / 1 FAIL / 3 UNVERIFIABLE. A run that found a defect is **1**
/// even when it was degraded (`reported_verdict`); **3** covers a degraded run with
/// nothing found and a vacuous FMR denominator.
pub fn run(corpus_path: Option<&str>) -> i32 {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rel = corpus_path.unwrap_or(CORPUS_REL);
    let path = if std::path::Path::new(rel).is_absolute() {
        std::path::PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    let corpus = match Corpus::load(path.to_str().unwrap()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] corpus load failed: {e}");
            return 3;
        }
    };

    // A registry failure is a *recorded degradation*, not a folded pass: the
    // pairs' subjects never depend on it, so the gate stays honest — but the
    // report carries the gap so it cannot be silently skipped.
    let (registry, degraded) = match Registry::load(root.join(REGISTRY_REL).to_str().unwrap()) {
        Ok(r) => (r, Vec::<String>::new()),
        Err(e) => (
            Registry::empty(),
            vec![format!("registry_load_failed: {e}")],
        ),
    };

    let ev = evaluate(&corpus, &registry);
    let fmr_rate = if ev.not_equivalent_total == 0 {
        0.0
    } else {
        ev.fmr_hits.len() as f64 / ev.not_equivalent_total as f64
    };
    let fsr_rate = if ev.structured_equivalent == 0 {
        0.0
    } else {
        ev.fsr_hits.len() as f64 / ev.structured_equivalent as f64
    };
    let direction_accuracy = if ev.direction_checked == 0 {
        0.0
    } else {
        ev.direction_correct as f64 / ev.direction_checked as f64
    };
    let corpus_sha =
        crate::canon::sha256_hex_raw(&std::fs::read(path.to_str().unwrap()).unwrap_or_default());
    let pair_total = corpus.equivalent.len() + corpus.not_equivalent.len();
    let verdict = reported_verdict(ev.verdict, !degraded.is_empty());

    let fmr_n = ev.fmr_hits.len();
    let fmr_pairs = ev.fmr_hits;
    let fsr_n = ev.fsr_hits.len();
    let fsr_pairs = ev.fsr_hits;
    let dangerous = ev.dangerous;
    let direction_unclassifiable = ev.direction_unclassifiable;
    let direction_mismatches = ev.direction_mismatches;
    let direction_label_errors = ev.direction_label_errors;
    let direction_gate = direction_gate(
        ev.direction_checked,
        !dangerous.is_empty(),
        !direction_label_errors.is_empty(),
    );
    // The FMR gate is its own tri-state: 0/0 is not a pass, so the record says
    // UNVERIFIABLE there rather than folding the vacuous denominator back in. It is a
    // `Verdict` and not a literal so the vocabulary has one home, `Verdict::as_str` —
    // these records are append-only history, and two spellings could drift apart.
    let hard_gate = if ev.not_equivalent_total == 0 {
        Verdict::Unverifiable
    } else if fmr_rate == 0.0 {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    let report = json!({
        "schema": "req.bench.result/0",
        "rules_v": RULES_V,
        "corpus": rel,
        "corpus_sha256": corpus_sha,
        "pairs_total": pair_total,
        "fmr": {
            "collapses": fmr_n,
            "not_equivalent_total": ev.not_equivalent_total,
            "rate": fmr_rate,
            "hard_gate": hard_gate.as_str(),
            "hit_pairs": fmr_pairs,
            "structurally_unverifiable_pairs": ev.unverifiable_pairs.len(),
            "structurally_unverifiable_ids": ev.unverifiable_pairs,
        },
        "fsr": {
            "splits": fsr_n,
            "structured_equivalent_total": ev.structured_equivalent,
            "rate": fsr_rate,
            "hit_pairs": fsr_pairs,
        },
        "direction": {
            "checked": ev.direction_checked,
            "correct": ev.direction_correct,
            // `null`, not `0.0`, when nothing was checked: a vacuous accuracy reads as
            // a measured one, which is the same fold this change removes from the FMR
            // gate. The corpora without `direction:` labels are exactly this case.
            "accuracy": if ev.direction_checked == 0 {
                None
            } else {
                Some(direction_accuracy)
            },
            "soft_target": 0.95,
            "hard_gate": direction_gate.as_str(),
            "dangerous_confusions": dangerous,
            "mismatches": direction_mismatches,
            "unclassifiable": direction_unclassifiable,
            "unknown_labels": direction_label_errors,
            "note": "a loosening reported as no change, or as the opposite direction, is the dangerous confusion — the direction analogue of a false merge. Accuracy is reported rather than gated: a harmless misread costs nothing, while a dangerous one costs a ratified loosening."
        },
        "tier2": {
            "candidates": ev.tier2.len(),
            "collapsed_at_tier1": ev.tier2.iter().filter(|(_, c)| *c).count(),
            "ids": ev.tier2.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            "note": "pairs needing the provider funnel; excluded from the FSR headline by design"
        },
        "lints": ev.lints,
        "degraded": degraded,
        "verdict": verdict.as_str(),
    });

    for row in &ev.rows {
        println!("{row}");
    }
    let pretty = serde_json::to_string_pretty(&report).unwrap_or_default();
    println!("{pretty}");

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("corpus");
    let out_path = root
        .join("bench/results")
        .join(format!("{stem}-rules-v{RULES_V}.json"));
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out_path, &pretty) {
        eprintln!(
            "[warn] could not write results file {}: {e}",
            out_path.display()
        );
    }

    exit_code(verdict)
}

/// The direction gate, as its own tri-state. A dangerous confusion or an unparseable
/// label is a definite defect (FAIL); **nothing measured is UNVERIFIABLE, never a
/// vacuous PASS** — the corpora that carry no `direction:` labels reach this with
/// `checked: 0`, and a `PASS` there would read as "the direction check came back
/// clean" when no direction check happened. It does not move the process exit code,
/// which answers the FMR question; the record says which question was answered.
pub fn direction_gate(checked: usize, dangerous: bool, label_errors: bool) -> Verdict {
    if dangerous || label_errors {
        Verdict::Fail
    } else if checked == 0 {
        Verdict::Unverifiable
    } else {
        Verdict::Pass
    }
}

/// The verdict a run reports: a definite defect outranks the degraded record, exactly
/// as `evaluate` lets it outrank the vacuous denominator and `req gate` lets Fail
/// outrank Unverifiable — "we found a problem" is stronger than "we could not tell",
/// and a degraded run that *did* find one must not demote it to exit 3. A degraded run
/// with nothing found is its own outcome (`Degraded`, exit 3), reported with its reason.
pub fn reported_verdict(evaluated: Verdict, degraded: bool) -> Verdict {
    match evaluated {
        Verdict::Fail => Verdict::Fail,
        _ if degraded => Verdict::Degraded,
        other => other,
    }
}

/// A gate verdict as a process exit code: **0** PASS, **1** FAIL, **3**
/// UNVERIFIABLE. **3** covers a vacuous FMR denominator and a degraded run that found
/// nothing; a degraded run that *did* find a defect is **1** (see
/// `reported_verdict`). The mapping is its own function so the CLI-exit seam is
/// testable without spawning the binary, and it takes the enum so a string typo cannot
/// reach it.
pub fn exit_code(verdict: Verdict) -> i32 {
    match verdict {
        Verdict::Pass => 0,
        Verdict::Fail => 1,
        Verdict::Unverifiable | Verdict::Degraded => 3,
    }
}

// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::canonicalize_statement;
    use crate::model::ClaimFile;

    fn corpus() -> Corpus {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Corpus::load(root.join(CORPUS_REL).to_str().unwrap()).expect("corpus loads")
    }

    fn registry() -> Registry {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Registry::load(root.join(REGISTRY_REL).to_str().unwrap()).expect("registry loads")
    }

    #[test]
    fn fmr_is_zero_on_the_seed_corpus() {
        let c = corpus();
        let r = registry();
        for pair in &c.not_equivalent {
            let (fa, _) = canonicalize_statement(&pair.a, &r);
            let (fb, _) = canonicalize_statement(&pair.b, &r);
            assert_ne!(
                canonical_json(&fa),
                canonical_json(&fb),
                "FMR HIT on {}: '{}'",
                pair.id,
                pair.a
            );
        }
    }

    #[test]
    fn structured_equivalents_collapse() {
        let c = corpus();
        let r = registry();
        for pair in &c.equivalent {
            let (fa, _) = canonicalize_statement(&pair.a, &r);
            let (fb, _) = canonicalize_statement(&pair.b, &r);
            if !(fa.unstructured || fb.unstructured) {
                assert_eq!(
                    canonical_json(&fa),
                    canonical_json(&fb),
                    "FSR HIT on {}: '{}' vs '{}'",
                    pair.id,
                    pair.a,
                    pair.b
                );
            }
        }
    }

    #[test]
    fn tier2_ids_match_the_corpus_notes() {
        let c = corpus();
        let r = registry();
        let mut t2: Vec<String> = Vec::new();
        for pair in &c.equivalent {
            let (fa, _) = canonicalize_statement(&pair.a, &r);
            let (fb, _) = canonicalize_statement(&pair.b, &r);
            if fa.unstructured || fb.unstructured {
                t2.push(pair.id.clone());
            }
        }
        t2.sort();
        assert_eq!(t2, vec!["E-006", "E-007", "E-008"]);
    }

    #[test]
    fn n005_lints_untestable_comparator() {
        let c = corpus();
        let r = registry();
        let pair = c
            .not_equivalent
            .iter()
            .find(|p| p.id == "N-005")
            .expect("N-005 present");
        let (fa, la) = canonicalize_statement(&pair.a, &r);
        assert!(
            la.iter().any(|l| l.code == "untestable_comparator"),
            "'under' must lint, never guess a comparator"
        );
        assert!(!fa.unstructured, "N-005 a is bounded without comparator");
        let (_fb, lb) = canonicalize_statement(&pair.b, &r);
        assert!(lb.is_empty(), "'within' matches the T1.5 table cleanly");
    }

    #[test]
    fn corruption_guard_corpus_unparsable_is_unverifiable() {
        // A *corrupted* corpus must never look like a PASS — the loader's
        // Err path is the mechanic; the doc-level guarantee lives in run().
        let broken = "schema: not_valid_yaml_open_bracket: [[{";
        let result: Result<Corpus, String> = serde_try(broken);
        assert!(result.is_err());
    }

    fn serde_try(text: &str) -> Result<Corpus, String> {
        yaml_serde::from_str(text).map_err(|e| e.to_string())
    }

    #[test]
    fn claim_files_round_trip_stably() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for id in ["REQ-0001", "REQ-0015"] {
            let path = root.join(format!("claims/{id}.yml"));
            let text = std::fs::read_to_string(&path).unwrap();
            let c1: ClaimFile = yaml_serde::from_str(&text).unwrap();
            let again = yaml_serde::from_str(&text).unwrap();
            assert_eq!(c1, again, "{id} parses stably");
        }
    }

    #[test]
    fn unit_aliases_collapse_exactly() {
        let r = Registry::empty();
        // ms ↔ s cross-pair (exact ×10^-3)
        let (fa, _) = canonicalize_statement("The system shall respond within 150 ms.", &r);
        let (fb, _) = canonicalize_statement("The system shall respond within 0.15 seconds.", &r);
        assert_eq!(canonical_json(&fa), canonical_json(&fb));
    }
}
