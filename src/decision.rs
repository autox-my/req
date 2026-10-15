//! The decision log and its hash chain (design step 8; ADR 0005).
//!
//! A **Decision** is the recorded act that gives a claim its authority: who accepted, rejected,
//! merged or superseded what, in what capacity, when, and — because a decision record that omits
//! it is not auditable — what was not chosen. This module stores those records as one file per
//! decision, each committing to its predecessor by hash.
//!
//! Three outcomes, and the boundaries between them are the whole design:
//!
//! * **VALID** — every stored hash recomputes from that decision's content *and* its `prev_hash`
//!   matches its predecessor's stored hash. The log is what it claims to be.
//! * **TAMPERED** — a stored hash does not recompute, or a `prev_hash` does not match the entry
//!   before it. A **definite** finding: content moved, or order moved.
//! * **UNVERIFIABLE** — a hash is unfilled, a file cannot be parsed, or **the log does not begin at
//!   the genesis anchor**. That last case is deliberately *not* TAMPERED: a truncated history and a
//!   forged first entry are indistinguishable from the file set alone, so the honest answer is that
//!   the question cannot be settled — never that it passed.
//!
//! An empty log is UNVERIFIABLE, not VALID. A log with nothing in it verifies nothing, and a check
//! that passes on no evidence is the failure this whole project is organised against.

use serde::Deserialize;

/// Bumped when the canonical bytes of a decision change, so old records never silently re-hash.
pub const DECISION_VERSION: u32 = 1;

/// Domain separation, distinct from the claim address domain: a decision hash and a claim address
/// can never be the same value meaning two different things.
pub const DOMAIN: &str = "req.decision.v1";

/// The anchor the first decision chains from. A log that does not start here is incomplete.
pub const GENESIS: &str = "req.genesis.v1";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Decision {
    /// `DEC-0001` — zero-padded so file order is chronological order, lexicographically.
    pub id: String,
    /// `accept` | `reject` | `merge` | `supersede` — the act, in the decision-maker's words.
    pub kind: String,
    pub claim_id: String,
    /// The address that was ratified. The *meaning*, not the wording: a claim reworded
    /// meaning-preservingly keeps this address, so the decision survives the edit.
    pub address: String,
    /// Who decided, and in what capacity. Authority is a person's, never a commit's (ADR 0005).
    pub actor: String,
    pub role: String,
    pub date: String,
    /// The dissent and the road not taken. Defaulted so an older record without one still parses.
    #[serde(default)]
    pub note: String,
    /// The predecessor's `hash`, or [`GENESIS`] for the first entry. Derived; written by `--fill`.
    #[serde(default)]
    pub prev_hash: Option<String>,
    /// This entry's chain hash. Derived; written by `--fill`. Never hand-written.
    #[serde(default)]
    pub hash: Option<String>,
}

/// The bytes a decision's hash covers: its content, in a fixed key order. `prev_hash` is chained
/// separately and `hash` is excluded — a hash cannot cover itself.
pub fn canonical_bytes(decision: &Decision) -> String {
    format!(
        "{{\"version\":{},\"id\":{},\"kind\":{},\"claim_id\":{},\"address\":{},\"actor\":{},\"role\":{},\"date\":{},\"note\":{}}}\n",
        DECISION_VERSION,
        quote(&decision.id),
        quote(&decision.kind),
        quote(&decision.claim_id),
        quote(&decision.address),
        quote(&decision.actor),
        quote(&decision.role),
        quote(&decision.date),
        quote(&decision.note)
    )
}

/// The hash of one decision in its chain position: `sha256(domain ‖ 0 ‖ prev_hash ‖ content)`.
pub fn compute_hash(prev: Option<&str>, decision: &Decision) -> String {
    let mut input = Vec::new();
    input.extend_from_slice(DOMAIN.as_bytes());
    input.push(0u8);
    input.extend_from_slice(prev.unwrap_or(GENESIS).as_bytes());
    input.push(b'\n');
    input.extend_from_slice(canonical_bytes(decision).as_bytes());
    crate::digest::sha256_hex(&input)
}

/// Fill `prev_hash` and `hash` in chain order, in memory. The caller writes the files; nothing here
/// touches the filesystem, so the derivation is testable without one.
pub fn fill(log: &mut [Decision]) {
    let mut prev: Option<String> = None;
    for decision in log.iter_mut() {
        let prev_ref = prev.as_deref();
        decision.prev_hash = Some(prev_ref.unwrap_or(GENESIS).to_string());
        decision.hash = Some(compute_hash(prev_ref, decision));
        prev = decision.hash.clone();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chain {
    Valid,
    Tampered(Vec<String>),
    Unverifiable(Vec<String>),
}

impl Chain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Chain::Valid => "VALID",
            Chain::Tampered(_) => "TAMPERED",
            Chain::Unverifiable(_) => "UNVERIFIABLE",
        }
    }

    pub fn reasons(&self) -> &[String] {
        match self {
            Chain::Valid => &[],
            Chain::Tampered(reasons) | Chain::Unverifiable(reasons) => reasons,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Chain::Valid => 0,
            Chain::Tampered(_) => 1,
            Chain::Unverifiable(_) => 3,
        }
    }
}

/// Walk the log and judge it. A definite finding outranks an unanswerable question, as everywhere
/// else in this repository.
pub fn verify_chain(log: &[Decision]) -> Chain {
    let mut tampered = Vec::new();
    let mut open = Vec::new();
    if log.is_empty() {
        return Chain::Unverifiable(vec![
            "the decision log is empty: nothing is recorded, so nothing is verified".to_string(),
        ]);
    }
    let mut expected_prev = GENESIS.to_string();
    for (position, decision) in log.iter().enumerate() {
        match (&decision.hash, &decision.prev_hash) {
            (None, _) | (_, None) => {
                open.push(format!("{}: derived fields are unfilled", decision.id));
                // The chain cannot advance, so every later entry is unjudged rather than wrong.
                break;
            }
            (Some(stored), Some(prev)) => {
                if position == 0 {
                    // The log's own anchor. A removed earlier entry and a forged first entry are
                    // indistinguishable from the file set alone, so this is an open question.
                    if *prev != GENESIS {
                        open.push(format!(
                            "{}: the log does not begin at the genesis anchor, so an earlier entry may be missing — a truncated history and a forged first entry look the same from here",
                            decision.id
                        ));
                    }
                } else if *prev != expected_prev {
                    tampered.push(format!(
                        "{}: chains from {} but {} precedes it — the entry was moved, or one before it was replaced",
                        decision.id,
                        &prev[..12.min(prev.len())],
                        &expected_prev[..12.min(expected_prev.len())]
                    ));
                }

                let recomputed = compute_hash(Some(prev), decision);
                if recomputed != *stored {
                    tampered.push(format!(
                        "{}: the stored hash does not recompute — the content was edited after it was recorded",
                        decision.id
                    ));
                }
                expected_prev = stored.clone();
            }
        }
    }
    if !tampered.is_empty() {
        Chain::Tampered(tampered)
    } else if !open.is_empty() {
        Chain::Unverifiable(open)
    } else {
        Chain::Valid
    }
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Upsert a `key: "value"` line, preserving every other line — including comments and the note a
/// human wrote. A derived field is written into the record, not over it.
pub fn upsert_line(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}:");
    let replacement = format!("{key}: \"{value}\"");
    let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    match lines
        .iter()
        .position(|line| line.trim_start().starts_with(&prefix))
    {
        Some(at) => lines[at] = replacement,
        None => lines.push(replacement),
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// `req decision [--fill] [--verify]`
///
/// `--fill` writes the derived chain fields into every `decisions/DEC-*.yml`; `--verify` (the
/// default) walks the chain. Exit: **0** VALID, **1** TAMPERED, **3** UNVERIFIABLE, **2** usage.
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
    let mut fill_files = false;
    for arg in args {
        match arg.as_str() {
            "--fill" => fill_files = true,
            "--verify" => fill_files = false,
            _ => {
                eprintln!("usage: req decision [--fill] [--verify]");
                return 2;
            }
        }
    }
    let paths = match log_paths(root) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let mut log = Vec::new();
    for path in &paths {
        match std::fs::read_to_string(path) {
            Ok(text) => match yaml_serde::from_str::<Decision>(&text) {
                Ok(decision) => log.push((path.clone(), decision)),
                Err(e) => {
                    eprintln!("[UNVERIFIABLE] parse {}: {e}", path.display());
                    return 3;
                }
            },
            Err(e) => {
                eprintln!("[UNVERIFIABLE] read {}: {e}", path.display());
                return 3;
            }
        }
    }
    if fill_files {
        let mut decisions: Vec<Decision> = log.iter().map(|(_, d)| d.clone()).collect();
        fill(&mut decisions);
        let mut written = 0;
        for ((path, before), after) in log.iter().zip(decisions.iter()) {
            if before.hash == after.hash && before.prev_hash == after.prev_hash {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let with_prev = upsert_line(
                &text,
                "prev_hash",
                after.prev_hash.as_deref().unwrap_or(GENESIS),
            );
            let with_hash = upsert_line(
                &with_prev,
                "hash",
                after.hash.as_deref().unwrap_or_default(),
            );
            if let Err(e) = std::fs::write(path, with_hash) {
                eprintln!("[UNVERIFIABLE] write {}: {e}", path.display());
                return 3;
            }
            written += 1;
            println!("filled {} ({} {})", after.id, after.kind, after.claim_id);
        }
        if written == 0 {
            println!(
                "nothing to fill: {} decision(s) already carry derived fields",
                decisions.len()
            );
        }
    }
    let decisions: Vec<Decision> = log.into_iter().map(|(_, d)| d).collect();
    println!("# Decision log: {} record(s)", decisions.len());
    for decision in &decisions {
        println!(
            "- {}  {} {}  by {} ({})  {}",
            decision.id,
            decision.kind,
            decision.claim_id,
            decision.actor,
            decision.role,
            decision.date
        );
    }
    let chain = verify_chain(&decisions);
    println!("\n[{}]", chain.as_str());
    for reason in chain.reasons() {
        println!("  - {reason}");
    }
    chain.exit_code()
}

/// The log's files, in chain order. Zero-padded ids make lexicographic order chronological, so the
/// order is a property of the names rather than of the filesystem.
fn log_paths(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let dir = root.join("decisions");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("DEC-") && name.ends_with(".yml"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    Ok(paths)
}
