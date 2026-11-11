//! Structural omissions (ADR 0039): what req can prove is missing from files it already reads.
//!
//! Three findings (a fourth with `need design`), each naming the claim, and never a verdict that the requirements are complete —
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

/// Where the committed Baselines live.
const BASELINES: &str = "baselines";

/// The number of checks the step runs, printed so a pass says what it examined.
const CHECKS: usize = 3;

/// `.req/completeness`, validated: the declared roots, whether `unrelated` is promoted to FAIL and
/// whether every applying claim needs a design element (`need design`, ADR 0040).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Policy {
    pub roots: BTreeSet<String>,
    pub strict_unrelated: bool,
    pub need_design: bool,
}

/// Validate `.req/completeness` against the corpus's ids. Grammar as the other `.req/` lists: `#`
/// starts a comment, a blank line is skipped, a trailing CR is stripped. A line is either one claim
/// id (a root), `strict unrelated` or `need design`. Anything else — an unknown word, an id the corpus
/// does not hold, two ids on one line — is refused naming `<file>:<line>`, every bad line in one run.
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
            ["need", "design"] => policy.need_design = true,
            [id] if known.contains(*id) => {
                policy.roots.insert((*id).to_string());
            }
            [id] => errors.push(format!(
                "{COMPLETENESS}:{number}: `{id}` is neither a claim id this corpus holds nor a known \
                 word: a line is one root claim id, `strict unrelated` or `need design`"
            )),
            many => errors.push(format!(
                "{COMPLETENESS}:{number}: `{}` is not a line this file takes: one root claim id per \
                 line, `strict unrelated` or `need design`",
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

/// What the step found. Every list is sorted by claim id and holds each id once.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Findings {
    pub in_no_release: Vec<String>,
    pub in_no_baseline: Vec<String>,
    pub unrelated: Vec<String>,
    /// Whether `unrelated` decides the verdict (`strict unrelated`) or is reported only.
    pub unrelated_judged: bool,
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
        if !self.in_no_release.is_empty() || (self.unrelated_judged && !self.unrelated.is_empty()) {
            1
        } else if !self.in_no_baseline.is_empty()
            || !self.not_counted.is_empty()
            || self.unsatisfied.as_ref().is_some_and(|ids| !ids.is_empty())
        {
            3
        } else {
            0
        }
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
        if self.exit() == 0 && self.unrelated.is_empty() {
            notes.push("no structural omission found".to_string());
        } else if self.exit() == 0 {
            notes.push(
                "no structural omission judged (unrelated claims are listed above)".to_string(),
            );
        }
        notes
    }

    /// The step's label: what was checked, never what is true.
    pub fn label() -> String {
        Self::label_for(false)
    }

    /// [`Findings::label`] for a policy: `need design` adds a check, and the label counts it.
    pub fn label_for(need_design: bool) -> String {
        format!(
            "structural omissions ({} checks)",
            CHECKS + usize::from(need_design)
        )
    }
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
        if !dir.join(format!("{name}.minisig")).is_file() {
            not_counted.push(format!(
                "{shown} has no signature beside it ({shown}.minisig): an emitted manifest is not yet \
                 a Baseline"
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
                Applies::No => {}
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
    // with a FAIL nobody established, so it is not named twice.
    let in_no_release: Vec<String> = ids
        .iter()
        .filter(|id| !applies_somewhere.contains(**id) && !undecided_somewhere.contains(**id))
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
    Ok(Findings {
        in_no_release,
        in_no_baseline,
        unrelated: unrelated(&rows, &policy.roots),
        unrelated_judged: policy.strict_unrelated,
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
