//! The README's drift replay (unit P-3; ADR 0040, ADR 0052).
//!
//! `docs/architecture/drift/req-replay.gif` is an animated, side-by-side replay of the three committed edits: a generic
//! baseline that flags every edit beside req, which marks a part only as `req satisfy` judged it. It is drawn as HTML and
//! screenshotted by `tools/demo/drift_replay.py` with the owner's local Google Chrome and assembled with `ffmpeg`; neither is a
//! repository dependency and neither runs here. What the gate can hold of it, without a browser:
//!
//! - **The record binds the GIF to what it was made from** (as P-2's `.png.json` records bind a picture): the SHA-256 of
//!   each input file (`edits.json`, the three `satisfy.txt`, the overlays that give its layout and its status words), of the
//!   script and of the GIF itself, the renderer versions, the frame size and the frame count. A changed input, a changed
//!   script or a changed GIF fails here and names the command that regenerates it.
//! - **The GIF is what the spec says**: 1600 to 1800 px wide, under 3 MB, paced to be followed (P-3-fix: an intro, three
//!   beats of 3.5 to 4 s for each edit, a summary; 40 to 55 s in all), looping, its last frame the longest hold.
//! - **The how-it-works picture** (`req-how-it-works.png`, P-3-fix) is bound to the script that drew it and to its own bytes.
//! - **Every word of req's output is read from the committed files**: the script's HTML frames (`--html-only`, no browser) carry
//!   the exact `req satisfy` line of each edit's `satisfy.txt` from that edit's frame on, and none before it.
//! - **The script refuses, and says what is missing**, when Chrome or ffmpeg is absent; it never falls back to another renderer.
//!
//! The oracles are read from the committed `docs/architecture/drift/` files or written in this file, never from the script.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use sha2::{Digest, Sha256};

const DRIFT: &str = "docs/architecture/drift";
const GIF: &str = "docs/architecture/drift/req-replay.gif";
const RECORD: &str = "docs/architecture/drift/req-replay.gif.json";
const SCRIPT: &str = "tools/demo/drift_replay.py";
const REGENERATE: &str = "python3 tools/demo/drift_replay.py";
const EDITS: [&str; 3] = ["cosmetic", "loosening", "tightening"];
const LINK: &str = "] src/bench.rs -> REQ-0010";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

fn sha256_hex(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for {SCRIPT} - install it first");
}

/// A scratch directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-drift-replay-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the scratch dir");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Run {
    code: Option<i32>,
    said: String,
}

/// Run the script from the repository root, with the renderers the test names (or none, so the script looks for its own).
fn script(args: &[&str], env: &[(&str, &Path)]) -> Run {
    let mut command = Command::new(python3());
    command
        .arg(root().join(SCRIPT))
        .args(args)
        .current_dir(root());
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().expect("runs python3");
    Run {
        code: out.status.code(),
        said: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// What a GIF's own bytes say: its canvas, its frames, each frame's delay (centiseconds) and whether it loops forever.
struct GifInfo {
    width: u16,
    height: u16,
    delays: Vec<u16>,
    loops_forever: bool,
}

fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// Walk the GIF's blocks (no image library, so no dependency to trust).
fn gif_info(bytes: &[u8]) -> GifInfo {
    assert!(
        bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"),
        "{GIF} is not a GIF file"
    );
    let width = le16(bytes, 6);
    let height = le16(bytes, 8);
    let packed = bytes[10];
    let mut at = 13;
    if packed & 0x80 != 0 {
        at += 3 * (1usize << ((packed & 7) + 1));
    }
    let skip_sub_blocks = |mut pos: usize| {
        loop {
            let n = usize::from(bytes[pos]);
            pos += 1;
            if n == 0 {
                return pos;
            }
            pos += n;
        }
    };
    let mut delays = Vec::new();
    let mut pending: u16 = 0;
    let mut loops_forever = false;
    loop {
        match bytes[at] {
            0x21 => {
                let label = bytes[at + 1];
                if label == 0xF9 {
                    pending = le16(bytes, at + 4);
                }
                if label == 0xFF && &bytes[at + 3..at + 14] == b"NETSCAPE2.0" {
                    loops_forever =
                        bytes[at + 14] == 3 && bytes[at + 15] == 1 && le16(bytes, at + 16) == 0;
                }
                at = skip_sub_blocks(at + 2);
            }
            0x2C => {
                let local = bytes[at + 9];
                let mut pos = at + 10;
                if local & 0x80 != 0 {
                    pos += 3 * (1usize << ((local & 7) + 1));
                }
                pos += 1; // the LZW minimum code size
                at = skip_sub_blocks(pos);
                delays.push(pending);
                pending = 0;
            }
            0x3B => break,
            other => panic!("{GIF}: unexpected block 0x{other:02x} at byte {at}"),
        }
    }
    GifInfo {
        width,
        height,
        delays,
        loops_forever,
    }
}

fn record() -> Value {
    serde_json::from_str(&read(&root().join(RECORD)))
        .unwrap_or_else(|e| panic!("{RECORD} is not JSON: {e}"))
}

/// The record names the files the replay is made from; each hash holds, or the GIF is stale and the command that makes it
/// again is named.
#[test]
fn the_record_binds_the_gif_to_the_files_it_was_made_from() {
    let record = record();
    assert_eq!(record["schema"], "req.drift-replay/1");
    let inputs = record["inputs"].as_object().expect("inputs: path to hash");
    let mut required = vec![format!("{DRIFT}/edits.json")];
    required.extend(EDITS.iter().map(|e| format!("{DRIFT}/{e}/satisfy.txt")));
    // The parts and their status words come from the overlays, so a record that dropped one would let them go stale.
    required.extend(EDITS.iter().map(|e| format!("{DRIFT}/{e}/overlay.json")));
    required.push("docs/architecture/overview-overlay.json".to_string());
    for path in &required {
        assert!(
            inputs.contains_key(path),
            "the record must bind `{path}`: the words in the replay come from it"
        );
    }
    for (path, hash) in inputs {
        assert_eq!(
            hash.as_str().expect("a hash"),
            sha256_hex(&root().join(path)),
            "{path} changed since the replay was made: the GIF is stale. Regenerate it by hand: `{REGENERATE}`"
        );
    }
    assert_eq!(record["script"], SCRIPT, "the record names its script");
    assert_eq!(
        record["script_sha256"].as_str().expect("a hash"),
        sha256_hex(&root().join(SCRIPT)),
        "{SCRIPT} changed since the replay was made: the GIF is stale. Regenerate it by hand: `{REGENERATE}`"
    );
    assert_eq!(record["gif"], GIF, "the record names its GIF");
    assert_eq!(
        record["gif_sha256"].as_str().expect("a hash"),
        sha256_hex(&root().join(GIF)),
        "{GIF} is not the file the record describes. Regenerate both by hand: `{REGENERATE}`"
    );
    for renderer in ["chrome", "ffmpeg"] {
        let version = record[renderer].as_str().unwrap_or("");
        assert!(
            version.to_lowercase().contains(renderer)
                && version.chars().any(|c| c.is_ascii_digit()),
            "the record carries the {renderer} version it was rendered with: `{version}`"
        );
    }
}

/// The record's frame size and frame count are the GIF's own; the GIF is the spec's: wide enough to read at full width on
/// GitHub, small, looping, paced so a first-time viewer can follow it (P-3-fix, owner review of PR #100: an intro of about 5 s,
/// then three beats of 3.5 to 4 s for each edit (the edit, the left reaction, the right reaction), then a summary held about
/// 6 s; the total is 40 to 55 s), and its last frame is the longest hold.
#[test]
fn the_gif_is_the_size_and_pace_the_spec_asks_for() {
    let bytes = fs::read(root().join(GIF)).unwrap_or_else(|e| panic!("{GIF}: {e}"));
    let info = gif_info(&bytes);
    let record = record();
    assert_eq!(
        record["frame_size"],
        serde_json::json!([info.width, info.height]),
        "the record's frame size is the GIF's canvas"
    );
    assert_eq!(
        record["frame_count"].as_u64(),
        u64::try_from(info.delays.len()).ok(),
        "the record's frame count is the GIF's"
    );
    assert!(
        (1600..=1800).contains(&info.width),
        "{} px wide: legible on GitHub at full width needs 1600 to 1800",
        info.width
    );
    assert!(
        bytes.len() < 3 * 1024 * 1024,
        "the GIF is {} bytes, over the 3 MB the README may carry",
        bytes.len()
    );
    assert!(info.loops_forever, "the GIF loops");
    assert_eq!(
        info.delays.len(),
        11,
        "an intro, three beats for each of three edits, a summary: {:?}",
        info.delays
    );
    // GIF delays are whole centiseconds taken from timestamp gaps, so a hold may be a few centiseconds off.
    let within = |delay: u16, low: u16, high: u16| (low..=high).contains(&delay);
    assert!(
        within(info.delays[0], 480, 520),
        "the intro is held about 5 s: {:?}",
        info.delays
    );
    for edit in 0..3 {
        for beat in 0..3 {
            let delay = info.delays[1 + 3 * edit + beat];
            assert!(
                within(delay, 345, 405),
                "edit {} beat {} is held 3.5 to 4 s, not {delay} cs: {:?}",
                edit + 1,
                beat + 1,
                info.delays
            );
        }
    }
    let total: u32 = info.delays.iter().map(|d| u32::from(*d)).sum();
    assert!(
        (4000..5500).contains(&total),
        "the replay runs {total} cs: 40 to 55 s so each beat can be read"
    );
    let last = *info.delays.last().expect("frames");
    assert!(
        within(last, 580, 620),
        "the summary is held about 6 s: {last} cs"
    );
    assert!(
        info.delays.iter().all(|d| *d <= last),
        "the final frame is the longest hold: {:?}",
        info.delays
    );
}

/// `src/bench.rs -> REQ-0010` as `req satisfy` printed it for one edit.
fn printed_line(edit: &str) -> String {
    read(&root().join(DRIFT).join(edit).join("satisfy.txt"))
        .lines()
        .find(|l| l.contains(LINK))
        .unwrap_or_else(|| panic!("{edit}: satisfy.txt has no line for the link"))
        .to_owned()
}

/// The text of an HTML page: its style and tags removed, entities decoded, whitespace collapsed.
fn text_of(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        if tail.starts_with("<style") {
            rest = tail.split_once("</style>").map_or("", |(_, after)| after);
            continue;
        }
        rest = tail.split_once('>').map_or("", |(_, after)| after);
    }
    out.push_str(rest);
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The values of every `name="..."` attribute in a page, in order.
fn attributes(html: &str, name: &str) -> Vec<String> {
    let needle = format!(" {name}=\"");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(&needle) {
        let tail = &rest[at + needle.len()..];
        let close = tail.find('"').expect("a closed attribute");
        out.push(
            tail[..close]
                .replace("&amp;", "&")
                .replace("&#x27;", "'")
                .replace("&lt;", "<")
                .replace("&gt;", ">"),
        );
        rest = &tail[close..];
    }
    out
}

fn html_frames(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the frames directory")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("frame-") && n.ends_with(".html"))
        .collect();
    // Numeric order: `frame-10` follows `frame-9`.
    names.sort_by_key(|n| {
        n.trim_start_matches("frame-")
            .trim_end_matches(".html")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    names.iter().map(|n| read(&dir.join(n))).collect()
}

/// Eleven states (P-3-fix): an intro; then, for each edit, three beats (the edit alone, the left reaction with the right panel
/// dimmed, the right reaction with the left panel dimmed); then a summary. The words of req's output are the committed
/// ones, and appear only in the right-reaction beat and the summary; the edit's words are `edits.json`'s, shown full width
/// from the first beat; the counters and the chip are what the files say. The banner above the panels says in plain words
/// what each beat is.
#[test]
fn every_word_of_reqs_output_is_read_from_the_committed_files() {
    let scratch = Scratch::new("html");
    let dir = scratch.0.join("frames");
    let run = script(&["--html-only", dir.to_str().expect("a path")], &[]);
    assert_eq!(run.code, Some(0), "{}", run.said);
    let frames = html_frames(&dir);
    assert_eq!(
        frames.len(),
        11,
        "an intro, three beats for each edit, a summary"
    );
    let edits: Value = serde_json::from_str(&read(&root().join(DRIFT).join("edits.json")))
        .expect("edits.json is JSON");
    let edits = edits["edits"].as_array().expect("edits");
    let texts: Vec<String> = frames.iter().map(|f| text_of(f)).collect();

    // The banner names the beat, and the intro says in plain words what the reader is about to see.
    let beats = [
        "intro", "edit", "left", "right", "edit", "left", "right", "edit", "left", "right",
        "summary",
    ];
    for (at, beat) in beats.iter().enumerate() {
        assert_eq!(
            attributes(&frames[at], "data-beat"),
            vec![(*beat).to_owned()],
            "frame {at} is the `{beat}` beat"
        );
    }
    for needed in [
        "The same three edits to one requirement, shown to two reviewers.",
        "Left: a check that flags every text change.",
        "Right: req, which judges the direction of the change.",
    ] {
        assert!(
            texts[0].contains(needed),
            "the intro says `{needed}`:\n{}",
            texts[0]
        );
    }

    // The intro: no edit, no output from req, nothing alarmed, both panels in the starting, all-holding state.
    for edit in EDITS {
        assert!(
            !texts[0].contains(&printed_line(edit)),
            "{edit}: req has said nothing in the intro"
        );
    }
    assert!(texts[0].contains("Edit 0 of 3"), "{}", texts[0]);

    for (k, edit) in edits.iter().enumerate() {
        let id = edit["id"].as_str().expect("an id");
        let before = edit["before"].as_str().expect("a before");
        let after = edit["after"].as_str().expect("an after");
        let line = printed_line(id);
        let (edit_beat, left_beat, right_beat) = (1 + 3 * k, 2 + 3 * k, 3 + 3 * k);
        for at in [edit_beat, left_beat, right_beat] {
            assert!(
                texts[at].contains(&format!("Edit {} of 3", k + 1)),
                "frame {at} counts the edit: {}",
                texts[at]
            );
            assert!(
                texts[at].contains(before) && texts[at].contains(after),
                "frame {at}: the edit, before and after, is shown as edits.json has it"
            );
        }
        assert!(
            texts[edit_beat].contains(&format!("Edit {} of 3:", k + 1)),
            "the banner names the edit: {}",
            texts[edit_beat]
        );
        for at in [edit_beat, left_beat] {
            assert!(
                !texts[at].contains(&line),
                "{id}: req has not said its line yet in frame {at}"
            );
        }
        assert!(
            texts[right_beat].contains(&line),
            "{id}: the right-reaction beat carries req's exact line from satisfy.txt:\n{line}\n{}",
            texts[right_beat]
        );
        let outcome = line
            .trim_start_matches('[')
            .split(']')
            .next()
            .expect("a bracket");
        assert_eq!(
            attributes(&frames[right_beat], "data-verdict"),
            vec![outcome.to_owned()],
            "{id}: the status chip carries the word req printed"
        );
        // The chip has not moved before req has spoken.
        let start_word = attributes(&frames[0], "data-verdict");
        for at in [edit_beat, left_beat] {
            assert_eq!(
                attributes(&frames[at], "data-verdict"),
                start_word,
                "frame {at}: the chip still shows the starting state"
            );
        }
    }
    assert!(
        texts[1].contains("Edit 1 of 3: the wording changes, the meaning does not"),
        "the re-word's caption says it in plain words: {}",
        texts[1]
    );

    // The summary keeps req's last line, and recaps each edit with the word req printed for it.
    assert!(texts[10].contains(&printed_line("tightening")));
    for edit in EDITS {
        let line = printed_line(edit);
        let word = line
            .trim_start_matches('[')
            .split(']')
            .next()
            .expect("a bracket");
        assert!(
            texts[10].contains(&format!("req: {word}")),
            "the summary recaps {edit} with `{word}`:\n{}",
            texts[10]
        );
    }
    assert!(
        texts[10].contains("three times") && texts[10].contains("once"),
        "the summary counts the alarms: {}",
        texts[10]
    );

    // Counters: the baseline's alarm rises once per edit, in its own beat; req's is raised once, by the tightening only.
    let counters = |at: usize| -> Vec<String> { attributes(&frames[at], "data-alarms") };
    let expected: [(&str, &str); 11] = [
        ("0", "0"),
        ("0", "0"),
        ("1", "0"),
        ("1", "0"),
        ("1", "0"),
        ("2", "0"),
        ("2", "0"),
        ("2", "0"),
        ("3", "0"),
        ("3", "1"),
        ("3", "1"),
    ];
    for (at, (left, right)) in expected.iter().enumerate() {
        assert_eq!(
            counters(at),
            vec![(*left).to_owned(), (*right).to_owned()],
            "frame {at}: alarms, baseline then req"
        );
    }

    // The panel not being talked about is dimmed: the right one while the left reacts, the left one while the right reacts.
    let dim = |at: usize| -> Vec<String> { attributes(&frames[at], "data-dim") };
    for (at, beat) in beats.iter().enumerate() {
        let want: [&str; 2] = match *beat {
            "left" => ["false", "true"],
            "right" => ["true", "false"],
            _ => ["false", "false"],
        };
        assert_eq!(
            dim(at),
            want.iter().map(|d| (*d).to_owned()).collect::<Vec<_>>(),
            "frame {at} ({beat}): which panel is dimmed"
        );
    }

    // ADR 0052's red frame: only the tightening's right reaction and the summary carry it, with the overlay's label.
    let overlay: Value = serde_json::from_str(&read(
        &root().join(DRIFT).join("tightening").join("overlay.json"),
    ))
    .expect("the overlay is JSON");
    let label = overlay["boundaries"][0]["label"].as_str().expect("a label");
    let framed: Vec<usize> = (0..11).filter(|at| texts[*at].contains(label)).collect();
    assert_eq!(
        framed,
        vec![9, 10],
        "the red frame is drawn for the tightening and only there"
    );
}

const HOW_PNG: &str = "docs/architecture/drift/req-how-it-works.png";
const HOW_RECORD: &str = "docs/architecture/drift/req-how-it-works.png.json";

/// The README's "How it works" picture replaced a Mermaid block (P-3-fix, owner review of PR #100). Its wording is fixed in the
/// script, so its record binds the script and the PNG, not data files; a changed script or PNG fails here and names the command.
#[test]
fn the_how_it_works_picture_is_bound_to_its_script_and_its_own_bytes() {
    let record: Value = serde_json::from_str(&read(&root().join(HOW_RECORD)))
        .unwrap_or_else(|e| panic!("{HOW_RECORD}: {e}"));
    assert_eq!(record["schema"], "req.drift-diagram/1");
    assert_eq!(record["png"], HOW_PNG);
    assert_eq!(
        record["png_sha256"].as_str().expect("a hash"),
        sha256_hex(&root().join(HOW_PNG)),
        "{HOW_PNG} is not the file the record describes. Regenerate both by hand: `{REGENERATE}`"
    );
    assert_eq!(record["script"], SCRIPT);
    assert_eq!(
        record["script_sha256"].as_str().expect("a hash"),
        sha256_hex(&root().join(SCRIPT)),
        "{SCRIPT} changed since the picture was made: it is stale. Regenerate it by hand: `{REGENERATE}`"
    );
    let bytes = fs::read(root().join(HOW_PNG)).unwrap_or_else(|e| panic!("{HOW_PNG}: {e}"));
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{HOW_PNG} is a PNG");
    let be =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    assert_eq!(
        record["frame_size"],
        serde_json::json!([be(16), be(20)]),
        "the record's size is the PNG's"
    );
    assert!(
        (1600..=1800).contains(&be(16)),
        "{} px wide: it shows at the README's full width",
        be(16)
    );
    assert!(
        bytes.len() < 600 * 1024,
        "{} bytes: a picture beside the GIF stays small",
        bytes.len()
    );
    let version = record["chrome"].as_str().unwrap_or("");
    assert!(
        version.to_lowercase().contains("chrome") && version.chars().any(|c| c.is_ascii_digit()),
        "the record carries the chrome version: `{version}`"
    );
}

/// The script refuses, non-zero and by name, when Chrome or ffmpeg is absent, and writes nothing. It does not look for
/// another renderer.
#[test]
fn the_script_refuses_without_chrome_or_ffmpeg_and_names_what_is_missing() {
    let scratch = Scratch::new("refuse");
    let out = scratch.0.join("out");
    let chrome = scratch.0.join("no-such-chrome");
    let ffmpeg = scratch.0.join("no-such-ffmpeg");
    let run = script(
        &["--out", out.to_str().expect("a path")],
        &[
            ("REQ_REPLAY_CHROME", &chrome),
            ("REQ_REPLAY_FFMPEG", &ffmpeg),
        ],
    );
    assert_eq!(
        run.code,
        Some(3),
        "UNVERIFIABLE (3): nothing was rendered:\n{}",
        run.said
    );
    for missing in [&chrome, &ffmpeg] {
        assert!(
            run.said.contains(&*missing.to_string_lossy()),
            "the refusal names {}:\n{}",
            missing.display(),
            run.said
        );
    }
    let written: BTreeMap<String, ()> = fs::read_dir(&scratch.0)
        .expect("the scratch dir")
        .filter_map(Result::ok)
        .map(|e| (e.file_name().to_string_lossy().into_owned(), ()))
        .collect();
    assert!(
        !written.contains_key("out") || fs::read_dir(&out).is_ok_and(|mut d| d.next().is_none()),
        "nothing is written when a renderer is missing: {written:?}"
    );
}
