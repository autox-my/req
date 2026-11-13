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

    /// A `req.claim/3` claim filed under `area` (ADR 0051), with no relation.
    fn claim_in(&self, id: &str, area: &str) {
        let text = format!(
            "schema: req.claim/3\n\
             claim_id: {id}\n\
             statement: \"{PLAIN}\"\n\
             applicability: []\n\
             relations: []\n\
             evidence: process\n\
             area: {area}\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-13\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(self.0.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }

    /// `.req/areas`, exactly as written.
    fn areas(&self, text: &str) {
        fs::create_dir_all(self.0.join(".req")).expect("makes .req");
        fs::write(self.0.join(".req/areas"), text).expect("writes .req/areas");
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
fn a_node_carries_its_seven_fields_in_order() {
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
            "area",
            "claim_id",
            "modality",
            "statement",
            "superseded_by"
        ]
    );
    assert_eq!(
        first["area"],
        serde_json::Value::Null,
        "a claim with no area says null"
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
        "area",
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

/// `trace --out` of a three-claim corpus, exactly as the **base build** (3144571, before `nodes` existed) wrote it:
/// the golden is a literal, captured once from that build and never from this one. (A Tier-1 rules bump moves the
/// header's `rules_v` and identity, and this literal with it.)
const BASE_TRACE_OUT: &str = "\
<!-- req projection: generator=req trace | rules_v=9 | selection=none | claims=3 | identity=sha256:e6f633104bc65598f15e318fbc27587949916e2652f00d93691a0c87d3861bf7 -->
# Traceability: (no context: nothing is decided by the caller)
edges are stored once, on the outgoing side (REQ-0020); the incoming
view below is derived by traversal, never stored

## Edges
- REQ-0002 -refines-> REQ-0001

## Incoming (derived)
- REQ-0001 <- refines: REQ-0002

## Findings
(none)
";

#[test]
fn trace_out_prints_the_bytes_the_base_build_printed() {
    let s = Scratch::new("text-unchanged");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("refines", "REQ-0001")]);
    s.claim("REQ-0003", PLAIN, &[], &[]);
    let out = s.path().join("trace.md");
    let run = s.req(&["trace", "--out", out.to_str().expect("utf-8 path")]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(
        fs::read_to_string(&out).expect("the projection"),
        BASE_TRACE_OUT,
        "the text projection is byte-identical to the base build's: adding `nodes` edits no text"
    );
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

// ---------------------------------------------------------------------------------------------------------------
// `areas`: the registry travels in the document (ADR 0051 decision 2, unit A-2)
// ---------------------------------------------------------------------------------------------------------------

/// The registry, hand-written: a label is free text, so one is non-ASCII, one holds a `#`, and the line order is not
/// the path's sort order (`interfaces` is declared before `diagnostics`).
const REGISTRY: &str = "\
# reading order
interfaces Interfaces
diagnostics Démarrage du système
diagnostics/startup Start-up and C# support
";

#[test]
fn areas_lists_the_registry_in_line_order_with_each_label() {
    let s = Scratch::new("areas-order");
    three(&s);
    s.areas(REGISTRY);
    let document = s.document(&[]);
    assert_eq!(
        document["areas"],
        serde_json::json!([
            {"path": "interfaces", "label": "Interfaces"},
            {"path": "diagnostics", "label": "Démarrage du système"},
            {"path": "diagnostics/startup", "label": "Start-up and C# support"},
        ]),
        "the registry's lines, in the registry's order, never sorted: {document}"
    );
}

#[test]
fn areas_is_an_empty_array_without_a_registry() {
    let s = Scratch::new("areas-none");
    three(&s);
    let document = s.document(&[]);
    assert_eq!(
        document["areas"],
        serde_json::json!([]),
        "no `.req/areas` is `[]`, not an absent key: {document}"
    );
}

#[test]
fn areas_carries_declared_areas_that_no_claim_names() {
    // The document is the registry; which areas have claims is the consumer's to judge from `nodes`.
    let s = Scratch::new("areas-unused");
    s.claim_in("REQ-0001", "interfaces");
    s.areas(REGISTRY);
    let document = s.document(&[]);
    assert_eq!(
        document["areas"].as_array().map(Vec::len),
        Some(3),
        "{document}"
    );
}

#[test]
fn areas_sits_between_nodes_and_edges_and_an_entry_keeps_path_then_label() {
    let s = Scratch::new("areas-place");
    three(&s);
    s.areas(REGISTRY);
    let out = s.trace(&[]);
    let at = |from: usize, key: &str| {
        from + out.stdout[from..]
            .find(&format!("\"{key}\""))
            .unwrap_or_else(|| panic!("`{key}` is in the document after {from}:\n{}", out.stdout))
    };
    // The last node's last field, then `areas`, then its entries' fields in order, then `edges`.
    let nodes = at(0, "nodes");
    let last_area_field = out.stdout.rfind("\"area\"").expect("a node carries `area`");
    let areas = at(nodes, "areas");
    let edges = at(areas, "edges");
    assert!(
        last_area_field < areas && areas < edges,
        "`areas` is after the nodes and before `edges`:\n{}",
        out.stdout
    );
    let path = at(areas, "path");
    let label = at(path, "label");
    assert!(
        areas < path && path < label && label < edges,
        "an entry is path, then label"
    );
}

#[test]
fn a_malformed_registry_is_exit_2_naming_every_bad_line_and_prints_nothing() {
    let s = Scratch::new("areas-malformed");
    three(&s);
    s.areas("diagnostics Fine\nBad_Path A label\ninterfaces\ndiagnostics Again\n");
    let out = s.trace(&[]);
    assert_eq!(
        out.code, 2,
        "a malformed registry is a usage error:\n{}",
        out.stderr
    );
    assert_eq!(out.stdout, "", "nothing is printed to stdout");
    for line in [".req/areas:2:", ".req/areas:3:", ".req/areas:4:"] {
        assert!(
            out.stderr.contains(line),
            "stderr names `{line}` as `req check` does:\n{}",
            out.stderr
        );
    }
    assert!(
        !out.stderr.contains(".req/areas:1:"),
        "a good line is not named:\n{}",
        out.stderr
    );
}

#[test]
fn an_unreadable_registry_is_exit_2_with_nothing_on_stdout() {
    let s = Scratch::new("areas-unreadable");
    three(&s);
    // A directory where the file belongs: it exists and cannot be used as a list.
    fs::create_dir_all(s.path().join(".req/areas")).expect("makes a directory named areas");
    let out = s.trace(&[]);
    assert_eq!(out.code, 2, "{}{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains(".req/areas"), "{}", out.stderr);
}

#[test]
fn text_trace_does_not_read_the_registry() {
    let s = Scratch::new("areas-text");
    s.claim("REQ-0001", LONG, &[], &[]);
    s.claim("REQ-0002", SHORT, &[], &[("refines", "REQ-0001")]);
    s.claim("REQ-0003", PLAIN, &[], &[]);
    let before = s.req(&["trace"]);
    s.areas(REGISTRY);
    let with_registry = s.req(&["trace"]);
    assert_eq!(
        (before.code, &before.stdout),
        (with_registry.code, &with_registry.stdout),
        "text `trace` is the same with a registry"
    );
    // Not even a malformed one: text `trace` does not parse it, and `docs/trace.md` passes `--check` unchanged.
    s.areas("Bad_Path\n");
    let malformed = s.req(&["trace"]);
    assert_eq!(
        (malformed.code, &malformed.stdout),
        (before.code, &before.stdout),
        "text `trace` never reads `.req/areas`:\n{}",
        malformed.stderr
    );
}

#[test]
fn trace_help_names_areas() {
    let out = Command::new(req_bin())
        .args(["trace", "--help"])
        .output()
        .expect("runs req trace --help");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        text.contains("`areas`"),
        "the help names the `areas` field:\n{text}"
    );
    assert!(
        text.contains(".req/areas"),
        "and where it comes from:\n{text}"
    );
}
