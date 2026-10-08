//! The Baseline: a signed statement of which addresses were ratified (ADR 0005, ADR 0010).
//!
//! A Baseline is a **manifest** — the rules version, the selection context, and every
//! applying claim's address — over which a human places one signature. Verification
//! re-derives those addresses from the working tree, so a claim edited after signing fails
//! rather than quietly passing; and the signature check is a **process boundary** to a pinned
//! verifier, so the core crate gains no cryptography and still no network.
//!
//! Three outcomes, as everywhere else. **Revoked** trust is FAIL (a human withdrew trust);
//! **expired** trust is UNVERIFIABLE (a Baseline is historical and no trusted signing time
//! exists, so the question is unanswerable rather than answered badly); an unknown key or a
//! missing signature is UNVERIFIABLE, never a silent pass.

use crate::select::{Applies, Context};
use crate::view::Row;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// The manifest format's own version — distinct from `RULES_V`, because the *envelope* and
/// the *rules* move independently.
pub const BASELINE_VERSION: u32 = 1;

/// Where the committed trust anchor lives. Data, reviewed like a corpus (ADR 0010).
pub const TRUST_PATH: &str = "baselines/trust.yml";

/// One ratified claim: its identity, and the address that identity is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestClaim {
    pub claim_id: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub rules_v: u32,
    /// The variant this Baseline is over — empty means every applying claim with no context.
    pub selection: BTreeMap<String, String>,
    pub claims: Vec<ManifestClaim>,
}

/// Build a manifest from the claims that apply in `selection`, sorted by id and deduplicated
/// so the bytes never depend on input order.
pub fn manifest(rows: &[Row], rules_v: u32, selection: &Context) -> Manifest {
    let mut claims: Vec<ManifestClaim> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| ManifestClaim {
            claim_id: row.claim_id.clone(),
            address: row.address.clone(),
        })
        .collect();
    claims.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    claims.dedup_by(|a, b| a.claim_id == b.claim_id);
    Manifest {
        version: BASELINE_VERSION,
        rules_v,
        selection: selection.clone(),
        claims,
    }
}

/// The **exact bytes a signature covers**: fixed key order, sorted selection and claims, no
/// incidental whitespace, one trailing newline. The same discipline as the address path —
/// what is signed must be reproducible byte for byte by anyone.
pub fn canonical_bytes(manifest: &Manifest) -> String {
    let selection = manifest
        .selection
        .iter()
        .map(|(key, value)| format!("{}:{}", json_string(key), json_string(value)))
        .collect::<Vec<_>>()
        .join(",");
    let claims = manifest
        .claims
        .iter()
        .map(|claim| {
            format!(
                "{{\"claim_id\":{},\"address\":{}}}",
                json_string(&claim.claim_id),
                json_string(&claim.address)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"version\":{},\"rules_v\":{},\"selection\":{{{selection}}},\"claims\":[{claims}]}}\n",
        manifest.version, manifest.rules_v
    )
}

/// Minimal JSON string escaping. Deliberately not a general serializer: the manifest's shape
/// is fixed here, and a general one would let the shape drift without anyone noticing.
fn json_string(value: &str) -> String {
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

/// Parse a manifest back. Reading is not the address path, so the ordinary JSON reader is
/// fine here — only the *emitted* bytes have to be canonical.
pub fn from_json(text: &str) -> Result<Manifest, String> {
    serde_json::from_str(text).map_err(|e| format!("manifest is not readable: {e}"))
}

/// What the working tree says about a signed manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drift {
    /// In the manifest, applying now, but at a different address — an edited claim.
    pub changed: Vec<String>,
    /// In the manifest, no longer applying at all.
    pub missing: Vec<String>,
    /// Applying now and not in the manifest — a claim added after signing.
    pub added: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.changed.is_empty() && self.missing.is_empty() && self.added.is_empty()
    }

    pub fn reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        for id in &self.changed {
            out.push(format!(
                "{id}: the address no longer recomputes — edited after signing"
            ));
        }
        for id in &self.missing {
            out.push(format!("{id}: in the Baseline but no longer applies"));
        }
        for id in &self.added {
            out.push(format!("{id}: applies now but is not in the Baseline"));
        }
        out
    }
}

pub fn drift(rows: &[Row], manifest: &Manifest) -> Drift {
    let current: BTreeMap<&str, &str> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| (row.claim_id.as_str(), row.address.as_str()))
        .collect();
    let mut out = Drift::default();
    for claim in &manifest.claims {
        match current.get(claim.claim_id.as_str()) {
            Some(address) if *address == claim.address => {}
            Some(_) => out.changed.push(claim.claim_id.clone()),
            None => out.missing.push(claim.claim_id.clone()),
        }
    }
    let signed: BTreeSet<&str> = manifest
        .claims
        .iter()
        .map(|claim| claim.claim_id.as_str())
        .collect();
    out.added = current
        .keys()
        .filter(|id| !signed.contains(*id))
        .map(|id| (*id).to_string())
        .collect();
    out
}

/// One trusted public key, with the human's statement about when it may be used.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TrustedKey {
    pub id: String,
    pub scheme: String,
    pub public_key: String,
    pub valid_from: String,
    #[serde(default)]
    pub valid_until: Option<String>,
    #[serde(default)]
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TrustFile {
    pub keys: Vec<TrustedKey>,
}

/// What the trust anchor says about a key. Expiry is its own state because it cannot be
/// decided without a signing time — see ADR 0010.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustState {
    Trusted,
    Revoked(String),
    Expired(String),
    Unknown,
}

impl TrustFile {
    pub fn load(text: &str) -> Result<TrustFile, String> {
        yaml_serde::from_str(text).map_err(|e| format!("trust file is not readable: {e}"))
    }

    pub fn key(&self, id: &str) -> Option<&TrustedKey> {
        self.keys.iter().find(|key| key.id == id)
    }

    /// `at` is an ISO-8601 date; ISO dates compare correctly as strings, so no date library
    /// and no locale enters this path.
    pub fn state(&self, id: &str, at: &str) -> TrustState {
        let Some(key) = self.key(id) else {
            return TrustState::Unknown;
        };
        if let Some(revoked_at) = &key.revoked_at {
            return TrustState::Revoked(format!("revoked at {revoked_at}"));
        }
        if let Some(valid_until) = &key.valid_until
            && at > valid_until.as_str()
        {
            return TrustState::Expired(format!("valid until {valid_until}, verifying at {at}"));
        }
        TrustState::Trusted
    }
}

/// A signature's verdict from the verifier process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureOutcome {
    Valid,
    Invalid(String),
    Unverifiable(String),
}

/// The verifier boundary. The core never links cryptography (ADR 0010); a scheme is a process.
pub trait SignatureVerifier {
    fn scheme(&self) -> &'static str;
    fn verify(&self, message: &[u8], signature: &str, public_key: &str) -> SignatureOutcome;
}

/// The pinned `minisign` CLI. Absent in this development environment, which the outcome
/// reports rather than guesses around.
pub struct MinisignCli;

impl SignatureVerifier for MinisignCli {
    fn scheme(&self) -> &'static str {
        "minisign"
    }

    fn verify(&self, message: &[u8], signature: &str, public_key: &str) -> SignatureOutcome {
        let dir = std::env::temp_dir().join(format!("req-baseline-{}", std::process::id()));
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return SignatureOutcome::Unverifiable(format!("no temporary directory: {e}"));
        }
        let msg = dir.join("baseline.json");
        let sig = dir.join("baseline.json.minisig");
        let key = dir.join("req.pub");
        let staged = std::fs::write(&msg, message)
            .and_then(|()| std::fs::write(&sig, signature))
            .and_then(|()| std::fs::write(&key, public_key));
        if let Err(e) = staged {
            return SignatureOutcome::Unverifiable(format!("could not stage inputs: {e}"));
        }
        let output = std::process::Command::new("minisign")
            .arg("-V")
            .arg("-p")
            .arg(&key)
            .arg("-m")
            .arg(&msg)
            .arg("-x")
            .arg(&sig)
            .output();
        let _ = std::fs::remove_dir_all(&dir);
        match output {
            Err(e) => SignatureOutcome::Unverifiable(format!("minisign is not runnable: {e}")),
            Ok(output) if output.status.success() => SignatureOutcome::Valid,
            Ok(output) => SignatureOutcome::Invalid(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ),
        }
    }
}

/// The verdict on a Baseline. A known defect outranks an open question, as in `req gate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Verified,
    Failed(Vec<String>),
    Unverifiable(Vec<String>),
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Verified => "VERIFIED",
            Outcome::Failed(_) => "FAIL",
            Outcome::Unverifiable(_) => "UNVERIFIABLE",
        }
    }

    pub fn reasons(&self) -> &[String] {
        match self {
            Outcome::Verified => &[],
            Outcome::Failed(reasons) | Outcome::Unverifiable(reasons) => reasons,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Outcome::Verified => 0,
            Outcome::Failed(_) => 1,
            Outcome::Unverifiable(_) => 3,
        }
    }
}

/// Verify a manifest against the working tree, a signature, and the trust anchor.
pub fn verify(
    rows: &[Row],
    manifest: &Manifest,
    signature: Option<&str>,
    trust: &TrustFile,
    key_id: &str,
    verifier: &dyn SignatureVerifier,
    now: &str,
) -> Outcome {
    let mut failed: Vec<String> = Vec::new();
    let mut open: Vec<String> = Vec::new();

    if manifest.version != BASELINE_VERSION {
        open.push(format!(
            "manifest version {} is not the version this build writes ({BASELINE_VERSION})",
            manifest.version
        ));
    }
    let drift = drift(rows, manifest);
    if !drift.is_clean() {
        failed.extend(drift.reasons());
    }

    match trust.state(key_id, now) {
        TrustState::Trusted => {}
        TrustState::Revoked(reason) => failed.push(format!("key {key_id} is {reason}")),
        TrustState::Expired(reason) => open.push(format!(
            "key {key_id} is {reason} — a Baseline is historical and no trusted signing time \
             exists, so this cannot be decided"
        )),
        TrustState::Unknown => open.push(format!("key {key_id} is not in the trust file")),
    }

    match signature {
        None => open.push(
            "no signature: this is a manifest, not yet a Baseline (nothing is ratified until \
             a human signs it)"
                .to_string(),
        ),
        Some(signature) => {
            if let Some(key) = trust.key(key_id) {
                if key.scheme != verifier.scheme() {
                    open.push(format!(
                        "key {key_id} is declared for scheme {} but the verifier is {}",
                        key.scheme,
                        verifier.scheme()
                    ));
                } else {
                    match verifier.verify(
                        canonical_bytes(manifest).as_bytes(),
                        signature,
                        &key.public_key,
                    ) {
                        SignatureOutcome::Valid => {}
                        SignatureOutcome::Invalid(why) => {
                            failed.push(format!("the signature does not verify: {why}"))
                        }
                        SignatureOutcome::Unverifiable(why) => open.push(why),
                    }
                }
            }
        }
    }

    if !failed.is_empty() {
        Outcome::Failed(failed)
    } else if !open.is_empty() {
        Outcome::Unverifiable(open)
    } else {
        Outcome::Verified
    }
}

/// `req baseline [--verify <file>] [--signature <file>] [--key <id>] [--now <date>] [k=v ...]`
///
/// With no `--verify`, prints the canonical manifest to stdout — the bytes a human signs.
/// Exit: **0** verified (or emitted), **1** FAIL, **3** UNVERIFIABLE, **2** usage.
pub fn run(args: &[String]) -> i32 {
    let mut context = Context::new();
    let mut verify_path: Option<String> = None;
    let mut signature_path: Option<String> = None;
    let mut key_id = "req-baseline-1".to_string();
    let mut now = today_utc();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].clone();
        index += 1;
        let mut take = |name: &str| -> Option<String> {
            let value = args.get(index).cloned();
            if value.is_some() {
                index += 1;
            } else {
                eprintln!("{name} needs a value");
            }
            value
        };
        match arg.as_str() {
            "--verify" => verify_path = take("--verify"),
            "--signature" => signature_path = take("--signature"),
            "--key" => match take("--key") {
                Some(value) => key_id = value,
                None => return 2,
            },
            "--now" => match take("--now") {
                Some(value) => now = value,
                None => return 2,
            },
            other => match other.split_once('=') {
                Some((key, value)) if !key.is_empty() => {
                    context.insert(key.to_string(), value.to_string());
                }
                _ => {
                    eprintln!(
                        "usage: req baseline [--verify <file>] [--signature <file>] [--key <id>] [--now <date>] [key=value ...]"
                    );
                    return 2;
                }
            },
        }
    }

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rows = match crate::view::load_rows(&context) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let Some(path) = verify_path else {
        let manifest = manifest(&rows, crate::canon::RULES_V, &context);
        print!("{}", canonical_bytes(&manifest));
        return 0;
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] cannot read {path}: {e}");
            return 3;
        }
    };
    let manifest = match from_json(&text) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let signature = match signature_path {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(e) => {
                eprintln!("[UNVERIFIABLE] cannot read {path}: {e}");
                return 3;
            }
        },
        None => {
            let default = format!("{path}.minisig");
            std::fs::read_to_string(&default).ok()
        }
    };
    let trust_text = std::fs::read_to_string(root.join(TRUST_PATH)).unwrap_or_default();
    let trust = match TrustFile::load(&trust_text) {
        Ok(trust) => trust,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let outcome = verify(
        &rows,
        &manifest,
        signature.as_deref(),
        &trust,
        &key_id,
        &MinisignCli,
        &now,
    );
    println!(
        "# Baseline {path}: {} claim(s), rules v{}, key {key_id}, verified at {now}",
        manifest.claims.len(),
        manifest.rules_v
    );
    println!("[{}]", outcome.as_str());
    for reason in outcome.reasons() {
        println!("  - {reason}");
    }
    outcome.exit_code()
}

/// Today's date in UTC as `YYYY-MM-DD`, by integer arithmetic — no date library, no locale,
/// no float. Expiry is evaluated at verification because no trusted signing time exists
/// (ADR 0010); the limitation is stated in the reason it produces, not hidden.
fn today_utc() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = seconds / 86_400;
    civil_from_days(days as i64)
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a proleptic Gregorian date.
fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}
