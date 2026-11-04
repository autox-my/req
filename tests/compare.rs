//! `req compare` — *what kind of change is this?*
//!
//! Direction is a safety classification, not a description. Telling a human "this
//! edit is cosmetic" when it actually weakened an obligation is how a loosened
//! requirement gets ratified, so the classes below are the ones a reviewer must never
//! confuse. Every expectation is built by construction — the mutation applied to a
//! corpus statement — never read off the classifier.

use std::path::PathBuf;
use std::process::Command;

use req::canon::{canonicalize_statement, claim_schema};
use req::compare::{Direction, compare};
use req::model::Registry;

fn dir(a: &str, b: &str) -> Direction {
    let r = Registry::empty();
    let (fa, _) = canonicalize_statement(a, &r);
    let (fb, _) = canonicalize_statement(b, &r);
    compare(&fa, &fb)
}

#[test]
fn identical_statements_are_identical() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall log each command."
        ),
        Direction::Identical
    );
}

/// A cosmetic edit (case, whitespace, punctuation, unit spelling) canonicalizes to the
/// same form, so it lands here too — which is the audit answer: the text changed, the
/// meaning did not.
#[test]
fn cosmetic_edits_are_identical_not_a_change() {
    assert_eq!(
        dir(
            "The system shall not exceed 2.5 amps of standby current.",
            "The system shall not exceed 2.50 A of standby current."
        ),
        Direction::Identical
    );
}

#[test]
fn weakening_and_strengthening_are_told_apart() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system may log each command."
        ),
        Direction::ModalityWeakening
    );
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system should log each command."
        ),
        Direction::ModalityWeakening
    );
    assert_eq!(
        dir(
            "The system may log each command.",
            "The system shall log each command."
        ),
        Direction::ModalityStrengthening
    );
}

/// A polarity flip is not a strengthening: forbidding X is a different obligation, not
/// a stricter version of requiring it.
#[test]
fn polarity_flip_is_its_own_class() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall not log each command."
        ),
        Direction::PolarityFlip
    );
}

/// The sign that is easy to get backwards: for an upper bound a *larger* number allows
/// more (loosening); for a lower bound a larger number demands more (tightening).
#[test]
fn numeric_direction_follows_the_operator_polarity() {
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond within 250 ms."
        ),
        Direction::NumericLoosening
    );
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond within 150 ms."
        ),
        Direction::NumericTightening
    );
    assert_eq!(
        dir(
            "The module shall store at least 1,000 records.",
            "The module shall store at least 1200 records."
        ),
        Direction::NumericTightening
    );
    assert_eq!(
        dir(
            "The module shall store at least 1,200 records.",
            "The module shall store at least 1000 records."
        ),
        Direction::NumericLoosening
    );
}

/// LT is stricter than LE, so the LE-vs-LT trap is a *tightening* — and it must never
/// be reported as a loosening.
#[test]
fn fewer_than_is_tighter_than_within() {
    assert_eq!(
        dir(
            "The system shall respond within 200 ms.",
            "The system shall respond in fewer than 200 ms."
        ),
        Direction::NumericTightening
    );
}

#[test]
fn conditions_narrow_and_widen_scope() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall log each command when auto-brake is enabled."
        ),
        Direction::ScopeNarrowing
    );
    assert_eq!(
        dir(
            "The system shall log each command when auto-brake is enabled.",
            "The system shall log each command."
        ),
        Direction::ScopeWidening
    );
}

/// The hazard the predicate-span bug taught us: same bound, different *thing* bounded.
#[test]
fn a_changed_qualifier_is_divergence_not_a_no_op() {
    assert_eq!(
        dir(
            "The system shall not exceed 2.5 A of standby current.",
            "The system shall not exceed 2.5 A of surge current."
        ),
        Direction::ScopeDivergence
    );
}

#[test]
fn different_predicates_are_unrelated() {
    assert_eq!(
        dir(
            "The system shall log each command.",
            "The system shall delete each command."
        ),
        Direction::Unrelated
    );
}

/// One unparseable side means there is nothing to compare structurally. That is its own
/// outcome — it must not be reported as "no change" (the dangerous direction) or forced
/// into a direction class.
#[test]
fn an_unparseable_side_is_unclassifiable_never_identical() {
    let r = Registry::empty();
    let (a, _) = canonicalize_statement("Authoring shall be friction-free.", &r);
    let (b, _) = canonicalize_statement("Authoring shall be robust.", &r);
    assert_eq!(compare(&a, &b), Direction::Unclassifiable);
}

/// This predicate is what the direction corpus will gate on (next slice), so pin the
/// definition of "dangerous" *before* it is used as a gate: a safety-relevant change
/// reported as no-change, or as the opposite direction, is dangerous; a correct answer,
/// an unrelated pair, or a false alarm on an unchanged pair is not.
#[test]
fn the_dangerous_confusion_gate_names_the_dangerous_pairs() {
    use req::compare::Direction::{
        Identical, ModalityStrengthening, ModalityWeakening, NumericLoosening, NumericTightening,
        PolarityFlip, ScopeNarrowing, ScopeWidening, Unrelated,
    };
    // A loosening waved through, or read backwards.
    assert!(Direction::is_dangerous_confusion(
        ModalityWeakening,
        Identical
    ));
    assert!(Direction::is_dangerous_confusion(
        ModalityWeakening,
        ModalityStrengthening
    ));
    assert!(Direction::is_dangerous_confusion(
        NumericLoosening,
        NumericTightening
    ));
    assert!(Direction::is_dangerous_confusion(
        ScopeWidening,
        ScopeNarrowing
    ));
    // A polarity flip waved through as no change.
    assert!(Direction::is_dangerous_confusion(PolarityFlip, Identical));
    // Correct answers are not dangerous.
    assert!(!Direction::is_dangerous_confusion(
        ModalityWeakening,
        ModalityWeakening
    ));
    assert!(!Direction::is_dangerous_confusion(
        NumericTightening,
        NumericTightening
    ));
    // Neither is a pair that claims no movement, nor a false alarm on an unchanged pair.
    assert!(!Direction::is_dangerous_confusion(Unrelated, Identical));
    assert!(!Direction::is_dangerous_confusion(
        Identical,
        NumericLoosening
    ));
    assert!(!Direction::is_dangerous_confusion(
        Direction::Unclassifiable,
        Identical
    ));
}

// ---------------------------------------------------------------------------
// A declared format this build cannot establish is NOT JUDGED — the loader's class
// ---------------------------------------------------------------------------

/// A minimal but *valid* registry: `compare` loads it before either side, and a fixture that
/// failed here would exit 3 for the wrong reason.
const REGISTRY: &str = "\
schema: req.subject_registry/0
version: 1
subjects:
  REQ_TOOL:
    label: The req tool
    aliases:
      - the req tool
";

/// The claim shape `compare` is asked about, parameterized by the declared `schema:` so the
/// two-sided test differs in the declaration and in nothing else.
fn claim_text(claim_id: &str, schema: &str) -> String {
    format!(
        "\
schema: {schema}
claim_id: {claim_id}
statement: >-
  The req tool shall treat a single claim as the unit of requirements authorship,
  review, and merge.
applicability: []
rationale:
  source: docs/adr/0001-the-unit-is-a-claim.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
"
    )
}

/// A throwaway corpus root: `claims/` (what `--root` validates) plus the registry `compare`
/// needs. The two sides are read by path, so the claim files also live here.
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

    /// Run the compiled binary — the class is a CLI contract (exit code and printed verdict),
    /// so it is read here rather than inferred from a library return value.
    fn compare(&self, a: &str, b: &str) -> (i32, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_req"))
            .arg("--root")
            .arg(&self.root)
            .arg("compare")
            .arg(self.root.join("claims").join(a))
            .arg(self.root.join("claims").join(b))
            .output()
            .expect("runs the compiled binary");
        (
            output.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// `compare`'s own docstring promises exit 3 for "a claim that will not load". A side whose
/// declared format this build does not write is exactly that: nothing inside it was compared to
/// anything, so classifying it would report a direction over a parse that was never judged —
/// and `req canon --verify` on the same file already exits 3.
#[test]
fn a_side_declaring_a_format_this_build_cannot_establish_is_not_judged() {
    let f = Fixture::new("a2-foreign-schema")
        .claim(
            "REQ-0001.yml",
            &claim_text(
                "REQ-0001",
                &format!(
                    "{}{}",
                    req::canon::CLAIM_SCHEMA_PREFIX,
                    req::canon::CLAIM_SCHEMA_V + 1
                ),
            ),
        )
        .claim("REQ-0002.yml", &claim_text("REQ-0002", &claim_schema()));

    let (code, out) = f.compare("REQ-0001.yml", "REQ-0002.yml");
    assert_eq!(
        code, 3,
        "a side this build cannot establish is UNVERIFIABLE, never a classification:\n{out}"
    );
    assert!(
        out.contains("NOT JUDGED"),
        "the verdict must say nothing inside the file was judged:\n{out}"
    );
    assert!(
        out.contains(&format!("req.claim/{}", req::canon::CLAIM_SCHEMA_V + 1)),
        "the verdict must name the declared format:\n{out}"
    );
}

/// A flagged pin never changes the classification, but `compare` reports it: the one visible
/// consequence of a pin in this command. The claim pins `modality: SHALL` (it agrees) and
/// `value: "0.2"` against a 500 ms statement (0.5 s derived, so a contradiction): of two pins one
/// is flagged, which is what tells "count the flagged ones" from "count every pin". The pair is
/// still classified (exit 0).
#[test]
fn a_flagged_pin_is_reported_and_never_changes_the_classification() {
    let pinned = format!(
        "{}pins:\n  modality: SHALL\n  value: \"0.2\"\n",
        claim_text("REQ-0001", &claim_schema())
            .replace(
                "The req tool shall treat a single claim as the unit of requirements authorship,\n  review, and merge.",
                "The API shall respond within 500 ms.",
            )
    );
    assert!(
        pinned.contains("within 500 ms"),
        "the statement edit landed:\n{pinned}"
    );
    // The pair differs in exactly the pin: the second side states the same claim without one.
    let unpinned = pinned
        .replace("pins:\n  modality: SHALL\n  value: \"0.2\"\n", "")
        .replace("REQ-0001", "REQ-0002");
    assert!(!unpinned.contains("pins:"), "{unpinned}");
    let f = Fixture::new("pin-flag-note")
        .claim("REQ-0001.yml", &pinned)
        .claim("REQ-0002.yml", &unpinned);

    let (code, out) = f.compare("REQ-0001.yml", "REQ-0002.yml");
    assert_eq!(code, 0, "the pair is still classified:\n{out}");
    assert!(out.contains("identical"), "{out}");
    assert!(
        out.contains("a pin disagrees with its claim's parse (1 / 0 flagged)"),
        "the flag is reported with the count for each side:\n{out}"
    );
}

/// The control: a file declaring the schema this build writes must classify exactly as it does
/// today. A guard that also refuses valid files is not the fix.
#[test]
fn a_side_declaring_the_current_schema_is_still_classified() {
    let f = Fixture::new("a2-current-schema")
        .claim("REQ-0001.yml", &claim_text("REQ-0001", &claim_schema()))
        .claim("REQ-0002.yml", &claim_text("REQ-0002", &claim_schema()));

    let (code, out) = f.compare("REQ-0001.yml", "REQ-0002.yml");
    assert_eq!(
        code, 0,
        "the current format is established by this build, so the pair is classified:\n{out}"
    );
    assert!(
        out.contains("identical"),
        "two identical statements are `identical`, and the exit code is 0:\n{out}"
    );
}
