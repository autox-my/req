//! `tools/graph/arch_overlay.py` — an archify `architecture` diagram, `req satisfy --format json` and `req trace --format
//! json` in, an overlay diagram and the four-column coverage matrix out (ADR 0052 decisions 1–5, unit O-1; ADR 0053, unit O-5).
//!
//! The seam is the script itself, run the way an adopter runs it (`python3 -I tools/graph/arch_overlay.py [--root <dir>]
//! <diagram.json> <satisfy.json> <trace.json> <out-dir>`), over **hand-written** documents: a diagram, a `req.satisfy/1`
//! and a `req.trace/1` authored here from ADR 0052 and ADR 0040, never captured from a run of `req`. The oracles are
//! ADR 0052's *Falsification* items; each test names the item it answers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn script() -> PathBuf {
    root().join("tools/graph/arch_overlay.py")
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
    panic!("python3 is required for tools/graph/arch_overlay.py - install it first");
}

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

/// A scratch directory holding a miniature repository (`repo/`) and the three input documents beside it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-graph-overlay-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("repo")).expect("creates the scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn repo(&self) -> PathBuf {
        self.0.join("repo")
    }

    /// A file in the miniature repository.
    fn file(&self, relative: &str) {
        let path = self.repo().join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("makes the dir");
        fs::write(path, "// a source file\n").expect("writes the file");
    }

    fn design(&self, roots: &str) {
        let path = self.repo().join(".req/design");
        fs::create_dir_all(path.parent().expect("parent")).expect("makes .req");
        fs::write(path, roots).expect("writes .req/design");
    }

    fn write_json(&self, name: &str, value: &Value) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, serde_json::to_string_pretty(value).expect("json")).expect("writes");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// The inputs, as documents; `inputs` writes them, `run` runs the converter over them.
struct Inputs {
    diagram: Value,
    satisfy: Value,
    trace: Value,
}

struct Paths {
    diagram: PathBuf,
    satisfy: PathBuf,
    trace: PathBuf,
}

impl Inputs {
    fn write(&self, s: &Scratch) -> Paths {
        Paths {
            diagram: s.write_json("diagram.json", &self.diagram),
            satisfy: s.write_json("satisfy.json", &self.satisfy),
            trace: s.write_json("trace.json", &self.trace),
        }
    }
}

fn convert(root_dir: Option<&Path>, paths: &Paths, out_dir: &Path, cwd: &Path) -> Run {
    let mut command = Command::new(python3());
    command.arg("-I").arg(script());
    if let Some(root_dir) = root_dir {
        command.arg("--root").arg(root_dir);
    }
    let out = command
        .arg(&paths.diagram)
        .arg(&paths.satisfy)
        .arg(&paths.trace)
        .arg(out_dir)
        .current_dir(cwd)
        .output()
        .expect("runs the converter");
    Run {
        code: out
            .status
            .code()
            .expect("the converter exits, never a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Write the inputs and run the converter into `<scratch>/out`, with `--root` the miniature repository.
fn run(s: &Scratch, inputs: &Inputs) -> Run {
    let paths = inputs.write(s);
    convert(Some(&s.repo()), &paths, &s.path("out"), &s.0)
}

fn overlay(s: &Scratch) -> Value {
    let text = fs::read_to_string(s.path("out/overlay.json")).expect("the overlay was written");
    serde_json::from_str(&text).expect("the overlay is JSON")
}

fn matrix(s: &Scratch) -> String {
    fs::read_to_string(s.path("out/matrix.md")).expect("the matrix was written")
}

// ---- fixtures ---------------------------------------------------------------------------------

fn component(id: &str, label: &str, sources: &[&str], col: i64) -> Value {
    let mut c = json!({
        "id": id,
        "type": "backend",
        "label": label,
        "pos": [40 + 170 * col, 120],
        "size": [150, 60],
    });
    if !sources.is_empty() {
        c["sources"] = Value::Array(
            sources
                .iter()
                .map(|path| json!({"path": path, "line": 1, "end_line": 9}))
                .collect(),
        );
    }
    c
}

fn diagram(components: Vec<Value>) -> Value {
    json!({
        "schema_version": 1,
        "diagram_type": "architecture",
        "meta": {
            "title": "Pump service",
            "output": "pump.html",
            "repository": {"url": "https://example.com/pump.git", "link_mode": "local-only", "revision": REVISION},
        },
        "components": components,
        "connections": [],
        "cards": [{"dot": "cyan", "title": "Author's card", "items": ["kept as written"]}],
    })
}

fn link(element: &str, claim: &str, outcome: &str) -> Value {
    json!({"element": element, "claim_id": claim, "outcome": outcome, "line": 1})
}

fn satisfy(links: Vec<Value>) -> Value {
    json!({
        "schema": "req.satisfy/1",
        "generator": "satisfy",
        "rules_v": 9,
        "verdict": "PASS",
        "links": links,
        "mentions": [],
        "unreadable": [],
    })
}

fn trace(claims: &[(&str, &str, &str)]) -> Value {
    json!({
        "schema": "req.trace/1",
        "generator": "trace",
        "rules_v": 9,
        "selection": {},
        "nodes": claims
            .iter()
            .map(|(id, statement, applies)| json!({
                "claim_id": id,
                "statement": statement,
                "modality": "SHALL",
                "address": format!("sha256:{}", "0".repeat(64)),
                "applies": applies,
                "superseded_by": [],
                "area": null,
                "source": "docs/requirements.md",
                "evidence": "machine",
                "tests": [],
            }))
            .collect::<Vec<_>>(),
        "edges": [],
        "derived_incoming": [],
        "findings": {},
    })
}

/// The corpus root of the miniature repository: three source files under the one design root `src`.
fn repo(s: &Scratch) {
    s.design("# design roots\nsrc\n");
    for file in ["src/a.rs", "src/b.rs", "src/c.rs", "docs/other.md"] {
        s.file(file);
    }
}

fn component_named<'a>(overlay: &'a Value, id: &str) -> &'a Value {
    overlay["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no component {id}"))
}

fn tag_of<'a>(overlay: &'a Value, id: &str) -> &'a str {
    component_named(overlay, id)["tag"]
        .as_str()
        .unwrap_or_else(|| panic!("component {id} has no tag"))
}

/// The status line (O-4): archify draws a `sublabel` at the overview zoom, so the status lives here, not in the `tag`.
fn status_of<'a>(overlay: &'a Value, id: &str) -> &'a str {
    component_named(overlay, id)["sublabel"]
        .as_str()
        .unwrap_or_else(|| panic!("component {id} has no sublabel"))
}

fn card<'a>(overlay: &'a Value, title: &str) -> Vec<&'a str> {
    overlay["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|c| c["title"] == title)
        .unwrap_or_else(|| panic!("no card titled {title:?}: {}", overlay["cards"]))["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item.as_str().expect("an item is a string"))
        .collect()
}

const STATUS_CARD: &str = "Components by status";
const UNREACHED_CARD: &str = "Claims no component reaches";
const OUTSIDE_CARD: &str = "Cited paths outside every design root";

const ORDER: [(&str, &str); 6] = [
    ("unknown_claim", "UNKNOWN CLAIM"),
    ("orphaned", "ORPHANED"),
    ("suspect", "SUSPECT"),
    ("unratified", "UNRATIFIED"),
    ("holds_weakened", "HOLDS (weakened)"),
    ("holds", "HOLDS"),
];

fn claims() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("REQ-0001", "The pump shall report its health.", "applies"),
        ("REQ-0002", "The pump shall log every request.", "applies"),
        (
            "REQ-0003",
            "The pump shall encrypt data at rest.",
            "unknown",
        ),
        (
            "REQ-0004",
            "The pump shall log to a remote host.",
            "does_not_apply",
        ),
    ]
}

// ---- Falsification 3: the worst outcome wins ------------------------------------------------

#[test]
fn a_component_citing_two_files_that_hold_and_are_suspect_is_suspect() {
    // Falsification 3 (ADR 0052): never HOLDS while a cited file's link has another outcome.
    let s = Scratch::new("worst");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component(
            "pump",
            "Pump",
            &["src/a.rs", "src/b.rs"],
            0,
        )]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds"),
            link("src/b.rs", "REQ-0002", "suspect"),
        ]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    let overlay = overlay(&s);
    // The worst state and how many of the claims are in it: one of the two is suspect (O-4: the sublabel).
    assert_eq!(status_of(&overlay, "pump"), "SUSPECT · 1 of 2 claims");
    assert!(
        component_named(&overlay, "pump").get("tag").is_none(),
        "no author text, no tag"
    );
    let card = card(&overlay, STATUS_CARD).join("\n");
    assert!(
        card.contains("REQ-0001") && card.contains("REQ-0002"),
        "{card}"
    );
}

#[test]
fn each_state_of_the_order_wins_over_every_state_after_it() {
    // ADR 0052 decision 2: unknown claim, orphaned, suspect, unratified, holds (weakened), holds.
    for start in 0..ORDER.len() {
        let s = Scratch::new("order");
        repo(&s);
        // One file declaring one claim per outcome from `start` on; the claim ids are distinct so no link is a
        // duplicate of another.
        let mut links = Vec::new();
        for (offset, (outcome, _)) in ORDER[start..].iter().enumerate() {
            links.push(link("src/a.rs", &format!("REQ-1{offset:03}"), outcome));
        }
        let inputs = Inputs {
            diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
            satisfy: satisfy(links),
            trace: trace(&claims()),
        };
        let out = run(&s, &inputs);
        // An unknown claim link is exit 3 (the page is still written); everything else is 0.
        let expected_exit = if ORDER[start].0 == "unknown_claim" {
            3
        } else {
            0
        };
        assert_eq!(
            out.code,
            expected_exit,
            "from {}: {}",
            ORDER[start].0,
            out.all()
        );
        let status = status_of(&overlay(&s), "pump").to_string();
        assert!(
            status.starts_with(ORDER[start].1),
            "from {}: the status word is {:?}, the sublabel is {status:?}",
            ORDER[start].0,
            ORDER[start].1
        );
    }
}

#[test]
fn stale_outranks_every_outcome() {
    // `stale` is not an outcome; it ranks worst of all (spec choice 3).
    let s = Scratch::new("stale-worst");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component(
            "pump",
            "Pump",
            &["src/a.rs", "src/gone.rs"],
            0,
        )]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "unknown_claim")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 3, "{}", out.all());
    assert_eq!(status_of(&overlay(&s), "pump"), "STALE");
}

#[test]
fn a_component_with_no_sources_and_one_whose_files_declare_nothing_are_no_claim() {
    let s = Scratch::new("no-claim");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("sketch", "Sketch", &[], 0),
            component("quiet", "Quiet", &["src/c.rs"], 1),
            component("pump", "Pump", &["src/a.rs"], 2),
        ]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "no claim is not a finding: {}", out.all());
    let overlay = overlay(&s);
    assert_eq!(status_of(&overlay, "sketch"), "no claim");
    assert_eq!(status_of(&overlay, "quiet"), "no claim");
    assert_eq!(status_of(&overlay, "pump"), "HOLDS · 1 claim");
}

// ---- Falsification 4: links follow the files, not the id --------------------------------------

#[test]
fn renaming_a_component_id_with_the_same_sources_keeps_its_claims_and_status() {
    // Falsification 4 (ADR 0052): a redrawn diagram keeps its links.
    let links = vec![
        link("src/a.rs", "REQ-0001", "suspect"),
        link("src/b.rs", "REQ-0002", "holds"),
        link("src/c.rs", "REQ-0003", "holds"),
    ];
    let run_with_id = |name: &str, id: &str| {
        let s = Scratch::new(name);
        repo(&s);
        let inputs = Inputs {
            diagram: diagram(vec![
                component(id, "Pump", &["src/a.rs", "src/b.rs"], 0),
                component("other", "Other", &["src/c.rs"], 1),
            ]),
            satisfy: satisfy(links.clone()),
            trace: trace(&claims()),
        };
        let out = run(&s, &inputs);
        assert_eq!(out.code, 0, "{}", out.all());
        (
            status_of(&overlay(&s), id).to_string(),
            card(&overlay(&s), STATUS_CARD)
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
            matrix(&s),
        )
    };
    let first = run_with_id("rename-a", "pump");
    let second = run_with_id("rename-b", "pump_service_v2");
    assert_eq!(first, second, "the id changed nothing the page shows");
    assert_eq!(first.0, "SUSPECT · 1 of 2 claims");
}

#[test]
fn a_link_for_a_file_no_component_cites_changes_no_status() {
    let s = Scratch::new("uncited");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds"),
            link("src/b.rs", "REQ-0002", "suspect"),
        ]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    let overlay = overlay(&s);
    assert_eq!(status_of(&overlay, "pump"), "HOLDS · 1 claim");
    // REQ-0002 is reached by no component, so the page lists it.
    assert!(
        card(&overlay, UNREACHED_CARD)
            .iter()
            .any(|i| i.contains("REQ-0002"))
    );
}

#[test]
fn line_ranges_in_sources_do_not_narrow_a_link() {
    // ADR 0052 decision 1: a line range locates evidence for the diagram; it does not narrow a link.
    let s = Scratch::new("ranges");
    repo(&s);
    let mut c = component("pump", "Pump", &[], 0);
    c["sources"] = json!([
        {"path": "src/a.rs", "line": 500, "end_line": 510},
        {"path": "src/a.rs", "line": 1},
    ]);
    let inputs = Inputs {
        diagram: diagram(vec![c]),
        satisfy: satisfy(vec![json!({
            "element": "src/a.rs", "claim_id": "REQ-0001", "outcome": "suspect", "line": 3
        })]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    // The claim is counted once, though two source entries cite the file.
    assert_eq!(status_of(&overlay(&s), "pump"), "SUSPECT · 1 claim");
}

// ---- Falsification 5: a stale diagram is flagged ----------------------------------------------

#[test]
fn a_cited_path_that_does_not_exist_is_stale_the_page_is_written_and_the_exit_is_3() {
    // Falsification 5 (ADR 0052).
    let s = Scratch::new("stale");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("pump", "Pump", &["src/removed.rs"], 0),
            component("ok", "Ok", &["src/a.rs"], 1),
        ]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(
        out.all().contains("src/removed.rs"),
        "names the path: {}",
        out.all()
    );
    assert!(out.all().contains("stale"), "says why: {}", out.all());
    let overlay = overlay(&s);
    assert_eq!(status_of(&overlay, "pump"), "STALE");
    assert_eq!(status_of(&overlay, "ok"), "HOLDS · 1 claim");
    assert!(
        card(&overlay, STATUS_CARD)
            .iter()
            .any(|item| item.contains("STALE") && item.contains("Pump"))
    );
    assert!(
        s.path("out/matrix.md").is_file(),
        "the matrix is written too"
    );
}

#[test]
fn an_unknown_claim_link_is_exit_3_and_the_page_is_still_written() {
    let s = Scratch::new("unknown-exit");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-9999", "unknown_claim")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 3, "{}", out.all());
    assert!(out.all().contains("REQ-9999"), "{}", out.all());
    assert!(s.path("out/overlay.json").is_file());
}

#[test]
fn the_root_defaults_to_the_current_directory() {
    let s = Scratch::new("default-root");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let paths = inputs.write(&s);
    // From the repository: the path exists. From elsewhere: it does not, so it is stale.
    let out = convert(None, &paths, &s.path("out-here"), &s.repo());
    assert_eq!(out.code, 0, "{}", out.all());
    let out = convert(None, &paths, &s.path("out-there"), &s.0);
    assert_eq!(out.code, 3, "{}", out.all());
}

// ---- outside the design roots -----------------------------------------------------------------

#[test]
fn a_cited_path_outside_every_design_root_is_named_in_its_card() {
    let s = Scratch::new("outside");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("pump", "Pump", &["src/a.rs", "docs/other.md"], 0),
            component("sketch", "Sketch", &[], 1),
        ]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    let overlay = overlay(&s);
    let items = card(&overlay, OUTSIDE_CARD);
    assert!(
        items.iter().any(|item| item.contains("docs/other.md")),
        "{items:?}"
    );
    assert!(
        !items.iter().any(|item| item.contains("src/a.rs")),
        "{items:?}"
    );
}

#[test]
fn with_no_design_file_every_cited_path_is_outside() {
    let s = Scratch::new("no-design");
    repo(&s);
    fs::remove_file(s.repo().join(".req/design")).expect("removes the list");
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        card(&overlay(&s), OUTSIDE_CARD)
            .iter()
            .any(|item| item.contains("src/a.rs"))
    );
}

// ---- presentation only ------------------------------------------------------------------------

#[test]
fn geometry_ids_connections_and_labels_are_unchanged_and_existing_text_is_kept() {
    let s = Scratch::new("presentation");
    repo(&s);
    let mut tagged = component("pump", "Pump", &["src/a.rs"], 0);
    tagged["tag"] = json!("app team");
    tagged["sublabel"] = json!("health endpoint");
    let mut input = diagram(vec![
        tagged,
        component("store", "Store", &["src/b.rs"], 1),
        component("sketch", "Sketch", &[], 2),
    ]);
    input["connections"] = json!([
        {"from": "pump", "to": "store", "label": "writes", "route": "straight"},
        {"id": "back", "from": "store", "to": "pump", "variant": "dashed", "via": [[10, 20]]},
    ]);
    input["boundaries"] = json!([{"kind": "region", "label": "prod", "wraps": ["pump", "store"]}]);
    input["layout"] = json!({"mode": "grid", "cols": 3});
    let inputs = Inputs {
        diagram: input.clone(),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds"),
            link("src/b.rs", "REQ-0002", "unratified"),
        ]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    let overlay = overlay(&s);
    for key in [
        "schema_version",
        "diagram_type",
        "connections",
        "boundaries",
        "layout",
    ] {
        assert_eq!(overlay[key], input[key], "`{key}` is the author's");
    }
    let before = input["components"].as_array().expect("components");
    let after = overlay["components"].as_array().expect("components");
    assert_eq!(before.len(), after.len());
    for (old, new) in before.iter().zip(after) {
        let strip = |c: &Value| {
            let mut c = c.clone();
            let object = c.as_object_mut().expect("object");
            object.remove("tag");
            object.remove("sublabel");
            c
        };
        assert_eq!(
            strip(old),
            strip(new),
            "same id, label, geometry and sources, same order"
        );
    }
    // O-4: the status takes the sublabel archify draws at the overview zoom; the author's sublabel is kept, moved to
    // the tag after the author's own tag text, never dropped.
    assert_eq!(status_of(&overlay, "pump"), "HOLDS · 1 claim");
    assert_eq!(tag_of(&overlay, "pump"), "app team | health endpoint");
    assert_eq!(status_of(&overlay, "store"), "UNRATIFIED · 1 claim");
    assert!(
        component_named(&overlay, "store").get("tag").is_none(),
        "no author text, no tag"
    );
    // The author's cards stay first and unchanged.
    assert_eq!(overlay["cards"][0], input["cards"][0]);
    // The title is the author's; the subtitle says what the page is made from.
    assert_eq!(overlay["meta"]["title"], input["meta"]["title"]);
    assert_eq!(overlay["meta"]["repository"], input["meta"]["repository"]);
    let subtitle = overlay["meta"]["subtitle"].as_str().expect("a subtitle");
    assert!(subtitle.contains(REVISION), "{subtitle}");
    assert!(subtitle.contains("rules_v 9"), "{subtitle}");
    assert!(subtitle.contains("7e45f36"), "{subtitle}");
}

#[test]
fn the_authors_sublabel_moves_to_the_tag_after_the_authors_tag_and_is_never_dropped() {
    let s = Scratch::new("keep-text");
    repo(&s);
    let mut only_sub = component("only_sub", "Only sub", &["src/b.rs"], 1);
    only_sub["sublabel"] = json!("reads the queue");
    let mut both = component("both", "Both", &["src/a.rs"], 0);
    both["tag"] = json!("app team");
    both["sublabel"] = json!("health endpoint");
    let mut only_tag = component("only_tag", "Only tag", &["src/c.rs"], 2);
    only_tag["tag"] = json!("owned by ops");
    let inputs = Inputs {
        diagram: diagram(vec![both, only_sub, only_tag]),
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    let overlay = overlay(&s);
    assert_eq!(tag_of(&overlay, "both"), "app team | health endpoint");
    assert_eq!(tag_of(&overlay, "only_sub"), "reads the queue");
    assert_eq!(tag_of(&overlay, "only_tag"), "owned by ops");
    for id in ["both", "only_sub", "only_tag"] {
        assert_eq!(status_of(&overlay, id), "no claim", "{id}");
    }
}

#[test]
fn the_status_line_names_the_worst_state_and_how_many_claims_are_in_it() {
    let s = Scratch::new("status-line");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("a", "A", &["src/a.rs"], 0),
            component("b", "B", &["src/b.rs"], 1),
            component("c", "C", &["src/c.rs"], 2),
        ]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds"),
            link("src/a.rs", "REQ-0002", "holds"),
            link("src/a.rs", "REQ-1001", "holds"),
            link("src/b.rs", "REQ-0001", "holds"),
            link("src/b.rs", "REQ-0002", "suspect"),
            link("src/b.rs", "REQ-1001", "holds_weakened"),
            link("src/c.rs", "REQ-0002", "suspect"),
        ]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let overlay = overlay(&s);
    assert_eq!(status_of(&overlay, "a"), "HOLDS · 3 claims");
    assert_eq!(status_of(&overlay, "b"), "SUSPECT · 1 of 3 claims");
    assert_eq!(status_of(&overlay, "c"), "SUSPECT · 1 claim");
}

#[test]
fn an_existing_subtitle_is_kept_and_added_to() {
    let s = Scratch::new("subtitle");
    repo(&s);
    let mut input = diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]);
    input["meta"]["subtitle"] = json!("the request path");
    let inputs = Inputs {
        diagram: input,
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let overlay = overlay(&s);
    let subtitle = overlay["meta"]["subtitle"].as_str().expect("a subtitle");
    assert!(subtitle.starts_with("the request path"), "{subtitle}");
    assert!(subtitle.contains("7e45f36"), "{subtitle}");
}

#[test]
fn a_diagram_with_no_pinned_revision_says_so_rather_than_claiming_currency() {
    let s = Scratch::new("no-revision");
    repo(&s);
    let mut input = diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]);
    input["meta"]
        .as_object_mut()
        .expect("meta")
        .remove("repository");
    let inputs = Inputs {
        diagram: input,
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let overlay = overlay(&s);
    let subtitle = overlay["meta"]["subtitle"].as_str().expect("a subtitle");
    assert!(subtitle.contains("no pinned revision"), "{subtitle}");
    assert!(
        subtitle.contains("rules_v 9") && subtitle.contains("7e45f36"),
        "{subtitle}"
    );
}

#[test]
fn a_long_claim_list_is_counted_in_the_status_line_and_the_status_card_holds_every_id() {
    let s = Scratch::new("long-list");
    repo(&s);
    let ids: Vec<String> = (1..=8).map(|n| format!("REQ-{n:04}")).collect();
    let nodes: Vec<(String, String, String)> = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                format!("{id} statement."),
                "applies".to_string(),
            )
        })
        .collect();
    let nodes: Vec<(&str, &str, &str)> = nodes
        .iter()
        .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
        .collect();
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(ids.iter().map(|id| link("src/a.rs", id, "holds")).collect()),
        trace: trace(&nodes),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let overlay = overlay(&s);
    assert_eq!(status_of(&overlay, "pump"), "HOLDS · 8 claims");
    let card = card(&overlay, STATUS_CARD).join("\n");
    for id in &ids {
        assert!(card.contains(id.as_str()), "{id} missing from {card}");
    }
}

/// A component with no `size`: archify draws it `layout.defaultW` = 120 px wide (render-architecture.mjs at 7e45f36), so
/// its tag has (120 - 8) / 3.6 = 31 columns at the 6 px floor, whatever the grid's cell is.
fn no_size(id: &str, label: &str, source: &str, tag: Option<&str>) -> Value {
    let mut c = json!({"id": id, "type": "backend", "label": label, "pos": [40, 120]});
    c["sources"] = json!([{"path": source, "line": 1}]);
    if let Some(tag) = tag {
        c["tag"] = json!(tag);
    }
    c
}

/// `no_size` with the author's sublabel, which the overlay moves to the tag.
fn no_size_sub(id: &str, source: &str, tag: Option<&str>, sublabel: &str) -> Value {
    let mut c = no_size(id, "Pump", source, tag);
    c["sublabel"] = json!(sublabel);
    c
}

/// The columns archify counts a string as (`textUnits` in renderers/shared at 7e45f36): a variation selector adds none, a
/// character followed by U+FE0F takes two, a wide character takes two and anything else one. This is the **subset** of
/// archify's wide ranges these tests use (Hangul Jamo, CJK and its neighbours, full-width forms, the Mahjong-to-symbols
/// block U+1F000..U+1FAFF and the CJK planes); an oracle written here from that rule, never read from the converter.
fn tag_units(tag: &str) -> usize {
    let chars: Vec<char> = tag.chars().collect();
    let wide = |c: char| {
        matches!(
            u32::from(c),
            0x1100..=0x115F
                | 0x2E80..=0xA4CF
                | 0xAC00..=0xD7A3
                | 0xFF01..=0xFF60
                | 0x1F000..=0x1FAFF
                | 0x20000..=0x3FFFD
        )
    };
    let mut total = 0;
    for (index, c) in chars.iter().enumerate() {
        if ('\u{FE00}'..='\u{FE0F}').contains(c) {
            continue;
        }
        total += if chars.get(index + 1) == Some(&'\u{FE0F}') || wide(*c) {
            2
        } else {
            1
        };
    }
    total
}

#[test]
fn a_component_with_no_size_is_120_px_wide_so_the_moved_text_has_31_columns() {
    // Exactly 31 columns fits: "the pump tea | health endpoint" is 12 + 3 + 15 = 30; with one more it is 31.
    let s = Scratch::new("width-120-fits");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![no_size_sub(
            "pump",
            "src/a.rs",
            Some("the pump team"),
            "health endpoint",
        )]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 0, "{}", out.all());
    let first = overlay(&s);
    let tag = tag_of(&first, "pump").to_string();
    assert_eq!(tag, "the pump team | health endpoint");
    assert_eq!(tag_units(&tag), 31);
    assert_eq!(status_of(&first, "pump"), "HOLDS · 1 claim");

    // One column more is what archify's `component-tag-overflow` refuses at 120 px: kept, named, exit 3.
    let s = Scratch::new("width-120-overflows");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![no_size_sub(
            "pump",
            "src/a.rs",
            Some("the pump teams"),
            "health endpoint",
        )]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 3, "{}", out.all());
    assert_eq!(
        tag_of(&overlay(&s), "pump"),
        "the pump teams | health endpoint",
        "the author's text is kept whole"
    );
}

#[test]
fn a_grid_cell_is_the_step_between_boxes_never_a_boxs_width() {
    // `layout.cellW` 200 is the grid step; a component with no `size` is still 120 px wide, so 31 columns: 31 fit, 32 do
    // not, whatever the cell is.
    for (name, text, exit) in [
        ("fits", "x".repeat(31), 0),
        ("overflows", "x".repeat(32), 3),
    ] {
        let s = Scratch::new(&format!("grid-cell-{name}"));
        repo(&s);
        let mut input = diagram(vec![json!({
            "id": "pump", "type": "backend", "label": "Pump", "row": 0, "col": 0,
            "sublabel": text,
            "sources": [{"path": "src/a.rs", "line": 1}],
        })]);
        input["layout"] = json!({"mode": "grid", "cols": 3, "cellW": 200});
        let inputs = Inputs {
            diagram: input,
            satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
            trace: trace(&claims()),
        };
        let out = run(&s, &inputs);
        assert_eq!(out.code, exit, "{name}: {}", out.all());
        assert_eq!(tag_of(&overlay(&s), "pump"), text, "{name}");
    }
}

#[test]
fn a_moved_text_the_tag_cannot_hold_is_kept_named_and_exit_3() {
    // "the pump team" (13) + " | " + "health endpoint is fine" (23) is 39 > 31 in a 120 px box. The author's text is
    // kept whole, the page is written, and the overlay says it would not pass archify validate.
    let s = Scratch::new("tag-overflow");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            no_size_sub(
                "pump",
                "src/c.rs",
                Some("the pump team"),
                "health endpoint is fine",
            ),
            no_size_sub("fits", "src/c.rs", Some("pump team"), "health"),
        ]),
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 3, "{}", out.all());
    let overlay = overlay(&s);
    assert_eq!(
        tag_of(&overlay, "pump"),
        "the pump team | health endpoint is fine",
        "kept"
    );
    assert!(
        out.stderr.contains("pump") && out.stderr.contains("tag") && out.stderr.contains("120 px"),
        "names the component and the reason: {}",
        out.stderr
    );
    assert!(!out.stderr.contains("'fits'"), "{}", out.stderr);
    assert_eq!(tag_of(&overlay, "fits"), "pump team | health");
    assert_eq!(status_of(&overlay, "pump"), "no claim");
    assert!(s.path("out/matrix.md").is_file());
}

#[test]
fn a_status_line_that_cannot_fit_is_written_named_and_exit_3() {
    // A 60 px box draws (60 - 8) / 3.6 = 14 columns at archify's 6 px floor; "HOLDS · 6 claims" is 16.
    let s = Scratch::new("status-overflow");
    repo(&s);
    let mut narrow = component("pump", "Pump", &["src/a.rs"], 0);
    narrow["size"] = json!([60, 60]);
    let mut roomy = component("roomy", "Roomy", &["src/b.rs"], 1);
    roomy["size"] = json!([100, 60]);
    let ids: Vec<String> = (1..=6).map(|n| format!("REQ-{n:04}")).collect();
    let rows: Vec<(String, String, String)> = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                format!("{id} statement."),
                "applies".to_string(),
            )
        })
        .collect();
    let rows: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
        .collect();
    let mut links: Vec<Value> = ids.iter().map(|id| link("src/a.rs", id, "holds")).collect();
    links.push(link("src/b.rs", "REQ-0001", "holds"));
    let inputs = Inputs {
        diagram: diagram(vec![narrow, roomy]),
        satisfy: satisfy(links),
        trace: trace(&rows),
    };
    let out = run(&s, &inputs);
    assert_eq!(out.code, 3, "{}", out.all());
    let overlay = overlay(&s);
    assert_eq!(
        status_of(&overlay, "pump"),
        "HOLDS · 6 claims",
        "the status line is written whole"
    );
    assert!(
        out.stderr.contains("pump")
            && out.stderr.contains("sublabel")
            && out.stderr.contains("60 px"),
        "names the component, the field and the box: {}",
        out.stderr
    );
    assert!(!out.stderr.contains("'roomy'"), "{}", out.stderr);
    assert_eq!(status_of(&overlay, "roomy"), "HOLDS · 1 claim");
    assert!(s.path("out/matrix.md").is_file());
}

#[test]
fn a_moved_text_is_measured_in_archify_columns_at_the_31_column_boundary() {
    // Each text is authored as a sublabel on a 120 px component, so the overlay moves it to the tag unchanged (there is
    // no author's tag). 31 columns fits; 32 is what archify's `component-tag-overflow` refuses, and the converter
    // must agree with archify about which is which, whatever the characters are.
    let x = |n: usize| "x".repeat(n);
    let cases: Vec<(&str, String, bool)> = vec![
        ("fifteen CJK fit (30)", "中".repeat(15), true),
        ("sixteen CJK overflow (32)", "中".repeat(16), false),
        // U+1F000 is wide in archify (2) though unicodedata says neutral: 30 + 2 = 32.
        (
            "a Mahjong tile is two columns",
            format!("{}\u{1F000}", x(30)),
            false,
        ),
        (
            "a Mahjong tile fits at 29",
            format!("{}\u{1F000}", x(29)),
            true,
        ),
        // U+1D360 is neutral in archify (1) though unicodedata says wide: 30 + 1 = 31.
        (
            "a counting rod is one column",
            format!("{}\u{1D360}", x(30)),
            true,
        ),
        // A variation selector adds nothing: U+4E2D (2) with U+FE0E is 2, so 29 + 2 = 31.
        (
            "a variation selector adds no column",
            format!("{}\u{4E2D}\u{FE0E}", x(29)),
            true,
        ),
        // U+FE0F asks for emoji presentation: the character takes two. 30 + 2 = 32.
        (
            "emoji presentation takes two",
            format!("{}\u{2764}\u{FE0F}", x(30)),
            false,
        ),
    ];
    for (index, (name, text, fits)) in cases.iter().enumerate() {
        let s = Scratch::new(&format!("columns-{index}"));
        repo(&s);
        let inputs = Inputs {
            diagram: diagram(vec![no_size_sub("pump", "src/c.rs", None, text)]),
            satisfy: satisfy(vec![]),
            trace: trace(&claims()),
        };
        let out = run(&s, &inputs);
        let written = tag_of(&overlay(&s), "pump").to_string();
        assert_eq!(&written, text, "{name}: the author's text is kept");
        assert_eq!(
            tag_units(&written) <= 31,
            *fits,
            "{name}: {} columns, so the oracle says it {}",
            tag_units(&written),
            if *fits { "fits" } else { "overflows" }
        );
        let expected = if *fits { 0 } else { 3 };
        assert_eq!(out.code, expected, "{name}: {}", out.all());
    }
}

#[test]
fn a_dot_design_root_is_the_whole_repository_so_nothing_under_it_is_outside() {
    // `.req/design` may list `.` (or `./`): the repository root, whose element ids are `src/a.rs`, never `./src/a.rs`.
    for root_line in [".", "./", "./."] {
        let s = Scratch::new("dot-root");
        repo(&s);
        s.design(&format!("# the whole repository\n{root_line}\n"));
        let inputs = Inputs {
            diagram: diagram(vec![component(
                "pump",
                "Pump",
                &["src/a.rs", "docs/other.md"],
                0,
            )]),
            satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
            trace: trace(&claims()),
        };
        let out = run(&s, &inputs);
        assert_eq!(out.code, 0, "{root_line:?}: {}", out.all());
        let overlay = overlay(&s);
        assert_eq!(
            card(&overlay, OUTSIDE_CARD),
            vec!["none"],
            "{root_line:?}: every cited path is under the root"
        );
        assert!(status_of(&overlay, "pump").starts_with("HOLDS"));
    }
}

#[test]
fn a_design_root_with_dot_segments_covers_the_files_under_it_and_a_dot_slash_absolute_covers_nothing()
 {
    // `src/.` and `./src/./` are the root `src`: `src/a.rs` is inside, `docs/other.md` is not. `.//tmp` is absolute (req
    // refuses it); here it must never be read as the repository-relative `tmp`, so `tmp/x.rs` stays outside.
    for root_line in ["src/.", "./src/./", "src//"] {
        let s = Scratch::new("segment-root");
        repo(&s);
        s.design(&format!("{root_line}\n"));
        let inputs = Inputs {
            diagram: diagram(vec![component(
                "pump",
                "Pump",
                &["src/a.rs", "docs/other.md"],
                0,
            )]),
            satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
            trace: trace(&claims()),
        };
        let out = run(&s, &inputs);
        assert_eq!(out.code, 0, "{root_line:?}: {}", out.all());
        assert_eq!(
            card(&overlay(&s), OUTSIDE_CARD),
            vec!["docs/other.md (cited by Pump)"],
            "{root_line:?}"
        );
    }
    let s = Scratch::new("absolute-root");
    repo(&s);
    s.file("tmp/x.rs");
    s.design(".//tmp\n");
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["tmp/x.rs"], 0)]),
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    assert_eq!(
        card(&overlay(&s), OUTSIDE_CARD),
        vec!["tmp/x.rs (cited by Pump)"]
    );
}

// ---- the matrix: four columns, a status in each cell (ADR 0053, unit O-5) -----------------------

const HEADER: &str = "| Source | Claim | Satisfied by | Verified by |";

/// One `req.trace/1` node as the coverage matrix reads it: `source`, `evidence` and `tests` after `area` (ADR 0053
/// decision 4), authored here from the ADR and never captured from a run.
fn covered(id: &str, statement: &str, source: Value, evidence: Value, tests: &[&str]) -> Value {
    json!({
        "claim_id": id,
        "statement": statement,
        "modality": "SHALL",
        "address": format!("sha256:{}", "0".repeat(64)),
        "applies": "applies",
        "superseded_by": [],
        "area": null,
        "source": source,
        "evidence": evidence,
        "tests": tests,
    })
}

fn trace_of(nodes: Vec<Value>) -> Value {
    let mut document = trace(&[]);
    document["nodes"] = Value::Array(nodes);
    document
}

/// The cells of a table row, split on a `|` that is not escaped, trimmed.
fn cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut previous = ' ';
    for ch in line.chars() {
        if ch == '|' && previous != '\\' {
            cells.push(String::new());
        } else {
            cells.last_mut().expect("a cell").push(ch);
        }
        previous = ch;
    }
    let inner = &cells[1..cells.len() - 1];
    inner.iter().map(|cell| cell.trim().to_string()).collect()
}

/// The row of `claim` in the matrix text: its cells.
fn row_of(text: &str, claim: &str) -> Vec<String> {
    let line = text
        .lines()
        .find(|line| line.starts_with('|') && line.contains(&format!("[{claim}]")))
        .unwrap_or_else(|| panic!("no row for {claim}:\n{text}"));
    cells(line)
}

#[test]
fn the_matrix_has_four_columns_a_row_per_applying_claim_by_id_and_names_each_claim() {
    let s = Scratch::new("matrix");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("zeta", "Zeta", &["src/b.rs"], 0),
            component("alpha", "Alpha", &["src/a.rs"], 1),
        ]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds_weakened"),
            link("src/b.rs", "REQ-0002", "suspect"),
            link("src/b.rs", "REQ-0001", "holds"),
        ]),
        trace: trace_of(vec![
            covered(
                "REQ-0002",
                "The pump shall log every request.",
                json!("docs/b.md"),
                json!("machine"),
                &[],
            ),
            covered(
                "REQ-0001",
                "The pump shall report its health.",
                json!("docs/a.md"),
                json!("machine"),
                &[],
            ),
            covered(
                "REQ-0003",
                "The pump shall encrypt data at rest.",
                json!("docs/c.md"),
                json!("machine"),
                &[],
            ),
        ]),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let text = matrix(&s);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], HEADER, "{text}");
    assert_eq!(lines[1], "| --- | --- | --- | --- |", "{text}");
    let ids: Vec<String> = lines[2..]
        .iter()
        .filter(|line| line.starts_with('|'))
        .map(|line| cells(line)[1].clone())
        .collect();
    assert_eq!(ids.len(), 3, "{text}");
    for (id, claim) in ids.iter().zip(["REQ-0001", "REQ-0002", "REQ-0003"]) {
        assert!(
            id.starts_with(&format!("[{claim}](claims/{claim}.yml)")),
            "{id}"
        );
    }
    let first = row_of(&text, "REQ-0001");
    assert_eq!(first.len(), 4, "{first:?}");
    assert_eq!(first[0], "docs/a.md", "the source, verbatim");
    assert!(
        first[1].ends_with("The pump shall report its health."),
        "{first:?}"
    );
    // Components follow the diagram's order (Zeta, then Alpha); each carries its own outcome.
    let zeta = first[2]
        .find("Zeta (holds)")
        .unwrap_or_else(|| panic!("{first:?}"));
    let alpha = first[2]
        .find("Alpha (holds (weakened))")
        .unwrap_or_else(|| panic!("{first:?}"));
    assert!(zeta < alpha, "{first:?}");
    assert!(
        row_of(&text, "REQ-0002")[2].contains("Zeta (suspect)"),
        "{text}"
    );
    // REQ-0004 is not in this trace; a claim that does not apply is no row (below).
    assert!(!text.contains("REQ-0004"), "{text}");
    // The unreached claim is still on the overlay's card.
    let overlay = overlay(&s);
    let items = card(&overlay, UNREACHED_CARD);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(items[0].contains("REQ-0003"), "{items:?}");
    assert!(text.ends_with('\n'));
}

#[test]
fn a_claim_that_does_not_apply_has_no_row_and_the_claim_link_follows_the_matrix_dir() {
    let s = Scratch::new("applies");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![]),
        trace: trace(&claims()),
    };
    let paths = inputs.write(&s);
    let out = Command::new(python3())
        .arg("-I")
        .arg(script())
        .arg("--root")
        .arg(s.repo())
        .args(["--matrix-dir", "docs/architecture"])
        .arg(&paths.diagram)
        .arg(&paths.satisfy)
        .arg(&paths.trace)
        .arg(s.path("out"))
        .output()
        .expect("runs the converter");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = matrix(&s);
    assert!(!text.contains("REQ-0004"), "{text}");
    assert!(
        row_of(&text, "REQ-0001")[1].starts_with("[REQ-0001](../../claims/REQ-0001.yml)"),
        "the link is relative to the file the matrix is read from:\n{text}"
    );
}

#[test]
fn a_cell_names_the_component_with_the_worst_outcome_of_its_files_and_each_file_once() {
    // ADR 0052 decision 2's order, per claim and component; ADR 0053 decision 6 links a file to its lines on GitHub.
    let s = Scratch::new("cell-worst");
    repo(&s);
    let mut diagram = diagram(vec![component(
        "pump",
        "Pump",
        &["src/a.rs", "src/b.rs"],
        0,
    )]);
    diagram["meta"]["repository"]["url"] = json!("https://github.com/acme/pump.git");
    let inputs = Inputs {
        diagram,
        satisfy: satisfy(vec![
            json!({"element": "src/a.rs", "claim_id": "REQ-0001", "outcome": "holds", "line": 7}),
            json!({"element": "src/b.rs", "claim_id": "REQ-0001", "outcome": "unratified", "line": 3}),
            json!({"element": "src/b.rs", "claim_id": "REQ-0001", "outcome": "unratified", "line": 9}),
        ]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(cell.contains("Pump (unratified)"), "{cell}");
    assert!(!cell.contains("Pump (holds)"), "{cell}");
    let base = format!("https://github.com/acme/pump/blob/{REVISION}");
    assert_eq!(
        cell.matches("src/a.rs").count(),
        2,
        "name and URL, once: {cell}"
    );
    // The line comes from the working tree and the URL pins a revision, so a line anchor could point at the wrong line of
    // the pinned file: the file is linked at the revision, with no `#L`.
    assert!(!cell.contains("#L"), "no line anchor: {cell}");
    assert!(cell.contains(&format!("]({base}/src/a.rs)")), "{cell}");
    assert_eq!(cell.matches("src/b.rs").count(), 2, "{cell}");
    assert!(cell.contains(&format!("]({base}/src/b.rs)")), "{cell}");
}

#[test]
fn a_file_is_named_without_a_link_when_the_repository_is_not_github_or_has_no_revision() {
    let s = Scratch::new("no-github");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(cell.contains("src/a.rs"), "{cell}");
    assert!(!cell.contains("http"), "no GitHub URL to point at: {cell}");
}

#[test]
fn a_label_and_a_source_with_a_pipe_do_not_break_the_table() {
    let s = Scratch::new("pipe");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Read | Write", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace_of(vec![covered(
            "REQ-0001",
            "The pump shall read | write.",
            json!("spec | section\n4.2"),
            json!("machine"),
            &[],
        )]),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let text = matrix(&s);
    let row = row_of(&text, "REQ-0001");
    assert_eq!(row.len(), 4, "{text}");
    assert_eq!(
        row[0], "spec \\| section 4.2",
        "pipe escaped, newline folded: {row:?}"
    );
    assert!(row[2].contains("Read \\| Write (holds)"), "{row:?}");
    assert!(row[1].contains("read \\| write"), "{row:?}");
}

// ---- a component links to its architecture page (ADR 0053 decision 6) ---------------------------

/// Run with `--page <path>` (and `--root` the miniature repository) into `<scratch>/out`.
fn convert_with_page(s: &Scratch, paths: &Paths, page: &[&str]) -> Run {
    let mut command = Command::new(python3());
    command.arg("-I").arg(script()).arg("--root").arg(s.repo());
    for arg in page {
        command.arg(arg);
    }
    let out = command
        .arg(&paths.diagram)
        .arg(&paths.satisfy)
        .arg(&paths.trace)
        .arg(s.path("out"))
        .current_dir(&s.0)
        .output()
        .expect("runs the converter");
    Run {
        code: out.status.code().expect("exits"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn with_a_page_each_component_links_to_its_node_on_that_page() {
    // archify's viewer focuses a node from `#focus=<node id>` (viewer runtime: `syncFocusFromHash`); the node id is the
    // component's id.
    let s = Scratch::new("page");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("pump", "Pump", &["src/a.rs"], 0),
            component("my part", "Read | Write", &["src/b.rs"], 1),
        ]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds"),
            link("src/b.rs", "REQ-0001", "suspect"),
        ]),
        trace: trace(&claims()),
    };
    let paths = inputs.write(&s);
    let out = convert_with_page(&s, &paths, &["--page", "../architecture.html"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(
        cell.contains("[Pump](../architecture.html#focus=pump) (holds)"),
        "{cell}"
    );
    assert!(
        cell.contains("[Read \\| Write](../architecture.html#focus=my%20part) (suspect)"),
        "the id is percent-encoded, the label escaped: {cell}"
    );
}

#[test]
fn an_absolute_https_page_is_used_as_given_and_a_relative_one_is_encoded() {
    // The matrix is read on github.com, where a relative `.html` link opens the source: the committed matrices point at the
    // published page, an absolute URL that is not re-encoded (`https://` keeps its colon).
    let s = Scratch::new("page-absolute");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let paths = inputs.write(&s);
    let url = "https://autox-my.github.io/req/architecture.html";
    let out = convert_with_page(&s, &paths, &["--page", url]);
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(
        cell.contains(&format!("[Pump]({url}#focus=pump) (holds)")),
        "{cell}"
    );
    // A relative path keeps being encoded, a parenthesis and a space included (they would end the link).
    fs::remove_dir_all(s.path("out")).expect("clears");
    let out = convert_with_page(&s, &paths, &["--page", "../a (b).html"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(
        cell.contains("[Pump](../a%20%28b%29.html#focus=pump) (holds)"),
        "{cell}"
    );
}

#[test]
fn a_page_that_is_not_https_or_has_a_fragment_is_refused_naming_the_option() {
    for page in [
        "http://autox-my.github.io/req/architecture.html",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "https://autox-my.github.io/req/architecture.html#focus=x",
        "https://",
        "https://?q=1",
        "https://#x",
        "//autox-my.github.io/req/architecture.html",
        "../architecture.html#section",
        "",
    ] {
        let s = Scratch::new("page-refused");
        repo(&s);
        let paths = full_inputs().write(&s);
        let out = convert_with_page(&s, &paths, &["--page", page]);
        assert_eq!(out.code, 2, "{page}: {}", out.all());
        assert!(out.stderr.contains("--page"), "{page}: {}", out.stderr);
        assert!(!s.path("out").exists(), "{page}: nothing is written");
    }
}

#[test]
fn brackets_and_backslashes_in_a_label_are_escaped_as_link_text() {
    let s = Scratch::new("page-brackets");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Read] [Write\\x", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    let paths = inputs.write(&s);
    let out = convert_with_page(&s, &paths, &["--page", "../architecture.html"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(
        cell.contains("[Read\\] \\[Write\\\\x](../architecture.html#focus=pump) (holds)"),
        "{cell}"
    );
}

#[test]
fn without_a_page_a_component_is_plain_text_nothing_is_guessed() {
    let s = Scratch::new("no-page");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let cell = row_of(&matrix(&s), "REQ-0001")[2].clone();
    assert!(cell.starts_with("Pump (holds)"), "{cell}");
    assert!(!cell.contains("#focus"), "{cell}");
}

#[test]
fn a_page_option_with_no_value_or_given_twice_is_usage_exit_2() {
    let s = Scratch::new("page-usage");
    repo(&s);
    let paths = full_inputs().write(&s);
    for page in [vec!["--page"], vec!["--page", "a.html", "--page", "b.html"]] {
        let out = convert_with_page(&s, &paths, &page);
        assert_eq!(out.code, 2, "{page:?}: {}", out.all());
        assert!(out.stderr.contains("usage:"), "{}", out.stderr);
    }
}

// ---- Falsification 5 (ADR 0053): a component is never a column ----------------------------------

#[test]
fn the_matrix_keeps_four_columns_whatever_the_number_of_components() {
    let mut widths = Vec::new();
    for count in [1usize, 5] {
        let s = Scratch::new(&format!("columns-{count}"));
        repo(&s);
        let components: Vec<Value> = (0..count)
            .map(|index| {
                component(
                    &format!("c{index}"),
                    &format!("Part {index}"),
                    &["src/a.rs"],
                    i64::try_from(index).expect("small"),
                )
            })
            .collect();
        let inputs = Inputs {
            diagram: diagram(components),
            satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
            trace: trace(&claims()),
        };
        assert_eq!(run(&s, &inputs).code, 0);
        let text = matrix(&s);
        assert_eq!(text.lines().next(), Some(HEADER), "{text}");
        for line in text.lines().filter(|line| line.starts_with('|')) {
            widths.push(cells(line).len());
        }
        let cell = &row_of(&text, "REQ-0001")[2];
        assert_eq!(
            cell.matches("(holds)").count(),
            count,
            "every component is named in the one cell: {cell}"
        );
    }
    assert!(widths.iter().all(|width| *width == 4), "{widths:?}");
}

// ---- Falsification 1 (ADR 0053): a gap is named, never blank ------------------------------------

#[test]
fn each_gap_is_named_and_no_cell_is_ever_blank() {
    let s = Scratch::new("gaps");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0005", "holds")]),
        trace: trace_of(vec![
            covered("REQ-0001", "Bare.", Value::Null, json!("machine"), &[]),
            covered("REQ-0002", "Empty source.", json!(""), Value::Null, &[]),
            covered(
                "REQ-0003",
                "Process.",
                json!("docs/p.md"),
                json!("process"),
                &[],
            ),
            covered(
                "REQ-0004",
                "Tested.",
                json!("docs/t.md"),
                json!("machine"),
                &["suite::one", "suite::two"],
            ),
            covered(
                "REQ-0005",
                "Linked.",
                json!("docs/l.md"),
                json!("machine"),
                &["suite::three"],
            ),
        ]),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let text = matrix(&s);
    for line in text.lines().filter(|line| line.starts_with('|')) {
        for cell in cells(line) {
            assert!(!cell.is_empty(), "a blank cell in `{line}`:\n{text}");
        }
    }
    let bare = row_of(&text, "REQ-0001");
    assert_eq!(bare[0], "no source");
    assert_eq!(bare[2], "no design element");
    assert_eq!(bare[3], "no test");
    let empty = row_of(&text, "REQ-0002");
    assert_eq!(empty[0], "no source", "an empty source is a gap: {empty:?}");
    assert_eq!(
        empty[3], "no test",
        "absent evidence is machine evidence: {empty:?}"
    );
    let process = row_of(&text, "REQ-0003");
    assert_eq!(process[3], "process evidence, no test asked", "{process:?}");
    let tested = row_of(&text, "REQ-0004");
    assert_eq!(tested[0], "docs/t.md");
    assert_eq!(tested[2], "no design element");
    assert!(tested[3].starts_with("2 tests"), "{tested:?}");
    assert!(
        tested[3].contains("suite::one") && tested[3].contains("suite::two"),
        "{tested:?}"
    );
    let linked = row_of(&text, "REQ-0005");
    assert!(linked[2].starts_with("Pump (holds)"), "{linked:?}");
    assert!(linked[3].starts_with("1 test"), "{linked:?}");
}

// ---- Falsification 2 (ADR 0053): a link that is not true is never plain covered ----------------

#[test]
fn a_suspect_link_shows_suspect_in_satisfied_by_never_plain_covered() {
    let s = Scratch::new("suspect");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![
            component("pump", "Pump", &["src/a.rs"], 0),
            component("store", "Store", &["src/b.rs"], 1),
        ]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "suspect"),
            link("src/b.rs", "REQ-0002", "orphaned"),
            link("src/a.rs", "REQ-0002", "holds"),
        ]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let text = matrix(&s);
    let one = row_of(&text, "REQ-0001")[2].clone();
    assert!(one.contains("Pump (suspect)"), "{one}");
    assert!(
        !one.contains("holds"),
        "a suspect link is not covered: {one}"
    );
    let two = row_of(&text, "REQ-0002")[2].clone();
    assert!(
        two.contains("Pump (holds)") && two.contains("Store (orphaned)"),
        "{two}"
    );
}

// ---- test outcomes, only with a report (ADR 0053 decision 5) ------------------------------------

const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="one">
    <testcase classname="suite" name="passes"/>
    <testcase classname="suite" name="fails"><failure message="exit 1"/></testcase>
    <testcase classname="suite" name="breaks"><error message="exit 9"/></testcase>
    <testcase classname="suite" name="skips"><skipped message="UNVERIFIABLE"/></testcase>
  </testsuite>
  <testsuite name="two">
    <testcase classname="other" name="a &amp; b"/>
  </testsuite>
</testsuites>
"#;

fn tested_inputs() -> Inputs {
    Inputs {
        diagram: diagram(vec![component("pump", "Pump", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace_of(vec![covered(
            "REQ-0001",
            "The pump shall report its health.",
            json!("docs/a.md"),
            json!("machine"),
            &[
                "suite::passes",
                "suite::fails",
                "suite::breaks",
                "suite::skips",
                "suite::missing",
                "other::a & b",
            ],
        )]),
    }
}

/// Run with `--test-report` (and `--root` the miniature repository) into `out_dir`.
fn convert_with_report(s: &Scratch, paths: &Paths, report: &Path, out_dir: &Path) -> Run {
    let out = Command::new(python3())
        .arg("-I")
        .arg(script())
        .arg("--root")
        .arg(s.repo())
        .arg("--test-report")
        .arg(report)
        .arg(&paths.diagram)
        .arg(&paths.satisfy)
        .arg(&paths.trace)
        .arg(out_dir)
        .current_dir(&s.0)
        .output()
        .expect("runs the converter");
    Run {
        code: out.status.code().expect("exits"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn a_report_adds_each_linked_tests_outcome_passed_failed_skipped_or_not_found() {
    let s = Scratch::new("report");
    repo(&s);
    let paths = tested_inputs().write(&s);
    let report = s.path("junit.xml");
    fs::write(&report, REPORT).expect("writes the report");
    let out = convert_with_report(&s, &paths, &report, &s.path("out"));
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[3].clone();
    assert!(cell.starts_with("6 tests"), "{cell}");
    for expected in [
        "suite::passes (passed)",
        "suite::fails (failed)",
        "suite::breaks (failed)",
        "suite::skips (skipped)",
        "suite::missing (not found)",
        "other::a & b (passed)",
    ] {
        assert!(cell.contains(expected), "`{expected}` in {cell}");
    }
}

#[test]
fn without_a_report_no_outcome_is_written() {
    let s = Scratch::new("no-report");
    repo(&s);
    let out = run(&s, &tested_inputs());
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[3].clone();
    assert!(cell.starts_with("6 tests"), "{cell}");
    assert!(cell.contains("suite::passes"), "{cell}");
    for word in ["passed", "failed", "skipped", "not found"] {
        assert!(!cell.contains(word), "an outcome without a report: {cell}");
    }
}

#[test]
fn a_run_without_the_flag_does_not_read_a_report_so_a_changed_report_changes_no_byte() {
    // Falsification 3 (ADR 0053): the committed matrix is a function of the repository.
    let s = Scratch::new("report-ignored");
    repo(&s);
    let paths = tested_inputs().write(&s);
    fs::write(s.path("junit.xml"), REPORT).expect("writes");
    let first = convert(Some(&s.repo()), &paths, &s.path("out-1"), &s.0);
    fs::write(s.path("junit.xml"), "this is not a report at all").expect("rewrites");
    let second = convert(Some(&s.repo()), &paths, &s.path("out-2"), &s.0);
    assert_eq!(
        (first.code, second.code),
        (0, 0),
        "{}{}",
        first.all(),
        second.all()
    );
    assert_eq!(tree(&s.path("out-1")), tree(&s.path("out-2")));
}

#[test]
fn two_runs_with_the_same_report_give_the_same_bytes() {
    // Falsification 4 (ADR 0053).
    let s = Scratch::new("report-determinism");
    repo(&s);
    let paths = tested_inputs().write(&s);
    let report = s.path("junit.xml");
    fs::write(&report, REPORT).expect("writes");
    let first = convert_with_report(&s, &paths, &report, &s.path("out-1"));
    let second = convert_with_report(&s, &paths, &report, &s.path("out-2"));
    assert_eq!((first.code, second.code), (0, 0));
    assert_eq!(tree(&s.path("out-1")), tree(&s.path("out-2")));
}

#[test]
fn a_report_only_changes_the_matrix_never_the_overlay() {
    let s = Scratch::new("report-overlay");
    repo(&s);
    let paths = tested_inputs().write(&s);
    let report = s.path("junit.xml");
    fs::write(&report, REPORT).expect("writes");
    assert_eq!(
        convert_with_report(&s, &paths, &report, &s.path("with")).code,
        0
    );
    assert_eq!(
        convert(Some(&s.repo()), &paths, &s.path("without"), &s.0).code,
        0
    );
    let (with, without) = (tree(&s.path("with")), tree(&s.path("without")));
    assert_eq!(with["overlay.json"], without["overlay.json"]);
    assert_ne!(with["matrix.md"], without["matrix.md"]);
}

#[test]
fn an_unreadable_or_malformed_report_is_exit_2_naming_it_and_writes_nothing() {
    let cases: [(&str, Option<&str>); 6] = [
        ("garbage", Some("not xml")),
        (
            "wrong root",
            Some("<report><testcase classname=\"a\" name=\"b\"/></report>"),
        ),
        (
            "empty classname",
            Some(
                "<testsuites><testsuite><testcase classname=\"\" name=\"b\"/></testsuite></testsuites>",
            ),
        ),
        (
            "conflicting duplicate",
            Some(
                "<testsuites><testsuite><testcase classname=\"a\" name=\"b\"/><testcase classname=\"a\" name=\"b\"><failure/></testcase></testsuite></testsuites>",
            ),
        ),
        (
            "a DTD",
            Some("<!DOCTYPE a [<!ENTITY x \"y\">]><testsuites/>"),
        ),
        ("missing file", None),
    ];
    for (label, text) in cases {
        let s = Scratch::new("report-bad");
        repo(&s);
        let paths = tested_inputs().write(&s);
        let report = s.path("junit.xml");
        if let Some(text) = text {
            fs::write(&report, text).expect("writes");
        }
        let out = convert_with_report(&s, &paths, &report, &s.path("out"));
        assert_eq!(out.code, 2, "{label}: {}", out.all());
        assert!(
            out.stderr.contains("junit.xml"),
            "{label} names the report: {}",
            out.stderr
        );
        assert!(!s.path("out").exists(), "{label}: nothing is written");
    }
}

#[test]
fn a_report_with_no_test_cases_reads_every_linked_test_as_not_found_and_a_lone_testsuite_root_is_read()
 {
    let s = Scratch::new("report-shapes");
    repo(&s);
    let paths = tested_inputs().write(&s);
    let report = s.path("junit.xml");
    fs::write(&report, "<testsuites/>").expect("writes");
    assert_eq!(
        convert_with_report(&s, &paths, &report, &s.path("empty")).code,
        0
    );
    let cell = row_of(
        &fs::read_to_string(s.path("empty/matrix.md")).expect("reads"),
        "REQ-0001",
    )[3]
    .clone();
    assert_eq!(cell.matches("(not found)").count(), 6, "{cell}");
    fs::write(
        &report,
        "<testsuite name=\"x\"><testcase classname=\"suite\" name=\"passes\"/></testsuite>",
    )
    .expect("writes");
    assert_eq!(
        convert_with_report(&s, &paths, &report, &s.path("lone")).code,
        0
    );
    let cell = row_of(
        &fs::read_to_string(s.path("lone/matrix.md")).expect("reads"),
        "REQ-0001",
    )[3]
    .clone();
    assert!(cell.contains("suite::passes (passed)"), "{cell}");
}

// ---- Falsification 1 and 2: determinism, nothing else touched ---------------------------------

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, found: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("lists") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(base, &path, found);
            } else {
                let name = path.strip_prefix(base).expect("under base");
                found.insert(
                    name.to_string_lossy().into_owned(),
                    fs::read(&path).expect("reads"),
                );
            }
        }
    }
    let mut found = BTreeMap::new();
    walk(dir, dir, &mut found);
    found
}

fn full_inputs() -> Inputs {
    Inputs {
        diagram: diagram(vec![
            component("pump", "Pump", &["src/a.rs", "docs/other.md"], 0),
            component("store", "Store", &["src/b.rs", "src/gone.rs"], 1),
            component("sketch", "Sketch", &[], 2),
        ]),
        satisfy: satisfy(vec![
            link("src/a.rs", "REQ-0001", "holds"),
            link("src/b.rs", "REQ-0002", "suspect"),
        ]),
        trace: trace(&claims()),
    }
}

#[test]
fn two_runs_over_the_same_inputs_give_byte_identical_directories() {
    // Falsification 1 (ADR 0052).
    let s = Scratch::new("determinism");
    repo(&s);
    let inputs = full_inputs();
    let paths = inputs.write(&s);
    let first = convert(Some(&s.repo()), &paths, &s.path("out-1"), &s.0);
    let second = convert(Some(&s.repo()), &paths, &s.path("out-2"), &s.0);
    assert_eq!(
        (first.code, second.code),
        (3, 3),
        "{}{}",
        first.all(),
        second.all()
    );
    let (one, two) = (tree(&s.path("out-1")), tree(&s.path("out-2")));
    assert_eq!(
        one.keys().collect::<Vec<_>>(),
        vec!["matrix.md", "overlay.json"]
    );
    assert_eq!(one, two);
    let overlay = String::from_utf8(one["overlay.json"].clone()).expect("utf-8");
    assert!(overlay.ends_with("}\n"), "a trailing newline");
}

#[test]
fn the_inputs_and_the_corpus_are_byte_identical_after_a_run_and_nothing_else_is_written() {
    // Falsification 2 (ADR 0052): never writes the user's diagram or the corpus.
    let s = Scratch::new("read-only");
    repo(&s);
    let inputs = full_inputs();
    let paths = inputs.write(&s);
    let before = tree(&s.0);
    let out = convert(Some(&s.repo()), &paths, &s.path("out"), &s.0);
    assert_eq!(out.code, 3, "{}", out.all());
    let mut after = tree(&s.0);
    let written: Vec<String> = after
        .keys()
        .filter(|name| name.starts_with("out/"))
        .cloned()
        .collect();
    assert_eq!(written, vec!["out/matrix.md", "out/overlay.json"]);
    for name in &written {
        after.remove(name);
    }
    assert_eq!(before, after, "everything but out/ is as it was");
}

#[test]
fn the_script_never_imports_a_network_module() {
    // Falsification 2: offline, standard library only.
    let text = fs::read_to_string(script()).expect("reads the script");
    for module in ["socket", "urllib", "http", "requests", "ssl", "subprocess"] {
        assert!(
            !text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with(&format!("import {module}"))
                    || line.starts_with(&format!("from {module}"))
            }),
            "the converter imports {module}"
        );
    }
}

// ---- refusals (spec choice B1) ----------------------------------------------------------------

fn refused(name: &str, edit: impl FnOnce(&mut Inputs, &Scratch), expect: &str) {
    let s = Scratch::new(name);
    repo(&s);
    let mut inputs = full_inputs();
    edit(&mut inputs, &s);
    let out = run(&s, &inputs);
    assert_eq!(out.code, 2, "{name}: {}", out.all());
    assert!(out.stderr.contains(expect), "{name}: {}", out.stderr);
    assert!(!s.path("out").exists(), "{name}: a refusal writes nothing");
}

#[test]
fn a_wrong_schema_is_exit_2_naming_it_and_writes_nothing() {
    refused(
        "bad-satisfy-schema",
        |i, _| i.satisfy["schema"] = json!("req.satisfy/2"),
        "req.satisfy/2",
    );
    refused(
        "bad-trace-schema",
        |i, _| i.trace["schema"] = json!("req.trace/2"),
        "req.trace/2",
    );
    refused(
        "bad-diagram-type",
        |i, _| i.diagram["diagram_type"] = json!("workflow"),
        "workflow",
    );
    refused(
        "bad-diagram-version",
        |i, _| i.diagram["schema_version"] = json!(2),
        "schema_version",
    );
}

#[test]
fn a_malformed_document_is_exit_2_naming_the_input() {
    refused(
        "bad-outcome",
        |i, _| i.satisfy["links"][0]["outcome"] = json!("probably"),
        "probably",
    );
    refused(
        "links-not-array",
        |i, _| i.satisfy["links"] = json!({}),
        "links",
    );
    refused(
        "components-not-array",
        |i, _| i.diagram["components"] = json!("pump"),
        "components",
    );
    refused(
        "sources-not-array",
        |i, _| i.diagram["components"][0]["sources"] = json!("src/a.rs"),
        "sources",
    );
    refused(
        "nodes-not-array",
        |i, _| i.trace["nodes"] = json!(3),
        "nodes",
    );
}

#[test]
fn malformed_json_and_a_missing_input_are_exit_2() {
    let s = Scratch::new("malformed-json");
    repo(&s);
    let paths = full_inputs().write(&s);
    fs::write(&paths.satisfy, "{not json").expect("overwrites");
    let out = convert(Some(&s.repo()), &paths, &s.path("out"), &s.0);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.stderr.contains("satisfy.json"), "{}", out.stderr);
    assert!(!s.path("out").exists());

    let paths = Paths {
        diagram: s.path("absent.json"),
        ..paths
    };
    let out = convert(Some(&s.repo()), &paths, &s.path("out"), &s.0);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.stderr.contains("absent.json"), "{}", out.stderr);
}

#[test]
fn an_existing_non_empty_out_dir_is_exit_2_and_is_left_as_it_was() {
    let s = Scratch::new("occupied");
    repo(&s);
    let paths = full_inputs().write(&s);
    fs::create_dir_all(s.path("out")).expect("makes out");
    fs::write(s.path("out/earlier.txt"), "an earlier page").expect("writes");
    let out = convert(Some(&s.repo()), &paths, &s.path("out"), &s.0);
    assert_eq!(out.code, 2, "{}", out.all());
    assert!(out.stderr.contains("out"), "{}", out.stderr);
    assert_eq!(
        tree(&s.path("out")).keys().collect::<Vec<_>>(),
        vec!["earlier.txt"]
    );
    // An existing *empty* directory is fine.
    fs::remove_file(s.path("out/earlier.txt")).expect("removes");
    let out = convert(Some(&s.repo()), &paths, &s.path("out"), &s.0);
    assert_eq!(out.code, 3, "{}", out.all());
}

#[test]
fn a_wrong_argument_list_is_exit_2_with_usage() {
    let out = Command::new(python3())
        .arg("-I")
        .arg(script())
        .arg("only-one-argument")
        .output()
        .expect("runs the converter");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage:"));
}

#[test]
fn a_namespaced_report_is_read_by_local_names() {
    // `req gate --test-report` reads a root that declares a default namespace; so does this.
    let s = Scratch::new("report-namespace");
    repo(&s);
    let paths = tested_inputs().write(&s);
    let report = s.path("junit.xml");
    fs::write(
        &report,
        "<testsuites xmlns=\"urn:junit\"><testsuite name=\"x\">\
         <testcase classname=\"suite\" name=\"passes\"/>\
         <testcase classname=\"suite\" name=\"fails\"><failure message=\"m\"/></testcase>\
         </testsuite></testsuites>",
    )
    .expect("writes");
    let out = convert_with_report(&s, &paths, &report, &s.path("out"));
    assert_eq!(out.code, 0, "{}", out.all());
    let cell = row_of(&matrix(&s), "REQ-0001")[3].clone();
    assert!(cell.contains("suite::passes (passed)"), "{cell}");
    assert!(cell.contains("suite::fails (failed)"), "{cell}");
    assert!(cell.contains("suite::missing (not found)"), "{cell}");
}

#[test]
fn doctype_text_inside_a_comment_or_cdata_is_not_a_declaration() {
    let s = Scratch::new("report-doctype-text");
    repo(&s);
    let paths = tested_inputs().write(&s);
    let report = s.path("junit.xml");
    fs::write(
        &report,
        "<testsuites><!-- <!DOCTYPE a [<!ENTITY x \"y\">]> --><testsuite name=\"x\">\
         <testcase classname=\"suite\" name=\"passes\">\
         <system-out><![CDATA[<!ENTITY x \"y\"> <!DOCTYPE z>]]></system-out></testcase>\
         </testsuite></testsuites>",
    )
    .expect("writes");
    let out = convert_with_report(&s, &paths, &report, &s.path("out"));
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(
        row_of(&matrix(&s), "REQ-0001")[3].contains("suite::passes (passed)"),
        "{}",
        matrix(&s)
    );
}

#[test]
fn a_real_doctype_is_still_refused_with_or_without_an_entity() {
    for (label, text) in [
        ("entity", "<!DOCTYPE a [<!ENTITY x \"y\">]><testsuites/>"),
        ("plain", "<!DOCTYPE testsuites><testsuites/>"),
        (
            "after a comment",
            "<!-- hi --><!DOCTYPE testsuites><testsuites/>",
        ),
    ] {
        let s = Scratch::new("report-doctype-real");
        repo(&s);
        let paths = tested_inputs().write(&s);
        let report = s.path("junit.xml");
        fs::write(&report, text).expect("writes");
        let out = convert_with_report(&s, &paths, &report, &s.path("out"));
        assert_eq!(out.code, 2, "{label}: {}", out.all());
        assert!(out.stderr.contains("junit.xml"), "{label}: {}", out.stderr);
        assert!(!s.path("out").exists(), "{label}");
    }
}
