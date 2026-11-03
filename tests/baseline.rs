//! The Baseline: canonical bytes, drift, and the three-state verdict.

mod common;

use req::baseline::{
    BASELINE_VERSION, Manifest, MinisignCli, OLDEST_VERIFIABLE_BASELINE_VERSION, Outcome,
    SignatureOutcome, SignatureVerifier, TrustFile, Verification, attest, canonical_bytes,
    create_staging_dir, drift, from_json, manifest, staging_dir, staging_dir_name,
};
use req::canon::RULES_V;
use req::decision::{self, Decision};
use req::model::{Rationale, Relation, rationale_digest, relations_digest};
use req::select::{Applies, Context};
use req::view::Row;

/// The signer's timestamp every fixture manifest carries. An authored assertion, exactly as the
/// CLI requires one — never a clock the code reads.
const FIXTURE_SIGNED_AT: &str = "2026-10-07T09:00:00Z";

/// The note the fixture Decision carries. Fixed, because the log's head is inside the signed
/// bytes: a fixture manifest attests the head of *this* log, and the wrapper below re-derives it.
const FIXTURE_DECISION_NOTE: &str = "the maintainer ratified REQ-A";

/// A one-entry Decision log, filled — the authority a v3 fixture manifest attests. Editing a
/// record after signing is falsification case 1 (ADR 0016).
fn fixture_log(note: &str) -> Vec<Decision> {
    let mut log = vec![Decision {
        id: "DEC-0001".to_string(),
        kind: "accept".to_string(),
        claim_id: "REQ-A".to_string(),
        address: "aaa".to_string(),
        actor: "the maintainer".to_string(),
        role: "maintainer".to_string(),
        date: "2026-10-07".to_string(),
        note: note.to_string(),
        prev_hash: None,
        hash: None,
    }];
    decision::fill(&mut log);
    log
}

/// The head accessor itself, on a two-entry log: the **last** record's hash and the count of
/// every record. With the repo's one-entry log these two properties are indistinguishable, so
/// the ADR's "a head alone cannot distinguish one entry" needs its own fixture.
#[test]
fn the_head_is_the_last_record_and_the_count_is_every_record() {
    let mut log = fixture_log(FIXTURE_DECISION_NOTE);
    let first = log[0].hash.clone().expect("filled");
    let mut second = log[0].clone();
    second.id = "DEC-0002".to_string();
    second.note = "the second record".to_string();
    second.prev_hash = None;
    second.hash = None;
    log.push(second);
    decision::fill(&mut log);
    let head = decision::head(&log).expect("a filled log has a head");
    assert_eq!(head.entries, 2, "the count is every record, not one");
    assert_eq!(
        head.hash,
        log[1].hash.clone().expect("filled"),
        "the head is the last record's hash"
    );
    assert_ne!(
        head.hash, first,
        "a head that returned the first entry is caught"
    );
}

/// A Baseline attests a **prefix**. Recording a new Decision is the ordinary way authority grows
/// (ADR 0005), so a Baseline signed before it must keep verifying — otherwise every
/// `req decision --fill` would force a re-sign with a key req cannot reach. A log shorter than
/// the attested prefix is still a finding, because the record it names is gone.
#[test]
fn a_decision_recorded_after_signing_does_not_invalidate_the_baseline() {
    let (rows, manifest) = signed_manifest();
    let mut log = fixture_log(FIXTURE_DECISION_NOTE);
    let mut appended = log[0].clone();
    appended.id = "DEC-0002".to_string();
    appended.note = "a later ratification".to_string();
    appended.prev_hash = None;
    appended.hash = None;
    log.push(appended);
    decision::fill(&mut log);
    let outcome = req::baseline::verify(
        &rows,
        &manifest,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Ok(&log),
        },
    );
    assert_eq!(
        outcome.exit_code(),
        0,
        "an appended decision is history: {outcome:?}"
    );

    // The `--fill` workflow's transient state: a record appended but not yet filled. The
    // attested prefix is intact, so the Baseline still verifies; the unfilled tail is
    // `req decision --verify`'s finding, not the manifest's.
    let mut unfilled = fixture_log(FIXTURE_DECISION_NOTE);
    unfilled.push(Decision {
        id: "DEC-0002".to_string(),
        kind: "accept".to_string(),
        claim_id: "REQ-B".to_string(),
        address: "bbb".to_string(),
        actor: "the maintainer".to_string(),
        role: "maintainer".to_string(),
        date: "2026-10-08".to_string(),
        note: "not yet filled".to_string(),
        prev_hash: None,
        hash: None,
    });
    let unfilled_outcome = req::baseline::verify(
        &rows,
        &manifest,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Ok(&unfilled),
        },
    );
    assert_eq!(
        unfilled_outcome.exit_code(),
        0,
        "an unfilled tail does not demote the attested prefix: {unfilled_outcome:?}"
    );

    // Attest the two-entry log, then show the log cannot be cut back below it.
    let mut candidate = manifest.clone();
    let two_entry_head = decision::head(&log).expect("filled");
    attest(&mut candidate, FIXTURE_SIGNED_AT, &two_entry_head)
        .expect("the fixture signing time is a valid attestation");
    let truncated = req::baseline::verify(
        &rows,
        &candidate,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Ok(&log[..1]),
        },
    );
    assert_eq!(truncated.exit_code(), 1, "{truncated:?}");
    assert!(
        truncated.reasons().join(" ").contains("attested prefix"),
        "{truncated:?}"
    );
}

/// The fixture log's head, as the manifest records it.
fn fixture_head() -> decision::Head {
    let log = fixture_log(FIXTURE_DECISION_NOTE);
    decision::head(&log).expect("a filled log has a head")
}

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

/// `printf 'req.tests.v1\0{"evidence":"machine","tests":[]}' | shasum -a 256` (the `\0` is a
/// literal NUL byte) — the fixture rows link no test, and the pinned v4 bytes must carry the
/// digest of that empty **declaration**. `tests/fill.rs` pins the same value against its own
/// oracle; an earlier draft hashed the bare list (60dd3ab0…) and the review caught the two pins
/// disagreeing — the implementation hashes the declaration.
const EMPTY_TESTS_DIGEST: &str = "f70c34ce463f8c0294ac8846abea5b911d5933c42a9d978522bff8c6b5445d53";

/// The fixture Decision's chain hash, computed **independently of this crate**:
/// `sha256("req.decision.v1" \0 "req.genesis.v1" "\n" <canonical decision bytes>)`, where the
/// canonical bytes are the fixed fixture record with note `the maintainer ratified REQ-A`.
/// Recomputing it through `decision::fill`/`head` would let a defect in the chain hashing (or in
/// the accessor's choice of record) agree with itself, which is exactly the pin that matters.
const FIXTURE_HEAD_HASH: &str = "5cd6c7ff145231c134616228dcc0a8c80ac28e9842d77b20e83f502b59bf6978";
const FIXTURE_HEAD_ENTRIES: usize = 1;

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
        // The fixtures link no test; the digest is the independently computed empty-list one,
        // so the pinned v4 canonical bytes below are not derived from the function they pin.
        tests: Vec::new(),
        evidence: req::model::Evidence::Machine,
        tests_hash: EMPTY_TESTS_DIGEST.to_string(),
        stored: req::fill::StoredState::Verified,
        rationale_source: provenance.source,
        rationale_date: provenance.date,
    }
}

fn trust(revoked: bool, valid_until: Option<&str>) -> TrustFile {
    trust_dates(revoked.then_some("2026-10-01"), valid_until)
}

/// The trust anchor with an explicit revocation date, so a prospective revocation (ADR 0016)
/// can be asserted rather than assumed: revoked *after* the signing time still verifies.
fn trust_dates(revoked_at: Option<&str>, valid_until: Option<&str>) -> TrustFile {
    let revoke_line = match revoked_at {
        Some(date) => format!("    revoked_at: \"{date}\"\n"),
        None => String::new(),
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
/// covers. Attested, because a v3 envelope carries its signing time and authority.
fn manifest_declaring(rules_v: u32, rows: &[Row], selection: &Context) -> Manifest {
    let mut manifest = manifest(rows, rules_v, selection);
    attest(&mut manifest, FIXTURE_SIGNED_AT, &fixture_head())
        .expect("the fixture signing time is a valid attestation");
    from_json(&canonical_bytes(&manifest))
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
    let mut manifest = manifest(&rows, RULES_V, &context);
    attest(&mut manifest, FIXTURE_SIGNED_AT, &fixture_head())
        .expect("the fixture signing time is a valid attestation");
    (rows, manifest)
}

/// [`req::baseline::verify`] with the fixture authority log — for the tests that do not exercise
/// the authority itself. Falsification case 1 drives the real function with an edited log, and
/// case 2 with none.
fn verify(
    rows: &[Row],
    manifest: &Manifest,
    signature: Option<&str>,
    trust: &TrustFile,
    key_id: &str,
    verifier: &dyn SignatureVerifier,
    now: &str,
) -> Outcome {
    let log = fixture_log(FIXTURE_DECISION_NOTE);
    req::baseline::verify(
        rows,
        manifest,
        &Verification {
            signature,
            trust,
            key_id,
            verifier,
            now,
            log: Ok(&log),
        },
    )
}

#[test]
fn canonical_bytes_do_not_depend_on_input_order() {
    let mut context = Context::new();
    context.insert("b".to_string(), "2".to_string());
    context.insert("a".to_string(), "1".to_string());
    let mut forward = manifest(
        &[
            row("REQ-A", "aaa", Applies::Yes),
            row("REQ-B", "bbb", Applies::Yes),
        ],
        5,
        &context,
    );
    let mut backward = manifest(
        &[
            row("REQ-B", "bbb", Applies::Yes),
            row("REQ-A", "aaa", Applies::Yes),
        ],
        5,
        &context,
    );
    attest(&mut forward, FIXTURE_SIGNED_AT, &fixture_head())
        .expect("the fixture signing time is a valid attestation");
    attest(&mut backward, FIXTURE_SIGNED_AT, &fixture_head())
        .expect("the fixture signing time is a valid attestation");
    assert_eq!(canonical_bytes(&forward), canonical_bytes(&backward));
    // Selection keys sorted, non-applying claims absent, one trailing newline, and the
    // fixture's real digests in the bytes — all interpolated from the named oracles at the top
    // of this file, so the pin has one home rather than a hand-copy per claim. The key order is
    // ADR 0016's, extended by ADR 0019: version, signed_at, authority, rules_v, selection,
    // claims, with `tests_hash` last in each claim record.
    let head = fixture_head();
    assert_eq!(
        head.hash, FIXTURE_HEAD_HASH,
        "the fixture head is the independently computed hash"
    );
    assert_eq!(head.entries, FIXTURE_HEAD_ENTRIES);
    assert_eq!(
        canonical_bytes(&forward),
        format!(
            "{{\"version\":4,\"signed_at\":\"{FIXTURE_SIGNED_AT}\",\
             \"authority\":{{\"head\":\"{}\",\"entries\":{}}},\"rules_v\":5,\
             \"selection\":{{\"a\":\"1\",\"b\":\"2\"}},\
             \"claims\":[{{\"claim_id\":\"REQ-A\",\"address\":\"aaa\",\
             \"relations_hash\":\"{EMPTY_RELATIONS_DIGEST}\",\
             \"rationale_hash\":\"{FIXTURE_RATIONALE_DIGEST}\",\
             \"tests_hash\":\"{EMPTY_TESTS_DIGEST}\"}},\
             {{\"claim_id\":\"REQ-B\",\"address\":\"bbb\",\
             \"relations_hash\":\"{EMPTY_RELATIONS_DIGEST}\",\
             \"rationale_hash\":\"{FIXTURE_RATIONALE_DIGEST}\",\
             \"tests_hash\":\"{EMPTY_TESTS_DIGEST}\"}}]}}\n",
            FIXTURE_HEAD_HASH, FIXTURE_HEAD_ENTRIES
        ),
        "the exact bytes are the stability pin: `canonical_bytes` is pure, so re-calling it \
         with the same argument would assert nothing"
    );
}

/// A v2 manifest keeps its historical byte shape — no `signed_at`/`authority` — so an old
/// signature still verifies over the bytes it was made over, and the stale envelope is reported
/// as *predates* rather than as a signature failure.
#[test]
fn a_v2_manifest_keeps_its_legacy_bytes_and_still_parses() {
    let text = format!(
        "{{\"version\":2,\"rules_v\":5,\"selection\":{{}},\"claims\":[{{\"claim_id\":\"REQ-A\",\
         \"address\":\"aaa\",\"relations_hash\":\"{EMPTY_RELATIONS_DIGEST}\",\
         \"rationale_hash\":\"{FIXTURE_RATIONALE_DIGEST}\"}}]}}\n"
    );
    let parsed = from_json(&text).expect("a v2 manifest still parses");
    assert_eq!(parsed.version, 2);
    assert!(parsed.signed_at.is_none() && parsed.authority.is_none());
    assert_eq!(
        canonical_bytes(&parsed),
        text,
        "the v2 bytes are the ones an old signature covers"
    );
}

/// A **v3** manifest is not a stale envelope under a v4 build: its historical signed bytes are
/// unchanged, its claims keep their four-key records (no `tests_hash`), and it still verifies.
/// What it cannot do is attest the test links — so they are **not judged** there, which is the
/// difference between "no link" (a finding) and "not signed" (a disclosure). This is the
/// compatibility half of ADR 0019 and the reason `BASELINE_VERSION` and the support floor are
/// two constants.
#[test]
fn a_v3_manifest_keeps_its_historical_bytes_and_still_verifies() {
    let (rows, v4) = signed_manifest();
    let mut v3 = v4.clone();
    v3.version = 3;
    for claim in &mut v3.claims {
        claim.tests_hash = None;
    }
    let bytes = canonical_bytes(&v3);
    assert!(
        !bytes.contains("tests_hash"),
        "v3 claim records stay four-key"
    );
    assert!(bytes.contains("\"version\":3"), "{bytes}");
    let parsed = from_json(&bytes).expect("a v3 manifest still parses");
    assert_eq!(parsed.version, 3);
    assert_eq!(
        canonical_bytes(&parsed),
        bytes,
        "the v3 bytes are the signed ones"
    );

    let outcome = verify(
        &rows,
        &parsed,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(
        outcome.exit_code(),
        0,
        "a v3 Baseline still verifies: {outcome:?}"
    );

    // And the links are not judged: changing a claim's `tests:` list under a v3 manifest does
    // not turn a verified Baseline into drift — it was never attested.
    let mut relinked: Vec<Row> = rows.clone();
    relinked[0].tests = vec!["req::some::test".to_string()];
    relinked[0].tests_hash = req::model::tests_digest(&relinked[0].tests, relinked[0].evidence);
    let unaware = verify(
        &relinked,
        &parsed,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(
        unaware.exit_code(),
        0,
        "a v3 manifest did not attest the links, so it cannot judge them: {unaware:?}"
    );

    // The direction the guards exist for: a **v3 file that happens to carry** `tests_hash`.
    // `canonical_bytes` must still omit it (or every historical signature stops verifying) and
    // `drift` must not judge it (or an unsigned field invents a tampering).
    let mut carrying = parsed.clone();
    carrying.claims[0].tests_hash = Some(EMPTY_TESTS_DIGEST.to_string());
    assert_eq!(
        canonical_bytes(&carrying),
        bytes,
        "a v3 manifest's signed bytes do not gain `tests_hash` just because the record carries one"
    );
    let carrying_outcome = verify(
        &relinked,
        &carrying,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(
        carrying_outcome.exit_code(),
        0,
        "a v3 manifest's `tests_hash` is not attested, so it cannot be judged: {carrying_outcome:?}"
    );
}

/// The v4 half: the manifest attests the tests digest for every claim, a v4 header without one
/// is corrupt, and a `tests:` change after signing is FAIL — the same class as a changed
/// address, because it is a changed record of what was ratified. The address does not move
/// (ADR 0019's ruling), which is exactly why the digest must be inside the signed bytes.
#[test]
fn a_v4_manifest_attests_the_tests_hash_and_a_change_is_a_failure() {
    let (rows, manifest) = signed_manifest();
    assert_eq!(manifest.version, BASELINE_VERSION);
    assert!(
        manifest
            .claims
            .iter()
            .all(|claim| claim.tests_hash.is_some()),
        "a v4 manifest attests the tests digest for every claim"
    );

    // A v4 declaration whose claim records lack `tests_hash` is corrupt, not a v3 envelope.
    let mut stripped = manifest.clone();
    for claim in &mut stripped.claims {
        claim.tests_hash = None;
    }
    let err = from_json(&canonical_bytes(&stripped))
        .expect_err("a v4 header without the attested tests digest is unreadable");
    assert!(err.contains("tests_hash"), "{err}");
    assert!(err.contains("re-sign"), "{err}");

    // The signature is not re-checked here: the stub validates, and the point is drift.
    let clean = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(clean.exit_code(), 0, "{clean:?}");

    // Re-link one claim while leaving its identity alone: the address is untouched (that is the
    // ruling), so only the attested tests digest can catch the edit.
    let mut relinked: Vec<Row> = rows.clone();
    relinked[0].tests = vec!["req::some::test".to_string()];
    relinked[0].tests_hash = req::model::tests_digest(&relinked[0].tests, relinked[0].evidence);
    assert_eq!(
        relinked[0].address, rows[0].address,
        "identity must not move"
    );
    let edited = verify(
        &relinked,
        &manifest,
        Some("sig"),
        &trust(false, None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(edited.exit_code(), 1, "{edited:?}");
    assert!(
        edited.reasons().join(" ").contains("authored test links"),
        "{edited:?}"
    );
}

/// `signed_at` is fed to `TrustFile::state`, whose comparisons are lexicographic on the date; a
/// present-but-unreadable value (an empty string, a different format, an impossible date) would
/// sort below every trust date and skip both the expiry and the revocation check, reading a
/// revoked key as trusted. The parse choke point rejects it, so neither the library nor the CLI
/// can be handed one.
#[test]
fn a_v3_manifest_with_an_unreadable_signed_at_is_rejected() {
    for bad in [
        "",
        "07/10/2026",
        "2026-10-07 09:00:00",
        "2026-02-31T00:00:00Z",
    ] {
        let text = format!(
            "{{\"version\":3,\"signed_at\":\"{bad}\",\
             \"authority\":{{\"head\":\"{FIXTURE_HEAD_HASH}\",\"entries\":1}},\
             \"rules_v\":5,\"selection\":{{}},\"claims\":[]}}"
        );
        let err = from_json(&text).expect_err("an unreadable signed_at is not a manifest");
        assert!(
            err.contains("not an RFC 3339 UTC timestamp"),
            "{bad:?}: {err}"
        );
        assert!(!err.contains("predates"), "{bad:?}: {err}");
    }
}

/// The **writer's** half of the same choke point, which used to be missing: `attest` stored its
/// argument verbatim, so a library caller (the CLI validates `--at`; the library did not) could
/// put a `signed_at` inside the signed bytes that the only reader of a v3 manifest rejects — the
/// failure then surfaced at verification as a parse error pointing at nothing. A refused call
/// writes nothing, so the manifest stays unattested instead of half-filled.
#[test]
fn attest_refuses_a_signing_time_the_reader_would_reject() {
    let mut manifest = manifest(&[], RULES_V, &Context::new());
    let head = fixture_head();
    for bad in [
        "",
        "07/10/2026",
        "2026-10-07 09:00:00",
        "2026-02-31T00:00:00Z",
    ] {
        let err = attest(&mut manifest, bad, &head)
            .expect_err("a spelling the reader rejects is not attestable");
        assert!(
            err.contains("not an RFC 3339 UTC timestamp"),
            "{bad:?}: {err}"
        );
        assert!(
            manifest.signed_at.is_none() && manifest.authority.is_none(),
            "a refused attestation writes nothing: {bad:?}"
        );
    }
    attest(&mut manifest, FIXTURE_SIGNED_AT, &head).expect("a strict timestamp is attestable");
    assert_eq!(manifest.signed_at.as_deref(), Some(FIXTURE_SIGNED_AT));
    // The two halves agree: what `attest` accepts, `from_json` reads.
    from_json(&canonical_bytes(&manifest)).expect("an attested manifest is readable");
}

/// A v3 declaration without either attestation is corrupt, not stale, and the message says what
/// a v3 envelope carries and how to produce one.
#[test]
fn a_v3_manifest_missing_its_attestation_is_rejected_with_the_remedy() {
    for missing in ["signed_at", "authority", "both"] {
        let signed = missing != "signed_at" && missing != "both";
        let authority = missing != "authority" && missing != "both";
        let mut fields = String::new();
        if signed {
            fields.push_str(&format!("\"signed_at\":\"{FIXTURE_SIGNED_AT}\","));
        }
        if authority {
            fields.push_str(&format!(
                "\"authority\":{{\"head\":\"{}\",\"entries\":1}},",
                fixture_head().hash
            ));
        }
        let text =
            format!("{{\"version\":3,{fields}\"rules_v\":5,\"selection\":{{}},\"claims\":[]}}");
        let err = from_json(&text).expect_err("a v3 manifest must attest both fields");
        assert!(err.contains("does not attest"), "{missing}: {err}");
        assert!(
            err.contains("--at") && err.contains("re-sign"),
            "{missing}: {err}"
        );
        assert!(!err.contains("predates"), "{missing}: {err}");
    }
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
    assert!(
        reasons.contains("authored test links"),
        "the attested surface is named, including the evidence added by ADR 0019: {reasons}"
    );
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
    assert!(
        reasons.contains("authored test links"),
        "the attested surface is named, including the evidence added by ADR 0019: {reasons}"
    );
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
    let err = from_json(&format!(
        "{{\"version\":3,\"signed_at\":\"{FIXTURE_SIGNED_AT}\",\
         \"authority\":{{\"head\":\"{}\",\"entries\":1}},\"rules_v\":5,\"selection\":{{}},\
         \"claims\":[{{\"address\":\"aaa\",\"relations_hash\":\"r\",\"rationale_hash\":\"q\"}}]}}",
        fixture_head().hash
    ))
    .expect_err("a current-version manifest without claim_id is unreadable");
    // Only the code-owned parts of the message are asserted: the serde detail is forwarded
    // verbatim but is serde's text, not a contract — the same reason the code does not branch
    // on it.
    assert!(err.contains("manifest is not readable"), "{err}");
    assert!(!err.contains("re-issue"), "{err}");
    assert!(!err.contains("ADR 0013"), "{err}");
}

/// A current-version manifest that lost exactly one attested hash — a bad merge dropping the
/// `rationale_hash` line while keeping `relations_hash` — is corrupt, not stale: it declares
/// the current version, so it gets the plain unreadable message rather than re-issue advice.
/// A stale manifest is missing *both* hashes and serde names only the first, so the field name
/// alone cannot tell them apart; the declared envelope version can.
#[test]
fn a_current_manifest_missing_one_attested_hash_is_corrupt_not_stale() {
    let err = from_json(&format!(
        "{{\"version\":3,\"signed_at\":\"{FIXTURE_SIGNED_AT}\",\
         \"authority\":{{\"head\":\"{}\",\"entries\":1}},\"rules_v\":5,\"selection\":{{}},\
         \"claims\":[{{\"claim_id\":\"REQ-A\",\"address\":\"aaa\",\"relations_hash\":\"r\"}}]}}",
        fixture_head().hash
    ))
    .expect_err("a current manifest missing rationale_hash is unreadable");
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

    // ...and the stale direction keeps the advice: an envelope below the support floor is
    // exactly the case ADR 0013's re-issue is for. `BASELINE_VERSION - 1` (v3) is now
    // *supported*, so the fixture must step one below `OLDEST_VERIFIABLE_BASELINE_VERSION`.
    manifest.version = OLDEST_VERIFIABLE_BASELINE_VERSION - 1;
    let stale = verify_at(&manifest);
    assert_eq!(stale.exit_code(), 3, "{stale:?}");
    assert!(stale.reasons().join(" ").contains("re-issue"));
}

// --- ADR 0016's falsification, one test per case ---------------------------------------------
//
// The five cases are the acceptance criterion (ADR 0016 "Falsification"). Cases 1, 3 and 4 are
// library-level because they need an explicit log, trust file and signing time — the parameters
// `verify` already takes, so no maintainer key and no new seam. Case 5 and the emission refusals
// are the CLI exit classes, measured through the compiled binary.

/// Case 1 — editing a Decision after signing. The stored chain hash no longer recomputes, so
/// the log is TAMPERED and the manifest's authority is a definite finding: FAIL, never "cannot
/// tell". The second half edits and **re-fills** the log, which changes its head: the manifest
/// still records the old one, so the head comparison itself fails too.
#[test]
fn editing_a_decision_after_signing_fails_on_the_authority() {
    let (rows, manifest) = signed_manifest();
    let mut edited = fixture_log(FIXTURE_DECISION_NOTE);
    edited[0].note = "rewritten after signing".to_string();
    let tampered = req::baseline::verify(
        &rows,
        &manifest,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Ok(&edited),
        },
    );
    assert_eq!(tampered.exit_code(), 1, "{tampered:?}");
    assert!(
        tampered.reasons().join(" ").contains("authority"),
        "the reason names the authority, not the claims: {tampered:?}"
    );

    let refilled = fixture_log("a different note, re-filled");
    let mismatch = req::baseline::verify(
        &rows,
        &manifest,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Ok(&refilled),
        },
    );
    assert_eq!(mismatch.exit_code(), 1, "{mismatch:?}");
    let reasons = mismatch.reasons().join(" ");
    assert!(reasons.contains("authority mismatch"), "{reasons}");
    assert!(
        !reasons.contains("no longer recomputes"),
        "no claim is blamed for an authority mismatch: {reasons}"
    );
}

/// Case 2 — deleting (or losing) the log. An absent log is "cannot tell", never "tampered": a
/// definite finding requires evidence, and absence is not evidence of an edit.
#[test]
fn an_absent_decision_log_is_unverifiable_never_tampered() {
    let (rows, manifest) = signed_manifest();
    let outcome = req::baseline::verify(
        &rows,
        &manifest,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Err("decisions/ is absent"),
        },
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("cannot be read"), "{reasons}");
    // The loader's own diagnosis travels: "missing", "parse error" and "unfilled" have
    // different remedies, so a generic "cannot be read" would hide the next step.
    assert!(reasons.contains("decisions/ is absent"), "{reasons}");
    assert!(!reasons.contains("edited"), "{reasons}");
    assert!(!reasons.contains("does not recompute"), "{reasons}");
}

/// Case 3 — expiry is judged at the signing time. A key valid until 2026-10-31 was in good
/// standing when the Baseline was signed on 2026-10-07, so it verifies even though today is
/// later; the pre-ADR-0016 answer ("no trusted signing time exists") must not be returned.
#[test]
fn an_expiry_after_the_signing_time_verifies_even_when_today_is_later() {
    let (rows, manifest) = signed_manifest();
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust_dates(None, Some("2026-10-31")),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(outcome.exit_code(), 0, "{outcome:?}");

    // The day of expiry itself is still inside the window ("at or after signed_at").
    let on_the_day = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust_dates(None, Some("2026-10-07")),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(on_the_day.exit_code(), 0, "{on_the_day:?}");
}

/// Case 4 — revocation is prospective. A key revoked on 2026-11-01 was trusted on 2026-10-07, so
/// the Baseline signed then still verifies; a revocation on or before the signing time still
/// fails, which is the direction ADR 0010 already had.
#[test]
fn a_key_revoked_after_the_signing_time_still_verifies() {
    let (rows, manifest) = signed_manifest();
    let later = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust_dates(Some("2026-11-01"), None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(later.exit_code(), 0, "{later:?}");

    let before = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust_dates(Some("2026-10-01"), None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(before.exit_code(), 1, "{before:?}");

    // ...and `--now` is an override of the judgement time, not a way to dodge it: judged after
    // the revocation, the same key FAILS.
    let after = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust_dates(Some("2026-11-01"), None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-11-15",
    );
    assert_eq!(after.exit_code(), 1, "{after:?}");

    // A timestamp-valued trust entry is reduced to its date, so it fires on its own day: the
    // key was revoked the same day the Baseline was signed.
    let same_day = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust_dates(Some("2026-10-07T12:00:00Z"), None),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(same_day.exit_code(), 1, "{same_day:?}");
}

/// #41's trust residue: a revocation is a definite finding only when this build can judge the
/// time the key is judged at. A v2 manifest is judged at today and a v3 one at its attested
/// `signed_at`, so both keep the documented FAIL; an envelope **newer** than this build's records
/// a signing time this build does not validate (the CLI judges it at today instead), so the same
/// revocation must stay UNVERIFIABLE — the version dispatch has already answered that way, and a
/// FAIL would outrank it and misdiagnose a newer envelope as a trust failure.
///
/// Gating the arm on `==` instead of `<=` would fail this test through the v2 half: the older
/// envelope would become "cannot be decided".
#[test]
fn a_revocation_this_build_cannot_judge_is_unverifiable_never_fail() {
    let (rows, manifest) = signed_manifest();
    // Revoked long before every judgement time used below.
    let revoked = trust_dates(Some("2020-01-01"), None);

    // v3: judged at the attested signing time, so the revocation is a definite finding.
    let v3 = verify(
        &rows,
        &manifest,
        Some("sig"),
        &revoked,
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(v3.exit_code(), 1, "{v3:?}");
    assert!(
        v3.reasons().join(" ").contains("revoked at 2020-01-01"),
        "{v3:?}"
    );

    // v2: judged at today, which is also after the revocation — still FAIL, as documented.
    let mut v2 = manifest.clone();
    v2.version = 2;
    v2.signed_at = None;
    v2.authority = None;
    let v2_outcome = verify(
        &rows,
        &v2,
        Some("sig"),
        &revoked,
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-21",
    );
    assert_eq!(v2_outcome.exit_code(), 1, "{v2_outcome:?}");
    assert!(
        v2_outcome
            .reasons()
            .join(" ")
            .contains("revoked at 2020-01-01"),
        "{v2_outcome:?}"
    );

    // A newer envelope: the same revocation is not this build's to turn into a definite FAIL.
    let mut newer = manifest.clone();
    newer.version = BASELINE_VERSION + 1;
    let outcome = verify(
        &rows,
        &newer,
        Some("sig"),
        &revoked,
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-21",
    );
    assert_eq!(
        outcome.exit_code(),
        3,
        "a newer envelope's trust verdict is UNVERIFIABLE: {outcome:?}"
    );
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("newer envelope"), "{reasons}");
    assert!(reasons.contains("cannot judge"), "{reasons}");
    assert!(
        reasons.contains("revocation cannot be decided here"),
        "the reason must name the undecidable judgement, not just the revocation: {reasons}"
    );

    // ...but a **broken anchor** is not a judgement about a time: it fails closed on every
    // envelope, a newer one included. Demoting it would print the state's own "treated as
    // revoked, never as trusted" and then "cannot be decided" in the same line, and the remedy
    // ("verify with a newer build") would send the reader away from the actual fix, the anchor.
    let mut broken = trust(false, None);
    broken.keys[0].revoked_at = Some("2026-13-01".to_string());
    let broken_outcome = verify(
        &rows,
        &newer,
        Some("sig"),
        &broken,
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-21",
    );
    assert_eq!(
        broken_outcome.exit_code(),
        1,
        "a time-independent anchor defect is FAIL on any envelope: {broken_outcome:?}"
    );
    let reasons = broken_outcome.reasons().join(" ");
    assert!(reasons.contains("not an ISO date"), "{reasons}");
    assert!(
        reasons.contains("the trust anchor itself cannot be read"),
        "{reasons}"
    );
    assert!(
        !reasons.contains("cannot judge"),
        "a broken anchor is decidable: {reasons}"
    );
}

/// A malformed anchor spelling fails **closed**. `date_of` truncates blindly, so a typo'd
/// month or a human date would sort below every judgement time and read a revoked key as
/// trusted; both bounds refuse rather than pass.
#[test]
fn a_malformed_trust_date_fails_closed() {
    let (rows, manifest) = signed_manifest();
    let mut revoked = trust(false, None);
    revoked.keys[0].revoked_at = Some("2026-13-01".to_string());
    let revoked_outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &revoked,
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(revoked_outcome.exit_code(), 1, "{revoked_outcome:?}");
    assert!(
        revoked_outcome
            .reasons()
            .join(" ")
            .contains("not an ISO date"),
        "{revoked_outcome:?}"
    );

    let mut expired = trust(false, None);
    expired.keys[0].valid_until = Some("Oct 20 2026".to_string());
    let expired_outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &expired,
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(expired_outcome.exit_code(), 1, "{expired_outcome:?}");
    assert!(
        expired_outcome
            .reasons()
            .join(" ")
            .contains("not an ISO date"),
        "{expired_outcome:?}"
    );
}

/// The committed Decision record, parsed by the test rather than through `decision::head`, so
/// the expected head is the file's own `hash:` value and the expected count is the number of
/// `DEC-*.yml` files — an oracle the implementation cannot satisfy by agreeing with itself.
fn committed_decision() -> Decision {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("decisions");
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("the decisions directory reads")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("DEC-") && name.ends_with(".yml"))
        })
        .collect();
    // Zero-padded ids make lexicographic order chronological; emission attests the **last**
    // record, so that is the record this oracle must name — pinning DEC-0001 would break the
    // moment a second decision is legitimately recorded.
    paths.sort();
    let path = paths.last().expect("the repo has a decision log");
    let text = std::fs::read_to_string(path).expect("the committed decision reads");
    yaml_serde::from_str(&text).expect("the committed decision parses")
}

fn committed_decision_count() -> usize {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("decisions");
    std::fs::read_dir(dir)
        .expect("the decisions directory reads")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("DEC-") && name.ends_with(".yml"))
        })
        .count()
}

/// The emission path attests the chain head and the signer's timestamp, end to end through the
/// CLI: the manifest's `signed_at` is the one supplied, and its `authority` is the log's head —
/// pinned against `DEC-0001.yml`'s own stored hash and the file count, not against `head()`.
#[test]
fn emitting_attests_the_chain_head_and_the_signing_time() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new(req_bin())
        .args(["baseline", "--at", FIXTURE_SIGNED_AT])
        .current_dir(&root)
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    let parsed = from_json(&stdout).expect("the emitted manifest parses");
    assert_eq!(parsed.version, BASELINE_VERSION);
    assert_eq!(parsed.signed_at.as_deref(), Some(FIXTURE_SIGNED_AT));
    let authority = parsed.authority.as_ref().expect("the head is attested");
    let record = committed_decision();
    assert_eq!(
        authority.head,
        record.hash.clone().expect("the committed record is filled"),
        "the attested head is DEC-0001's own stored hash"
    );
    assert_eq!(
        authority.entries,
        committed_decision_count(),
        "the attested entry count is the number of DEC-*.yml files"
    );
}

/// A head that matches but a count that does not is also an authority mismatch: `entries` is
/// what distinguishes "one entry" from "a log rebuilt to one entry with the same head". Without
/// this, deleting the comparison in `verify` would leave the suite green.
#[test]
fn an_authority_with_the_right_head_but_the_wrong_entry_count_fails() {
    let (rows, mut manifest) = signed_manifest();
    manifest.authority.as_mut().expect("attested").entries += 1;
    let log = fixture_log(FIXTURE_DECISION_NOTE);
    let outcome = req::baseline::verify(
        &rows,
        &manifest,
        &Verification {
            signature: Some("sig"),
            trust: &trust(false, None),
            key_id: "req-baseline-1",
            verifier: &Stub(SignatureOutcome::Valid),
            now: FIXTURE_SIGNED_AT,
            log: Ok(&log),
        },
    );
    assert_eq!(outcome.exit_code(), 1, "{outcome:?}");
    let reasons = outcome.reasons().join(" ");
    assert!(reasons.contains("entries"), "{reasons}");
}

/// Case 5 — no signing time is a usage error, never a guessed one. So is a timestamp the tool
/// cannot read, and so is `--at` on the verification path (the manifest carries its own time).
#[test]
fn a_manifest_without_a_signing_time_is_a_usage_error() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run = |args: &[&str]| {
        let output = std::process::Command::new(req_bin())
            .args(args)
            .current_dir(&root)
            .output()
            .expect("runs the compiled binary");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    let (code, stderr) = run(&["baseline"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--at"), "{stderr}");
    assert!(stderr.contains("never invents"), "{stderr}");

    let (code, stderr) = run(&["baseline", "--at", "2026-10-20 18:30:00"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("RFC 3339"), "{stderr}");

    // A calendar-impossible date is not an RFC 3339 timestamp either; accepting it would put a
    // date that does not exist inside the signed bytes.
    for impossible in [
        "2026-02-31T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-13-01T00:00:00Z",
    ] {
        let (code, stderr) = run(&["baseline", "--at", impossible]);
        assert_eq!(code, Some(2), "{impossible}: {stderr}");
        assert!(stderr.contains("RFC 3339"), "{impossible}: {stderr}");
    }

    // `--now` is the judgement-time override and is validated like `--at`: an unvalidated
    // string sorts below every trust date and would read a revoked key as trusted.
    let (code, stderr) = run(&[
        "baseline",
        "--verify",
        "baselines/air-gapped.json",
        "--now",
        "",
    ]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--now"), "{stderr}");
    let (code, stderr) = run(&[
        "baseline",
        "--verify",
        "baselines/air-gapped.json",
        "--now",
        "1",
    ]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--now"), "{stderr}");

    let (code, stderr) = run(&[
        "baseline",
        "--verify",
        "baselines/air-gapped.json",
        "--at",
        FIXTURE_SIGNED_AT,
    ]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("emission flag"), "{stderr}");
}

/// A head that attests nothing must not be signed: a freshly initialised corpus has no decision
/// log, so emission refuses (exit 3) instead of printing a manifest with an empty authority.
/// The tampered arm of the emission refusal, not only the absent-log one: an edited record whose
/// stored hash no longer recomputes must not be signed either.
#[test]
fn emitting_refuses_a_tampered_decision_log() {
    let fixture = Fixture::new("cli-emit-tampered-decisions");
    let init = std::process::Command::new(req_bin())
        .arg("init")
        .current_dir(fixture.path())
        .output()
        .expect("runs the compiled binary");
    assert_eq!(init.status.code(), Some(0), "{init:?}");

    // Build the tampered record from the *parsed* committed record and re-serialize it, so the
    // tamper exists whatever the record's wording is; the stored hash is carried over unchanged,
    // which is exactly the edit-after-recording the chain detects.
    let record = committed_decision();
    let mut tampered_record = record.clone();
    tampered_record.note = format!("{}\n(rewritten after it was recorded)", record.note);
    let quote = |value: &str| serde_json::to_string(value).expect("quotes a YAML scalar");
    let tampered = format!(
        "id: {}\nkind: {}\nclaim_id: {}\naddress: {}\nactor: {}\nrole: {}\ndate: {}\nnote: {}\nprev_hash: {}\nhash: {}\n",
        quote(&tampered_record.id),
        quote(&tampered_record.kind),
        quote(&tampered_record.claim_id),
        quote(&tampered_record.address),
        quote(&tampered_record.actor),
        quote(&tampered_record.role),
        quote(&tampered_record.date),
        quote(&tampered_record.note),
        quote(
            tampered_record
                .prev_hash
                .as_deref()
                .unwrap_or("req.genesis.v1")
        ),
        quote(
            tampered_record
                .hash
                .as_deref()
                .expect("the committed record is filled")
        ),
    );
    assert_ne!(
        tampered_record.note, record.note,
        "the tamper changes the content the stored hash covers"
    );
    std::fs::create_dir_all(fixture.path().join("decisions")).expect("creates decisions/");
    std::fs::write(
        fixture.path().join(format!("decisions/{}.yml", record.id)),
        tampered,
    )
    .expect("writes the tampered decision");

    let output = std::process::Command::new(req_bin())
        .args(["baseline", "--at", FIXTURE_SIGNED_AT])
        .current_dir(fixture.path())
        .output()
        .expect("runs the compiled binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a tampered log is a definite finding, not an environment problem: {stderr}"
    );
    assert!(stderr.contains("[FAIL]"), "{stderr}");
    assert!(stderr.contains("does not verify"), "{stderr}");
    assert!(stderr.contains("must not be signed"), "{stderr}");
}

#[test]
fn emitting_refuses_a_corpus_with_no_readable_decision_log() {
    let fixture = Fixture::new("cli-emit-no-decisions");
    let init = std::process::Command::new(req_bin())
        .arg("init")
        .current_dir(fixture.path())
        .output()
        .expect("runs the compiled binary");
    assert_eq!(init.status.code(), Some(0), "{init:?}");

    let output = std::process::Command::new(req_bin())
        .args(["baseline", "--at", FIXTURE_SIGNED_AT])
        .current_dir(fixture.path())
        .output()
        .expect("runs the compiled binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert!(stderr.contains("refusing to emit"), "{stderr}");
    assert!(stderr.contains("decision log"), "{stderr}");
}

/// ADR 0016 case 3, end to end through the CLI: the key is judged at the manifest's
/// `signed_at`, not at today. The fixture trust anchor expires *after* the fixture signing time
/// but *before* today (2026-10-21), so `[VERIFIED]` proves `run()` derived the judgement time
/// from `signed_at`; `--now` after the expiry then FAILs, proving the override is real.
/// A throwaway key (`minisign -G -W`, no passphrase) signs the fixture manifest: the
/// maintainer's key is never in a test, and no new seam is needed.
#[test]
fn a_key_expired_today_but_valid_at_signing_verifies_through_the_cli() {
    require_minisign();
    let fixture = Fixture::new("cli-v3-expiry");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (src, dst) in [
        ("claims/REQ-0001.yml", "claims/REQ-0001.yml"),
        ("aliases/subjects.yml", "aliases/subjects.yml"),
        ("decisions/DEC-0001.yml", "decisions/DEC-0001.yml"),
    ] {
        let to = fixture.path().join(dst);
        std::fs::create_dir_all(to.parent().expect("a parent")).expect("creates the directory");
        std::fs::copy(root.join(src), &to).expect("copies the fixture corpus file");
    }

    let public_key = fixture.path().join("req.pub");
    let secret_key = fixture.path().join("req.key");
    let generated = std::process::Command::new("minisign")
        .args(["-G", "-W", "-f", "-p"])
        .arg(&public_key)
        .arg("-s")
        .arg(&secret_key)
        .output()
        .expect("runs minisign");
    assert!(
        generated.status.success(),
        "throwaway key generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let manifest_path = fixture.path().join("baseline.json");
    let emitted = std::process::Command::new(req_bin())
        .arg("--root")
        .arg(fixture.path())
        .args(["baseline", "--at", FIXTURE_SIGNED_AT])
        .output()
        .expect("runs the compiled binary");
    assert_eq!(
        emitted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    std::fs::write(&manifest_path, &emitted.stdout).expect("writes the emitted manifest");
    let signed = std::process::Command::new("minisign")
        .args(["-Sm"])
        .arg(&manifest_path)
        .arg("-s")
        .arg(&secret_key)
        .output()
        .expect("runs minisign");
    assert!(
        signed.status.success(),
        "fixture signing failed: {}",
        String::from_utf8_lossy(&signed.stderr)
    );

    let key_text = std::fs::read_to_string(&public_key).expect("reads the fixture public key");
    let indented = key_text
        .lines()
        .map(|line| format!("      {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let trust_dir = fixture.path().join("baselines");
    std::fs::create_dir_all(&trust_dir).expect("creates baselines/");
    std::fs::write(
        trust_dir.join("trust.yml"),
        format!(
            "keys:\n  - id: \"req-baseline-1\"\n    scheme: \"minisign\"\n    public_key: |\n{indented}\n    valid_from: \"2026-01-01\"\n    valid_until: \"2026-10-10\"\n    revoked_at: \"2098-01-01\"\n"
        ),
    )
    .expect("writes the fixture trust anchor");

    let run = |now: Option<&str>| {
        let mut command = std::process::Command::new(req_bin());
        command
            .arg("--root")
            .arg(fixture.path())
            .args(["baseline", "--verify"])
            .arg(&manifest_path);
        if let Some(now) = now {
            command.args(["--now", now]);
        }
        let output = command.output().expect("runs the compiled binary");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    };

    // Judged at signed_at (2026-10-07), before valid_until 2026-10-10: verified.
    let (code, stdout) = run(None);
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("[VERIFIED]"), "{stdout}");

    // Judged at --now (2026-10-21), after the expiry: the same Baseline FAILS.
    let (code, stdout) = run(Some("2026-10-21"));
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("[FAIL]"), "{stdout}");
    assert!(
        stdout.contains("outside the key's validity window"),
        "{stdout}"
    );

    // The signature covers `signed_at` and `authority`. Change only `signed_at`, to another
    // timestamp inside the trust window, so trust and the authority check still pass and the
    // only reason left for a failure is the signature — the test that would catch a verifier
    // re-canonicalizing a reduced (v2) shape.
    let original = std::fs::read_to_string(&manifest_path).expect("reads the emitted manifest");
    let tampered_text = original.replacen(
        &format!("\"signed_at\":\"{FIXTURE_SIGNED_AT}\""),
        "\"signed_at\":\"2026-10-08T09:00:00Z\"",
        1,
    );
    assert_ne!(
        tampered_text, original,
        "the emitted manifest carries the fixture signed_at"
    );
    let tampered_path = fixture.path().join("tampered.json");
    std::fs::write(&tampered_path, tampered_text).expect("writes the tampered manifest");
    std::fs::copy(
        fixture.path().join("baseline.json.minisig"),
        fixture.path().join("tampered.json.minisig"),
    )
    .expect("copies the signature beside the tampered manifest");
    let output = std::process::Command::new(req_bin())
        .arg("--root")
        .arg(fixture.path())
        .args(["baseline", "--verify"])
        .arg(&tampered_path)
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "an edit inside the signed bytes is FAIL: {stdout}"
    );
    assert!(stdout.contains("[FAIL]"), "{stdout}");
    assert!(stdout.contains("signature does not verify"), "{stdout}");

    // A *newer* envelope must not be judged at a `signed_at` this build never validated. If it
    // were, a far-future value (or an RFC 3339 offset form a future envelope may use) could make
    // the trust comparison produce a definite FAIL for a manifest this build explicitly says it
    // cannot judge. The horizons are deliberately out of the clock's reach: a wrong verifier
    // judging at `signed_at` (9999) trips the 2098 revocation, while the correct one judges at
    // today and reports only the version.
    let versioned = std::fs::read_to_string(&manifest_path)
        .expect("reads the emitted manifest")
        .replacen("\"version\":4", "\"version\":5", 1)
        .replacen(
            &format!("\"signed_at\":\"{FIXTURE_SIGNED_AT}\""),
            "\"signed_at\":\"9999-01-01T00:00:00Z\"",
            1,
        );
    let newer_path = fixture.path().join("newer.json");
    std::fs::write(&newer_path, versioned).expect("writes the newer-envelope manifest");
    let output = std::process::Command::new(req_bin())
        .arg("--root")
        .arg(fixture.path())
        .args(["baseline", "--verify"])
        .arg(&newer_path)
        .output()
        .expect("runs the compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(3),
        "a newer envelope is UNVERIFIABLE, never FAIL on an unvalidated time: {stdout}"
    );
    assert!(stdout.contains("newer"), "{stdout}");
    assert!(
        !stdout.contains("revoked"),
        "the unvalidated signed_at must not judge the key: {stdout}"
    );
}

/// ADR 0016 falsification case 2 through the **real loader**, not the branch: the synthetic test
/// above hands `verify` an `Err("decisions/ is absent")` directly, which pins the verdict but not
/// the path the CLI takes. An absent `decisions/` directory is deliberately the **empty-log** case
/// (`log_paths` answers an empty file set for a missing directory; pinned by
/// `tests/decision.rs::the_gate_blocks_an_empty_decision_log_without_a_skip`), so what travels
/// here is the loader's own diagnosis — "the decision log is empty: nothing is recorded" — and the
/// verdict must stay UNVERIFIABLE: absence is not tampering.
///
/// A real throwaway key signs the fixture, so the *only* reason left after the log is removed is
/// the absent authority. An unsigned fixture would answer UNVERIFIABLE for the missing signature
/// as well, and the test could not tell the two apart.
#[test]
fn an_absent_decision_log_is_unverifiable_through_the_cli() {
    require_minisign();
    let fixture = Fixture::new("cli-absent-decisions");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (src, dst) in [
        ("claims/REQ-0001.yml", "claims/REQ-0001.yml"),
        ("aliases/subjects.yml", "aliases/subjects.yml"),
        ("decisions/DEC-0001.yml", "decisions/DEC-0001.yml"),
    ] {
        let to = fixture.path().join(dst);
        std::fs::create_dir_all(to.parent().expect("a parent")).expect("creates the directory");
        std::fs::copy(root.join(src), &to).expect("copies the fixture corpus file");
    }

    let public_key = fixture.path().join("req.pub");
    let secret_key = fixture.path().join("req.key");
    let generated = std::process::Command::new("minisign")
        .args(["-G", "-W", "-f", "-p"])
        .arg(&public_key)
        .arg("-s")
        .arg(&secret_key)
        .output()
        .expect("runs minisign");
    assert!(
        generated.status.success(),
        "throwaway key generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let manifest_path = fixture.path().join("baseline.json");
    let emitted = std::process::Command::new(req_bin())
        .arg("--root")
        .arg(fixture.path())
        .args(["baseline", "--at", FIXTURE_SIGNED_AT])
        .output()
        .expect("runs the compiled binary");
    assert_eq!(
        emitted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    std::fs::write(&manifest_path, &emitted.stdout).expect("writes the emitted manifest");
    let signed = std::process::Command::new("minisign")
        .args(["-Sm"])
        .arg(&manifest_path)
        .arg("-s")
        .arg(&secret_key)
        .output()
        .expect("runs minisign");
    assert!(
        signed.status.success(),
        "fixture signing failed: {}",
        String::from_utf8_lossy(&signed.stderr)
    );

    let key_text = std::fs::read_to_string(&public_key).expect("reads the fixture public key");
    let indented = key_text
        .lines()
        .map(|line| format!("      {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let trust_dir = fixture.path().join("baselines");
    std::fs::create_dir_all(&trust_dir).expect("creates baselines/");
    std::fs::write(
        trust_dir.join("trust.yml"),
        format!(
            "keys:\n  - id: \"req-baseline-1\"\n    scheme: \"minisign\"\n    public_key: |\n{indented}\n    valid_from: \"2026-01-01\"\n    revoked_at: \"2098-01-01\"\n"
        ),
    )
    .expect("writes the fixture trust anchor");

    let run = || {
        let output = std::process::Command::new(req_bin())
            .arg("--root")
            .arg(fixture.path())
            .args(["baseline", "--verify"])
            .arg(&manifest_path)
            .output()
            .expect("runs the compiled binary");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    };

    // The fixture verifies while its log is present, so the deletion is the only variable.
    let (code, stdout) = run();
    assert_eq!(
        code,
        Some(0),
        "the signed fixture must verify before the log is removed: {stdout}"
    );
    assert!(stdout.contains("[VERIFIED]"), "{stdout}");

    std::fs::remove_dir_all(fixture.path().join("decisions")).expect("removes decisions/");
    let (code, stdout) = run();
    assert_eq!(
        code,
        Some(3),
        "an absent log is UNVERIFIABLE, never FAIL: {stdout}"
    );
    assert!(stdout.contains("[UNVERIFIABLE]"), "{stdout}");
    assert!(stdout.contains("cannot be re-derived"), "{stdout}");
    // The loader's own diagnosis travels, so the remedy ("record a decision") is named rather
    // than a generic "cannot be read" that could be an unparsable file with a different remedy.
    assert!(stdout.contains("the decision log is empty"), "{stdout}");
    assert!(stdout.contains("nothing is recorded"), "{stdout}");
    // ...and no tamper vocabulary: absence is not evidence of an edit.
    for vocabulary in ["edited", "does not recompute", "authority mismatch"] {
        assert!(
            !stdout.contains(vocabulary),
            "{vocabulary:?} must not appear: {stdout}"
        );
    }
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
        // Explicit, not inherited: `baseline::run` reads the trust anchor from the resolved
        // corpus root, so a run from any other cwd would find no anchor and answer
        // UNVERIFIABLE (exit 3) — this test would pass for the wrong reason.
        .current_dir(&root)
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

/// Expiry is judged at the **signing time** for a v3 Baseline (ADR 0016): a key that had
/// already expired then was not valid when the Baseline was signed, which is a definite finding
/// — the symmetric answer to revocation at/before the signing time. A v2 Baseline has no trusted
/// signing time, so the older ADR 0010 answer — "cannot be decided" (UNVERIFIABLE) — still
/// stands for it.
#[test]
fn a_key_expired_before_the_signing_time_fails() {
    let (rows, manifest) = signed_manifest();
    let outcome = verify(
        &rows,
        &manifest,
        Some("sig"),
        &trust(false, Some("2026-09-01")),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        FIXTURE_SIGNED_AT,
    );
    assert_eq!(outcome.exit_code(), 1, "{outcome:?}");
    let reasons = outcome.reasons().join(" ");
    assert!(
        reasons.contains("outside the key's validity window"),
        "{reasons}"
    );

    // v2: no field to judge it at, so the old undecidable answer is the honest one.
    let mut v2 = manifest;
    v2.version = 2;
    let v2_outcome = verify(
        &rows,
        &v2,
        Some("sig"),
        &trust(false, Some("2026-09-01")),
        "req-baseline-1",
        &Stub(SignatureOutcome::Valid),
        "2026-10-07",
    );
    assert_eq!(v2_outcome.exit_code(), 3, "{v2_outcome:?}");
    assert!(
        v2_outcome
            .reasons()
            .join(" ")
            .contains("no trusted signing time"),
        "{v2_outcome:?}"
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
    // One copy, in `tests/common`: the same decision had a byte-identical twin in
    // `tests/decision.rs`, and a third written from scratch panicked in CI.
    common::point_home_at_toolchain(&mut command);
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
        planned.iter().any(
            |line| line.starts_with("PLAN ") && line.contains("cargo nextest run --profile ci")
        ),
        "{stdout}"
    );
    assert!(stdout.contains("GATE: UNVERIFIABLE"), "{stdout}");
}

/// `--fast` is the iteration mode: it plans strictly fewer checks, its banner cannot be quoted
/// as the gate's verdict, and where it could only be a no-op it is a usage error. Measured on the
/// maintainer's machine (2026-11-03, single runs): a warm full run is ~34s, a fast one ~20s.
#[cfg(unix)]
#[test]
fn the_fast_run_skips_the_expensive_checks_and_never_reads_as_the_gate() {
    let fast = check_sh_args(&["--fast".as_ref(), "plan".as_ref()]);
    let fast_out = String::from_utf8_lossy(&fast.stdout);
    let fast_lines: Vec<String> = fast_out
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(
        fast.status.code(),
        Some(3),
        "a plan executes nothing, so it is UNVERIFIABLE, never a pass:\n{fast_out}"
    );

    // Each dropped step is NAMED as skipped (FAST, not SKIP: the tooling is present)...
    for skipped in [
        "FAST clippy --all-targets -- -D warnings",
        "FAST the external corpora (v1-v3) and the direction gold set",
    ] {
        assert!(
            fast_lines.iter().any(|line| line.starts_with(skipped)),
            "the fast plan must name `{skipped}` as skipped:\n{fast_out}"
        );
    }
    // ...the steps that decide behaviour are still planned...
    for kept in [
        "PLAN cargo nextest run --profile ci",
        "PLAN cargo run -q -- canon",
        "PLAN cargo run -q -- bench",
        "PLAN cargo run -q -- gate",
        "PLAN cargo run -q -- render --check docs/spec.md",
    ] {
        assert!(
            fast_lines.iter().any(|line| line.starts_with(kept)),
            "the fast plan must keep `{kept}`:\n{fast_out}"
        );
    }
    // The quickstart rehearsal is kept in both modes (AGENTS.md says so, and the page's
    // "five minutes" is measured by it): the plan must show it, or a `fastskip` could drop it
    // with the claim still green.
    assert!(
        fast_lines
            .iter()
            .any(|line| line.starts_with("PLAN ") && line.contains("tools/quickstart.sh")),
        "the fast plan must keep the quickstart rehearsal:\n{fast_out}"
    );
    // ...and the banner is the fast one, so a scrollback cannot mistake it for the gate's.
    assert!(
        fast_out.contains("GATE: UNVERIFIABLE (fast plan only"),
        "a fast plan must not print the plain banner:\n{fast_out}"
    );

    // The full plan still plans what fast drops: the difference is the flag, not a changed script.
    let full = check_sh_args(&["plan".as_ref()]);
    let full_out = String::from_utf8_lossy(&full.stdout);
    let full_lines: Vec<String> = full_out
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    for planned in [
        "PLAN cargo clippy",
        "external-v1",
        "direction-v1",
        // The rehearsal is kept in both modes: this is the full plan's half of the pin, so a
        // block moved into a fast-only branch would leave the fast assertion green while the
        // full gate stopped checking the page's claim.
        "tools/quickstart.sh",
    ] {
        assert!(
            full_lines.iter().any(|line| line.contains(planned)),
            "the full plan must still plan `{planned}`:\n{full_out}"
        );
    }

    // The local review-tool checks were removed (M2-1, owner ruling 2026-11-03): the review that
    // blocks merges is CI's, and nothing in either plan may name the deleted scripts, the review
    // tool's pin report, or a FAST line for them. A step that reappears (or a stale name left in
    // a banner) fails here rather than silently costing every iteration again.
    for (label, out) in [("fast", &fast_out), ("full", &full_out)] {
        for gone in [
            "check_rules",
            "check_review_scope",
            "review-rule",
            "review-scope",
            "review tool, not a gate",
        ] {
            assert!(
                !out.contains(gone),
                "the {label} plan must not mention the removed review check `{gone}`:\n{out}"
            );
        }
    }

    // Where the flag could only be a no-op, or is unknown, it is rejected rather than ignored.
    for args in [
        ["--fast", "baseline"],
        ["--fast", "decision"],
        ["--fat", "plan"],
    ] {
        let out = check_sh_args(&args.map(std::ffi::OsStr::new));
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?} must be a usage error, not a quiet run of a different scope"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage:"),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
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

/// The release gate over this repository's own corpus. `scripts/check.sh` must run `req gate`
/// once per declared release: it was the one subcommand the gate never invoked, which is how
/// six variant-scoped claims in `claims/` went unchecked while a bare `gate` printed PASS
/// (`15 applying, 6 undecided`, measured). Pinned from the same plan output as the Baseline
/// wiring above, so a refactor that drops the step fails here instead of passing silently.
#[cfg(unix)]
#[test]
fn the_default_gate_run_gates_every_declared_release() {
    let variants = common::variant_lines();
    assert!(
        !variants.is_empty(),
        ".req/variants must declare at least one release, or the gate examines no variant"
    );

    let plan = check_sh_args(&["plan".as_ref()]);
    let stdout = String::from_utf8_lossy(&plan.stdout);
    let planned: Vec<String> = stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    for variant in &variants {
        // The whole token sequence, not a normalized tail: `ends_with` accepts a *different* context
        // whose trailing pairs happen to match, which is exactly the drift these tests exist to catch.
        // Tokens are whitespace-insensitive, so a tab or a second space between pairs is fine.
        let expected: Vec<&str> = variant.split_whitespace().collect();
        let planned_gate = |line: &str| {
            let words: Vec<&str> = line.split_whitespace().collect();
            // The gate step carries the report the test run wrote: the evidence is part of the
            // invocation, not a file the gate might find later.
            words.starts_with(&[
                "PLAN",
                "cargo",
                "run",
                "-q",
                "--",
                "gate",
                "--test-report",
                "target/nextest/ci/junit.xml",
            ]) && words.len() == 8 + expected.len()
                && words[8..] == expected[..]
        };
        assert!(
            planned.iter().any(|line| planned_gate(line)),
            "the default run must gate the declared release `{variant}`:\n{stdout}"
        );
    }
}

/// A throwaway root holding a copy of `scripts/check.sh` beside a **fake toolchain whose `cargo`
/// exits 0 and prints an empty JSON object**, so a test can drive the script to its banner on a
/// tree that is only the script: every `cargo …` step "passes", and the only thing that can fail
/// the run is the branch under test. The object is what the `--format json` smoke step parses —
/// the same reason the real binary prints one — so the fixture's cargo is a stand-in for the
/// command as well as for the toolchain. `Fixture` owns the name's uniqueness and the cleanup —
/// including on a panicking assertion.
#[cfg(unix)]
fn script_fixture(tag: &str) -> Fixture {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::unique(tag);
    std::fs::create_dir_all(fixture.path().join("scripts")).expect("creates scripts/");
    std::fs::copy(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/check.sh"),
        fixture.path().join("scripts/check.sh"),
    )
    .expect("copies check.sh");
    let bin = fixture.path().join("home/.rustup/toolchains/fake/bin");
    std::fs::create_dir_all(&bin).expect("creates the fake toolchain");
    // The stub answers the test runner two ways: `nextest --version` for the pin probe, and the
    // run itself (writes the JUnit artefact the gate step will read). `cargo-nextest` exists on
    // PATH too, so the script's "is the tooling there" probe answers yes — a SKIP would make the
    // healthy direction red for the wrong reason.
    let stub = "#!/bin/sh\n\
        if [ \"$1\" = nextest ] && [ \"$2\" = --version ]; then\n\
          echo 'cargo-nextest 0.9.149 (stub)'\n\
          exit 0\n\
        fi\n\
        if [ \"$1\" = nextest ]; then\n\
          mkdir -p target/nextest/ci\n\
          printf '<testsuites name=\"stub\" tests=\"0\" skipped=\"0\" failures=\"0\" errors=\"0\"></testsuites>\\n' > target/nextest/ci/junit.xml\n\
        fi\n\
        echo '{}'\n\
        exit 0\n";
    std::fs::write(bin.join("cargo"), stub).expect("writes the stub cargo");
    std::fs::write(bin.join("cargo-nextest"), stub).expect("writes the stub nextest");
    for stub in ["cargo", "cargo-nextest"] {
        std::fs::set_permissions(bin.join(stub), std::fs::Permissions::from_mode(0o755))
            .expect("makes the stub executable");
    }
    fixture
}

/// Run the copied `scripts/check.sh` to its banner. HOME and PATH are the fixture's, not the
/// developer's: the fake toolchain is what the script resolves, and nothing ambient (an installed
/// `ocr`, say) leaks into the run, so the fixture is the same tree on a laptop and in CI.
#[cfg(unix)]
fn script_banner(fixture: &Fixture) -> std::process::Output {
    let bin = fixture.path().join("home/.rustup/toolchains/fake/bin");
    std::process::Command::new("bash")
        .arg(fixture.path().join("scripts/check.sh"))
        .current_dir(fixture.path())
        .env("HOME", fixture.path().join("home"))
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
        )
        .output()
        .expect("runs the copied scripts/check.sh")
}

#[cfg(unix)]
#[test]
fn a_missing_variants_file_is_a_failure_not_a_skip() {
    // The branch pinned here: with `.req/variants` absent the run must end `GATE: FAIL`, never
    // `GATE: PASS` with the release gate skipped — "a skipped check is never a pass" is the invariant
    // this script exists to keep, and this is the branch whose removal would silently restore the hole
    // the release-gate step closed. It takes the banner to prove, not a message: every other step is
    // driven to pass by the fixture's stub `cargo`, so whatever flips the banner is this branch.
    let fixture = script_fixture("req-gate-no-variants");
    let missing = script_banner(&fixture);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&missing.stdout),
        String::from_utf8_lossy(&missing.stderr)
    );
    assert!(
        text.contains("FAIL  .req/variants is missing or unreadable"),
        "a missing variants file must be reported as a failure, never skipped:\n{text}"
    );
    assert!(
        text.contains("GATE: FAIL"),
        "and the run must say so in its banner:\n{text}"
    );
    assert_eq!(
        missing.status.code(),
        Some(1),
        "a failed run exits 1, not 0 and not the plan's UNVERIFIABLE:\n{text}"
    );

    // The healthy direction: the same script with the file in place ends `GATE: PASS` and gates both
    // releases, so the assertions above are the branch's doing and not the fixture's.
    std::fs::create_dir_all(fixture.path().join(".req")).expect("creates .req/");
    std::fs::copy(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".req/variants"),
        fixture.path().join(".req/variants"),
    )
    .expect("copies .req/variants");
    let healthy = script_banner(&fixture);
    let healthy_text = format!(
        "{}{}",
        String::from_utf8_lossy(&healthy.stdout),
        String::from_utf8_lossy(&healthy.stderr)
    );
    assert!(
        healthy_text.contains("GATE: PASS"),
        "a healthy tree passes:\n{healthy_text}"
    );
    assert_eq!(
        healthy.status.code(),
        Some(0),
        "a passing run exits 0 — the banner alone would let a script print PASS and exit non-zero:\n{healthy_text}"
    );
    assert!(
        !healthy_text.contains(".req/variants is missing"),
        "a healthy tree must not report a missing variants file:\n{healthy_text}"
    );
    // The count comes from the data, not from a literal: `.req/variants` is data, and the `.req/**`
    // review rule explicitly anticipates another line when a new claim needs one.
    let declared = common::variant_lines().len();
    assert_eq!(
        healthy_text
            .lines()
            .filter(|line| line.starts_with("== cargo run -q -- gate"))
            .count(),
        declared,
        "every declared release is gated ({declared} in .req/variants):\n{healthy_text}"
    );

    // The other way the step could examine nothing: a file that is *there* and declares no release.
    // Every line is a comment, so the loop iterates zero times — and a run that examined nothing must
    // not report PASS.
    std::fs::write(
        fixture.path().join(".req/variants"),
        "# nothing declared yet\n",
    )
    .expect("writes a comment-only variants file");
    let empty = script_banner(&fixture);
    let empty_text = format!(
        "{}{}",
        String::from_utf8_lossy(&empty.stdout),
        String::from_utf8_lossy(&empty.stderr)
    );
    assert!(
        empty_text.contains("declares no release"),
        "a variants file with no release must be reported, never treated as a pass:\n{empty_text}"
    );
    assert!(
        empty_text.contains("GATE: FAIL"),
        "and the run must say so in its banner:\n{empty_text}"
    );
    assert_eq!(
        empty.status.code(),
        Some(1),
        "and exit 1, so a caller keying on the status sees the failure:\n{empty_text}"
    );
}
