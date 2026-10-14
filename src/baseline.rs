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
use std::path::{Path, PathBuf};

/// The manifest format's own version — distinct from `RULES_V`, because the *envelope* and
/// the *rules* move independently.
/// The manifest schema this build writes and verifies. v2 attests the audit surface
/// (ADR 0013); a v1 manifest is stale and must be re-issued and re-signed by the human.
pub const BASELINE_VERSION: u32 = 2;

/// Where the committed trust anchor lives. Data, reviewed like a corpus (ADR 0010).
pub const TRUST_PATH: &str = "baselines/trust.yml";

/// One ratified claim: its identity, and the audit surface attested beside it. Identity
/// stays meaning-only (ADR 0007) — the hashes cover what an auditor relies on that the
/// address deliberately does not (ADR 0013).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestClaim {
    pub claim_id: String,
    pub address: String,
    pub relations_hash: String,
    pub rationale_hash: String,
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
            relations_hash: row.relations_hash.clone(),
            rationale_hash: row.rationale_hash.clone(),
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
                "{{\"claim_id\":{},\"address\":{},\"relations_hash\":{},\"rationale_hash\":{}}}",
                json_string(&claim.claim_id),
                json_string(&claim.address),
                json_string(&claim.relations_hash),
                json_string(&claim.rationale_hash)
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

/// The one field a parse error leaves readable: the envelope version the manifest declares.
/// Recovered leniently — `Option` defaults, everything else ignored — because the error itself
/// is about the very fields a stale manifest does not carry.
#[derive(Deserialize)]
struct EnvelopeVersion {
    version: Option<u32>,
}

/// The advice a stale envelope's reader is given, shared by the parse path and [`verify`] so
/// the wording cannot drift between the two places it is printed.
const REISSUE_ADVICE: &str = "re-issue and re-sign (ADR 0013)";

/// Parse a manifest back. Reading is not the address path, so the ordinary JSON reader is
/// fine here — only the *emitted* bytes have to be canonical.
pub fn from_json(text: &str) -> Result<Manifest, String> {
    serde_json::from_str(text).map_err(|e| {
        let detail = e.to_string();
        // Branch on the *declared envelope version*, recovered by a lenient parse that
        // survives the missing fields serde is complaining about. Not on serde's message text,
        // which is not a stable contract; and not on the field name alone, because a stale
        // manifest is missing *both* attested hashes while serde names only the first — a
        // current-version manifest that lost exactly one hash would otherwise be told it is
        // stale. The stale test is *any declared version below this build's*, derived from
        // BASELINE_VERSION rather than hardcoded, so the next envelope bump cannot silently
        // drop the advice; a manifest declaring the current version (or newer) is corrupt, and
        // re-issue advice would send its reader off to re-sign over a defect that was never a
        // stale envelope (ADR 0013).
        let declared = serde_json::from_str::<EnvelopeVersion>(text)
            .ok()
            .and_then(|envelope| envelope.version);
        // Binding the recovered version keeps the invariant local: the stale branch runs only
        // when a declared version below this build's was observed, so the message cannot print
        // a fabricated fallback.
        if let Some(version) = declared.filter(|version| *version < BASELINE_VERSION) {
            format!(
                "manifest is not readable: {detail} — this manifest declares version {version}, \
                     which predates the version this build writes ({BASELINE_VERSION}): \
                     {REISSUE_ADVICE}"
            )
        } else {
            format!("manifest is not readable: {detail}")
        }
    })
}

/// What the working tree says about a signed manifest.
///
/// The categories are **mutually exclusive, by precedence**: a claim whose address no longer
/// recomputes is recorded in `changed` alone, even when its attested surface moved too —
/// identity first, as in [`drift`]. Consumers merge the lists (`audit::coverage` extends
/// `drifted` with `attested_changed`) and rely on that exclusivity to avoid double-counting an
/// id, so a claim whose address *and* attested surface both moved is reported as `changed`
/// only; `reasons()` says the address no longer recomputes and does not separately name the
/// audit surface. Reporting both would mean pushing to both lists and de-duplicating at every
/// merge site.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drift {
    /// In the manifest, applying now, but at a different address — an edited claim.
    pub changed: Vec<String>,
    /// In the manifest at the same address, but the attested audit surface (relations or
    /// rationale, ADR 0013) no longer recomputes — edited after signing.
    pub attested_changed: Vec<String>,
    /// In the manifest, no longer applying at all.
    pub missing: Vec<String>,
    /// Applying now and not in the manifest — a claim added after signing.
    pub added: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.changed.is_empty()
            && self.attested_changed.is_empty()
            && self.missing.is_empty()
            && self.added.is_empty()
    }

    pub fn reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        for id in &self.changed {
            out.push(format!(
                "{id}: the address no longer recomputes — edited after signing"
            ));
        }
        for id in &self.attested_changed {
            out.push(format!(
                "{id}: the relations or rationale no longer recompute — edited after \
                 signing (ADR 0013)"
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
    let current: BTreeMap<&str, (&str, &str, &str)> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| {
            (
                row.claim_id.as_str(),
                (
                    row.address.as_str(),
                    row.relations_hash.as_str(),
                    row.rationale_hash.as_str(),
                ),
            )
        })
        .collect();
    let mut out = Drift::default();
    for claim in &manifest.claims {
        match current.get(claim.claim_id.as_str()) {
            // Identity first; the audit surface only attests when the identity still matches.
            // The precedence is **structural** — nested, not match-arm order — so the
            // exclusivity the categories are documented to have (and `audit::coverage`'s merge
            // relies on) does not depend on which arm happens to come first.
            Some((address, relations, rationale)) => {
                if *address != claim.address {
                    out.changed.push(claim.claim_id.clone())
                } else if *relations != claim.relations_hash || *rationale != claim.rationale_hash {
                    out.attested_changed.push(claim.claim_id.clone())
                }
            }
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

/// The name of one staging attempt: `req-baseline-<pid>-<nanos-since-epoch>-<attempt>`.
///
/// The process id alone is a name another local process can guess and pre-create; the clock
/// and the attempt counter are what make guessing it fail rather than pay off.
pub fn staging_dir_name(attempt: u32) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("req-baseline-{}-{nanos}-{attempt}", std::process::id())
}

/// Create the staging directory under `base` exclusively, mode 0700 on Unix.
///
/// `DirBuilder::create` — never `create_dir_all` — so a path that already exists, whether a
/// directory left behind by an earlier run or a symlink planted at the name, is an
/// `AlreadyExists` error instead of a directory we then write the trusted inputs into. On
/// Unix the directory is created 0700, which keeps another local user out of the staged
/// manifest between writing it and minisign reading it; other platforms get the platform
/// default, so that privacy is a Unix property and not a portable one.
pub fn create_staging_dir(base: &Path, name: &str) -> std::io::Result<PathBuf> {
    let dir = base.join(name);
    staging_dir_builder().create(&dir)?;
    Ok(dir)
}

/// The `DirBuilder` a staging directory is created through, configured for 0700 on Unix.
///
/// The builder is configured and returned rather than mutated in place, so a non-Unix build
/// has no `mut` binding to warn about.
fn staging_dir_builder() -> std::fs::DirBuilder {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    }
    #[cfg(not(unix))]
    {
        std::fs::DirBuilder::new()
    }
}

/// A staging directory that owns itself, so no exit path can orphan it.
///
/// The name is fresh on every call (pid + clock + attempt), so an early return leaves a
/// directory behind that nothing will ever reuse — a leak per failed verification. A guard
/// makes removal happen on *every* way out of the verifier: an early return after staging,
/// a write failure, an unrunnable verifier, a non-zero exit and a parse failure alike.
pub struct StagingDir(PathBuf);

impl StagingDir {
    /// The directory itself, for building the paths handed to the verifier.
    ///
    /// There is deliberately no constructor from an arbitrary path: the field is private and
    /// [`staging_dir`] is the only way to obtain a guard, so `Drop` can only ever remove a
    /// directory this module just created under the temp dir.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        // Best effort: the verifier's verdict is the outcome that matters, and a directory
        // that outlives it is a smaller problem than a panic while unwinding.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One staging directory for this verification, retrying on a name collision (a retry that a
/// predictable name could not make safe, but a fresh name plus exclusive creation does).
///
/// The only way to obtain a [`StagingDir`]: it always creates a fresh, exclusively-created
/// directory under the temp dir, so the guard's `Drop` removes only what this call made and
/// no caller can hand it an arbitrary path to delete.
pub fn staging_dir() -> std::io::Result<StagingDir> {
    let base = std::env::temp_dir();
    let mut attempt = 0;
    loop {
        match create_staging_dir(&base, &staging_dir_name(attempt)) {
            Ok(dir) => return Ok(StagingDir(dir)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 16 => {
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// The pinned `minisign` CLI. Absent in this development environment, which the outcome
/// reports rather than guesses around.
pub struct MinisignCli;

impl SignatureVerifier for MinisignCli {
    fn scheme(&self) -> &'static str {
        "minisign"
    }

    fn verify(&self, message: &[u8], signature: &str, public_key: &str) -> SignatureOutcome {
        let dir = match staging_dir() {
            Ok(dir) => dir,
            Err(e) => {
                // Names what failed: `staging_dir` fails on an exhausted run of
                // `AlreadyExists` collisions as well as on a permission error or a full
                // temp dir, so "no temporary directory" described only the last of those.
                return SignatureOutcome::Unverifiable(format!(
                    "could not create a staging directory: {e}"
                ));
            }
        };
        let msg = dir.path().join("baseline.json");
        let sig = dir.path().join("baseline.json.minisig");
        let key = dir.path().join("req.pub");
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

    // The direction of the mismatch decides the advice: a *stale* envelope is exactly the case
    // ADR 0013's re-issue is for, but a manifest written by a *newer* build must not be told to
    // re-sign with this binary — that would downgrade the attested surface instead of upgrading
    // the reader.
    if manifest.version < BASELINE_VERSION {
        open.push(format!(
            "manifest version {} predates the version this build writes ({BASELINE_VERSION}) — \
             {REISSUE_ADVICE}",
            manifest.version
        ));
    } else if manifest.version > BASELINE_VERSION {
        open.push(format!(
            "manifest version {} is newer than the version this build writes \
             ({BASELINE_VERSION}) — verify with a build that writes it",
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
    let Some(path) = verify_path else {
        let rows = match crate::view::load_rows(&context) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] {e}");
                return 3;
            }
        };
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
    let effective = match resolve_selection(&context, &manifest) {
        Ok(selection) => selection,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    let rows = match crate::view::load_rows(&effective) {
        Ok(rows) => rows,
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

/// The selection a Baseline is verified (and audited) under: **the one the manifest records**.
///
/// The manifest defines the variant, so a caller's context is not an input to that question.
/// An explicit context that contradicts the record is a **usage error**, not a finding: a
/// mismatch re-derived under the wrong variant surfaces as drift in the claims, which reads as a
/// defect in the requirements rather than a mistake in the invocation. (It did exactly that
/// during the first signing: an intact Baseline reported `REQ-0012: in the Baseline but no longer
/// applies` because verification had been given an empty context.)
pub fn resolve_selection(explicit: &Context, manifest: &Manifest) -> Result<Context, String> {
    if explicit.is_empty() || *explicit == manifest.selection {
        return Ok(manifest.selection.clone());
    }
    let show = |context: &Context| {
        if context.is_empty() {
            "(none)".to_string()
        } else {
            context
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    Err(format!(
        "this Baseline records selection [{}] but [{}] was passed: verify it without a context \
         (the manifest defines the variant), or pass exactly the recorded one",
        show(&manifest.selection),
        show(explicit)
    ))
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
