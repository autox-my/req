//! Kind declarations and the judgement of a claim's `fields` against them (ADR 0056, decisions 2 to 4).
//!
//! A claim of `req.claim/4` may carry `kind` (one name) and `fields` (a map of field name to value). Both
//! are authored, optional and **outside identity**: nothing here is read by [`crate::canon::derive`], and
//! nothing here is hashed in this unit. A value has no type until a kind declares it, and the declaration
//! is one file per kind, `.req/kinds/<kind>.yml`, `schema: req.kind/1`.
//!
//! Two things live here, and each has one home:
//!
//! * [`Declarations::read`] reads and validates `.req/kinds/`. A malformed declaration is exit **2**, every
//!   bad file and key in one run, as `.req/areas` is; an unreadable or newer one is exit **3**
//!   ([`ReadError::exit_code`]). [`precheck`] is the CLI boundary's one call.
//! * [`judge_fields`] judges one claim's `kind` and `fields` against the declarations. The corpus loader
//!   ([`crate::canon::load_claims`]) and `req compare` both call it, so there is one judgement. A refusal is
//!   exit 3, NOT JUDGED, and names the claim, the kind, the field and the declared alternatives.
//!
//! Absent is UNKNOWN: a field a claim does not carry has no [`JudgedField`], never a default and never a
//! `null` (decision 4). A `null` is refused.
//!
//! The values arrive from `yaml_serde` already typed, and the two are told apart by that kind, never by
//! re-reading text: a quoted `"3"` is a string and an unquoted `3` is a number. No float and no timestamp is
//! ever built; a date is checked by hand with integer arithmetic.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use yaml_serde::Value;

/// Where the declarations live, relative to the corpus root.
pub const KINDS_DIR: &str = ".req/kinds";
/// The declaration format this build reads (`schema: req.kind/<n>`).
pub const KIND_SCHEMA_V: u32 = 1;
const KIND_SCHEMA_PREFIX: &str = "req.kind/";

/// The eight field types, by the spelling a declaration uses.
pub const TYPE_NAMES: &[&str] = &[
    "string", "integer", "decimal", "bool", "enum", "ref", "date", "list",
];
/// The element types a `list` may have: the types that are not `bool` and not `list`.
const LIST_ELEMENT_NAMES: &[&str] = &["string", "integer", "decimal", "enum", "ref", "date"];

/// A kind or field name: `[a-z][a-z0-9_]*` — ASCII, lower case, usable as an identifier by a later check.
pub fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// An `enum` token: `[A-Za-z0-9][A-Za-z0-9_.-]*`.
fn is_enum_token(token: &str) -> bool {
    let mut chars = token.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// The error a name fails with, shared with `parse_claim` so the grammar is stated once.
pub fn name_grammar() -> &'static str {
    "[a-z][a-z0-9_]*"
}

// ---------------------------------------------------------------------------
// The declarations
// ---------------------------------------------------------------------------

/// A field's declared type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    String,
    Integer,
    Decimal,
    Bool,
    /// The kind's `values`, in the order declared (the order is the declared order, ascending, when the
    /// field is `ordered`).
    Enum(Vec<String>),
    Ref,
    Date,
    /// A set of one element type, never `Bool` and never `List`.
    List(Box<FieldType>),
}

impl FieldType {
    /// The spelling a declaration uses.
    pub fn name(&self) -> &'static str {
        match self {
            FieldType::String => "string",
            FieldType::Integer => "integer",
            FieldType::Decimal => "decimal",
            FieldType::Bool => "bool",
            FieldType::Enum(_) => "enum",
            FieldType::Ref => "ref",
            FieldType::Date => "date",
            FieldType::List(_) => "list",
        }
    }

    /// The element type's spelling, for a list.
    pub fn of_name(&self) -> Option<&'static str> {
        match self {
            FieldType::List(element) => Some(element.name()),
            _ => None,
        }
    }

    /// The declared alternatives, as a refusal states them: the `values` of an enum, or the type.
    pub fn describe(&self) -> String {
        match self {
            FieldType::Enum(values) => format!("enum ({})", values.join(", ")),
            FieldType::List(element) => format!("list of {}", element.describe()),
            other => other.name().to_string(),
        }
    }
}

/// One declared field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub ty: FieldType,
    pub required: bool,
    /// An `enum` only: its `values` are in ascending order. Declared, and read by nothing in this build
    /// (decision 9).
    pub ordered: bool,
}

/// One declared kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind {
    pub label: Option<String>,
    pub fields: BTreeMap<String, FieldDecl>,
}

/// Every kind declared under `.req/kinds/`, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declarations {
    kinds: BTreeMap<String, Kind>,
}

impl Declarations {
    pub fn kind(&self, name: &str) -> Option<&Kind> {
        self.kinds.get(name)
    }

    pub fn kinds(&self) -> impl Iterator<Item = (&str, &Kind)> {
        self.kinds.iter().map(|(name, kind)| (name.as_str(), kind))
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// An authored error with one remedy: exit 2.
    Malformed,
    /// A file this build cannot judge (newer) or cannot read: exit 3.
    Unjudged,
}

/// Why the declarations could not be used: every problem, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    problems: Vec<(Class, String)>,
}

impl ReadError {
    /// 2 when any declaration is malformed (an author must fix it before anything else can be judged), 3
    /// when the only problems are a newer format or an unreadable file.
    pub fn exit_code(&self) -> i32 {
        if self.problems.iter().any(|(c, _)| *c == Class::Malformed) {
            2
        } else {
            3
        }
    }

    pub fn messages(&self) -> Vec<String> {
        self.problems.iter().map(|(_, m)| m.clone()).collect()
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.messages().join("\n"))
    }
}

impl Declarations {
    /// Read `.req/kinds/` under `root`. An absent directory is no declaration, not an error. Every bad file
    /// and key is collected, so an author fixes the directory in one pass.
    pub fn read(root: &Path) -> Result<Declarations, ReadError> {
        let dir = root.join(KINDS_DIR);
        let mut problems: Vec<(Class, String)> = Vec::new();
        match std::fs::symlink_metadata(&dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Declarations::default());
            }
            Err(e) => {
                return Err(ReadError {
                    problems: vec![(Class::Unjudged, format!("read {}: {e}", dir.display()))],
                });
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(ReadError {
                    problems: vec![(
                        Class::Malformed,
                        format!(
                            "{}: `{KINDS_DIR}` is a directory of `<kind>.yml` files, one per kind",
                            dir.display()
                        ),
                    )],
                });
            }
            Ok(_) => {}
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                return Err(ReadError {
                    problems: vec![(Class::Unjudged, format!("read {}: {e}", dir.display()))],
                });
            }
        };
        let mut listed: Vec<(std::path::PathBuf, String, Option<std::fs::FileType>)> = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    problems.push((Class::Unjudged, format!("read {}: {e}", dir.display())));
                    continue;
                }
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            listed.push((entry.path(), name, entry.file_type().ok()));
        }
        listed.sort_by(|a, b| a.0.cmp(&b.0));
        let mut kinds = BTreeMap::new();
        for (path, name, file_type) in listed {
            let shown = path.display().to_string();
            // A stray entry: not a regular `*.yml` file. A subdirectory, a symlink and a special file are
            // places a declaration could hide.
            let regular = file_type.is_some_and(|t| t.is_file());
            let Some(stem) = name.strip_suffix(".yml").filter(|_| regular) else {
                problems.push((
                    Class::Malformed,
                    format!(
                        "{shown}: `{name}` is a stray entry: `{KINDS_DIR}` holds one regular `<kind>.yml` \
                         file per kind and nothing else (not a subdirectory, a symlink or another extension)"
                    ),
                ));
                continue;
            };
            if !is_name(stem) {
                problems.push((
                    Class::Malformed,
                    format!(
                        "{shown}: `{stem}` is not a kind name: the file's stem is the kind's name, \
                         `{}` (ASCII, lower case)",
                        name_grammar()
                    ),
                ));
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) => {
                    problems.push((Class::Unjudged, format!("read {shown}: {e}")));
                    continue;
                }
            };
            match parse_kind(&text) {
                Ok(Some(kind)) => {
                    kinds.insert(stem.to_string(), kind);
                }
                // A newer format: not judged, and its keys are not known to this build.
                Ok(None) => problems.push((Class::Unjudged, newer_message(&shown, &text))),
                Err(errors) => {
                    for error in errors {
                        problems.push((Class::Malformed, format!("{shown}: {error}")));
                    }
                }
            }
        }
        if problems.is_empty() {
            Ok(Declarations { kinds })
        } else {
            Err(ReadError { problems })
        }
    }
}

fn newer_message(shown: &str, text: &str) -> String {
    let declared = yaml_serde::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("schema").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    format!(
        "{shown}: declares schema {declared}, newer than the {KIND_SCHEMA_PREFIX}{KIND_SCHEMA_V} this build \
         reads: the declaration was NOT JUDGED — the file is not wrong, the reader is old. Read it with a \
         build that reads {declared}, which is not this one"
    )
}

/// One declaration file. `Ok(None)` is a newer format (not judged); `Err` is every malformed key.
fn parse_kind(text: &str) -> Result<Option<Kind>, Vec<String>> {
    let value: Value = yaml_serde::from_str(text)
        .map_err(|e| vec![format!("does not parse as a kind declaration: {e}")])?;
    let Value::Mapping(map) = &value else {
        return Err(vec![
            "a kind declaration is a mapping with `schema`, an optional `label` and `fields`"
                .to_string(),
        ]);
    };
    let mut errors: Vec<String> = Vec::new();
    // The format first: a newer one has keys this build does not know.
    match map.get("schema") {
        None => errors.push("missing `schema`: write `schema: req.kind/1`".to_string()),
        Some(Value::String(declared)) => {
            match declared
                .strip_prefix(KIND_SCHEMA_PREFIX)
                .and_then(|numeral| numeral.parse::<u32>().ok().filter(|n| n.to_string() == *numeral))
            {
                Some(KIND_SCHEMA_V) => {}
                Some(n) if n > KIND_SCHEMA_V => return Ok(None),
                _ => errors.push(format!(
                    "`schema` is {declared:?}, not `{KIND_SCHEMA_PREFIX}<n>` with a canonical numeral (this build reads \
                     `{KIND_SCHEMA_PREFIX}{KIND_SCHEMA_V}`)"
                )),
            }
        }
        Some(_) => errors.push("`schema` is not a string".to_string()),
    }
    let mut label = None;
    let mut fields = BTreeMap::new();
    let mut saw_fields = false;
    for (key, item) in map {
        let Some(key) = key.as_str() else {
            errors.push("a key that is not a string".to_string());
            continue;
        };
        match key {
            "schema" => {}
            "label" => match item {
                Value::String(s) => label = Some(s.clone()),
                _ => errors.push("`label` is not a string".to_string()),
            },
            "fields" => {
                saw_fields = true;
                parse_fields(item, &mut fields, &mut errors);
            }
            other => errors.push(format!(
                "unknown key `{other}`: a kind declaration has `schema`, `label` and `fields` and no other"
            )),
        }
    }
    if !saw_fields {
        errors.push("missing `fields`: a kind with no fields writes `fields: {}`".to_string());
    }
    if errors.is_empty() {
        Ok(Some(Kind { label, fields }))
    } else {
        Err(errors)
    }
}

fn parse_fields(item: &Value, fields: &mut BTreeMap<String, FieldDecl>, errors: &mut Vec<String>) {
    let Value::Mapping(map) = item else {
        errors.push("`fields` is a mapping of field name to its declaration".to_string());
        return;
    };
    for (name, declaration) in map {
        let Some(name) = name.as_str() else {
            errors.push("`fields` has a name that is not a string".to_string());
            continue;
        };
        if !is_name(name) {
            errors.push(format!(
                "field `{name}`: a field name is `{}` (ASCII, lower case)",
                name_grammar()
            ));
            continue;
        }
        match parse_field(name, declaration) {
            Ok(decl) => {
                fields.insert(name.to_string(), decl);
            }
            Err(mut found) => errors.append(&mut found),
        }
    }
}

fn parse_field(name: &str, declaration: &Value) -> Result<FieldDecl, Vec<String>> {
    let at = |message: String| format!("field `{name}`: {message}");
    let Value::Mapping(map) = declaration else {
        return Err(vec![at(
            "a declaration is a mapping with `type`".to_string()
        )]);
    };
    let mut errors: Vec<String> = Vec::new();
    let (mut ty, mut values, mut of, mut ordered, mut required) = (None, None, None, None, None);
    for (key, item) in map {
        let Some(key) = key.as_str() else {
            errors.push(at("a key that is not a string".to_string()));
            continue;
        };
        match key {
            "type" => ty = Some(item),
            "values" => values = Some(item),
            "of" => of = Some(item),
            "ordered" => ordered = Some(item),
            "required" => required = Some(item),
            other => errors.push(at(format!(
                "unknown key `{other}`: a field has `type`, `values`, `ordered`, `required` and `of` and no other \
                 (there is no default, range, pattern or cross-field rule: ADR 0056 decision 3)"
            ))),
        }
    }
    let flag = |item: Option<&Value>, key: &str, errors: &mut Vec<String>| -> Option<bool> {
        match item {
            None => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => {
                errors.push(at(format!("`{key}` is `true` or `false`")));
                None
            }
        }
    };
    let ordered_flag = flag(ordered, "ordered", &mut errors);
    let required_flag = flag(required, "required", &mut errors);
    let type_name = match ty {
        None => {
            errors.push(at(format!(
                "missing `type`: one of {}",
                TYPE_NAMES.join(", ")
            )));
            None
        }
        Some(Value::String(s)) if TYPE_NAMES.contains(&s.as_str()) => Some(s.as_str()),
        Some(_) => {
            errors.push(at(format!("`type` is one of {}", TYPE_NAMES.join(", "))));
            None
        }
    };
    let element_name = match (type_name, of) {
        (Some("list"), Some(Value::String(s))) if LIST_ELEMENT_NAMES.contains(&s.as_str()) => {
            Some(s.as_str())
        }
        (Some("list"), Some(Value::String(s))) if s == "bool" || s == "list" => {
            errors.push(at(format!(
                "a list of `{s}` is refused: a list is a set of `{}`",
                LIST_ELEMENT_NAMES.join("`, `")
            )));
            None
        }
        (Some("list"), Some(_)) => {
            errors.push(at(format!(
                "`of` is one of {}",
                LIST_ELEMENT_NAMES.join(", ")
            )));
            None
        }
        (Some("list"), None) => {
            errors.push(at(format!(
                "a `list` needs `of`: one of {}",
                LIST_ELEMENT_NAMES.join(", ")
            )));
            None
        }
        (Some(_), Some(_)) => {
            errors.push(at("`of` is valid on a `list` only".to_string()));
            None
        }
        _ => None,
    };
    // `values` belongs to an enum, or to a list of enum.
    let has_enum = type_name == Some("enum") || element_name == Some("enum");
    let enum_values = match values {
        None => {
            if has_enum {
                errors.push(at(
                    "an `enum` needs `values`: its distinct tokens".to_string()
                ));
            }
            None
        }
        Some(_) if type_name.is_some() && !has_enum => {
            errors.push(at(
                "`values` is valid on an `enum` or a list of `enum` only".to_string(),
            ));
            None
        }
        Some(Value::Sequence(items)) => parse_enum_values(name, items, &mut errors),
        Some(_) => {
            errors.push(at("`values` is a list of tokens".to_string()));
            None
        }
    };
    if ordered_flag.is_some() && type_name.is_some() && type_name != Some("enum") {
        errors.push(at("`ordered` is valid on an `enum` only".to_string()));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let scalar = |element: &str| -> Option<FieldType> {
        Some(match element {
            "string" => FieldType::String,
            "integer" => FieldType::Integer,
            "decimal" => FieldType::Decimal,
            "bool" => FieldType::Bool,
            "enum" => FieldType::Enum(enum_values.clone()?),
            "ref" => FieldType::Ref,
            "date" => FieldType::Date,
            _ => return None,
        })
    };
    let resolved = match (type_name, element_name) {
        (Some("list"), Some(element)) => scalar(element).map(|e| FieldType::List(Box::new(e))),
        (Some(other), _) => scalar(other),
        (None, _) => None,
    };
    match resolved {
        Some(ty) => Ok(FieldDecl {
            ty,
            required: required_flag.unwrap_or(false),
            ordered: ordered_flag.unwrap_or(false),
        }),
        None => Err(vec![at("the declaration could not be read".to_string())]),
    }
}

fn parse_enum_values(name: &str, items: &[Value], errors: &mut Vec<String>) -> Option<Vec<String>> {
    let at = |message: String| format!("field `{name}`: {message}");
    if items.is_empty() {
        errors.push(at(
            "`values` is empty: an `enum` needs at least one value".to_string()
        ));
        return None;
    }
    let mut values: Vec<String> = Vec::new();
    let mut ok = true;
    for item in items {
        let Value::String(token) = item else {
            errors.push(at(
                "`values` holds a token YAML would type (a boolean, a number or null): quote it, as a `string` is"
                    .to_string(),
            ));
            ok = false;
            continue;
        };
        if !is_enum_token(token) {
            errors.push(at(format!(
                "`values` has {token:?}, which is not a token `[A-Za-z0-9][A-Za-z0-9_.-]*`"
            )));
            ok = false;
        } else if values.contains(token) {
            errors.push(at(format!(
                "`values` has {token:?} twice: the values are distinct"
            )));
            ok = false;
        } else {
            values.push(token.clone());
        }
    }
    ok.then_some(values)
}

/// The CLI boundary's one call: judge the declarations once, before a command that loads claims runs.
/// `None` when they are usable (or there are none); otherwise everything is printed to stderr and the exit
/// code is returned: 2 for a malformed declaration (every bad file and key), 3 for one that is newer or
/// unreadable. The loader reads them again and would refuse in the same words, as exit 3 (a staged
/// revision of `req diff`); deciding the class here is what keeps a command from folding 2 into 3.
pub fn precheck(root: &Path) -> Option<i32> {
    let Err(error) = Declarations::read(root) else {
        return None;
    };
    let code = error.exit_code();
    for message in error.messages() {
        eprintln!(
            "{}{message}",
            if code == 3 { "[UNVERIFIABLE] " } else { "" }
        );
    }
    eprintln!(
        "nothing was run: a declaration under `{KINDS_DIR}` that cannot be judged is refused, never read around"
    );
    Some(code)
}

// ---------------------------------------------------------------------------
// Judging a claim's fields
// ---------------------------------------------------------------------------

/// A judged value, in canonical text: an `integer` and a `decimal` are their canonical strings, a `bool` is a
/// boolean, a `list` is its elements as authored (sorting for a digest is not done here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Bool(bool),
    List(Vec<String>),
}

impl fmt::Display for FieldValue {
    /// The diagnostic spelling `req canon` prints: text as it is (quoted when it holds whitespace), a
    /// boolean as `true` or `false`, a list as `[a,b]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldValue::Text(text) if text.chars().any(char::is_whitespace) => {
                write!(f, "{text:?}")
            }
            FieldValue::Text(text) => f.write_str(text),
            FieldValue::Bool(b) => write!(f, "{b}"),
            FieldValue::List(items) => write!(f, "[{}]", items.join(",")),
        }
    }
}

/// One judged field: its name, its declared type (and element type) and its value in canonical text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgedField {
    pub name: String,
    pub type_name: &'static str,
    pub of: Option<&'static str>,
    pub value: FieldValue,
}

/// Judge one claim's `kind` and `fields` against the declarations. Returns the fields sorted by name, one per
/// carried field (an absent field has no entry: absent is UNKNOWN), or **every** problem found, each naming the
/// claim, the kind, the field and the declared alternatives.
///
/// A missing required field and a `ref` that points at no claim are not refused here: they are findings
/// (ADR 0056 decision 4), because a claim written before the kind gained a required field, or before its target,
/// must still load.
pub fn judge_fields(
    claim_id: &str,
    kind: Option<&str>,
    fields: &BTreeMap<String, Value>,
    declarations: &Declarations,
) -> Result<Vec<JudgedField>, Vec<String>> {
    let Some(kind_name) = kind else {
        return if fields.is_empty() {
            Ok(Vec::new())
        } else {
            Err(vec![format!(
                "{claim_id}: carries `fields` without a `kind`: a value has no type until a kind declares it"
            )])
        };
    };
    let Some(declared) = declarations.kind(kind_name) else {
        let known: Vec<&str> = declarations.kinds().map(|(name, _)| name).collect();
        return Err(vec![format!(
            "{claim_id}: kind `{kind_name}` has no declaration: expected `{KINDS_DIR}/{kind_name}.yml` (declared \
             kinds: {}). With no declaration there is no type, so no claim is accepted untyped",
            if known.is_empty() {
                "none".to_string()
            } else {
                known.join(", ")
            }
        )]);
    };
    let mut judged = Vec::new();
    let mut errors = Vec::new();
    for (name, value) in fields {
        let Some(decl) = declared.fields.get(name) else {
            let known: Vec<&str> = declared.fields.keys().map(String::as_str).collect();
            errors.push(format!(
                "{claim_id}: kind `{kind_name}`: field `{name}` is not declared (declared: {})",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
            continue;
        };
        match judge_value(&decl.ty, value) {
            Ok(value) => judged.push(JudgedField {
                name: name.clone(),
                type_name: decl.ty.name(),
                of: decl.ty.of_name(),
                value,
            }),
            Err(why) => errors.push(format!(
                "{claim_id}: kind `{kind_name}`: field `{name}`: {why}; declared {}",
                decl.ty.describe()
            )),
        }
    }
    if errors.is_empty() {
        Ok(judged)
    } else {
        Err(errors)
    }
}

fn judge_value(ty: &FieldType, value: &Value) -> Result<FieldValue, String> {
    match (ty, value) {
        (_, Value::Null) => Err("`null` is neither absent nor a value: omit the key".to_string()),
        (FieldType::Bool, Value::Bool(b)) => Ok(FieldValue::Bool(*b)),
        (FieldType::Bool, other) => Err(format!(
            "{} is not a boolean: only a scalar the loader delivers as a YAML boolean is one (`true`, `True`, \
             `TRUE`, and the false forms; not `yes`, `on` or a quoted string)",
            describe_delivered(other)
        )),
        (FieldType::List(element), Value::Sequence(items)) => {
            let mut seen: Vec<String> = Vec::new();
            for (position, item) in items.iter().enumerate() {
                let one = judge_value(element, item)
                    .map_err(|why| format!("element {}: {why}", position + 1))?;
                let FieldValue::Text(text) = one else {
                    return Err(format!(
                        "element {}: a list holds scalars of one type",
                        position + 1
                    ));
                };
                if seen.contains(&text) {
                    return Err(format!("duplicate element {text:?}: a list is a set"));
                }
                seen.push(text);
            }
            Ok(FieldValue::List(seen))
        }
        (FieldType::List(_), other) => Err(format!("{} is not a list", describe_delivered(other))),
        (scalar, Value::String(text)) => judge_text(scalar, text).map(FieldValue::Text),
        (_, other) => Err(format!(
            "{} is not text: quote the value, so that it is the text it looks like",
            describe_delivered(other)
        )),
    }
}

fn describe_delivered(value: &Value) -> String {
    match value {
        Value::Bool(b) => format!("the unquoted scalar `{b}` (delivered as a boolean)"),
        Value::Number(n) => format!("the unquoted number `{n}`"),
        Value::String(s) => format!("the string {s:?}"),
        Value::Null => "`null`".to_string(),
        Value::Sequence(_) => "a list".to_string(),
        Value::Mapping(_) => "a mapping".to_string(),
        Value::Tagged(_) => "a tagged value".to_string(),
    }
}

fn judge_text(ty: &FieldType, text: &str) -> Result<String, String> {
    match ty {
        FieldType::String => {
            if text.is_empty() {
                Err("an empty string".to_string())
            } else if text.chars().any(char::is_control) {
                Err("a newline or other control character: a string is one line".to_string())
            } else if text.trim() != text {
                Err("leading or trailing space: a string is compared exactly".to_string())
            } else {
                Ok(text.to_string())
            }
        }
        FieldType::Integer => match text.parse::<i64>() {
            Ok(n) if n.to_string() == text => Ok(text.to_string()),
            _ => Err(format!(
                "{text:?} is not a canonical integer within i64 (`\"3\"`, `\"-12\"`; never `\"007\"`, `\"+1\"` or `\"-0\"`)"
            )),
        },
        FieldType::Decimal => {
            if crate::canon::is_canonical_decimal(text) {
                Ok(text.to_string())
            } else {
                Err(format!(
                    "{text:?} is not a canonical exact decimal string (`\"0.3\"`; never `\"0.30\"`, `\".3\"` or an exponent)"
                ))
            }
        }
        FieldType::Enum(values) => {
            if values.iter().any(|v| v == text) {
                Ok(text.to_string())
            } else {
                Err(format!("{text:?} is not one of the values"))
            }
        }
        FieldType::Ref => {
            if crate::canon::is_claim_stem(text) {
                Ok(text.to_string())
            } else {
                Err(format!("{text:?} is not a claim id (`REQ-<digits>`)"))
            }
        }
        FieldType::Date => {
            if is_calendar_date(text) {
                Ok(text.to_string())
            } else {
                Err(format!(
                    "{text:?} is not a calendar date `YYYY-MM-DD` (no clock, no zone, no impossible day)"
                ))
            }
        }
        // A bool and a list are judged before this point; a text can be neither.
        FieldType::Bool | FieldType::List(_) => {
            Err("a string is not a value of this type".to_string())
        }
    }
}

/// `YYYY-MM-DD`, ASCII digits, a real day of a real month, leap years included. Integer arithmetic only.
fn is_calendar_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = text.get(range)?;
        if part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse::<u32>().ok()
        } else {
            None
        }
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}
