//! `req canon --fill` — writing the *derived* fields back into claim files.
//!
//! Two properties matter more than the mechanics:
//!   1. the stored address recomputes from the stored canonical form (hashing §5);
//!   2. nothing authored is touched — the file is edited surgically, not re-emitted.

use req::canon::{address_hex, canonical_json, canonicalize_statement};
use req::fill::{StoredState, fill_text, verify_text};
use req::model::{ClaimFile, Registry};

const SAMPLE: &str = "\
schema: req.claim/0
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
