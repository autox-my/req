//! Every key a claim file can carry has a classification, and an attested one is wired (ADR 0056
//! decision 5.2, task F-2; *Falsification* 8).
//!
//! The research behind the ADR found the missing piece: nothing enumerated `ClaimFile`'s keys, so a key
//! added to the struct was attested, or deliberately not, by whoever remembered to say so. This test
//! serialises a fully populated `ClaimFile`, requires each top-level key to be in **exactly one** of
//! three lists — identity, attested, or the named unattested allowlist, each with its reason — and, for
//! every attested key, changes it after the manifest is emitted and requires `drift` to see it. A key in
//! the attested list that no digest covers fails there, so a classification is never a label alone.
//!
//! It must not pass vacuously, so the serialised key count is asserted against the lists' total and the
//! checker is itself run over a key it does not know (the in-test mutation). The mutation the envelope
//! records is the real one: add a field to `ClaimFile`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use req::baseline::{attest, drift, manifest};
use req::canon::RULES_V;
use req::decision::Head;
use req::model::{ClaimFile, Evidence, Pins, Rationale, Relation};
use req::select::Context;

/// A key that is part of what a claim *means*: the address is derived from it.
const IDENTITY: &[(&str, &str)] = &[
    (
        "statement",
        "the canonical form is derived from it; a change moves the address",
    ),
    (
        "applicability",
        "an input to the canonical form (ADR 0003): a change moves the address",
    ),
];

/// A key a Baseline attests, without moving the address.
const ATTESTED: &[(&str, &str)] = &[
    ("relations", "relations_hash (ADR 0013)"),
    ("rationale", "rationale_hash (ADR 0013)"),
    ("tests", "tests_hash (ADR 0019)"),
    (
        "evidence",
        "tests_hash: the evidence mode rides the same digest as the links (ADR 0019)",
    ),
    (
        "kind",
        "fields_hash: the claim's class (ADR 0056 decision 6)",
    ),
    ("fields", "fields_hash (ADR 0056 decision 6)"),
    (
        "pins",
        "pins_hash (ADR 0056 decision 7, which closes ADR 0022's named loss)",
    ),
];

/// A key the Baseline deliberately does not attest, each by the decision that says so.
const UNATTESTED: &[(&str, &str)] = &[
    (
        "area",
        "placement only, outside the Baseline on purpose (ADR 0051 decision 4)",
    ),
    (
        "schema",
        "the manifest carries no claim schema, so a format bump moves no signature (ADR 0022)",
    ),
    (
        "claim_id",
        "the id the manifest records beside the address: the record's own key, not a surface",
    ),
];

/// A claim with **every** key present. An exhaustive literal, with no `..` base: a field added to
/// `ClaimFile` stops this compiling, which is the first line of the mutation check.
fn fully_populated() -> ClaimFile {
    ClaimFile {
        schema: "req.claim/4".to_string(),
        claim_id: "REQ-0300".to_string(),
        statement: "The service shall respond to a health check within 200 ms.".to_string(),
        applicability: vec!["deployment_mode == air_gapped".to_string()],
        relations: vec![Relation {
            kind: "refines".to_string(),
            to: "REQ-0301".to_string(),
        }],
        tests: vec!["tests::health::responds".to_string()],
        evidence: Evidence::Machine,
        rationale: Rationale {
            source: "docs/a.md".to_string(),
            date: "2026-11-18".to_string(),
            rejected_alternatives: vec!["none".to_string()],
            assumptions: vec!["one".to_string()],
        },
        pins: Some(Pins {
            modality: Some("SHALL".to_string()),
            operator: Some("LE".to_string()),
            value: Some("0.2".to_string()),
            unit: Some("s".to_string()),
            subject: None,
        }),
        area: Some("diagnostics/startup".to_string()),
        kind: Some("safety".to_string()),
        fields: BTreeMap::from([(
            "asil".to_string(),
            yaml_serde::Value::String("C".to_string()),
        )]),
    }
}

fn serialised_keys(claim: &ClaimFile) -> BTreeSet<String> {
    serde_json::to_value(claim)
        .expect("a claim file serialises")
        .as_object()
        .expect("a claim file is an object")
        .keys()
        .cloned()
        .collect()
}

/// What is wrong with a classification of `keys`: a key in no list, a key in two, a listed key that
/// serialises nowhere. Empty when the classification is exactly right.
fn classification_errors(keys: &BTreeSet<String>) -> Vec<String> {
    let lists = [
        ("identity", IDENTITY),
        ("attested", ATTESTED),
        ("unattested", UNATTESTED),
    ];
    let mut errors = Vec::new();
    for key in keys {
        let homes: Vec<&str> = lists
            .iter()
            .filter(|(_, entries)| entries.iter().any(|(name, _)| name == key))
            .map(|(list, _)| *list)
            .collect();
        match homes.as_slice() {
            [] => errors.push(format!(
                "`{key}` is in no list: say whether it is identity, attested or named unattested"
            )),
            [_] => {}
            many => errors.push(format!(
                "`{key}` is in {} lists: {}",
                many.len(),
                many.join(", ")
            )),
        }
    }
    for (list, entries) in lists {
        for (name, reason) in entries {
            if !keys.contains(*name) {
                errors.push(format!(
                    "`{name}` ({list}) names no key a claim file serialises"
                ));
            }
            if reason.trim().is_empty() {
                errors.push(format!("`{name}` ({list}) carries no reason"));
            }
        }
    }
    errors
}

#[test]
fn every_serialised_key_is_in_exactly_one_list() {
    let keys = serialised_keys(&fully_populated());
    let errors = classification_errors(&keys);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn the_check_does_not_pass_vacuously() {
    let keys = serialised_keys(&fully_populated());
    assert_eq!(
        keys.len(),
        IDENTITY.len() + ATTESTED.len() + UNATTESTED.len(),
        "the lists total exactly the keys a fully populated claim serialises: {keys:?}"
    );
    assert_eq!(keys.len(), 12, "twelve keys today");
    // The mutation, in the check itself: a key added to the claim file and to no list is reported.
    let mut with_new_key = keys.clone();
    with_new_key.insert("sign_off".to_string());
    let errors = classification_errors(&with_new_key);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("`sign_off` is in no list")),
        "{errors:?}"
    );
    // ... and so is a classification that lists a key twice, or one that is not a key.
    let mut doubled = keys.clone();
    doubled.remove("area");
    let errors = classification_errors(&doubled);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("`area` (unattested) names no key")),
        "{errors:?}"
    );
}

// --- the behavioural leg: an attested key is wired --------------------------------------------------------

struct Corpus(PathBuf);

impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const KIND_FIELDS: &str = "\
fields:
  asil:
    type: enum
    values: [QM, A, B, C, D]
";

impl Corpus {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "claim-file-coverage-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let registry = dir.join("aliases/subjects.yml");
        fs::create_dir_all(registry.parent().expect("a parent")).expect("makes the dir");
        fs::copy(repo.join("aliases/subjects.yml"), registry).expect("copies the registry");
        for kind in ["safety", "quality"] {
            let to = dir.join(format!(".req/kinds/{kind}.yml"));
            fs::create_dir_all(to.parent().expect("a parent")).expect("makes the dir");
            fs::write(to, format!("schema: req.kind/1\n{KIND_FIELDS}")).expect("writes the kind");
        }
        // The relation target, so the corpus is whole; it is not attested-changed by anything below.
        let corpus = Corpus(dir);
        corpus.write_claim(&ClaimFile {
            schema: "req.claim/4".to_string(),
            claim_id: "REQ-0301".to_string(),
            statement: "The service shall log each request.".to_string(),
            applicability: Vec::new(),
            relations: Vec::new(),
            tests: Vec::new(),
            evidence: Evidence::Process,
            rationale: Rationale {
                source: "docs/b.md".to_string(),
                date: "2026-11-18".to_string(),
                rejected_alternatives: Vec::new(),
                assumptions: Vec::new(),
            },
            pins: None,
            area: None,
            kind: None,
            fields: BTreeMap::new(),
        });
        corpus
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn write_claim(&self, claim: &ClaimFile) {
        let path = self.0.join(format!("claims/{}.yml", claim.claim_id));
        fs::create_dir_all(path.parent().expect("a parent")).expect("makes the dir");
        fs::write(
            path,
            yaml_serde::to_string(claim).expect("a claim serialises"),
        )
        .expect("writes");
    }
}

/// The claim the legs start from: every attested key at a value the leg can change, and the evidence
/// machine with no links (so adding one is a change of `tests`, flipping the mode one of `evidence`).
fn base() -> ClaimFile {
    ClaimFile {
        tests: Vec::new(),
        applicability: Vec::new(),
        ..fully_populated()
    }
}

/// One change per attested key: the key, and what to do to the claim so only it moves.
fn mutations() -> BTreeMap<&'static str, fn(&mut ClaimFile)> {
    let mut m: BTreeMap<&'static str, fn(&mut ClaimFile)> = BTreeMap::new();
    m.insert("relations", |c| {
        c.relations.push(Relation {
            kind: "refines".to_string(),
            to: "REQ-0301".to_string(),
        })
    });
    m.insert("rationale", |c| {
        c.rationale.source = "docs/other.md".to_string()
    });
    m.insert("tests", |c| {
        c.tests.push("tests::health::other".to_string())
    });
    m.insert("evidence", |c| c.evidence = Evidence::Process);
    m.insert("kind", |c| c.kind = Some("quality".to_string()));
    m.insert("fields", |c| {
        c.fields.insert(
            "asil".to_string(),
            yaml_serde::Value::String("D".to_string()),
        );
    });
    m.insert("pins", |c| {
        if let Some(pins) = c.pins.as_mut() {
            pins.unit = None;
        }
    });
    m
}

fn emitted_then(change: fn(&mut ClaimFile)) -> req::baseline::Drift {
    let corpus = Corpus::new();
    let mut claim = base();
    claim.relations = Vec::new();
    claim.fields = BTreeMap::from([(
        "asil".to_string(),
        yaml_serde::Value::String("C".to_string()),
    )]);
    corpus.write_claim(&claim);
    let context = Context::new();
    let rows = req::view::load_rows(&context, corpus.root()).expect("the corpus loads");
    let mut signed = manifest(&rows, RULES_V, &context);
    attest(
        &mut signed,
        "2026-11-18T09:00:00Z",
        &Head {
            hash: "0".repeat(64),
            entries: 1,
        },
    )
    .expect("a valid attestation");
    assert!(
        drift(&rows, &signed).is_clean(),
        "the untouched corpus is clean before the change"
    );
    change(&mut claim);
    corpus.write_claim(&claim);
    let after = req::view::load_rows(&context, corpus.root()).expect("the edited corpus loads");
    drift(&after, &signed)
}

#[test]
fn every_attested_key_has_a_change_that_drift_reports() {
    let wired = mutations();
    let listed: BTreeSet<&str> = ATTESTED.iter().map(|(name, _)| *name).collect();
    let tested: BTreeSet<&str> = wired.keys().copied().collect();
    assert_eq!(
        listed, tested,
        "each attested key has exactly one behavioural leg, and each leg is for an attested key"
    );
    for (key, change) in wired {
        let d = emitted_then(change);
        assert!(
            d.changed.is_empty(),
            "`{key}` is outside identity: it must not move the address, so it must not be `changed`: {d:?}"
        );
        assert_eq!(
            d.attested_changed.len(),
            1,
            "changing `{key}` after the manifest was emitted is invisible to the Baseline: {d:?}"
        );
        assert!(!d.is_clean(), "{key}");
    }
}

/// `area` is the allowlist's one key a claim can change in place and still load: `schema` cannot move
/// without the file's format changing (a `/3` file may not carry `kind`), and `claim_id` is the file's
/// own name. Re-filing a claim needs no signature (ADR 0051 decision 4).
#[test]
fn a_named_unattested_key_is_not_judged() {
    let area = emitted_then(|c| c.area = Some("elsewhere/else".to_string()));
    assert!(
        area.is_clean(),
        "area is placement, outside the Baseline on purpose: {area:?}"
    );
}
