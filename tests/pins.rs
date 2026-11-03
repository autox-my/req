//! Pins (ADR 0022): what an author states a claim **must parse to**, judged against the parse
//! derived at load. This file replaced `tests/parity_annotations.rs`, whose four-case table
//! (contradiction / omission / annotation / ok, ADR 0014) judged fifteen authored slots that
//! `req.claim/2` removed. What is left is a two-outcome rule with one way to agree:
//!
//! | the parse holds a value for the key? | equal to the pin? | verdict |
//! |---|---|---|
//! | yes | yes | agrees |
//! | yes | no | flag — contradiction (the derived value is named) |
//! | no | — | flag — not carried |
//!
//! There is no annotation category: a pin the parse does not carry is exactly the bound that
//! would otherwise leave identity in silence, and the annotation is what hid REQ-0020's defect.
//! Every expectation below is stated from the statement's words (an independent oracle), never
//! read off the canonicalizer.

use std::path::{Path, PathBuf};
use std::process::Command;

use req::canon::{PinVerdict, canonicalize_statement, evaluate_pins, parse_claim};
use req::model::{Pins, Registry, validate_pins};

/// A registry that resolves the fixtures' subject, in memory: `Registry::load` reads a file, so
/// the fixture writes one into a per-process directory and removes it once loaded.
fn registry() -> Registry {
    let dir = std::env::temp_dir().join(format!("pins-registry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("creates the registry fixture directory");
    let path = dir.join(format!("subjects-{:?}.yml", std::thread::current().id()));
    std::fs::write(
        &path,
        "schema: req.subject_registry/0\nversion: 1\nsubjects:\n  API:\n    label: The API\n    aliases:\n      - the api\n",
    )
    .expect("writes the registry fixture");
    let loaded = Registry::load(path.to_str().expect("utf-8 path")).expect("registry loads");
    let _ = std::fs::remove_file(&path);
    loaded
}

/// `200 ms` is `0.2 s` under T1.4, `within` is `LE` under T1.5: the oracle is the statement.
const QUANTITY: &str = "The API shall respond to a health check within 200 ms.";
/// A statement with no quantity at all.
const QUALITATIVE: &str =
    "The API shall treat a single claim as the unit of requirements authorship.";
/// REQ-0007's statement: two obligations, so the §4 fallback — nothing structured to hold.
const UNSTRUCTURED: &str = "Authoring shall be friction-free and the decision boundary shall be \
                            deliberately high-friction, demanding role, meaning, and evidence there.";

fn pins(
    modality: Option<&str>,
    subject: Option<&str>,
    operator: Option<&str>,
    value: Option<&str>,
    unit: Option<&str>,
) -> Pins {
    Pins {
        modality: modality.map(String::from),
        subject: subject.map(String::from),
        operator: operator.map(String::from),
        value: value.map(String::from),
        unit: unit.map(String::from),
    }
}

fn verdicts(statement: &str, pins: &Pins) -> Vec<(&'static str, PinVerdict)> {
    let (form, _) = canonicalize_statement(statement, &registry());
    evaluate_pins(pins, &form)
        .into_iter()
        .map(|check| (check.key, check.verdict))
        .collect()
}

/// One table row: its name, the statement, the pins, and the verdicts expected per key.
type PinRow = (
    &'static str,
    &'static str,
    Pins,
    Vec<(&'static str, PinVerdict)>,
);

fn contradiction(derived: &str) -> PinVerdict {
    PinVerdict::Contradiction(derived.to_string())
}

/// The whole rule, table-driven over the three statement shapes and every pinnable key.
#[test]
fn evaluate_pins_agrees_contradicts_or_is_not_carried() {
    let rows: Vec<PinRow> = vec![
        (
            "every slot of the quantity agrees",
            QUANTITY,
            pins(
                Some("SHALL"),
                Some("API"),
                Some("LE"),
                Some("0.2"),
                Some("s"),
            ),
            vec![
                ("modality", PinVerdict::Agrees),
                ("subject", PinVerdict::Agrees),
                ("operator", PinVerdict::Agrees),
                ("value", PinVerdict::Agrees),
                ("unit", PinVerdict::Agrees),
            ],
        ),
        (
            "every slot of the quantity contradicted, each naming the parse",
            QUANTITY,
            pins(
                Some("SHOULD"),
                Some("BENCH"),
                Some("LT"),
                Some("200"),
                Some("ms"),
            ),
            vec![
                ("modality", contradiction("SHALL")),
                ("subject", contradiction("API")),
                ("operator", contradiction("LE")),
                ("value", contradiction("0.2")),
                ("unit", contradiction("s")),
            ],
        ),
        (
            "a quantity pinned on a qualitative statement is not carried",
            QUALITATIVE,
            pins(None, None, Some("EQ"), Some("1"), Some("s")),
            vec![
                ("operator", PinVerdict::NotCarried),
                ("value", PinVerdict::NotCarried),
                ("unit", PinVerdict::NotCarried),
            ],
        ),
        (
            "`unit: none` agrees with a unitless parse",
            QUALITATIVE,
            pins(Some("SHALL"), Some("API"), None, None, Some("none")),
            vec![
                ("modality", PinVerdict::Agrees),
                ("subject", PinVerdict::Agrees),
                ("unit", PinVerdict::Agrees),
            ],
        ),
        (
            "`unit: none` over a parse that carries a unit is a contradiction",
            QUANTITY,
            pins(None, None, None, None, Some("none")),
            vec![("unit", contradiction("s"))],
        ),
        (
            "an unstructured statement carries nothing, so every pin flags",
            UNSTRUCTURED,
            pins(
                Some("SHALL"),
                Some("API"),
                Some("EQ"),
                Some("0"),
                Some("none"),
            ),
            vec![
                ("modality", PinVerdict::NotCarried),
                ("subject", PinVerdict::NotCarried),
                ("operator", PinVerdict::NotCarried),
                ("value", PinVerdict::NotCarried),
                ("unit", PinVerdict::NotCarried),
            ],
        ),
        ("no pins, no verdicts", QUANTITY, Pins::default(), vec![]),
    ];
    for (name, statement, pinned, expected) in rows {
        assert_eq!(verdicts(statement, &pinned), expected, "{name}");
    }
}

/// A pin's values are compared exactly, so a pin that could never agree — a modality outside
/// the canonicalizer's words, a comparator outside T1.5, a non-canonical decimal — is refused
/// at load where the file can be named, rather than surfacing as a contradiction whose cause is
/// a typo. `req.claim/1`'s `1_000`, `200ms` and `0.2s` (which it had to classify as quantities
/// rather than annotations) cannot be pinned at all.
#[test]
fn a_pin_that_could_never_agree_is_refused_at_load() {
    let ok = pins(
        Some("SHALL-NOT"),
        Some("API"),
        Some("GE"),
        Some("-2.5"),
        Some("s"),
    );
    assert_eq!(validate_pins(&ok), Ok(()));
    for (name, bad) in [
        ("modality", pins(Some("shall"), None, None, None, None)),
        (
            "modality",
            pins(Some("UNSTRUCTURED"), None, None, None, None),
        ),
        ("operator", pins(None, None, Some("<="), None, None)),
        ("operator", pins(None, None, Some("none"), None, None)),
        ("value", pins(None, None, None, Some("0.30"), None)),
        ("value", pins(None, None, None, Some(".3"), None)),
        ("value", pins(None, None, None, Some("1_000"), None)),
        ("value", pins(None, None, None, Some("200ms"), None)),
        ("value", pins(None, None, None, Some("3e-1"), None)),
        ("value", pins(None, None, None, Some("+1"), None)),
        ("subject", pins(None, Some(""), None, None, None)),
        ("unit", pins(None, None, None, None, Some(" s"))),
    ] {
        let error = validate_pins(&bad).expect_err("refused");
        assert!(error.contains(name), "{name}: {error}");
    }
}

/// `pins:` is strict: a key that is not pinnable is a parse error, and so is the qualitative
/// `attribute` the annotation category used to carry (ADR 0022 removed it).
#[test]
fn only_the_five_pinnable_keys_parse() {
    let base = "schema: req.claim/2\nclaim_id: REQ-0001\nstatement: The API shall respond within 200 ms.\n\
                applicability: []\nrelations: []\nrationale:\n  source: x\n  date: 2026-10-07\n  \
                rejected_alternatives: []\n  assumptions: []\n";
    for key in [
        "modality: SHALL",
        "subject: API",
        "operator: LE",
        "value: \"0.2\"",
        "unit: s",
    ] {
        parse_claim(&format!("{base}pins:\n  {key}\n"))
            .unwrap_or_else(|e| panic!("`{key}` is pinnable: {e}"));
    }
    for key in ["attribute: latency", "conditions: []", "qualifier: x"] {
        let error = parse_claim(&format!("{base}pins:\n  {key}\n")).expect_err("refused");
        let name = key.split(':').next().unwrap();
        assert!(error.contains(name), "{key}: {error}");
    }
}

// ---------------------------------------------------------------------------
// The committed corpus, through the CLI.
// ---------------------------------------------------------------------------

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_at(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(env!("CARGO_BIN_EXE_req"))
        .args(args)
        .current_dir(cwd)
        // The tests assert against the checkout's own corpus, so an inherited root would
        // silently point them at another tree.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn checkout() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn row<'a>(stdout: &'a str, id: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.starts_with(id) && line[id.len()..].starts_with(' '))
        .unwrap_or_else(|| panic!("no row for {id} in:\n{stdout}"))
}

/// The migration's pins hold on the committed corpus. The count is data, from the migration
/// rule (`tools/migrate_claims.py`): a numeric authored `value`, an operator other than
/// `EQ`/none, a unit other than `none` — REQ-0008 (value), REQ-0009 (value, GE), REQ-0010
/// (value, LE), REQ-0022 (value) = 6 pins on 4 claims of 21. If a claim joins the corpus or a
/// pin is added, it moves.
#[test]
fn the_corpus_pins_hold_six_pins_on_four_claims() {
    let out = run_at(&checkout(), &["canon"]);
    assert_eq!(out.code, 0, "canon exit code:\n{}", out.stderr);
    assert!(
        out.stdout
            .contains("claims: 21 (1 unstructured); pins: 6 on 4 claim(s), 0 flagged"),
        "{}",
        out.stdout
    );
    // REQ-0007 is derived-unstructured, as it was authored-unstructured in req.claim/1.
    assert!(row(&out.stdout, "REQ-0007").contains(" unstructured "));
    // REQ-0014's `exactly one commit` yields `1 / EQ`: shown as the parse, with no pin to hold
    // and no annotation category to hide it in.
    let stray = row(&out.stdout, "REQ-0014");
    assert!(
        stray.contains("op EQ") && stray.contains("value 1 "),
        "{stray}"
    );
    assert!(!out.stdout.contains("FLAG"), "{}", out.stdout);
}

/// No `RULES_V` bump and no address moves: only the file contract changed. REQ-0015's address is
/// the witness this file has carried since ADR 0014 (`4a15b28c…`), measured before the change.
#[test]
fn the_parity_change_moves_no_address() {
    let out = run_at(&checkout(), &["render"]);
    let prohibition = row(&out.stdout.replace("- REQ-", "REQ-"), "REQ-0015").to_string();
    assert!(
        prohibition.contains("4a15b28c8eba"),
        "REQ-0015's address must not move:\n{prohibition}"
    );
}
