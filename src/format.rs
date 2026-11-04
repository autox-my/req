//! `--format`: the text rendering, or a JSON document on stdout (pack C).
//!
//! The document is a **rendering** of the projection the text path already built, never a second
//! derivation and never a second answer: each command computes its verdict once, folds the exit
//! code from it, and either prints text or serializes the same structures. JSON and text
//! therefore cannot disagree about a class of answer.
//!
//! Three rules shape what is emitted:
//!
//! * **the default stays `text`**, so every doc, test and script that reads a bare command is
//!   unaffected;
//! * **`--format` is orthogonal to the verdict** — `0`/`1`/`3`/`2` keep their meanings under both
//!   formats — and a JSON *projection on disk* is deliberately not implemented: `--out`/`--check`
//!   beside it are usage errors naming the follow-on rather than a silent half-feature;
//! * **determinism is contract**: the selection is a `BTreeMap`, arrays are ordered by claim id,
//!   no float ever appears (a decimal is a canonical string), and two runs are byte-identical.
//!
//! The schema string is versioned **independently of [`crate::canon::RULES_V`]**: the rules
//! version decides addresses, the schema decides shape. Coupling them would force a schema bump
//! for a meaning-preserving rule change, and a shape change without a rules change would be
//! unrepresentable. The payload carries `rules_v` as data, and a build writes only the schema
//! version it knows — the same rule the claim file schema follows.

use serde::Serialize;

/// One schema string per command, versioned independently of `RULES_V` (see the module doc).
pub const SCHEMA_RENDER: &str = "req.render/1";
pub const SCHEMA_TRACE: &str = "req.trace/1";
pub const SCHEMA_AUDIT: &str = "req.audit/1";
/// `/2` (ADR 0022): the first policy is `pins_hold`, where `/1` had `addresses_current` — a
/// policy was renamed, so the shape moved; the exit contract did not.
pub const SCHEMA_GATE: &str = "req.gate/2";

/// The two renderings. `Text` is the default, and the only one a bare command uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

/// Strip `--format <value>` out of one command's arguments, leaving the rest untouched.
///
/// Shared by the four commands the pack scopes, so the accepted values, the error wording and the
/// default cannot drift between them. `usage` is the invoked command's synopsis, so an error names
/// the verb and the shape it accepts rather than a generic line.
///
/// A value that is option-shaped is refused as a missing value: `--format --out` is a `--format`
/// typo, and reading `--out` as a format would be a silent answer to a question nobody asked.
pub fn take_format(args: &[String], usage: &str) -> Result<(Format, Vec<String>), i32> {
    let mut format = Format::Text;
    let mut rest = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        if args[index] != "--format" {
            rest.push(args[index].clone());
            index += 1;
            continue;
        }
        let Some(value) = args.get(index + 1) else {
            eprintln!("{usage}\n`--format` needs a value: text or json");
            return Err(2);
        };
        if value.is_empty() || value.starts_with('-') {
            eprintln!("{usage}\n`--format` needs a value (text or json), not {value:?}");
            return Err(2);
        }
        format = match value.as_str() {
            "text" => Format::Text,
            "json" => Format::Json,
            other => {
                eprintln!(
                    "{usage}\nunknown format {other:?}: `--format` takes `text` (the default) or \
                     `json`"
                );
                return Err(2);
            }
        };
        index += 2;
    }
    Ok((format, rest))
}

/// Serialize one document. The field types are strings, borrows, sorted maps and vectors, all of
/// which serialize infallibly — the expectation is the same one `model.rs` records for the
/// attested audit surface, not a swallowed error.
pub fn document<T: Serialize>(document: &T) -> String {
    serde_json::to_string_pretty(document)
        .expect("a JSON document of strings, maps and vectors serializes infallibly")
}
