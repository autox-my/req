//! The Baseline: a signed statement of which addresses were ratified (ADR 0005, ADR 0010,
//! ADR 0016).
//!
//! A Baseline is a **manifest** — the rules version, the selection context, and every
//! applying claim's address — over which a human places one signature. Verification
//! re-derives those addresses from the working tree, so a claim edited after signing fails
//! rather than quietly passing; and the signature check is a **process boundary** to a pinned
//! verifier, so the core crate gains no cryptography and still no network.
//!
//! Envelope v3 (ADR 0016) attests two more facts inside the signed bytes: the **Decision log's
//! chain head** (`authority`), which ties the signature to the authority that ratified those
//! addresses, and the **signing time** (`signed_at`), which adjudicates the *key* — expiry and
//! prospective revocation are judged at signing time, never at today's clock.
//!
//! The rules version is judged **before** drift ([`rules_mismatch`]), because drift is derived
//! from it: a manifest written under a different canonical rules version is UNVERIFIABLE, with
//! the remedy in the direction of the mismatch, rather than a per-claim tampering report.
//!
//! Three outcomes, as everywhere else. **Revoked** trust is FAIL **when this build can judge the
//! time the key is judged at** (a human withdrew trust); an envelope *newer* than this build's
//! records a signing time this build may not interpret, so the same revocation cannot become a
//! definite finding there — it is UNVERIFIABLE, and the version dispatch has already said so. A
//! trust anchor whose own `revoked_at` cannot be read is a separate, **time-independent** case
//! ([`TrustState::RevokedAnchor`]): it fails closed on every envelope, because the defect is the
//! anchor and not the envelope's signing time.
//! **Expired** trust is UNVERIFIABLE (a v2 Baseline is historical and no trusted signing time
//! exists; a v3 Baseline records one, so the expiry is judged at it — still never a silent
//! pass); an unknown key or a missing signature is UNVERIFIABLE. A v3 manifest whose recorded
//! authority no longer re-derives from `decisions/` is FAIL — the same class as a hand-edited
//! digest.

use crate::decision;
use crate::select::{Applies, Context};
use crate::view::Row;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The manifest format's own version — distinct from `RULES_V`, because the *envelope* and
/// the *rules* move independently.
/// The manifest schema this build writes. v4 attests the claim's authored test links
/// (`tests_hash`, ADR 0019); v3 attests the Decision log's chain head and the signer's timestamp
/// (ADR 0016); v2 attests the audit surface (ADR 0013).
pub const BASELINE_VERSION: u32 = 4;
/// The first envelope that attests each claim's authored test links (ADR 0019). A manifest older
/// than this predates the disclosure, and the disclosure is about tests — not about whatever the
/// oldest supported envelope happens to be.
pub const TESTS_ATTESTED_FROM: u32 = 4;

/// The **oldest envelope this build still verifies**.
///
/// A bump is not a cliff for signed history: a v3 manifest's historical signed bytes are
/// unchanged by v4, and the one thing v4 adds — the attested `tests_hash` — is simply *not
/// attested* by a v3 manifest, which is reported once rather than guessed at. That is why the
/// support floor moves independently of [`BASELINE_VERSION`]: v4 is what this build writes, v3
/// is what it still honours. A v2 manifest (ADR 0013) predates the authority and signing time a
/// v3 Baseline attests, so it remains a re-issue candidate — UNVERIFIABLE, never silently
/// re-signed by this build.
pub const OLDEST_VERIFIABLE_BASELINE_VERSION: u32 = 3;

/// Where the committed trust anchor lives. Data, reviewed like a corpus (ADR 0010).
pub const TRUST_PATH: &str = "baselines/trust.yml";

/// One ratified claim: its identity, and the audit surface attested beside it. Identity
/// stays meaning-only (ADR 0007) — the hashes cover what an auditor relies on that the
/// address deliberately does not (ADR 0013, ADR 0019).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestClaim {
    pub claim_id: String,
    pub address: String,
    pub relations_hash: String,
    pub rationale_hash: String,
    /// The authored test links' digest (v4, ADR 0019). `Option` so a **v3** manifest — which
    /// never attested them — still parses and verifies over its historical bytes; a v4 manifest
    /// must carry it for every claim, and [`from_json`] refuses one that does not.
    #[serde(default)]
    pub tests_hash: Option<String>,
}

/// The authority a v3 signature is tied to (ADR 0016): the Decision log's chain head and how
/// many entries the log holds. `entries` rides beside `head` because a head alone cannot
/// distinguish "one entry" from a log rebuilt to one entry with the same head — impossible in
/// principle, cheap to state in fact.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Authority {
    pub head: String,
    pub entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub version: u32,
    /// The signer's RFC 3339 UTC timestamp. Present in a v3 manifest, **inside the signed
    /// bytes**; absent (v2) means the envelope predates ADR 0016. `Option` so a v2 manifest
    /// still parses.
    #[serde(default)]
    pub signed_at: Option<String>,
    /// The Decision log's chain head (v3). `Option` for the same reason: a v2 manifest is not
    /// required to carry an authority it never attested.
    #[serde(default)]
    pub authority: Option<Authority>,
    pub rules_v: u32,
    /// The variant this Baseline is over — empty means every applying claim with no context.
    pub selection: BTreeMap<String, String>,
    pub claims: Vec<ManifestClaim>,
}

/// Build a manifest from the claims that apply in `selection`, sorted by id and deduplicated
/// so the bytes never depend on input order.
///
/// The result is an **unattested** v4 envelope: `signed_at` and `authority` are filled by
/// [`attest`], which only the emit path calls (`req baseline --at <ts>`). A draft is never
/// emitted — [`canonical_bytes`] leaves the attestation out until it is present, and
/// [`from_json`] rejects a v4 manifest that lacks it.
pub fn manifest(rows: &[Row], rules_v: u32, selection: &Context) -> Manifest {
    let mut claims: Vec<ManifestClaim> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| ManifestClaim {
            claim_id: row.claim_id.clone(),
            address: row.address.clone(),
            relations_hash: row.relations_hash.clone(),
            rationale_hash: row.rationale_hash.clone(),
            tests_hash: Some(row.tests_hash.clone()),
        })
        .collect();
    claims.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    claims.dedup_by(|a, b| a.claim_id == b.claim_id);
    Manifest {
        version: BASELINE_VERSION,
        signed_at: None,
        authority: None,
        rules_v,
        selection: selection.clone(),
        claims,
    }
}

/// Fill the two v3 attestations into a manifest. `signed_at` is the signer's assertion, never a
/// clock req invents; `head` is the Decision log's chain head, re-derived from `decisions/`.
///
/// **The time is validated here, not only at the CLI.** `from_json` rejects a v3 manifest whose
/// `signed_at` is not an RFC 3339 UTC timestamp — the shape is inside the signed bytes, and
/// `TrustFile::state` compares dates lexicographically, so an unreadable spelling would sort
/// below every trust date and read a revoked key as trusted. The CLI validates `--at` before it
/// gets here, but `attest` is a **library** entry point: without this check a caller could emit
/// an "attestation" that the only reader of a v3 manifest refuses, and the failure would surface
/// as a parse error at verification time with nothing to point at. An invalid time writes
/// nothing, so a rejected call leaves the manifest unattested rather than half-filled.
pub fn attest(
    manifest: &mut Manifest,
    signed_at: &str,
    head: &decision::Head,
) -> Result<(), String> {
    if !valid_signing_time(signed_at) {
        return Err(format!(
            "`signed_at` {signed_at:?} is not an RFC 3339 UTC timestamp with a `Z` offset, so it \
             cannot be attested — the value is inside the signed bytes and adjudicates the key's \
             expiry and revocation; pass the signer's `--at` value (e.g. 2026-10-20T18:30:00Z)"
        ));
    }
    manifest.signed_at = Some(signed_at.to_string());
    manifest.authority = Some(Authority {
        head: head.hash.clone(),
        entries: head.entries,
    });
    Ok(())
}

/// The **exact bytes a signature covers**: fixed key order, sorted selection and claims, no
/// incidental whitespace, one trailing newline. The same discipline as the address path —
/// what is signed must be reproducible byte for byte by anyone.
///
/// v3's fixed key order is `version, signed_at, authority, rules_v, selection, claims`
/// (ADR 0016), and v4 appends `tests_hash` to each claim record (ADR 0019). Both are pinned by
/// tests because this byte string is what the human signs.
/// A **v2** manifest keeps its historical shape (no `signed_at`/`authority` keys), and a **v3**
/// manifest keeps its four-key claim records (no `tests_hash`), so an old signature still
/// verifies over the bytes it was made over and a stale envelope is reported as *predates*,
/// never as a signature failure.
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
            // v4 appends `tests_hash` (ADR 0019). A v3 manifest's claims keep their historical
            // four-key shape, because that is the byte string its signature covers — adding a
            // key with a null value would silently stop every old signature from verifying.
            let tests = match &claim.tests_hash {
                Some(tests_hash) if manifest.version >= 4 => {
                    format!(",\"tests_hash\":{}", json_string(tests_hash))
                }
                _ => String::new(),
            };
            format!(
                "{{\"claim_id\":{},\"address\":{},\"relations_hash\":{},\"rationale_hash\":{}{tests}}}",
                json_string(&claim.claim_id),
                json_string(&claim.address),
                json_string(&claim.relations_hash),
                json_string(&claim.rationale_hash)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    // The attested fields are emitted only for v3, and only once both are present: an
    // unattested draft serializes in the v2 shape, which `from_json` rejects for a v3
    // declaration — it is never a signable string.
    let attestation = match (manifest.signed_at.as_deref(), &manifest.authority) {
        (Some(signed_at), Some(authority)) if manifest.version >= 3 => format!(
            "\"signed_at\":{},\"authority\":{{\"head\":{},\"entries\":{}}},",
            json_string(signed_at),
            json_string(&authority.head),
            authority.entries
        ),
        _ => String::new(),
    };
    format!(
        "{{\"version\":{},{attestation}\"rules_v\":{},\"selection\":{{{selection}}},\"claims\":[{claims}]}}\n",
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
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
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

/// Whether this build judges a manifest of `version` by its attested signing time.
///
/// The support window, not a point: v4 is what this build writes, v3 is what it still honours,
/// and both attest `signed_at` — so both have their key judged at it (ADR 0016, ADR 0019). A
/// newer envelope's field is not this build's to interpret, and an older (v2) manifest carries
/// none, so a caller must not read a signing time off either.
fn judges_signing_time(version: u32) -> bool {
    (OLDEST_VERIFIABLE_BASELINE_VERSION..=BASELINE_VERSION).contains(&version)
}

/// Parse a manifest back. Reading is not the address path, so the ordinary JSON reader is
/// fine here — only the *emitted* bytes have to be canonical.
///
/// A **v3** manifest must attest both `signed_at` and `authority`; one that declares v3 and
/// lacks either is corrupt, not stale, and is rejected with a message that says what a v3
/// envelope carries and how to produce one. v2 manifests parse without them (the fields are
/// `Option`), so the envelope-version dispatch in [`verify`] can report *predates* — never a
/// parse error, and never a signature failure over bytes the old build did not write.
pub fn from_json(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| {
        let detail = e.to_string();
        // Branch on the *declared envelope version*, recovered by a lenient parse that
        // survives the missing fields serde is complaining about. Not on serde's message text,
        // which is not a stable contract; and not on the field name alone, because a stale
        // manifest is missing *both* attested hashes while serde names only the first — a
        // current-version manifest that lost exactly one hash would otherwise be told it is
        // stale. The stale test is *any declared version below the oldest this build still
        // verifies*, derived from the constant rather than hardcoded, so the next envelope bump
        // cannot silently drop the advice; a manifest declaring a supported version (v3) or a
        // newer one is corrupt, and re-issue advice would send its reader off to re-sign over a
        // defect that was never a stale envelope (ADR 0013, ADR 0019).
        let declared = serde_json::from_str::<EnvelopeVersion>(text)
            .ok()
            .and_then(|envelope| envelope.version);
        // Binding the recovered version keeps the invariant local: the stale branch runs only
        // when a declared version below this build's was observed, so the message cannot print
        // a fabricated fallback.
        if let Some(version) =
            declared.filter(|version| *version < OLDEST_VERIFIABLE_BASELINE_VERSION)
        {
            format!(
                "manifest is not readable: {detail} — this manifest declares version {version}, \
                     which predates the version this build writes ({BASELINE_VERSION}): \
                     {REISSUE_ADVICE}"
            )
        } else {
            format!("manifest is not readable: {detail}")
        }
    })?;
    // Every envelope in the support window attests its signing time and authority (ADR 0016);
    // **v4** adds the tests digest per claim (ADR 0019). A v4 header with v3 records is corrupt,
    // not a v3 envelope wearing a new number — so the field set is checked per version, and the
    // message names what the declared envelope must carry.
    if judges_signing_time(manifest.version) {
        let mut missing: Vec<&str> = [
            ("signed_at", manifest.signed_at.is_none()),
            ("authority", manifest.authority.is_none()),
        ]
        .into_iter()
        .filter_map(|(field, absent)| absent.then_some(field))
        .collect();
        if manifest.version == BASELINE_VERSION
            && manifest
                .claims
                .iter()
                .any(|claim| claim.tests_hash.is_none())
        {
            missing.push("`tests_hash` for every claim");
        }
        if !missing.is_empty() {
            let carries = if manifest.version == BASELINE_VERSION {
                "`signed_at`, `authority` and a `tests_hash` per claim"
            } else {
                "`signed_at` and `authority`"
            };
            return Err(format!(
                "manifest is not readable: this manifest declares version {} but does not \
                 attest {} — a v{} envelope carries {carries}; emit one with `req baseline \
                 --at <RFC 3339 UTC timestamp>`, then re-sign",
                manifest.version,
                missing.join(" or "),
                manifest.version
            ));
        }
    }
    // **A present `signed_at` must also be a time**, on every envelope whose signing time this
    // build judges — v3 history included, because that time now adjudicates the key here. It is
    // fed to `TrustFile::state`, whose comparisons are lexicographic on the date; an empty or
    // non-date spelling sorts below every trust date and would skip both the expiry and the
    // revocation check — reading a revoked key as trusted. This is the one choke point both the
    // library and the CLI use.
    if judges_signing_time(manifest.version)
        && let Some(signed_at) = &manifest.signed_at
        && !valid_signing_time(signed_at)
    {
        return Err(format!(
            "manifest is not readable: `signed_at` {signed_at:?} is not an RFC 3339 UTC \
             timestamp, so the key's expiry and revocation cannot be judged; re-issue with \
             `req baseline --at <RFC 3339 UTC timestamp>` and re-sign"
        ));
    }
    Ok(manifest)
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
    /// In the manifest at the same address, but the attested surface (relations, rationale, or
    /// — in a v4 envelope — the authored test links; ADR 0013, ADR 0019) no longer recomputes —
    /// edited after signing. Identity stayed put, which is exactly why this is its own category:
    /// the edit is not a reworded requirement, it is a changed record of what was ratified.
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
                "{id}: the relations, rationale or authored test links no longer recompute — \
                 edited after signing (ADR 0013, ADR 0019)"
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

/// The manifest's declared canonical rules version, when it disagrees with the one this build
/// writes. `None` from [`rules_mismatch`] is the only state in which [`drift`] has a common
/// basis, because a claim's address is *derived from* the rules version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulesMismatch {
    /// The canonical rules version the manifest declares.
    pub manifest_rules_v: u32,
    /// The canonical rules version this build writes ([`crate::canon::RULES_V`]).
    pub build_rules_v: u32,
}

impl RulesMismatch {
    /// What a reader must be told, in the direction of the mismatch.
    ///
    /// Two things are non-negotiable in the wording, and both are why this is one function
    /// rather than a `format!` per call site: it says **drift was NOT judged** — otherwise a
    /// reader sees no drift findings and concludes the claims match — and it gives the remedy the
    /// mismatch calls for. A manifest older than the build is re-signed under the current rules
    /// (the case ADR 0013's `REISSUE_ADVICE` is for); a manifest *newer* than the build must be
    /// verified with a build that writes it, never re-signed with this binary, which would
    /// downgrade what the manifest attests.
    pub fn reason(&self) -> String {
        let Self {
            manifest_rules_v,
            build_rules_v,
        } = *self;
        let head = format!(
            "the manifest declares canonical rules v{manifest_rules_v} but this build writes \
             v{build_rules_v}: drift was NOT judged — a claim's address is derived from the rules \
             version, so a comparison across two rule sets is void, not merely inconclusive"
        );
        if manifest_rules_v < build_rules_v {
            format!("{head}. Re-sign the manifest under the current rules: {REISSUE_ADVICE}")
        } else {
            format!(
                "{head}. Verify with a build that writes rules v{manifest_rules_v}; this binary \
                 cannot judge it, and re-signing under the current rules would downgrade what the \
                 manifest attests"
            )
        }
    }
}

/// Whether `manifest` was written under a different canonical rules version than this build's.
///
/// Consulted by both [`verify`] and `audit::coverage`, so the two cannot disagree about whether
/// drift may be judged at all — the same reason the audit packet derives its coverage from
/// [`drift`] rather than recomputing it.
pub fn rules_mismatch(manifest: &Manifest) -> Option<RulesMismatch> {
    let build_rules_v = crate::canon::RULES_V;
    (manifest.rules_v != build_rules_v).then_some(RulesMismatch {
        manifest_rules_v: manifest.rules_v,
        build_rules_v,
    })
}

/// The working tree against a signed manifest, claim by claim.
///
/// The comparison is only meaningful under a **matching rules version**: an address is derived
/// from the rules, so under a mismatch every claim reads as changed and the per-claim verdicts
/// are artefacts of comparing two different rule sets. Callers must consult [`rules_mismatch`]
/// first — [`verify`] returns before it judges drift, and `audit::coverage` reports NOT JUDGED —
/// rather than reading this alone.
pub fn drift(rows: &[Row], manifest: &Manifest) -> Drift {
    let current: BTreeMap<&str, (&str, &str, &str, &str)> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| {
            (
                row.claim_id.as_str(),
                (
                    row.address.as_str(),
                    row.relations_hash.as_str(),
                    row.rationale_hash.as_str(),
                    row.tests_hash.as_str(),
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
            Some((address, relations, rationale, tests)) => {
                if *address != claim.address {
                    out.changed.push(claim.claim_id.clone())
                } else if *relations != claim.relations_hash
                    || *rationale != claim.rationale_hash
                    // Only a v4 manifest **attests** the tests digest, so only there is it
                    // judged: a v3 manifest (or one parsed from a v3 file that happens to carry
                    // the key) did not sign it, and comparing an unsigned field would invent a
                    // tampering. A v4 manifest always carries one (from_json enforces it), so a
                    // changed `tests_hash` after signing is FAIL — the same class as a changed
                    // address (ADR 0019).
                    // `verify` does not return early for a newer envelope before drift, so the
                    // guard is the closed support window (4..=BASELINE_VERSION), not `>= 4`: a
                    // future envelope's `tests_hash` is not this build's to compare, and reading
                    // a difference there as "edited after signing" would FAIL a manifest the
                    // build already reported UNVERIFIABLE for being newer.
                    || ((4..=BASELINE_VERSION).contains(&manifest.version)
                        && claim
                            .tests_hash
                            .as_deref()
                            .is_some_and(|signed| signed != *tests))
                {
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
    /// The organisation or function holding the key (ADR 0042). Optional.
    #[serde(default)]
    pub party: Option<String>,
    /// The capacity every signature of this key is made in — its meaning (ADR 0042). Optional.
    #[serde(default)]
    pub role: Option<String>,
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
    /// A revocation **dated** at or before the time the key is judged at.
    Revoked(String),
    /// The anchor's own `revoked_at` cannot be read, so the key is treated as revoked (fail
    /// closed). Kept apart from [`TrustState::Revoked`] because this is **not a judgement about a
    /// time**: it stays a definite finding on every envelope version, including a newer one whose
    /// signing time this build cannot interpret. Folding the two together would let a newer
    /// envelope turn "your trust anchor is broken" into "we cannot tell", with the wrong remedy.
    RevokedAnchor(String),
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

    /// `at` is the time the key is judged at: the manifest's `signed_at` for a v3 Baseline, or
    /// today for a v2 one (ADR 0016). It may be a full RFC 3339 timestamp or a bare ISO date;
    /// both compare correctly as strings once reduced to their date, so no date library and no
    /// locale enters this path.
    ///
    /// **Revocation is prospective** (ADR 0016): a key revoked *after* `at` was still trusted
    /// when the Baseline was signed. Expiry likewise: a key valid **until at or after** `at`
    /// verifies, even when today is later.
    pub fn state(&self, id: &str, at: &str) -> TrustState {
        let Some(key) = self.key(id) else {
            return TrustState::Unknown;
        };
        // Both sides are reduced to their `YYYY-MM-DD` date: a timestamp-valued trust entry must
        // fire on its own day, and a timestamp-valued judgement time must not read a same-day
        // expiry as past because a clock came along.
        //
        // **The behaviour: a date this build cannot read fails closed.** `valid_judgement_time`
        // accepts only a calendar-valid `YYYY-MM-DD` (month 1-12, the month's real last day, year
        // >= 1970) or an RFC 3339 timestamp whose date part is valid. Anything else — a typo'd
        // month (`2026-13-01`), an unpadded day (`2026-10-1`), a human spelling (`Oct 20 2026`) —
        // is reported as **revoked** (for `revoked_at`, as [`TrustState::RevokedAnchor`], because
        // the defect is the anchor and not a judgement about a time) or **expired** (for
        // `valid_until`), never as trusted. `date_of` truncates blindly, so without that check a
        // malformed spelling would sort below every real date and read a revoked key as trusted;
        // the check is why it cannot. Pinned by
        // `tests/baseline.rs::a_malformed_trust_date_fails_closed`.
        let at_date = date_of(at);
        if let Some(revoked_at) = &key.revoked_at {
            if !valid_judgement_time(revoked_at) {
                return TrustState::RevokedAnchor(format!(
                    "revoked at {revoked_at}, which is not an ISO date or RFC 3339 timestamp — \
                     treated as revoked, never as trusted"
                ));
            }
            if at_date >= date_of(revoked_at) {
                return TrustState::Revoked(format!("revoked at {revoked_at}"));
            }
        }
        if let Some(valid_until) = &key.valid_until {
            if !valid_judgement_time(valid_until) {
                return TrustState::Expired(format!(
                    "valid until {valid_until}, which is not an ISO date or RFC 3339 timestamp — \
                     treated as expired, never as trusted"
                ));
            }
            if at_date > date_of(valid_until) {
                return TrustState::Expired(format!("valid until {valid_until}, judging at {at}"));
            }
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

/// Why a candidate base could not be used, kept apart so the fallback can act on the *reason*.
///
/// A chain refusal and an ordinary permission error carry the same `io::ErrorKind`, and only the
/// refusal — from either pass — justifies trying the next candidate.
enum BaseRefusal {
    /// The chain refused it: another local user could swap the staging directory out of it.
    Unsafe(std::io::Error),
    /// The filesystem's answer for this base.
    Io(std::io::Error),
}

/// Take the first usable candidate, falling through on a refusal and stopping on anything else.
///
/// The environment's temp dir is the first candidate and what every ordinary run uses; the
/// user's own cache root is the fallback for a system whose temp dir is one another local user
/// may write, which the chain reports as `Unsafe` whether the provisional pass or the
/// ownership pass is the one that catches it. Any other failure — a full or unwritable
/// filesystem, an exhausted run of name collisions — is that base's answer, and relocating into
/// the persistent cache tree would hide it. The first refusal is kept, so an operator is told
/// about the temp dir they configured rather than only about the fallback.
fn first_usable<T>(
    results: impl IntoIterator<Item = Result<T, BaseRefusal>>,
) -> std::io::Result<T> {
    let mut refusal = None;
    for result in results {
        match result {
            Ok(value) => return Ok(value),
            Err(BaseRefusal::Unsafe(e)) => {
                refusal.get_or_insert(e);
            }
            Err(BaseRefusal::Io(e)) => return Err(e),
        }
    }
    Err(refusal.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no staging base is available")
    }))
}

/// The same, over the candidate bases [`staging_dir`] chose.
fn staging_dir_first_safe(bases: &[PathBuf]) -> std::io::Result<StagingDir> {
    first_usable(bases.iter().map(|base| staging_dir_at(base)))
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

/// The staging attempt loop against an explicit base, erasing the reason. The unit tests drive
/// this seam; the fallback needs [`staging_dir_at`]'s reason, so nothing in the binary does.
#[cfg(test)]
fn staging_dir_in(base: &Path) -> std::io::Result<StagingDir> {
    staging_dir_at(base).map_err(|refusal| match refusal {
        BaseRefusal::Unsafe(e) | BaseRefusal::Io(e) => e,
    })
}

/// The same, with the reason kept, so the fallback can tell a chain refusal from an I/O error.
fn staging_dir_at(base: &Path) -> Result<StagingDir, BaseRefusal> {
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
    ensure_base(base).map_err(BaseRefusal::Io)?;
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
                    let me = std::fs::metadata(guard.path())
                        .map_err(BaseRefusal::Io)?
                        .uid();
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
            Err(e) => return Err(BaseRefusal::Io(e)),
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
///
/// The reason is part of the result: a component this refuses is [`BaseRefusal::Unsafe`], while
/// a `stat` that fails for any other reason is [`BaseRefusal::Io`] — the filesystem's answer,
/// which must not be relabelled as a refusal, or the fallback would move on and hide it.
#[cfg(unix)]
fn require_safe_base(base: &Path, me: Option<u32>) -> Result<(), BaseRefusal> {
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
            // A `stat` failure is the filesystem's answer, not a refusal: relabelling it would
            // send the fallback to the next candidate instead of surfacing the real cause.
            Err(e) => return Err(BaseRefusal::Io(e)),
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
            return Err(BaseRefusal::Unsafe(std::io::Error::new(
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
            )));
        }
    }
    Ok(())
}

/// No portable ownership or mode check exists; on other platforms this adds nothing and the
/// staging directory keeps the platform default, as elsewhere in this module.
#[cfg(not(unix))]
fn require_safe_base(_base: &Path, _me: Option<u32>) -> Result<(), BaseRefusal> {
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

/// The inputs verification judges beside the claims: the signature, the trust anchor, the
/// verifier process, the time the **key** is judged at, and — for a v3 envelope — the Decision
/// log whose chain head it attests.
///
/// Bundled because they travel together and a `verify` with eight positional arguments is a call
/// site waiting to be misread; the tests construct one directly, which is also the seam that
/// keeps a fixture key, an explicit signing time and a fixture log out of the filesystem.
#[derive(Clone, Copy)]
pub struct Verification<'a> {
    pub signature: Option<&'a str>,
    pub trust: &'a TrustFile,
    pub key_id: &'a str,
    pub verifier: &'a dyn SignatureVerifier,
    /// The time the key is judged at: a v3 manifest's `signed_at`, or today for v2 (ADR 0016).
    pub now: &'a str,
    /// The decision log as read from `decisions/`, or the loader's reason when it could not be
    /// read (absent directory, unreadable or unparsable file). An absent log is UNVERIFIABLE,
    /// never FAIL: "cannot tell" and "tampered" are different claims, and only a head that
    /// *re-derives differently* is the definite finding ADR 0016 calls for. The reason travels
    /// so a broken log is as actionable when verifying as when emitting.
    pub log: Result<&'a [decision::Decision], &'a str>,
}

/// One line of `.req/signing`: a signature is required from a key holding this party and role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyLine {
    pub party: String,
    pub role: String,
}

/// A signature file `<manifest>.<key-id>.minisig` beside the manifest (ADR 0042).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyedSignature {
    pub key_id: String,
    /// The signature text, or why the file could not be read.
    pub signature: Result<String, String>,
}

/// The party signatures and the signing policy (ADR 0042), beside [`Verification`].
#[derive(Clone, Copy)]
pub struct Parties<'a> {
    pub keyed: &'a [KeyedSignature],
    pub policy: &'a [PolicyLine],
}

/// Where a repository declares who must sign (ADR 0042 decision 3).
pub const SIGNING_POLICY_PATH: &str = ".req/signing";

/// A party or role token, and a key id in a signature file name: `[A-Za-z0-9._-]+`.
pub fn is_signing_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Parse `.req/signing`: the grammar of the other `.req/` lists (`#` starts a comment, blank lines
/// are skipped), one `<party> <role>` per line. Every malformed line is named `<file>:<line>` in
/// one run. A line repeated *n* times asks for *n* signatures (decision 3).
pub fn parse_signing_policy(text: &str) -> Result<Vec<PolicyLine>, Vec<String>> {
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let content = raw.split('#').next().unwrap_or("");
        let tokens: Vec<&str> = content
            .split([' ', '\t'])
            .filter(|word| !word.is_empty())
            .collect();
        match tokens.as_slice() {
            [] => {}
            [party, role] if is_signing_token(party) && is_signing_token(role) => {
                lines.push(PolicyLine {
                    party: (*party).to_string(),
                    role: (*role).to_string(),
                });
            }
            _ => errors.push(format!(
                "{SIGNING_POLICY_PATH}:{number}: `{}` is not `<party> <role>` (two words, each \
                 `[A-Za-z0-9._-]+`)",
                tokens.join(" ")
            )),
        }
    }
    if errors.is_empty() {
        Ok(lines)
    } else {
        Err(errors)
    }
}

/// The trust anchor's verdict on `key_id` at `now`, as findings. Shared by the one-signature path
/// and every keyed signature, so a key is judged by one rule wherever its signature lies.
fn judge_trust(
    trust: &TrustFile,
    key_id: &str,
    now: &str,
    manifest: &Manifest,
    failed: &mut Vec<String>,
    open: &mut Vec<String>,
) {
    match trust.state(key_id, now) {
        TrustState::Trusted => {}
        // **A revocation is judged at a time this build must be able to interpret.** For a v2
        // manifest the judged time is today and for a v3 one it is the attested `signed_at`
        // (ADR 0016), so a key revoked at or before it was not trusted when the human signed —
        // a definite finding, symmetric with expiry at signing. An envelope **newer** than this
        // build's records a signing time this build does not validate (and the CLI falls back to
        // today for it), so the same revocation cannot be turned into a definite FAIL: the
        // version dispatch above already answered UNVERIFIABLE with the remedy of verifying under
        // a build that writes it, and a FAIL here would outrank it and misdiagnose the change.
        // Gating on `==` instead of `<=` would break the documented v2 answer, which is still
        // "judged at today".
        TrustState::Revoked(reason) => {
            if manifest.version <= BASELINE_VERSION {
                failed.push(format!("key {key_id} is {reason}"));
            } else {
                open.push(format!(
                    "key {key_id} is {reason} — a newer envelope (v{}) records a signing time this \
                     build cannot judge, so the revocation cannot be decided here; verify with a \
                     build that writes that envelope",
                    manifest.version
                ));
            }
        }
        // **A broken anchor is not a judgement about a time**, so no envelope version can make it
        // "cannot tell": it fails closed on every manifest this build reads, a newer one included.
        // Reporting it as UNVERIFIABLE would also hand out the wrong remedy — a build that writes
        // the newer envelope reads the same unreadable `baselines/trust.yml`.
        TrustState::RevokedAnchor(reason) => failed.push(format!(
            "key {key_id} is {reason} — the trust anchor itself cannot be read, so this is not a \
             question about the envelope's signing time"
        )),
        // A v3 manifest records its signing time, so expiry is judged *at that time* (ADR
        // 0016): a key that had already expired then was not trusted when the Baseline was
        // signed, which is a definite finding — symmetric with revocation at/before the signing
        // time. A v2 manifest has no trusted signing time, which is the older ADR 0010 answer.
        // `--now` may supply the judged time instead, so the reason names the time it actually
        // used rather than claiming it was `signed_at`.
        TrustState::Expired(reason) => {
            if judges_signing_time(manifest.version) {
                failed.push(format!(
                    "key {key_id} is {reason} — the judged time ({now}) is outside the key's \
                     validity window, so the Baseline was not signed under a trusted key \
                     (ADR 0016)"
                ));
            } else {
                open.push(format!(
                    "key {key_id} is {reason} — a Baseline is historical and no trusted signing \
                     time exists, so this cannot be decided"
                ));
            }
        }
        TrustState::Unknown => open.push(format!("key {key_id} is not in the trust file")),
    }
}

/// What checking one signature against its key found.
enum SignatureVerdict {
    /// The verifier accepted it. Whether the key was *trusted* is [`judge_trust`]'s question.
    Valid,
    /// The trust file holds no such key: [`judge_trust`] already said so.
    NoKey,
    /// The key is declared for another scheme than the verifier's; the text names the key.
    WrongScheme(String),
    Invalid(String),
    Unverifiable(String),
}

/// One signature, checked against its key's scheme and the manifest's canonical bytes.
fn check_signature(
    manifest: &Manifest,
    trust: &TrustFile,
    verifier: &dyn SignatureVerifier,
    key_id: &str,
    signature: &str,
) -> SignatureVerdict {
    let Some(key) = trust.key(key_id) else {
        return SignatureVerdict::NoKey;
    };
    if key.scheme != verifier.scheme() {
        return SignatureVerdict::WrongScheme(format!(
            "key {key_id} is declared for scheme {} but the verifier is {}",
            key.scheme,
            verifier.scheme()
        ));
    }
    match verifier.verify(
        canonical_bytes(manifest).as_bytes(),
        signature,
        &key.public_key,
    ) {
        SignatureOutcome::Valid => SignatureVerdict::Valid,
        SignatureOutcome::Invalid(why) => SignatureVerdict::Invalid(why),
        SignatureOutcome::Unverifiable(why) => SignatureVerdict::Unverifiable(why),
    }
}

/// Judge one signature: push what it found, and say whether it counts towards a policy line —
/// it verified, and its key is trusted at the judged time. `named` prefixes the key id to a
/// finding that would not otherwise say which key it concerns (the party path); the
/// one-signature path keeps its historical text.
fn judge_signature(
    manifest: &Manifest,
    checks: &Verification<'_>,
    key_id: &str,
    signature: &str,
    named: bool,
    failed: &mut Vec<String>,
    open: &mut Vec<String>,
) -> bool {
    let prefix = if named {
        format!("key {key_id}: ")
    } else {
        String::new()
    };
    match check_signature(manifest, checks.trust, checks.verifier, key_id, signature) {
        SignatureVerdict::Valid => {
            matches!(checks.trust.state(key_id, checks.now), TrustState::Trusted)
        }
        SignatureVerdict::NoKey => false,
        SignatureVerdict::WrongScheme(why) => {
            open.push(why);
            false
        }
        SignatureVerdict::Invalid(why) => {
            failed.push(format!("{prefix}the signature does not verify: {why}"));
            false
        }
        SignatureVerdict::Unverifiable(why) => {
            open.push(format!("{prefix}{why}"));
            false
        }
    }
}

/// How a signing key is named in a header or a finding: `key <id> (<party> <role>)`, or
/// `key <id>` when the key carries no party and role.
pub fn key_label(trust: &TrustFile, key_id: &str) -> String {
    match trust.key(key_id) {
        Some(TrustedKey {
            party: Some(party),
            role: Some(role),
            ..
        }) => format!("key {key_id} ({party} {role})"),
        _ => format!("key {key_id}"),
    }
}

/// [`verify`] with ADR 0042's party signatures and signing policy.
///
/// Drift, the rules version and the authority are judged exactly as in [`verify`]. Then every
/// present signature is judged under today's per-key rules (a signature that does not verify, or
/// a key revoked at the signing time, is FAIL; a key the trust file does not hold is
/// UNVERIFIABLE), and then the policy: each `<party> <role>` line must be met by a distinct
/// verified key holding that party and role (one key counts towards one line only), and an unmet
/// line is UNVERIFIABLE — `awaiting <party> <role>`, never a pass. With no policy and no keyed
/// signature file the answer is [`verify`]'s, byte for byte.
pub fn verify_parties(
    rows: &[Row],
    manifest: &Manifest,
    checks: &Verification<'_>,
    parties: &Parties<'_>,
) -> Outcome {
    verify_inner(rows, manifest, checks, parties)
}

/// Verify a manifest against the working tree and the checks in [`Verification`].
pub fn verify(rows: &[Row], manifest: &Manifest, checks: &Verification<'_>) -> Outcome {
    verify_inner(
        rows,
        manifest,
        checks,
        &Parties {
            keyed: &[],
            policy: &[],
        },
    )
}

fn verify_inner(
    rows: &[Row],
    manifest: &Manifest,
    checks: &Verification<'_>,
    parties: &Parties<'_>,
) -> Outcome {
    let Verification {
        signature,
        trust,
        key_id,
        verifier: _,
        now,
        log,
    } = *checks;
    let mut failed: Vec<String> = Vec::new();
    let mut open: Vec<String> = Vec::new();

    // The direction of the mismatch decides the advice: a *stale* envelope is exactly the case
    // ADR 0013's re-issue is for, but a manifest written by a *newer* build must not be told to
    // re-sign with this binary — that would downgrade the attested surface instead of upgrading
    // the reader.
    if manifest.version < OLDEST_VERIFIABLE_BASELINE_VERSION {
        open.push(format!(
            "manifest version {} predates the oldest version this build verifies \
             ({OLDEST_VERIFIABLE_BASELINE_VERSION}, v{BASELINE_VERSION} is what it writes) — \
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

    // **v3 attests the authority it was signed under (ADR 0016); v4 keeps that attestation.**
    // Re-derive the head of the
    // **prefix** the manifest names: `authority.entries` says which record its head points at, so
    // decisions recorded *after* the signature are ordinary history growth (ADR 0005) and must
    // not invalidate a Baseline the human signed before them. The comparison is that record's
    // stored hash against the attested one. A disagreement is FAIL — the same class as a
    // hand-edited digest; a log too short to hold the attested record is FAIL too (what the human
    // signed over is not there); a log that cannot be read is UNVERIFIABLE. `verify_chain`
    // distinguishes "the content moved" from "there is nothing to judge", and the three answers
    // must not be flattened into one.
    if judges_signing_time(manifest.version) {
        match (&manifest.signed_at, &manifest.authority) {
            (Some(_), Some(authority)) => match log {
                Err(why) => open.push(format!(
                    "the decision log cannot be read, so the authority this manifest attests \
                     cannot be re-derived: {why}"
                )),
                Ok(log) => {
                    // Judge the **attested prefix**, not the log's present end: an unfilled or
                    // hand-edited decision recorded *after* `entries` is `req decision --verify`'s
                    // finding, not a reason to demote or fail a Baseline the human already
                    // signed. (The prefix's existence and head are checked below.)
                    let attested = &log[..authority.entries.min(log.len())];
                    match decision::verify_chain(attested) {
                    decision::Chain::Tampered(reasons) => failed.push(format!(
                        "the authority this manifest attests does not verify: {} — the decision \
                         log was edited after it was recorded (ADR 0016)",
                        reasons.join("; ")
                    )),
                    decision::Chain::Unverifiable(reasons) => open.push(format!(
                        "the authority this manifest attests cannot be re-derived: {}",
                        reasons.join("; ")
                    )),
                    decision::Chain::Valid => {
                        // **A head is a commitment to a prefix, not to the log's present end.**
                        // The manifest records `entries`, which says *which* record its head
                        // names; verification checks that record carries the attested hash and
                        // that the chain through it verifies. Decisions recorded afterwards are
                        // ordinary history growth (ADR 0005) and must not invalidate a Baseline
                        // the human signed before them — equality with the current head would
                        // do exactly that, on every `req decision --fill`.
                        match authority
                            .entries
                            .checked_sub(1)
                            .and_then(|position| log.get(position))
                        {
                            None => failed.push(format!(
                                "authority mismatch: the manifest attests {} decision entr{} but \
                                 the log holds {} — the attested prefix is not there (ADR 0016)",
                                authority.entries,
                                if authority.entries == 1 { "y" } else { "ies" },
                                log.len()
                            )),
                            Some(record) => match record.hash.as_deref() {
                                Some(stored) if stored == authority.head => {}
                                Some(stored) => failed.push(format!(
                                    "authority mismatch: decision {} carries {} but the manifest \
                                     attests {} (ADR 0016)",
                                    authority.entries,
                                    short_hash(stored),
                                    short_hash(&authority.head)
                                )),
                                None => open.push(format!(
                                    "the decision at position {} carries no chain hash, so the \
                                     attested head cannot be located (ADR 0016)",
                                    authority.entries
                                )),
                            },
                        }
                    }
                    }
                }
            },
            _ => open.push(format!(
                "manifest declares version {} but does not attest signed_at and authority — a v3 \
                 or v4 envelope carries both; re-issue with `req baseline --at <ts>` and re-sign",
                manifest.version
            )),
        }
    }

    // The one-signature path (no policy, no keyed file) is today's, text for text. A repository
    // that declares a policy or carries a keyed signature file is on the party path (ADR 0042):
    // the default `.minisig` then counts only when it is there, so a key that signed by file name
    // alone is not reported missing for want of the default key's signature.
    let party_path = !parties.keyed.is_empty() || !parties.policy.is_empty();
    if !party_path || signature.is_some() {
        judge_trust(trust, key_id, now, manifest, &mut failed, &mut open);
    }

    // A newer envelope's signed bytes are not reconstructable by this build: its canonical shape
    // may carry fields this code has never seen, and checking a signature over a guessed shape
    // would FAIL a manifest the build cannot read. The version dispatch above already answered
    // UNVERIFIABLE with the "verify with a build that writes it" remedy, so the signature is left
    // unjudged rather than misjudged.
    if manifest.version <= BASELINE_VERSION {
        // The keys whose signature verified under a trusted key: the set a policy line is met from.
        let mut verified: BTreeSet<String> = BTreeSet::new();
        if let Some(signature) = signature {
            if judge_signature(
                manifest,
                checks,
                key_id,
                signature,
                party_path,
                &mut failed,
                &mut open,
            ) {
                verified.insert(key_id.to_string());
            }
        } else if !party_path {
            open.push(
                "no signature: this is a manifest, not yet a Baseline (nothing is ratified until \
                 a human signs it)"
                    .to_string(),
            );
        }
        if party_path {
            for keyed in parties.keyed {
                let id = keyed.key_id.as_str();
                match &keyed.signature {
                    Err(why) => open.push(format!("key {id}: {why}")),
                    Ok(text) => {
                        judge_trust(trust, id, now, manifest, &mut failed, &mut open);
                        if judge_signature(manifest, checks, id, text, true, &mut failed, &mut open)
                        {
                            verified.insert(id.to_string());
                        }
                    }
                }
            }
            let present = signature.is_some() || !parties.keyed.is_empty();
            if parties.policy.is_empty() {
                if !present {
                    open.push(
                        "no signature: this is a manifest, not yet a Baseline (nothing is \
                         ratified until a human signs it)"
                            .to_string(),
                    );
                }
            } else {
                // One key counts towards one line only (TUF's rule): a key is assigned to the
                // first line with room that its party and role match, and then it is spent.
                // "One key" is one piece of key MATERIAL, not one id: two trust entries holding the
                // same public key under different ids verify the same signing act, and must not
                // meet two lines (review A-1-review-1). A key is spent by its material.
                let mut spent: BTreeSet<String> = BTreeSet::new();
                for line in parties.policy {
                    let holder = verified.iter().find_map(|id| {
                        let key = trust.key(id)?;
                        let material = key_material(&key.public_key);
                        (!spent.contains(&material)
                            && key.party.as_deref() == Some(line.party.as_str())
                            && key.role.as_deref() == Some(line.role.as_str()))
                        .then_some(material)
                    });
                    match holder {
                        Some(material) => {
                            spent.insert(material);
                        }
                        None => open.push(format!(
                            "awaiting {} {}: no verified signature from a distinct key holding \
                             that party and role",
                            line.party, line.role
                        )),
                    }
                }
            }
        }
    }

    // **The rules version is judged before drift, and the comparison it invalidates is not
    // merely inconclusive.** A claim's address is *derived from* the canonical rules version, so
    // when the manifest's `rules_v` disagrees with this build's `RULES_V` the drift comparison
    // has no common basis: any per-claim verdict it produced would be an artefact of comparing
    // two different rule sets, and blaming each claim for "being edited after signing" sends an
    // auditor hunting for a tampering that never happened.
    //
    // That is why this *returns* instead of pushing its reason onto `open` and carrying on: a
    // real rules bump also makes `drift` report every claim as `changed`, those land in
    // `failed`, and `Failed` would outrank this `Unverifiable` — exit 1, with the misdiagnosis
    // printed underneath. Drift is **void** here, not open, and [`RulesMismatch::reason`] says
    // so in those words because "we could not tell" and "the comparison does not apply" are
    // different claims.
    //
    // Placed after the trust and signature checks deliberately: those judge the manifest's own
    // signed bytes, which do not depend on the rules version, so a definite finding among them
    // (a revoked key, a signature that does not verify) still outranks this open question
    // exactly as `req gate`'s policies do. The guard is a precedence change about *drift*, not
    // a blanket suppression of what verification found.
    if let Some(mismatch) = rules_mismatch(manifest) {
        if !failed.is_empty() {
            return Outcome::Failed(failed);
        }
        open.push(mismatch.reason());
        return Outcome::Unverifiable(open);
    }

    let drift = drift(rows, manifest);
    if !drift.is_clean() {
        failed.extend(drift.reasons());
    }

    if !failed.is_empty() {
        Outcome::Failed(failed)
    } else if !open.is_empty() {
        Outcome::Unverifiable(open)
    } else {
        Outcome::Verified
    }
}

/// Write `bytes` to `target` only once they are complete: a temp file beside it, then a rename.
/// A shell redirect (`> file`) truncates the target before `req` has produced anything, so a
/// refusal leaves a 0-byte manifest that `minisign -Sm` will sign; this never does. The temp name is
/// created exclusively and removed on failure.
fn write_manifest_atomically(target: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let directory = match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => std::path::Path::new("."),
    };
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("the path names no file"))?
        .to_string_lossy()
        .into_owned();
    // The name carries the clock as well as the pid: a run killed between `open` and `rename`
    // leaves its staging file behind, and a recycled pid (or PID 1 in a container) must not make
    // every later run collide with it.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let staged = directory.join(format!(
        ".{name}.req-staging-{}-{nanos}",
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let finished = written.and_then(|()| std::fs::rename(&staged, target));
    if finished.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    finished
}

/// `req baseline --at <ts> [--verify <file>] [--signature <file>] [--key <id>] [--now <date>] [k=v ...]`
///
/// With no `--verify`, prints the canonical manifest to stdout — the bytes a human signs. The
/// signing time is **authored** (`--at`), never a clock req invents; emitting without one is a
/// usage error (ADR 0016).
/// Exit: **0** verified (or emitted), **1** FAIL, **3** UNVERIFIABLE, **2** usage.
pub fn run(args: &[String], root: &std::path::Path) -> i32 {
    let mut context = Context::new();
    let mut verify_path: Option<String> = None;
    let mut signature_path: Option<String> = None;
    let mut key_id = "req-baseline-1".to_string();
    let mut now = today_utc();
    let mut explicit_now = false;
    let mut at_value: Option<String> = None;
    let mut out_path: Option<String> = None;
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
                Some(value) => {
                    now = value;
                    explicit_now = true;
                }
                None => return 2,
            },
            "--at" => match take("--at") {
                Some(value) => at_value = Some(value),
                None => return 2,
            },
            "--out" => match take("--out") {
                Some(value) if !value.is_empty() && !value.starts_with('-') => {
                    out_path = Some(value);
                }
                Some(value) => {
                    eprintln!("--out needs a file path, not {value:?}");
                    return 2;
                }
                None => return 2,
            },
            // An option-shaped argument is a usage error, never a selection pair: `--out=file` would
            // otherwise be read as the selection `--out == file`, print the manifest instead of
            // writing it, and carry a bogus slot in the bytes a human signs, with exit 0.
            other if other.starts_with('-') => {
                eprintln!(
                    "unrecognized option: {other} (flags take their value as the next argument)"
                );
                return 2;
            }
            other => match other.split_once('=') {
                Some((key, value)) if !key.is_empty() => {
                    context.insert(key.to_string(), value.to_string());
                }
                _ => {
                    eprintln!(
                        "usage: req baseline --at <RFC 3339 UTC timestamp> [--out <file>] \
                         [--verify <file>] [--signature <file>] [--key <id>] [--now <date>] \
                         [key=value ...]"
                    );
                    return 2;
                }
            },
        }
    }

    // `--now` is the explicit judgement-time override, so it must be a time: an unvalidated
    // string would compare below every trust date and read a revoked key as trusted (a fail-open
    // on the one flag meant to be diagnostic). A bare date is accepted for compatibility with
    // the pre-v3 override; a full RFC 3339 UTC timestamp is reduced to its date by `state`.
    if explicit_now && !valid_judgement_time(&now) {
        eprintln!(
            "--now must be a date (YYYY-MM-DD) or an RFC 3339 UTC timestamp, e.g. \
             2026-10-20 or 2026-10-20T18:30:00Z: {now}"
        );
        return 2;
    }

    let Some(path) = verify_path else {
        // The signing act's time is the signer's assertion: req never invents one (ADR 0016), so
        // an emission without `--at` is a usage error, not a defaulted field.
        let Some(at) = at_value else {
            eprintln!(
                "req baseline needs --at <RFC 3339 UTC timestamp>, e.g. \
                 --at 2026-10-20T18:30:00Z: req never invents a signing time (ADR 0016)"
            );
            return 2;
        };
        if explicit_now {
            eprintln!("--now is a verification override; emission takes its time from --at");
            return 2;
        }
        if !valid_signing_time(&at) {
            eprintln!(
                "--at must be an RFC 3339 UTC timestamp with a `Z` offset, e.g. \
                 2026-10-20T18:30:00Z: {at}"
            );
            return 2;
        }
        let rows = match crate::view::load_rows(&context, root) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] {e}");
                return 3;
            }
        };
        // A head that attests nothing must not be signed: the manifest would claim an authority
        // the log cannot supply. Absent, unparsable and tampered logs are all refusals here,
        // with the reason the chain gives.
        let log = match read_decisions(root) {
            Ok(log) => log,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] refusing to emit: {e}");
                return 3;
            }
        };
        match decision::verify_chain(&log) {
            decision::Chain::Valid => {}
            decision::Chain::Tampered(reasons) => {
                // A stored hash that no longer recomputes is a definite finding (the chain's own
                // `exit_code` maps Tampered to 1), not an inconclusive environment problem.
                eprintln!(
                    "[FAIL] refusing to emit: the decision log does not verify, and a manifest \
                     that attests a broken chain must not be signed — {}",
                    reasons.join("; ")
                );
                return 1;
            }
            decision::Chain::Unverifiable(reasons) => {
                eprintln!(
                    "[UNVERIFIABLE] refusing to emit: the decision log cannot be judged, and a \
                     head that attests nothing must not be signed — {}",
                    reasons.join("; ")
                );
                return 3;
            }
        }
        let head = decision::head(&log)
            .expect("a VALID decision chain is non-empty and ends in a filled hash");
        let mut manifest = manifest(&rows, crate::canon::RULES_V, &context);
        // `--at` was validated above, so this cannot fail on this path; it is handled rather
        // than unwrapped because a panic here would be a traceback instead of a verdict, and
        // the CLI's validation is not `attest`'s contract (a library caller gets the same
        // refusal, not a manifest that `from_json` would reject).
        if let Err(e) = attest(&mut manifest, &at, &head) {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
        let bytes = canonical_bytes(&manifest);
        let Some(out) = out_path else {
            print!("{bytes}");
            return 0;
        };
        return match write_manifest_atomically(std::path::Path::new(&out), bytes.as_bytes()) {
            Ok(()) => {
                eprintln!(
                    "wrote {out} ({} bytes): unsigned, sign it with `minisign -Sm {out}`",
                    bytes.len()
                );
                0
            }
            Err(e) => {
                eprintln!("[UNVERIFIABLE] cannot write {out}: {e}; nothing was written");
                3
            }
        };
    };

    if out_path.is_some() {
        eprintln!(
            "--out is an emission flag: it writes the manifest a human signs, and a verification \
             writes nothing"
        );
        return 2;
    }

    if at_value.is_some() {
        eprintln!(
            "--at is an emission flag: a Baseline's signing time is carried in the manifest \
             (`signed_at`), and verification judges the key at it"
        );
        return 2;
    }

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
    let rows = match crate::view::load_rows(&effective, root) {
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
    // The Decision log is read even for a v2 manifest — cheap, and `verify` ignores it there.
    // An unreadable log becomes `None`, which a v3 manifest reports as UNVERIFIABLE rather than
    // FAIL (absence is not tampering).
    let decisions = read_decisions(root);
    let log = decisions
        .as_ref()
        .map(|log| log.as_slice())
        .map_err(|error| error.as_str());
    // The time the **key** is judged at (ADR 0016): the signer's `signed_at` for a v3 Baseline,
    // today for a v2 one, and `--now` as the explicit override in either case.
    let judged_at = if explicit_now {
        now
    } else {
        // Only the envelope whose `signed_at` this build **validates** may supply the judgement
        // time. A newer manifest's field is not this build's to interpret (and `from_json`
        // deliberately does not validate it), so its trust verdict stays on today's clock — the
        // version dispatch already answers UNVERIFIABLE for it.
        match manifest.signed_at.as_deref() {
            Some(signed_at) if judges_signing_time(manifest.version) => signed_at.to_string(),
            _ => today_utc(),
        }
    };

    // The signing policy and the keyed signature files beside the manifest (ADR 0042). A
    // repository with neither takes the one-signature path, unchanged.
    let policy = match read_signing_policy(root) {
        Ok(policy) => policy,
        Err(PolicyRefusal::Usage(errors)) => {
            for error in errors {
                eprintln!("{error}");
            }
            return 2;
        }
        Err(PolicyRefusal::Unreadable(why)) => {
            eprintln!("[UNVERIFIABLE] {why}");
            return 3;
        }
    };
    let keyed = keyed_signatures(Path::new(&path));
    let outcome = verify_parties(
        &rows,
        &manifest,
        &Verification {
            signature: signature.as_deref(),
            trust: &trust,
            key_id: &key_id,
            verifier: &MinisignCli,
            now: &judged_at,
            log,
        },
        &Parties {
            keyed: &keyed,
            policy: &policy,
        },
    );
    // The header names every signature judged. On the one-signature path that is the `--key` key
    // alone, as before; with keyed files or a policy it is each signature present.
    let signers = if keyed.is_empty() && policy.is_empty() {
        key_label(&trust, &key_id)
    } else {
        let mut labels: Vec<String> = Vec::new();
        if signature.is_some() {
            labels.push(key_label(&trust, &key_id));
        }
        labels.extend(keyed.iter().map(|k| key_label(&trust, &k.key_id)));
        if labels.is_empty() {
            "unsigned".to_string()
        } else {
            labels.join(", ")
        }
    };
    match manifest.signed_at.as_deref() {
        Some(signed_at) => println!(
            "# Baseline {path}: {} claim(s), rules v{}, {signers}, signed at {signed_at}, \
             verified at {judged_at}",
            manifest.claims.len(),
            manifest.rules_v
        ),
        None => println!(
            "# Baseline {path}: {} claim(s), rules v{}, {signers}, verified at {judged_at}",
            manifest.claims.len(),
            manifest.rules_v
        ),
    }
    // A v3 manifest is honoured, but it cannot attest what v4 added, and the reader is owed
    // that fact **once** rather than a per-claim silence that could read as coverage. This is a
    // disclosure, not a finding: the drifted addresses and audit surface are still judged, and
    // the verdict is not rewritten by it (ADR 0019).
    // "Predates what this build writes" is a relationship, not the coincidence that the oldest
    // supported envelope happens to be one below the write version: key it off both constants so
    // a future bump cannot tell a freshly written manifest that it predates itself.
    if (OLDEST_VERIFIABLE_BASELINE_VERSION..TESTS_ATTESTED_FROM).contains(&manifest.version) {
        println!(
            "  - predates envelope v{BASELINE_VERSION}: the authored test links are NOT attested \
             by this manifest (ADR 0019) — re-issue and re-sign to attest them"
        );
    }
    println!("[{}]", outcome.as_str());
    for reason in outcome.reasons() {
        println!("  - {reason}");
    }
    outcome.exit_code()
}

enum PolicyRefusal {
    Usage(Vec<String>),
    Unreadable(String),
}

/// `.req/signing`, or no policy when the file is absent. A file that exists and cannot be read is
/// not absent: a skipped policy would let a Baseline verify without the signatures it asked for.
fn read_signing_policy(root: &Path) -> Result<Vec<PolicyLine>, PolicyRefusal> {
    let path = root.join(SIGNING_POLICY_PATH);
    match std::fs::read(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(PolicyRefusal::Unreadable(format!(
            "cannot read {SIGNING_POLICY_PATH}: {error}"
        ))),
        Ok(bytes) => match String::from_utf8(bytes) {
            Err(_) => Err(PolicyRefusal::Unreadable(format!(
                "{SIGNING_POLICY_PATH} is not valid UTF-8 text"
            ))),
            Ok(text) => parse_signing_policy(&text).map_err(PolicyRefusal::Usage),
        },
    }
}

/// The key itself, without minisign's `untrusted comment:` line or surrounding whitespace: two trust
/// entries whose material is equal are the same key, whatever their ids or comments say.
fn key_material(public_key: &str) -> String {
    public_key
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `<manifest>.<key-id>.minisig` beside `manifest`, in key-id order. The default
/// `<manifest>.minisig` is not among them (it is the `--key` key's signature). A file whose id is
/// not a signing token is kept, as an unreadable signature that names it: never ignored.
pub(crate) fn keyed_signatures(manifest: &Path) -> Vec<KeyedSignature> {
    let Some(name) = manifest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    let directory = match manifest.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let prefix = format!("{name}.");
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let id = file.strip_prefix(&prefix)?.strip_suffix(".minisig")?;
            // A signature is a regular file: a directory of that name must not make a manifest
            // count as signed (review A-1-review-1).
            entry
                .path()
                .is_file()
                .then(|| (id.to_string(), entry.path()))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .map(|(key_id, file)| {
            let signature = if is_signing_token(&key_id) {
                std::fs::read_to_string(&file)
                    .map_err(|error| format!("cannot read {}: {error}", file.display()))
            } else {
                Err(format!(
                    "{} names a key id that is not `[A-Za-z0-9._-]+`",
                    file.display()
                ))
            };
            KeyedSignature { key_id, signature }
        })
        .collect()
}

/// The Decision log as `decisions/` holds it, in chain order. One loader, so emission and
/// verification attest and re-derive the same way.
fn read_decisions(root: &Path) -> Result<Vec<decision::Decision>, String> {
    let paths = decision::log_paths(root)?;
    let log = decision::read_log(&paths)?;
    Ok(log.into_iter().map(|(_, decision)| decision).collect())
}

/// The first characters of a digest, for a reason a human reads. Never decides anything: the
/// full values are compared, and only the message is shortened.
fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// The date part of a date-or-timestamp: `YYYY-MM-DD`, the prefix both spellings share. A trust
/// entry and a judgement time are compared through this, so neither a clock nor a timestamp-
/// valued anchor can make a same-day comparison wrong.
fn date_of(value: &str) -> &str {
    value.get(..10).unwrap_or(value)
}

/// A real calendar date. `1..=31` per month is not enough: a signed `2026-02-31` would be used
/// as a judgement date and cannot be corrected without re-signing.
fn valid_calendar_date(year: u32, month: u32, day: u32) -> bool {
    if year < 1970 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let last_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=last_day).contains(&day)
}

/// A `--now` value: a bare ISO date, or the full RFC 3339 UTC timestamp `--at` requires.
pub(crate) fn valid_judgement_time(value: &str) -> bool {
    if value.contains('T') {
        return valid_signing_time(value);
    }
    let b = value.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let is_digits = |bytes: &[u8]| bytes.iter().all(u8::is_ascii_digit);
    if !is_digits(&b[0..4]) || !is_digits(&b[5..7]) || !is_digits(&b[8..10]) {
        return false;
    }
    let number = |field: &[u8]| {
        std::str::from_utf8(field)
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
    };
    match (number(&b[0..4]), number(&b[5..7]), number(&b[8..10])) {
        (Some(year), Some(month), Some(day)) => valid_calendar_date(year, month, day),
        _ => false,
    }
}

/// An RFC 3339 UTC timestamp with a `Z` offset — the only shape `--at` accepts. req never
/// invents a signing time, so a value it cannot read is a usage error rather than a defaulted
/// field (ADR 0016). Fractional seconds are accepted (and preserved verbatim, because they are
/// inside the signed bytes); a leap second is not.
fn valid_signing_time(value: &str) -> bool {
    let Some(body) = value.strip_suffix('Z') else {
        return false;
    };
    let (core, fraction) = match body.split_once('.') {
        Some((core, fraction)) => (core, Some(fraction)),
        None => (body, None),
    };
    let b = core.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return false;
    }
    let is_digits = |bytes: &[u8]| bytes.iter().all(u8::is_ascii_digit);
    let fields = [
        &b[0..4],
        &b[5..7],
        &b[8..10],
        &b[11..13],
        &b[14..16],
        &b[17..19],
    ];
    if !fields.iter().all(|field| is_digits(field)) {
        return false;
    }
    if let Some(fraction) = fraction
        && (fraction.is_empty() || fraction.len() > 9 || !is_digits(fraction.as_bytes()))
    {
        return false;
    }
    let number = |field: &[u8]| {
        std::str::from_utf8(field)
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
    };
    match (
        number(&b[0..4]),
        number(&b[5..7]),
        number(&b[8..10]),
        number(&b[11..13]),
        number(&b[14..16]),
        number(&b[17..19]),
    ) {
        (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) => {
            valid_calendar_date(year, month, day) && hour < 24 && minute < 60 && second < 60
        }
        _ => false,
    }
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
/// no float. It is the judgement time for a **v2** Baseline, which carries no trusted signing
/// time (ADR 0010); a v3 one is judged at its manifest's `signed_at` (ADR 0016).
fn today_utc() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = seconds / 86_400;
    civil_from_days(i64::try_from(days).expect("days since the epoch divided by 86_400 fit an i64"))
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
            Err(BaseRefusal::Unsafe(error)) => error,
            Err(BaseRefusal::Io(error)) => {
                panic!("the refusal must be the chain's, not an I/O error: {error}")
            }
        };
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `stat` failure that is not `NotFound` is the filesystem's answer, not a refusal, and it
    /// has to stay `Io` or the fallback would quietly move to the next candidate. A symlink loop
    /// produces one without needing a second uid.
    #[cfg(unix)]
    #[test]
    fn a_stat_failure_is_an_io_error_not_a_refusal() {
        let outer = unique_base("req-symlink-loop");
        std::fs::create_dir_all(&outer).expect("the fixture outer directory");
        let looped = outer.join("loop");
        std::os::unix::fs::symlink(&looped, &looped).expect("the loop symlink");

        match require_safe_base(&looped.join("staging"), None) {
            Err(BaseRefusal::Io(error)) => {
                assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
            }
            Err(BaseRefusal::Unsafe(_)) => panic!("a stat failure is not a refusal"),
            Ok(()) => panic!("a looping path cannot be validated"),
        }

        let _ = std::fs::remove_dir_all(&outer);
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

    /// The fallback acts on the reason, not on the error kind: a chain refusal falls through to
    /// the next candidate *whichever pass produced it* — the ownership pass reports the same
    /// variant, which is the case this pins without needing a second uid — while an ordinary I/O
    /// failure is that base's answer, and the first refusal is the one an operator is told.
    #[test]
    fn a_chain_refusal_falls_through_but_an_io_error_does_not() {
        fn refusal() -> BaseRefusal {
            BaseRefusal::Unsafe(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "refused",
            ))
        }
        fn io_failure() -> BaseRefusal {
            BaseRefusal::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "eacces",
            ))
        }

        assert_eq!(
            first_usable([Err(refusal()), Ok(7)]).expect("the second candidate is used"),
            7
        );

        let error =
            first_usable::<u8>([Err(io_failure()), Ok(7)]).expect_err("an I/O error is reported");
        assert_eq!(error.to_string(), "eacces");

        let error = first_usable::<u8>([Err(refusal()), Err(refusal())])
            .expect_err("every candidate was refused");
        assert_eq!(error.to_string(), "refused");
    }
}
