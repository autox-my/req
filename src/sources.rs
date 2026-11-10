//! Does a line anchor in a claim's `rationale.source` still point at a line? (issue #19)
//!
//! A claim records where it came from: `rationale.source: docs/plan.md#L390`. The plan is edited, the
//! file shrinks or is deleted, and the anchor goes on pointing at something that is not there. Nothing
//! used to notice, and the source is the one field a reviewer uses to ask "which part of the plan is
//! this claim for". This module answers the part a machine can answer for certain: **is the anchor a
//! line of a file that exists?**
//!
//! What it does *not* do is decide whether the line still says what the claim was written from. That
//! is a human reading; a line count cannot tell, and a guess would be a false FAIL on a release gate.
//! So only a source that is unmistakably a repository path **with a line anchor** is judged:
//!
//! * `path#L<n>`, `path#L<n>-L<m>`, `path#L<n>-<m>` — no whitespace, no `://`, not absolute, no `..`;
//! * anything else (prose, a URL, a claim id, a heading anchor, a path with no anchor) is left alone.
//!
//! The path is looked up beside the corpus (`claims/`'s parent) and, if the corpus sits in a
//! subdirectory of a git checkout, at the checkout's root: a source is "a repo path".
//!
//! Three verdicts, and no fourth: the anchor holds; the anchor is *definitely* wrong (the file does not
//! exist, is a directory, or has fewer lines; or the anchor is not a line at all); or the file exists
//! and **cannot be read**, which is "cannot be told" (UNVERIFIABLE), never a defect and never a pass.

use std::path::{Component, Path, PathBuf};

use crate::view::Row;

/// A line anchor: the first and last line it names (equal for `#L12`).
#[derive(Debug, PartialEq, Eq)]
struct Anchor {
    first: usize,
    last: usize,
}

/// The claims judged, and what was found. `checked` is the number of sources that were judged at all,
/// so a pass over zero of them can be told from a pass over some.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SourceReport {
    pub checked: usize,
    /// Definite defects, one line each, in claim order.
    pub failed: Vec<String>,
    /// Anchors into a file that exists but could not be read.
    pub unverifiable: Vec<String>,
}

/// Parse `L12`, `L12-L20` or `L12-20` (the part after `#`). `None` is "not a line anchor": the
/// source is then not judged.
fn parse_anchor(fragment: &str) -> Option<(usize, usize)> {
    let rest = fragment.strip_prefix('L')?;
    let (first, last) = match rest.split_once('-') {
        Some((a, b)) => (a, b.strip_prefix('L').unwrap_or(b)),
        None => (rest, rest),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(first) || !digits(last) {
        return None;
    }
    Some((first.parse().ok()?, last.parse().ok()?))
}

/// `(path, anchor)` when `source` is a repository path with a line anchor, else `None`.
fn split_source(source: &str) -> Option<(&str, Anchor, &str)> {
    let source = source.trim();
    if source.is_empty() || source.chars().any(char::is_whitespace) || source.contains("://") {
        return None;
    }
    let (path, fragment) = source.split_once('#')?;
    let (first, last) = parse_anchor(fragment)?;
    let p = Path::new(path);
    let inside = !path.is_empty()
        && !p.is_absolute()
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    inside.then_some((path, Anchor { first, last }, fragment))
}

/// How many lines the bytes hold: a final line without a newline still counts, an empty file has none.
fn line_count(bytes: &[u8]) -> usize {
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    newlines + usize::from(bytes.last().is_some_and(|b| *b != b'\n'))
}

/// Judge the `rationale.source` of every row given. `roots` are the directories a repo path is looked
/// up in, in order (the corpus root, then the checkout root when that is different).
pub fn check(rows: &[&Row], roots: &[PathBuf]) -> SourceReport {
    let mut report = SourceReport::default();
    for row in rows {
        let source = row.rationale_source.trim();
        let Some((path, anchor, _)) = split_source(source) else {
            continue;
        };
        report.checked += 1;
        let who = format!("{}: rationale.source {source}", row.claim_id);
        if anchor.first == 0 || anchor.last == 0 {
            report
                .failed
                .push(format!("{who} is not a line (lines start at 1)"));
            continue;
        }
        if anchor.last < anchor.first {
            report
                .failed
                .push(format!("{who} is not a range (it ends before it starts)"));
            continue;
        }
        let Some(found) = roots.iter().map(|r| r.join(path)).find(|p| p.exists()) else {
            report
                .failed
                .push(format!("{who} names a file that does not exist"));
            continue;
        };
        if found.is_dir() {
            report.failed.push(format!(
                "{who} names a directory, and a line anchor needs a file"
            ));
            continue;
        }
        match std::fs::read(&found) {
            Ok(bytes) => {
                let lines = line_count(&bytes);
                if anchor.last > lines {
                    report.failed.push(format!(
                        "{who} is beyond the last line of {path} (it has {lines})"
                    ));
                }
            }
            Err(err) => report.unverifiable.push(format!(
                "{who} cannot be read, so its length is unknown ({err})"
            )),
        }
    }
    report
}

/// The directories a repo path is resolved against: the corpus root, then the nearest ancestor that
/// holds a `.git` (a directory, or the file a worktree has) when it is not the corpus root itself.
pub fn roots_for(corpus_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![corpus_root.to_path_buf()];
    if let Some(checkout) = corpus_root
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .filter(|dir| *dir != corpus_root)
    {
        roots.push(checkout.to_path_buf());
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_is_read_in_its_three_spellings_and_nothing_else() {
        assert_eq!(parse_anchor("L12"), Some((12, 12)));
        assert_eq!(parse_anchor("L12-L20"), Some((12, 20)));
        assert_eq!(parse_anchor("L12-20"), Some((12, 20)));
        for not in [
            "", "L", "l12", "12", "L1x", "L1-", "L-3", "L1-L", "imaging", "L1-2-3", "L+1",
        ] {
            assert_eq!(parse_anchor(not), None, "{not:?}");
        }
    }

    #[test]
    fn lines_are_counted_the_way_an_editor_numbers_them() {
        assert_eq!(line_count(b""), 0);
        assert_eq!(line_count(b"a"), 1);
        assert_eq!(line_count(b"a\n"), 1);
        assert_eq!(line_count(b"a\nb"), 2);
        assert_eq!(line_count(b"\n\n"), 2);
    }
}
