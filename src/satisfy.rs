//! Satisfaction links (ADR 0040): does a design still meet the claim it was recorded against?
//!
//! A **design element** is a file under a root listed in `.req/design`; it declares the claims it
//! satisfies with a line `req-satisfies: <claim-id>`. A human **records** a link with `req satisfy
//! --record <element> <claim-id>`, which writes the claim's statement, verbatim, to `.req/satisfied`
//! — the meaning the link was judged against. Nothing derived is stored (no address, no rules
//! version, no date), so a record survives a `RULES_V` bump and needs no git history.
//!
//! A link is judged by the **direction** of the claim's change since it was recorded, both
//! statements re-derived under the current rules ([`judge`]):
//!
//! * same address → **holds** (a cosmetic edit, or none);
//! * a direction in [`ADMITTED_CARRIES`] → **holds (weakened)**, reported by name;
//! * any other direction → **suspect** (UNVERIFIABLE: the design may still comply, nobody has shown
//!   it), naming the element, the claim and the direction, until a human re-records it;
//! * declared with no record → **unratified** (UNVERIFIABLE); a declared claim the corpus does not
//!   hold → UNVERIFIABLE; a record with no declaration left → **orphaned**, listed, not a finding.
//!
//! A `req-satisfies:` with **no** claim id after it is a **mention** (ADR 0052 decision 6), not a
//! declaration: help text and comments that document the marker are counted and listed, never a link
//! and never UNVERIFIABLE. A token that is present but malformed is still named (UNKNOWN CLAIM).
//!
//! The comparison's own selectors are the claim's *current* ones for both sides: a statement edit is
//! what is judged, and the record holds no applicability to compare.
//!
//! Admission is compiled in and pinned to the measurement (`tests/satisfy_admission.rs` reads
//! `bench/results/direction-v4-rules-v<RULES_V>.json`); the record is never read at runtime, because
//! an adopter does not have it. Scope classes are never carried, whatever the measurement says.

use crate::canon::{self, RULES_V};
use crate::compare::{Direction, compare};
use crate::format::{self, Format};
use crate::model::Registry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The optional list of design roots, repository-relative.
pub const DESIGN: &str = ".req/design";
/// The record of ratified links, repository-relative.
pub const SATISFIED: &str = ".req/satisfied";
/// The record's schema string.
pub const SCHEMA_SATISFIED: &str = "req.satisfied/1";
/// The listing's JSON schema string.
pub const SCHEMA_SATISFY: &str = "req.satisfy/1";
/// The token a design element carries to declare a link.
const MARKER: &str = "req-satisfies:";

/// The weakening classes decision 5 may admit — the only classes that can ever carry a link.
const WEAKENING: [Direction; 3] = [
    Direction::ModalityWeakening,
    Direction::NumericLoosening,
    Direction::MembershipWidening,
];

/// The classes whose links carry over a change (ADR 0040 decision 5): exactly those
/// `bench/results/direction-v4-rules-v<RULES_V>.json` reports `admitted: true` — zero false carries
/// on the largest direction gold set, with the support minimum. Today that is `modality_weakening`
/// (0 over 271) and `numeric_loosening` (0 over 139, from PURE's numeric statements; admitted by the
/// owner on 2026-11-12, N-1); `membership_widening` has no support on any gold set.
///
/// A test reads the record and asserts equality, so a rules bump or a re-measurement that changes
/// admission fails CI until this constant is changed deliberately.
pub const ADMITTED_CARRIES: &[Direction] =
    &[Direction::ModalityWeakening, Direction::NumericLoosening];

/// Whether a change in this direction carries a link. A weakening class the measurement admitted,
/// and nothing else: a scope class is refused here whatever [`ADMITTED_CARRIES`] says (decision 6).
pub fn carries(direction: Direction) -> bool {
    WEAKENING.contains(&direction) && ADMITTED_CARRIES.contains(&direction)
}

/// How one link stands against the claim it was recorded against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgement {
    /// Same address: the meaning did not move.
    Holds,
    /// The requirement loosened in an admitted class; the link is not re-opened.
    HoldsWeakened(Direction),
    /// Any other direction: UNVERIFIABLE until a human re-records it.
    Suspect(Direction),
}

/// Judge one link: the statement it was recorded against, and the claim's statement now, both
/// re-derived under the current rules. `applicability` is the claim's current selectors, applied to
/// both sides (an input to the address, never part of the statement).
pub fn judge(
    recorded: &str,
    current: &str,
    applicability: &[String],
    registry: &Registry,
) -> Judgement {
    let derive = |text: &str| {
        let (mut form, _lints) = canon::canonicalize_statement(text, registry);
        form.applicability = applicability.iter().cloned().collect();
        form
    };
    let (before, after) = (derive(recorded), derive(current));
    let address = |form| canon::address_hex(&canon::canonical_json(form));
    if address(&before) == address(&after) {
        return Judgement::Holds;
    }
    let direction = compare(&before, &after);
    if direction == Direction::Identical {
        return Judgement::Holds;
    }
    if carries(direction) && moved_on_its_axis_alone(&before, &after, direction) {
        Judgement::HoldsWeakened(direction)
    } else {
        Judgement::Suspect(direction)
    }
}

/// Did the claim move on `direction`'s axis and nowhere else? `compare` names the first axis that
/// differs (modality before conditions before the bound), so a compound edit — SHALL→SHOULD with a
/// tighter bound, or with a guard added — would otherwise be read as the weakening alone and carried.
/// The measurement (decision 5) is of single-axis pairs; a carry is earned on one axis only
/// (review S-2-review-1). Put the after side's axis back to the before side's: if the forms are then
/// equal, nothing else moved.
fn moved_on_its_axis_alone(
    before: &canon::CanonicalForm,
    after: &canon::CanonicalForm,
    direction: Direction,
) -> bool {
    let mut reverted = after.clone();
    match direction {
        Direction::ModalityWeakening => reverted.modality = before.modality,
        Direction::NumericLoosening => {
            reverted.operator = before.operator;
            reverted.value = before.value;
        }
        Direction::MembershipWidening => reverted.members = before.members.clone(),
        // Nothing else is ever carried (`carries`), so there is no axis to isolate.
        _ => return false,
    }
    canon::canonical_json(&reverted) == canon::canonical_json(before)
}

/// One recorded link: the meaning it was ratified against. `deny_unknown_fields`, as every `.req/`
/// file and claim: an unknown key is a load error naming it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub element: String,
    pub claim_id: String,
    /// The claim's statement **verbatim** at recording time.
    pub statement: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordFile {
    schema: String,
    #[serde(default)]
    links: Vec<Record>,
}

/// Why links could not be judged at all. `Malformed` is a usage error (exit 2, every bad line
/// named); `Unverifiable` is a state req cannot read (exit 3) — never a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Malformed(Vec<String>),
    Unverifiable(String),
}

impl LinkError {
    /// The lines to print.
    pub fn lines(&self) -> Vec<String> {
        match self {
            LinkError::Malformed(lines) => lines.clone(),
            LinkError::Unverifiable(reason) => vec![reason.clone()],
        }
    }

    /// The exit code the error earns on its own.
    pub fn exit(&self) -> i32 {
        match self {
            LinkError::Malformed(_) => 2,
            LinkError::Unverifiable(_) => 3,
        }
    }
}

/// Validate `.req/design`: one repo-relative directory or file per line (`.` or `./` is the repository root, held as the
/// empty root). Grammar as the other
/// `.req/` lists — `#` starts a comment, a blank line is skipped, a trailing CR is stripped. An
/// absolute path, a `..` component or a line of several words is malformed, and every bad line is
/// named `<file>:<line>` in one run.
pub fn parse_roots(text: &str) -> Result<Vec<String>, Vec<String>> {
    let mut roots = Vec::new();
    let mut errors = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        // A leading `./` is not part of the path, so it is taken off first: `.//tmp/x` is then `/tmp/x`, absolute,
        // and refused like it, never joined onto the corpus root (where it would replace it).
        let relative = line.trim_start_matches("./");
        let reason = if line.split_whitespace().count() > 1 {
            Some("a line is exactly one repo-relative path")
        } else if relative.starts_with('/') || relative.starts_with('\\') {
            Some("an absolute path is not repo-relative")
        } else if line.split(['/', '\\']).any(|part| part == "..") {
            Some("a `..` component leaves the repository")
        } else {
            None
        };
        if let Some(reason) = reason {
            errors.push(format!("{DESIGN}:{number}: `{line}`: {reason}"));
            continue;
        }
        // Normalised by segments: an empty or `.` segment says nothing, so `src`, `src/.`, `./src/./` and `src//`
        // are one root and an element id never carries a dot segment (ADR 0040). Nothing left is the repository
        // root itself (`.`, `./`), held as the empty root.
        let normalized: Vec<&str> = relative
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect();
        roots.push(normalized.join("/"));
    }
    if errors.is_empty() {
        Ok(roots)
    } else {
        Err(errors)
    }
}

/// The declared roots: `Ok(None)` when `.req/design` is absent (no links, said so), a usage error
/// for a malformed line, UNVERIFIABLE for a file that exists and cannot be read.
pub fn read_roots(root: &Path) -> Result<Option<Vec<String>>, LinkError> {
    let path = root.join(DESIGN);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(LinkError::Unverifiable(format!(
                "{DESIGN} exists but cannot be read ({error}): a pinned list that became unusable \
                 is not absent, and a skipped check is never a pass"
            )));
        }
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| LinkError::Unverifiable(format!("{DESIGN} is not valid UTF-8 text")))?;
    parse_roots(&text).map(Some).map_err(LinkError::Malformed)
}

/// One `req-satisfies:` occurrence with a token after it: the element (repo-relative,
/// `/`-separated), the token written after the marker — a claim id when it is well-formed, else
/// carried as written so it is named — and its line. A marker with no token is a [`Mention`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub element: String,
    pub claim_id: String,
    pub line: usize,
}

/// One `req-satisfies:` with no claim id after it (ADR 0052 decision 6): a **mention**, such as help
/// text or a comment that documents the marker. It is counted and shown, never a link and never
/// UNVERIFIABLE; a token that is present but malformed is not a mention, it is still named.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mention {
    pub element: String,
    pub line: usize,
}

/// What reading the design roots found.
#[derive(Debug, Default)]
pub struct Scan {
    /// Every distinct `(element, claim)` declared, sorted.
    pub declarations: Vec<Declaration>,
    /// Every distinct `(element, line)` holding the marker with no claim id, sorted.
    pub mentions: Vec<Mention>,
    /// Roots and files that could not be read, each with the reason: UNVERIFIABLE, never skipped.
    pub unreadable: Vec<String>,
}

/// The token after each `req-satisfies:` on a line: a maximal run of ASCII alphanumerics, `-`, `_`.
fn tokens(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find(MARKER) {
        rest = &rest[at + MARKER.len()..];
        let after = rest.trim_start_matches([' ', '\t']);
        let token: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        found.push(token);
    }
    found
}

fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_string()
    } else {
        format!("{directory}/{name}")
    }
}

/// Read `relative` (a file or a directory, recursively) into `files`; anything unreadable lands in
/// `unreadable`. Symlinks are not followed and a directory named `.git` is not entered.
fn walk(
    root: &Path,
    relative: &str,
    files: &mut BTreeMap<String, String>,
    unreadable: &mut Vec<String>,
) {
    let path = root.join(relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            unreadable.push(format!(
                "{relative}: a design root that does not exist (listed in {DESIGN}): a root that \
                 vanished is silent decay, not an empty one"
            ));
            return;
        }
        Err(error) => {
            unreadable.push(format!("{relative}: cannot be examined: {error}"));
            return;
        }
    };
    let kind = metadata.file_type();
    if kind.is_symlink() {
        unreadable.push(format!(
            "{relative}: a symlink, which is not followed or read"
        ));
    } else if kind.is_file() {
        match std::fs::read(&path) {
            Err(error) => unreadable.push(format!("{relative}: cannot be read: {error}")),
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => {
                    files.insert(relative.to_string(), text);
                }
                Err(_) => unreadable.push(format!(
                    "{relative}: is not valid UTF-8 text, so its links cannot be read"
                )),
            },
        }
    } else if kind.is_dir() {
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => {
                unreadable.push(format!("{relative}: cannot be listed: {error}"));
                return;
            }
        };
        let mut names: Vec<String> = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    unreadable.push(format!("{relative}: cannot be listed: {error}"));
                    continue;
                }
            };
            match entry.file_name().into_string() {
                Ok(name) if name == ".git" => {}
                Ok(name) => names.push(name),
                Err(name) => unreadable.push(format!(
                    "{relative}: an entry named {name:?} is not valid UTF-8 and cannot be a path \
                     an element id can name"
                )),
            }
        }
        names.sort();
        for name in names {
            walk(root, &join(relative, &name), files, unreadable);
        }
    } else {
        unreadable.push(format!("{relative}: neither a file nor a directory"));
    }
}

/// Read every design root and collect the declarations. `.req/design` absent is an empty scan.
pub fn scan(root: &Path) -> Result<Scan, LinkError> {
    let Some(roots) = read_roots(root)? else {
        return Ok(Scan::default());
    };
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    let mut unreadable = Vec::new();
    for relative in &roots {
        walk(root, relative, &mut files, &mut unreadable);
    }
    unreadable.sort();
    unreadable.dedup();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut declarations = Vec::new();
    let mut mentions: BTreeSet<Mention> = BTreeSet::new();
    for (element, text) in &files {
        for (index, line) in text.lines().enumerate() {
            for claim_id in tokens(line) {
                if claim_id.is_empty() {
                    mentions.insert(Mention {
                        element: element.clone(),
                        line: index + 1,
                    });
                } else if seen.insert((element.clone(), claim_id.clone())) {
                    declarations.push(Declaration {
                        element: element.clone(),
                        claim_id,
                        line: index + 1,
                    });
                }
            }
        }
    }
    declarations.sort_by(|a, b| (&a.element, &a.claim_id).cmp(&(&b.element, &b.claim_id)));
    Ok(Scan {
        declarations,
        mentions: mentions.into_iter().collect(),
        unreadable,
    })
}

/// The claim ids some element declares, for the completeness step and the audit. A design root that
/// cannot be read makes the answer UNVERIFIABLE: the absence of a declaration was not established.
pub fn declared_claims(root: &Path) -> Result<BTreeSet<String>, LinkError> {
    let scanned = scan(root)?;
    if !scanned.unreadable.is_empty() {
        return Err(LinkError::Unverifiable(format!(
            "design roots cannot be read, so which claims an element satisfies is not established: {}",
            scanned.unreadable.join("; ")
        )));
    }
    Ok(scanned
        .declarations
        .into_iter()
        .map(|declaration| declaration.claim_id)
        .collect())
}

/// Load `.req/satisfied`. Absent is no record. A file that cannot be read, names an unknown key,
/// declares another schema or repeats a link is an `Err` — never read as "no record".
pub fn load_records(root: &Path) -> Result<Vec<Record>, String> {
    let path = root.join(SATISFIED);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{SATISFIED} cannot be read: {error}")),
    };
    let file: RecordFile =
        yaml_serde::from_str(&text).map_err(|error| format!("{SATISFIED}: {error}"))?;
    if file.schema != SCHEMA_SATISFIED {
        return Err(format!(
            "{SATISFIED}: schema is {:?}, but this build reads {SCHEMA_SATISFIED:?}",
            file.schema
        ));
    }
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for record in &file.links {
        if !seen.insert((&record.element, &record.claim_id)) {
            return Err(format!(
                "{SATISFIED}: {} -> {} is recorded twice: a link has one record",
                record.element, record.claim_id
            ));
        }
    }
    Ok(file.links)
}

/// The record file's bytes: sorted by `(element, claim_id)`, so a re-record is a minimal diff.
fn render_records(mut links: Vec<Record>) -> Result<String, String> {
    links.sort_by(|a, b| (&a.element, &a.claim_id).cmp(&(&b.element, &b.claim_id)));
    let file = RecordFile {
        schema: SCHEMA_SATISFIED.to_string(),
        links,
    };
    yaml_serde::to_string(&file).map_err(|error| format!("{SATISFIED} cannot be written: {error}"))
}

/// A link's standing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Judged(Judgement),
    /// Declared, never recorded.
    Unratified,
    /// Declared against a claim id the corpus does not hold (or a token that is not one).
    UnknownClaim,
    /// Recorded, no longer declared: listed, not a finding.
    Orphaned,
}

impl Outcome {
    /// UNVERIFIABLE: a suspect, unratified or unknown-claim link. An orphan is not a finding.
    pub fn blocks(&self) -> bool {
        matches!(
            self,
            Outcome::Judged(Judgement::Suspect(_)) | Outcome::Unratified | Outcome::UnknownClaim
        )
    }

    /// The word the JSON document uses.
    pub fn as_json(&self) -> &'static str {
        match self {
            Outcome::Judged(Judgement::Holds) => "holds",
            Outcome::Judged(Judgement::HoldsWeakened(_)) => "holds_weakened",
            Outcome::Judged(Judgement::Suspect(_)) => "suspect",
            Outcome::Unratified => "unratified",
            Outcome::UnknownClaim => "unknown_claim",
            Outcome::Orphaned => "orphaned",
        }
    }

    /// The direction the change since recording read as, when there was a change.
    pub fn direction(&self) -> Option<Direction> {
        match self {
            Outcome::Judged(Judgement::HoldsWeakened(d) | Judgement::Suspect(d)) => Some(*d),
            _ => None,
        }
    }
}

/// One link in the listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub element: String,
    pub claim_id: String,
    /// The line of the declaration; `None` for an orphaned record.
    pub line: Option<usize>,
    pub outcome: Outcome,
}

impl Link {
    /// The sentence a finding is reported in: names the element, the claim and why.
    pub fn finding(&self) -> String {
        let head = format!("{} -> {}", self.element, self.claim_id);
        match self.outcome {
            Outcome::Judged(Judgement::Suspect(direction)) => format!(
                "{head}: suspect — the claim changed since it was recorded ({}); re-record it once \
                 a human has checked the design: `req satisfy --record {} {}`",
                direction.as_str(),
                self.element,
                self.claim_id
            ),
            Outcome::Unratified => format!(
                "{head}: unratified — declared and never recorded: `req satisfy --record {} {}`",
                self.element, self.claim_id
            ),
            Outcome::UnknownClaim => format!(
                "{head}: names a claim the corpus does not hold ({}:{})",
                self.element,
                self.line.unwrap_or(0)
            ),
            Outcome::Judged(_) | Outcome::Orphaned => format!("{head}: {}", self.outcome.as_json()),
        }
    }
}

/// Everything one evaluation found.
#[derive(Debug, Default)]
pub struct Evaluation {
    /// Declared links, then orphaned records, each sorted by `(element, claim_id)`.
    pub links: Vec<Link>,
    /// Every `req-satisfies:` with no claim id: shown, never a link, and no part of the verdict.
    pub mentions: Vec<Mention>,
    /// Design roots and files that could not be read.
    pub unreadable: Vec<String>,
}

impl Evaluation {
    /// 0 every link holds (or there are none), 3 any link blocks or anything was unreadable.
    pub fn exit(&self) -> i32 {
        if !self.unreadable.is_empty() || self.links.iter().any(|link| link.outcome.blocks()) {
            3
        } else {
            0
        }
    }
}

/// Read the design roots and the record, load the corpus and judge every declared link.
pub fn evaluate(root: &Path) -> Result<Evaluation, LinkError> {
    let scanned = scan(root)?;
    let records = load_records(root).map_err(LinkError::Unverifiable)?;
    if scanned.declarations.is_empty() && records.is_empty() {
        return Ok(Evaluation {
            links: Vec::new(),
            mentions: scanned.mentions,
            unreadable: scanned.unreadable,
        });
    }
    let (claims, registry) = if scanned.declarations.is_empty() {
        (Vec::new(), Registry::empty())
    } else {
        (
            canon::load_claims(root).map_err(LinkError::Unverifiable)?,
            canon::load_registry(root).map_err(LinkError::Unverifiable)?,
        )
    };
    let mut links = Vec::new();
    let mut declared: BTreeSet<(&str, &str)> = BTreeSet::new();
    for declaration in &scanned.declarations {
        declared.insert((&declaration.element, &declaration.claim_id));
        let claim = claims
            .iter()
            .find(|loaded| loaded.claim.claim_id == declaration.claim_id);
        let record = records.iter().find(|record| {
            record.element == declaration.element && record.claim_id == declaration.claim_id
        });
        let outcome = match (claim, record) {
            (None, _) => Outcome::UnknownClaim,
            (Some(_), None) => Outcome::Unratified,
            (Some(loaded), Some(record)) => Outcome::Judged(judge(
                &record.statement,
                &loaded.claim.statement,
                &loaded.claim.applicability,
                &registry,
            )),
        };
        links.push(Link {
            element: declaration.element.clone(),
            claim_id: declaration.claim_id.clone(),
            line: Some(declaration.line),
            outcome,
        });
    }
    let mut orphans: Vec<Link> = records
        .iter()
        .filter(|record| !declared.contains(&(record.element.as_str(), record.claim_id.as_str())))
        .map(|record| Link {
            element: record.element.clone(),
            claim_id: record.claim_id.clone(),
            line: None,
            outcome: Outcome::Orphaned,
        })
        .collect();
    orphans.sort_by(|a, b| (&a.element, &a.claim_id).cmp(&(&b.element, &b.claim_id)));
    links.extend(orphans);
    Ok(Evaluation {
        links,
        mentions: scanned.mentions,
        unreadable: scanned.unreadable,
    })
}

/// What the `design_satisfied` gate policy found over a selection's applying claims.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GateFindings {
    pub judged: usize,
    pub holding: usize,
    pub weakened: usize,
    /// One line per link (or unreadable root) that cannot be confirmed.
    pub unverifiable: Vec<String>,
}

/// Judge the links of the applying claims, plus every link naming a claim the corpus does not hold
/// (it belongs to no selection). A link to a claim the selection excludes or leaves undecided is not
/// this policy's: the first is not this variant's problem, the second is the gate's own
/// undecided answer. An orphaned record is not a finding.
pub fn gate_findings(root: &Path, applying: &BTreeSet<&str>) -> GateFindings {
    let evaluation = match evaluate(root) {
        Ok(evaluation) => evaluation,
        Err(error) => {
            return GateFindings {
                unverifiable: error.lines(),
                ..GateFindings::default()
            };
        }
    };
    let mut found = GateFindings {
        unverifiable: evaluation.unreadable.clone(),
        ..GateFindings::default()
    };
    for link in &evaluation.links {
        let counted = match link.outcome {
            Outcome::Orphaned => false,
            Outcome::UnknownClaim => true,
            _ => applying.contains(link.claim_id.as_str()),
        };
        if !counted {
            continue;
        }
        found.judged += 1;
        match link.outcome {
            Outcome::Judged(Judgement::Holds) => found.holding += 1,
            Outcome::Judged(Judgement::HoldsWeakened(_)) => found.weakened += 1,
            _ => found.unverifiable.push(link.finding()),
        }
    }
    found
}

const USAGE: &str =
    "usage: req satisfy [--format text|json]\n       req satisfy --record <element> <claim-id>";

fn print_text(evaluation: &Evaluation, has_design: bool) {
    let declared = evaluation
        .links
        .iter()
        .filter(|link| link.outcome != Outcome::Orphaned)
        .count();
    if declared == 0 && !evaluation.unreadable.is_empty() {
        println!(
            "# req satisfy: no satisfaction link could be read under {DESIGN} (see the UNVERIFIABLE lines)"
        );
    } else if declared == 0 {
        if has_design {
            println!(
                "# req satisfy: no satisfaction link declared under {DESIGN} (nothing to judge)"
            );
        } else {
            println!(
                "# req satisfy: no {DESIGN}, so no satisfaction link is declared (nothing to judge)"
            );
        }
    } else {
        println!("# req satisfy: {declared} link(s) declared");
    }
    if !evaluation.mentions.is_empty() {
        println!(
            "# req satisfy: {} mention(s) of `{MARKER}` with no claim id (not links)",
            evaluation.mentions.len()
        );
    }
    for link in &evaluation.links {
        let (word, detail) = match link.outcome {
            Outcome::Judged(Judgement::Holds) => ("HOLDS", String::new()),
            Outcome::Judged(Judgement::HoldsWeakened(direction)) => (
                "HOLDS (weakened)",
                format!(
                    ": {} — the requirement loosened; the link is not re-opened",
                    direction.as_str()
                ),
            ),
            Outcome::Judged(Judgement::Suspect(direction)) => (
                "SUSPECT",
                format!(
                    ": {} since it was recorded — UNVERIFIABLE until a human re-records it \
                     (`req satisfy --record {} {}`)",
                    direction.as_str(),
                    link.element,
                    link.claim_id
                ),
            ),
            Outcome::Unratified => (
                "UNRATIFIED",
                format!(
                    ": declared, never recorded — UNVERIFIABLE until a human records it \
                     (`req satisfy --record {} {}`)",
                    link.element, link.claim_id
                ),
            ),
            Outcome::UnknownClaim => (
                "UNKNOWN CLAIM",
                format!(
                    ": the corpus holds no claim {:?} (declared at {}:{}) — UNVERIFIABLE",
                    link.claim_id,
                    link.element,
                    link.line.unwrap_or(0)
                ),
            ),
            Outcome::Orphaned => (
                "ORPHANED",
                ": recorded, no longer declared (listed, not a finding)".to_string(),
            ),
        };
        println!("[{word}] {} -> {}{detail}", link.element, link.claim_id);
    }
    for mention in &evaluation.mentions {
        println!(
            "[MENTION] {}:{}: {MARKER} with no claim id — not a link",
            mention.element, mention.line
        );
    }
    for reason in &evaluation.unreadable {
        println!("[UNVERIFIABLE] {reason}");
    }
    println!(
        "verdict: {}",
        if evaluation.exit() == 0 {
            "PASS — every declared link holds"
        } else {
            "UNVERIFIABLE — at least one link, or design file, cannot be confirmed"
        }
    );
}

#[derive(Serialize)]
struct LinkDocument<'a> {
    element: &'a str,
    claim_id: &'a str,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
}

#[derive(Serialize)]
struct MentionDocument<'a> {
    element: &'a str,
    line: usize,
}

#[derive(Serialize)]
struct SatisfyDocument<'a> {
    schema: &'static str,
    generator: &'static str,
    rules_v: u32,
    verdict: &'static str,
    links: Vec<LinkDocument<'a>>,
    mentions: Vec<MentionDocument<'a>>,
    unreadable: &'a [String],
}

fn print_json(evaluation: &Evaluation) {
    let document = SatisfyDocument {
        schema: SCHEMA_SATISFY,
        generator: "satisfy",
        rules_v: RULES_V,
        verdict: crate::gate::verdict_word(evaluation.exit()),
        links: evaluation
            .links
            .iter()
            .map(|link| LinkDocument {
                element: &link.element,
                claim_id: &link.claim_id,
                outcome: link.outcome.as_json(),
                direction: link.outcome.direction().map(|d| d.as_str()),
                line: link.line,
            })
            .collect(),
        mentions: evaluation
            .mentions
            .iter()
            .map(|mention| MentionDocument {
                element: &mention.element,
                line: mention.line,
            })
            .collect(),
        unreadable: &evaluation.unreadable,
    };
    println!("{}", format::document(&document));
}

/// `req satisfy --record <element> <claim-id>`: the human act. Refuses (exit 2) a link no element
/// declares and a claim the corpus does not hold; refuses (exit 3) to overwrite a record it cannot
/// read. Writes the claim's statement verbatim, replacing that one link's record.
fn record(element: &str, claim_id: &str, root: &Path) -> i32 {
    let element = element.trim_start_matches("./");
    let scanned = match scan(root) {
        Ok(scanned) => scanned,
        Err(error) => {
            for line in error.lines() {
                eprintln!("{line}");
            }
            return error.exit();
        }
    };
    if !scanned
        .declarations
        .iter()
        .any(|d| d.element == element && d.claim_id == claim_id)
    {
        eprintln!(
            "{element} does not declare `{MARKER} {claim_id}` under a root in {DESIGN}: a link is \
             authored in the design element first, and only then recorded"
        );
        for reason in &scanned.unreadable {
            eprintln!("note: {reason}");
        }
        return 2;
    }
    let claims = match canon::load_claims(root) {
        Ok(claims) => claims,
        Err(reason) => {
            eprintln!("[UNVERIFIABLE] {reason}");
            return 3;
        }
    };
    let Some(claim) = claims.iter().find(|c| c.claim.claim_id == claim_id) else {
        eprintln!(
            "{claim_id}: the corpus holds no such claim, so nothing can be recorded against it"
        );
        return 2;
    };
    let mut links = match load_records(root) {
        Ok(links) => links,
        Err(reason) => {
            eprintln!(
                "[UNVERIFIABLE] {reason}\nnothing was written: an unreadable record is not overwritten"
            );
            return 3;
        }
    };
    links.retain(|r| !(r.element == element && r.claim_id == claim_id));
    links.push(Record {
        element: element.to_string(),
        claim_id: claim_id.to_string(),
        statement: claim.claim.statement.clone(),
    });
    let text = match render_records(links) {
        Ok(text) => text,
        Err(reason) => {
            eprintln!("[UNVERIFIABLE] {reason}");
            return 3;
        }
    };
    let target = root.join(SATISFIED);
    if let Some(parent) = target.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "[UNVERIFIABLE] {} cannot be created: {error}",
            parent.display()
        );
        return 3;
    }
    if let Err(error) = std::fs::write(&target, text) {
        eprintln!("[UNVERIFIABLE] {SATISFIED} cannot be written: {error}");
        return 3;
    }
    println!("recorded {element} -> {claim_id} in {SATISFIED}");
    println!("  against: {}", claim.claim.statement.trim_end());
    0
}

/// `req satisfy [--format text|json]` / `req satisfy --record <element> <claim-id>`
///
/// Exit: **0** every declared link holds (or there are none, said so), **3** any link is
/// UNVERIFIABLE (suspect, unratified, unknown claim) or a design file cannot be read, **2** usage
/// or a malformed `.req/design`. `--record` is 0 written, 2 refused, 3 not writable.
pub fn run(args: &[String], root: &Path) -> i32 {
    if args.first().map(String::as_str) == Some("--record") {
        return match args {
            [_, element, claim_id] if !element.starts_with('-') && !claim_id.starts_with('-') => {
                record(element, claim_id, root)
            }
            _ => {
                eprintln!("{USAGE}\n`--record` takes exactly an element and a claim id");
                2
            }
        };
    }
    let (format, rest) = match format::take_format(args, USAGE) {
        Ok(parsed) => parsed,
        Err(code) => return code,
    };
    if !rest.is_empty() {
        eprintln!("{USAGE}\nunexpected argument: {}", rest.join(" "));
        return 2;
    }
    let evaluation = match evaluate(root) {
        Ok(evaluation) => evaluation,
        Err(error) => {
            for line in error.lines() {
                eprintln!("{line}");
            }
            if error.exit() == 3 {
                eprintln!("[UNVERIFIABLE] links were not judged");
            }
            return error.exit();
        }
    };
    if format == Format::Json {
        print_json(&evaluation);
    } else {
        let has_design = root.join(DESIGN).is_file();
        print_text(&evaluation, has_design);
    }
    evaluation.exit()
}
