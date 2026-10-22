//! Claim slots vs identity: which authored slots reach the canonical form, and what
//! happens to the ones that cannot.
//!
//! Three gaps were closed here, all of the same shape as the predicate-span bug — an
//! authored slot that identity ignored:
//!   * `applicability` (ADR 0003 says it is identity) had no field in the form at all;
//!   * `conditions` were parsed but never compared with what the author wrote;
//!   * `quantifier`/`exception`/`temporal` have no representation, and authoring one was
//!     silently dropped. It is now refused, because the honest answer to "the rules
//!     cannot carry this" is to say so, not to hash a form that ignores the author.

use req::canon::{canonical_json, evaluate_claim};
use req::model::{ClaimFile, Registry};

const CLAIM: &str = "\
schema: req.claim/1
claim_id: REQ-X
statement: The system shall log each command.
modality: SHALL
subject: SYSTEM
applicability: []
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

fn claim(text: &str) -> ClaimFile {
    yaml_serde::from_str(text).expect("claim parses")
}

/// The authored selector slot, which the loader now requires to be present: the one
/// line a variant claim edits. The count assertion is what makes the helper safe: a
/// template with *no* anchor would leave the "variant" identical to the base (and the
/// failure would read as a bug in `evaluate_claim`), while a second anchor would let
/// `replacen(…, 1)` edit one line and silently corrupt the other.
fn with_applicability(predicate: &str) -> String {
    assert_eq!(
        CLAIM.matches("applicability: []").count(),
        1,
        "the CLAIM template must hold exactly one `applicability: []` line: this \
         helper edits that one line"
    );
    CLAIM.replacen(
        "applicability: []",
        &format!("applicability:\n  - {predicate}"),
        1,
    )
}

/// ADR 0003 says variants are selectors and applicability is part of identity. Before
/// rules v5 the canonical form had no applicability field at all, so two claims
/// differing only in their selector got the SAME address — a silent false merge.
#[test]
fn authored_applicability_is_part_of_identity() {
    let base = claim(CLAIM);
    let variant = claim(&with_applicability("deployment_mode == air_gapped"));
    let a = evaluate_claim(&base, &Registry::empty());
    let b = evaluate_claim(&variant, &Registry::empty());
    assert_ne!(canonical_json(&a.form), canonical_json(&b.form));
    assert!(
        b.form
            .applicability
            .contains("deployment_mode == air_gapped")
    );
    assert!(a.form.applicability.is_empty());
}

/// Conditions ARE parsed (T1.8), so an authored list that disagrees with the parse is a
/// divergence — before this, authored conditions were silently unverified.
#[test]
fn authored_conditions_are_compared_with_the_parse() {
    let plain = claim(CLAIM);
    assert!(
        evaluate_claim(&plain, &Registry::empty()).conditions_ok,
        "no guard in the statement, no conditions authored"
    );

    let guarded = claim(&CLAIM.replace(
        "statement: The system shall log each command.",
        "statement: The system shall log each command when the brake is engaged.",
    ));
    let v = evaluate_claim(&guarded, &Registry::empty());
    assert!(
        !v.conditions_ok,
        "a parsed guard disagrees with authored []"
    );
    assert!(v.conditions_detail.contains("parsed"));
    assert!(v.flags() >= 1);
}

/// A slot the rules cannot represent is a gap in the *rules*. Authoring one is refused
/// — reported, and `req canon --fill` will not write an address that ignores it — never
/// silently dropped from identity.
#[test]
fn authoring_an_unrepresentable_slot_is_refused_not_ignored() {
    for (slot, line) in [
        ("quantifier", "quantifier: EACH"),
        ("exception", "exception: unless the operator overrides"),
        ("temporal", "temporal: at all times"),
    ] {
        let text = CLAIM.replace(
            "unstructured: false",
            &format!("unstructured: false\n{line}"),
        );
        let v = evaluate_claim(&claim(&text), &Registry::empty());
        assert!(!v.slots_ok, "{slot} must be reported");
        assert!(v.slots_detail.contains(slot), "{}", v.slots_detail);
        assert!(v.flags() >= 1);
    }
}
