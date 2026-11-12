//! `req.trace/1` carries its nodes (ADR 0046 decision 2, unit V-1).
//!
//! The seam is the compiled `req`: a consumer parses stdout. Every oracle is the ADR's decision or one of its
//! *Falsification* items, written before the field existed and never read back from a run. A claim with no
//! relation used to be invisible in the document; a withdrawn claim carried no sign of its successor.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

/// At most 100 ms, and at least 200 ms: no value satisfies both, so a successor and its predecessor never agree.
const SHORT: &str = "The API shall respond in no more than 100 ms.";
const LONG: &str = "The API shall respond in at least 200 ms.";
const PLAIN: &str = "The service shall log every request.";

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-trace-nodes-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        let init = Command::new(req_bin())
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("runs req init");
        assert_eq!(init.status.code(), Some(0), "req init must succeed");
        fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn claim(&self, id: &str, statement: &str, applicability: &[&str], relations: &[(&str, &str)]) {
        let applicability = if applicability.is_empty() {
            "applicability: []".to_string()
        } else {
            let lines: Vec<String> = applicability.iter().map(|p| format!("  - {p}")).collect();
            format!("applicability:\n{}", lines.join("\n"))
        };
        let relations = if relations.is_empty() {
            "relations: []".to_string()
        } else {
            let lines: Vec<String> = relations
                .iter()
                .map(|(kind, to)| format!("  - type: {kind}\n    to: {to}"))
                .collect();
            format!("relations:\n{}", lines.join("\n"))
        };
        let text = format!(
            "schema: req.claim/2\n\
             claim_id: {id}\n\
             statement: \"{statement}\"\n\
             {applicability}\n\
             {relations}\n\
             evidence: process\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-12\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    fn req(&self, args: &[&str]) -> Out {
        let out = Command::new(req_bin())
            .args(args)
            .current_dir(&self.0)
            .env_remove("REQ_ROOT")
            .output()
            .expect("runs req");
        Out {
            code: out.status.code().expect("req exits, never a signal"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn trace(&self, selection: &[&str]) -> Out {
        let mut args = vec!["trace", "--format", "json"];
        args.extend_from_slice(selection);
        self.req(&args)
    }

    fn document(&self, selection: &[&str]) -> Value {
        let out = self.trace(selection);
        serde_json::from_str(&out.stdout)
            .unwrap_or_else(|e| panic!("trace JSON: {e}\n{}{}", out.stdout, out.stderr))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn nodes(document: &Value) -> &Vec<Value> {
    document["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("the document carries `nodes`: {document}"))
}

fn node<'a>(document: &'a Value, id: &str) -> &'a Value {
    nodes(document)
        .iter()
        .find(|n| n["claim_id"] == id)
        .unwrap_or_else(|| panic!("{id} is a node of the document: {document}"))
}

/// Three claims written out of order, one of them with no relation at all.
fn three(s: &Scratch) {
    s.claim("REQ-0003", PLAIN, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("refines", "REQ-0001")]);
    s.claim("REQ-0001", LONG, &[], &[]);
}

#[test]
fn nodes_lists_every_claim_including_one_with_no_relation_sorted_by_claim_id() {
    let s = Scratch::new("every-claim");
    three(&s);
    s.claim("REQ-0004", "The service shall keep an audit log.", &[], &[]);
    let document = s.document(&[]);
    let ids: Vec<&str> = nodes(&document)
        .iter()
        .map(|n| n["claim_id"].as_str().expect("a claim_id string"))
        .collect();
    assert_eq!(
        ids,
        ["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"],
        "one node per claim, REQ-0003 and REQ-0004 having no relation, sorted by claim_id"
    );
}

#[test]
fn a_node_carries_the_six_fields_in_order() {
    let s = Scratch::new("six-fields");
    three(&s);
    let document = s.document(&[]);
    let first = &nodes(&document)[0];
    let keys: Vec<&str> = first
        .as_object()
        .expect("a node is an object")
        .keys()
        .map(String::as_str)
        .collect();
    // The workspace's JSON writer sorts object keys (BTreeMap), so the spelling below is the sorted
    // set; the *declared* order is checked on the bytes, which are what a consumer reads.
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "address",
            "applies",
            "claim_id",
            "modality",
            "statement",
            "superseded_by"
        ]
    );
    assert_eq!(first["claim_id"], "REQ-0001");
    assert_eq!(first["statement"], LONG);
    assert_eq!(first["modality"], "SHALL");
    assert_eq!(first["applies"], "applies");
    assert_eq!(first["superseded_by"], serde_json::json!([]));
    let address = first["address"].as_str().expect("an address string");
    assert!(
        address.starts_with("sha256:") && address.len() == "sha256:".len() + 64,
        "the address is spelled as the other documents spell it: {address}"
    );
}

#[test]
fn a_withdrawn_claim_is_does_not_apply_and_names_its_successor() {
    let s = Scratch::new("withdrawn");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("supersedes", "REQ-0001")]);
    let document = s.document(&[]);
    let old = node(&document, "REQ-0001");
    assert_eq!(old["applies"], "does_not_apply");
    assert_eq!(old["superseded_by"], serde_json::json!(["REQ-0002"]));
    let new = node(&document, "REQ-0002");
    assert_eq!(new["applies"], "applies");
    assert_eq!(new["superseded_by"], serde_json::json!([]));
}

#[test]
fn applies_is_the_tri_state_in_the_selection() {
    let s = Scratch::new("tri-state");
    s.claim("REQ-0001", PLAIN, &[], &[]);
    s.claim(
        "REQ-0002",
        "The service shall encrypt data at rest.",
        &["deployment == cloud"],
        &[],
    );
    let in_cloud = s.document(&["deployment=cloud"]);
    assert_eq!(node(&in_cloud, "REQ-0002")["applies"], "applies");
    let elsewhere = s.document(&["deployment=onprem"]);
    assert_eq!(node(&elsewhere, "REQ-0002")["applies"], "does_not_apply");
    assert_eq!(
        nodes(&elsewhere).len(),
        2,
        "an excluded claim is still a node"
    );
    let unknown = s.document(&[]);
    assert_eq!(node(&unknown, "REQ-0002")["applies"], "unknown");
}

#[test]
fn nodes_sit_between_selection_and_edges_and_a_node_keeps_its_field_order_in_the_bytes() {
    let s = Scratch::new("key-place");
    three(&s);
    let out = s.trace(&[]);
    let at = |from: usize, key: &str| {
        from + out.stdout[from..]
            .find(&format!("\"{key}\""))
            .unwrap_or_else(|| panic!("`{key}` is in the document after {from}:\n{}", out.stdout))
    };
    // The writer keeps declaration order, so the document's order is a byte-level contract.
    let selection = at(0, "selection");
    let nodes = at(selection, "nodes");
    let edges = at(nodes, "edges");
    assert!(selection < nodes && nodes < edges);
    let mut cursor = nodes;
    for key in [
        "claim_id",
        "statement",
        "modality",
        "address",
        "applies",
        "superseded_by",
    ] {
        let next = at(cursor, key);
        assert!(next < edges, "`{key}` is a node field, before `edges`");
        cursor = next;
    }
}

#[test]
fn two_runs_give_identical_bytes() {
    let s = Scratch::new("twice");
    three(&s);
    let a = s.trace(&[]);
    let b = s.trace(&[]);
    assert_eq!(a.code, b.code);
    assert_eq!(
        a.stdout, b.stdout,
        "the document is deterministic (decision 4)"
    );
}

#[test]
fn trace_out_is_byte_identical_to_the_base_build() {
    // The text matrix does not change: pinned here by the bytes of a hand-checked corpus, whose
    // matrix was produced by the base build (3144571) before `nodes` existed.
    let s = Scratch::new("text-unchanged");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("refines", "REQ-0001")]);
    let out = s.path().join("trace.md");
    let run = s.req(&["trace", "--out", out.to_str().expect("utf-8 path")]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    let text = fs::read_to_string(&out).expect("the projection");
    assert!(
        !text.contains("\"nodes\""),
        "the text projection carries no document key:\n{text}"
    );
    let check = s.req(&["trace", "--check", out.to_str().expect("utf-8 path")]);
    assert_eq!(check.code, 0, "{}{}", check.stdout, check.stderr);
    let plain = s.req(&["trace"]);
    let plain_text: Vec<&str> = plain.stdout.lines().collect();
    for line in text.lines().filter(|l| !l.starts_with("<!--")) {
        assert!(
            plain_text.contains(&line),
            "`--out` is the plain text matrix: `{line}` is not in\n{}",
            plain.stdout
        );
    }
}

#[test]
fn trace_help_names_nodes_and_the_graph_page() {
    let out = Command::new(req_bin())
        .args(["trace", "--help"])
        .output()
        .expect("runs req trace --help");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(text.contains("nodes"), "the help names `nodes`:\n{text}");
    assert!(
        text.contains("examples/graph/README.md"),
        "the help points at the graph page:\n{text}"
    );
}
