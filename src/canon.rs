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
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::model::Registry;

/// Tier-1 normalization-rules version. 0 -> 1 added Markdown emphasis stripping
/// (T1.2b); 1 -> 2 added prohibition via "never", multi-obligation detection,
/// qualitative-predicate classification, and a precise comparator lint; 2 -> 3 fixed
/// the predicate span and made the canonical bytes valid JSON; 3 -> 4 stopped
/// resolving "may not"/"may never" (ambiguous negation scope — a real false merge on
/// the widened external corpus); 4 -> 5 put authored applicability into identity
/// (ADR 0003 said it belonged there; the form had no field for it at all). Addresses
/// computed under older rules are re-canonicalization candidates, never silently
/// rewritten (hashing spec §5).
pub const RULES_V: u32 = 5;
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
    /// Authored variant selectors (ADR 0003). **An input, not a derivation**: the
    /// statement does not carry formal selector syntax yet, so the canonical layer
    /// takes these from the claim and they are part of the address. Two claims that
    /// differ only in applicability are different claims — which the form could not
    /// say before rules v5.
    pub applicability: BTreeSet<String>,
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
    let app_list = f
        .applicability
        .iter()
        .map(|a| format!("\"{}\"", json_escape(a)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"action\":\"{}\",\"applicability\":[{}],\"conditions\":[{}],\"exception\":{},",
            "\"modality\":\"{}\",",
            "\"operator\":{},\"qualifier\":\"{}\",\"raw\":\"{}\",\"subject\":\"{}\",",
            "\"temporal\":{},\"unstructured\":{},\"unit\":{},\"value\":{}}}",
        ),
        json_escape(&f.action),
        app_list,
        cond_list,
        opt_str(&f.exception),
        // Quoted by the format string around it, because these are JSON bytes (T1.9)
        // and a consumer parses them: an unquoted enum made the canonical form invalid
        // JSON, which is undetectable from the hash and from any diff.
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
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{2013}', '\u{2014}'], "-");
    // T1.2b: Markdown emphasis delimiters are typographic, not semantic — real
    // sources wrap the modal ("Doorstop **shall** support the storage of …"), and
    // without this the modality is missed and the claim falls to the unstructured
    // fallback. Only delimiter pairs are stripped: a lone `*` may be a wildcard,
    // and `_` is meaningful inside identifiers (T1.1).
    let joined = joined.replace("**", "").replace('`', "");
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

/// T1.1: lowercase, preserving all-caps/underscore tokens (REQ_TOOL, NODE_ID);
/// possessive suffixes are stripped grammatically ("tool's" → "tool") — they carry
/// no meaning and their presence was a *subject-boundary* diagnosis bug (the v0
/// subject scan read "the req tool's semantic layer" as the whole subject).
fn normalize_word(word: &str) -> String {
    let word = word.strip_suffix("'s").unwrap_or(word);
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

/// Small curated word-number table (T1.3 lexical form) — "exactly zero on the
/// parity corpus" is a bounded claim, not prose. Conservative on purpose: only
/// 0..=10, and only when the word is a *solo* token (a "one hundred" phrase is
/// out of v0 scope and stays unresolved).
fn word_number(word: &str) -> Option<Decimal> {
    let n = match word {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        _ => return None,
    };
    Some(Decimal::new(n, 0))
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
        if suffix.is_empty() {
            None
        } else {
            Some(suffix)
        },
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
        Some("s"),
        -3,
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
    // ratio (canonical "none") — percent is ×10⁻² exact (÷100)
    put(&["%", "percent", "per cent", "pct"], None, -2);
    // self-canonical (NOT decimally convertible in Tier-1 — e.g. km/h is ×5/18)
    for u in [
        "km/h", "m/s", "mph", "min", "mins", "minute", "minutes", "hour", "hours", "day", "days",
        "db", "dbs", "rpm", "bar", "psi", "kpa", "mpa", "kcal", "byte", "bytes", "kib", "mib",
        "gib", "row", "rows", "record", "records",
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
) -> (Option<Decimal>, Option<String>, usize) {
    // The third return is the count of *unit tokens consumed* after the digit:
    // 1 for a known unit word or a digit-suffixed unit, 0 for a word that is not a
    // unit at all — that word stays in the qualifier, which is an identity field
    // ("...of at most 0.3 on the parity corpus" keeps "on the parity corpus")
    // and two different qualifier phrases never collapse.
    let Some(d) = parse_decimal(num) else {
        lints.push(Lint {
            code: "untestable_quantity".to_string(),
            detail: format!("number {num:?} is not exactly representable"),
        });
        return (None, None, 0);
    };
    let Some(tok) = unit_token else {
        return (Some(d), None, 0);
    };
    let unit_word: String = tok
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '%')
        .to_lowercase();
    if unit_word.is_empty() {
        return (Some(d), None, 0);
    }
    let table = units();
    if let Some(def) = table.get(unit_word.as_str()) {
        let scaled = scale(d, def.factor_power);
        let value = if unit_word == "c" || unit_word == "celsius" {
            scaled.checked_add(Decimal::new(27315, 2)).unwrap_or(scaled)
        } else {
            scaled
        };
        (
            Some(value),
            Some(def.canonical.unwrap_or("none").to_string()),
            1,
        )
    } else {
        // Unknown *unit-shaped* tokens (SI-ish symbols, slash forms) keep
        // verbatim, never guessed (T1.4). Prose words after the number are NOT
        // units — they move into the qualifier instead of becoming junk units.
        let unit_shaped = unit_word.contains('/')
            || unit_word.contains('%')
            || (unit_word.len() <= 4 && !is_prose_word(&unit_word));
        if unit_shaped {
            lints.push(Lint {
                code: "untestable_quantity".to_string(),
                detail: format!("unit {unit_word:?} is not in the T1.4 table; kept verbatim"),
            });
            (Some(d), Some(unit_word), 1)
        } else {
            (Some(d), None, 0)
        }
    }
}

/// Prose words after a number ("on", "each", "records"…) that are plainly not
/// units. Units are the curated table's job; this keeps unknown short words
/// from becoming junk units.
fn is_prose_word(w: &str) -> bool {
    matches!(
        w,
        "on" | "in"
            | "of"
            | "at"
            | "the"
            | "a"
            | "an"
            | "each"
            | "per"
            | "for"
            | "to"
            | "with"
            | "and"
            | "over"
            | "under"
            | "about"
            | "across"
    )
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
    // "may not" / "may never" are deliberately ABSENT: the scope of the negation is
    // ambiguous in English (prohibition vs permission to refrain), T1.6 never
    // sanctioned resolving them, and resolving them produced a real false merge on
    // the widened external corpus. They are linted and left unstructured below.
    // "never" negates too: "shall never X" is a prohibition, not an obligation to
    // X. Reading it as SHALL would put the wrong modality into the audit artifact.
    // (Absorbed negation runs first, so "shall never exceed 200" still resolves as
    // SHALL with comparator LE — the comparison's negation, not the modality's.)
    ("shall never", Modality::ShallNot),
    ("must never", Modality::ShallNot),
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

/// Words that *could* be a comparator attempt when the T1.5 table has no phrase for
/// them ("under 200 ms"). A curated list, not a catch-all: a name before a number
/// ("IEC 61508") is not a comparator attempt, and linting it was noise on real
/// external prose. Precision costs nothing: an unresolved comparator is never
/// guessed, so the claim cannot collapse either way.
const AMBIGUOUS_COMPARATOR_WORDS: &[&str] = &[
    "under",
    "over",
    "about",
    "around",
    "approximately",
    "approx",
    "roughly",
    "nearly",
    "below",
    "above",
    "exceeding",
    "near",
    "circa",
    "upward",
    "downward",
];

fn looks_like_comparator(word: &str) -> bool {
    AMBIGUOUS_COMPARATOR_WORDS.contains(&word)
}

/// Predicates that cannot be canonicalized into an operator and a value: a
/// qualitative word is not measurable. Curated and deliberately narrow, and the
/// failure directions are asymmetric in our favour — a false hit costs a split
/// (noise), a miss costs nothing worse than it costs today.
const QUALITATIVE_WORDS: &[&str] = &[
    "friction-free",
    "seamless",
    "intuitive",
    "user-friendly",
    "easy",
    "robust",
    "efficient",
    "flexible",
    "simple",
    "smooth",
    "quick",
    "fast",
    "elegant",
    "powerful",
    "modern",
    "nice",
    "clean",
    // Inflections of the list above: a qualitative claim does not become measurable by
    // adding "-ly".
    "easily",
    "quickly",
    "simply",
    "smoothly",
    "robustly",
    "efficiently",
    "flexibly",
    "seamlessly",
    "intuitively",
    "cleanly",
];

fn qualitative_predicate(action: &str, qualifier: &str) -> Option<String> {
    let text = format!("{action} {qualifier}").to_lowercase();
    for token in text.split(|c: char| !c.is_alphanumeric() && c != '-') {
        // Hyphenated compounds are one token ("easy-to-use") and the qualitative word is
        // the head, so check the token and its prefix before the first hyphen. A false
        // hit costs a split (noise); a miss costs nothing worse than today.
        let head = token.split('-').next().unwrap_or(token);
        for candidate in [token, head] {
            if QUALITATIVE_WORDS.contains(&candidate) {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

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
        if let Some(p) = text.rfind(g)
            && is_word_boundary(text, p, g.len())
            && best.is_none_or(|(bp, _)| p > bp)
        {
            best = Some((p, g.len()));
        }
    }
    best
}

fn extract_conditions(body: &str, lints: &mut Vec<Lint>) -> (String, BTreeSet<String>) {
    let mut remainder = body.to_string();
    let mut conditions: BTreeSet<String> = BTreeSet::new();
    while let Some((pos, word_len)) = find_last_guard(&remainder) {
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

/// T1.6 modality over the *whole* statement's words; must run BEFORE the number
/// scan so the value's digit cannot be confused with a name-digit in the
/// subject. Returns (modality, modal-phrase start word, end word).
///
/// The **earliest** match by position wins; at the same start index the longest
/// phrase wins ("shall never" over "shall"). Phrase-table order must not decide
/// position: it made a later "shall never" shadow an earlier "shall", which hid the
/// first obligation of a two-obligation statement.
fn scan_modality(words: &[&str], lints: &mut Vec<Lint>) -> (Option<Modality>, usize, usize) {
    let mut best: Option<(usize, usize, Modality)> = None; // (start, len, modality)
    for (phrase, m) in MODALITY_PHRASES {
        let pw: Vec<&str> = phrase.split_whitespace().collect();
        if let Some(start) = word_span_start(words, &pw) {
            let better = match best {
                None => true,
                Some((best_start, best_len, _)) => {
                    start < best_start || (start == best_start && pw.len() > best_len)
                }
            };
            if better {
                best = Some((start, pw.len(), *m));
            }
        }
    }
    if let Some((start, len, modality)) = best {
        return (Some(modality), start, start + len);
    }
    if word_span_start(words, &["will"]).is_some() {
        lints.push(Lint {
            code: "ambiguous_modality".to_string(),
            detail: "'will' has no deontic reading we can resolve; claim stays unstructured"
                .to_string(),
        });
    }
    (None, 0, 0)
}

/// True when the pre-modal span looks like a *value-led fragment*: a digit token
/// immediately followed by a known unit word ("12 kN shall be applied…"). That is
/// the subject-fail case (a value occupying the subject slot, e.g. a passive).
/// Digit-bearing *names* ("a tier 2 provider's proposal output…") return false —
/// the digit is part of a name, not a value.
fn value_led_subject(subj: &[&str]) -> bool {
    let table = units();
    for (i, w) in subj.iter().enumerate() {
        // number and unit in one token: "12kn", "200ms"
        if let Some((_, Some(suffix))) = leading_numeric_part(w) {
            let unit_word: String = suffix
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if table.contains_key(unit_word.as_str()) {
                return true;
            }
        }
        if let Some((_, None)) = leading_numeric_part(w)
            && let Some(next) = subj.get(i + 1)
        {
            let next_l = next
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '/')
                .to_lowercase();
            if table.contains_key(next_l.as_str()) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

/// Tier-1 canonicalization of a statement. Always returns a canonical form:
/// structured, or the unstructured fallback (hashing spec §4). Resolution failure
/// never errors: an unparseable statement gets its own raw canonical bytes (so it
/// can never merge with a structured claim, and two different unparseable
/// statements differ from each other).
pub fn canonicalize_statement(text: &str, registry: &Registry) -> (CanonicalForm, Vec<Lint>) {
    let mut lints = Vec::new();
    let norm = normalize_text(text);

    // T1.8 first, so the obligation body is parsed clean of trailing guards.
    let (body, conditions) = extract_conditions(&norm, &mut lints);
    let words: Vec<&str> = body.split_whitespace().collect();

    // ORDER MATTERS (fix: "a tier 2 provider's ... 2 percent" took the name-digit
    // as the value): the modal phrase is resolved FIRST over all body words; the
    // numeric value is then searched only in the post-modal region.
    let (mut modality, modal_start, modal_end) = scan_modality(&words, &mut lints);

    // Absorbed negation: `<modal> not <comparative stem>` — the modal stays
    // non-negated and the comparison's own negation is absorbed into the
    // comparator; the negated-modal phrases in the table are for claims with no
    // comparative, so this ordered check runs before the table's own entries.
    let mut comparator: Option<Comp> = None;
    let mut comp_start_abs: Option<usize> = None;
    // Where the absorbed phrase (`<modal> <negator> <stem>`) ends, so the predicate
    // span starts after it rather than after the modal word alone.
    let mut absorb_end: Option<usize> = None;
    'absorb: for (modal_word, modal_val) in [
        ("shall", Modality::Shall),
        ("must", Modality::Shall),
        ("may", Modality::May),
        ("should", Modality::Should),
    ] {
        for negator in ["not", "never"] {
            for (stem, bare) in NEGATION_ABSORPTION_STEMS {
                let pw: Vec<&str> = vec![modal_word, negator, stem];
                if let Some(start) = word_span_start(&words, &pw)
                    && start == modal_start
                {
                    comparator = Some(bare.invert());
                    comp_start_abs = Some(start + 1); // right after the modal
                    absorb_end = Some(start + 3); // past the whole absorbed phrase
                    modality = Some(modal_val);
                    break 'absorb;
                }
            }
        }
    }

    // "may not X" / "may never X": the *scope* of the negation is not a Tier-1
    // question — "is not permitted to X" and "is permitted to not X" are different
    // obligations written with the same words. T1.6 never listed them, and resolving
    // them produced a real false merge on the widened external corpus (a shall->may
    // mutation collapsed). Ambiguous → linted, unstructured, never guessed. The
    // absorbed form ("may not exceed 200") is unaffected: there the negation belongs
    // to the comparison, which is exactly what absorption records.
    let ambiguous_may_not = absorb_end.is_none()
        && [["may", "not"], ["may", "never"]]
            .iter()
            .any(|p| word_span_start(&words, p).is_some());
    if ambiguous_may_not {
        lints.push(Lint {
            code: "ambiguous_modality".to_string(),
            detail: "'may not'/'may never' is ambiguous (prohibition vs permission to \
                     refrain); claim stays unstructured"
                .to_string(),
        });
    }

    // ORDER: comparator first (it anchors the value), then the value, then the
    // action. The absorbed-negation case (a comparative directly after the modal)
    // has already set comparator+anchor above; the plain table runs next.
    let post_modal: Vec<&str> = words[modal_end.min(words.len())..].to_vec();

    let mut comp_len: usize = 0;
    if comparator.is_none() {
        let mut best: Option<(usize, usize, Comp)> = None; // (phrase words, local start, comp)
        for (phrase, cmp) in COMPARATOR_PHRASES {
            let pw: Vec<&str> = phrase.split_whitespace().collect();
            if let Some(local_start) = word_span_start(&post_modal, &pw)
                && best.is_none_or(|(wc, _, _)| pw.len() > wc)
            {
                best = Some((pw.len(), local_start, *cmp));
            }
        }
        if let Some((pw_len, local_start, cmp)) = best {
            comparator = Some(cmp);
            comp_start_abs = Some(modal_end + local_start);
            comp_len = pw_len;
        }
    }

    // T1.3/T1.4: the first numeric token AFTER the comparator's phrase — digit-run,
    // word-number ("zero"), or a digit-suffixed unit ("200ms") — resolved exactly.
    // Without the anchor, the scan falls back to the region's first number, which
    // is exactly where a name-digit used to be taken ("tier 2 provider").
    let scan_from = match comp_start_abs {
        Some(abs) if abs >= modal_end => abs - modal_end + comp_len,
        Some(_) => comp_len,
        None => 0,
    };
    let mut num_local: Option<usize> = None;
    let mut numeric: Option<(String, Option<Decimal>)> = None;
    let mut unit_suffix: Option<String> = None;
    for (i, tok) in post_modal.iter().enumerate().skip(scan_from) {
        if let Some((num, suffix)) = leading_numeric_part(tok) {
            num_local = Some(i);
            numeric = Some((num, None));
            unit_suffix = suffix;
            break;
        }
        if let Some(d) = word_number(tok) {
            num_local = Some(i);
            numeric = Some((String::new(), Some(d)));
            break;
        }
    }
    let num_idx = num_local.map(|i| i + modal_end);
    let (value, unit, _unit_consumed) = match num_local {
        Some(i) => {
            let tok = unit_suffix
                .clone()
                .or_else(|| post_modal.get(i + 1).map(|w| w.to_string()));
            match &numeric {
                Some((_digits, Some(word_val))) => (Some(*word_val), None, 0),
                Some((digits, None)) => resolve_value_unit(digits, tok.as_deref(), &mut lints),
                None => (None, None, 0),
            }
        }
        None => (None, None, 0),
    };

    // T1.5 comparator (plain table) over the region between the modal phrase and
    // the value token — longest word-sequence wins.
    let comp_region: Vec<&str> = match num_idx {
        Some(n) if n > modal_end => words[modal_end..n].to_vec(),
        None => words[modal_end..].to_vec(),
        _ => Vec::new(),
    };

    // untestable-comparator lint: fire only for a word that could plausibly *be* a
    // comparator (see AMBIGUOUS_COMPARATOR_WORDS).
    if value.is_some()
        && comparator.is_none()
        && let Some(last) = comp_region.last()
        && looks_like_comparator(last)
    {
        lints.push(lint_untestable_comparator(last));
    }

    // action: the predicate span — from the end of the modal phrase (or of the whole
    // absorbed `<modal> <negator> <stem>` phrase) to the comparator, else to the value
    // token, else to the end of the statement.
    //
    // This span is *identity*. Dropping it when no comparator exists made two
    // unbounded obligations with the same subject and modality collapse — 58 false
    // merges on the external corpus's predicate-swap pairs, the one failure this
    // design exists to prevent.
    let action_start = absorb_end.unwrap_or(modal_end).max(modal_end);
    let action_end = match comp_start_abs {
        Some(cs) if cs > action_start => cs,
        _ => num_idx.unwrap_or(words.len()),
    };
    let action = words
        .get(action_start..action_end.min(words.len()))
        .unwrap_or(&[])
        .join(" ");
    let operator = comparator;

    // A second obligation in one statement is not one claim. claim-format's authoring
    // rule says "shall … and shall …" is two claims, split before filing; hashing the
    // first obligation and hiding the rest in the tail is exactly what split a
    // meaning-preserving external pair (StrictDoc SDOC-SSS-49).
    let tail_after_modal = &words[modal_end.min(words.len())..];
    let second_obligation = MODALITY_PHRASES.iter().any(|(phrase, _)| {
        let pw: Vec<&str> = phrase.split_whitespace().collect();
        word_span_start(tail_after_modal, &pw).is_some()
    });
    if second_obligation {
        lints.push(Lint {
            code: "multiple_obligations".to_string(),
            detail: "a second obligation follows; claim-format requires one obligation per claim"
                .to_string(),
        });
    }

    // T1.7 subject, v1: the *longest registered alias* covering a prefix of the
    // pre-modal words wins; the leftover pre-modal words belong to the action
    // phrase, not the subject ("the req tool's semantic layer" → REQ_TOOL +
    // action-prefix "semantic layer"). Digit-rejection is narrowed to its real
    // case: a value-led fragment (digit followed by a known unit, e.g. fatal for
    // "12 kN shall be applied...") — while digit-bearing *names* ("a tier 2
    // provider's…") now parse as subjects. No registered-hit span and no unit
    // rejection ever silently widens what counts as the same subject.
    let subject: Option<(String, usize)> = match modality {
        Some(_) => {
            let subj: Vec<&str> = words[..modal_start.min(words.len())].to_vec();
            if subj.is_empty() || value_led_subject(&subj) {
                None
            } else {
                let max_span = subj.len().min(8);
                Some(
                    registry
                        .resolve_longest_prefix(&subj, max_span)
                        .unwrap_or_else(|| (registry.resolve(&subj.join(" ")), subj.len())),
                )
            }
        }
        None => None,
    };

    // qualifier: the post-value phrase — an identity field, never dropped. The
    // tail starts after the digit token plus whatever unit token was consumed.
    let qualifier = match num_idx {
        Some(i) => {
            let tail_start = if unit_suffix.is_some() {
                i + 1
            } else {
                i + 1 + _unit_consumed
            };
            words.get(tail_start..).unwrap_or(&[]).join(" ")
        }
        None => String::new(),
    };

    // Testability: a qualitative predicate is not measurable, so it cannot be
    // canonicalized into an operator and a value. It belongs in the unstructured
    // fallback with a lint — this *derives* REQ-0007's authored `unstructured: true`
    // rather than merely trusting it.
    let untestable_qualifier = qualitative_predicate(&action, &qualifier);
    if let Some(word) = &untestable_qualifier {
        lints.push(Lint {
            code: "untestable_qualifier".to_string(),
            detail: format!(
                "{word:?} is not measurable; state a bound or leave the claim unstructured"
            ),
        });
    }
    let unparseable = second_obligation || untestable_qualifier.is_some() || ambiguous_may_not;

    let Some((subject, _span)) = subject.filter(|_| !unparseable) else {
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
            // The parser derives no selector: applicability is an *input* the claim
            // supplies, injected by `evaluate_claim` before the form is hashed.
            applicability: BTreeSet::new(),
            exception: None,
            temporal: None,
        },
        lints,
    )
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
/// One claim's parity-invariant verdict: the parse, and whether each *authored*
/// slot agrees with it. Shared by `req canon` (which reports the divergence) and
/// `req canon --fill` (which refuses to derive for a divergent claim) — the two must
/// never drift apart.
pub struct ClaimVerdict {
    pub form: CanonicalForm,
    pub unstructured_ok: bool,
    pub modality_ok: bool,
    pub subject_ok: bool,
    pub value_unit_ok: bool,
    pub op_ok: bool,
    /// Parsed conditions vs authored conditions: a claim that gains one is a different
    /// claim (T1.8), so disagreement here is a divergence and not a detail.
    pub conditions_ok: bool,
    pub conditions_detail: String,
    /// Authored slots the canonical layer cannot represent (quantifier, exception,
    /// temporal). A gap in the *rules*, reported and refused — never silently ignored.
    pub slots_ok: bool,
    pub slots_detail: String,
    pub v_parsed: String,
    pub authored_v: String,
    pub op_parsed: String,
    pub op_checkable: bool,
}

impl ClaimVerdict {
    pub fn flags(&self) -> usize {
        [
            self.unstructured_ok,
            self.modality_ok,
            self.subject_ok,
            self.value_unit_ok,
            self.op_ok,
            self.conditions_ok,
            self.slots_ok,
        ]
        .iter()
        .filter(|ok| !**ok)
        .count()
    }
}

/// The parity invariant, per claim: re-parse `statement` and compare the derived
/// form against every authored slot that is actually checkable.
pub fn evaluate_claim(claim: &crate::model::ClaimFile, registry: &Registry) -> ClaimVerdict {
    let (mut form, _lints) = canonicalize_statement(&claim.statement, registry);
    // Authored selectors are an *input* to the canonical form (ADR 0003), sorted for
    // determinism. They are not derived from the statement — the formal selector syntax
    // is deferred — so they are set here, and the parity invariant below compares only
    // what the parser produces.
    form.applicability = claim.applicability.iter().cloned().collect();

    let unstructured_ok = form.unstructured == claim.unstructured;
    let modality_ok = claim.unstructured || form.modality_str() == claim.modality;
    // An authored-unstructured claim has no structured subject to compare; the
    // unstructured tag is the field that carries that decision.
    let subject_ok = claim.unstructured || form.subject == claim.subject;

    // value: only really checkable when the *authored* value is numeric
    let authored_v = claim.value.clone().unwrap_or_default();
    let authored_is_num =
        !authored_v.is_empty() && authored_v.chars().next().unwrap_or('x').is_ascii_digit();
    let v_parsed = form
        .value
        .as_ref()
        .map(canonical_decimal)
        .unwrap_or_default();
    let u_parsed = form.unit.clone().unwrap_or_else(|| "none".to_string());
    let authored_u = claim.unit.clone().unwrap_or_else(|| "none".to_string());
    let value_unit_ok = !authored_is_num || (v_parsed == authored_v && u_parsed == authored_u);

    let authored_op = claim.operator.clone().unwrap_or_default();
    let op_parsed = form.operator_str().to_string();
    let op_checkable = authored_is_num;
    let op_ok = !op_checkable || op_parsed == authored_op;

    // Conditions ARE parsed (T1.8), so they are comparable — and a claim that gains one
    // is a different claim, which makes an authored list that disagrees with the parse a
    // divergence rather than a detail. Before this, authored conditions were silently
    // unverified.
    let authored_conditions: BTreeSet<String> = claim.conditions.iter().cloned().collect();
    let conditions_ok = authored_conditions == form.conditions;
    let conditions_detail = if conditions_ok {
        String::new()
    } else {
        format!(
            "conditions parsed [{}] vs authored [{}]",
            form.conditions
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
            authored_conditions
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        )
    };

    // Slots the canonical layer cannot represent. Authoring one is not a mistake in the
    // claim — it is a gap in the rules — so it is reported and `--fill` refuses, rather
    // than writing an address that silently ignores what the author wrote.
    let mut unrepresented: Vec<&str> = Vec::new();
    if claim.quantifier.is_some() {
        unrepresented.push("quantifier");
    }
    if claim.exception.is_some() {
        unrepresented.push("exception");
    }
    if claim.temporal.is_some() {
        unrepresented.push("temporal");
    }
    let slots_ok = unrepresented.is_empty();
    let slots_detail = unrepresented.join(",");

    ClaimVerdict {
        form,
        unstructured_ok,
        modality_ok,
        subject_ok,
        value_unit_ok,
        op_ok,
        conditions_ok,
        conditions_detail,
        slots_ok,
        slots_detail,
        v_parsed,
        authored_v,
        op_parsed,
        op_checkable,
    }
}

/// One claim file, loaded and validated: the path it came from, the file name the
/// scan already validated as UTF-8 (the naming pass re-derives nothing, so a broken
/// invariant cannot turn into a misleading "empty stem" complaint), its bytes (the
/// writers preserve comments, so `text` is what `--fill` edits), and its parse.
#[derive(Debug)]
pub struct LoadedClaim {
    pub path: std::path::PathBuf,
    pub name: String,
    pub text: String,
    pub claim: crate::model::ClaimFile,
}

/// The canonical `REQ-<digits>` stem: uppercase exactly, because the canonical
/// filename is `REQ-<digits>.yml` (claim-format.md). This is the *naming* rule.
fn is_claim_stem(stem: &str) -> bool {
    let Some(digits) = stem.strip_prefix("REQ-") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// The *candidate* stem: the same shape, case-insensitively. A claim renamed with the
/// case knocked out of its stem (`req-0008.txt`, `Req-0008`) must still be seen and
/// reported — that is the identity/deletion failure this loader exists for — so
/// candidacy is deliberately looser than the naming rule, which stays exact.
fn looks_like_claim_stem(stem: &str) -> bool {
    let Some(prefix) = stem.get(..4) else {
        return false;
    };
    prefix.eq_ignore_ascii_case("req-")
        && stem.len() > 4
        && stem[4..].bytes().all(|b| b.is_ascii_digit())
}

/// Is this entry even a candidate — something that *might* be a claim file? Anything
/// else in `claims/` (the README, editor droppings) is not a claim and is left alone;
/// a candidate that turns out not to match the contract is an error, never a silent
/// drop.
///
/// The gate is deliberately wider than `*.yml`/`*.yaml`: `REQ-0008.txt`, `REQ-0008`,
/// `REQ-0008.yml.disabled`, the hidden `.REQ-0008` and the lowercase `req-0008.txt`
/// are claim-shaped names, and renaming a claim out of the contract must be loud
/// rather than a way to disable a requirement in silence. A leading dot is stripped
/// first, because `Path::extension` sees none on `.REQ-0008` and the leading segment
/// would otherwise be empty.
fn looks_like_claim_file(name: &str) -> bool {
    let path = std::path::Path::new(name);
    let yaml_extension = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml"));
    let visible = name.trim_start_matches('.');
    let leading_segment = visible.split_once('.').map_or(visible, |(head, _)| head);
    yaml_extension || looks_like_claim_stem(leading_segment)
}

/// The one corpus loader (F2/F3). Every candidate file under `claims/` either loads
/// as a claim or fails the corpus: identity is a content address over the set of
/// files, so a file that vanishes silently deletes a requirement.
///
/// Errors, in diagnosis order:
/// - an unclassifiable entry: a name that is not valid UTF-8 (it cannot be told from a
///   claim written in another encoding), or a **directory** — the corpus is flat, so a
///   subdirectory is a place a claim can be filed out of sight;
/// - an unreadable file, or YAML that does not parse under the strict `ClaimFile`
///   (a misspelled key names itself; a missing `applicability`/`relations` does too);
/// - two files declaring one `claim_id` — their **content** is compared, so an
///   identical copy and a divergent rewrite are named differently;
/// - a claim-shaped file that is not `REQ-<digits>.yml` (`req-0008.yml`,
///   `REQ-0008.yaml`, `REQ-0008.txt`, `notes.yml`);
/// - a filename whose stem is not the `claim_id` it declares (claim-format.md);
/// - an **empty** corpus: zero claims is indistinguishable from every claim having
///   vanished, so it is UNVERIFIABLE, never a pass.
///
/// The duplicate pass runs over every parsed candidate *before* the naming pass, so
/// the one way two files can share an id — the `.yaml` twin of a `.yml` — is
/// reported as the duplicate it is rather than as a filename complaint.
pub fn load_claims(root: &std::path::Path) -> Result<Vec<LoadedClaim>, String> {
    let dir = root.join("claims");
    let mut candidates: Vec<(std::path::PathBuf, String)> = Vec::new();
    for entry in
        std::fs::read_dir(&dir).map_err(|e| format!("claims dir {}: {e}", dir.display()))?
    {
        let entry = entry.map_err(|e| format!("claims dir {}: {e}", dir.display()))?;
        // A `read_dir` error is a corpus that cannot be read, not a file that is
        // absent; the old `filter_map(|e| e.ok())` discarded exactly that.
        let file_type = entry
            .file_type()
            .map_err(|e| format!("claims dir {}: {e}", dir.display()))?;
        let path = entry.path();
        // The corpus is flat: `mv claims/REQ-0008.yml claims/archive/` would otherwise
        // be a silent deletion — the entry is not claim-shaped, and the walk does not
        // recurse. A symlink to a directory is the same hole wearing a disguise.
        if file_type.is_dir() || (file_type.is_symlink() && path.is_dir()) {
            return Err(format!(
                "{}: `claims/` is flat, one claim per file — a directory here is an \
                 error, not a place a claim can live",
                path.display()
            ));
        }
        let raw_name = entry.file_name();
        let Some(name) = raw_name.to_str() else {
            return Err(format!(
                "{}: this entry's name is not valid UTF-8 (shown lossily), so it cannot \
                 be classified as a claim or as a non-claim",
                path.display()
            ));
        };
        if !looks_like_claim_file(name) {
            continue;
        }
        // `file_type` comes from the directory read and does not follow symlinks, so
        // this is the contract the message states: a symlinked claim is not a claim.
        if !file_type.is_file() {
            return Err(format!(
                "{}: a claim file must be a regular file (a symlink or special file is \
                 not one)",
                path.display()
            ));
        }
        candidates.push((path, name.to_string()));
    }
    if candidates.is_empty() {
        return Err(format!(
            "claims dir {}: no claim files — an empty corpus is indistinguishable \
             from one whose claims all vanished, so it is UNVERIFIABLE, never a pass",
            dir.display()
        ));
    }
    candidates.sort();

    let mut loaded: Vec<LoadedClaim> = Vec::new();
    for (path, name) in candidates {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let claim: crate::model::ClaimFile =
            yaml_serde::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
        loaded.push(LoadedClaim {
            path,
            name,
            text,
            claim,
        });
    }

    // Duplicate ids, with the files' content compared: an identical copy is the
    // fully silent case, a divergent rewrite is one that would collapse two
    // meanings into one address.
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, loaded_claim) in loaded.iter().enumerate() {
        if let Some(&first) = seen.get(loaded_claim.claim.claim_id.as_str()) {
            let compared = if loaded[first].text == loaded_claim.text {
                "identical content"
            } else {
                "content differs"
            };
            return Err(format!(
                "duplicate claim_id {}: {} and {} ({compared})",
                loaded_claim.claim.claim_id,
                loaded[first].path.display(),
                loaded_claim.path.display()
            ));
        }
        seen.insert(loaded_claim.claim.claim_id.as_str(), i);
    }

    for loaded_claim in loaded.iter() {
        let path = &loaded_claim.path;
        // The naming pass and the stem it reports come from one strip: if the suffix
        // and the `<digits>` shape both hold, that IS the stem, and re-deriving it
        // (with an `unwrap_or_default`) could only mask a broken invariant.
        let Some(stem) = loaded_claim
            .name
            .strip_suffix(".yml")
            .filter(|s| is_claim_stem(s))
        else {
            return Err(format!(
                "{}: not a claim filename — a claim file is `REQ-<digits>.yml` \
                 (filename = claim_id)",
                path.display()
            ));
        };
        if stem != loaded_claim.claim.claim_id {
            return Err(format!(
                "{}: claim_id {} does not match the filename stem {stem} \
                 (claim-format.md: filename = claim_id)",
                path.display(),
                loaded_claim.claim.claim_id
            ));
        }
    }
    // `candidates` was sorted by path before parsing, so `loaded` already is: the
    // order never depends on how the filesystem enumerated the directory.
    Ok(loaded)
}

/// The subject registry. A caller that cannot load it reports UNVERIFIABLE.
pub fn load_registry(root: &std::path::Path) -> Result<Registry, String> {
    Registry::load(root.join("aliases/subjects.yml").to_str().unwrap())
        .map_err(|e| format!("registry load failed: {e}"))
}

pub fn run_claim_diagnostics() -> i32 {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    run_claim_diagnostics_at(&root)
}

/// The diagnostics command over an explicit root. The repository's own corpus is
/// what `run_claim_diagnostics` loads; the parameter exists so the strict loader can
/// be exercised against a fixture corpus without writing into `claims/` (which root
/// a command *uses* is F6's separate, undecided question).
pub fn run_claim_diagnostics_at(root: &std::path::Path) -> i32 {
    let registry = match load_registry(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };
    let claims = match load_claims(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let mut checked = 0usize;
    let mut flags_total = 0usize;
    for loaded in claims.iter() {
        let claim = &loaded.claim;
        let verdict = evaluate_claim(claim, &registry);
        checked += 1;
        flags_total += verdict.flags();
        println!(
            "{:<9} {:<9} {:<9} {:<12} {:<10} {:<8} {:<9}",
            claim.claim_id,
            mark(
                verdict.unstructured_ok,
                verdict.form.unstructured.to_string()
            ),
            mark(verdict.modality_ok, verdict.form.modality_str().to_string()),
            mark(verdict.subject_ok, verdict.form.subject.clone()),
            mark(
                verdict.value_unit_ok,
                format!("{}/{}", verdict.v_parsed, verdict.authored_v)
            ),
            mark(
                verdict.op_ok,
                if verdict.op_checkable {
                    verdict.op_parsed.clone()
                } else {
                    "n/a".to_string()
                }
            ),
            mark(
                verdict.conditions_ok && verdict.slots_ok,
                if !verdict.slots_ok {
                    format!("unrepresentable: {}", verdict.slots_detail)
                } else {
                    verdict.conditions_detail.clone()
                }
            ),
        );
    }
    let _ = &registry;
    println!(
        "claims: {checked}; discrepancy flags: {flags_total} (v0 diagnostic: attribute lexicon, rationale wording, and applicability predicate syntax unverified by design; authoring quantifier/exception/temporal is refused, never ignored)"
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
