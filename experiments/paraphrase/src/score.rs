//! The scorer (P-1 spec §4). Per generator, never pooled: the false-split rate over
//! judge-confirmed restatements with a Wilson interval, the split by structured/unstructured
//! side, which canonical-form fields differ and the ranked difference shapes, the false merges on
//! the alterations, and coverage.

use crate::data::{Actor, GeneratorFile, JudgeFile, Statements, sha256_hex};
use crate::judge::{Admission, Exclusion, Verdict, admit};
use crate::shape::{bucket, differing_fields};
use crate::stats::{fixed4, rate, wilson};
use req::canon::{
    CanonicalForm, RULES_V, address_hex, canonical_decimal, canonical_json, canonicalize_statement,
};
use req::model::Registry;
use serde::Serialize;
use std::collections::BTreeMap;

pub const RESULT_SCHEMA: &str = "req.paraphrase.result/1";

/// How many difference shapes the report ranks, and how many examples each carries.
pub const TOP_SHAPES: usize = 10;
pub const EXAMPLES_PER_SHAPE: usize = 2;

/// Carried verbatim in every record: a number without its caveat is the thing this project
/// exists to prevent.
pub const EVIDENCE_GRADE: &str = "Model-authored paraphrase, blind to Tier-1 (generators were never shown the rules or any canonical form), filtered by a judge from a different model family; NOT human-written. Generators share training-data biases with each other and with the judges. The headline is a lower bound on how far real writers diverge from one another, not an estimate of it. It validates nothing and refutes nothing about the bet: Tier-1's own scope excludes paraphrase, voice and word order by design.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    Unverifiable,
}

impl Outcome {
    pub fn exit_code(self) -> i32 {
        match self {
            Outcome::Pass => 0,
            Outcome::Fail => 1,
            Outcome::Unverifiable => 3,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Record {
    pub schema: &'static str,
    pub rules_v: u32,
    pub evidence_grade: &'static str,
    pub statements_sha256: String,
    pub statements: usize,
    pub verdict: Outcome,
    /// One report per generator. Deliberately no pooled figure anywhere in this record.
    pub generators: Vec<GeneratorReport>,
}

#[derive(Debug, Serialize)]
pub struct GeneratorReport {
    pub generator: Actor,
    pub file_sha256: String,
    pub judges_used: Vec<String>,
    pub judges_not_used_same_family: Vec<String>,
    pub coverage: Coverage,
    pub unverifiable: Option<String>,
    pub headline: Option<Headline>,
    /// Restatements outside the headline, each with its reason.
    pub excluded: Vec<Excluded>,
    pub alterations: AlterationReport,
}

#[derive(Debug, Serialize)]
pub struct Coverage {
    pub statements_total: usize,
    pub statements_covered: usize,
    pub restatements_generated: usize,
    pub admitted: usize,
    pub excluded_rejected: usize,
    pub excluded_unsure: usize,
    pub excluded_unjudged: usize,
}

#[derive(Debug, Serialize)]
pub struct Headline {
    pub n: u32,
    pub false_splits: u32,
    pub false_split_rate: String,
    pub wilson95_lo: String,
    pub wilson95_hi: String,
    pub by_structure: Vec<StructureRow>,
    pub shapes: Vec<ShapeRow>,
    pub splits: Vec<SplitDetail>,
}

#[derive(Debug, Serialize)]
pub struct StructureRow {
    pub class: &'static str,
    pub n: u32,
    pub false_splits: u32,
    pub false_split_rate: String,
    pub wilson95_lo: String,
    pub wilson95_hi: String,
}

#[derive(Debug, Serialize)]
pub struct ShapeRow {
    pub shape: String,
    pub count: u32,
    pub examples: Vec<SplitDetail>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitDetail {
    pub item_id: String,
    pub class: &'static str,
    pub shape: String,
    pub statement: String,
    pub restatement: String,
    pub differences: Vec<FieldDiff>,
    pub restatement_lints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDiff {
    pub field: &'static str,
    pub original: String,
    pub restated: String,
}

#[derive(Debug, Serialize)]
pub struct Excluded {
    pub item_id: String,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct AlterationReport {
    pub total: u32,
    /// Alterations whose address equals the original's: false merges, reported by id.
    pub false_merges: Vec<FalseMerge>,
}

#[derive(Debug, Serialize)]
pub struct FalseMerge {
    pub item_id: String,
    pub element: String,
    pub statement: String,
    pub alteration: String,
    /// What the judges said about whether the alteration kept the obligation (an annotation: a
    /// collapse is a finding either way).
    pub judge_verdicts: Vec<String>,
}

pub struct Inputs<'a> {
    pub statements: &'a Statements,
    pub statements_sha: &'a str,
    pub generators: &'a [(GeneratorFile, String)],
    pub judges: &'a [JudgeFile],
    pub registry: &'a Registry,
}

struct Canon {
    form: CanonicalForm,
    address: String,
    lints: Vec<String>,
}

fn canon(text: &str, registry: &Registry) -> Canon {
    let (form, lints) = canonicalize_statement(text, registry);
    let address = address_hex(&canonical_json(&form));
    Canon {
        form,
        address,
        lints: lints.into_iter().map(|l| l.code).collect(),
    }
}

pub fn structure_class(original_unstructured: bool, restated_unstructured: bool) -> &'static str {
    match (original_unstructured, restated_unstructured) {
        (false, false) => "both_structured",
        (false, true) => "restatement_unstructured",
        (true, false) => "original_unstructured",
        (true, true) => "both_unstructured",
    }
}

const CLASSES: [&str; 4] = [
    "both_structured",
    "restatement_unstructured",
    "original_unstructured",
    "both_unstructured",
];

fn field_value(f: &CanonicalForm, field: &str) -> String {
    match field {
        "action" => f.action.clone(),
        "applicability" => f
            .applicability
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(" | "),
        "conditions" => f.conditions.iter().cloned().collect::<Vec<_>>().join(" | "),
        "exception" => f.exception.clone().unwrap_or_else(|| "none".into()),
        "modality" => f.modality_str().to_string(),
        "operator" => f.operator_str().to_string(),
        "qualifier" => f.qualifier.clone(),
        "subject" => f.subject.clone(),
        "temporal" => f.temporal.clone().unwrap_or_else(|| "none".into()),
        "unit" => f.unit.clone().unwrap_or_else(|| "none".into()),
        "value" => f
            .value
            .as_ref()
            .map(canonical_decimal)
            .unwrap_or_else(|| "none".into()),
        "raw" => f.raw.clone(),
        "structure" => if f.unstructured {
            "unstructured"
        } else {
            "structured"
        }
        .to_string(),
        _ => String::new(),
    }
}

/// Ranks shapes by count (descending), then name; keeps `TOP_SHAPES`, each with the first
/// `EXAMPLES_PER_SHAPE` splits (in item-id order). Pure.
pub fn rank_shapes(splits: &[SplitDetail]) -> Vec<ShapeRow> {
    let mut by: BTreeMap<&str, Vec<&SplitDetail>> = BTreeMap::new();
    for s in splits {
        by.entry(s.shape.as_str()).or_default().push(s);
    }
    let mut rows: Vec<ShapeRow> = by
        .into_iter()
        .map(|(shape, mut v)| {
            v.sort_by(|a, b| a.item_id.cmp(&b.item_id));
            ShapeRow {
                shape: shape.to_string(),
                count: v.len() as u32,
                examples: v.into_iter().take(EXAMPLES_PER_SHAPE).cloned().collect(),
            }
        })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.shape.cmp(&b.shape)));
    rows.truncate(TOP_SHAPES);
    rows
}

fn structure_row(class: &'static str, n: u32, k: u32) -> StructureRow {
    let (lo, hi) = match wilson(k, n) {
        Some(i) => (fixed4(i.lo), fixed4(i.hi)),
        None => ("n/a".into(), "n/a".into()),
    };
    StructureRow {
        class,
        n,
        false_splits: k,
        false_split_rate: rate(k, n),
        wilson95_lo: lo,
        wilson95_hi: hi,
    }
}

fn verdict_word(v: Option<Verdict>) -> &'static str {
    match v {
        Some(Verdict::Yes) => "yes",
        Some(Verdict::No) => "no",
        Some(Verdict::Unsure) => "unsure",
        None => "none",
    }
}

/// Judges eligible for a generator: a different model family. A same-family judge is named, not
/// used.
pub fn eligible<'a>(
    g: &Actor,
    judges: &'a [JudgeFile],
) -> (Vec<&'a JudgeFile>, Vec<&'a JudgeFile>) {
    judges
        .iter()
        .partition(|j| !j.judge.family.eq_ignore_ascii_case(&g.family))
}

pub fn score(inp: &Inputs) -> Record {
    let verdict_of: BTreeMap<(&str, &str, &str), Verdict> = inp
        .judges
        .iter()
        .flat_map(|j| {
            j.verdicts.iter().map(move |v| {
                (
                    (
                        j.judge.id.as_str(),
                        v.generator.as_str(),
                        v.item_id.as_str(),
                    ),
                    v.verdict,
                )
            })
        })
        .collect();

    let mut reports = Vec::new();
    for (g, file_sha) in inp.generators {
        let (panel, same_family) = eligible(&g.generator, inp.judges);
        let by_id: BTreeMap<&str, &str> = inp
            .statements
            .statements
            .iter()
            .map(|s| (s.id.as_str(), s.text.as_str()))
            .collect();

        let mut coverage = Coverage {
            statements_total: inp.statements.statements.len(),
            statements_covered: 0,
            restatements_generated: 0,
            admitted: 0,
            excluded_rejected: 0,
            excluded_unsure: 0,
            excluded_unjudged: 0,
        };
        let mut excluded = Vec::new();
        let mut splits: Vec<SplitDetail> = Vec::new();
        let mut admitted_by_class: BTreeMap<&'static str, (u32, u32)> =
            CLASSES.iter().map(|c| (*c, (0, 0))).collect();
        let mut merges = Vec::new();
        let mut alt_total = 0u32;
        let mut unknown_items: Vec<String> = Vec::new();

        for item in &g.items {
            let Some(&stext) = by_id.get(item.statement_id.as_str()) else {
                unknown_items.push(item.statement_id.clone());
                continue;
            };
            coverage.statements_covered += 1;
            let orig = canon(stext, inp.registry);

            for r in &item.restatements {
                coverage.restatements_generated += 1;
                let verdicts: Vec<Option<Verdict>> = panel
                    .iter()
                    .map(|j| {
                        verdict_of
                            .get(&(j.judge.id.as_str(), g.generator.id.as_str(), r.id.as_str()))
                            .copied()
                    })
                    .collect();
                match admit(&verdicts) {
                    Admission::Excluded(why) => {
                        match why {
                            Exclusion::Rejected => coverage.excluded_rejected += 1,
                            Exclusion::Unsure => coverage.excluded_unsure += 1,
                            Exclusion::Unjudged => coverage.excluded_unjudged += 1,
                        }
                        excluded.push(Excluded {
                            item_id: r.id.clone(),
                            reason: why.as_str(),
                        });
                    }
                    Admission::Admitted => {
                        coverage.admitted += 1;
                        let rc = canon(&r.text, inp.registry);
                        let class = structure_class(orig.form.unstructured, rc.form.unstructured);
                        let entry = admitted_by_class.entry(class).or_default();
                        entry.0 += 1;
                        if rc.address != orig.address {
                            entry.1 += 1;
                            let fields = differing_fields(&orig.form, &rc.form);
                            splits.push(SplitDetail {
                                item_id: r.id.clone(),
                                class,
                                shape: bucket(&orig.form, &rc.form),
                                statement: stext.to_string(),
                                restatement: r.text.clone(),
                                differences: fields
                                    .iter()
                                    .map(|f| FieldDiff {
                                        field: f,
                                        original: field_value(&orig.form, f),
                                        restated: field_value(&rc.form, f),
                                    })
                                    .collect(),
                                restatement_lints: rc.lints,
                            });
                        }
                    }
                }
            }

            // Alterations are never judge-filtered: a collapse is a finding, never averaged away.
            alt_total += 1;
            let ac = canon(&item.alteration.text, inp.registry);
            if ac.address == orig.address {
                merges.push(FalseMerge {
                    item_id: item.alteration.id.clone(),
                    element: item.alteration.element.clone(),
                    statement: stext.to_string(),
                    alteration: item.alteration.text.clone(),
                    judge_verdicts: panel
                        .iter()
                        .map(|j| {
                            let v = verdict_of
                                .get(&(
                                    j.judge.id.as_str(),
                                    g.generator.id.as_str(),
                                    item.alteration.id.as_str(),
                                ))
                                .copied();
                            format!("{}={}", j.judge.id, verdict_word(v))
                        })
                        .collect(),
                });
            }
        }

        splits.sort_by(|a, b| a.item_id.cmp(&b.item_id));
        let n = coverage.admitted as u32;
        let k = splits.len() as u32;
        let unverifiable = if !unknown_items.is_empty() {
            Some(format!(
                "{} item(s) name a statement that is not in the sample ({}): the file was not validated against this sample, so its counts cannot be trusted",
                unknown_items.len(),
                unknown_items.join(", ")
            ))
        } else if panel.is_empty() {
            Some("no judge from a different model family than the generator: nothing can be admitted to the headline".to_string())
        } else if n == 0 {
            Some(
                "no restatement was admitted by the judges: there is no rate to report".to_string(),
            )
        } else {
            None
        };
        let headline = if unverifiable.is_none() {
            let i = wilson(k, n);
            Some(Headline {
                n,
                false_splits: k,
                false_split_rate: rate(k, n),
                wilson95_lo: i.map_or("n/a".into(), |i| fixed4(i.lo)),
                wilson95_hi: i.map_or("n/a".into(), |i| fixed4(i.hi)),
                by_structure: CLASSES
                    .iter()
                    .map(|c| {
                        let (cn, ck) = admitted_by_class[c];
                        structure_row(c, cn, ck)
                    })
                    .collect(),
                shapes: rank_shapes(&splits),
                splits,
            })
        } else {
            None
        };

        reports.push(GeneratorReport {
            generator: g.generator.clone(),
            file_sha256: file_sha.clone(),
            judges_used: panel.iter().map(|j| j.judge.id.clone()).collect(),
            judges_not_used_same_family: same_family.iter().map(|j| j.judge.id.clone()).collect(),
            coverage,
            unverifiable,
            headline,
            excluded,
            alterations: AlterationReport {
                total: alt_total,
                false_merges: merges,
            },
        });
    }

    let verdict = if reports
        .iter()
        .any(|r| !r.alterations.false_merges.is_empty())
    {
        Outcome::Fail
    } else if reports.is_empty() || reports.iter().any(|r| r.unverifiable.is_some()) {
        Outcome::Unverifiable
    } else {
        Outcome::Pass
    };
    Record {
        schema: RESULT_SCHEMA,
        rules_v: RULES_V,
        evidence_grade: EVIDENCE_GRADE,
        statements_sha256: inp.statements_sha.to_string(),
        statements: inp.statements.statements.len(),
        verdict,
        generators: reports,
    }
}

/// The record's bytes (pretty JSON + newline) and the content-named file stem.
pub fn record_bytes(rec: &Record) -> (Vec<u8>, String) {
    let mut s = serde_json::to_string_pretty(rec).expect("record serializes");
    s.push('\n');
    let stem = format!(
        "paraphrase-rules-v{}-{}",
        rec.rules_v,
        &sha256_hex(s.as_bytes())[..12]
    );
    (s.into_bytes(), stem)
}

pub fn render_text(rec: &Record) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let _ = writeln!(
        o,
        "paraphrase probe (P-1) — rules_v {}, {} statements",
        rec.rules_v, rec.statements
    );
    let _ = writeln!(o, "evidence grade: {}", rec.evidence_grade);
    let _ = writeln!(o, "per generator, never pooled:");
    for g in &rec.generators {
        let c = &g.coverage;
        let _ = writeln!(
            o,
            "\n== generator {} ({}, {})",
            g.generator.id, g.generator.family, g.generator.model
        );
        let _ = writeln!(
            o,
            "   judges used: {}{}",
            if g.judges_used.is_empty() {
                "none".to_string()
            } else {
                g.judges_used.join(", ")
            },
            if g.judges_not_used_same_family.is_empty() {
                String::new()
            } else {
                format!(
                    "   (not used, same family: {})",
                    g.judges_not_used_same_family.join(", ")
                )
            }
        );
        let _ = writeln!(
            o,
            "   coverage: {}/{} statements; {} restatements generated, {} admitted, excluded: {} different, {} unsure, {} unjudged",
            c.statements_covered,
            c.statements_total,
            c.restatements_generated,
            c.admitted,
            c.excluded_rejected,
            c.excluded_unsure,
            c.excluded_unjudged
        );
        match (&g.headline, &g.unverifiable) {
            (Some(h), _) => {
                let _ = writeln!(
                    o,
                    "   false-split rate: {}/{} = {}  (95% Wilson [{}, {}])",
                    h.false_splits, h.n, h.false_split_rate, h.wilson95_lo, h.wilson95_hi
                );
                for r in &h.by_structure {
                    let _ = writeln!(
                        o,
                        "     {:<26} {}/{} = {}  [{}, {}]",
                        r.class,
                        r.false_splits,
                        r.n,
                        r.false_split_rate,
                        r.wilson95_lo,
                        r.wilson95_hi
                    );
                }
                let _ = writeln!(o, "   difference shapes (top {TOP_SHAPES}):");
                for s in &h.shapes {
                    let _ = writeln!(o, "     {:>3}  {}", s.count, s.shape);
                    for e in &s.examples {
                        let _ = writeln!(o, "           {} | {}", e.item_id, e.restatement);
                        for d in &e.differences {
                            let _ = writeln!(
                                o,
                                "             {}: {:?} -> {:?}",
                                d.field, d.original, d.restated
                            );
                        }
                    }
                }
            }
            (None, Some(why)) => {
                let _ = writeln!(o, "   UNVERIFIABLE: {why}");
            }
            (None, None) => {}
        }
        let a = &g.alterations;
        let _ = writeln!(
            o,
            "   alterations: {} total, {} false merge(s)",
            a.total,
            a.false_merges.len()
        );
        for m in &a.false_merges {
            let _ = writeln!(
                o,
                "     FALSE MERGE {} ({}): {:?}  [judges: {}]",
                m.item_id,
                m.element,
                m.alteration,
                if m.judge_verdicts.is_empty() {
                    "none".to_string()
                } else {
                    m.judge_verdicts.join(", ")
                }
            );
        }
    }
    let word = match rec.verdict {
        Outcome::Pass => "PASS (alteration false merges = 0; every generator measured)",
        Outcome::Fail => "FAIL (an alteration shares its original's address: a false merge)",
        Outcome::Unverifiable => "UNVERIFIABLE (a generator could not be measured)",
    };
    let _ = writeln!(o, "\nverdict: {word}");
    o
}

pub fn load_registry(path: &str) -> Result<Registry, String> {
    Registry::load(path)
}
