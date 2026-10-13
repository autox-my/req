//! Selectors: which claims apply in a variant? (ADR 0003)
//!
//! A variant is a **selector over one model**, never a branch with its own copy. The
//! selector lives in the claim's `applicability` (part of identity since rules v5), and
//! evaluating it is tri-state on purpose: a predicate the context does not decide is
//! `Unknown` — **never** `No` (REQ-0016), because FALSE would drop a safety claim out of
//! a variant without anyone noticing.
//!
//! The grammar is deliberately tiny — `<key> == <value>` and `<key> >= <number>` — and
//! anything else is `Unknown` with its text kept: an unparsed selector is never guessed,
//! and never silently satisfied either.

use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// A variant context: the decisions a selection is made against.
pub type Context = BTreeMap<String, String>;

/// The three states. Note there is no `Default` that picks one: every caller has to say
/// what it does with `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    Yes,
    No,
    Unknown,
}

impl Applies {
    pub fn as_str(&self) -> &'static str {
        match self {
            Applies::Yes => "applies",
            Applies::No => "does_not_apply",
            Applies::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    Eq {
        key: String,
        value: String,
    },
    Ge {
        key: String,
        value: String,
    },
    /// Syntax the grammar does not cover. Kept verbatim so the report can show what it
    /// could not read.
    Unsupported {
        text: String,
    },
}

impl Predicate {
    /// The predicate as written, for a report a human can act on.
    pub fn source(&self) -> String {
        match self {
            Predicate::Eq { key, value } => format!("{key} == {value}"),
            Predicate::Ge { key, value } => format!("{key} >= {value}"),
            Predicate::Unsupported { text } => text.clone(),
        }
    }
}

/// Parse one authored predicate. Anything not in the grammar becomes `Unsupported`.
pub fn parse_predicate(text: &str) -> Predicate {
    if let Some((key, value)) = text.split_once("==") {
        let (key, value) = (key.trim(), value.trim());
        if !key.is_empty() && !value.is_empty() {
            return Predicate::Eq {
                key: key.to_string(),
                value: value.to_string(),
            };
        }
    }
    if let Some((key, value)) = text.split_once(">=") {
        let (key, value) = (key.trim(), value.trim());
        if !key.is_empty() && !value.is_empty() {
            return Predicate::Ge {
                key: key.to_string(),
                value: value.to_string(),
            };
        }
    }
    Predicate::Unsupported {
        text: text.trim().to_string(),
    }
}

/// Evaluate one predicate against a context.
pub fn evaluate(predicate: &Predicate, context: &Context) -> Applies {
    match predicate {
        Predicate::Eq { key, value } => match context.get(key) {
            // The context has not decided this key: UNKNOWN, not FALSE (REQ-0016).
            None => Applies::Unknown,
            Some(actual) if actual == value => Applies::Yes,
            Some(_) => Applies::No,
        },
        Predicate::Ge { key, value } => {
            let Some(actual) = context.get(key) else {
                return Applies::Unknown;
            };
            // Exact decimal comparison, and no guessing when either side is not a number.
            let (Ok(actual), Ok(bound)) = (actual.parse::<Decimal>(), value.parse::<Decimal>())
            else {
                return Applies::Unknown;
            };
            if actual >= bound {
                Applies::Yes
            } else {
                Applies::No
            }
        }
        Predicate::Unsupported { .. } => Applies::Unknown,
    }
}

/// A claim applies when **every** predicate it carries does. A decisive `No` excludes it
/// whatever else is undecided; anything left undecided is `Unknown`.
pub fn applies(predicates: &[Predicate], context: &Context) -> Applies {
    let mut unknown = false;
    for predicate in predicates {
        match evaluate(predicate, context) {
            Applies::No => return Applies::No,
            Applies::Unknown => unknown = true,
            Applies::Yes => {}
        }
    }
    if unknown {
        Applies::Unknown
    } else {
        Applies::Yes
    }
}

/// `req select key=value [key=value ...]` — tri-state selection over `claims/`.
///
/// Exit codes: **0** every claim decided, **2** usage, **3** at least one claim left
/// undecided. Undecided is its own outcome and is never folded into "does not apply".
pub fn run(args: &[String]) -> i32 {
    let mut context = Context::new();
    for arg in args {
        match arg.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                context.insert(key.to_string(), value.to_string());
            }
            _ => {
                eprintln!("usage: req select key=value [key=value ...]");
                return 2;
            }
        }
    }

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let claims = match crate::canon::load_claims(&root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] {e}");
            return 3;
        }
    };

    let (mut yes, mut no, mut unknown) = (0usize, 0usize, 0usize);
    for loaded in claims.iter() {
        let claim = &loaded.claim;
        let predicates: Vec<Predicate> = claim
            .applicability
            .iter()
            .map(|p| parse_predicate(p))
            .collect();
        match applies(&predicates, &context) {
            Applies::Yes => {
                yes += 1;
                if predicates.is_empty() {
                    println!("{}  applies  (no selector)", claim.claim_id);
                } else {
                    println!("{}  applies", claim.claim_id);
                }
            }
            Applies::No => no += 1,
            Applies::Unknown => {
                unknown += 1;
                let undecided: Vec<String> = predicates
                    .iter()
                    .filter(|p| evaluate(p, &context) == Applies::Unknown)
                    .map(|p| p.source())
                    .collect();
                println!(
                    "{}  UNKNOWN  undecided: {}",
                    claim.claim_id,
                    undecided.join("; ")
                );
            }
        }
    }
    println!(
        "applies {yes}, does not apply {no}, unknown {unknown} \
         (an undecided claim is never reported as excluded)"
    );
    if unknown > 0 { 3 } else { 0 }
}
