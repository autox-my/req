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
    /// Per weakening class: the false carries and the support behind the zero (ADR 0040
    /// decision 5). Always the three classes, in `WEAKENING_CLASSES` order, so a class with no
    /// pairs is *present and not admitted* rather than absent. Reported, not a gate verdict.
    pub false_carry: Vec<FalseCarry>,
    pub rows: Vec<String>,
    pub verdict: Verdict,
    /// The FMR gate, evaluated once with `verdict` so the record cannot re-derive it.
    pub fmr_gate: Verdict,
    /// The direction gate, evaluated once with `verdict` so the record cannot
    /// re-derive "is there a direction defect" from the same lists differently.
    pub direction_gate: Verdict,
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

/// The fewest scored opposing pairs a weakening class needs behind a zero false-carry count
/// before it is `admitted` (ADR 0040 implementation note; owner go-ahead 2026-11-11). Zero
/// out of five is not evidence; 100 is the floor the owner agreed.
pub const MIN_FALSE_CARRY_SUPPORT: usize = 100;

/// One weakening class's false-carry record (ADR 0040 decision 5): a weakening class may
/// carry a satisfaction link only if nothing that is not a weakening is classified into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalseCarry {
    /// The weakening class the carry is counted under — the **reported** one.
    pub class: &'static str,
    /// Pairs labelled a non-weakening class and reported as `class`.
    pub count: usize,
    /// Scored pairs labelled the opposite of `class` (strengthening, tightening, narrowing).
    pub support: usize,
    /// `count == 0` **and** `support >= MIN_FALSE_CARRY_SUPPORT`.
    pub admitted: bool,
    /// The carries, named: `<pair id>: expected <label>, reported <class>`.
    pub pairs: Vec<String>,
}

/// The weakening classes and, for each, the label that is its opposite.
fn weakening_classes() -> [(crate::compare::Direction, crate::compare::Direction); 3] {
    use crate::compare::Direction::*;
    [
        (ModalityWeakening, ModalityStrengthening),
        (NumericLoosening, NumericTightening),
        (MembershipWidening, MembershipNarrowing),
    ]
}

/// Labels a weakening class must never be reported for: a strengthening, a tightening, a
/// polarity flip or a scope change.
fn is_non_weakening_label(label: crate::compare::Direction) -> bool {
    use crate::compare::Direction::*;
    matches!(
        label,
        ModalityStrengthening
            | NumericTightening
            | MembershipNarrowing
            | PolarityFlip
            | ScopeNarrowing
            | ScopeWidening
            | ScopeDivergence
    )
}

/// The direction tally, carried through both pair loops as one value.
#[derive(Default)]
struct DirectionTally {
    /// (reported class, `pair: expected, reported`) for each false carry.
    carries: Vec<(crate::compare::Direction, String)>,
    /// Expected labels of the scored pairs, for the support counts.
    scored_labels: Vec<crate::compare::Direction>,
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
    tally.scored_labels.push(expected);
    if is_non_weakening_label(expected) && weakening_classes().iter().any(|(w, _)| *w == reported) {
        tally.carries.push((
            reported,
            format!(
                "{}: expected {}, reported {}",
                pair.id,
                expected.as_str(),
                reported.as_str()
            ),
        ));
    }
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
    //
    // The same fold hides in a *non-empty* denominator made entirely of pairs whose
    // sides cannot be parsed: `fmr_gate` is the single expression of the rule, and both
    // the corpus verdict and the record read it — neither re-derives it.
    //
    // Both gates are evaluated here, once, and carried on `Evaluation`: `run` records
    // what was evaluated rather than spelling "is there a defect" a second time from the
    // same lists.
    let direction_defect = !tally.dangerous.is_empty() || !tally.label_errors.is_empty();
    let fmr = fmr_gate(
        fmr_hits.len(),
        corpus.not_equivalent.len(),
        unverifiable_pairs.len(),
    );
    let verdict = if direction_defect { Verdict::Fail } else { fmr };
    let direction = direction_gate(tally.checked, direction_defect);
    let false_carry = weakening_classes()
        .iter()
        .map(|(weakening, opposite)| {
            let pairs: Vec<String> = tally
                .carries
                .iter()
                .filter(|(reported, _)| reported == weakening)
                .map(|(_, named)| named.clone())
                .collect();
            let support = tally
                .scored_labels
                .iter()
                .filter(|l| *l == opposite)
                .count();
            FalseCarry {
                class: weakening.as_str(),
                count: pairs.len(),
                support,
                admitted: pairs.is_empty() && support >= MIN_FALSE_CARRY_SUPPORT,
                pairs,
            }
        })
        .collect();

    Evaluation {
        false_carry,
        verdict,
        fmr_gate: fmr,
        direction_gate: direction,
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
/// record the result under `bench/results/`. A relative corpus path is resolved against the
/// corpus root, which is a runtime parameter like every other read path (ADR 0012).
///
/// Exit codes: 0 PASS / 1 FAIL / 3 UNVERIFIABLE. A run that found a defect is **1**
/// even when it was degraded (`reported_verdict`); **3** covers a degraded run with
/// nothing found and a vacuous FMR denominator.
pub fn run(corpus_path: Option<&str>, root: &std::path::Path) -> i32 {
    let rel = corpus_path.unwrap_or(CORPUS_REL);
    let path = if std::path::Path::new(rel).is_absolute() {
        std::path::PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    let corpus = match Corpus::load(
        path.to_str()
            .expect("a corpus path under the repository root is valid UTF-8"),
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] corpus load failed: {e}");
            return 3;
        }
    };

    // A registry failure is a *recorded degradation*, not a folded pass: the
    // pairs' subjects never depend on it, so the gate stays honest — but the
    // report carries the gap so it cannot be silently skipped.
    let (registry, degraded) = match Registry::load(
        root.join(REGISTRY_REL)
            .to_str()
            .expect("a registry path under the repository root is valid UTF-8"),
    ) {
        Ok(r) => (r, Vec::<String>::new()),
        Err(e) => (
            Registry::empty(),
            vec![format!("registry_load_failed: {e}")],
        ),
    };

    let ev = evaluate(&corpus, &registry);
    // The three rates below are printed in the report and recorded for a human: they are
    // never part of an address, so a float here is a display concern, not Tier-1 math.
    #[expect(
        clippy::as_conversions,
        clippy::float_arithmetic,
        reason = "a printed rate over corpus counts, never part of an address"
    )]
    let fmr_rate = if ev.not_equivalent_total == 0 {
        0.0
    } else {
        ev.fmr_hits.len() as f64 / ev.not_equivalent_total as f64
    };
    #[expect(
        clippy::as_conversions,
        clippy::float_arithmetic,
        reason = "a printed rate over corpus counts, never part of an address"
    )]
    let fsr_rate = if ev.structured_equivalent == 0 {
        0.0
    } else {
        ev.fsr_hits.len() as f64 / ev.structured_equivalent as f64
    };
    #[expect(
        clippy::as_conversions,
        clippy::float_arithmetic,
        reason = "a printed rate over corpus counts, never part of an address"
    )]
    let direction_accuracy = if ev.direction_checked == 0 {
        0.0
    } else {
        ev.direction_correct as f64 / ev.direction_checked as f64
    };
    // The record's provenance pins the exact corpus bytes hashed, so a read failure here is
    // UNVERIFIABLE — hashing empty bytes would append a record whose corpus_sha256 pins no
    // real corpus, which is a provenance field that says nothing.
    let corpus_sha = match std::fs::read(&path) {
        Ok(bytes) => crate::digest::sha256_hex(&bytes),
        Err(e) => {
            eprintln!("[UNVERIFIABLE] cannot read {}: {e}", path.display());
            return 3;
        }
    };
    let pair_total = corpus.equivalent.len() + corpus.not_equivalent.len();
    let degraded_run = !degraded.is_empty();
    let verdict = reported_verdict(ev.verdict, degraded_run);

    let fmr_n = ev.fmr_hits.len();
    let fmr_pairs = ev.fmr_hits;
    let fsr_n = ev.fsr_hits.len();
    let fsr_pairs = ev.fsr_hits;
    let dangerous = ev.dangerous;
    let direction_unclassifiable = ev.direction_unclassifiable;
    let direction_mismatches = ev.direction_mismatches;
    let direction_label_errors = ev.direction_label_errors;
    let false_carry: serde_json::Map<String, serde_json::Value> = ev
        .false_carry
        .iter()
        .map(|c| {
            (
                c.class.to_string(),
                json!({
                    "count": c.count,
                    "support": c.support,
                    "admitted": c.admitted,
                    "pairs": c.pairs,
                }),
            )
        })
        .collect();
    // The FMR gate comes from `evaluate` with the verdict: the record reads what was
    // evaluated instead of calling `fmr_gate` a second time from the same numbers, and
    // it is a `Verdict`, not a literal, so the vocabulary has one home in
    // `Verdict::as_str`. These records are append-only history.
    let hard_gate = ev.fmr_gate;
    let report = json!({
        "schema": RECORD_SCHEMA,
        "rules_v": RULES_V,
        "corpus": rel,
        "corpus_sha256": corpus_sha,
        "pairs_total": pair_total,
        "fmr": {
            "collapses": fmr_n,
            "not_equivalent_total": ev.not_equivalent_total,
            // `null` when the denominator measured nothing — the vacuous 0.0 beside
            // `hard_gate: "UNVERIFIABLE"` is the fold this change removes.
            //
            // The rate's denominator is *every* `not_equivalent` pair, unverifiable
            // ones included, so on a corpus with mixed coverage it is diluted against
            // the rate over comparable pairs — `structurally_unverifiable_pairs` is
            // what a reader subtracts to get the latter, and the gate itself is
            // count-based, so the dilution changes no verdict.
            "rate": reported_rate(
                fmr_measurable(ev.not_equivalent_total, ev.unverifiable_pairs.len()),
                fmr_rate,
            ),
            "hard_gate": hard_gate.as_str(),
            "hit_pairs": fmr_pairs,
            "structurally_unverifiable_pairs": ev.unverifiable_pairs.len(),
            "structurally_unverifiable_ids": ev.unverifiable_pairs,
        },
        "fsr": {
            "splits": fsr_n,
            "structured_equivalent_total": ev.structured_equivalent,
            // The same rule as the FMR rate and the direction accuracy: an FSR over no
            // structured equivalent pairs is `null`, not a measured-looking `0.0`. A
            // `/1` consumer that learned `null` means "not measured" in two fields must
            // not find `0.0` meaning it in the third.
            "rate": reported_rate(ev.structured_equivalent > 0, fsr_rate),
            "hit_pairs": fsr_pairs,
        },
        "direction": {
            "checked": ev.direction_checked,
            "correct": ev.direction_correct,
            // `null`, not `0.0`, when nothing was checked: a vacuous accuracy reads as
            // a measured one, the same fold `reported_rate` removes from the FMR rate.
            // The corpora without `direction:` labels are exactly this case.
            "accuracy": reported_rate(ev.direction_checked > 0, direction_accuracy),
            "soft_target": 0.95,
            "hard_gate": ev.direction_gate.as_str(),
            "dangerous_confusions": dangerous,
            "mismatches": direction_mismatches,
            "unclassifiable": direction_unclassifiable,
            "unknown_labels": direction_label_errors,
            // ADR 0040 decision 5. Reported, not gated: admission of a weakening class to carry
            // a satisfaction link is that ADR's input. `admitted` is `count == 0` and
            // `support >= MIN_FALSE_CARRY_SUPPORT`; a class with no pairs is present, `admitted: false`.
            "false_carry": false_carry,
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

/// Is the FMR denominator a real measurement? A pair whose side cannot be parsed is
/// **reported** (`structurally_unverifiable_pairs`), never silently dropped — but a
/// denominator made *entirely* of such pairs, like an empty one, is 0/0: a regression
/// that swept every mutation into the unstructured fallback would otherwise read as
/// rate 0.0 and PASS. One test, used by `evaluate` and the record, so the verdict and
/// `hard_gate` cannot disagree about whether anything was measured.
pub fn fmr_measurable(not_equivalent_total: usize, structurally_unverifiable: usize) -> bool {
    not_equivalent_total > structurally_unverifiable
}

/// The discriminator the gate writes into every record, bumped with the value space
/// under it: `/0` had a binary `hard_gate`, a non-null `direction.accuracy` and a
/// degraded run that always wrote the degraded wording. A consumer migrating from `/0`
/// is the one that would read a vacuous `rate: 0.0` as a clean measurement, so the
/// generation has to be visible in the record itself.
pub const RECORD_SCHEMA: &str = "req.bench.result/1";

/// A rate is reported only when it was measured: `null` when the denominator measured
/// nothing, for the FMR rate and the direction accuracy alike. `/0` wrote `0.0` there,
/// which reads as a clean measurement rather than as a question nobody could answer.
pub fn reported_rate(measured: bool, rate: f64) -> Option<f64> {
    if measured { Some(rate) } else { None }
}

/// The FMR gate as one value: a collapse is FAIL; a denominator that measured nothing
/// is UNVERIFIABLE; no collapse over a measured denominator is PASS. `evaluate`'s
/// verdict and the record's `hard_gate` both call it, so "nothing collapsed" is spelled
/// exactly once — the pairs counted, not a float `rate == 0.0` beside it, which is the
/// same fact written twice and free to drift.
pub fn fmr_gate(
    collapses: usize,
    not_equivalent_total: usize,
    structurally_unverifiable: usize,
) -> Verdict {
    if collapses > 0 {
        Verdict::Fail
    } else if fmr_measurable(not_equivalent_total, structurally_unverifiable) {
        Verdict::Pass
    } else {
        Verdict::Unverifiable
    }
}

/// The direction gate, as its own tri-state. A dangerous confusion or an unparseable
/// label is a definite defect (FAIL); **nothing measured is UNVERIFIABLE, never a
/// vacuous PASS** — the corpora that carry no `direction:` labels reach this with
/// `checked: 0`, and a `PASS` there would read as "the direction check came back
/// clean" when no direction check happened.
///
/// What does *not* reach the exit code is this gate's UNVERIFIABLE state: the process
/// verdict answers the FMR question, so a corpus whose direction check was vacuous
/// still exits 0 when the FMR denominator measured clean. A direction **defect** does
/// drive exit 1 — `evaluate` folds it into the corpus verdict, and
/// `a_direction_label_error_outranks_the_vacuous_denominator` pins that.
pub fn direction_gate(checked: usize, defect: bool) -> Verdict {
    if defect {
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

    /// The package root at runtime. `cargo test` runs with the working directory set to the
    /// package root, so the walk-up finds `claims/`: the tests exercise the same runtime
    /// resolution the CLI uses, and no build path is baked into the test binary (ADR 0012).
    fn repo_root() -> std::path::PathBuf {
        crate::root::resolve_corpus_root(std::env::current_dir, None, None)
            .expect("the package root holds claims/")
    }

    fn corpus() -> Corpus {
        Corpus::load(repo_root().join(CORPUS_REL).to_str().unwrap()).expect("corpus loads")
    }

    fn registry() -> Registry {
        Registry::load(repo_root().join(REGISTRY_REL).to_str().unwrap()).expect("registry loads")
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
    fn claim_file_parses_stably() {
        let root = repo_root();
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
