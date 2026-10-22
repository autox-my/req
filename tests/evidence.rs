//! The evidence of recorded claims whose proof is **structural or measured**, written down as
//! tests so a claim's `tests:` link names a real check rather than a hope.
//!
//! This file exists because two of req's own claims cannot be proven by a behavioural example:
//!
//! * REQ-0012 — *no network access for any claim operation configured offline* and REQ-0015 —
//!   *shall not operate a continuous two-way synchronization bridge*. An absence is not observed
//!   by exercising one path; it is observed in the **shape of the program** (no network client in
//!   the dependency graph, no socket API, no background runtime) and in its **interface** (no
//!   sync/poll command). These tests read the crate's own sources and manifest, which is exactly
//!   the reviewable surface for a negative architectural claim.
//! * REQ-0009 and REQ-0010 — the direction gold set's accuracy soft target and the parity corpus's
//!   false-split soft target. `req bench` measures them, but the gate's bench step is not a test
//!   the JUnit report carries; these tests evaluate the corpora **in process** (the pure seam,
//!   no result file written) and assert the published targets.
//! * REQ-0018 — numeric fields serialize as exact decimal strings. Pinned on the canonical bytes
//!   themselves: `"0.3"` as a JSON string, never the number `0.3` and never `"0.30"`.
//!
//! Every expectation here is independent of the implementation it judges: the dependency list is
//! the manifest (a human review artifact), the socket list is the source text, and the two rates
//! are computed from the corpora's own labels.

use req::bench::evaluate;
use req::canon::{canonical_json, canonicalize_statement};
use req::model::{Corpus, Registry};
use std::path::{Path, PathBuf};

/// The crate root, at runtime — the same path discipline every other test uses.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Dependency names in a Cargo manifest's `[dependencies]` table.
///
/// Deliberately a small line reader, not a TOML parser: the question is which crates the **core**
/// links, and a full parse would make this test depend on the format's whole grammar to answer a
/// question that only needs the table's left-hand sides.
fn runtime_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_dependencies = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line == "[dependencies]";
            continue;
        }
        if !in_dependencies || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            deps.push(name.trim().to_string());
        }
    }
    deps
}

/// The crates that would put a network client in the core: HTTP clients, TLS stacks, sockets,
/// WebSocket clients and the async runtimes they need. A new dependency is a decision a human
/// makes; this list is what makes REQ-0012's absence **checkable** rather than asserted in prose.
const NETWORK_OR_ASYNC: &[&str] = &[
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "surf",
    "attohttpc",
    "tungstenite",
    "tokio-tungstenite",
    "rustls",
    "native-tls",
    "openssl",
    "socket2",
    "mio",
    "tokio",
    "async-std",
    "smol",
    "axum",
    "warp",
];

/// The socket and concurrency APIs a network client or a polling bridge would have to call.
const NETWORK_AND_LOOP_APIS: &[&str] = &[
    "std::net",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "std::thread",
    "thread::spawn",
    "async fn",
    "tokio::",
];

/// Every Rust source file of the core crate (the binary's direct sources only; the test tree is
/// not the shipped program).
fn source_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let src = repo_root().join("src");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("the crate has a src/ directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    entries.sort();
    files.extend(entries);
    let build = repo_root().join("build.rs");
    if build.is_file() {
        files.push(build);
    }
    files
}

/// **REQ-0012.** The core crate links no network client and contains no socket API, so a claim
/// operation cannot reach the network even if one were present — the air-gapped deployment is a
/// property of the program, not of a runtime flag.
#[test]
fn the_core_crate_carries_no_network_client() {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("the crate manifest is readable");
    let deps = runtime_dependencies(&manifest);
    assert!(
        !deps.is_empty(),
        "the dependency reader found no [dependencies] entries — it would pass vacuously"
    );

    let banned: Vec<&str> = deps
        .iter()
        .map(String::as_str)
        .filter(|name| NETWORK_OR_ASYNC.contains(name))
        .collect();
    assert!(
        banned.is_empty(),
        "the core crate links a network client or async runtime: {banned:?} — a claim operation \
         could then need the network (REQ-0012)"
    );

    let sources = source_files();
    assert!(!sources.is_empty(), "the source scan found no files");
    let mut offenders = Vec::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).expect("a source file is readable");
        for api in NETWORK_AND_LOOP_APIS {
            if text.contains(api) {
                offenders.push(format!("{}: {api}", path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "network/concurrency API in the core crate: {offenders:?} — claim operations must stay \
         offline (REQ-0012)"
    );
}

/// **REQ-0015.** The tool operates no continuous two-way synchronization bridge: there is no
/// background runtime or spawned loop in the crate, and the command surface exposes no
/// sync/mirror/watch verb. Git is transport and the corpus is read per invocation — a bridge
/// would have to be one of these, and there is none.
#[test]
fn no_command_starts_a_sync_or_polling_loop() {
    // The same source scan as REQ-0012's covers the loop half: a polling bridge needs a runtime
    // or a spawned thread, and neither appears.
    let sources = source_files();
    let mut offenders = Vec::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).expect("a source file is readable");
        for api in ["std::thread", "thread::spawn", "loop {", "while true"] {
            // `loop {` alone would be a parser loop, which is not a bridge; only the combination
            // with a concurrency primitive is the thing this claim forbids. The concurrency half
            // is what is asserted, so a bare `loop {` is not an offender.
            if api == "loop {" && !text.contains("std::thread") && !text.contains("thread::spawn") {
                continue;
            }
            if text.contains(api) {
                offenders.push(format!("{}: {api}", path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a background loop/runtime is the shape of a sync bridge, and the crate has none \
         (REQ-0015): {offenders:?}"
    );

    // The interface half: every verb is a one-shot command over the corpus. A bridge would have
    // to be named; the help text is the published command surface.
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_req"))
        .arg("--help")
        .output()
        .expect("the compiled binary answers --help");
    assert!(help.status.success(), "--help is a successful answer");
    let text = String::from_utf8_lossy(&help.stdout).to_lowercase();
    for verb in ["sync", "bridge", "mirror", "watch", "daemon", "poll"] {
        assert!(
            !text.contains(verb),
            "the command surface names a continuous operation ({verb:?}) — a two-way bridge is \
             what REQ-0015 forbids:\n{text}"
        );
    }
}

/// The `req bench` gate's own measurements, in process: load the corpus, evaluate it, assert the
/// published target. `evaluate` is pure, so this writes no result record — the append-only
/// history under `bench/results/` is the CLI's, not a test's.
fn evaluate_corpus(relative: &str) -> (Corpus, Registry, req::bench::Evaluation) {
    let root = repo_root();
    let corpus = Corpus::load(root.join(relative).to_str().expect("a UTF-8 path"))
        .expect("the corpus loads");
    let registry = Registry::load(root.join("aliases/subjects.yml").to_str().unwrap())
        .expect("the registry loads");
    let evaluation = evaluate(&corpus, &registry);
    (corpus, registry, evaluation)
}

/// **REQ-0009.** On the structural direction gold set the harness classifies direction with
/// accuracy of at least 0.95, with **zero** dangerous confusions. The accuracy target is a soft
/// one (a harmless misread is noise); the hard gate stays zero dangerous confusions, which is the
/// direction analogue of a false merge.
#[test]
fn the_direction_gold_set_meets_its_accuracy_target() {
    let (_corpus, _registry, evaluation) = evaluate_corpus("bench/corpus/direction-v1.yml");
    assert!(
        evaluation.direction_checked > 0,
        "an accuracy over no classified pairs is vacuous, not a measurement: {:?}",
        evaluation.direction_checked
    );
    assert!(
        evaluation.direction_label_errors.is_empty(),
        "a misspelled direction label switches the check off: {:?}",
        evaluation.direction_label_errors
    );
    assert!(
        evaluation.dangerous.is_empty(),
        "a dangerous direction confusion is the analogue of a false merge: {:?}",
        evaluation.dangerous
    );
    let accuracy = evaluation.direction_correct as f64 / evaluation.direction_checked as f64;
    assert!(
        accuracy >= 0.95,
        "direction accuracy {accuracy:.6} is below the 0.95 soft target ({} correct of {})",
        evaluation.direction_correct,
        evaluation.direction_checked
    );
}

/// **REQ-0010.** On the seed parity corpus the false-split rate is at most 0.30 at v0. The
/// denominator is the structured-equivalent pairs — a rate over none would be vacuous, so it is
/// asserted non-zero rather than read as zero.
#[test]
fn the_parity_corpus_meets_its_false_split_target() {
    let (_corpus, _registry, evaluation) = evaluate_corpus("bench/corpus/parity.yml");
    assert!(
        evaluation.structured_equivalent > 0,
        "the false-split rate has no denominator — nothing was measured"
    );
    let fsr = evaluation.fsr_hits.len() as f64 / evaluation.structured_equivalent as f64;
    assert!(
        fsr <= 0.30,
        "false-split rate {fsr:.6} is above the 0.30 soft target ({} splits of {} equivalent \
         pairs)",
        evaluation.fsr_hits.len(),
        evaluation.structured_equivalent
    );
}

/// **REQ-0018.** A numeric claim field is serialized as an **exact decimal string**: the canonical
/// bytes carry `"value":"0.3"` — a JSON string, canonicalized, never the float `0.3` and never the
/// non-canonical `"0.30"`. The oracle is the documented canonical-decimal rule, not the
/// serializer's output.
#[test]
fn canonical_forms_serialize_quantities_as_exact_decimal_strings() {
    let registry = Registry::empty();
    // 0.30 s canonicalizes to 0.3 s: the trailing zero is dropped, exactly as the rule requires,
    // and the JSON value is a string.
    let (form, _lints) =
        canonicalize_statement("The system shall respond within 0.30 seconds.", &registry);
    let json = canonical_json(&form);
    let value: serde_json::Value = serde_json::from_str(&json).expect("canonical form is JSON");
    let value = value
        .get("value")
        .expect("the canonical form carries `value`");
    assert!(
        value.is_string(),
        "a quantity must serialize as an exact decimal **string**, not a JSON number: {value:?} \
         in {json}"
    );
    assert_eq!(
        value.as_str(),
        Some("0.3"),
        "the canonical decimal is `0.3`, never `0.30` or a float: {json}"
    );

    // A second value whose exact form differs from the authored text (leading zero, exponent)
    // pins the canonicalization rather than one lucky string.
    let (form, _lints) =
        canonicalize_statement("The system shall respond within 200 ms.", &registry);
    let json = canonical_json(&form);
    let value: serde_json::Value = serde_json::from_str(&json).expect("canonical form is JSON");
    assert_eq!(
        value.get("value").and_then(serde_json::Value::as_str),
        Some("0.2"),
        "200 ms is exactly 0.2 s, as a string: {json}"
    );
}

/// The path the tests above scan is the one `scripts/check.sh` runs the gate from: asserted once,
/// so a changed layout makes the structural claims vacuous loudly rather than silently.
#[test]
fn the_scanned_source_tree_is_the_shipped_crate() {
    let sources = source_files();
    assert!(
        sources
            .iter()
            .any(|path| path.ends_with(Path::new("src/main.rs"))),
        "the source scan must reach src/main.rs: {sources:?}"
    );
    assert!(
        sources
            .iter()
            .any(|path| path.ends_with(Path::new("src/canon.rs"))),
        "the source scan must reach src/canon.rs: {sources:?}"
    );
}
