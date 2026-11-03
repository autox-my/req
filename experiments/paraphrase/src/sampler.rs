//! The deterministic sampler (P-1 spec §1): `sha256(pair id)` ordering over the `a` sides of the
//! external-v3 corpus, taking the first N that are single-obligation shall/must sentences of 6–40
//! words and are not found quoted in the committed files the rules were developed against.

use crate::data::{STATEMENTS_SCHEMA, Sampling, Statement, Statements, loose, sha256_hex};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const MIN_WORDS: usize = 6;
pub const MAX_WORDS: usize = 40;
/// Run length (in word tokens) whose verbatim reappearance in a tuning file counts as "quoted".
pub const NGRAM: usize = 8;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub pair_id: String,
    pub text: String,
    pub source: String,
}

#[derive(Deserialize)]
struct Pair {
    id: String,
    a: String,
    #[serde(default)]
    source: String,
}

#[derive(Deserialize)]
struct Corpus {
    #[serde(default)]
    equivalent: Vec<Pair>,
    #[serde(default)]
    not_equivalent: Vec<Pair>,
}

pub fn parse_corpus(yaml: &str) -> Result<Vec<Candidate>, String> {
    let c: Corpus = yaml_serde::from_str(yaml).map_err(|e| format!("parse corpus: {e}"))?;
    Ok(c.equivalent
        .into_iter()
        .chain(c.not_equivalent)
        .map(|p| Candidate {
            pair_id: p.id,
            text: p.a,
            source: p.source,
        })
        .collect())
}

/// Lower-cased alphanumeric word tokens: the comparison is blind to case and punctuation.
pub fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn count_word(toks: &[String], w: &str) -> usize {
    toks.iter().filter(|t| t.as_str() == w).count()
}

/// One obligation, one sentence, 6–40 words. A conservative heuristic: a text it rejects is
/// merely not sampled; one it accepts is still read by a person before it is believed.
pub fn is_single_obligation(text: &str) -> bool {
    let words = text.split_whitespace().count();
    if !(MIN_WORDS..=MAX_WORDS).contains(&words) {
        return false;
    }
    // A backslash is an encoding artifact in the source prose (`\92`), not text to restate.
    if text.contains(['\n', ';', ':', '\\']) || text.contains(" - ") {
        return false;
    }
    // Exactly one sentence: a terminator may only end the text.
    let body = text.trim_end().trim_end_matches(['.', '!', '?']);
    let chars: Vec<char> = body.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?') && chars.get(i + 1).is_some_and(|n| n.is_whitespace()) {
            return false;
        }
    }
    let toks = tokens(text);
    let obligations = count_word(&toks, "shall") + count_word(&toks, "must");
    let other_modals = ["should", "may", "will", "might"]
        .iter()
        .map(|w| count_word(&toks, w))
        .sum::<usize>();
    obligations == 1 && other_modals == 0
}

/// The n-gram index of the tuning files.
#[derive(Default)]
pub struct Haystack {
    grams: BTreeSet<Vec<String>>,
    text_lower: String,
    /// Word tokens of every file, joined by single spaces with a `|` between files.
    runs: String,
    pub files: usize,
}

impl Haystack {
    pub fn add(&mut self, content: &str) {
        let toks = tokens(content);
        for w in toks.windows(NGRAM) {
            self.grams.insert(w.to_vec());
        }
        self.runs.push_str(&toks.join(" "));
        self.runs.push_str(" | ");
        self.text_lower.push_str(&content.to_lowercase());
        self.text_lower.push('\n');
        self.files += 1;
    }

    /// Is the candidate's pair id present, or any run of `NGRAM` consecutive word tokens of its
    /// text (the whole text when it is shorter than that)?
    pub fn hit(&self, c: &Candidate) -> bool {
        if self.text_lower.contains(&c.pair_id.to_lowercase()) {
            return true;
        }
        let toks = tokens(&c.text);
        if toks.len() < NGRAM {
            // Too short to window: the whole token run must appear as a run in some file.
            let needle = toks.join(" ");
            return !needle.is_empty() && self.runs.contains(&needle);
        }
        toks.windows(NGRAM).any(|w| self.grams.contains(w))
    }
}

/// Roots (relative to the repo root) whose committed text the rules were developed against.
/// Deliberately absent: the generated corpora (`external-v1..v4`, `direction-v1`, `tier2-*`).
/// They are built from the same pinned prose as the sample's own source, so scanning them would
/// exclude every candidate and say nothing about which statements a *rule* was fitted to; what
/// does say so is the text that quotes a statement as a finding — tests, docs, result records.
pub const TUNING_ROOTS: &[&str] = &[
    "src",
    "tests",
    "docs",
    "claims",
    "bench/results",
    "bench/corpus/parity.yml",
    "README.md",
    "CONTEXT.md",
    "AGENTS.md",
];

const TEXT_EXT: &[&str] = &["md", "rs", "yml", "yaml", "json", "py", "toml", "sh", "txt"];

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
    } else if let Ok(rd) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            collect(&e, out);
        }
    }
}

/// One haystack per tuning root, so a hit can name the root that holds it.
pub struct Tuning {
    pub roots: Vec<(String, Haystack)>,
    pub missing: Vec<String>,
}

impl Tuning {
    pub fn files(&self) -> usize {
        self.roots.iter().map(|(_, h)| h.files).sum()
    }

    /// The first tuning root (in `TUNING_ROOTS` order) that quotes the candidate.
    pub fn hit_root(&self, c: &Candidate) -> Option<&str> {
        self.roots
            .iter()
            .find(|(_, h)| h.hit(c))
            .map(|(r, _)| r.as_str())
    }
}

/// Builds the per-root haystacks under `repo_root`. A root that does not exist is named: an
/// unscanned root must not read as a clean scan.
pub fn build_tuning(repo_root: &Path) -> Result<Tuning, String> {
    let mut roots = Vec::new();
    let mut missing = Vec::new();
    for root in TUNING_ROOTS {
        let mut hay = Haystack::default();
        let p = repo_root.join(root);
        if !p.exists() {
            missing.push((*root).to_string());
            continue;
        }
        let mut files = Vec::new();
        collect(&p, &mut files);
        for f in files {
            let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !TEXT_EXT.contains(&ext) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&f) {
                hay.add(&content);
            }
        }
        roots.push(((*root).to_string(), hay));
    }
    let t = Tuning { roots, missing };
    if t.files() == 0 {
        return Err(
            "no tuning file could be scanned: the exclusion check cannot be performed".to_string(),
        );
    }
    Ok(t)
}

pub struct Outcome {
    pub statements: Vec<Statement>,
    pub candidates: usize,
    pub eligible_texts: usize,
    /// Excluded pair ids with the root that quotes each.
    pub excluded_tuned: Vec<(String, String)>,
}

/// The pure draw. `tuned` decides exclusion; the order is `sha256(pair id)` ascending.
pub fn draw(
    cands: &[Candidate],
    count: usize,
    tuned: &dyn Fn(&Candidate) -> Option<String>,
) -> Outcome {
    let mut order: Vec<(String, &Candidate)> = cands
        .iter()
        .map(|c| (sha256_hex(c.pair_id.as_bytes()), c))
        .collect();
    order.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.pair_id.cmp(&b.1.pair_id)));

    let eligible_texts = cands
        .iter()
        .filter(|c| is_single_obligation(&c.text))
        .map(|c| loose(&c.text))
        .collect::<BTreeSet<_>>()
        .len();

    let mut seen = BTreeSet::new();
    let mut picked = Vec::new();
    let mut excluded = Vec::new();
    for (_, c) in order {
        if picked.len() == count {
            break;
        }
        if !is_single_obligation(&c.text) || !seen.insert(loose(&c.text)) {
            continue;
        }
        if let Some(root) = tuned(c) {
            excluded.push((c.pair_id.clone(), root));
            continue;
        }
        picked.push(Statement {
            id: c.pair_id.clone(),
            text: c.text.clone(),
            source: c.source.clone(),
            words: c.text.split_whitespace().count(),
        });
    }
    Outcome {
        statements: picked,
        candidates: cands.len(),
        eligible_texts,
        excluded_tuned: excluded,
    }
}

/// Honest wording of what the exclusion check can and cannot see.
pub fn tuned_check_text(files: usize, missing: &[String]) -> String {
    let mut s = format!(
        "Scanned {files} committed text file(s) under {} for each candidate's pair id and for any run of {NGRAM} consecutive word tokens of its text (case- and punctuation-insensitive); a hit excludes the candidate. \
This sees verbatim and quoted-fragment reuse in committed files only. It cannot see tuning done from a paraphrased mention, from uncommitted work or from conversation, and the corpus's own source repositories were read while the rules were developed, so 'not tuned on' cannot be established: only 'not found quoted'. The generated corpora (external-v1..v4, direction-v1, tier2-*) are not scanned: they are built from the same pinned prose and would exclude every candidate.",
        TUNING_ROOTS.join(", ")
    );
    if !missing.is_empty() {
        s.push_str(&format!(
            " NOT SCANNED (absent at this root): {}.",
            missing.join(", ")
        ));
    }
    s
}

pub fn build(corpus_path: &Path, repo_root: &Path, count: usize) -> Result<Statements, String> {
    let bytes =
        std::fs::read(corpus_path).map_err(|e| format!("read {}: {e}", corpus_path.display()))?;
    let text = String::from_utf8(bytes.clone())
        .map_err(|e| format!("{}: not UTF-8: {e}", corpus_path.display()))?;
    let cands = parse_corpus(&text)?;
    let tuning = build_tuning(repo_root)?;
    let out = draw(&cands, count, &|c| tuning.hit_root(c).map(str::to_string));
    let mut by_root: BTreeMap<String, usize> = BTreeMap::new();
    for (_, r) in &out.excluded_tuned {
        *by_root.entry(r.clone()).or_default() += 1;
    }
    let corpus_name = corpus_path
        .strip_prefix(repo_root)
        .unwrap_or(corpus_path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Statements {
        schema: STATEMENTS_SCHEMA.to_string(),
        source_corpus: corpus_name,
        source_corpus_sha256: sha256_hex(&bytes),
        sampling: Sampling {
            method: format!(
                "sort the `a` side of every pair (equivalent + not_equivalent) by sha256(pair id) ascending; walk it, taking the first {count} that are single-obligation shall/must sentences of {MIN_WORDS}-{MAX_WORDS} words, whose text is new, and that the tuned-on check does not exclude"
            ),
            requested: count,
            drawn: out.statements.len(),
            candidates: out.candidates,
            eligible_texts: out.eligible_texts,
            excluded_tuned: out.excluded_tuned.len(),
            excluded_tuned_by_root: by_root,
            excluded_tuned_ids: out.excluded_tuned.into_iter().map(|(id, _)| id).collect(),
            tuned_check: tuned_check_text(tuning.files(), &tuning.missing),
        },
        statements: out.statements,
    })
}
