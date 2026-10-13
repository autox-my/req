//! F2/F3 — the strict corpus loader: an unreadable-*as-a-claim* corpus is an error,
//! never a silent drop.
//!
//! Each test builds a throwaway corpus under `CARGO_TARGET_TMPDIR` and asks the
//! loader (and, for the exit-code cases, the `canon` command) about it. The committed
//! corpus is the *other* fixture: it must still load, which is what makes the
//! strictness non-breaking rather than merely stricter.
//!
//! Vocabulary: a **claim** is one file in `claims/` whose filename is its `claim_id`
//! (docs/claim-format.md). Identity is a content address, so a file that vanishes
//! from the corpus deletes a requirement — the asymmetry this loader exists for.

use std::path::{Path, PathBuf};

use req::canon::{load_claims, run_claim_diagnostics_at};

/// A minimal but *valid* registry: the diagnostics command loads it before the
/// claims, and a fixture that failed here would exit 3 for the wrong reason.
const REGISTRY: &str = "\
schema: req.subject_registry/0
version: 1
subjects:
  REQ_TOOL:
    label: The req tool
    aliases:
      - the req tool
";

/// A real claim shape (claims/REQ-0001.yml), parameterized by `claim_id` so a fixture
/// can misname a file or duplicate an id without also being malformed.
fn claim_text(claim_id: &str) -> String {
    format!(
        "\
schema: req.claim/0
claim_id: {claim_id}
statement: >-
  The req tool shall treat a single claim as the unit of requirements authorship,
  review, and merge.
modality: SHALL
subject: REQ_TOOL
attribute: unit_of_work
operator: EQ
value: claim
unit: none
conditions: []
applicability: []
exception: null
temporal: null
quantifier: null
unstructured: false
canonical:
  rules_v: 5
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/adr/0001-the-unit-is-a-claim.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
"
    )
}

/// A throwaway corpus root: `claims/` plus the registry the commands need.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("claims")).expect("claims dir");
        std::fs::create_dir_all(root.join("aliases")).expect("aliases dir");
        std::fs::write(root.join("aliases/subjects.yml"), REGISTRY).expect("registry");
        Fixture { root }
    }

    fn claim(self, file: &str, text: &str) -> Self {
        std::fs::write(self.root.join("claims").join(file), text).expect("claim file");
        self
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

// ---------------------------------------------------------------------------
// F2 — a misspelled or omitted key is a parse error, not a default
// ---------------------------------------------------------------------------

#[test]
fn a_misspelled_key_is_a_parse_error_and_exits_3() {
    let f = Fixture::new("f2-typo").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace("applicability:", "applicabilty:"),
    );

    let err = load_claims(f.path()).expect_err("a misspelled key must not load");
    assert!(
        err.contains("applicabilty"),
        "the error must name the unknown field: {err}"
    );
    assert_eq!(
        run_claim_diagnostics_at(f.path()),
        3,
        "a claim that does not parse is UNVERIFIABLE, never a pass"
    );
}

#[test]
fn a_dropped_relations_key_is_a_parse_error_and_exits_3() {
    // `deny_unknown_fields` is half a fix: it constrains extra keys, never a missing
    // one. Dropping `relations` leaves the address untouched while the edges vanish
    // from `trace` and `impact`, so the key itself must be required.
    let f = Fixture::new("f2-no-relations").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace("relations: []\n", ""),
    );

    let err = load_claims(f.path()).expect_err("a missing relations key must not load");
    assert!(
        err.contains("relations"),
        "the error must name the missing field: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

// ---------------------------------------------------------------------------
// F3 — a claim file that is not where the contract says it is is an error
// ---------------------------------------------------------------------------

#[test]
fn a_lowercase_claim_file_errors_rather_than_vanishing() {
    let f = Fixture::new("f3-lowercase").claim("req-0008.yml", &claim_text("REQ-0008"));

    let err = load_claims(f.path()).expect_err("req-0008.yml must not vanish silently");
    assert!(
        err.contains("req-0008.yml"),
        "the error must name the file: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn a_yaml_extension_errors_rather_than_vanishing() {
    let f = Fixture::new("f3-yaml-ext").claim("REQ-0008.yaml", &claim_text("REQ-0008"));

    let err = load_claims(f.path()).expect_err("REQ-0008.yaml must not vanish silently");
    assert!(
        err.contains("REQ-0008.yaml"),
        "the error must name the file: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn a_stem_that_is_not_the_claim_id_errors() {
    let f = Fixture::new("f3-stem").claim("REQ-0009.yml", &claim_text("REQ-0010"));

    let err = load_claims(f.path()).expect_err("filename = claim_id is the contract");
    assert!(err.contains("REQ-0009"), "{err}");
    assert!(err.contains("REQ-0010"), "{err}");
    assert_eq!(
        run_claim_diagnostics_at(f.path()),
        3,
        "a misplaced claim is UNVERIFIABLE, never a pass"
    );
}

#[test]
fn duplicate_claim_ids_error_and_the_content_is_compared() {
    // Two files can carry one `claim_id` in exactly one way once the stem rule holds:
    // the `.yaml` twin of a `.yml`. Comparing content, not just the id, separates the
    // fully-silent case (an identical copy) from the divergent one (a rewrite that
    // would collapse two meanings into one address).
    let text = claim_text("REQ-0007");
    let identical = Fixture::new("f3-dup-identical")
        .claim("REQ-0007.yml", &text)
        .claim("REQ-0007.yaml", &text);
    let err = load_claims(identical.path()).expect_err("a duplicate id must not load");
    assert!(err.contains("REQ-0007"), "{err}");
    assert!(
        err.contains("identical content"),
        "an identical duplicate must be named as such: {err}"
    );

    let divergent = Fixture::new("f3-dup-divergent")
        .claim("REQ-0007.yml", &text)
        .claim(
            "REQ-0007.yaml",
            &text.replace("value: claim", "value: tool"),
        );
    let err = load_claims(divergent.path()).expect_err("a duplicate id must not load");
    assert!(
        err.contains("content differs"),
        "a divergent duplicate must be named as a content clash, not just an id clash: {err}"
    );
}

// ---------------------------------------------------------------------------
// Review findings on the loader itself
// ---------------------------------------------------------------------------

#[test]
fn a_misspelled_nested_key_is_a_parse_error_and_exits_3() {
    // `deny_unknown_fields` at the top level is not enough. `rationale`'s two
    // defaulted lists mean a typo one level down loaded as an empty list: the same
    // silent-loss class F2 exists for, one nesting level past the fix.
    let f = Fixture::new("f2-nested-typo").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace("rejected_alternatives:", "rejected_alternativs:"),
    );

    let err = load_claims(f.path()).expect_err("a misspelled nested key must not load");
    assert!(
        err.contains("rejected_alternativs"),
        "the error must name the unknown field: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn a_claim_renamed_out_of_the_contract_errors_rather_than_vanishing() {
    // Claim-shaped names that the extension-only candidate gate used to skip. Each
    // one is a requirement deleted from the corpus in silence — the failure the
    // loader exists for — so renaming a claim must be loud, not a way to disable it.
    // The leading-dot shapes are the same hole seen from the hidden-file side.
    for file in [
        "REQ-0008.txt",
        "REQ-0008",
        "REQ-0008.yml.disabled",
        ".REQ-0008",
        ".REQ-0008.txt",
    ] {
        let f = Fixture::new("f3-renamed").claim(file, &claim_text("REQ-0008"));
        let err = load_claims(f.path()).expect_err("a claim-shaped rename must not vanish");
        assert!(err.contains(file), "the error must name the file: {err}");
        assert_eq!(run_claim_diagnostics_at(f.path()), 3, "{file}");
    }
}

#[test]
fn a_subdirectory_in_claims_is_a_corpus_error() {
    // `mv claims/REQ-0008.yml claims/archive/` is the natural way to "move it out of
    // the way", and it deleted the requirement in silence: the corpus is flat, so a
    // directory here is refused rather than skipped.
    let f = Fixture::new("f3-subdir").claim("REQ-0008.yml", &claim_text("REQ-0008"));
    let claims = f.path().join("claims");
    std::fs::create_dir_all(claims.join("archive")).expect("archive dir");
    std::fs::rename(
        claims.join("REQ-0008.yml"),
        claims.join("archive/REQ-0008.yml"),
    )
    .expect("move the claim out of the way");

    let err = load_claims(f.path()).expect_err("a claim filed in a subdirectory must not vanish");
    assert!(
        err.contains("archive"),
        "the error must name the directory: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn an_empty_corpus_is_unverifiable_never_a_pass() {
    // `mv claims/*.yml claims/archive/` — or any whole-corpus accident — left a
    // loader that returned `Ok(vec![])`, a command that printed `claims: 0;
    // discrepancy flags: 0`, and exit 0. The fully-vanished corpus is the vanish
    // case, so it can never be a pass.
    let f = Fixture::new("f3-empty");

    let err = load_claims(f.path()).expect_err("an empty corpus must not load");
    assert!(err.contains("no claim files"), "{err}");
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

// ---------------------------------------------------------------------------
// Non-breaking: the committed corpus is the fixture that must still load
// ---------------------------------------------------------------------------

#[test]
fn the_committed_corpus_loads_under_the_strict_loader() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let claims = load_claims(&root).expect("the committed corpus must load under the strict rules");
    // The expected id set is the tripwire: a claim file *deleted* from the corpus
    // errors in no other test — the loader cannot know how many claims there should
    // be — and naming the ids says which requirement vanished, rather than leaving it
    // to be reverse-engineered from a count. Extend this list deliberately when a
    // requirement is added; an id that *drops* out is a deleted requirement, and a
    // deliberate retirement updates this list in its own reviewable commit.
    let ids: Vec<&str> = claims.iter().map(|c| c.claim.claim_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004", "REQ-0005", "REQ-0006", "REQ-0007",
            "REQ-0008", "REQ-0009", "REQ-0010", "REQ-0011", "REQ-0012", "REQ-0013", "REQ-0014",
            "REQ-0015", "REQ-0016", "REQ-0017", "REQ-0018", "REQ-0019", "REQ-0020", "REQ-0021",
        ],
        "the committed corpus is REQ-0001..REQ-0021, one claim per file"
    );
}
