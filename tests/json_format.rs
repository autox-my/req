//! Pack C — `--format {text,json}` on `render`, `trace`, `audit` and `gate`.
//!
//! Red-first: this file was written before the flag existed, so every `--format json`
//! invocation below started as a usage error (exit 2). The seam is the **CLI**, not a
//! serializer: a consumer parses stdout, so that is what these tests read.
//!
//! Three properties are load-bearing, and each has its own tests below:
//!
//! * **text is the default** and is byte-identical to an explicit `--format text`: adding a
//!   document must not edit the rendering a human already reads;
//! * **JSON and text return the same exit code** on the same corpus — the document is a
//!   rendering of the verdict, never a new class of answer;
//! * **two runs are byte-identical** — determinism is contract (BTreeMap, sorted arrays, no
//!   floats), not a hope.
//!
//! The top-level shape is pinned per command because the point of the pack is that an agent
//! can consume structure instead of scraping text. The key set may grow; the top-level keys
//! named here may not vanish.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

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
        // The resolver reads `REQ_ROOT` before the walk-up, so an inherited value would name a
        // corpus and make each fixture check something other than what it says.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Distinguishes fixtures within one process; the process id extends it across processes.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir(name: &str) -> PathBuf {
    let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "req-json-format-{name}-{}-{seq}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creates the fixture directory");
    dir
}

/// `text.replace(old, new)` that fails, naming `old`, when the starter claim no longer contains
/// it: a template edit must not turn a fixture's substitution into a silent no-op.
fn replaced(text: String, old: &str, new: &str) -> String {
    assert!(
        text.contains(old),
        "the starter claim no longer contains {old:?}: the fixture's substitution is a no-op"
    );
    text.replace(old, new)
}

/// A fixture corpus created by the real bootstrap (`req init`): authored `req.claim/2` claims,
/// whose parse and address the loader derives — nothing derived is written (ADR 0022).
struct Corpus(PathBuf);

impl Corpus {
    /// The starter corpus: one applying claim, no selector, full provenance.
    fn new(name: &str) -> Self {
        let dir = unique_dir(name);
        let out = run_at(&dir, &["init"]);
        assert_eq!(
            out.code, 0,
            "`req init` must create the fixture corpus:\n{}\n{}",
            out.stdout, out.stderr
        );
        // Link the starter claim to a test the fixture report carries: the release gate now asks
        // for evidence, and every test in this file judges a *rendering*, not the missing report.
        let claim = fs::read_to_string(dir.join("claims/REQ-0001.yml")).expect("the starter claim");
        fs::write(
            dir.join("claims/REQ-0001.yml"),
            replaced(
                claim,
                "relations: []\n",
                "relations: []\ntests:\n  - \"fixture::REQ-0001\"\n",
            ),
        )
        .expect("links the starter claim");
        Corpus(dir)
    }

    /// The starter corpus plus a second claim with the given statement, applicability and
    /// outgoing relations. It carries **no pins**: it is judged only by what it derives, so a
    /// statement without a 200 ms bound is not a `pins_hold` failure by inheritance. Nothing
    /// derived is written: the loader derives it (ADR 0022).
    fn with_second_claim(
        name: &str,
        statement: &str,
        applicability: &[&str],
        relations: &[(&str, &str)],
    ) -> Self {
        let corpus = Corpus::new(name);
        corpus.add_claim("REQ-0002", statement, applicability, relations);
        corpus
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn claim_path(&self, id: &str) -> PathBuf {
        self.path().join("claims").join(format!("{id}.yml"))
    }

    fn add_claim(
        &self,
        id: &str,
        statement: &str,
        applicability: &[&str],
        relations: &[(&str, &str)],
    ) {
        let first = fs::read_to_string(self.claim_path("REQ-0001")).expect("the starter claim");
        let applicability_block = if applicability.is_empty() {
            "applicability: []".to_string()
        } else {
            let mut block = String::from("applicability:");
            for predicate in applicability {
                block.push_str(&format!("\n  - {predicate}"));
            }
            block
        };
        let relations_block = if relations.is_empty() {
            "relations: []".to_string()
        } else {
            let mut block = String::from("relations:");
            for (kind, to) in relations {
                block.push_str(&format!("\n  - type: {kind}\n    to: {to}"));
            }
            block
        };
        // Every substitution must land: a template edit that moves one of these literals fails
        // here, naming it, instead of leaving a copy that quietly kept the starter's statement.
        let text = replaced(first, "claim_id: REQ-0001", &format!("claim_id: {id}"));
        let text = replaced(text, "fixture::REQ-0001", &format!("fixture::{id}"));
        let text = replaced(
            text,
            "statement: \"The API shall respond to a health check within 200 ms.\"",
            &format!("statement: {statement:?}"),
        );
        let text = replaced(text, "applicability: []", &applicability_block);
        let text = replaced(text, "relations: []", &relations_block);
        // The starter's pins describe the starter's own statement: drop that block, and only
        // that block (every following top-level key is kept), then check the authored fields
        // survived and the pins did not.
        let text = match text.find("\npins:\n") {
            Some(at) => {
                let after = &text[at + 1..];
                let end = after
                    .lines()
                    .skip(1)
                    .position(|l| !l.is_empty() && !l.starts_with(' '))
                    .map(|n| {
                        after
                            .lines()
                            .take(n + 1)
                            .map(|l| l.len() + 1)
                            .sum::<usize>()
                    });
                match end {
                    Some(len) => format!("{}{}", &text[..at + 1], &after[len..]),
                    None => text[..at + 1].to_string(),
                }
            }
            None => text,
        };
        assert!(
            !text.contains("\npins:"),
            "the copied claim still carries the starter's pins:\n{text}"
        );
        for key in [
            "schema:",
            "claim_id:",
            "statement:",
            "applicability:",
            "relations:",
            "rationale:",
        ] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "the copied claim lost `{key}`:\n{text}"
            );
        }
        fs::write(self.claim_path(id), text).expect("writes the second claim");
    }

    /// Write the JUnit report the gate reads, from the links the fixture claims carry, and return
    /// its path. Regenerated per call, so a claim added after the last report is still covered.
    fn report(&self) -> String {
        let path = self.path().join("junit.xml");
        common::passing_junit(&self.path().join("claims"), &path);
        path.to_string_lossy().into_owned()
    }

    /// `req gate` with this fixture's report, and any extra flags/context.
    fn run_gate(&self, extra: &[&str]) -> Outcome {
        let report = self.report();
        let mut args = vec!["gate", "--test-report", report.as_str()];
        args.extend_from_slice(extra);
        run_at(self.path(), &args)
    }

    /// Rewrite a claim's statement. Nothing is stored to re-derive (ADR 0022): the next load
    /// derives the new parse and address. A statement whose bound no longer matches the starter
    /// claim's `pins:` is the definite defect `gate` reports as FAIL (`pins_hold`).
    fn rewrite_statement(&self, id: &str, statement: &str) {
        let path = self.claim_path(id);
        let text = fs::read_to_string(&path).expect("the claim file");
        let mut replaced = false;
        let rewritten: Vec<String> = text
            .lines()
            .map(|line| {
                if line.starts_with("statement: ") {
                    replaced = true;
                    format!("statement: {statement:?}")
                } else {
                    line.to_string()
                }
            })
            .collect();
        assert!(replaced, "the fixture claim has a statement line to move");
        fs::write(&path, format!("{}\n", rewritten.join("\n"))).expect("writes the edited claim");
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Parse a JSON document, naming the output when it is not one.
fn parse(stdout: &str) -> Value {
    serde_json::from_str(stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}):\n{stdout}"))
}

/// The sorted top-level keys of an object, for exact-shape assertions.
fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("the document is a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

fn assert_keys(value: &Value, expected: &[&str]) {
    assert_eq!(
        keys(value),
        expected,
        "the document's top-level keys are the pinned shape:\n{}",
        serde_json::to_string_pretty(value).expect("a parsed document re-serializes")
    );
}

// ---------------------------------------------------------------------------
// The flag contract: default text, text spelled explicitly, unknown values, --
// format beside --out/--check, and the command scope.
// ---------------------------------------------------------------------------

/// The default is `text`, and `--format text` is the same bytes: the flag must not become a
/// second renderer for the rendering already in use.
#[test]
fn text_is_the_default_and_explicit_text_is_byte_identical() {
    let corpus = Corpus::with_second_claim(
        "text-default",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[("refines", "REQ-0001")],
    );
    // gate bare is PASS on this corpus (no selector, full provenance); audit is UNVERIFIABLE
    // with no Baseline. Both classes are fine here: this test is about the rendering, not the
    // verdict, and the codes are compared too.
    for command in ["render", "trace", "gate", "audit"] {
        let default = run_at(corpus.path(), &[command]);
        let explicit = run_at(corpus.path(), &[command, "--format", "text"]);
        assert_eq!(
            default.code, explicit.code,
            "`{command}` and `{command} --format text` are the same answer"
        );
        assert_eq!(
            default.stdout, explicit.stdout,
            "`{command} --format text` must print exactly the default text:\n\
             default:\n{}\nexplicit:\n{}",
            default.stdout, explicit.stdout
        );
        assert!(
            !default.stdout.trim_start().starts_with('{'),
            "`{command}`'s default output is still human text, not a JSON document:\n{}",
            default.stdout
        );
    }
}

/// A value outside `text|json` is a usage error (2), never a default or a guess.
#[test]
fn an_unknown_format_value_is_a_usage_error() {
    let corpus = Corpus::new("unknown-format");
    for command in ["render", "trace", "gate", "audit"] {
        let out = run_at(corpus.path(), &[command, "--format", "yaml"]);
        assert_eq!(
            out.code, 2,
            "`{command} --format yaml` must be a usage error:\n{}\n{}",
            out.stdout, out.stderr
        );
        assert_eq!(out.stdout, "", "a usage error writes no document");
        assert!(
            out.stderr.contains("yaml")
                && out.stderr.contains("text")
                && out.stderr.contains("json"),
            "the error names the refused value and the two it accepts:\n{}",
            out.stderr
        );

        let missing = run_at(corpus.path(), &[command, "--format"]);
        assert_eq!(
            missing.code, 2,
            "`{command} --format` with no value is a usage error:\n{}",
            missing.stderr
        );
    }
}

/// v1 renders JSON to stdout only. `--out`/`--check` are text projections, so pairing either
/// with `--format json` is a usage error that says where the extension point is — and it
/// writes nothing.
#[test]
fn json_beside_out_or_check_is_a_usage_error_and_writes_nothing() {
    let corpus = Corpus::with_second_claim(
        "json-out",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let out_path = corpus.path().join("spec-json.md");
    let check_path = corpus.path().join("trace-json.md");
    fs::write(&check_path, "not a projection\n").expect("writes the check fixture");

    let out = run_at(
        corpus.path(),
        &[
            "render",
            "--format",
            "json",
            "--out",
            out_path.to_str().expect("fixture path is UTF-8"),
        ],
    );
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(!out_path.exists(), "a refused write writes nothing");
    assert!(
        out.stderr.contains("--format json")
            && out.stderr.contains("stdout")
            && out.stderr.contains("follow-on"),
        "the error states that JSON is stdout only and names the extension point:\n{}",
        out.stderr
    );

    let before = fs::read_to_string(&check_path).expect("the check fixture is readable");
    let check = run_at(
        corpus.path(),
        &[
            "trace",
            "--format",
            "json",
            "--check",
            check_path.to_str().expect("fixture path is UTF-8"),
        ],
    );
    assert_eq!(check.code, 2, "{}", check.stderr);
    assert_eq!(
        fs::read_to_string(&check_path).expect("the check fixture is readable"),
        before,
        "a refused check writes nothing"
    );
}

/// The scope is exactly the four commands. `select`, `impact`, `canon`, `baseline` and
/// `decision` keep their own contracts; `--format` there is an unknown argument, exit 2.
#[test]
fn format_json_is_scoped_to_the_four_commands() {
    let corpus = Corpus::new("format-scope");
    for argv in [
        &["select", "--format", "json"][..],
        &["impact", "REQ-0001", "--format", "json"][..],
        &["canon", "--format", "json"][..],
        &["baseline", "--format", "json"][..],
        &["decision", "--format", "json"][..],
    ] {
        let out = run_at(corpus.path(), argv);
        assert_eq!(
            out.code, 2,
            "{argv:?} is outside the pack's scope and must stay a usage error:\n{}\n{}",
            out.stdout, out.stderr
        );
    }
}

// ---------------------------------------------------------------------------
// render
// ---------------------------------------------------------------------------

/// The render document: envelope, the three sections kept apart, the shared row shape, and an
/// identity that is the projection's own digest over the applying addresses.
#[test]
fn render_json_has_the_pinned_shape_and_keeps_the_sections_apart() {
    let corpus = Corpus::with_second_claim(
        "render-shape",
        "The API shall respond to a readiness check within 200 ms.",
        &["deployment_mode == air_gapped"],
        &[],
    );

    let out = run_at(corpus.path(), &["render", "--format", "json"]);
    assert_eq!(
        out.code, 3,
        "one claim is undecided, so the plain verdict is open under both formats:\n{}",
        out.stderr
    );
    let document = parse(&out.stdout);
    assert_keys(
        &document,
        &[
            "applies",
            "generator",
            "identity",
            "outside",
            "rules_v",
            "schema",
            "selection",
            "undecided",
        ],
    );
    assert_eq!(document["schema"], "req.render/1");
    assert_eq!(document["generator"], "render");
    assert_eq!(document["rules_v"], req::canon::RULES_V);
    assert_eq!(document["selection"], serde_json::json!({}));

    // The separation the text path keeps is the separation the document keeps: an undecided
    // claim is in `undecided`, never in `applies`.
    assert_eq!(document["applies"].as_array().expect("an array").len(), 1);
    assert_eq!(document["undecided"].as_array().expect("an array").len(), 1);
    assert_eq!(document["outside"].as_array().expect("an array").len(), 0);
    assert_eq!(document["applies"][0]["claim_id"], "REQ-0001");
    assert_eq!(document["applies"][0]["applies"], "applies");
    assert_eq!(document["undecided"][0]["claim_id"], "REQ-0002");
    assert_eq!(document["undecided"][0]["applies"], "unknown");

    // One row shape, built from the real `Row`: the internals the audit surface judges stay out.
    assert_keys(
        &document["applies"][0],
        &[
            "address",
            "applies",
            "claim_id",
            "modality",
            "rationale",
            "relations",
            "selectors",
            "statement",
        ],
    );
    assert_eq!(
        document["undecided"][0]["selectors"][0],
        "deployment_mode == air_gapped"
    );

    // The address is the loaded claim's own address, rendered with its algorithm; the identity
    // is `view::identity_digest` over the applying addresses — never recomputed here.
    let context = req::select::Context::new();
    let rows = req::view::load_rows(&context, corpus.path()).expect("the fixture corpus loads");
    for section in ["applies", "undecided"] {
        for row in document[section].as_array().expect("an array") {
            let id = row["claim_id"].as_str().expect("a claim id");
            let loaded = rows
                .iter()
                .find(|row| row.claim_id == id)
                .expect("the document names a loaded claim");
            assert_eq!(
                row["address"],
                format!("sha256:{}", loaded.address),
                "the address is the loaded claim's, with its algorithm named"
            );
        }
    }
    let applying: Vec<String> = rows
        .iter()
        .filter(|row| row.applies == req::select::Applies::Yes)
        .map(|row| row.address.clone())
        .collect();
    assert_eq!(
        document["identity"],
        format!("sha256:{}", req::view::identity_digest(&applying)),
        "identity is the projection's digest over the sorted applying addresses"
    );

    // With the variant supplied every claim applies, and the identity moves with the set.
    let decided = run_at(
        corpus.path(),
        &["render", "--format", "json", "deployment_mode=air_gapped"],
    );
    assert_eq!(decided.code, 0, "{}", decided.stderr);
    let decided = parse(&decided.stdout);
    assert_eq!(
        decided["selection"],
        serde_json::json!({"deployment_mode": "air_gapped"})
    );
    assert_eq!(decided["applies"].as_array().expect("an array").len(), 2);
    assert_eq!(decided["undecided"].as_array().expect("an array").len(), 0);
    assert_ne!(decided["identity"], document["identity"]);
}

/// `identity` moves when a claim's address moves: it is derived from the corpus, not cached in
/// the renderer.
#[test]
fn identity_moves_when_a_claim_address_moves() {
    let corpus = Corpus::with_second_claim(
        "identity-moves",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let before = run_at(corpus.path(), &["render", "--format", "json"]);
    assert_eq!(before.code, 0, "{}", before.stderr);
    let before = parse(&before.stdout);

    corpus.rewrite_statement(
        "REQ-0002",
        "The API shall answer a readiness check within 200 ms.",
    );
    let after = run_at(corpus.path(), &["render", "--format", "json"]);
    assert_eq!(after.code, 0, "{}", after.stderr);
    let after = parse(&after.stdout);
    assert_ne!(
        before["identity"], after["identity"],
        "the digest is over the addresses, so a moved address moves it"
    );
    assert_ne!(
        before["applies"][1]["address"], after["applies"][1]["address"],
        "the moved claim's address is re-derived"
    );
}

// ---------------------------------------------------------------------------
// trace
// ---------------------------------------------------------------------------

/// The trace document: the outgoing edges, the **derived** incoming edges (stored once,
/// REQ-0020), and the findings.
#[test]
fn trace_json_serializes_edges_derived_incoming_and_findings() {
    let corpus = Corpus::with_second_claim(
        "trace-shape",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[("refines", "REQ-0001")],
    );
    let out = run_at(corpus.path(), &["trace", "--format", "json"]);
    assert_eq!(out.code, 0, "the edge graph is settled:\n{}", out.stderr);
    let document = parse(&out.stdout);
    assert_keys(
        &document,
        &[
            "derived_incoming",
            "edges",
            "findings",
            "generator",
            "rules_v",
            "schema",
            "selection",
        ],
    );
    assert_eq!(document["schema"], "req.trace/1");
    assert_eq!(document["generator"], "trace");
    assert_eq!(document["rules_v"], req::canon::RULES_V);

    let edge = serde_json::json!({"from": "REQ-0002", "relation": "refines", "to": "REQ-0001"});
    assert_eq!(document["edges"], serde_json::json!([edge.clone()]));
    assert_eq!(
        document["derived_incoming"],
        serde_json::json!([edge]),
        "the incoming edge is derived from the one stored outgoing edge"
    );
    assert_keys(&document["edges"][0], &["from", "relation", "to"]);
    assert_keys(&document["findings"], &["dangling", "undecided_edges"]);
    assert_eq!(document["findings"]["dangling"], serde_json::json!([]));
    assert_eq!(
        document["findings"]["undecided_edges"],
        serde_json::json!([])
    );
}

/// An edge that leaves the corpus is UNVERIFIABLE in both renderings, and the JSON names it
/// with **the same string** the text path prints — one source of truth for the classification,
/// not a second derivation.
#[test]
fn a_dangling_edge_is_named_in_json_exactly_as_the_text_path_names_it() {
    let corpus = Corpus::with_second_claim(
        "trace-dangling",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[("derives-from", "REQ-9999")],
    );
    let text = run_at(corpus.path(), &["trace"]);
    assert_eq!(text.code, 3, "an edge to no claim leaves the matrix open");
    let json = run_at(corpus.path(), &["trace", "--format", "json"]);
    assert_eq!(
        json.code, text.code,
        "the exit class is the verdict's, not the rendering's"
    );
    let document = parse(&json.stdout);
    let dangling = document["findings"]["dangling"]
        .as_array()
        .expect("an array");
    assert_eq!(dangling.len(), 1, "{document}");
    let named = dangling[0].as_str().expect("a finding is a string");
    assert!(
        named.contains("REQ-9999") && named.contains("not in the corpus"),
        "the finding names the missing claim and why: {named}"
    );
    assert!(
        text.stdout.contains(&format!("UNVERIFIABLE: {named}")),
        "the JSON finding is the text path's own line, word for word:\ntext:\n{}\njson finding: {named}",
        text.stdout
    );
}

/// An edge into a claim the variant leaves undecided is a finding needing a decision, in both
/// renderings — and the JSON `undecided_edges` is `TraceFindings` verbatim.
#[test]
fn an_undecided_edge_is_reported_in_json() {
    // The *target* must be undecided: an edge whose source is unknown is still an edge into a
    // claim the gate has to examine, and only the target's state is what `trace` classifies.
    let corpus = Corpus::new("trace-undecided");
    corpus.add_claim(
        "REQ-0002",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[("refines", "REQ-0003")],
    );
    corpus.add_claim(
        "REQ-0003",
        "The API shall answer a readiness check within 200 ms.",
        &["deployment_mode == air_gapped"],
        &[],
    );
    let json = run_at(corpus.path(), &["trace", "--format", "json"]);
    assert_eq!(json.code, 3, "{}", json.stderr);
    let document = parse(&json.stdout);
    let undecided = document["findings"]["undecided_edges"]
        .as_array()
        .expect("an array");
    assert_eq!(undecided.len(), 1, "{document}");
    assert!(
        undecided[0]
            .as_str()
            .expect("a finding is a string")
            .contains("REQ-0003")
    );
}

// ---------------------------------------------------------------------------
// gate
// ---------------------------------------------------------------------------

/// The gate document's shape: five policies, each with its own outcome, the folded verdict, and
/// the undecided claims with what they are missing.
#[test]
fn gate_json_has_the_pinned_shape() {
    let corpus = Corpus::with_second_claim(
        "gate-shape",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let out = corpus.run_gate(&["--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let document = parse(&out.stdout);
    assert_keys(
        &document,
        &[
            "generator",
            "policies",
            "process_evidenced",
            "rules_v",
            "schema",
            "selection",
            "undecided",
            "verdict",
        ],
    );
    assert_eq!(document["schema"], "req.gate/2");
    assert_eq!(document["generator"], "gate");
    assert_eq!(document["rules_v"], req::canon::RULES_V);
    assert_eq!(document["verdict"], "pass");
    assert_eq!(document["undecided"], serde_json::json!([]));
    // Nothing here is a process declaration, and an empty `tests:` list is never inferred as one.
    assert_eq!(document["process_evidenced"], serde_json::json!([]));

    let policies = document["policies"].as_array().expect("an array");
    let names: Vec<&str> = policies
        .iter()
        .map(|policy| policy["policy"].as_str().expect("a policy name"))
        .collect();
    assert_eq!(
        names,
        [
            "pins_hold",
            "relations_resolve",
            "dependencies_decided",
            "rationale_present",
            "tests_linked",
            "selectors_confirmed",
        ],
        "the policies keep the order the text report and the docs name"
    );
    for policy in policies {
        // `checked` (the number of pins judged) is on `pins_hold` alone.
        if policy["policy"] == "pins_hold" {
            assert_keys(policy, &["checked", "details", "outcome", "policy"]);
            // The starter claim pins three keys (operator, value, unit); nothing else here pins.
            assert_eq!(policy["checked"], 3, "{policy}");
            continue;
        }
        // `counts` (the claims by verdict, issue #20) is on `tests_linked` alone.
        if policy["policy"] == "tests_linked" {
            assert_keys(policy, &["counts", "details", "outcome", "policy"]);
        } else {
            assert_keys(policy, &["details", "outcome", "policy"]);
        }
        assert_eq!(policy["outcome"], "pass");
        assert_eq!(policy["details"], serde_json::json!([]));
    }
}

/// The verdict is the same fold the exit code applies, for all three classes. A test per class,
/// and the text and JSON runs must agree on the code every time.
#[test]
fn gate_json_verdict_and_exit_code_agree_for_all_three_classes() {
    // PASS: no selector, provenance present, every pin holds.
    let pass = Corpus::with_second_claim(
        "gate-pass",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    // UNVERIFIABLE: a selector the bare invocation cannot decide.
    let undecided = Corpus::with_second_claim(
        "gate-undecided",
        "The API shall respond to a readiness check within 200 ms.",
        &["deployment_mode == air_gapped"],
        &[],
    );
    // FAIL: a definite defect — a claim whose statement no longer holds the bound it pins
    // (`value: "0.2"` pinned, `500 ms` = 0.5 s derived): `pins_hold` fails.
    let defect = Corpus::with_second_claim(
        "gate-defect",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    defect.rewrite_statement(
        "REQ-0001",
        "The API shall answer a readiness check within 500 ms.",
    );

    for (corpus, expected_code, expected_verdict) in [
        (&pass, 0, "pass"),
        (&undecided, 3, "unverifiable"),
        (&defect, 1, "fail"),
    ] {
        let text = corpus.run_gate(&[]);
        let json = corpus.run_gate(&["--format", "json"]);
        assert_eq!(
            text.code, expected_code,
            "the text run's class is the fixture's: {}",
            text.stdout
        );
        assert_eq!(
            json.code, text.code,
            "JSON and text return the same code on the same corpus"
        );
        let document = parse(&json.stdout);
        assert_eq!(
            document["verdict"],
            expected_verdict,
            "the verdict is the fold the exit code applies:\n{}",
            serde_json::to_string_pretty(&document).expect("re-serializes")
        );
    }

    // The FAIL is `pins_hold`'s, for the reason the fixture names, not some other policy's.
    let defect_document = parse(&defect.run_gate(&["--format", "json"]).stdout);
    let pins_hold = defect_document["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|p| p["policy"] == "pins_hold")
        .expect("pins_hold is in the document");
    assert_eq!(pins_hold["outcome"], "fail", "{pins_hold}");
    assert_eq!(
        pins_hold["details"],
        serde_json::json!(["REQ-0001: pin value 0.2 contradicts the parse (0.5)"]),
        "{pins_hold}"
    );

    // The undecided claim is named with what the invocation is missing, never dropped.
    let json = undecided.run_gate(&["--format", "json"]);
    let document = parse(&json.stdout);
    assert_eq!(
        document["undecided"],
        serde_json::json!([{"claim_id": "REQ-0002", "missing": ["deployment_mode"]}])
    );
    assert_keys(&document["undecided"][0], &["claim_id", "missing"]);

    // A definite defect outranks an open question: with both present the verdict is fail.
    let both = Corpus::with_second_claim(
        "gate-both",
        "The API shall respond to a readiness check within 200 ms.",
        &["deployment_mode == air_gapped"],
        &[],
    );
    both.rewrite_statement(
        "REQ-0001",
        "The API shall answer a readiness check within 500 ms.",
    );
    let json = both.run_gate(&["--format", "json"]);
    assert_eq!(
        json.code, 1,
        "Fail outranks Unverifiable under both renderings"
    );
    assert_eq!(parse(&json.stdout)["verdict"], "fail");
}

// ---------------------------------------------------------------------------
// audit
// ---------------------------------------------------------------------------

/// With no Baseline the packet still prints, and the document says nothing is ratified: the
/// key is present with `false`, coverage is `null` (nothing was compared), and the finding
/// that names the reason is carried.
#[test]
fn audit_json_reports_an_unsigned_corpus_as_unratified() {
    let corpus = Corpus::with_second_claim(
        "audit-unsigned",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let text = run_at(corpus.path(), &["audit"]);
    let json = run_at(corpus.path(), &["audit", "--format", "json"]);
    assert_eq!(json.code, text.code, "the exit class is unchanged");
    assert_eq!(json.code, 3, "an unsigned audit is unverifiable");

    let document = parse(&json.stdout);
    assert_keys(
        &document,
        &[
            "coverage",
            "findings",
            "generator",
            "ratified",
            "rules_v",
            "schema",
            "selection",
        ],
    );
    assert_eq!(document["schema"], "req.audit/1");
    assert_eq!(document["generator"], "audit");
    assert_eq!(document["rules_v"], req::canon::RULES_V);
    assert_eq!(document["ratified"], false);
    assert_eq!(
        document["coverage"],
        Value::Null,
        "no Baseline means nothing was compared; `null` is not an empty comparison"
    );
    let findings = document["findings"].as_array().expect("an array");
    assert_eq!(findings.len(), 1, "{document}");
    let finding = findings[0].as_str().expect("a finding is a string");
    assert!(
        finding.starts_with("UNVERIFIABLE: ") && finding.contains("no Baseline supplied"),
        "the finding carries its class and the packet's own reason: {finding}"
    );
}

/// A signed manifest that still covers the corpus: `ratified: true`, every claim covered, no
/// findings. The manifest is built through the library's own derivation (never hand-written).
#[test]
fn audit_json_reports_coverage_for_a_matching_baseline() {
    let corpus = Corpus::with_second_claim(
        "audit-covered",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let manifest_path = corpus.path().join("baseline.json");
    write_manifest(&corpus, &manifest_path);

    let json = run_at(
        corpus.path(),
        &[
            "audit",
            "--baseline",
            manifest_path.to_str().expect("fixture path is UTF-8"),
            "--format",
            "json",
        ],
    );
    assert_eq!(
        json.code, 0,
        "every applying claim is covered:\n{}",
        json.stdout
    );
    let document = parse(&json.stdout);
    assert_eq!(document["ratified"], true);
    assert_keys(
        &document["coverage"],
        &["drifted", "missing", "not_judged", "signed", "uncovered"],
    );
    assert_eq!(
        document["coverage"]["signed"],
        serde_json::json!(["REQ-0001", "REQ-0002"])
    );
    assert_eq!(document["coverage"]["drifted"], serde_json::json!([]));
    assert_eq!(document["coverage"]["uncovered"], serde_json::json!([]));
    assert_eq!(document["coverage"]["missing"], serde_json::json!([]));
    assert_eq!(document["coverage"]["not_judged"], Value::Null);
    assert_eq!(document["findings"], serde_json::json!([]));
}

/// A manifest written under another canonical rules version is **NOT JUDGED**: coverage is
/// reported as such, with no claim covered or drifted — an empty comparison, never a clean one.
#[test]
fn audit_json_reports_a_foreign_rules_version_as_not_judged() {
    let corpus = Corpus::with_second_claim(
        "audit-not-judged",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let manifest_path = corpus.path().join("foreign.json");
    let rules_v = req::canon::RULES_V;
    let foreign = format!(
        "{{\"version\":3,\"signed_at\":\"2026-10-21T00:00:00Z\",\"authority\":\
         {{\"head\":\"{}\",\"entries\":1}},\"rules_v\":{},\"selection\":{{}},\"claims\":[]}}",
        "a".repeat(64),
        rules_v.saturating_sub(1),
    );
    fs::write(&manifest_path, foreign).expect("writes the foreign manifest");

    let text = run_at(
        corpus.path(),
        &[
            "audit",
            "--baseline",
            manifest_path.to_str().expect("fixture path is UTF-8"),
        ],
    );
    assert_eq!(text.code, 3, "NOT JUDGED is an open question, not a pass");
    assert!(
        text.stdout.contains("NOT JUDGED"),
        "the text packet says coverage was not judged:\n{}",
        text.stdout
    );

    let json = run_at(
        corpus.path(),
        &[
            "audit",
            "--baseline",
            manifest_path.to_str().expect("fixture path is UTF-8"),
            "--format",
            "json",
        ],
    );
    assert_eq!(json.code, text.code, "the exit class is unchanged");
    let document = parse(&json.stdout);
    assert_eq!(document["ratified"], true);
    let reason = document["coverage"]["not_judged"]
        .as_str()
        .expect("`not_judged` is the reason string when coverage could not be judged");
    assert!(
        reason.contains("rules"),
        "the reason names the rules-version mismatch: {reason}"
    );
    for list in ["signed", "drifted", "uncovered", "missing"] {
        assert_eq!(
            document["coverage"][list],
            serde_json::json!([]),
            "`{list}` is empty because nothing was compared, not because everything matched"
        );
    }
}

/// Build a v3 manifest through the library's derivation and write its canonical bytes. Nothing
/// here signs it: the audit packet reports coverage, and authority belongs to `baseline --verify`.
fn write_manifest(corpus: &Corpus, path: &Path) {
    let context = req::select::Context::new();
    let rows = req::view::load_rows(&context, corpus.path()).expect("the fixture corpus loads");
    let mut manifest = req::baseline::manifest(&rows, req::canon::RULES_V, &context);
    req::baseline::attest(
        &mut manifest,
        "2026-10-21T00:00:00Z",
        &req::decision::Head {
            hash: "a".repeat(64),
            entries: 1,
        },
    )
    .expect("the fixture signing time is a valid attestation");
    fs::write(path, req::baseline::canonical_bytes(&manifest)).expect("writes the manifest");
}

// ---------------------------------------------------------------------------
// Determinism and the help pages
// ---------------------------------------------------------------------------

/// Two runs on the same corpus are byte-identical for every command: the contract is
/// determinism (BTreeMap, sorted arrays, no floats), not a probability.
#[test]
fn every_json_document_is_byte_identical_across_two_runs() {
    let corpus = Corpus::with_second_claim(
        "determinism",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[("refines", "REQ-0001")],
    );
    for command in ["render", "trace", "gate", "audit"] {
        let first = run_at(corpus.path(), &[command, "--format", "json"]);
        let second = run_at(corpus.path(), &[command, "--format", "json"]);
        assert_ne!(
            first.stdout, "",
            "`{command} --format json` emits a document"
        );
        assert_eq!(
            first.stdout, second.stdout,
            "`{command} --format json` must be byte-identical across runs"
        );
        assert_eq!(first.code, second.code, "`{command}`: code stable too");
    }
}

/// The flag works wherever the text command works: the root is resolved upstream of the
/// renderer (ADR 0012), so a subdirectory of the corpus sees the same document.
#[test]
fn json_works_from_a_subdirectory_where_text_works() {
    let corpus = Corpus::with_second_claim(
        "subdirectory",
        "The API shall respond to a readiness check within 200 ms.",
        &[],
        &[],
    );
    let nested = corpus.path().join("nested/deeper");
    fs::create_dir_all(&nested).expect("creates the nested fixture");
    let at_root = run_at(corpus.path(), &["render", "--format", "json"]);
    let nested_out = run_at(&nested, &["render", "--format", "json"]);
    assert_eq!(nested_out.code, at_root.code, "{}", nested_out.stderr);
    assert_eq!(
        nested_out.stdout, at_root.stdout,
        "the same corpus is the same document from anywhere below it"
    );
}

/// Each page carries the JSON contract: the flag's two values and the schema string a consumer
/// should look for. The pages are the checked docs (`tests/cli_version.rs`), so this is where a
/// consumer learns the shape.
#[test]
fn each_help_page_carries_the_format_contract() {
    // A directory with no `claims/` ancestor, asserted rather than assumed: a help page must be
    // readable before the corpus exists.
    let dir = unique_dir("help");
    if let Some(found) = dir.ancestors().find(|dir| req::root::is_corpus_root(dir)) {
        panic!(
            "the fixture assumes no ancestor holds claims/, but {} does",
            found.display()
        );
    }
    for (command, schema) in [
        ("render", "req.render/1"),
        ("trace", "req.trace/1"),
        ("audit", "req.audit/1"),
        ("gate", "req.gate/2"),
    ] {
        let out = run_at(&dir, &[command, "--help"]);
        assert_eq!(out.code, 0, "`{command} --help`:\n{}", out.stderr);
        assert!(
            out.stdout.contains("--format") && out.stdout.contains("json"),
            "`{command}`'s page names the flag and its values:\n{}",
            out.stdout
        );
        assert!(
            out.stdout.contains(schema),
            "`{command}`'s page names its schema string `{schema}`:\n{}",
            out.stdout
        );
        assert!(
            out.stdout.contains("text"),
            "`{command}`'s page names the default rendering:\n{}",
            out.stdout
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
