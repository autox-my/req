//! Which canonical-form fields differ between two forms, and the *shape* bucket that ranks them.
//! The shape is the actionable output: the most frequent shapes are the rules that would recover
//! the most pairs.

use req::canon::CanonicalForm;

/// Names of the fields in which `a` and `b` differ, alphabetical. A structured form against an
/// unstructured one differs in `structure` alone (their slots are not comparable); two
/// unstructured forms differ in `raw` (the fallback hashes the normalized text).
pub fn differing_fields(a: &CanonicalForm, b: &CanonicalForm) -> Vec<&'static str> {
    if a.unstructured != b.unstructured {
        return vec!["structure"];
    }
    if a.unstructured {
        return if a.raw != b.raw {
            vec!["raw"]
        } else {
            Vec::new()
        };
    }
    let mut out = Vec::new();
    if a.action != b.action {
        out.push("action");
    }
    if a.applicability != b.applicability {
        out.push("applicability");
    }
    if a.conditions != b.conditions {
        out.push("conditions");
    }
    if a.exception != b.exception {
        out.push("exception");
    }
    if a.modality != b.modality {
        out.push("modality");
    }
    if a.operator != b.operator {
        out.push("operator");
    }
    if a.qualifier != b.qualifier {
        out.push("qualifier");
    }
    if a.subject != b.subject {
        out.push("subject");
    }
    if a.temporal != b.temporal {
        out.push("temporal");
    }
    if a.unit != b.unit {
        out.push("unit");
    }
    if a.value != b.value {
        out.push("value");
    }
    out
}

/// The shape bucket: the differing fields joined with `+` (`"qualifier"`, `"action+subject"`).
/// `"unexplained"` when no compared field differs — which, for two forms with different
/// addresses, means the comparison missed a field and is itself a finding.
pub fn bucket(a: &CanonicalForm, b: &CanonicalForm) -> String {
    let fields = differing_fields(a, b);
    if fields.is_empty() {
        "unexplained".to_string()
    } else {
        fields.join("+")
    }
}
