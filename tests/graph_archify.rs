//! `tools/graph/trace_to_archify.py` — one `req.trace/1` document in, a directory of archify diagram files out
//! (ADR 0046 unit V-1, extended by ADR 0050 unit V-2).
//!
//! The seam is the script itself, run the way an adopter runs it (`python3 -I tools/graph/trace_to_archify.py
//! <trace.json> <out-dir>`), over **hand-written** `req.trace/1` fixtures: nodes, edges and findings are authored
//! here from the ADRs' decisions, never captured from a run of `req`. The oracles are the ADRs' *Falsification*
//! items: deterministic bytes, nothing lost and nothing invented, every claim on exactly one group page, an edge
//! with an end on a page drawn there (as an edge or a stub), no page past `PAGE_LIMIT`, no network, and a schema it
//! was not written for refused.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn script() -> PathBuf {
    root().join("tools/graph/trace_to_archify.py")
}

/// `python3` as an absolute path, so a missing interpreter is a named environment problem and not a failing check.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/graph/trace_to_archify.py - install it first");
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-graph-archify-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Run {
    code: i32,
    stderr: String,
}

/// The page limit a test sets, through the converter's test-only environment override.
const LIMIT_VAR: &str = "REQ_GRAPH_PAGE_LIMIT";

fn run_with(input: &Path, out_dir: &Path, limit: Option<&str>) -> Run {
    let mut command = Command::new(python3());
    command
        .arg("-I")
        .arg(script())
        .arg(input)
        .arg(out_dir)
        .current_dir(root())
        .env_remove(LIMIT_VAR);
    if let Some(limit) = limit {
        command.env(LIMIT_VAR, limit);
    }
    let out = command.output().expect("runs the converter");
    Run {
        code: out
            .status
            .code()
            .expect("the converter exits, never a signal"),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn run(input: &Path, out_dir: &Path) -> Run {
    run_with(input, out_dir, None)
}

fn node(id: &str, statement: &str, applies: &str, superseded_by: &[&str]) -> Value {
    json!({
        "claim_id": id,
        "statement": statement,
        "modality": "SHALL",
        "address": format!("sha256:{}", "0".repeat(64)),
        "applies": applies,
        "superseded_by": superseded_by,
    })
}

fn edge(from: &str, relation: &str, to: &str) -> Value {
    json!({"from": from, "relation": relation, "to": to})
}

/// Five claims and one named-but-missing target, written out of order:
///
/// * REQ-0001 is the root; REQ-0002 refines it; REQ-0004 derives from REQ-0002 (three layers);
/// * REQ-0003 is withdrawn by REQ-0004, which supersedes it;
/// * REQ-0005 is undecided and has no relation of its own, but REQ-0004 refines it (an undecided edge);
/// * REQ-0002 derives from REQ-0009, a claim the corpus does not hold (a dangling edge).
fn fixture() -> Value {
    json!({
        "schema": "req.trace/1",
        "generator": "trace",
        "rules_v": 9,
        "selection": {"deployment": "cloud"},
        "nodes": [
            node("REQ-0001", "The service shall log every request.", "applies", &[]),
            node("REQ-0002", "The service shall log the caller of every request.", "applies", &[]),
            node("REQ-0003", "The API shall respond in at least 200 ms.", "does_not_apply", &["REQ-0004"]),
            node("REQ-0004", "The API shall respond in no more than 100 ms.", "applies", &[]),
            node("REQ-0005", "The service shall encrypt data at rest.", "unknown", &[]),
        ],
        "edges": [
            edge("REQ-0002", "derives-from", "REQ-0009"),
            edge("REQ-0002", "refines", "REQ-0001"),
            edge("REQ-0004", "derives-from", "REQ-0002"),
            edge("REQ-0004", "refines", "REQ-0005"),
            edge("REQ-0004", "supersedes", "REQ-0003"),
        ],
        "derived_incoming": [],
        "findings": {
            "dangling": ["derives-from -> REQ-0009 (referenced by REQ-0002, not in the corpus)"],
            "undecided_edges": ["REQ-0004 -refines-> REQ-0005 (target undecided)"],
        },
    })
}

fn write_fixture(scratch: &Scratch, name: &str, document: &Value) -> PathBuf {
    let path = scratch.path(name);
    fs::write(
        &path,
        serde_json::to_string_pretty(document).expect("serializes"),
    )
    .expect("writes the fixture");
    path
}

/// Every `*.json` page of an output directory, by page name (the file stem), plus the file names found.
fn read_pages(dir: &Path) -> BTreeMap<String, Value> {
    let mut pages = BTreeMap::new();
    for entry in fs::read_dir(dir).expect("the output directory") {
        let path = entry.expect("an entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a file name")
            .to_string();
        let stem = name
            .strip_suffix(".json")
            .unwrap_or_else(|| panic!("only .json pages are written, found {name}"));
        let page = serde_json::from_str(&fs::read_to_string(&path).expect("the page"))
            .unwrap_or_else(|e| panic!("{name} is JSON: {e}"));
        pages.insert(stem.to_string(), page);
    }
    pages
}

/// Convert and read every page back, with a page limit; the run must succeed.
fn converted_pages(
    scratch: &Scratch,
    document: &Value,
    limit: Option<&str>,
) -> BTreeMap<String, Value> {
    let input = write_fixture(scratch, "trace.json", document);
    let output = scratch.path("graph");
    let ran = run_with(&input, &output, limit);
    assert_eq!(
        ran.code, 0,
        "the converter reads a req.trace/1 document:\n{}",
        ran.stderr
    );
    read_pages(&output)
}

/// Convert a document that fits one page and return that page (`all.json`, and nothing else).
fn converted(scratch: &Scratch, document: &Value) -> Value {
    let mut pages = converted_pages(scratch, document, None);
    assert_eq!(
        pages.keys().collect::<Vec<_>>(),
        vec!["all"],
        "a document within the limit is one page, and no index"
    );
    pages.remove("all").expect("all.json")
}

fn components(graph: &Value) -> &Vec<Value> {
    graph["components"].as_array().expect("components")
}

fn component<'a>(graph: &'a Value, id: &str) -> &'a Value {
    components(graph)
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("{id} is drawn: {graph}"))
}

fn connection<'a>(graph: &'a Value, from: &str, label: &str, to: &str) -> &'a Value {
    graph["connections"]
        .as_array()
        .expect("connections")
        .iter()
        .find(|c| c["from"] == from && c["label"] == label && c["to"] == to)
        .unwrap_or_else(|| panic!("({from}, {label}, {to}) is drawn: {graph}"))
}

/// Falsification 3: every node id and every `(from, relation, to)` is in the output, and nothing else is.
#[test]
fn every_node_and_every_edge_is_drawn_and_nothing_else() {
    let scratch = Scratch::new("equality");
    let document = fixture();
    let graph = converted(&scratch, &document);

    // The document's nodes, and the one target an edge names that the corpus does not hold.
    let expected: BTreeSet<&str> = [
        "REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004", "REQ-0005", "REQ-0009",
    ]
    .into();
    let drawn: BTreeSet<&str> = components(&graph)
        .iter()
        .map(|c| c["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(
        drawn, expected,
        "one component per node, and the named missing target"
    );
    assert_eq!(
        components(&graph).len(),
        expected.len(),
        "no component twice"
    );

    let mut wanted: Vec<(String, String, String)> = document["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            (
                e["from"].as_str().expect("from").to_string(),
                e["relation"].as_str().expect("relation").to_string(),
                e["to"].as_str().expect("to").to_string(),
            )
        })
        .collect();
    let mut got: Vec<(String, String, String)> = graph["connections"]
        .as_array()
        .expect("connections")
        .iter()
        .map(|c| {
            (
                c["from"].as_str().expect("from").to_string(),
                c["label"].as_str().expect("label").to_string(),
                c["to"].as_str().expect("to").to_string(),
            )
        })
        .collect();
    wanted.sort();
    got.sort();
    assert_eq!(
        got, wanted,
        "every (from, relation, to), and no edge the document does not hold"
    );

    // A node's label is its claim id, and the statement is carried in full (never shortened).
    for n in document["nodes"].as_array().expect("nodes") {
        let id = n["claim_id"].as_str().expect("id");
        assert_eq!(component(&graph, id)["label"], id);
        let carried = graph.to_string();
        assert!(
            carried.contains(n["statement"].as_str().expect("statement")),
            "the statement of {id} is in the file"
        );
    }
}

/// A claim with no relation at all is still a node of the picture.
#[test]
fn a_claim_with_no_relation_is_still_drawn() {
    let scratch = Scratch::new("isolated");
    let mut document = fixture();
    document["nodes"].as_array_mut().expect("nodes").push(node(
        "REQ-0006",
        "The service shall keep an audit log.",
        "applies",
        &[],
    ));
    let graph = converted(&scratch, &document);
    assert_eq!(component(&graph, "REQ-0006")["label"], "REQ-0006");
}

/// Every file of a directory, by name, with its bytes.
fn directory_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .expect("the output directory")
        .map(|e| {
            let path = e.expect("an entry").path();
            (
                path.file_name()
                    .and_then(|n| n.to_str())
                    .expect("a name")
                    .to_string(),
                fs::read(&path).expect("the file"),
            )
        })
        .collect()
}

/// Falsification (ADR 0050, 4): one document gives one directory, byte for byte, in every file.
#[test]
fn two_runs_give_identical_directories() {
    let scratch = Scratch::new("determinism");
    for (name, document, limit) in [
        ("single", fixture(), None),
        ("split", families(), Some("5")),
        ("parts", oversized_family(), Some("5")),
    ] {
        let input = write_fixture(&scratch, &format!("{name}.json"), &document);
        let first = scratch.path(&format!("{name}-first"));
        let second = scratch.path(&format!("{name}-second"));
        assert_eq!(run_with(&input, &first, limit).code, 0);
        assert_eq!(run_with(&input, &second, limit).code, 0);
        let (a, b) = (directory_bytes(&first), directory_bytes(&second));
        assert!(!a.is_empty());
        assert_eq!(a, b, "{name}: the converter has no clock and no randomness");
    }
}

/// The document may be read from stdin (`-`), and gives the same bytes as the file path does.
#[test]
fn a_dash_reads_the_document_from_stdin() {
    let scratch = Scratch::new("stdin");
    let input = write_fixture(&scratch, "trace.json", &fixture());
    let from_file = scratch.path("file");
    assert_eq!(run(&input, &from_file).code, 0);

    let from_stdin = scratch.path("stdin");
    let mut child = Command::new(python3())
        .arg("-I")
        .arg(script())
        .arg("-")
        .arg(&from_stdin)
        .current_dir(root())
        .stdin(Stdio::piped())
        .spawn()
        .expect("runs the converter");
    use std::io::Write as _;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&fs::read(&input).expect("the fixture"))
        .expect("writes stdin");
    assert!(child.wait().expect("waits").success());
    assert_eq!(directory_bytes(&from_file), directory_bytes(&from_stdin));
}

/// Falsification 5: a schema it was not written for is refused, naming it; a malformed document too; and
/// nothing is written either time.
#[test]
fn another_schema_version_is_refused_with_nothing_written() {
    let scratch = Scratch::new("schema");
    let mut document = fixture();
    document["schema"] = json!("req.trace/2");
    let input = write_fixture(&scratch, "trace.json", &document);
    let output = scratch.path("graph");
    let ran = run(&input, &output);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(
        ran.stderr.contains("req.trace/2"),
        "the refusal names the schema:\n{}",
        ran.stderr
    );
    assert!(!output.exists(), "a refusal writes nothing");
}

#[test]
fn malformed_json_is_refused_with_nothing_written() {
    let scratch = Scratch::new("malformed");
    let input = scratch.path("trace.json");
    fs::write(&input, "{\"schema\": \"req.trace/1\", \"nodes\": [").expect("writes the fixture");
    let output = scratch.path("graph");
    let ran = run(&input, &output);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(
        ran.stderr.contains("malformed JSON"),
        "the refusal names the malformed JSON, as the schema refusal names the schema:\n{}",
        ran.stderr
    );
    assert!(!output.exists(), "a refusal writes nothing");
}

/// A `superseded_by` that is not an array of claim ids is a named refusal, not a traceback.
#[test]
fn a_superseded_by_that_is_not_claim_ids_is_refused_with_nothing_written() {
    let scratch = Scratch::new("superseded-type");
    let mut document = fixture();
    document["nodes"][0]["superseded_by"] = json!([1]);
    let input = write_fixture(&scratch, "trace.json", &document);
    let output = scratch.path("graph");
    let ran = run(&input, &output);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(
        ran.stderr.starts_with("refused: "),
        "a named refusal, not a traceback:\n{}",
        ran.stderr
    );
    assert!(ran.stderr.contains("superseded_by"), "{}", ran.stderr);
    assert!(!output.exists(), "a refusal writes nothing");
}

/// An empty `nodes` would be `components: []`, which archify refuses: the converter refuses first, by name.
#[test]
fn a_document_with_no_nodes_is_refused_with_nothing_written() {
    let scratch = Scratch::new("empty-nodes");
    let mut document = fixture();
    document["nodes"] = json!([]);
    document["edges"] = json!([]);
    document["findings"] = json!({"dangling": [], "undecided_edges": []});
    let input = write_fixture(&scratch, "trace.json", &document);
    let output = scratch.path("graph");
    let ran = run(&input, &output);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(
        ran.stderr.starts_with("refused: "),
        "a named refusal, not a traceback:\n{}",
        ran.stderr
    );
    assert!(ran.stderr.contains("nodes"), "{}", ran.stderr);
    assert!(!output.exists(), "a refusal writes nothing");
}

/// A document that names the right schema but lacks a part of it is malformed, not guessed at.
#[test]
fn a_document_without_its_nodes_is_refused_with_nothing_written() {
    let scratch = Scratch::new("no-nodes");
    let mut document = fixture();
    document.as_object_mut().expect("an object").remove("nodes");
    let input = write_fixture(&scratch, "trace.json", &document);
    let output = scratch.path("graph");
    let ran = run(&input, &output);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(
        ran.stderr.contains("nodes"),
        "the refusal names what is missing:\n{}",
        ran.stderr
    );
    assert!(!output.exists(), "a refusal writes nothing");
}

/// Withdrawn, does-not-apply, undecided and ordinary claims are four different drawings, and the page says so in
/// text as well as in colour.
#[test]
fn a_withdrawn_node_is_marked_distinctly() {
    let scratch = Scratch::new("withdrawn");
    let graph = converted(&scratch, &fixture());
    let applies = component(&graph, "REQ-0004");
    let withdrawn = component(&graph, "REQ-0003");
    let undecided = component(&graph, "REQ-0005");
    assert_ne!(withdrawn["type"], applies["type"]);
    assert_ne!(withdrawn["type"], undecided["type"]);
    assert_ne!(undecided["type"], applies["type"]);
    assert!(
        withdrawn["tag"]
            .as_str()
            .expect("a tag")
            .contains("withdrawn"),
        "colour alone never says it: {withdrawn}"
    );
    assert!(
        undecided["tag"]
            .as_str()
            .expect("a tag")
            .contains("undecided")
    );
    // The successor that withdrew it is carried, from the document, in the findings card.
    let cards = graph["cards"].as_array().expect("cards");
    let item = cards
        .iter()
        .flat_map(|c| c["items"].as_array().expect("items"))
        .map(|i| i.as_str().expect("an item"))
        .find(|i| i.starts_with("REQ-0003:") && i.contains("withdrawn"))
        .unwrap_or_else(|| panic!("a findings item for REQ-0003: {cards:?}"));
    assert!(item.contains("REQ-0004"), "the successor is named: {item}");
}

#[test]
fn a_does_not_apply_node_that_is_not_withdrawn_is_drawn_apart_from_a_withdrawn_one() {
    let scratch = Scratch::new("outside");
    let mut document = fixture();
    document["nodes"].as_array_mut().expect("nodes").push(node(
        "REQ-0006",
        "The service shall encrypt data in transit.",
        "does_not_apply",
        &[],
    ));
    let graph = converted(&scratch, &document);
    let outside = component(&graph, "REQ-0006");
    assert_ne!(
        outside["type"],
        component(&graph, "REQ-0003")["type"],
        "outside is not withdrawn"
    );
    assert_ne!(
        outside["type"],
        component(&graph, "REQ-0004")["type"],
        "outside is not applying"
    );
}

/// A dangling edge and an undecided edge each look unlike an ordinary edge, and unlike each other; the edge keeps
/// its relation as its label either way.
#[test]
fn a_dangling_edge_and_an_undecided_edge_are_marked_distinctly() {
    let scratch = Scratch::new("edges");
    let graph = converted(&scratch, &fixture());
    let ordinary = connection(&graph, "REQ-0002", "refines", "REQ-0001");
    let dangling = connection(&graph, "REQ-0002", "derives-from", "REQ-0009");
    let undecided = connection(&graph, "REQ-0004", "refines", "REQ-0005");
    assert!(
        ordinary.get("variant").is_none(),
        "an ordinary edge is the default: {ordinary}"
    );
    assert!(dangling.get("variant").is_some(), "{dangling}");
    assert!(undecided.get("variant").is_some(), "{undecided}");
    assert_ne!(dangling["variant"], undecided["variant"]);
    // The target of a dangling edge is drawn as what it is: named by an edge, not in the corpus.
    assert!(
        component(&graph, "REQ-0009")["tag"]
            .as_str()
            .expect("a tag")
            .contains("not in the corpus")
    );
}

/// Placement is computed: a claim sits on a later row than every claim it derives from or refines, and every
/// coordinate is an integer.
#[test]
fn layers_follow_the_derives_from_and_refines_depth_and_every_number_is_an_integer() {
    let scratch = Scratch::new("layers");
    let graph = converted(&scratch, &fixture());
    let y = |id: &str| {
        component(&graph, id)["pos"][1]
            .as_i64()
            .expect("an integer y")
    };
    assert!(y("REQ-0001") < y("REQ-0002"), "REQ-0002 refines REQ-0001");
    assert!(
        y("REQ-0002") < y("REQ-0004"),
        "REQ-0004 derives from REQ-0002"
    );

    fn only_integers(value: &Value) {
        match value {
            Value::Number(n) => assert!(n.is_i64() || n.is_u64(), "a float in the output: {n}"),
            Value::Array(items) => items.iter().for_each(only_integers),
            Value::Object(map) => map.values().for_each(only_integers),
            _ => {}
        }
    }
    only_integers(&graph);
}

/// A relation cycle must not hang the layout or change what is drawn.
#[test]
fn a_derives_from_cycle_is_drawn_and_finishes() {
    let scratch = Scratch::new("cycle");
    let mut document = fixture();
    document["edges"].as_array_mut().expect("edges").push(edge(
        "REQ-0001",
        "derives-from",
        "REQ-0004",
    ));
    let graph = converted(&scratch, &document);
    connection(&graph, "REQ-0001", "derives-from", "REQ-0004");
}

/// Falsification 4, statically: the converter imports no module that can reach a network, start a process or read
/// a clock or a random source. (Behaviour is what the other tests pin; this pins the shape.)
#[test]
fn the_converter_imports_nothing_that_can_reach_a_network() {
    let source = fs::read_to_string(script()).expect("the converter source");
    let forbidden = [
        "socket",
        "urllib",
        "http",
        "ssl",
        "subprocess",
        "requests",
        "ftplib",
        "smtplib",
        "telnetlib",
        "asyncio",
        "random",
        "secrets",
        "time",
        "datetime",
        "uuid",
        "shutil",
        "tempfile",
        "multiprocessing",
        "threading",
    ];
    let mut imported = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim_start();
        let names = if let Some(rest) = line.strip_prefix("import ") {
            rest.split(',')
                .map(|m| m.trim().split(' ').next().unwrap_or(""))
                .collect::<Vec<_>>()
        } else if let Some(rest) = line.strip_prefix("from ") {
            vec![rest.split(' ').next().unwrap_or("")]
        } else {
            continue;
        };
        for name in names {
            imported.insert(name.split('.').next().unwrap_or("").to_string());
        }
    }
    assert!(
        !imported.is_empty(),
        "the scan found the converter's imports"
    );
    for name in forbidden {
        assert!(
            !imported.contains(name),
            "the converter imports `{name}`: {imported:?}"
        );
    }
    // `os` reaches processes, so the only thing the converter may take from it is the page-limit override.
    if imported.contains("os") {
        let os_lines: Vec<&str> = source
            .lines()
            .map(str::trim_start)
            .filter(|l| l.starts_with("import os") || l.starts_with("from os"))
            .collect();
        assert_eq!(
            os_lines,
            vec!["from os import environ"],
            "`os` is imported only for `environ`"
        );
    }
}

/// The pin is a constant in the file, and the output carries it where the schema has a free-text field.
#[test]
fn the_archify_pin_is_in_the_file_and_in_the_output() {
    let source = fs::read_to_string(script()).expect("the converter source");
    assert!(source.contains("ARCHIFY_COMMIT = \"7e45f36\""));
    let scratch = Scratch::new("pin");
    let graph = converted(&scratch, &fixture());
    assert!(
        graph["meta"]["subtitle"]
            .as_str()
            .expect("a subtitle")
            .contains("7e45f36"),
        "{}",
        graph["meta"]
    );
}

// ---------------------------------------------------------------------------
// Placement: no edge is left to archify's automatic route or label (V-1-fix, F1)
// ---------------------------------------------------------------------------

fn small_document(nodes: Vec<Value>, edges: Vec<Value>, dangling: &[&str]) -> Value {
    json!({
        "schema": "req.trace/1",
        "generator": "trace",
        "rules_v": 9,
        "selection": {},
        "nodes": nodes,
        "edges": edges,
        "derived_incoming": [],
        "findings": {"dangling": dangling, "undecided_edges": []},
    })
}

/// A box, as `(x0, y0, x1, y1)`.
type Rect = (i64, i64, i64, i64);

fn boxes(graph: &Value) -> Vec<(String, Rect)> {
    components(graph)
        .iter()
        .map(|c| {
            let (x, y) = (
                c["pos"][0].as_i64().expect("x"),
                c["pos"][1].as_i64().expect("y"),
            );
            let (w, h) = (
                c["size"][0].as_i64().expect("w"),
                c["size"][1].as_i64().expect("h"),
            );
            (
                c["id"].as_str().expect("id").to_string(),
                (x, y, x + w, y + h),
            )
        })
        .collect()
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// The rule, over the output alone: every connection carries an explicit route whose label sits on a routed
/// segment, and neither that label nor any routed segment touches a node. archify's own automatic route puts the
/// label of an adjacent-row edge inside the source node, so none is left to it.
fn assert_every_edge_is_explicitly_placed(graph: &Value) {
    let nodes = boxes(graph);
    for c in graph["connections"].as_array().expect("connections") {
        let id = c["id"].as_str().expect("an id");
        let via: Vec<(i64, i64)> = c["via"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} has no explicit route: {c}"))
            .iter()
            .map(|p| (p[0].as_i64().expect("x"), p[1].as_i64().expect("y")))
            .collect();
        assert!(
            via.len() >= 2,
            "{id}: the label segment needs two route points: {c}"
        );
        assert_eq!(
            c["labelSegment"], 1,
            "{id}: the label sits on the first routed segment: {c}"
        );
        assert!(
            c["fromSide"].is_string() && c["toSide"].is_string(),
            "{id}: sides are pinned: {c}"
        );

        // The label: centred on the segment's midpoint, as wide as the text needs (7 px a character, generously).
        let label = c["label"].as_str().expect("a label");
        let (mx, my) = ((via[0].0 + via[1].0) / 2, (via[0].1 + via[1].1) / 2);
        let half_w = 7 * i64::try_from(label.len()).expect("a short label") / 2 + 4;
        let rect = (mx - half_w, my - 9, mx + half_w, my + 9);
        for (node, at) in &nodes {
            assert!(
                !overlaps(rect, *at),
                "{id}: the label {label:?} at {rect:?} overlaps {node} {at:?}"
            );
        }
        // Every routed segment runs in free space.
        for pair in via.windows(2) {
            let seg = (
                pair[0].0.min(pair[1].0),
                pair[0].1.min(pair[1].1),
                pair[0].0.max(pair[1].0) + 1,
                pair[0].1.max(pair[1].1) + 1,
            );
            for (node, at) in &nodes {
                assert!(
                    !overlaps(seg, *at),
                    "{id}: the route {seg:?} crosses {node} {at:?}"
                );
            }
        }
    }
}

/// Shape 1: a successor and the claim it supersedes, one row below it in the same column. The corpus: REQ-0001
/// supersedes REQ-0003, which derives from REQ-0002.
#[test]
fn a_downward_adjacent_supersedes_edge_has_its_label_off_the_node() {
    let scratch = Scratch::new("shape-supersedes");
    let document = small_document(
        vec![
            node(
                "REQ-0001",
                "The API shall respond in no more than 100 ms.",
                "applies",
                &[],
            ),
            node(
                "REQ-0002",
                "The service shall log every request.",
                "applies",
                &[],
            ),
            node(
                "REQ-0003",
                "The API shall respond in at least 200 ms.",
                "does_not_apply",
                &["REQ-0001"],
            ),
        ],
        vec![
            edge("REQ-0001", "supersedes", "REQ-0003"),
            edge("REQ-0003", "derives-from", "REQ-0002"),
        ],
        &[],
    );
    let graph = converted(&scratch, &document);
    // The shape is the one the review named: the target is exactly one row below its source.
    let y = |id: &str| component(&graph, id)["pos"][1].as_i64().expect("y");
    assert!(y("REQ-0003") > y("REQ-0001"));
    assert_every_edge_is_explicitly_placed(&graph);
}

/// Shape 2: a parent that names its child (a downward `constrains`), the child deriving from the parent.
#[test]
fn a_downward_parent_to_child_edge_has_its_label_off_the_node() {
    let scratch = Scratch::new("shape-constrains");
    let document = small_document(
        vec![
            node(
                "REQ-0001",
                "The service shall log every request.",
                "applies",
                &[],
            ),
            node(
                "REQ-0002",
                "The service shall log the caller of every request.",
                "applies",
                &[],
            ),
        ],
        vec![
            edge("REQ-0001", "constrains", "REQ-0002"),
            edge("REQ-0002", "derives-from", "REQ-0001"),
        ],
        &[],
    );
    let graph = converted(&scratch, &document);
    assert_every_edge_is_explicitly_placed(&graph);
}

/// Shape 3: an edge into a claim the corpus does not hold, whose placeholder is drawn one row below.
#[test]
fn a_dangling_target_one_row_below_has_its_label_off_the_node() {
    let scratch = Scratch::new("shape-dangling");
    let document = small_document(
        vec![node(
            "REQ-0001",
            "The service shall log every request.",
            "applies",
            &[],
        )],
        vec![edge("REQ-0001", "refines", "REQ-0404")],
        &["refines -> REQ-0404 (referenced by REQ-0001, not in the corpus)"],
    );
    let graph = converted(&scratch, &document);
    let y = |id: &str| component(&graph, id)["pos"][1].as_i64().expect("y");
    assert!(y("REQ-0404") > y("REQ-0001"));
    assert_every_edge_is_explicitly_placed(&graph);
}

/// The rule holds on the general fixture too: neighbours in one row, upward edges and long edges alike.
#[test]
fn every_edge_of_the_general_fixture_is_explicitly_placed() {
    let scratch = Scratch::new("shape-general");
    let graph = converted(&scratch, &fixture());
    assert_every_edge_is_explicitly_placed(&graph);
}

// ---------------------------------------------------------------------------
// V-2 (ADR 0050): a graph page shows a declared slice, grouped and laid out in layers
// ---------------------------------------------------------------------------

fn claim(id: &str) -> Value {
    node(
        id,
        &format!("The system shall do what {id} says."),
        "applies",
        &[],
    )
}

fn document(selection: Value, nodes: Vec<Value>, edges: Vec<Value>) -> Value {
    json!({
        "schema": "req.trace/1",
        "generator": "trace",
        "rules_v": 9,
        "selection": selection,
        "nodes": nodes,
        "edges": edges,
        "derived_incoming": [],
        "findings": {"dangling": [], "undecided_edges": []},
    })
}

/// Two families and one standalone claim, hand-written:
///
/// * family REQ-0001 holds 0001 (root), 0002 and 0003 (each below 0001) and 0004 (below 0002);
/// * family REQ-0010 holds 0010 (root), 0011 and 0013 (below 0010) and 0012 (below 0011);
/// * REQ-0020 has no edge at all, so it is standalone;
/// * four relations cross from one family to the other, none of them a `derives-from` or `refines`, so no claim
///   changes family: 0012 and 0013 constrain 0004 and 0003, 0002 constrains 0011, and 0011 informs 0002.
fn families() -> Value {
    document(
        json!({"deployment": "cloud"}),
        vec![
            claim("REQ-0001"),
            claim("REQ-0002"),
            claim("REQ-0003"),
            claim("REQ-0004"),
            claim("REQ-0010"),
            claim("REQ-0011"),
            claim("REQ-0012"),
            claim("REQ-0013"),
            claim("REQ-0020"),
        ],
        vec![
            edge("REQ-0002", "refines", "REQ-0001"),
            edge("REQ-0003", "derives-from", "REQ-0001"),
            edge("REQ-0004", "derives-from", "REQ-0002"),
            edge("REQ-0011", "refines", "REQ-0010"),
            edge("REQ-0013", "derives-from", "REQ-0010"),
            edge("REQ-0012", "derives-from", "REQ-0011"),
            edge("REQ-0012", "constrains", "REQ-0004"),
            edge("REQ-0013", "constrains", "REQ-0003"),
            edge("REQ-0002", "constrains", "REQ-0011"),
            edge("REQ-0011", "informs", "REQ-0002"),
        ],
    )
}

const FAMILY_A: [&str; 4] = ["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"];
const FAMILY_B: [&str; 4] = ["REQ-0010", "REQ-0011", "REQ-0012", "REQ-0013"];

/// The page that holds a claim of `families()` when the limit splits it, written out by hand.
fn family_page(id: &str) -> &'static str {
    if FAMILY_A.contains(&id) {
        "family-REQ-0001"
    } else if FAMILY_B.contains(&id) {
        "family-REQ-0010"
    } else {
        "standalone"
    }
}

/// One family of twelve claims in five layers, written out by hand:
///
/// * layer 0: 0001; layer 1: 0002, 0003, 0004 (refine 0001);
/// * layer 2: 0005, 0006, 0007 (derive from 0002, 0003, 0004); layer 3: 0008, 0009, 0010 (from 0005, 0006, 0007);
/// * layer 4: 0011, 0012 (from 0008, 0009).
///
/// With a limit of five, whole layers fill a part until the next one would not fit: layers 0 and 1 (four claims),
/// layer 2 (three: with layer 3 it would be six), then layers 3 and 4 (five).
fn oversized_family() -> Value {
    let ids: Vec<String> = (1..=12).map(|n| format!("REQ-{n:04}")).collect();
    let edges = vec![
        edge("REQ-0002", "refines", "REQ-0001"),
        edge("REQ-0003", "refines", "REQ-0001"),
        edge("REQ-0004", "refines", "REQ-0001"),
        edge("REQ-0005", "derives-from", "REQ-0002"),
        edge("REQ-0006", "derives-from", "REQ-0003"),
        edge("REQ-0007", "derives-from", "REQ-0004"),
        edge("REQ-0008", "derives-from", "REQ-0005"),
        edge("REQ-0009", "derives-from", "REQ-0006"),
        edge("REQ-0010", "derives-from", "REQ-0007"),
        edge("REQ-0011", "derives-from", "REQ-0008"),
        edge("REQ-0012", "derives-from", "REQ-0009"),
    ];
    document(json!({}), ids.iter().map(|i| claim(i)).collect(), edges)
}

const PART_1: [&str; 4] = ["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"];
const PART_2: [&str; 3] = ["REQ-0005", "REQ-0006", "REQ-0007"];
const PART_3: [&str; 5] = ["REQ-0008", "REQ-0009", "REQ-0010", "REQ-0011", "REQ-0012"];

fn part_page(id: &str) -> &'static str {
    if PART_1.contains(&id) {
        "family-REQ-0001"
    } else if PART_2.contains(&id) {
        "family-REQ-0001-part-2"
    } else {
        "family-REQ-0001-part-3"
    }
}

/// The tags the ADR gives the two kinds of node that are not claims of a page: a stub (ADR 0050 decision 4, a claim held
/// on another page) and the target of a dangling edge (ADR 0046). Every other node is a claim of the page.
const STUB_TAG: &str = "on another page";
const DANGLING_TAG: &str = "not in the corpus";

/// The ids a page draws as claims, by tag: anything that is neither a stub nor a dangling target.
fn claims_on(page: &Value) -> BTreeSet<String> {
    components(page)
        .iter()
        .filter(|c| c["tag"] != STUB_TAG && c["tag"] != DANGLING_TAG)
        .map(|c| c["id"].as_str().expect("an id").to_string())
        .collect()
}

fn subtitle(page: &Value) -> &str {
    page["meta"]["subtitle"].as_str().expect("a subtitle")
}

fn boundaries(page: &Value) -> Vec<&Value> {
    page["boundaries"]
        .as_array()
        .map(|b| b.iter().collect())
        .unwrap_or_default()
}

fn names(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn wraps(boundary: &Value) -> BTreeSet<String> {
    boundary["wraps"]
        .as_array()
        .expect("wraps")
        .iter()
        .map(|w| w.as_str().expect("an id").to_string())
        .collect()
}

fn connections_of(page: &Value) -> Vec<&Value> {
    page["connections"]
        .as_array()
        .map(|c| c.iter().collect())
        .unwrap_or_default()
}

fn group_pages(pages: &BTreeMap<String, Value>) -> BTreeMap<&String, &Value> {
    pages.iter().filter(|(name, _)| *name != "index").collect()
}

/// A document of about eight claims in two families plus one standalone claim is one page, `all.json`: a `region`
/// frame per family named by its root, a focus view per family, and the standalone claim outside every frame.
#[test]
fn a_document_within_the_limit_is_one_page_with_a_frame_and_a_view_per_family() {
    let scratch = Scratch::new("one-page");
    let page = converted(&scratch, &families());

    let frames = boundaries(&page);
    assert_eq!(frames.len(), 2, "one frame per family: {page}");
    assert!(frames.iter().all(|b| b["kind"] == "region"));
    let by_label: BTreeMap<&str, BTreeSet<String>> = frames
        .iter()
        .map(|b| (b["label"].as_str().expect("a label"), wraps(b)))
        .collect();
    assert_eq!(
        by_label,
        BTreeMap::from([
            ("REQ-0001", names(&FAMILY_A)),
            ("REQ-0010", names(&FAMILY_B)),
        ]),
        "frames are named by the root ids"
    );

    let views = page["meta"]["views"].as_array().expect("views");
    assert_eq!(views.len(), 2, "a focus view per family: {views:?}");
    let focuses: BTreeSet<BTreeSet<String>> = views
        .iter()
        .map(|v| {
            v["focus"]
                .as_array()
                .expect("focus")
                .iter()
                .map(|f| f.as_str().expect("an id").to_string())
                .collect()
        })
        .collect();
    assert_eq!(
        focuses,
        BTreeSet::from([names(&FAMILY_A), names(&FAMILY_B)])
    );

    let standalone = component(&page, "REQ-0020");
    assert_eq!(standalone["label"], "REQ-0020");
    assert!(
        frames.iter().all(|b| !wraps(b).contains("REQ-0020")),
        "the standalone claim is outside both frames"
    );
}

/// ADR 0050 decision 3: past the limit there is an index plus one page per group; every claim is on exactly one
/// group page (falsification 2) and the index has a node per group and a connection per (group pair, relation).
#[test]
fn past_the_limit_there_is_an_index_and_a_page_per_group_with_every_claim_once() {
    let scratch = Scratch::new("split");
    let document = families();
    let pages = converted_pages(&scratch, &document, Some("5"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["family-REQ-0001", "family-REQ-0010", "index", "standalone"]
    );

    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (name, page) in group_pages(&pages) {
        let on_page = claims_on(page);
        assert!(on_page.len() <= 5, "{name} holds {} claims", on_page.len());
        for id in on_page {
            assert_eq!(family_page(&id), name, "{id} is on the page of its group");
            *seen.entry(id).or_default() += 1;
        }
    }
    let expected: BTreeMap<String, usize> = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| (n["claim_id"].as_str().expect("id").to_string(), 1))
        .collect();
    assert_eq!(seen, expected, "every claim on exactly one group page");

    let index = &pages["index"];
    let nodes: BTreeSet<String> = components(index)
        .iter()
        .map(|c| c["id"].as_str().expect("an id").to_string())
        .collect();
    assert_eq!(
        nodes,
        names(&["family-REQ-0001", "family-REQ-0010", "standalone"]),
        "one node per group, named by its page"
    );
    let drawn: BTreeSet<(String, String, String)> = connections_of(index)
        .iter()
        .map(|c| {
            (
                c["from"].as_str().expect("from").to_string(),
                c["to"].as_str().expect("to").to_string(),
                c["label"].as_str().expect("label").to_string(),
            )
        })
        .collect();
    let wanted: BTreeSet<(String, String, String)> = [
        ("family-REQ-0001", "family-REQ-0010", "constrains \u{d7}1"),
        ("family-REQ-0010", "family-REQ-0001", "constrains \u{d7}2"),
        ("family-REQ-0010", "family-REQ-0001", "informs \u{d7}1"),
    ]
    .iter()
    .map(|(a, b, c)| ((*a).to_string(), (*b).to_string(), (*c).to_string()))
    .collect();
    assert_eq!(drawn, wanted, "one connection per group pair and relation");
    assert_eq!(connections_of(index).len(), 3, "and no connection twice");
}

/// Falsification 3: an edge with an end on a page is on it, as a drawn edge whose far end is either a claim of the
/// page or a stub naming the page that holds it; an edge with no end on the page is not.
fn assert_edges_are_drawn_or_stubbed(
    document: &Value,
    pages: &BTreeMap<String, Value>,
    page_of: &dyn Fn(&str) -> &'static str,
) {
    let edges = document["edges"].as_array().expect("edges");
    for (name, page) in group_pages(pages) {
        let here = claims_on(page);
        let touching: Vec<&Value> = edges
            .iter()
            .filter(|e| {
                here.contains(e["from"].as_str().expect("from"))
                    || here.contains(e["to"].as_str().expect("to"))
            })
            .collect();
        assert_eq!(
            connections_of(page).len(),
            touching.len(),
            "{name}: exactly the edges with an end on the page are drawn"
        );
        for e in &touching {
            let (from, label, to) = (
                e["from"].as_str().expect("from"),
                e["relation"].as_str().expect("relation"),
                e["to"].as_str().expect("to"),
            );
            connection(page, from, label, to);
            for end in [from, to] {
                if here.contains(end) {
                    continue;
                }
                let far = component(page, end);
                assert_eq!(far["type"], "cloud", "{name}: {end} is outside the page");
                let held = document["nodes"]
                    .as_array()
                    .expect("nodes")
                    .iter()
                    .any(|n| n["claim_id"] == end);
                if !held {
                    assert_eq!(far["tag"], DANGLING_TAG, "{name}: {end} is dangling");
                    assert!(
                        boundaries(page).iter().all(|b| !wraps(b).contains(end)),
                        "{name}: a dangling target sits in no frame"
                    );
                    continue;
                }
                assert_eq!(far["tag"], STUB_TAG, "{name}: {end} is a stub");
                let frame = boundaries(page)
                    .into_iter()
                    .find(|b| wraps(b).contains(end))
                    .unwrap_or_else(|| panic!("{name}: the stub {end} sits in a frame: {page}"));
                assert!(
                    frame["label"]
                        .as_str()
                        .expect("a label")
                        .contains(page_of(end)),
                    "{name}: the stub {end} names {}: {frame}",
                    page_of(end)
                );
            }
        }
        // Every stub is the end of an edge drawn on this page.
        for c in components(page).iter().filter(|c| c["type"] == "cloud") {
            let id = c["id"].as_str().expect("an id");
            assert!(
                touching.iter().any(|e| e["from"] == id || e["to"] == id),
                "{name}: the stub {id} ends no edge"
            );
        }
    }
}

#[test]
fn a_cross_group_edge_is_drawn_on_both_pages_ending_in_a_stub_that_names_the_other() {
    let scratch = Scratch::new("cross-group");
    let document = families();
    let pages = converted_pages(&scratch, &document, Some("5"));
    assert_edges_are_drawn_or_stubbed(&document, &pages, &family_page);

    // The spec's own example, by hand: 0012 (family REQ-0010) constrains 0004 (family REQ-0001).
    let source_page = &pages["family-REQ-0010"];
    connection(source_page, "REQ-0012", "constrains", "REQ-0004");
    let stub = component(source_page, "REQ-0004");
    assert_eq!(stub["type"], "cloud");
    assert_eq!(stub["label"], "REQ-0004");
    let frame = boundaries(source_page)
        .into_iter()
        .find(|b| wraps(b).contains("REQ-0004"))
        .expect("the stub's frame");
    assert!(
        frame["label"]
            .as_str()
            .expect("label")
            .contains("family-REQ-0001")
    );
}

/// ADR 0050 decision 3: a group past the limit is cut into parts by layer; no page holds more than the limit
/// (falsification 5), each part says `part i of n`, and every claim is on exactly one part.
#[test]
fn an_oversized_family_is_cut_into_parts_by_layer_and_no_page_exceeds_the_limit() {
    let scratch = Scratch::new("parts");
    let document = oversized_family();
    let pages = converted_pages(&scratch, &document, Some("5"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "family-REQ-0001",
            "family-REQ-0001-part-2",
            "family-REQ-0001-part-3",
            "index"
        ]
    );
    assert_eq!(claims_on(&pages["family-REQ-0001"]), names(&PART_1));
    assert_eq!(claims_on(&pages["family-REQ-0001-part-2"]), names(&PART_2));
    assert_eq!(claims_on(&pages["family-REQ-0001-part-3"]), names(&PART_3));

    for (i, name) in [
        "family-REQ-0001",
        "family-REQ-0001-part-2",
        "family-REQ-0001-part-3",
    ]
    .iter()
    .enumerate()
    {
        let page = &pages[*name];
        assert!(claims_on(page).len() <= 5, "{name}");
        assert!(
            subtitle(page).contains(&format!("part {} of 3", i + 1)),
            "{name}: {}",
            subtitle(page)
        );
    }
    assert_eq!(
        components(&pages["index"]).len(),
        1,
        "the family is one group"
    );
    assert!(connections_of(&pages["index"]).is_empty());

    // An edge between two parts is a stub on each side, naming the other part's page.
    assert_edges_are_drawn_or_stubbed(&document, &pages, &part_page);
}

/// A group can be one layer wide (the standalone group always is): a layer over the limit is cut in claim id order
/// into consecutive parts of at most the limit, every claim on exactly one part.
#[test]
fn a_one_layer_group_over_the_limit_is_cut_in_claim_id_order() {
    // Twelve claims and no relation: all standalone, one layer.
    let ids: Vec<String> = (1..=12).map(|n| format!("REQ-{n:04}")).collect();
    let loose = document(json!({}), ids.iter().map(|i| claim(i)).collect(), vec![]);
    let scratch = Scratch::new("one-layer-standalone");
    let pages = converted_pages(&scratch, &loose, Some("5"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "index",
            "standalone",
            "standalone-part-2",
            "standalone-part-3"
        ]
    );
    let expected = [
        ("standalone", &ids[0..5]),
        ("standalone-part-2", &ids[5..10]),
        ("standalone-part-3", &ids[10..12]),
    ];
    let mut seen = BTreeSet::new();
    for (i, (name, members)) in expected.iter().enumerate() {
        let on_page = claims_on(&pages[*name]);
        assert!(on_page.len() <= 5, "{name}");
        assert_eq!(on_page, members.iter().cloned().collect(), "{name}");
        assert!(subtitle(&pages[*name]).contains(&format!("part {} of 3", i + 1)));
        assert!(seen.is_disjoint(&on_page), "{name} repeats a claim");
        seen.extend(on_page);
    }
    assert_eq!(seen.len(), 12);
    assert_eq!(components(&pages["index"]).len(), 1, "one group");

    // The same inside a family: a root with seven children is two layers, the second wider than the limit of four.
    let mut nodes = vec![claim("REQ-0001")];
    let mut edges = Vec::new();
    for n in 2..=8 {
        let id = format!("REQ-{n:04}");
        nodes.push(claim(&id));
        edges.push(edge(&id, "refines", "REQ-0001"));
    }
    let wide = document(json!({}), nodes, edges);
    let scratch = Scratch::new("one-layer-family");
    let pages = converted_pages(&scratch, &wide, Some("4"));
    let parts: Vec<BTreeSet<String>> = [
        "family-REQ-0001",
        "family-REQ-0001-part-2",
        "family-REQ-0001-part-3",
    ]
    .iter()
    .map(|name| claims_on(&pages[*name]))
    .collect();
    assert_eq!(
        parts[0],
        names(&["REQ-0001"]),
        "the root's layer fits whole"
    );
    assert_eq!(
        parts[1],
        names(&["REQ-0002", "REQ-0003", "REQ-0004", "REQ-0005"])
    );
    assert_eq!(parts[2], names(&["REQ-0006", "REQ-0007", "REQ-0008"]));
}

/// Falsification 1: the subtitle states the slice, and a group page's counts match the document.
#[test]
fn the_title_and_subtitle_state_the_slice_and_the_counts_match_the_document() {
    let scratch = Scratch::new("subtitle");
    let one = converted(&scratch, &families());
    assert_eq!(one["meta"]["title"], "Requirement graph");
    assert_eq!(
        subtitle(&one),
        "deployment=cloud \u{b7} all claims \u{b7} 9 of 9 claims \u{b7} 10 relations \u{b7} rules_v 9 \u{b7} archify 7e45f36"
    );

    let scratch = Scratch::new("subtitle-split");
    let pages = converted_pages(&scratch, &families(), Some("5"));
    let tail = "rules_v 9 \u{b7} archify 7e45f36";
    assert_eq!(
        subtitle(&pages["family-REQ-0001"]),
        format!(
            "deployment=cloud \u{b7} family REQ-0001 \u{b7} 4 of 9 claims \u{b7} 7 relations \u{b7} {tail}"
        )
    );
    assert_eq!(
        subtitle(&pages["family-REQ-0010"]),
        format!(
            "deployment=cloud \u{b7} family REQ-0010 \u{b7} 4 of 9 claims \u{b7} 7 relations \u{b7} {tail}"
        )
    );
    assert_eq!(
        subtitle(&pages["standalone"]),
        format!(
            "deployment=cloud \u{b7} standalone claims \u{b7} 1 of 9 claims \u{b7} 0 relations \u{b7} {tail}"
        )
    );
    assert_eq!(
        subtitle(&pages["index"]),
        format!(
            "deployment=cloud \u{b7} index of 3 groups \u{b7} 9 of 9 claims \u{b7} 4 relations \u{b7} {tail}"
        )
    );
    assert_eq!(
        pages["family-REQ-0001"]["meta"]["title"],
        "Requirement graph: family REQ-0001"
    );
    assert_eq!(
        pages["standalone"]["meta"]["title"],
        "Requirement graph: standalone claims"
    );
    assert_eq!(pages["index"]["meta"]["title"], "Requirement graph");
}

/// Three layers: 0001; 0002 and 0003 refine it; 0004, 0005 and 0006 derive from 0003, 0003 and 0002. In claim-id
/// order the third claim's edge crosses the first two (two crossings); the median order puts 0006 first and crosses
/// none.
#[test]
fn a_layer_is_ordered_by_the_median_of_its_neighbours_not_by_claim_id() {
    let scratch = Scratch::new("ordering");
    let document = document(
        json!({}),
        ["0001", "0002", "0003", "0004", "0005", "0006"]
            .iter()
            .map(|n| claim(&format!("REQ-{n}")))
            .collect(),
        vec![
            edge("REQ-0002", "refines", "REQ-0001"),
            edge("REQ-0003", "refines", "REQ-0001"),
            edge("REQ-0004", "derives-from", "REQ-0003"),
            edge("REQ-0005", "derives-from", "REQ-0003"),
            edge("REQ-0006", "derives-from", "REQ-0002"),
        ],
    );
    let page = converted(&scratch, &document);
    let x = |id: &str| component(&page, id)["pos"][0].as_i64().expect("x");
    let y = |id: &str| component(&page, id)["pos"][1].as_i64().expect("y");
    assert!(y("REQ-0001") < y("REQ-0002") && y("REQ-0002") < y("REQ-0004"));
    assert_eq!(y("REQ-0004"), y("REQ-0005"));
    assert_eq!(y("REQ-0004"), y("REQ-0006"));
    assert!(
        x("REQ-0002") < x("REQ-0003"),
        "layer 1 keeps id order on ties"
    );
    assert!(
        x("REQ-0006") < x("REQ-0004") && x("REQ-0004") < x("REQ-0005"),
        "layer 2 follows the median, so the edges do not cross: 0006 under 0002, then 0004 and 0005 under 0003"
    );
}

/// ADR 0050 decision 5: an edge that skips a layer runs through a reserved waypoint in that layer, a free slot of
/// the row, so it crosses no node (and the general rule below still holds).
#[test]
fn a_long_edge_passes_through_a_waypoint_in_each_layer_it_crosses() {
    let scratch = Scratch::new("waypoints");
    let document = document(
        json!({}),
        ["0001", "0002", "0003", "0004"]
            .iter()
            .map(|n| claim(&format!("REQ-{n}")))
            .collect(),
        vec![
            edge("REQ-0002", "refines", "REQ-0001"),
            edge("REQ-0003", "derives-from", "REQ-0002"),
            // Skips layer 1, which holds 0002 and 0004.
            edge("REQ-0003", "derives-from", "REQ-0001"),
            edge("REQ-0004", "refines", "REQ-0001"),
        ],
    );
    let page = converted(&scratch, &document);
    let row = |id: &str| {
        let y = component(&page, id)["pos"][1].as_i64().expect("y");
        (y, y + component(&page, id)["size"][1].as_i64().expect("h"))
    };
    let (top, bottom) = row("REQ-0002");
    assert_eq!(
        row("REQ-0004"),
        (top, bottom),
        "0002 and 0004 share layer 1"
    );
    let long = connection(&page, "REQ-0003", "derives-from", "REQ-0001");
    let inside_the_crossed_row = long["via"].as_array().expect("via").iter().any(|p| {
        let y = p[1].as_i64().expect("y");
        y > top && y < bottom
    });
    assert!(
        inside_the_crossed_row,
        "the route passes through layer 1 at a waypoint: {long}"
    );
    assert_every_edge_is_explicitly_placed(&page);
    assert_no_two_nodes_overlap(&page);
}

fn assert_no_two_nodes_overlap(page: &Value) {
    let nodes = boxes(page);
    for (i, (a, at)) in nodes.iter().enumerate() {
        for (b, bt) in &nodes[i + 1..] {
            assert!(!overlaps(*at, *bt), "{a} {at:?} overlaps {b} {bt:?}");
        }
    }
}

/// A deterministic mid-size document: `blocks` families of twenty (a binary tree, a long edge to the root from
/// every fifth claim, a cross-family `constrains` from every seventh), and `loose` claims with no edge.
fn synthetic(blocks: usize, loose: usize) -> Value {
    let total = blocks * 20;
    let id = |n: usize| format!("REQ-{n:04}");
    let mut nodes: Vec<Value> = (1..=total).map(|n| claim(&id(n))).collect();
    let mut edges = Vec::new();
    for n in 1..=total {
        let position = (n - 1) % 20;
        let root = n - position;
        if position > 0 {
            let parent = root + (position - 1) / 2;
            let relation = if position % 3 == 0 {
                "refines"
            } else {
                "derives-from"
            };
            edges.push(edge(&id(n), relation, &id(parent)));
            if position >= 7 && position % 5 == 0 {
                edges.push(edge(&id(n), "derives-from", &id(root)));
            }
        }
        if n % 7 == 0 {
            edges.push(edge(&id(n), "constrains", &id(total + 1 - n)));
        }
    }
    nodes.extend((1..=loose).map(|n| claim(&id(total + n))));
    document(json!({"deployment": "cloud"}), nodes, edges)
}

/// The placement rules hold on every page of a mid-size document, one page and split: no two nodes overlap, every
/// edge is explicitly routed with its label off the nodes, and no route crosses a node.
#[test]
fn placement_holds_on_every_page_of_a_mid_size_document() {
    let scratch = Scratch::new("synthetic");
    let document = synthetic(4, 9);
    for limit in [None, Some("30")] {
        let pages = converted_pages(&scratch, &document, limit);
        assert_eq!(pages.len() > 1, limit.is_some(), "{limit:?}");
        for (name, page) in group_pages(&pages) {
            assert_no_two_nodes_overlap(page);
            assert_every_edge_is_explicitly_placed(page);
            assert!(
                canvas_width(page) <= MAX_WIDTH,
                "{name}: {}",
                canvas_width(page)
            );
            assert_frames_inside_the_canvas(page);
            assert!(claims_on(page).len() <= 30 || limit.is_none(), "{name}");
        }
        if limit.is_some() {
            let pages_of: BTreeSet<String> = group_pages(&pages)
                .values()
                .flat_map(|p| claims_on(p))
                .collect();
            assert_eq!(pages_of.len(), 80 + 9, "every claim is on a group page");
        }
        let _ = fs::remove_dir_all(scratch.path("graph"));
    }
}

/// The page speaks req's words: the five states, the claim panel's labels and the reach directions (ADR 0050 decision
/// 6). archify's `upstream` follows an edge backwards, to the claims that point at the focused one, which are the
/// claims that rest on it.
#[test]
fn the_page_speaks_reqs_words() {
    let scratch = Scratch::new("words");
    let page = converted(&scratch, &fixture());
    let t = |key: &str| {
        page["meta"]["translations"][key]
            .as_str()
            .unwrap_or_else(|| panic!("no translation for {key}: {}", page["meta"]))
            .to_string()
    };
    assert_eq!(
        page["meta"]["locale"], "en",
        "archify applies translations only to a document that names its locale"
    );
    assert_eq!(t("node.context.architecture"), "Claim");
    assert_eq!(t("viewer.passport.eyebrow"), "Claim");
    assert_eq!(t("viewer.passport.upstream"), "Rests on this");
    assert_eq!(t("viewer.passport.downstream"), "This rests on");
    // The five states name no architecture word, and no two share a word.
    let kinds: BTreeSet<String> = ["backend", "messagebus", "external", "security", "cloud"]
        .iter()
        .map(|k| t(&format!("viewer.kind.{k}")))
        .collect();
    assert_eq!(kinds.len(), 5, "{kinds:?}");
    for word in [
        "Backend",
        "Frontend",
        "Database",
        "Message bus",
        "Cloud",
        "Security",
        "External",
    ] {
        assert!(!kinds.contains(word), "{word} is archify's word, not req's");
    }
    for key in ["backend", "messagebus", "external", "security", "cloud"] {
        assert_eq!(
            t(&format!("legend.architecture.{key}")),
            t(&format!("viewer.kind.{key}")),
            "the legend and the panel say the same"
        );
    }
}

/// ADR 0050 decision 7: the findings card first and only when there is a finding, then one card per group on the
/// page; no card per claim; each item is `claim id: statement`, a finding's with its reason.
#[test]
fn the_cards_are_findings_then_one_per_group() {
    let scratch = Scratch::new("cards");
    let page = converted(&scratch, &fixture());
    let cards = page["cards"].as_array().expect("cards");
    let first = &cards[0];
    assert!(
        first["title"]
            .as_str()
            .expect("title")
            .starts_with("Findings"),
        "{first}"
    );
    let items: Vec<&str> = first["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| i.as_str().expect("item"))
        .collect();
    let find = |id: &str| {
        items
            .iter()
            .find(|i| i.starts_with(&format!("{id}: ")))
            .copied()
            .unwrap_or_else(|| panic!("a finding for {id}: {items:?}"))
    };
    assert!(find("REQ-0003").contains("withdrawn") && find("REQ-0003").contains("REQ-0004"));
    assert!(find("REQ-0003").contains("The API shall respond in at least 200 ms."));
    assert!(find("REQ-0005").contains("undecided"));
    assert!(
        find("REQ-0002").contains("REQ-0009"),
        "the dangling target is named"
    );
    assert_eq!(
        items.len(),
        3,
        "withdrawn, undecided and dangling, nothing else: {items:?}"
    );

    // Three families here (0001, 0003 and 0005 are roots), so three group cards, and no card per claim.
    let groups = &cards[1..];
    assert_eq!(groups.len(), 3, "{cards:?}");
    let listed: Vec<&str> = groups
        .iter()
        .flat_map(|g| g["items"].as_array().expect("items"))
        .map(|i| i.as_str().expect("item"))
        .collect();
    assert_eq!(listed.len(), 5, "every claim once: {listed:?}");
    assert!(listed.contains(&"REQ-0001: The service shall log every request."));

    // With nothing to report there is no findings card.
    let scratch = Scratch::new("cards-clean");
    let clean = converted(&scratch, &families());
    let titles: Vec<&str> = clean["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .map(|c| c["title"].as_str().expect("title"))
        .collect();
    assert!(
        titles.iter().all(|t| !t.starts_with("Findings")),
        "{titles:?}"
    );
    assert_eq!(titles.len(), 3, "a card per group: {titles:?}");
}

/// The index page's cards list each group page's file name, since archify has no link between pages.
#[test]
fn the_index_cards_name_each_group_page_file() {
    let scratch = Scratch::new("index-cards");
    let pages = converted_pages(&scratch, &families(), Some("5"));
    let text = pages["index"]["cards"].to_string();
    for file in [
        "family-REQ-0001.json",
        "family-REQ-0010.json",
        "standalone.json",
    ] {
        assert!(text.contains(file), "{file} is listed: {text}");
    }
}

/// A non-empty output directory is refused, naming it, so stale pages never mix with new ones; nothing is written
/// and what was there stays.
#[test]
fn a_non_empty_output_directory_is_refused_with_nothing_written() {
    let scratch = Scratch::new("nonempty");
    let input = write_fixture(&scratch, "trace.json", &fixture());
    let output = scratch.path("graph");
    fs::create_dir_all(&output).expect("the directory");
    fs::write(output.join("stale.json"), "{}").expect("a stale page");
    let ran = run(&input, &output);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
    assert!(
        ran.stderr.contains(output.to_str().expect("a path")),
        "the refusal names the directory:\n{}",
        ran.stderr
    );
    assert_eq!(
        directory_bytes(&output),
        BTreeMap::from([("stale.json".to_string(), b"{}".to_vec())]),
        "nothing was written, nothing was removed"
    );
}

/// An existing empty directory is fine: it holds nothing stale.
#[test]
fn an_existing_empty_output_directory_is_written_into() {
    let scratch = Scratch::new("empty-dir");
    let input = write_fixture(&scratch, "trace.json", &fixture());
    let output = scratch.path("graph");
    fs::create_dir_all(&output).expect("the directory");
    assert_eq!(run(&input, &output).code, 0);
    assert!(output.join("all.json").is_file());
}

/// The page-limit override is read only when set, and a value that is not a positive integer is a named refusal.
#[test]
fn a_page_limit_that_is_not_a_positive_integer_is_refused_with_nothing_written() {
    for bad in ["0", "-3", "many", ""] {
        let scratch = Scratch::new("bad-limit");
        let input = write_fixture(&scratch, "trace.json", &fixture());
        let output = scratch.path("graph");
        let ran = run_with(&input, &output, Some(bad));
        assert_eq!(ran.code, 2, "{bad:?}: {}", ran.stderr);
        assert!(ran.stderr.contains(LIMIT_VAR), "{bad:?}: {}", ran.stderr);
        assert!(!output.exists(), "{bad:?}: a refusal writes nothing");
    }
}

/// `PAGE_LIMIT` is a named constant beside the converter's others (ADR 0050 decision 3), and the sweep count too.
#[test]
fn the_page_limit_is_a_named_constant_of_150() {
    let source = fs::read_to_string(script()).expect("the converter source");
    assert!(source.contains("PAGE_LIMIT = 150"));
    assert!(source.contains("SWEEPS = "));
}

// ---------------------------------------------------------------------------
// V-2-fix (ADR 0050 decision 5, amended): wide rows wrap, rows of blocks, frames inside the canvas
// ---------------------------------------------------------------------------

/// The layout constants the converter documents, written out here as oracles: a layer wider than `WRAP` slots
/// continues on bands `PITCH_Y` apart; a canvas is at most `MAX_WIDTH` wide with both margins of `MARGIN_X`; a frame
/// pads its members by 30 px (archify's own padding), and its title sits inside that top padding.
const WRAP: usize = 7;
const MAX_WIDTH: i64 = 1710;
const MARGIN_X: i64 = 130;
const PITCH_Y: i64 = 190;
const FRAME_PAD: i64 = 30;

/// The canvas width the layout reserves: the rightmost box plus the margin.
fn canvas_width(page: &Value) -> i64 {
    boxes(page)
        .iter()
        .map(|(_, (_, _, x1, _))| *x1)
        .max()
        .expect("a box")
        + MARGIN_X
}

/// A frame as archify 7e45f36 draws it, from the rule in its source (`renderers/architecture/render-architecture.mjs`
/// `boundaryRect`, `boundaryLabelWidth`, `expandBoundaryForReadableTitle`, `measureBoundaryTitle`, and
/// `desktop-readability.mjs`), read in the scratch clone and written out here as integers:
///
/// * the members' box padded by `FRAME_PAD` on every side, and 20 px more below;
/// * a title is `units * font * 0.6 + 10` px wide (at least 30) with a 4 px inset each side, `font` being at least 6 px
///   times the canvas width over 930 (a 930 px reader), so about 11 px on a canvas of `MAX_WIDTH`;
/// * a frame narrower than its title is widened by the difference, half to each side;
/// * the title rail is `font + 7` high (at least 16), sits 4 px above the members, and the frame reaches 4 px above it.
///
/// The canvas is taken at its widest, `MAX_WIDTH`, so the font is the largest a page can see.
fn archify_frame(page: &Value, frame: &Value) -> Rect {
    let members = wraps(frame);
    let inside: Vec<Rect> = boxes(page)
        .into_iter()
        .filter(|(id, _)| members.contains(id))
        .map(|(_, r)| r)
        .collect();
    assert!(!inside.is_empty(), "a frame wraps something: {frame}");
    let (left, top, right, bottom) = (
        inside.iter().map(|r| r.0).min().expect("x0"),
        inside.iter().map(|r| r.1).min().expect("y0"),
        inside.iter().map(|r| r.2).max().expect("x1"),
        inside.iter().map(|r| r.3).max().expect("y1"),
    );
    let units = i64::try_from(frame["label"].as_str().expect("a label").chars().count())
        .expect("a short label");
    let title = ((units * 36 * MAX_WIDTH + 9299) / 9300 + 10).max(30) + 8;
    let width = right - left + 2 * FRAME_PAD;
    let extra = (title - width).max(0);
    let height = ((6 * MAX_WIDTH + 929) / 930 + 7).max(16);
    (
        left - FRAME_PAD - (extra + 1) / 2,
        top - FRAME_PAD.max(height + 8),
        left - FRAME_PAD + width + extra / 2,
        bottom + 20,
    )
}

/// Every frame, and every route point, lies inside the canvas: a frame as archify draws it (title widening and rail
/// included) starts at or past the origin, and ends, with archify's 40 px margin, within `MAX_WIDTH`.
fn assert_frames_inside_the_canvas(page: &Value) {
    for frame in boundaries(page) {
        let (x0, y0, x1, _) = archify_frame(page, frame);
        assert!(x0 >= 0 && y0 >= 0, "{frame} starts at ({x0}, {y0})");
        assert!(
            x1 + 40 <= MAX_WIDTH,
            "{frame} ends at {x1}: the canvas, with archify's margin, passes MAX_WIDTH"
        );
    }
    for c in page["connections"].as_array().expect("connections") {
        for p in c["via"].as_array().expect("via") {
            let (x, y) = (p[0].as_i64().expect("x"), p[1].as_i64().expect("y"));
            assert!(
                x >= 0 && y >= 0,
                "{c} has a route point ({x}, {y}) off the canvas"
            );
        }
    }
}

/// A binary heap of `n` claims numbered from `first`: claim `k` derives from (or, every third, refines) claim
/// `(k - 1) / 2`. The widest layer is the last, so the shape is wide for its size.
fn heap(first: usize, n: usize) -> (Vec<String>, Vec<Value>) {
    let ids: Vec<String> = (0..n).map(|k| format!("REQ-{:04}", first + k)).collect();
    let edges = (1..n)
        .map(|k| {
            let relation = if k % 3 == 0 {
                "refines"
            } else {
                "derives-from"
            };
            edge(&ids[k], relation, &ids[(k - 1) / 2])
        })
        .collect();
    (ids, edges)
}

/// Two families of 34 claims (68), one page, with a relation each way across them: the widest layer of each is 16
/// slots, past `WRAP`, so each wraps onto bands. The page is no wider than `MAX_WIDTH`, every frame lies inside the
/// canvas, and placement holds.
#[test]
fn a_wide_two_family_page_fits_max_width_and_its_frames_lie_inside_the_canvas() {
    let (a, mut edges) = heap(1, 34);
    let (b, more) = heap(101, 34);
    edges.extend(more);
    edges.push(edge(&a[20], "constrains", &b[25]));
    edges.push(edge(&b[30], "constrains", &a[15]));
    let nodes = a.iter().chain(&b).map(|i| claim(i)).collect();
    let scratch = Scratch::new("wide");
    let page = converted(&scratch, &document(json!({}), nodes, edges));

    assert_eq!(claims_on(&page).len(), 68);
    assert!(
        canvas_width(&page) <= MAX_WIDTH,
        "the canvas is {} wide",
        canvas_width(&page)
    );
    assert_eq!(boundaries(&page).len(), 2);
    assert_frames_inside_the_canvas(&page);
    assert_no_two_nodes_overlap(&page);
    assert_every_edge_is_explicitly_placed(&page);
    // No row holds more than WRAP boxes: bands are WRAP wide at most.
    let mut per_row: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    for (_, (x0, y0, _, _)) in boxes(&page) {
        let family = i64::from(x0 > 1000);
        *per_row.entry((y0, family)).or_default() += 1;
    }
    assert!(per_row.values().all(|n| *n <= WRAP), "{per_row:?}");
}

/// ADR 0050 decision 5 (amended): a layer wider than `WRAP` continues on further bands directly below, in the same
/// median order. REQ-0002 and REQ-0003 refine REQ-0001; 0015..0018 derive from 0002 and 0010..0014 from 0003. By the
/// median 0002's children come first, so the nine claims of that layer read, band by band and left to right:
/// 0015 0016 0017 0018 0010 0011 0012 / 0013 0014, the second band one row (`PITCH_Y`) below the first.
#[test]
fn a_layer_wider_than_wrap_continues_on_bands_below_in_the_same_order() {
    let mut nodes: Vec<Value> = ["0001", "0002", "0003"]
        .iter()
        .map(|n| claim(&format!("REQ-{n}")))
        .collect();
    let mut edges = vec![
        edge("REQ-0002", "refines", "REQ-0001"),
        edge("REQ-0003", "refines", "REQ-0001"),
    ];
    for n in (10..=14).chain(15..=18) {
        let id = format!("REQ-{n:04}");
        nodes.push(claim(&id));
        let parent = if n <= 14 { "REQ-0003" } else { "REQ-0002" };
        edges.push(edge(&id, "derives-from", parent));
    }
    let scratch = Scratch::new("bands");
    let page = converted(&scratch, &document(json!({}), nodes, edges));

    let mut wide: Vec<(i64, i64, String)> = boxes(&page)
        .into_iter()
        .filter(|(id, _)| id.as_str() >= "REQ-0010")
        .map(|(id, (x0, y0, _, _))| (y0, x0, id))
        .collect();
    wide.sort();
    let order: Vec<&str> = wide.iter().map(|(_, _, id)| id.as_str()).collect();
    assert_eq!(
        order,
        vec![
            "REQ-0015", "REQ-0016", "REQ-0017", "REQ-0018", "REQ-0010", "REQ-0011", "REQ-0012",
            "REQ-0013", "REQ-0014"
        ]
    );
    let rows: BTreeSet<i64> = wide.iter().map(|(y, _, _)| *y).collect();
    assert_eq!(rows.len(), 2, "two bands");
    let ys: Vec<i64> = rows.into_iter().collect();
    assert_eq!(ys[1] - ys[0], PITCH_Y, "the second band is directly below");
    assert_eq!(wide.iter().filter(|(y, _, _)| *y == ys[0]).count(), WRAP);
    assert_no_two_nodes_overlap(&page);
    assert_every_edge_is_explicitly_placed(&page);
}

/// A block that would take the page past `MAX_WIDTH` starts a new row of blocks. Eight families of three claims, each a
/// root with two children (two slots wide), fit three to a row of blocks: three rows. Relations run between the first
/// and the last family, across rows of blocks, and between two roots of the first row.
#[test]
fn blocks_that_would_pass_max_width_start_a_new_row_of_blocks() {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for k in 0..8 {
        let root = format!("REQ-{:04}", 10 * k + 1);
        nodes.push(claim(&root));
        for c in 2..=3 {
            let id = format!("REQ-{:04}", 10 * k + c);
            nodes.push(claim(&id));
            edges.push(edge(&id, "refines", &root));
        }
    }
    edges.push(edge("REQ-0001", "constrains", "REQ-0072"));
    edges.push(edge("REQ-0071", "constrains", "REQ-0003"));
    // Two roots on the first row: its route runs in the channel above row 0, which the top margin must hold.
    edges.push(edge("REQ-0001", "constrains", "REQ-0011"));
    let scratch = Scratch::new("block-rows");
    let page = converted(&scratch, &document(json!({}), nodes, edges));

    assert!(canvas_width(&page) <= MAX_WIDTH, "{}", canvas_width(&page));
    let root_rows: BTreeSet<i64> = (0..8)
        .map(|k| {
            component(&page, &format!("REQ-{:04}", 10 * k + 1))["pos"][1]
                .as_i64()
                .expect("y")
        })
        .collect();
    assert_eq!(root_rows.len(), 3, "three rows of blocks: {root_rows:?}");
    assert_eq!(boundaries(&page).len(), 8);
    assert_frames_inside_the_canvas(&page);
    assert_no_two_nodes_overlap(&page);
    assert_every_edge_is_explicitly_placed(&page);
}

/// The dangling target of a small document's page is drawn outside every frame.
#[test]
fn a_dangling_target_is_drawn_outside_every_family_frame() {
    let scratch = Scratch::new("dangling-outside");
    let mut document = families();
    document["edges"].as_array_mut().expect("edges").push(edge(
        "REQ-0003",
        "constrains",
        "REQ-0099",
    ));
    document["findings"]["dangling"] =
        json!(["constrains -> REQ-0099 (referenced by REQ-0003, not in the corpus)"]);
    let page = converted(&scratch, &document);
    let target = component(&page, "REQ-0099");
    assert_eq!(target["tag"], DANGLING_TAG);
    assert!(
        boundaries(&page)
            .iter()
            .all(|b| !wraps(b).contains("REQ-0099")),
        "no frame wraps it"
    );
    let (x0, y0, x1, y1) = (
        target["pos"][0].as_i64().expect("x"),
        target["pos"][1].as_i64().expect("y"),
        target["pos"][0].as_i64().expect("x") + 130,
        target["pos"][1].as_i64().expect("y") + 64,
    );
    for frame in boundaries(&page) {
        assert!(
            !overlaps((x0, y0, x1, y1), archify_frame(&page, frame)),
            "REQ-0099 lies inside the frame {frame}"
        );
    }
    assert_eq!(
        connection(&page, "REQ-0003", "constrains", "REQ-0099")["variant"],
        "security"
    );
    assert_every_edge_is_explicitly_placed(&page);
}

/// A dangling far end on a split page: the edge is drawn on its claim's page, the target is tagged as ADR 0046 says,
/// sits in no frame, and is not counted as a claim (falsification 2 still holds).
#[test]
fn a_dangling_far_end_on_a_split_page_is_drawn_and_is_not_a_claim() {
    let scratch = Scratch::new("dangling-split");
    let mut document = families();
    document["edges"].as_array_mut().expect("edges").push(edge(
        "REQ-0003",
        "constrains",
        "REQ-0099",
    ));
    document["findings"]["dangling"] =
        json!(["constrains -> REQ-0099 (referenced by REQ-0003, not in the corpus)"]);
    let pages = converted_pages(&scratch, &document, Some("5"));
    assert_edges_are_drawn_or_stubbed(&document, &pages, &family_page);
    let page = &pages["family-REQ-0001"];
    assert_eq!(component(page, "REQ-0099")["tag"], DANGLING_TAG);
    assert!(!claims_on(page).contains("REQ-0099"));
    assert_eq!(claims_on(page), names(&FAMILY_A));
    // Stubs carry the ADR's tag, word for word.
    assert_eq!(component(page, "REQ-0012")["tag"], "on another page");
    assert_eq!(component(page, "REQ-0099")["tag"], "not in the corpus");
}

/// A family whose claims form a cycle with no root outside it is named after its lowest-id member, and a cycle that
/// leads out to a root belongs to that root's family. 0001 <- 0002 <- 0003 <- 0001 is one family, REQ-0001 (the layout
/// calls 0003 its root); 0020 and 0021 derive from each other and 0020 also from 0010, so both are family REQ-0010.
#[test]
fn a_cycle_with_no_root_outside_it_is_named_after_its_lowest_id_member() {
    let nodes = ["0001", "0002", "0003", "0010", "0020", "0021"]
        .iter()
        .map(|n| claim(&format!("REQ-{n}")))
        .collect();
    let edges = vec![
        edge("REQ-0001", "derives-from", "REQ-0002"),
        edge("REQ-0002", "derives-from", "REQ-0003"),
        edge("REQ-0003", "derives-from", "REQ-0001"),
        edge("REQ-0020", "derives-from", "REQ-0010"),
        edge("REQ-0020", "derives-from", "REQ-0021"),
        edge("REQ-0021", "derives-from", "REQ-0020"),
    ];
    let scratch = Scratch::new("cycle-family");
    let document = document(json!({}), nodes, edges);
    let page = converted(&scratch, &document);
    let by_label: BTreeMap<&str, BTreeSet<String>> = boundaries(&page)
        .iter()
        .map(|b| (b["label"].as_str().expect("label"), wraps(b)))
        .collect();
    assert_eq!(
        by_label,
        BTreeMap::from([
            ("REQ-0001", names(&["REQ-0001", "REQ-0002", "REQ-0003"])),
            ("REQ-0010", names(&["REQ-0010", "REQ-0020", "REQ-0021"])),
        ])
    );
    // And the same on a split run: the page names follow the family names.
    let scratch = Scratch::new("cycle-family-split");
    let pages = converted_pages(&scratch, &document, Some("3"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["family-REQ-0001", "family-REQ-0010", "index"]
    );
}

/// The index is capped like every page: more groups than the limit are cut into parts that declare themselves. Seven
/// two-claim families and a limit of three give `index` and two more parts; every group is on exactly one part, and a
/// connection to a group on another part ends on a stub naming that part.
#[test]
fn an_index_past_the_limit_is_cut_into_parts_that_declare_themselves() {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for k in 0..7 {
        let root = format!("REQ-{:04}", 10 * k + 1);
        let child = format!("REQ-{:04}", 10 * k + 2);
        nodes.push(claim(&root));
        nodes.push(claim(&child));
        edges.push(edge(&child, "refines", &root));
    }
    // Family 1 (part 1) to family 7 (part 3); family 4 and 5 are both on part 2.
    edges.push(edge("REQ-0001", "constrains", "REQ-0061"));
    edges.push(edge("REQ-0031", "constrains", "REQ-0041"));
    let scratch = Scratch::new("index-parts");
    let pages = converted_pages(&scratch, &document(json!({}), nodes, edges), Some("3"));
    let index_names = ["index", "index-part-2", "index-part-3"];
    for name in index_names {
        assert!(pages.contains_key(name), "{name} is written");
    }
    assert_eq!(
        pages.len(),
        3 + 7,
        "three index pages and a page per family"
    );

    let mut seen = BTreeSet::new();
    for (k, name) in index_names.iter().enumerate() {
        let page = &pages[*name];
        let groups: BTreeSet<String> = components(page)
            .iter()
            .filter(|c| c["tag"] != STUB_TAG)
            .map(|c| c["id"].as_str().expect("id").to_string())
            .collect();
        assert!(groups.len() <= 3, "{name} holds {} groups", groups.len());
        assert!(seen.is_disjoint(&groups), "{name} repeats a group");
        seen.extend(groups);
        assert!(
            subtitle(page).contains(&format!("part {} of 3", k + 1)),
            "{name}: {}",
            subtitle(page)
        );
        assert!(
            subtitle(page).contains("index of 7 groups"),
            "{}",
            subtitle(page)
        );
    }
    assert_eq!(seen.len(), 7, "every group on exactly one index page");

    // 0001 constrains 0061: group 1 is on `index`, group 7 on `index-part-3`.
    let first = &pages["index"];
    connection(
        first,
        "family-REQ-0001",
        "constrains \u{d7}1",
        "family-REQ-0061",
    );
    let stub = component(first, "family-REQ-0061");
    assert_eq!(stub["tag"], STUB_TAG);
    let frame = boundaries(first)
        .into_iter()
        .find(|b| wraps(b).contains("family-REQ-0061"))
        .expect("the stub's frame");
    assert!(
        frame["label"]
            .as_str()
            .expect("label")
            .contains("index-part-3")
    );
    let last = &pages["index-part-3"];
    connection(
        last,
        "family-REQ-0001",
        "constrains \u{d7}1",
        "family-REQ-0061",
    );
    assert_eq!(component(last, "family-REQ-0001")["tag"], STUB_TAG);
    // A connection inside one part is plain, with no stub.
    let second = &pages["index-part-2"];
    connection(
        second,
        "family-REQ-0031",
        "constrains \u{d7}1",
        "family-REQ-0041",
    );
    assert!(boundaries(second).is_empty());
    assert_every_edge_is_explicitly_placed(first);
}

/// `WRAP` and `MAX_WIDTH` are named constants beside the converter's others, and the converter's own comment states the
/// measured floor they come from.
#[test]
fn wrap_and_max_width_are_named_constants() {
    let source = fs::read_to_string(script()).expect("the converter source");
    assert!(source.contains("WRAP = 7"));
    assert!(source.contains("MAX_WIDTH = 1710"));
}

/// A long title widens its frame past its members, and a frame on the left edge needs that room inside the margin (the
/// failure V-2 had on a wide page, where the title scales with the canvas). Root ids of 18 characters, about the longest
/// archify draws in a node box, make a stub title of `page family-` and the id: the frame is widened, towards both
/// sides, and must still start inside the canvas. Longer ids are refused by archify itself (`component-label-overflow`),
/// so this is the widest title a page can carry.
#[test]
fn a_long_frame_title_does_not_push_its_frame_off_the_canvas() {
    let long = format!("REQ-{}", "A".repeat(14));
    let child = format!("{long}1");
    let document = document(
        json!({}),
        vec![
            claim("REQ-0002"),
            claim("REQ-0003"),
            claim(&long),
            claim(&child),
        ],
        vec![
            edge("REQ-0003", "refines", "REQ-0002"),
            edge(&child, "refines", &long),
            edge("REQ-0003", "constrains", &child),
        ],
    );
    // One page: the family frames carry the long root id as their title.
    let scratch = Scratch::new("long-title-all");
    let page = converted(&scratch, &document);
    assert_eq!(boundaries(&page).len(), 2);
    assert_frames_inside_the_canvas(&page);

    // Split: the stub on the first family's page sits in a frame titled `page family-REQ-AAAA...`.
    let scratch = Scratch::new("long-title-split");
    let pages = converted_pages(&scratch, &document, Some("3"));
    let frame = boundaries(&pages["family-REQ-0002"])
        .into_iter()
        .next()
        .expect("a stub frame");
    assert!(frame["label"].as_str().expect("label").chars().count() >= 30);
    // The index names a group by its root id, which fits a node box; "family <id>" would not.
    assert_eq!(
        component(&pages["index"], &format!("family-{long}"))["label"],
        long.as_str()
    );
    for page in group_pages(&pages).values() {
        assert_frames_inside_the_canvas(page);
        assert_no_two_nodes_overlap(page);
        assert_every_edge_is_explicitly_placed(page);
    }
}

// ---------------------------------------------------------------------------
// A-2 (ADR 0051 decision 5): the graph groups by area when any claim carries one
// ---------------------------------------------------------------------------

/// A claim filed under `area` (`None` is a claim with no area: the node's `area` is `null`, as `req.trace/1` writes it).
fn filed(id: &str, area: Option<&str>) -> Value {
    let mut node = claim(id);
    node["area"] = area.map_or(Value::Null, |a| json!(a));
    node
}

/// A document with the registry's lines, `(path, label)`, in the order given: that order is the reading order.
fn with_areas(mut document: Value, registry: &[(&str, &str)]) -> Value {
    document["areas"] = Value::Array(
        registry
            .iter()
            .map(|(path, label)| json!({"path": path, "label": label}))
            .collect(),
    );
    document
}

/// The registry of `by_area()`, in an order that is **not** the path's sort order (`diagnostics` sorts first).
const REGISTRY: [(&str, &str); 3] = [
    ("interfaces", "Interfaces"),
    ("power", "Power management"),
    ("diagnostics", "Diagnostics & logging"),
];

const INTERFACES: [&str; 3] = ["REQ-0001", "REQ-0002", "REQ-0003"];
const POWER: [&str; 2] = ["REQ-0010", "REQ-0011"];
const DIAGNOSTICS: [&str; 3] = ["REQ-0020", "REQ-0021", "REQ-0022"];
const NO_AREA: [&str; 2] = ["REQ-0030", "REQ-0031"];

/// The page that holds a claim of `by_area()` when the limit splits it, written out by hand.
fn area_page(id: &str) -> &'static str {
    if INTERFACES.contains(&id) {
        "area-interfaces"
    } else if POWER.contains(&id) {
        "area-power"
    } else if DIAGNOSTICS.contains(&id) {
        "area-diagnostics"
    } else {
        "no-area"
    }
}

/// Ten claims, hand-written: three areas the registry declares, in the order interfaces, power, diagnostics, and two
/// claims with no area.
///
/// * interfaces: 0001 (root), 0002 refines it, 0003 derives from it;
/// * power: 0010 (root), 0011 derives from it;
/// * diagnostics: 0020 (root), 0021 refines it, 0022 has no edge at all and is undecided in this selection;
/// * no area: 0030 has no edge, 0031's only edge leaves its group;
/// * three edges cross groups: 0011 constrains 0002, 0021 derives from 0003 (a `derives-from` that does not lie inside
///   its group, so it does not lay anything out) and 0031 derives from 0001.
fn by_area() -> Value {
    let mut nodes = vec![
        filed("REQ-0001", Some("interfaces")),
        filed("REQ-0002", Some("interfaces")),
        filed("REQ-0003", Some("interfaces")),
        filed("REQ-0010", Some("power")),
        filed("REQ-0011", Some("power")),
        filed("REQ-0020", Some("diagnostics")),
        filed("REQ-0021", Some("diagnostics")),
        filed("REQ-0022", Some("diagnostics")),
        filed("REQ-0030", None),
        filed("REQ-0031", None),
    ];
    nodes[7]["applies"] = json!("unknown");
    with_areas(
        document(
            json!({"deployment": "cloud"}),
            nodes,
            vec![
                edge("REQ-0002", "refines", "REQ-0001"),
                edge("REQ-0003", "derives-from", "REQ-0001"),
                edge("REQ-0011", "derives-from", "REQ-0010"),
                edge("REQ-0021", "refines", "REQ-0020"),
                edge("REQ-0011", "constrains", "REQ-0002"),
                edge("REQ-0021", "derives-from", "REQ-0003"),
                edge("REQ-0031", "derives-from", "REQ-0001"),
            ],
        ),
        &REGISTRY,
    )
}

fn frame_labels(page: &Value) -> Vec<String> {
    boundaries(page)
        .iter()
        .map(|b| b["label"].as_str().expect("a label").to_string())
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

/// A frame as the box of its members padded by archify's 30 px, enough to say two frames do not overlap.
fn padded(page: &Value, frame: &Value) -> Rect {
    let members = wraps(frame);
    let inside: Vec<Rect> = boxes(page)
        .into_iter()
        .filter(|(id, _)| members.contains(id))
        .map(|(_, r)| r)
        .collect();
    (
        inside.iter().map(|r| r.0).min().expect("x0") - FRAME_PAD,
        inside.iter().map(|r| r.1).min().expect("y0") - FRAME_PAD,
        inside.iter().map(|r| r.2).max().expect("x1") + FRAME_PAD,
        inside.iter().map(|r| r.3).max().expect("y1") + FRAME_PAD,
    )
}

/// ADR 0051 decision 5: with any area on any claim, the groups are the areas. One page, a `region` frame per area named by
/// its **label**, in the registry's line order, then `no area`; no family frame; a focus view per group.
#[test]
fn one_page_by_area_has_a_frame_per_area_named_by_its_label_in_registry_order_then_no_area() {
    let scratch = Scratch::new("area-one-page");
    let page = converted(&scratch, &by_area());

    assert_eq!(
        frame_labels(&page),
        strings(&[
            "Interfaces",
            "Power management",
            "Diagnostics & logging",
            "no area"
        ]),
        "labels, in the registry's order (not the path order), and `no area` last: {page}"
    );
    let by_label: BTreeMap<String, BTreeSet<String>> = boundaries(&page)
        .iter()
        .map(|b| (b["label"].as_str().expect("label").to_string(), wraps(b)))
        .collect();
    assert_eq!(by_label["Interfaces"], names(&INTERFACES));
    assert_eq!(by_label["Power management"], names(&POWER));
    assert_eq!(by_label["Diagnostics & logging"], names(&DIAGNOSTICS));
    assert_eq!(by_label["no area"], names(&NO_AREA));
    assert!(
        boundaries(&page).iter().all(|b| b["kind"] == "region"),
        "every frame is a region"
    );

    // No family is computed or shown while areas are the key.
    for root in ["REQ-0001", "REQ-0010", "REQ-0020"] {
        assert!(
            !frame_labels(&page).contains(&root.to_string()),
            "{root} names no frame"
        );
    }
    assert!(
        !subtitle(&page).contains("famil"),
        "{}",
        subtitle(&page)
    );

    // The focus views follow the groups, in group order.
    let views = page["meta"]["views"].as_array().expect("views");
    let focuses: Vec<BTreeSet<String>> = views
        .iter()
        .map(|v| {
            v["focus"]
                .as_array()
                .expect("focus")
                .iter()
                .map(|f| f.as_str().expect("id").to_string())
                .collect()
        })
        .collect();
    assert_eq!(
        focuses,
        vec![
            names(&INTERFACES),
            names(&POWER),
            names(&DIAGNOSTICS),
            names(&NO_AREA)
        ],
        "a focus view per group, at most five groups"
    );
    assert_eq!(views[0]["label"], "Interfaces");
    assert_eq!(views[3]["label"], "no area");

    // The findings card first, then one card per group in group order.
    let titles: Vec<&str> = page["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .map(|c| c["title"].as_str().expect("a title"))
        .collect();
    assert_eq!(
        titles,
        vec![
            "Findings (1)",
            "Interfaces (3 claims)",
            "Power management (2 claims)",
            "Diagnostics & logging (3 claims)",
            "no area (2 claims)"
        ]
    );
    assert!(
        subtitle(&page).contains("all claims") && subtitle(&page).contains("10 of 10 claims"),
        "{}",
        subtitle(&page)
    );
}

/// ADR 0051 decision 5 and 0050 decision 5: a group is laid out over the edges that lie inside it; a claim with none sits
/// in the group's own grid, inside its frame, and frames of different groups never overlap.
#[test]
fn inside_an_area_the_layered_method_runs_over_its_own_edges_and_a_loose_claim_sits_in_the_groups_grid() {
    let scratch = Scratch::new("area-inside");
    let page = converted(&scratch, &by_area());
    let at = |id: &str| {
        let (_, (x, y, _, _)) = boxes(&page)
            .into_iter()
            .find(|(i, _)| i == id)
            .unwrap_or_else(|| panic!("{id} is drawn"));
        (x, y)
    };
    // interfaces: 0001 is a layer above 0002 and 0003 (both below it).
    assert!(at("REQ-0001").1 < at("REQ-0002").1 && at("REQ-0001").1 < at("REQ-0003").1);
    // power: 0011 is below 0010.
    assert!(at("REQ-0010").1 < at("REQ-0011").1);
    // diagnostics: 0021 is below 0020 over their own edge, and 0022, with no edge in its group, is in the grid under both.
    assert!(at("REQ-0020").1 < at("REQ-0021").1, "0021 refines 0020");
    assert!(
        at("REQ-0022").1 > at("REQ-0020").1 && at("REQ-0022").1 > at("REQ-0021").1,
        "a claim with no edge inside its group sits in the group's grid, below the layers"
    );
    // 0021's `derives-from` to 0003 crosses groups: it does not put 0021 a layer below a claim of another group.
    assert!(
        at("REQ-0021").1 <= at("REQ-0020").1 + PITCH_Y,
        "a cross-group edge lays nothing out"
    );
    // The frames of two groups never overlap.
    let frames = boundaries(&page);
    for (i, a) in frames.iter().enumerate() {
        for b in &frames[i + 1..] {
            assert!(
                !overlaps(padded(&page, a), padded(&page, b)),
                "{a} overlaps {b}"
            );
        }
    }
    assert_no_two_nodes_overlap(&page);
    assert_every_edge_is_explicitly_placed(&page);
    assert_frames_inside_the_canvas(&page);
    // A cross-group edge on one page is a drawn edge between the two claims, not a stub.
    connection(&page, "REQ-0011", "constrains", "REQ-0002");
    assert!(components(&page).iter().all(|c| c["type"] != "cloud"));
}

/// Mutation anchor: a claim at `diagnostics` and one at `diagnostics/startup` are two groups, and each claim is in
/// exactly one (ADR 0051 *Falsification* 2).
#[test]
fn a_parent_area_and_its_child_are_two_groups_and_each_claim_is_in_exactly_one() {
    let document = with_areas(
        document(
            json!({}),
            vec![
                filed("REQ-0001", Some("diagnostics")),
                filed("REQ-0002", Some("diagnostics/startup")),
                filed("REQ-0003", Some("diagnostics/startup")),
                filed("REQ-0004", Some("diagnostics")),
            ],
            vec![
                edge("REQ-0002", "refines", "REQ-0001"),
                edge("REQ-0003", "refines", "REQ-0002"),
            ],
        ),
        &[("diagnostics", "Diagnostics"), ("diagnostics/startup", "Start-up")],
    );
    let scratch = Scratch::new("area-parent-child");
    let page = converted(&scratch, &document);
    let frames = boundaries(&page);
    assert_eq!(frame_labels(&page), strings(&["Diagnostics", "Start-up"]));
    assert_eq!(wraps(frames[0]), names(&["REQ-0001", "REQ-0004"]));
    assert_eq!(wraps(frames[1]), names(&["REQ-0002", "REQ-0003"]));
    for id in ["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"] {
        let holding = frames.iter().filter(|f| wraps(f).contains(id)).count();
        assert_eq!(holding, 1, "{id} is in exactly one group");
    }

    // Split: two group pages, the child's file named with `.` for `/`, and every claim on exactly one of them.
    let scratch = Scratch::new("area-parent-child-split");
    let pages = converted_pages(&scratch, &document, Some("3"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["area-diagnostics", "area-diagnostics.startup", "index"]
    );
    assert_eq!(
        claims_on(&pages["area-diagnostics"]),
        names(&["REQ-0001", "REQ-0004"])
    );
    assert_eq!(
        claims_on(&pages["area-diagnostics.startup"]),
        names(&["REQ-0002", "REQ-0003"])
    );
    // archify's id grammar has no `.`: no id on any page may carry the page file's.
    let grammar = |id: &str| {
        let mut chars = id.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    for (name, page) in &pages {
        for c in components(page) {
            let id = c["id"].as_str().expect("an id");
            assert!(grammar(id), "{name}: `{id}` is outside archify's id grammar");
        }
    }
    let labels: BTreeSet<&str> = components(&pages["index"])
        .iter()
        .map(|c| c["label"].as_str().expect("label"))
        .collect();
    assert_eq!(
        labels,
        BTreeSet::from(["diagnostics", "startup"]),
        "an index node is labelled with the path's last segment"
    );
}

/// An undeclared area is named by its path and ordered after the declared areas, in path order, before `no area`; the
/// registry does not have to hold it (ADR 0051 decision 3).
#[test]
fn an_undeclared_area_is_named_by_its_path_and_ordered_after_the_declared_ones() {
    let document = with_areas(
        document(
            json!({}),
            vec![
                filed("REQ-0001", Some("interfaces")),
                filed("REQ-0002", Some("zeta")),
                filed("REQ-0003", Some("alpha/beta")),
                filed("REQ-0004", None),
            ],
            vec![],
        ),
        &[("interfaces", "Interfaces")],
    );
    let scratch = Scratch::new("area-undeclared");
    let page = converted(&scratch, &document);
    assert_eq!(
        frame_labels(&page),
        strings(&["Interfaces", "alpha/beta", "zeta", "no area"]),
        "declared first, then undeclared by path (`alpha/beta` sorts before `zeta`, and before `interfaces`)"
    );

    let scratch = Scratch::new("area-undeclared-split");
    let pages = converted_pages(&scratch, &document, Some("1"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "area-alpha.beta",
            "area-interfaces",
            "area-zeta",
            "index",
            "no-area"
        ]
    );
    // The index lists the group pages in group order, naming each page file and group.
    let items: Vec<String> = pages["index"]["cards"][0]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| i.as_str().expect("item").to_string())
        .collect();
    let files: Vec<&str> = items
        .iter()
        .map(|i| i.split(':').next().expect("a file"))
        .collect();
    assert_eq!(
        files,
        vec![
            "area-interfaces.json",
            "area-alpha.beta.json",
            "area-zeta.json",
            "no-area.json"
        ]
    );
}

/// ADR 0050 decision 3 applies to areas: past the limit, one page per group plus an index; every claim on exactly one
/// group page, and a cross-area edge ends in a stub naming the target's page.
#[test]
fn split_by_area_gives_a_page_per_area_an_index_every_claim_once_and_stubs_naming_the_other_page() {
    let scratch = Scratch::new("area-split");
    let document = by_area();
    let pages = converted_pages(&scratch, &document, Some("4"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "area-diagnostics",
            "area-interfaces",
            "area-power",
            "index",
            "no-area"
        ]
    );
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (name, page) in group_pages(&pages) {
        let on_page = claims_on(page);
        assert!(on_page.len() <= 4, "{name} holds {} claims", on_page.len());
        for id in on_page {
            assert_eq!(area_page(&id), name, "{id} is on its area's page");
            *seen.entry(id).or_default() += 1;
        }
    }
    let expected: BTreeMap<String, usize> = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| (n["claim_id"].as_str().expect("id").to_string(), 1))
        .collect();
    assert_eq!(seen, expected, "every claim on exactly one group page");
    assert_edges_are_drawn_or_stubbed(&document, &pages, &area_page);

    // The spec's own example, by hand: 0011 (power) constrains 0002 (interfaces).
    let power = &pages["area-power"];
    connection(power, "REQ-0011", "constrains", "REQ-0002");
    let stub = component(power, "REQ-0002");
    assert_eq!(stub["type"], "cloud");
    let frame = boundaries(power)
        .into_iter()
        .find(|b| wraps(b).contains("REQ-0002"))
        .expect("the stub's frame");
    assert!(
        frame["label"].as_str().expect("label").contains("area-interfaces"),
        "{frame}"
    );

    // A group page speaks the group's name: the title, the subtitle and the card.
    assert_eq!(power["meta"]["title"], "Requirement graph: Power management");
    assert!(subtitle(power).contains("Power management"), "{}", subtitle(power));
    assert!(subtitle(power).contains("2 of 10 claims"), "{}", subtitle(power));
    assert_eq!(power["cards"][0]["title"], "Power management (2 claims)");
    assert_eq!(
        pages["area-diagnostics"]["cards"][0]["title"], "Findings (1)",
        "the findings card comes first"
    );
    assert_eq!(
        pages["area-diagnostics"]["cards"][1]["title"],
        "Diagnostics & logging (3 claims)"
    );

    // The index: a node per group, in group order, each labelled by its path's last segment, and a connection per
    // ordered pair of groups and relation.
    let index = &pages["index"];
    let labels: Vec<&str> = components(index)
        .iter()
        .map(|c| c["label"].as_str().expect("label"))
        .collect();
    assert_eq!(labels, vec!["interfaces", "power", "diagnostics", "no area"]);
    let drawn: BTreeSet<(String, String, String)> = connections_of(index)
        .iter()
        .map(|c| {
            (
                c["from"].as_str().expect("from").to_string(),
                c["to"].as_str().expect("to").to_string(),
                c["label"].as_str().expect("label").to_string(),
            )
        })
        .collect();
    let wanted: BTreeSet<(String, String, String)> = [
        ("area-power", "area-interfaces", "constrains \u{d7}1"),
        ("area-diagnostics", "area-interfaces", "derives-from \u{d7}1"),
        ("no-area", "area-interfaces", "derives-from \u{d7}1"),
    ]
    .iter()
    .map(|(a, b, c)| ((*a).to_string(), (*b).to_string(), (*c).to_string()))
    .collect();
    assert_eq!(drawn, wanted, "one connection per group pair and relation");
    assert_eq!(connections_of(index).len(), 3);
}

/// An area past the limit is cut into parts by layer, as a family is, and no page exceeds the limit; the parts keep V-2's
/// `-part-<i>` suffix.
#[test]
fn an_oversized_area_is_cut_into_parts_by_layer_and_no_page_exceeds_the_limit() {
    let mut document = oversized_family();
    for n in document["nodes"].as_array_mut().expect("nodes") {
        n["area"] = json!("core");
    }
    let document = with_areas(document, &[("core", "Core")]);
    let scratch = Scratch::new("area-parts");
    let pages = converted_pages(&scratch, &document, Some("5"));
    assert_eq!(
        pages.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "area-core",
            "area-core-part-2",
            "area-core-part-3",
            "index"
        ]
    );
    assert_eq!(claims_on(&pages["area-core"]), names(&PART_1));
    assert_eq!(claims_on(&pages["area-core-part-2"]), names(&PART_2));
    assert_eq!(claims_on(&pages["area-core-part-3"]), names(&PART_3));
    for (i, name) in ["area-core", "area-core-part-2", "area-core-part-3"]
        .iter()
        .enumerate()
    {
        let page = &pages[*name];
        assert!(claims_on(page).len() <= 5, "{name}");
        assert!(
            subtitle(page).contains(&format!("part {} of 3", i + 1)),
            "{name}: {}",
            subtitle(page)
        );
        assert!(subtitle(page).contains("Core"), "{name}: {}", subtitle(page));
    }
    let part = |id: &str| -> &'static str {
        if PART_1.contains(&id) {
            "area-core"
        } else if PART_2.contains(&id) {
            "area-core-part-2"
        } else {
            "area-core-part-3"
        }
    };
    assert_edges_are_drawn_or_stubbed(&document, &pages, &part);
}

/// With no area on any claim, every byte is today's family output, whether the nodes say `area: null` and the document
/// holds `areas: []`, or the document holds a registry no claim uses.
#[test]
fn with_no_area_anywhere_the_output_is_byte_identical_to_the_family_grouping() {
    let scratch = Scratch::new("area-none");
    let plain = families();
    let mut nulled = families();
    for n in nulled["nodes"].as_array_mut().expect("nodes") {
        n["area"] = Value::Null;
    }
    let empty = with_areas(nulled.clone(), &[]);
    let unused = with_areas(nulled, &[("interfaces", "Interfaces")]);
    for (limit, tag) in [(None, "one"), (Some("5"), "split")] {
        let reference = scratch.path(&format!("plain-{tag}"));
        let input = write_fixture(&scratch, &format!("plain-{tag}.json"), &plain);
        assert_eq!(run_with(&input, &reference, limit).code, 0);
        let expected = directory_bytes(&reference);
        assert!(!expected.is_empty());
        for (name, document) in [("empty", &empty), ("unused", &unused)] {
            let input = write_fixture(&scratch, &format!("{name}-{tag}.json"), document);
            let out = scratch.path(&format!("{name}-{tag}"));
            assert_eq!(run_with(&input, &out, limit).code, 0);
            assert_eq!(
                directory_bytes(&out),
                expected,
                "{name}/{tag}: no claim carries an area, so the groups are families and nothing moves"
            );
        }
    }
}

/// Mutation anchor: **any** claim carrying an area is enough: claims with none do not send the page back to families.
#[test]
fn one_claim_with_an_area_among_many_without_is_enough_to_group_by_area() {
    let mut document = families();
    document["nodes"][0]["area"] = json!("interfaces");
    let document = with_areas(document, &[("interfaces", "Interfaces")]);
    let scratch = Scratch::new("area-one");
    let page = converted(&scratch, &document);
    assert_eq!(frame_labels(&page), strings(&["Interfaces", "no area"]));
}

/// The widest label a node box holds, from archify's rule at 7e45f36 (read in a scratch clone, written out here): a label
/// is `units * 6.6` px against the box width plus 8, so 20 units in a 130 px box; a tag or sublabel is `units * 3.6` px
/// against the width minus 8, so 33. A unit is one column, two for a wide (East Asian or emoji) character.
const LABEL_UNITS: i64 = 20;
const TAG_UNITS: i64 = 33;
/// The converter's own bound on a frame title, in the same units: a title that wide does not widen a one-box frame past the
/// 13 px the margin holds (ADR 0050's note), so no frame leaves the canvas.
const FRAME_TITLE_UNITS: i64 = 30;

fn units(text: &str) -> i64 {
    text.chars()
        .map(|c| match u32::from(c) {
            0x1100..=0x115F
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE6F
            | 0xFF01..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F000..=0x1FAFF
            | 0x20000..=0x3FFFD => 2,
            _ => 1,
        })
        .sum()
}

const LONG_LATIN: &str = "Démarrage du système et journal des défauts";
const LONG_CJK: &str = "診断・起動・障害記録の管理に関する要件一式（拡張版）";
const LONG_ASCII: &str = "Operations, maintenance and field-service procedures for fielded units in cold-weather deployments";

/// Four areas whose labels are longer than any node box or frame title holds, in two scripts and in one long ASCII
/// sentence, and a cross-area edge.
fn long_labels() -> Value {
    with_areas(
        document(
            json!({}),
            vec![
                filed("REQ-0001", Some("diagnostics")),
                filed("REQ-0002", Some("diagnostics")),
                filed("REQ-0003", Some("interfaces/can-bus")),
                filed("REQ-0004", Some("power")),
                filed("REQ-0005", Some("ops")),
            ],
            vec![
                edge("REQ-0002", "refines", "REQ-0001"),
                edge("REQ-0003", "constrains", "REQ-0001"),
                edge("REQ-0005", "informs", "REQ-0004"),
            ],
        ),
        &[
            ("diagnostics", LONG_LATIN),
            ("interfaces/can-bus", LONG_CJK),
            ("power", "Power"),
            ("ops", LONG_ASCII),
        ],
    )
}

/// archify refuses a label wider than its box (`component-label-overflow`) and widens a frame to its title. A registry
/// label is free text in any language, so a node or frame title never carries one it cannot fit; the full name is in the
/// page's title, subtitle, cards and (up to 48 characters) focus views. Mutation anchor: the full label on an index node.
#[test]
fn a_long_non_ascii_label_renders_through_the_index_and_a_group_page_without_overflow() {
    let document = long_labels();

    // One page: frame titles are bounded, the full names are in the cards, and no frame leaves the canvas.
    let scratch = Scratch::new("area-long-one");
    let page = converted(&scratch, &document);
    for frame in boundaries(&page) {
        let label = frame["label"].as_str().expect("label");
        assert!(
            units(label) <= FRAME_TITLE_UNITS,
            "the frame title `{label}` is {} units wide",
            units(label)
        );
    }
    let titles: Vec<&str> = page["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .map(|c| c["title"].as_str().expect("title"))
        .collect();
    for full in [LONG_LATIN, LONG_CJK, LONG_ASCII] {
        assert!(
            titles.iter().any(|t| t.starts_with(full)),
            "the card keeps the whole name `{full}`: {titles:?}"
        );
    }
    let views = page["meta"]["views"].as_array().expect("views");
    assert_eq!(views.len(), 4);
    for v in views {
        assert!(
            v["label"].as_str().expect("label").chars().count() <= 48,
            "archify's schema caps a view label at 48: {v}"
        );
    }
    assert_eq!(views[0]["label"], LONG_LATIN, "a name that fits stays whole");
    assert_frames_inside_the_canvas(&page);
    assert_no_two_nodes_overlap(&page);

    // Split: the index and the group pages.
    let scratch = Scratch::new("area-long-split");
    let pages = converted_pages(&scratch, &document, Some("3"));
    for (name, page) in &pages {
        for c in components(page) {
            let label = c["label"].as_str().expect("label");
            assert!(
                units(label) <= LABEL_UNITS,
                "{name}: `{label}` is {} units, past what a 130 px box holds",
                units(label)
            );
            if let Some(tag) = c["tag"].as_str() {
                assert!(
                    units(tag) <= TAG_UNITS,
                    "{name}: the tag `{tag}` is {} units",
                    units(tag)
                );
            }
        }
        for frame in boundaries(page) {
            let label = frame["label"].as_str().expect("label");
            assert!(units(label) <= FRAME_TITLE_UNITS, "{name}: `{label}`");
        }
        assert_frames_inside_the_canvas(page);
    }
    let index = &pages["index"];
    let by_id: BTreeMap<&str, &Value> = components(index)
        .iter()
        .map(|c| (c["id"].as_str().expect("id"), c))
        .collect();
    assert_eq!(by_id.len(), 4, "a node per group");
    // The node is labelled by the path's last segment, and the name rides in its tag, cut where the tag ends.
    let diagnostics = by_id["area-diagnostics"];
    assert_eq!(diagnostics["label"], "diagnostics");
    let tag = diagnostics["tag"].as_str().expect("tag");
    let stem = tag.strip_suffix('\u{2026}').expect("a cut name ends in an ellipsis");
    assert!(LONG_LATIN.starts_with(stem), "`{tag}` is the start of the name");
    let can_bus = by_id["area-interfaces_can-bus"];
    assert_eq!(can_bus["label"], "can-bus");
    assert!(
        can_bus["tag"].as_str().expect("tag").starts_with("診断"),
        "{can_bus}"
    );
    // The whole name is on the index, in its card.
    let card = serde_json::to_string(&index["cards"]).expect("cards");
    for full in [LONG_LATIN, LONG_CJK, LONG_ASCII] {
        assert!(card.contains(full), "the index card holds `{full}`");
    }
    // And on the group page: its title, its subtitle and its card.
    let group = &pages["area-diagnostics"];
    assert_eq!(
        group["meta"]["title"],
        format!("Requirement graph: {LONG_LATIN}")
    );
    assert!(subtitle(group).contains(LONG_LATIN), "{}", subtitle(group));
    assert_eq!(
        group["cards"][0]["title"],
        format!("{LONG_LATIN} (2 claims)")
    );
}

/// Falsification 4 (ADR 0050): one document, one directory, byte for byte, for every shape the areas add.
#[test]
fn two_runs_over_an_area_document_give_identical_directories() {
    let scratch = Scratch::new("area-determinism");
    let mut core = oversized_family();
    for n in core["nodes"].as_array_mut().expect("nodes") {
        n["area"] = json!("core");
    }
    for (name, document, limit) in [
        ("single", by_area(), None),
        ("split", by_area(), Some("4")),
        ("parts", with_areas(core, &[("core", "Core")]), Some("5")),
        ("long", long_labels(), Some("3")),
    ] {
        let input = write_fixture(&scratch, &format!("{name}.json"), &document);
        let first = scratch.path(&format!("{name}-first"));
        let second = scratch.path(&format!("{name}-second"));
        assert_eq!(run_with(&input, &first, limit).code, 0);
        assert_eq!(run_with(&input, &second, limit).code, 0);
        let (a, b) = (directory_bytes(&first), directory_bytes(&second));
        assert!(!a.is_empty());
        assert_eq!(a, b, "{name}: no clock and no randomness");
    }
}

/// `areas` and `area` are read from the document, and a value of the wrong shape is a refusal that writes nothing.
#[test]
fn an_areas_or_area_of_the_wrong_shape_is_refused_with_nothing_written() {
    let base = by_area();
    let mut bad: Vec<(&str, Value)> = Vec::new();
    let mut not_an_array = base.clone();
    not_an_array["areas"] = json!("interfaces");
    bad.push(("areas-string", not_an_array));
    let mut no_label = base.clone();
    no_label["areas"] = json!([{"path": "interfaces"}]);
    bad.push(("entry-no-label", no_label));
    let mut no_path = base.clone();
    no_path["areas"] = json!([{"label": "Interfaces"}]);
    bad.push(("entry-no-path", no_path));
    let mut twice = base.clone();
    twice["areas"] = json!([
        {"path": "interfaces", "label": "A"},
        {"path": "interfaces", "label": "B"}
    ]);
    bad.push(("path-twice", twice));
    let mut number = base.clone();
    number["nodes"][0]["area"] = json!(7);
    bad.push(("area-number", number));
    let mut shouting = base;
    shouting["nodes"][0]["area"] = json!("Not/A/Path");
    bad.push(("area-not-a-path", shouting));
    for (name, document) in bad {
        let scratch = Scratch::new(&format!("area-bad-{name}"));
        let input = write_fixture(&scratch, "trace.json", &document);
        let out = scratch.path("graph");
        let ran = run(&input, &out);
        assert_eq!(ran.code, 2, "{name}: {}", ran.stderr);
        assert!(ran.stderr.starts_with("refused:"), "{name}: {}", ran.stderr);
        assert!(!out.exists(), "{name}: nothing is written on a refusal");
    }
}

// --- the whole path: a corpus with areas, through `req trace --format json`, through the converter ------------------

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// A corpus made by the real bootstrap, three `req.claim/3` claims filed under three areas, and a registry whose labels
/// are the reader-facing names. Returns its `req trace --format json` document.
fn corpus_document(scratch: &Scratch) -> String {
    let dir = scratch.path("corpus");
    fs::create_dir_all(&dir).expect("makes the corpus dir");
    let init = Command::new(req_bin())
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("runs req init");
    assert_eq!(init.status.code(), Some(0), "req init must succeed");
    fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
    for (id, area, relations) in [
        ("REQ-0001", "interfaces", ""),
        ("REQ-0002", "diagnostics/startup", "relations:\n  - type: refines\n    to: REQ-0001\n"),
        ("REQ-0003", "power", ""),
    ] {
        let relations = if relations.is_empty() {
            "relations: []\n"
        } else {
            relations
        };
        let text = format!(
            "schema: req.claim/3\nclaim_id: {id}\n\
             statement: \"The {id} service shall respond to a health check within 200 ms.\"\n\
             applicability: []\n{relations}evidence: process\narea: {area}\n\
             rationale:\n  source: docs/requirements.md\n  date: 2026-11-13\n  \
             rejected_alternatives: []\n  assumptions: []\n"
        );
        fs::write(dir.join(format!("claims/{id}.yml")), text).expect("writes a claim");
    }
    fs::create_dir_all(dir.join(".req")).expect("makes .req");
    fs::write(
        dir.join(".req/areas"),
        "power Power management\ninterfaces Interfaces\ndiagnostics/startup Démarrage du système\n",
    )
    .expect("writes the registry");
    let out = Command::new(req_bin())
        .args(["trace", "--format", "json"])
        .current_dir(&dir)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req trace");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

/// Mutation anchor: the document carries the registry, and the converter reads nothing else, so a frame is named by its
/// label. A document with no `areas` would name the frames by their paths.
#[test]
fn a_frame_is_named_by_the_label_req_trace_carries_from_the_registry() {
    let scratch = Scratch::new("area-end-to-end");
    let text = corpus_document(&scratch);
    let document: Value = serde_json::from_str(&text).expect("the trace document");
    let page = converted(&scratch, &document);
    assert_eq!(
        frame_labels(&page),
        strings(&["Power management", "Interfaces", "Démarrage du système"]),
        "the labels of `.req/areas`, in its line order: {page}"
    );
}
