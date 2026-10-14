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
    let (rationale_source, rationale_date, rationale_hash) =
        common::rationale_columns("docs/adr/0010-baseline-signing-and-trust.md", "2026-10-07");
    // One idiom with every other row fixture (`common::relation_columns`): the columns and
    // their digest are derived together, so the empty case is a single expression, not two
    // that must be kept in step.
    let (relations, relations_hash) = common::relation_columns(Vec::new());
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: address.to_string(),
        applies,
        selectors: Vec::new(),
        relations,
        relations_hash,
        rationale_hash,
        stored: req::fill::StoredState::Verified,
        rationale_source,
        rationale_date,
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
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(err.contains("claim_id"), "{err}");
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
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(err.contains("rationale_hash"), "{err}");
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
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(err.contains("version"), "{err}");
    assert!(!err.contains("re-issue"), "{err}");
    assert!(!err.contains("ADR 0013"), "{err}");
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
/// surface moved. Pinned because a reordered match arm would break it silently: verification
/// would still fail, but the packet would count the claim twice.
#[test]
fn a_claim_whose_address_and_attested_surface_both_moved_is_changed_alone() {
    // A real relation digest, not a placeholder: the row is the state `load_rows` produces
    // from the edited file.
    let relations = vec![Relation {
        kind: "refines".to_string(),
        to: "REQ-B".to_string(),
    }];
    let (rows, manifest) = signed_manifest();
    let edited: Vec<Row> = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if r.claim_id == "REQ-B" {
                // Both surfaces move: a reworded statement (a new address) and a relation
                // added after signing (a new attested hash).
                r.address = "bbb-changed".to_string();
                r.relations = relations
                    .iter()
                    .map(|relation| (relation.kind.clone(), relation.to.clone()))
                    .collect();
                r.relations_hash = relations_digest(&relations);
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
