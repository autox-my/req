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
//!
//! `--fill` is the only writer here, and it answers to the same judgement: it writes a derived field
//! that is **absent**, and refuses — leaving every byte exactly as it was — to change one that is
//! **present** and disagrees with its content. Recomputing over an edited record would write the
//! edit's own hash over the evidence `--verify` reports, turning a definite finding into a clean
//! bill of health. The judgement is therefore made *before* anything is written, so a log with one
//! definite finding is untouched rather than half-filled. (A write that fails part-way can still
//! leave a partially filled log — the records before it carry derived fields, the command exits 3,
//! and `--verify` then reports UNVERIFIABLE rather than passing.)
//! req-satisfies: REQ-0002 — Decision: actor, role, date, ratified address and dissent note
//! req-satisfies: REQ-0031 — verify_chain: an edited record no longer recomputes, so it is TAMPERED

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
///
/// This derivation chains from [`GENESIS`] **unconditionally** and overwrites every field: it
/// ignores a record's stored `prev_hash`. Chaining over it therefore derives an absent `hash` from
/// the genesis anchor while the record declares another one — reporting a stored hash that does not
/// recompute where none was ever stored — and chains each successor to a hash that declared anchor
/// never implied, blaming an untouched record for a break that is not its own.
///
/// `--fill` judges and writes through `as_filled`, the anchor-aware derivation, for exactly that
/// reason; it does not call this function. The two agree byte for byte wherever every stored derived
/// field is absent or already agrees with the derivation — the only path `--fill` writes on, since it
/// writes nothing unless the judgement is Valid. Reach for this one only to derive a chain of records
/// that carries no stored derived fields of its own.
pub fn fill(log: &mut [Decision]) {
    let mut prev: Option<String> = None;
    for decision in log.iter_mut() {
        let prev_ref = prev.as_deref();
        decision.prev_hash = Some(prev_ref.unwrap_or(GENESIS).to_string());
        decision.hash = Some(compute_hash(prev_ref, decision));
        prev = decision.hash.clone();
    }
}

/// The log's chain head: the last record's stored `hash`, and how many records the log holds.
///
/// `None` when the log is empty or the last record's `hash` is unfilled — a head that attests
/// nothing. This is the value a v3 Baseline records in `authority.head` and re-derives on
/// verification (ADR 0016); `entries` rides beside it because a head alone cannot distinguish
/// "one entry" from a log rebuilt to one entry with the same head — impossible in principle,
/// cheap to state in fact.
///
/// **Precondition: the caller judges the chain first** ([`verify_chain`]). This function reads
/// the *stored* hash, and a stored hash that no longer recomputes is exactly what an edit after
/// the fact looks like: signing this value without the judgement would attest an edited record
/// as the chain's committed head — the failure ADR 0016 exists to close. `baseline`'s emit path
/// refuses unless the chain is [`Chain::Valid`].
///
/// The value is the log's **present** head. A Baseline attests a *position* as well (`entries`):
/// it names record `entries - 1`, so a decision recorded afterwards does not move what the
/// Baseline committed to. Verification therefore locates the attested record rather than
/// comparing against this function's latest answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub hash: String,
    pub entries: usize,
}

pub fn head(log: &[Decision]) -> Option<Head> {
    let last = log.last()?;
    Some(Head {
        hash: last.hash.clone()?,
        entries: log.len(),
    })
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

/// What a Decision's signature file says about it (ADR 0042 decision 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribution {
    /// No signature file: the record's `role` is a claim, not a proof.
    Asserted,
    /// Verified under a key trusted at the record's date, and the record's `role` is the key's.
    Signed {
        key_id: String,
        party: Option<String>,
        role: String,
    },
    /// A definite finding: the signature does not verify, the key was revoked or expired at the
    /// record's date, or the record's `role` is not the key's.
    Failed(String),
    /// The question cannot be answered: an unknown key, no verifier, an unreadable signature.
    Unverifiable(String),
}

/// Judge one `decisions/DEC-NNNN.yml.<key-id>.minisig` over [`canonical_bytes`] of `decision`.
///
/// The key is judged at the record's own `date` (ADR 0016's rule, with the Decision's date as the
/// signing time: a Decision carries no other). A verified signature whose key holds a different
/// `role` than the record states — or no role at all — is FAIL: a role claimed without a key that
/// holds it is exactly what ADR 0042 refuses to count as a proof.
pub fn judge_signature(
    decision: &Decision,
    key_id: &str,
    signature: &str,
    trust: &crate::baseline::TrustFile,
    verifier: &dyn crate::baseline::SignatureVerifier,
) -> Attribution {
    use crate::baseline::{SignatureOutcome, TrustState};
    let Some(key) = trust.key(key_id) else {
        return Attribution::Unverifiable(format!("key {key_id} is not in the trust file"));
    };
    if !crate::baseline::valid_judgement_time(&decision.date) {
        return Attribution::Unverifiable(format!(
            "{} has a date ({}) that is not an ISO date, so key {key_id} cannot be judged at it",
            decision.id, decision.date
        ));
    }
    match trust.state(key_id, &decision.date) {
        TrustState::Trusted => {}
        TrustState::Revoked(why) | TrustState::RevokedAnchor(why) | TrustState::Expired(why) => {
            return Attribution::Failed(format!(
                "key {key_id} is {why} — not trusted at {}, the date of {}",
                decision.date, decision.id
            ));
        }
        TrustState::Unknown => {
            return Attribution::Unverifiable(format!("key {key_id} is not in the trust file"));
        }
    }
    if key.scheme != verifier.scheme() {
        return Attribution::Unverifiable(format!(
            "key {key_id} is declared for scheme {} but the verifier is {}",
            key.scheme,
            verifier.scheme()
        ));
    }
    match verifier.verify(
        canonical_bytes(decision).as_bytes(),
        signature,
        &key.public_key,
    ) {
        SignatureOutcome::Valid => {}
        SignatureOutcome::Invalid(why) => {
            return Attribution::Failed(format!(
                "the signature of key {key_id} over {} does not verify: {why}",
                decision.id
            ));
        }
        SignatureOutcome::Unverifiable(why) => {
            return Attribution::Unverifiable(format!("key {key_id}: {why}"));
        }
    }
    match key.role.as_deref() {
        Some(role) if role == decision.role => Attribution::Signed {
            key_id: key_id.to_string(),
            party: key.party.clone(),
            role: role.to_string(),
        },
        Some(role) => Attribution::Failed(format!(
            "{} states role `{}` but key {key_id} signs as `{role}`: a role is a property of the \
             key someone was trusted with",
            decision.id, decision.role
        )),
        None => Attribution::Failed(format!(
            "{} states role `{}` but key {key_id} holds no role in the trust file",
            decision.id, decision.role
        )),
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
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
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

/// The log **as `--fill` would leave it**: every record carries both derived fields, and a stored
/// one is kept verbatim. This is the derivation `--fill` judges and writes through, and on a Valid
/// judgement it agrees with [`fill`].
///
/// Each record's **effective anchor** is its stored `prev_hash` when it has one, and otherwise the
/// predecessor's effective hash — [`GENESIS`] at position 0. An absent `hash` is computed over that
/// anchor. Deriving over a chain that ignored a stored anchor would make the proposed log contradict
/// itself and report a finding against records nobody touched; a stored field is evidence, and
/// `verify_chain` is the only judge of the result.
fn as_filled(log: &[Decision]) -> Vec<Decision> {
    let mut out = Vec::with_capacity(log.len());
    // The effective hash of the previous record — what a record with no stored `prev_hash` chains
    // from, so a stored anchor's successors follow *it* rather than a fresh genesis-anchored chain.
    let mut prev: Option<String> = None;
    for record in log {
        let prev_hash = record
            .prev_hash
            .clone()
            .unwrap_or_else(|| prev.clone().unwrap_or_else(|| GENESIS.to_string()));
        let hash = record
            .hash
            .clone()
            .unwrap_or_else(|| compute_hash(Some(&prev_hash), record));
        prev = Some(hash.clone());
        out.push(Decision {
            prev_hash: Some(prev_hash),
            hash: Some(hash),
            ..record.clone()
        });
    }
    out
}

/// `req decision [--fill] [--verify]`
///
/// `--fill` writes the derived chain fields into every `decisions/DEC-*.yml` that lacks them,
/// judging the log **as the write would leave it** first: a stored field that disagrees with its
/// content is a definite finding, so nothing is written and the verdict is the reason. `--verify`
/// (the default) walks the chain. Exit: **0** VALID, **1** TAMPERED, **3** UNVERIFIABLE, **2** usage.
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
    let mut log = match read_log(&paths) {
        Ok(log) => log,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    if fill_files {
        // Judge first, write second. The log as the write would leave it — a stored field kept, an
        // absent one derived over the anchor the record actually carries — is what must verify, and
        // only a Valid log is written at all.
        let records: Vec<Decision> = log.iter().map(|(_, d)| d.clone()).collect();
        let proposed = as_filled(&records);
        let judgement = verify_chain(&proposed);
        // An empty log is not a definite finding — there is nothing to fill, and the tail below
        // reports it UNVERIFIABLE. Any other non-Valid judgement means writing nothing at all.
        if !proposed.is_empty() && judgement != Chain::Valid {
            let remedy = match &judgement {
                Chain::Tampered(_) => {
                    "a stored derived field disagrees with its record — the content was edited after \
                     it was recorded, or the entry was moved — and re-filling it would write the \
                     change's own hash over the evidence"
                }
                _ => {
                    "the log does not verify, and filling it would settle a question the files \
                     cannot answer"
                }
            };
            // The refusal *is* this command's report, so it goes on stdout with the rest of it —
            // the `filled …` lines, the listing and the verdict — exactly as `canon --fill` prints
            // its `REFUSED` report. Only the read/parse failures above stay on stderr.
            println!(
                "[{}] refusing to fill: {remedy}; no file is written",
                judgement.as_str()
            );
            println!(
                "  re-filling is not the remedy — restore the record, or record a new decision that supersedes it"
            );
            for reason in judgement.reasons() {
                println!("  - {reason}");
            }
            return judgement.exit_code();
        }
        let mut written = 0;
        for ((path, before), after) in log.iter().zip(proposed.iter()) {
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
                proposed.len()
            );
        }
        // The verdict below describes the log **as it now stands on disk**, not the records read
        // on the way in: a successful fill reports VALID and exits 0.
        log = match read_log(&paths) {
            Ok(log) => log,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] {e}");
                return 3;
            }
        };
    }
    // Who stands behind each record (ADR 0042 decision 5). Read here, once, so the listing and the
    // verdict describe the same files; a record with no signature file is *asserted*.
    let signed = SignedDecisions::read(&log);
    let decisions: Vec<Decision> = log.into_iter().map(|(_, d)| d).collect();
    println!("# Decision log: {} record(s)", decisions.len());
    for (decision, provenance) in decisions.iter().zip(&signed.lines) {
        println!(
            "- {}  {} {}  by {} ({})  {}",
            decision.id,
            decision.kind,
            decision.claim_id,
            decision.actor,
            decision.role,
            decision.date
        );
        for line in provenance {
            println!("    {line}");
        }
    }
    let chain = verify_chain(&decisions);
    println!("\n[{}]", chain.as_str());
    for reason in chain.reasons() {
        println!("  - {reason}");
    }
    let mut code = chain.exit_code();
    // A definite finding outranks an open question, as everywhere else: the chain's TAMPERED (1)
    // and a signature FAIL (1) stand over any UNVERIFIABLE (3). A chain that is VALID stays VALID
    // in its own line; the signatures answer in a line of their own.
    if !signed.failed.is_empty() {
        println!("[FAIL] signed Decisions");
        for reason in &signed.failed {
            println!("  - {reason}");
        }
        code = 1;
    }
    if !signed.open.is_empty() {
        println!("[UNVERIFIABLE] signed Decisions");
        for reason in &signed.open {
            println!("  - {reason}");
        }
        if code == 0 {
            code = 3;
        }
    }
    code
}

/// The provenance of every record in the log: per record the lines the listing prints under it,
/// and the findings that move the verdict.
struct SignedDecisions {
    lines: Vec<Vec<String>>,
    failed: Vec<String>,
    open: Vec<String>,
}

impl SignedDecisions {
    fn read(log: &[(std::path::PathBuf, Decision)]) -> Self {
        let mut out = SignedDecisions {
            lines: Vec::new(),
            failed: Vec::new(),
            open: Vec::new(),
        };
        // The trust file is read only when some record carries a signature: a log with none needs
        // no anchor, and its output must not depend on one.
        let files: Vec<Vec<crate::baseline::KeyedSignature>> = log
            .iter()
            .map(|(path, _)| crate::baseline::keyed_signatures(path))
            .collect();
        let root = log
            .first()
            .and_then(|(path, _)| path.parent())
            .and_then(std::path::Path::parent);
        let trust = if files.iter().all(Vec::is_empty) {
            None
        } else {
            let text = root
                .map(|root| root.join(crate::baseline::TRUST_PATH))
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default();
            Some(crate::baseline::TrustFile::load(&text))
        };
        for ((record_path, decision), signatures) in log.iter().zip(files) {
            let mut lines = Vec::new();
            // `DEC-NNNN.yml.minisig` — the spelling a Baseline's default signature uses — names no
            // key, so no key can judge it. Saying "asserted (no signature file)" beside it would be
            // false (review A-1-review-1): it is a signature req cannot attribute, UNVERIFIABLE.
            let unnamed = {
                let mut spelled = record_path.as_os_str().to_owned();
                spelled.push(".minisig");
                std::path::PathBuf::from(spelled)
            };
            if unnamed.is_file() {
                lines.push(format!(
                    "a signature that names no key: {} (sign as <record>.<key-id>.minisig)",
                    unnamed.display()
                ));
                out.open.push(format!(
                    "{}: {} names no key, so it cannot be judged — rename it \
                     {}.<key-id>.minisig",
                    decision.id,
                    unnamed.display(),
                    decision.id
                ));
            } else if signatures.is_empty() {
                lines.push(
                    "asserted (no signature file: its role is a claim, not a proof)".to_string(),
                );
            }
            for keyed in signatures {
                let judged = match (&trust, &keyed.signature) {
                    (_, Err(why)) => Attribution::Unverifiable(why.clone()),
                    (Some(Err(why)), _) => Attribution::Unverifiable(why.clone()),
                    (Some(Ok(trust)), Ok(signature)) => judge_signature(
                        decision,
                        &keyed.key_id,
                        signature,
                        trust,
                        &crate::baseline::MinisignCli,
                    ),
                    (None, _) => continue,
                };
                match judged {
                    Attribution::Signed {
                        key_id,
                        party,
                        role,
                    } => {
                        let holder = match party {
                            Some(party) => format!("{party} {role}"),
                            None => role,
                        };
                        lines.push(format!(
                            "signed by {key_id} ({holder}), actor {}",
                            decision.actor
                        ));
                    }
                    Attribution::Failed(why) => {
                        lines.push(format!("signature of {} FAILS", keyed.key_id));
                        out.failed.push(format!("{}: {why}", decision.id));
                    }
                    Attribution::Unverifiable(why) => {
                        lines.push(format!("signature of {} cannot be judged", keyed.key_id));
                        out.open.push(format!("{}: {why}", decision.id));
                    }
                    Attribution::Asserted => {}
                }
            }
            out.lines.push(lines);
        }
        out
    }
}

/// Read the log from disk, in chain order, keeping the path each record came from so a caller can
/// re-read after writing and judge what is now on disk rather than what it read first.
pub(crate) fn read_log(
    paths: &[std::path::PathBuf],
) -> Result<Vec<(std::path::PathBuf, Decision)>, String> {
    let mut log = Vec::with_capacity(paths.len());
    for path in paths {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let decision = yaml_serde::from_str::<Decision>(&text)
            .map_err(|e| format!("parse {}: {e}", path.display()))?;
        log.push((path.clone(), decision));
    }
    Ok(log)
}

/// The log's files, in chain order. Zero-padded ids make lexicographic order chronological, so the
/// order is a property of the names rather than of the filesystem.
pub(crate) fn log_paths(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
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
