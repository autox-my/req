//! The Baseline: canonical bytes, drift, and the three-state verdict.

use req::baseline::{
    BASELINE_VERSION, Manifest, MinisignCli, Outcome, SignatureOutcome, SignatureVerifier,
    TrustFile, canonical_bytes, drift, from_json, manifest, verify,
};
use req::model::{Rationale, Relation, rationale_digest, relations_digest};
use req::select::{Applies, Context};
use req::view::Row;

fn row(id: &str, address: &str, applies: Applies) -> Row {
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: address.to_string(),
        applies,
        selectors: Vec::new(),
        relations: Vec::new(),
        relations_hash: "relations-hash".to_string(),
        rationale_hash: "rationale-hash".to_string(),
        stored: req::fill::StoredState::Verified,
        rationale_source: "docs/adr/0010-baseline-signing-and-trust.md".to_string(),
        rationale_date: "2026-10-07".to_string(),
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
    // Selection keys sorted, non-applying claims absent, and one trailing newline.
    assert_eq!(
        canonical_bytes(&forward),
        "{\"version\":2,\"rules_v\":5,\"selection\":{\"a\":\"1\",\"b\":\"2\"},\
         \"claims\":[{\"claim_id\":\"REQ-A\",\"address\":\"aaa\",\"relations_hash\":\"relations-hash\",\"rationale_hash\":\"rationale-hash\"},\
         {\"claim_id\":\"REQ-B\",\"address\":\"bbb\",\"relations_hash\":\"relations-hash\",\"rationale_hash\":\"rationale-hash\"}]}\n"
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

/// ADR 0013: the audit surface is attested — editing a relation after signing is detected
/// even when the meaning (and so the address) is untouched.
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
        relations_digest(&[]),
        "an empty list still attests"
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
