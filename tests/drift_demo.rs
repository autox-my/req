//! What drift looks like on the architecture (ADR 0052 decision 7, step 4; unit O-6).
//!
//! `docs/architecture/drift/edits.json` names three edits to one of req's own claims: a cosmetic re-wording, a loosening and
//! a tightening. `tools/graph/build_architecture.py --drift` applies each to a throw-away COPY of the corpus, runs `req
//! satisfy` and `req trace` there and converts the overview diagram into an overlay, a matrix and the exact `req satisfy`
//! text per edit under `docs/architecture/drift/<edit-id>/`. `docs/architecture-drift.html` is archify's rendering of the
//! tightening overlay. The oracles below are written from the spec and ADR 0040, never from the generated files: an edit
//! that tightens a bound is SUSPECT, one that relaxes it HOLDS (weakened), a re-wording that keeps the meaning HOLDS.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

const DRIFT: &str = "docs/architecture/drift";
const PAGE_URL: &str = "https://autox-my.github.io/req/architecture-drift.html";
/// The claim every edit changes, and the box that cites the file declaring its link (`src/bench.rs`).
const CLAIM: &str = "REQ-0010";
const BOX: &str = "Evidence";

/// (edit id, the link line `req satisfy` must print for it, the status word the box carries).
const EXPECTED: [(&str, &str, &str); 3] = [
    ("cosmetic", "[HOLDS] src/bench.rs -> REQ-0010", "HOLDS · "),
    (
        "loosening",
        "[HOLDS (weakened)] src/bench.rs -> REQ-0010: numeric_loosening",
        "HOLDS (weakened) · ",
    ),
    (
        "tightening",
        "[SUSPECT] src/bench.rs -> REQ-0010: numeric_tightening since it was recorded",
        "SUSPECT · 1 of ",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

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

fn json(path: &Path) -> Value {
    serde_json::from_str(&read(path))
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

/// A scratch copy of the parts of the repository the build reads (and the drift directory), never this checkout.
struct Copy(PathBuf);

impl Copy {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-drift-demo-{}-{}-{name}",
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
        // Every file the overview cites, wherever it lives.
        let overview = json(&root().join("docs/architecture/req-overview.archify.json"));
        for component in overview["components"].as_array().expect("components") {
            for source in component["sources"].as_array().into_iter().flatten() {
                let path = source["path"].as_str().expect("a source path");
                let target = dir.join(path);
                if !target.exists() {
                    fs::create_dir_all(target.parent().expect("a parent")).expect("makes the dir");
                    fs::copy(root().join(path), &target).expect("copies a cited file");
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

/// Every file under `dir`, relative path to bytes.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("reads the dir") {
            let entry = entry.expect("a dir entry");
            let path = entry.path();
            if entry.file_type().expect("a file type").is_dir() {
                walk(base, &path, into);
            } else {
                let relative = path
                    .strip_prefix(base)
                    .expect("under base")
                    .to_string_lossy()
                    .into_owned();
                into.insert(relative, fs::read(&path).expect("reads a file"));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

struct Run {
    code: Option<i32>,
    said: String,
}

/// `python3 <repo>/tools/graph/build_architecture.py <args>` from `repo`, `req` first on `PATH` unless `with_req` is off.
fn build(repo: &Path, args: &[&str], with_req: bool) -> Run {
    let interpreter = python3();
    let mut entries = Vec::new();
    if with_req {
        entries.push(req_bin().parent().expect("a parent dir").to_path_buf());
    }
    let own = std::env::var_os("PATH").unwrap_or_default();
    entries.extend(std::env::split_paths(&own).filter(|dir| {
        // A `req` elsewhere on PATH would defeat the "no req" run.
        with_req || !dir.join("req").exists()
    }));
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

fn drift_file(edit: &str, name: &str) -> PathBuf {
    root().join(DRIFT).join(edit).join(name)
}

fn box_status(overlay: &Value) -> String {
    overlay["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|component| component["label"] == BOX)
        .unwrap_or_else(|| panic!("the overlay has a {BOX} box"))["sublabel"]
        .as_str()
        .expect("a status line")
        .to_string()
}

#[test]
fn the_committed_drift_directory_is_current() {
    let run = build(&root(), &["--check-drift", "docs/architecture/drift"], true);
    assert_eq!(
        run.code,
        Some(0),
        "docs/architecture/drift is stale or unverifiable (regenerate with `python3 tools/graph/build_architecture.py \
         --drift docs/architecture/drift`, then render docs/architecture-drift.html again):\n{}",
        run.said
    );
    for (edit, _, _) in EXPECTED {
        for name in ["overlay.json", "matrix.md", "satisfy.txt"] {
            assert!(
                run.said
                    .contains(&format!("[CURRENT] {DRIFT}/{edit}/{name}")),
                "--check-drift must name {edit}/{name} as current:\n{}",
                run.said
            );
        }
    }
}

/// The three edits, from the data file: the claim every one changes, the statement before (the claim as it stands in
/// `claims/`, so the edit is never to anything but what is committed), and the outcome each is expected to give.
#[test]
fn the_edits_are_data_over_one_real_claim() {
    let edits = json(&root().join(DRIFT).join("edits.json"));
    let list = edits["edits"].as_array().expect("an edits list");
    let ids: Vec<&str> = list.iter().filter_map(|e| e["id"].as_str()).collect();
    assert_eq!(ids, ["cosmetic", "loosening", "tightening"]);
    let claim = read(&root().join(format!("claims/{CLAIM}.yml")));
    let folded: String = claim
        .lines()
        .skip_while(|line| !line.starts_with("statement:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    for edit in list {
        assert_eq!(edit["claim_id"], CLAIM);
        assert_eq!(
            edit["before"].as_str().expect("a before statement"),
            folded,
            "`before` is the claim's statement as committed"
        );
        assert_ne!(edit["before"], edit["after"], "an edit that edits");
    }
}

/// The cosmetic edit is a re-WORDING, not a change of numeral: the words differ, every number is written as before, and
/// `req compare` (an oracle independent of `satisfy`) says the two statements are identical, one meaning fingerprint.
#[test]
fn the_cosmetic_edit_rewords_and_req_compare_says_identical() {
    let edits = json(&root().join(DRIFT).join("edits.json"));
    let cosmetic = edits["edits"]
        .as_array()
        .expect("an edits list")
        .iter()
        .find(|edit| edit["id"] == "cosmetic")
        .expect("a cosmetic edit");
    let before = cosmetic["before"].as_str().expect("before");
    let after = cosmetic["after"].as_str().expect("after");
    let split = |text: &str| -> (Vec<String>, Vec<String>) {
        let tokens = text.split_whitespace().map(|t| t.trim_end_matches('.'));
        let (numbers, words): (Vec<&str>, Vec<&str>) =
            tokens.partition(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()));
        (
            words.into_iter().map(str::to_string).collect(),
            numbers.into_iter().map(str::to_string).collect(),
        )
    };
    let (before_words, before_numbers) = split(before);
    let (after_words, after_numbers) = split(after);
    assert_ne!(before_words, after_words, "the cosmetic edit changes words");
    assert_eq!(
        before_numbers, after_numbers,
        "the cosmetic edit leaves every number written as it was (a re-wording, not a decimal form)"
    );
    // Two claim files, the committed claim with each statement, compared by req.
    let claim = read(&root().join(format!("claims/{CLAIM}.yml")));
    let lines: Vec<&str> = claim.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "statement: >-")
        .expect("a folded statement");
    let end = start
        + 1
        + lines[start + 1..]
            .iter()
            .take_while(|l| l.starts_with("  "))
            .count();
    let with = |statement: &str| {
        let mut out: Vec<String> = lines[..=start].iter().map(|l| l.to_string()).collect();
        out.push(format!("  {statement}"));
        out.extend(lines[end..].iter().map(|l| l.to_string()));
        out.join("\n") + "\n"
    };
    let dir = std::env::temp_dir().join(format!("req-drift-compare-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("makes the dir");
    fs::write(dir.join("a.yml"), with(before)).expect("writes a");
    fs::write(dir.join("b.yml"), with(after)).expect("writes b");
    let output = Command::new(req_bin())
        .arg("compare")
        .arg(dir.join("a.yml"))
        .arg(dir.join("b.yml"))
        .current_dir(root())
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req compare");
    let _ = fs::remove_dir_all(&dir);
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains(&format!("{CLAIM} -> {CLAIM}: identical")),
        "req compare must call the cosmetic edit identical:\n{said}"
    );
}

/// Each edit's committed `satisfy.txt` says what req says of the edited link, and the box that cites the file carries the
/// same word in the overlay.
#[test]
fn each_edit_has_the_outcome_its_direction_gives() {
    for (edit, line, status) in EXPECTED {
        let text = read(&drift_file(edit, "satisfy.txt"));
        assert!(
            text.lines().any(|l| l.starts_with(line)),
            "{edit}: satisfy.txt must say `{line}`:\n{text}"
        );
        let overlay = json(&drift_file(edit, "overlay.json"));
        let shown = box_status(&overlay);
        assert!(
            shown.starts_with(status),
            "{edit}: the {BOX} box must read `{status}…`, it reads `{shown}`"
        );
        // Every other box is untouched by an edit to one claim: no other link moved.
        let moved: Vec<&str> = overlay["components"]
            .as_array()
            .expect("components")
            .iter()
            .filter(|c| c["label"] != BOX)
            .filter_map(|c| c["sublabel"].as_str())
            .filter(|s| s.starts_with("SUSPECT") || s.contains("weakened"))
            .collect();
        assert!(moved.is_empty(), "{edit}: another box moved: {moved:?}");
    }
}

/// The point of the demonstration: a re-wording that keeps the meaning raises no alarm, so the whole verdict is PASS.
#[test]
fn the_cosmetic_edit_leaves_every_link_holding() {
    let text = read(&drift_file("cosmetic", "satisfy.txt"));
    assert!(
        text.contains("verdict: PASS"),
        "the cosmetic edit is a PASS:\n{text}"
    );
    for word in ["[SUSPECT]", "weakened", "[UNRATIFIED]", "[UNKNOWN"] {
        assert!(
            !text.contains(word),
            "the cosmetic edit moved a link ({word}):\n{text}"
        );
    }
    // The loosening passes too (an admitted carry, named), the tightening does not.
    assert!(read(&drift_file("loosening", "satisfy.txt")).contains("verdict: PASS"));
    assert!(read(&drift_file("tightening", "satisfy.txt")).contains("verdict: UNVERIFIABLE"));
}

/// The edit is never made to this repository's corpus: in a copy, regenerating changes nothing but the drift directory,
/// and the claim is byte-for-byte what it was.
#[test]
fn the_corpus_is_untouched_after_the_build() {
    let copy = Copy::new("untouched");
    let before = snapshot(&copy.0);
    let claim = copy.path(&format!("claims/{CLAIM}.yml"));
    let claim_before = fs::read(&claim).expect("reads the claim");
    let run = build(&copy.0, &["--drift", DRIFT], true);
    assert_eq!(run.code, Some(0), "{}", run.said);
    let after = snapshot(&copy.0);
    assert_eq!(
        fs::read(&claim).expect("reads the claim"),
        claim_before,
        "the build edited the claim in the checkout, not in a copy"
    );
    let changed: Vec<&String> = after
        .iter()
        .filter(|(path, bytes)| before.get(*path) != Some(bytes))
        .map(|(path, _)| path)
        .chain(before.keys().filter(|path| !after.contains_key(*path)))
        .collect();
    assert!(
        changed.iter().all(|path| path.starts_with(DRIFT)),
        "the build wrote outside the drift directory: {changed:?}"
    );
    // Nothing is left behind of the throw-away copies, next to the checkout or in it.
    let leftovers: Vec<&String> = after
        .keys()
        .filter(|path| path.starts_with(DRIFT))
        .filter(|path| {
            let rest = path.trim_start_matches(DRIFT).trim_start_matches('/');
            let (edit, name) = rest.split_once('/').unwrap_or(("", rest));
            !(name == "edits.json"
                || name == "req-drift.png"
                || EXPECTED.iter().any(|(id, _, _)| *id == edit)
                    && ["overlay.json", "matrix.md", "satisfy.txt"].contains(&name))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "unexpected files under the drift directory: {leftovers:?}"
    );
}

/// An edit that no longer does what its name says is a stale demonstration, and the check names the file.
#[test]
fn a_tightening_that_no_longer_tightens_makes_the_check_stale() {
    let copy = Copy::new("not-tightening");
    let path = copy.path(&format!("{DRIFT}/edits.json"));
    let text = read(&path);
    let mut edits: Value = serde_json::from_str(&text).expect("edits.json is JSON");
    let loosening = edits["edits"][1]["after"].clone();
    edits["edits"][2]["after"] = loosening;
    fs::write(
        &path,
        serde_json::to_string_pretty(&edits).expect("writes JSON"),
    )
    .expect("edits the edits");
    let run = build(&copy.0, &["--check-drift", DRIFT], true);
    assert_eq!(
        run.code,
        Some(1),
        "a tightening that loosens must be stale, not current:\n{}",
        run.said
    );
    assert!(
        run.said
            .contains(&format!("STALE {DRIFT}/tightening/satisfy.txt")),
        "the stale file must be named:\n{}",
        run.said
    );
}

/// A committed `satisfy.txt` is compared, not trusted: say HOLDS where req says SUSPECT and the check is stale.
#[test]
fn a_hand_edited_satisfy_text_is_stale() {
    let copy = Copy::new("hand-edited");
    let path = copy.path(&format!("{DRIFT}/tightening/satisfy.txt"));
    let text = read(&path).replace("[SUSPECT]", "[HOLDS]");
    fs::write(&path, text).expect("edits the text");
    let run = build(&copy.0, &["--check-drift", DRIFT], true);
    assert_eq!(run.code, Some(1), "{}", run.said);
    assert!(
        run.said
            .contains(&format!("STALE {DRIFT}/tightening/satisfy.txt")),
        "{}",
        run.said
    );
}

#[test]
fn the_check_is_unverifiable_without_req() {
    let run = build(&root(), &["--check-drift", DRIFT], false);
    assert_eq!(
        run.code,
        Some(3),
        "no `req` is UNVERIFIABLE, never current:\n{}",
        run.said
    );
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The page is archify's rendering of the TIGHTENING overlay, and says what it is: a demonstration on a copy, naming the
/// edit, that points back to the real page.
#[test]
fn the_page_is_the_tightening_overlay_and_says_it_is_a_demonstration() {
    let page = read(&root().join("docs/architecture-drift.html"));
    let overlay = json(&drift_file("tightening", "overlay.json"));
    let subtitle = overlay["meta"]["subtitle"].as_str().expect("a subtitle");
    assert!(
        page.contains(&escape(subtitle)),
        "the page's subtitle must be the overlay's:\n{subtitle}"
    );
    for component in overlay["components"].as_array().expect("components") {
        let id = component["id"].as_str().expect("an id");
        let label = component["label"].as_str().expect("a label");
        let status = component["sublabel"].as_str().expect("a status line");
        assert!(
            page.contains(&format!(
                "data-node-id=\"{id}\" data-node-label=\"{}\"",
                escape(label)
            )),
            "the page has no node {id} labelled {label}: render it again from the tightening overlay"
        );
        assert!(
            page.contains(&format!("data-node-sublabel=\"{}\"", escape(status))),
            "the page does not show `{status}` for {id}: render it again"
        );
    }
    assert!(
        box_status(&overlay).starts_with("SUSPECT · 1 of "),
        "the rendered overlay is the tightening"
    );
    let meta = overlay["meta"].to_string();
    assert!(
        meta.contains("demonstration") && meta.contains("copy"),
        "the subtitle says this is a demonstration on a copy: {meta}"
    );
    let cards = overlay["cards"].to_string();
    assert!(
        cards.contains(CLAIM) && cards.contains("at most 0.20"),
        "a card names the edit: {cards}"
    );
    assert!(
        cards.contains("architecture.html"),
        "a card points back to the real page: {cards}"
    );
    for fetched in ["<script src=", "<link rel=\"stylesheet\"", "@import url("] {
        assert!(
            !page.contains(fetched),
            "the page must not fetch `{fetched}`"
        );
    }
    for local in ["/Users/", "/private/", "/home/", "C:\\Users"] {
        assert!(
            !page.contains(local),
            "the page leaks a local path `{local}`"
        );
    }
}

/// The README names the three outcomes in req's words, taken from the generated files, and links the page and the three
/// `satisfy.txt` files; the image is there and small.
#[test]
fn the_readme_shows_the_three_outcomes_in_the_words_req_printed() {
    let readme = read(&root().join("README.md"));
    let section = readme
        .split("## Requirements on the architecture")
        .nth(1)
        .expect("the README has the section")
        .split("\n## ")
        .next()
        .expect("a section body");
    assert!(section.contains("What drift looks like"));
    assert!(
        section.contains(PAGE_URL),
        "the README links the drift page"
    );
    for (edit, line, status) in EXPECTED {
        // req's words for the edited link, exactly as `req satisfy` printed them.
        let printed = read(&drift_file(edit, "satisfy.txt"))
            .lines()
            .find(|l| l.starts_with(line))
            .unwrap_or_else(|| panic!("{edit}: no link line"))
            .to_string();
        let word = printed
            .split(']')
            .next()
            .expect("a bracket")
            .trim_start_matches('[');
        assert!(
            section.contains(&format!("`{word}`")) || section.contains(word),
            "{edit}: the README must carry `{word}`"
        );
        assert!(
            section.contains(&format!("{DRIFT}/{edit}/satisfy.txt")),
            "the README links {edit}'s satisfy.txt"
        );
        let shown = box_status(&json(&drift_file(edit, "overlay.json")));
        assert!(shown.starts_with(status));
        assert!(
            section.contains(&shown),
            "the README must quote the generated status `{shown}`:\n{section}"
        );
    }
    assert!(
        section.contains("numeric_tightening"),
        "the README names the direction req gave the tightening"
    );
    assert!(
        section.contains(&format!("{DRIFT}/req-drift.png")),
        "the README shows the drift page's image"
    );
    let image = root().join(DRIFT).join("req-drift.png");
    let size = fs::metadata(&image)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", image.display()))
        .len();
    assert!(size > 0 && size < 600 * 1024, "the image is {size} bytes");
}

/// The Pages index gains one link, inside the architecture item.
#[test]
fn the_index_links_the_drift_page_in_the_architecture_item() {
    let index = read(&root().join("docs/index.html"));
    assert_eq!(index.matches("href=\"architecture-drift.html\"").count(), 1);
    let item = index
        .lines()
        .find(|line| line.contains("href=\"architecture.html\""))
        .expect("the architecture item");
    assert!(
        item.contains("href=\"architecture-drift.html\""),
        "the link is in the architecture item: {item}"
    );
}
