//! `docs/architecture.html` (the overview), `docs/architecture-detail.html` and the files behind them stay true to the
//! tool (ADR 0052 decision 7, step 3, units O-3 and O-4).
//!
//! The two diagrams (`docs/architecture/req.archify.json`, the detail, and `req-overview.archify.json`) are an
//! architect's reading of the code and are reviewed like a document. `overlay.json` and `matrix.md` (the detail) and
//! `overview-overlay.json` and `overview-matrix.md` are computed from them, from `req satisfy --format json` and from `req trace
//! --format json` of this repository, by `tools/graph/build_architecture.py`; they are checked the way a projection
//! is: regenerated and compared, never hand-edited. The page itself is rendered by archify (Node and a browser), which
//! the gate cannot run, so what a test can read back of it is the part archify writes as attributes: every component's
//! id, label and tag, the subtitle and every source link (ADR 0052's note names the rest as a gap).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The two diagrams and what is made from each: (the architect's diagram, its overlay, its matrix, its page).
const VIEWS: [(&str, &str, &str, &str); 2] = [
    (
        "req-overview.archify.json",
        "overview-overlay.json",
        "overview-matrix.md",
        "docs/architecture.html",
    ),
    (
        "req.archify.json",
        "overlay.json",
        "matrix.md",
        "docs/architecture-detail.html",
    ),
];

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// `python3` as an absolute path, resolved before the child's `PATH` is rewritten, so a missing interpreter is a named
/// environment problem and not a failing check.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/graph/build_architecture.py - install it first");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

/// A scratch copy of the parts of the repository the build reads, never this checkout.
struct Copy(PathBuf);

impl Copy {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-architecture-page-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        for tree in [
            "claims",
            "aliases",
            ".req",
            "src",
            "decisions",
            "baselines",
            "docs/architecture",
            "tools/graph",
        ] {
            copy_tree(&root().join(tree), &dir.join(tree));
        }
        // Every file either diagram cites, wherever it lives.
        for (diagram, _, _, _) in VIEWS {
            let diagram: Value =
                serde_json::from_str(&read(&root().join("docs/architecture").join(diagram)))
                    .expect("the diagram is JSON");
            for component in diagram["components"].as_array().expect("components") {
                for source in component["sources"].as_array().into_iter().flatten() {
                    let path = source["path"].as_str().expect("a source path");
                    let target = dir.join(path);
                    if !target.exists() {
                        fs::create_dir_all(target.parent().expect("a parent"))
                            .expect("makes the dir");
                        fs::copy(root().join(path), &target).expect("copies a cited file");
                    }
                }
            }
        }
        Copy(dir)
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("makes the dir");
    for entry in fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let entry = entry.expect("a dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copies a file");
        }
    }
}

struct Run {
    code: Option<i32>,
    said: String,
}

/// `python3 <repo>/tools/graph/build_architecture.py <args>` with `req` first on `PATH`, run from `repo` (the script
/// reads the repository it lives in, so a copy checks itself).
fn build(repo: &Path, args: &[&str]) -> Run {
    let interpreter = python3();
    let mut entries = vec![req_bin().parent().expect("a parent dir").to_path_buf()];
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(entries).expect("valid PATH");
    let output = Command::new(interpreter)
        .arg("tools/graph/build_architecture.py")
        .args(args)
        .current_dir(repo)
        .env("PATH", path)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/graph/build_architecture.py");
    Run {
        code: output.status.code(),
        said: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

#[test]
fn the_architecture_files_are_current_with_the_tool() {
    let run = build(&root(), &["--check", "docs/architecture"]);
    match run.code {
        Some(0) => {}
        Some(3) => panic!(
            "UNVERIFIABLE, not stale: the build could not run `req`:\n{}",
            run.said
        ),
        Some(1) => panic!(
            "docs/architecture is stale (regenerate with `python3 tools/graph/build_architecture.py --out \
             docs/architecture`, then render docs/architecture.html again):\n{}",
            run.said
        ),
        other => panic!(
            "tools/graph/build_architecture.py ended abnormally ({other:?}):\n{}",
            run.said
        ),
    }
    // Both diagrams are covered: the check names each generated file as current (O-4).
    for (_, overlay, matrix, _) in VIEWS {
        for name in [overlay, matrix] {
            assert!(
                run.said
                    .contains(&format!("[CURRENT] docs/architecture/{name}")),
                "--check must cover {name}:\n{}",
                run.said
            );
        }
    }
}

/// ADR 0052 falsification 3 as the page shows it: a linked claim whose meaning moved must turn its components SUSPECT,
/// so a committed overlay that still says HOLDS is stale and the check says which file.
#[test]
fn a_tightened_claim_makes_the_overlay_stale() {
    let copy = Copy::new("tightened");
    let claim = copy.path("claims/REQ-0028.yml");
    let text = read(&claim);
    let tightened = text.replace(
        "The req tool shall fail the verification",
        "The req tool shall fail within 200 ms the verification",
    );
    assert_ne!(
        text, tightened,
        "the statement REQ-0028 is expected to open with that sentence"
    );
    fs::write(&claim, tightened).expect("tightens the claim in the copy");
    let run = build(&copy.0, &["--check", "docs/architecture"]);
    assert_eq!(
        run.code,
        Some(1),
        "a moved claim must make the page stale:\n{}",
        run.said
    );
    // Both views cite src/baseline.rs, where REQ-0028 is linked: the detail and the overview are each stale, and each
    // is named (an overview the check skipped would stay HOLDS).
    for (_, overlay, _, _) in VIEWS {
        assert!(
            run.said
                .contains(&format!("STALE docs/architecture/{overlay}")),
            "the stale file {overlay} must be named:\n{}",
            run.said
        );
    }
    let regenerated = build(&copy.0, &["--out", "docs/architecture"]);
    assert_eq!(regenerated.code, Some(0), "{}", regenerated.said);
    for (_, overlay, _, _) in VIEWS {
        let regenerated: Value =
            serde_json::from_str(&read(&copy.path("docs/architecture").join(overlay)))
                .expect("the overlay is JSON");
        let baselines = regenerated["components"]
            .as_array()
            .expect("components")
            .iter()
            .find(|c| {
                c["label"]
                    .as_str()
                    .is_some_and(|l| l.starts_with("Baseline"))
            })
            .unwrap_or_else(|| panic!("{overlay} has a Baseline component"));
        assert!(
            baselines["sublabel"]
                .as_str()
                .is_some_and(|status| status.starts_with("SUSPECT")),
            "{overlay}: the baselines component must now be drawn SUSPECT: {baselines}"
        );
    }
}

/// O-4: the front page is a diagram a reader can take in, so it is small. The detail keeps every part.
#[test]
fn the_overview_has_at_most_twelve_components_and_the_detail_stays_whole() {
    let overview: Value = serde_json::from_str(&read(
        &root().join("docs/architecture/req-overview.archify.json"),
    ))
    .expect("the overview is JSON");
    let parts = overview["components"].as_array().expect("components").len();
    assert!(
        (1..=12).contains(&parts),
        "the overview has {parts} components, at most 12"
    );
    let detail: Value =
        serde_json::from_str(&read(&root().join("docs/architecture/req.archify.json")))
            .expect("the detail is JSON");
    assert!(
        detail["components"].as_array().expect("components").len() > parts,
        "the detailed diagram keeps its own, larger page"
    );
    assert_eq!(
        overview["meta"]["repository"]["revision"], detail["meta"]["repository"]["revision"],
        "the overview is pinned to the revision the detail is"
    );
}

/// O-4-fix: "no claim" on a box means none of its cited files declares a link, not that its area has no satisfied
/// requirement (REQ-0001, area authoring, is drawn under Identity, where src/canon.rs declares it). Each page's reading
/// card says so, in the sentence the status line needs beside it.
#[test]
fn each_reading_card_says_where_a_status_comes_from() {
    for (_, overlay, _, _) in VIEWS {
        let overlay_json: Value =
            serde_json::from_str(&read(&root().join("docs/architecture").join(overlay)))
                .expect("the overlay is JSON");
        let card = overlay_json["cards"]
            .as_array()
            .expect("cards")
            .iter()
            .find(|card| card["title"] == "Reading the picture")
            .unwrap_or_else(|| panic!("{overlay} has a reading card"));
        let said = card["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter_map(Value::as_str)
            .any(|item| {
                item.contains("links in the files a box cites")
                    && item.contains("area")
                    && item.contains("no claim")
            });
        assert!(
            said,
            "{overlay}: the reading card must say a status comes from the links in the cited files, not a claim's area: {card}"
        );
    }
}

/// O-4: each page names the other, in a card.
#[test]
fn each_page_points_at_the_other() {
    let overview = read(&root().join("docs/architecture.html"));
    let detail = read(&root().join("docs/architecture-detail.html"));
    // A card is plain text in archify 7e45f36 (no link), so the item ends with the sibling page's file name.
    assert!(
        overview.contains("architecture-detail.html</li>"),
        "the overview page must point at the detailed page in a card"
    );
    assert!(
        detail.contains("architecture.html</li>"),
        "the detailed page must point at the overview page in a card"
    );
    // The Pages index promises every part, so its one item links both pages, the overview first.
    let index = read(&root().join("docs/index.html"));
    let overview_link = index
        .find("href=\"architecture.html\"")
        .expect("the index links the overview page");
    let detail_link = index
        .find("href=\"architecture-detail.html\"")
        .expect("the index links the detailed page");
    assert!(
        overview_link < detail_link,
        "the overview is linked before the detail"
    );
    assert_eq!(
        index.matches("architecture.html\"").count()
            + index.matches("architecture-detail.html\"").count(),
        2,
        "one item, two links"
    );
    let item = index
        .lines()
        .find(|line| line.contains("href=\"architecture.html\""))
        .expect("the item");
    assert!(
        item.contains("href=\"architecture-detail.html\""),
        "both links are in the one item: {item}"
    );
    let detail: Value =
        serde_json::from_str(&read(&root().join("docs/architecture/req.archify.json")))
            .expect("the detail is JSON");
    let parts = detail["components"].as_array().expect("components").len();
    assert!(
        item.contains(&format!("{parts} parts")),
        "the item names the detailed page's {parts} parts: {item}"
    );
}

/// ADR 0052 decision 4: a cited file that is gone is a failure of the page, with the converter's reason, never a
/// quietly redrawn component.
#[test]
fn a_deleted_cited_file_fails_the_check_with_the_stale_reason() {
    let copy = Copy::new("deleted");
    fs::remove_file(copy.path("src/junit.rs")).expect("deletes a cited file in the copy");
    let run = build(&copy.0, &["--check", "docs/architecture"]);
    assert_eq!(run.code, Some(1), "{}", run.said);
    assert!(
        run.said.contains("cannot be made honestly") && run.said.contains("src/junit.rs"),
        "the converter's stale reason must be reported, naming the file:\n{}",
        run.said
    );
}

/// A `req` that cannot read what it judges has not judged it: `req satisfy` exits 3 and lists the file in `unreadable`
/// while still printing a document whose `links` are missing that file's. Writing or comparing the overlay from it would
/// draw the parts of that file as NO CLAIM and call the page current.
#[test]
fn an_unreadable_source_is_unverifiable_never_current() {
    use std::os::unix::fs::PermissionsExt;
    let copy = Copy::new("unreadable");
    let before = read(&copy.path("docs/architecture/overlay.json"));
    let audit = copy.path("src/audit.rs");
    fs::set_permissions(&audit, fs::Permissions::from_mode(0o000)).expect("chmod 000 the copy");
    assert!(
        fs::read(&audit).is_err(),
        "this test needs a user that mode 000 locks out (not root)"
    );
    let check = build(&copy.0, &["--check", "docs/architecture"]);
    assert_eq!(check.code, Some(3), "{}", check.said);
    assert!(
        check.said.contains("UNVERIFIABLE") && check.said.contains("src/audit.rs"),
        "the unreadable file must be named:\n{}",
        check.said
    );
    let out = build(&copy.0, &["--out", "docs/architecture"]);
    assert_eq!(out.code, Some(3), "{}", out.said);
    assert_eq!(
        before,
        read(&copy.path("docs/architecture/overlay.json")),
        "an UNVERIFIABLE run must not rewrite the committed overlay"
    );
}

/// A `req` that prints another schema is not the tool the converter was written against.
#[test]
fn another_schema_is_unverifiable() {
    use std::os::unix::fs::PermissionsExt;
    let copy = Copy::new("schema");
    let stub = copy.path("stub-req");
    fs::write(
        &stub,
        "#!/bin/sh\ncase \"$3\" in\n  satisfy) echo '{\"schema\":\"req.satisfy/2\",\"links\":[],\"unreadable\":[]}' ;;\n  *) echo '{\"schema\":\"req.trace/1\"}' ;;\nesac\nexit 0\n",
    )
    .expect("writes a stub req");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    let run = build(
        &copy.0,
        &[
            "--req",
            stub.to_str().expect("utf-8"),
            "--check",
            "docs/architecture",
        ],
    );
    assert_eq!(run.code, Some(3), "{}", run.said);
    assert!(
        run.said.contains("UNVERIFIABLE") && run.said.contains("req.satisfy/2"),
        "the schema req reported must be named:\n{}",
        run.said
    );
}

/// The converter refusing an input (exit 2) cannot be fixed by regenerating, so it is not a stale page: it is
/// UNVERIFIABLE, with the converter's message and no advice to regenerate.
#[test]
fn a_refused_input_is_unverifiable_not_stale() {
    let copy = Copy::new("refused");
    fs::write(
        copy.path("docs/architecture/req.archify.json"),
        "{ not json\n",
    )
    .expect("breaks the diagram in the copy");
    let run = build(&copy.0, &["--check", "docs/architecture"]);
    assert_eq!(run.code, Some(3), "{}", run.said);
    assert!(
        run.said.contains("UNVERIFIABLE") && run.said.contains("req.archify.json"),
        "the converter's refusal must be reported:\n{}",
        run.said
    );
    assert!(
        !run.said.contains("regenerate"),
        "a refused input is not fixed by regenerating:\n{}",
        run.said
    );
}

#[test]
fn the_check_is_unverifiable_without_req() {
    let interpreter = python3();
    let output = Command::new(interpreter)
        .args([
            "tools/graph/build_architecture.py",
            "--check",
            "docs/architecture",
        ])
        .current_dir(root())
        .env("PATH", "")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the script");
    assert_eq!(
        output.status.code(),
        Some(3),
        "no `req` is UNVERIFIABLE, not stale"
    );
    let usage = Command::new(python3())
        .args(["tools/graph/build_architecture.py"])
        .current_dir(root())
        .output()
        .expect("runs the script");
    assert_eq!(usage.status.code(), Some(2), "no mode is a usage error");
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The page is archify's rendering of `overlay.json`. archify does not embed the document itself (the page holds a
/// rendered SVG, not a JSON copy), so a test cannot compare documents; it can read back what archify writes as
/// attributes. A page rendered from an older overlay differs in a tag, a label, the subtitle or a source link.
#[test]
fn the_page_carries_what_the_overlay_says() {
    for (_, overlay, _, page) in VIEWS {
        page_carries(page, overlay);
    }
}

fn page_carries(page: &str, overlay: &str) {
    let page = read(&root().join(page));
    let overlay: Value =
        serde_json::from_str(&read(&root().join("docs/architecture").join(overlay)))
            .expect("the overlay is JSON");
    let repository = &overlay["meta"]["repository"];
    let revision = repository["revision"].as_str().expect("a pinned revision");
    let subtitle = overlay["meta"]["subtitle"].as_str().expect("a subtitle");
    assert!(
        page.contains(&escape(subtitle)),
        "the page's subtitle must be the overlay's (revision, rules_v, archify):\n{subtitle}"
    );
    for component in overlay["components"].as_array().expect("components") {
        let id = component["id"].as_str().expect("an id");
        let label = component["label"].as_str().expect("a label");
        assert!(
            page.contains(&format!(
                "data-node-id=\"{id}\" data-node-label=\"{}\"",
                escape(label)
            )),
            "the page has no node {id} labelled {label}: render it again from overlay.json"
        );
        // O-4: the status is the sublabel, drawn at the overview zoom; the tag is the author's text.
        let status = component["sublabel"].as_str().expect("a status line");
        assert!(
            page.contains(&format!("data-node-sublabel=\"{}\"", escape(status))),
            "the page does not show the status `{status}` of {id}: render it again from the overlay"
        );
        if let Some(tag) = component["tag"].as_str() {
            assert!(
                page.contains(&format!("data-node-tag=\"{}\"", escape(tag))),
                "the page does not carry the tag `{tag}` of {id}: render it again from overlay.json"
            );
        }
        for source in component["sources"].as_array().into_iter().flatten() {
            let path = source["path"].as_str().expect("a source path");
            let line = source["line"].as_u64().expect("a start line");
            let end = source["end_line"].as_u64().expect("an end line");
            let anchor = if line == end {
                format!("#L{line}")
            } else {
                format!("#L{line}-L{end}")
            };
            assert!(
                page.contains(&format!(
                    "https://github.com/autox-my/req/blob/{revision}/{path}{anchor}"
                )),
                "the page does not link {path}{anchor} at revision {revision}"
            );
        }
    }
}

/// archify embeds the evidence it verified as JSON (`archify-source-evidence-data`: the repository, and every node's
/// sources). That is a document a test can read back, so the page's evidence must equal the overlay's `meta.repository`
/// and component `sources`: a page rendered before a citation or the pinned revision changed fails here.
#[test]
fn the_page_embeds_the_overlays_source_evidence() {
    for (_, overlay, _, page) in VIEWS {
        page_embeds_evidence(page, overlay);
    }
}

fn page_embeds_evidence(page: &str, overlay: &str) {
    let page = read(&root().join(page));
    let opening = "<script id=\"archify-source-evidence-data\" type=\"application/json\">";
    let start = page
        .find(opening)
        .expect("the page embeds its source evidence")
        + opening.len();
    let end = start + page[start..].find("</script>").expect("the script closes");
    let embedded: Value =
        serde_json::from_str(&page[start..end]).expect("the embedded evidence is JSON");
    let overlay: Value =
        serde_json::from_str(&read(&root().join("docs/architecture").join(overlay)))
            .expect("the overlay is JSON");
    assert_eq!(
        embedded["verified"],
        Value::Bool(true),
        "archify verified the citations"
    );
    let repository = &overlay["meta"]["repository"];
    assert_eq!(embedded["repository"]["url"], repository["url"]);
    assert_eq!(embedded["repository"]["revision"], repository["revision"]);
    let mut references = 0;
    let mut cited = 0;
    for component in overlay["components"].as_array().expect("components") {
        let id = component["id"].as_str().expect("an id");
        let Some(sources) = component["sources"].as_array() else {
            assert!(
                embedded["nodes"].get(id).is_none(),
                "the page cites sources for {id}, which the overlay gives none"
            );
            continue;
        };
        cited += 1;
        references += sources.len();
        let shown = embedded["nodes"][id]
            .as_array()
            .unwrap_or_else(|| panic!("the page has no sources for {id}: render it again"));
        assert_eq!(shown.len(), sources.len(), "the sources of {id} differ");
        for (page_source, source) in shown.iter().zip(sources) {
            assert_eq!(page_source["path"], source["path"], "{id}");
            assert_eq!(page_source["line"], source["line"], "{id}");
            assert_eq!(page_source["endLine"], source["end_line"], "{id}");
            assert_eq!(page_source.get("label"), source.get("label"), "{id}");
        }
    }
    assert_eq!(
        embedded["nodes"].as_object().expect("nodes").len(),
        cited,
        "the page cites sources for a node the overlay does not"
    );
    assert_eq!(embedded["referenceCount"], Value::from(references));
}

/// The page is committed, standalone and offline: no script or stylesheet is fetched, and no path of the machine that
/// rendered it leaks into it.
#[test]
fn the_pages_are_standalone_and_leak_no_local_path() {
    for (_, _, _, page) in VIEWS {
        standalone(page);
    }
}

fn standalone(page: &str) {
    let page = read(&root().join(page));
    for fetched in ["<script src=", "<link rel=\"stylesheet\"", "@import url("] {
        assert!(
            !page.contains(fetched),
            "the page must not fetch `{fetched}`"
        );
    }
    for local in ["/Users/", "/private/", "/home/", "C:\\Users"] {
        assert!(
            !page.contains(local),
            "the page leaks a local path: `{local}`"
        );
    }
}

/// The README's counts come from the generated matrix, so a re-recorded link or a new claim cannot leave the front page
/// quoting last month's numbers.
#[test]
fn the_readme_quotes_the_counts_the_matrix_shows() {
    // The README's counts are the matrices': the overview's for the front page, the detail's for the second sentence.
    // ADR 0053: a row has four cells (Source | Claim | Satisfied by | Verified by). A claim is linked when its *Satisfied by*
    // is not the named gap `no design element`; a claim-part pair is one `<part> (holds)` or `<part> (holds (weakened))`.
    let counts = |name: &str| {
        let matrix = read(&root().join("docs/architecture").join(name));
        let mut claims = 0;
        let mut linked = 0;
        let mut holding = 0;
        for row in matrix.lines().filter(|line| line.starts_with("| ")) {
            let cells: Vec<&str> = row.split(" | ").collect();
            if cells.len() < 4 || !cells[1].starts_with("[REQ-") {
                continue;
            }
            claims += 1;
            let satisfied = cells[2];
            if satisfied != "no design element" {
                linked += 1;
            }
            holding += satisfied.matches(" (holds)").count()
                + satisfied.matches(" (holds (weakened))").count();
        }
        (claims, linked, holding)
    };
    let (claims, linked, holding) = counts("overview-matrix.md");
    let (_, _, detail_holding) = counts("matrix.md");
    let unreached = claims - linked;
    let readme = read(&root().join("README.md"));
    let section = readme
        .split("## Requirements on the architecture")
        .nth(1)
        .expect("the README has the section")
        .split("\n## ")
        .next()
        .expect("a section body")
        .replace('\n', " ");
    for expected in [
        format!("{linked} of req's {claims} claims are linked"),
        format!("all {holding} claim–part pairs hold"),
        format!("({detail_holding} on the detailed page"),
        format!("the {unreached} claims no part reaches"),
    ] {
        assert!(
            section.contains(&expected),
            "the README section must say `{expected}` (the matrix shows it):\n{section}"
        );
    }
}

/// ADR 0053 decision 6: a part in the matrix links to the page that draws it, at its node (`#focus=<component id>`), the
/// overview matrix to the overview page and the detail matrix to the detail page, relative to `docs/architecture/`.
#[test]
fn each_matrix_links_its_parts_to_the_page_that_draws_them() {
    for (diagram, _, matrix, page) in VIEWS {
        let file = page.trim_start_matches("docs/");
        let diagram: Value =
            serde_json::from_str(&read(&root().join("docs/architecture").join(diagram)))
                .expect("the diagram is JSON");
        let text = read(&root().join("docs/architecture").join(matrix));
        let linked: Vec<&str> = diagram["components"]
            .as_array()
            .expect("components")
            .iter()
            .filter_map(|component| component["id"].as_str())
            .filter(|id| text.contains(&format!("](../{file}#focus={id})")))
            .collect();
        assert!(
            !linked.is_empty(),
            "{matrix} links no part to ../{file}#focus=<id>"
        );
        // The fragment names a node archify drew: its `data-node-id` is the component's id.
        let html = read(&root().join(page));
        for id in &linked {
            assert!(
                html.contains(&format!("data-node-id=\"{id}\"")),
                "{page} draws no node `{id}`"
            );
        }
        let other = if file == "architecture.html" {
            "architecture-detail.html"
        } else {
            "architecture.html"
        };
        assert!(
            !text.contains(&format!("](../{other}#")),
            "{matrix} links to the other page"
        );
    }
}
