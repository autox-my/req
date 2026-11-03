//! The release gate: three outcomes, and the scoping that makes it useful.

mod common;

use common::{rationale, row, stored};
use req::fill::StoredState;
use req::gate::{Outcome, evaluate_claims, exit_code};
use req::select::{Applies, Context};

use std::sync::atomic::{AtomicU64, Ordering};

/// Fixture reports run concurrently (one test binary, many threads), so a path keyed by the
/// process id alone is shared state: two tests would write and read the same file.
static JUNIT_SEQ: AtomicU64 = AtomicU64::new(0);
fn junit_serial() -> u64 {
    JUNIT_SEQ.fetch_add(1, Ordering::Relaxed)
}

#[test]
fn a_clean_selection_passes_every_policy() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[]),
    ];
    let results = evaluate_claims(&rows, &Context::new());
    assert_eq!(exit_code(&results, 0), 0, "{results:?}");
    assert!(results.iter().all(|r| r.outcome == Outcome::Pass));
}

/// ADR 0015 at the library seam: the selection is part of the verdict, not only the policies.
/// A count of undecided claims makes the run UNVERIFIABLE even when every policy settled in
/// the affirmative — "I could not decide what this release contains" is not a pass — while a
/// definite defect still outranks it, because "we found a problem" is the stronger statement.
#[test]
fn an_undecided_claim_is_unverifiable_and_a_defect_outranks_it() {
    let clean = evaluate_claims(&[row("REQ-A", Applies::Yes, &[])], &Context::new());
    assert_eq!(exit_code(&clean, 0), 0, "{clean:?}");
    assert_eq!(
        exit_code(&clean, 1),
        3,
        "one claim the gate did not examine is enough: {clean:?}"
    );

    let defect = evaluate_claims(
        &[stored(
            row("REQ-A", Applies::Yes, &[]),
            StoredState::Mismatch("stored address does not recompute".to_string()),
        )],
        &Context::new(),
    );
    assert_eq!(
        exit_code(&defect, 1),
        1,
        "Fail outranks the undecided count: {defect:?}"
    );
}

/// The variant-scoping property: a claim the selection excludes is not this variant's
/// problem, however broken it is.
#[test]
fn claims_outside_the_selection_do_not_fail_the_gate() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[]),
        stored(
            row("REQ-B", Applies::No, &[]),
            StoredState::Mismatch("tampered digest".to_string()),
        ),
        rationale(row("REQ-C", Applies::No, &[]), ""),
    ];
    let results = evaluate_claims(&rows, &Context::new());
    assert_eq!(exit_code(&results, 0), 0, "{results:?}");
}

#[test]
fn a_hand_edited_digest_fails_and_an_unfilled_claim_does_not_pass() {
    let tampered = vec![stored(
        row("REQ-A", Applies::Yes, &[]),
        StoredState::Mismatch("stored address does not recompute".to_string()),
    )];
    let results = evaluate_claims(&tampered, &Context::new());
    assert_eq!(exit_code(&results, 0), 1);
    assert_eq!(results[0].outcome, Outcome::Fail);
    assert!(results[0].offenders[0].contains("does not recompute"));

    let unfilled = vec![stored(
        row("REQ-A", Applies::Yes, &[]),
        StoredState::Candidate("unfilled".to_string()),
    )];
    let results = evaluate_claims(&unfilled, &Context::new());
    assert_eq!(
        exit_code(&results, 0),
        3,
        "not wrong, but not confirmed either"
    );
    assert_eq!(results[0].outcome, Outcome::Unverifiable);
}

#[test]
fn a_defect_outranks_an_open_question() {
    let rows = vec![
        rationale(row("REQ-A", Applies::Yes, &[]), ""),
        row("REQ-B", Applies::Yes, &[("refines", "REQ-GONE")]),
    ];
    let results = evaluate_claims(&rows, &Context::new());
    assert_eq!(exit_code(&results, 0), 1, "{results:?}");
}

#[test]
fn an_undecided_dependency_blocks_the_release() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Unknown, &[]),
    ];
    let results = evaluate_claims(&rows, &Context::new());
    assert_eq!(exit_code(&results, 0), 3);
    let decided = results
        .iter()
        .find(|r| r.name == "dependencies_decided")
        .expect("the policy exists");
    assert_eq!(decided.outcome, Outcome::Unverifiable);
    assert!(decided.offenders[0].contains("REQ-B"));
}

/// The invocation policy, at the library seam: a row set with nothing applying verified
/// nothing, however well-formed the invocation. The CLI reaches the same guard through a
/// fixture (`cli::a_variant_that_selects_no_claim_is_unverifiable_not_a_pass`); this pins the
/// empty-corpus edge of it, which no corpus fixture can produce.
#[test]
fn a_selection_with_nothing_applying_verifies_nothing() {
    let results = evaluate_claims(&[], &Context::new());
    let policy = results
        .iter()
        .find(|r| r.name == "selectors_confirmed")
        .expect("the invocation is judged as a policy");
    assert_eq!(policy.outcome, Outcome::Unverifiable, "{policy:?}");
    assert_eq!(exit_code(&results, 0), 3);
}

// ---------------------------------------------------------------------------------------------
// The public seam is the CLI: `req gate [key=value ...]` over a corpus root, observed through its
// exit code and its printed lines. The rows above judge policies over hand-built rows; these judge
// the *invocation*, which is where the defect lived — a context the corpus cannot confirm narrowed
// the selection silently, and the gate still said PASS.
// ---------------------------------------------------------------------------------------------

mod cli {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// The gate's exit code and everything it said — stdout and stderr in one string, because a
    /// refusal is an outcome and the question is what the invocation reported either way.
    pub struct Outcome {
        pub code: i32,
        pub text: String,
    }

    impl Outcome {
        /// The `# Release gate: N applying claim(s), M undecided` line: the count the oracle
        /// measured, printed by the gate itself.
        pub fn headline(&self) -> &str {
            self.text.lines().next().unwrap_or_default()
        }

        /// The `undecided (not examined by this invocation): …` line — the claims the selection
        /// left open, which the gate names when they change the verdict (ADR 0015). Panics when
        /// the line is absent: "the gate said nothing about them" is the defect the line rules
        /// out, so a test cannot pass by finding nothing.
        pub fn undecided(&self) -> &str {
            self.text
                .lines()
                .find(|line| line.trim_start().starts_with("undecided (not examined"))
                .unwrap_or_else(|| panic!("no undecided-claims line in:\n{}", self.text))
        }

        /// One policy's block, from its `[OUTCOME] name` header to the next one.
        pub fn policy(&self, name: &str) -> String {
            let marker = format!("] {name}\n");
            let at = self
                .text
                .find(&marker)
                .unwrap_or_else(|| panic!("no policy `{name}` in:\n{}", self.text));
            let start = self.text[..at]
                .rfind('[')
                .expect("a policy header starts `[`");
            let rest = &self.text[start..];
            match rest.find("\n[") {
                Some(end) => rest[..end].to_string(),
                None => rest.to_string(),
            }
        }

        /// Whether any word inside one policy's block is `key`. Scoped deliberately: the merged
        /// stdout+stderr carries the resolved `corpus root:` path and every other policy's
        /// offenders, none of which can name a context key *for this policy*, so a substring scan
        /// over all of it would redden for a reason that has nothing to do with the rule under
        /// test — a checkout path containing `/runs/` is enough.
        pub fn policy_names(&self, name: &str, key: &str) -> bool {
            self.policy(name)
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|word| word == key)
        }
    }

    /// The absolute path of the compiled binary, so the tests do not depend on `$PATH`.
    pub fn req_bin() -> PathBuf {
        std::env::var_os("CARGO_BIN_EXE_req")
            .map(PathBuf::from)
            .expect("integration tests run with CARGO_BIN_EXE_req set")
    }

    /// Write a JUnit report of the given `(test id, state)` cases, where `state` is
    /// `"passed"`, `"failed"` or `"skipped"`. The ids are `classname::name`, exactly the identity a
    /// claim's `tests:` entry names, so a test can speak the report's language without a lot of
    /// XML.
    pub fn write_junit(path: &Path, cases: &[(&str, &str)]) {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"nextest-run\" \
             tests=\"{}\" skipped=\"0\" failures=\"0\" errors=\"0\">\n",
            cases.len()
        );
        xml.push_str("    <testsuite name=\"fixture\">\n");
        for (id, state) in cases {
            let (classname, name) = id
                .rsplit_once("::")
                .expect("a test id is `classname::name`");
            match *state {
                "passed" => xml.push_str(&format!(
                    "        <testcase name=\"{name}\" classname=\"{classname}\"/>\n"
                )),
                "failed" => xml.push_str(&format!(
                    "        <testcase name=\"{name}\" classname=\"{classname}\">\
                     <failure message=\"assertion failed\" type=\"test failure\">boom</failure>\
                     </testcase>\n"
                )),
                "skipped" => xml.push_str(&format!(
                    "        <testcase name=\"{name}\" classname=\"{classname}\">\
                     <skipped message=\"ignored\"/></testcase>\n"
                )),
                other => panic!("unknown fixture state {other:?}"),
            }
        }
        xml.push_str("    </testsuite>\n</testsuites>\n");
        std::fs::write(path, xml).expect("writes the fixture report");
    }

    /// The dogfood corpus, whose selector vocabulary the oracle read: `deployment_mode ==
    /// air_gapped`, `corpus_set == parity` / `parity_and_adversarial`, `set ==
    /// direction_gold_structural`, `field == value`. (It carried `runs >= 20` until REQ-0011 was
    /// retired, ADR 0021; the `Ge` rows below therefore run on a fixture.)
    /// The gate run over this repository's own corpus, with `--root` explicit rather than relying
    /// on the cwd walk-up. `pub` so the dogfood tests at the crate root — which assert the same
    /// thing about `.req/variants` — do not re-derive the binary and the invocation.
    pub fn dogfood(args: &[&str]) -> Outcome {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // The dogfood gate reads the *repository's* links; this fixture report answers them all
        // as passed so a test about the release context is not re-answering the evidence question.
        // The real gate step in `scripts/check.sh` feeds it the report nextest actually wrote.
        let report = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "dogfood-junit-{}-{}.xml",
            std::process::id(),
            crate::junit_serial()
        ));
        crate::common::passing_junit(&root.join("claims"), &report);
        let mut full: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        full.push("--test-report".to_string());
        full.push(report.to_string_lossy().into_owned());
        gate_at(&root, &full)
    }

    fn gate_at(root: &Path, args: &[String]) -> Outcome {
        let output = Command::new(req_bin())
            .arg("--root")
            .arg(root)
            .arg("gate")
            .args(args)
            .output()
            .expect("runs the compiled binary");
        Outcome {
            code: output.status.code().unwrap_or(-1),
            text: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    }

    /// A throwaway corpus root under the build tree Cargo guarantees is writable, removed when the
    /// test ends — including when an assertion panics.
    pub struct Fixture(PathBuf);

    impl Fixture {
        pub fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            // A process-local counter carries uniqueness rather than the clock, whose resolution
            // is not one; the pid keeps two concurrent runs apart.
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
                "gate-{tag}-{}-{nanos}-{serial}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            // `--root <dir>` is only accepted when the directory is a corpus root (ADR 0012), so
            // every fixture carries `claims/` and the subject registry beside it.
            std::fs::create_dir_all(dir.join("claims")).expect("creates claims/");
            std::fs::create_dir_all(dir.join("aliases")).expect("creates aliases/");
            std::fs::copy(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("aliases/subjects.yml"),
                dir.join("aliases/subjects.yml"),
            )
            .expect("copies the subject registry");
            Fixture(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        /// Add one claim carrying the given `applicability` predicates. Its authored slots are a
        /// parity-clean shape (they must be: `canon --fill` refuses a contradicted parse); the
        /// derived fields are written by `canon --fill`, never by this fixture.
        pub fn claim(&self, id: &str, applicability: &[&str]) {
            // A default link into the fixture report, so a test about **another** policy is not
            // re-answering the evidence question; the evidence tests use `claim_with` explicitly.
            let link = format!("fixture::{id}");
            self.claim_with(id, applicability, &[link.as_str()], "machine");
        }

        /// A claim with an explicit evidence declaration: `tests` are the ids its report must
        /// carry (or none), and `evidence` is `machine` or `process`. Written through one
        /// template so a fixture cannot be half-migrated to the new file format.
        pub fn claim_with(&self, id: &str, applicability: &[&str], tests: &[&str], evidence: &str) {
            std::fs::write(
                self.0.join("claims").join(format!("{id}.yml")),
                claim_text(id, applicability, tests, evidence),
            )
            .expect("writes a claim");
        }

        /// `req canon --fill` — the only thing that may write an address or a canonical form.
        pub fn fill(&self) {
            let output = Command::new(req_bin())
                .arg("--root")
                .arg(self.path())
                .arg("canon")
                .arg("--fill")
                .output()
                .expect("runs the compiled binary");
            assert!(
                output.status.success(),
                "the fixture must fill cleanly: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        /// The gate over this fixture, with a report that passes every link the fixture's claims
        /// carry. A test about another policy must not be answering "was a test report supplied?"
        /// instead — that question has its own tests, which call [`Fixture::gate_raw`] or pass a
        /// report of their own.
        pub fn gate(&self, args: &[&str]) -> Outcome {
            let report = self.passing_report();
            self.gate_with_report(args, &report)
        }

        /// The gate with no `--test-report` at all: the policy-cannot-be-exercised state.
        pub fn gate_raw(&self, args: &[&str]) -> Outcome {
            let full: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            gate_at(self.path(), &full)
        }

        /// The gate with a report of the caller's choosing.
        pub fn gate_with_report(&self, args: &[&str], report: &Path) -> Outcome {
            let mut full: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            full.push("--test-report".to_string());
            full.push(report.to_string_lossy().into_owned());
            gate_at(self.path(), &full)
        }

        /// A JUnit fixture whose every claimed link passes.
        pub fn passing_report(&self) -> PathBuf {
            let path = self.path().join(format!(
                "junit-{}-{}.xml",
                std::process::id(),
                crate::junit_serial()
            ));
            crate::common::passing_junit(&self.path().join("claims"), &path)
        }

        /// Overwrite the `address:` line `canon --fill` already wrote. The fixture does this on
        /// purpose, and only here: it is the hand-edited digest `addresses_current` exists to
        /// catch. Authoring an address into the template instead would be that defect, not a test
        /// of it — which is why the template's anchor is only ever a placeholder.
        pub fn tamper_address(&self, id: &str) {
            let path = self.0.join("claims").join(format!("{id}.yml"));
            let text = std::fs::read_to_string(&path).expect("reads the filled claim");
            let corrupted: Vec<String> = text
                .lines()
                .map(|line| {
                    if line.starts_with("address: ") {
                        format!("address: {}", "f".repeat(64))
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            std::fs::write(&path, format!("{}\n", corrupted.join("\n")))
                .expect("writes the corrupted claim");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// One authored claim. The `canonical`/`address` block is deliberately empty: it is the anchor
    /// `canon --fill` writes into, and `Fixture::fill` asserts the fill succeeded before any test
    /// observes the gate.
    fn claim_text(id: &str, applicability: &[&str], tests: &[&str], evidence: &str) -> String {
        let tests_block = if tests.is_empty() {
            "tests: []\n".to_string()
        } else {
            let mut block = String::from("tests:\n");
            for test in tests {
                block.push_str(&format!("  - {test:?}\n"));
            }
            block
        };
        let block = if applicability.is_empty() {
            "applicability: []\n".to_string()
        } else {
            let mut block = String::from("applicability:\n");
            for predicate in applicability {
                block.push_str(&format!("  - {predicate}\n"));
            }
            block
        };
        format!(
            r#"schema: req.claim/1
claim_id: {id}
statement: >-
  The req tool shall require numeric claim fields to be serialized as exact
  decimal strings.
modality: SHALL
subject: REQ_TOOL
attribute: numeric_value_encoding
operator: EQ
value: exact_decimal_strings
unit: none
conditions: []
{block}exception: null
temporal: null
quantifier: null
unstructured: false
{tests_block}evidence: {evidence}
canonical:
  rules_v: 0
  subject_registry: 0
  form: |-
    {{}}
address: {zeros}
rationale:
  source: tests/release_gate.rs
  date: 2026-10-07
  rejected_alternatives: []
  assumptions: []
relations: []
"#,
            zeros = "0".repeat(64)
        )
    }

    // ---- the context the corpus confirms: the policy passes, the release stays open ----------

    /// The declared value, on the corpus that declares it: the fix refuses typo'd values, not the
    /// vocabulary the corpus actually has. The **policy** is what this pins. The run as a whole is
    /// UNVERIFIABLE because four scoped claims are left undecided (ADR 0015), which is why the
    /// exit code is 3 while `selectors_confirmed` is PASS.
    #[test]
    fn the_value_the_corpus_declares_is_confirmed() {
        let out = dogfood(&["field=value"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.headline().contains("17 applying claim(s), 4 undecided"),
            "{}",
            out.headline()
        );
        assert!(
            out.policy("selectors_confirmed").starts_with("[PASS]"),
            "{}",
            out.policy("selectors_confirmed")
        );
        assert!(
            out.undecided().contains("REQ-0008"),
            "the verdict must name what it did not examine: {}",
            out.undecided()
        );
    }

    /// The same for the other declared deployment value: the entry is confirmed, and the run is
    /// still UNVERIFIABLE for the claims the selection leaves open (ADR 0015).
    #[test]
    fn the_declared_air_gapped_value_is_confirmed() {
        let out = dogfood(&["deployment_mode=air_gapped"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.headline().contains("17 applying claim(s), 4 undecided"),
            "{}",
            out.headline()
        );
        assert!(
            out.policy("selectors_confirmed").starts_with("[PASS]"),
            "{}",
            out.policy("selectors_confirmed")
        );
    }

    // ---- the invocation rows: nothing selected, and a key nothing mentions ---------------------

    /// A variant in which nothing applies verified nothing, so it is UNVERIFIABLE — never PASS.
    /// The dogfood corpus cannot reach this: 16 of its 21 claims carry no predicate and so always
    /// apply, which is exactly why the guard needs a fixture.
    #[test]
    fn a_variant_that_selects_no_claim_is_unverifiable_not_a_pass() {
        let fixture = Fixture::new("vacuous");
        fixture.claim("REQ-9001", &["mode == beta", "region == eu"]);
        fixture.fill();
        let out = fixture.gate(&["mode=beta"]);
        // `region` is absent from the context, so the only claim is undecided: 0 applying, and on
        // `main` that printed `verdict: PASS` and exited 0.
        assert!(
            out.headline().contains("0 applying claim(s), 1 undecided"),
            "{}",
            out.headline()
        );
        assert_eq!(out.code, 3, "{}", out.text);
        let block = out.policy("selectors_confirmed");
        assert!(block.starts_with("[UNVERIFIABLE]"), "{block}");
        assert!(
            block.contains("no claim applies in this variant"),
            "{block}"
        );
        // `mode=beta` IS declared, so the vacuity alone decided this: the four claim policies
        // pass over the empty applying set.
        for name in [
            "addresses_current",
            "relations_resolve",
            "dependencies_decided",
            "rationale_present",
        ] {
            assert!(
                out.policy(name).starts_with("[PASS]"),
                "{name}: {}",
                out.policy(name)
            );
        }
    }

    // ---- ADR 0015: the selection is part of the verdict ---------------------------------------

    /// A claim the invocation leaves undecided is never examined, so the invocation cannot be
    /// green however cleanly every policy settles over the claims it did apply. The fixture
    /// carries one **unconditional** claim on purpose: with only the scoped claim, nothing
    /// applies and the vacuity guard above already exits 3 — today's defect is a gate that
    /// passes *with something examined beside* the claim it skipped, so that is the shape that
    /// can fail before this change.
    #[test]
    fn a_bare_gate_that_leaves_a_claim_undecided_is_unverifiable() {
        let fixture = Fixture::new("undecided");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["mode == beta"]);
        fixture.fill();

        let bare = fixture.gate(&[]);
        assert_eq!(bare.code, 3, "{}", bare.text);
        assert!(
            bare.headline().contains("1 applying claim(s), 1 undecided"),
            "{}",
            bare.headline()
        );
        // Nothing is wrong: every policy passes. The undecided claim alone decides the verdict.
        assert!(
            bare.policy("selectors_confirmed").starts_with("[PASS]"),
            "{}",
            bare.policy("selectors_confirmed")
        );
        assert!(bare.text.contains("verdict: UNVERIFIABLE"), "{}", bare.text);
        // The gate must say *which* claim it did not examine, and what to do about it.
        assert!(
            bare.undecided().contains("REQ-9002"),
            "{}",
            bare.undecided()
        );
        assert!(
            !bare.undecided().contains("REQ-9001"),
            "a claim that applies was examined and must not be listed: {}",
            bare.undecided()
        );
        assert!(bare.text.contains("remedy:"), "{}", bare.text);
        // The remedy has two directions and this corpus is the context-decidable one: REQ-9002's
        // `mode == beta` is decided by a context. A regression that always printed the unreadable
        // predicate's wording would send the caller to rewrite a predicate that is perfectly fine,
        // and `remedy:` alone would not notice.
        assert!(
            bare.text.contains("pass the variant context"),
            "a context-decidable claim gets the context remedy: {}",
            bare.text
        );
        assert!(
            !bare.text.contains("rewrite the predicate"),
            "and not the unreadable-predicate remedy: {}",
            bare.text
        );
        // req never reads `.req/variants` (the file says so itself): it is the caller's script
        // convention, so the remedy must name the release list, not a file req knows nothing about.
        assert!(
            !bare.text.contains(".req/variants"),
            "the remedy names the caller's release list, not a file req never reads: {}",
            bare.text
        );

        // The same corpus, with the context that decides the claim: green.
        let decided = fixture.gate(&["mode=beta"]);
        assert_eq!(decided.code, 0, "{}", decided.text);
        assert!(
            decided
                .headline()
                .contains("2 applying claim(s), 0 undecided"),
            "{}",
            decided.headline()
        );
        // The negative direction of the disclosure: a decided release must print neither the
        // "not examined" list nor a remedy. Without this, a regression that printed both lines
        // unconditionally would keep every assertion above green.
        assert!(
            !decided.text.contains("undecided (not examined"),
            "a decided release must not print the disclosure: {}",
            decided.text
        );
        assert!(
            !decided.text.contains("remedy:"),
            "a decided release must not print a remedy: {}",
            decided.text
        );
    }

    /// Fail still outranks the open question after ADR 0015: a corpus whose bare gate finds a
    /// defect **and** leaves a claim undecided is FAIL (1), never 3 — "we found a problem" is
    /// the stronger statement, and a change that made the undecided count swallow a known
    /// defect would reverse the asymmetry the rest of req keeps.
    #[test]
    fn a_defect_outranks_an_undecided_claim() {
        let fixture = Fixture::new("defect-undecided");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["mode == beta"]);
        fixture.fill();
        fixture.tamper_address("REQ-9001");

        let out = fixture.gate(&[]);
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(
            out.headline().contains("1 undecided"),
            "the undecided claim is still counted: {}",
            out.headline()
        );
        assert!(
            out.policy("addresses_current").starts_with("[FAIL]"),
            "{}",
            out.policy("addresses_current")
        );
    }

    /// A context key no predicate in the corpus mentions decided nothing at all: it selected
    /// nothing and excluded nothing, so naming it is honest. This corpus carries no selector
    /// outside the grammar, so the message states what the corpus decides and claims nothing
    /// about unread selectors (the fixture below pins the case where it does).
    #[test]
    fn a_key_no_applicability_predicate_mentions_is_refused() {
        let out = dogfood(&["nonsense=whatever"]);
        // The oracle: 16 applying, 5 undecided — one claim MORE undecided than the 4 a run that
        // supplies the corpus's declared value leaves, because this entry decided nothing.
        assert!(
            out.headline().contains("16 applying claim(s), 5 undecided"),
            "{}",
            out.headline()
        );
        assert_eq!(out.code, 3, "{}", out.text);
        let block = out.policy("selectors_confirmed");
        assert!(block.starts_with("[UNVERIFIABLE]"), "{block}");
        assert!(block.contains("nonsense=whatever"), "{block}");
        assert!(
            block.contains("nothing in the corpus decides `nonsense`"),
            "{block}"
        );
        assert!(
            !block.contains("outside the `==`/`>=` grammar"),
            "no unparsed selector exists here, so the message must not claim one: {block}"
        );
    }

    // ---- the commit-2 rows: the Eq vocabulary ------------------------------------------------

    /// The defect, on the corpus the oracle measured: `field=val` is a typo, so REQ-0018 drops
    /// out of the selection — silently, at exit 0. The refusal must name the value the corpus
    /// declares, so the near miss is a two-second fix.
    #[test]
    fn a_typoed_eq_value_is_refused_and_names_the_declared_one() {
        let out = dogfood(&["field=val"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.headline().contains("16 applying claim(s), 4 undecided"),
            "the silently narrower selection: {}",
            out.headline()
        );
        let block = out.policy("selectors_confirmed");
        assert!(block.starts_with("[UNVERIFIABLE]"), "{block}");
        assert!(block.contains("field=val"), "{block}");
        assert!(
            block.contains("field == value"),
            "the declared alternative must be named: {block}"
        );
        // Nothing else is wrong with the *policies*: this refusal is one reason for the 3, and
        // the four undecided claims ADR 0015 counts are the other.
        for name in [
            "addresses_current",
            "relations_resolve",
            "dependencies_decided",
            "rationale_present",
        ] {
            assert!(
                out.policy(name).starts_with("[PASS]"),
                "{name}: {}",
                out.policy(name)
            );
        }
    }

    /// The same rule on the other declared key, whose near miss is one underscore: `airgapped`
    /// against `air_gapped`.
    #[test]
    fn the_air_gapped_typo_is_refused_and_names_the_declared_value() {
        let out = dogfood(&["deployment_mode=airgapped"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.headline().contains("16 applying claim(s), 4 undecided"),
            "{}",
            out.headline()
        );
        let block = out.policy("selectors_confirmed");
        assert!(block.contains("deployment_mode=airgapped"), "{block}");
        assert!(block.contains("deployment_mode == air_gapped"), "{block}");
    }

    /// A legitimate variant the corpus does not describe is refused **on purpose** (pack F1;
    /// ADR 0006 — the friction belongs at the decision boundary). It is not a bug, and a future
    /// reader must re-make the decision in the open rather than "fix" this test.
    #[test]
    fn a_legitimate_variant_outside_the_vocabulary_is_refused_on_purpose() {
        let out = dogfood(&["deployment_mode=cloud"]);
        assert_eq!(out.code, 3, "{}", out.text);
        let block = out.policy("selectors_confirmed");
        assert!(block.contains("deployment_mode=cloud"), "{block}");
        assert!(block.contains("deployment_mode == air_gapped"), "{block}");
    }

    /// The `Ge` exemption is not optional: `runs=100` matches no predicate literally, yet
    /// `runs >= 20` decides it Yes. On a fixture since REQ-0011 (the dogfood corpus's only `Ge`
    /// predicate) was retired: without the exemption the entry would be refused as an unconfirmed
    /// value and the run would not be green, so the exit code alone pins it.
    #[test]
    fn a_ge_value_is_confirmed_by_comparison_and_never_flagged() {
        let fixture = Fixture::new("ge-compared");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["runs >= 20"]);
        fixture.fill();
        let out = fixture.gate(&["runs=100"]);
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.headline().contains("2 applying claim(s), 0 undecided"),
            "100 decides `runs >= 20` Yes: {}",
            out.headline()
        );
        assert!(
            out.policy("selectors_confirmed").starts_with("[PASS]"),
            "100 is decided by comparison, not by enumeration: {}",
            out.policy("selectors_confirmed")
        );
        assert!(
            !out.policy_names("selectors_confirmed", "runs"),
            "no finding in this policy may name the `runs` key: {}",
            out.policy("selectors_confirmed")
        );
    }

    /// A non-decimal value for a `Ge` key is not guessed and not refused: the predicate decides
    /// nothing, so its claim lands in the undecided count, while the entry itself stays confirmed —
    /// a `Ge` key enumerates no legal values to check it against. The gate reports the undecided
    /// count rather than inventing a finding — and since ADR 0015 that count is itself enough to
    /// keep the run UNVERIFIABLE. (A fixture since REQ-0011 was retired, ADR 0021.)
    #[test]
    fn a_non_numeric_ge_value_is_confirmed_and_surfaces_as_undecided() {
        let fixture = Fixture::new("ge-non-numeric");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["runs >= 20"]);
        fixture.fill();
        let out = fixture.gate(&["runs=abc"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.headline().contains("1 applying claim(s), 1 undecided"),
            "{}",
            out.headline()
        );
        assert!(
            out.undecided().contains("REQ-9002"),
            "the claim the entry could not decide is named: {}",
            out.undecided()
        );
        assert!(
            out.policy("selectors_confirmed").starts_with("[PASS]"),
            "{}",
            out.policy("selectors_confirmed")
        );
        assert!(
            !out.policy_names("selectors_confirmed", "runs"),
            "the entry is confirmed, so no finding in this policy may name it: {}",
            out.policy("selectors_confirmed")
        );
    }

    /// The sub-case the corpus cannot settle, pinned: `revision` is declared by both an `Eq` and a
    /// `Ge` predicate. The choice, recorded — the `Eq` predicates enumerate a closed vocabulary,
    /// so a value outside it is unconfirmed even though the `Ge` predicate would compare it.
    /// Letting the comparison rescue such a value would re-open the silent narrowing for exactly
    /// the keys that carry both.
    #[test]
    fn an_eq_vocabulary_governs_a_key_that_also_carries_a_ge_predicate() {
        let fixture = Fixture::new("mixed");
        fixture.claim("REQ-9001", &["revision == 3"]);
        fixture.claim("REQ-9002", &["revision >= 3"]);
        fixture.fill();

        // Declared: confirmed, both claims apply, nothing to refuse.
        let declared = fixture.gate(&["revision=3"]);
        assert_eq!(declared.code, 0, "{}", declared.text);
        assert!(
            declared.policy("selectors_confirmed").starts_with("[PASS]"),
            "{}",
            declared.policy("selectors_confirmed")
        );

        // Undeclared, and comparable by the `Ge` predicate: still refused.
        let undeclared = fixture.gate(&["revision=4"]);
        assert_eq!(undeclared.code, 3, "{}", undeclared.text);
        let block = undeclared.policy("selectors_confirmed");
        assert!(block.starts_with("[UNVERIFIABLE]"), "{block}");
        assert!(block.contains("revision=4"), "{block}");
        assert!(
            block.contains("revision == 3"),
            "the declared value is named: {block}"
        );
    }

    /// A selector outside the tiny grammar is not read, so its keys are unknown — therefore the
    /// refusal must not assert that the corpus never mentions the key. It quotes the unparsed
    /// selectors verbatim instead, and the claim they live on surfaces as undecided, which is why
    /// `1 applying` sits beside `1 undecided` rather than the vacuity guard firing.
    #[test]
    fn an_unparsed_selector_is_quoted_rather_than_asserted_absent() {
        let fixture = Fixture::new("unparsed");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["field = value"]);
        fixture.fill();
        let out = fixture.gate(&["field=value"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.headline().contains("1 applying claim(s), 1 undecided"),
            "the unread selector leaves its claim undecided, never excluded: {}",
            out.headline()
        );
        let block = out.policy("selectors_confirmed");
        assert!(block.starts_with("[UNVERIFIABLE]"), "{block}");
        assert!(block.contains("field=value"), "{block}");
        assert!(
            block.contains("field = value"),
            "the unparsed selector is quoted, not guessed at: {block}"
        );
        assert!(
            !block.contains("no applicability predicate in the corpus mentions"),
            "the gate must not assert absence of a key an unread selector may carry: {block}"
        );
        // An unread predicate is decided by no context, so the remedy must say to rewrite it —
        // not to pass a variant context, which would send the caller looking for a value that
        // cannot exist.
        assert!(
            out.text.contains("is not in the selector grammar"),
            "the remedy names the unread predicate: {}",
            out.text
        );
        assert!(
            out.text.contains("rewrite the predicate"),
            "and says what to do about it: {}",
            out.text
        );
        // The negative direction: when *every* undecided claim is unreadable, no "rest" is left
        // for a context to decide, so the context remedy must be absent. Without this, a mutation
        // turning the `else if` into a plain `else` keeps the positive assertion green.
        assert!(
            !out.text.contains("pass the variant context"),
            "when no context can make a claim apply, the context remedy is the wrong advice: {}",
            out.text
        );
        // And the false claim the review found: an unreadable predicate does not make the corpus
        // permanently red — a context can still exclude the claim.
        assert!(
            !out.text.contains("cannot go green"),
            "a context can still exclude the claim: {}",
            out.text
        );
    }

    /// The **mixed** bucket: one undecided claim is unreadable and another is context-decidable, so
    /// the remedy must name both moves. Every other fixture puts all undecided claims in one bucket,
    /// which leaves this branch — "rewrite the predicate above, then pass the variant context that
    /// decides the rest" — unexercised.
    #[test]
    fn a_mixed_undecided_set_gets_both_remedies() {
        let fixture = Fixture::new("mixed-undecided");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["field = value"]);
        fixture.claim("REQ-9003", &["mode == beta"]);
        fixture.fill();
        let out = fixture.gate(&[]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.text
                .contains("rewrite the predicate above, then pass the variant context"),
            "both remedies are named, in that order: {}",
            out.text
        );
        // The mixed branch's wording was reworded too; the unchanged prefix above would also
        // match the old text, so the file name is asserted absent here as well.
        assert!(
            !out.text.contains(".req/variants"),
            "neither remedy may name a file req never reads: {}",
            out.text
        );
    }

    /// A `Ge` whose bound is not a decimal parses — the grammar does not validate the bound — but
    /// `select::evaluate` returns UNKNOWN for it because the comparison needs decimals on both
    /// sides. It therefore belongs in the "no context can decide it" bucket: telling the caller to
    /// pass the variant context sends them looking for a value they cannot supply, since no
    /// `runs=<whatever>` makes `runs >= twenty` comparable.
    #[test]
    fn a_ge_with_a_non_decimal_bound_is_named_not_left_to_a_context() {
        let fixture = Fixture::new("ge-nondecimal");
        fixture.claim("REQ-9001", &[]);
        fixture.claim("REQ-9002", &["runs >= twenty"]);
        fixture.fill();
        let out = fixture.gate(&["runs=25"]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            out.text.contains("runs >= twenty"),
            "the unreadable bound is quoted: {}",
            out.text
        );
        assert!(
            out.text.contains("rewrite the predicate"),
            "and the remedy is to rewrite it, not to pass a context: {}",
            out.text
        );
    }

    // ---- all three outcomes through the public seam ------------------------------------------

    /// The third outcome, end to end: an address that does not recompute is a **defect** — FAIL,
    /// exit 1 — not an open question. `canon --fill` wrote the address and the fixture then
    /// corrupted it, which is exactly the hand-edited digest `addresses_current` exists to catch;
    /// with PASS and UNVERIFIABLE pinned above, the whole tri-state is reachable from the CLI.
    #[test]
    fn a_hand_edited_address_fails_the_gate() {
        let fixture = Fixture::new("tampered");
        fixture.claim("REQ-9001", &[]);
        fixture.fill();
        fixture.tamper_address("REQ-9001");

        let out = fixture.gate(&[]);
        assert_eq!(out.code, 1, "{}", out.text);
        let block = out.policy("addresses_current");
        assert!(block.starts_with("[FAIL]"), "{block}");
        assert!(block.contains("REQ-9001"), "{block}");
        assert!(
            out.policy("selectors_confirmed").starts_with("[PASS]"),
            "the context is empty, so the invocation raises nothing: {}",
            out.policy("selectors_confirmed")
        );
        assert!(out.text.contains("verdict: FAIL"), "{}", out.text);
    }
}

// ---------------------------------------------------------------------------
// Dogfood: the gate over this repository's own corpus, once per declared release
// ---------------------------------------------------------------------------

/// What `req gate` would be handed for one line.
fn context_of(line: &str) -> Context {
    let mut context = Context::new();
    for pair in line.split_whitespace() {
        let (key, value) = pair
            .split_once('=')
            .unwrap_or_else(|| panic!("a release line is key=value pairs, not {line:?}"));
        context.insert(key.to_string(), value.to_string());
    }
    context
}

/// Coverage, not counting: a claim that applies in **no** declared release is a claim the gate
/// never examines — which is the whole failure the variants list exists to prevent. Being
/// excluded in one release and applied in another is what a variant *is*, so this asks only
/// that every claim is applied somewhere.
#[test]
fn every_claim_applies_in_at_least_one_declared_release() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let claims = req::canon::load_claims(&root).expect("loads this repository's corpus");
    let contexts: Vec<Context> = common::variant_lines()
        .iter()
        .map(|l| context_of(l))
        .collect();
    assert!(!contexts.is_empty(), ".req/variants must declare a release");

    let mut unexamined = Vec::new();
    for loaded in &claims {
        let predicates: Vec<_> = loaded
            .claim
            .applicability
            .iter()
            .map(|text| req::select::parse_predicate(text))
            .collect();
        let states: Vec<Applies> = contexts
            .iter()
            .map(|context| req::select::applies(&predicates, context))
            .collect();
        if states.contains(&Applies::Yes) {
            continue;
        }
        // `Unknown` is not `No`, and the remedies differ, so the failure says which case it is —
        // the same distinction the gate and `select` keep ("an undecided claim is never treated as
        // absent"). `Unknown` means a key is decided but its value matches no line; `No` means no
        // line applies the claim at all.
        // Three shapes, three remedies. `Unknown` is the context *not deciding* the predicate (a
        // key no line supplies, or a predicate the grammar does not parse); `No` is the context
        // deciding it and the value not matching. A claim can be `Unknown` in one line and `No` in
        // another — common when a new predicate key reaches only some lines — so "every release" is
        // only true of the all-`Unknown` case.
        let id = &loaded.claim.claim_id;
        let undecided = states.iter().filter(|s| **s == Applies::Unknown).count();
        if undecided == states.len() {
            unexamined.push(format!(
                "{id}: undecided in every release — no line supplies a key its predicate needs"
            ));
        } else if undecided > 0 {
            unexamined.push(format!(
                "{id}: undecided in {undecided} of {} releases and decided as excluded in the rest \
                 — one line must supply the missing key, and one must match its value",
                states.len()
            ));
        } else {
            unexamined.push(format!(
                "{id}: excluded by every release — every line decides it and no value matches"
            ));
        }
    }
    assert!(
        unexamined.is_empty(),
        "these claims are examined by no declared release, so `req gate` never checks them:\n  {}",
        unexamined.join("\n  ")
    );
}

/// Each declared release is green through the binary the gate would actually run — the check
/// that catches a badly-chosen context. Measured before this existed: a context carrying only
/// `set` makes REQ-0009 exit 3, because REQ-0009 refines REQ-0008 and REQ-0008 is then
/// undecided; the gate refuses to answer instead of skipping.
///
/// That refusal is pinned by `an_incomplete_context_is_not_green` below, not by the loop here: today's
/// two lines are complete, so a regression that stopped refusing an undecided dependency would leave
/// this test green — the negative direction is what makes the doc's claim true.
///
/// ADR 0015 does not retire that pin. A `gate` that leaves any claim undecided now exits 3 on the
/// count alone, so this loop's negative direction has to assert the **policy**, not just the code:
/// otherwise the undecided-count rule would keep it green while `dependencies_decided` silently
/// stopped answering.
#[test]
fn every_declared_release_is_gate_green() {
    let lines = common::variant_lines();
    assert!(!lines.is_empty(), ".req/variants must declare a release");
    for line in lines {
        let args: Vec<&str> = line.split_whitespace().collect();
        let out = cli::dogfood(&args);
        assert_eq!(
            out.code, 0,
            "the declared release `{line}` is not green:\n{}",
            out.text
        );
        // Green is not enough: a line is a **complete** release context, so it must leave nothing
        // undecided. Since ADR 0015 an undecided claim also makes the gate exit 3, but the
        // assertion stays because it names the defect precisely — a line that stopped deciding
        // every scoped claim must fail here as the *incomplete line* it is, not as a generic
        // UNVERIFIABLE.
        let headline = out.headline();
        assert!(
            headline.contains(", 0 undecided"),
            "the declared release `{line}` leaves a claim undecided, so the gate does not examine \
             it — a complete release context decides every scoped claim:\n{headline}"
        );
    }
}

/// The negative direction: a context that leaves a dependency undecided must **not** be green. Without
/// it, `every_declared_release_is_gate_green` only exercises today's two declared lines, and the
/// refusal its doc describes would be unpinned — a gate that stopped answering UNVERIFIABLE for an
/// incomplete context would pass every test in this file.
#[test]
fn an_incomplete_context_is_not_green() {
    // `set` alone leaves REQ-0008 undecided and REQ-0009 refines REQ-0008: measured, exit 3 with
    // `17 applying claim(s), 5 undecided (an undecided claim is never treated as absent)`.
    let out = cli::dogfood(&["set=direction_gold_structural"]);
    // Exactly 3, not merely non-zero: 1 is FAIL (a defect found) and 2 is a usage error, and neither is
    // the answer "I could not decide what this release contains". A regression that collapsed
    // undecided into a generic failure would satisfy `!= 0` and mean something else entirely.
    assert_eq!(
        out.code, 3,
        "an incomplete context must be UNVERIFIABLE (3), not FAIL (1) or a usage error (2):\n{}",
        out.text
    );
    assert!(
        out.headline().contains("undecided"),
        "and it must say what is undecided rather than skipping it:\n{}",
        out.headline()
    );
    // The *dependency* is still what this test is about. Since ADR 0015 the undecided count alone
    // would keep the exit code at 3, so a regression that stopped refusing an undecided dependency
    // would now pass a code-only assertion — the policy is asserted so this test still fails for
    // its own reason.
    let decided = out.policy("dependencies_decided");
    assert!(decided.starts_with("[UNVERIFIABLE]"), "{decided}");
    assert!(
        decided.contains("REQ-0009 -refines-> REQ-0008"),
        "the undecided dependency must be named: {decided}"
    );
}

/// ADR 0015's falsification, run: a bare `req gate` over this repository's corpus leaves the five
/// variant-scoped claims undecided, so it exits 3 and **names each one**, with the remedy stated.
/// `every_declared_release_is_gate_green` above is the other half — a declared release decides
/// every claim and exits 0. If a bare gate can still exit 0 while the banner counts an undecided
/// claim, the ADR has not landed.
#[test]
fn a_bare_gate_is_unverifiable_and_names_every_undecided_claim() {
    let out = cli::dogfood(&[]);
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(
        out.headline().contains("16 applying claim(s), 5 undecided"),
        "{}",
        out.headline()
    );
    let named = out.undecided();
    for id in ["REQ-0008", "REQ-0009", "REQ-0010", "REQ-0012", "REQ-0018"] {
        assert!(
            named.contains(id),
            "the undecided line must name {id}, which the invocation did not examine: {named}"
        );
    }
    assert!(
        out.text.contains("remedy:"),
        "the gate must state the remedy, not only the finding: {}",
        out.text
    );
}

// ---------------------------------------------------------------------------------------------
// The test-evidence policy (ADR 0019): the four outcomes, the report-level unverifiable, and the
// process declaration that is reported PASS-as-declared and **never inferred from an empty list**.
// These run the CLI so the exit codes are the fold the acceptance table names, not a policy's
// own field.
// ---------------------------------------------------------------------------------------------

mod evidence {
    use super::cli::{self, Fixture, write_junit};

    /// The gate over a one-claim corpus and a report of the caller's choosing.
    fn gate_with(tag: &str, tests: &[&str], cases: &[(&str, &str)]) -> cli::Outcome {
        let fixture = Fixture::new(tag);
        fixture.claim_with("REQ-9001", &[], tests, "machine");
        fixture.fill();
        let report = fixture.path().join("report.xml");
        write_junit(&report, cases);
        fixture.gate_with_report(&[], &report)
    }

    /// **UNVERIFIABLE — no linked test.** The flagship answer: a claim the gate cannot show a
    /// passing proof for is named, and the exit code is the existing 3.
    #[test]
    fn a_claim_with_no_linked_test_is_unverifiable_and_named() {
        let fixture = Fixture::new("evidence-no-link");
        fixture.claim_with("REQ-9001", &[], &[], "machine");
        fixture.claim_with("REQ-9002", &[], &["fixture::b"], "machine");
        fixture.fill();
        let report = fixture.path().join("report.xml");
        write_junit(&report, &[("fixture::b", "passed")]);
        let out = fixture.gate_with_report(&[], &report);
        assert_eq!(
            out.code, 3,
            "no link is UNVERIFIABLE, never a pass:\n{}",
            out.text
        );
        let policy = out.policy("tests_linked");
        assert!(policy.starts_with("[UNVERIFIABLE]"), "{policy}");
        assert!(
            policy.contains("REQ-9001: no linked test"),
            "the claim is named: {policy}"
        );
        // The linked-and-passing claim is not an offender.
        assert!(!policy.contains("REQ-9002"), "{policy}");
    }

    /// **UNVERIFIABLE — linked test not in the report.** The rename defence: a renamed or deleted
    /// test must never read as a pass.
    #[test]
    fn a_linked_test_missing_from_the_report_is_unverifiable_never_pass() {
        let out = gate_with(
            "evidence-renamed",
            &["fixture::renamed_away"],
            &[("fixture::other", "passed")],
        );
        assert_eq!(out.code, 3, "{}", out.text);
        let policy = out.policy("tests_linked");
        assert!(policy.starts_with("[UNVERIFIABLE]"), "{policy}");
        assert!(
            policy.contains("linked test not in the report: fixture::renamed_away"),
            "{policy}"
        );
    }

    /// **FAIL — a linked test failed.** A failure outranks the open questions around it.
    #[test]
    fn a_failed_linked_test_fails_the_gate() {
        let out = gate_with(
            "evidence-failed",
            &["fixture::a", "fixture::b"],
            &[("fixture::a", "failed"), ("fixture::b", "passed")],
        );
        assert_eq!(out.code, 1, "a failed proof is a defect:\n{}", out.text);
        let policy = out.policy("tests_linked");
        assert!(policy.starts_with("[FAIL]"), "{policy}");
        assert!(
            policy.contains("linked test failed: fixture::a"),
            "{policy}"
        );
    }

    /// **FAIL — every linked test present and none passed.** A skipped test is present and is
    /// **not** a pass; that is the distinction the outcome table exists for.
    #[test]
    fn a_skipped_test_is_not_a_pass() {
        let out = gate_with(
            "evidence-skipped",
            &["fixture::ignored"],
            &[("fixture::ignored", "skipped")],
        );
        assert_eq!(
            out.code, 1,
            "a skipped test is present and not a pass:\n{}",
            out.text
        );
        let policy = out.policy("tests_linked");
        assert!(policy.starts_with("[FAIL]"), "{policy}");
        assert!(policy.contains("a skipped test is not a pass"), "{policy}");

        // A skipped test beside a passed one is not a failure: any linked test that passed is
        // enough to prove the claim (the failure branch is "none passed").
        let out = gate_with(
            "evidence-skip-and-pass",
            &["fixture::ignored", "fixture::passing"],
            &[
                ("fixture::ignored", "skipped"),
                ("fixture::passing", "passed"),
            ],
        );
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.policy("tests_linked").starts_with("[PASS]"),
            "{}",
            out.text
        );
    }

    /// **The report itself.** No report, an unreadable one, or one with zero test cases is
    /// UNVERIFIABLE **once**, naming the reason — never "no failures found", and never repeated
    /// per claim (noise is how a real red gets ignored).
    #[test]
    fn an_unreadable_report_is_unverifiable_once_naming_the_reason() {
        let missing = gate_with("evidence-no-report", &["fixture::a"], &[]);
        // (The empty report is its own case below; first the missing file.)
        assert_eq!(missing.code, 3, "{}", missing.text);

        let fixture = Fixture::new("evidence-unreadable");
        fixture.claim_with("REQ-9001", &[], &["fixture::a"], "machine");
        fixture.claim_with("REQ-9002", &[], &["fixture::b"], "machine");
        fixture.fill();
        let out = fixture.gate_raw(&[]);
        assert_eq!(out.code, 3, "{}", out.text);
        let policy = out.policy("tests_linked");
        assert_eq!(
            policy.matches("no --test-report was supplied").count(),
            1,
            "the report-level reason is stated once, not once per claim: {policy}"
        );
        assert!(policy.starts_with("[UNVERIFIABLE]"), "{policy}");

        let unreadable = fixture.gate_with_report(&[], &fixture.path().join("absent.xml"));
        assert_eq!(unreadable.code, 3, "{}", unreadable.text);
        let policy = unreadable.policy("tests_linked");
        assert_eq!(
            policy.matches("the test report cannot be read").count(),
            1,
            "{policy}"
        );

        let empty = fixture.path().join("empty.xml");
        write_junit(&empty, &[]);
        let zero = fixture.gate_with_report(&[], &empty);
        assert_eq!(
            zero.code, 3,
            "zero test cases verify nothing:\n{}",
            zero.text
        );
        assert!(
            zero.policy("tests_linked")
                .contains("carries no test cases"),
            "{}",
            zero.text
        );
    }

    /// **A flag without its value is a usage error**, before the corpus or the report is touched:
    /// exit 2, not UNVERIFIABLE — a malformed invocation is not a question about evidence.
    #[test]
    fn test_report_without_a_value_is_a_usage_error() {
        let fixture = Fixture::new("evidence-missing-value");
        fixture.claim("REQ-9001", &[]);
        fixture.fill();
        let out = fixture.gate_raw(&["--test-report"]);
        assert_eq!(out.code, 2, "{}", out.text);
        assert!(
            out.text.contains("--test-report needs a value"),
            "{}",
            out.text
        );
    }

    /// **A process declaration is an answer, not a missing link.** It passes as declared, is
    /// named per claim and counted; an empty `tests:` list is *never* inferred to be one.
    #[test]
    fn a_process_declaration_passes_as_declared_and_is_counted() {
        let fixture = Fixture::new("evidence-process");
        fixture.claim_with("REQ-9001", &[], &[], "process");
        fixture.claim_with("REQ-9002", &[], &[], "process");
        fixture.fill();
        // No report at all: nothing machine-evidenced needs it.
        let out = fixture.gate_raw(&[]);
        assert_eq!(out.code, 0, "{}", out.text);
        let policy = out.policy("tests_linked");
        assert!(policy.starts_with("[PASS]"), "{policy}");
        assert!(
            policy.contains("process-evidenced (declared, not machine-proven): REQ-9001, REQ-9002"),
            "{policy}"
        );
        assert!(
            out.text.contains("process-evidenced: 2"),
            "the summary must carry the count: {}",
            out.text
        );

        // The same corpus with the declaration removed is NOT process-evidenced: the default is
        // machine, and an empty link list is the no-link finding. This is the mutation the outcome
        // table calls out — process must never be inferred from empty tests.
        let machine = Fixture::new("evidence-process-not-inferred");
        machine.claim_with("REQ-9001", &[], &[], "machine");
        machine.fill();
        let out = machine.gate_raw(&[]);
        assert_eq!(out.code, 3, "{}", out.text);
        assert!(
            !out.text.contains("process-evidenced"),
            "an empty `tests:` list must never be reported as process evidence: {}",
            out.text
        );
        assert!(
            out.policy("tests_linked")
                .contains("no --test-report was supplied"),
            "{}",
            out.text
        );
    }
}
