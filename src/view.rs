//! Views: a selection compiled into something a person reads (design step 9).
//!
//! `render` is the spec view — the 150% model with its selectors visible — and `trace` is
//! the traceability matrix. Both are **projections**: they read claims, they never write
//! one, and selecting changes no address.
//!
//! Three rules carry over from the layers below, and each is why this module exists:
//!
//! * an **undecided** claim is shown, in its own section. A view that drops it is exactly
//!   the silent exclusion ADR 0003 and REQ-0016 forbid, one layer up;
//! * **incoming relations are derived** here, because REQ-0020 stores each relation once
//!   on the outgoing side;
//! * every **edge that leaves the selection** is named — to a claim the variant excludes
//!   (legitimate, reported) or to one that is undecided (a gap needing a decision). A
//!   trace that quietly loses the parent of an applicable claim is worse than no trace.
//!
//! The view is deterministic: rows are sorted by claim id, and nothing here iterates a
//! hash map.
//!
//! # Committed projections
//!
//! ADR 0018: a view is a function of the corpus, so it can be **committed** as an artefact and
//! proven current by regeneration. `--out <path>` writes the view with a machine-readable header
//! (`<!-- req projection: generator=… | rules_v=… | selection=… | claims=… | identity=sha256:… -->`);
//! `--check <path>` regenerates and compares **bytes**, and never writes. The header makes a
//! stale file self-evidently stale and lets the check refuse to compare two different things
//! (another build's `rules_v`, a selection it cannot reproduce, another command's projection) —
//! but it is never the oracle: the corpus is (ADR 0018 §3, falsification case 1).
//!
//! The plain commands are untouched: `render` and `trace` with no flag print the same bytes and
//! answer the same 0/3 classes as before, because a view printed for a human and a document
//! committed for a repository are different things.

use crate::canon::RULES_V;
use crate::check::{Listing, read_list};
use crate::completeness::{AREAS, AreaEntry, parse_areas};
use crate::digest::sha256_hex;
use crate::format::{self, Format, SCHEMA_RENDER, SCHEMA_TRACE};
use crate::model::Evidence;
use crate::select::{Applies, Context, applies, parse_predicate};
use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One claim, as the view needs it: its identity, what it says, and how it selected.
#[derive(Debug, Clone)]
pub struct Row {
    pub claim_id: String,
    pub statement: String,
    pub modality: String,
    pub address: String,
    pub applies: Applies,
    pub selectors: Vec<String>,
    /// `(kind, to)` — the outgoing side only (REQ-0020).
    pub relations: Vec<(String, String)>,
    /// ADR 0013: the attested audit surface, hashed from the **parsed** claim at load time,
    /// so emit and verify compare like with like. Identity (the address) stays meaning-only.
    pub relations_hash: String,
    pub rationale_hash: String,
    /// The authored test links, verbatim, and their attested digest (ADR 0019). Identity does not
    /// carry either: a test rename moves no address, and the manifest attests the digest so an
    /// agent cannot quietly unlink what it cannot prove.
    ///
    /// They are **not** in `impl Serialize for Row`: the document's row shape is the view a
    /// consumer reads (`claim_id`, `statement`, `modality`, `address`, `applies`, `selectors`,
    /// `relations`, `rationale`), and the evidence declaration is audit-surface data like
    /// `relations_hash`/`rationale_hash` — it rides the Baseline, not the projection. (`pins` are
    /// judged by the gate but not attested by the Baseline: ADR 0022's named loss.)
    /// Adding them would be a document-shape change, not a rendering detail.
    pub tests: Vec<String>,
    /// The declared evidence mode (machine by default, process when a human said so), and the
    /// digest over the whole evidence declaration — both outside identity (ADR 0019).
    pub evidence: Evidence,
    pub tests_hash: String,
    /// Each authored pin's verdict against the parse derived at load (ADR 0022): the release
    /// gate's `pins_hold` reads it. Empty when the claim pins nothing.
    pub pins: Vec<crate::canon::PinCheck>,
    /// Provenance, for the audit policy and the view.
    pub rationale_source: String,
    pub rationale_date: String,
    /// ADR 0049: the successors that withdrew this claim in this selection, sorted; empty unless
    /// the claim is withdrawn (then `applies` is `No`).
    pub superseded_by: Vec<String>,
    /// Where the claim is filed (`req.claim/3`, ADR 0051): a path of stable keys, or `None`. Placement
    /// only — outside identity and in no attested hash, so nothing but the JSON documents and
    /// `req canon` reads it, and no text projection changes.
    pub area: Option<String>,
}

/// What a trace could not settle. Both are outcomes, not warnings: `dangling` is
/// UNVERIFIABLE, `undecided_edges` needs a decision from whoever owns the variant.
///
/// `Serialize` is derived so `--format json` carries the findings **verbatim** — the document is
/// the same classification the text path prints, not a second one.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct TraceFindings {
    pub dangling: Vec<String>,
    pub undecided_edges: Vec<String>,
}

/// The spec view: what applies, what is undecided, and how much is outside this variant.
pub fn render(rows: &[Row], context: &Context) -> String {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));

    let count = |state: Applies| sorted.iter().filter(|r| r.applies == state).count();
    let mut out = String::new();
    out.push_str(&format!("# Selection: {}\n", describe(context)));
    out.push_str(&format!(
        "applies {}, undecided {}, outside the selection {} \
         (an undecided claim is never reported as excluded)\n",
        count(Applies::Yes),
        count(Applies::Unknown),
        count(Applies::No),
    ));

    for (state, title) in [(Applies::Yes, "Applies"), (Applies::Unknown, "Undecided")] {
        let matching: Vec<&&Row> = sorted.iter().filter(|r| r.applies == state).collect();
        out.push_str(&format!("\n## {title} ({})\n", matching.len()));
        for row in matching {
            out.push_str(&format!(
                "- {}  {}  {}  {}\n",
                row.claim_id,
                short(&row.address),
                row.modality,
                row.statement,
            ));
            if row.selectors.is_empty() {
                out.push_str("    selector: (none — applies to every variant)\n");
            } else {
                out.push_str(&format!("    selector: {}\n", row.selectors.join("; ")));
            }
            for (kind, to) in &row.relations {
                out.push_str(&format!("    -> {kind} {to}\n"));
            }
        }
    }

    let outside: Vec<&&Row> = sorted.iter().filter(|r| r.applies == Applies::No).collect();
    out.push_str(&format!("\n## Outside the selection ({})\n", outside.len()));
    if outside.is_empty() {
        out.push_str("(none)\n");
    } else {
        // ADR 0049 decision 7: a withdrawn claim is named with its successors on a line of its own,
        // and only when some claim is withdrawn, so a corpus without `supersedes` prints as before.
        let (withdrawn, plain): (Vec<&&Row>, Vec<&&Row>) =
            outside.iter().partition(|r| !r.superseded_by.is_empty());
        if !plain.is_empty() {
            let ids: Vec<&str> = plain.iter().map(|r| r.claim_id.as_str()).collect();
            out.push_str(&format!("{}\n", ids.join(", ")));
        }
        for row in withdrawn {
            out.push_str(&format!("{}  {}\n", row.claim_id, not_applying(row)));
        }
    }
    out
}

/// The traceability matrix, selection-scoped. Returns the view and what it could not
/// settle.
pub fn trace(rows: &[Row], context: &Context) -> (String, TraceFindings) {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    let known: std::collections::BTreeSet<&str> =
        sorted.iter().map(|r| r.claim_id.as_str()).collect();
    let state_of = |id: &str| sorted.iter().find(|r| r.claim_id == id).map(|r| r.applies);

    let mut findings = TraceFindings::default();
    let mut out = String::new();
    out.push_str(&format!("# Traceability: {}\n", describe(context)));
    out.push_str("edges are stored once, on the outgoing side (REQ-0020); the incoming\n");
    out.push_str("view below is derived by traversal, never stored\n");

    out.push_str("\n## Edges\n");
    let mut edges = 0usize;
    for row in &sorted {
        for (kind, to) in &row.relations {
            edges += 1;
            if !known.contains(to.as_str()) {
                findings.dangling.push(format!(
                    "{} -> {to} (referenced by {}, not in the corpus)",
                    kind, row.claim_id
                ));
                out.push_str(&format!(
                    "- {} -{kind}-> {to}  UNVERIFIABLE: no such claim\n",
                    row.claim_id
                ));
                continue;
            }
            match state_of(to) {
                Some(Applies::Unknown) | Some(Applies::No) => {
                    let target = state_of(to).expect("known");
                    if target == Applies::Unknown {
                        findings.undecided_edges.push(format!(
                            "{} -{kind}-> {to} (target undecided)",
                            row.claim_id
                        ));
                        out.push_str(&format!(
                            "- {} -{kind}-> {to}  [target undecided]\n",
                            row.claim_id
                        ));
                    } else if let Some(withdrawn) = sorted
                        .iter()
                        .find(|r| r.claim_id == *to && !r.superseded_by.is_empty())
                    {
                        out.push_str(&format!(
                            "- {} -{kind}-> {to}  [target {}]\n",
                            row.claim_id,
                            not_applying(withdrawn)
                        ));
                    } else {
                        out.push_str(&format!(
                            "- {} -{kind}-> {to}  [target outside the selection]\n",
                            row.claim_id
                        ));
                    }
                }
                _ => out.push_str(&format!("- {} -{kind}-> {to}\n", row.claim_id)),
            }
        }
    }
    if edges == 0 {
        out.push_str("(none)\n");
    }

    out.push_str("\n## Incoming (derived)\n");
    let mut any_incoming = false;
    for row in &sorted {
        let incoming: Vec<String> = sorted
            .iter()
            .flat_map(|other| {
                other
                    .relations
                    .iter()
                    .filter(|(_, to)| to == &row.claim_id)
                    .map(move |(kind, _)| format!("{kind}: {}", other.claim_id))
            })
            .collect();
        if !incoming.is_empty() {
            any_incoming = true;
            out.push_str(&format!("- {} <- {}\n", row.claim_id, incoming.join(", ")));
        }
    }
    if !any_incoming {
        out.push_str("(none)\n");
    }

    out.push_str("\n## Findings\n");
    if findings.dangling.is_empty() && findings.undecided_edges.is_empty() {
        out.push_str("(none)\n");
    }
    for line in &findings.dangling {
        out.push_str(&format!("- UNVERIFIABLE: {line}\n"));
    }
    for line in &findings.undecided_edges {
        out.push_str(&format!("- undecided edge: {line}\n"));
    }
    (out, findings)
}

/// How a text view says a *withdrawn* claim does not apply (ADR 0049 decision 7): it names its
/// successors. Callers use it only for a row whose `superseded_by` is non-empty; every other claim
/// keeps the plain wording the view already had.
pub fn not_applying(row: &Row) -> String {
    format!(
        "does not apply (superseded by {})",
        row.superseded_by.join(", ")
    )
}

fn describe(context: &Context) -> String {
    if context.is_empty() {
        return "(no context: nothing is decided by the caller)".to_string();
    }
    context
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn short(address: &str) -> String {
    address.chars().take(12).collect()
}

// ---------------------------------------------------------------------------
// JSON documents (pack C)
// ---------------------------------------------------------------------------
//
// One row shape for both views, built from the real [`Row`] — not a second struct a field
// could be added to without the other rendering noticing. `pins`, `relations_hash` and
// `rationale_hash` are what the gate and the audit surface *judge*, not what a consumer reads,
// so they stay out of the document; the rendering's contract is exactly the fields below.

/// One outgoing relation in the document's shape.
#[derive(Serialize)]
struct RelationDocument<'a> {
    relation: &'a str,
    to: &'a str,
}

/// The provenance a row exposes: source and date, the same two the text view prints.
#[derive(Serialize)]
struct RationaleDocument<'a> {
    source: &'a str,
    date: &'a str,
}

impl Serialize for Row {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let relations: Vec<RelationDocument<'_>> = self
            .relations
            .iter()
            .map(|(kind, to)| RelationDocument { relation: kind, to })
            .collect();
        let rationale = RationaleDocument {
            source: &self.rationale_source,
            date: &self.rationale_date,
        };
        let mut row = serializer.serialize_struct("Row", 10)?;
        row.serialize_field("claim_id", &self.claim_id)?;
        row.serialize_field("statement", &self.statement)?;
        row.serialize_field("modality", &self.modality)?;
        // The address is the loaded claim's own address, rendered with the algorithm that
        // derives it — never re-derived or hand-written here.
        row.serialize_field("address", &format!("sha256:{}", self.address))?;
        row.serialize_field("applies", self.applies.as_str())?;
        row.serialize_field("selectors", &self.selectors)?;
        row.serialize_field("relations", &relations)?;
        row.serialize_field("rationale", &rationale)?;
        row.serialize_field("superseded_by", &self.superseded_by)?;
        // ADR 0051: additive, after `superseded_by` (the ADR 0049 precedent); `null` when absent, so a
        // consumer never has to tell a missing key from an unfiled claim.
        row.serialize_field("area", &self.area)?;
        row.end()
    }
}

/// `render`'s document. The three sections keep the text path's separation: an undecided claim
/// is in `undecided`, never in `applies`.
#[derive(Serialize)]
struct RenderDocument<'a> {
    schema: &'static str,
    generator: &'static str,
    rules_v: u32,
    selection: &'a Context,
    applies: Vec<&'a Row>,
    undecided: Vec<&'a Row>,
    outside: Vec<&'a Row>,
    identity: String,
}

/// One edge in the document's shape — used for both directions, because they are the same edge
/// seen from the storing side and the derived side.
#[derive(Serialize)]
struct EdgeDocument<'a> {
    from: &'a str,
    relation: &'a str,
    to: &'a str,
}

/// One claim as `trace`'s document names it (ADR 0046 decision 2): every claim in the corpus is a
/// node, including one with no relation, so a renderer holds the whole graph. The field order is
/// the document's contract and the writer keeps it.
#[derive(Serialize)]
struct NodeDocument<'a> {
    claim_id: &'a str,
    statement: &'a str,
    modality: &'a str,
    address: String,
    applies: &'static str,
    superseded_by: &'a [String],
    area: Option<&'a str>,
}

/// One line of `.req/areas` as `trace`'s document names it (ADR 0051 decision 2, unit A-2): the area's path and the
/// reader-facing label a renderer shows in place of the key. `path` then `label` is the field order.
#[derive(Serialize)]
struct AreaDocument<'a> {
    path: &'a str,
    label: &'a str,
}

/// `trace`'s document: the nodes (every claim), the registry's areas in its line order, the outgoing edges, the derived
/// incoming edges (REQ-0020 stores each relation once), and the findings.
#[derive(Serialize)]
struct TraceDocument<'a> {
    schema: &'static str,
    generator: &'static str,
    rules_v: u32,
    selection: &'a Context,
    nodes: Vec<NodeDocument<'a>>,
    areas: Vec<AreaDocument<'a>>,
    edges: Vec<EdgeDocument<'a>>,
    derived_incoming: Vec<EdgeDocument<'a>>,
    findings: &'a TraceFindings,
}

/// The rows a view sorts for both renderings: claim id order, whatever order the loader used.
fn sorted_rows(rows: &[Row]) -> Vec<&Row> {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    sorted
}

/// The `render` document. The identity is [`identity_digest`] over the applying addresses — the
/// same digest the committed projection's header carries, so the two artefacts cannot disagree
/// about what was released.
pub fn render_document(rows: &[Row], context: &Context) -> String {
    let sorted = sorted_rows(rows);
    let section = |state: Applies| -> Vec<&Row> {
        sorted
            .iter()
            .copied()
            .filter(|row| row.applies == state)
            .collect()
    };
    let addresses: Vec<String> = sorted
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| row.address.clone())
        .collect();
    let document = RenderDocument {
        schema: SCHEMA_RENDER,
        generator: "render",
        rules_v: RULES_V,
        selection: context,
        applies: section(Applies::Yes),
        undecided: section(Applies::Unknown),
        outside: section(Applies::No),
        identity: format!("sha256:{}", identity_digest(&addresses)),
    };
    format::document(&document)
}

/// The `trace` document, and the findings the plain verdict folds into its exit code. The
/// findings come from [`trace`] — the same call the text path makes — so the document cannot
/// classify an edge differently from the report beside it. `areas` is the validated `.req/areas`, in
/// its line order (`&[]` with no registry): the document carries it so a renderer that reads the
/// document and nothing else can name an area by its label (ADR 0051 decision 2).
pub fn trace_document(
    rows: &[Row],
    context: &Context,
    areas: &[AreaEntry],
) -> (String, TraceFindings) {
    let (_, findings) = trace(rows, context);
    let sorted = sorted_rows(rows);

    let nodes: Vec<NodeDocument<'_>> = sorted
        .iter()
        .map(|row| NodeDocument {
            claim_id: &row.claim_id,
            statement: &row.statement,
            modality: &row.modality,
            // The address is spelled as the other documents spell it (see `impl Serialize for Row`).
            address: format!("sha256:{}", row.address),
            applies: row.applies.as_str(),
            superseded_by: &row.superseded_by,
            area: row.area.as_deref(),
        })
        .collect();

    let mut edges = Vec::new();
    for row in &sorted {
        for (kind, to) in &row.relations {
            edges.push(EdgeDocument {
                from: &row.claim_id,
                relation: kind,
                to,
            });
        }
    }
    // The reverse view is derived by traversal, exactly as the text matrix derives it: each
    // stored relation contributes one incoming edge to its target.
    let mut derived_incoming = Vec::new();
    for target in &sorted {
        for other in &sorted {
            for (kind, to) in &other.relations {
                if to == &target.claim_id {
                    derived_incoming.push(EdgeDocument {
                        from: &other.claim_id,
                        relation: kind,
                        to: &target.claim_id,
                    });
                }
            }
        }
    }

    let document = TraceDocument {
        schema: SCHEMA_TRACE,
        generator: "trace",
        rules_v: RULES_V,
        selection: context,
        nodes,
        areas: areas
            .iter()
            .map(|entry| AreaDocument {
                path: &entry.path,
                label: &entry.label,
            })
            .collect(),
        edges,
        derived_incoming,
        findings: &findings,
    };
    (format::document(&document), findings)
}

// ---------------------------------------------------------------------------
// Committed projections (ADR 0018)
// ---------------------------------------------------------------------------

/// What produced a projection: the two commands are two artefacts, and a check made with the
/// other one would compare two different documents.
const GENERATOR_RENDER: &str = "req render";
const GENERATOR_TRACE: &str = "req trace";

/// The header grammar. The line is an HTML comment so it disappears when the markdown is
/// rendered, and the fields are ` | `-separated so a reader (or an agent) can grep one axis.
const HEADER_OPEN: &str = "<!-- req projection: ";
const HEADER_CLOSE: &str = " -->";
const HEADER_FIELDS: [&str; 5] = ["generator", "rules_v", "selection", "claims", "identity"];

/// `sha256(concat(sorted(addresses).map(|a| a + "\n")))`, lowercase hex.
///
/// The definition is pinned by a hand-computed fixture in `tests/projection.rs`; it is an
/// artefact of the projection, never a claim address, so it moves no identity and no `RULES_V`.
/// The sort is explicit: a caller hands whichever order it found the rows in.
pub fn identity_digest(addresses: &[String]) -> String {
    let mut sorted: Vec<&str> = addresses.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let mut input = String::new();
    for address in sorted {
        input.push_str(address);
        input.push('\n');
    }
    sha256_hex(input.as_bytes())
}

/// The canonical spelling of a context in a header: keys sorted (the `Context` is a
/// `BTreeMap`), `k=v` pairs joined by spaces, or the explicit `none` when the caller decided
/// nothing. `None` when a key or value carries whitespace: that spelling could not be parsed
/// back, and writing a header the check cannot reproduce would be a lie.
fn selection_text(context: &Context) -> Option<String> {
    if context.is_empty() {
        return Some("none".to_string());
    }
    let mut out = String::new();
    for (key, value) in context {
        if key.is_empty()
            || key.contains(char::is_whitespace)
            || value.contains(char::is_whitespace)
        {
            return None;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(key);
        out.push('=');
        out.push_str(value);
    }
    Some(out)
}

/// The inverse of [`selection_text`], for `--check`: the header's selection must be the
/// canonical spelling of a context this build can reproduce, or the question cannot be judged.
fn context_from_selection(selection: &str) -> Result<Context, String> {
    if selection == "none" {
        return Ok(Context::new());
    }
    if selection.is_empty() {
        return Err("it is empty (`none` is the explicit no-context spelling)".to_string());
    }
    let mut context = Context::new();
    for pair in selection.split(' ') {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(format!("`{pair}` is not a `key=value` pair"));
        };
        if key.is_empty() {
            return Err(format!("`{pair}` has an empty key"));
        }
        if context.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("`{key}` is named more than once"));
        }
    }
    match selection_text(&context) {
        Some(canonical) if canonical == selection => Ok(context),
        Some(canonical) => Err(format!(
            "`{selection}` is not the canonical spelling (`{canonical}`)"
        )),
        None => Err("a key or value carries whitespace".to_string()),
    }
}

/// The header `--out` writes, or `None` when the invocation's selection cannot be spelled in a
/// header (a value with whitespace) — writing an artefact that cannot be checked would be worse
/// than refusing.
fn projection_header(generator: &str, rows: &[Row], context: &Context) -> Option<String> {
    let selection = selection_text(context)?;
    let addresses: Vec<String> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes)
        .map(|row| row.address.clone())
        .collect();
    Some(format!(
        "{HEADER_OPEN}generator={generator} | rules_v={RULES_V} | selection={selection} | claims={} | identity=sha256:{} -->",
        addresses.len(),
        identity_digest(&addresses),
    ))
}

/// A parsed projection header. Only the fields a check needs are kept; the digest is printed
/// back to the reader but never used as the oracle (ADR 0018 §3).
struct ProjectionHeader {
    generator: String,
    rules_v: u32,
    selection: String,
    claims: usize,
    identity: String,
}

/// Parse the first line of a committed projection, naming what is missing when it is not one.
fn parse_header(line: &str) -> Result<ProjectionHeader, String> {
    let inner = line
        .strip_prefix(HEADER_OPEN)
        .ok_or_else(|| format!("it must start with `{HEADER_OPEN}`"))?;
    let inner = inner
        .strip_suffix(HEADER_CLOSE)
        .ok_or_else(|| format!("it must end with `{HEADER_CLOSE}`"))?;
    let fields: Vec<&str> = inner.split(" | ").collect();
    if fields.len() != HEADER_FIELDS.len() {
        return Err(format!(
            "it must carry {} fields ({}) separated by ` | `, found {}",
            HEADER_FIELDS.len(),
            HEADER_FIELDS.join(", "),
            fields.len()
        ));
    }
    let mut values = Vec::with_capacity(fields.len());
    for (field, name) in fields.iter().zip(HEADER_FIELDS) {
        let value = field
            .strip_prefix(&format!("{name}="))
            .ok_or_else(|| format!("the field `{field}` must be `{name}=…`"))?;
        values.push(value);
    }
    let generator = values[0];
    if generator != GENERATOR_RENDER && generator != GENERATOR_TRACE {
        return Err(format!(
            "`generator={generator}` is not `{GENERATOR_RENDER}` or `{GENERATOR_TRACE}`"
        ));
    }
    let rules_v: u32 = values[1]
        .parse()
        .map_err(|_| format!("`rules_v={}` is not a decimal version", values[1]))?;
    let claims: usize = values[3]
        .parse()
        .map_err(|_| format!("`claims={}` is not a claim count", values[3]))?;
    let identity = values[4];
    let hex = identity
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("`identity={identity}` is not `sha256:<64 lowercase hex>`"))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "`identity={identity}` is not `sha256:<64 lowercase hex>`"
        ));
    }
    Ok(ProjectionHeader {
        generator: generator.to_string(),
        rules_v,
        selection: values[2].to_string(),
        claims,
        identity: identity.to_string(),
    })
}

/// Where a stored projection and a fresh one first disagree, by line — so the reader can tell
/// whether the corpus moved or the file was edited.
fn first_difference(stored: &str, fresh: &str) -> String {
    let stored_lines: Vec<&str> = stored.lines().collect();
    let fresh_lines: Vec<&str> = fresh.lines().collect();
    for (index, (stored_line, fresh_line)) in
        stored_lines.iter().zip(fresh_lines.iter()).enumerate()
    {
        if stored_line != fresh_line {
            return format!(
                "line {} differs\n  projection: {stored_line}\n  fresh:      {fresh_line}",
                index + 1
            );
        }
    }
    if stored_lines.len() < fresh_lines.len() {
        format!(
            "the projection ends at line {}; a fresh projection continues with\n  {}",
            stored_lines.len(),
            fresh_lines[stored_lines.len()]
        )
    } else if stored_lines.len() > fresh_lines.len() {
        format!(
            "the projection has {} line(s) a fresh projection does not; line {} is\n  {}",
            stored_lines.len() - fresh_lines.len(),
            fresh_lines.len() + 1,
            stored_lines[fresh_lines.len()]
        )
    } else if stored.replace("\r\n", "\n") == fresh.replace("\r\n", "\n") {
        // `str::lines()` strips a trailing `\r`, so a CRLF checkout reaches this branch with
        // every line agreeing; naming a missing final newline there would blame the wrong byte.
        "the lines agree but their line endings differ (CRLF vs LF)".to_string()
    } else {
        "the lines agree but the trailing bytes differ (a missing final newline)".to_string()
    }
}

/// One invocation's shape: the plain print, or one of the two committed-projection questions.
enum Mode {
    Plain,
    Out(PathBuf),
    Check(PathBuf),
}

struct Invocation {
    mode: Mode,
    context: Context,
    format: Format,
}

/// Parse the argv of `render`/`trace`. A help flag never reaches here: `main.rs` answers it
/// before the handler, wherever it appears.
fn parse_invocation(args: &[String], command: &str) -> Result<Invocation, i32> {
    let usage = || {
        format!(
            "usage: req {command} [--out <path>] [--check <path>] [--format text|json] \
             [key=value ...]"
        )
    };
    // The format flag is stripped first, by the shared parser, so its own errors and the rest of
    // the command's grammar are read in one place each.
    let (format, args) = format::take_format(args, &usage())?;
    let mut out: Option<PathBuf> = None;
    let mut check: Option<PathBuf> = None;
    let mut context = Context::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--out" || arg == "--check" {
            let Some(value) = args.get(index + 1) else {
                eprintln!("{}\n`{arg}` needs a path", usage());
                return Err(2);
            };
            // An option-shaped value is a missing value, not a file named like a flag: writing
            // to `--check`, or to `-x` after a `--out` typo, would be a silent answer to a
            // question nobody asked. Paths beginning with `-` are not supported on purpose.
            if value.is_empty() || value.starts_with('-') {
                eprintln!("{}\n`{arg}` needs a path, not {value:?}", usage());
                return Err(2);
            }
            if out.is_some() || check.is_some() {
                eprintln!(
                    "{}\n`--out` and `--check` are one question each: give one of them, once",
                    usage()
                );
                return Err(2);
            }
            if arg == "--out" {
                out = Some(PathBuf::from(value));
            } else {
                check = Some(PathBuf::from(value));
            }
            index += 2;
            continue;
        }
        // An option-shaped argument is a usage error, never a context pair. Without this,
        // `--out=spec.md` would be read as the selection `--out == spec.md`: the invocation
        // asked for a *write*, got a plain print, and was answered with exit 0 — the "written"
        // class as a false green, and `--check=spec.md` the same for "current".
        if arg.starts_with('-') {
            eprintln!("{}\nunrecognized flag: {arg}", usage());
            return Err(2);
        }
        match arg.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                context.insert(key.to_string(), value.to_string());
            }
            _ => {
                eprintln!("{}", usage());
                return Err(2);
            }
        }
        index += 1;
    }
    let mode = match (out, check) {
        (Some(path), None) => Mode::Out(path),
        (None, Some(path)) => Mode::Check(path),
        (None, None) => Mode::Plain,
        (Some(_), Some(_)) => unreachable!("the parse above refuses both flags"),
    };
    // v1 renders JSON to stdout only. A committed projection's header contract was designed for
    // text, and pretending it already works for JSON would be a half-feature; the error says
    // exactly where the extension point is.
    if format == Format::Json && !matches!(mode, Mode::Plain) {
        eprintln!(
            "{}\n`--format json` writes to stdout only: a JSON projection on disk with \
             `--out`/`--check` is a follow-on, not this version",
            usage()
        );
        return Err(2);
    }
    Ok(Invocation {
        mode,
        context,
        format,
    })
}

/// The view and whether its plain verdict is open (unsettled). `--out`/`--check` do not use the
/// second value: the artefact's content is complete even when the verdict is open, and the exit
/// class belongs to the verdict (fact 5).
fn project(generator: &str, rows: &[Row], context: &Context) -> (String, bool) {
    if generator == GENERATOR_TRACE {
        let (view, findings) = trace(rows, context);
        let open = !findings.dangling.is_empty() || !findings.undecided_edges.is_empty();
        (view, open)
    } else {
        let open = rows.iter().any(|row| row.applies == Applies::Unknown);
        (render(rows, context), open)
    }
}

/// The JSON rendering of a plain view. The open/settled answer is derived from the same findings
/// the document carries, so the exit code is the verdict's and never the rendering's.
fn json_document(
    generator: &str,
    rows: &[Row],
    context: &Context,
    areas: &[AreaEntry],
) -> (String, bool) {
    if generator == GENERATOR_TRACE {
        let (document, findings) = trace_document(rows, context, areas);
        let open = !findings.dangling.is_empty() || !findings.undecided_edges.is_empty();
        (format!("{document}\n"), open)
    } else {
        let open = rows.iter().any(|row| row.applies == Applies::Unknown);
        (format!("{}\n", render_document(rows, context)), open)
    }
}

/// `.req/areas`, validated, in its line order: `Ok(vec![])` when the file does not exist. A malformed registry is
/// every bad `<file>:<line>` (one parser, [`parse_areas`], as `req check` reads it); one that exists and cannot be
/// used as a list is the reason, as `req check` names it.
fn read_areas(root: &Path) -> Result<Vec<AreaEntry>, Vec<String>> {
    match read_list(root, AREAS) {
        Listing::Absent => Ok(Vec::new()),
        Listing::Unreadable(reason) => Err(vec![reason]),
        Listing::Text(text) => parse_areas(&text),
    }
}

/// `req render [--out <path>] [--check <path>] [--format text|json] [key=value ...]`
pub fn run_render(args: &[String], root: &Path) -> i32 {
    run_view(args, root, GENERATOR_RENDER)
}

/// `req trace [--out <path>] [--check <path>] [--format text|json] [key=value ...]`
pub fn run_trace(args: &[String], root: &Path) -> i32 {
    run_view(args, root, GENERATOR_TRACE)
}

fn run_view(args: &[String], root: &Path, generator: &str) -> i32 {
    let command = if generator == GENERATOR_TRACE {
        "trace"
    } else {
        "render"
    };
    let invocation = match parse_invocation(args, command) {
        Ok(invocation) => invocation,
        Err(code) => return code,
    };
    match &invocation.mode {
        Mode::Plain => {
            // `trace --format json` carries the registry (ADR 0051): a malformed or unreadable one is a usage
            // error, decided before anything is printed. Text `trace` and `render` never read it.
            let areas = if generator == GENERATOR_TRACE && invocation.format == Format::Json {
                match read_areas(root) {
                    Ok(areas) => areas,
                    Err(errors) => {
                        for error in &errors {
                            eprintln!("{error}");
                        }
                        eprintln!(
                            "nothing was printed: a malformed `{AREAS}` is refused, never read around"
                        );
                        return 2;
                    }
                }
            } else {
                Vec::new()
            };
            let rows = match load_rows(&invocation.context, root) {
                Ok(rows) => rows,
                Err(error) => {
                    eprintln!("[UNVERIFIABLE] {error}");
                    return 3;
                }
            };
            // The plain verdict is computed once, for whichever rendering is asked for: the
            // document and the exit code cannot disagree about whether the view is settled.
            let (output, open) = match invocation.format {
                Format::Text => project(generator, &rows, &invocation.context),
                Format::Json => json_document(generator, &rows, &invocation.context, &areas),
            };
            print!("{output}");
            if open { 3 } else { 0 }
        }
        Mode::Out(path) => {
            let rows = match load_rows(&invocation.context, root) {
                Ok(rows) => rows,
                Err(error) => {
                    eprintln!("[UNVERIFIABLE] {error}");
                    return 3;
                }
            };
            let Some(header) = projection_header(generator, &rows, &invocation.context) else {
                eprintln!(
                    "[UNVERIFIABLE] cannot write {path}: the selection carries whitespace, so \
                     it cannot be spelled in a projection header",
                    path = path.display()
                );
                return 3;
            };
            let (view, _) = project(generator, &rows, &invocation.context);
            // A header this build cannot parse back is never written: the check would have no
            // way to judge the artefact, and a projection nobody can check is decoration.
            let Ok(parsed) = parse_header(&header) else {
                eprintln!(
                    "[UNVERIFIABLE] cannot write {}: the header this build would write does not \
                     parse back",
                    path.display()
                );
                return 3;
            };
            let text = format!("{header}\n{view}");
            match std::fs::write(path, text) {
                Ok(()) => {
                    println!(
                        "[WROTE] {} ({} applying claim(s), {})",
                        path.display(),
                        parsed.claims,
                        parsed.identity
                    );
                    0
                }
                Err(error) => {
                    eprintln!("[UNVERIFIABLE] cannot write {}: {error}", path.display());
                    3
                }
            }
        }
        Mode::Check(path) => run_check(generator, command, path, &invocation.context, root),
    }
}

/// `--check`: compare a committed projection against a fresh one, and write nothing.
///
/// The order is the decision: a file that is not a projection, another build's `rules_v`, a
/// selection this build cannot reproduce and another command's artefact are all **3** — the
/// check cannot compare two different things — while a byte difference is **1**, with the first
/// differing line named. The header's digest is never compared alone.
fn run_check(
    generator: &str,
    command: &str,
    path: &Path,
    invocation_context: &Context,
    root: &Path,
) -> i32 {
    let judged = check_projection(generator, command, path, invocation_context, root);
    print!("{}", judged.stdout);
    eprint!("{}", judged.stderr);
    judged.code
}

/// What one `--check` decided, without printing it: the exit, and the lines it would have written
/// to each stream. `req check` runs the same judgement for each listed projection (ADR 0023 §4)
/// and reports the lines under the step, so a stale file still names its first differing line.
pub struct ProjectionCheck {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `req render --check <path>`'s judgement, with no context named: the projection is judged under
/// its own recorded selection, exactly as the documented CI line does. The one call `req check`
/// makes per listed projection.
pub fn check_render_projection(path: &Path, root: &Path) -> ProjectionCheck {
    check_projection(GENERATOR_RENDER, "render", path, &Context::new(), root)
}

/// The judgement of one projection **listed in `.req/projections`**, by whichever command made it
/// (ADR 0018 amended): the generator its header names picks `render --check` or `trace --check`, each
/// judging the file under its own recorded selection, and the kind (`"render"` or `"trace"`) names the
/// step. A file whose header cannot be read at all falls to `render`'s judgement, which says why it is
/// not a projection (3) in the words it always used.
pub fn check_listed_projection(path: &Path, root: &Path) -> (&'static str, ProjectionCheck) {
    let generator = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.lines().next().and_then(|line| parse_header(line).ok()))
        .map(|header| header.generator);
    match generator.as_deref() {
        Some(GENERATOR_TRACE) => (
            "trace",
            check_projection(GENERATOR_TRACE, "trace", path, &Context::new(), root),
        ),
        _ => ("render", check_render_projection(path, root)),
    }
}

fn check_projection(
    generator: &str,
    command: &str,
    path: &Path,
    invocation_context: &Context,
    root: &Path,
) -> ProjectionCheck {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let code = judge_projection(
        generator,
        command,
        path,
        invocation_context,
        root,
        &mut stdout,
        &mut stderr,
    );
    ProjectionCheck {
        code,
        stdout,
        stderr,
    }
}

fn judge_projection(
    generator: &str,
    command: &str,
    path: &Path,
    invocation_context: &Context,
    root: &Path,
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    // The same lines the command always printed, collected: a `println!`/`eprintln!` in this body
    // appends to the stream it names.
    macro_rules! println {
        ($($arg:tt)*) => {{
            stdout.push_str(&format!($($arg)*));
            stdout.push('\n');
        }};
    }
    macro_rules! eprintln {
        ($($arg:tt)*) => {{
            stderr.push_str(&format!($($arg)*));
            stderr.push('\n');
        }};
    }
    let stored = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!(
                "[UNVERIFIABLE] cannot read {}: {error}\nthis path is not a projection the \
                 check can judge (a listed projection that is missing is a defect, not a pass)",
                path.display()
            );
            return 3;
        }
    };
    let Some(first_line) = stored.lines().next().filter(|line| !line.trim().is_empty()) else {
        eprintln!(
            "[UNVERIFIABLE] {} is not a projection: it has no header (write one with \
             `{command} --out {}`)",
            path.display(),
            path.display()
        );
        return 3;
    };
    let header = match parse_header(first_line) {
        Ok(header) => header,
        Err(reason) => {
            eprintln!(
                "[UNVERIFIABLE] {} is not a projection: its header is not the grammar this \
                 build writes — {reason}\nwrite one with `{command} --out {}`; a projection is \
                 generated, never hand-written",
                path.display(),
                path.display()
            );
            return 3;
        }
    };
    if header.rules_v != RULES_V {
        eprintln!(
            "[UNVERIFIABLE] {} was written under rules_v={}, this build writes rules_v={RULES_V}: \
             its addresses mean something else, so the comparison is void rather than failed — \
             regenerate with `{command} --out {}`",
            path.display(),
            header.rules_v,
            path.display()
        );
        return 3;
    }
    if header.generator != generator {
        let other = if header.generator == GENERATOR_TRACE {
            "trace"
        } else {
            "render"
        };
        eprintln!(
            "[UNVERIFIABLE] {} is a `{}` projection; `{command}` cannot judge it — run \
             `{other} --check {}`",
            path.display(),
            header.generator,
            path.display()
        );
        return 3;
    }
    let context = match context_from_selection(&header.selection) {
        Ok(context) => context,
        Err(reason) => {
            eprintln!(
                "[UNVERIFIABLE] {}: the header's selection cannot be reproduced: {reason}",
                path.display()
            );
            return 3;
        }
    };
    // One question per invocation: the artefact records the selection it was built under, so an
    // explicit context that names a *different* one is a second answer, never an override. No
    // context at all is not a claim of `none` — it means "judge it under its own selection",
    // which is the shape the documented CI line uses.
    if !invocation_context.is_empty() {
        match selection_text(invocation_context) {
            Some(named) if named == header.selection => {}
            Some(named) => {
                eprintln!(
                    "[UNVERIFIABLE] {} was built under selection `{}`, but the invocation names \
                     `{named}`; drop the context arguments and check the projection against its \
                     own selection",
                    path.display(),
                    header.selection
                );
                return 3;
            }
            None => {
                eprintln!(
                    "[UNVERIFIABLE] the invocation's selection carries whitespace, so it cannot \
                     match a projection header"
                );
                return 3;
            }
        }
    }
    let rows = match load_rows(&context, root) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("[UNVERIFIABLE] {error}");
            return 3;
        }
    };
    let Some(fresh_header) = projection_header(generator, &rows, &context) else {
        eprintln!("[UNVERIFIABLE] the corpus's projection header cannot be reproduced");
        return 3;
    };
    let (fresh_view, _) = project(generator, &rows, &context);
    let fresh = format!("{fresh_header}\n{fresh_view}");
    if fresh == stored {
        println!(
            "[CURRENT] {} ({} applying claim(s), {})",
            path.display(),
            header.claims,
            header.identity
        );
        return 0;
    }
    eprintln!(
        "[STALE] {} differs from a fresh projection of this corpus: {}\nregenerate with \
         `{command} --out {}`; a committed projection is checked, never edited",
        path.display(),
        first_difference(&stored, &fresh),
        path.display()
    );
    1
}

/// The shared row set every view is a projection of. The root is explicit: the corpus a
/// command reads is resolved once at the CLI boundary (ADR 0012), never from a build path.
pub fn load_rows(context: &Context, root: &std::path::Path) -> Result<Vec<Row>, String> {
    let claims = crate::canon::load_claims(root)?;
    let mut rows = Vec::new();
    for loaded in claims.iter() {
        let claim = &loaded.claim;
        // The address is the one derived at load (ADR 0022): every view, the gate and the
        // Baseline read the same bytes, and none is stored to disagree with.
        let derived = &loaded.derived;
        let predicates: Vec<_> = claim
            .applicability
            .iter()
            .map(|p| parse_predicate(p))
            .collect();
        rows.push(Row {
            claim_id: claim.claim_id.clone(),
            statement: claim.statement.replace('\n', " "),
            modality: derived.modality_label().to_string(),
            address: derived.address.clone(),
            applies: applies(&predicates, context),
            selectors: claim.applicability.clone(),
            relations: claim
                .relations
                .iter()
                .map(|r| (r.kind.clone(), r.to.clone()))
                .collect(),
            relations_hash: crate::model::relations_digest(&claim.relations),
            rationale_hash: crate::model::rationale_digest(&claim.rationale),
            tests: claim.tests.clone(),
            evidence: claim.evidence,
            tests_hash: crate::model::tests_digest(&claim.tests, claim.evidence),
            pins: derived.pins.clone(),
            rationale_source: claim.rationale.source.clone(),
            rationale_date: claim.rationale.date.clone(),
            superseded_by: Vec::new(),
            area: claim.area.clone(),
        });
    }
    withdraw_superseded(&mut rows);
    Ok(rows)
}

/// The relation kind ADR 0049 gives a meaning: authored on the **successor**, pointing at the claim
/// it replaces.
pub const SUPERSEDES: &str = "supersedes";

/// What the `supersedes` relations of a corpus say, computed once from the rows and shared by the
/// loader (which folds it), the gate (which fails a cycle and names what decides an undecided
/// claim) and every text view (ADR 0049 decisions 2 and 5).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Supersession {
    /// Target id -> the ids that supersede it. A member of a cycle is excluded as a successor and as
    /// a target, and an edge whose `to` the corpus does not hold is not here (it withdraws nothing).
    pub successors: BTreeMap<String, BTreeSet<String>>,
    /// Cycles, self-loops included, each named once and rotated to start at its lowest id
    /// (`[REQ-A, REQ-B]` is `REQ-A -> REQ-B -> REQ-A`), sorted. Every member of a cycle appears in at
    /// least one of them; a component with chords may hold cycles that are not named. A cycle is a
    /// defect, never a withdrawal.
    pub cycles: Vec<Vec<String>>,
}

/// Read the `supersedes` relations of `rows`, which hold the whole corpus (not one selection's
/// applying claims): a cycle is a property of the corpus, so it is found in every selection.
///
/// Deterministic: ordered maps and sets throughout, and the shortest cycle through a member is found
/// by a breadth-first search that takes neighbours in id order.
pub fn supersession(rows: &[Row]) -> Supersession {
    let known: BTreeSet<&str> = rows.iter().map(|row| row.claim_id.as_str()).collect();
    // `edges[a]` = the claims `a` supersedes, as a successor-to-target graph.
    let mut edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for row in rows {
        for (kind, to) in &row.relations {
            if kind == SUPERSEDES && known.contains(to.as_str()) {
                edges
                    .entry(row.claim_id.as_str())
                    .or_default()
                    .insert(to.as_str());
            }
        }
    }

    // A claim is on a cycle exactly when a path leads from it back to itself. The shortest such
    // path is the cycle that is named for it.
    let mut members: BTreeSet<&str> = BTreeSet::new();
    let mut named: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut covered: BTreeSet<&str> = BTreeSet::new();
    for start in edges.keys().copied() {
        let Some(path) = shortest_cycle(&edges, start) else {
            continue;
        };
        members.insert(start);
        if covered.contains(start) {
            continue;
        }
        covered.extend(path.iter().copied());
        let lowest = path
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.cmp(b))
            .map(|(at, _)| at)
            .unwrap_or(0);
        let mut rotated: Vec<String> = path[lowest..].iter().map(|id| (*id).to_string()).collect();
        rotated.extend(path[..lowest].iter().map(|id| (*id).to_string()));
        named.insert(rotated);
    }

    let mut successors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (from, targets) in &edges {
        if members.contains(from) {
            continue;
        }
        for to in targets {
            if !members.contains(to) {
                successors
                    .entry((*to).to_string())
                    .or_default()
                    .insert((*from).to_string());
            }
        }
    }
    Supersession {
        successors,
        cycles: named.into_iter().collect(),
    }
}

/// The shortest path `start -> ... -> start` (the nodes of it, `start` first), or `None` when no edge
/// path leads back to `start`.
fn shortest_cycle<'a>(
    edges: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    start: &'a str,
) -> Option<Vec<&'a str>> {
    let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
    let mut queue: std::collections::VecDeque<&str> = std::collections::VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        for next in edges.get(current).into_iter().flatten().copied() {
            if next == start {
                let mut path = vec![current];
                let mut at = current;
                while at != start {
                    at = parent.get(at).copied()?;
                    path.push(at);
                }
                path.reverse();
                return Some(path);
            }
            if !parent.contains_key(next) {
                parent.insert(next, current);
                queue.push_back(next);
            }
        }
    }
    None
}

/// ADR 0049 decision 2: fold each claim's own answer with the **own** answer of each successor (the
/// successor's predicate, not its folded state, so a chain needs no recursion and a withdrawn B never
/// brings A back).
///
/// * a successor applies -> the claim is withdrawn: it does not apply, and names the applying
///   successors;
/// * else a successor is undecided and the claim's own answer is not *does not apply* -> undecided;
/// * else the claim keeps its own answer.
fn withdraw_superseded(rows: &mut [Row]) {
    let supersession = supersession(rows);
    if supersession.successors.is_empty() {
        return;
    }
    let own: BTreeMap<String, Applies> = rows
        .iter()
        .map(|row| (row.claim_id.clone(), row.applies))
        .collect();
    for row in rows.iter_mut() {
        let Some(successors) = supersession.successors.get(&row.claim_id) else {
            continue;
        };
        let applying: Vec<String> = successors
            .iter()
            .filter(|id| own.get(*id) == Some(&Applies::Yes))
            .cloned()
            .collect();
        if !applying.is_empty() {
            row.applies = Applies::No;
            row.superseded_by = applying;
        } else if row.applies != Applies::No
            && successors
                .iter()
                .any(|id| own.get(id) == Some(&Applies::Unknown))
        {
            row.applies = Applies::Unknown;
        }
    }
}
