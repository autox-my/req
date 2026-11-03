//! The three file schemas (statements, restatements, judgements) and their validators. A
//! validator reports every problem it finds; a file that fails is never scored (UNVERIFIABLE),
//! because a number computed over a malformed file would carry the file's defects as findings.

use crate::judge::Verdict;
use crate::protocol;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const STATEMENTS_SCHEMA: &str = "req.paraphrase.statements/1";
pub const RESTATEMENTS_SCHEMA: &str = "req.paraphrase.restatements/1";
pub const JUDGEMENTS_SCHEMA: &str = "req.paraphrase.judgements/1";

/// Restatements asked of each generator per statement.
pub const RESTATEMENTS_PER_STATEMENT: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statements {
    pub schema: String,
    pub source_corpus: String,
    pub source_corpus_sha256: String,
    pub sampling: Sampling,
    pub statements: Vec<Statement>,
}

/// How the sample was drawn, with its counts: the record of what was excluded and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sampling {
    pub method: String,
    pub requested: usize,
    pub drawn: usize,
    /// Pairs (`equivalent` + `not_equivalent`) whose `a` side was a candidate.
    pub candidates: usize,
    /// Distinct candidate texts that are single-obligation shall/must sentences of 6–40 words.
    pub eligible_texts: usize,
    pub excluded_tuned: usize,
    /// Which tuning root quotes the excluded candidates (first match in the scan order).
    pub excluded_tuned_by_root: BTreeMap<String, usize>,
    pub excluded_tuned_ids: Vec<String>,
    pub tuned_check: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    /// The corpus pair id the statement was drawn from (stable, and unique in the sample).
    pub id: String,
    pub text: String,
    pub source: String,
    pub words: usize,
}

/// A model that generated or judged: the family is what the "different family" rule compares.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub family: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorFile {
    pub schema: String,
    pub generator: Actor,
    pub statements_sha256: String,
    pub prompt: String,
    pub items: Vec<GenItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenItem {
    pub statement_id: String,
    pub restatements: Vec<Restatement>,
    pub alteration: Alteration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Restatement {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alteration {
    pub id: String,
    pub text: String,
    pub element: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeFile {
    pub schema: String,
    pub judge: Actor,
    pub statements_sha256: String,
    pub prompt: String,
    pub verdicts: Vec<JudgeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeItem {
    pub generator: String,
    pub item_id: String,
    pub verdict: Verdict,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads and parses a JSON file; returns the value and the sha256 of the exact bytes read.
pub fn read_json<T: for<'de> Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<(T, String), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let value =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok((value, sha256_hex(&bytes)))
}

/// Collapses whitespace and case, drops trailing sentence punctuation: "is this a copy".
pub fn loose(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '!', '?'])
        .to_lowercase()
}

/// `restatement ids` are derived, not authored: `<statement id>/R<n>`; the alteration is `/ALT`.
pub fn restatement_id(statement_id: &str, n: usize) -> String {
    format!("{statement_id}/R{n}")
}

pub fn alteration_id(statement_id: &str) -> String {
    format!("{statement_id}/ALT")
}

fn actor_errors(kind: &str, a: &Actor, stem: &str, errs: &mut Vec<String>) {
    let ok_id = !a.id.is_empty()
        && a.id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if !ok_id {
        errs.push(format!(
            "{kind}.id {:?} must be non-empty [a-z0-9._-]",
            a.id
        ));
    }
    if a.id != stem {
        errs.push(format!(
            "{kind}.id {:?} must equal the file name {stem:?}",
            a.id
        ));
    }
    if a.family.trim().is_empty() {
        errs.push(format!(
            "{kind}.family is empty (the different-family rule compares it)"
        ));
    }
    if a.model.trim().is_empty() {
        errs.push(format!("{kind}.model is empty"));
    }
}

pub fn validate_statements(s: &Statements) -> Vec<String> {
    let mut errs = Vec::new();
    if s.schema != STATEMENTS_SCHEMA {
        errs.push(format!(
            "schema {:?} is not {STATEMENTS_SCHEMA:?}",
            s.schema
        ));
    }
    if s.statements.is_empty() {
        errs.push("no statements".to_string());
    }
    if s.sampling.drawn != s.statements.len() {
        errs.push(format!(
            "sampling.drawn {} disagrees with {} statements",
            s.sampling.drawn,
            s.statements.len()
        ));
    }
    if s.sampling.excluded_tuned != s.sampling.excluded_tuned_ids.len() {
        errs.push("sampling.excluded_tuned disagrees with excluded_tuned_ids".to_string());
    }
    let mut ids = BTreeSet::new();
    let mut texts = BTreeSet::new();
    for st in &s.statements {
        if st.id.is_empty() || st.id.contains('/') {
            errs.push(format!(
                "statement id {:?} must be non-empty and contain no '/'",
                st.id
            ));
        }
        if !ids.insert(st.id.as_str()) {
            errs.push(format!("duplicate statement id {}", st.id));
        }
        if st.text.trim().is_empty() {
            errs.push(format!("{}: empty text", st.id));
        }
        if !texts.insert(loose(&st.text)) {
            errs.push(format!("{}: text duplicates another statement", st.id));
        }
        if st.source.trim().is_empty() {
            errs.push(format!("{}: no source citation", st.id));
        }
        if st.words != st.text.split_whitespace().count() {
            errs.push(format!(
                "{}: words {} disagrees with the text",
                st.id, st.words
            ));
        }
    }
    errs
}

/// Problems with a generator file, plus how many statements it left uncovered (a note, not an
/// error: coverage is reported per generator, never hidden).
pub fn validate_generator(
    g: &GeneratorFile,
    statements: &Statements,
    statements_sha: &str,
    stem: &str,
) -> (Vec<String>, usize) {
    let mut errs = Vec::new();
    if g.schema != RESTATEMENTS_SCHEMA {
        errs.push(format!(
            "schema {:?} is not {RESTATEMENTS_SCHEMA:?}",
            g.schema
        ));
    }
    actor_errors("generator", &g.generator, stem, &mut errs);
    if g.statements_sha256 != statements_sha {
        errs.push(
            "statements_sha256 does not match the statements file this was generated from"
                .to_string(),
        );
    }
    if g.prompt != protocol::generator_template() {
        errs.push(
            "prompt is not the protocol's generator prompt (`tasks generate` prints it)"
                .to_string(),
        );
    }
    let by_id: BTreeMap<&str, &Statement> = statements
        .statements
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();
    let mut seen = BTreeSet::new();
    let mut texts_seen = BTreeSet::new();
    // loose(statement) -> its id, for the whole sample: a generated text that reproduces a
    // *different* statement would canonicalize differently from its own and score as a false split.
    let statement_by_text: BTreeMap<String, &str> = statements
        .statements
        .iter()
        .map(|s| (loose(&s.text), s.id.as_str()))
        .collect();
    for item in &g.items {
        let Some(st) = by_id.get(item.statement_id.as_str()) else {
            errs.push(format!(
                "{}: not a statement in the sample",
                item.statement_id
            ));
            continue;
        };
        if !seen.insert(item.statement_id.as_str()) {
            errs.push(format!("{}: appears twice", item.statement_id));
        }
        if item.restatements.len() != RESTATEMENTS_PER_STATEMENT {
            errs.push(format!(
                "{}: {} restatements, the protocol asks for {RESTATEMENTS_PER_STATEMENT}",
                item.statement_id,
                item.restatements.len()
            ));
        }
        let mut all: Vec<(&str, &str)> = Vec::new();
        for (i, r) in item.restatements.iter().enumerate() {
            let want = restatement_id(&item.statement_id, i + 1);
            if r.id != want {
                errs.push(format!(
                    "{}: restatement id {:?}, expected {want:?}",
                    item.statement_id, r.id
                ));
            }
            all.push((&r.id, &r.text));
        }
        let want_alt = alteration_id(&item.statement_id);
        if item.alteration.id != want_alt {
            errs.push(format!(
                "{}: alteration id {:?}, expected {want_alt:?}",
                item.statement_id, item.alteration.id
            ));
        }
        if !protocol::ELEMENTS.contains(&item.alteration.element.as_str()) {
            errs.push(format!(
                "{}: alteration element {:?} is not one of {}",
                item.statement_id,
                item.alteration.element,
                protocol::ELEMENTS.join("|")
            ));
        }
        all.push((&item.alteration.id, &item.alteration.text));
        for (id, text) in all {
            if text.trim().is_empty() {
                errs.push(format!("{id}: empty text"));
            } else if loose(text) == loose(&st.text) {
                errs.push(format!(
                    "{id}: is a copy of the statement (modulo case/spacing/final period)"
                ));
            } else if let Some(other) = statement_by_text
                .get(&loose(text))
                .filter(|other| **other != st.id.as_str())
            {
                errs.push(format!(
                    "{id}: is a copy of another statement of the sample ({other})"
                ));
            } else if !texts_seen.insert(loose(text)) {
                errs.push(format!("{id}: duplicates another generated text"));
            }
        }
    }
    let uncovered = statements
        .statements
        .iter()
        .filter(|s| !seen.contains(s.id.as_str()))
        .count();
    (errs, uncovered)
}

/// Problems with a judge file. `generators` is the set of validated generator files the verdicts
/// may refer to.
pub fn validate_judge(
    j: &JudgeFile,
    statements_sha: &str,
    stem: &str,
    generators: &[GeneratorFile],
) -> Vec<String> {
    let mut errs = Vec::new();
    if j.schema != JUDGEMENTS_SCHEMA {
        errs.push(format!(
            "schema {:?} is not {JUDGEMENTS_SCHEMA:?}",
            j.schema
        ));
    }
    actor_errors("judge", &j.judge, stem, &mut errs);
    if j.statements_sha256 != statements_sha {
        errs.push("statements_sha256 does not match the statements file".to_string());
    }
    if j.prompt != protocol::judge_template() {
        errs.push(
            "prompt is not the protocol's judge prompt (`tasks judge` prints it)".to_string(),
        );
    }
    let mut known: BTreeSet<(String, String)> = BTreeSet::new();
    for g in generators {
        for it in &g.items {
            for r in &it.restatements {
                known.insert((g.generator.id.clone(), r.id.clone()));
            }
            known.insert((g.generator.id.clone(), it.alteration.id.clone()));
        }
    }
    let mut seen = BTreeSet::new();
    for v in &j.verdicts {
        let key = (v.generator.clone(), v.item_id.clone());
        if !known.contains(&key) {
            errs.push(format!(
                "verdict for unknown item {}:{}",
                v.generator, v.item_id
            ));
        }
        if !seen.insert(key) {
            errs.push(format!("{}:{}: judged twice", v.generator, v.item_id));
        }
    }
    errs
}
