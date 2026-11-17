//! The README leads with what req does, not how it does it (unit P-1; ADR 0040, ADR 0052).
//!
//! The first screen is a row of badges that read real sources, a plain sentence, two links to the demonstration, an animated
//! replay of the three edits (unit P-3; it replaced the before-and-after picture pair of P-1) and a table of the three edits
//! with the verdict req gave each. The mechanism (`claim_id`, canonical meaning, content addresses) is the depth that
//! follows. What a test can hold of that without judging prose: the first paragraph is free of the mechanism's words; the two
//! hero links are there, in order; the replay is the first picture, linked to the guided page, and its alt text and caption say
//! what the files say; the badges point at real targets; each verdict in the table is the outcome word in the edit's committed
//! `satisfy.txt` (read here, never typed); the NOTE and the Contents are there and resolve; the folds are balanced and hide
//! neither the honest limits nor the diagram; the diagram, the quickstart script and the *Status* heading are still there;
//! the sections come in the order the page promises.
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
const REPLAY: &str = "docs/architecture/drift/req-replay.gif";
const REPO: &str = "autox-my/req";
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
    // P-3: a row of badges sits directly under the title; the first *paragraph* is the sentence after it.
    if out.first().is_some_and(|p| is_badge_row(p)) {
        out.remove(0);
    }
    out
}

/// A paragraph made only of linked images (`[![alt](shield)](target)`), whose shields are web addresses.
fn is_badge_row(paragraph: &str) -> bool {
    let rest = paragraph.replace(' ', "");
    rest.starts_with("[![")
        && rest
            .split("[![")
            .skip(1)
            .all(|badge| badge.contains("](https://") && badge.ends_with(')'))
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
    // The badges are images too; the hero links come before the first *picture* (the replay).
    let first_image = readme
        .find(&format!("]({REPLAY})"))
        .expect("the README has the replay");
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

/// The number of printed outcomes that are `HOLDS` (either kind) and that are `SUSPECT`, read from the committed output.
fn printed_counts() -> (usize, usize) {
    let words: Vec<String> = ["cosmetic", "loosening", "tightening"]
        .iter()
        .map(|edit| printed_outcome(edit))
        .collect();
    (
        words.iter().filter(|w| w.starts_with("HOLDS")).count(),
        words.iter().filter(|w| w.as_str() == "SUSPECT").count(),
    )
}

/// How many times, as the words the alt text uses (P-3: counts are read from the files, the English is written here).
fn times(n: usize) -> &'static str {
    match n {
        1 => "once",
        2 => "twice",
        3 => "three times",
        _ => panic!("no word for {n} in the alt text"),
    }
}

/// P-3 moved this from the matched pair P-2 made (`req-before.png` and `req-tightening.png`, one pixel size, each linking the
/// guided page): the first picture is now the replay, linked to the guided page, and the pair is no longer on the README.
/// The pair stays in the repository, because `docs/drift.html` shows it, and stays one size, so the page's aspect ratio holds.
#[test]
fn the_replay_is_the_first_picture_and_the_old_pair_is_off_the_readme() {
    let readme = readme();
    let images: Vec<String> = image_paths(&readme)
        .into_iter()
        .filter(|path| !path.starts_with("https://"))
        .collect();
    assert_eq!(
        images.first().map(String::as_str),
        Some(REPLAY),
        "the first picture is the replay: {images:?}"
    );
    let line = readme
        .lines()
        .find(|l| l.contains(REPLAY))
        .expect("a line with the replay");
    assert!(
        line.trim_end().ends_with(&format!("]({DRIFT_URL})")),
        "the replay links the guided page:\n{line}"
    );
    for retired in [BEFORE, TIGHTENING, "| Before | After one edit |"] {
        assert!(
            !readme.contains(retired),
            "the README no longer carries the before/after table (`{retired}`)"
        );
    }
    for picture in [BEFORE, TIGHTENING] {
        assert!(
            root().join(picture).is_file(),
            "{picture} stays: docs/drift.html shows it"
        );
    }
    assert_eq!(
        png_size(BEFORE),
        png_size(TIGHTENING),
        "the two pictures are the same size, so the only difference the guided page shows is the one edit"
    );
}

/// The replay's alt text says what happens, in words read from the files: three edits (`edits.json`), the verdict word of
/// each (`satisfy.txt`), the baseline alarming once per edit, req holding twice and marking the Evidence box SUSPECT once.
#[test]
fn the_replays_alt_text_says_what_the_files_say() {
    let readme = readme();
    let line = readme
        .lines()
        .find(|l| l.contains(REPLAY))
        .expect("the line with the replay");
    let alt = line
        .strip_prefix("[![")
        .and_then(|rest| rest.split("](").next())
        .expect("the replay is a linked image with alt text");
    let edits: Value = serde_json::from_str(&read(&root().join(DRIFT).join("edits.json")))
        .expect("edits.json is JSON");
    let count = edits["edits"].as_array().expect("edits").len();
    let (holds, suspect) = printed_counts();
    assert_eq!(
        (count, holds, suspect),
        (3, 2, 1),
        "the story the alt text tells is three edits, two held, one suspect"
    );
    for needed in [
        "three edits".to_owned(),
        format!("alarm {}", times(count)),
        format!("holds {}", times(holds)),
        format!("{} {}", "SUSPECT", times(suspect)),
        "Evidence box".to_owned(),
        "red".to_owned(),
    ] {
        assert!(
            alt.contains(&needed),
            "the alt text must say `{needed}`:\n{alt}"
        );
    }
    for edit in ["cosmetic", "loosening", "tightening"] {
        let word = printed_outcome(edit);
        assert!(
            alt.contains(&format!("({word})"))
                || alt.contains(&format!("{word},"))
                || alt.contains(&format!("{word} ")),
            "the alt text must carry the word req printed for {edit} (`{word}`):\n{alt}"
        );
    }
    assert!(
        alt.contains("(HOLDS (weakened))"),
        "the loosening's word is `HOLDS (weakened)`, not `HOLDS`:\n{alt}"
    );
}

/// One caption line under the replay: a schematic replay, the baseline is generic and not a named tool, req's words are its
/// committed `req satisfy` output, and the data files are named.
#[test]
fn the_replays_caption_says_what_it_is_and_names_its_data() {
    let readme = readme();
    let lines: Vec<&str> = readme.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains(REPLAY))
        .expect("the replay line");
    assert!(
        lines[at + 1].trim().is_empty(),
        "a blank line, then the caption"
    );
    let caption = lines[at + 2];
    assert!(
        !caption.trim().is_empty() && lines[at + 3].trim().is_empty(),
        "the caption is one line:\n{caption}"
    );
    for needed in [
        "schematic",
        "generic baseline",
        "not a named tool",
        "`req satisfy`",
        "edits.json",
        "satisfy.txt",
        "committed",
    ] {
        assert!(
            caption.contains(needed),
            "the caption must say `{needed}`:\n{caption}"
        );
    }
}

/// The badges are the row directly under the title, and each reads a real source: the workflow file the CI badge names exists
/// and is the one that has the `gate` job, the licence is the Apache-2.0 the repository carries, the release badge types no
/// version (it resolves from GitHub), and the Pages badge is the site `PAGES_URL` names.
#[test]
fn the_badges_read_real_sources() {
    let readme = readme();
    let row = readme.lines().nth(2).expect("a line under the title");
    assert!(
        readme.lines().nth(1) == Some("") && is_badge_row(row),
        "the badges are the row directly under the title:\n{row}"
    );
    let badges: Vec<(&str, &str)> = row
        .split("[![")
        .skip(1)
        .map(|badge| {
            let (alt, rest) = badge.split_once("](").expect("an alt");
            let (shield, rest) = rest.split_once(")](").expect("a shield and a target");
            let target = rest.trim_end().trim_end_matches(')').trim_end();
            let _ = shield;
            (alt, target)
        })
        .collect();
    let find = |needle: &str| {
        row.split("[![")
            .skip(1)
            .find(|b| b.contains(needle))
            .unwrap_or_else(|| panic!("no badge for `{needle}`:\n{row}"))
    };
    // CI: the workflow file exists and has the `gate` job.
    let ci = find("actions/workflow");
    assert!(
        ci.contains(&format!("{REPO}/ci.yml")) && ci.contains("branch=main") && ci.contains("gate"),
        "the CI badge reads `ci.yml` on `main` and is labelled `gate`:\n{ci}"
    );
    let workflow = read(&root().join(".github/workflows/ci.yml"));
    assert!(
        workflow.lines().any(|l| l == "  gate:"),
        "ci.yml has the `gate` job the badge names"
    );
    assert!(
        ci.contains(&format!(
            "https://github.com/{REPO}/actions/workflows/ci.yml"
        )),
        "the CI badge links the workflow:\n{ci}"
    );
    // The latest release: resolved by the shield, so no version is typed.
    let release = find("/github/v/release/");
    assert!(
        release.contains(&format!("/github/v/release/{REPO}"))
            && release.contains(&format!("https://github.com/{REPO}/releases/latest")),
        "the release badge reads the latest release and links it:\n{release}"
    );
    assert!(
        !row.contains("v0.") && !row.contains("0.5"),
        "the badge row types no version:\n{row}"
    );
    // The licence is the file in the repository.
    let licence = find("/github/license/");
    assert!(
        licence.contains(&format!("/github/license/{REPO}")),
        "the licence badge reads the repository's licence:\n{licence}"
    );
    let text = read(&root().join("LICENSE"));
    assert!(
        text.contains("Apache License") && text.contains("Version 2.0"),
        "LICENSE is Apache-2.0, as the badge says"
    );
    // The Pages site is the one PAGES_URL names.
    let pages_url = read(&root().join("tools/graph/build_architecture.py"))
        .lines()
        .find_map(|l| l.strip_prefix("PAGES_URL = \""))
        .map(|rest| rest.trim_end_matches('"').to_owned())
        .expect("build_architecture.py names PAGES_URL");
    let pages = find("/website");
    let encoded = pages_url.replace(':', "%3A").replace('/', "%2F");
    assert!(
        pages.contains(&format!("url={encoded}")) && pages.contains(&format!("]({pages_url}/)")),
        "the Pages badge reads and links {pages_url}/:\n{pages}"
    );
    assert_eq!(badges.len(), 4, "CI, release, licence, Pages: {badges:?}");
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

/// GitHub's anchor for a heading: lower-cased, punctuation dropped, spaces to hyphens.
fn slug(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .replace(' ', "-")
}

/// The NOTE callout follows the three-row table and its sentence, and is two plain sentences: the re-wording and the
/// loosening raised nothing, because req judges the direction of a change, not the fact of it.
#[test]
fn the_note_follows_the_table_and_is_two_plain_sentences() {
    let readme = readme();
    let lines: Vec<&str> = readme.lines().collect();
    let table_end = lines
        .iter()
        .rposition(|l| l.starts_with('|') && l.contains("satisfy.txt"))
        .expect("the three-row table");
    let at = lines
        .iter()
        .position(|l| *l == "> [!NOTE]")
        .expect("a `> [!NOTE]` callout");
    assert!(at > table_end, "the NOTE comes after the table");
    assert_eq!(
        lines.iter().filter(|l| **l == "> [!NOTE]").count(),
        1,
        "one NOTE"
    );
    let body: Vec<&str> = lines[at + 1..]
        .iter()
        .take_while(|l| l.starts_with('>'))
        .map(|l| l.trim_start_matches('>').trim())
        .collect();
    assert!(!body.is_empty(), "the NOTE has a body");
    let text = body.join(" ");
    let sentences = text
        .match_indices(['.', '?', '!'])
        .filter(|(i, _)| text[i + 1..].is_empty() || text[i + 1..].starts_with(' '))
        .count();
    assert_eq!(sentences, 2, "two sentences:\n{text}");
    for needed in ["re-wording", "loosening", "raised nothing", "direction"] {
        assert!(
            text.contains(needed),
            "the NOTE must say `{needed}`:\n{text}"
        );
    }
    assert!(
        text.contains("not the fact"),
        "the NOTE gives the reason, the direction and not the fact of a change:\n{text}"
    );
}

/// A short Contents list follows the hero block (before the first section) and links every heading that follows; each link
/// resolves to a heading of the page.
#[test]
fn the_contents_list_links_every_section_and_each_link_resolves() {
    let readme = readme();
    let lines: Vec<&str> = readme.lines().collect();
    let contents = lines
        .iter()
        .position(|l| *l == "## Contents")
        .expect("a `## Contents` heading");
    let first_section = lines
        .iter()
        .position(|l| *l == "## Three edits, three verdicts")
        .expect("the first section");
    let replay = lines
        .iter()
        .position(|l| l.contains(REPLAY))
        .expect("the replay");
    assert!(
        replay < contents && contents < first_section,
        "the Contents follow the hero block and come before the first section"
    );
    let block: String = lines[contents + 1..first_section].join(" ");
    let mut linked: Vec<String> = Vec::new();
    let mut rest = block.as_str();
    while let Some(at) = rest.find("](#") {
        let tail = &rest[at + 3..];
        let close = tail.find(')').expect("a closed link");
        linked.push(tail[..close].to_owned());
        rest = &tail[close..];
    }
    let headings: Vec<&str> = lines
        .iter()
        .skip(first_section)
        .filter(|l| l.starts_with("## "))
        .map(|l| &l[3..])
        .collect();
    let slugs: Vec<String> = headings.iter().map(|h| slug(h)).collect();
    assert_eq!(
        linked, slugs,
        "the Contents link every heading that follows, in order, and each anchor is that heading's"
    );
    assert!(
        block.len() < 900,
        "the Contents are short ({} bytes):\n{block}",
        block.len()
    );
}

/// The folds: each `<details>` is closed, has a one-line `<summary>` and a blank line after it (or the Markdown inside does
/// not render), and none hides the honest limits (*Status*), the diagram or the three-row table.
#[test]
fn the_folds_are_balanced_and_hide_neither_the_limits_nor_the_diagram() {
    let readme = readme();
    let lines: Vec<&str> = readme.lines().collect();
    let mut depth = 0usize;
    let mut folded: Vec<usize> = Vec::new();
    let mut folds = 0;
    for (n, line) in lines.iter().enumerate() {
        if *line == "<details>" {
            folds += 1;
            depth += 1;
            let summary = lines.get(n + 1).copied().unwrap_or("");
            assert!(
                summary.starts_with("<summary>")
                    && summary.ends_with("</summary>")
                    && summary.len() > "<summary></summary>".len(),
                "line {}: a one-line summary follows `<details>`:\n{summary}",
                n + 1
            );
            assert!(
                lines.get(n + 2).is_some_and(|l| l.is_empty()),
                "line {}: a blank line follows the summary",
                n + 1
            );
        } else if *line == "</details>" {
            assert!(depth > 0, "line {}: `</details>` closes nothing", n + 1);
            depth -= 1;
        } else if depth > 0 {
            folded.push(n);
        }
    }
    assert_eq!(depth, 0, "every `<details>` is closed");
    assert!(folds >= 2, "the depth is behind folds ({folds})");
    for must_show in ["## Status", "```mermaid", "| **re-word**", "> [!NOTE]"] {
        let hit = lines
            .iter()
            .position(|l| l.starts_with(must_show))
            .unwrap_or_else(|| panic!("`{must_show}` is on the page"));
        assert!(
            !folded.contains(&hit),
            "`{must_show}` is outside every fold (line {})",
            hit + 1
        );
    }
    for heading in lines.iter().filter(|l| l.starts_with("## ")) {
        let at = lines.iter().position(|l| l == heading).expect("a heading");
        assert!(
            !folded.contains(&at),
            "`{heading}` is outside every fold: Contents must reach it"
        );
    }
}
