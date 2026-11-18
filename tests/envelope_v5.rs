//! The two new digests and the v5 envelope's library seam (ADR 0056 decisions 6 and 7, task F-2):
//! `req::model::{fields_digest, pins_digest}`, `req::baseline::{manifest, canonical_bytes, from_json,
//! drift}` and `Drift::reasons`. The compiled CLI is `tests/envelope_v5_cli.rs`.
//!
//! Oracles are independent of the code under test. Each digest below was computed with Python's
//! `hashlib` from the byte string decision 6 specifies, and the command is in the comment above it:
//!
//! ```text
//! python3 -I -c 'import hashlib,sys; print(hashlib.sha256(b"req.fields.v1\0" + sys.argv[1].encode()).hexdigest())' '<json>'
//! ```
//!
//! * L7 — determinism over an oracle, the list reversed and the fields given in another order;
//! * L8 — the same bytes under the five tags give five distinct digests;
//! * L9 — a v4 manifest has the bytes it had before this build;
//! * Falsification 5 — an absent field and an empty list hash differently, two surfaces never trade a digest.

mod common;

use req::baseline::{
    BASELINE_VERSION, FIELDS_ATTESTED_FROM, PINS_ATTESTED_FROM, Surface, TESTS_ATTESTED_FROM,
    attest, canonical_bytes, drift, from_json, manifest,
};
use req::canon::RULES_V;
use req::decision;
use req::kinds::{FieldValue, JudgedField};
use req::model::{
    FIELDS_TAG, PINS_TAG, Pins, RATIONALE_TAG, RELATIONS_TAG, TESTS_TAG, fields_digest, pins_digest,
};
use req::select::{Applies, Context};

/// `sha256(tag ‖ bytes)` as lowercase hex, written here with the `sha2` crate and not through the
/// code under test.
fn sha256_prefixed(tag: &[u8], bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(tag);
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn field(
    name: &str,
    type_name: &'static str,
    of: Option<&'static str>,
    value: FieldValue,
) -> JudgedField {
    JudgedField {
        name: name.to_string(),
        type_name,
        of,
        value,
    }
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

fn list(items: &[&str]) -> FieldValue {
    FieldValue::List(items.iter().map(|s| (*s).to_string()).collect())
}

/// Every type the loader delivers, with a string that needs escaping, in the order the ADR's byte rule
/// sorts them (by byte: `review_due` before `reviewed`, because `_` is 0x5f and `e` is 0x65).
fn every_type() -> Vec<JudgedField> {
    vec![
        field("asil", "enum", None, text("C")),
        field("count", "integer", None, text("-12")),
        field(
            "mitigated_by",
            "list",
            Some("ref"),
            list(&["REQ-0102", "REQ-0107"]),
        ),
        field("note", "string", None, text("a \"quoted\" note")),
        field("ratio", "decimal", None, text("0.3")),
        field("review_due", "date", None, text("2027-03-01")),
        field("reviewed", "bool", None, FieldValue::Bool(true)),
    ]
}

/// `{"kind":"safety","fields":{"asil":{"type":"enum","value":"C"},"count":{"type":"integer","value":"-12"},
/// "mitigated_by":{"type":"list","of":"ref","value":["REQ-0102","REQ-0107"]},"note":{"type":"string",
/// "value":"a \"quoted\" note"},"ratio":{"type":"decimal","value":"0.3"},"review_due":{"type":"date",
/// "value":"2027-03-01"},"reviewed":{"type":"bool","value":true}}}` under `req.fields.v1\0`.
const EVERY_TYPE_DIGEST: &str = "b326493a76f6cafe09b0e65554867dbe271ff55c1f559fbc2e43bc276b2d9d63";
/// `{"kind":null,"fields":{}}` under `req.fields.v1\0`.
const NO_KIND_DIGEST: &str = "bd8eccf8db138c1b17e3ecfa21353e83bb838fd0e25bb0ab05440b32969a7318";
/// `{"kind":"safety","fields":{}}` under `req.fields.v1\0`.
const KIND_ONLY_DIGEST: &str = "2a7289fd55c4017c0c093e4095287d766a85e8afcdc75088a3ffc9e362b45ee2";
/// `{"kind":"safety","fields":{"mitigated_by":{"type":"list","of":"ref","value":[]}}}` under `req.fields.v1\0`.
const EMPTY_LIST_DIGEST: &str = "1a055b39f8b69a5e704f828328abb74866205ddde61b69103577fcc08b68f1d1";
/// `[["modality","SHALL"],["subject","SUBJ-1"],["operator","LE"],["value","0.2"],["unit","s"]]` under `req.pins.v1\0`.
const ALL_PINS_DIGEST: &str = "1c2e51bd8edb24f9a8e17e1695e2ca3f3fef2132b1e38faf1cc056012d9f3720";
/// `[["modality","SHALL"],["value","0.2"]]` under `req.pins.v1\0`.
const TWO_PINS_DIGEST: &str = "5fc0800dfe36c5c777276b7658fd0e2e798beacdb4ad1ba256f8dfeff8dacd12";
/// `[]` under `req.pins.v1\0`.
const NO_PINS_DIGEST: &str = "5cbd189c50e965f5a3d0b885802ef340255946b2fb3fae1d58c3345dacf19e04";

// --- L7: determinism over an independent oracle -----------------------------------------------------------

#[test]
fn the_fields_digest_matches_an_independent_oracle() {
    assert_eq!(
        fields_digest(Some("safety"), &every_type()),
        EVERY_TYPE_DIGEST
    );
}

#[test]
fn the_fields_digest_does_not_depend_on_the_order_fields_or_list_elements_are_given_in() {
    let mut reordered = every_type();
    reordered.reverse();
    for judged in &mut reordered {
        if let FieldValue::List(items) = &mut judged.value {
            items.reverse();
        }
    }
    assert_eq!(
        reordered[4].value,
        list(&["REQ-0107", "REQ-0102"]),
        "the fixture really is reversed"
    );
    assert_eq!(
        fields_digest(Some("safety"), &reordered),
        EVERY_TYPE_DIGEST,
        "fields sort by byte and a list is a set: sorted at hash time (the way `tests` is)"
    );
}

#[test]
fn a_claim_with_no_kind_and_no_fields_hashes_the_null_kind_object() {
    assert_eq!(fields_digest(None, &[]), NO_KIND_DIGEST);
    assert_eq!(fields_digest(Some("safety"), &[]), KIND_ONLY_DIGEST);
    assert_ne!(
        fields_digest(None, &[]),
        fields_digest(Some("safety"), &[]),
        "adding a kind later is a change"
    );
}

#[test]
fn the_pins_digest_matches_an_independent_oracle_over_the_keys_present() {
    let all = Pins {
        modality: Some("SHALL".into()),
        subject: Some("SUBJ-1".into()),
        operator: Some("LE".into()),
        value: Some("0.2".into()),
        unit: Some("s".into()),
    };
    assert_eq!(pins_digest(Some(&all)), ALL_PINS_DIGEST);
    let two = Pins {
        modality: Some("SHALL".into()),
        value: Some("0.2".into()),
        ..Pins::default()
    };
    assert_eq!(pins_digest(Some(&two)), TWO_PINS_DIGEST);
}

#[test]
fn an_empty_pins_map_and_no_pins_hash_alike() {
    assert_eq!(pins_digest(None), NO_PINS_DIGEST);
    assert_eq!(pins_digest(Some(&Pins::default())), NO_PINS_DIGEST);
}

#[test]
fn deleting_one_pin_changes_the_pins_digest() {
    let full = Pins {
        modality: Some("SHALL".into()),
        value: Some("0.2".into()),
        ..Pins::default()
    };
    let deleted = Pins {
        modality: Some("SHALL".into()),
        ..Pins::default()
    };
    assert_ne!(pins_digest(Some(&full)), pins_digest(Some(&deleted)));
    assert_ne!(pins_digest(Some(&deleted)), pins_digest(None));
}

// --- falsification 5 ------------------------------------------------------------------------------------

#[test]
fn an_absent_field_and_an_empty_list_hash_differently() {
    let absent = fields_digest(Some("safety"), &[]);
    let empty = fields_digest(
        Some("safety"),
        &[field("mitigated_by", "list", Some("ref"), list(&[]))],
    );
    assert_eq!(empty, EMPTY_LIST_DIGEST);
    assert_ne!(absent, empty, "absent is UNKNOWN, an empty set is a value");
}

#[test]
fn the_type_tag_rides_in_the_hash() {
    let as_string = fields_digest(Some("k"), &[field("v", "string", None, text("3"))]);
    let as_integer = fields_digest(Some("k"), &[field("v", "integer", None, text("3"))]);
    assert_ne!(
        as_string, as_integer,
        "a field redeclared as another type changes the digest"
    );
    let ref_list = fields_digest(
        Some("k"),
        &[field("v", "list", Some("ref"), list(&["REQ-1"]))],
    );
    let string_list = fields_digest(
        Some("k"),
        &[field("v", "list", Some("string"), list(&["REQ-1"]))],
    );
    assert_ne!(ref_list, string_list, "and so does the element type");
}

// --- L8: domain separation --------------------------------------------------------------------------------

#[test]
fn the_same_bytes_under_the_five_tags_give_five_distinct_digests() {
    let tags: [(&str, &[u8]); 5] = [
        ("relations", RELATIONS_TAG),
        ("rationale", RATIONALE_TAG),
        ("tests", TESTS_TAG),
        ("fields", FIELDS_TAG),
        ("pins", PINS_TAG),
    ];
    let mut seen = std::collections::BTreeMap::new();
    for (name, tag) in tags {
        let digest = sha256_prefixed(tag, b"[]");
        if let Some(other) = seen.insert(digest, name) {
            panic!("{name} and {other} trade a digest over the same bytes");
        }
    }
    assert_eq!(FIELDS_TAG, b"req.fields.v1\0");
    assert_eq!(PINS_TAG, b"req.pins.v1\0");
}

#[test]
fn two_surfaces_never_trade_a_digest_over_what_serialises_alike() {
    // `[]` is what an empty relations list, an empty pins map, and an empty set all serialise to.
    assert_ne!(req::model::relations_digest(&[]), pins_digest(None));
    // And the empty fields object is its own bytes, under its own tag.
    assert_ne!(
        fields_digest(None, &[]),
        sha256_prefixed(TESTS_TAG, br#"{"kind":null,"fields":{}}"#)
    );
    assert_eq!(
        fields_digest(None, &[]),
        sha256_prefixed(FIELDS_TAG, br#"{"kind":null,"fields":{}}"#),
        "the digest is exactly `sha256(FIELDS_TAG || bytes)`"
    );
}

// --- the manifest ------------------------------------------------------------------------------------------

const SIGNED_AT: &str = "2026-11-18T09:00:00Z";

fn fixture_head() -> decision::Head {
    decision::Head {
        hash: "5cd6c7ff145231c134616228dcc0a8c80ac28e9842d77b20e83f502b59bf6978".to_string(),
        entries: 1,
    }
}

fn signed_rows() -> (Vec<req::view::Row>, req::baseline::Manifest) {
    let rows = vec![
        common::row("REQ-A", Applies::Yes, &[]),
        common::row("REQ-B", Applies::Yes, &[]),
    ];
    let mut m = manifest(&rows, RULES_V, &Context::new());
    attest(&mut m, SIGNED_AT, &fixture_head()).expect("a valid attestation");
    (rows, m)
}

#[test]
fn this_build_writes_envelope_v5_and_the_constants_name_where_each_hash_began() {
    assert_eq!(BASELINE_VERSION, 5);
    assert_eq!(TESTS_ATTESTED_FROM, 4);
    assert_eq!(FIELDS_ATTESTED_FROM, 5);
    assert_eq!(PINS_ATTESTED_FROM, 5);
    assert_eq!(
        req::baseline::OLDEST_VERIFIABLE_BASELINE_VERSION,
        3,
        "the support floor does not move"
    );
}

#[test]
fn a_v5_manifest_appends_fields_hash_then_pins_hash_after_tests_hash() {
    let (_, m) = signed_rows();
    let bytes = canonical_bytes(&m);
    let record = |id: &str| {
        format!(
            "{{\"claim_id\":\"{id}\",\"address\":\"{id:0>12}\",\"relations_hash\":\"{}\",\
             \"rationale_hash\":\"{}\",\"tests_hash\":\"{}\",\"fields_hash\":\"{NO_KIND_DIGEST}\",\
             \"pins_hash\":\"{NO_PINS_DIGEST}\"}}",
            common::EMPTY_RELATIONS_DIGEST,
            common::row("X", Applies::Yes, &[]).rationale_hash,
            common::EMPTY_TESTS_DIGEST,
        )
    };
    assert!(
        bytes.starts_with("{\"version\":5,\"signed_at\":\"2026-11-18T09:00:00Z\","),
        "{bytes}"
    );
    assert!(
        bytes.ends_with(&format!(
            "\"claims\":[{},{}]}}\n",
            record("REQ-A"),
            record("REQ-B")
        )),
        "the two keys follow `tests_hash`, in that order:\n{bytes}"
    );
    let parsed = from_json(&bytes).expect("a v5 manifest reads back");
    assert_eq!(parsed, m);
}

#[test]
fn a_v5_manifest_missing_either_hash_is_refused_naming_it() {
    let (_, m) = signed_rows();
    for (label, strip) in [("fields_hash", true), ("pins_hash", false)] {
        let mut stripped = m.clone();
        for claim in &mut stripped.claims {
            if strip {
                claim.fields_hash = None;
            } else {
                claim.pins_hash = None;
            }
        }
        // The bytes of a manifest with the key dropped from its records, as a hand edit leaves them.
        let mut text = canonical_bytes(&m);
        let needle = if strip {
            format!(",\"fields_hash\":\"{NO_KIND_DIGEST}\"")
        } else {
            format!(",\"pins_hash\":\"{NO_PINS_DIGEST}\"")
        };
        assert!(text.contains(&needle));
        text = text.replace(&needle, "");
        let err = from_json(&text).expect_err("a v5 manifest must carry both hashes");
        assert!(err.contains(label), "{label}: {err}");
        assert!(err.contains("re-sign"), "{err}");
    }
}

#[test]
fn a_v5_manifest_missing_tests_hash_is_still_refused() {
    let (_, m) = signed_rows();
    let text = canonical_bytes(&m).replace(
        &format!(",\"tests_hash\":\"{}\"", common::EMPTY_TESTS_DIGEST),
        "",
    );
    let err = from_json(&text).expect_err("tests_hash is required from v4 on");
    assert!(err.contains("tests_hash"), "{err}");
}

/// L9. A v4 manifest keeps the bytes it was signed over: neither new key is appended, and a record
/// that carries them does not gain them in the signed bytes.
#[test]
fn a_v4_manifest_keeps_its_bytes_under_this_build() {
    let v4 = format!(
        "{{\"version\":4,\"signed_at\":\"{SIGNED_AT}\",\"authority\":{{\"head\":\"{}\",\"entries\":1}},\
         \"rules_v\":9,\"selection\":{{}},\"claims\":[{{\"claim_id\":\"REQ-A\",\"address\":\"aaa\",\
         \"relations_hash\":\"{}\",\"rationale_hash\":\"{}\",\"tests_hash\":\"{}\"}}]}}\n",
        fixture_head().hash,
        common::EMPTY_RELATIONS_DIGEST,
        common::row("X", Applies::Yes, &[]).rationale_hash,
        common::EMPTY_TESTS_DIGEST,
    );
    let parsed = from_json(&v4).expect("a v4 manifest still parses");
    assert_eq!(parsed.version, 4);
    assert_eq!(
        canonical_bytes(&parsed),
        v4,
        "the v4 bytes are the signed ones"
    );

    // The same manifest carrying both new keys: they are left out of what is signed.
    let mut carrying = parsed.clone();
    carrying.claims[0].fields_hash = Some(NO_KIND_DIGEST.to_string());
    carrying.claims[0].pins_hash = Some(NO_PINS_DIGEST.to_string());
    assert_eq!(
        canonical_bytes(&carrying),
        v4,
        "a v4 manifest's signed bytes do not gain keys because the record carries them"
    );
}

#[test]
fn the_committed_baselines_are_byte_for_byte_the_bytes_this_build_writes_for_their_version() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baselines");
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).expect("reads baselines/") {
        let path = entry.expect("an entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("reads");
        let parsed = from_json(&text).expect("a committed manifest parses");
        assert_eq!(
            parsed.version,
            4,
            "{} is not re-emitted by this unit",
            path.display()
        );
        assert_eq!(
            canonical_bytes(&parsed),
            text,
            "{}: the bytes its signature covers",
            path.display()
        );
        seen += 1;
    }
    assert_eq!(seen, 3);
}

// --- drift: which surface moved --------------------------------------------------------------------------

fn changed(rows: &[req::view::Row], m: &req::baseline::Manifest) -> Vec<(String, Vec<Surface>)> {
    drift(rows, m)
        .attested_changed
        .into_iter()
        .map(|c| (c.claim_id, c.surfaces))
        .collect()
}

#[test]
fn drift_records_which_surface_changed_for_each_claim() {
    let (rows, m) = signed_rows();
    assert!(drift(&rows, &m).is_clean());

    let mut edited = rows.clone();
    edited[0].fields_hash = "f".repeat(64);
    assert_eq!(
        changed(&edited, &m),
        vec![("REQ-A".to_string(), vec![Surface::Fields])]
    );

    let mut edited = rows.clone();
    edited[1].pins_hash = "f".repeat(64);
    assert_eq!(
        changed(&edited, &m),
        vec![("REQ-B".to_string(), vec![Surface::Pins])]
    );

    let mut edited = rows.clone();
    edited[0].relations_hash = "f".repeat(64);
    edited[0].tests_hash = "f".repeat(64);
    edited[0].pins_hash = "f".repeat(64);
    assert_eq!(
        changed(&edited, &m),
        vec![(
            "REQ-A".to_string(),
            vec![Surface::Relations, Surface::Tests, Surface::Pins]
        )],
        "the surfaces are listed in the fixed order, relations, rationale, tests, fields, pins"
    );
}

#[test]
fn an_address_change_stays_alone_and_precedes_the_attested_surface() {
    let (rows, m) = signed_rows();
    let mut edited = rows.clone();
    edited[0].address = "moved".to_string();
    edited[0].fields_hash = "f".repeat(64);
    let d = drift(&edited, &m);
    assert_eq!(d.changed, vec!["REQ-A".to_string()]);
    assert!(
        d.attested_changed.is_empty(),
        "the categories stay exclusive, by precedence"
    );
}

#[test]
fn the_reasons_name_the_surface_in_a_fixed_order_and_never_a_hash() {
    let (rows, m) = signed_rows();
    let mut edited = rows.clone();
    edited[0].rationale_hash = "a".repeat(64);
    edited[0].fields_hash = "b".repeat(64);
    let reasons = drift(&edited, &m).reasons();
    assert_eq!(
        reasons,
        vec![
            "REQ-A: the rationale and the fields no longer recompute — edited after signing \
             (ADR 0013, ADR 0019, ADR 0056)"
                .to_string()
        ]
    );
    for reason in &reasons {
        assert!(
            !reason.contains(&"a".repeat(8)) && !reason.contains(&"b".repeat(8)),
            "{reason}"
        );
    }

    let mut three = rows.clone();
    three[0].relations_hash = "a".repeat(64);
    three[0].rationale_hash = "a".repeat(64);
    three[0].pins_hash = "a".repeat(64);
    assert_eq!(
        drift(&three, &m).reasons(),
        vec![
            "REQ-A: the relations, the rationale and the pins no longer recompute — edited after \
             signing (ADR 0013, ADR 0019, ADR 0056)"
                .to_string()
        ]
    );

    let mut only = rows.clone();
    only[1].tests_hash = "a".repeat(64);
    let reasons = drift(&only, &m).reasons();
    assert_eq!(reasons.len(), 1);
    assert!(
        reasons[0].starts_with(
            "REQ-B: the authored test links no longer recompute — edited after signing"
        ),
        "{reasons:?}"
    );
    assert!(!reasons[0].contains("ADR 0056"), "{reasons:?}");
}

// --- the window each hash is judged in -----------------------------------------------------------------------

#[test]
fn the_new_hashes_are_judged_only_inside_their_closed_window() {
    let (rows, m) = signed_rows();
    let mut edited = rows.clone();
    edited[0].fields_hash = "f".repeat(64);
    edited[0].pins_hash = "f".repeat(64);

    // v4: it did not sign them, so it cannot judge them — even when the record carries them.
    let mut v4 = m.clone();
    v4.version = FIELDS_ATTESTED_FROM - 1;
    assert!(drift(&edited, &v4).is_clean(), "{:?}", drift(&edited, &v4));
    for claim in &mut v4.claims {
        claim.fields_hash = Some("x".repeat(64));
        claim.pins_hash = Some("x".repeat(64));
    }
    assert!(
        drift(&edited, &v4).is_clean(),
        "an unsigned field invents no tampering"
    );

    // v5 judges them ...
    assert_eq!(drift(&edited, &m).attested_changed.len(), 1);

    // ... and a future envelope's hash is not this build's to compare, so it is never `>=`.
    let mut future = m.clone();
    future.version = BASELINE_VERSION + 1;
    assert!(
        drift(&edited, &future).is_clean(),
        "{:?}",
        drift(&edited, &future)
    );
}
