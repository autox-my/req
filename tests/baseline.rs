//! The Baseline: canonical bytes, drift, and the three-state verdict.

mod common;

use req::baseline::{
    BASELINE_VERSION, Manifest, MinisignCli, Outcome, SignatureOutcome, SignatureVerifier,
    TrustFile, canonical_bytes, drift, from_json, manifest, verify,
};
use req::model::{Rationale, Relation, rationale_digest, relations_digest};
use req::select::{Applies, Context};
use req::view::Row;

/// Pinned oracles, computed independently of the code under test and named once so the pinned
/// bytes below cannot carry a stale hand-copy. Asserting a digest against itself recomputed
/// would pin nothing, because it could not fail; never regenerate these from the implementation.
///
/// `printf 'req.relations.v1\0[]' | shasum -a 256` (the `\0` is a literal NUL byte):
const EMPTY_RELATIONS_DIGEST: &str =
    "479fcec2cd9b2af0bc13fa4054307413459ae1ad7ca6344435530570226dc1af";
/// `printf 'req.rationale.v1\0{"source":"docs/adr/0010-baseline-signing-and-trust.md","date":"2026-10-07","rejected_alternatives":[],"assumptions":[]}' | shasum -a 256` — the fixture rationale every row in this file carries:
const FIXTURE_RATIONALE_DIGEST: &str =
    "bd57ad973193c6a0537045b60db5890f854b15616391b6ffe0e661c5709185c5";

fn row(id: &str, address: &str, applies: Applies) -> Row {
    // One idiom with every other row fixture (`common::rationale_columns`): the displayed
    // provenance and its attested digest are derived together. The pinned canonical bytes below
    // were computed over exactly this rationale.
    let provenance = common::rationale_columns(
        "docs/adr/0010-baseline-signing-and-trust.md",
        common::FIXTURE_DATE,
    );
    // One idiom with every other row fixture (`common::relation_columns`): the columns and
    // their digest are derived together, so the empty case is a single expression, not two
    // that must be kept in step.
    let (relations, relations_hash) = common::relation_columns(&[]);
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: address.to_string(),
        applies,
        selectors: Vec::new(),
        relations,
        relations_hash,
        rationale_hash: provenance.hash,
        stored: req::fill::StoredState::Verified,
        rationale_source: provenance.source,
        rationale_date: provenance.date,
    }
}

fn trust(revoked: bool, valid_until: Option<&str>) -> TrustFile {
    let revoke_line = if revoked {
        "    revoked_at: \"2026-10-01\"\n"
    } else {
        ""
    };
    let until_line = match valid_until {
        Some(date) => format!("    valid_until: \"{date}\"\n"),
        None => String::new(),
    };
    TrustFile::load(&format!(
        "keys:\n  - id: \"req-baseline-1\"\n    scheme: \"minisign\"\n    public_key: \"RWQf\"\n    valid_from: \"2026-01-01\"\n{until_line}{revoke_line}"
    ))
    .expect("trust file parses")
}

/// Answers exactly what the test says it should — an oracle for the *combination* logic, and
/// no substitute for the real verifier (which this environment does not have).
struct Stub(SignatureOutcome);

impl SignatureVerifier for Stub {
    fn scheme(&self) -> &'static str {
        "minisign"
    }
    fn verify(&self, _message: &[u8], _signature: &str, _public_key: &str) -> SignatureOutcome {
        self.0.clone()
    }
}

fn signed_manifest() -> (Vec<Row>, Manifest) {
    let mut context = Context::new();
    context.insert("deployment_mode".to_string(), "air_gapped".to_string());
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes),
        row("REQ-B", "bbb", Applies::Yes),
        row("REQ-C", "ccc", Applies::No),
    ];
    let manifest = manifest(&rows, 5, &context);
    (rows, manifest)
}

#[test]
fn canonical_bytes_do_not_depend_on_input_order() {
    let mut context = Context::new();
    context.insert("b".to_string(), "2".to_string());
    context.insert("a".to_string(), "1".to_string());
    let forward = manifest(
        &[
            row("REQ-A", "aaa", Applies::Yes),
            row("REQ-B", "bbb", Applies::Yes),
        ],
        5,
        &context,
    );
    let backward = manifest(
        &[
            row("REQ-B", "bbb", Applies::Yes),
            row("REQ-A", "aaa", Applies::Yes),
        ],
        5,
        &context,
    );
    assert_eq!(canonical_bytes(&forward), canonical_bytes(&backward));
    // Selection keys sorted, non-applying claims absent, one trailing newline, and the
    // fixture's real digests in the bytes — both interpolated from the named oracles at the top
    // of this file, so the pin has one home rather than a hand-copy per claim.
    assert_eq!(
        canonical_bytes(&forward),
        format!(
            "{{\"version\":2,\"rules_v\":5,\"selection\":{{\"a\":\"1\",\"b\":\"2\"}},\
             \"claims\":[{{\"claim_id\":\"REQ-A\",\"address\":\"aaa\",\
             \"relations_hash\":\"{EMPTY_RELATIONS_DIGEST}\",\
             \"rationale_hash\":\"{FIXTURE_RATIONALE_DIGEST}\"}},\
             {{\"claim_id\":\"REQ-B\",\"address\":\"bbb\",\
             \"relations_hash\":\"{EMPTY_RELATIONS_DIGEST}\",\
             \"rationale_hash\":\"{FIXTURE_RATIONALE_DIGEST}\"}}]}}\n"
        )
    );
}

#[test]
fn a_manifest_round_trips_through_its_own_bytes() {
    let (_, manifest) = signed_manifest();
    let parsed = from_json(&canonical_bytes(&manifest)).expect("parses its own output");
    assert_eq!(parsed, manifest);
    assert_eq!(parsed.version, BASELINE_VERSION);
}

/// The property a Baseline exists for: editing a claim after signing is detected, not
/// silently blessed.
#[test]
fn an_edited_claim_fails_verification() {
    let (rows, manifest) = signed_manifest();
    let edited: Vec<Row> = rows
        .iter()
        .map(|r| {
            if r.claim_id == "REQ-B" {
                row("REQ-B", "bbb-changed", Applies::Yes)
            } else {
                r.clone()
            }
        })
        .collect();
    let outcome = verify(
        &edited,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome.exit_code(), 1);
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("REQ-B"), "{reasons}");
    assert!(reasons.contains("no longer recomputes"), "{reasons}");
}

#[test]
fn a_claim_added_after_signing_fails_verification() {
    let (mut rows, manifest) = signed_manifest();
    rows.push(row("REQ-D", "ddd", Applies::Yes));
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome.exit_code(), 1);
    assert!(outcome.reasons().join(" ").contains("REQ-D"));
}

/// ADR 0013: identity stays meaning-only; the audit surface is attested *beside* the
/// address — in the struct, in the signed bytes, and through the round-trip.
#[test]
fn the_manifest_carries_the_attested_hashes() {
    let (rows, manifest) = signed_manifest();
    for claim in &manifest.claims {
        let row = rows
            .iter()
            .find(|r| r.claim_id == claim.claim_id)
            .expect("manifest claims come from rows");
        assert_eq!(
            claim.relations_hash, row.relations_hash,
            "{}",
            claim.claim_id
        );
        assert_eq!(
            claim.rationale_hash, row.rationale_hash,
            "{}",
            claim.claim_id
        );
    }
    let bytes = canonical_bytes(&manifest);
    assert!(bytes.contains("\"relations_hash\""), "{bytes}");
    assert!(bytes.contains("\"rationale_hash\""), "{bytes}");
    let parsed = from_json(&bytes).expect("parses its own output");
    assert_eq!(parsed, manifest);
}

/// ADR 0013: the audit surface is attested — a post-signing relation edit is detected even
/// when the meaning (and so the address) is untouched. The test simulates that edit by
/// changing the attested hash, because `load_rows` computes the hash *from* the content:
/// the edit happens in a claim file, the loader re-derives the hash from it, and a Row is
/// what arrives here — so overriding the hash is exactly the state an edited relation
/// produces at this seam. (The address, computed only from meaning, is untouched.)
#[test]
fn an_edited_relation_after_signing_fails_verification() {
    let (rows, manifest) = signed_manifest();
    let edited: Vec<Row> = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if r.claim_id == "REQ-B" {
                r.relations_hash = format!("{}-edited", r.relations_hash);
            }
            r
        })
        .collect();
    let outcome = verify(
        &edited,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome.exit_code(), 1);
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("REQ-B"), "{reasons}");
    assert!(reasons.contains("relations or rationale"), "{reasons}");
}

/// Same attestation, other surface: the rationale is provenance, and provenance is part of
/// what was signed.
#[test]
fn an_edited_rationale_after_signing_fails_verification() {
    let (rows, manifest) = signed_manifest();
    let edited: Vec<Row> = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if r.claim_id == "REQ-A" {
                r.rationale_hash = format!("{}-edited", r.rationale_hash);
            }
            r
        })
        .collect();
    let outcome = verify(
        &edited,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome.exit_code(), 1);
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("REQ-A"), "{reasons}");
    assert!(reasons.contains("relations or rationale"), "{reasons}");
}

/// A v1 manifest carries no attested hashes: it is stale, and the error must say what to do
/// about it rather than naming a serde field.
#[test]
fn a_v1_manifest_names_the_reissue() {
    let err = from_json(
        "{\"version\":1,\"rules_v\":5,\"selection\":{},\"claims\":[{\"claim_id\":\"REQ-A\",\"address\":\"aaa\"}]}",
    )
    .expect_err("a v1 manifest has no attested hashes");
    assert!(err.contains("re-issue"), "{err}");
    assert!(err.contains("ADR 0013"), "{err}");
}

/// Ordinary corruption — here a manifest missing `claim_id`, not the attested hashes —
/// must get the plain unreadable message. The v1 advice ("re-issue and re-sign", ADR 0013)
/// is for a manifest *without the attested hashes*; a corrupt manifest is not a version of
/// anything, and sending its reader off to re-sign would re-issue over a defect that was
/// never a stale envelope.
#[test]
fn a_manifest_missing_claim_id_gets_the_plain_unreadable_message() {
    let err = from_json(
        "{\"version\":2,\"rules_v\":5,\"selection\":{},\"claims\":[{\"address\":\"aaa\",\"relations_hash\":\"r\",\"rationale_hash\":\"q\"}]}",
    )
    .expect_err("a manifest without claim_id is unreadable");
    // Only the code-owned parts of the message are asserted: the serde detail is forwarded
    // verbatim but is serde's text, not a contract — the same reason the code does not branch
    // on it.
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(!err.contains("re-issue"), "{err}");
    assert!(!err.contains("ADR 0013"), "{err}");
}

/// A v2 manifest that lost exactly one attested hash — a bad merge dropping the
/// `rationale_hash` line while keeping `relations_hash` — is corrupt, not stale: it declares
/// the current version, so it gets the plain unreadable message rather than v1 re-issue advice.
/// A stale v1 is missing *both* hashes and serde names only the first, so the field name alone
/// cannot tell them apart; the declared envelope version can.
#[test]
fn a_v2_manifest_missing_one_attested_hash_is_corrupt_not_v1() {
    let err = from_json(
        "{\"version\":2,\"rules_v\":5,\"selection\":{},\"claims\":[{\"claim_id\":\"REQ-A\",\"address\":\"aaa\",\"relations_hash\":\"r\"}]}",
    )
    .expect_err("a v2 manifest missing rationale_hash is unreadable");
    // Code-owned parts only; the serde detail is forwarded, not a contract (see above).
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(!err.contains("re-issue"), "{err}");
    assert!(!err.contains("ADR 0013"), "{err}");
}

/// The lenient recovery that decides v1 advice fires only on a *declared* version 1. A manifest
/// with no version field at all is ordinary corruption with nothing to recover, so it gets the
/// plain unreadable message — pinned so a widened recovery cannot start advising on it.
#[test]
fn a_manifest_with_no_version_field_gets_the_plain_message() {
    let err = from_json("{\"rules_v\":5,\"selection\":{},\"claims\":[]}")
        .expect_err("a manifest without a version field is unreadable");
    // Code-owned parts only; the serde detail is forwarded, not a contract.
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(!err.contains("re-issue"), "{err}");
    assert!(!err.contains("ADR 0013"), "{err}");
}

/// The stale-envelope contract is *any declared version below this build's* — derived from
/// BASELINE_VERSION, not hardcoded to 1 — so the next envelope bump cannot silently turn a
/// current-version manifest missing the then-new fields into an unreadable-manifest message
/// with no advice. BASELINE_VERSION cannot move in a test, so the generalization is pinned with
/// a version-0 declaration: below this build's whatever else it is, and the stale branch fires
/// exactly as it would for a v2 manifest at a v3 bump.
#[test]
fn a_manifest_declaring_any_older_version_is_stale() {
    let err = from_json(
        "{\"version\":0,\"rules_v\":5,\"selection\":{},\"claims\":[{\"claim_id\":\"REQ-A\",\"address\":\"aaa\"}]}",
    )
    .expect_err("a manifest declaring a version below this build's is stale, not corrupt");
    assert!(err.contains("re-issue"), "{err}");
    assert!(err.contains("ADR 0013"), "{err}");
}

/// A manifest written by a *newer* build must not be told to re-issue and re-sign with this
/// (older) binary: that would downgrade the attested surface instead of upgrading the reader.
/// The re-issue advice is for a stale (older) envelope only.
#[test]
fn a_newer_manifest_is_not_told_to_downgrade() {
    let (rows, mut manifest) = signed_manifest();
    let verify_at = |manifest: &Manifest| {
        verify(
            &rows,
            manifest,
            Some("sig"),
            &trust(false, None),
            "req-baseline-1",
            &Stub(SignatureOutcome::Valid),
            "2026-10-07",
        )
    };
    manifest.version = BASELINE_VERSION + 1;
    let newer = verify_at(&manifest);
    assert_eq!(newer.exit_code(), 3, "{newer:?}");
    let reasons = newer.reasons().join(" ");
    assert!(reasons.contains("newer"), "{reasons}");
    assert!(!reasons.contains("re-issue"), "{reasons}");

    // ...and the stale direction keeps the advice: an older envelope is exactly the case
    // ADR 0013's re-issue is for.
    manifest.version = BASELINE_VERSION - 1;
    let stale = verify_at(&manifest);
    assert_eq!(stale.exit_code(), 3, "{stale:?}");
    assert!(stale.reasons().join(" ").contains("re-issue"));
}

/// The audit hashes are over the parsed values, canonically serialized: deterministic for
/// identical values, distinct across surfaces, moved by an edit, and defined for the empty
/// list — an empty `relations:` still attests.
#[test]
fn the_audit_hashes_are_deterministic_and_separate() {
    let relations = vec![Relation {
        kind: "derives-from".to_string(),
        to: "REQ-0007".to_string(),
    }];
    let rationale = Rationale {
        source: "docs/design.md".to_string(),
        date: "2026-10-07".to_string(),
        rejected_alternatives: vec!["a cloud provider".to_string()],
        assumptions: vec!["local models suffice".to_string()],
    };
    let a = relations_digest(&relations);
    assert_eq!(a, relations_digest(&relations), "same values, same hash");
    assert_ne!(
        a,
        rationale_digest(&rationale),
        "different surfaces, different hashes"
    );
    let edited = vec![Relation {
        kind: "derives-from".to_string(),
        to: "REQ-0008".to_string(),
    }];
    assert_ne!(a, relations_digest(&edited), "an edit must move the hash");
    assert_eq!(
        relations_digest(&[]),
        EMPTY_RELATIONS_DIGEST,
        "an empty list still attests — against the shasum oracle, not against itself"
    );
}

#[test]
fn a_verified_baseline_is_the_only_verified_outcome() {
    let (rows, manifest) = signed_manifest();
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome, Outcome::Verified);
    assert_eq!(outcome.exit_code(), 0);
    // ...and the same tree without a signature is never verified: a manifest is not a
    // Baseline until a human signs it.
    let unsigned = verify(
        &rows,
        &manifest,
        None,
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(unsigned.exit_code(), 3);
    assert!(unsigned.reasons().join(" ").contains("not yet a Baseline"));
}

#[test]
fn an_unknown_key_is_unverifiable_and_a_revoked_key_fails() {
    let (rows, manifest) = signed_manifest();
    let unknown = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "someone-elses-key",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(unknown.exit_code(), 3, "{unknown:?}");

    let revoked = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(true, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(revoked.exit_code(), 1, "{revoked:?}");
    assert!(revoked.reasons().join(" ").contains("revoked"));
}

/// Expiry is UNVERIFIABLE, not FAIL: the Baseline is historical and no trusted signing time
/// exists, so the question cannot be answered either way (ADR 0010).
#[test]
fn an_expired_key_is_unverifiable_rather_than_failed() {
    let (rows, manifest) = signed_manifest();
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, Some("2026-09-01")),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert!(
        outcome
            .reasons()
            .join(" ")
            .contains("no trusted signing time")
    );
}

#[test]
fn a_bad_signature_fails_and_a_missing_verifier_does_not_pass() {
    let (rows, manifest) = signed_manifest();
    let bad = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Invalid("bad signature".to_string())),
        "2026-10-07",
    );
    assert_eq!(bad.exit_code(), 1);
    assert!(bad.reasons().join(" ").contains("does not verify"));

    let absent = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Unverifiable(
            "minisign is not runnable".to_string(),
        )),
        "2026-10-07",
    );
    assert_eq!(absent.exit_code(), 3);
}

/// A verifier must never report a bogus signature as `Valid`. Whether it answers `Invalid` or
/// "cannot answer" depends on whether the tool is installed, and this test must not depend on
/// that — it did, and installing `minisign` turned a true property into a false failure. A test
/// that pins the environment instead of the behaviour is the same class of mistake as a corpus
/// label taken from the implementation.
#[test]
fn the_real_verifier_never_calls_a_bogus_signature_valid() {
    let outcome = MinisignCli.verify(b"payload", "untrusted comment: sig", "RWQf");
    assert!(
        matches!(
            outcome,
            SignatureOutcome::Invalid(_) | SignatureOutcome::Unverifiable(_)
        ),
        "a bogus signature must never verify: {outcome:?}"
    );
}

// --- The gate (scripts/check.sh, review finding F5): the committed Baseline is wired in.
//
// Tooling these tests require, deliberately with no skip path: a working `minisign` — a
// hard requirement recorded in the gate itself (`brew install minisign`), because a run
// without a verifier answers UNVERIFIABLE rather than passing — and, for the gate tests,
// `bash`, `git`, and the `$HOME/.rustup/toolchains` layout scripts/check.sh resolves. A
// missing tool fails the suite loudly; skipping would drop the Baseline check silently,
// and a skipped check is never a pass. The gate tests are `#[cfg(unix)]` because they
// spawn a shell script.
//
// Fixtures live under `CARGO_TARGET_TMPDIR`, the repo's pattern (tests/loader.rs), and the
// gate tests hand the step the binary this test run already built through `REQ_GATE_BIN`:
// the Baseline step is then exercised without nesting a second cargo invocation — and a
// second build-dir lock — inside `cargo test`.

/// The absolute path of the compiled binary, so CLI tests do not depend on $PATH.
fn req_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(std::path::PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// The verifier the Baseline tests need, asserted up front so a machine without it fails
/// naming the tool and its install line, rather than as a puzzling `[UNVERIFIABLE]` where a
/// test expected `[VERIFIED]`. There is deliberately no skip path: the gate's Baseline step
/// has none either, because an unverified Baseline must never read as a pass (ADR 0010).
fn require_minisign() {
    let installed = std::process::Command::new("minisign")
        .arg("-v")
        .output()
        .is_ok_and(|out| out.status.success());
    assert!(
        installed,
        "minisign is required — install it first: `brew install minisign` \
         (Debian: `apt-get install minisign`)"
    );
}

/// A fixture directory that removes itself when the test ends — including when an
/// assertion panics, which a trailing cleanup call would skip.
struct Fixture(std::path::PathBuf);

impl Fixture {
    /// A fresh directory for one fixture, under the build tree Cargo guarantees is
    /// writable. A leftover from an earlier crashed run is removed first, so "fresh"
    /// holds.
    fn new(name: &str) -> Self {
        let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the fixture directory");
        Fixture(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A copy of the committed Baseline with one hex digit of one address flipped, the
/// signature still beside it: the bytes no longer match what was signed and the
/// address no longer recomputes — both definite findings, so exit 1.
fn tampered_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("baselines/air-gapped.json"))
        .expect("the committed manifest reads");
    let marker = "\"address\":\"";
    let at = text.find(marker).expect("the manifest records an address") + marker.len();
    let mut bytes = text.into_bytes();
    let byte = bytes
        .get_mut(at)
        .expect("a hex address follows the \"address\":\" marker");
    *byte = if *byte == b'a' { b'b' } else { b'a' };
    let path = dir.join("air-gapped.json");
    std::fs::write(&path, bytes).expect("writes the tampered manifest");
    std::fs::copy(
        root.join("baselines/air-gapped.json.minisig"),
        dir.join("air-gapped.json.minisig"),
    )
    .expect("copies the signature beside it");
    path
}

/// The committed manifest verbatim, into a freshly created directory: with no signature
/// beside it nothing is ratified, so verification must answer UNVERIFIABLE (exit 3) —
/// never a pass. That no signature exists is guaranteed by `Fixture::new`, not cleaned
/// up after the fact.
#[cfg(unix)]
fn unsigned_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = dir.join("air-gapped.json");
    std::fs::copy(root.join("baselines/air-gapped.json"), &path).expect("copies the manifest");
    path
}

/// The gate runs exactly this against the committed artifacts, and the review's gap
/// asked for it by name: the committed Baseline must verify against the working tree —
/// here, before an audit ever has to.
#[test]
fn the_committed_baseline_verifies_against_the_working_tree() {
    require_minisign();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new(req_bin())
        .args(["baseline", "--verify"])
        .arg(root.join("baselines/air-gapped.json"))
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "exit {:?}:\n{stdout}",
        output.status.code()
    );
    assert!(stdout.contains("[VERIFIED]"), "{stdout}");
}

/// Any byte changed after signing is a definite finding, not an open question: the
/// signature no longer matches and the address no longer recomputes.
#[test]
fn a_tampered_baseline_does_not_verify() {
    require_minisign();
    let fixture = Fixture::new("cli-tampered");
    let manifest = tampered_fixture(fixture.path());
    let output = std::process::Command::new(req_bin())
        .args(["baseline", "--verify"])
        .arg(&manifest)
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "tamper must FAIL (exit 1), not UNVERIFIABLE:\n{stdout}"
    );
    assert!(stdout.contains("[FAIL]"), "{stdout}");
}

/// Runs `scripts/check.sh` with `args` from the tree under test, handing the Baseline step
/// the binary this test run already built.
#[cfg(unix)]
fn check_sh_args(args: &[&std::ffi::OsStr]) -> std::process::Output {
    check_sh_env(args, &req_bin())
}

/// ...with an explicit `REQ_GATE_BIN` value, so the seam's own validation is testable.
/// Two things are wired up: the pinned toolchain's HOME — check.sh resolves it under
/// `$HOME/.rustup/toolchains`, and this workspace keeps it in .tools/home instead (gitignored;
/// see .tools/README.md), shared through the git common dir so a feature worktree finds the
/// main checkout's — and `REQ_GATE_BIN`, which points the Baseline step at a binary instead of
/// nesting a second cargo inside `cargo test`.
#[cfg(unix)]
fn check_sh_env(args: &[&std::ffi::OsStr], req_gate_bin: &std::path::Path) -> std::process::Output {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = std::process::Command::new("bash");
    command
        .arg(root.join("scripts/check.sh"))
        .args(args)
        .current_dir(&root)
        .env("REQ_GATE_BIN", req_gate_bin);
    let home_has_rustup = std::env::var_os("HOME")
        .is_some_and(|h| std::path::Path::new(&h).join(".rustup/toolchains").exists());
    if !home_has_rustup {
        // git is a hard requirement of these tests (documented above): its failure is
        // reported here, with git's own message, rather than swallowed into an empty path
        // that would surface later as check.sh's generic "no rust toolchain".
        let common = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .output()
            .expect("git is required to locate the shared toolchain home");
        assert!(
            common.status.success(),
            "git rev-parse --git-common-dir failed: {}",
            String::from_utf8_lossy(&common.stderr)
        );
        let common = String::from_utf8_lossy(&common.stdout).trim().to_string();
        // Failing here, by name, beats letting check.sh exit 2 with its generic "no rust
        // toolchain" for a layout this helper was supposed to find.
        let candidate = std::path::PathBuf::from(&common)
            .parent()
            .map(|main| main.join(".tools/home"))
            .filter(|h| h.is_absolute() && h.join(".rustup/toolchains").exists());
        let Some(home) = candidate else {
            panic!(
                "no toolchain home for check.sh: $HOME/.rustup/toolchains is absent and so is \
                 <git common dir>/../.tools/home/.rustup/toolchains (see .tools/README.md)"
            );
        };
        command.env("HOME", home);
    }
    command.output().expect("runs scripts/check.sh")
}

/// The Baseline step, with an optional fixture in place of the committed Baseline.
#[cfg(unix)]
fn check_sh(baseline: Option<&std::path::Path>) -> std::process::Output {
    match baseline {
        Some(path) => check_sh_args(&["baseline".as_ref(), path.as_os_str()]),
        None => check_sh_args(&["baseline".as_ref()]),
    }
}

/// The default run's own call site — the F5 wiring that re-verifies the committed
/// Baseline on every gate invocation. The step tests above cannot reach it: `baseline`
/// takes the standalone branch and returns before the default flow. The `plan` step
/// prints the default run's commands from the same call sites and executes none, so this
/// pins the wiring without running the gate. It is UNVERIFIABLE (exit 3): a plan is not a
/// pass.
#[cfg(unix)]
#[test]
fn the_default_gate_run_verifies_the_committed_baseline() {
    let plan = check_sh_args(&["plan".as_ref()]);
    let stdout = String::from_utf8_lossy(&plan.stdout);
    assert_eq!(
        plan.status.code(),
        Some(3),
        "a plan executes nothing, so it is UNVERIFIABLE, never a pass:\n{stdout}"
    );
    assert!(
        stdout.contains("baseline --verify baselines/air-gapped.json"),
        "the default run must verify the committed Baseline:\n{stdout}"
    );
    // ...and it is the default run being planned, not the single step.
    assert!(stdout.contains("PLAN  cargo test"), "{stdout}");
    assert!(stdout.contains("GATE: UNVERIFIABLE"), "{stdout}");
}

/// The step-name contract fails fast: an unknown step, or an argument the contract does
/// not define, exits 2 with usage on stderr — never a quiet run of a different scope.
#[cfg(unix)]
#[test]
fn the_gate_rejects_an_unknown_step_and_stray_arguments() {
    let unknown = check_sh_args(&["nonsense".as_ref()]);
    assert_eq!(
        unknown.status.code(),
        Some(2),
        "an unknown step must be a usage error:\n{}",
        String::from_utf8_lossy(&unknown.stdout)
    );
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("usage:"),
        "{}",
        String::from_utf8_lossy(&unknown.stderr)
    );

    let stray = check_sh_args(&["baseline".as_ref(), "a".as_ref(), "b".as_ref()]);
    assert_eq!(
        stray.status.code(),
        Some(2),
        "a stray argument must be a usage error:\n{}",
        String::from_utf8_lossy(&stray.stdout)
    );
    assert!(
        String::from_utf8_lossy(&stray.stderr).contains("unexpected argument: b"),
        "{}",
        String::from_utf8_lossy(&stray.stderr)
    );

    // A path the step cannot mean is rejected here, not forwarded to req as a mystery:
    // an empty value, or one req would read as its own flag.
    for bad in ["", "-x"] {
        let rejected = check_sh_args(&["baseline".as_ref(), bad.as_ref()]);
        assert_eq!(
            rejected.status.code(),
            Some(2),
            "{bad:?} must be a usage error:\n{}",
            String::from_utf8_lossy(&rejected.stdout)
        );
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains("baseline file must be a path"),
            "{}",
            String::from_utf8_lossy(&rejected.stderr)
        );
    }
}

/// The binary seam refuses what it cannot mean, by name and with exit 2: a value that is not
/// a regular executable file. A directory is the interesting case — `-x` is true for it, so
/// testing only executability would send it to a generic gate failure instead.
#[cfg(unix)]
#[test]
fn the_gate_refuses_a_binary_override_that_is_not_an_executable_file() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for bad in [root.clone(), root.join("nowhere-at-all")] {
        let refused = check_sh_env(&["baseline".as_ref()], &bad);
        assert_eq!(
            refused.status.code(),
            Some(2),
            "{} must be refused as a usage error:\n{}",
            bad.display(),
            String::from_utf8_lossy(&refused.stdout)
        );
        assert!(
            String::from_utf8_lossy(&refused.stderr)
                .contains("REQ_GATE_BIN is not an executable file"),
            "{}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }
}

/// The red test of the wiring: the gate runs the Baseline step, and a Baseline that
/// does not verify fails it. Before the step existed the gate stayed green while the
/// committed Baseline no longer verified — the exact blind spot F5 files.
#[cfg(unix)]
#[test]
fn the_gate_fails_when_the_baseline_does_not_verify() {
    require_minisign();
    let fixture = Fixture::new("gate-tampered");
    let manifest = tampered_fixture(fixture.path());
    let output = check_sh(Some(&manifest));
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The verdict line, not just a non-zero exit: the gate must fail *about the
    // Baseline*, not about some unrelated check that happens to be red.
    assert!(stdout.contains("[FAIL]"), "{stdout}");
    assert!(
        !output.status.success(),
        "a tampered Baseline must fail the gate (exit {:?}):\n{stdout}",
        output.status.code()
    );
    assert!(stdout.contains("BASELINE STEP: FAIL"), "{stdout}");
}

/// The decision the wiring records: exit 3 — UNVERIFIABLE, here an unsigned manifest —
/// blocks. "We could not tell" is not a pass; a skipped check is never a pass.
#[cfg(unix)]
#[test]
fn the_gate_blocks_an_unverifiable_baseline() {
    // Without the verifier this test would pass for the wrong reason: a missing minisign
    // answers [UNVERIFIABLE] just as a missing signature does, so the unsigned-manifest path
    // it claims to prove would never be exercised.
    require_minisign();
    let fixture = Fixture::new("gate-unsigned");
    let manifest = unsigned_fixture(fixture.path());
    let output = check_sh(Some(&manifest));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[UNVERIFIABLE]"), "{stdout}");
    assert!(
        !output.status.success(),
        "UNVERIFIABLE must block the gate (exit {:?}):\n{stdout}",
        output.status.code()
    );
    assert!(stdout.contains("BASELINE STEP: FAIL"), "{stdout}");
}

/// ...and the committed Baseline passes the step it is wired into: the step exists and
/// is green on the tree it guards. Removing the step fails here, not silently. The step
/// reports its own verdict, so one check is never mistaken for the gate's pass.
#[cfg(unix)]
#[test]
fn the_gate_verifies_the_committed_baseline() {
    require_minisign();
    let output = check_sh(None);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the committed Baseline must pass the gate (exit {:?}):\n{stdout}",
        output.status.code()
    );
    assert!(stdout.contains("[VERIFIED]"), "{stdout}");
    assert!(stdout.contains("BASELINE STEP: PASS"), "{stdout}");
    assert!(
        !stdout.contains("GATE: PASS"),
        "one check is not the gate:\n{stdout}"
    );
}

#[test]
fn drift_names_changed_missing_and_added_separately() {
    let (mut rows, manifest) = signed_manifest();
    rows.retain(|r| r.claim_id != "REQ-A");
    rows.push(row("REQ-B", "bbb-changed", Applies::Yes));
    rows.push(row("REQ-D", "ddd", Applies::Yes));
    let drift = drift(&rows, &manifest);
    assert_eq!(drift.missing, vec!["REQ-A".to_string()]);
    assert_eq!(drift.changed, vec!["REQ-B".to_string()]);
    assert_eq!(drift.added, vec!["REQ-D".to_string()]);
    assert!(!drift.is_clean());
}

/// The invariant `audit::coverage`'s merge relies on: the drift categories are mutually
/// exclusive by precedence. A claim whose address **and** attested surface both moved is
/// recorded in `changed` alone — never in `attested_changed` too — so merging the two lists
/// cannot double-count an id, and `reasons()` names the address without also claiming the audit
/// surface moved. Pinned because inverting the nesting — checking the attested surface before
/// identity — would break it silently: verification would still fail, but the packet would
/// count the claim twice.
#[test]
fn a_claim_whose_address_and_attested_surface_both_moved_is_changed_alone() {
    // The columns and their digest derived together by the shared helper: a real digest, not a
    // placeholder — the row is the state `load_rows` produces from the edited file.
    let (relations, relations_hash) = common::relation_columns(&[("refines", "REQ-B")]);
    let (rows, manifest) = signed_manifest();
    let edited: Vec<Row> = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if r.claim_id == "REQ-B" {
                // Both surfaces move: a reworded statement (a new address) and a relation
                // added after signing (a new attested hash).
                r.address = "bbb-changed".to_string();
                r.relations = relations.clone();
                r.relations_hash = relations_hash.clone();
            }
            r
        })
        .collect();
    let drift = drift(&edited, &manifest);
    assert_eq!(drift.changed, vec!["REQ-B".to_string()]);
    assert!(
        drift.attested_changed.is_empty(),
        "an address change subsumes the attested one; the categories are exclusive: {drift:?}"
    );
    let reasons = drift.reasons().join(" ");
    assert!(reasons.contains("no longer recomputes"), "{reasons}");
    assert!(
        !reasons.contains("relations or rationale"),
        "precedence states only the address change: {reasons}"
    );
}

/// The manifest defines the variant, so verifying with no context must resolve to the selection
/// the Baseline records. Verifying under an empty context is what produced the phantom
/// `REQ-0012: in the Baseline but no longer applies` on the first signed Baseline.
#[test]
fn an_empty_context_resolves_to_the_selection_the_baseline_records() {
    let (_, manifest) = signed_manifest();
    let resolved = req::baseline::resolve_selection(&Context::new(), &manifest).expect("resolves");
    assert_eq!(resolved, manifest.selection);
    assert!(!resolved.is_empty(), "the fixture records a selection");
    // Passing exactly the recorded selection is accepted (a no-op, not a contradiction).
    let explicit =
        req::baseline::resolve_selection(&manifest.selection, &manifest).expect("accepted");
    assert_eq!(explicit, manifest.selection);
}

/// A contradiction is the caller's mistake, so it is reported as one — naming both selections —
/// instead of being computed as drift in the claims.
#[test]
fn a_contradicting_context_is_a_usage_error_not_a_finding() {
    let (_, manifest) = signed_manifest();
    let mut other = Context::new();
    other.insert("deployment_mode".to_string(), "networked".to_string());
    let error = req::baseline::resolve_selection(&other, &manifest).expect_err("contradiction");
    assert!(error.contains("records selection"), "{error}");
    assert!(error.contains("air_gapped"), "{error}");
    assert!(error.contains("networked"), "{error}");
}
