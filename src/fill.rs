//! `req canon --fill` — write the **derived** fields (`canonical.form`, `address`)
//! back into the claim files.
//!
//! Two rules shape this module:
//!
//! 1. **Derived, never authored.** Only this path may write those fields; a
//!    hand-written digest is a critical finding (AGENTS.md).
//! 2. **The parity invariant gates the write.** A claim whose authored slots diverge
//!    from its parse is *refused*: an address over a parse that contradicts the claim
//!    would enshrine the contradiction in the audit artifact. A refusal is reported
//!    as UNVERIFIABLE — never folded into success.
//!
//! The file is edited **surgically**, not re-emitted: field order, comments and
//! folded scalars belong to the author, and round-tripping the YAML through a
//! serializer would rewrite all of them.

use crate::canon::{
    RULES_V, address_hex, canonical_json, evaluate_claim, load_claims, load_registry,
};
use std::path::Path;

/// Rewrite one claim file's derived fields. Pure: text in, text out.
///
/// `form_json` is stored verbatim as a YAML literal block scalar, so the bytes that
/// were hashed are exactly the bytes on disk — the property `address_hex(stored) ==
/// stored address` depends on it.
///
/// Returns `Err` rather than writing when an expected anchor is missing: a fill that
/// cannot find `canonical.form`/`address` in the text must not touch the file.
pub fn fill_text(
    text: &str,
    form_json: &str,
    address: &str,
    rules_v: u32,
    registry_v: u32,
) -> Result<String, String> {
    let mut out = String::new();
    let mut in_canonical = false;
    let mut dropping_body_at: Option<usize> = None;
    let (mut seen_rules, mut seen_form, mut seen_address) = (false, false, false);

    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Drop a previously written block-scalar body (idempotence).
        if let Some(key_indent) = dropping_body_at {
            if trimmed.is_empty() || indent > key_indent {
                continue;
            }
            dropping_body_at = None;
        }

        if in_canonical && indent == 0 {
            in_canonical = false;
        }

        if indent == 0 && trimmed.starts_with("canonical:") {
            in_canonical = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if indent == 0 && trimmed.starts_with("address:") {
            seen_address = true;
            out.push_str("address: ");
            out.push_str(address);
            out.push('\n');
            continue;
        }
        if in_canonical {
            if trimmed.starts_with("rules_v:") {
                seen_rules = true;
                out.push_str(&" ".repeat(indent));
                out.push_str(&format!("rules_v: {rules_v}\n"));
                continue;
            }
            if trimmed.starts_with("subject_registry:") {
                out.push_str(&" ".repeat(indent));
                out.push_str(&format!("subject_registry: {registry_v}\n"));
                continue;
            }
            if trimmed.starts_with("form:") {
                seen_form = true;
                out.push_str(&" ".repeat(indent));
                out.push_str("form: |-\n");
                let pad = " ".repeat(indent + 2);
                for chunk in form_json.lines() {
                    out.push_str(&pad);
                    out.push_str(chunk);
                    out.push('\n');
                }
                dropping_body_at = Some(indent);
                continue;
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    if !(seen_rules && seen_form && seen_address) {
        return Err(format!(
            "refusing to write: anchors not found (rules_v: {seen_rules}, form: {seen_form}, address: {seen_address})"
        ));
    }
    Ok(out)
}

/// Fill every claim whose parse agrees with its authored slots.
///
/// Exit codes: **0** all filled (or already filled), **3** at least one claim was
/// refused because its authored slots diverge from its parse.
pub fn run_fill(root: &Path) -> i32 {
    let registry = match load_registry(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let claims = match load_claims(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let (mut filled, mut unchanged, mut refused) = (0usize, 0usize, 0usize);
    for loaded in claims.iter() {
        let text = &loaded.text;
        let claim = &loaded.claim;
        let path = &loaded.path;
        let verdict = evaluate_claim(claim, &registry);
        if verdict.flags() > 0 {
            println!(
                "{}  REFUSED  {} discrepancy flag(s): authored slots and parse disagree — \
                 re-word the statement or leave it unfilled",
                claim.claim_id,
                verdict.flags()
            );
            refused += 1;
            continue;
        }

        let form_json = canonical_json(&verdict.form);
        let address = address_hex(&form_json);
        let updated = match fill_text(text, &form_json, &address, RULES_V, registry.version as u32)
        {
            Ok(u) => u,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] {}: {e}", path.display());
                return 3;
            }
        };
        if updated == *text {
            println!("{}  filled (unchanged)", claim.claim_id);
            unchanged += 1;
            continue;
        }
        if let Err(e) = std::fs::write(path, &updated) {
            eprintln!("[UNVERIFIABLE] write {}: {e}", path.display());
            return 3;
        }
        println!("{}  filled  {}", claim.claim_id, &address[..12]);
        filled += 1;
    }

    println!(
        "rules_v {RULES_V}; filled {filled}, already current {unchanged}, refused {refused} \
         (a refusal is UNVERIFIABLE, never a pass)"
    );
    if refused > 0 { 3 } else { 0 }
}

/// The state of a claim file's *stored* derived fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredState {
    /// The stored bytes are what the current parse produces, and the stored address
    /// recomputes from them (REQ-0017).
    Verified,
    /// Unfilled, or filled under different rules: a re-canonicalization candidate.
    /// Never a failure, and never a pass.
    Candidate(String),
    /// Stored form and/or address disagree with the parse. AGENTS.md: a hand-written
    /// `address`/`canonical.form` is a critical finding — this is what detects it.
    Mismatch(String),
}

/// Verify one claim file's stored derived fields against the expected form.
pub fn verify_text(text: &str, expected_form_json: &str, rules_v: u32) -> StoredState {
    let claim: crate::model::ClaimFile = match yaml_serde::from_str(text) {
        Ok(c) => c,
        Err(e) => return StoredState::Mismatch(format!("unparsable: {e}")),
    };
    let Some(stored_form) = claim.canonical.form.clone() else {
        return StoredState::Candidate("unfilled".to_string());
    };
    if claim.canonical.rules_v != Some(rules_v) {
        return StoredState::Candidate(format!(
            "filled under rules {:?}, current {rules_v}",
            claim.canonical.rules_v
        ));
    }
    // The address must recompute from the *stored* bytes (hashing §5).
    let recomputed = address_hex(&stored_form);
    match claim.address.clone() {
        Some(a) if a == recomputed => {}
        Some(a) => {
            return StoredState::Mismatch(format!(
                "stored address {a} does not recompute from the stored form ({recomputed})"
            ));
        }
        None => return StoredState::Mismatch("form stored but no address".to_string()),
    }
    // A self-consistent pair can still be the wrong pair: compare to the parse.
    if stored_form != expected_form_json {
        return StoredState::Mismatch(
            "stored form is not what the current parse produces".to_string(),
        );
    }
    StoredState::Verified
}

/// Verify every claim file's stored derived fields.
///
/// Exit: **0** all verified, **1** any mismatch, **3** any candidate (unfilled, or
/// filled under other rules) or unreadable file.
pub fn run_verify(root: &Path) -> i32 {
    let registry = match load_registry(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let claims = match load_claims(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let (mut verified, mut candidates, mut mismatches) = (0usize, 0usize, 0usize);
    for loaded in claims.iter() {
        let text = &loaded.text;
        let claim = &loaded.claim;
        let expected = canonical_json(&evaluate_claim(claim, &registry).form);
        match verify_text(text, &expected, RULES_V) {
            StoredState::Verified => {
                verified += 1;
                let short = claim
                    .address
                    .as_deref()
                    .map(|a| &a[..12.min(a.len())])
                    .unwrap_or("");
                println!("{}  verified  {short}", claim.claim_id);
            }
            StoredState::Candidate(why) => {
                candidates += 1;
                println!("{}  CANDIDATE  {why}", claim.claim_id);
            }
            StoredState::Mismatch(why) => {
                mismatches += 1;
                println!("{}  MISMATCH  {why}", claim.claim_id);
            }
        }
    }
    println!(
        "rules_v {RULES_V}; verified {verified}, candidates {candidates}, mismatches {mismatches}"
    );
    if mismatches > 0 {
        1
    } else if candidates > 0 {
        3
    } else {
        0
    }
}

/// Fill one file path — used by tests and any caller with an explicit path. The root is
/// still explicit: the registry an address depends on is read from a resolved corpus root
/// (ADR 0012), never from the path this binary happened to be built in.
pub fn fill_path(path: &Path, root: &Path) -> Result<String, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let registry = load_registry(root)?;
    let claim: crate::model::ClaimFile =
        yaml_serde::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let verdict = evaluate_claim(&claim, &registry);
    if verdict.flags() > 0 {
        return Err(format!("{}: {} flags", claim.claim_id, verdict.flags()));
    }
    let form_json = canonical_json(&verdict.form);
    let address = address_hex(&form_json);
    fill_text(
        &text,
        &form_json,
        &address,
        RULES_V,
        registry.version as u32,
    )
}
