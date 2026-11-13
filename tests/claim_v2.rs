//! `req.claim/2` (ADR 0022): a claim is authored, its parse is derived at load, and no
//! fingerprint is stored in a claim file.
//!
//! Every test drives the compiled binary — the public seam a re-implementation and an adopter
//! both read — against either the committed corpus or a throwaway fixture corpus. The
//! expectations are independent of the implementation:
//!
//! * (a) the 21 dogfood addresses are the list recorded **before** the migration (base commit
//!   3935d0b, rules v6), from each file's stored `address:` and from `req render`, which agreed;
//!   `baselines/air-gapped.json` attests 17 of them with the same bytes;
//! * (h) the REQ-0020 pair differs by one meaning-bearing word, by construction;
//! * every other case states its input and the verdict the spec names for it.

mod common;

use std::path::{Path, PathBuf};

/// The claims re-worded after the migration, and the address each now derives. The migration
/// itself moved none; REQ-0017 was then re-worded on purpose, because its old statement (each
/// claim file stores its canonical form and address) became false under `req.claim/2` — the CI
/// review of M3 found it. REQ-0007 was re-worded by M4-1 (the owner split off its decision-boundary
/// half as REQ-0023, which is not in the recorded list). The new values are the ones the re-emitted
/// Baseline carries.
const RE_WORDED_AFTER_MIGRATION: &[(&str, &str)] = &[
    (
        "REQ-0007",
        "d5f74f33838ed8d38007808350f74faba5d6f93600ff0bc354788f9b294b4d88",
    ),
    (
        "REQ-0017",
        "f0c1c332f895c8bf94534b133f65fa6866c8a145e83690dd8b252a79a67e784d",
    ),
];

/// The 21 claims' `claim_id → address`, recorded at 3935d0b (rules v6) before anything was
/// edited. A pure format change moves none of them: this list is the proof (ADR 0022), except
/// for the ids in `RE_WORDED_AFTER_MIGRATION`.
const PRE_MIGRATION_ADDRESSES: &[(&str, &str)] = &[
    (
        "REQ-0001",
        "f559f5a905e4b8d595d064ec960aa695450c84176f9e1577cf2a060904e89e28",
    ),
    (
        "REQ-0002",
        "6e6ba2ec76ee4ebd98d2c1e6a6257f4f0a7bfcbc714f216d314e70d3f83e3d90",
    ),
    (
        "REQ-0003",
        "57364f54a906400f9e97858ef705c781053009ae5fd756c509362441f9faacc3",
    ),
    (
        "REQ-0004",
        "ddc6992dd05b4da58ae21e5dda821d488f7fcaea04b9a5849ab8d11e00f2bf80",
    ),
    (
        "REQ-0005",
        "6bbc0295880a0f773542804c0cfc5c702f368f8fbf2b7670327fda247f2a75dd",
    ),
    (
        "REQ-0006",
        "94d043e0d3c600fe82f03ceaf4dffdd2cf89bb03f55fc32ecffba38f598d7f5a",
    ),
    (
        "REQ-0007",
        "7689bfe6d984cbee345e532d800b43b83018210c4c62dca0e2a59f89201a6c78",
    ),
    (
        "REQ-0008",
        "29decbdac12baf3df364bd398a86507db5de701671bb910322cae32440fd09c9",
    ),
    (
        "REQ-0009",
        "9f25a65419a3e25d5d6a6e9a60c8b8e4a45390242450e04a3a2bfa277f45969d",
    ),
    (
        "REQ-0010",
        "e4f9b487832690c76b49562f9b865571e1f22bd2e0b0b4413ba4f0dc86ddaaf2",
    ),
    (
        "REQ-0012",
        "efa29f8ef62d9178e6f9d74b7dce6d058a2f01f805f132bc7d8e7da574427751",
    ),
    (
        "REQ-0013",
        "170b4ba0205cded0ab4c1d2d2d783cc4ab356e024d989b22d269fb79e3fe7912",
    ),
    (
        "REQ-0014",
        "4e99cbc538f719368ee8fcb6a3f273e5c05ee60968cb0b97517215156bf65281",
    ),
    (
        "REQ-0015",
        "4a15b28c8eba8717efbc27310119ca1968a43c3bd04e1363795838fc8363a319",
    ),
    (
        "REQ-0016",
        "04836a98d468416483e5c0d714e3e190c76c6142208cea5fdb7b129cfddf8bda",
    ),
    (
        "REQ-0017",
        "eeaaeb19f0686e8ba5a04559f3c40c33797e5f0243d7179c341f7f3f70a3510e",
    ),
    (
        "REQ-0018",
        "5e95be2d018feee258010c1bdfd049a842b058dd526c377f168dff16ca7c829a",
    ),
    (
        "REQ-0019",
        "25713319754e957b274d2826061e6239004555867b0273d5af6241dac1cbf1ec",
    ),
    (
        "REQ-0020",
        "b455b84714c2f2fe8c9ca64dc925467112c24317a739b47262b74a8b32d2696a",
    ),
    (
        "REQ-0021",
        "fc5e269b978ec10df17be91824ef807e0803a8f04ca9c216a21d9e942f166783",
    ),
    (
        "REQ-0022",
        "01e9dc781ad7e2e762d9d7d5c0358e28799bf506b1d7e5af397af55bf25bf201",
    ),
];

/// The keys `req.claim/1` carried that `req.claim/2` removed (ADR 0022). Stated from the spec,
/// not read off the model, so the loader's list cannot shrink with nothing failing.
const REMOVED_V1_KEYS: &[&str] = &[
    "modality",
    "subject",
    "attribute",
    "operator",
    "value",
    "unit",
    "conditions",
    "exception",
    "temporal",
    "quantifier",
    "unstructured",
    "canonical",
    "address",
];

fn checkout() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn both(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

fn req(root: &Path, args: &[&str]) -> Outcome {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_req"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A throwaway corpus: this repository's own subject registry (so `the req tool` resolves as it
/// does for the dogfood claims) and whatever claims a test writes.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("claim-v2-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("claims")).expect("claims dir");
        std::fs::create_dir_all(root.join("aliases")).expect("aliases dir");
        std::fs::copy(
            checkout().join("aliases/subjects.yml"),
            root.join("aliases/subjects.yml"),
        )
        .expect("the repository's registry copies");
        Fixture { root }
    }

    fn claim(self, id: &str, text: &str) -> Self {
        std::fs::write(self.root.join("claims").join(format!("{id}.yml")), text)
            .expect("claim file");
        self
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A `req.claim/2` file: the authored fields, `evidence: process` so the gate's evidence policy
/// is settled without a report, and an optional trailing block (`pins:` or a stray key).
fn v2_claim(id: &str, statement: &str, extra: &str) -> String {
    format!(
        "schema: req.claim/2\n\
         claim_id: {id}\n\
         statement: >-\n  {statement}\n\
         applicability: []\n\
         relations: []\n\
         evidence: process\n\
         rationale:\n  source: docs/claim-format.md\n  date: 2026-11-04\n  \
         rejected_alternatives: []\n  assumptions: []\n\
         {extra}"
    )
}

/// The `claim_id → address` pairs of a `req render --format json` document (every section).
fn rendered_addresses(stdout: &str) -> Vec<(String, String)> {
    let document: serde_json::Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("render JSON: {e}\n{stdout}"));
    let mut pairs = Vec::new();
    for section in ["applies", "undecided", "outside"] {
        for row in document[section].as_array().into_iter().flatten() {
            let id = row["claim_id"].as_str().expect("claim_id").to_string();
            let address = row["address"]
                .as_str()
                .expect("address")
                .trim_start_matches("sha256:")
                .to_string();
            pairs.push((id, address));
        }
    }
    pairs.sort();
    pairs
}

// ---------------------------------------------------------------------------
// (a) the addresses did not move, and none is stored
// ---------------------------------------------------------------------------

#[test]
fn the_dogfood_addresses_are_the_pre_migration_list_and_none_is_stored() {
    let mut stored_anywhere = Vec::new();
    for (id, _) in PRE_MIGRATION_ADDRESSES {
        let text = std::fs::read_to_string(checkout().join(format!("claims/{id}.yml")))
            .expect("every recorded claim still exists");
        // /3 since the owner filed the claims under areas (ADR 0051): /3 is /2 plus `area`, nothing derived.
        if !text
            .lines()
            .any(|l| l == "schema: req.claim/2" || l == "schema: req.claim/3")
        {
            stored_anywhere.push(format!("{id}: not a req.claim/2 or /3 file"));
        }
        for line in text.lines() {
            if line.starts_with("address:") || line.starts_with("canonical:") {
                stored_anywhere.push(format!("{id}: stores `{line}`"));
            }
        }
    }
    assert!(
        stored_anywhere.is_empty(),
        "a req.claim/2 or /3 corpus stores no fingerprint: {stored_anywhere:?}"
    );

    let out = req(&checkout(), &["render", "--format", "json"]);
    // Only the recorded ids: adding a legitimate claim later must not read as an address moving.
    let rendered: Vec<(String, String)> = rendered_addresses(&out.stdout)
        .into_iter()
        .filter(|(id, _)| PRE_MIGRATION_ADDRESSES.iter().any(|(r, _)| r == id))
        .collect();
    let expected: Vec<(String, String)> = PRE_MIGRATION_ADDRESSES
        .iter()
        .map(|(id, a)| {
            let address = RE_WORDED_AFTER_MIGRATION
                .iter()
                .find(|(reworded, _)| reworded == id)
                .map_or(*a, |(_, moved)| *moved);
            (id.to_string(), address.to_string())
        })
        .collect();
    assert_eq!(
        rendered, expected,
        "a format change moves no address: the derived list must equal the recorded one, \
         save the claims re-worded on purpose"
    );
}

// ---------------------------------------------------------------------------
// (b) a v2 file carrying a v1 slot is a parse error naming the key
// ---------------------------------------------------------------------------

#[test]
fn a_v2_file_carrying_a_v1_slot_is_a_parse_error_naming_the_key() {
    for key in REMOVED_V1_KEYS {
        let f = Fixture::new(&format!("v1-slot-{key}")).claim(
            "REQ-0001",
            &v2_claim(
                "REQ-0001",
                "The req tool shall emit proposals for reviewer ratification.",
                &format!("{key}: null\n"),
            ),
        );
        let out = req(f.path(), &["canon"]);
        assert_eq!(out.code, 3, "`{key}` must fail the load: {}", out.both());
        assert!(
            out.stderr.contains(&format!("`{key}`")) && out.stderr.contains("req.claim/2"),
            "the error names `{key}` and the format that removed it: {}",
            out.stderr
        );
        assert!(
            out.stderr.contains("removed"),
            "the error says the key was removed, not that it is unknown: {}",
            out.stderr
        );
    }
}

// ---------------------------------------------------------------------------
// (c) pins: agree / contradiction / not carried, each through the CLI
// ---------------------------------------------------------------------------

#[test]
fn a_pin_agrees_contradicts_or_is_not_carried_and_canon_says_which() {
    let f = Fixture::new("pins")
        .claim(
            "REQ-0001",
            &v2_claim(
                "REQ-0001",
                "The benchmark harness shall report a false merge rate of at most 0.02.",
                "pins:\n  modality: SHALL\n  operator: LE\n  value: \"0.02\"\n",
            ),
        )
        .claim(
            "REQ-0002",
            &v2_claim(
                "REQ-0002",
                "The benchmark harness shall report a false merge rate of at most 0.02.",
                "pins:\n  operator: LT\n",
            ),
        )
        .claim(
            "REQ-0003",
            &v2_claim(
                "REQ-0003",
                "The req tool shall emit proposals for reviewer ratification.",
                "pins:\n  value: \"5\"\n",
            ),
        );
    let out = req(f.path(), &["canon"]);
    assert_eq!(out.code, 0, "canon reports, it never gates: {}", out.both());
    let line_of = |id: &str, key: &str| -> String {
        let mut in_claim = false;
        for line in out.stdout.lines() {
            if line.starts_with("REQ-") {
                in_claim = line.starts_with(id);
            } else if in_claim && line.trim_start().starts_with(&format!("pin {key} ")) {
                return line.to_string();
            }
        }
        panic!("no `pin {key}` line for {id}:\n{}", out.stdout)
    };
    assert!(
        line_of("REQ-0001", "value").contains("agrees"),
        "{}",
        out.stdout
    );
    assert!(
        line_of("REQ-0001", "operator").contains("agrees"),
        "{}",
        out.stdout
    );
    assert!(
        line_of("REQ-0001", "modality").contains("agrees"),
        "{}",
        out.stdout
    );
    let contradiction = line_of("REQ-0002", "operator");
    assert!(
        contradiction.contains("FLAG") && contradiction.contains("contradiction"),
        "LT pinned over a parse of LE: {contradiction}"
    );
    assert!(
        contradiction.contains("LE"),
        "the flag names what the parse says: {contradiction}"
    );
    let not_carried = line_of("REQ-0003", "value");
    assert!(
        not_carried.contains("FLAG") && not_carried.contains("not carried"),
        "a value the parse does not hold: {not_carried}"
    );
}

// ---------------------------------------------------------------------------
// (d) a v1 file is a migration candidate, and the message names the tool
// ---------------------------------------------------------------------------

#[test]
fn a_v1_file_is_not_judged_and_names_the_migration_tool() {
    // REQ-0020 as it was committed at 3935d0b, verbatim but for the id: a real v1 file.
    let v1 = "schema: req.claim/1\nclaim_id: REQ-0001\nstatement: >-\n  The req tool shall \
              serialize each relation on the outgoing side of its origin\n  claim.\n\
              modality: SHALL\nsubject: REQ_TOOL\nattribute: null\noperator: null\nvalue: null\n\
              unit: none\nconditions: []\napplicability: []\nexception: null\ntemporal: null\n\
              quantifier: null\nunstructured: false\ntests: []\nevidence: machine\ncanonical:\n  \
              rules_v: 6\n  subject_registry: 1\n  form: null\naddress: null\nrationale:\n  \
              source: docs/claim-format.md\n  date: 2026-10-07\n  rejected_alternatives: []\n  \
              assumptions: []\nrelations: []\n";
    let f = Fixture::new("v1-file").claim("REQ-0001", v1);
    let out = req(f.path(), &["canon"]);
    assert_eq!(out.code, 3, "a v1 file is NOT JUDGED: {}", out.both());
    assert!(
        out.stderr.contains("NOT JUDGED") && out.stderr.contains("req.claim/1"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("tools/migrate_claims.py"),
        "the remedy is named: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// (e) `canon --fill` and `--verify` are removed
// ---------------------------------------------------------------------------

#[test]
fn canon_fill_and_verify_are_usage_errors_that_say_nothing_is_stored() {
    for flag in ["--fill", "--verify"] {
        let out = req(&checkout(), &["canon", flag]);
        assert_eq!(
            out.code,
            2,
            "`canon {flag}` is a usage error: {}",
            out.both()
        );
        assert!(
            out.stderr
                .contains("nothing derived is stored in req.claim/2"),
            "{}",
            out.stderr
        );
        assert!(
            out.stderr.contains("req gate"),
            "the check for pins is named: {}",
            out.stderr
        );
    }
}

// ---------------------------------------------------------------------------
// (f) `pins_hold` fails on a flagged pin, and says "zero pins" when there are none
// ---------------------------------------------------------------------------

fn policy_block<'a>(stdout: &'a str, name: &str) -> &'a str {
    let header = stdout
        .lines()
        .find(|l| l.ends_with(&format!("] {name}")))
        .unwrap_or_else(|| panic!("no `{name}` policy:\n{stdout}"));
    let start = stdout.find(header).expect("the header is in the text");
    let rest = &stdout[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn pins_hold_fails_on_a_flagged_pin() {
    let f = Fixture::new("pins-hold-flag").claim(
        "REQ-0001",
        &v2_claim(
            "REQ-0001",
            "The benchmark harness shall report a false merge rate of at most 0.02.",
            "pins:\n  operator: LT\n",
        ),
    );
    let out = req(f.path(), &["gate"]);
    assert_eq!(out.code, 1, "a flagged pin is a defect: {}", out.both());
    let block = policy_block(&out.stdout, "pins_hold");
    assert!(block.starts_with("[FAIL] pins_hold"), "{block}");
    assert!(block.contains("REQ-0001"), "the claim is named: {block}");
    assert!(
        !out.stdout.contains("addresses_current"),
        "the policy was replaced: {}",
        out.stdout
    );
}

#[test]
fn pins_hold_on_zero_pins_passes_and_says_so() {
    let f = Fixture::new("pins-hold-zero").claim(
        "REQ-0001",
        &v2_claim(
            "REQ-0001",
            "The req tool shall emit proposals for reviewer ratification.",
            "",
        ),
    );
    let out = req(f.path(), &["gate"]);
    let block = policy_block(&out.stdout, "pins_hold");
    assert!(block.starts_with("[PASS] pins_hold"), "{block}");
    assert!(
        block.contains("zero pins"),
        "a pass over nothing says it held nothing: {block}"
    );
    assert_eq!(out.code, 0, "{}", out.both());
}

#[test]
fn pins_hold_passes_when_every_pin_agrees_and_counts_them() {
    let f = Fixture::new("pins-hold-agree").claim(
        "REQ-0001",
        &v2_claim(
            "REQ-0001",
            "The benchmark harness shall report a false merge rate of at most 0.02.",
            "pins:\n  operator: LE\n  value: \"0.02\"\n",
        ),
    );
    let out = req(f.path(), &["gate"]);
    assert_eq!(out.code, 0, "{}", out.both());
    let block = policy_block(&out.stdout, "pins_hold");
    assert!(block.starts_with("[PASS] pins_hold"), "{block}");
    assert!(block.contains("2 pin(s)"), "{block}");
}

// ---------------------------------------------------------------------------
// (g) the gate document is `req.gate/2`, with `pins_hold` where `addresses_current` was
// ---------------------------------------------------------------------------

#[test]
fn the_gate_document_is_req_gate_2_with_pins_hold_first() {
    let variant = common::variant_lines()
        .into_iter()
        .next()
        .expect("a declared release");
    let mut args = vec!["gate", "--format", "json"];
    args.extend(variant.split_whitespace());
    let out = req(&checkout(), &args);
    let document: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("gate JSON: {e}\n{}", out.both()));
    assert_eq!(document["schema"], "req.gate/2");
    let names: Vec<&str> = document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .map(|p| p["policy"].as_str().expect("policy name"))
        .collect();
    assert_eq!(
        names,
        [
            "pins_hold",
            "relations_resolve",
            "dependencies_decided",
            "rationale_present",
            "tests_linked",
            "design_satisfied",
            "contradictions",
            "selectors_confirmed"
        ],
        "a rename in place: the list keeps its order"
    );
    assert_eq!(document["policies"][0]["outcome"], "pass");
}

// ---------------------------------------------------------------------------
// (h) the REQ-0020 regression pair still differs through the v2 loader
// ---------------------------------------------------------------------------

#[test]
fn the_req_0020_pair_gets_two_addresses_through_the_v2_loader() {
    let outgoing =
        "The req tool shall serialize each relation on the outgoing side of its origin claim.";
    let incoming =
        "The req tool shall serialize each relation on the incoming side of its origin claim.";
    let f = Fixture::new("req-0020")
        .claim("REQ-0020", &v2_claim("REQ-0020", outgoing, ""))
        .claim("REQ-0021", &v2_claim("REQ-0021", incoming, ""));
    let out = req(f.path(), &["render", "--format", "json"]);
    assert_eq!(out.code, 0, "two v2 claims render settled: {}", out.both());
    let rendered = rendered_addresses(&out.stdout);
    assert_eq!(rendered.len(), 2, "{}", out.both());
    assert_ne!(
        rendered[0].1, rendered[1].1,
        "outgoing vs incoming is a different requirement; one address is a false merge"
    );
    let recorded = PRE_MIGRATION_ADDRESSES
        .iter()
        .find(|(id, _)| *id == "REQ-0020")
        .expect("REQ-0020 is recorded")
        .1;
    assert_eq!(
        rendered[0].1, recorded,
        "the outgoing statement under no applicability is REQ-0020's recorded address"
    );
}
