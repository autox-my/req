//! Q-1 item 2 — invariants of the Tier-1 canonicaliser, checked over generated statements.
//!
//! No property-testing crate: a fixed-seed integer PRNG (xorshift64*, wrapping integer ops, no
//! floats) composes statements from the vocabularies the rules already know — the subjects of
//! `aliases/subjects.yml`, the modalities, the comparators, values, units and conditions. A few
//! thousand cases per invariant, deterministic run to run, so a failure names a reproducible
//! input.
//!
//! The oracle is **independent of the canonicaliser**: a [`Spec`] says what a statement *means*
//! (subject, modality, comparator class, an exact quantity in integer base units, conditions),
//! and "same meaning" is decided from the spec, never read off the implementation. A property
//! that fails on today's code is a finding for the orchestrator, not a test to weaken.
//!
//! Arithmetic is integers only: a quantity is `mantissa × factor / 10^digits`, compared as an
//! exact integer in the dimension's smallest unit.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use req::canon::{
    CanonicalForm, Lint, address_hex, canonical_json, canonicalize_statement, normalize_text,
};
use req::model::Registry;

// ---------------------------------------------------------------------------
// The PRNG: xorshift64*, integer only
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).expect("a small bound");
        usize::try_from(self.next() % n).expect("below a usize bound")
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const CASES: usize = 3000;

fn registry() -> Registry {
    Registry::load("aliases/subjects.yml").expect("the repository's own registry loads")
}

fn canon(text: &str, registry: &Registry) -> (CanonicalForm, Vec<Lint>) {
    canonicalize_statement(text, registry)
}

fn address(text: &str, registry: &Registry) -> String {
    address_hex(&canonical_json(&canon(text, registry).0))
}

// ---------------------------------------------------------------------------
// Vocabularies
// ---------------------------------------------------------------------------

/// The registered surface aliases with the registry subject each resolves to (read from the
/// registry file, so the oracle knows two aliases of one subject are one subject), plus one
/// unregistered subject that stands for itself.
fn subjects() -> Vec<(String, String)> {
    let text = std::fs::read_to_string("aliases/subjects.yml").expect("the registry file");
    let mut found: Vec<(String, String)> = Vec::new();
    let mut key = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 2 && trimmed.ends_with(':') {
            key = trimmed.trim_end_matches(':').to_string();
        } else if let Some(alias) = trimmed.strip_prefix("- ") {
            let alias = alias.trim();
            if indent >= 6 && !key.is_empty() && !alias.contains(':') {
                found.push((alias.to_string(), key.clone()));
            }
        }
    }
    found.push((
        "the charging station".to_string(),
        "the charging station".to_string(),
    ));
    found
}

fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The modal phrases the rules resolve, with the modality each carries.
const MODALS: &[(&str, &str)] = &[
    ("shall", "SHALL"),
    ("must", "SHALL"),
    ("should", "SHOULD"),
    ("may", "MAY"),
    ("shall not", "SHALL-NOT"),
];

const ACTIONS: &[&str] = &[
    "respond",
    "complete the check",
    "store",
    "apply",
    "drain the buffer",
];

const TAILS: &[&str] = &["", "", "", " after the request", " on the parity corpus"];

/// A comparator class and the phrases the T1.5 table reads as that class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Le,
    Lt,
    Ge,
    Gt,
    Eq,
}

const PHRASES: &[(Class, &[&str])] = &[
    (
        Class::Le,
        &[
            "within",
            "no later than",
            "at most",
            "up to",
            "no more than",
            "not more than",
        ],
    ),
    (Class::Lt, &["less than", "in fewer than", "in less than"]),
    (Class::Ge, &["at least", "no less than", "not less than"]),
    (Class::Gt, &["more than", "greater than"]),
    (Class::Eq, &["exactly", "equal to"]),
];

fn phrases_of(class: Class) -> &'static [&'static str] {
    PHRASES
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, p)| *p)
        .expect("every class has phrases")
}

/// A dimension, and its unit aliases with the exact factor into the dimension's base unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dim {
    Time,
    Length,
}

/// `(alias, dimension, factor in the base unit)`: time in nanoseconds, length in micrometres.
const UNITS: &[(&str, Dim, u128)] = &[
    ("s", Dim::Time, 1_000_000_000),
    ("sec", Dim::Time, 1_000_000_000),
    ("seconds", Dim::Time, 1_000_000_000),
    ("ms", Dim::Time, 1_000_000),
    ("msec", Dim::Time, 1_000_000),
    ("milliseconds", Dim::Time, 1_000_000),
    ("us", Dim::Time, 1_000),
    ("microseconds", Dim::Time, 1_000),
    ("ns", Dim::Time, 1),
    ("nanoseconds", Dim::Time, 1),
    ("m", Dim::Length, 1_000_000),
    ("meters", Dim::Length, 1_000_000),
    ("mm", Dim::Length, 1_000),
    ("millimeters", Dim::Length, 1_000),
    ("cm", Dim::Length, 10_000),
    ("km", Dim::Length, 1_000_000_000),
];

fn unit_row(alias: &str) -> (Dim, u128) {
    UNITS
        .iter()
        .find(|(a, _, _)| *a == alias)
        .map(|(_, d, f)| (*d, *f))
        .expect("a known unit alias")
}

/// An exact quantity as written: a decimal literal (`digits` after the point at most 3) and a unit alias.
#[derive(Clone, Debug)]
struct Qty {
    /// The literal as written, e.g. "0.150".
    text: String,
    unit: &'static str,
}

impl Qty {
    /// The quantity in the dimension's base unit, scaled by 10^3 so a three-digit fraction is an
    /// integer. Equal quantities are equal integers; nothing is rounded.
    fn exact(&self) -> (Dim, u128) {
        let (whole, fraction) = self.text.split_once('.').unwrap_or((&self.text, ""));
        let digits = u32::try_from(fraction.len()).expect("at most three digits");
        let mantissa: u128 = format!("{whole}{fraction}")
            .parse()
            .expect("a digit string");
        let (dim, factor) = unit_row(self.unit);
        (dim, mantissa * factor * 10u128.pow(3 - digits))
    }
}

/// A decimal literal from integers: a whole part and 0–3 fraction digits, never a float.
fn literal(rng: &mut Rng) -> String {
    let whole = rng.below(2000) + 1;
    match rng.below(3) {
        0 => whole.to_string(),
        1 => format!("{whole}.{}", rng.below(9) + 1),
        _ => format!("{whole}.{:03}", rng.below(999) + 1),
    }
}

fn qty(rng: &mut Rng) -> Qty {
    let (alias, _, _) = *rng.pick(UNITS);
    Qty {
        text: literal(rng),
        unit: alias,
    }
}

/// The same quantity, written with another alias of the same dimension and a different literal
/// that is exactly equal when one exists (a whole number of a smaller unit, or its trailing-zero form).
fn equal_twin(rng: &mut Rng, q: &Qty) -> Qty {
    let (dim, base) = q.exact();
    // every alias of the dimension whose factor divides the base value exactly into an integer
    // number of three-digit-fraction steps; the literal is rebuilt from integers.
    let mut options: Vec<Qty> = Vec::new();
    for (alias, d, factor) in UNITS {
        if *d != dim {
            continue;
        }
        let scaled = base / factor;
        if scaled * factor != base {
            continue;
        }
        // `scaled` is value × 10^3; render it with 3 fraction digits, then drop trailing zeros
        // by a random amount (including all of them), to vary the literal.
        let (whole, fraction) = (scaled / 1000, scaled % 1000);
        let full = format!("{whole}.{fraction:03}");
        let trimmed = full.trim_end_matches('0').trim_end_matches('.').to_string();
        options.push(Qty {
            text: trimmed,
            unit: alias,
        });
        options.push(Qty {
            text: full,
            unit: alias,
        });
    }
    rng.pick(&options).clone()
}

// ---------------------------------------------------------------------------
// A statement's meaning, and its rendering
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Spec {
    subject: String,
    /// What the registry says the subject is: two aliases of one subject share it.
    subject_key: String,
    modal: &'static str,
    modality: &'static str,
    action: &'static str,
    bound: Option<(Class, &'static str, Qty)>,
    tail: &'static str,
    /// `(guard, condition)` and whether it leads the sentence.
    cond: Option<(&'static str, String, bool)>,
}

impl Spec {
    fn render(&self) -> String {
        let mut body = format!("{} {} {}", self.subject, self.modal, self.action);
        if let Some((_, phrase, q)) = &self.bound {
            body.push_str(&format!(" {phrase} {} {}", q.text, q.unit));
        }
        body.push_str(self.tail);
        match &self.cond {
            Some((guard, cond, true)) => {
                format!("{} {cond}, {}.", sentence_case(guard), lower_first(&body))
            }
            Some((guard, cond, false)) => format!("{} {guard} {cond}.", sentence_case(&body)),
            None => format!("{}.", sentence_case(&body)),
        }
    }

    /// Whether the meaning is the same as `other`'s, decided from the spec alone. A comparator
    /// phrase inside one class is the same bound; a quantity is the same iff exactly equal.
    fn same_meaning(&self, other: &Spec) -> bool {
        let bound = |s: &Spec| s.bound.as_ref().map(|(c, _, q)| (*c, q.exact()));
        self.subject_key == other.subject_key
            && self.modality == other.modality
            && self.action == other.action
            && bound(self) == bound(other)
            && self.tail == other.tail
            && self.cond.as_ref().map(|(g, c, _)| (*g, c.clone()))
                == other.cond.as_ref().map(|(g, c, _)| (*g, c.clone()))
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

const GUARDS: &[&str] = &["when", "if", "while", "unless"];

fn condition(rng: &mut Rng) -> String {
    const STATES: &[&str] = &[
        "auto-brake is enabled",
        "the valve is open",
        "the request is retried",
        "the link is degraded",
    ];
    format!("{} on channel {}", rng.pick(STATES), rng.below(90) + 10)
}

fn spec(rng: &mut Rng, subjects: &[(String, String)], bounded: bool) -> Spec {
    let (modal, modality) = *rng.pick(MODALS);
    let bound = bounded.then(|| {
        let (class, phrases) = rng.pick(PHRASES);
        (*class, *rng.pick(phrases), qty(rng))
    });
    let (alias, key) = rng.pick(subjects);
    Spec {
        subject: sentence_case(alias),
        subject_key: key.clone(),
        modal,
        modality,
        action: rng.pick(ACTIONS),
        bound,
        tail: rng.pick(TAILS),
        cond: None,
    }
}

fn with_condition(rng: &mut Rng, base: &Spec) -> Spec {
    let mut s = base.clone();
    s.cond = Some((*rng.pick(GUARDS), condition(rng), rng.below(2) == 0));
    s
}

fn structured(text: &str, registry: &Registry) -> bool {
    !canon(text, registry).0.unstructured
}

// ---------------------------------------------------------------------------
// Noise: the inputs a generator that only writes well-formed prose would never try
// ---------------------------------------------------------------------------

fn noise(rng: &mut Rng, subjects: &[(String, String)]) -> String {
    const BITS: &[&str] = &[
        "shall",
        "shall not",
        "may not",
        "must never",
        "within",
        "at most",
        "less than",
        "exactly",
        "one of",
        "and",
        "or",
        ",",
        ".",
        ";",
        "(",
        ")",
        "\"",
        "'",
        "-",
        "--",
        "+",
        "0",
        "0.0",
        "00",
        "1e5",
        "1,000",
        "99999999999999999999999999999999",
        "0.0000000000000000000001",
        "٣",
        "é",
        "e\u{301}",
        "\u{a0}",
        "\u{200b}",
        "ß",
        "İ",
        "ms",
        "%",
        "°c",
        "km/h",
        "when",
        "if",
        "unless",
        "the",
        "of",
        "respond",
        "log each command",
        "\n",
        "\t",
        "  ",
    ];
    let mut words: Vec<String> = Vec::new();
    for _ in 0..rng.below(14) {
        if rng.below(4) == 0 {
            words.push(rng.pick(subjects).0.clone());
        } else {
            words.push((*rng.pick(BITS)).to_string());
        }
    }
    words.join(" ")
}

// ---------------------------------------------------------------------------
// The invariants
// ---------------------------------------------------------------------------

/// Well-formed, plain and noisy inputs, all from one fixed seed.
fn inputs(seed: u64) -> Vec<String> {
    let subjects = subjects();
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for i in 0..CASES {
        if i % 3 == 0 {
            out.push(noise(&mut rng, &subjects));
        } else {
            let base = spec(&mut rng, &subjects, i % 3 == 1);
            let s = if rng.below(2) == 0 {
                with_condition(&mut rng, &base)
            } else {
                base
            };
            out.push(s.render());
        }
    }
    out
}

#[test]
fn canonicalising_never_panics_and_is_deterministic() {
    let registry = registry();
    for text in inputs(0x5EED_0001) {
        let first = catch_unwind(AssertUnwindSafe(|| {
            let (form, _) = canon(&text, &registry);
            let json = canonical_json(&form);
            (address_hex(&json), json)
        }))
        .unwrap_or_else(|_| panic!("canonicalize_statement panicked on {text:?}"));
        let second = {
            let (form, _) = canon(&text, &registry);
            let json = canonical_json(&form);
            (address_hex(&json), json)
        };
        assert_eq!(first, second, "a second call disagrees on {text:?}");
    }
}

/// The nearest true property: the code offers no rendering of a canonical form back to prose
/// (the form is JSON, not text), so idempotence is stated on the one normalisation the rules
/// apply to prose — `normalize_text` — and on what that normalisation is *for*: re-reading the
/// normalised text gives the same address as the original.
#[test]
fn normalisation_is_idempotent_and_re_reading_it_keeps_the_address() {
    let registry = registry();
    for text in inputs(0x5EED_0002) {
        let once = normalize_text(&text);
        let twice = normalize_text(&once);
        assert_eq!(once, twice, "normalize_text is not idempotent on {text:?}");
        assert_eq!(
            address(&text, &registry),
            address(&once, &registry),
            "canonicalising the normalised text moved the address of {text:?} (normalised: {once:?})"
        );
    }
}

/// T1.10: a statement that parses as structured leaves no word unattributed. The code reports an
/// unattributed word as an `uncovered_content` lint and falls back to unstructured, so (1) a
/// structured form never carries that lint, and (2) the lint is the *only* thing that makes a
/// well-formed statement unstructured here. Stated on identity: a fresh content word appended to
/// the tail of a structured statement either moves the address or leaves the structured form.
#[test]
fn the_coverage_invariant_holds() {
    let registry = registry();
    let subjects = subjects();
    let mut rng = Rng::new(0x5EED_0003);
    let mut checked = 0usize;
    for _ in 0..CASES {
        let s = spec(&mut rng, &subjects, true);
        let text = s.render();
        let (form, lints) = canon(&text, &registry);
        if form.unstructured {
            continue;
        }
        checked += 1;
        assert!(
            !lints.iter().any(|l| l.code == "uncovered_content"),
            "a structured form with an uncovered word: {text:?} {lints:?}"
        );
        // a nonce word nobody has a slot for
        let nonce = format!("zq{}x", rng.below(100_000));
        let extended = text.trim_end_matches('.').to_string() + " " + &nonce + ".";
        let (after, _) = canon(&extended, &registry);
        assert!(
            after.unstructured || address(&extended, &registry) != address(&text, &registry),
            "the word {nonce:?} left identity in silence: {text:?} -> {extended:?}"
        );
    }
    assert!(
        checked > CASES / 2,
        "too few structured cases ({checked}) for the property to mean anything"
    );
}

#[test]
fn adding_a_condition_always_moves_the_address() {
    let registry = registry();
    let subjects = subjects();
    let mut rng = Rng::new(0x5EED_0004);
    let mut checked = 0usize;
    for _ in 0..CASES {
        let bounded = rng.below(2) == 0;
        let base = spec(&mut rng, &subjects, bounded);
        let conditioned = with_condition(&mut rng, &base);
        let (plain, conditioned_text) = (base.render(), conditioned.render());
        if !structured(&plain, &registry) {
            continue;
        }
        checked += 1;
        assert_ne!(
            address(&plain, &registry),
            address(&conditioned_text, &registry),
            "a condition was added and the address did not move: {plain:?} -> {conditioned_text:?}"
        );
    }
    assert!(checked > CASES / 2, "too few structured cases ({checked})");
}

#[test]
fn le_never_collapses_with_lt_and_ge_never_with_gt() {
    let registry = registry();
    let subjects = subjects();
    let mut rng = Rng::new(0x5EED_0005);
    let mut checked = 0usize;
    for _ in 0..CASES {
        let base = spec(&mut rng, &subjects, true);
        let q = qty(&mut rng);
        let (lower, upper) = if rng.below(2) == 0 {
            (Class::Le, Class::Lt)
        } else {
            (Class::Ge, Class::Gt)
        };
        let mut a = base.clone();
        let mut b = base.clone();
        a.bound = Some((lower, *rng.pick(phrases_of(lower)), q.clone()));
        b.bound = Some((upper, *rng.pick(phrases_of(upper)), q));
        let (ta, tb) = (a.render(), b.render());
        if structured(&ta, &registry) && structured(&tb, &registry) {
            checked += 1;
        }
        assert_ne!(
            address(&ta, &registry),
            address(&tb, &registry),
            "{lower:?} and {upper:?} collapsed: {ta:?} vs {tb:?}"
        );
    }
    assert!(checked > CASES / 2, "too few structured pairs ({checked})");
}

#[test]
fn unit_conversion_is_exact() {
    let registry = registry();
    let subjects = subjects();
    let mut rng = Rng::new(0x5EED_0006);
    let (mut same, mut apart) = (0usize, 0usize);
    for _ in 0..CASES {
        let base = spec(&mut rng, &subjects, true);
        let (class, phrase, q) = base.bound.clone().expect("a bounded spec");
        // an exactly equal quantity in another unit alias or literal form
        let mut twin = base.clone();
        twin.bound = Some((class, phrase, equal_twin(&mut rng, &q)));
        let (ta, tb) = (base.render(), twin.render());
        if structured(&ta, &registry) {
            assert_eq!(
                address(&ta, &registry),
                address(&tb, &registry),
                "equal quantities must share an address: {ta:?} vs {tb:?}"
            );
            same += 1;
        }
        // an unequal quantity (or another dimension) never shares one
        let mut other = base.clone();
        let q2 = qty(&mut rng);
        other.bound = Some((class, phrase, q2));
        if !base.same_meaning(&other) {
            let tc = other.render();
            assert_ne!(
                address(&ta, &registry),
                address(&tc, &registry),
                "unequal quantities collapsed: {ta:?} vs {tc:?}"
            );
            apart += 1;
        }
    }
    assert!(
        same > CASES / 2 && apart > CASES / 2,
        "same {same}, apart {apart}"
    );
}

/// Beyond the named traps: any two specs the oracle calls different never share an address.
#[test]
fn a_difference_in_meaning_never_collapses() {
    let registry = registry();
    let subjects = subjects();
    let mut rng = Rng::new(0x5EED_0007);
    let mut apart = 0usize;
    for _ in 0..CASES {
        let bounded = rng.below(4) != 0;
        let a = spec(&mut rng, &subjects, bounded);
        let mut b = a.clone();
        match rng.below(4) {
            0 => {
                let (alias, key) = rng.pick(&subjects);
                b.subject = sentence_case(alias);
                b.subject_key = key.clone();
            }
            1 => {
                let (modal, modality) = *rng.pick(MODALS);
                b.modal = modal;
                b.modality = modality;
            }
            2 => b.action = rng.pick(ACTIONS),
            _ => b.tail = rng.pick(TAILS),
        }
        if a.same_meaning(&b) {
            continue;
        }
        let (ta, tb) = (a.render(), b.render());
        assert_ne!(
            address(&ta, &registry),
            address(&tb, &registry),
            "collapsed: {ta:?} vs {tb:?}"
        );
        apart += 1;
    }
    assert!(apart > CASES / 4, "too few differing pairs ({apart})");
}

// ---------------------------------------------------------------------------
// Known-answer twins: the generator's own wording, against the labelled corpus
// ---------------------------------------------------------------------------

/// A corpus pair, restated as two specs. Rendering the specs must reproduce the corpus text, the
/// oracle must give the corpus's label, and the canonicaliser must agree with both.
fn twin_specs() -> Vec<(&'static str, Spec, Spec, bool)> {
    let system = |action: &'static str,
                  bound: Option<(Class, &'static str, Qty)>,
                  tail: &'static str| Spec {
        subject: "The system".to_string(),
        subject_key: "the system".to_string(),
        modal: "shall",
        modality: "SHALL",
        action,
        bound,
        tail,
        cond: None,
    };
    let q = |text: &str, unit: &'static str| Qty {
        text: text.to_string(),
        unit,
    };
    let mut n004_b = system("log each command", None, "");
    n004_b.cond = Some(("when", "auto-brake is enabled".to_string(), false));
    let mut n001_b = system("log each command", None, "");
    n001_b.modal = "may";
    n001_b.modality = "MAY";
    vec![
        (
            "E-001",
            system("respond", Some((Class::Le, "within", q("200", "ms"))), ""),
            system(
                "respond",
                Some((Class::Le, "no later than", q("200", "ms"))),
                "",
            ),
            true,
        ),
        (
            "E-002",
            system(
                "complete the check",
                Some((Class::Le, "within", q("150", "ms"))),
                "",
            ),
            system(
                "complete the check",
                Some((Class::Le, "within", q("0.15", "seconds"))),
                "",
            ),
            true,
        ),
        (
            "E-012",
            system(
                "complete the check",
                Some((Class::Le, "no more than", q("3", "seconds"))),
                " after the request",
            ),
            system(
                "complete the check",
                Some((Class::Le, "at most", q("3", "seconds"))),
                " after the request",
            ),
            true,
        ),
        ("N-001", system("log each command", None, ""), n001_b, false),
        (
            "N-002",
            system("respond", Some((Class::Le, "within", q("200", "ms"))), ""),
            system(
                "respond",
                Some((Class::Lt, "in fewer than", q("200", "ms"))),
                "",
            ),
            false,
        ),
        (
            "N-003",
            system("respond", Some((Class::Le, "within", q("200", "ms"))), ""),
            system("respond", Some((Class::Le, "within", q("250", "ms"))), ""),
            false,
        ),
        ("N-004", system("log each command", None, ""), n004_b, false),
    ]
}

#[test]
fn every_generated_shape_has_a_known_answer_twin_in_the_corpus() {
    let registry = registry();
    let corpus = std::fs::read_to_string("bench/corpus/parity.yml").expect("the parity corpus");
    let parsed: yaml_serde::Value = yaml_serde::from_str(&corpus).expect("the corpus parses");
    let mut statements: std::collections::BTreeMap<String, (String, String)> = Default::default();
    let mut labelled: std::collections::BTreeMap<String, bool> = Default::default();
    for (section, equivalent) in [("equivalent", true), ("not_equivalent", false)] {
        for pair in parsed[section].as_sequence().expect("a pair list") {
            let id = pair["id"].as_str().expect("an id").to_string();
            let (a, b) = (
                pair["a"].as_str().expect("a"),
                pair["b"].as_str().expect("b"),
            );
            statements.insert(id.clone(), (a.to_string(), b.to_string()));
            labelled.insert(id, equivalent);
        }
    }

    for (id, a, b, same) in twin_specs() {
        let (corpus_a, corpus_b) = statements
            .get(id)
            .unwrap_or_else(|| panic!("{id} is not in the corpus"));
        // the twin is the corpus pair, word for word: the generator and the corpus say one thing
        assert_eq!(
            &a.render(),
            corpus_a,
            "{id}: the generator's wording drifted from the corpus"
        );
        assert_eq!(
            &b.render(),
            corpus_b,
            "{id}: the generator's wording drifted from the corpus"
        );
        // the corpus label, the oracle and the canonicaliser agree
        assert_eq!(
            labelled[id], same,
            "{id}: the twin's label is not the corpus label"
        );
        assert_eq!(
            a.same_meaning(&b),
            same,
            "{id}: the oracle disagrees with the corpus"
        );
        assert_eq!(
            address(corpus_a, &registry) == address(corpus_b, &registry),
            same,
            "{id}: the canonicaliser disagrees with the corpus"
        );
    }
    // the properties above run over these shapes and their neighbours
    let covered: BTreeSet<&str> = twin_specs().iter().map(|(id, ..)| *id).collect();
    assert!(covered.contains("N-002") && covered.contains("E-002") && covered.contains("N-004"));
}
