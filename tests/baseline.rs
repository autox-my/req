//! The Baseline: canonical bytes, drift, and the three-state verdict.

mod common;

use req::baseline::{
    BASELINE_VERSION, Manifest, MinisignCli, Outcome, SignatureOutcome, SignatureVerifier,
    TrustFile, canonical_bytes, create_staging_dir, drift, from_json, manifest, staging_dir,
    staging_dir_name, verify,
};
use req::canon::RULES_V;
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

/// The variant the fixtures sign under. Named once: the manifest records the selection, so a
/// second hand-written copy of it is a second thing to keep in step.
fn air_gapped() -> Context {
    let mut context = Context::new();
    context.insert("deployment_mode".to_string(), "air_gapped".to_string());
    context
}

/// The manifest a test signs, as the CLI reads one: through the library's own canonical writer
/// and parser, so the declared `rules_v` under test survives exactly the bytes a signature
/// covers.
fn manifest_declaring(rules_v: u32, rows: &[Row], selection: &Context) -> Manifest {
    from_json(&canonical_bytes(&manifest(rows, rules_v, selection)))
        .expect("a fixture manifest round-trips through its own bytes")
}

fn signed_manifest() -> (Vec<Row>, Manifest) {
    let context = air_gapped();
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes),
        row("REQ-B", "bbb", Applies::Yes),
        row("REQ-C", "ccc", Applies::No),
    ];
    // `RULES_V`, not a literal: the rules version is now *read* by verification, so a fixture
    // pinning a hand-copied number would silently become a mismatch fixture — and every test
    // built on it would change meaning — the moment the constant moves.
    let manifest = manifest(&rows, RULES_V, &context);
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

// --- The canonical rules version, judged before drift -----------------------------------------
//
// The seam is `baseline --verify <file>`: the exit class and the printed reasons. The genuine
// case — the manifest's bytes untouched and correctly signed while the *build* writes a
// different canonical rules version — cannot be produced through the CLI in this repository
// without either moving `RULES_V` (forbidden by the pack: it would move every address) or
// signing with the secret key (the signing act is the human's), so the first four tests drive
// the same `verify` the CLI calls, with the signature oracle answering `Valid` for a manifest
// that is in fact untouched. The two CLI tests below cover what *can* be observed end to end: a
// post-signing `rules_v` edit, which is a definite finding, and `audit --baseline`.

/// Case (i), red before this change: a real rules bump. The manifest was signed under the
/// previous rules version, so *every* address in the working tree has moved. The old `verify`
/// exits 1 and blames every claim for "being edited after signing", sending an auditor hunting
/// for a tampering that never happened instead of re-signing. The rules version is not a question
/// drift can answer: a claim's address is *derived from* it, so the comparison across two rule
/// sets is void, not inconclusive.
#[test]
fn a_rules_version_bump_is_unverifiable_and_blames_no_claim() {
    let context = air_gapped();
    let signed = vec![
        row("REQ-A", "addr-signed-a", Applies::Yes),
        row("REQ-B", "addr-signed-b", Applies::Yes),
        row("REQ-C", "addr-signed-c", Applies::No),
    ];
    let manifest = manifest_declaring(RULES_V - 1, &signed, &context);
    // The working tree after the bump: the same claims, every address recomputed under the
    // current rules.
    let bumped = vec![
        row("REQ-A", "addr-current-a", Applies::Yes),
        row("REQ-B", "addr-current-b", Applies::Yes),
        row("REQ-C", "addr-signed-c", Applies::No),
    ];
    let outcome = verify(
        &bumped,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    // An unanswerable question is 3, never 1 — the exit class comes from the tri-state contract,
    // not from what the implementation happens to print.
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains(&format!("v{}", RULES_V - 1)), "{reasons}");
    assert!(reasons.contains(&format!("v{RULES_V}")), "{reasons}");
    assert!(reasons.contains("drift was NOT judged"), "{reasons}");
    assert!(reasons.contains("re-issue"), "{reasons}");
    // THE DEFECT IS THE MISDIAGNOSIS, not only the exit code: no per-claim drift reason may
    // appear *anywhere* in the output. Asserting exit 3 alone would pass with the claim-by-claim
    // blame still printed underneath it.
    for id in ["REQ-A", "REQ-B"] {
        assert!(
            !reasons.contains(id),
            "no per-claim blame under a rules mismatch: {reasons}"
        );
    }
    assert!(!reasons.contains("no longer recomputes"), "{reasons}");
    assert!(!reasons.contains("edited after signing"), "{reasons}");
}

/// Case (ii), the mirror image and the same guard: a manifest that misdeclares an older rules
/// version while its addresses are current under this build's rules. Nothing has moved and the
/// signature is valid, so the old `verify` answers **Verified, exit 0** — silently accepting a
/// manifest whose own declared provenance is wrong.
#[test]
fn a_manifest_misdeclaring_an_older_rules_version_is_not_verified() {
    let context = air_gapped();
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes),
        row("REQ-B", "bbb", Applies::Yes),
    ];
    let manifest = manifest_declaring(RULES_V - 1, &rows, &context);
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(
        outcome.exit_code(),
        3,
        "a misdeclared rules version is not a pass: {outcome:?}"
    );
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains(&format!("v{}", RULES_V - 1)), "{reasons}");
    assert!(reasons.contains(&format!("v{RULES_V}")), "{reasons}");
    assert!(reasons.contains("drift was NOT judged"), "{reasons}");
}

/// The remedy follows the direction of the mismatch, exactly as the envelope's does: a manifest
/// written by a *newer* rules version must be verified with a build that writes it. Telling it to
/// re-sign under this binary's rules would downgrade what the manifest attests.
#[test]
fn a_newer_rules_version_is_not_told_to_re_sign_with_this_binary() {
    let context = air_gapped();
    let rows = vec![row("REQ-A", "aaa", Applies::Yes)];
    let manifest = manifest_declaring(RULES_V + 1, &rows, &context);
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("drift was NOT judged"), "{reasons}");
    assert!(reasons.contains("build that writes"), "{reasons}");
    assert!(
        !reasons.contains("re-issue"),
        "a newer manifest must not be told to re-issue with this binary: {reasons}"
    );
}

/// The guard is a precedence change, not a blanket suppression of drift: a manifest whose rules
/// version **matches** and whose addresses have moved is still FAIL (exit 1), with its per-claim
/// reason — the same class as a hand-edited digest. This is the test that would fail if the guard
/// were widened into "a rules version is present, so skip drift".
#[test]
fn a_matching_rules_version_with_a_moved_address_still_fails() {
    let context = air_gapped();
    let rows = vec![
        row("REQ-A", "aaa", Applies::Yes),
        row("REQ-B", "bbb", Applies::Yes),
    ];
    let manifest = manifest_declaring(RULES_V, &rows, &context);
    assert_eq!(manifest.rules_v, RULES_V, "this is the matching case");
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
    assert_eq!(outcome.exit_code(), 1, "{outcome:?}");
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("REQ-B"), "{reasons}");
    assert!(reasons.contains("no longer recomputes"), "{reasons}");
    assert!(
        !reasons.contains("REQ-A"),
        "only the moved claim is named: {reasons}"
    );
}

/// The other half of the precedence decision: the guard judges the rules version the manifest
/// *declares*, and a definite finding about the manifest's own signed bytes still outranks it.
/// Editing `rules_v` after signing changes the signed bytes, so through the CLI this is FAIL —
/// never the guard's UNVERIFIABLE, which would hide a real tamper behind an open question.
#[test]
fn a_rules_version_edited_after_signing_still_fails_through_the_cli() {
    require_minisign();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = Fixture::new("cli-rules-v-tampered");
    let text = std::fs::read_to_string(root.join("baselines/air-gapped.json"))
        .expect("the committed manifest reads");
    let mut parsed = from_json(&text).expect("the committed manifest parses");
    assert_eq!(
        parsed.rules_v, RULES_V,
        "the fixture is the committed manifest, whose declared rules version matches this build"
    );
    parsed.rules_v = RULES_V - 1;
    let manifest = fixture.path().join("air-gapped.json");
    std::fs::write(&manifest, canonical_bytes(&parsed)).expect("writes the edited manifest");
    std::fs::copy(
        root.join("baselines/air-gapped.json.minisig"),
        fixture.path().join("air-gapped.json.minisig"),
    )
    .expect("copies the signature, which no longer covers these bytes");

    let output = std::process::Command::new(req_bin())
        .args(["baseline", "--verify"])
        .arg(&manifest)
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a post-signing rules_v edit is a definite finding, not an open question:\n{stdout}"
    );
    assert!(stdout.contains("[FAIL]"), "{stdout}");
    assert!(
        !stdout.contains("[UNVERIFIABLE]"),
        "the mismatch must not swallow a signature failure:\n{stdout}"
    );
}

/// The audit decision (pack option (a)): `audit --baseline` shares the guard rather than judging
/// drift beneath a mismatched rules version. The manifest here is the committed one with only its
/// declared `rules_v` moved, so every address still matches and the old packet is a clean
/// `VERIFIED` — exactly the false comfort this removes. Audit's contract is not rewritten: a
/// coverage question that was not answered is UNVERIFIABLE (exit 3) and says so, and its
/// provenance policies, which do not depend on the rules version, still run.
#[test]
fn an_audit_under_a_misdeclared_rules_version_reports_coverage_not_judged() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = Fixture::new("cli-audit-rules-v");
    let text = std::fs::read_to_string(root.join("baselines/air-gapped.json"))
        .expect("the committed manifest reads");
    let mut parsed = from_json(&text).expect("the committed manifest parses");
    parsed.rules_v = RULES_V - 1;
    let manifest = fixture.path().join("air-gapped.json");
    std::fs::write(&manifest, canonical_bytes(&parsed)).expect("writes the edited manifest");

    let output = std::process::Command::new(req_bin())
        .args(["audit", "--baseline"])
        .arg(&manifest)
        .current_dir(&root)
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(3),
        "coverage that was not judged is UNVERIFIABLE, never a pass:\n{stdout}"
    );
    assert!(stdout.contains("NOT JUDGED"), "{stdout}");
    assert!(stdout.contains("drift was NOT judged"), "{stdout}");
    assert!(
        !stdout.contains("edited after signing"),
        "no claim is blamed for a comparison that never happened:\n{stdout}"
    );
    assert!(
        !stdout.contains("[covered]"),
        "nothing is reported as covered when nothing was compared:\n{stdout}"
    );
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
// `bash`, `git`, and a `$HOME/.rustup/toolchains` layout scripts/check.sh resolves. A
// missing tool fails the suite loudly; skipping would drop the Baseline check silently,
// and a skipped check is never a pass. `cargo test` therefore needs `minisign` installed,
// exactly as `scripts/check.sh` does. The gate tests are `#[cfg(unix)]` because they
// spawn a shell script.
//
// Fixtures live under `CARGO_TARGET_TMPDIR`, the repo's pattern (tests/loader.rs), and the
// gate tests hand the step the binary this test run already built through `REQ_GATE_BIN`:
// the Baseline step is then exercised without nesting a second cargo invocation — and a
// second build-dir lock — inside `cargo test`.
//
// Only the `baseline` step and `plan` are driven from here. `all` is the gate's full run:
// invoking it from a test would run `cargo test` inside `cargo test`, which recurses.

/// The absolute path of the compiled binary, so CLI tests do not depend on $PATH.
fn req_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(std::path::PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// The verifier the Baseline tests need, asserted up front so a machine without it fails
/// naming the tool and its install line, rather than as a puzzling `[UNVERIFIABLE]` where a
/// test expected `[VERIFIED]`.
///
/// This is a deliberate prerequisite of the suite: `cargo test` needs `minisign` on the host.
/// The alternative — skipping the Baseline assertions when the tool is absent — would leave the
/// verification untested and green, the same failure the gate refuses by carrying no SKIP on its
/// Baseline step (AGENTS.md). It is a different question from
/// `the_real_verifier_never_calls_a_bogus_signature_valid` above: that test asserts a property
/// which must hold whether or not the tool is installed (a bogus signature is never `Valid`),
/// while these assert the outcome of a real verification, which exists only with the tool.
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

    /// A fixture whose name is unique to this process and moment, for a test that must not
    /// collide with a concurrent run of itself. A process-local counter carries the guarantee
    /// rather than the clock, whose resolution is not one. The cleanup is `Fixture`'s, not a
    /// second copy's.
    fn unique(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        // `unwrap_or_default` collapses a pre-epoch clock error to zero and a coarse clock can
        // repeat a value, and a repeated name would make `Fixture::new` delete a sibling
        // fixture's live directory.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        Fixture::new(&format!("{tag}-{}-{nanos}-{serial}", std::process::id()))
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
/// signature still beside it: the bytes no longer match what was signed and the address no
/// longer recomputes — both definite findings, so exit 1.
///
/// The manifest is read and written through the library's own parser and canonical writer,
/// so the tamper depends on the manifest *schema* rather than on the exact byte formatting
/// of the committed file (a pretty-printed re-emit would otherwise break the fixture
/// instead of exercising the FAIL path).
fn tampered_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("baselines/air-gapped.json"))
        .expect("the committed manifest reads");
    let mut parsed = from_json(&text).expect("the committed manifest parses");
    let entry = parsed
        .claims
        .first_mut()
        .expect("the manifest records at least one claim");
    let mut address = entry.address.clone().into_bytes();
    let byte = address
        .first_mut()
        .expect("an address is a non-empty hex string");
    *byte = if *byte == b'a' { b'b' } else { b'a' };
    entry.address = String::from_utf8(address).expect("hex is ASCII");
    let path = dir.join("air-gapped.json");
    std::fs::write(&path, canonical_bytes(&parsed)).expect("writes the tampered manifest");
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
/// Two things are wired up: HOME, so `scripts/check.sh` finds the same toolchain this
/// workspace's own gate runs under, and `REQ_GATE_BIN`, which points the Baseline step at a
/// binary instead of nesting a second cargo inside `cargo test`.
#[cfg(unix)]
fn check_sh_env(args: &[&std::ffi::OsStr], req_gate_bin: &std::path::Path) -> std::process::Output {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = std::process::Command::new("bash");
    command
        .arg(root.join("scripts/check.sh"))
        .args(args)
        .current_dir(&root)
        .env("REQ_GATE_BIN", req_gate_bin);
    // The checkout's own toolchain home wins when it has one — that is how the gate is run by
    // hand in this workspace (.tools/README.md) — so the test drives the toolchain the gate is
    // meant to use rather than whatever the ambient HOME happens to hold. Where there is none,
    // HOME is left exactly as the developer or CI set it (CI installs the pinned channel into
    // $HOME/.rustup and means it). Neither present is a failure named here, not check.sh's
    // generic exit 2.
    let home_has_rustup = std::env::var_os("HOME")
        .is_some_and(|h| std::path::Path::new(&h).join(".rustup/toolchains").exists());
    match workspace_toolchain_home(&root) {
        Some(workspace) => {
            command.env("HOME", workspace);
        }
        None if home_has_rustup => {}
        None => panic!(
            "no toolchain home for check.sh: this checkout has no .tools/home/.rustup/toolchains \
             and $HOME/.rustup/toolchains does not exist either (see .tools/README.md)"
        ),
    }
    command.output().expect("runs scripts/check.sh")
}

/// The toolchain home this checkout keeps for `scripts/check.sh`, when it has one: `.tools/home`
/// beside the git common dir (.tools/README.md), which a feature worktree shares with the main
/// checkout. `git` is a hard requirement of these tests: its failure is reported here, with
/// git's own message, rather than swallowed into an empty path.
#[cfg(unix)]
fn workspace_toolchain_home(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let common = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .expect("git is required to locate the shared toolchain home");
    assert!(
        common.status.success(),
        "git rev-parse --git-common-dir failed: {}",
        String::from_utf8_lossy(&common.stderr)
    );
    let common = String::from_utf8_lossy(&common.stdout).trim().to_string();
    std::path::PathBuf::from(common)
        .parent()
        .map(|main| main.join(".tools/home"))
        .filter(|home| home.is_absolute() && home.join(".rustup/toolchains").exists())
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
    // Compared on normalized lines: the plan's content is the contract, its column padding is
    // not, so a cosmetic reformat of check.sh must not fail this test.
    let planned: Vec<String> = stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    assert!(
        planned.iter().any(|line| line.starts_with("PLAN ")
            && line.contains("baseline --verify baselines/air-gapped.json")),
        "the default run must verify the committed Baseline:\n{stdout}"
    );
    // ...and it is the default run being planned, not the single step.
    assert!(
        planned
            .iter()
            .any(|line| line.starts_with("PLAN ") && line.contains("cargo test")),
        "{stdout}"
    );
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
    use std::os::unix::fs::PermissionsExt;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // A regular file with the execute bit cleared is the only value that reaches the second
    // half of the guard: a directory and a missing path already fail its regular-file test.
    let fixture = Fixture::new("binary-override");
    let plain = fixture.path().join("not-executable");
    std::fs::write(&plain, b"#!/bin/sh\nexit 0\n").expect("writes a plain file");
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644))
        .expect("clears the execute bit");

    for bad in [root.clone(), root.join("nowhere-at-all"), plain.clone()] {
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

/// F9: the staging directory must be *created*, never reused. A path that already exists —
/// left behind by an earlier run, or a symlink planted at a predictable name — is an error,
/// not a directory whose contents we then write the trusted inputs into.
#[test]
fn creating_the_same_staging_dir_twice_is_already_exists() {
    let base = Fixture::unique("req-f9-exclusive");
    let name = staging_dir_name(0);
    let created = create_staging_dir(base.path(), &name).expect("the first creation succeeds");
    assert_eq!(created, base.path().join(&name));
    let second = create_staging_dir(base.path(), &name);
    assert!(
        second.is_err(),
        "a second creation must not reuse the existing directory: {second:?}"
    );
    assert_eq!(
        second.unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
}

/// F9: the exclusive creation is what makes a *planted symlink* harmless — `mkdir` refuses any
/// existing entry, so a symlink at the staging name is neither followed nor turned into a
/// directory whose contents the trusted inputs are written through. This is the threat the
/// explicit-creation comment names; the test above only re-creates a real directory.
#[cfg(unix)]
#[test]
fn a_symlink_at_the_staging_name_is_not_followed() {
    let base = Fixture::unique("req-f9-symlink");
    let elsewhere = Fixture::unique("req-f9-symlink-target");
    let name = staging_dir_name(0);
    std::os::unix::fs::symlink(elsewhere.path(), base.path().join(&name))
        .expect("the symlink is planted at the staging name");

    let error = create_staging_dir(base.path(), &name)
        .expect_err("an existing entry, symlink included, is not reused");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    let planted = base.path().join(&name);
    assert!(
        planted.is_symlink(),
        "the planted entry is still a symlink, not a directory that was followed: {planted:?}"
    );
    assert_eq!(
        std::fs::read_link(&planted).expect("the planted symlink reads"),
        elsewhere.path()
    );
}

/// F9: mode 0700, so another local user cannot read the staged manifest — or, worse, replace
/// the files between staging and the verifier process reading them. The kernel masks the
/// requested mode with the umask, which can only remove bits, so the property asserted is the
/// privacy itself — no group or other bit — rather than exact equality with 0700.
#[cfg(unix)]
#[test]
fn the_staging_dir_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let base = Fixture::unique("req-f9-mode");
    let dir = create_staging_dir(base.path(), &staging_dir_name(0)).expect("created");
    let mode = std::fs::metadata(&dir)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o077,
        0,
        "no group or other bit is set on the staging directory: {mode:o}"
    );
}

/// F9: the guard owns the directory. The verifier holds the guard for the whole call, so an
/// early return, a failed write, an unrunnable verifier or a parse failure drops it on the way
/// out; this pins the drop itself, which is the mechanism every one of those paths relies on.
#[test]
fn dropping_the_staging_guard_removes_its_directory() {
    let guard = staging_dir().expect("a fresh staging directory under the temp dir");
    let dir = guard.path().to_path_buf();
    assert!(dir.exists(), "the staged directory exists before the drop");
    drop(guard);
    assert!(
        !dir.exists(),
        "dropping the guard removes the staging directory: {dir:?}"
    );
}

/// The cleanup a failing assertion relies on. The F9 tests used to carry a trailing
/// `remove_dir_all`, which a panicking `assert!` never reaches, so the base of a *failing* run
/// was the one that leaked. This pins the fixture's own drop without a deliberate panic: that
/// `Drop` runs while unwinding is a language guarantee, whereas hiding a deliberate panic's
/// output needs a process-global hook swap that would also mute a concurrent test's genuine
/// failure.
#[test]
fn a_fixture_removes_its_directory_on_drop() {
    let fixture = Fixture::unique("req-f9-cleanup");
    let dir = fixture.path().to_path_buf();
    assert!(dir.exists(), "the fixture directory exists before the drop");
    drop(fixture);
    assert!(
        !dir.exists(),
        "dropping the fixture removes its directory: {dir:?}"
    );
}
