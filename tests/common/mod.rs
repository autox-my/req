//! A `Row` fixture shared by the view, impact and gate tests.
//!
//! Separate test targets cannot share code except through a module like this one, and
//! three copies of the same fixture would drift apart — which is the failure this whole
//! project is about.

// This module is compiled into *every* test target that declares `mod common;`, so a
// helper one target does not use is not dead code — it is in use by another.
#![allow(dead_code)]

use req::fill::StoredState;
use req::model::{Rationale, Relation, rationale_digest, relations_digest};
use req::select::Applies;
use req::view::Row;

/// The full rationale a row's provenance fields stand for. The attested digest is over all
/// four fields, and these fixtures carry none of the optional ones — this is the single place
/// the fixture's rationale shape is written, so the hash and the displayed provenance cannot
/// drift apart. Public so every row-fixture target shares one shape instead of keeping its own
/// copy: this suite's stated failure mode is fixture copies drifting apart.
pub fn rationale_of(source: &str, date: &str) -> Rationale {
    Rationale {
        source: source.to_string(),
        date: date.to_string(),
        rejected_alternatives: vec![],
        assumptions: vec![],
    }
}

/// The date every fixture's provenance carries — one home, so the bare literal is not repeated
/// at every call site across the fixture files.
pub const FIXTURE_DATE: &str = "2026-10-07";

/// A Row's relation columns and the digest over the same relations, derived together so the two
/// cannot be edited apart. Takes the tuple form `row()` already receives, so the conversion to
/// the typed relations is written here — once — rather than kept in step at every call site.
pub fn relation_columns(rels: &[(&str, &str)]) -> (Vec<(String, String)>, String) {
    let relations: Vec<Relation> = rels
        .iter()
        .map(|(kind, to)| Relation {
            kind: kind.to_string(),
            to: to.to_string(),
        })
        .collect();
    let digest = relations_digest(&relations);
    let columns = relations.into_iter().map(|r| (r.kind, r.to)).collect();
    (columns, digest)
}

/// The provenance a row displays, with the attested digest over the same rationale. A named
/// struct, not a tuple: all three fields are `String`, so a transposed tuple binding would
/// compile silently and pair the digest with the wrong rationale — the exact drift this helper
/// exists to prevent (`relation_columns`' tuple is safe only because its element types differ).
pub struct Provenance {
    pub source: String,
    pub date: String,
    pub hash: String,
}

/// The rationale-side mirror of `relation_columns`: the displayed provenance and the attested
/// digest over the same rationale, derived together, so pairing the hash to the source and date
/// it covers rests on one definition rather than per-call-site discipline.
pub fn rationale_columns(source: &str, date: &str) -> Provenance {
    let rationale = rationale_of(source, date);
    Provenance {
        hash: rationale_digest(&rationale),
        source: rationale.source,
        date: rationale.date,
    }
}

/// A settled, verified, fully-provenanced claim with the given outgoing relations.
pub fn row(id: &str, applies: Applies, rels: &[(&str, &str)]) -> Row {
    // Real digests over this row's own content: a placeholder hash beside real relations
    // is a Row state `load_rows` can never produce, and a fixture that could not exist
    // proves nothing about the layers above it.
    let (relations, relations_hash) = relation_columns(rels);
    let provenance = rationale_columns(
        "docs/adr/0007-tier1-collapses-only-on-meaning-preserving-rules.md",
        FIXTURE_DATE,
    );
    Row {
        claim_id: id.to_string(),
        statement: format!("{id} shall log each command."),
        modality: "SHALL".to_string(),
        address: format!("{id:0>12}"),
        applies,
        selectors: Vec::new(),
        relations,
        relations_hash,
        rationale_hash: provenance.hash,
        tests: Vec::new(),
        tests_hash: req::model::tests_digest(&[], req::model::Evidence::Machine),
        evidence: req::model::Evidence::Machine,
        stored: StoredState::Verified,
        rationale_source: provenance.source,
        rationale_date: provenance.date,
    }
}

pub fn stored(mut row: Row, state: StoredState) -> Row {
    row.stored = state;
    row
}

pub fn rationale(mut row: Row, source: &str) -> Row {
    // The provenance and its attested digest are derived together, so a changed source cannot
    // leave the old hash behind — the unproducible row this module exists to prevent.
    let provenance = rationale_columns(source, &row.rationale_date);
    row.rationale_source = provenance.source;
    row.rationale_date = provenance.date;
    row.rationale_hash = provenance.hash;
    row
}

/// The releases this repository declares, from `.req/variants`, as raw `key=value ...` lines.
///
/// **One reader, so the tests and the script cannot disagree about which lines exist.** The grammar
/// is `scripts/check.sh`'s: `#` starts a comment, and a line that is empty after stripping it is
/// skipped. A second, differently written parser is how the tests come to assert the gate for one set
/// of releases while the gate runs another.
pub fn variant_lines() -> Vec<String> {
    std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".req/variants"),
    )
    .expect(".req/variants declares this corpus's releases")
    .lines()
    .map(|line| line.split('#').next().unwrap_or("").trim().to_string())
    .filter(|line| !line.is_empty())
    .collect()
}

/// The toolchain home this checkout keeps for `scripts/check.sh`, when it has one: `.tools/home`
/// beside the git common dir (.tools/README.md), which a feature worktree shares with the main
/// checkout. `git` is a hard requirement of these tests: its failure is reported here, with git's
/// own message, rather than swallowed into an empty path.
#[cfg(unix)]
pub fn workspace_toolchain_home(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let common = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .expect("git is required to locate the shared toolchain home");
    assert!(
        common.status.success(),
        "git rev-parse --git-common-dir failed: {}",
        String::from_utf8_lossy(&common.stderr)
    );
    let common = String::from_utf8_lossy(&common.stdout).trim().to_string();
    std::path::PathBuf::from(common)
        .parent()
        .map(|main| main.join(".tools/home"))
        .filter(|home| home.is_absolute() && home.join(".rustup/toolchains").exists())
}

/// Point `command`'s HOME at the toolchain home `scripts/check.sh` should see.
///
/// The workspace's `.tools/home` when it has one, **else the ambient HOME** — CI installs the pinned
/// channel into `$HOME/.rustup` and means it. One copy, in the one place test binaries can share:
/// `tests/baseline.rs` had this and `tests/decision.rs` a byte-identical second, and a third written
/// for one new test demanded the workspace layout and panicked in CI while the gate was green here.
#[cfg(unix)]
pub fn point_home_at_toolchain(command: &mut std::process::Command) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let home_has_rustup = std::env::var_os("HOME")
        .is_some_and(|h| std::path::Path::new(&h).join(".rustup/toolchains").exists());
    match workspace_toolchain_home(&root) {
        Some(workspace) => {
            command.env("HOME", workspace);
        }
        None if home_has_rustup => {}
        None => panic!(
            "no toolchain home for check.sh: this checkout has no .tools/home/.rustup/toolchains \
             and $HOME/.rustup/toolchains does not exist either (see .tools/README.md)"
        ),
    }
}
