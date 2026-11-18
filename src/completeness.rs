//! Structural omissions (ADR 0039): what req can prove is missing from files it already reads.
//!
//! Three findings (a fourth with `need design`, a fifth with `.req/areas`, a sixth and seventh with `.req/kinds/`), each naming the claim, and never a verdict that the requirements are complete —
//! req holds no hazard list, no stakeholder needs and no standard's clauses, so it has nothing to be
//! complete *against*:
//!
//! * **`in_no_release`** — every declared release line excludes the claim (`No` in each). A claim
//!   nothing examines is the silent omission ADR 0015 forbids, one level up: **FAIL**. A claim some
//!   line leaves *undecided* is not named here: that line's gate already answers UNVERIFIABLE, and a
//!   FAIL would outrank an open question with a defect nobody established.
//! * **`in_no_baseline`** — the claim applies in a release line and no **signed** manifest under
//!   `baselines/` names it. Not a defect (it may be new since the last signing), but not ratified
//!   either: **UNVERIFIABLE**. A manifest counts only when its `<file>.minisig` lies beside it; whether
//!   that signature verifies stays `req baseline --verify`'s answer (the gate runs it).
//! * **`unrelated`** — the claim has no outgoing and no derived incoming relation. Reported, and FAIL
//!   only when `.req/completeness` says `strict unrelated`; a claim listed there as a root is exempt.
//! * **`undeclared_area`** (only with `.req/areas`, ADR 0051) — the claim names an area the registry does not
//!   declare. Reported, and FAIL only when `.req/completeness` says `strict areas`: authoring stays
//!   friction-free (ADR 0006), so a path nobody declared is a finding and never a load error.
//! * **`missing_field`** and **`dangling_field_ref`** (only with `.req/kinds/`, ADR 0056 decision 4) — a claim whose
//!   kind declares a field `required: true` and that carries no value for it: **UNVERIFIABLE**, and FAIL only when
//!   `.req/completeness` says `strict fields` (a claim written before its kind gained a required field must still
//!   load); and a `ref` field, or a list of them, naming a claim the corpus does not hold: **UNVERIFIABLE**, the
//!   class of a dangling relation. Each names the claim and the field, never a verdict on the value.
//! * **`unsatisfied`** (only with `need design`, ADR 0040) — the claim applies in a release line and no
//!   design element declares `req-satisfies:` for it: **UNVERIFIABLE**. A repository policy rather than
//!   a per-claim field, so no claim-schema change.
//!
//! The step is a repository question — release lines, Baselines and claims together — so it lives in
//! `req check`, which already reads all three (ADR 0023).

use crate::select::{Applies, Context};
use crate::view::Row;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

/// The optional policy file, repository-relative.
pub const COMPLETENESS: &str = ".req/completeness";

/// The optional area registry, repository-relative (ADR 0051 decision 3).
pub const AREAS: &str = ".req/areas";

/// Where the committed Baselines live.
const BASELINES: &str = "baselines";

/// The number of checks the step runs, printed so a pass says what it examined.
const CHECKS: usize = 3;

/// One line of `.req/areas`: an area path and the reader-facing label for it, in any language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaEntry {
    pub path: String,
    pub label: String,
}

/// Validate `.req/areas` (ADR 0051 decision 3). Grammar as the other `.req/` lists: a trailing CR is
/// stripped and a blank line is skipped. A line is `<path> <label>`: the path is the claim key's grammar
/// ([`crate::model::validate_area`], one rule with one home), the label is the rest of the line,
/// trimmed, never empty. A line **starting** with `#` is a comment; a `#` inside a label is part of the
/// label (a name such as `C# support` is not cut short). A malformed path, an empty label and a path
/// declared twice are refused naming `<file>:<line>`, every bad line in one run. Line order is the
/// reading order, and a parent need not be declared.
pub fn parse_areas(text: &str) -> Result<Vec<AreaEntry>, Vec<String>> {
    let mut entries: Vec<AreaEntry> = Vec::new();
    let mut first_seen: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut errors = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (path, label) = match line.split_once([' ', '\t']) {
            Some((path, rest)) => (path, rest.trim()),
            None => (line, ""),
        };
        if let Err(reason) = crate::model::validate_area(path) {
            errors.push(format!("{AREAS}:{number}: {reason}"));
            continue;
        }
        if label.is_empty() {
            errors.push(format!(
                "{AREAS}:{number}: `{path}` has no label: a line is `<path> <label>`, and the label is \
                 the name a reader sees (in any language), never the key"
            ));
            continue;
        }
        if let Some(first) = first_seen.get(path) {
            errors.push(format!(
                "{AREAS}:{number}: `{path}` is declared twice (first at {AREAS}:{first}): one line per \
                 area"
            ));
            continue;
        }
        first_seen.insert(path.to_string(), number);
        entries.push(AreaEntry {
            path: path.to_string(),
            label: label.to_string(),
        });
    }
    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}

/// `.req/completeness`, validated: the declared roots, whether `unrelated` is promoted to FAIL,
/// whether every applying claim needs a design element (`need design`, ADR 0040) and whether an
/// undeclared area is a FAIL (`strict areas`, ADR 0051). `areas` is the validated `.req/areas`, read
/// beside it by the caller; `None` when the repository declares none, which is no finding at all.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Policy {
    pub roots: BTreeSet<String>,
    pub strict_unrelated: bool,
    pub need_design: bool,
    pub strict_areas: bool,
    /// `strict fields` (ADR 0056): a `missing_field` is a FAIL and not an open question.
    pub strict_fields: bool,
    pub areas: Option<Vec<AreaEntry>>,
    /// Whether the repository declares any kind (`.req/kinds/`): with none, neither field finding exists and the
    /// step's label does not move. Set by the caller, which reads the declarations once.
    pub kinds: bool,
}

/// Validate `.req/completeness` against the corpus's ids. Grammar as the other `.req/` lists: `#`
/// starts a comment, a blank line is skipped, a trailing CR is stripped. A line is either one claim
/// id (a root), `strict unrelated`, `strict areas`, `strict fields` or `need design`. Anything else — an unknown
/// word, an id the corpus does not hold, two ids on one line — is refused naming `<file>:<line>`, every bad line in one run.
pub fn parse_policy(text: &str, known: &BTreeSet<String>) -> Result<Policy, Vec<String>> {
    let mut policy = Policy::default();
    let mut errors = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let line = line.split('#').next().unwrap_or("");
        let words: Vec<&str> = line
            .split([' ', '\t'])
            .filter(|word| !word.is_empty())
            .collect();
        match words.as_slice() {
            [] => {}
            ["strict", "unrelated"] => policy.strict_unrelated = true,
            ["strict", "areas"] => policy.strict_areas = true,
            ["strict", "fields"] => policy.strict_fields = true,
            ["need", "design"] => policy.need_design = true,
            [id] if known.contains(*id) => {
                policy.roots.insert((*id).to_string());
            }
            [id] => errors.push(format!(
                "{COMPLETENESS}:{number}: `{id}` is neither a claim id this corpus holds nor a known \
                 word: a line is one root claim id, `strict unrelated`, `strict areas`, `strict fields` or \
                 `need design`"
            )),
            many => errors.push(format!(
                "{COMPLETENESS}:{number}: `{}` is not a line this file takes: one root claim id per \
                 line, `strict unrelated`, `strict areas`, `strict fields` or `need design`",
                many.join(" ")
            )),
        }
    }
    if errors.is_empty() {
        Ok(policy)
    } else {
        Err(errors)
    }
}

/// A claim naming an area the registry does not declare (ADR 0051): the claim and the area it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UndeclaredArea {
    pub claim_id: String,
    pub area: String,
}

/// A claim of a kind that declares a field `required: true` and carries no value for it (ADR 0056 decision 4):
/// the claim, the kind and the field. Names only: absent is UNKNOWN, never a default.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MissingField {
    pub claim_id: String,
    pub kind: String,
    pub field: String,
}

/// A `ref` field (or a list of them) naming a claim the corpus does not hold (ADR 0056 decision 4): the claim, the
/// field and the missing id. The audit packet prints the first two only, because the id is a field *value*.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DanglingFieldRef {
    pub claim_id: String,
    pub field: String,
    #[serde(rename = "ref")]
    pub target: String,
}

/// The two field findings of a corpus with kinds (ADR 0056 decision 4), over some rows.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FieldFindings {
    pub missing_field: Vec<MissingField>,
    pub dangling_field_ref: Vec<DanglingFieldRef>,
}

/// The claims of `rows` whose kind declares a required field they do not carry, sorted by claim and field. A
/// claim with no kind has no declaration to miss, and a kind the declarations lack was already refused at load.
pub fn missing_fields(
    rows: &[Row],
    declarations: &crate::kinds::Declarations,
) -> Vec<MissingField> {
    let mut found = Vec::new();
    for row in rows {
        let Some(kind) = row.kind.as_deref() else {
            continue;
        };
        let Some(declared) = declarations.kind(kind) else {
            continue;
        };
        for (name, decl) in &declared.fields {
            if decl.required && !row.fields.iter().any(|field| &field.name == name) {
                found.push(MissingField {
                    claim_id: row.claim_id.clone(),
                    kind: kind.to_string(),
                    field: name.clone(),
                });
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The `ref` values of `rows` that name no claim of `universe` (the whole corpus: a target outside a selection is
/// still a claim), sorted by claim, field and id. Reads the judged type, never the text of a value.
pub fn dangling_field_refs(rows: &[Row], universe: &BTreeSet<&str>) -> Vec<DanglingFieldRef> {
    use crate::kinds::FieldValue;
    let mut found = Vec::new();
    for row in rows {
        for field in &row.fields {
            let targets: Vec<&String> = match (&field.value, field.type_name, field.of) {
                (FieldValue::Text(id), "ref", _) => vec![id],
                (FieldValue::List(ids), "list", Some("ref")) => ids.iter().collect(),
                _ => continue,
            };
            for id in targets {
                if !universe.contains(id.as_str()) {
                    found.push(DanglingFieldRef {
                        claim_id: row.claim_id.clone(),
                        field: field.name.clone(),
                        target: id.clone(),
                    });
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// What the step found. Every list is sorted by claim id and holds each id once.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Findings {
    pub in_no_release: Vec<String>,
    pub in_no_baseline: Vec<String>,
    pub unrelated: Vec<String>,
    /// Whether `unrelated` decides the verdict (`strict unrelated`) or is reported only.
    pub unrelated_judged: bool,
    /// With `.req/areas` (ADR 0051): the claims naming an area it does not declare. `None` when no
    /// registry is declared — absent from the JSON, so "not judged" is not read as "none found".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undeclared_area: Option<Vec<UndeclaredArea>>,
    /// Whether `undeclared_area` decides the verdict (`strict areas`) or is reported only.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub undeclared_area_judged: bool,
    /// With `.req/kinds/` (ADR 0056): the claims missing a required field. `None` when no kind is declared —
    /// absent from the JSON, so "not judged" is not read as "none found".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_field: Option<Vec<MissingField>>,
    /// Whether `missing_field` decides the verdict (`strict fields`) or is an open question.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing_field_judged: bool,
    /// With `.req/kinds/` (ADR 0056): the `ref` values naming no claim. UNVERIFIABLE, whatever the policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dangling_field_ref: Option<Vec<DanglingFieldRef>>,
    /// With `need design` (ADR 0040): the applying claims no design element declares a link for.
    /// `None` when the policy does not ask — absent from the JSON, so "not judged" is not read as
    /// "none found".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsatisfied: Option<Vec<String>>,
    /// The signed manifests that were read, by repository path.
    pub baselines: Vec<String>,
    /// Manifests that did not count, each with the reason (no signature beside it, unreadable).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_counted: Vec<String>,
}

impl Findings {
    /// FAIL outranks UNVERIFIABLE outranks PASS, as everywhere else in req.
    pub fn exit(&self) -> i32 {
        if !self.in_no_release.is_empty()
            || (self.unrelated_judged && !self.unrelated.is_empty())
            || (self.undeclared_area_judged && !self.undeclared().is_empty())
            || (self.missing_field_judged && !self.missing().is_empty())
        {
            1
        } else if !self.in_no_baseline.is_empty()
            || !self.not_counted.is_empty()
            || !self.missing().is_empty()
            || !self.dangling().is_empty()
            || self.unsatisfied.as_ref().is_some_and(|ids| !ids.is_empty())
        {
            3
        } else {
            0
        }
    }

    /// The undeclared-area findings, empty when none or when no registry is declared.
    fn undeclared(&self) -> &[UndeclaredArea] {
        self.undeclared_area.as_deref().unwrap_or(&[])
    }

    fn missing(&self) -> &[MissingField] {
        self.missing_field.as_deref().unwrap_or(&[])
    }

    fn dangling(&self) -> &[DanglingFieldRef] {
        self.dangling_field_ref.as_deref().unwrap_or(&[])
    }

    /// The lines the text view prints under the step.
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if !self.in_no_release.is_empty() {
            notes.push(format!(
                "in_no_release (FAIL): {} — applies in no .req/variants line, so no release examines \
                 it; retire it or declare the release that ships it",
                self.in_no_release.join(", ")
            ));
        }
        if self.baselines.is_empty() {
            notes.push(format!(
                "no signed Baseline under {BASELINES}/: nothing in this repository is ratified \
                 (sign one: `req baseline --at <ts> --out {BASELINES}/<name>.json`, then `minisign -Sm`)"
            ));
        }
        for reason in &self.not_counted {
            notes.push(format!("not counted: {reason}"));
        }
        if !self.in_no_baseline.is_empty() {
            notes.push(format!(
                "in_no_baseline (UNVERIFIABLE): {} — applies in a declared release and no signed \
                 Baseline names it",
                self.in_no_baseline.join(", ")
            ));
        }
        if let Some(ids) = self.unsatisfied.as_ref().filter(|ids| !ids.is_empty()) {
            notes.push(format!(
                "unsatisfied (UNVERIFIABLE): {} — applies in a declared release and no design element \
                 declares `req-satisfies:` for it (`need design` in {COMPLETENESS}; roots in {})",
                ids.join(", "),
                crate::satisfy::DESIGN
            ));
        }
        if !self.unrelated.is_empty() {
            let how = if self.unrelated_judged {
                "FAIL, `strict unrelated`"
            } else {
                "reported, not judged"
            };
            notes.push(format!(
                "unrelated ({how}): {} — no relation in or out; add the missing relation, or list \
                 the claim as a root in {COMPLETENESS}",
                self.unrelated.join(", ")
            ));
        }
        if !self.undeclared().is_empty() {
            let how = if self.undeclared_area_judged {
                "FAIL, `strict areas`"
            } else {
                "reported, not judged"
            };
            let named: Vec<String> = self
                .undeclared()
                .iter()
                .map(|found| format!("{} ({})", found.claim_id, found.area))
                .collect();
            notes.push(format!(
                "undeclared_area ({how}): {} — names an area {AREAS} does not declare; add the line \
                 `<path> <label>` there, or file the claim under a declared area",
                named.join(", ")
            ));
        }
        if !self.missing().is_empty() {
            let how = if self.missing_field_judged {
                "FAIL, `strict fields`"
            } else {
                "UNVERIFIABLE"
            };
            let named: Vec<String> = self
                .missing()
                .iter()
                .map(|found| format!("{} ({}: {})", found.claim_id, found.kind, found.field))
                .collect();
            notes.push(format!(
                "missing_field ({how}): {} — the kind declares the field `required: true` and the claim carries no \
                 value for it (absent is UNKNOWN, never a default); supply it, or relax the declaration",
                named.join(", ")
            ));
        }
        if !self.dangling().is_empty() {
            let named: Vec<String> = self
                .dangling()
                .iter()
                .map(|found| format!("{} ({}: {})", found.claim_id, found.field, found.target))
                .collect();
            notes.push(format!(
                "dangling_field_ref (UNVERIFIABLE): {} — a `ref` field names a claim this corpus does not hold \
                 (the target may be authored later; a `ref` field is not a relation and adds no edge)",
                named.join(", ")
            ));
        }
        let reported_only = !self.unrelated.is_empty() || !self.undeclared().is_empty();
        if self.exit() == 0 && !reported_only {
            notes.push("no structural omission found".to_string());
        } else if self.exit() == 0 {
            notes.push(
                "no structural omission judged (reported-only claims are listed above)".to_string(),
            );
        }
        notes
    }

    /// The step's label: what was checked, never what is true.
    pub fn label() -> String {
        Self::label_for(&Policy::default())
    }

    /// [`Findings::label`] for a policy: `need design`, a declared `.req/areas` and declared `.req/kinds/` each add a
    /// check, and the label counts them.
    pub fn label_for(policy: &Policy) -> String {
        format!(
            "structural omissions ({} checks)",
            CHECKS
                + usize::from(policy.need_design)
                + usize::from(policy.areas.is_some())
                + usize::from(policy.kinds)
        )
    }
}

/// The claims naming an area `declared` does not hold, sorted by claim id. A claim with no area is no
/// finding: filing is optional, and `strict areas` judges what a claim says, not what it leaves out.
pub fn undeclared_areas(rows: &[Row], declared: &[AreaEntry]) -> Vec<UndeclaredArea> {
    let known: BTreeSet<&str> = declared.iter().map(|entry| entry.path.as_str()).collect();
    let mut found: Vec<UndeclaredArea> = rows
        .iter()
        .filter_map(|row| {
            let area = row.area.as_deref()?;
            (!known.contains(area)).then(|| UndeclaredArea {
                claim_id: row.claim_id.clone(),
                area: area.to_string(),
            })
        })
        .collect();
    found.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    found.dedup();
    found
}

/// Ids of claims with no outgoing relation and no incoming one from any claim in the corpus.
pub fn unrelated(rows: &[Row], roots: &BTreeSet<String>) -> Vec<String> {
    let targets: BTreeSet<&str> = rows
        .iter()
        .flat_map(|row| row.relations.iter().map(|(_, to)| to.as_str()))
        .collect();
    let mut ids: Vec<String> = rows
        .iter()
        .filter(|row| row.relations.is_empty() && !targets.contains(row.claim_id.as_str()))
        .filter(|row| !roots.contains(&row.claim_id))
        .map(|row| row.claim_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The ids every signed manifest under `baselines/` names, the manifests read, and the ones that
/// did not count (with the reason). A missing `baselines/` directory is no manifest, not an error.
fn signed_coverage(root: &Path) -> (BTreeSet<String>, Vec<String>, Vec<String>) {
    let mut covered = BTreeSet::new();
    let mut read = Vec::new();
    let mut not_counted = Vec::new();
    let dir = root.join(BASELINES);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (covered, read, not_counted);
        }
        Err(error) => {
            not_counted.push(format!("{BASELINES}/ cannot be listed: {error}"));
            return (covered, read, not_counted);
        }
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    for name in names {
        let shown = format!("{BASELINES}/{name}");
        let path = dir.join(&name);
        if !path.is_file() {
            continue;
        }
        // A signature is the default `<manifest>.minisig` or, since ADR 0042, any party's
        // `<manifest>.<key-id>.minisig`. Whether one verifies is `req baseline --verify`'s question.
        if !dir.join(format!("{name}.minisig")).is_file()
            && crate::baseline::keyed_signatures(&path).is_empty()
        {
            not_counted.push(format!(
                "{shown} has no signature beside it ({shown}.minisig or {shown}.<key-id>.minisig): \
                 an emitted manifest is not yet a Baseline"
            ));
            continue;
        }
        let parsed = std::fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| crate::baseline::from_json(&text));
        match parsed {
            Ok(manifest) => {
                covered.extend(manifest.claims.into_iter().map(|claim| claim.claim_id));
                read.push(shown);
            }
            Err(reason) => not_counted.push(format!("{shown} cannot be read: {reason}")),
        }
    }
    (covered, read, not_counted)
}

/// Run the three checks over the declared release contexts (the default variant when the list
/// declares none — the caller passes one empty context).
pub fn evaluate(root: &Path, releases: &[Context], policy: &Policy) -> Result<Findings, String> {
    let mut applies_somewhere: BTreeSet<String> = BTreeSet::new();
    let mut undecided_somewhere: BTreeSet<String> = BTreeSet::new();
    let mut withdrawn_somewhere: BTreeSet<String> = BTreeSet::new();
    let mut all_rows: Option<Vec<Row>> = None;
    for context in releases {
        let rows = crate::view::load_rows(context, root)?;
        for row in &rows {
            match row.applies {
                Applies::Yes => {
                    applies_somewhere.insert(row.claim_id.clone());
                }
                Applies::Unknown => {
                    undecided_somewhere.insert(row.claim_id.clone());
                }
                Applies::No => {
                    if !row.superseded_by.is_empty() {
                        withdrawn_somewhere.insert(row.claim_id.clone());
                    }
                }
            }
        }
        if all_rows.is_none() {
            all_rows = Some(rows);
        }
    }
    let rows = match all_rows {
        Some(rows) => rows,
        None => crate::view::load_rows(&Context::new(), root)?,
    };
    let mut ids: Vec<&str> = rows.iter().map(|row| row.claim_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();

    // Excluded by every declared release. A claim some line leaves undecided is already that
    // line's UNVERIFIABLE (ADR 0015); calling it a defect here would outrank the open question
    // with a FAIL nobody established, so it is not named twice. A claim a release *withdraws* (its
    // successor applies there) was examined by that release and is kept on purpose: ADR 0049
    // decision 3 rejects deletion, so telling its author to "retire it" would contradict the decision.
    let in_no_release: Vec<String> = ids
        .iter()
        .filter(|id| {
            !applies_somewhere.contains(**id)
                && !undecided_somewhere.contains(**id)
                && !withdrawn_somewhere.contains(**id)
        })
        .map(|id| (*id).to_string())
        .collect();
    let (covered, baselines, not_counted) = signed_coverage(root);
    let in_no_baseline: Vec<String> = ids
        .iter()
        .filter(|id| applies_somewhere.contains(**id) && !covered.contains(**id))
        .map(|id| (*id).to_string())
        .collect();
    // `need design` (ADR 0040): a claim some release applies and no element declares a link for. A
    // design root that cannot be read leaves the question open, so the step is UNVERIFIABLE with the
    // reason, never a list read as empty.
    let unsatisfied = if policy.need_design {
        let declared = crate::satisfy::declared_claims(root).map_err(|e| e.lines().join("; "))?;
        Some(
            ids.iter()
                .filter(|id| applies_somewhere.contains(**id) && !declared.contains(**id))
                .map(|id| (*id).to_string())
                .collect(),
        )
    } else {
        None
    };
    // ADR 0056 decision 4: the two field findings, only when the repository declares a kind. The declarations are
    // read once, here; a corpus that loaded has declarations that read.
    let declarations =
        crate::kinds::Declarations::read(root).map_err(|e| e.messages().join("; "))?;
    let (missing_field, dangling_field_ref) = if declarations.is_empty() {
        (None, None)
    } else {
        let universe: BTreeSet<&str> = ids.iter().copied().collect();
        (
            Some(missing_fields(&rows, &declarations)),
            Some(dangling_field_refs(&rows, &universe)),
        )
    };
    Ok(Findings {
        in_no_release,
        in_no_baseline,
        unrelated: unrelated(&rows, &policy.roots),
        missing_field,
        missing_field_judged: policy.strict_fields && !declarations.is_empty(),
        dangling_field_ref,
        unrelated_judged: policy.strict_unrelated,
        undeclared_area: policy
            .areas
            .as_ref()
            .map(|declared| undeclared_areas(&rows, declared)),
        // `strict areas` judges the registry's claims; with no registry there is nothing to judge.
        undeclared_area_judged: policy.strict_areas && policy.areas.is_some(),
        unsatisfied,
        baselines,
        not_counted,
    })
}

/// Applying claims (in this selection) that no design element declares a link for — what `req audit`
/// lists under *Structural omissions* when `.req/completeness` says `need design` (ADR 0040).
pub fn unsatisfied_in(rows: &[Row], root: &Path) -> Result<Vec<String>, crate::satisfy::LinkError> {
    let declared = crate::satisfy::declared_claims(root)?;
    let mut ids: Vec<String> = rows
        .iter()
        .filter(|row| row.applies == Applies::Yes && !declared.contains(&row.claim_id))
        .map(|row| row.claim_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// The ids the corpus holds, for validating `.req/completeness` before anything runs.
pub fn known_ids(root: &Path) -> Result<BTreeSet<String>, String> {
    Ok(crate::view::load_rows(&Context::new(), root)?
        .into_iter()
        .map(|row| row.claim_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn the_policy_grammar_takes_roots_and_strict_unrelated_and_nothing_else() {
        let policy = parse_policy(
            "# c\nREQ-1 # root\n\nstrict unrelated\r\n",
            &known(&["REQ-1"]),
        )
        .expect("parses");
        assert!(policy.strict_unrelated);
        assert_eq!(policy.roots, known(&["REQ-1"]));
        let errors = parse_policy(
            "strict\nREQ-2\nstrict unrelated extra\n",
            &known(&["REQ-1"]),
        )
        .expect_err("three bad lines");
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[1].starts_with(".req/completeness:2:"), "{errors:?}");
    }

    #[test]
    fn the_areas_grammar_is_path_then_label_and_names_every_bad_line() {
        let entries =
            parse_areas("# order\n\ndiagnostics/startup  Start-up (診断) \r\nplain C# support\n")
                .expect("parses");
        assert_eq!(entries[0].path, "diagnostics/startup");
        assert_eq!(entries[0].label, "Start-up (診断)");
        assert_eq!(entries[1].label, "C# support", "an inline # is label text");
        let errors = parse_areas("a A\na B\nb\nBad x\n").expect_err("three bad lines");
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[0].starts_with(".req/areas:2:"), "{errors:?}");
    }

    #[test]
    fn the_exit_ranks_a_failure_above_an_open_question() {
        let mut findings = Findings {
            in_no_baseline: vec!["A".into()],
            baselines: vec!["b".into()],
            ..Findings::default()
        };
        assert_eq!(findings.exit(), 3);
        findings.in_no_release = vec!["B".into()];
        assert_eq!(findings.exit(), 1);
        let reported = Findings {
            unrelated: vec!["C".into()],
            baselines: vec!["b".into()],
            ..Findings::default()
        };
        assert_eq!(
            reported.exit(),
            0,
            "unrelated is reported only unless strict"
        );
        let strict = Findings {
            unrelated_judged: true,
            ..reported
        };
        assert_eq!(strict.exit(), 1);
    }
}
