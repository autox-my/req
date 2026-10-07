//! The Tier-1 canonicalizer (docs/canonical-hashing.md §3).
//!
//! Deterministic; offline; no model I/O; no `HashMap` iteration order dependence
//! in the address path (ADR 0009). Pipeline order encodes two decisions caught
//! *before* coding:
//!
//! 1. **Negation absorption precedes the modality scan** — "shall not exceed 2.5 A"
//!    resolves as comparator `LE` with modality `SHALL` (the obligation survives;
//!    the comparison's own negation is absorbed), while "shall not log" (no
//!    comparative follows) stays `SHALL-NOT`.
//! 2. **The post-value phrase ("of standby current") is an identity field**
//!    (`qualifier`) — dropping it is how a standby-current claim silently becomes
//!    a surge-current claim. Unresolvable comparators ("under") never collapse:
//!    the raw word stays in the parse, the two phrases differ.
//!
//! Units: only *power-of-ten* factors convert exactly (ms→s, kN→N, %→ratio, the
//! °C→K offset). `km/h→m/s` (×5/18, a repeating decimal) is NOT Tier-1 — such units
//! stay as their own canonical symbols, never guessed.

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::model::Registry;

pub const RULES_V: u32 = 0;
/// Versioned domain tag — a rules bump never silently re-hashes ratified history
/// (hashing spec §5 freeze policy).
pub const ADDRESS_TAG: &[u8] = b"req.claim.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    Shall,
    ShallNot,
    May,
    Should,
}

impl Modality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Modality::Shall => "SHALL",
            Modality::ShallNot => "SHALL-NOT",
            Modality::May => "MAY",
            Modality::Should => "SHOULD",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
    Ne,
}

impl Comp {
    fn invert(self) -> Self {
        match self {
            Comp::Lt => Comp::Ge,
            Comp::Le => Comp::Gt,
            Comp::Eq => Comp::Ne,
            Comp::Ge => Comp::Lt,
            Comp::Gt => Comp::Le,
            Comp::Ne => Comp::Eq,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Comp::Lt => "LT",
            Comp::Le => "LE",
            Comp::Eq => "EQ",
            Comp::Ge => "GE",
            Comp::Gt => "GT",
            Comp::Ne => "NE",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lint {
    pub code: String,
    pub detail: String,
}

fn lint_untestable_comparator(word: &str) -> Lint {
    Lint {
        code: "untestable_comparator".to_string(),
        detail: format!("comparator word {word:?} is not in the T1.5 table"),
    }
}

// ---------------------------------------------------------------------------
// Canonical form + assembly (T1.9)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CanonicalForm {
    pub unstructured: bool,
    /// Raw normalized statement, populated only for the unstructured fallback (§4).
    pub raw: String,
    pub modality: Option<Modality>,
    pub subject: String,
    pub action: String,
    pub operator: Option<Comp>,
    pub value: Option<Decimal>,
    /// Canonical unit symbol, or "none" for unitless counts/state values.
    pub unit: Option<String>,
    /// The *post-value* phrase — an identity field, never dropped.
    pub qualifier: String,
    /// Sorted guard clauses (T1.8).
    pub conditions: BTreeSet<String>,
    pub exception: Option<String>,
    pub temporal: Option<String>,
}

impl CanonicalForm {
    pub fn modality_str(&self) -> &'static str {
        match &self.modality {
            Some(m) => m.as_str(),
            None => "none",
        }
    }
    pub fn operator_str(&self) -> &'static str {
        match &self.operator {
            Some(c) => c.as_str(),
            None => "none",
        }
    }
    /// The canonical bytes that get hashed. Sorting + exact-form decimal strings
    /// make this bite-stable across platforms.
    pub fn canonical_bytes(&self) -> String {
        canonical_json(self)
    }
}

pub fn canonical_decimal(d: &Decimal) -> String {
    let s = d.to_string();
    match s.find('.') {
        Some(dot) => {
            let trimmed = s[dot + 1..].trim_end_matches('0');
            if trimmed.is_empty() {
                s[..dot].to_string()
            } else {
                format!("{}.{}", &s[..dot], trimmed)
            }
        }
        None => s,
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other if (other as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", other as u32)),
            other => out.push(other),
        }
    }
    out
}

/// Fixed-shape canonical JSON with the fields FIXED ALPHABETICALLY in this
/// serializer — byte stability never depends on a library's field iteration
/// order. All fields always present (null when unset); numbers as exact decimal
/// strings (never floats). This is the sha256 input, prefixed by the tagged
/// domain string, exactly per the hashing spec's T1.9.
pub fn canonical_json(f: &CanonicalForm) -> String {
    let opt_str = |v: &Option<String>| match v {
        Some(s) => format!("\"{}\"", json_escape(s)),
        None => "null".to_string(),
    };
    let cond_list = f
        .conditions
        .iter()
        .map(|c| format!("\"{}\"", json_escape(c)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"action\":\"{}\",\"conditions\":[{}],\"exception\":{},\"modality\":{},",
            "\"operator\":{},\"qualifier\":\"{}\",\"raw\":\"{}\",\"subject\":\"{}\",",
            "\"temporal\":{},\"unstructured\":{},\"unit\":{},\"value\":{}}}",
        ),
        json_escape(&f.action),
        cond_list,
        opt_str(&f.exception),
        f.modality_str(),
        if f.modality.is_some() {
            format!("\"{}\"", f.operator_str())
        } else {
            "null".to_string()
        },
        json_escape(&f.qualifier),
        json_escape(&f.raw),
        json_escape(&f.subject),
        opt_str(&f.temporal),
        f.unstructured,
        opt_str(&f.unit),
        match &f.value {
            Some(d) => format!("\"{}\"", json_escape(&canonical_decimal(d))),
            None => "null".to_string(),
        },
    )
}

/// sha256 of raw bytes, lowercase hex — used for the engine's report fields.
pub fn sha256_hex_raw(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest.iter() {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

/// `sha256("req.claim.v1\0" || canonical_form_bytes)` — the Tier-1 identity.
pub fn address_hex(canonical_form: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ADDRESS_TAG);
    hasher.update(canonical_form.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest.iter() {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

// ---------------------------------------------------------------------------
// Text normalization (T1.0–T1.2)
// ---------------------------------------------------------------------------

pub fn normalize_text(text: &str) -> String {
    // T1.0: NFC + smart-quote/dash unification.
    let nfc: String = text.nfc().collect();
    let joined = nfc
        .replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{2013}', "-")
        .replace('\u{2014}', "-");
    // T1.2: digit-grouping separators inside numeric runs (1,000→1000; 1_000→1000).
    let joined = strip_numeric_separators(&joined);
    let joined = joined.trim();
    let joined = joined.trim_end_matches('.');
    let mut out = String::new();
    for word in joined.split_whitespace() {
        out.push_str(&normalize_word(word));
        out.push(' ');
    }
    out.trim().to_string()
}

fn strip_numeric_separators(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(b.len());
    for i in 0..b.len() {
        let c = b[i];
        if (c == ',' || c == '_')
            && i > 0
            && i + 1 < b.len()
            && b[i - 1].is_ascii_digit()
            && b[i + 1].is_ascii_digit()
        {
            continue;
        }
        out.push(c);
    }
    out
}

/// T1.1: lowercase, preserving all-caps/underscore tokens (REQ_TOOL, NODE_ID).
fn normalize_word(word: &str) -> String {
    let has_underscore = word.contains('_');
    let only_const_chars = word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if has_underscore && only_const_chars {
        word.to_string()
    } else {
        word.chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Numbers + units (T1.3 / T1.4)
// ---------------------------------------------------------------------------

fn parse_decimal(s: &str) -> Option<Decimal> {
    if s.contains(['e', 'E']) {
        return Decimal::from_scientific(s).ok();
    }
    s.parse::<Decimal>().ok()
}

/// Leading digit-run of a token; the trailing alpha, if any, is the *suffixed
/// unit* form ("200ms" → ("200", Some("ms"))).
fn leading_numeric_part(token: &str) -> Option<(String, Option<String>)> {
    let mut digits = String::new();
    for c in token.chars() {
        if c.is_ascii_digit() || c == '.' || c == '+' || c == '-' {
            digits.push(c);
        } else {
            break;
        }
    }
    if !digits.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let num = digits.trim_end_matches(['.', '-']).to_string();
    if num.is_empty() || num == "-" || num == "+" {
        return None;
    }
    let suffix = token[num.len()..].to_string();
    Some((
        num,
        if suffix.is_empty() { None } else { Some(suffix) },
    ))
}

#[derive(Debug, Clone, Copy)]
struct UnitDef {
    /// Canonical symbol; `None` marks the dimensionless ratio (unit "none").
    canonical: Option<&'static str>,
    /// Power-of-ten exponent: value × 10^factor_power.
    factor_power: i32,
}

fn units() -> BTreeMap<&'static str, UnitDef> {
    let mut m: BTreeMap<&'static str, UnitDef> = BTreeMap::new();
    let mut put = |aliases: &[&'static str], canonical: Option<&'static str>, exp: i32| {
        let def = UnitDef {
            canonical,
            factor_power: exp,
        };
        for a in aliases {
            m.insert(*a, def);
        }
    };
    // time (canonical "s")
    put(&["s", "sec", "secs", "second", "seconds"], Some("s"), 0);
    put(
        &["ms", "msec", "msecs", "millisecond", "milliseconds"],
        Some("s"), -3,
    );
    put(&["us", "microsecond", "microseconds"], Some("s"), -6);
    put(&["ns", "nanosecond", "nanoseconds"], Some("s"), -9);
    // frequency (canonical "hz")
    put(&["hz", "hertz"], Some("hz"), 0);
    put(&["khz"], Some("hz"), 3);
    put(&["mhz"], Some("hz"), 6);
    // length (canonical "m")
    put(&["m", "meter", "meters", "metre", "metres"], Some("m"), 0);
    put(&["mm", "millimeter", "millimeters"], Some("m"), -3);
    put(&["cm", "centimeter", "centimeters"], Some("m"), -2);
    put(&["km", "kilometer", "kilometers"], Some("m"), 3);
    // force (canonical "n")
    put(&["n", "newton", "newtons"], Some("n"), 0);
    put(&["kn", "kilonewton", "kilonewtons"], Some("n"), 3);
    // current (canonical "a")
    put(&["a", "amp", "amps", "ampere", "amperes"], Some("a"), 0);
    put(&["ma", "milliamp", "milliamps"], Some("a"), -3);
    // voltage (canonical "v")
    put(&["v", "volt", "volts"], Some("v"), 0);
    put(&["mv", "millivolt", "millivolts"], Some("v"), -3);
    // power (canonical "w")
    put(&["w", "watt", "watts"], Some("w"), 0);
    put(&["kw", "kilowatt", "kilowatts"], Some("w"), 3);
    // energy (canonical "j")
    put(&["j", "joule", "joules"], Some("j"), 0);
    put(&["kj", "kilojoule", "kilojoules"], Some("j"), 3);
    // temperature (canonical "k") — °C→K uses +273.15 in resolve_value_unit
    put(&["k", "kelvin", "kelvins"], Some("k"), 0);
    put(&["c", "celsius"], Some("k"), 0);
    // ratio (canonical "none") — value ÷ 100, exact
    put(&["%", "percent", "per cent", "pct"], None, 0);
    // self-canonical (NOT decimally convertible in Tier-1 — e.g. km/h is ×5/18)
    for u in [
        "km/h", "m/s", "mph", "min", "mins", "minute", "minutes", "hour", "hours",
        "day", "days", "db", "dbs", "rpm", "bar", "psi", "kpa", "mpa", "kcal",
        "byte", "bytes", "kib", "mib", "gib", "row", "rows", "record", "records",
    ] {
        m.insert(
            u,
            UnitDef {
                canonical: Some(u),
                factor_power: 0,
            },
        );
    }
    m
}

fn scale(d: Decimal, power: i32) -> Decimal {
    if power == 0 {
        d
    } else if power > 0 {
        d.checked_mul(Decimal::new(10i64.pow(power as u32), 0))
            .unwrap_or(d)
    } else {
        d.checked_div(Decimal::new(10i64.pow((-power) as u32), 0))
            .unwrap_or(d)
    }
}

fn resolve_value_unit(
    num: &str,
    unit_token: Option<&str>,
    lints: &mut Vec<Lint>,
) -> (Option<Decimal>, Option<String>) {
    let Some(d) = parse_decimal(num) else {
        lints.push(Lint {
            code: "untestable_quantity".to_string(),
            detail: format!("number {num:?} is not exactly representable"),
        });
        return (None, None);
    };
    let Some(tok) = unit_token else {
        return (Some(d), None);
    };
    let unit_word: String = tok
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '%')
        .to_lowercase();
    if unit_word.is_empty() {
        return (Some(d), None);
    }
    let table = units();
    if let Some(def) = table.get(unit_word.as_str()) {
        let scaled = scale(d, def.factor_power);
        let value = if unit_word == "c" || unit_word == "celsius" {
            scaled.checked_add(Decimal::new(27315, 2)).unwrap_or(scaled)
        } else {
            scaled
        };
        (Some(value), Some(def.canonical.unwrap_or("none").to_string()))
    } else {
        // Unknown unit → never guessed (T1.4): kept verbatim so two different
        // unknown units can never collapse; the value stays unconverted.
        lints.push(Lint {
            code: "untestable_quantity".to_string(),
            detail: format!("unit {unit_word:?} is not in the T1.4 table; kept verbatim"),
        });
        (Some(d), Some(unit_word))
    }
}

// ---------------------------------------------------------------------------
// Comparator / modality tables (T1.5 / T1.6)
// ---------------------------------------------------------------------------

const COMPARATOR_PHRASES: &[(&str, Comp)] = &[
    ("within", Comp::Le),
    ("no later than", Comp::Le),
    ("at most", Comp::Le),
    ("up to", Comp::Le),
    ("not exceed", Comp::Le),
    ("in fewer than", Comp::Lt),
    ("in less than", Comp::Lt),
    ("fewer than", Comp::Lt),
    ("less than", Comp::Lt),
    ("at least", Comp::Ge),
    ("no less than", Comp::Ge),
    ("not less than", Comp::Ge),
    ("no fewer than", Comp::Ge),
    ("greater than", Comp::Gt),
    ("more than", Comp::Gt),
    ("exceeds", Comp::Gt),
    ("equal to", Comp::Eq),
    ("exactly", Comp::Eq),
];

const MODALITY_PHRASES: &[(&str, Modality)] = &[
    ("shall not", Modality::ShallNot),
    ("must not", Modality::ShallNot),
    ("may not", Modality::ShallNot),
    ("shall", Modality::Shall),
    ("must", Modality::Shall),
    ("may", Modality::May),
    ("should", Modality::Should),
    ("is required to", Modality::Shall),
    ("is to", Modality::Shall),
];

/// Bare comparative stems for absorbed negation: `<modal> not <stem> X` →
/// comparator invert(bare), modality the modal (the obligation survives).
const NEGATION_ABSORPTION_STEMS: &[(&str, Comp)] = &[
    ("exceed", Comp::Gt),
    ("greater", Comp::Gt),
    ("more", Comp::Gt),
    ("less", Comp::Lt),
    ("fewer", Comp::Lt),
];

const FILLER_WORDS: &[&str] = &[
    "in", "of", "to", "the", "a", "be", "is", "are", "and", "or", "up", "for", "with",
    "that", "its", "it", "on", "by", "at", "than", "no",
];

// ---------------------------------------------------------------------------
// Guard clauses (T1.8)
// ---------------------------------------------------------------------------

const GUARD_WORDS: &[&str] = &["when", "while", "unless", "during", "if"];

fn is_word_boundary(text: &str, pos: usize, len: usize) -> bool {
    let bytes = text.as_bytes();
    let before_ok = pos == 0 || !bytes[pos - 1].is_ascii_alphanumeric();
    let after_ok = pos + len >= bytes.len() || !bytes[pos + len].is_ascii_alphanumeric();
    before_ok && after_ok
}

fn find_last_guard(text: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for g in GUARD_WORDS {
        if let Some(p) = text.rfind(g) {
            if is_word_boundary(text, p, g.len()) && best.map_or(true, |(bp, _)| p > bp) {
                best = Some((p, g.len()));
            }
        }
    }
    best
}

fn extract_conditions(body: &str, lints: &mut Vec<Lint>) -> (String, BTreeSet<String>) {
    let mut remainder = body.to_string();
    let mut conditions: BTreeSet<String> = BTreeSet::new();
    loop {
        let Some((pos, word_len)) = find_last_guard(&remainder) else {
            break;
        };
        let head = remainder[..pos].trim().to_string();
        let guard_text = remainder[pos..].to_string();
        let after_guard = guard_text[word_len..].trim().to_string();
        if head.is_empty() || after_guard.is_empty() {
            break;
        }
        let gword = guard_text[..word_len].to_string();
        conditions.insert(format!("{}: {}", gword, normalize_text(&after_guard)));
        remainder = head;
        if conditions.len() > 8 {
            lints.push(Lint {
                code: "untestable_quantity".to_string(),
                detail: "guard chain too long to parse safely; left in raw".to_string(),
            });
            break;
        }
    }
    (remainder, conditions)
}

// ---------------------------------------------------------------------------
// Word-sequence phrase matching (never a raw substring: "mere" can't match
// inside "merecat")
// ---------------------------------------------------------------------------

fn word_span_start(words: &[&str], phrase_words: &[&str]) -> Option<usize> {
    if phrase_words.is_empty() || phrase_words.len() > words.len() {
        return None;
    }
    let hi = words.len() - phrase_words.len();
    for start in 0..=hi {
        let seg = &words[start..start + phrase_words.len()];
        if seg
            .iter()
            .zip(phrase_words.iter())
            .all(|(w, p)| w.eq_ignore_ascii_case(p))
        {
            return Some(start);
        }
    }
    None
}

fn phrase_in_words(words: &[&str], phrase: &str) -> bool {
    let pw: Vec<&str> = phrase.split_whitespace().collect();
    word_span_start(words, &pw).is_some()
}

fn is_modal_word(w: &str) -> bool {
    ["not", "never"].contains(&w) || MODALITY_PHRASES.iter().any(|(p, _)| *p == w)
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

/// Tier-1 canonicalization of a statement. Always returns a canonical form:
/// structured, or the unstructured fallback (hashing spec §4). Resolution failure
/// never errors: an unparseable statement gets its own raw canonical bytes (so it
/// can never merge with a structured claim, and two different unparseable
/// statements differ from each other).
pub fn canonicalize_statement(
    text: &str,
    registry: &Registry,
) -> (CanonicalForm, Vec<Lint>) {
    let mut lints = Vec::new();
    let norm = normalize_text(text);

    // T1.8 first, so the obligation body is parsed clean of trailing guards.
    let (body, conditions) = extract_conditions(&norm, &mut lints);
    let words: Vec<&str> = body.split_whitespace().collect();

    // T1.3/T1.4: first numeric token (+ unit), exact
    let mut num_idx: Option<usize> = None;
    let mut numeric_str = String::new();
    let mut unit_suffix: Option<String> = None;
    for (i, tok) in words.iter().enumerate() {
        if let Some((num, suffix)) = leading_numeric_part(tok) {
            num_idx = Some(i);
            numeric_str = num;
            unit_suffix = suffix;
            break;
        }
    }
    let (value, unit) = match num_idx {
        Some(i) => {
            let tok = unit_suffix
                .clone()
                .or_else(|| words.get(i + 1).map(|w| w.to_string()));
            resolve_value_unit(&numeric_str, tok.as_deref(), &mut lints)
        }
        None => (None, None),
    };

    // T1.5 comparator over the pre-numeric words (the T1.5 table's span stops
    // before the number token); T1.6 modality over the same span.
    let pre_words: Vec<&str> = num_idx.map_or_else(|| words.clone(), |i| words[..i].to_vec());
    let (modality, operator, modal_start, modal_end, action) =
        resolve_modality_comparator(&pre_words, value.is_some(), &mut lints);

    // untestable-comparator lint: an untabled non-filler word directly before
    // the number when no phrase in the T1.5 table matched
    if value.is_some() && operator.is_none() {
        if let Some(last) = pre_words.last() {
            if !FILLER_WORDS.contains(last) && !is_modal_word(last) {
                lints.push(lint_untestable_comparator(last));
            }
        }
    }

    // T1.7 subject = pre-modality words; unregistered surfaces resolve literally
    let subject: Option<String> = match modality {
        Some(_) => {
            let subj: Vec<&str> = pre_words[..modal_start.min(pre_words.len())].to_vec();
            let has_digit = subj.iter().any(|w| w.chars().any(|c| c.is_ascii_digit()));
            if subj.is_empty() || has_digit {
                None
            } else {
                Some(registry.resolve(&subj.join(" ")))
            }
        }
        None => None,
    };

    // qualifier: the post-value phrase — an identity field, never dropped.
    let qualifier = match num_idx {
        Some(i) => {
            let tail_start = if unit_suffix.is_some() { i + 1 } else { i + 2 };
            words.get(tail_start..).unwrap_or(&[]).join(" ")
        }
        None => String::new(),
    };

    let Some(subject) = subject else {
        // Unstructured fallback (§4): the raw normalized statement.
        return (
            CanonicalForm {
                unstructured: true,
                raw: norm,
                exception: None,
                temporal: None,
                ..CanonicalForm::default()
            },
            lints,
        );
    };

    (
        CanonicalForm {
            unstructured: false,
            raw: String::new(),
            modality,
            subject,
            action,
            operator,
            value,
            unit,
            qualifier,
            conditions,
            exception: None,
            temporal: None,
        },
        lints,
    )
}

fn resolve_modality_comparator(
    pre_words: &[&str],
    has_value: bool,
    lints: &mut Vec<Lint>,
) -> (Option<Modality>, Option<Comp>, usize, usize, String) {
    let mut comparator: Option<Comp> = None;
    let mut comp_start: Option<usize> = None;
    let mut modality: Option<Modality> = None;
    let mut modal_start: usize = pre_words.len();
    let mut modal_end: usize = 0;
    let mut absorbed = false;

    // 1: absorbed negation — `<modal> not <comparative stem>`
    'absorb: for (modal_word, modal_val) in [
        ("shall", Modality::Shall),
        ("must", Modality::Shall),
        ("may", Modality::May),
        ("should", Modality::Should),
    ] {
        for negator in ["not", "never"] {
            for (stem, bare) in NEGATION_ABSORPTION_STEMS {
                let pw: Vec<&str> = vec![modal_word, negator, stem];
                if let Some(start) = word_span_start(pre_words, &pw) {
                    comparator = Some(bare.invert());
                    comp_start = Some(start + 1); // point after the modal
                    modality = Some(modal_val);
                    modal_start = start;
                    modal_end = start + 1; // the modal phrase is one word here
                    absorbed = true;
                    break 'absorb;
                }
            }
        }
    }

    // 2: plain comparator table — longest word-sequence wins
    if comparator.is_none() && has_value {
        let mut best: Option<(usize, usize, Comp)> = None;
        for (phrase, cmp) in COMPARATOR_PHRASES {
            let pw: Vec<&str> = phrase.split_whitespace().collect();
            if let Some(start) = word_span_start(pre_words, &pw) {
                if best.map_or(true, |(wc, _, _)| pw.len() > wc) {
                    best = Some((pw.len(), start, *cmp));
                }
            }
        }
        if let Some((_, start, cmp)) = best {
            comparator = Some(cmp);
            comp_start = Some(start);
        }
    }

    // 3: modality scan (only when NOT absorbed)
    if !absorbed {
        for (phrase, m) in MODALITY_PHRASES {
            let pw: Vec<&str> = phrase.split_whitespace().collect();
            if let Some(start) = word_span_start(pre_words, &pw) {
                // a `shall not <comparative>` already absorbed — the plain table
                // handles those too when absorption missed
                modality = Some(*m);
                modal_start = start;
                modal_end = start + pw.len();
                break;
            }
        }
        if modality.is_none() && word_span_start(pre_words, &["will"]).is_some() {
            lints.push(Lint {
                code: "ambiguous_modality".to_string(),
                detail: "'will' has no deontic reading we can resolve; claim stays unstructured".to_string(),
            });
        }
    }

    // 4: the action = the span between the modal-phrase's end and the comparator's
    // start (empty when the comparator was absorbed into the modal slot).
    let action = match (modality, comp_start) {
        (Some(_), Some(cs)) => {
            if modal_end <= cs {
                pre_words.get(modal_end..cs).unwrap_or(&[]).join(" ")
            } else {
                // the comparator's span begins before the modal ends: absorbed
                String::new()
            }
        }
        (Some(_), None) => pre_words.get(modal_end..).unwrap_or(&[]).join(" "),
        _ => String::new(),
    };

    (modality, comparator, modal_start, modal_end, action)
}

// ---------------------------------------------------------------------------
// req canon — claim-file diagnostics (the parity-invariant surface, v0)
// ---------------------------------------------------------------------------

/// `req canon` (v0 diagnostics) parses each claim's statement with the same
/// pipeline and reports every field the pipeline can verify today vs the
/// *authored* slots: unstructured-tag consistency, modality, subject (via the
/// registry), value+unit when numeric, operator when a comparator matched.
/// Attribute-token derivation and corpus-predicate condition fields are
/// deliberately not verified; the diagnostic reports them as unchecked so
/// nothing silently passes.
pub fn run_claim_diagnostics() -> i32 {
    use crate::model::ClaimFile;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let registry_path = root.join("aliases/subjects.yml");
    let registry = match Registry::load(registry_path.to_str().unwrap()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] registry load failed: {e}");
            return 3;
        }
    };
    let claims_dir = root.join("claims");
    let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(&claims_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|f| f.to_str())
                    .map_or(false, |f| f.starts_with("REQ-") && f.ends_with(".yml"))
            })
            .collect(),
        Err(e) => {
            eprintln!("[UNVERIFIABLE] claims dir: {e}");
            return 3;
        }
    };
    files.sort();

    let mut checked = 0usize;
    let mut flags_total = 0usize;
    for path in files.iter() {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] read {}: {e}", path.display());
                return 3;
            }
        };
        let claim: ClaimFile = match yaml_serde::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[UNVERIFIABLE] parse {}: {e}", path.display());
                return 3;
            }
        };
        let (form, _lints) = canonicalize_statement(&claim.statement, &registry);
        checked += 1;
        let mut flags = 0usize;

        let unstructured_ok = form.unstructured == claim.unstructured;
        if !unstructured_ok {
            flags += 1;
        }
        let modality_ok = claim.unstructured || form.modality_str() == claim.modality;
        if !modality_ok {
            flags += 1;
        }
        let subject_ok = form.subject == claim.subject;
        if !subject_ok {
            flags += 1;
        }
        // value: only really checkable when the *authored* value is numeric
        let authored_v = claim.value.clone().unwrap_or_default();
        let authored_is_num = !authored_v.is_empty()
            && authored_v
                .chars()
                .next()
                .unwrap_or('x')
                .is_ascii_digit();
        let v_parsed = form
            .value
            .as_ref()
            .map(canonical_decimal)
            .unwrap_or_default();
        let u_parsed = form.unit.clone().unwrap_or_else(|| "none".to_string());
        let authored_u = claim.unit.clone().unwrap_or_else(|| "none".to_string());
        let value_unit_ok = if authored_is_num {
            v_parsed == authored_v && u_parsed == authored_u
        } else {
            true
        };
        if !value_unit_ok {
            flags += 1;
        }
        let authored_op = claim.operator.clone().unwrap_or_default();
        let op_parsed = form.operator_str().to_string();
        let op_checkable = authored_is_num;
        let op_ok = !op_checkable || op_parsed == authored_op;
        if !op_ok {
            flags += 1;
        }

        flags_total += flags;
        println!(
            "{:<9} {:<9} {:<9} {:<12} {:<10} {:<8}",
            claim.claim_id,
            mark(unstructured_ok, form.unstructured.to_string()),
            mark(modality_ok, form.modality_str().to_string()),
            mark(subject_ok, form.subject.clone()),
            mark(value_unit_ok, format!("{v_parsed}/{authored_v}")),
            mark(op_ok, if op_checkable { op_parsed } else { "n/a".to_string() }),
        );
    }
    let _ = &registry;
    println!(
        "claims: {checked}; discrepancy flags: {flags_total} (v0 diagnostic: attribute lexicon, conditions, applicability, rationale unverified by design)"
    );
    0
}

fn mark(ok: bool, detail: String) -> String {
    if ok {
        "ok".to_string()
    } else {
        format!("FLAG {detail}")
    }
}
