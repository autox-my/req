//! `tools/graph/arch_overlay.py` — an archify `architecture` diagram, `req satisfy --format json` and `req trace --format
//! json` in, an overlay diagram and a claims × components matrix out (ADR 0052 decisions 1–5, unit O-1).
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
    let tag = tag_of(&overlay(&s), "pump").to_string();
    assert!(tag.starts_with("SUSPECT"), "{tag}");
    assert!(
        tag.contains("REQ-0001") && tag.contains("REQ-0002"),
        "{tag}"
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
        let tag = tag_of(&overlay(&s), "pump").to_string();
        assert!(
            tag.starts_with(ORDER[start].1),
            "from {}: the status word is {:?}, the tag is {tag:?}",
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
    assert!(tag_of(&overlay(&s), "pump").starts_with("STALE"));
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
    assert_eq!(tag_of(&overlay, "sketch"), "NO CLAIM");
    assert_eq!(tag_of(&overlay, "quiet"), "NO CLAIM");
    assert!(tag_of(&overlay, "pump").starts_with("HOLDS"));
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
            tag_of(&overlay(&s), id).to_string(),
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
    assert!(first.0.starts_with("SUSPECT"), "{}", first.0);
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
    let tag = tag_of(&overlay, "pump");
    assert!(
        tag.starts_with("HOLDS") && !tag.contains("REQ-0002"),
        "{tag}"
    );
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
    let tag = tag_of(&overlay(&s), "pump").to_string();
    assert!(
        tag.starts_with("SUSPECT") && tag.contains("REQ-0001"),
        "{tag}"
    );
    // The claim appears once, though two source entries cite the file.
    assert_eq!(tag.matches("REQ-0001").count(), 1, "{tag}");
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
    assert!(tag_of(&overlay, "pump").starts_with("STALE"));
    assert!(tag_of(&overlay, "ok").starts_with("HOLDS"));
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
            c.as_object_mut().expect("object").remove("tag");
            c
        };
        assert_eq!(
            strip(old),
            strip(new),
            "same id, label, geometry and sources, same order"
        );
    }
    // The author's tag and sublabel are kept and added to, never replaced.
    let tag = tag_of(&overlay, "pump");
    assert!(tag.contains("app team") && tag.contains("HOLDS"), "{tag}");
    assert_eq!(
        component_named(&overlay, "pump")["sublabel"],
        "health endpoint"
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
fn a_long_claim_list_is_cut_to_fit_the_tag_and_the_status_card_holds_every_id() {
    // archify's tag is one unwrapped line at the 6 px floor: 33 columns in a 150 px box (spec, archify 7e45f36).
    let s = Scratch::new("long-tag");
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
    let tag = tag_of(&overlay, "pump");
    assert!(tag.chars().count() <= 40, "{tag}");
    assert!(tag.starts_with("HOLDS") && tag.contains("+"), "{tag}");
    let card = card(&overlay, STATUS_CARD).join("\n");
    for id in &ids {
        assert!(card.contains(id.as_str()), "{id} missing from {card}");
    }
}

// ---- the matrix -------------------------------------------------------------------------------

#[test]
fn the_matrix_has_a_row_per_applying_claim_a_column_per_component_and_lists_the_unreached() {
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
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let text = matrix(&s);
    let lines: Vec<&str> = text.lines().collect();
    // Columns follow the diagram's order (Zeta, then Alpha); rows are claims by id.
    assert_eq!(lines[0], "| claim | Zeta | Alpha |", "{text}");
    assert_eq!(lines[1], "| --- | --- | --- |", "{text}");
    assert_eq!(lines[2], "| REQ-0001 | holds | holds_weakened |", "{text}");
    assert_eq!(lines[3], "| REQ-0002 | suspect |  |", "{text}");
    assert_eq!(lines[4], "| REQ-0003 |  |  |", "{text}");
    // REQ-0004 does not apply in the selection: no row.
    assert!(!text.contains("| REQ-0004"), "{text}");
    // The claim no component reaches is listed, with its statement.
    let unreached = text
        .split("## Claims no component reaches")
        .nth(1)
        .expect("the list");
    assert!(unreached.contains("REQ-0003"), "{text}");
    assert!(
        unreached.contains("The pump shall encrypt data at rest."),
        "{text}"
    );
    assert!(
        !unreached.contains("REQ-0001") && !unreached.contains("REQ-0002"),
        "{text}"
    );
    assert!(text.ends_with('\n'));
    // The overlay's card says the same.
    let overlay = overlay(&s);
    let items = card(&overlay, UNREACHED_CARD);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(items[0].contains("REQ-0003"), "{items:?}");
}

#[test]
fn a_cell_holds_the_worst_outcome_when_a_component_reaches_a_claim_through_two_files() {
    let s = Scratch::new("cell-worst");
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
            link("src/b.rs", "REQ-0001", "unratified"),
        ]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    assert!(
        matrix(&s).contains("| REQ-0001 | unratified |"),
        "{}",
        matrix(&s)
    );
}

#[test]
fn a_label_with_a_pipe_does_not_break_the_table() {
    let s = Scratch::new("pipe");
    repo(&s);
    let inputs = Inputs {
        diagram: diagram(vec![component("pump", "Read | Write", &["src/a.rs"], 0)]),
        satisfy: satisfy(vec![link("src/a.rs", "REQ-0001", "holds")]),
        trace: trace(&claims()),
    };
    assert_eq!(run(&s, &inputs).code, 0);
    let text = matrix(&s);
    assert!(
        text.lines()
            .next()
            .expect("header")
            .contains("Read \\| Write"),
        "{text}"
    );
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
