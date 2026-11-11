//! A Baseline signed by named parties (ADR 0042, task A-1).
//!
//! Two seams. The library one drives `verify_parties` and `decision::judge_signature` with a
//! fixture verifier that accepts a signature only when its text names the key's public key — an
//! oracle for the *combination* logic (which key counts towards which policy line), not for
//! cryptography. The CLI one drives the compiled `req` against real throwaway minisign keys, so a
//! two-party Baseline is verified end to end and offline. Every expectation is written here from
//! the ADR's *Falsification* list, never read back from the implementation.

use std::cell::RefCell;

use req::baseline::{
    KeyedSignature, Manifest, Outcome, Parties, PolicyLine, SignatureOutcome, SignatureVerifier,
    TrustFile, Verification, attest, canonical_bytes, manifest, parse_signing_policy, verify,
    verify_parties,
};
use req::canon::RULES_V;
use req::decision::{self, Attribution, Decision};
use req::select::Context;

const SIGNED_AT: &str = "2026-10-07T09:00:00Z";

fn log() -> Vec<Decision> {
    let mut log = vec![Decision {
        id: "DEC-0001".to_string(),
        kind: "accept".to_string(),
        claim_id: "REQ-A".to_string(),
        address: "aaa".to_string(),
        actor: "Dana Example".to_string(),
        role: "requirements-owner".to_string(),
        date: "2026-10-07".to_string(),
        note: "ratified".to_string(),
        prev_hash: None,
        hash: None,
    }];
    decision::fill(&mut log);
    log
}

/// A manifest of an empty selection, attested over the fixture log: nothing to drift, so the
/// verdict is the signatures' alone.
fn fixture_manifest() -> Manifest {
    let mut manifest = manifest(&[], RULES_V, &Context::new());
    let head = decision::head(&log()).expect("a filled log has a head");
    attest(&mut manifest, SIGNED_AT, &head).expect("the fixture signing time is valid");
    manifest
}

/// A trust file from `(id, party, role, revoked_at)` rows. The public key is `PK-<id>`, which the
/// fixture verifier compares against the signature text.
type Holder = Option<(&'static str, &'static str)>;

fn trust(keys: &[(&str, Holder, Option<&str>)]) -> TrustFile {
    let mut text = String::from("keys:\n");
    for (id, holder, revoked_at) in keys {
        text.push_str(&format!(
            "  - id: \"{id}\"\n    scheme: \"minisign\"\n    public_key: \"PK-{id}\"\n    valid_from: \"2026-01-01\"\n"
        ));
        if let Some((party, role)) = holder {
            text.push_str(&format!("    party: \"{party}\"\n    role: \"{role}\"\n"));
        }
        if let Some(date) = revoked_at {
            text.push_str(&format!("    revoked_at: \"{date}\"\n"));
        }
    }
    TrustFile::load(&text).expect("the fixture trust file parses")
}

/// The signature text key `id` makes: valid for exactly that key.
fn sig(id: &str) -> String {
    format!("sig-by:PK-{id}")
}

/// Accepts a signature when its text is `sig-by:<the key it is checked against>`, and records
/// the bytes it was asked to judge.
struct PerKey {
    seen: RefCell<Vec<Vec<u8>>>,
}

impl PerKey {
    fn new() -> Self {
        PerKey {
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl SignatureVerifier for PerKey {
    fn scheme(&self) -> &'static str {
        "minisign"
    }
    fn verify(&self, message: &[u8], signature: &str, public_key: &str) -> SignatureOutcome {
        self.seen.borrow_mut().push(message.to_vec());
        if signature == format!("sig-by:{public_key}") {
            SignatureOutcome::Valid
        } else {
            SignatureOutcome::Invalid("not this key's signature".to_string())
        }
    }
}

fn keyed(ids: &[&str]) -> Vec<KeyedSignature> {
    ids.iter()
        .map(|id| KeyedSignature {
            key_id: (*id).to_string(),
            signature: Ok(sig(id)),
        })
        .collect()
}

fn policy(lines: &[(&str, &str)]) -> Vec<PolicyLine> {
    lines
        .iter()
        .map(|(party, role)| PolicyLine {
            party: (*party).to_string(),
            role: (*role).to_string(),
        })
        .collect()
}

/// Judge the fixture manifest with `default` as the `.minisig` beside it (a signature by the
/// `--key` key, `req-baseline-1`), `keyed` files and `lines` of policy.
fn judge(
    trust: &TrustFile,
    verifier: &dyn SignatureVerifier,
    default: Option<&str>,
    keyed: &[KeyedSignature],
    lines: &[PolicyLine],
) -> Outcome {
    let log = log();
    verify_parties(
        &[],
        &fixture_manifest(),
        &Verification {
            signature: default,
            trust,
            key_id: "req-baseline-1",
            verifier,
            now: SIGNED_AT,
            log: Ok(&log),
        },
        &Parties {
            keyed,
            policy: lines,
        },
    )
}

const OEM: Holder = Some(("oem", "requirements-owner"));
const ACME: Holder = Some(("acme-tier1", "safety-manager"));

fn two_parties() -> TrustFile {
    trust(&[("oem-ro", OEM, None), ("acme-sm", ACME, None)])
}

fn text(outcome: &Outcome) -> String {
    outcome.reasons().join(" | ")
}

// ---- Falsification 1: verifies under a policy while a required party/role has no signature ----

#[test]
fn a_policy_line_with_no_verified_signature_is_unverifiable_and_names_the_party() {
    let lines = policy(&[
        ("oem", "requirements-owner"),
        ("acme-tier1", "safety-manager"),
    ]);
    let outcome = judge(
        &two_parties(),
        &PerKey::new(),
        None,
        &keyed(&["oem-ro"]),
        &lines,
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert!(
        text(&outcome).contains("awaiting acme-tier1 safety-manager"),
        "{}",
        text(&outcome)
    );
    assert!(
        !text(&outcome).contains("awaiting oem"),
        "the met line is not awaited: {}",
        text(&outcome)
    );
}

#[test]
fn a_policy_met_by_distinct_verified_signatures_verifies() {
    let lines = policy(&[
        ("oem", "requirements-owner"),
        ("acme-tier1", "safety-manager"),
    ]);
    let outcome = judge(
        &two_parties(),
        &PerKey::new(),
        None,
        &keyed(&["oem-ro", "acme-sm"]),
        &lines,
    );
    assert_eq!(outcome, Outcome::Verified, "{outcome:?}");
}

#[test]
fn a_verified_signature_with_the_wrong_role_does_not_meet_a_line() {
    // The signer holds `oem requirements-owner`; the line wants `oem safety-manager`.
    let lines = policy(&[("oem", "safety-manager")]);
    let outcome = judge(
        &two_parties(),
        &PerKey::new(),
        None,
        &keyed(&["oem-ro"]),
        &lines,
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert!(text(&outcome).contains("awaiting oem safety-manager"));
}

#[test]
fn a_key_without_a_party_and_role_cannot_meet_a_policy_line() {
    let anonymous = trust(&[("loose", None, None)]);
    let lines = policy(&[("oem", "requirements-owner")]);
    let outcome = judge(&anonymous, &PerKey::new(), None, &keyed(&["loose"]), &lines);
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert!(text(&outcome).contains("awaiting oem requirements-owner"));
}

#[test]
fn the_default_signature_counts_as_the_key_it_names() {
    // `<manifest>.json.minisig` is a signature by the `--key` key; when that key holds the
    // party and role, it meets the line.
    let holder = trust(&[("req-baseline-1", OEM, None)]);
    let lines = policy(&[("oem", "requirements-owner")]);
    let outcome = judge(
        &holder,
        &PerKey::new(),
        Some(&sig("req-baseline-1")),
        &[],
        &lines,
    );
    assert_eq!(outcome, Outcome::Verified, "{outcome:?}");
}

// ---- Falsification 2: one key counted twice towards a repeated line ----

#[test]
fn one_key_counts_once_towards_a_repeated_line() {
    let two_oem_keys = trust(&[("req-baseline-1", OEM, None), ("oem-b", OEM, None)]);
    let lines = policy(&[("oem", "requirements-owner"), ("oem", "requirements-owner")]);
    // The same key signs twice: as the default `.minisig` and as a keyed file.
    let outcome = judge(
        &two_oem_keys,
        &PerKey::new(),
        Some(&sig("req-baseline-1")),
        &keyed(&["req-baseline-1"]),
        &lines,
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    let awaited = text(&outcome)
        .matches("awaiting oem requirements-owner")
        .count();
    assert_eq!(awaited, 1, "one reason per unmet line: {}", text(&outcome));

    // A second, distinct key meets the second line.
    let both = judge(
        &two_oem_keys,
        &PerKey::new(),
        Some(&sig("req-baseline-1")),
        &keyed(&["oem-b"]),
        &lines,
    );
    assert_eq!(both, Outcome::Verified, "{both:?}");
}

/// Review A-1-review-1 (MEDIUM): two trust entries holding the SAME public key under different ids
/// both verify one signing act, so a doubled line was met by one key. "One key counts once" is about
/// key material, not ids.
#[test]
fn one_public_key_under_two_ids_counts_once() {
    let trust = TrustFile::load(
        "keys:\n\
         \x20 - id: \"oem-a\"\n    scheme: \"minisign\"\n    public_key: \"PK-shared\"\n    \
         valid_from: \"2026-01-01\"\n    party: \"oem\"\n    role: \"requirements-owner\"\n\
         \x20 - id: \"oem-b\"\n    scheme: \"minisign\"\n    public_key: \"untrusted comment: same key, another id\\nPK-shared\"\n    \
         valid_from: \"2026-01-01\"\n    party: \"oem\"\n    role: \"requirements-owner\"\n",
    )
    .expect("the fixture trust file parses");
    let same_act: Vec<KeyedSignature> = ["oem-a", "oem-b"]
        .iter()
        .map(|id| KeyedSignature {
            key_id: (*id).to_string(),
            signature: Ok("sig-by:PK-shared".to_string()),
        })
        .collect();
    struct Shared;
    impl SignatureVerifier for Shared {
        fn scheme(&self) -> &'static str {
            "minisign"
        }
        fn verify(&self, _: &[u8], signature: &str, public_key: &str) -> SignatureOutcome {
            if signature == "sig-by:PK-shared" && public_key.ends_with("PK-shared") {
                SignatureOutcome::Valid
            } else {
                SignatureOutcome::Invalid("not this key's signature".to_string())
            }
        }
    }
    let lines = policy(&[("oem", "requirements-owner"), ("oem", "requirements-owner")]);
    let outcome = judge(&trust, &Shared, None, &same_act, &lines);
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert_eq!(
        text(&outcome)
            .matches("awaiting oem requirements-owner")
            .count(),
        1,
        "{}",
        text(&outcome)
    );
}

#[test]
fn a_repeated_line_with_no_signature_is_awaited_once_per_line() {
    let two_oem_keys = trust(&[("oem-a", OEM, None), ("oem-b", OEM, None)]);
    let lines = policy(&[("oem", "requirements-owner"), ("oem", "requirements-owner")]);
    let outcome = judge(&two_oem_keys, &PerKey::new(), None, &[], &lines);
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert_eq!(
        text(&outcome)
            .matches("awaiting oem requirements-owner")
            .count(),
        2,
        "{}",
        text(&outcome)
    );
}

// ---- Falsification 3: a second signature changes the bytes or invalidates the first ----

#[test]
fn a_second_signature_changes_no_bytes_and_invalidates_nothing() {
    let manifest = fixture_manifest();
    let bytes = canonical_bytes(&manifest);
    let trust = two_parties();

    let one = PerKey::new();
    let first_only = judge(&trust, &one, None, &keyed(&["oem-ro"]), &[]);
    assert_eq!(first_only, Outcome::Verified, "{first_only:?}");

    let two = PerKey::new();
    let both = judge(&trust, &two, None, &keyed(&["oem-ro", "acme-sm"]), &[]);
    assert_eq!(both, Outcome::Verified, "{both:?}");

    // Every party's signature is judged over the same manifest bytes, the same bytes whether or
    // not a second party has signed.
    for message in one.seen.borrow().iter().chain(two.seen.borrow().iter()) {
        assert_eq!(message.as_slice(), bytes.as_bytes());
    }
    assert_eq!(one.seen.borrow().len(), 1);
    assert_eq!(two.seen.borrow().len(), 2);
    assert_eq!(canonical_bytes(&manifest), bytes);
}

// ---- Falsification 4: no policy and no party fields verifies exactly as before ----

#[test]
fn without_a_policy_or_party_fields_verification_is_what_it_was() {
    let cases: Vec<(TrustFile, Option<String>, Box<dyn SignatureVerifier>)> = vec![
        (
            trust(&[("req-baseline-1", None, None)]),
            Some(sig("req-baseline-1")),
            Box::new(PerKey::new()),
        ),
        (
            trust(&[("req-baseline-1", None, None)]),
            None,
            Box::new(PerKey::new()),
        ),
        (
            trust(&[("req-baseline-1", None, None)]),
            Some("forged".to_string()),
            Box::new(PerKey::new()),
        ),
        (
            trust(&[("someone-else", None, None)]),
            Some(sig("req-baseline-1")),
            Box::new(PerKey::new()),
        ),
        (
            trust(&[("req-baseline-1", None, Some("2026-10-01"))]),
            Some(sig("req-baseline-1")),
            Box::new(PerKey::new()),
        ),
    ];
    let log = log();
    for (trust, default, verifier) in &cases {
        let checks = Verification {
            signature: default.as_deref(),
            trust,
            key_id: "req-baseline-1",
            verifier: verifier.as_ref(),
            now: SIGNED_AT,
            log: Ok(&log),
        };
        let before = verify(&[], &fixture_manifest(), &checks);
        let after = verify_parties(
            &[],
            &fixture_manifest(),
            &checks,
            &Parties {
                keyed: &[],
                policy: &[],
            },
        );
        assert_eq!(
            before, after,
            "no policy, no keyed file: byte-for-byte as before"
        );
    }
}

#[test]
fn a_party_field_alone_changes_nothing_without_a_policy() {
    // A key carrying party and role, and no `.req/signing`: one verified signature, as today.
    let holder = trust(&[("req-baseline-1", OEM, None)]);
    let outcome = judge(
        &holder,
        &PerKey::new(),
        Some(&sig("req-baseline-1")),
        &[],
        &[],
    );
    assert_eq!(outcome, Outcome::Verified, "{outcome:?}");
}

// ---- The rest of decision 4: every present signature is judged ----

#[test]
fn a_keyed_signature_naming_an_unknown_key_is_unverifiable_and_named() {
    let outcome = judge(
        &two_parties(),
        &PerKey::new(),
        None,
        &keyed(&["oem-ro", "stranger"]),
        &[],
    );
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert!(
        text(&outcome).contains("stranger") && text(&outcome).contains("not in the trust file"),
        "never ignored, and named: {}",
        text(&outcome)
    );
}

#[test]
fn a_keyed_signature_that_does_not_verify_fails_and_names_its_key() {
    let forged = vec![KeyedSignature {
        key_id: "acme-sm".to_string(),
        signature: Ok("forged".to_string()),
    }];
    let outcome = judge(&two_parties(), &PerKey::new(), None, &forged, &[]);
    assert_eq!(outcome.exit_code(), 1, "{outcome:?}");
    assert!(text(&outcome).contains("acme-sm"), "{}", text(&outcome));
}

#[test]
fn a_key_revoked_at_the_signing_time_fails() {
    let revoked = trust(&[("oem-ro", OEM, Some("2026-10-07"))]);
    let outcome = judge(&revoked, &PerKey::new(), None, &keyed(&["oem-ro"]), &[]);
    assert_eq!(outcome.exit_code(), 1, "{outcome:?}");
    assert!(text(&outcome).contains("oem-ro") && text(&outcome).contains("revoked"));
}

#[test]
fn a_failure_outranks_an_awaited_party() {
    let lines = policy(&[
        ("oem", "requirements-owner"),
        ("acme-tier1", "safety-manager"),
    ]);
    let forged = vec![KeyedSignature {
        key_id: "oem-ro".to_string(),
        signature: Ok("forged".to_string()),
    }];
    let outcome = judge(&two_parties(), &PerKey::new(), None, &forged, &lines);
    assert_eq!(outcome.exit_code(), 1, "{outcome:?}");
}

#[test]
fn an_unreadable_signature_file_is_unverifiable_and_named() {
    let unreadable = vec![KeyedSignature {
        key_id: "oem-ro".to_string(),
        signature: Err("cannot read b.json.oem-ro.minisig: permission denied".to_string()),
    }];
    let outcome = judge(&two_parties(), &PerKey::new(), None, &unreadable, &[]);
    assert_eq!(outcome.exit_code(), 3, "{outcome:?}");
    assert!(text(&outcome).contains("permission denied"));
}

#[test]
fn without_a_policy_one_verified_signature_is_enough_and_none_is_not() {
    let one = judge(
        &two_parties(),
        &PerKey::new(),
        None,
        &keyed(&["acme-sm"]),
        &[],
    );
    assert_eq!(one, Outcome::Verified, "{one:?}");
    let none = judge(&two_parties(), &PerKey::new(), None, &[], &[]);
    assert_eq!(none.exit_code(), 3, "{none:?}");
    assert!(text(&none).contains("no signature"), "{}", text(&none));
}

// ---- `.req/signing` ----

#[test]
fn a_signing_policy_reads_party_role_lines_and_skips_comments() {
    let parsed = parse_signing_policy(
        "# who must sign\n\noem requirements-owner\nacme-tier1 safety-manager   # trailing\noem requirements-owner",
    )
    .expect("a well-formed policy parses");
    assert_eq!(
        parsed,
        policy(&[
            ("oem", "requirements-owner"),
            ("acme-tier1", "safety-manager"),
            ("oem", "requirements-owner"),
        ])
    );
}

#[test]
fn every_malformed_policy_line_is_named_in_one_run() {
    let errors = parse_signing_policy("oem requirements-owner\nsolo\na b c\nok role\nbad! role\n")
        .expect_err("a malformed policy is refused");
    assert_eq!(errors.len(), 3, "{errors:?}");
    for (error, line) in errors.iter().zip(["2", "3", "5"]) {
        assert!(
            error.contains(&format!(".req/signing:{line}")),
            "{error} should name line {line}"
        );
    }
}

// ---- Signed Decisions (ADR decision 5) ----

fn decision_with_role(role: &str) -> Decision {
    let mut decision = log().remove(0);
    decision.role = role.to_string();
    decision
}

#[test]
fn a_signed_decision_with_the_keys_role_is_reported_signed() {
    let verifier = PerKey::new();
    let record = decision_with_role("requirements-owner");
    let result =
        decision::judge_signature(&record, "oem-ro", &sig("oem-ro"), &two_parties(), &verifier);
    assert_eq!(
        result,
        Attribution::Signed {
            key_id: "oem-ro".to_string(),
            party: Some("oem".to_string()),
            role: "requirements-owner".to_string(),
        }
    );
    // The signature covers the record's canonical bytes.
    assert_eq!(
        verifier.seen.borrow().as_slice(),
        [decision::canonical_bytes(&record).into_bytes()]
    );
}

// ---- Falsification 5: a signed Decision whose role differs from its key's verifies ----

#[test]
fn a_signed_decision_whose_role_differs_from_its_keys_fails() {
    let record = decision_with_role("maintainer");
    let result = decision::judge_signature(
        &record,
        "oem-ro",
        &sig("oem-ro"),
        &two_parties(),
        &PerKey::new(),
    );
    match result {
        Attribution::Failed(why) => {
            assert!(
                why.contains("maintainer") && why.contains("requirements-owner"),
                "{why}"
            );
        }
        other => panic!("a role mismatch must FAIL, got {other:?}"),
    }
}

#[test]
fn a_decision_signed_by_a_key_with_no_role_fails() {
    let record = decision_with_role("requirements-owner");
    let result = decision::judge_signature(
        &record,
        "loose",
        &sig("loose"),
        &trust(&[("loose", None, None)]),
        &PerKey::new(),
    );
    assert!(matches!(result, Attribution::Failed(_)), "{result:?}");
}

#[test]
fn a_decision_signature_that_does_not_verify_fails() {
    let record = decision_with_role("requirements-owner");
    let result =
        decision::judge_signature(&record, "oem-ro", "forged", &two_parties(), &PerKey::new());
    assert!(matches!(result, Attribution::Failed(_)), "{result:?}");
}

#[test]
fn a_decision_key_revoked_at_the_records_date_fails() {
    let record = decision_with_role("requirements-owner");
    let revoked = trust(&[("oem-ro", OEM, Some("2026-10-07"))]);
    let result =
        decision::judge_signature(&record, "oem-ro", &sig("oem-ro"), &revoked, &PerKey::new());
    assert!(matches!(result, Attribution::Failed(_)), "{result:?}");
}

#[test]
fn a_decision_signed_by_an_unknown_key_is_unverifiable() {
    let record = decision_with_role("requirements-owner");
    let result = decision::judge_signature(
        &record,
        "stranger",
        &sig("stranger"),
        &two_parties(),
        &PerKey::new(),
    );
    assert!(matches!(result, Attribution::Unverifiable(_)), "{result:?}");
}

#[test]
fn a_decision_signature_no_verifier_can_check_is_unverifiable() {
    struct Absent;
    impl SignatureVerifier for Absent {
        fn scheme(&self) -> &'static str {
            "minisign"
        }
        fn verify(&self, _: &[u8], _: &str, _: &str) -> SignatureOutcome {
            SignatureOutcome::Unverifiable("minisign is not runnable".to_string())
        }
    }
    let record = decision_with_role("requirements-owner");
    let result =
        decision::judge_signature(&record, "oem-ro", &sig("oem-ro"), &two_parties(), &Absent);
    assert!(matches!(result, Attribution::Unverifiable(_)), "{result:?}");
}

// ---- The CLI, with real minisign keys, offline ----

mod cli {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use req::decision::{self, Decision};

    pub struct Corpus(PathBuf);

    impl Corpus {
        /// A corpus root with the repository's own claim and Decision log, so a manifest emitted
        /// from it attests a real chain head.
        pub fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
                "party-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            for file in [
                "claims/REQ-0001.yml",
                "aliases/subjects.yml",
                "decisions/DEC-0001.yml",
            ] {
                let to = dir.join(file);
                std::fs::create_dir_all(to.parent().expect("a parent")).expect("creates the dir");
                std::fs::copy(root.join(file), &to).expect("copies a corpus file");
            }
            Corpus(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        pub fn write(&self, relative: &str, text: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates the dir");
            std::fs::write(path, text).expect("writes the file");
        }

        pub fn req(&self, args: &[&str]) -> Out {
            let output = Command::new(env!("CARGO_BIN_EXE_req"))
                .arg("--root")
                .arg(&self.0)
                .args(args)
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                // Nothing about verification may reach a network (ADR 0042 falsification 6): a
                // proxy that refuses every connection must change no verdict.
                .env("HTTP_PROXY", "http://127.0.0.1:9")
                .env("HTTPS_PROXY", "http://127.0.0.1:9")
                .output()
                .expect("runs the compiled binary");
            Out {
                code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }
        }
    }

    impl Drop for Corpus {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub struct Out {
        pub code: i32,
        pub stdout: String,
        pub stderr: String,
    }

    impl Out {
        pub fn all(&self) -> String {
            format!("{}{}", self.stdout, self.stderr)
        }
    }

    /// `minisign -G -W` in the corpus: the key pair's names are `<name>.pub` / `<name>.key`.
    pub fn generate(corpus: &Corpus, name: &str) -> String {
        let public = corpus.path().join(format!("{name}.pub"));
        let secret = corpus.path().join(format!("{name}.key"));
        let out = Command::new("minisign")
            .args(["-G", "-W", "-f", "-p"])
            .arg(&public)
            .arg("-s")
            .arg(&secret)
            .output()
            .expect("runs minisign");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::read_to_string(&public).expect("reads the public key")
    }

    /// Sign `file` with key `name`, writing the signature to `signature`.
    pub fn sign(corpus: &Corpus, name: &str, file: &Path, signature: &Path) {
        let out = Command::new("minisign")
            .args(["-S", "-m"])
            .arg(file)
            .arg("-s")
            .arg(corpus.path().join(format!("{name}.key")))
            .arg("-x")
            .arg(signature)
            .output()
            .expect("runs minisign");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// One trust entry, the public key indented as a YAML block scalar.
    pub fn trust_entry(id: &str, public_key: &str, holder: Option<(&str, &str)>) -> String {
        let indented = public_key
            .lines()
            .map(|line| format!("      {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let holder = match holder {
            Some((party, role)) => format!("    party: \"{party}\"\n    role: \"{role}\"\n"),
            None => String::new(),
        };
        format!(
            "  - id: \"{id}\"\n    scheme: \"minisign\"\n    public_key: |\n{indented}\n    valid_from: \"2026-01-01\"\n{holder}"
        )
    }

    /// Emit the manifest at the fixture signing time into `baseline.json`.
    pub fn emit(corpus: &Corpus) -> PathBuf {
        let out = corpus.req(&["baseline", "--at", "2026-10-07T09:00:00Z"]);
        assert_eq!(out.code, 0, "{}", out.all());
        let path = corpus.path().join("baseline.json");
        std::fs::write(&path, &out.stdout).expect("writes the manifest");
        path
    }

    /// A filled Decision record as YAML, and the record itself.
    pub fn decision_file(id: &str, role: &str) -> (String, Decision) {
        let mut log = vec![Decision {
            id: id.to_string(),
            kind: "accept".to_string(),
            claim_id: "REQ-0001".to_string(),
            address: "aaa".to_string(),
            actor: "Dana Example".to_string(),
            role: role.to_string(),
            date: "2026-10-07".to_string(),
            note: "ratified".to_string(),
            prev_hash: None,
            hash: None,
        }];
        decision::fill(&mut log);
        let record = log.remove(0);
        let text = format!(
            "id: {id}\nkind: accept\nclaim_id: REQ-0001\naddress: aaa\nactor: \"Dana Example\"\nrole: {role}\ndate: \"2026-10-07\"\nnote: ratified\nprev_hash: \"{}\"\nhash: \"{}\"\n",
            record.prev_hash.as_deref().unwrap_or_default(),
            record.hash.as_deref().unwrap_or_default()
        );
        (text, record)
    }
}

fn require_minisign() {
    let installed = Command::new("minisign")
        .arg("-v")
        .output()
        .is_ok_and(|out| out.status.success());
    assert!(
        installed,
        "minisign is required — install it first: `brew install minisign`"
    );
}

use std::process::Command;

/// A two-party corpus: two throwaway keys, a trust file naming each key's party and role, and
/// the manifest signed by none yet.
fn two_party_corpus(tag: &str) -> (cli::Corpus, std::path::PathBuf) {
    require_minisign();
    let corpus = cli::Corpus::new(tag);
    let oem = cli::generate(&corpus, "oem-ro");
    let acme = cli::generate(&corpus, "acme-sm");
    corpus.write(
        "baselines/trust.yml",
        &format!(
            "keys:\n{}{}",
            cli::trust_entry("oem-ro", &oem, Some(("oem", "requirements-owner"))),
            cli::trust_entry("acme-sm", &acme, Some(("acme-tier1", "safety-manager")))
        ),
    );
    corpus.write(
        ".req/signing",
        "# both sides sign\noem requirements-owner\nacme-tier1 safety-manager\n",
    );
    let manifest = cli::emit(&corpus);
    (corpus, manifest)
}

fn sign_as(corpus: &cli::Corpus, key: &str, manifest: &std::path::Path) {
    let name = manifest.file_name().expect("a name").to_string_lossy();
    let signature = manifest.with_file_name(format!("{name}.{key}.minisig"));
    cli::sign(corpus, key, manifest, &signature);
}

#[test]
fn a_baseline_awaits_the_second_party_then_verifies_offline() {
    let (corpus, manifest) = two_party_corpus("two-party");
    let path = manifest.to_string_lossy().into_owned();

    sign_as(&corpus, "oem-ro", &manifest);
    let awaiting = corpus.req(&["baseline", "--verify", &path]);
    assert_eq!(awaiting.code, 3, "{}", awaiting.all());
    assert!(
        awaiting
            .stdout
            .contains("awaiting acme-tier1 safety-manager"),
        "{}",
        awaiting.all()
    );
    assert!(
        awaiting
            .stdout
            .contains("key oem-ro (oem requirements-owner)"),
        "the header names every signature judged: {}",
        awaiting.all()
    );

    sign_as(&corpus, "acme-sm", &manifest);
    let both = corpus.req(&["baseline", "--verify", &path]);
    assert_eq!(both.code, 0, "{}", both.all());
    assert!(both.stdout.contains("[VERIFIED]"), "{}", both.all());
    assert!(both.stdout.contains("key oem-ro (oem requirements-owner)"));
    assert!(
        both.stdout
            .contains("key acme-sm (acme-tier1 safety-manager)")
    );
}

#[test]
fn a_second_signature_leaves_the_first_signature_and_the_manifest_untouched() {
    let (corpus, manifest) = two_party_corpus("countersign");
    let path = manifest.to_string_lossy().into_owned();
    sign_as(&corpus, "oem-ro", &manifest);
    let before = std::fs::read(&manifest).expect("reads the manifest");
    let first = std::fs::read(corpus.path().join("baseline.json.oem-ro.minisig"))
        .expect("reads the first signature");
    sign_as(&corpus, "acme-sm", &manifest);
    assert_eq!(std::fs::read(&manifest).expect("reads"), before);
    assert_eq!(
        std::fs::read(corpus.path().join("baseline.json.oem-ro.minisig")).expect("reads"),
        first
    );
    // Without a policy line for acme, the OEM signature alone still verifies on its own.
    corpus.write(".req/signing", "oem requirements-owner\n");
    let out = corpus.req(&["baseline", "--verify", &path]);
    assert_eq!(out.code, 0, "{}", out.all());
}

#[test]
fn a_tampered_manifest_fails_under_two_signatures() {
    let (corpus, manifest) = two_party_corpus("tampered");
    sign_as(&corpus, "oem-ro", &manifest);
    sign_as(&corpus, "acme-sm", &manifest);
    let original = std::fs::read_to_string(&manifest).expect("reads");
    std::fs::write(
        &manifest,
        original.replacen("2026-10-07T09:00:00Z", "2026-10-08T09:00:00Z", 1),
    )
    .expect("tampers");
    let out = corpus.req(&["baseline", "--verify", &manifest.to_string_lossy()]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.stdout.contains("[FAIL]"), "{}", out.all());
}

#[test]
fn a_signature_file_naming_a_key_the_trust_file_lacks_is_unverifiable_and_named() {
    let (corpus, manifest) = two_party_corpus("unknown-file");
    sign_as(&corpus, "oem-ro", &manifest);
    sign_as(&corpus, "acme-sm", &manifest);
    std::fs::copy(
        corpus.path().join("baseline.json.oem-ro.minisig"),
        corpus.path().join("baseline.json.stranger.minisig"),
    )
    .expect("plants a signature file for an unknown key");
    let out = corpus.req(&["baseline", "--verify", &manifest.to_string_lossy()]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.stdout.contains("stranger") && out.stdout.contains("not in the trust file"),
        "{}",
        out.all()
    );
}

#[test]
fn a_malformed_signing_policy_is_a_usage_error_naming_every_line() {
    let (corpus, manifest) = two_party_corpus("bad-policy");
    sign_as(&corpus, "oem-ro", &manifest);
    corpus.write(".req/signing", "oem requirements-owner\nsolo\na b c\n");
    let out = corpus.req(&["baseline", "--verify", &manifest.to_string_lossy()]);
    assert_eq!(out.code, 2, "{}", out.all());
    let all = out.all();
    assert!(all.contains(".req/signing:2"), "{all}");
    assert!(all.contains(".req/signing:3"), "{all}");
}

#[test]
fn a_repository_with_no_policy_and_one_default_signature_prints_what_it_always_did() {
    require_minisign();
    let corpus = cli::Corpus::new("legacy");
    let key = cli::generate(&corpus, "req-baseline-1");
    corpus.write(
        "baselines/trust.yml",
        &format!("keys:\n{}", cli::trust_entry("req-baseline-1", &key, None)),
    );
    let manifest = cli::emit(&corpus);
    cli::sign(
        &corpus,
        "req-baseline-1",
        &manifest,
        &corpus.path().join("baseline.json.minisig"),
    );
    let out = corpus.req(&["baseline", "--verify", &manifest.to_string_lossy()]);
    assert_eq!(out.code, 0, "{}", out.all());
    let header = out.stdout.lines().next().expect("a header");
    assert!(
        header.contains(", key req-baseline-1, signed at 2026-10-07T09:00:00Z, verified at "),
        "{header}"
    );
    assert!(
        !header.contains("(oem") && !header.contains(") "),
        "no party, no party annotation: {header}"
    );
    assert!(out.stdout.contains("[VERIFIED]"));
}

// ---- signed Decisions through `req decision` ----

/// A corpus whose only Decision is `DEC-0001` with `record_role`, signed by `oem-ro` (role
/// `requirements-owner`) when `sign` is set.
fn decision_corpus(tag: &str, record_role: &str, sign: bool) -> cli::Corpus {
    require_minisign();
    let corpus = cli::Corpus::new(tag);
    let oem = cli::generate(&corpus, "oem-ro");
    corpus.write(
        "baselines/trust.yml",
        &format!(
            "keys:\n{}",
            cli::trust_entry("oem-ro", &oem, Some(("oem", "requirements-owner")))
        ),
    );
    let (text, record) = cli::decision_file("DEC-0001", record_role);
    corpus.write("decisions/DEC-0001.yml", &text);
    if sign {
        let bytes = corpus.path().join("decision.bytes");
        std::fs::write(&bytes, decision::canonical_bytes(&record)).expect("writes the bytes");
        cli::sign(
            &corpus,
            "oem-ro",
            &bytes,
            &corpus.path().join("decisions/DEC-0001.yml.oem-ro.minisig"),
        );
    }
    corpus
}

#[test]
fn a_decision_signed_with_a_matching_role_is_reported_signed() {
    let corpus = decision_corpus("dec-signed", "requirements-owner", true);
    let out = corpus.req(&["decision", "--verify"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        out.stdout
            .contains("signed by oem-ro (oem requirements-owner)"),
        "{}",
        out.all()
    );
    assert!(out.stdout.contains("[VALID]"), "{}", out.all());
}

#[test]
fn a_decision_signed_under_a_different_role_fails() {
    let corpus = decision_corpus("dec-mismatch", "maintainer", true);
    let out = corpus.req(&["decision", "--verify"]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.stdout.contains("[FAIL]"), "{}", out.all());
    assert!(out.stdout.contains("DEC-0001"), "{}", out.all());
}

#[test]
fn an_unsigned_decision_is_asserted_and_the_verdict_is_unchanged() {
    let corpus = decision_corpus("dec-asserted", "maintainer", false);
    let out = corpus.req(&["decision", "--verify"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("asserted"), "{}", out.all());
    assert!(out.stdout.contains("[VALID]"), "{}", out.all());
}

#[test]
fn a_decision_signature_by_an_unknown_key_is_unverifiable() {
    let corpus = decision_corpus("dec-unknown", "requirements-owner", true);
    std::fs::rename(
        corpus.path().join("decisions/DEC-0001.yml.oem-ro.minisig"),
        corpus
            .path()
            .join("decisions/DEC-0001.yml.stranger.minisig"),
    )
    .expect("renames the signature to an unknown key");
    let out = corpus.req(&["decision", "--verify"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.stdout.contains("stranger"), "{}", out.all());
}

/// Review A-1-review-1 (low): `DEC-NNNN.yml.minisig` — the spelling a Baseline's default signature
/// uses — names no key, so it cannot be judged; it must say so, never print "asserted (no
/// signature file…)", which would be false.
#[test]
fn a_decision_signature_that_names_no_key_is_unverifiable_not_asserted() {
    let corpus = decision_corpus("dec-no-id", "requirements-owner", false);
    corpus.write(
        "decisions/DEC-0001.yml.minisig",
        "untrusted comment: no key id\n",
    );
    let out = corpus.req(&["decision", "--verify"]);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(!out.stdout.contains("asserted"), "{}", out.all());
    assert!(out.all().contains("DEC-0001.yml.minisig"), "{}", out.all());
}
