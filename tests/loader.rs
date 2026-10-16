//! F2/F3 — the strict corpus loader: an unreadable-*as-a-claim* corpus is an error,
//! never a silent drop.
//!
//! Each test builds a throwaway corpus under `CARGO_TARGET_TMPDIR` and asks the
//! loader (and, for the exit-code cases, the `canon` command) about it. The committed
//! corpus is the *other* fixture: it must still load, which is what makes the
//! strictness non-breaking rather than merely stricter.
//!
//! Vocabulary: a **claim** is one file in `claims/` whose filename is its `claim_id`
//! (docs/claim-format.md). Identity is a content address, so a file that vanishes
//! from the corpus deletes a requirement — the asymmetry this loader exists for.

use std::path::{Path, PathBuf};

use req::canon::{
    ClaimSchema, claim_schema, classify_claim_schema, classify_claim_schema_at, load_claims,
    run_claim_diagnostics_at,
};

/// Run the compiled binary against a fixture root. The *class* is a CLI contract — `canon
/// --verify` exits 0 verified, 1 mismatch, 3 not judgeable — so the exit codes are read here
/// rather than inferred from a library return value.
fn req(root: &Path, args: &[&str]) -> (i32, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_req"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

/// A minimal but *valid* registry: the diagnostics command loads it before the
/// claims, and a fixture that failed here would exit 3 for the wrong reason.
const REGISTRY: &str = "\
schema: req.subject_registry/0
version: 1
subjects:
  REQ_TOOL:
    label: The req tool
    aliases:
      - the req tool
";

/// A real claim shape (claims/REQ-0001.yml), parameterized by `claim_id` so a fixture
/// can misname a file or duplicate an id without also being malformed.
fn claim_text(claim_id: &str) -> String {
    format!(
        "\
schema: req.claim/0
claim_id: {claim_id}
statement: >-
  The req tool shall treat a single claim as the unit of requirements authorship,
  review, and merge.
modality: SHALL
subject: REQ_TOOL
attribute: unit_of_work
operator: EQ
value: claim
unit: none
conditions: []
applicability: []
exception: null
temporal: null
quantifier: null
unstructured: false
canonical:
  rules_v: 5
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/adr/0001-the-unit-is-a-claim.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
"
    )
}

/// A throwaway corpus root: `claims/` plus the registry the commands need.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("claims")).expect("claims dir");
        std::fs::create_dir_all(root.join("aliases")).expect("aliases dir");
        std::fs::write(root.join("aliases/subjects.yml"), REGISTRY).expect("registry");
        Fixture { root }
    }

    fn claim(self, file: &str, text: &str) -> Self {
        std::fs::write(self.root.join("claims").join(file), text).expect("claim file");
        self
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

/// Remove the fixture when the test ends — including on a panic, which is exactly
/// when a half-written corpus is most likely to be mistaken for real state. The
/// pre-create cleanup keeps reruns deterministic; this keeps `target/` clean.
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ---------------------------------------------------------------------------
// F2 — a misspelled or omitted key is a parse error, not a default
// ---------------------------------------------------------------------------

#[test]
fn a_misspelled_key_is_a_parse_error_and_exits_3() {
    let f = Fixture::new("f2-typo").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace("applicability:", "applicabilty:"),
    );

    let err = load_claims(f.path()).expect_err("a misspelled key must not load");
    assert!(
        err.contains("applicabilty"),
        "the error must name the unknown field: {err}"
    );
    assert_eq!(
        run_claim_diagnostics_at(f.path()),
        3,
        "a claim that does not parse is UNVERIFIABLE, never a pass"
    );
}

#[test]
fn a_dropped_relations_key_is_a_parse_error_and_exits_3() {
    // `deny_unknown_fields` is half a fix: it constrains extra keys, never a missing
    // one. Dropping `relations` leaves the address untouched while the edges vanish
    // from `trace` and `impact`, so the key itself must be required.
    let f = Fixture::new("f2-no-relations").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace("relations: []\n", ""),
    );

    let err = load_claims(f.path()).expect_err("a missing relations key must not load");
    assert!(
        err.contains("relations"),
        "the error must name the missing field: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

// ---------------------------------------------------------------------------
// F3 — a claim file that is not where the contract says it is is an error
// ---------------------------------------------------------------------------

#[test]
fn a_lowercase_claim_file_errors_rather_than_vanishing() {
    let f = Fixture::new("f3-lowercase").claim("req-0008.yml", &claim_text("REQ-0008"));

    let err = load_claims(f.path()).expect_err("req-0008.yml must not vanish silently");
    assert!(
        err.contains("req-0008.yml"),
        "the error must name the file: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn a_yaml_extension_errors_rather_than_vanishing() {
    let f = Fixture::new("f3-yaml-ext").claim("REQ-0008.yaml", &claim_text("REQ-0008"));

    let err = load_claims(f.path()).expect_err("REQ-0008.yaml must not vanish silently");
    assert!(
        err.contains("REQ-0008.yaml"),
        "the error must name the file: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn a_stem_that_is_not_the_claim_id_errors() {
    let f = Fixture::new("f3-stem").claim("REQ-0009.yml", &claim_text("REQ-0010"));

    let err = load_claims(f.path()).expect_err("filename = claim_id is the contract");
    assert!(err.contains("REQ-0009"), "{err}");
    assert!(err.contains("REQ-0010"), "{err}");
    assert_eq!(
        run_claim_diagnostics_at(f.path()),
        3,
        "a misplaced claim is UNVERIFIABLE, never a pass"
    );
}

#[test]
fn duplicate_claim_ids_error_and_the_content_is_compared() {
    // Two files can carry one `claim_id` in exactly one way once the stem rule holds:
    // the `.yaml` twin of a `.yml`. Comparing content, not just the id, separates the
    // fully-silent case (an identical copy) from the divergent one (a rewrite that
    // would collapse two meanings into one address).
    let text = claim_text("REQ-0007");
    let identical = Fixture::new("f3-dup-identical")
        .claim("REQ-0007.yml", &text)
        .claim("REQ-0007.yaml", &text);
    let err = load_claims(identical.path()).expect_err("a duplicate id must not load");
    assert!(err.contains("REQ-0007"), "{err}");
    assert!(
        err.contains("identical content"),
        "an identical duplicate must be named as such: {err}"
    );

    let divergent = Fixture::new("f3-dup-divergent")
        .claim("REQ-0007.yml", &text)
        .claim(
            "REQ-0007.yaml",
            &text.replace("value: claim", "value: tool"),
        );
    let err = load_claims(divergent.path()).expect_err("a duplicate id must not load");
    assert!(
        err.contains("content differs"),
        "a divergent duplicate must be named as a content clash, not just an id clash: {err}"
    );
}

// ---------------------------------------------------------------------------
// Review findings on the loader itself
// ---------------------------------------------------------------------------

#[test]
fn a_dropped_rationale_key_is_a_parse_error_and_exits_3() {
    // The same shape as the dropped `relations:` key. The rationale sits outside the
    // canonical form, so losing a list leaves the address untouched and nothing else
    // in the tooling ever reads these fields — the loss would be silent. AGENTS.md: a
    // rationale is an object with source, date, rejected alternatives, assumptions.
    // The anchors carry their indentation: dropping a bare `assumptions: []` would
    // leave the removed line's indent on the next key, which YAML then reads as a
    // continuation of `date:` rather than as a missing rationale key.
    for (line, field) in [
        ("  rejected_alternatives: []\n", "rejected_alternatives"),
        ("  assumptions: []\n", "assumptions"),
    ] {
        let f = Fixture::new("f2-rationale")
            .claim("REQ-0001.yml", &claim_text("REQ-0001").replace(line, ""));

        let err = load_claims(f.path()).expect_err("a missing rationale key must not load");
        assert!(
            err.contains(field),
            "the error must name the missing field: {err}"
        );
        assert_eq!(run_claim_diagnostics_at(f.path()), 3, "{field}");
    }
}

#[test]
fn non_claim_entries_in_claims_are_left_alone() {
    // The tolerant side of the gate. The committed corpus depends on it —
    // `claims/README.md` is not a claim — so a gate tightened to error on every
    // unrecognized name would be wrong, and nothing else pins that direction.
    let f = Fixture::new("f3-strays").claim("REQ-0001.yml", &claim_text("REQ-0001"));
    for stray in ["README.md", ".DS_Store", "notes.txt", "archive.tar.gz"] {
        std::fs::write(f.path().join("claims").join(stray), "not a claim\n").expect("stray file");
    }

    let claims = load_claims(f.path()).expect("strays are not claims and are left alone");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].claim.claim_id, "REQ-0001");
}

#[test]
fn a_misspelled_nested_key_is_a_parse_error_and_exits_3() {
    // `deny_unknown_fields` at the top level is not enough. `rationale`'s two
    // defaulted lists mean a typo one level down loaded as an empty list: the same
    // silent-loss class F2 exists for, one nesting level past the fix.
    let f = Fixture::new("f2-nested-typo").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace("rejected_alternatives:", "rejected_alternativs:"),
    );

    let err = load_claims(f.path()).expect_err("a misspelled nested key must not load");
    assert!(
        err.contains("rejected_alternativs"),
        "the error must name the unknown field: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn a_claim_renamed_out_of_the_contract_errors_rather_than_vanishing() {
    // Claim-shaped names that the extension-only candidate gate used to skip. Each
    // one is a requirement deleted from the corpus in silence — the failure the
    // loader exists for — so renaming a claim must be loud, not a way to disable it.
    // The leading-dot and case-variant shapes are the same hole seen from the hidden
    // file and the lowercase-rename side: a claim renamed to `req-0008.txt` must be
    // reported, even though `REQ-<digits>.yml` is uppercase by contract.
    for file in [
        "REQ-0008.txt",
        "REQ-0008",
        "REQ-0008.yml.disabled",
        ".REQ-0008",
        ".REQ-0008.txt",
        "req-0008",
        "req-0008.txt",
        "Req-0008",
    ] {
        let f = Fixture::new("f3-renamed").claim(file, &claim_text("REQ-0008"));
        let err = load_claims(f.path()).expect_err("a claim-shaped rename must not vanish");
        assert!(err.contains(file), "the error must name the file: {err}");
        assert_eq!(run_claim_diagnostics_at(f.path()), 3, "{file}");
    }
}

#[test]
fn a_subdirectory_in_claims_is_a_corpus_error() {
    // `mv claims/REQ-0008.yml claims/archive/` is the natural way to "move it out of
    // the way", and it deleted the requirement in silence: the corpus is flat, so a
    // directory here is refused rather than skipped.
    let f = Fixture::new("f3-subdir").claim("REQ-0008.yml", &claim_text("REQ-0008"));
    let claims = f.path().join("claims");
    std::fs::create_dir_all(claims.join("archive")).expect("archive dir");
    std::fs::rename(
        claims.join("REQ-0008.yml"),
        claims.join("archive/REQ-0008.yml"),
    )
    .expect("move the claim out of the way");

    let err = load_claims(f.path()).expect_err("a claim filed in a subdirectory must not vanish");
    assert!(
        err.contains("archive"),
        "the error must name the directory: {err}"
    );
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

#[test]
fn an_empty_corpus_is_unverifiable_never_a_pass() {
    // `mv claims/*.yml claims/archive/` — or any whole-corpus accident — left a
    // loader that returned `Ok(vec![])`, a command that printed `claims: 0;
    // discrepancy flags: 0`, and exit 0. The fully-vanished corpus is the vanish
    // case, so it can never be a pass.
    let f = Fixture::new("f3-empty");

    let err = load_claims(f.path()).expect_err("an empty corpus must not load");
    assert!(err.contains("no claim files"), "{err}");
    assert_eq!(run_claim_diagnostics_at(f.path()), 3);
}

// ---------------------------------------------------------------------------
// The declared file-format version is judged, not decoration
// ---------------------------------------------------------------------------

/// The values a claim file's `schema:` may carry and what each one means for this build. The
/// **direction** decides the remedy, exactly as it does for `rules_v` in a Baseline manifest:
/// an older file is a migration candidate, a newer one is a question this build cannot answer
/// (the file is not wrong, the reader is old), and a value of no `req.claim/<n>` shape at all is
/// neither — it is a broken file, and it is reported as one rather than guessed at.
#[test]
fn a_mismatched_schema_is_never_reported_verified() {
    // The measured defect: `schema` was parsed and never compared, and the address does not
    // cover the field, so `req.claim/99` and `nonsense` both reported `verified` with
    // mismatches 0 — the one check that would have noticed could not.
    let f = Fixture::new("schema-mismatch").claim("REQ-0001.yml", &claim_text("REQ-0001"));
    let path = f.path().join("claims/REQ-0001.yml");

    // Fill first: an unfilled claim is a candidate (exit 3) for an unrelated reason, so the
    // fixture has to be verifiable *today* for the failure to be about the schema.
    let (fill_code, fill_text) = req(f.path(), &["canon", "--fill"]);
    assert_eq!(fill_code, 0, "{fill_text}");
    let (clean_code, clean_text) = req(f.path(), &["canon", "--verify"]);
    assert_eq!(
        clean_code, 0,
        "precondition: the filled fixture verifies under its own schema\n{clean_text}"
    );
    assert!(clean_text.contains("mismatches 0"), "{clean_text}");
    let current = std::fs::read_to_string(&path).expect("the filled fixture");

    for declared in [
        format!(
            "{}{}",
            req::canon::CLAIM_SCHEMA_PREFIX,
            req::canon::CLAIM_SCHEMA_V + 1
        ),
        "nonsense".to_string(),
    ] {
        let rewritten = current.replace(
            &format!("schema: {}", claim_schema()),
            &format!("schema: {declared}"),
        );
        assert!(
            rewritten.contains(&format!("schema: {declared}")),
            "the fixture's schema line was rewritten"
        );
        std::fs::write(&path, &rewritten).expect("rewrites the declared schema");

        let (code, text) = req(f.path(), &["canon", "--verify"]);
        assert_eq!(
            code, 3,
            "schema {declared:?} is UNVERIFIABLE, never a pass:\n{text}"
        );
        assert!(
            text.contains("NOT JUDGED"),
            "the file's parse must be reported as NOT JUDGED, not silently skipped:\n{text}"
        );
        assert!(
            !text.contains("verified"),
            "a claim whose schema this build cannot judge must never print `verified`:\n{text}"
        );
        assert_eq!(
            run_claim_diagnostics_at(f.path()),
            3,
            "the diagnostics command judges it too, not only `--verify`"
        );
    }
}

#[test]
fn a_newer_schema_names_the_reader_as_old_and_keeps_the_file() {
    let declared = format!(
        "{}{}",
        req::canon::CLAIM_SCHEMA_PREFIX,
        req::canon::CLAIM_SCHEMA_V + 1
    );
    let f = Fixture::new("schema-newer").claim(
        "REQ-0001.yml",
        &claim_text("REQ-0001").replace(
            &format!("schema: {}", claim_schema()),
            &format!("schema: {declared}"),
        ),
    );

    let err = load_claims(f.path()).expect_err("a schema this build does not write must not load");
    assert!(err.contains(&declared), "names the declared value: {err}");
    assert!(err.contains("NOT JUDGED"), "{err}");
    assert!(
        err.contains("newer"),
        "the direction decides the remedy: a file from a newer format is read with a build that \
         writes it, never re-filled with this one: {err}"
    );
    assert_eq!(
        classify_claim_schema(&declared),
        ClaimSchema::Newer(req::canon::CLAIM_SCHEMA_V + 1)
    );
}

#[test]
fn a_malformed_schema_is_a_broken_file_not_a_version_question() {
    // The class decision, pinned: a value that is not of the `req.claim/<n>` shape has no
    // direction to classify, and this build cannot tell a typo from an identifier of a naming
    // scheme it does not implement. It is reported as a broken file — the loader's class, the
    // same one every other unloadable corpus error takes — rather than asserted to be a defect
    // the tool cannot actually establish. See the PR body for why FAIL was rejected.
    for declared in [
        "nonsense",
        "",
        "req.claim/",
        "req.claim/abc",
        "req.claim",
        "claim/0",
        "req.claim/-1",
        "req.claim/ 0",
        // A numeral that parses but is not the identifier the doc states. `00` and `+0` both
        // *parse* to the current version, so a bare parse would report them `verified` — the same
        // silent acceptance this guard exists to remove, one character along. `value` already
        // carries the canonical-decimal rule ("0.3", never "0.30"); the schema numeral is held to
        // it too.
        "req.claim/00",
        "req.claim/+0",
        "req.claim/  ",
    ] {
        let f = Fixture::new("schema-malformed").claim(
            "REQ-0001.yml",
            &claim_text("REQ-0001").replace(
                &format!("schema: {}", claim_schema()),
                &format!("schema: {declared:?}"),
            ),
        );

        assert_eq!(
            classify_claim_schema(declared),
            ClaimSchema::Malformed,
            "{declared:?} is not of the req.claim/<n> shape"
        );
        let err = load_claims(f.path()).expect_err("a broken schema must not load");
        assert!(
            err.contains(&format!("{declared:?}")),
            "the error must quote the value the file declares ({declared:?}): {err}"
        );
        assert!(
            err.contains("broken file"),
            "a malformed value is not a version question: {err}"
        );
        assert!(err.contains("NOT JUDGED"), "{err}");
        assert_eq!(
            run_claim_diagnostics_at(f.path()),
            3,
            "{declared:?} is UNVERIFIABLE, never a pass and never a silent load"
        );
    }
}

#[test]
fn the_older_direction_classifies_and_is_worded_as_a_migration_candidate() {
    // No fixture can declare an *older* format while `CLAIM_SCHEMA_V` is 0 — that is what the
    // version being the first one means — so the direction is exercised against an explicit build
    // version rather than reasoned about: an untested branch is free to be wrong on the day the
    // first format bump arrives, which is the day it matters most. `rules_mismatch`'s direction is
    // reachable the same way, by a manifest declaring another `rules_v`.
    let older = format!("{}{}", req::canon::CLAIM_SCHEMA_PREFIX, 0);
    assert_eq!(
        classify_claim_schema_at(&older, 1),
        ClaimSchema::Older(0),
        "a file from the previous format is a migration candidate, not a broken file"
    );
    assert_eq!(
        classify_claim_schema_at(&older, 0),
        ClaimSchema::Current,
        "the same value against this build is Current"
    );

    // The wording is the contract: a reader who is told only "could not be read" goes looking for
    // a syntax error that is not there.
    let reason = ClaimSchema::Older(0)
        .reason(&older)
        .expect("a mismatch has a reason");
    assert!(reason.contains("NOT JUDGED"), "{reason}");
    assert!(reason.contains("predates"), "{reason}");
    assert!(
        reason.contains("migration candidate"),
        "an older format is a migration, and a format bump moves no address: {reason}"
    );
    assert!(
        reason.contains(&claim_schema()),
        "the remedy names the value to migrate to: {reason}"
    );
    assert_eq!(classify_claim_schema(&claim_schema()), ClaimSchema::Current);
}

/// Every `req.claim/<n>` literal one line carries. Scanning the line means a reformatting cannot
/// quietly drop the check.
fn line_schema_literals(line: &str) -> Vec<String> {
    let prefix = req::canon::CLAIM_SCHEMA_PREFIX;
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find(prefix) {
        let after = &rest[at + prefix.len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            found.push(format!("{prefix}{digits}"));
        }
        rest = after;
    }
    found
}

/// The document's **canonical declarations** of the file format: the `Status:` line and the
/// `| File format |` row of the version-axes table.
///
/// Anchored deliberately. `docs/claim-format.md` is where the older/newer axis is explained, so
/// it legitimately names other formats in prose; collecting *every* mention would redden on a
/// correct illustrative edit and invite weakening the oracle. The two anchors are the places the
/// contract states what this build's format *is*.
///
/// A missing anchor — or an anchor carrying no declaration, or more than one — is a loud `Err`,
/// never an empty list: a check that matches nothing must not read as a pass
/// (`tools/review/check_rules.py`'s lesson). The document stays the oracle either way; this only
/// says **which** literal in it is the declaration.
fn declared_schema_literals(text: &str) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let (mut seen_status, mut seen_row) = (false, false);
    for line in text.lines() {
        let anchor = if line.starts_with("Status:") {
            seen_status = true;
            "the `Status:` line"
        } else if line.starts_with("| File format |") {
            seen_row = true;
            "the `| File format |` version-axis row"
        } else {
            continue;
        };
        match line_schema_literals(line).as_slice() {
            [one] => found.push(one.clone()),
            other => {
                return Err(format!(
                    "{anchor} must declare exactly one `req.claim/<n>` value, found {}",
                    other.len()
                ));
            }
        }
    }
    if !seen_status {
        return Err(
            "docs/claim-format.md has no `Status:` line, so the format's canonical declaration \
             is missing"
                .to_string(),
        );
    }
    if !seen_row {
        return Err(
            "docs/claim-format.md has no `| File format |` row, so the version-axis declaration \
             is missing"
                .to_string(),
        );
    }
    Ok(found)
}

/// The document under test — read once, so both the oracle and the mutation cases start from the
/// bytes the repository actually ships.
fn claim_format_doc() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/claim-format.md"))
        .expect("the claim format doc is part of the tree")
}

/// The next `req.claim/<n>` after `literal`: a value the document does not state, so a mutation
/// cannot be mistaken for the untouched text.
fn next_declaration(literal: &str) -> String {
    let prefix = req::canon::CLAIM_SCHEMA_PREFIX;
    let n: u32 = literal
        .strip_prefix(prefix)
        .and_then(|numeral| numeral.parse().ok())
        .expect("a declaration is `req.claim/<digits>`");
    format!("{prefix}{}", n + 1)
}

/// The document with the **`Status:` line's** declaration replaced by `replacement`.
///
/// Aimed at the declaration rather than at the first literal in the file, and loud on every way
/// that can fail to happen: a document with no `Status:` line, a `Status:` line that does not
/// carry `from`, or a replacement that lands nowhere would otherwise leave Side 2 asserting over
/// text the mutation never touched — and a check that matches nothing must not read as a pass
/// (`tools/review/check_rules.py`'s lesson). The `| File format |` row is deliberately not the
/// mutation site: Side 2 exists to pin that the *declaration* is read, and the `Status:` line is
/// the first of the two.
fn replacing_the_status_declaration(doc: &str, from: &str, to: &str) -> String {
    let mut out = String::new();
    let mut replaced = false;
    for line in doc.lines() {
        if !replaced && line.starts_with("Status:") {
            assert!(
                line.contains(from),
                "the `Status:` line must declare {from:?}, and it says: {line}"
            );
            out.push_str(&line.replace(from, to));
            replaced = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    assert!(
        replaced,
        "docs/claim-format.md has no `Status:` line, so the mutation has nothing to aim at"
    );
    out
}

#[test]
fn the_schema_constant_equals_the_documented_literal() {
    // **The doc is the spec**, so it is the oracle: the constant is compared against the literal
    // in docs/claim-format.md, never derived from the loader's own value. A test that restated
    // `claim_schema()` would agree with the code whatever it said.
    let doc = claim_format_doc();
    // `.expect`: a document that lost its declaration errors here rather than asserting nothing.
    let literals = declared_schema_literals(&doc).expect("both canonical declarations are present");
    for literal in literals {
        assert_eq!(
            literal,
            claim_schema(),
            "docs/claim-format.md states this build's file format; the constant must not drift \
             from it"
        );
    }
}

/// The oracle is the **declaration**, not every mention. `docs/claim-format.md` is exactly where
/// the older/newer axis is explained, so a legitimate illustrative mention must not redden the
/// check — while a mutated **declaration** must. Both sides are the deliverable: one alone leaves
/// the fix free to either scan everything (fragile) or scan nothing (vacuous).
#[test]
fn the_doc_oracle_reads_the_declaration_and_ignores_the_prose() {
    let doc = claim_format_doc();
    let declared = declared_schema_literals(&doc).expect("both canonical declarations are present");
    assert_eq!(
        declared.len(),
        2,
        "the format is declared twice: the `Status:` line and the version-axis table row"
    );

    let other = next_declaration(&declared[0]);

    // Side 1 — a legitimate illustrative mention in the prose does not reach the oracle.
    let illustrative = format!(
        "{doc}\n\nAn illustration: a file declaring `{other}` is a question for a build that \
         writes it, not a defect.\n"
    );
    assert_eq!(
        declared_schema_literals(&illustrative).expect("both anchors survive the addition"),
        declared,
        "an illustrative mention in the prose must not enter the oracle"
    );

    // Side 2 — the declaration is what the oracle reads, so mutating it is detected. The
    // mutation is aimed at the `Status:` line by name, so it cannot land on some earlier prose
    // mention and leave the declaration untouched while this test reports success.
    let mutated_doc = replacing_the_status_declaration(
        &doc,
        &format!("`{}`", declared[0]),
        &format!("`{other}`"),
    );
    let mutated =
        declared_schema_literals(&mutated_doc).expect("both anchors survive the mutation");
    assert_ne!(
        mutated[0], declared[0],
        "a mutated `Status:` declaration must be read by the oracle"
    );
    assert_eq!(
        mutated[1], declared[1],
        "the version-axis table row is a second anchor, not a fallback"
    );
}

/// A check that matches nothing must not read as a pass: dropping either canonical declaration is
/// a loud `Err`, never a silently vacuous oracle.
#[test]
fn a_missing_declaration_anchor_is_an_error_not_an_empty_oracle() {
    let doc = claim_format_doc();

    let without_status = doc
        .lines()
        .filter(|line| !line.starts_with("Status:"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = declared_schema_literals(&without_status)
        .expect_err("the declaration's home must not vanish silently");
    assert!(err.contains("Status:"), "{err}");

    let without_row = doc
        .lines()
        .filter(|line| !line.starts_with("| File format |"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = declared_schema_literals(&without_row)
        .expect_err("the version-axis declaration must not vanish silently");
    assert!(err.contains("File format"), "{err}");
}

// ---------------------------------------------------------------------------
// Non-breaking: the committed corpus is the fixture that must still load
// ---------------------------------------------------------------------------

#[test]
fn the_committed_corpus_loads_under_the_strict_loader() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let claims = load_claims(&root).expect("the committed corpus must load under the strict rules");
    // The expected id set is the tripwire: a claim file *deleted* from the corpus
    // errors in no other test — the loader cannot know how many claims there should
    // be — and naming the ids says which requirement vanished, rather than leaving it
    // to be reverse-engineered from a count. Extend this list deliberately when a
    // requirement is added; an id that *drops* out is a deleted requirement, and a
    // deliberate retirement updates this list in its own reviewable commit.
    let ids: Vec<&str> = claims.iter().map(|c| c.claim.claim_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004", "REQ-0005", "REQ-0006", "REQ-0007",
            "REQ-0008", "REQ-0009", "REQ-0010", "REQ-0011", "REQ-0012", "REQ-0013", "REQ-0014",
            "REQ-0015", "REQ-0016", "REQ-0017", "REQ-0018", "REQ-0019", "REQ-0020", "REQ-0021",
        ],
        "the committed corpus is REQ-0001..REQ-0021, one claim per file"
    );
}
