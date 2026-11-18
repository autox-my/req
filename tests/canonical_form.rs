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

/// Every field the spec's T1.9 schema names must be present in the bytes, **in the order
/// the spec lists them**. That order is frozen and is not byte-sorted: `unstructured`
/// precedes `unit`. The order is read from the raw bytes, never from a parsed
/// `serde_json::Map`, which sorts its keys and so could not see it.
#[test]
fn canonical_bytes_carry_the_documented_schema_keys_in_the_documented_order() {
    for json in cases() {
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let obj = v.as_object().expect("a JSON object");
        // `quantifier` is deliberately absent: the parser does not derive one, and
        // authoring one is refused rather than ignored (see tests/gate.rs). Everything
        // else the schema names must be here.
        let expected = [
            "action",
            "applicability",
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
        assert_eq!(
            top_level_keys_in_byte_order(&json),
            expected,
            "keys are not in the T1.9 order in {json}"
        );
        assert_eq!(
            expected.to_vec(),
            documented_t19_keys(),
            "the test's list is the one docs/canonical-hashing.md states"
        );
    }
}

/// The top-level keys of a canonical form, in the order they appear in its bytes.
fn top_level_keys_in_byte_order(json: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    let mut current = String::new();
    let mut expect_key = false;
    for c in json.chars() {
        if in_string {
            if escaped {
                escaped = false;
                current.push(c);
            } else if c == '\\' {
                escaped = true;
                current.push(c);
            } else if c == '"' {
                in_string = false;
                if expect_key {
                    keys.push(std::mem::take(&mut current));
                    expect_key = false;
                }
                current.clear();
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => {
                depth += 1;
                expect_key = depth == 1 && c == '{';
            }
            '}' | ']' => depth -= 1,
            ',' => expect_key = depth == 1,
            _ => {}
        }
    }
    keys
}

/// The T1.9 key list as `docs/canonical-hashing.md` writes it: ``{action, …, value}``.
fn documented_t19_keys() -> Vec<String> {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/canonical-hashing.md"),
    )
    .expect("reads docs/canonical-hashing.md");
    let start = doc.find("``{action,").expect("the T1.9 schema list") + 3;
    let end = start + doc[start..].find("}``").expect("the list ends");
    doc[start..end]
        .split(',')
        .map(|k| k.trim().to_string())
        .collect()
}
