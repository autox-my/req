//! `req canon --fill` — writing the *derived* fields back into claim files.
//!
//! Two properties matter more than the mechanics:
//!   1. the stored address recomputes from the stored canonical form (hashing §5);
//!   2. nothing authored is touched — the file is edited surgically, not re-emitted.

use req::canon::{address_hex, canonical_json, canonicalize_statement, evaluate_claim};
use req::fill::{StoredState, fill_text, verify_text};
use req::model::{ClaimFile, Evidence, Registry, tests_digest};

const SAMPLE: &str = "\
schema: req.claim/1
claim_id: REQ-9999
statement: >-
  The system shall respond within 200 ms.
modality: SHALL
subject: SYSTEM
attribute: response_latency
operator: LE
value: \"0.2\"
unit: s
conditions: []
applicability: []
exception: null
temporal: null
quantifier: null
unstructured: false
canonical:
  rules_v: null
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
";

fn derived() -> (String, String) {
    let (form, _) = canonicalize_statement(
        "The system shall respond within 200 ms.",
        &Registry::empty(),
    );
    let json = canonical_json(&form);
    let address = address_hex(&json);
    (json, address)
}

#[test]
fn fill_writes_derived_fields_and_the_address_recomputes() {
    let (json, address) = derived();
    let filled = fill_text(SAMPLE, &json, &address, 2, 1).expect("anchors are present");

    // The claim still parses, and the stored fields are the derived ones.
    let claim: ClaimFile = yaml_serde::from_str(&filled).expect("still a claim file");
    let stored = claim.canonical.form.clone().expect("form written");
    assert_eq!(stored, json);
    assert_eq!(claim.canonical.rules_v, Some(2));
    assert_eq!(claim.canonical.subject_registry, 1);
    assert_eq!(claim.address.as_deref(), Some(address.as_str()));

    // hashing §5: the address is recomputable from the *stored bytes* alone.
    assert_eq!(address_hex(&stored), claim.address.unwrap());
}

#[test]
fn fill_touches_only_the_derived_fields() {
    let (json, address) = derived();
    let filled = fill_text(SAMPLE, &json, &address, 2, 1).expect("anchors are present");
    assert!(filled.contains("statement: >-\n  The system shall respond within 200 ms."));
    assert!(filled.contains("rationale:\n  source: docs/adr/"));
    assert!(filled.contains("relations: []"));
    assert!(!filled.contains("form: null"));
    assert!(!filled.contains("address: null"));
    assert_eq!(
        filled.lines().count(),
        SAMPLE.lines().count() + json.lines().count()
    );
}

#[test]
fn fill_is_idempotent() {
    let (json, address) = derived();
    let once = fill_text(SAMPLE, &json, &address, 2, 1).expect("anchors are present");
    let twice = fill_text(&once, &json, &address, 2, 1).expect("anchors are present");
    assert_eq!(once, twice, "a second fill must not duplicate the block");

    // A multi-line body must be replaced, not appended to: the drop rule keys on
    // indentation, and the canonical JSON being single-line today is not a guarantee.
    let pretty = "{\n  \"action\": \"respond\",\n  \"unstructured\": false\n}";
    let once = fill_text(SAMPLE, pretty, &address, 2, 1).expect("anchors are present");
    let twice = fill_text(&once, pretty, &address, 2, 1).expect("anchors are present");
    assert_eq!(once, twice, "a multi-line block body must not accumulate");
    assert_eq!(once.matches("\"action\"").count(), 1);
}

#[test]
fn fill_refuses_text_without_the_anchors() {
    // Never write a file that lost its structure: no anchors, no write.
    assert!(fill_text("claim_id: REQ-1\n", "{}", "00", 2, 1).is_err());
    let no_address = SAMPLE.replace("address: null", "address_line: null");
    assert!(fill_text(&no_address, "{}", "00", 2, 1).is_err());
}

/// The check that makes "never hand-write an address" enforceable rather than
/// aspirational: the stored form must be the one the current parse produces, and the
/// stored address must recompute from the stored bytes (REQ-0017).
#[test]
fn verify_accepts_a_filled_file_and_catches_tampering() {
    let (json, address) = derived();
    let filled = fill_text(SAMPLE, &json, &address, 3, 1).expect("anchors");

    assert!(matches!(
        verify_text(&filled, &json, 3),
        StoredState::Verified
    ));

    // Unfilled is a *candidate*, never a pass.
    assert!(matches!(
        verify_text(SAMPLE, &json, 3),
        StoredState::Candidate(_)
    ));
    // A rules drift is a candidate too, not a failure.
    assert!(matches!(
        verify_text(&filled, &json, 4),
        StoredState::Candidate(_)
    ));

    // A hand-edited digest: address no longer recomputes from the stored bytes.
    let zeroed = "0".repeat(64);
    let tampered = filled.replace(&address, &zeroed);
    assert!(matches!(
        verify_text(&tampered, &json, 3),
        StoredState::Mismatch(_)
    ));

    // Internally consistent but *wrong*: a form and an address that match each other
    // yet are not what the parse produces. This is the case a recompute-only check
    // would miss, and the reason the expected form is passed in.
    let poisoned_form = json.replace("\"action\":\"respond\"", "\"action\":\"delete\"");
    assert_ne!(poisoned_form, json);
    let poisoned =
        fill_text(SAMPLE, &poisoned_form, &address_hex(&poisoned_form), 3, 1).expect("anchors");
    assert!(matches!(
        verify_text(&poisoned, &json, 3),
        StoredState::Mismatch(_)
    ));
}

/// A file declaring a format this build does not write is **NOT JUDGED** by this reader too:
/// nothing inside it was compared to anything, so the answer is the exit-3 class — never
/// `Mismatch`, because exit 1 is a *definite* finding (a hand-edited digest) and "this build
/// cannot establish the format" is not one.
///
/// The guard is defensive on the CLI path (`view::load_rows` loads through `load_claims` first),
/// which is exactly why it is pinned here rather than left to a caller that may never exercise
/// it: a public reader that parses a file itself must not be free to skip the declaration.
#[test]
fn verify_text_refuses_a_format_this_build_does_not_write() {
    let (json, _) = derived();
    let foreign = SAMPLE.replace("schema: req.claim/1", "schema: req.claim/2");
    assert_ne!(foreign, SAMPLE, "the fixture's declaration moved");

    match verify_text(&foreign, &json, 3) {
        StoredState::Candidate(why) => {
            assert!(why.contains("NOT JUDGED"), "{why}");
            assert!(
                why.contains("req.claim/2"),
                "the declared value is named: {why}"
            );
        }
        other => panic!("a format this build cannot establish is not {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// ADR 0019: the authored evidence declaration is outside identity and attested beside it.
// ---------------------------------------------------------------------------------------------

/// **The human's ruling, pinned**: adding or removing a `tests:` entry (or flipping the evidence
/// mode) leaves the claim's **address** untouched. Identity is the canonical form over meaning;
/// a test rename is not a meaning change, so it must never move an address.
#[test]
fn adding_or_removing_a_test_link_moves_no_address() {
    let address_of = |text: &str| {
        let claim: ClaimFile = yaml_serde::from_str(text).expect("the fixture is a claim file");
        let verdict = evaluate_claim(&claim, &Registry::empty());
        address_hex(&canonical_json(&verdict.form))
    };
    let base = address_of(SAMPLE);
    let with = |block: &str| {
        let text = SAMPLE.replace(
            "unstructured: false\n",
            &format!("unstructured: false\n{block}"),
        );
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
