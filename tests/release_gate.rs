//! The release gate: three outcomes, and the scoping that makes it useful.

mod common;

use common::{rationale, row, stored};
use req::fill::StoredState;
use req::gate::{Outcome, evaluate, exit_code};
use req::select::{Applies, Context};

#[test]
fn a_clean_selection_passes_every_policy() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Yes, &[]),
    ];
    let results = evaluate(&rows, &Context::new());
    assert_eq!(exit_code(&results), 0, "{results:?}");
    assert!(results.iter().all(|r| r.outcome == Outcome::Pass));
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
    let results = evaluate(&rows, &Context::new());
    assert_eq!(exit_code(&results), 0, "{results:?}");
}

#[test]
fn a_hand_edited_digest_fails_and_an_unfilled_claim_does_not_pass() {
    let tampered = vec![stored(
        row("REQ-A", Applies::Yes, &[]),
        StoredState::Mismatch("stored address does not recompute".to_string()),
    )];
    let results = evaluate(&tampered, &Context::new());
    assert_eq!(exit_code(&results), 1);
    assert_eq!(results[0].outcome, Outcome::Fail);
    assert!(results[0].offenders[0].contains("does not recompute"));

    let unfilled = vec![stored(
        row("REQ-A", Applies::Yes, &[]),
        StoredState::Candidate("unfilled".to_string()),
    )];
    let results = evaluate(&unfilled, &Context::new());
    assert_eq!(
        exit_code(&results),
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
    let results = evaluate(&rows, &Context::new());
    assert_eq!(exit_code(&results), 1, "{results:?}");
}

#[test]
fn an_undecided_dependency_blocks_the_release() {
    let rows = vec![
        row("REQ-A", Applies::Yes, &[("refines", "REQ-B")]),
        row("REQ-B", Applies::Unknown, &[]),
    ];
    let results = evaluate(&rows, &Context::new());
    assert_eq!(exit_code(&results), 3);
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
    let results = evaluate(&[], &Context::new());
    let policy = results
        .iter()
        .find(|r| r.name == "selectors_confirmed")
        .expect("the invocation is judged as a policy");
    assert_eq!(policy.outcome, Outcome::Unverifiable, "{policy:?}");
    assert_eq!(exit_code(&results), 3);
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

    /// The dogfood corpus, whose selector vocabulary the oracle read: `deployment_mode ==
    /// air_gapped`, `runs >= 20`, `corpus_set == parity` / `parity_and_adversarial`, `set ==
    /// direction_gold_structural`, `field == value`.
    fn dogfood(args: &[&str]) -> Outcome {
        gate_at(&PathBuf::from(env!("CARGO_MANIFEST_DIR")), args)
    }

    fn gate_at(root: &Path, args: &[&str]) -> Outcome {
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
            std::fs::write(
                self.0.join("claims").join(format!("{id}.yml")),
                claim_text(id, applicability),
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

        pub fn gate(&self, args: &[&str]) -> Outcome {
            gate_at(self.path(), args)
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
    fn claim_text(id: &str, applicability: &[&str]) -> String {
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
            r#"schema: req.claim/0
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

    // ---- the context the corpus confirms: still a PASS ---------------------------------------

    /// The declared value, on the corpus that declares it: the fix refuses typo'd values, not the
    /// vocabulary the corpus actually has.
    #[test]
    fn the_value_the_corpus_declares_still_passes() {
        let out = dogfood(&["field=value"]);
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.headline().contains("16 applying claim(s), 5 undecided"),
            "{}",
            out.headline()
        );
        assert!(
            out.policy("selectors_confirmed").starts_with("[PASS]"),
            "{}",
            out.policy("selectors_confirmed")
        );
    }

    #[test]
    fn the_declared_air_gapped_value_still_passes() {
        let out = dogfood(&["deployment_mode=air_gapped"]);
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.headline().contains("16 applying claim(s), 5 undecided"),
            "{}",
            out.headline()
        );
    }

    // ---- the invocation rows: nothing selected, and a key nothing mentions ---------------------

    /// A variant in which nothing applies verified nothing, so it is UNVERIFIABLE — never PASS.
    /// The dogfood corpus cannot reach this: 15 of its 21 claims carry no predicate and so always
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

    /// A context key no predicate in the corpus mentions decided nothing at all: it selected
    /// nothing and excluded nothing, so naming it is honest. This corpus carries no selector
    /// outside the grammar, so the message states what the corpus decides and claims nothing
    /// about unread selectors (the fixture below pins the case where it does).
    #[test]
    fn a_key_no_applicability_predicate_mentions_is_refused() {
        let out = dogfood(&["nonsense=whatever"]);
        // The oracle: 15 applying, 6 undecided — one claim MORE undecided than the 5 the
        // context-free run has, because the entry decided nothing.
        assert!(
            out.headline().contains("15 applying claim(s), 6 undecided"),
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
            out.headline().contains("15 applying claim(s), 5 undecided"),
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
        // Nothing else is wrong with this corpus, so the refusal is the whole reason for the 3.
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
            out.headline().contains("15 applying claim(s), 5 undecided"),
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
    /// `runs >= 20` decides it Yes. It already exits 3 — for the unrelated reason asserted below —
    /// so a bare `assert_eq!(code, 3)` would pass with the exemption missing entirely.
    #[test]
    fn a_ge_value_is_confirmed_by_comparison_and_never_flagged() {
        let out = dogfood(&["runs=100"]);
        assert_eq!(out.code, 3, "{}", out.text);
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
        // The reason this run is UNVERIFIABLE at all, and it predates this change.
        let decided = out.policy("dependencies_decided");
        assert!(decided.starts_with("[UNVERIFIABLE]"), "{decided}");
        assert!(
            decided.contains("REQ-0011 -refines-> REQ-0008"),
            "{decided}"
        );
    }

    /// A non-decimal value for a `Ge` key is not guessed and not refused: the predicate decides
    /// nothing, so REQ-0011 lands in the undecided count (6 here against the 5 of a decided
    /// `runs`), while the entry itself stays confirmed — a `Ge` key enumerates no legal values to
    /// check it against. The gate reports the undecided count rather than inventing a finding.
    #[test]
    fn a_non_numeric_ge_value_is_confirmed_and_surfaces_as_undecided() {
        let out = dogfood(&["runs=abc"]);
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.headline().contains("15 applying claim(s), 6 undecided"),
            "{}",
            out.headline()
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

    let mut uncovered = Vec::new();
    for loaded in &claims {
        let predicates: Vec<_> = loaded
            .claim
            .applicability
            .iter()
            .map(|text| req::select::parse_predicate(text))
            .collect();
        if !contexts
            .iter()
            .any(|context| req::select::applies(&predicates, context) == Applies::Yes)
        {
            uncovered.push(loaded.claim.claim_id.clone());
        }
    }
    assert!(
        uncovered.is_empty(),
        "these claims apply in no declared release, so `req gate` never examines them: {uncovered:?}"
    );
}

/// Each declared release is green through the binary the gate would actually run — the check
/// that catches a badly-chosen context. Measured before this existed: a context carrying only
/// `set` makes REQ-0009 exit 3, because REQ-0009 refines REQ-0008 and REQ-0008 is then
/// undecided; the gate refuses to answer instead of skipping.
#[test]
fn every_declared_release_is_gate_green() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let binary = std::env::var_os("CARGO_BIN_EXE_req")
        .map(std::path::PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set");
    let lines = common::variant_lines();
    assert!(!lines.is_empty(), ".req/variants must declare a release");
    for line in lines {
        let out = std::process::Command::new(&binary)
            .arg("gate")
            .args(line.split_whitespace())
            .current_dir(&root)
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs `req gate`");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(0),
            "the declared release `{line}` is not green:\n{stdout}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Green is not enough, and this is the invariant a green gate does not give you: only
        // `corpus_set` is dependency-guarded, so dropping `set`, `runs`, `deployment_mode` or
        // `field` leaves that claim **undecided in this release — unexamined** — while the gate
        // still exits 0. A line is a complete release context, so it must leave nothing undecided.
        assert!(
            stdout.contains(", 0 undecided"),
            "the declared release `{line}` leaves a claim undecided, so the gate does not examine \
             it — a complete release context decides every scoped claim:\n{stdout}"
        );
    }
}
