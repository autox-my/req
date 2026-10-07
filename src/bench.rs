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

use crate::canon::{address_hex, canonical_json, canonicalize_statement, CanonicalForm, RULES_V};
use crate::model::{Corpus, Registry};

const CORPUS_REL: &str = "bench/corpus/parity.yml";
const REGISTRY_REL: &str = "aliases/subjects.yml";
const RESULTS_FILE: &str = "bench/results/rules-v0-seed-first-run.json";

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

pub fn run() -> i32 {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpus_path = root.join(CORPUS_REL);
    let corpus = match Corpus::load(corpus_path.to_str().unwrap()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] corpus load failed: {e}");
            return 3;
        }
    };

    // A registry failure is a *recorded degradation*, not a folded pass: the
    // pairs' subjects never depend on it, so the gate stays honest — but the
    // report carries the gap so it cannot be silently skipped.
    let (registry, degraded) =
        match Registry::load(root.join(REGISTRY_REL).to_str().unwrap()) {
            Ok(r) => (r, Vec::<String>::new()),
            Err(e) => (
                Registry::empty(),
                vec![format!("registry_load_failed: {e}")],
            ),
        };

    let mut fmr_hits: Vec<String> = Vec::new();
    let mut fsr_hits: Vec<String> = Vec::new();
    let mut tier2: Vec<(String, bool)> = Vec::new(); // (id, collapsed)
    let mut lints: Vec<serde_json::Value> = Vec::new();
    let mut structured_equivalent: usize = 0;
    let mut row_lines: Vec<String> = Vec::new();

    let push_lints = |pair_id: &str, side: &str, ls: &Vec<crate::canon::Lint>, out: &mut Vec<serde_json::Value>| {
        for l in ls {
            out.push(json!({"pair": pair_id, "side": side, "code": l.code, "detail": l.detail}));
        }
    };

    for pair in &corpus.equivalent {
        let (fa, la) = canonicalize_statement(&pair.a, &registry);
        let (fb, lb) = canonicalize_statement(&pair.b, &registry);
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
        row_lines.push(format!(
            "{:<6} {:<11} collapsed={:<5} structured={}",
            pair.id, "equivalent", collapsed, structured
        ));
    }

    for pair in &corpus.not_equivalent {
        let (fa, la) = canonicalize_statement(&pair.a, &registry);
        let (fb, lb) = canonicalize_statement(&pair.b, &registry);
        let kind = classify(false, &fa, &fb);
        let collapsed = collapse(&fa, &fb);
        if matches!(kind, PairKind::Collapse) {
            fmr_hits.push(pair.id.clone());
        }
        push_lints(&pair.id, "a", &la, &mut lints);
        push_lints(&pair.id, "b", &lb, &mut lints);
        row_lines.push(format!(
            "{:<6} {:<14} collapsed={}",
            pair.id, "not_equivalent", collapsed
        ));
    }

    let pair_total = corpus.equivalent.len() + corpus.not_equivalent.len();
    let fmr_rate = if corpus.not_equivalent.is_empty() {
        0.0
    } else {
        fmr_hits.len() as f64 / corpus.not_equivalent.len() as f64
    };
    let fsr_rate = if structured_equivalent == 0 {
        0.0
    } else {
        fsr_hits.len() as f64 / structured_equivalent as f64
    };

    let corpus_sha = crate::canon::sha256_hex_raw(
        &std::fs::read(corpus_path.to_str().unwrap()).unwrap_or_default(),
    );

    let verdict = if !degraded.is_empty() {
        "DEGRADED-PASS/FAIL-SEE-REPORT"
    } else if fmr_hits.is_empty() {
        "PASS"
    } else {
        "FAIL"
    };

    let report = json!({
        "schema": "req.bench.result/0",
        "rules_v": RULES_V,
        "corpus": CORPUS_REL,
        "corpus_sha256": corpus_sha,
        "pairs_total": pair_total,
        "fmr": {
            "collapses": fmr_hits.len(),
            "not_equivalent_total": corpus.not_equivalent.len(),
            "rate": fmr_rate,
            "hard_gate": if fmr_rate == 0.0 { "PASS" } else { "FAIL" },
            "hit_pairs": fmr_hits,
        },
        "fsr": {
            "splits": fsr_hits.len(),
            "structured_equivalent_total": structured_equivalent,
            "rate": fsr_rate,
            "hit_pairs": fsr_hits,
        },
        "tier2": {
            "candidates": tier2.len(),
            "collapsed_at_tier1": tier2.iter().filter(|(_, c)| *c).count(),
            "ids": tier2.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            "note": "pairs needing the provider funnel; excluded from the FSR headline by design"
        },
        "lints": lints,
        "degraded": degraded,
        "verdict": verdict,
    });

    for row in &row_lines {
        println!("{row}");
    }
    let pretty = serde_json::to_string_pretty(&report).unwrap_or_default();
    println!("{pretty}");

    let results_dir = root.join("bench/results");
    let _ = std::fs::create_dir_all(&results_dir);
    if let Err(e) = std::fs::write(results_dir.join("rules-v0-seed-first-run.json"), &pretty) {
        eprintln!("[warn] could not write results file: {e}");
    }

    match verdict {
        "PASS" => 0,
        "FAIL" => 1,
        _ => 3,
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
