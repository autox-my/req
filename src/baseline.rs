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

/// How many fresh names one verification will try before giving up on a collision. The name
/// already carries a clock reading, so a real collision is unreachable; the bound is here only
/// so a pathological filesystem cannot make the loop run forever.
const STAGING_ATTEMPTS: u32 = 16;

// The four `pub` items from here to `staging_dir` are public for one reason: tests/baseline.rs
// pins this contract through the real interface (exclusive creation, 0700, the planted-symlink
// case, the guard's drop). The verifier's own surface is `MinisignCli::verify`; nothing outside
// this crate is meant to call these. Moving those tests into this module was weighed and
// rejected — they would need a directory fixture of their own, and this suite's stated failure
// mode is fixture copies drifting apart (`tests/common/mod.rs`).
/// The name of one staging attempt:
/// `req-baseline-<pid>-<nanos-since-epoch>-<attempt>-<serial>-<salt>`.
///
/// The process id alone is a name another local process can guess and pre-create; the clock, a
/// per-call counter and a process-local `RandomState` seed are what make guessing it fail
/// rather than pay off. The seed carries the guarantee the clock cannot: a pre-epoch or
/// unreadable clock would otherwise collapse the name to `req-baseline-<pid>-0-<attempt>`, the
/// one string an attacker could pre-create to make every verification `Unverifiable`.
///
/// `RandomState` is documented only as *unpredictable*, not as a randomness API, so this leans
/// on std's seeding rather than on a cryptographic guarantee. That assumption is explicit: it
/// suffices because a wrong guess costs the attacker a failed pre-creation while a right one
/// would cost every verification, and a stronger source would mean a new dependency
/// (`Cargo.lock` is a pin).
#[doc(hidden)]
pub fn staging_dir_name(attempt: u32) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let salt = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!(
        "req-baseline-{}-{nanos}-{attempt}-{serial}-{salt:016x}",
        std::process::id()
    )
}

/// Create the staging directory under `base` exclusively, mode 0700 on Unix.
///
/// `DirBuilder::create` — never `create_dir_all` — so a path that already exists, whether a
/// directory left behind by an earlier run or a symlink planted at the name, is an
/// `AlreadyExists` error instead of a directory we then write the trusted inputs into. On
/// Unix the directory is created 0700, which keeps another local user out of the staged
/// manifest between writing it and minisign reading it; other platforms get the platform
/// default, so that privacy is a Unix property and not a portable one. `base` itself is the
/// caller's to provide: this creates only the final component, and [`staging_dir`] creates a
/// missing base before calling.
#[doc(hidden)]
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
#[doc(hidden)]
#[must_use = "dropping the guard removes the staging directory it owns"]
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
        // that outlives it is a smaller problem than a panic while unwinding. A failure here —
        // or a SIGKILL, which skips the drop entirely — leaks one uniquely-named directory per
        // run under the base. There is no logger in a core crate with no I/O surface, and a
        // retry on a later run would target a different name, so the leak is left to the
        // platform's temp reaper and recorded here rather than quietly implied.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One staging directory for this verification, retrying on a name collision (a retry that a
/// predictable name could not make safe, but a fresh name plus exclusive creation does).
///
/// The only way to obtain a [`StagingDir`]: it always creates a fresh, exclusively-created
/// directory under the temp dir, so the guard's `Drop` removes only what this call made and
/// no caller can hand it an arbitrary path to delete.
///
/// The temp dir itself must be one only its owner may write, or one that is sticky and owned by
/// you or root, as `/tmp` (1777, root-owned) is. That is the operator contract, and it is
/// checked rather than assumed: another local user who may write the base could unlink the 0700
/// staging directory and put a symlink in its place, so such a base is never staged into. A
/// base that does not exist yet is created 0700 and satisfies the same contract.
///
/// When the temp dir cannot be used, the user's own cache directory is tried before refusing,
/// so a hardened environment does not become one where no verification can run at all. The
/// temp dir's refusal is therefore only *reported* — as `Unverifiable`, naming the directory to
/// fix — when no candidate is safe; a run that falls back says nothing, because the verifier
/// reports through its outcome vocabulary and a warning on stderr is not part of it. `pub` for
/// the integration tests, which pin this contract through the real interface — the verifier's
/// own surface is
/// `MinisignCli::verify`.
#[doc(hidden)]
pub fn staging_dir() -> std::io::Result<StagingDir> {
    let mut bases = Vec::new();
    // `temp_dir()` returns TMPDIR/TMP/TEMP verbatim on Unix, so it can be relative; the chain
    // check needs an absolute base, or `ancestors()` would stop at the empty component and skip
    // every directory above the working directory — the ones a rename attack would use.
    if let Ok(temp) = std::path::absolute(std::env::temp_dir()) {
        bases.push(temp);
    }
    if let Some(root) = private_staging_root() {
        bases.push(root);
    }
    staging_dir_first_safe(&bases)
}

/// Try each candidate base in order and return the first staging directory that can be made.
///
/// The environment's temp dir is the first candidate and what every ordinary run uses; the
/// user's own cache root is the fallback for a system whose temp dir is one another local user
/// may write. Only a refusal moves on to the next candidate: a transient failure — a full or
/// unwritable filesystem, an exhausted run of name collisions — is that base's answer, and
/// relocating into the persistent cache tree would hide it. The first refusal is kept, so an
/// operator is told about the temp dir they configured rather than only about the fallback.
fn staging_dir_first_safe(bases: &[PathBuf]) -> std::io::Result<StagingDir> {
    let mut refusal = None;
    for base in bases {
        // Only a chain refusal moves on to the next candidate, and the chain is asked directly:
        // an unwritable directory yields the very same `PermissionDenied` from `mkdir`, but that
        // is the filesystem's answer for this base, and relocating staging into the persistent
        // cache tree would hide it from the operator. The first candidate that passes the chain
        // is used, and its own errors are reported rather than second-guessed. On platforms with
        // no mode or owner to check there is no refusal, so the first candidate is the only one.
        match require_safe_base(base, None) {
            Ok(()) => return staging_dir_in(base),
            Err(e) => {
                refusal.get_or_insert(e);
            }
        }
    }
    Err(refusal.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no staging base is available")
    }))
}

/// A private staging root under the user's own cache directory, for a system whose temp dir is
/// not one only its owner may write. `None` when the environment names no home to put it in, in
/// which case the temp dir's own refusal is what the caller reports.
///
/// Only absolute values are used: the XDG basedir spec requires an absolute `XDG_CACHE_HOME`,
/// and a relative one would silently resolve against the process's working directory. The root
/// is a cache directory and is meant to persist: the guard removes the staging directory inside
/// it, and the empty parents stay, as a cache directory's do.
fn private_staging_root() -> Option<PathBuf> {
    fn absolute(dir: std::ffi::OsString) -> Option<PathBuf> {
        let path = PathBuf::from(dir);
        path.is_absolute().then_some(path)
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .and_then(absolute)
        .or_else(|| {
            std::env::var_os("HOME")
                .and_then(absolute)
                .map(|home| home.join(".cache"))
        })?;
    Some(cache.join("req/staging"))
}

/// The staging attempt loop against an explicit base, so a test can drive it without mutating
/// the process environment.
fn staging_dir_in(base: &Path) -> std::io::Result<StagingDir> {
    // `temp_dir()` is read from the environment (TMPDIR/TMP/TEMP) and may name a directory
    // that does not exist yet, and `create_staging_dir` makes only the final component. The
    // pre-F9 `create_dir_all` covered that; `ensure_base` keeps that behaviour while choosing
    // the mode rather than inheriting it from the umask.
    //
    // The first pass validates the chain *before* anything is created, so a refusal here cannot
    // leave a directory behind inside a location it has just called untrustworthy. A component
    // that does not exist yet is skipped by the check and created 0700 by `ensure_base`, which
    // cannot be group- or other-writable.
    //
    // The second pass needs our effective uid, so it can only run once the staging directory
    // exists. If it refuses — a sticky component owned by someone else — the base `ensure_base`
    // created is left in place: undoing it would mean knowing which components were ours and
    // which were already there, and `remove_dir` on a pre-existing empty base would delete a
    // directory we did not create. That leak is accepted and recorded here rather than papered
    // over, and it is one empty directory on a path that requires an attacker-owned sticky
    // ancestor.
    require_safe_base(base, None)?;
    ensure_base(base)?;
    let mut attempt = 0;
    loop {
        match create_staging_dir(base, &staging_dir_name(attempt)) {
            Ok(dir) => {
                let guard = StagingDir(dir);
                // The directory just created is owned by our effective uid, which the standard
                // library does not otherwise expose. With it, the sticky-and-writable components
                // are re-checked for an owner who could unlink our entry; a refusal drops the
                // guard, so the transient directory goes with it.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let me = std::fs::metadata(guard.path())?.uid();
                    require_safe_base(base, Some(me))?;
                }
                return Ok(guard);
            }
            // Advance only while another fresh name remains, so `STAGING_ATTEMPTS` names are
            // tried — attempt 0 through `STAGING_ATTEMPTS - 1` — and not one more.
            Err(e)
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    && attempt + 1 < STAGING_ATTEMPTS =>
            {
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Create `base` if it is missing — mode 0700 on Unix — and leave an existing directory's mode
/// alone, since a `TMPDIR` someone else configured is not ours to chmod.
fn ensure_base(base: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    // `recursive` makes an existing directory a success and leaves its mode alone, so a
    // `TMPDIR` someone else configured is not chmodded; whether it is a safe place to stage
    // into is `require_safe_base`'s question, not this call's. An existing *non*-directory is
    // still an error.
    builder.create(base)
}

/// Refuse a base another local user could swap the staging directory out of.
///
/// `create_staging_dir` makes the staging directory exclusively and 0700, but exclusivity says
/// nothing about the directory holding it: removing an entry needs write permission on the
/// *parent*, so in a group- or other-writable directory another local user can unlink the
/// freshly created 0700 directory, plant a symlink at the same name, and have the trusted
/// inputs written — and the guard's `remove_dir_all` aimed — where they choose.
///
/// The sticky bit narrows that but does not close it, because the restriction does not apply to
/// the directory's own owner: a same-group attacker who owns a sticky, group-writable directory
/// on the path can still unlink our entry. A writable component is therefore safe only when it
/// is sticky *and* its owner is root or us — which is why `me` exists. The standard library
/// exposes no `geteuid`, so the caller learns our uid from the directory it has just created
/// and re-checks with `Some(me)`; the pre-creation pass runs with `None`, accepts a sticky
/// component provisionally, and the second pass is what refuses it. `/tmp` (1777, root-owned)
/// passes both.
///
/// The check walks the whole chain, and it is still a check followed by an act: the `mkdir` is
/// a separate syscall, so a base that passes can be swapped in between. Closing that needs
/// `openat`/`O_NOFOLLOW` semantics the standard library does not offer without a new
/// dependency, so what remains is a race a local attacker has microseconds to win rather than
/// the standing invitation of staging into a directory anyone may write. A base that no pass
/// accepts makes the verification `Unverifiable`, which is the honest outcome when the inputs
/// cannot be staged somewhere only we can reach them, and the reason names the directory so an
/// operator can fix the mode rather than guess.
///
/// The caller passes an absolute path: `ancestors()` of a relative one stops at the empty
/// component, which would silently skip every directory between the base and the working
/// directory — the very components a rename attack would use.
#[cfg(unix)]
fn require_safe_base(base: &Path, me: Option<u32>) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    // A real assert, not a debug one: the failure it guards is silent in release — a relative
    // base would skip every component above the working directory — and the guard is cheap.
    assert!(
        base.is_absolute(),
        "the chain check needs an absolute base, or it would skip the components above the base"
    );
    // Every directory on the way down, not just the leaf: renaming the leaf's parent is enough
    // to move the leaf elsewhere, so a private base under a writable ancestor is no safer.
    for dir in base.ancestors() {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let meta = match std::fs::metadata(dir) {
            Ok(meta) => meta,
            // Not created yet: `ensure_base` makes it 0700 inside a chain already checked above
            // it, so it cannot be the unsafe link.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let mode = meta.permissions().mode();
        if mode & 0o022 == 0 {
            continue; // only the owner may write here, so no one else can unlink our entry
        }
        let owner = meta.uid();
        let owner_is_trusted = owner == 0 || me == Some(owner);
        let sticky = mode & 0o1000 != 0;
        // With `me` unknown — before anything is created — this is provisional; the second
        // pass, once our uid is known, is what refuses a sticky directory owned by someone
        // else, whose owner could unlink our entry regardless of the sticky bit.
        if !sticky || (me.is_some() && !owner_is_trusted) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "the staging base {} runs through {}, which is group/other-writable {}: make \
                     the temporary directory private (mode 0700), or sticky and owned by you or \
                     root, or point TMPDIR at one that is",
                    base.display(),
                    dir.display(),
                    if sticky {
                        "and owned by another user"
                    } else {
                        "without the sticky bit"
                    }
                ),
            ));
        }
    }
    Ok(())
}

/// No portable ownership or mode check exists; on other platforms this adds nothing and the
/// staging directory keeps the platform default, as elsewhere in this module.
#[cfg(not(unix))]
fn require_safe_base(_base: &Path, _me: Option<u32>) -> std::io::Result<()> {
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A base under the real temp dir, unique to this process and moment. Absolutized for the
    /// same reason `staging_dir` does it: `TMPDIR` is returned verbatim and may be relative.
    fn unique_base(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::path::absolute(std::env::temp_dir())
            .expect("the temp dir resolves to an absolute path")
            .join(format!("{tag}-{}-{nanos}", std::process::id()))
    }

    /// Whether the ambient temp dir's chain passes even the provisional check. On a host whose
    /// temp dir sits under a group/other-writable, non-sticky ancestor the production refusal
    /// is correct, but a test that needs a *successful* staging run has nothing to assert.
    fn ambient_temp_dir_is_usable() -> bool {
        std::path::absolute(std::env::temp_dir())
            .map(|temp| require_safe_base(&temp, None).is_ok())
            .unwrap_or(false)
    }

    /// A base that does not exist yet — an unusual `TMPDIR` — must be created rather than
    /// failing the verification as `Unverifiable`, which the pre-F9 `create_dir_all` covered.
    /// The staging name itself stays exclusively created, so this does not weaken the guard.
    #[test]
    fn a_missing_base_is_created_for_the_staging_directory() {
        if !ambient_temp_dir_is_usable() {
            eprintln!("skipping: the ambient temp dir chain is not private or root-sticky");
            return;
        }
        let base = unique_base("req-missing-base");
        assert!(!base.exists(), "the fixture base starts absent");

        let guard = staging_dir_in(&base).expect("a missing base is created, not an error");
        let dir = guard.path().to_path_buf();
        assert!(
            dir.exists(),
            "the staging directory exists under the created base"
        );
        drop(guard);
        assert!(
            !dir.exists(),
            "the guard still removes the staging directory"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Unlinking the 0700 staging directory needs write permission on the directory that
    /// holds it, not on the staging directory itself. A base another local user may write,
    /// without the sticky bit, therefore lets them replace the staging directory with a
    /// symlink and aim the trusted inputs — and the guard's `remove_dir_all` — at a path they
    /// chose.
    #[cfg(unix)]
    #[test]
    fn a_base_another_user_could_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let base = unique_base("req-unsafe-base");
        std::fs::create_dir_all(&base).expect("the fixture base");
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o777))
            .expect("the fixture base is made world-writable");

        let error = match staging_dir_in(&base) {
            Ok(_) => panic!("an unsafe base must be refused, not staged into"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A safe leaf under an unsafe parent is still a base another user can swap — renaming the
    /// parent is enough — so the whole chain is checked, not just the leaf.
    #[cfg(unix)]
    #[test]
    fn a_base_under_a_writable_ancestor_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let outer = unique_base("req-unsafe-ancestor");
        std::fs::create_dir_all(&outer).expect("the fixture outer directory");
        std::fs::set_permissions(&outer, std::fs::Permissions::from_mode(0o777))
            .expect("the outer directory is made world-writable");
        let base = outer.join("staging");
        std::fs::create_dir_all(&base).expect("the fixture base");
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700))
            .expect("the base itself is private");

        let error = match staging_dir_in(&base) {
            Ok(_) => panic!("a base under a writable ancestor must be refused"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&outer);
    }

    /// A refusal must not leave a directory behind inside the location it has just called
    /// untrustworthy, so the chain is checked before anything is created.
    #[cfg(unix)]
    #[test]
    fn a_refused_base_is_not_created() {
        use std::os::unix::fs::PermissionsExt;
        let outer = unique_base("req-unsafe-parent-no-create");
        std::fs::create_dir_all(&outer).expect("the fixture outer directory");
        std::fs::set_permissions(&outer, std::fs::Permissions::from_mode(0o777))
            .expect("the outer directory is made world-writable");
        let base = outer.join("never-created");

        assert!(
            staging_dir_in(&base).is_err(),
            "a chain through a writable ancestor is refused"
        );
        assert!(
            !base.exists(),
            "the refusal must not leave {} behind",
            base.display()
        );

        let _ = std::fs::remove_dir_all(&outer);
    }

    /// A temp dir another local user may write must not become a verification that cannot run
    /// at all: the next candidate base is used, and only when none is safe does it refuse.
    #[cfg(unix)]
    #[test]
    fn an_unsafe_base_falls_back_to_a_safe_one() {
        use std::os::unix::fs::PermissionsExt;
        if !ambient_temp_dir_is_usable() {
            eprintln!("skipping: the ambient temp dir chain is not private or root-sticky");
            return;
        }
        let unsafe_base = unique_base("req-fallback-unsafe");
        std::fs::create_dir_all(&unsafe_base).expect("the fixture unsafe base");
        std::fs::set_permissions(&unsafe_base, std::fs::Permissions::from_mode(0o777))
            .expect("the unsafe base is made world-writable");
        let safe_base = unique_base("req-fallback-safe");

        let guard = staging_dir_first_safe(&[unsafe_base.clone(), safe_base.clone()])
            .expect("the safe candidate is used when the temp dir is not");
        let dir = guard.path().to_path_buf();
        assert!(
            dir.starts_with(&safe_base),
            "the staging directory is under the safe base: {dir:?}"
        );
        drop(guard);

        let _ = std::fs::remove_dir_all(&unsafe_base);
        let _ = std::fs::remove_dir_all(&safe_base);
    }

    /// `/tmp` itself is 1777: writable by everyone, but the sticky bit means only an entry's
    /// owner may remove it, so it must not be refused — a check that rejected it would break
    /// every verification on a system whose `TMPDIR` is `/tmp`.
    #[cfg(unix)]
    #[test]
    fn a_sticky_base_writable_by_others_is_accepted() {
        use std::os::unix::fs::PermissionsExt;
        if !ambient_temp_dir_is_usable() {
            eprintln!("skipping: the ambient temp dir chain is not private or root-sticky");
            return;
        }
        let base = unique_base("req-sticky-base");
        std::fs::create_dir_all(&base).expect("the fixture base");
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o1777))
            .expect("the fixture base is made sticky and world-writable");

        let guard = staging_dir_in(&base).expect("a sticky base is safe to stage in");
        let dir = guard.path().to_path_buf();
        drop(guard);
        assert!(
            !dir.exists(),
            "the guard still removes the staging directory"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The sticky bit does not protect against the directory's *own owner*, who may unlink any
    /// entry in it. A sticky component owned by someone other than us or root is refused, which
    /// the second pass catches once our uid is known from the directory we just created.
    #[cfg(unix)]
    #[test]
    fn a_sticky_component_owned_by_another_user_is_refused() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let base = unique_base("req-sticky-other-owner");
        std::fs::create_dir_all(&base).expect("the fixture base");
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o1777))
            .expect("the fixture base is made sticky and world-writable");
        let ours = std::fs::metadata(&base).expect("metadata").uid();
        // Owner 0 is trusted by construction, so when the suite itself runs as root there is no
        // "another user" to name and the case cannot exist: skip it rather than assert it.
        if ours == 0 {
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        let other = if ours == 1 { 2 } else { 1 };

        let error = match require_safe_base(&base, Some(other)) {
            Ok(()) => panic!("a sticky directory owned by another user must be refused"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The chain check refuses a relative base: its `ancestors()` stops at the empty component,
    /// skipping every directory above the working directory. `staging_dir` absolutizes
    /// `temp_dir()` before this point, so this fires only if that is ever bypassed.
    #[cfg(unix)]
    #[test]
    #[should_panic(expected = "absolute base")]
    fn a_relative_base_is_rejected_by_the_chain_check() {
        let _ = require_safe_base(Path::new("relative/staging"), None);
    }
}
