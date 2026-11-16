//! The README leads with what req does, not how it does it (unit P-1; ADR 0040, ADR 0052).
//!
//! The first screen is a plain sentence, two links to the demonstration, a before-and-after picture pair and a table of three
//! edits with the verdict req gave each. The mechanism (`claim_id`, canonical meaning, content addresses) is the depth that
//! follows. What a test can hold of that without judging prose: the first paragraph is free of the mechanism's words; the two
//! hero links are there, in order; the two pictures exist and are the same size; each verdict in the table is the outcome word
//! in the edit's committed `satisfy.txt` (read here, never typed); the diagram, the quickstart script and the *Status* heading
//! are still there; the sections come in the order the page promises.
//!
//! The oracles are read from the committed `docs/architecture/drift/` files or written in this file, never taken from the
//! README, so a README that drifts from what req printed fails here.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const DEMO_URL: &str = "https://fadasr.github.io/req-demo/";
const DRIFT_URL: &str = "https://autox-my.github.io/req/drift.html";
const DRIFT: &str = "docs/architecture/drift";
const BEFORE: &str = "docs/architecture/drift/req-before.png";
const TIGHTENING: &str = "docs/architecture/drift/req-tightening.png";
/// The three outcome words `req satisfy` prints, as ADR 0040 decision 3 names them.
const OUTCOMES: [&str; 3] = ["HOLDS", "HOLDS (weakened)", "SUSPECT"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

fn readme() -> String {
    read(&root().join("README.md"))
}

/// The paragraphs after the `# ` title line: runs of non-empty lines, in order.
fn paragraphs_after_title(readme: &str) -> Vec<String> {
    let body = readme
        .split_once('\n')
        .map(|(_, rest)| rest)
        .expect("a README has a title line");
    assert!(readme.starts_with("# "), "the first line is the title");
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        out.push(current.join(" "));
    }
    out
}

/// Pixel size from a PNG's IHDR chunk, read from the bytes (no image library, so no dependency to trust).
fn png_size(relative: &str) -> (u32, u32) {
    let bytes = fs::read(root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} cannot be read: {error}"));
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "{relative} is not a PNG file"
    );
    assert_eq!(&bytes[12..16], b"IHDR", "{relative}: IHDR comes first");
    let be =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    (be(16), be(20))
}

/// The image paths in the order they appear in the README (`![alt](path)`).
fn image_paths(readme: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = readme;
    while let Some(start) = rest.find("![") {
        let after = &rest[start..];
        let Some(open) = after.find("](") else { break };
        let tail = &after[open + 2..];
        let Some(close) = tail.find(')') else { break };
        out.push(tail[..close].to_owned());
        rest = &tail[close..];
    }
    out
}

/// The backtick-delimited spans of one line.
fn code_spans(line: &str) -> Vec<&str> {
    line.split('`').skip(1).step_by(2).collect()
}

/// The outcome word `req satisfy` printed for `src/bench.rs -> REQ-0010` in one edit's committed output.
fn printed_outcome(edit: &str) -> String {
    let text = read(&root().join(DRIFT).join(edit).join("satisfy.txt"));
    let line = text
        .lines()
        .find(|l| l.contains("] src/bench.rs -> REQ-0010"))
        .unwrap_or_else(|| panic!("{edit}: satisfy.txt has no line for src/bench.rs -> REQ-0010"));
    line.trim_start_matches('[')
        .split(']')
        .next()
        .expect("a bracket")
        .to_owned()
}

/// The first paragraph under the title is one plain sentence about the outcome: none of the mechanism's words.
#[test]
fn the_first_paragraph_is_the_outcome_not_the_mechanism() {
    let readme = readme();
    let paragraphs = paragraphs_after_title(&readme);
    let first = paragraphs
        .first()
        .expect("a first paragraph")
        .to_lowercase();
    for jargon in ["claim_id", "canonical", "content address", "fingerprint"] {
        assert!(
            !first.contains(jargon),
            "the first paragraph must not lead with the mechanism (`{jargon}`):\n{first}"
        );
    }
    assert!(
        first.contains("meaning") && first.contains("design"),
        "the first paragraph names the problem (a requirement's meaning changes) and the outcome (the design that needs a second look):\n{first}"
    );
}

/// The two hero links: the live demo, then req's own guided page, in that order, on one line, before the first picture.
#[test]
fn the_two_hero_links_come_first_in_order() {
    let readme = readme();
    let demo = readme
        .find(DEMO_URL)
        .expect("the README links the live demo");
    let drift = readme
        .find(DRIFT_URL)
        .expect("the README links the guided drift page");
    let first_image = readme.find("![").expect("the README has a picture");
    assert!(
        demo < drift,
        "the live demo is linked before the guided page"
    );
    assert!(
        drift < first_image,
        "both hero links come before the first picture"
    );
    let line_of = |at: usize| readme[..at].matches('\n').count();
    assert_eq!(
        line_of(demo),
        line_of(drift),
        "the two links are on one line"
    );
    assert!(
        line_of(demo) < 12,
        "the hero links are on the first screen (line {})",
        line_of(demo)
    );
    let line = readme.lines().nth(line_of(demo)).expect("the hero line");
    assert!(
        line.contains("Live demo") && line.contains("What drift looks like"),
        "the links are labelled `Live demo` and `What drift looks like`:\n{line}"
    );
}

/// The picture pair is the matched pair P-2 made: the first two pictures, both there, the same pixel size, each linking the
/// guided page.
#[test]
fn the_before_and_after_pictures_exist_and_are_one_size() {
    let readme = readme();
    let images = image_paths(&readme);
    assert_eq!(
        images.first().map(String::as_str),
        Some(BEFORE),
        "the first picture is the one before any edit: {images:?}"
    );
    assert_eq!(
        images.get(1).map(String::as_str),
        Some(TIGHTENING),
        "the second picture is the tightened one: {images:?}"
    );
    assert_eq!(
        png_size(BEFORE),
        png_size(TIGHTENING),
        "the two pictures are the same size, so the only difference a reader sees is the one edit"
    );
    for picture in [BEFORE, TIGHTENING] {
        let line = readme
            .lines()
            .find(|l| l.contains(picture))
            .expect("a line with the picture");
        assert!(
            line.contains(DRIFT_URL),
            "{picture} links the guided page:\n{line}"
        );
    }
}

/// The three-row table: each edit's verdict is the word `req satisfy` printed for it, read from the committed output.
#[test]
fn each_verdict_in_the_table_is_the_word_req_printed() {
    let readme = readme();
    let edits: Value = serde_json::from_str(&read(&root().join(DRIFT).join("edits.json")))
        .expect("edits.json is JSON");
    let edits = edits["edits"].as_array().expect("edits");
    assert_eq!(edits.len(), 3, "three edits");
    let lines: Vec<&str> = readme.lines().collect();
    let mut block_lengths = Vec::new();
    for edit in edits {
        let id = edit["id"].as_str().expect("an id");
        let kind = edit["kind"].as_str().expect("a kind");
        let marker = format!("**{kind}**");
        let rows: Vec<(usize, &&str)> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with('|') && l.contains(&marker))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "exactly one table row for `{marker}`: {rows:?}"
        );
        let (at, row) = rows[0];
        let word = printed_outcome(id);
        let outcomes: Vec<&str> = code_spans(row)
            .into_iter()
            .filter(|span| OUTCOMES.contains(span))
            .collect();
        assert_eq!(
            outcomes,
            vec![word.as_str()],
            "{id}: the row must say exactly `{word}` (req printed it in {DRIFT}/{id}/satisfy.txt):\n{row}"
        );
        assert!(
            row.contains(&format!("{DRIFT}/{id}/satisfy.txt")),
            "{id}: the row links the full output"
        );
        // The table this row is in: the consecutive `|` lines around it.
        let mut start = at;
        while start > 0 && lines[start - 1].starts_with('|') {
            start -= 1;
        }
        let mut end = at;
        while end + 1 < lines.len() && lines[end + 1].starts_with('|') {
            end += 1;
        }
        block_lengths.push((start, end));
    }
    assert!(
        block_lengths.windows(2).all(|pair| pair[0] == pair[1]),
        "the three edits are rows of one table: {block_lengths:?}"
    );
    let (start, end) = block_lengths[0];
    assert_eq!(end - start + 1, 5, "a header, its rule and three rows");
}

/// The diagram, the quickstart script and the honest limits are all still on the page.
#[test]
fn the_diagram_the_script_and_status_are_there() {
    let readme = readme();
    assert!(
        readme.contains("```mermaid"),
        "the how-it-works diagram is a Mermaid block (it renders on GitHub)"
    );
    assert!(
        readme.contains("tools/demo/try_drift.py"),
        "the README sends a newcomer to the quickstart script"
    );
    assert!(
        readme.lines().any(|l| l == "## Status"),
        "*Status* is still a heading of its own: the honest limits stay"
    );
    assert!(
        readme.contains("docs/adopting.md"),
        "the quickstart is a demonstration: adopting req is linked, not restated"
    );
}

/// The sections come in the order the page promises: the outcome first, the depth after.
#[test]
fn the_sections_come_in_the_promised_order() {
    let readme = readme();
    let headings = [
        "## Three edits, three verdicts",
        "## What SUSPECT means",
        "## Try it in three commands",
        "## How it works",
        "## Where to look next",
        "## What a claim is",
        "## The bet, stated so it can fail",
        "## What it does",
        "## The two differentiators",
        "## Status",
        "## Install",
        "## Development",
        "## Licence",
    ];
    let mut last = 0;
    for heading in headings {
        let at = readme
            .lines()
            .position(|l| l == heading)
            .unwrap_or_else(|| panic!("the README has no `{heading}` heading"));
        assert!(
            at > last,
            "`{heading}` is out of order (line {at}, after line {last})"
        );
        last = at;
    }
}

/// What the page must not say (the spec's list): nothing unsourced about other tools, no colour flips, and the retired
/// picture is neither shown nor kept.
#[test]
fn it_says_nothing_it_cannot_back() {
    let readme = readme().to_lowercase();
    for phrase in [
        "colour flip",
        "color flip",
        "no other tool",
        "only tool",
        "unlike doorstop",
        "better than",
        "req-drift.png",
    ] {
        assert!(
            !readme.contains(phrase),
            "the README must not say `{phrase}`"
        );
    }
    assert!(
        !root().join(DRIFT).join("req-drift.png").exists(),
        "req-drift.png was retired for the matched pair (ADR 0052)"
    );
}

/// The numbers *Status* gives are the repository's: `RULES_V` is the constant in `src/canon.rs`, the dogfood claims are the
/// files in `claims/`, and the corpora are the ones `scripts/check.sh` benches (P-1-fix: a stale number is a claim the code
/// contradicts, AGENTS.md "Docs are claims too").
#[test]
fn status_numbers_are_the_repositorys() {
    let readme = readme();
    let status = readme
        .split("\n## Status\n")
        .nth(1)
        .expect("a Status section")
        .split("\n## ")
        .next()
        .expect("a body")
        .replace('\n', " ");
    let canon = read(&root().join("src/canon.rs"));
    let rules_v = canon
        .lines()
        .find_map(|l| l.strip_prefix("pub const RULES_V: u32 = "))
        .map(|rest| rest.trim_end_matches(';').to_owned())
        .expect("src/canon.rs declares RULES_V");
    assert!(
        status.contains(&format!("`RULES_V {rules_v}`")),
        "Status must name RULES_V {rules_v}:\n{status}"
    );
    let claims = fs::read_dir(root().join("claims"))
        .expect("claims/")
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("REQ-") && name.ends_with(".yml")
        })
        .count();
    assert!(
        status.contains(&format!("{claims} dogfood claims")),
        "Status must say {claims} dogfood claims:\n{status}"
    );
    // The corpora the gate benches: every `bench bench/corpus/<name>.yml` line of check.sh, plus the seed parity run.
    let check = read(&root().join("scripts/check.sh"));
    let gated = check
        .lines()
        .filter(|l| {
            l.trim_start()
                .starts_with("run cargo run -q -- bench bench/corpus/")
        })
        .count()
        + 1;
    let words = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    assert!(
        status.contains(&format!("{} corpora", words[gated])),
        "Status must say {} corpora (the gate benches {gated}):\n{status}",
        words[gated]
    );
}

/// The before picture's alt text must not say every part holds: parts with no linked requirement read "no claim".
#[test]
fn the_before_pictures_alt_text_is_true_of_the_picture() {
    let readme = readme();
    let line = readme
        .lines()
        .find(|l| l.contains(BEFORE))
        .expect("the line with the before picture");
    let alt = line.split("](").next().expect("alt text").to_lowercase();
    assert!(
        !alt.contains("every one holding"),
        "five of the twelve parts read `no claim`; the alt text must not say every one holds:\n{alt}"
    );
    assert!(
        alt.contains("linked requirement"),
        "the alt text says what holds: every part with a linked requirement:\n{alt}"
    );
}

/// The guided page reserves the pictures' own aspect ratio; a different one stretches them.
#[test]
fn the_guided_pages_aspect_ratio_is_the_pictures() {
    let (width, height) = png_size(BEFORE);
    let template = read(&root().join("tools/demo/drift.html"));
    assert!(
        template.contains(&format!("aspect-ratio: {width} / {height}")),
        "tools/demo/drift.html must reserve aspect-ratio: {width} / {height}"
    );
}
