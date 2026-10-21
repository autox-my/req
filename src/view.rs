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

use crate::canon::{RULES_V, address_hex, canonical_json, evaluate_claim};
use crate::digest::sha256_hex;
use crate::select::{Applies, Context, applies, parse_predicate};
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
    /// What the *stored* derived fields say, checked against a fresh parse: a release gate
    /// has to know whether the address on disk is the one the current rules produce.
    pub stored: crate::fill::StoredState,
    /// Provenance, for the audit policy and the view.
    pub rationale_source: String,
    pub rationale_date: String,
}

/// What a trace could not settle. Both are outcomes, not warnings: `dangling` is
/// UNVERIFIABLE, `undecided_edges` needs a decision from whoever owns the variant.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
                "- {}  {}  {}  {}{}\n",
                row.claim_id,
                short(&row.address),
                row.modality,
                row.statement,
                stored_marker(&row.stored)
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
        let ids: Vec<&str> = outside.iter().map(|r| r.claim_id.as_str()).collect();
        out.push_str(&format!("{}\n", ids.join(", ")));
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

/// A view that showed a stale or tampered address as if it were current would be worse than
/// no view: the stored state is part of what the reader needs to see.
fn stored_marker(stored: &crate::fill::StoredState) -> String {
    match stored {
        crate::fill::StoredState::Verified => String::new(),
        crate::fill::StoredState::Candidate(why) => format!("  [candidate: {why}]"),
        crate::fill::StoredState::Mismatch(why) => format!("  [MISMATCH: {why}]"),
    }
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
}

/// Parse the argv of `render`/`trace`. A help flag never reaches here: `main.rs` answers it
/// before the handler, wherever it appears.
fn parse_invocation(args: &[String], command: &str) -> Result<Invocation, i32> {
    let usage = || format!("usage: req {command} [--out <path>] [--check <path>] [key=value ...]");
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
            // to `--check` would be a silent answer to a question nobody asked.
            if value.is_empty() || value.starts_with("--") {
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
    Ok(Invocation { mode, context })
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

/// `req render [--out <path>] [--check <path>] [key=value ...]`
pub fn run_render(args: &[String], root: &Path) -> i32 {
    run_view(args, root, GENERATOR_RENDER)
}

/// `req trace [--out <path>] [--check <path>] [key=value ...]`
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
            let rows = match load_rows(&invocation.context, root) {
                Ok(rows) => rows,
                Err(error) => {
                    eprintln!("[UNVERIFIABLE] {error}");
                    return 3;
                }
            };
            let (view, open) = project(generator, &rows, &invocation.context);
            print!("{view}");
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
    let registry = crate::canon::load_registry(root)?;
    let claims = crate::canon::load_claims(root)?;
    let mut rows = Vec::new();
    for loaded in claims.iter() {
        let text = &loaded.text;
        let claim = &loaded.claim;
        let verdict = evaluate_claim(claim, &registry);
        let form_json = canonical_json(&verdict.form);
        let address = address_hex(&form_json);
        let predicates: Vec<_> = claim
            .applicability
            .iter()
            .map(|p| parse_predicate(p))
            .collect();
        rows.push(Row {
            claim_id: claim.claim_id.clone(),
            statement: claim.statement.replace('\n', " "),
            modality: claim.modality.clone(),
            address,
            applies: applies(&predicates, context),
            selectors: claim.applicability.clone(),
            relations: claim
                .relations
                .iter()
                .map(|r| (r.kind.clone(), r.to.clone()))
                .collect(),
            relations_hash: crate::model::relations_digest(&claim.relations),
            rationale_hash: crate::model::rationale_digest(&claim.rationale),
            // The same check `req canon --verify` runs, so the view and the gate cannot
            // disagree about whether a stored address is current.
            stored: crate::fill::verify_text(text, &form_json, crate::canon::RULES_V),
            rationale_source: claim.rationale.source.clone(),
            rationale_date: claim.rationale.date.clone(),
        });
    }
    Ok(rows)
}
