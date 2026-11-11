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

// ---------------------------------------------------------------------------------------------
// ADR 0044 (R-2): a change on several axes has a direction only when every axis agrees, and a
// bound under a prohibition swaps its senses. Every expectation is derived from the ADR's
// decisions and from English: which way each single mutation points is the label, and the
// compound's label is the rule of decisions 3–4 applied to those two, never `compare`'s output.
// ---------------------------------------------------------------------------------------------

use req::compare::{Effect, axes};

fn axes_of(a: &str, b: &str) -> Vec<Direction> {
    let r = Registry::empty();
    let (fa, _) = canonicalize_statement(a, &r);
    let (fb, _) = canonicalize_statement(b, &r);
    axes(&fa, &fb)
}

/// Decision 3, the case that motivated it: SHOULD→SHALL strengthens, 10→100 ms loosens tenfold.
/// Reporting the modality alone told `--fail-on weakened` that nothing was loosened.
#[test]
fn a_strengthened_modality_with_a_loosened_bound_has_no_direction() {
    let (a, b) = (
        "The system should respond within 10 ms.",
        "The system shall respond within 100 ms.",
    );
    assert_eq!(dir(a, b), Direction::ScopeDivergence);
    assert_eq!(
        axes_of(a, b),
        vec![
            Direction::ModalityStrengthening,
            Direction::NumericLoosening
        ]
    );
    // The mirror: the same two mutations the other way round are still a compound of opposites.
    assert_eq!(
        dir(b, a),
        Direction::ScopeDivergence,
        "SHALL→SHOULD weakens, 100→10 ms tightens"
    );
}

/// Decision 3, agreeing axes: the class is the first moved axis's, and the other is named.
#[test]
fn a_weakened_modality_with_a_loosened_bound_is_a_weakening_naming_both_axes() {
    let (a, b) = (
        "The system shall respond within 100 ms.",
        "The system should respond within 200 ms.",
    );
    assert_eq!(dir(a, b), Direction::ModalityWeakening);
    assert_eq!(
        axes_of(a, b),
        vec![Direction::ModalityWeakening, Direction::NumericLoosening]
    );
    // And both axes tightening is a strengthening.
    assert_eq!(dir(b, a), Direction::ModalityStrengthening);
    assert_eq!(
        axes_of(b, a),
        vec![
            Direction::ModalityStrengthening,
            Direction::NumericTightening
        ]
    );
}

/// The agreeing compound's effect is right: it permits more.
#[test]
fn an_agreeing_compound_keeps_its_effect() {
    let r = Registry::empty();
    let (fa, _) = canonicalize_statement("The system shall respond within 100 ms.", &r);
    let (fb, _) = canonicalize_statement("The system should respond within 200 ms.", &r);
    assert_eq!(
        compare(&fa, &fb).effect(fa.modality),
        Effect::PermitsMore,
        "SHALL→SHOULD and 100→200 ms both permit more"
    );
}

#[test]
fn a_weakened_modality_with_a_tightened_bound_has_no_direction() {
    assert_eq!(
        dir(
            "The system shall respond within 100 ms.",
            "The system should respond within 50 ms."
        ),
        Direction::ScopeDivergence
    );
}

/// Decision 4: a guard that moves together with the modality is read against no single
/// modality, so the pair has no direction — whichever way the modality moved.
#[test]
fn a_modality_change_with_a_guard_is_scope_divergence() {
    let guarded = "when the brake is engaged";
    for (a, b) in [
        (
            "The system shall stop within 2 s.".to_string(),
            format!("The system should stop within 2 s {guarded}."),
        ),
        (
            "The system should stop within 2 s.".to_string(),
            format!("The system shall stop within 2 s {guarded}."),
        ),
        (
            format!("The system shall stop within 2 s {guarded}."),
            "The system should stop within 2 s.".to_string(),
        ),
    ] {
        assert_eq!(dir(&a, &b), Direction::ScopeDivergence, "{a} -> {b}");
    }
}

/// Decision 3 over a scope axis that does carry an effect: a guard added to an obligation permits
/// more (ADR 0041), so it agrees with a loosened bound and opposes a tightened one.
#[test]
fn a_guard_agrees_or_disagrees_with_the_bound_by_effect() {
    let plain = "The system shall stop within 2 s.";
    assert_eq!(
        dir(
            plain,
            "The system shall stop within 3 s when the brake is engaged."
        ),
        Direction::ScopeNarrowing,
        "guard added (permits more) + bound loosened (permits more)"
    );
    assert_eq!(
        dir(
            plain,
            "The system shall stop within 1 s when the brake is engaged."
        ),
        Direction::ScopeDivergence,
        "guard added (permits more) + bound tightened (permits less)"
    );
    // A guard on a permission claims no direction, so it cannot agree with anything.
    assert_eq!(
        dir(
            "The system may stop within 2 s.",
            "The system may stop within 3 s when the brake is engaged."
        ),
        Direction::ScopeDivergence
    );
}

/// An identity field that moves beside a directed axis leaves the pair with no direction.
#[test]
fn a_changed_qualifier_beside_a_loosened_bound_is_scope_divergence() {
    assert_eq!(
        dir(
            "The system shall not exceed 2.5 A of standby current.",
            "The system shall not exceed 3 A of surge current."
        ),
        Direction::ScopeDivergence
    );
}

/// Decision 5: under SHALL NOT the bound's senses swap. "more than 3 s" is the forbidden region;
/// raising it to 4 forbids less.
#[test]
fn a_bound_under_a_prohibition_swaps_its_senses() {
    let (three, four) = (
        "The system shall not be more than 3 s late.",
        "The system shall not be more than 4 s late.",
    );
    assert_eq!(dir(three, four), Direction::NumericLoosening);
    assert_eq!(dir(four, three), Direction::NumericTightening);
    // The lower-bound twin: "less than" forbidden, so lowering the number forbids less.
    let (two, one) = (
        "The system shall not be less than 2 s late.",
        "The system shall not be less than 1 s late.",
    );
    assert_eq!(dir(two, one), Direction::NumericLoosening);
    assert_eq!(dir(one, two), Direction::NumericTightening);
}

/// Decision 5, the other half: an absorbed negation is an obligation and keeps its senses.
#[test]
fn an_absorbed_negation_is_an_obligation_and_keeps_its_senses() {
    let (a, b) = (
        "The system shall not exceed 200 ms.",
        "The system shall not exceed 300 ms.",
    );
    assert_eq!(dir(a, b), Direction::NumericLoosening);
    assert_eq!(dir(b, a), Direction::NumericTightening);
}

/// The invariant behind decision 3, over a spread of compound pairs: whenever the moved axes
/// do not share one effect, the class is never a direction.
#[test]
fn a_pair_whose_axes_disagree_is_never_given_a_direction() {
    let pairs = [
        (
            "The system should respond within 10 ms.",
            "The system shall respond within 100 ms.",
        ),
        (
            "The system shall respond within 100 ms.",
            "The system may respond within 50 ms.",
        ),
        (
            "The system may respond within 50 ms.",
            "The system shall respond within 100 ms.",
        ),
        (
            "The system shall not be more than 3 s late.",
            "The system shall not be more than 2 s late when the brake is engaged.",
        ),
    ];
    let r = Registry::empty();
    for (a, b) in pairs {
        let (fa, _) = canonicalize_statement(a, &r);
        let moved = axes_of(a, b);
        assert!(moved.len() >= 2, "{a} -> {b}: {moved:?}");
        let effects: Vec<Effect> = moved.iter().map(|d| d.effect(fa.modality)).collect();
        let agree = effects.iter().all(|e| *e == effects[0]) && effects[0] != Effect::NoDirection;
        if !agree {
            assert!(
                matches!(
                    dir(a, b),
                    Direction::ScopeDivergence | Direction::PolarityFlip
                ),
                "{a} -> {b}: axes {moved:?} disagree but the class is {:?}",
                dir(a, b)
            );
        }
    }
}

/// A single-axis pair has one axis, and it is the class: decision 2.
#[test]
fn a_single_axis_pair_has_exactly_its_own_class_as_its_axes() {
    for (a, b) in [
        (
            "The system shall log each command.",
            "The system may log each command.",
        ),
        (
            "The system shall respond within 200 ms.",
            "The system shall respond within 300 ms.",
        ),
        (
            "The system shall stop within 2 s.",
            "The system shall stop within 2 s when the tank is full.",
        ),
    ] {
        assert_eq!(axes_of(a, b), vec![dir(a, b)], "{a} -> {b}");
    }
}

#[test]
fn the_cli_names_every_moved_axis_beside_the_class() {
    let claim = |id: &str, statement: &str| {
        format!(
            "schema: {}\nclaim_id: {id}\nstatement: {statement:?}\napplicability: []\nrationale:\n  \
             source: tests/compare.rs\n  date: 2026-11-11\n  rejected_alternatives: []\n  assumptions: []\n\
             relations: []\n",
            claim_schema()
        )
    };
    let f = Fixture::new("r2-also")
        .claim(
            "REQ-0001.yml",
            &claim("REQ-0001", "The req tool shall respond within 100 ms."),
        )
        .claim(
            "REQ-0002.yml",
            &claim("REQ-0002", "The req tool should respond within 200 ms."),
        )
        .claim(
            "REQ-0003.yml",
            &claim("REQ-0003", "The req tool shall respond within 300 ms."),
        )
        .claim(
            "REQ-0004.yml",
            &claim("REQ-0004", "The req tool should respond within 10 ms."),
        )
        .claim(
            "REQ-0005.yml",
            &claim("REQ-0005", "The req tool shall respond within 100 ms."),
        );

    let (code, out) = f.compare("REQ-0001.yml", "REQ-0002.yml");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(": modality_weakening"), "{out}");
    assert!(out.contains("also: numeric_loosening"), "{out}");
    assert!(out.contains("permits strictly more"), "{out}");

    // A single axis names nothing beside itself.
    let (code, out) = f.compare("REQ-0001.yml", "REQ-0003.yml");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(": numeric_loosening"), "{out}");
    assert!(!out.contains("also:"), "{out}");

    // A mixed compound is `scope_divergence`, with both of its axes named and no effect claimed.
    let (code, out) = f.compare("REQ-0004.yml", "REQ-0005.yml");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(": scope_divergence"), "{out}");
    assert!(
        out.contains("also: modality_strengthening, numeric_loosening"),
        "{out}"
    );
    assert!(!out.contains("permits"), "{out}");
}
