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
use unicode_normalization::UnicodeNormalization;

use crate::model::Registry;

/// Tier-1 normalization-rules version. 0 -> 1 added Markdown emphasis stripping
/// (T1.2b); 1 -> 2 added prohibition via "never", multi-obligation detection,
/// qualitative-predicate classification, and a precise comparator lint; 2 -> 3 fixed
/// the predicate span and made the canonical bytes valid JSON; 3 -> 4 stopped
/// resolving "may not"/"may never" (ambiguous negation scope — a real false merge on
/// the widened external corpus); 4 -> 5 put authored applicability into identity
/// (ADR 0003 said it belonged there; the form had no field for it at all); 5 -> 6 added the
/// coverage invariant (T1.10: no statement word leaves identity silently) and the whole-span
/// subject match (T1.7 as written) after two false merges in this repository's own corpus;
/// 6 -> 7 added the `IN` operator (ADR 0030): an enumerated "one of {A, B, C}" is a set in the
/// value slot and no longer the bound `1`; 7 -> 8 read a leading condition (`When X, the pump shall
/// stop`) as a condition, not as part of the subject (ADR 0038, issue #18); 8 -> 9 read "no more than" /
/// "not more than" / "no greater than" / "not greater than" as an upper bound (LE) where the bare
/// "more than" had read a lower one (GT) (R-1).
/// Addresses computed under older rules are re-canonicalization candidates, never silently
/// rewritten (hashing spec §5).
pub const RULES_V: u32 = 9;
/// The claim **file format's** own version (`docs/claim-format.md`), an axis that moves
/// independently of the normalization rules: a schema bump is a migration and moves no
/// address, while a rules bump re-derives every address. Named here, beside the other
/// identity-path versions, because two callers need one value — `--version` reports it and
/// the loader judges the value a file declares — and a literal at either site would be a
/// second home, free to drift from the contract the doc states.
///
/// 0 -> 1 adds the authored `tests:` list (ADR 0019): a claim carries the ids of the tests that
/// prove it, attested in the Baseline and **outside identity**. The bump is mechanical — a
/// format bump moves no address — but the loader judges it by direction, so a `req.claim/0` file
/// is a migration candidate (NOT JUDGED) rather than silently accepted by a build that writes a
/// field the old file's contract did not have.
///
/// 1 -> 2 (ADR 0022): a claim is **authored**, its parse is **derived**. The eleven authored
/// parse slots that re-stated the statement and the stored `canonical`/`address` are removed; an optional
/// `pins:` states what the parse must be where it is load-bearing. The address derivation and
/// its bytes do not change, so the bump moves no address — the dogfood list and the signed
/// Baseline are the proof. A `req.claim/1` file is a migration candidate
/// (`tools/migrate_claims.py`).
pub const CLAIM_SCHEMA_V: u32 = 2;
/// The `schema:` identifier's prefix: the value is `req.claim/<version>`. Split from the
/// version so the version is stated once, as a number, and the identifier is derived.
pub const CLAIM_SCHEMA_PREFIX: &str = "req.claim/";

/// The `schema:` value this build reads and writes — `req.claim/<CLAIM_SCHEMA_V>`, derived
/// rather than re-typed so no site can name a different one.
pub fn claim_schema() -> String {
    format!("{CLAIM_SCHEMA_PREFIX}{CLAIM_SCHEMA_V}")
}

/// What a claim file's declared `schema:` says, judged against the one this build writes.
///
/// The direction decides the remedy, as it does for `rules_v` in a Baseline manifest: a file
/// written under an **older** format is a migration candidate (a format bump moves no address,
/// `docs/claim-format.md`), a **newer** one is a question this build cannot answer because the
/// file is not wrong — the reader is old — and a value of no `req.claim/<n>` shape at all is
/// neither: it is a broken file, and [`ClaimSchema::reason`] says so rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimSchema {
    /// The schema this build reads and writes.
    Current,
    /// A schema this build no longer writes: a migration candidate.
    Older(u32),
    /// A schema newer than this build's: the reader is old, the file is not wrong.
    Newer(u32),
    /// Not the identifier this build's contract defines — no `req.claim/<n>` shape, or a numeral
    /// that is not canonical (`req.claim/00`): a broken file, not a version question.
    Malformed,
}

/// Judge a declared `schema:` value against the one this build writes. Pure, so the three
/// outcomes are testable without a file.
pub fn classify_claim_schema(declared: &str) -> ClaimSchema {
    classify_claim_schema_at(declared, CLAIM_SCHEMA_V)
}

/// The same judgement against an explicitly given build version.
///
/// The build version is a parameter for the same reason [`crate::baseline::rules_mismatch`]'s
/// direction is reachable from a test: the `Older` arm is what a file written before the first
/// format bump declares, and it must be judged **by direction** rather than as a parse failure.
/// Callers judge with [`claim_schema`]; only a test names another build.
pub fn classify_claim_schema_at(declared: &str, build_v: u32) -> ClaimSchema {
    let Some(numeral) = declared.strip_prefix(CLAIM_SCHEMA_PREFIX) else {
        return ClaimSchema::Malformed;
    };
    let Ok(declared_v) = numeral.parse::<u32>() else {
        return ClaimSchema::Malformed;
    };
    // The numeral is canonical, exactly as a claim's `value` is ("0.3", never "0.30"): `00` and
    // `+0` parse to the current version but are not the identifier `docs/claim-format.md`
    // defines, so they are a malformed value rather than a silent `Current`.
    if declared_v.to_string() != numeral {
        return ClaimSchema::Malformed;
    }
    match declared_v.cmp(&build_v) {
        std::cmp::Ordering::Equal => ClaimSchema::Current,
        std::cmp::Ordering::Less => ClaimSchema::Older(declared_v),
        std::cmp::Ordering::Greater => ClaimSchema::Newer(declared_v),
    }
}

impl ClaimSchema {
    /// What a reader must be told about a file whose declared schema is not the current one, or
    /// `None` when there is nothing to say.
    ///
    /// The wording is not cosmetic. Nothing inside the file was compared to anything, so every
    /// arm says **NOT JUDGED** — as [`crate::baseline::RulesMismatch::reason`] does for a rules
    /// version, and for the same reason: a reader told only that the file "could not be read"
    /// goes looking for a syntax error that is not there, while a reader told nothing concludes
    /// the file was checked. Each arm names the declared value, the value this build writes, and
    /// the one remedy the direction calls for.
    pub fn reason(&self, declared: &str) -> Option<String> {
        let current = claim_schema();
        match self {
            ClaimSchema::Current => None,
            ClaimSchema::Older(_) => Some(format!(
                "declares schema {declared}, which predates {current}: the claim was NOT JUDGED — \
                 the file is a migration candidate, and a format bump moves no address \
                 (docs/claim-format.md). Migrate it to {current} with tools/migrate_claims.py \
                 (a dry run by default; `--write` rewrites the file)"
            )),
            ClaimSchema::Newer(_) => Some(format!(
                "declares schema {declared}, newer than the {current} this build writes: the claim \
                 was NOT JUDGED — the file is not wrong, the reader is old. Read it with a build \
                 that writes {declared}, which is not this one"
            )),
            ClaimSchema::Malformed => Some(format!(
                "declares schema {declared:?}, which is not the {CLAIM_SCHEMA_PREFIX}<n> \
                 identifier docs/claim-format.md defines — the numeral is canonical, so `00` is \
                 not `0`: the claim was NOT JUDGED — the file cannot be established as a claim \
                 of this build's format at all, so it is a broken file, not a version question"
            )),
        }
    }
}

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
    /// Membership of a set (ADR 0030): "one of {A, B, C}". The set lives in
    /// [`CanonicalForm::members`]; there is no number.
    In,
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
            // No comparative stem inverts a membership; the absorbed-negation table
            // never produces `In`, so this arm only keeps the match total.
            Comp::In => Comp::In,
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
            Comp::In => "IN",
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
    /// The members of an `IN` set (ADR 0030), lower-cased and sorted by the `BTreeSet`;
    /// empty for every other operator. Serialised in the `value` slot as a JSON array.
    pub members: BTreeSet<String>,
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
    /// The value as a human reads it in `req canon`: the decimal, or `{a|b|c}` for an `IN` set.
    pub fn value_display(&self) -> Option<String> {
        if self.operator == Some(Comp::In) {
            return Some(format!(
                "{{{}}}",
                self.members.iter().cloned().collect::<Vec<_>>().join("|")
            ));
        }
        self.value.as_ref().map(canonical_decimal)
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
            other if u32::from(other) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", u32::from(other)))
            }
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
        match (&f.operator, &f.value) {
            // A set rides in the value slot as a sorted array, so no other claim's bytes move
            // (ADR 0030 §2): a number is a string, a set is a list, nothing is null.
            (Some(Comp::In), _) => format!(
                "[{}]",
                f.members
                    .iter()
                    .map(|m| format!("\"{}\"", json_escape(m)))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            (_, Some(d)) => format!("\"{}\"", json_escape(&canonical_decimal(d))),
            (_, None) => "null".to_string(),
        },
    )
}

/// `sha256("req.claim.v1\0" || canonical_form_bytes)` — the Tier-1 identity.
pub fn address_hex(canonical_form: &str) -> String {
    crate::digest::sha256_hex_prefixed(ADDRESS_TAG, canonical_form.as_bytes())
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
        d.checked_mul(Decimal::new(10i64.pow(power.unsigned_abs()), 0))
            .unwrap_or(d)
    } else {
        d.checked_div(Decimal::new(10i64.pow(power.unsigned_abs()), 0))
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
    // The negated upper bounds (rules v9, R-1): the mirrors of "no less than" / "not less than" below. Without them the
    // bare "more than" / "greater than" (GT) matched inside "no more than" and read an upper bound as a lower one.
    ("no more than", Comp::Le),
    ("not more than", Comp::Le),
    ("no greater than", Comp::Le),
    ("not greater than", Comp::Le),
    ("in fewer than", Comp::Lt),
    ("in less than", Comp::Lt),
    ("fewer than", Comp::Lt),
    ("less than", Comp::Lt),
    ("at least", Comp::Ge),
    ("no less than", Comp::Ge),
    ("not less than", Comp::Ge),
    ("no fewer than", Comp::Ge),
    ("not fewer than", Comp::Ge),
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

/// The only words a statement may carry without a slot (T1.10). Every entry is a word that
/// can be swapped or deleted **without moving the address**, so the list is a false-merge
/// surface and is kept to articles. Prepositions are not here: "before"/"after" and
/// "on"/"off" change meaning. Widen it only when a recorded mutation case shows that a
/// substitution cannot.
const COVERAGE_STOPWORDS: &[&str] = &["a", "an", "the"];

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

/// Guard words that open a sentence as a condition (rules v8, issue #18): the trailing set, plus
/// `where` — which is *not* a trailing guard, because "log the change where it is configured" is a
/// place or a relative pronoun far more often than a condition, but a sentence that **starts** with
/// "Where" is the EARS optional-feature condition.
const LEADING_GUARD_WORDS: &[&str] = &["when", "while", "unless", "during", "if", "where"];

/// A leading condition, `<guard> <condition>, <subject> <modal> …` (rules v8, issue #18): the guard word,
/// the condition (between the guard and the last comma before the subject, the comma dropped) and the
/// rest of the sentence, starting at the subject.
///
/// Only read when the sentence is unambiguous: it starts with a guard word, has a modal phrase, and a
/// comma ends the condition with **both** a non-empty condition and a non-empty subject on either side.
/// The condition ends at the **last** comma before the modal phrase, so a list inside the condition
/// ("When the door, the lock or the alarm fails, the pump shall stop") stays inside it. A sentence with
/// no such comma is left exactly as rules v7 read it (the whole clause in the subject): not guessed.
fn split_leading_condition(text: &str) -> Option<(String, String, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let guard = *words.first()?;
    if !LEADING_GUARD_WORDS.contains(&guard) {
        return None;
    }
    // Scanned for the position only: the lints it may raise belong to the real scan, later.
    let (modality, modal_start, _) = scan_modality(&words, &mut Vec::new());
    modality?;
    // The last comma-ended word before the modal, past the guard word, leaving a subject after it.
    let end = (1..modal_start.saturating_sub(1))
        .rev()
        .find(|&i| words[i].ends_with(','))?;
    let condition = words[1..=end].join(" ");
    let condition = condition.trim_end_matches(',').trim();
    if condition.is_empty() {
        return None;
    }
    Some((
        guard.to_string(),
        condition.to_string(),
        words[end + 1..].join(" "),
    ))
}

fn extract_conditions(body: &str, lints: &mut Vec<Lint>) -> (String, BTreeSet<String>) {
    let mut conditions: BTreeSet<String> = BTreeSet::new();
    // The leading condition first: a guard word inside it must not be taken for the trailing one.
    let mut remainder = match split_leading_condition(body) {
        Some((guard, condition, rest)) => {
            conditions.insert(format!("{guard}: {}", normalize_text(&condition)));
            rest
        }
        None => body.to_string(),
    };
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
// Membership: "one of {A, B, C}" (T1.5, rules v7, ADR 0030)
// ---------------------------------------------------------------------------

/// What `find_membership` found after the modal phrase.
enum Membership {
    /// No "one of", or one with no enumeration (`one of the target market countries`): the
    /// statement parses exactly as it did before `IN` existed.
    Absent,
    /// The index of the word "one" and the enumeration read after "of".
    Parsed(usize, Enumeration),
    /// An enumeration the parser will not guess at. The claim stays unstructured with this lint.
    Refused(Lint),
}

struct Enumeration {
    members: BTreeSet<String>,
    /// The words naming what is chosen from ("standards" in "the four standards: …"), minus the
    /// article, the count and "following". Identity, never dropped.
    header: String,
    /// Whatever follows the last member ("and no other standard"). Identity, never dropped.
    tail: String,
}

impl Enumeration {
    fn qualifier(&self) -> String {
        [self.header.as_str(), self.tail.as_str()]
            .iter()
            .filter(|p| !p.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn membership_lint(code: &str, detail: String) -> Lint {
    Lint {
        code: code.to_string(),
        detail,
    }
}

/// A count a list header may state: "two".."ten" (the curated word-number table) or a bare
/// number up to 99. "one" is not a count of a list.
fn header_count(word: &str) -> Option<usize> {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    if word == "one" {
        return None;
    }
    if let Some(d) = word_number(word) {
        return usize::try_from(d.mantissa()).ok();
    }
    if !word.is_empty() && word.len() <= 2 && word.chars().all(|c| c.is_ascii_digit()) {
        return word.parse().ok();
    }
    None
}

/// Split at top-level (depth-0) occurrences of `sep`, where depth counts `(` `[` `{`.
fn split_top(text: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    parts.into_iter().map(|p| p.trim().to_string()).collect()
}

/// Depth-0 positions (byte offsets) of the word `or` surrounded by spaces, or leading the text.
fn top_level_or(item: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    for (i, c) in item.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && c == 'o'
            && item[i..].starts_with("or ")
            && (i == 0 || item.as_bytes()[i - 1] == b' ')
        {
            out.push(i);
        }
    }
    out
}

/// The first depth-0 colon that ends a word (`sources:` yes, `1500:2019` no).
fn header_colon(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 && (i + 1 == bytes.len() || bytes[i + 1] == b' ') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Words that make a "member" a clause rather than a name. Outside parentheses only.
const MEMBER_CONNECTIVES: &[&str] = &["and", "but", "except", "unless", "otherwise", "nor"];

fn read_enumeration(rest: &str) -> Result<Option<Enumeration>, Lint> {
    let rest = rest.trim().trim_end_matches('.').trim();
    if rest.is_empty() {
        return Ok(None);
    }
    let unclear = |why: &str| {
        Err(membership_lint(
            "membership_unclear",
            format!("{why}; claim stays unstructured"),
        ))
    };

    // Shape 1: a bracketed list, no header: `one of {a, b, c}`.
    let first = rest.chars().next().unwrap_or(' ');
    let (header_text, list_text, tail_text, bracketed) = if matches!(first, '{' | '[' | '(') {
        let mut depth = 0i32;
        let mut close = None;
        for (i, c) in rest.char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            return unclear("an opening bracket is never closed");
        };
        let inner = &rest[first.len_utf8()..close];
        let after = rest[close + 1..]
            .trim()
            .trim_start_matches([',', ';'])
            .trim();
        (String::new(), inner.to_string(), after.to_string(), true)
    } else if let Some(colon) = header_colon(rest)
        && split_top(&rest[..colon], ',').len() == 1
    {
        // Shape 2: `one of <header>: a, b or c` (the header may be empty or "the following").
        (
            rest[..colon].trim().to_string(),
            rest[colon + 1..].trim().to_string(),
            String::new(),
            false,
        )
    } else {
        let segs = split_top(rest, ',');
        if segs.len() < 2 {
            return Ok(None); // no delimiter: prose, not an enumeration
        }
        let lead: Vec<&str> = segs[0].split_whitespace().collect();
        let counted = !lead.is_empty()
            && lead.len() <= 6
            && (header_count(lead[0]).is_some()
                || lead[0] == "following"
                || (lead[0] == "the"
                    && lead
                        .get(1)
                        .is_some_and(|w| header_count(w).is_some() || *w == "following")));
        if counted {
            // Shape 3: `one of the four sources, a, b or c`.
            (segs[0].clone(), segs[1..].join(", "), String::new(), false)
        } else {
            // Shape 4: a bare list, `one of a, b or c` — needs its `or`, below.
            (String::new(), rest.to_string(), String::new(), false)
        }
    };

    // The list text: comma items; the item holding the top-level `or` is the last member.
    let items = split_top(&list_text, ',');
    let mut members: Vec<String> = Vec::new();
    let mut tail_items: Vec<String> = Vec::new();
    let mut closed = false;
    for (n, item) in items.iter().enumerate() {
        if closed {
            tail_items.push(item.clone());
            continue;
        }
        let ors = top_level_or(item);
        match ors.len() {
            0 => members.push(item.clone()),
            1 => {
                let at = ors[0];
                let (left, right) = (item[..at].trim(), item[at + 3..].trim());
                if !left.is_empty() {
                    members.push(left.to_string());
                }
                members.push(right.to_string());
                closed = true;
            }
            _ => return unclear("a list item holds more than one `or`"),
        }
        let _ = n;
    }
    let has_or = closed;
    if !bracketed && !has_or && header_text.is_empty() {
        return Ok(None); // `one of a, b, c` with nothing to end the list: not enumerated
    }
    let tail = if bracketed {
        tail_text
    } else {
        tail_items.join(", ")
    };

    // A member is a short name: no clause connective outside parentheses, at most 8 such words.
    let mut seen = BTreeSet::new();
    for m in &members {
        let m = m.trim().trim_end_matches(['.', ';']).trim();
        if m.is_empty() {
            return unclear("an empty list member");
        }
        let mut depth = 0i32;
        let mut outside = String::new();
        for c in m.chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ if depth == 0 => outside.push(c),
                _ => {}
            }
        }
        let outside_words: Vec<&str> = outside.split_whitespace().collect();
        if outside_words.len() > 8 || outside_words.iter().any(|w| MEMBER_CONNECTIVES.contains(w)) {
            return unclear(&format!("list member {m:?} reads as a clause, not a name"));
        }
        if !seen.insert(m.to_string()) {
            return unclear(&format!("the member {m:?} appears twice"));
        }
    }
    if seen.len() < 2 {
        return unclear("a set of fewer than two members");
    }

    // The header: what is chosen from. The count it states must match the members.
    let words: Vec<&str> = header_text.split_whitespace().collect();
    let mut stated: Option<usize> = None;
    let mut header_words = Vec::new();
    for w in words {
        let bare = w.trim_matches(|c: char| !c.is_alphanumeric());
        if stated.is_none()
            && let Some(c) = header_count(bare)
        {
            stated = Some(c);
        } else if !matches!(bare, "the" | "these" | "those" | "following" | "a" | "an") {
            header_words.push(bare.to_string());
        }
    }
    if let Some(c) = stated
        && c != seen.len()
    {
        return Err(membership_lint(
            "membership_count_mismatch",
            format!(
                "the statement says {c} but lists {}; claim stays unstructured",
                seen.len()
            ),
        ));
    }
    Ok(Some(Enumeration {
        members: seen,
        header: header_words.join(" "),
        tail: tail.trim().trim_end_matches('.').trim().to_string(),
    }))
}

/// Look for `one of <enumeration>` in the words after the modal phrase.
fn find_membership(words: &[&str], from: usize) -> Membership {
    let Some(k) =
        (from..words.len().saturating_sub(1)).find(|&i| words[i] == "one" && words[i + 1] == "of")
    else {
        return Membership::Absent;
    };
    match read_enumeration(&words[k + 2..].join(" ")) {
        Ok(Some(e)) => Membership::Parsed(k, e),
        Ok(None) => Membership::Absent,
        Err(lint) => Membership::Refused(lint),
    }
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

    // Membership (rules v7): an enumerated "one of {…}" is a set. It runs before the comparator
    // table and the number scan because "one" would otherwise be read as the bound 1 and the
    // members would fall into the qualifier as free text (issue #17).
    let mut membership: Option<(usize, Enumeration)> = None;
    let mut membership_refused = false;
    if comparator.is_none() {
        match find_membership(&words, modal_end) {
            Membership::Parsed(k, e) => {
                comparator = Some(Comp::In);
                comp_start_abs = Some(k);
                membership = Some((k, e));
            }
            Membership::Refused(lint) => {
                lints.push(lint);
                membership_refused = true;
            }
            Membership::Absent => {}
        }
    }

    // ORDER: comparator first (it anchors the value), then the value, then the
    // action. The absorbed-negation case (a comparative directly after the modal)
    // has already set comparator+anchor above; the plain table runs next.
    let post_modal: Vec<&str> = words[modal_end.min(words.len())..].to_vec();

    let mut comp_len: usize = if membership.is_some() { 2 } else { 0 };
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
    // A set has no number: the scan would read the "1" out of "one of" or a digit inside a member.
    let scan_from = if membership.is_some() {
        post_modal.len()
    } else {
        scan_from
    };
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

    // T1.7 subject (rules v6: the **whole span**). A registry alias matches only when it
    // is the *entire* pre-modal span; anything else is a literal subject carrying every
    // word. Through rules v5 the longest alias covering a *prefix* won and the leftover
    // words fell in no slot, so "the req tool's semantic layer" and "the req tool's audit
    // layer" were both `REQ_TOOL` — a false merge inside this repository's own corpus,
    // and a violation of T1.7 as written ("unregistered synonyms are not collapsed").
    // Digit-rejection is narrowed to its real case: a value-led fragment (digit followed
    // by a known unit, e.g. fatal for "12 kN shall be applied...") — while digit-bearing
    // *names* ("a tier 2 provider's…") parse as subjects.
    let subject: Option<String> = match modality {
        Some(_) => {
            let subj: Vec<&str> = words[..modal_start.min(words.len())].to_vec();
            if subj.is_empty() || value_led_subject(&subj) {
                None
            } else {
                Some(registry.resolve(&subj.join(" ")))
            }
        }
        None => None,
    };

    // qualifier: the post-value phrase — an identity field, never dropped. The
    // tail starts after the digit token plus whatever unit token was consumed.
    let tail_start = num_idx.map(|i| {
        if unit_suffix.is_some() {
            i + 1
        } else {
            i + 1 + _unit_consumed
        }
    });
    let qualifier = match (&membership, tail_start) {
        (Some((_, e)), _) => e.qualifier(),
        (None, Some(start)) => words.get(start..).unwrap_or(&[]).join(" "),
        (None, None) => String::new(),
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
    let unparseable = second_obligation
        || untestable_qualifier.is_some()
        || ambiguous_may_not
        || membership_refused;

    // T1.10 coverage invariant (rules v6): every word *position* of the body must be held
    // by a slot or be a stopword, or the claim is unstructured. Checked only for a parse
    // that would otherwise be structured, so the lint names the one reason that is new
    // rather than burying it under the others.
    let mut uncovered_content = false;
    if subject.is_some() && !unparseable {
        let mut covered = vec![false; words.len()];
        let mut mark = |range: std::ops::Range<usize>| {
            for slot in covered.iter_mut().take(range.end).skip(range.start) {
                *slot = true;
            }
        };
        // subject: the whole pre-modal span, and the modal phrase (the absorbed
        // `<modal> <negator> <stem>` phrase is the modality and the comparator at once).
        mark(0..modal_start);
        mark(modal_start..absorb_end.unwrap_or(modal_end).max(modal_end));
        // action: the predicate span; comparator: its phrase, even when no value follows.
        mark(action_start..action_end);
        if comp_len > 0
            && let Some(cs) = comp_start_abs
        {
            mark(cs..cs + comp_len);
        }
        // value (+ unit) and the qualifier tail. A numeric token that did not resolve to a
        // value, or whose attached suffix did not resolve to a unit ("200-300", "5foobarbaz"),
        // is *not* held: the text it carried left the form.
        // The whole enumeration — the "one of" phrase is already held as the comparator — is
        // carried by the members, the header noun and the tail.
        if let Some((k, _)) = &membership {
            mark(k + 2..words.len());
        }
        if let Some(n) = num_idx {
            if value.is_some() && (unit_suffix.is_none() || unit.is_some()) {
                mark(n..n + 1);
                if unit_suffix.is_none() {
                    mark(n + 1..n + 1 + _unit_consumed);
                }
            }
            if let Some(start) = tail_start {
                mark(start..words.len());
            }
        }
        let uncovered: Vec<String> = words
            .iter()
            .zip(covered.iter())
            .filter(|(w, held)| !**held && !COVERAGE_STOPWORDS.contains(*w))
            .map(|(w, _)| {
                let bare = w.trim_matches(|c: char| !c.is_alphanumeric());
                if bare.is_empty() { *w } else { bare }.to_string()
            })
            .collect();
        if !uncovered.is_empty() {
            lints.push(Lint {
                code: "uncovered_content".to_string(),
                detail: uncovered.join(", "),
            });
            uncovered_content = true;
        }
    }
    let unparseable = unparseable || uncovered_content;

    let Some(subject) = subject.filter(|_| !unparseable) else {
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
            members: membership.map(|(_, e)| e.members).unwrap_or_default(),
            unit,
            qualifier,
            conditions,
            // The parser derives no selector: applicability is an *input* the claim
            // supplies, injected by `derive` before the form is hashed.
            applicability: BTreeSet::new(),
            exception: None,
            temporal: None,
        },
        lints,
    )
}

// ---------------------------------------------------------------------------
// Derived at load (ADR 0022): the parse, the form, the address, the pins' verdicts
// ---------------------------------------------------------------------------

/// What a claim's statement **derives** — computed when the claim is loaded and stored nowhere
/// (`req.claim/2`, ADR 0022). Every reader of an address reads this one, so `render`, `trace`,
/// `impact`, `baseline`, `audit`, `gate` and `compare` hash the same bytes.
#[derive(Debug, Clone)]
pub struct Derived {
    /// The canonical form: the statement's parse, with the authored `applicability` injected
    /// (an *input* to identity, ADR 0003 — the statement carries no selector syntax).
    pub form: CanonicalForm,
    /// What the canonicalizer could not decide (T1.6/T1.10), reported rather than guessed.
    pub lints: Vec<Lint>,
    /// The canonical bytes (T1.9) — the sha256 input.
    pub canonical: String,
    /// `sha256("req.claim.v1\0" ‖ canonical)`, the meaning fingerprint (ADR 0020).
    pub address: String,
    /// Each authored pin's verdict against [`Derived::form`], in [`crate::model::Pins::entries`]
    /// order; empty when the claim pins nothing.
    pub pins: Vec<PinCheck>,
}

impl Derived {
    /// Whether the statement is derived-unstructured (the §4 fallback, REQ-0007). Derived,
    /// never authored: `req.claim/2` has no `unstructured:` key.
    pub fn unstructured(&self) -> bool {
        self.form.unstructured
    }

    /// The modality a view shows: the derived one, or `UNSTRUCTURED` for the fallback — the
    /// word `req.claim/1` authors wrote for such a claim, kept so a projection's bytes do not
    /// move with the format.
    pub fn modality_label(&self) -> &'static str {
        if self.form.unstructured {
            "UNSTRUCTURED"
        } else {
            self.form.modality_str()
        }
    }

    /// The pins that flag (a contradiction or a slot the parse does not carry).
    pub fn flagged_pins(&self) -> impl Iterator<Item = &PinCheck> {
        self.pins.iter().filter(|check| check.flags())
    }
}

/// Derive a claim's form, address, lints and pin verdicts. Pure: the claim and the registry in,
/// the derivation out — the one place an address is computed from a claim, so no reader can
/// hash a different form.
pub fn derive(claim: &crate::model::ClaimFile, registry: &Registry) -> Derived {
    let (mut form, lints) = canonicalize_statement(&claim.statement, registry);
    // Authored selectors are an *input* to the canonical form (ADR 0003), sorted for
    // determinism by the `BTreeSet`. They are not derived from the statement — the formal
    // selector syntax is deferred — so they are set here, before the form is hashed.
    form.applicability = claim.applicability.iter().cloned().collect();
    let canonical = canonical_json(&form);
    let address = address_hex(&canonical);
    let pins = claim
        .pins
        .as_ref()
        .map(|pins| evaluate_pins(pins, &form))
        .unwrap_or_default();
    Derived {
        form,
        lints,
        canonical,
        address,
        pins,
    }
}

/// One pin's verdict (ADR 0022). Two outcomes besides agreement, and **both flag**: there is no
/// annotation category, because a pin the parse does not carry is precisely the bound that would
/// otherwise leave identity in silence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinVerdict {
    /// The derived form holds exactly the pinned value.
    Agrees,
    /// The derived form holds something else for this key — named, so the flag is checkable.
    Contradiction(String),
    /// The derived form holds no value for this key at all (an unstructured statement, or one
    /// whose parse found no comparator, number or unit).
    NotCarried,
}

/// One pinned key, what it pinned, and the verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinCheck {
    pub key: &'static str,
    pub pinned: String,
    pub verdict: PinVerdict,
}

impl PinCheck {
    /// A contradiction and a not-carried pin both flag; only agreement does not.
    pub fn flags(&self) -> bool {
        !matches!(self.verdict, PinVerdict::Agrees)
    }

    /// The verdict in words, as `req canon` and the gate print it.
    pub fn describe(&self) -> String {
        match &self.verdict {
            PinVerdict::Agrees => format!("pin {} {}: agrees", self.key, self.pinned),
            PinVerdict::Contradiction(derived) => format!(
                "pin {} {}: FLAG contradiction — the parse says {derived}",
                self.key, self.pinned
            ),
            PinVerdict::NotCarried => format!(
                "pin {} {}: FLAG not carried — the parse holds no {}",
                self.key, self.pinned, self.key
            ),
        }
    }
}

/// Is `value` a canonical exact decimal string — the form [`canonical_decimal`] writes (`"0.3"`,
/// never `"0.30"`, `".3"`, `"+1"` or `"3e-1"`)? The claim-format rule for a pinned `value`.
pub fn is_canonical_decimal(value: &str) -> bool {
    !value.contains(['e', 'E'])
        && value
            .parse::<Decimal>()
            .is_ok_and(|d| canonical_decimal(&d) == value)
}

/// Judge every pin against a derived form. Pure, and the whole rule (ADR 0022):
///
/// * the form holds no value for the key → **not carried** (a flag);
/// * it holds the pinned value exactly → **agrees**;
/// * it holds another → **contradiction** (a flag), naming the derived value.
///
/// `value` is compared as the canonical decimal string the form serializes, so `"0.3"` agrees
/// with a parse of `0.30` and `"0.30"` cannot be pinned at all ([`crate::model::validate_pins`]).
/// `unit: none` is the no-unit marker: it agrees with a form that carries no unit (or the
/// dimensionless `none`), and is contradicted by one that carries a unit. An unstructured form
/// carries nothing, so every pin on it is not carried.
pub fn evaluate_pins(pins: &crate::model::Pins, form: &CanonicalForm) -> Vec<PinCheck> {
    let structured = !form.unstructured;
    let derived_unit = form.unit.clone().filter(|u| u != "none");
    pins.entries()
        .into_iter()
        .map(|(key, pinned)| {
            let derived: Option<String> = if !structured {
                None
            } else {
                match key {
                    "modality" => form.modality.map(|m| m.as_str().to_string()),
                    "subject" => Some(form.subject.clone()).filter(|s| !s.is_empty()),
                    "operator" => form.operator.map(|c| c.as_str().to_string()),
                    "value" => form.value.as_ref().map(canonical_decimal),
                    "unit" => Some(derived_unit.clone().unwrap_or_else(|| "none".to_string())),
                    _ => None,
                }
            };
            let verdict = match derived {
                // `unit: none` pinned over a form with a unit is a contradiction, and a real
                // unit pinned over a unitless form is a unit the parse does not carry.
                Some(d) if key == "unit" && d == "none" && pinned != "none" => {
                    PinVerdict::NotCarried
                }
                Some(d) if d == pinned => PinVerdict::Agrees,
                Some(d) => PinVerdict::Contradiction(d),
                None => PinVerdict::NotCarried,
            };
            PinCheck {
                key,
                pinned: pinned.to_string(),
                verdict,
            }
        })
        .collect()
}

/// Parse one claim file's text under this build's contract (`req.claim/2`), in the order a
/// reader needs the answers:
///
/// 1. the declared `schema:` — a file of another format is NOT JUDGED
///    ([`ClaimSchema::reason`]), before its keys are held to a contract they were not written
///    against (a `req.claim/1` file carries every removed key, and the remedy is a migration,
///    not fifteen key errors);
/// 2. a removed `req.claim/1` key, **named** — never silently ignored;
/// 3. the strict [`crate::model::ClaimFile`] (a misspelled key, a missing `relations:`);
/// 4. the `pins:` values and the `tests:` declaration.
///
/// The one parse every reader calls — the corpus loader and `compare`'s two paths — so the
/// judgement has one home.
pub fn parse_claim(text: &str) -> Result<crate::model::ClaimFile, String> {
    #[derive(serde::Deserialize)]
    struct DeclaredSchema {
        #[serde(default)]
        schema: Option<String>,
    }
    if let Ok(DeclaredSchema {
        schema: Some(declared),
    }) = yaml_serde::from_str::<DeclaredSchema>(text)
        && let Some(reason) = classify_claim_schema(&declared).reason(&declared)
    {
        return Err(reason);
    }
    if let Ok(keys) = yaml_serde::from_str::<BTreeMap<String, serde::de::IgnoredAny>>(text) {
        let removed: Vec<String> = crate::model::REMOVED_V1_KEYS
            .iter()
            .filter(|key| keys.contains_key(**key))
            .map(|key| format!("`{key}`"))
            .collect();
        if !removed.is_empty() {
            return Err(format!(
                "carries {}, removed in {current} (ADR 0022): the parse is derived from \
                 `statement` when the claim is loaded and nothing derived is stored. A removed \
                 `modality`, `subject`, `operator`, `value` or `unit` can be stated as a pin under \
                 `pins:`. `conditions`, `unstructured`, `canonical` and `address` are derived: \
                 the statement carries them. `exception`, `temporal`, `quantifier` and \
                 `attribute` have no slot in the form at all: their words belong in `statement`, \
                 and `req canon` lints any the parse refused to hold (`uncovered_content`). \
                 `tools/migrate_claims.py` migrates a whole `req.claim/1` file",
                removed.join(", "),
                current = claim_schema()
            ));
        }
    }
    let claim: crate::model::ClaimFile =
        yaml_serde::from_str(text).map_err(|e| format!("does not parse as a claim: {e}"))?;
    if let Some(pins) = &claim.pins {
        crate::model::validate_pins(pins)?;
    }
    // The authored link list is part of the file contract even though it is outside
    // identity: an empty or duplicated id is refused here, where the file can be named,
    // rather than becoming a confusing gate finding later (ADR 0019).
    crate::model::validate_tests(&claim.tests, claim.evidence)?;
    Ok(claim)
}

/// One claim file, loaded and validated: the path it came from, the file name the
/// scan already validated as UTF-8 (the naming pass re-derives nothing, so a broken
/// invariant cannot turn into a misleading "empty stem" complaint), its bytes, its parse,
/// and what its statement **derives** — computed here, at load, and stored nowhere (ADR 0022).
#[derive(Debug)]
pub struct LoadedClaim {
    pub path: std::path::PathBuf,
    pub name: String,
    pub text: String,
    pub claim: crate::model::ClaimFile,
    pub derived: Derived,
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
///   (a misspelled key names itself; a missing `applicability`/`relations` does too; a removed
///   `req.claim/1` key is named as removed, ADR 0022);
/// - a `schema:` that is not the file format this build writes (`CLAIM_SCHEMA_V`), judged
///   **first** by [`parse_claim`] and **by direction**, as [`crate::baseline::rules_mismatch`]
///   judges `rules_v`: older is a migration candidate, newer is a question this build cannot
///   answer, and a value of no `req.claim/<n>` shape is a broken file. All three are NOT JUDGED,
///   and refusing here means no command — `canon`, the views, the gate, `compare` — can quietly
///   ignore the declaration. `compare` reads its two sides by path rather than through this
///   loader, and calls [`parse_claim`] itself so the judgement still has one home;
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
///
/// The registry is loaded first (a corpus whose registry cannot be read derives nothing, and is
/// UNVERIFIABLE), and every claim's [`Derived`] is computed against it.
pub fn load_claims(root: &std::path::Path) -> Result<Vec<LoadedClaim>, String> {
    let registry = load_registry(root)?;
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
        // The declared format first, then the removed keys, the strict contract, the pins and
        // the test links — all in `parse_claim`, so every command inherits the checks rather
        // than each one re-deciding whether to look.
        let claim = parse_claim(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let derived = derive(&claim, &registry);
        loaded.push(LoadedClaim {
            path,
            name,
            text,
            claim,
            derived,
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
    Registry::load(
        root.join("aliases/subjects.yml")
            .to_str()
            .expect("a registry path under the corpus root is valid UTF-8"),
    )
    .map_err(|e| format!("registry load failed: {e}"))
}

/// The diagnostics command over an explicit root: `claims/` and `aliases/` are read from
/// it. The root is resolved once at the CLI boundary and threaded to every read path
/// (ADR 0012), so this is also the seam the strict loader is exercised through against a
/// fixture corpus — without writing into the repository's own `claims/`.
///
/// It prints the **derived** parse per claim (structure, modality, subject, operator, value,
/// unit, lints) and each pin's verdict, and it reports: exit 0 whatever it found, 3 when the
/// corpus cannot be read. The check that gates pins is `req gate` (`pins_hold`).
pub fn run_claim_diagnostics_at(root: &std::path::Path) -> i32 {
    run_claim_diagnostics_with(root, false)
}

/// [`run_claim_diagnostics_at`] with the opt-in `--fail-on-flag` (issue #106): the same output on
/// stdout, and a flagged pin on **any** claim — applying in some variant or not — exits 1, naming
/// the claims on stderr. It answers "is any pin wrong" before test evidence exists; it is not a
/// release verdict, so a clean run says so and never prints the gate's word `PASS`. A corpus that
/// cannot be read stays 3 either way.
pub fn run_claim_diagnostics_with(root: &std::path::Path, fail_on_flag: bool) -> i32 {
    let claims = match load_claims(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let (mut unstructured, mut pins, mut pinned_claims, mut flagged, mut linted) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut flagged_claims: Vec<&str> = Vec::new();
    for loaded in claims.iter() {
        let derived = &loaded.derived;
        let form = &derived.form;
        let dash = |s: Option<String>| s.unwrap_or_else(|| "-".to_string());
        let lints: Vec<&str> = derived.lints.iter().map(|l| l.code.as_str()).collect();
        unstructured += usize::from(derived.unstructured());
        linted += usize::from(!lints.is_empty());
        let (modality, subject, operator, value, unit) = if derived.unstructured() {
            (
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
            )
        } else {
            (
                form.modality_str().to_string(),
                Some(form.subject.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "-".to_string()),
                dash(form.operator.map(|c| c.as_str().to_string())),
                dash(form.value_display()),
                dash(form.unit.clone().filter(|u| u != "none")),
            )
        };
        println!(
            "{:<9} {:<12} {:<9} {:<12} op {:<4} value {:<6} unit {:<5} lints {}",
            loaded.claim.claim_id,
            if derived.unstructured() {
                "unstructured"
            } else {
                "structured"
            },
            modality,
            subject,
            operator,
            value,
            unit,
            if lints.is_empty() {
                "-".to_string()
            } else {
                lints.join(",")
            },
        );
        // What the row cannot hold: the words a lint names, and the identity-bearing parts of the
        // form (a condition or qualifier that reached the address, or did not).
        if !derived.unstructured() {
            if !form.conditions.is_empty() {
                let conditions: Vec<&str> = form.conditions.iter().map(String::as_str).collect();
                println!("  conditions: {}", conditions.join("; "));
            }
            if !form.qualifier.is_empty() {
                println!("  qualifier: {}", form.qualifier);
            }
        }
        for lint in &derived.lints {
            println!("  lint {}: {}", lint.code, lint.detail);
        }
        pins += derived.pins.len();
        pinned_claims += usize::from(!derived.pins.is_empty());
        let before = flagged;
        for check in &derived.pins {
            flagged += usize::from(check.flags());
            println!("  {}", check.describe());
        }
        if flagged > before {
            flagged_claims.push(loaded.claim.claim_id.as_str());
        }
    }
    println!(
        "claims: {} ({unstructured} unstructured); pins: {pins} on {pinned_claims} claim(s), \
         {flagged} flagged; lints on {linted} claim(s) \
         (diagnostics: the parse is derived from `statement` at load and stored nowhere \
         (req.claim/2); a pin agrees or is a flag — a contradiction, or a slot the parse does \
         not carry; `req canon` reports and fails on a flag only with `--fail-on-flag`; the check that gates a \
         release is `req gate` (pins_hold))",
        claims.len()
    );
    if !fail_on_flag {
        return 0;
    }
    if flagged_claims.is_empty() {
        eprintln!(
            "--fail-on-flag: no pin is flagged on any claim. This is not a release verdict: \
             tests, relations and decisions were not judged; `req gate` judges those."
        );
        0
    } else {
        eprintln!(
            "--fail-on-flag: {flagged} pin(s) flagged on {} claim(s): {}. Re-word the statement or \
             leave the flag; never edit a pin to fit the parse.",
            flagged_claims.len(),
            flagged_claims.join(", ")
        );
        1
    }
}

/// The message `req canon --fill` and `req canon --verify` answer with (exit 2): the flags are
/// removed in `req.claim/2`, because there is nothing stored to write or to check (ADR 0022).
pub const FILL_VERIFY_REMOVED: &str = "req canon --fill and --verify are removed: nothing derived \
is stored in req.claim/2 — the parse, the canonical form and the address are computed from \
`statement` when a claim is loaded — so there is nothing to fill or verify. `req canon` shows the \
derived parse and each pin's verdict; the check for pins is `req gate` (pins_hold).";
