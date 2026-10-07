//! Conformance of the canonical bytes to the format the spec documents (T1.9).
//!
//! The canonical form is the audit artifact *and* the sha256 input. A porcelain, an
//! audit tool or a re-implementation reads it, so "JSON with sorted keys" is a
//! contract, not a description — and a field the schema names but the serializer
//! omits is a silent hole in identity.

use req::canon::{canonical_json, canonicalize_statement};
use req::model::Registry;

fn cases() -> Vec<String> {
    let r = Registry::empty();
    [
        "The system shall respond within 200 ms.",
        "The req tool shall never rewrite a claim.",
        "Authoring shall be friction-free.",
        "If the brake is engaged, the system shall log each command.",
    ]
    .iter()
    .map(|t| canonical_json(&canonicalize_statement(t, &r).0))
    .collect()
}

#[test]
fn canonical_bytes_are_valid_json() {
    for json in cases() {
        serde_json::from_str::<serde_json::Value>(&json)
            .unwrap_or_else(|e| panic!("canonical form is not JSON ({e}):\n{json}"));
    }
}

/// Every field the spec's T1.9 schema names must be present in the bytes, and the
/// keys must be in sorted order (the spec's "JSON with sorted keys").
#[test]
fn canonical_bytes_carry_the_documented_schema_keys_sorted() {
    for json in cases() {
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let obj = v.as_object().expect("a JSON object");
        // `quantifier` is deliberately absent: the parser does not derive one yet, and
        // the spec says so (canonical-hashing.md T1.9). Everything else must be here.
        let expected = [
            "action",
            "conditions",
            "exception",
            "modality",
            "operator",
            "qualifier",
            "raw",
            "subject",
            "temporal",
            "unstructured",
            "unit",
            "value",
        ];
        for key in expected {
            assert!(
                obj.contains_key(key),
                "schema field {key:?} missing from {json}"
            );
        }
        assert_eq!(obj.len(), expected.len(), "unexpected field in {json}");
        let keys: Vec<&String> = obj.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "keys are not sorted in {json}");
    }
}
