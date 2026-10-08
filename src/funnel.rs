//! The Tier-2 funnel: which pairs are worth asking a provider about? (design step 5)
//!
//! Content addressing is O(1) per claim; semantic *proposals* are pairwise, so the funnel
//! exists to keep provider calls to a shortlist. The design's shape is
//! `exact-duplicate pass → blocking → shortlist → adjudication → human ratification`, and
//! this module builds the part that is ours and must be deterministic: the **exact-duplicate
//! pass** and the **blocking**, plus the provider seam behind them.
//!
//! ## What this environment can and cannot do
//!
//! There is no local model here (no ollama/llama.cpp/vllm, no torch/transformers, and DSH's
//! own LLM plugins are all remote), and REQ-0012 forbids making a network call mandatory. So
//! the default provider is [`NoProvider`] and it answers **`Unverifiable`** — never
//! "unrelated", which would be a guess dressed as a decision. Tier-2 *metrics* (flip-rate
//! stability, precision/recall) are therefore UNVERIFIABLE in this environment rather than
//! reported as zero, and the funnel says so.
//!
//! ## The property that matters here
//!
//! Blocking is a cost control, so under-blocking looks harmless — it is not. A pair whose two
//! sides share too few terms is **never proposed**, which is a duplicate silently dropped.
//! The failure directions are therefore asymmetric like everything else: missing a block is a
//! defect, an over-large block is merely expensive. [`blocking_misses`] measures the first
//! against the corpus's own equivalent pairs, and a test pins it at zero.

use crate::select::Context;
use crate::view::{Row, load_rows};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Terms a pair must share before it is worth a provider call. Two, not one: a single shared
/// word ("system", "shall") would make a block out of half the corpus.
pub const MIN_SHARED_TERMS: usize = 2;

/// Words that carry no discriminating power in requirement prose. Deliberately short and
/// boring — a bigger list is a place for silent recall loss to hide.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "can", "do", "does", "for",
    "from", "has", "have", "in", "into", "is", "it", "its", "may", "must", "no", "not", "of", "on",
    "or", "rather", "shall", "should", "so", "than", "that", "the", "their", "them", "then",
    "there", "these", "this", "to", "was", "were", "when", "which", "while", "with", "work",
];

/// Content terms of a statement: lowercased, stopwords dropped, plurals folded by a trailing
/// `s`, order-independent. No stemming beyond that — a real stemmer is a dependency and a
/// place for two implementations to disagree.
pub fn content_terms(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
        .filter(|token| !token.is_empty())
        // `.` and `_` stay *inside* a token so "0.15" and "brake_type" survive whole; the
        // ends are trimmed because a full stop is punctuation, and one left on the end would
        // defeat the plural fold below ("documents." never folds to "document").
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|token| !token.is_empty())
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .map(|token| {
            // Fold a trailing plural, but never a number ("records" → "record", "12" stays).
            if token.len() > 3 && token.ends_with('s') && !token.ends_with("ss") {
                token[..token.len() - 1].to_string()
            } else {
                token
            }
        })
        .collect()
}

/// How many content terms two statements share.
pub fn shared_terms(a: &str, b: &str) -> usize {
    content_terms(a).intersection(&content_terms(b)).count()
}

/// An inverted index: term → the claim ids carrying it, each list sorted. This is the
/// blocking structure — a pair is a candidate when it shares at least `min_shared` terms.
pub fn inverted_index(rows: &[Row]) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        for term in content_terms(&row.statement) {
            index.entry(term).or_default().push(row.claim_id.clone());
        }
    }
    for ids in index.values_mut() {
        ids.sort();
        ids.dedup();
    }
    index
}

/// The pairs worth a provider call: at least `min_shared` shared terms, deduplicated and in a
/// stable order.
pub fn candidate_pairs(rows: &[Row], min_shared: usize) -> Vec<(String, String)> {
    let index = inverted_index(rows);
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for ids in index.values() {
        for (i, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(i + 1) {
                *counts.entry((a.clone(), b.clone())).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, shared)| *shared >= min_shared)
        .map(|(pair, _)| pair)
        .collect()
}

/// Every pair, for the cost comparison the design cares about: 5000 claims is ~12.5M pairs
/// before blocking.
pub fn naive_pairs(claims: usize) -> usize {
    claims.saturating_sub(1) * claims / 2
}

/// Equivalent pairs in a corpus that blocking would **never propose** — the silent-duplicate
/// direction, and the number this module exists to keep at zero.
pub fn blocking_misses(corpus: &crate::model::Corpus, min_shared: usize) -> Vec<String> {
    let mut misses = Vec::new();
    for pair in &corpus.equivalent {
        let shared = shared_terms(&pair.a, &pair.b);
        if shared < min_shared {
            misses.push(format!(
                "{}: only {shared} shared term(s), needs {min_shared}",
                pair.id
            ));
        }
    }
    misses
}

/// One claim as a provider sees it.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub claim_id: &'a str,
    pub statement: &'a str,
}

/// What a provider may answer. Confidence is an integer out of 100 rather than a float: a
/// proposal's confidence is not exact, and an integer keeps locale and precision questions out
/// of a record a human will sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adjudication {
    Equivalent {
        confidence: u8,
    },
    Related {
        confidence: u8,
    },
    Unrelated {
        confidence: u8,
    },
    /// No answer, and never a guess: no provider configured, a provider error, or a pair the
    /// provider declines. This is a first-class outcome, not a failure to report.
    Unverifiable {
        reason: String,
    },
}

impl Adjudication {
    pub fn as_str(&self) -> &'static str {
        match self {
            Adjudication::Equivalent { .. } => "equivalent",
            Adjudication::Related { .. } => "related",
            Adjudication::Unrelated { .. } => "unrelated",
            Adjudication::Unverifiable { .. } => "unverifiable",
        }
    }
}

/// The Tier-2 provider boundary. Pluggable by design (ADR 0004: the model proposes, a human
/// disposes), and never required for any claim operation.
pub trait Provider {
    /// The provider's identity, recorded on every proposal it produces — the design asks for
    /// the model/version string verbatim, and a proposal without provenance is not auditable.
    fn id(&self) -> String;
    fn adjudicate(&self, a: &Candidate<'_>, b: &Candidate<'_>) -> Adjudication;
}

/// The offline default, and the honest one in this environment: it refuses to guess.
pub struct NoProvider;

impl Provider for NoProvider {
    fn id(&self) -> String {
        "none".to_string()
    }

    fn adjudicate(&self, _a: &Candidate<'_>, _b: &Candidate<'_>) -> Adjudication {
        Adjudication::Unverifiable {
            reason: "no Tier-2 provider is configured; a local one is required by REQ-0012 \
                     and none is installed, so nothing is proposed rather than guessed"
                .to_string(),
        }
    }
}

/// One proposed edge, with the provider that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub a: String,
    pub b: String,
    pub adjudication: Adjudication,
    pub provider: String,
}

/// Run the funnel: skip pairs Tier-1 already resolved, block the rest, then adjudicate.
///
/// Returns `(proposals, already_identical)` — the latter counted, not adjudicated, because two
/// claims at one address are one requirement and a provider has nothing to decide.
pub fn propose(rows: &[Row], provider: &dyn Provider, min_shared: usize) -> (Vec<Proposal>, usize) {
    let by_id: BTreeMap<&str, &Row> = rows.iter().map(|r| (r.claim_id.as_str(), r)).collect();
    let mut already_identical = 0;
    let mut proposals = Vec::new();
    for (a, b) in candidate_pairs(rows, min_shared) {
        let (Some(row_a), Some(row_b)) = (by_id.get(a.as_str()), by_id.get(b.as_str())) else {
            continue;
        };
        if row_a.address == row_b.address {
            already_identical += 1;
            continue;
        }
        let candidate_a = Candidate {
            claim_id: &row_a.claim_id,
            statement: &row_a.statement,
        };
        let candidate_b = Candidate {
            claim_id: &row_b.claim_id,
            statement: &row_b.statement,
        };
        proposals.push(Proposal {
            a: row_a.claim_id.clone(),
            b: row_b.claim_id.clone(),
            adjudication: provider.adjudicate(&candidate_a, &candidate_b),
            provider: provider.id(),
        });
    }
    (proposals, already_identical)
}

/// `req propose [--min-shared N] [key=value ...]`
///
/// Exit: **0** every candidate adjudicated, **3** anything left unadjudicated (no provider, or
/// a provider that declined), **2** usage. Unadjudicated is its own outcome: `req propose` with
/// no provider prints the funnel's shape and refuses to call anything unrelated.
pub fn run(args: &[String]) -> i32 {
    let mut context = Context::new();
    let mut min_shared = MIN_SHARED_TERMS;
    let mut gold_path: Option<String> = None;
    let mut provider_cmd: Option<String> = None;
    let mut repeats = 1usize;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        index += 1;
        if arg == "--provider-cmd" {
            let Some(value) = args.get(index) else {
                eprintln!("--provider-cmd needs a command");
                return 2;
            };
            index += 1;
            provider_cmd = Some(value.clone());
            continue;
        }
        if arg == "--repeats" {
            let Some(value) = args.get(index) else {
                eprintln!("--repeats needs a count");
                return 2;
            };
            index += 1;
            match value.parse::<usize>() {
                Ok(n) if n > 0 => repeats = n,
                _ => {
                    eprintln!("--repeats must be a positive integer");
                    return 2;
                }
            }
            continue;
        }
        if arg == "--gold" {
            let Some(value) = args.get(index) else {
                eprintln!("--gold needs a file");
                return 2;
            };
            index += 1;
            gold_path = Some(value.clone());
            continue;
        }
        if arg == "--min-shared" {
            let Some(value) = args.get(index) else {
                eprintln!("usage: req propose [--min-shared N] [key=value ...]");
                return 2;
            };
            index += 1;
            match value.parse::<usize>() {
                Ok(n) if n > 0 => min_shared = n,
                _ => {
                    eprintln!("--min-shared must be a positive integer");
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
                eprintln!("usage: req propose [--min-shared N] [key=value ...]");
                return 2;
            }
        }
    }

    let rows = match load_rows(&context) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let applying: Vec<Row> = rows
        .iter()
        .filter(|r| r.applies == crate::select::Applies::Yes)
        .cloned()
        .collect();
    let (proposals, already_identical) = propose(&applying, &NoProvider, min_shared);

    let naive = naive_pairs(applying.len());
    let candidates = candidate_pairs(&applying, min_shared).len();
    println!(
        "# Tier-2 funnel: {} applying claim(s), {} candidate pair(s) of {} naive pair(s)",
        applying.len(),
        candidates,
        naive
    );
    println!("  exact-duplicate pass: {already_identical} pair(s) already share an address");
    println!(
        "  blocking: {min_shared} shared content term(s) minimum, {} terms indexed",
        inverted_index(&applying).len()
    );
    let provider = NoProvider;
    println!("  provider: {} (offline default)", provider.id());
    let unverifiable = proposals
        .iter()
        .filter(|p| matches!(p.adjudication, Adjudication::Unverifiable { .. }))
        .count();
    println!(
        "\n{} proposal(s), {unverifiable} unadjudicated:",
        proposals.len()
    );
    for proposal in proposals.iter().take(12) {
        println!(
            "- {} ~ {}  [{}]",
            proposal.a,
            proposal.b,
            proposal.adjudication.as_str()
        );
    }
    if proposals.len() > 12 {
        println!("  … {} more", proposals.len() - 12);
    }
    let mut gold_misses = 0usize;
    if let Some(path) = &gold_path {
        let corpus = match Tier2Corpus::load(path) {
            Ok(corpus) => corpus,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] {e}");
                return 3;
            }
        };
        let count = |label: Tier2Label| corpus.pairs.iter().filter(|p| p.label == label).count();
        let equivalents = count(Tier2Label::Equivalent);
        let misses = tier2_blocking_misses(&corpus, min_shared);
        gold_misses = misses.len();
        println!("\n## Tier-2 gold set: {path}");
        println!(
            "  {equivalents} equivalent, {} related, {} unrelated",
            count(Tier2Label::Related),
            count(Tier2Label::Unrelated)
        );
        println!(
            "  blocking recall: {}/{} equivalent pair(s) reachable",
            equivalents - gold_misses,
            equivalents
        );
        for miss in &misses {
            println!("  - MISS {miss}");
        }
        println!("  provider metrics: UNVERIFIABLE unless --provider-cmd is given (see below)");
    }
    let mut false_merges = 0usize;
    let mut flip_percent = 0u32;
    if let (Some(path), Some(command)) = (&gold_path, &provider_cmd) {
        let corpus = match Tier2Corpus::load(path) {
            Ok(corpus) => corpus,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] {e}");
                return 3;
            }
        };
        let provider = CommandProvider::new(command.clone());
        let score = score_gold(&corpus, &provider, min_shared);
        let (flipped, measured) = flip_count(&corpus, &provider, min_shared, repeats);
        flip_percent = ((flipped * 100).checked_div(measured)).unwrap_or(0) as u32;
        false_merges = score.false_positives;
        println!("  provider: {}", provider.id());
        println!(
            "  proposed {} pair(s); unverifiable {}; not proposed {}",
            score.scored, score.unverifiable, score.not_proposed
        );
        println!(
            "  equivalent: tp {} fp {} fn {} — precision {} recall {}",
            score.true_positives,
            score.false_positives,
            score.false_negatives,
            score
                .precision()
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "n/a".to_string()),
            score
                .recall()
                .map(|r| format!("{r}%"))
                .unwrap_or_else(|| "n/a".to_string())
        );
        for (label, ids) in [
            ("FALSE MERGE (called equivalent)", &score.false_positive_ids),
            ("MISSED equivalent", &score.false_negative_ids),
        ] {
            if !ids.is_empty() {
                println!("  {label}: {}", ids.join(", "));
            }
        }
        println!(
            "  REQ-0011 flip rate: {flipped}/{measured} pair(s) changed across {repeats} run(s) = {flip_percent}% (at most 2% required)"
        );
        if score.false_positives > 0 {
            println!("  - FALSE MERGE: the provider called a non-equivalent pair equivalent");
        }
    }
    if gold_misses > 0 || false_merges > 0 || flip_percent > 2 {
        println!(
            "\nverdict: FAIL — unreachable equivalents {gold_misses}, false merges {false_merges}, flip rate {flip_percent}%"
        );
        return 1;
    }
    println!(
        "\nverdict: {}",
        if unverifiable == 0 {
            "PASS — every candidate adjudicated"
        } else {
            "UNVERIFIABLE — no provider; every candidate is reported as unadjudicated, never as unrelated"
        }
    );
    if unverifiable == 0 { 0 } else { 3 }
}

// ---------------------------------------------------------------------------------------------
// The Tier-2 gold set: a labelled evaluation corpus for a provider.

/// Which relationship a gold-set pair asserts. Three, because the provider's answer is three-way:
/// collapsing on `related` is a false merge, and that is the error this set exists to catch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier2Label {
    Equivalent,
    Related,
    Unrelated,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tier2Pair {
    pub id: String,
    pub a: String,
    pub b: String,
    pub label: Tier2Label,
    /// The transformation that produced the pair. This is what makes the label reviewable: the
    /// corpus is reviewed through the generator and this field, never line by line.
    pub transform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tier2Corpus {
    pub pairs: Vec<Tier2Pair>,
}

impl Tier2Corpus {
    pub fn load(path: &str) -> Result<Tier2Corpus, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        yaml_serde::from_str(&text).map_err(|e| format!("parse {path}: {e}"))
    }
}

/// Equivalent pairs the gold set labels that blocking would **never propose** — the silent-duplicate
/// direction, measured on a set built for it rather than on the seed corpus's three pairs.
///
/// Related and unrelated pairs are deliberately not required to be reachable: they are not
/// duplicates, so failing to propose one costs nothing but a missed *proposal*, and an unreachable
/// negative measures the blocker's cost rather than its recall.
pub fn tier2_blocking_misses(corpus: &Tier2Corpus, min_shared: usize) -> Vec<String> {
    corpus
        .pairs
        .iter()
        .filter(|pair| pair.label == Tier2Label::Equivalent)
        .filter(|pair| shared_terms(&pair.a, &pair.b) < min_shared)
        .map(|pair| {
            format!(
                "{}: only {} shared term(s)",
                pair.id,
                shared_terms(&pair.a, &pair.b)
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Measuring a provider against the gold set.

/// A provider that is an **external command**: req writes the pair and the choice set to its stdin
/// and reads the chosen label from its stdout. Nothing about the model is baked in — swapping
/// `llama-server` for `GLiNER2.5-Decide` is one argument — and the core still links no ML and
/// opens no socket. `sh -c` is used, so the command may be a pipeline.
pub struct CommandProvider {
    command: String,
}

impl CommandProvider {
    pub fn new(command: String) -> Self {
        Self { command }
    }
}

/// The request a command provider receives. Documented because it is a contract: a model server
/// wrapper only has to read two lines and print one word.
pub fn provider_request(a: &str, b: &str) -> String {
    format!("A: {a}\nB: {b}\nCHOOSE: equivalent|related|unrelated\n")
}

/// Read a label out of a provider's answer. `unrelated` is checked before `related` because the
/// former contains the latter — the classic substring trap, and one that would silently turn every
/// `unrelated` into `related`. A trailing percentage, if the provider states one, becomes the
/// confidence; absent, confidence is 0, meaning **not stated** (not "no confidence", which is a
/// number the provider never gave).
pub fn parse_adjudication(stdout: &str) -> Adjudication {
    // A provider may answer with the documented object (a choice plus a confidence in integer
    // percent) or with the bare word. JSON is tried first, because the confidence lives there and a
    // threshold sweep needs it; a JSON-looking answer that does not parse still falls through to the
    // word scan, so a partial answer beats none.
    let trimmed = stdout.trim();
    if trimmed.starts_with('{')
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
    {
        let choice = value
            .get("choice")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();
        let confidence = value
            .get("confidence")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .min(100) as u8;
        return match choice.as_str() {
            "equivalent" => Adjudication::Equivalent { confidence },
            "related" => Adjudication::Related { confidence },
            "unrelated" => Adjudication::Unrelated { confidence },
            other => Adjudication::Unverifiable {
                reason: format!("the provider chose an unknown option: {other:?}"),
            },
        };
    }
    let lowered = stdout.to_lowercase();
    let (label, at, word) = if let Some(at) = lowered.find("unrelated") {
        (AdjudicationKind::Unrelated, at, "unrelated")
    } else if let Some(at) = lowered.find("related") {
        (AdjudicationKind::Related, at, "related")
    } else if let Some(at) = lowered.find("equivalent") {
        (AdjudicationKind::Equivalent, at, "equivalent")
    } else {
        return Adjudication::Unverifiable {
            reason: format!("no label in the provider's answer: {}", stdout.trim()),
        };
    };
    let tail = &lowered[at + word.len()..];
    let stated = tail
        .split(|c: char| !c.is_ascii_digit())
        .find(|token| !token.is_empty())
        .and_then(|token| token.parse::<u8>().ok())
        .filter(|value| *value <= 100)
        .unwrap_or(0);
    match label {
        AdjudicationKind::Equivalent => Adjudication::Equivalent { confidence: stated },
        AdjudicationKind::Related => Adjudication::Related { confidence: stated },
        AdjudicationKind::Unrelated => Adjudication::Unrelated { confidence: stated },
    }
}

enum AdjudicationKind {
    Equivalent,
    Related,
    Unrelated,
}

impl Provider for CommandProvider {
    fn id(&self) -> String {
        format!("cmd:{}", self.command)
    }

    fn adjudicate(&self, a: &Candidate<'_>, b: &Candidate<'_>) -> Adjudication {
        use std::io::Write;
        let mut child = match std::process::Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return Adjudication::Unverifiable {
                    reason: format!("cannot run the provider command: {e}"),
                };
            }
        };
        let wrote = child
            .stdin
            .as_mut()
            .map(|stdin| {
                stdin
                    .write_all(provider_request(a.statement, b.statement).as_bytes())
                    .is_ok()
            })
            .unwrap_or(false);
        if !wrote {
            // The same rule as a non-zero exit: never report a failure without the provider's own
            // words. Whether this path or the exit-status path fires is a *race* — a command that
            // exits before reading its input can lose either — so the reason must not depend on
            // which one won, and the test for it must not either.
            let detail = match child.wait_with_output() {
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if stderr.is_empty() {
                        format!(
                            "the provider command closed its input and exited {}",
                            output.status
                        )
                    } else {
                        format!("the provider command closed its input: {stderr}")
                    }
                }
                Err(e) => format!("the provider command closed its input: {e}"),
            };
            return Adjudication::Unverifiable { reason: detail };
        }
        match child.wait_with_output() {
            Ok(output) if output.status.success() => {
                parse_adjudication(&String::from_utf8_lossy(&output.stdout))
            }
            // A provider that fails must say so. Reporting `unverifiable` with no reason is how a
            // crash hides: a wrapper without its executable bit exited 126 for all 68 pairs and the
            // harness blamed the model for it — a silent, single-word failure that read as the
            // model declining to answer.
            Ok(output) => Adjudication::Unverifiable {
                reason: format!(
                    "the provider command exited {} — {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            },
            Err(e) => Adjudication::Unverifiable {
                reason: format!("the provider command failed: {e}"),
            },
        }
    }
}

/// How a provider did on the gold set. Counts, not rates: a rate over 36 pairs invites more
/// confidence than 36 pairs can carry, and the caller can divide.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GoldScore {
    /// Labelled equivalent, and the provider said equivalent.
    pub true_positives: usize,
    /// Labelled anything else, and the provider said equivalent — **the false merge**, the one
    /// error whose cost is asymmetric.
    pub false_positives: usize,
    /// Labelled equivalent, and the provider said related or unrelated — a missed duplicate.
    pub false_negatives: usize,
    /// Labelled anything else, and the provider agreed it was not equivalent.
    pub true_negatives: usize,
    /// The provider declined to answer.
    pub unverifiable: usize,
    /// Sharing too few terms for blocking, so the provider never saw it.
    pub not_proposed: usize,
    /// Pairs the provider was actually asked about.
    pub scored: usize,
    /// The pairs behind `false_positives` and `false_negatives`, by id. The count says how bad it
    /// is; the ids say what to fix, and the criteria are written against the transformations those
    /// ids name.
    pub false_positive_ids: Vec<String>,
    pub false_negative_ids: Vec<String>,
}

impl GoldScore {
    /// Integer percent, floor. No floats anywhere near a reported number, for the same reason the
    /// address path forbids them: two runs must print the same digits.
    pub fn precision(&self) -> Option<u32> {
        let denominator = self.true_positives + self.false_positives;
        (denominator > 0).then(|| (self.true_positives * 100 / denominator) as u32)
    }

    pub fn recall(&self) -> Option<u32> {
        let denominator = self.true_positives + self.false_negatives;
        (denominator > 0).then(|| (self.true_positives * 100 / denominator) as u32)
    }
}

fn predicted(adjudication: &Adjudication) -> Option<Tier2Label> {
    match adjudication {
        Adjudication::Equivalent { .. } => Some(Tier2Label::Equivalent),
        Adjudication::Related { .. } => Some(Tier2Label::Related),
        Adjudication::Unrelated { .. } => Some(Tier2Label::Unrelated),
        Adjudication::Unverifiable { .. } => None,
    }
}

/// Score a provider over the pairs blocking would propose, and count the rest as not proposed.
pub fn score_gold(corpus: &Tier2Corpus, provider: &dyn Provider, min_shared: usize) -> GoldScore {
    let mut score = GoldScore::default();
    for pair in &corpus.pairs {
        if shared_terms(&pair.a, &pair.b) < min_shared {
            score.not_proposed += 1;
            continue;
        }
        let verdict = provider.adjudicate(
            &Candidate {
                claim_id: &pair.id,
                statement: &pair.a,
            },
            &Candidate {
                claim_id: &pair.id,
                statement: &pair.b,
            },
        );
        score.scored += 1;
        match predicted(&verdict) {
            None => score.unverifiable += 1,
            Some(Tier2Label::Equivalent) if pair.label == Tier2Label::Equivalent => {
                score.true_positives += 1
            }
            Some(Tier2Label::Equivalent) => {
                score.false_positives += 1;
                score.false_positive_ids.push(pair.id.clone());
            }
            Some(_) if pair.label == Tier2Label::Equivalent => {
                score.false_negatives += 1;
                score.false_negative_ids.push(pair.id.clone());
            }
            Some(_) => score.true_negatives += 1,
        }
    }
    score
}

/// REQ-0011: how many pairs change their answer across repeated identical runs. The count is
/// measured, never assumed from architecture — a single-forward-pass classifier should be stable,
/// but "should be" is not a measurement, and llama.cpp's own thread and batching effects are a
/// reason to check rather than trust.
pub fn flip_count(
    corpus: &Tier2Corpus,
    provider: &dyn Provider,
    min_shared: usize,
    repeats: usize,
) -> (usize, usize) {
    let mut flipped = 0;
    let mut measured = 0;
    for pair in &corpus.pairs {
        if shared_terms(&pair.a, &pair.b) < min_shared {
            continue;
        }
        measured += 1;
        let candidate_a = Candidate {
            claim_id: &pair.id,
            statement: &pair.a,
        };
        let candidate_b = Candidate {
            claim_id: &pair.id,
            statement: &pair.b,
        };
        let first = provider.adjudicate(&candidate_a, &candidate_b);
        for _ in 1..repeats {
            if provider.adjudicate(&candidate_a, &candidate_b) != first {
                flipped += 1;
                break;
            }
        }
    }
    (flipped, measured)
}
