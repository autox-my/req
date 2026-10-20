//! The parity check's four cases (ADR 0014, as amended 2026-10-20).
//!
//! The check exists to catch an authored slot that **contradicts** the parse. It used to
//! conflate that with a different case — an authored slot the canonical form does not
//! derive at all — and the corpus states the second case seventeen times (a qualitative
//! `attribute = value` triple: `operator: EQ`, `unit: none`, a non-numeric `value`).
//!
//! | parse derived it? | author wrote it? | verdict |
//! |---|---|---|
//! | yes | yes, disagrees | flag — contradiction |
//! | yes | no | flag — omission |
//! | no | yes | annotation — counted, never a flag |
//! | no | no | ok |
//!
//! `attribute` stays unverified: its lexicon has no parser to agree with, so comparing it
//! would flag nearly every claim and teach authors to ignore the report.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use req::canon::{SlotCheck, evaluate_claim};
use req::model::{ClaimFile, Registry};
use std::sync::OnceLock;

fn claim(text: &str) -> ClaimFile {
    yaml_serde::from_str(text).expect("claim parses")
}

/// A registry that resolves the fixtures' subject. It is built once: `Registry::load` reads a
/// file, and a per-test copy would repeat the same YAML four times.
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("parity-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("creates the registry fixture directory");
        let path = dir.join("subjects.yml");
        std::fs::write(
            &path,
            "schema: req.subject_registry/0\nversion: 1\nsubjects:\n  API:\n    label: The API\n    aliases:\n      - the api\n",
        )
        .expect("writes the registry fixture");
        Registry::load(path.to_str().expect("utf-8 path")).expect("registry loads")
    })
}

/// A quantity claim whose parse derives `value 0.2 s`, `operator LE`. It is the fixture the
/// contradiction and omission cases mutate: every slot is quantity-shaped, so a bad authored
/// slot has something to contradict.
const QUANTITY: &str = "\
schema: req.claim/0
claim_id: REQ-0001
statement: The API shall respond to a health check within 200 ms.
modality: SHALL
subject: API
attribute: health_check_latency
operator: LE
value: \"0.2\"
unit: s
conditions: []
applicability: []
exception: null
temporal: null
quantifier: null
unstructured: false
canonical:
  rules_v: null
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/claim-format.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
";

/// The qualitative triple the corpus authors (REQ-0001's shape): an `attribute = value`
/// statement with no quantity to derive.
const QUALITATIVE: &str = "\
schema: req.claim/0
claim_id: REQ-0001
statement: The API shall treat a single claim as the unit of requirements authorship.
modality: SHALL
subject: API
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
  rules_v: null
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/claim-format.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
";

/// REQ-0007's shape: the `UNSTRUCTURED` claim, whose slots are null *by design*. The
/// sentinel is the trap — `operator_str()` answers `"none"`, never `""`, so a comparison
/// that forgets it reports this claim as a contradiction.
const UNSTRUCTURED: &str = "\
schema: req.claim/0
claim_id: REQ-0001
statement: >-
  Authoring shall be friction-free and the decision boundary shall be deliberately
  high-friction, demanding role, meaning, and evidence there.
modality: UNSTRUCTURED
subject: API
attribute: null
operator: null
value: null
unit: none
conditions: []
applicability: []
exception: null
temporal: null
quantifier: null
unstructured: true
canonical:
  rules_v: null
  subject_registry: 1
  form: null
address: null
rationale:
  source: docs/claim-format.md
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
";

/// A non-numeric authored value on a quantity claim is a **contradiction** now. Before
/// ADR 0014 the comparison was gated on the *authored* value looking numeric, so this
/// passed silently: a prohibition was where a wrong slot was least visible.
#[test]
fn a_non_numeric_value_that_contradicts_a_quantity_is_a_flag() {
    let v = evaluate_claim(
        &claim(&QUANTITY.replace("value: \"0.2\"", "value: present")),
        registry(),
    );
    assert_eq!(
        v.value_check,
        SlotCheck::Contradiction,
        "the parse derived 0.2 s; `present` contradicts it"
    );
    assert_eq!(v.flags(), 1, "exactly the value slot is a discrepancy");
}

/// The other direction the old gate could not see: the parse derived a value and the author
/// left it out. Empty is not a qualitative annotation — it is an omission.
#[test]
fn an_omitted_value_on_a_quantity_is_a_flag() {
    let v = evaluate_claim(
        &claim(&QUANTITY.replace("value: \"0.2\"", "value: \"\"")),
        registry(),
    );
    assert_eq!(
        v.value_check,
        SlotCheck::Omission,
        "the parse derived 0.2; the author wrote nothing"
    );
    assert_eq!(v.flags(), 1);
}

/// The qualitative triple is an annotation: reported, counted, and never a flag. This is the
/// case that invalidated ADR 0014's original "compare always" rule, which flagged 31 slots
/// across 17 of 21 claims.
#[test]
fn a_qualitative_triple_is_an_annotation_not_a_flag() {
    let v = evaluate_claim(&claim(QUALITATIVE), registry());
    assert_eq!(v.flags(), 0, "an annotation is not a contradiction");
    assert_eq!(v.value_check, SlotCheck::Annotation);
    assert_eq!(v.op_check, SlotCheck::Annotation);
    assert_eq!(
        v.unit_check,
        SlotCheck::Ok,
        "`unit: none` is the no-unit sentinel, not an authored unit"
    );
    assert!(v.has_annotation());
    assert_eq!(v.annotations(), 2, "value and operator are annotations");
}

/// The annotation class must not become a way to silence a real contradiction. A
/// qualitative *authoring* of a statement that carries a comparator still disagrees with
/// the parsed operator — and that is a flag, not an annotation.
#[test]
fn a_qualitative_triple_on_a_measurable_statement_still_flags_the_operator() {
    let authored = QUANTITY
        .replace("operator: LE", "operator: EQ")
        .replace("value: \"0.2\"", "value: absent")
        .replace("unit: s", "unit: none");
    let v = evaluate_claim(&claim(&authored), registry());
    assert_eq!(
        v.op_check,
        SlotCheck::Contradiction,
        "the parse derived LE; the author wrote EQ"
    );
    assert_eq!(v.value_check, SlotCheck::Annotation);
    assert_eq!(v.flags(), 1, "the operator contradiction is not hidden");
}

/// The sentinel regression guard. `Form::operator_str()` returns `"none"`, and the unit is
/// defaulted the same way, so "the parse derived one" is `!= "none"`, never "non-empty".
/// A comparison against the raw sentinel reports REQ-0007 as a contradiction.
#[test]
fn the_unstructured_claim_reads_ok_not_flagged() {
    let v = evaluate_claim(&claim(UNSTRUCTURED), registry());
    assert_eq!(v.flags(), 0, "null slots by design are not a contradiction");
    assert_eq!(v.value_check, SlotCheck::Ok);
    assert_eq!(v.unit_check, SlotCheck::Ok);
    assert_eq!(v.op_check, SlotCheck::Ok);
    assert!(
        !v.has_annotation(),
        "nothing was authored, so nothing to report"
    );
}

/// Deliberate: `attribute` stays unverified. Its lexicon has no parser to agree with, so a
/// wrong authored attribute is not flagged and must not become a test that "fixes" it.
#[test]
fn a_wrong_authored_attribute_is_not_flagged_by_design() {
    let v = evaluate_claim(
        &claim(&QUANTITY.replace(
            "attribute: health_check_latency",
            "attribute: something_entirely_different",
        )),
        registry(),
    );
    assert_eq!(
        v.flags(),
        0,
        "attribute is unverified by design — a flag here would teach authors to ignore \
         the report"
    );
}

// ---------------------------------------------------------------------------
// The seams the pack names: the CLI's summary, and the addresses it must not move.
// ---------------------------------------------------------------------------

/// The absolute path of the compiled binary, so the test does not depend on `$PATH`.
fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_at(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        // The tests assert against the checkout's own corpus, so an inherited root would
        // silently point them at another tree.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn checkout() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The repo root is a corpus; these tests assert over the real one, because the numbers
/// (`flags: 0`, the annotation count) are claims about this corpus, not about a fixture.
fn corpus(args: &[&str]) -> Outcome {
    let out = run_at(&checkout(), args);
    assert!(
        !out.stderr.contains("[UNVERIFIABLE]"),
        "the checkout's corpus must load:\n{}",
        out.stderr
    );
    out
}

fn row<'a>(stdout: &'a str, id: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.starts_with(id) && line[id.len()..].starts_with(' '))
        .unwrap_or_else(|| panic!("no row for {id} in:\n{stdout}"))
}

#[test]
fn the_corpus_has_zero_flags_and_sixteen_annotations() {
    let out = corpus(&["canon"]);
    assert_eq!(out.code, 0, "canon exit code:\n{}", out.stderr);
    assert!(
        out.stdout.contains("discrepancy flags: 0"),
        "a rule that fixed the flags by going silent would fail the annotation half:\n{}",
        out.stdout
    );
    // 16, not the 17 the pack measured on a draft. The arithmetic is the oracle: 21 claims
    // less the four numeric ones (REQ-0008..0011), less the one `UNSTRUCTURED` claim whose
    // slots are null by design (REQ-0007) = 16. The 17 was the naive prototype's count of
    // *flagged* claims, and it included REQ-0007 — the sentinel false positive this rule
    // exists to prevent. The number is data: if a claim joins the corpus, it moves.
    assert!(
        out.stdout.contains("annotations: 16"),
        "16 of 21 claims carry the qualitative triple (measured, not a constant):\n{}",
        out.stdout
    );

    // The annotation is visible per claim, in a cell distinct from `ok` and from `FLAG`.
    let annotated = row(&out.stdout, "REQ-0001");
    assert!(
        annotated.contains("ann"),
        "the qualitative triple must be visible in its own cell:\n{annotated}"
    );
    assert!(!annotated.contains("FLAG"), "{annotated}");

    // REQ-0007's slots are null by design: ok, not flagged and not annotated.
    let unstructured = row(&out.stdout, "REQ-0007");
    assert!(
        !unstructured.contains("FLAG") && !unstructured.contains("ann"),
        "the UNSTRUCTURED claim has nothing to report:\n{unstructured}"
    );
}

/// No `RULES_V` bump and no address moves: only the *checking* of authored slots changed.
/// REQ-0015's address is the pack's witness (`4a15b28c…`), measured before the change.
#[test]
fn the_parity_change_moves_no_address() {
    let out = corpus(&["canon", "--verify"]);
    assert_eq!(out.code, 0, "canon --verify:\n{}", out.stdout);
    assert!(
        out.stdout.contains("mismatches 0"),
        "every stored address must still recompute:\n{}",
        out.stdout
    );
    let prohibition = row(&out.stdout, "REQ-0015");
    assert!(
        prohibition.contains("4a15b28c8eba"),
        "REQ-0015's address must not move:\n{prohibition}"
    );
}

// ---------------------------------------------------------------------------
// `--fill` refuses what the parity check flags, through the same verdict.
// ---------------------------------------------------------------------------

static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A directory with no `claims/` ancestor: `req init` creates a corpus there, and the later
/// `canon --fill` walk-up cannot find the checkout's own.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("{name}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the fixture directory");
        Fixture(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, text: &str) {
        std::fs::write(self.0.join(rel), text).expect("writes the fixture file");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `req canon --fill` reuses `evaluate_claim`, so the refusal follows the verdict rather than
/// a second check that could drift from it (ADR 0014's own reasoning for sharing the seam).
#[test]
fn fill_refuses_a_contradicted_quantity_and_writes_nothing() {
    let fx = Fixture::new("parity-annotation-fill");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "init:\n{}", init.stderr);

    let contradicted = QUANTITY.replace("value: \"0.2\"", "value: present");
    fx.write("claims/REQ-0001.yml", &contradicted);

    let out = run_at(fx.path(), &["canon", "--fill"]);
    assert_eq!(
        out.code, 3,
        "a contradicted slot makes a fill UNVERIFIABLE, never a write:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("REFUSED"),
        "the refusal must be named:\n{}",
        out.stdout
    );
    assert_eq!(
        std::fs::read_to_string(fx.path().join("claims/REQ-0001.yml")).expect("reads"),
        contradicted,
        "a refused fill writes nothing at all"
    );
}

/// An omission is refused the same way, for the same reason.
#[test]
fn fill_refuses_an_omitted_value_and_writes_nothing() {
    let fx = Fixture::new("parity-annotation-fill-omission");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "init:\n{}", init.stderr);

    let omitted = QUANTITY.replace("value: \"0.2\"", "value: \"\"");
    fx.write("claims/REQ-0001.yml", &omitted);

    let out = run_at(fx.path(), &["canon", "--fill"]);
    assert_eq!(out.code, 3, "{}", out.stdout);
    assert!(out.stdout.contains("REFUSED"), "{}", out.stdout);
    assert_eq!(
        std::fs::read_to_string(fx.path().join("claims/REQ-0001.yml")).expect("reads"),
        omitted
    );
}

/// The corpus itself, which is all annotations and no contradictions, still fills green: an
/// annotation is never a refusal.
#[test]
fn fill_accepts_the_corpus_that_only_annotates() {
    let out = corpus(&["canon", "--fill"]);
    assert_eq!(
        out.code, 0,
        "annotations must not block a fill — only contradictions and omissions do:\n{}",
        out.stdout
    );
    assert!(out.stdout.contains("refused 0"), "{}", out.stdout);
}
