//! T1.10 — **no statement word leaves identity silently** (rules v6).
//!
//! Evidence: two false merges in this repository's own corpus, each found by hand and not by the
//! benchmark. REQ-0020 ("…exactly once, on the **outgoing** side of its origin claim") and
//! REQ-0004 ("The req tool's **semantic** layer shall emit…") each kept the same address when one
//! meaning-bearing word was replaced, because the word sat in no slot of the canonical form.
//!
//! Every expectation here is built by construction: the two statements of a pair differ in
//! meaning because one word was swapped for another, and no expected value is read off the
//! canonicalizer. The positive cases at the end exist so the invariant cannot pass by making
//! everything unstructured.

use req::canon::{CanonicalForm, Lint, address_hex, canonical_json, canonicalize_statement};
use req::model::Registry;

fn registry() -> Registry {
    Registry::load("aliases/subjects.yml").expect("the repository's own registry loads")
}

fn canon(text: &str) -> (CanonicalForm, Vec<Lint>) {
    canonicalize_statement(text, &registry())
}

fn address(text: &str) -> String {
    address_hex(&canonical_json(&canon(text).0))
}

fn lint_detail(text: &str, code: &str) -> Option<String> {
    canon(text)
        .1
        .into_iter()
        .find(|l| l.code == code)
        .map(|l| l.detail)
}

const REQ_0020_OUTGOING: &str = "The req tool shall serialize each relation exactly once, on the outgoing side of its origin claim.";
const REQ_0020_INCOMING: &str = "The req tool shall serialize each relation exactly once, on the incoming side of its origin claim.";
const REQ_0004_SEMANTIC: &str =
    "The req tool's semantic layer shall emit proposals for reviewer ratification.";
const REQ_0004_AUDIT: &str =
    "The req tool's audit layer shall emit proposals for reviewer ratification.";

// ---------------------------------------------------------------------------
// Mechanism 1: a dropped tail
// ---------------------------------------------------------------------------

/// REQ-0020 at the baseline: "exactly" was taken as the `EQ` comparator, no value followed, and
/// every word after it sat in no slot, so "outgoing" and "incoming" hashed alike.
#[test]
fn req_0020_trailing_clause_moves_the_address() {
    assert_ne!(
        address(REQ_0020_OUTGOING),
        address(REQ_0020_INCOMING),
        "outgoing vs incoming is a different requirement; the same address is a false merge"
    );
}

/// The invariant's answer for a statement it cannot cover is the unstructured fallback, whose
/// `raw` is the whole normalised statement — which is how REQ-0007 is already addressed.
#[test]
fn an_uncovered_statement_is_unstructured_and_carries_its_raw_text() {
    let (form, _) = canon(REQ_0020_OUTGOING);
    assert!(form.unstructured);
    assert!(
        form.raw.contains("outgoing side of its origin claim"),
        "{form:?}"
    );
}

/// The lint names every uncovered word, in statement order, so a reader can see what the parse
/// refused to carry. The stopword `the` is never listed.
#[test]
fn the_lint_names_the_uncovered_words() {
    let detail =
        lint_detail(REQ_0020_OUTGOING, "uncovered_content").expect("uncovered_content lint");
    assert_eq!(
        detail, "once, on, outgoing, side, of, its, origin, claim",
        "the words after `exactly` that no slot holds"
    );
}

/// **Positional, not token-set.** The dropped word `relation` also appears in the action. A check
/// that asked "is this word somewhere in the form's text" would pass the second statement and
/// give both the same address; the position of a word, not its spelling, is what must be held.
#[test]
fn a_dropped_word_that_also_appears_elsewhere_is_still_uncovered() {
    let kept = "The system shall serialize each relation exactly";
    let dup = "The system shall serialize each relation exactly relation";
    assert_ne!(
        address(kept),
        address(dup),
        "a trailing word is a meaning change even when the same word appears earlier"
    );
    let (form, _) = canon(dup);
    assert!(form.unstructured, "{form:?}");
    assert_eq!(
        lint_detail(dup, "uncovered_content").as_deref(),
        Some("relation")
    );
}

/// Stopwords are `a`, `an`, `the` and nothing else. `on` is a preposition ("on"/"off") and
/// swapping it can change meaning, so a statement whose only unattributed word is `on` is not
/// covered.
#[test]
fn a_preposition_is_not_a_stopword() {
    let on = "The system shall serialize each relation exactly on";
    let off = "The system shall serialize each relation exactly off";
    assert!(canon(on).0.unstructured, "`on` is content, not a stopword");
    assert!(canon(off).0.unstructured);
    assert_ne!(address(on), address(off));
    assert_eq!(lint_detail(on, "uncovered_content").as_deref(), Some("on"));
}

/// The three stopwords really are free: an article in a gap between slots moves nothing, and a
/// statement with an article missing is covered all the same.
#[test]
fn an_article_in_a_gap_is_covered() {
    assert_eq!(
        address("The system shall respond within the 200 ms"),
        address("The system shall respond within 200 ms")
    );
    assert!(
        !canon("The system shall respond within a 200 ms")
            .0
            .unstructured
    );
}

/// Any-position substitution, by construction: replacing any one word of these statements with a
/// token absent from them is a different statement, so it must get a different address. This is
/// the property the `external-v4` any-position class measures at scale.
#[test]
fn replacing_any_one_word_moves_the_address() {
    for statement in [
        REQ_0020_OUTGOING,
        REQ_0004_SEMANTIC,
        "The req tool shall serialize each relation exactly once, on the outgoing side of its origin claim",
        "The system shall respond within 200 ms of the request.",
        "The req tool shall refuse a claim whose address is stale when the baseline is signed.",
    ] {
        let base = address(statement);
        let words: Vec<&str> = statement.split_whitespace().collect();
        for i in 0..words.len() {
            let mut mutated = words.clone();
            mutated[i] = "zzzunseen";
            let candidate = mutated.join(" ");
            assert_ne!(
                address(&candidate),
                base,
                "replacing word {i} ({:?}) of {statement:?} kept the address",
                words[i]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Mechanism 2: a prefix subject match
// ---------------------------------------------------------------------------

/// REQ-0004 at the baseline: "the req tool's semantic layer" resolved to `REQ_TOOL` because the
/// registry matched a *prefix*, and "semantic layer" sat in no slot.
#[test]
fn req_0004_subject_head_moves_the_address() {
    assert_ne!(
        address(REQ_0004_SEMANTIC),
        address(REQ_0004_AUDIT),
        "a semantic layer and an audit layer are different subjects"
    );
}

/// T1.7 as written: an alias matches only when it is the **entire** subject span. "the req tool
/// semantic layer" (the possessive is stripped by T1.1) is not a registered alias.
#[test]
fn a_registry_alias_matches_the_whole_subject_span_only() {
    let (whole, _) = canon("The req tool shall emit proposals.");
    assert!(!whole.unstructured);
    assert_eq!(whole.subject, "REQ_TOOL", "the whole span is an alias");

    let (longer, _) = canon(REQ_0004_SEMANTIC);
    assert_ne!(
        longer.subject, "REQ_TOOL",
        "a registered alias that is only the head of the span must not claim the subject"
    );
}

/// The shape the possessive-head mutation produces: `<subject> shall` → `<subject>'s
/// <component> shall`. The component is part of what the claim is about.
#[test]
fn a_possessive_head_is_not_the_bare_subject() {
    assert_ne!(
        address("The req tool shall emit proposals."),
        address("The req tool's index shall emit proposals.")
    );
    assert_ne!(
        address("The benchmark harness shall report the rate."),
        address("The benchmark harness's runner shall report the rate.")
    );
}

// ---------------------------------------------------------------------------
// `req compare` no longer says the pairs are identical
// ---------------------------------------------------------------------------

fn compare_cli(a: &str, b: &str, tag: &str) -> (i32, String) {
    use std::process::Command;
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("coverage-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let claim = |name: &str, statement: &str| -> String {
        let text = format!(
            "schema: req.claim/1\nclaim_id: {name}\nstatement: >-\n  {statement}\nmodality: SHALL\n\
             subject: REQ_TOOL\nattribute: null\noperator: null\nvalue: null\nunit: none\n\
             conditions: []\napplicability: []\nexception: null\ntemporal: null\nquantifier: null\n\
             unstructured: false\ncanonical:\n  rules_v: null\n  subject_registry: 1\n  form: null\n\
             address: null\nrationale:\n  source: docs/claim-format.md\n  date: 2026-10-07\n  \
             rejected_alternatives: []\n  assumptions: []\nrelations: []\n"
        );
        let path = dir.join(format!("{name}.yml"));
        std::fs::write(&path, text).expect("claim file");
        path.to_string_lossy().into_owned()
    };
    let (pa, pb) = (claim("REQ-9001", a), claim("REQ-9002", b));
    let out = Command::new(env!("CARGO_BIN_EXE_req"))
        .arg("--root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["compare", &pa, &pb])
        .output()
        .expect("runs the compiled binary");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn compare_no_longer_answers_identical_for_the_req_0020_pair() {
    let (code, text) = compare_cli(REQ_0020_OUTGOING, REQ_0020_INCOMING, "req0020");
    assert!(!text.contains(": identical"), "{text}");
    assert_ne!(code, 0, "an unparsed side is nothing to classify: {text}");
}

#[test]
fn compare_no_longer_answers_identical_for_the_req_0004_pair() {
    let (_code, text) = compare_cli(REQ_0004_SEMANTIC, REQ_0004_AUDIT, "req0004");
    assert!(!text.contains(": identical"), "{text}");
}

// ---------------------------------------------------------------------------
// Must stay structured: the invariant cannot pass by making everything unstructured
// ---------------------------------------------------------------------------

#[test]
fn exactly_zero_stays_structured() {
    let (form, _) = canon("The benchmark harness shall report exactly zero false merges.");
    assert!(!form.unstructured, "{form:?}");
    assert_eq!(form.operator_str(), "EQ");
}

#[test]
fn at_most_two_percent_stays_structured() {
    let (form, _) = canon("The system shall drop at most 2 percent of requests.");
    assert!(!form.unstructured, "{form:?}");
    assert_eq!(form.operator_str(), "LE");
}

#[test]
fn a_registered_whole_span_subject_stays_structured() {
    let (form, _) = canon("The benchmark harness shall write one result record per rules version.");
    assert!(!form.unstructured, "{form:?}");
    assert_eq!(form.subject, "BENCH");
}

#[test]
fn never_stays_structured() {
    let (form, _) = canon("The req tool shall never merge two claims automatically.");
    assert!(!form.unstructured, "{form:?}");
    assert_eq!(form.modality_str(), "SHALL-NOT");
}

#[test]
fn a_guard_clause_stays_structured() {
    let (form, _) = canon("The system shall refuse the claim when the address is stale.");
    assert!(!form.unstructured, "{form:?}");
    assert_eq!(form.conditions.len(), 1);
}

#[test]
fn a_value_with_its_unit_and_qualifier_stays_structured() {
    let (form, _) = canon("The system shall not exceed 2.5 A of standby current.");
    assert!(!form.unstructured, "{form:?}");
    assert_eq!(form.qualifier, "of standby current");
}
