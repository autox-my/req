//! `tools/graph/trace_to_archify.py` — one `req.trace/1` document in, one archify diagram file out
//! (ADR 0046, unit V-1).
//!
//! The seam is the script itself, run the way an adopter runs it (`python3 -I tools/graph/trace_to_archify.py
//! <trace.json> <out.json>`), over a **hand-written** `req.trace/1` fixture: its nodes, edges and findings are
//! authored here from the ADR's decisions, never captured from a run of `req`. The oracles are the ADR's
//! *Falsification* items: deterministic bytes (1), nothing lost and nothing invented (3), no network (4), and a
//! schema it was not written for refused (5).

use std::collections::BTreeSet;
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

fn run(input: &Path, output: &Path) -> Run {
    let out = Command::new(python3())
        .arg("-I")
        .arg(script())
        .arg(input)
        .arg(output)
        .current_dir(root())
        .output()
        .expect("runs the converter");
    Run {
        code: out
            .status
            .code()
            .expect("the converter exits, never a signal"),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
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

/// Convert the fixture and read the archify file back; the run must succeed.
fn converted(scratch: &Scratch, document: &Value) -> Value {
    let input = write_fixture(scratch, "trace.json", document);
    let output = scratch.path("graph.json");
    let ran = run(&input, &output);
    assert_eq!(
        ran.code, 0,
        "the converter reads a req.trace/1 document:\n{}",
        ran.stderr
    );
    serde_json::from_str(&fs::read_to_string(&output).expect("the output"))
        .expect("the output is JSON")
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

/// Falsification 1: one document gives one file, byte for byte.
#[test]
fn two_runs_give_identical_bytes() {
    let scratch = Scratch::new("determinism");
    let input = write_fixture(&scratch, "trace.json", &fixture());
    let first = scratch.path("first.json");
    let second = scratch.path("second.json");
    assert_eq!(run(&input, &first).code, 0);
    assert_eq!(run(&input, &second).code, 0);
    assert_eq!(
        fs::read(&first).expect("first"),
        fs::read(&second).expect("second"),
        "the converter has no clock and no randomness"
    );
}

/// The document may be read from stdin (`-`), and gives the same bytes as the file path does.
#[test]
fn a_dash_reads_the_document_from_stdin() {
    let scratch = Scratch::new("stdin");
    let input = write_fixture(&scratch, "trace.json", &fixture());
    let from_file = scratch.path("file.json");
    assert_eq!(run(&input, &from_file).code, 0);

    let from_stdin = scratch.path("stdin.json");
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
    assert_eq!(
        fs::read(&from_file).expect("file"),
        fs::read(&from_stdin).expect("stdin")
    );
}

/// Falsification 5: a schema it was not written for is refused, naming it; a malformed document too; and
/// nothing is written either time.
#[test]
fn another_schema_version_is_refused_with_nothing_written() {
    let scratch = Scratch::new("schema");
    let mut document = fixture();
    document["schema"] = json!("req.trace/2");
    let input = write_fixture(&scratch, "trace.json", &document);
    let output = scratch.path("graph.json");
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
    let output = scratch.path("graph.json");
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
    let output = scratch.path("graph.json");
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
    let output = scratch.path("graph.json");
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
    let output = scratch.path("graph.json");
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
    // The successor that withdrew it is carried, from the document.
    let cards = graph["cards"].as_array().expect("cards");
    let card = cards
        .iter()
        .find(|c| c["title"] == "REQ-0003")
        .expect("a card for REQ-0003");
    assert!(
        card.to_string().contains("REQ-0004"),
        "the successor is named: {card}"
    );
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
        "os",
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
