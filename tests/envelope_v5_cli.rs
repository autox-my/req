//! Baseline envelope v5, through the compiled `req` (ADR 0056 decisions 6, 7 and 8, task F-2).
//!
//! The seam is the CLI over a scratch corpus signed with a throwaway real `minisign` key (the
//! suite's stated prerequisite, `tests/baseline.rs::require_minisign`), and over the three committed
//! v4 Baselines. Every digest expectation below was computed **outside this crate**, with Python's
//! `hashlib`, from the byte string the ADR specifies; the command is in the comment above each one.
//!
//! Nothing here asserts a path: the scratch corpus lives under Cargo's per-target tmp dir, and no
//! output is compared against a location.
//!
//! * Falsification 2 — a field edit, a `kind` change and a pin deletion each fail `--verify` at exit
//!   1, the output names the surface and says the signature verified;
//! * Falsification 3 — a v4 manifest still verifies, one stripped of `tests_hash` is refused, a v5
//!   one stripped of `fields_hash` or `pins_hash` is refused;
//! * L9 — the three committed v4 Baselines stay `[VERIFIED]` and print the fields-and-pins line;
//! * decision 6 — a claim with no kind and no fields carries the digest of `{"kind":null,"fields":{}}`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const SIGNED_AT: &str = "2026-11-18T09:00:00Z";

/// The line a v4 manifest is owed, once (decision 6, "what v5 means for existing signatures").
const FIELDS_LINE: &str =
    "predates envelope v5: the authored fields and pins are NOT attested by this manifest";
/// The line a v3 manifest is owed beside it.
const TESTS_LINE: &str = "predates envelope v4: the authored test links are NOT attested";

/// `printf 'req.fields.v1\0%s' '{"kind":null,"fields":{}}' | shasum -a 256`
const EMPTY_FIELDS_DIGEST: &str =
    "bd8eccf8db138c1b17e3ecfa21353e83bb838fd0e25bb0ab05440b32969a7318";
/// `printf 'req.pins.v1\0%s' '[]' | shasum -a 256`
const EMPTY_PINS_DIGEST: &str = "5cbd189c50e965f5a3d0b885802ef340255946b2fb3fae1d58c3345dacf19e04";
/// `printf 'req.fields.v1\0%s' '{"kind":"safety","fields":{"asil":{"type":"enum","value":"C"},"mitigated_by":{"type":"list","of":"ref","value":["REQ-0202"]}}}' | shasum -a 256`
const SAFETY_FIELDS_DIGEST: &str =
    "2f3b6ceca3e2b4fa5b6c1fc8a2ddf08222fdeb80ece4681551ef972ec0fb695b";
/// `printf 'req.pins.v1\0%s' '[["modality","SHALL"],["operator","LE"],["value","0.2"],["unit","s"]]' | shasum -a 256`
const PINNED_PINS_DIGEST: &str = "33a91fd581e190292d1dfa20f7760c0222988d360b3a6eecdaddd876718b07ec";

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

fn run(root: &Path, args: &[&str]) -> Out {
    let out = Command::new(req_bin())
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req");
    Out {
        code: out.status.code().expect("req exits, never a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn require_minisign() {
    let installed = Command::new("minisign")
        .arg("-v")
        .output()
        .is_ok_and(|out| out.status.success());
    assert!(
        installed,
        "minisign is required — install it first: `brew install minisign` (Debian: `apt-get install minisign`)"
    );
}

/// A scratch corpus that removes itself, panics included.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const SAFETY_KIND: &str = "\
schema: req.kind/1
label: Safety-related requirement
fields:
  asil:
    type: enum
    values: [QM, A, B, C, D]
    ordered: true
    required: true
  mitigated_by:
    type: list
    of: ref
";

/// Another kind with the same fields, so a `kind` change is the only thing that moves.
const QUALITY_KIND: &str = "\
schema: req.kind/1
fields:
  asil:
    type: enum
    values: [QM, A, B, C, D]
  mitigated_by:
    type: list
    of: ref
";

fn claim(id: &str, extra: &str, source: &str) -> String {
    format!(
        "schema: req.claim/4\n\
         claim_id: {id}\n\
         statement: \"The {id} service shall respond to a health check within 200 ms.\"\n\
         applicability: []\n\
         relations: []\n\
         evidence: process\n\
         {extra}\
         rationale:\n  source: {source}\n  date: 2026-11-18\n  rejected_alternatives: []\n  assumptions: []\n"
    )
}

const REQ_0201_EXTRA: &str = "\
kind: safety
fields:
  asil: C
  mitigated_by: [REQ-0202]
pins:
  modality: SHALL
  operator: LE
  value: \"0.2\"
  unit: s
";

impl Scratch {
    /// A corpus made by the real bootstrap with three claims: REQ-0201 carries a kind, fields and
    /// pins; REQ-0202 carries none of them; REQ-0203 is REQ-0202 with an empty `pins: {}`.
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "envelope-v5-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        let init = Command::new(req_bin())
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("runs req init");
        assert_eq!(init.status.code(), Some(0), "req init must succeed");
        fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        let scratch = Scratch(dir);
        scratch.write(
            "claims/REQ-0201.yml",
            &claim("REQ-0201", REQ_0201_EXTRA, "docs/a.md"),
        );
        scratch.write("claims/REQ-0202.yml", &claim("REQ-0202", "", "docs/b.md"));
        scratch.write(
            "claims/REQ-0203.yml",
            &claim("REQ-0203", "pins: {}\n", "docs/c.md"),
        );
        scratch.write(".req/kinds/safety.yml", SAFETY_KIND);
        scratch.write(".req/kinds/quality.yml", QUALITY_KIND);
        // The Decision log a v3-and-later manifest attests: the repository's own first record.
        scratch.write(
            "decisions/DEC-0001.yml",
            &fs::read_to_string(repo().join("decisions/DEC-0001.yml")).expect("reads DEC-0001"),
        );
        scratch
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, text).expect("writes the file");
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.0.join(relative)).expect("reads the file")
    }

    /// Emit a manifest, sign it with a throwaway key and write the trust anchor that holds it.
    fn emit_and_sign(&self) -> PathBuf {
        require_minisign();
        let public_key = self.0.join("req.pub");
        let secret_key = self.0.join("req.key");
        let generated = Command::new("minisign")
            .args(["-G", "-W", "-f", "-p"])
            .arg(&public_key)
            .arg("-s")
            .arg(&secret_key)
            .output()
            .expect("runs minisign");
        assert!(
            generated.status.success(),
            "throwaway key generation failed"
        );
        let emitted = run(&self.0, &["baseline", "--at", SIGNED_AT]);
        assert_eq!(emitted.code, 0, "{}", emitted.all());
        let manifest = self.0.join("baseline.json");
        fs::write(&manifest, &emitted.stdout).expect("writes the manifest");
        let signed = Command::new("minisign")
            .arg("-Sm")
            .arg(&manifest)
            .arg("-s")
            .arg(&secret_key)
            .output()
            .expect("runs minisign");
        assert!(signed.status.success(), "fixture signing failed");
        let key_text = fs::read_to_string(&public_key).expect("reads the public key");
        let indented = key_text
            .lines()
            .map(|line| format!("      {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.write(
            "baselines/trust.yml",
            &format!(
                "keys:\n  - id: \"req-baseline-1\"\n    scheme: \"minisign\"\n    public_key: |\n{indented}\n    valid_from: \"2026-01-01\"\n"
            ),
        );
        manifest
    }

    /// A **v4** manifest over this corpus, signed with the throwaway key: the v5 manifest
    /// [`Self::emit_and_sign`] writes, with the version set to 4 and both v5 keys removed. That is
    /// byte for byte what a v4 build wrote, because v5 only appends `fields_hash` and `pins_hash`
    /// after `tests_hash` (ADR 0056 decision 6). It stands in for a v4 manifest signed before
    /// ADR 0056, now that the committed Baselines are v5.
    fn emit_and_sign_v4(&self) -> PathBuf {
        let v5 = self.emit_and_sign();
        let v4_text = v4_of(&fs::read_to_string(&v5).expect("reads the v5 manifest"));
        let v4 = self.0.join("baseline-v4.json");
        fs::write(&v4, v4_text).expect("writes the v4 manifest");
        self.sign(&v4);
        v4
    }

    fn sign(&self, manifest: &Path) {
        let signed = Command::new("minisign")
            .arg("-Sm")
            .arg(manifest)
            .arg("-s")
            .arg(self.0.join("req.key"))
            .output()
            .expect("runs minisign");
        assert!(signed.status.success(), "fixture signing failed");
    }

    fn verify(&self, manifest: &Path) -> Out {
        let path = manifest.to_str().expect("a UTF-8 path");
        run(&self.0, &["baseline", "--verify", path])
    }
}

/// A v5 manifest's text as the v4 envelope wrote it: `"version":4` and no `fields_hash` or `pins_hash`.
fn v4_of(v5: &str) -> String {
    let mut out = v5.replacen("{\"version\":5,", "{\"version\":4,", 1);
    assert_ne!(out, v5, "the manifest declares version 5");
    for key in ["fields_hash", "pins_hash"] {
        let marker = format!(",\"{key}\":\"");
        let mut removed = 0;
        while let Some(start) = out.find(&marker) {
            let value = start + marker.len();
            let end = value + out[value..].find('"').expect("a closing quote");
            out.replace_range(start..=end, "");
            removed += 1;
        }
        assert!(removed > 0, "the v5 manifest carries `{key}`");
    }
    out
}

/// The `fields_hash` / `pins_hash` the emitted manifest records for one claim.
fn recorded(manifest: &Path, claim_id: &str, key: &str) -> String {
    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest).expect("reads the manifest"))
            .expect("the manifest is JSON");
    let record = doc["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["claim_id"] == claim_id)
        .unwrap_or_else(|| panic!("{claim_id} is in the manifest"));
    record[key]
        .as_str()
        .unwrap_or_else(|| panic!("{claim_id} carries `{key}`"))
        .to_string()
}

// --- emission ---------------------------------------------------------------------------------------

#[test]
fn req_baseline_emits_envelope_v5_with_the_two_digests_per_claim() {
    let s = Scratch::new("emit");
    let emitted = run(s.path(), &["baseline", "--at", SIGNED_AT]);
    assert_eq!(emitted.code, 0, "{}", emitted.all());
    assert!(
        emitted.stdout.starts_with("{\"version\":5,"),
        "this build writes v5:\n{}",
        emitted.stdout
    );
    let manifest = s.path().join("emitted.json");
    fs::write(&manifest, &emitted.stdout).expect("writes");
    // The independent oracle: a claim with a kind, fields and pins ...
    assert_eq!(
        recorded(&manifest, "REQ-0201", "fields_hash"),
        SAFETY_FIELDS_DIGEST
    );
    assert_eq!(
        recorded(&manifest, "REQ-0201", "pins_hash"),
        PINNED_PINS_DIGEST
    );
    // ... and one with neither carries the digest of `{"kind":null,"fields":{}}` and of `[]`, so
    // *adding* a kind later is a change (decision 6).
    assert_eq!(
        recorded(&manifest, "REQ-0202", "fields_hash"),
        EMPTY_FIELDS_DIGEST
    );
    assert_eq!(
        recorded(&manifest, "REQ-0202", "pins_hash"),
        EMPTY_PINS_DIGEST
    );
    // `pins: {}` and no `pins:` hash alike (decision 7).
    assert_eq!(
        recorded(&manifest, "REQ-0203", "pins_hash"),
        recorded(&manifest, "REQ-0202", "pins_hash"),
        "an empty pins map is no pins"
    );
    // The two keys follow `tests_hash`, in this order, in each claim record.
    let order: Vec<usize> = ["\"tests_hash\"", "\"fields_hash\"", "\"pins_hash\""]
        .iter()
        .map(|key| {
            emitted
                .stdout
                .find(key)
                .unwrap_or_else(|| panic!("{key} is in the bytes"))
        })
        .collect();
    assert!(order[0] < order[1] && order[1] < order[2], "{order:?}");
}

#[test]
fn a_v5_baseline_over_the_untouched_tree_verifies_and_prints_no_disclosure() {
    let s = Scratch::new("clean");
    let manifest = s.emit_and_sign();
    let out = s.verify(&manifest);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("[VERIFIED]"), "{}", out.all());
    assert!(
        !out.stdout.contains("predates"),
        "a v5 manifest predates nothing:\n{}",
        out.stdout
    );
}

// --- falsification 2: an edit the address does not see ------------------------------------------------

#[test]
fn a_field_edit_after_signing_fails_verify_and_names_the_fields() {
    let s = Scratch::new("field-edit");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0201.yml");
    assert!(text.contains("asil: C"));
    s.write("claims/REQ-0201.yml", &text.replace("asil: C", "asil: D"));
    let out = s.verify(&manifest);
    assert_eq!(
        out.code,
        1,
        "an edited field is FAIL, not a pass:\n{}",
        out.all()
    );
    assert!(out.stdout.contains("[FAIL]"), "{}", out.all());
    assert!(
        out.stdout
            .contains("REQ-0201: the fields no longer recompute — edited after signing (ADR 0013, ADR 0019, ADR 0056)"),
        "the failure names the claim and the surface:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("the pins") && !out.stdout.contains("the relations"),
        "only the surface that moved is named:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("signature itself verified"),
        "the manifest was untouched and the working tree moved, and the output says so:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains(SAFETY_FIELDS_DIGEST),
        "a failure names a surface, never a hash value:\n{}",
        out.stdout
    );
}

#[test]
fn a_kind_change_after_signing_fails_verify_as_a_fields_change() {
    let s = Scratch::new("kind-change");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0201.yml");
    s.write(
        "claims/REQ-0201.yml",
        &text.replace("kind: safety", "kind: quality"),
    );
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout
            .contains("REQ-0201: the fields no longer recompute"),
        "{}",
        out.stdout
    );
}

#[test]
fn adding_a_kind_to_a_claim_that_had_none_fails_verify() {
    let s = Scratch::new("kind-added");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0202.yml");
    let with_kind = text.replace("evidence: process\n", "evidence: process\nkind: quality\n");
    assert_ne!(text, with_kind);
    s.write("claims/REQ-0202.yml", &with_kind);
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout
            .contains("REQ-0202: the fields no longer recompute"),
        "{}",
        out.stdout
    );
}

/// Probe C2 of the research record: deleting a pin left every attested value byte-identical and the
/// Baseline `[VERIFIED]`. With `pins_hash` it is FAIL (decision 7).
#[test]
fn a_pin_deletion_after_signing_fails_verify_and_names_the_pins() {
    let s = Scratch::new("pin-deleted");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0201.yml");
    let without = text.replace("  unit: s\n", "");
    assert_ne!(text, without);
    s.write("claims/REQ-0201.yml", &without);
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "a deleted pin is FAIL:\n{}", out.all());
    assert!(
        out.stdout
            .contains("REQ-0201: the pins no longer recompute — edited after signing (ADR 0013, ADR 0019, ADR 0056)"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("signature itself verified"),
        "{}",
        out.stdout
    );
}

#[test]
fn two_surfaces_that_moved_are_both_named_in_a_fixed_order() {
    let s = Scratch::new("two-surfaces");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0201.yml");
    // The rationale and a field, in one edit: the ADR's own example sentence.
    s.write(
        "claims/REQ-0201.yml",
        &text
            .replace("asil: C", "asil: D")
            .replace("source: docs/a.md", "source: docs/other.md"),
    );
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout.contains(
            "REQ-0201: the rationale and the fields no longer recompute — edited after signing (ADR 0013, ADR 0019, ADR 0056)"
        ),
        "{}",
        out.stdout
    );
}

#[test]
fn a_rationale_only_edit_does_not_cite_the_fields_adr() {
    let s = Scratch::new("rationale-only");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0202.yml");
    s.write(
        "claims/REQ-0202.yml",
        &text.replace("source: docs/b.md", "source: docs/other.md"),
    );
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout.contains(
            "REQ-0202: the rationale no longer recomputes — edited after signing (ADR 0013, ADR 0019)"
        ),
        "{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("ADR 0056"),
        "ADR 0056 is cited only when a fields or pins surface is among those named:\n{}",
        out.stdout
    );
}

#[test]
fn an_edit_of_the_address_alone_does_not_claim_the_signature_is_the_reason() {
    // A reworded statement moves the address: `the address no longer recomputes`, as before. The
    // signature line is about the manifest being untouched, and is printed beside any drift.
    let s = Scratch::new("address");
    let manifest = s.emit_and_sign();
    let text = s.read("claims/REQ-0202.yml");
    s.write(
        "claims/REQ-0202.yml",
        &text.replace("within 200 ms", "within 100 ms"),
    );
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stdout
            .contains("REQ-0202: the address no longer recomputes — edited after signing"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("signature itself verified"),
        "{}",
        out.stdout
    );
}

#[test]
fn a_bad_signature_is_not_described_as_verified() {
    let s = Scratch::new("bad-signature");
    let manifest = s.emit_and_sign();
    let text = fs::read_to_string(&manifest).expect("reads");
    // A byte inside the signed bytes (the verifier re-derives them, so whitespace would change nothing).
    let tampered = text.replacen(SIGNED_AT, "2026-11-18T09:00:01Z", 1);
    assert_ne!(tampered, text);
    fs::write(&manifest, tampered).expect("tampers");
    let out = s.verify(&manifest);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.stdout.contains("does not verify"), "{}", out.stdout);
    assert!(
        !out.stdout.contains("signature itself verified"),
        "the line is only true when the signature verified:\n{}",
        out.stdout
    );
}

// --- falsification 3: what a manifest must carry -------------------------------------------------------

fn without_key(manifest: &Path, key: &str, into: &Path) {
    let mut doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest).expect("reads")).expect("JSON");
    for claim in doc["claims"].as_array_mut().expect("claims") {
        claim.as_object_mut().expect("an object").remove(key);
    }
    fs::write(into, doc.to_string()).expect("writes");
}

#[test]
fn a_v5_manifest_stripped_of_fields_hash_or_pins_hash_is_refused() {
    let s = Scratch::new("strip-v5");
    let manifest = s.emit_and_sign();
    for key in ["fields_hash", "pins_hash"] {
        let stripped = s.path().join(format!("no-{key}.json"));
        without_key(&manifest, key, &stripped);
        let out = s.verify(&stripped);
        assert_eq!(out.code, 3, "{key}: {}", out.all());
        assert!(
            out.all().contains("is not readable"),
            "{key}: {}",
            out.all()
        );
        assert!(
            out.all().contains(key),
            "the refusal names what is missing ({key}):\n{}",
            out.all()
        );
    }
}

#[test]
fn a_v4_manifest_stripped_of_tests_hash_is_refused() {
    let s = Scratch::new("strip-v4");
    let v4 = s.emit_and_sign_v4();
    let stripped = s.path().join("no-tests-hash.json");
    without_key(&v4, "tests_hash", &stripped);
    let out = s.verify(&stripped);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains("tests_hash"), "{}", out.all());
}

/// A v4 manifest that happens to carry the two new keys is treated as a v3 manifest carrying
/// `tests_hash` is: they are left out of what is verified, so the signature still holds and a
/// differing value invents no tampering. Pinned so a later change to it is deliberate.
#[test]
fn a_v4_manifest_carrying_the_new_hashes_does_not_judge_them() {
    require_minisign();
    let s = Scratch::new("v4-carrying");
    let v4 = s.emit_and_sign_v4();
    let mut doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&v4).expect("reads")).expect("JSON");
    for claim in doc["claims"].as_array_mut().expect("claims") {
        let record = claim.as_object_mut().expect("an object");
        record.insert("fields_hash".into(), "not-a-digest".into());
        record.insert("pins_hash".into(), "not-a-digest".into());
    }
    let carrying = s.path().join("carrying.json");
    fs::write(&carrying, doc.to_string()).expect("writes");
    let signature = format!("{}.minisig", v4.display());
    let out = run(
        s.path(),
        &[
            "baseline",
            "--verify",
            carrying.to_str().expect("UTF-8"),
            "--signature",
            &signature,
        ],
    );
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("[VERIFIED]"), "{}", out.all());
}

// --- L9: a signed v4 manifest, and the committed v5 Baselines --------------------------------------------

#[test]
fn a_signed_v4_manifest_stays_verified_and_says_once_what_it_does_not_attest() {
    let s = Scratch::new("v4-signed");
    let v4 = s.emit_and_sign_v4();
    let out = s.verify(&v4);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.stdout.contains("[VERIFIED]"), "{}", out.stdout);
    assert_eq!(
        out.stdout.matches(FIELDS_LINE).count(),
        1,
        "says once, not per claim, that it does not attest fields and pins:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains(TESTS_LINE),
        "a v4 manifest attests its test links, so it is not told it does not:\n{}",
        out.stdout
    );

    // Unattested by v4: a pin deleted after signing leaves it VERIFIED (probe C2), which v5 catches.
    s.write(
        "claims/REQ-0201.yml",
        &s.read("claims/REQ-0201.yml").replace("  unit: s\n", ""),
    );
    let edited = s.verify(&v4);
    assert_eq!(edited.code, 0, "{}", edited.all());
}

#[test]
fn the_three_committed_baselines_are_v5_verified_and_disclose_nothing() {
    require_minisign();
    let mut checked = 0;
    for name in [
        "air-gapped.json",
        "release-parity.json",
        "release-parity-and-adversarial.json",
    ] {
        let path = repo().join("baselines").join(name);
        let text = fs::read_to_string(&path).expect("reads the committed manifest");
        assert!(
            text.starts_with("{\"version\":5,"),
            "{name} is at envelope v5 (re-signed after the v0.6.0 release)"
        );
        let out = run(
            &repo(),
            &["baseline", "--verify", path.to_str().expect("UTF-8")],
        );
        assert_eq!(out.code, 0, "{name}: {}", out.all());
        assert!(out.stdout.contains("[VERIFIED]"), "{name}: {}", out.stdout);
        assert!(
            !out.stdout.contains("predates"),
            "{name} attests every surface, so it predates nothing:\n{}",
            out.stdout
        );
        checked += 1;
    }
    assert_eq!(checked, 3);
}

// --- the disclosure, per envelope ---------------------------------------------------------------------

/// A manifest of `version`, unsigned: the disclosure is printed before the verdict, so an unsigned one
/// shows it. `claims: []`, so the verdict is a FAIL for drift, which is not what is asserted.
fn manifest_text(version: u32) -> String {
    let head = "5cd6c7ff145231c134616228dcc0a8c80ac28e9842d77b20e83f502b59bf6978";
    format!(
        "{{\"version\":{version},\"signed_at\":\"{SIGNED_AT}\",\"authority\":{{\"head\":\"{head}\",\"entries\":1}},\
         \"rules_v\":9,\"selection\":{{}},\"claims\":[]}}\n"
    )
}

fn disclosure_for(version: u32, name: &str) -> String {
    let s = Scratch::new(name);
    let path = s.path().join("m.json");
    fs::write(&path, manifest_text(version)).expect("writes");
    // A trust anchor with no key: the verdict is UNVERIFIABLE, and the disclosure is printed first.
    s.write("baselines/trust.yml", "keys: []\n");
    s.verify(&path).stdout
}

#[test]
fn a_v3_manifest_gets_both_lines_a_v4_one_the_fields_line_and_a_v5_one_none() {
    let v3 = disclosure_for(3, "disclose-v3");
    assert!(v3.contains(TESTS_LINE), "{v3}");
    assert!(v3.contains(FIELDS_LINE), "{v3}");
    assert!(
        !v3.contains("predates envelope v5: the authored test links"),
        "the test-links line names the envelope that introduced test links, not this build's:\n{v3}"
    );

    let v4 = disclosure_for(4, "disclose-v4");
    assert!(!v4.contains(TESTS_LINE), "{v4}");
    assert!(v4.contains(FIELDS_LINE), "{v4}");

    let v5 = disclosure_for(5, "disclose-v5");
    assert!(
        !v5.contains("predates"),
        "no manifest is told it predates itself:\n{v5}"
    );
}

// --- the help page --------------------------------------------------------------------------------------

#[test]
fn the_baseline_help_page_says_what_v5_attests_and_what_it_does_not() {
    let out = Command::new(req_bin())
        .args(["baseline", "--help"])
        .output()
        .expect("runs req");
    let help = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        help.contains("fields_hash") && help.contains("pins_hash"),
        "{help}"
    );
    assert!(
        help.contains(".req/kinds") && help.contains("not attested"),
        "the declarations are the named loss:\n{help}"
    );
    assert!(
        help.contains("v4") && help.contains("predates envelope v5"),
        "{help}"
    );
}
