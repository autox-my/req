//! Claim slots vs identity: which authored fields reach the canonical form, and what happens to
//! the slots that `req.claim/1` let an author write beside the parse.
//!
//! Three gaps were closed here, all of the same shape as the predicate-span bug — an authored
//! slot that identity ignored:
//!   * `applicability` (ADR 0003 says it is identity) had no field in the form at all;
//!   * `conditions` were parsed but never compared with what the author wrote — since
//!     `req.claim/2` they are only derived, and a guard in the statement moves the address;
//!   * `quantifier`/`exception`/`temporal` have no representation, and authoring one was
//!     silently dropped. `req.claim/2` removed the keys (ADR 0022): authoring one is a parse
//!     error that names it, because the honest answer to "the rules cannot carry this" is to say
//!     so, not to hash a form that ignores the author.

use req::canon::{canonical_json, derive, parse_claim};
use req::model::{ClaimFile, Registry};

const CLAIM: &str = "\
schema: req.claim/2
claim_id: REQ-X
statement: The system shall log each command.
applicability: []
relations: []
rationale:
  source: docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
";

fn claim(text: &str) -> ClaimFile {
    parse_claim(text).expect("claim parses")
}

/// The authored selector slot, which the loader requires to be present: the one line a variant
/// claim edits. The count assertion is what makes the helper safe: a template with *no* anchor
/// would leave the "variant" identical to the base (and the failure would read as a bug in
/// `derive`), while a second anchor would let `replacen(…, 1)` edit one line and silently
/// corrupt the other.
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
    let a = derive(&base, &Registry::empty());
    let b = derive(&variant, &Registry::empty());
    assert_ne!(canonical_json(&a.form), canonical_json(&b.form));
    assert_ne!(a.address, b.address);
    assert!(
        b.form
            .applicability
            .contains("deployment_mode == air_gapped")
    );
    assert!(a.form.applicability.is_empty());
}

/// Conditions are parsed (T1.8) and, since `req.claim/2`, only derived: a statement that gains a
/// guard clause is a different claim, so the derived condition reaches the form and moves the
/// address — there is no authored list left for it to disagree with.
#[test]
fn a_guard_in_the_statement_is_a_derived_condition_and_moves_the_address() {
    let plain = derive(&claim(CLAIM), &Registry::empty());
    assert!(plain.form.conditions.is_empty(), "no guard, no condition");

    let guarded = derive(
        &claim(&CLAIM.replace(
            "statement: The system shall log each command.",
            "statement: The system shall log each command when the brake is engaged.",
        )),
        &Registry::empty(),
    );
    assert_eq!(guarded.form.conditions.len(), 1, "{:?}", guarded.form);
    assert_ne!(
        plain.address, guarded.address,
        "a gained guard is a new claim"
    );
}

/// A slot the rules cannot represent is a gap in the *rules*. `req.claim/2` has no key for it:
/// authoring one is a parse error that names the key — never silently dropped from identity.
#[test]
fn authoring_an_unrepresentable_slot_is_refused_not_ignored() {
    for (slot, line) in [
        ("quantifier", "quantifier: EACH"),
        ("exception", "exception: unless the operator overrides"),
        ("temporal", "temporal: at all times"),
    ] {
        let text = format!("{CLAIM}{line}\n");
        let error = parse_claim(&text).expect_err("an unrepresentable slot is refused");
        assert!(error.contains(&format!("`{slot}`")), "{slot}: {error}");
        assert!(error.contains("removed in req.claim/2"), "{slot}: {error}");
    }
}
