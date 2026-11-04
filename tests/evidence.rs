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

/// Dependency names a Cargo manifest declares, in **every spelling that links a crate**.
///
/// A small line reader, not a TOML parser — but not a naive one: `[dependencies.reqwest]` (table
/// form), `[target.'cfg(unix)'.dependencies]` (target-specific) and `http = { package = "reqwest" }`
/// (renamed) all link the crate, and all three are invisible to a reader that only looks at
/// `key = value` lines under a literal `[dependencies]`. The caller asserts the *known* set was
/// found, so a reader regression fails instead of passing vacuously.
fn runtime_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_dependencies = false;
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let header = line.trim_matches(|c| c == '[' || c == ']');
            in_dependencies = header == "dependencies"
                || header.starts_with("dependencies.")
                || header.ends_with(".dependencies");
            if let Some(name) = header.strip_prefix("dependencies.") {
                deps.push(name.to_string());
            }
            continue;
        }
        if !in_dependencies {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        deps.push(name.trim().to_string());
        // `http = { package = "reqwest", … }` links `reqwest`, not `http`.
        if let Some(quoted) = value
            .split("package")
            .nth(1)
            .and_then(|after| after.split('"').nth(1))
        {
            deps.push(quoted.to_string());
        }
    }
    deps.sort();
    deps.dedup();
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
    /// Every `*.rs` under `dir`, recursively. A flat `read_dir(&src)` would miss
    /// `src/net/client.rs` — and a network client in a submodule is still a network client, which is
    /// exactly the absence REQ-0012 asserts.
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("reads the source directory") {
            let path = entry.expect("reads a directory entry").path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&repo_root().join("src"), &mut found);
    found.sort();
    found
}

/// **REQ-0012.** The core crate links no network client and contains no socket API, so a claim
/// operation cannot reach the network even if one were present — the air-gapped deployment is a
/// property of the program, not of a runtime flag.
#[test]
fn the_core_crate_carries_no_network_client() {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("the crate manifest is readable");
    let deps = runtime_dependencies(&manifest);
    // The reader must have found the crates this manifest is known to declare: an empty or partial
    // read would pass the absence check vacuously, which is the failure mode this assertion closes.
    for expected in [
        "serde",
        "serde_json",
        "yaml_serde",
        "rust_decimal",
        "sha2",
        "unicode-normalization",
    ] {
        assert!(
            deps.iter().any(|dep| dep == expected),
            "the dependency reader missed `{expected}` (it must see table, target-specific and \
             renamed forms): {deps:?}"
        );
    }

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

// --- Four claims whose proof is a behaviour of the whole tool ------------------------------------
//
// REQ-0001, REQ-0006, REQ-0007 and REQ-0021 were linked to tests written for other purposes. The
// review caught REQ-0006's link directly: `impact`'s traversal test builds `Row` values in memory,
// so it stays green even if the loader starts refusing derived claims. The strict bar is "if this
// test failed, would the requirement be rejected?" — so each of the four gets a test named for the
// claim and asserting exactly what the claim obliges, through the CLI seam.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

/// A directory with no `claims/` ancestor, so a fixture that starts empty really is empty.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let seq = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("{name}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the scratch directory");
        if let Some(found) = dir.ancestors().find(|d| req::root::is_corpus_root(d)) {
            panic!(
                "the scratch directory assumes no ancestor holds claims/, but {} does",
                found.display()
            );
        }
        Scratch(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap_or_else(|e| panic!("reads {rel}: {e}"))
    }
    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("creates the parent directory");
        }
        std::fs::write(&path, text).unwrap_or_else(|e| panic!("writes {rel}: {e}"));
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_here(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(dir)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Every `claims/*.yml`, sorted, as `(file name, bytes)` — the snapshot an automatic rewrite or
/// merge would change.
fn claim_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir.join("claims"))
        .expect("reads claims/")
        .filter_map(|entry| {
            let entry = entry.expect("reads a directory entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".yml")
                .then(|| (name, std::fs::read(entry.path()).expect("reads a claim")))
        })
        .collect();
    found.sort();
    found
}

/// The rows of a `render --format json` document, from all three sections.
fn render_rows(stdout: &str) -> Vec<serde_json::Value> {
    let document: serde_json::Value =
        serde_json::from_str(stdout).expect("render --format json emits a JSON document");
    ["applies", "undecided", "outside"]
        .iter()
        .flat_map(|section| {
            document[section]
                .as_array()
                .unwrap_or_else(|| panic!("render document has no `{section}` array"))
                .clone()
        })
        .collect()
}

fn address_of(rows: &[serde_json::Value], claim_id: &str) -> String {
    rows.iter()
        .find(|row| row["claim_id"] == claim_id)
        .unwrap_or_else(|| panic!("no row for {claim_id}"))["address"]
        .as_str()
        .expect("a row address is a string")
        .to_string()
}

/// `req init`'s starter statement, exactly as the template writes it. The starter is the generic
/// API example, not this repository's own REQ-0001 — a fixture that assumed otherwise edits nothing.
const STARTER_STATEMENT: &str =
    "statement: \"The API shall respond to a health check within 200 ms.\"\n";
const STARTER_MAY: &str = "statement: \"The API may respond to a health check within 200 ms.\"\n";
const STARTER_SHOULD: &str =
    "statement: \"The API should respond to a health check within 200 ms.\"\n";
const STARTER_500: &str = "statement: \"The API shall respond to a health check within 500 ms.\"\n";

/// A minimal nextest-shaped report carrying the ids the corpus links.
fn write_report(dir: &Path, ids: &[&str]) -> PathBuf {
    let cases: String = ids
        .iter()
        .map(|id| {
            let (classname, name) = id.rsplit_once("::").expect("an id is classname::name");
            format!("    <testcase name=\"{name}\" classname=\"{classname}\"/>\n")
        })
        .collect();
    let path = dir.join("junit.xml");
    std::fs::write(
        &path,
        format!(
            "<testsuites name=\"fixture\" tests=\"{}\">\n  <testsuite name=\"req\" tests=\"{}\">\n{cases}  </testsuite>\n</testsuites>\n",
            ids.len(),
            ids.len()
        ),
    )
    .expect("writes the report fixture");
    path
}

/// REQ-0001 — *a single claim is the unit of authorship, review, and merge*: one file is one claim,
/// one claim is one row, one claim is one address, and editing one claim moves only its own address.
#[test]
fn a_claim_is_the_unit_of_authorship_review_and_merge() {
    let scratch = Scratch::new("req-0001-unit");
    assert_eq!(run_here(scratch.path(), &["init"]).0, 0, "init bootstraps");

    let (code, stdout, _) = run_here(scratch.path(), &["render", "--format", "json"]);
    assert_eq!(code, 0);
    assert_eq!(render_rows(&stdout).len(), 1, "one claim file, one row");

    // A second obligation: copying the file is not enough — identity is over **meaning**, so the
    // obligation itself must differ (a different modality), not merely its wording.
    let starter = scratch.read("claims/REQ-0001.yml");
    assert!(
        starter.contains(STARTER_STATEMENT),
        "the fixture edits the template's own statement"
    );
    let second = starter
        .replace("claim_id: REQ-0001", "claim_id: REQ-0002")
        .replace(STARTER_STATEMENT, STARTER_MAY);
    scratch.write("claims/REQ-0002.yml", &second);
    // Nothing to fill (ADR 0022): the loader derives both claims' parses and addresses.
    assert_eq!(run_here(scratch.path(), &["canon"]).0, 0);

    let (_, stdout, _) = run_here(scratch.path(), &["render", "--format", "json"]);
    let rows = render_rows(&stdout);
    assert_eq!(rows.len(), 2, "two files, two rows");
    let first_before = address_of(&rows, "REQ-0001");
    let second_before = address_of(&rows, "REQ-0002");
    assert_ne!(
        first_before, second_before,
        "two different obligations must not share an address"
    );

    // A meaning edit moves that claim's address — and only that one.
    let edited = second.replace(STARTER_MAY, STARTER_SHOULD);
    scratch.write("claims/REQ-0002.yml", &edited);
    let (_, stdout, _) = run_here(scratch.path(), &["render", "--format", "json"]);
    let rows = render_rows(&stdout);
    assert_eq!(rows.len(), 2, "editing a claim merges and splits nothing");
    assert_eq!(
        address_of(&rows, "REQ-0001"),
        first_before,
        "an untouched claim keeps its address"
    );
    assert_ne!(
        address_of(&rows, "REQ-0002"),
        second_before,
        "the edited claim's address moved"
    );
    assert_eq!(
        claim_files(scratch.path()).len(),
        2,
        "one file per claim, still"
    );
}

/// REQ-0006 — *permit splitting a claim into derived claims without constraint*: a three-deep chain
/// of authored `derives-from` relations loads, re-derives, and resolves — no Decision demanded, no
/// arity limit, no refusal.
#[test]
fn derived_claims_are_accepted_without_constraint() {
    let scratch = Scratch::new("req-0006-derived");
    assert_eq!(run_here(scratch.path(), &["init"]).0, 0);
    let starter = scratch.read("claims/REQ-0001.yml");

    // Each derived claim must carry a *distinct obligation*: two statements that parse to the same
    // form share an address — identity is over meaning, not a bug. Modality and the bound are both
    // in the form; the starter's `value` pin follows the bound so every pin still holds.
    assert!(
        starter.contains(STARTER_STATEMENT),
        "the fixture edits the template's own statement"
    );
    for (id, parent, statement, pinned_value) in [
        ("REQ-0002", "REQ-0001", STARTER_MAY, "value: \"0.2\""),
        ("REQ-0003", "REQ-0002", STARTER_SHOULD, "value: \"0.2\""),
        ("REQ-0004", "REQ-0003", STARTER_500, "value: \"0.5\""),
    ] {
        assert!(
            starter.contains("value: \"0.2\""),
            "the starter template pins `value: \"0.2\"`: the pin edit below would be a no-op"
        );
        let derived = starter
            .replace("claim_id: REQ-0001", &format!("claim_id: {id}"))
            .replace(STARTER_STATEMENT, statement)
            .replace("value: \"0.2\"", pinned_value)
            .replace(
                "relations: []",
                &format!("relations:\n  - type: derives-from\n    to: {parent}"),
            );
        assert!(
            derived.contains(pinned_value),
            "the pin edit did not land ({pinned_value}):\n{derived}"
        );
        assert!(
            derived.contains(&format!("to: {parent}")),
            "the fixture's relation edit is a no-op: the starter template changed its `relations:` line"
        );
        scratch.write(&format!("claims/{id}.yml"), &derived);
    }

    let (canon_code, canon_out, canon_err) = run_here(scratch.path(), &["canon"]);
    assert_eq!(
        canon_code, 0,
        "the loader accepts the derived claims: {canon_out}{canon_err}"
    );
    // The property per claim, from the structural lines (`  pin <key> <value>: …`, one under each
    // claim's row): every one of the four claims has at least one pin verdict and none is a flag.
    for id in ["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"] {
        let mut lines = canon_out.lines();
        assert!(
            lines.any(|l| l.starts_with(id)),
            "canon prints a row for {id}:\n{canon_out}"
        );
        let verdicts: Vec<&str> = lines
            .take_while(|l| l.starts_with("  "))
            .filter(|l| l.trim_start().starts_with("pin "))
            .collect();
        assert!(!verdicts.is_empty(), "{id} has pins to hold:\n{canon_out}");
        assert!(
            verdicts.iter().all(|l| !l.contains("FLAG")),
            "{id}: every pin holds: {verdicts:?}"
        );
    }

    let (code, stdout, stderr) = run_here(scratch.path(), &["trace"]);
    assert_eq!(
        code, 0,
        "a three-deep derived chain resolves, and no command demands a Decision first:\n{stdout}{stderr}"
    );
    for id in ["REQ-0001", "REQ-0002", "REQ-0003", "REQ-0004"] {
        assert!(stdout.contains(id), "trace names {id}");
    }
    let refused = stderr.to_lowercase();
    for demand in ["decision", "arity", "constraint"] {
        assert!(
            !refused.contains(demand),
            "a split must be accepted without a `{demand}`: {stderr}"
        );
    }

    let (_, stdout, _) = run_here(scratch.path(), &["render", "--format", "json"]);
    let rows = render_rows(&stdout);
    assert_eq!(
        rows.len(),
        4,
        "every derived claim is one row and one claim"
    );
    let mut addresses: Vec<String> = rows
        .iter()
        .map(|row| row["address"].as_str().unwrap_or_default().to_string())
        .collect();
    addresses.sort();
    addresses.dedup();
    assert_eq!(addresses.len(), 4, "each obligation keeps its own address");
}

/// REQ-0007 — *authoring is friction-free and the decision boundary is deliberately high-friction*,
/// both halves in one measurement: one command boots a green corpus, and the same corpus refuses to
/// pass until the variant it carries is decided.
#[test]
fn authoring_is_frictionless_and_the_decision_boundary_is_not() {
    let scratch = Scratch::new("req-0007-boundary");
    // Half one: one command, and the corpus is green (no authoring ceremony).
    assert_eq!(run_here(scratch.path(), &["init"]).0, 0);
    assert_eq!(run_here(scratch.path(), &["canon"]).0, 0);

    // Half two: an applicability the invocation does not decide is never treated as absent.
    let claim = scratch.read("claims/REQ-0001.yml");
    assert!(
        claim.contains("applicability: []"),
        "the starter has no selector"
    );
    assert!(
        !claim.contains("\ntests:"),
        "the starter carries no evidence link — this test authors one, as an adopter would"
    );
    let link = "req::evidence::authoring_is_frictionless_and_the_decision_boundary_is_not";
    scratch.write(
        "claims/REQ-0001.yml",
        &claim
            .replace("applicability: []", "applicability: [\"runs >= 100\"]")
            .replace(
                "relations: []\n",
                &format!("relations: []\ntests:\n  - \"{link}\"\nevidence: machine\n"),
            ),
    );

    let linked = "req::evidence::authoring_is_frictionless_and_the_decision_boundary_is_not";
    let report = write_report(scratch.path(), &[linked]);
    let report_arg = report.to_string_lossy().into_owned();
    let (undecided_code, undecided_out, _) =
        run_here(scratch.path(), &["gate", "--test-report", &report_arg]);
    assert_eq!(
        undecided_code, 3,
        "the boundary refuses to pass on an undecided claim:\n{undecided_out}"
    );
    let (decided_code, decided_out, _) = run_here(
        scratch.path(),
        &["gate", "runs=100", "--test-report", &report_arg],
    );
    assert_eq!(
        decided_code, 0,
        "deciding the variant clears the boundary:\n{decided_out}"
    );
    assert!(decided_out.contains("verdict: PASS"), "{decided_out}");
}

/// REQ-0021 — *never rewrite a claim or merge two claims automatically*.
///
/// Two claims, because a merge path needs a pair: with one claim it could never fire, and the test
/// would be green for the wrong reason (the review caught exactly that). Every observation is on
/// the **whole bytes** of every claim, not one line: a read-only command must leave the corpus
/// byte-identical, and there is no writer of derived fields left to run (`canon --fill` is a
/// usage error since `req.claim/2`, ADR 0022) — it must write nothing at all either.
#[test]
fn no_command_rewrites_or_merges_claims_automatically() {
    let scratch = Scratch::new("req-0021-readonly");
    assert_eq!(run_here(scratch.path(), &["init"]).0, 0);
    let starter = scratch.read("claims/REQ-0001.yml");
    assert!(
        starter.contains(STARTER_STATEMENT),
        "the fixture edits the template's own statement"
    );
    scratch.write(
        "claims/REQ-0002.yml",
        &starter
            .replace("claim_id: REQ-0001", "claim_id: REQ-0002")
            .replace(STARTER_STATEMENT, STARTER_MAY),
    );
    assert_eq!(run_here(scratch.path(), &["canon"]).0, 0);
    let before = claim_files(scratch.path());
    assert_eq!(before.len(), 2, "two obligations, two files");

    // `bench` writes a results record under bench/results; `propose` takes the explicit pair and
    // answers UNVERIFIABLE offline. Neither may touch a claim.
    let parity = format!("{}/bench/corpus/parity.yml", env!("CARGO_MANIFEST_DIR"));
    assert_eq!(run_here(scratch.path(), &["bench", &parity]).0, 0);
    let (propose_code, _, _) = run_here(
        scratch.path(),
        &["propose", "claims/REQ-0001.yml", "claims/REQ-0002.yml"],
    );
    assert_eq!(
        propose_code, 3,
        "propose answers UNVERIFIABLE offline, without writing"
    );
    assert_eq!(
        claim_files(scratch.path()),
        before,
        "a read-only command rewrote or merged a claim"
    );

    // The removed writer writes nothing: `canon --fill` and `--verify` are usage errors, every
    // byte stays where it was, and no file disappears (a merge would remove one).
    for removed in ["--fill", "--verify"] {
        assert_eq!(run_here(scratch.path(), &["canon", removed]).0, 2);
    }
    assert_eq!(run_here(scratch.path(), &["canon"]).0, 0);
    assert_eq!(
        claim_files(scratch.path()),
        before,
        "a command changed a claim"
    );
    assert_eq!(
        claim_files(scratch.path()).len(),
        2,
        "a merge changed the claim set"
    );
}

/// Every file under `dir` as `(relative path, bytes)`, sorted — the whole tree, not only
/// `claims/`, so a proposal written anywhere (a claim, a sidecar, a log) shows as a difference.
fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(at).expect("reads a directory") {
            let path = entry.expect("reads an entry").path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((rel, std::fs::read(&path).expect("reads a file")));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// REQ-0004 — *the req tool shall emit proposals for reviewer ratification* (ADR 0004, ADR 0021).
///
/// What the core guarantees is the single-pair seam: a provider's answer comes out as an
/// **unratified proposal** and never as an edit. Observed through the CLI on two real claim files:
/// the whole scratch tree is byte-identical after every run (no file written, no address moved);
/// with no provider the answer is UNVERIFIABLE, never "unrelated"; with a provider the label is
/// printed under the `proposal (unratified)` marker with the provider command recorded verbatim;
/// a provider that fails or answers nothing readable is UNVERIFIABLE, and a bare call is usage.
#[test]
fn a_provider_answer_is_an_unratified_proposal_never_an_edit() {
    let scratch = Scratch::new("req-0004-seam");
    assert_eq!(run_here(scratch.path(), &["init"]).0, 0);
    let starter = scratch.read("claims/REQ-0001.yml");
    scratch.write(
        "claims/REQ-0002.yml",
        &starter
            .replace("claim_id: REQ-0001", "claim_id: REQ-0002")
            .replace(STARTER_STATEMENT, STARTER_MAY),
    );
    let before = tree(scratch.path());
    let pair = ["claims/REQ-0001.yml", "claims/REQ-0002.yml"];
    let propose = |extra: &[&str]| {
        let mut args = vec!["propose"];
        args.extend(pair);
        args.extend(extra);
        run_here(scratch.path(), &args)
    };

    // Offline: nothing is proposed rather than guessed.
    let (code, stdout, stderr) = propose(&[]);
    assert_eq!(code, 3, "no provider is UNVERIFIABLE:\n{stdout}{stderr}");
    assert!(
        format!("{stdout}{stderr}").contains("UNVERIFIABLE"),
        "named as such:\n{stdout}{stderr}"
    );
    assert!(
        !stdout.contains("proposal (unratified)") && !stdout.to_lowercase().contains("unrelated"),
        "an offline run proposes nothing and does not guess:\n{stdout}"
    );

    // A provider that answers: a proposal, labelled, with its provenance. The provider also
    // records what req wrote to its stdin, in a directory outside the scratch corpus.
    let sink = Scratch::new("req-0004-seam-sink");
    let seen = sink.path().join("stdin.txt");
    let command = format!("cat > {}; echo equivalent", seen.display());
    let (code, stdout, stderr) = propose(&["--provider-cmd", &command]);
    assert_eq!(
        code, 0,
        "a provider's answer is a proposal:\n{stdout}{stderr}"
    );
    assert!(
        stdout.contains("proposal (unratified): equivalent"),
        "the label travels under the unratified marker:\n{stdout}"
    );
    assert!(
        stdout.contains(&command),
        "provider recorded verbatim:\n{stdout}"
    );
    let request = std::fs::read_to_string(&seen).expect("the provider was given the pair");
    for prefix in ["ID: ", "A: ", "B: ", "DIFF: ", "CHOOSE: "] {
        assert!(
            request.contains(prefix),
            "line protocol {prefix:?}:\n{request}"
        );
    }

    // A JSON answer carries its confidence; it is still only a proposal.
    let json = r#"cat >/dev/null; echo '{"choice":"related","confidence":87}'"#;
    let (code, stdout, _) = propose(&["--provider-cmd", json]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("proposal (unratified): related"),
        "{stdout}"
    );
    assert!(stdout.contains("87"), "confidence is shown:\n{stdout}");

    // A failing or unreadable provider is UNVERIFIABLE, never a proposal.
    for broken in ["cat >/dev/null; exit 1", "cat >/dev/null; echo hello"] {
        let (code, stdout, _) = propose(&["--provider-cmd", broken]);
        assert_eq!(code, 3, "{broken}:\n{stdout}");
        assert!(
            !stdout.contains("proposal (unratified)"),
            "{broken}:\n{stdout}"
        );
    }

    // Usage: a pair is two files.
    assert_eq!(run_here(scratch.path(), &["propose"]).0, 2);
    assert_eq!(run_here(scratch.path(), &["propose", pair[0]]).0, 2);
    assert_eq!(
        tree(scratch.path()),
        before,
        "`req propose` wrote or changed a file; a proposal is never an edit"
    );
}

/// Every claim's linked test id must exist — checked under `cargo test`, not only in CI.
///
/// `tests/release_gate.rs`'s dogfood report is synthesized *from* these links and marks every one
/// passed, so it can never fail on a stale, renamed or typo'd id: the real report lives only in
/// `scripts/check.sh`, outside the suite. This is the oracle that can fail under `cargo test` — a
/// checked-in list of the ids as nextest writes them (`classname::name`), taken from a real report
/// at pin time, plus the report itself when one is on disk.
#[test]
fn every_linked_test_id_exists() {
    let oracle: Vec<String> =
        std::fs::read_to_string(repo_root().join("tests/linked-test-ids.txt"))
            .expect("the linked-id oracle is readable")
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .map(|line| line.trim().to_string())
            .collect();

    let mut found: Vec<String> = Vec::new();
    let mut missing = Vec::new();
    for claim in std::fs::read_dir(repo_root().join("claims")).expect("reads claims/") {
        let path = claim.expect("reads a directory entry").path();
        let text = std::fs::read_to_string(&path).expect("reads a claim");
        let Some(block) = text
            .find("\ntests:\n")
            .map(|start| &text[start + "\ntests:\n".len()..])
        else {
            continue;
        };
        for line in block
            .lines()
            .take_while(|line| line.trim_start().starts_with('-'))
        {
            let id = line.trim().trim_start_matches('-').trim().trim_matches('"');
            found.push(id.to_string());
            if !oracle.iter().any(|listed| listed == id) {
                missing.push(format!("{}: {id}", path.display()));
            }
        }
    }
    assert!(
        !found.is_empty(),
        "no claim links were read — this test would pass vacuously"
    );
    assert!(
        missing.is_empty(),
        "a linked test id is not in tests/linked-test-ids.txt (stale, renamed or typo'd — \
         regenerate it from a real report): {missing:?}"
    );

    // When a real report is on disk (a local nextest run), the links must be in *it* too.
    let report = repo_root().join("target/nextest/ci/junit.xml");
    if let Ok(text) = std::fs::read_to_string(&report) {
        let mut rest = text.as_str();
        let mut real: Vec<String> = Vec::new();
        while let Some(at) = rest.find("<testcase ") {
            rest = &rest[at + 10..];
            let Some(end) = rest.find('>') else { break };
            let element = &rest[..end];
            let attr = |name: &str| -> Option<String> {
                let key = format!("{name}=\"");
                let start = element.find(&key)? + key.len();
                let tail = &element[start..];
                tail.find('"').map(|stop| tail[..stop].to_string())
            };
            if let (Some(name), Some(classname)) = (attr("name"), attr("classname")) {
                real.push(format!("{classname}::{name}"));
            }
            rest = &rest[end..];
        }
        let absent: Vec<&String> = found.iter().filter(|id| !real.contains(id)).collect();
        assert!(
            absent.is_empty(),
            "the report at {} does not carry these linked ids: {absent:?}",
            report.display()
        );
    }
}
