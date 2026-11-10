//! `req selftest` — known-answer checks of **this binary** (ADR 0034).
//!
//! After an install, or on a machine nobody has run req on, the question is "does this build, here,
//! derive what the project's reference build derives?". The answers are compiled in, so the command
//! reads no corpus, resolves no root and needs no network: a SHA-256 test vector, field-level
//! expectations written from the hashing spec, equal and different pairs, address regression pins,
//! direction labels, and the seed parity corpus run through the false-merge gate.
//!
//! It is **not a defence against a tampered binary** (a binary that lies about its own checks passes
//! them) and not a proof that the rules are right: it catches the accidental differences, a wrong
//! build, a platform that computes something else, a rules change whose goldens were not regenerated.
//!
//! Two kinds of expectation, kept apart on purpose. The *field* rows, the *same*/*different* pairs, the
//! *direction* labels and the parity corpus's labels are independent oracles, written from the spec and
//! by hand. The *address goldens* are regression pins frozen from the implementation at a rules
//! version: they detect change, not error, and a compile-time assertion refuses a build whose goldens
//! belong to another `RULES_V`, so a rules change cannot forget them (the rules-change protocol).

use crate::bench;
use crate::canon::{
    CanonicalForm, RULES_V, address_hex, canonical_decimal, canonical_json, canonicalize_statement,
};
use crate::compare::compare;
use crate::model::{Corpus, Registry};

const PARITY_YAML: &str = include_str!("../bench/corpus/parity.yml");
const REGISTRY_YAML: &str = include_str!("../aliases/subjects.yml");

/// The rules version the address goldens below were recorded under.
const GOLDEN_RULES_V: u32 = 8;
const _: () = assert!(
    GOLDEN_RULES_V == RULES_V,
    "RULES_V moved: regenerate the address goldens in src/selftest.rs deliberately, in the same commit (hashing spec section 5)"
);

/// What a statement must derive, written from the spec. `operator` is `none` for a form with no bound.
#[derive(Debug, Clone)]
pub struct FieldRow {
    pub statement: String,
    pub modality: String,
    pub operator: String,
    pub value: Option<String>,
    pub unit: Option<String>,
    pub members: Vec<String>,
}

/// Two claims and the direction `compare` must report between them.
#[derive(Debug, Clone)]
pub struct DirectionRow {
    pub a: String,
    pub b: String,
    pub expected: String,
}

/// Every answer the command checks, as data, so a test can make one wrong and watch that check fail.
#[derive(Debug, Clone)]
pub struct KnownAnswers {
    pub digest_input: String,
    pub digest_expected: String,
    pub fields: Vec<FieldRow>,
    /// Statements that must share one address.
    pub same: Vec<(String, String)>,
    /// Statements that must NOT share one (the false-merge direction).
    pub different: Vec<(String, String)>,
    pub golden_rules_v: u32,
    /// `(statement, address)` regression pins under `golden_rules_v`.
    pub goldens: Vec<(String, String)>,
    pub directions: Vec<DirectionRow>,
    pub registry_yaml: String,
    pub parity_yaml: String,
}

/// One line of the report.
#[derive(Debug, Clone)]
pub struct Check {
    pub kind: &'static str,
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn s(text: &str) -> String {
    text.to_string()
}

fn field(
    statement: &str,
    modality: &str,
    operator: &str,
    value: Option<&str>,
    unit: Option<&str>,
    members: &[&str],
) -> FieldRow {
    FieldRow {
        statement: s(statement),
        modality: s(modality),
        operator: s(operator),
        value: value.map(s),
        unit: unit.map(s),
        members: members.iter().map(|m| s(m)).collect(),
    }
}

const RESPOND_200: &str = "The system shall respond within 200 ms.";
const RESPOND_500: &str = "The system shall respond within 500 ms.";
const RESPOND_LT: &str = "The system shall respond in less than 200 ms.";
const THREE_STANDARDS: &str =
    "The report shall cite one of the three standards, ISO 9001, IEC 61508 or IEC 62304.";
const TWO_STANDARDS: &str =
    "The report shall cite one of the two standards, ISO 9001 or IEC 61508.";

/// The reference answers compiled into this build.
pub fn known_answers() -> KnownAnswers {
    KnownAnswers {
        // NIST FIPS 180-2, appendix B.1.
        digest_input: s("abc"),
        digest_expected: s("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        fields: vec![
            // The hashing spec's worked example: 200 ms is operator LE, value 0.2, unit s.
            field(RESPOND_200, "SHALL", "LE", Some("0.2"), Some("s"), &[]),
            // The classic trap: "less than" is LT, a different claim from LE.
            field(RESPOND_LT, "SHALL", "LT", Some("0.2"), Some("s"), &[]),
            field(
                "The system shall not log a secret.",
                "SHALL-NOT",
                "none",
                None,
                None,
                &[],
            ),
            // ADR 0030: an enumerated "one of" is a set, sorted, with no number.
            field(
                THREE_STANDARDS,
                "SHALL",
                "IN",
                None,
                None,
                &["iec 61508", "iec 62304", "iso 9001"],
            ),
        ],
        same: vec![
            (
                s(RESPOND_200),
                s("The system shall respond no later than 0.2 seconds."),
            ),
            // Parity corpus E-005: an exact SI factor.
            (
                s("The system shall apply at most 12 kN to the pedal."),
                s("The system shall apply at most 12000 N to the pedal."),
            ),
            // ADR 0030: the order of a set's members is not meaning.
            (
                s(THREE_STANDARDS),
                s(
                    "The report shall cite one of the three standards, IEC 62304, ISO 9001 or IEC 61508.",
                ),
            ),
        ],
        different: vec![
            (s(RESPOND_200), s(RESPOND_LT)),
            (s(RESPOND_200), s(RESPOND_500)),
            (
                s("The system shall log a secret."),
                s("The system shall not log a secret."),
            ),
            (s(THREE_STANDARDS), s(TWO_STANDARDS)),
        ],
        golden_rules_v: GOLDEN_RULES_V,
        goldens: vec![
            (
                s(RESPOND_200),
                s("3b944b2365cd157ec3299c8bf834ec65c31d6e853d87ea9e3ba0f2433716fa73"),
            ),
            (
                s(RESPOND_LT),
                s("57ee9ede21932f45f510a5d210e0cc5b555baca7f53f4023a557e36b289c0270"),
            ),
            (
                s("The system shall not log a secret."),
                s("21189c5cf37226a5232f95cde40e40b5895a9ea3e15b59a29ac8ba6d951c79d2"),
            ),
            (
                s(THREE_STANDARDS),
                s("f836e6cb64345f5164fdf98cacae2dba5d729a3bfbc1bcab470c15fb99424bba"),
            ),
            (
                s("The system shall apply at most 12 kN to the pedal."),
                s("11f3a67a3ed7ac57bb0ee5b7f18575aa9b85ff1b2dff60ffdd7fff9da566cbd1"),
            ),
            (
                s("The system shall log each command when the brake is engaged."),
                s("3479535242f57b34cbed8d20731707b26d7fb31ee7cf062b4bca29f1c443f329"),
            ),
            (
                s("Authoring shall be friction-free."),
                s("d5f74f33838ed8d38007808350f74faba5d6f93600ff0bc354788f9b294b4d88"),
            ),
        ],
        directions: vec![
            DirectionRow {
                a: s(RESPOND_200),
                b: s(RESPOND_500),
                expected: s("numeric_loosening"),
            },
            DirectionRow {
                a: s(RESPOND_500),
                b: s(RESPOND_200),
                expected: s("numeric_tightening"),
            },
            DirectionRow {
                a: s(RESPOND_200),
                b: s("The system should respond within 200 ms."),
                expected: s("modality_weakening"),
            },
            DirectionRow {
                a: s("The system shall log a secret."),
                b: s("The system shall not log a secret."),
                expected: s("polarity_flip"),
            },
            DirectionRow {
                a: s("The system shall log each command."),
                b: s("The system shall log each command when the brake is engaged."),
                expected: s("scope_narrowing"),
            },
            DirectionRow {
                a: s(THREE_STANDARDS),
                b: s(TWO_STANDARDS),
                expected: s("membership_narrowing"),
            },
        ],
        registry_yaml: s(REGISTRY_YAML),
        parity_yaml: s(PARITY_YAML),
    }
}

fn derive(text: &str, registry: &Registry) -> CanonicalForm {
    canonicalize_statement(text, registry).0
}

fn check(kind: &'static str, name: String, ok: bool, detail: String) -> Check {
    Check {
        kind,
        name,
        ok,
        detail,
    }
}

/// Run every check. `Err` means the embedded data could not be read, which is a build defect: the answer
/// to "is this binary right?" cannot be told, and that is never a pass.
pub fn run_checks(known: &KnownAnswers) -> Result<Vec<Check>, String> {
    let registry = Registry::parse(&known.registry_yaml, "the embedded aliases/subjects.yml")?;
    let corpus = Corpus::parse(&known.parity_yaml, "the embedded bench/corpus/parity.yml")?;
    let mut out = Vec::new();

    let digest = crate::digest::sha256_hex(known.digest_input.as_bytes());
    out.push(check(
        "digest",
        format!("sha256({:?})", known.digest_input),
        digest == known.digest_expected,
        format!("got {digest}, the reference is {}", known.digest_expected),
    ));

    for row in &known.fields {
        let form = derive(&row.statement, &registry);
        let got = FieldRow {
            statement: row.statement.clone(),
            modality: form.modality_str().to_string(),
            operator: form.operator_str().to_string(),
            value: form.value.as_ref().map(canonical_decimal),
            unit: form.unit.clone().filter(|u| u != "none"),
            members: form.members.iter().cloned().collect(),
        };
        let ok = !form.unstructured
            && got.modality == row.modality
            && got.operator == row.operator
            && got.value == row.value
            && got.unit == row.unit
            && got.members == row.members;
        out.push(check(
            "fields",
            row.statement.clone(),
            ok,
            format!(
                "{} {} {:?} {:?} {:?}; the spec says {} {} {:?} {:?} {:?}",
                got.modality,
                got.operator,
                got.value,
                got.unit,
                got.members,
                row.modality,
                row.operator,
                row.value,
                row.unit,
                row.members
            ),
        ));
    }

    for (a, b) in &known.same {
        let (ja, jb) = (
            canonical_json(&derive(a, &registry)),
            canonical_json(&derive(b, &registry)),
        );
        out.push(check(
            "same",
            format!("{a:?} = {b:?}"),
            ja == jb,
            format!("{} against {}", address_hex(&ja), address_hex(&jb)),
        ));
    }
    for (a, b) in &known.different {
        let (ja, jb) = (
            canonical_json(&derive(a, &registry)),
            canonical_json(&derive(b, &registry)),
        );
        out.push(check(
            "different",
            format!("{a:?} != {b:?}"),
            ja != jb,
            format!("both derive {}", address_hex(&ja)),
        ));
    }

    if known.golden_rules_v != RULES_V {
        out.push(check(
            "address",
            "address goldens".to_string(),
            false,
            format!(
                "the goldens were recorded under rules_v {} and this build writes rules_v {RULES_V}: they say nothing about it",
                known.golden_rules_v
            ),
        ));
    } else {
        for (statement, expected) in &known.goldens {
            let got = address_hex(&canonical_json(&derive(statement, &registry)));
            out.push(check(
                "address",
                statement.clone(),
                &got == expected,
                format!(
                    "{statement:?} derives {got}, the pin under rules_v {RULES_V} is {expected}"
                ),
            ));
        }
    }

    for row in &known.directions {
        let got = compare(&derive(&row.a, &registry), &derive(&row.b, &registry));
        out.push(check(
            "direction",
            format!("{:?} -> {:?}", row.a, row.b),
            got.as_str() == row.expected,
            format!("reported {}, expected {}", got.as_str(), row.expected),
        ));
    }

    let evaluation = bench::evaluate(&corpus, &registry);
    let pairs = corpus.equivalent.len() + corpus.not_equivalent.len();
    out.push(check(
        "parity",
        format!(
            "seed parity corpus, {pairs} pairs, {} not-equivalent",
            evaluation.not_equivalent_total
        ),
        evaluation.fmr_gate == bench::Verdict::Pass,
        format!(
            "false-merge gate {}; false merges: {}",
            evaluation.fmr_gate,
            if evaluation.fmr_hits.is_empty() {
                "none".to_string()
            } else {
                evaluation.fmr_hits.join(", ")
            }
        ),
    ));
    Ok(out)
}

const USAGE: &str = "usage: req selftest";

fn tool_note(program: &str, args: &[&str], absent: &str) -> String {
    match std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) => {
            let text = String::from_utf8_lossy(if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            });
            let first = text.lines().next().unwrap_or("").trim().to_string();
            format!(
                "info: {program}: {}",
                if first.is_empty() { "present" } else { &first }
            )
        }
        Err(_) => format!("info: {program}: not found — {absent}"),
    }
}

/// `req selftest` — see the module doc and `SELFTEST_HELP` in `main.rs`.
pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}\nunrecognized argument: {}", args[0]);
        return 2;
    }
    let checks = match run_checks(&known_answers()) {
        Ok(checks) => checks,
        Err(cause) => {
            eprintln!(
                "[UNVERIFIABLE] the answers compiled into this build cannot be read: {cause}"
            );
            return 3;
        }
    };
    println!(
        "req selftest: known-answer checks of this binary (not a defence against a tampered binary)\n\
         rules_v {RULES_V}, baseline_envelope {}, claim_schema {}, built from {}",
        crate::baseline::BASELINE_VERSION,
        crate::canon::claim_schema(),
        env!("REQ_BUILD_SHA")
    );
    for c in &checks {
        println!(
            "{} {}: {}",
            if c.ok { "PASS" } else { "FAIL" },
            c.kind,
            c.name
        );
        if !c.ok {
            println!("     {}", c.detail);
        }
    }
    println!(
        "{}",
        tool_note(
            "git",
            &["--version"],
            "revisions cannot be read (diff, pin)"
        )
    );
    println!(
        "{}",
        tool_note(
            "minisign",
            &["-v"],
            "a Baseline cannot be verified (UNVERIFIABLE)"
        )
    );
    let failed = checks.iter().filter(|c| !c.ok).count();
    if failed == 0 {
        println!("selftest: PASS ({} checks)", checks.len());
        0
    } else {
        println!("selftest: FAIL ({failed} of {} checks)", checks.len());
        1
    }
}
