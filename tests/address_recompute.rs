//! What outlived `req canon --fill` after `req.claim/2` (ADR 0022): nothing writes a derived
//! field any more, because none is stored. These tests keep REQ-0017's property — the address is
//! derived at load as the versioned-tag digest of the canonical bytes, and the one the signed
//! Baseline attests — and two rules of the old file that outlived the writer: a test link is
//! outside identity, and the evidence digest matches an independent oracle.
//!
//! The file was `tests/fill.rs` until REQ-0017 was re-worded to what v2 guarantees (the CI review
//! of M3 found its old statement false); its link and the Baseline moved with it.

use req::canon::{derive, load_claims, parse_claim};
use req::model::{Evidence, Registry, tests_digest};
use sha2::{Digest, Sha256};

/// `sha256("req.claim.v1\0" ‖ bytes)`, computed here with `sha2` directly — not through
/// `canon::address_hex` — so the recomputation is an independent oracle for the derivation.
fn oracle_address(canonical: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"req.claim.v1\0");
    hasher.update(canonical.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// REQ-0017's evidence: the address recomputes from the canonical bytes, and the signed manifest
/// stores the address the claim derives at load.
#[test]
fn the_address_recomputes_from_the_canonical_bytes_and_matches_the_signed_manifest() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let claims = load_claims(&root).expect("the committed corpus loads");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("baselines/air-gapped.json"))
            .expect("the signed Baseline is committed"),
    )
    .expect("the manifest is JSON");
    let attested: std::collections::BTreeMap<&str, &str> = manifest["claims"]
        .as_array()
        .expect("the manifest lists claims")
        .iter()
        .map(|c| {
            (
                c["claim_id"].as_str().expect("claim_id"),
                c["address"].as_str().expect("address"),
            )
        })
        .collect();
    assert!(!attested.is_empty(), "the manifest attests claims");

    for loaded in &claims {
        let derived = &loaded.derived;
        // hashing §5: the address recomputes from the canonical bytes alone.
        assert_eq!(
            oracle_address(&derived.canonical),
            derived.address,
            "{}: the address does not recompute from its canonical bytes",
            loaded.claim.claim_id
        );
        // Nothing derived is stored in the file (ADR 0022)…
        for line in loaded.text.lines() {
            assert!(
                !line.starts_with("address:") && !line.starts_with("canonical:"),
                "{}: a req.claim/2 file stores no fingerprint: {line}",
                loaded.claim.claim_id
            );
        }
        // …and the address the signed manifest *does* store is the one the claim derives.
        if let Some(signed) = attested.get(loaded.claim.claim_id.as_str()) {
            assert_eq!(
                *signed, derived.address,
                "{}: the signed manifest's address is not the one derived at load",
                loaded.claim.claim_id
            );
        }
    }
    assert_eq!(
        claims
            .iter()
            .filter(|c| attested.contains_key(c.claim.claim_id.as_str()))
            .count(),
        attested.len(),
        "every attested claim is in the corpus"
    );
}

const SAMPLE: &str = "\
schema: req.claim/2
claim_id: REQ-9999
statement: >-
  The system shall respond within 200 ms.
applicability: []
relations: []
rationale:
  source: docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
";

/// **The human's ruling, pinned**: adding or removing a `tests:` entry (or flipping the evidence
/// mode) leaves the claim's **address** untouched. Identity is the canonical form over meaning;
/// a test rename is not a meaning change, so it must never move an address.
#[test]
fn adding_or_removing_a_test_link_moves_no_address() {
    let address_of = |text: &str| {
        let claim = parse_claim(text).expect("the fixture is a claim file");
        derive(&claim, &Registry::empty()).address
    };
    let base = address_of(SAMPLE);
    let with = |block: &str| {
        let text = SAMPLE.replace("relations: []\n", &format!("relations: []\n{block}"));
        assert_ne!(text, SAMPLE, "the fixture carries the anchor");
        address_of(&text)
    };
    // One link, two links, and a process declaration: the form does not carry any of them.
    assert_eq!(
        with("tests:\n  - \"a::b\"\n"),
        base,
        "adding a link moved the address"
    );
    assert_eq!(
        with("tests:\n  - \"a::b\"\n  - \"c::d\"\n"),
        base,
        "a second link moved the address"
    );
    assert_eq!(
        with("tests: []\nevidence: process\n"),
        base,
        "the evidence declaration moved the address"
    );
    // ...and a rename is just a different string in the same list.
    assert_eq!(
        with("tests:\n  - \"renamed::away\"\n"),
        base,
        "a rename inside `tests:` moved the address"
    );
    // A pin is outside identity too: it reports, it never moves an address (ADR 0022).
    assert_eq!(
        with("pins:\n  operator: GT\n"),
        base,
        "a pin moved the address"
    );
}

/// The attested digest, pinned against a **hand-computed oracle** rather than recomputed the way
/// the code computes it: `printf 'req.tests.v1\0{"evidence":"machine","tests":[]}' | shasum -a 256`.
/// The declaration is order-free (sorted at hash time) and the evidence mode is *inside* it, so
/// flipping machine to process cannot leave the signed bytes unchanged.
#[test]
fn the_evidence_digest_matches_an_independent_oracle() {
    const EMPTY_MACHINE: &str = "f70c34ce463f8c0294ac8846abea5b911d5933c42a9d978522bff8c6b5445d53";
    const ONE_MACHINE: &str = "db5945dbc66ff28c2b42f765b09cecfeab4d7cf34598814784d94b1261e427ed";
    const EMPTY_PROCESS: &str = "f3d1bcc93f181d5f66b7793f7496ccd9fc75ee66bd6431d274a74f4ed739422c";

    assert_eq!(tests_digest(&[], Evidence::Machine), EMPTY_MACHINE);
    assert_eq!(
        tests_digest(&["a::b".to_string()], Evidence::Machine),
        ONE_MACHINE
    );
    assert_eq!(tests_digest(&[], Evidence::Process), EMPTY_PROCESS);
    assert_ne!(
        tests_digest(&[], Evidence::Machine),
        tests_digest(&[], Evidence::Process),
        "the evidence mode is attested, or a quiet machine->process flip would be invisible"
    );

    // Sorted at hash time: authored order is not meaning.
    let forward = vec!["b::x".to_string(), "a::y".to_string()];
    let backward = vec!["a::y".to_string(), "b::x".to_string()];
    assert_eq!(
        tests_digest(&forward, Evidence::Machine),
        tests_digest(&backward, Evidence::Machine)
    );
}
