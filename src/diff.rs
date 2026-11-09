//! `req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]` (ADR 0026):
//! what changed in **meaning** between two states of a corpus.
//!
//! The command joins three things that already exist and judges nothing itself: the loader
//! ([`canon::load_claims`], so there is one place that judges a claim file), the direction
//! classifier ([`compare::compare`], so the quality of a direction is exactly `req compare`'s) and
//! the pin verdicts ([`Derived::pins`]). Claims are matched by `claim_id` (ADR 0001); the address
//! is only ever compared to say *whether the meaning moved*, and to offer a renumber **candidate**.
//!
//! A revision is read **without checking anything out**: the local `git` binary lists the two
//! trees this command needs (`claims/` and `aliases/`), their blobs are read raw, unfiltered, into
//! a private temporary directory, and the ordinary loaders run on that directory. `git` is a
//! process boundary (ADR 0009): its environment is scrubbed of every `GIT_*` redirection, it reads
//! no user or system configuration, no command used runs a hook, and none fetches or touches a
//! network. The temporary directory is removed on every way out, including a signal (see `Staging`).
//!
//! The exit is not a verdict. 0 the report was produced; 1 only with `--fail-on` and a named class
//! or flag present; 3 UNVERIFIABLE (it cannot be produced); 2 usage. The words PASS, FAIL and
//! UNVERIFIABLE never reach stdout: this describes a change, `req check` is the release verdict.

use crate::canon::{self, LoadedClaim};
use crate::compare::{self, Direction};
use crate::format::{self, Format};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, Read};
use std::path::Path;
use std::process::{Command, Stdio};

/// The document's schema string (ADR 0026 §6), versioned independently of `RULES_V` like every
/// command's (`format.rs`).
pub const SCHEMA_DIFF: &str = "req.diff/1";

const USAGE: &str =
    "usage: req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]";

/// What the report calls the working-tree side.
const WORKING_TREE: &str = "working-tree";

// ---------------------------------------------------------------------------
// Vocabulary: classes, flags, and the `--fail-on` spellings
// ---------------------------------------------------------------------------

/// The one class every claim id present on either side gets (ADR 0026 §2); `Unchanged` is counted
/// and never listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Added,
    Removed,
    Weakened,
    Tightened,
    Reshaped,
    NotJudged,
    Reworded,
    Metadata,
    Unchanged,
}

impl Class {
    const ALL: [Class; 9] = [
        Class::Added,
        Class::Removed,
        Class::Weakened,
        Class::Tightened,
        Class::Reshaped,
        Class::NotJudged,
        Class::Reworded,
        Class::Metadata,
        Class::Unchanged,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Class::Added => "added",
            Class::Removed => "removed",
            Class::Weakened => "weakened",
            Class::Tightened => "tightened",
            Class::Reshaped => "reshaped",
            Class::NotJudged => "not-judged",
            Class::Reworded => "reworded",
            Class::Metadata => "metadata",
            Class::Unchanged => "unchanged",
        }
    }

    /// The class a changed meaning earns from its direction (ADR 0026 §2).
    fn of_direction(direction: Direction) -> Class {
        if direction.is_loosening() {
            return Class::Weakened;
        }
        match direction {
            Direction::ModalityStrengthening
            | Direction::NumericTightening
            | Direction::MembershipNarrowing
            | Direction::ScopeNarrowing => Class::Tightened,
            Direction::Unclassifiable => Class::NotJudged,
            // A different obligation rather than a stronger or weaker one; and `Identical`, which
            // cannot reach here (identical canonical forms share an address, and an address that
            // differs is a form that differs) — a human reads it rather than trusting that proof.
            Direction::PolarityFlip
            | Direction::ScopeDivergence
            | Direction::Unrelated
            | Direction::Identical => Class::Reshaped,
            Direction::ModalityWeakening
            | Direction::NumericLoosening
            | Direction::MembershipWidening
            | Direction::ScopeWidening => Class::Weakened,
        }
    }
}

/// A fact about a claim beside its class. Declaration order is print order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Flag {
    PinsChanged,
    PinFlagged,
    TestsLost,
    TestsAdded,
    /// An added claim has the meaning fingerprint of a removed one: a candidate renumber, reported
    /// as a candidate and never inferred as a rename (ADR 0017 §2).
    SameAddressAs(String),
}

impl Flag {
    fn text(&self) -> String {
        match self {
            Flag::PinsChanged => "pins-changed".to_string(),
            Flag::PinFlagged => "pin-flagged".to_string(),
            Flag::TestsLost => "tests-lost".to_string(),
            Flag::TestsAdded => "tests-added".to_string(),
            Flag::SameAddressAs(id) => format!("same-address-as {id}"),
        }
    }
}

/// The classes and flags `--fail-on` may name (ADR 0026 §4), in the order they are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FailOn {
    Weakened,
    Removed,
    Reshaped,
    NotJudged,
    PinsChanged,
    PinFlagged,
    TestsLost,
}

impl FailOn {
    fn parse(word: &str) -> Option<FailOn> {
        [
            FailOn::Weakened,
            FailOn::Removed,
            FailOn::Reshaped,
            FailOn::NotJudged,
            FailOn::PinsChanged,
            FailOn::PinFlagged,
            FailOn::TestsLost,
        ]
        .into_iter()
        .find(|candidate| candidate.as_str() == word)
    }

    fn as_str(self) -> &'static str {
        match self {
            FailOn::Weakened => "weakened",
            FailOn::Removed => "removed",
            FailOn::Reshaped => "reshaped",
            FailOn::NotJudged => "not-judged",
            FailOn::PinsChanged => "pins-changed",
            FailOn::PinFlagged => "pin-flagged",
            FailOn::TestsLost => "tests-lost",
        }
    }

    fn matches(self, entry: &Entry) -> bool {
        match self {
            FailOn::Weakened => entry.class == Class::Weakened,
            FailOn::Removed => entry.class == Class::Removed,
            FailOn::Reshaped => entry.class == Class::Reshaped,
            FailOn::NotJudged => entry.class == Class::NotJudged,
            FailOn::PinsChanged => entry.flags.contains(&Flag::PinsChanged),
            FailOn::PinFlagged => entry.flags.contains(&Flag::PinFlagged),
            FailOn::TestsLost => entry.flags.contains(&Flag::TestsLost),
        }
    }
}

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

struct Options {
    base: String,
    head: Option<String>,
    /// `None` when `--fail-on` was not given (the exit is then never 1).
    fail_on: Option<BTreeSet<FailOn>>,
    format: Format,
}

/// Parse the arguments, or print the usage line and say exit 2. Every spelling the contract does
/// not name is a usage error: a typo must never reach a process or a report.
fn parse_options(args: &[String]) -> Result<Options, i32> {
    let (format, rest) = format::take_format(args, USAGE)?;
    let usage_error = |why: &str| -> Result<Options, i32> {
        eprintln!("{USAGE}\n{why}");
        Err(2)
    };
    let (mut base, mut head, mut fail_on): (Option<String>, Option<String>, Option<String>) =
        (None, None, None);
    let mut index = 0;
    while index < rest.len() {
        let flag = rest[index].as_str();
        let slot = match flag {
            "--base" => &mut base,
            "--head" => &mut head,
            "--fail-on" => &mut fail_on,
            other => return usage_error(&format!("unrecognized argument: {other}")),
        };
        let Some(value) = rest.get(index + 1) else {
            return usage_error(&format!("`{flag}` needs a value"));
        };
        // A value beginning with `-` is never a revision or a class list: refusing it here, before
        // any process runs, is what keeps a revision from ever being read as a `git` option.
        if value.is_empty() || value.starts_with('-') {
            return usage_error(&format!("`{flag}` needs a value, not {value:?}"));
        }
        if slot.replace(value.clone()).is_some() {
            return usage_error(&format!("`{flag}` was given twice"));
        }
        index += 2;
    }
    let Some(base) = base else {
        return usage_error("`--base <rev>` is required");
    };
    let fail_on = match fail_on {
        None => None,
        Some(list) => {
            let mut classes = BTreeSet::new();
            for word in list.split(',') {
                let Some(class) = FailOn::parse(word) else {
                    let known: Vec<&str> = [
                        FailOn::Weakened,
                        FailOn::Removed,
                        FailOn::Reshaped,
                        FailOn::NotJudged,
                        FailOn::PinsChanged,
                        FailOn::PinFlagged,
                        FailOn::TestsLost,
                    ]
                    .iter()
                    .map(|c| c.as_str())
                    .collect();
                    return usage_error(&format!(
                        "`--fail-on` takes a comma-separated subset of {}, not {word:?}",
                        known.join(", ")
                    ));
                };
                classes.insert(class);
            }
            Some(classes)
        }
    };
    Ok(Options {
        base,
        head,
        fail_on,
        format,
    })
}

// ---------------------------------------------------------------------------
// git, as a process boundary
// ---------------------------------------------------------------------------

/// A `git` command with a scrubbed environment: every `GIT_*` variable the caller exported is
/// removed (a `GIT_DIR` would redirect which repository is read), user and system configuration
/// are switched off, and a missing object is never fetched. `--literal-pathspecs` and
/// `--no-replace-objects` keep a ref or a name from changing which tree is read.
fn git(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(cwd);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(&key);
        }
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["--literal-pathspecs", "--no-replace-objects"]);
    command
}

fn spawn_failure(error: &std::io::Error) -> String {
    format!("cannot run `git`: {error} — `req diff` reads revisions through the local git binary")
}

/// Run one `git` command to completion. A spawn failure or a non-zero exit is the `Err`, carrying
/// git's own message.
fn git_output(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = git(cwd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_failure(&e))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let message = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "`git {}` failed ({}): {}",
        args.join(" "),
        output.status,
        message.trim()
    ))
}

/// Where the corpus lies in its work tree: the path of the root under the top of the work tree,
/// with a trailing `/`, or empty when the root is the top.
fn work_tree_prefix(root: &Path) -> Result<String, String> {
    let not_inside = |detail: &str| {
        format!(
            "the corpus root {} is not inside a git work tree — `req diff` reads revisions of a \
             repository{detail}",
            root.display()
        )
    };
    let output = git(root)
        .args(["rev-parse", "--is-inside-work-tree", "--show-prefix"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_failure(&e))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(not_inside(&format!(" (git: {})", message.trim())));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut lines = text.lines();
    if lines.next() != Some("true") {
        return Err(not_inside(""));
    }
    Ok(lines.next().unwrap_or_default().to_string())
}

/// A revision as it was asked for and as it resolved.
struct Revision {
    label: String,
    sha: String,
}

impl Revision {
    fn short(&self) -> &str {
        self.sha.get(..12).unwrap_or(&self.sha)
    }
}

fn resolve(root: &Path, label: &str) -> Result<Revision, String> {
    // `label` never begins with `-` (a usage error before this runs), so it cannot be read as an
    // option; `^{commit}` peels a tag and refuses a tree or a blob.
    let spec = format!("{label}^{{commit}}");
    let output = git(root)
        .args(["rev-parse", "--verify", "--quiet", &spec])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_failure(&e))?;
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || sha.is_empty() {
        let message = String::from_utf8_lossy(&output.stderr);
        let detail = if message.trim().is_empty() {
            String::new()
        } else {
            format!(" (git: {})", message.trim())
        };
        return Err(format!(
            "revision {label:?} does not resolve to a commit in this repository{detail} — a \
             shallow checkout needs the base fetched (`fetch-depth: 0`)"
        ));
    }
    Ok(Revision {
        label: label.to_string(),
        sha,
    })
}

/// One blob to materialise: its path under the corpus root and the object that holds it.
struct TreeEntry {
    path: String,
    oid: String,
}

/// List `claims/` and `aliases/` of a revision's corpus tree. Only regular files are accepted: a
/// symlink would let a revision point the loader at any file this process can read, and a
/// submodule is not a claim.
fn list_corpus(root: &Path, revision: &Revision, prefix: &str) -> Result<Vec<TreeEntry>, String> {
    let tree = format!("{}:{}", revision.sha, prefix.trim_end_matches('/'));
    // `<sha>:` (an empty path) is the root tree.
    let bytes = git_output(
        root,
        &[
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            &tree,
            "--",
            "claims",
            "aliases",
        ],
    )
    .map_err(|first| {
        // A corpus directory that does not exist at this revision is "no corpus here", not a
        // git failure to be quoted; anything else is git's own message.
        if git_output(root, &["cat-file", "-e", &tree]).is_err() {
            format!(
                "{}: no corpus at {:?} in this revision (no such directory)",
                revision.label,
                prefix.trim_end_matches('/')
            )
        } else {
            first
        }
    })?;
    let mut entries = Vec::new();
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let Some((meta, path)) = record.split_once('\t') else {
            return Err(format!("unreadable `git ls-tree` record: {record:?}"));
        };
        let fields: Vec<&str> = meta.split(' ').collect();
        let [mode, kind, oid] = fields.as_slice() else {
            return Err(format!("unreadable `git ls-tree` record: {record:?}"));
        };
        if !(path.starts_with("claims/") || path.starts_with("aliases/")) {
            return Err(format!(
                "{}: {path:?} is not a directory — `claims/` and `aliases/` are directories",
                revision.label
            ));
        }
        if *kind != "blob" || !matches!(*mode, "100644" | "100755") {
            return Err(format!(
                "{}: {path:?} is a {kind} (mode {mode}), not a regular file — a symlink or a \
                 submodule is not a claim or a registry",
                revision.label
            ));
        }
        if path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(format!(
                "{}: {path:?} is not a path inside the corpus",
                revision.label
            ));
        }
        entries.push(TreeEntry {
            path: path.to_string(),
            oid: (*oid).to_string(),
        });
    }
    Ok(entries)
}

/// The raw bytes of each object, in order, through one `git cat-file --batch`. No filter runs on
/// a blob read this way, so a line-ending setting or an attribute cannot change a claim's bytes.
///
/// The request, the answer and git's complaint travel through files in `scratch` (inside the
/// staging directory) rather than pipes: a duplex pipe to a child needs a second thread to avoid a
/// deadlock, and this crate runs none (REQ-0015). `git` runs to completion, then its output is read.
fn read_blobs(root: &Path, scratch: &Path, oids: &[&str]) -> Result<Vec<Vec<u8>>, String> {
    if oids.is_empty() {
        return Ok(Vec::new());
    }
    let (request, answer, complaint) = (
        scratch.join("cat-file.request"),
        scratch.join("cat-file.answer"),
        scratch.join("cat-file.stderr"),
    );
    let io = |what: &str, e: std::io::Error| format!("staging {what}: {e}");
    std::fs::create_dir_all(scratch).map_err(|e| io("scratch", e))?;
    let lines: String = oids.iter().map(|oid| format!("{oid}\n")).collect();
    std::fs::write(&request, lines).map_err(|e| io("a request", e))?;
    let status = git(root)
        .args(["cat-file", "--batch"])
        .stdin(File::open(&request).map_err(|e| io("a request", e))?)
        .stdout(File::create(&answer).map_err(|e| io("an answer", e))?)
        .stderr(File::create(&complaint).map_err(|e| io("a log", e))?)
        .status()
        .map_err(|e| spawn_failure(&e))?;
    if !status.success() {
        let message = std::fs::read_to_string(&complaint).unwrap_or_default();
        return Err(format!(
            "`git cat-file --batch` failed ({status}): {}",
            message.trim()
        ));
    }
    let bytes = std::fs::read(&answer).map_err(|e| io("an answer", e))?;
    parse_batch(&bytes, oids)
}

fn parse_batch(bytes: &[u8], oids: &[&str]) -> Result<Vec<Vec<u8>>, String> {
    let mut reader = std::io::Cursor::new(bytes);
    let mut blobs = Vec::with_capacity(oids.len());
    for oid in oids {
        let mut header = Vec::new();
        reader
            .read_until(b'\n', &mut header)
            .map_err(|e| format!("reading `git cat-file` output: {e}"))?;
        let header = String::from_utf8_lossy(&header).trim_end().to_string();
        let fields: Vec<&str> = header.split(' ').collect();
        let size = match fields.as_slice() {
            [_, "blob", size] => size.parse::<usize>().ok(),
            [_, "missing"] => {
                return Err(format!(
                    "object {oid} is not in this repository — a shallow or partial clone needs \
                     the base fetched (`fetch-depth: 0`); `req diff` never fetches"
                ));
            }
            _ => None,
        };
        let Some(size) = size else {
            return Err(format!(
                "unreadable `git cat-file` header for {oid}: {header:?}"
            ));
        };
        let mut content = vec![0_u8; size];
        reader
            .read_exact(&mut content)
            .map_err(|e| format!("reading object {oid}: {e}"))?;
        let mut newline = [0_u8; 1];
        reader
            .read_exact(&mut newline)
            .map_err(|e| format!("reading object {oid}: {e}"))?;
        blobs.push(content);
    }
    Ok(blobs)
}

/// Write a revision's `claims/` and `aliases/` under `dest`, and return how many entries were
/// written.
fn materialise(
    root: &Path,
    revision: &Revision,
    prefix: &str,
    scratch: &Path,
    dest: &Path,
) -> Result<usize, String> {
    let entries = list_corpus(root, revision, prefix)?;
    let oids: Vec<&str> = entries.iter().map(|e| e.oid.as_str()).collect();
    let blobs = read_blobs(root, scratch, &oids)?;
    for (entry, content) in entries.iter().zip(blobs) {
        let target = dest.join(&entry.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("staging {}: {e}", parent.display()))?;
        }
        std::fs::write(&target, content).map_err(|e| format!("staging {}: {e}", entry.path))?;
    }
    Ok(entries.len())
}

// ---------------------------------------------------------------------------
// The temporary directory, and what removes it
// ---------------------------------------------------------------------------

/// The staging directory and what removes it on every way out.
///
/// `Drop` covers every return and a panic. A signal (or SIGKILL) skips it, so a **dead-man's
/// switch** covers the rest: a tiny `sh` helper blocked reading a pipe whose only write end this
/// process holds. However the process ends, the kernel closes that end, the helper reads EOF and
/// removes the directory. No signal handler, no thread, no polling loop and no `unsafe` — the crate
/// has none of the first three by contract (REQ-0012, REQ-0015). Best effort by construction: where
/// there is no `/bin/sh` or `rm` the helper does not start and `Drop` is the whole mechanism.
struct Staging {
    /// Dropped first: the directory goes while the helper is still waiting.
    dir: crate::baseline::StagingDir,
    /// Dropped second: closing it releases the helper, which removes whatever is left.
    _switch: Option<std::process::ChildStdin>,
}

impl Staging {
    fn new() -> Result<Staging, String> {
        let dir = crate::baseline::staging_dir().map_err(|e| {
            format!("cannot create a temporary directory to read revisions into: {e}")
        })?;
        let switch = Command::new("/bin/sh")
            .args([
                "-c",
                "IFS= read -r _line; rm -rf -- \"$1\"",
                "req-diff-cleanup",
            ])
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .and_then(|mut helper| helper.stdin.take());
        Ok(Staging {
            dir,
            _switch: switch,
        })
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// One side of the comparison, loaded.
struct Side {
    claims: Vec<LoadedClaim>,
    /// `aliases/subjects.yml` as bytes (`None` when absent), compared for `registry: changed`.
    registry: Option<Vec<u8>>,
}

impl Side {
    /// `display` is the revision a staged `dir` stands for: the loader names paths under `dir`,
    /// and the reader needs the revision, not a temporary path. `None` for a real directory.
    fn load(dir: &Path, display: Option<&str>) -> Result<Side, String> {
        let relabel = |message: String| match display {
            Some(display) => message
                .replace(&format!("{}/", dir.display()), display)
                .replace(&dir.display().to_string(), display.trim_end_matches(':')),
            None => message,
        };
        let claims = canon::load_claims(dir).map_err(relabel)?;
        let registry = std::fs::read(dir.join("aliases/subjects.yml")).ok();
        Ok(Side { claims, registry })
    }

    fn by_id(&self) -> BTreeMap<&str, &LoadedClaim> {
        self.claims
            .iter()
            .map(|loaded| (loaded.claim.claim_id.as_str(), loaded))
            .collect()
    }
}

/// One listed claim.
struct Entry {
    claim_id: String,
    class: Class,
    flags: Vec<Flag>,
    base_address: Option<String>,
    head_address: Option<String>,
    /// The direction word, for a changed meaning only.
    direction: Option<Direction>,
}

/// What the whole comparison found.
struct Report {
    entries: Vec<Entry>,
    counts: BTreeMap<Class, usize>,
    registry_changed: bool,
}

fn flagged(loaded: &LoadedClaim) -> bool {
    loaded.derived.flagged_pins().next().is_some()
}

fn pin_entries(loaded: &LoadedClaim) -> Vec<(&'static str, &str)> {
    loaded
        .claim
        .pins
        .as_ref()
        .map(crate::model::Pins::entries)
        .unwrap_or_default()
}

fn test_ids(loaded: &LoadedClaim) -> BTreeSet<&str> {
    loaded.claim.tests.iter().map(String::as_str).collect()
}

/// Whether anything outside identity differs: relations, rationale, tests and evidence (compared
/// by the digests the Baseline attests, which already treat the link list as order-free), and the
/// pins (compared structurally, never by file bytes).
fn metadata_differs(base: &LoadedClaim, head: &LoadedClaim) -> bool {
    use crate::model::{rationale_digest, relations_digest, tests_digest};
    let (b, h) = (&base.claim, &head.claim);
    relations_digest(&b.relations) != relations_digest(&h.relations)
        || rationale_digest(&b.rationale) != rationale_digest(&h.rationale)
        || tests_digest(&b.tests, b.evidence) != tests_digest(&h.tests, h.evidence)
        || pin_entries(base) != pin_entries(head)
}

/// Classify one claim id present on both sides.
fn classify_pair(base: &LoadedClaim, head: &LoadedClaim) -> Entry {
    let (b, h) = (&base.claim, &head.claim);
    let (base_address, head_address) = (&base.derived.address, &head.derived.address);
    let statement_edited = b.statement != h.statement;
    // Applicability is an *input* to the address (ADR 0003), so changing it is a change of
    // meaning even when the statement is byte-identical; its order is not.
    let applicability_edited = b.applicability.iter().collect::<BTreeSet<_>>()
        != h.applicability.iter().collect::<BTreeSet<_>>();

    let (class, direction) = if statement_edited || applicability_edited {
        if base_address == head_address {
            // The text changed and the meaning fingerprint did not.
            (Class::Reworded, None)
        } else {
            let direction = compare::compare(&base.derived.form, &head.derived.form);
            (Class::of_direction(direction), Some(direction))
        }
    } else if metadata_differs(base, head) || (flagged(head) && !flagged(base)) {
        // Same statement and applicability: the claim's own text did not move, so a different
        // address here is the registry's doing and is reported once, at the top. A pin that
        // *newly* disagrees with the head parse (a registry edit can do it with the claim file
        // byte-identical) is a change a `--fail-on pin-flagged` gate must see; a disagreement
        // present on both sides is not a change and stays unlisted.
        (Class::Metadata, None)
    } else {
        (Class::Unchanged, None)
    };

    let mut flags = Vec::new();
    if pin_entries(base) != pin_entries(head) {
        flags.push(Flag::PinsChanged);
    }
    if flagged(head) {
        flags.push(Flag::PinFlagged);
    }
    let (before, after) = (test_ids(base), test_ids(head));
    if before.difference(&after).next().is_some() {
        flags.push(Flag::TestsLost);
    }
    if after.difference(&before).next().is_some() {
        flags.push(Flag::TestsAdded);
    }
    Entry {
        claim_id: b.claim_id.clone(),
        class,
        flags,
        base_address: Some(base_address.clone()),
        head_address: Some(head_address.clone()),
        direction,
    }
}

fn compare_sides(base: &Side, head: &Side) -> Report {
    let (base_by_id, head_by_id) = (base.by_id(), head.by_id());
    let ids: BTreeSet<&str> = base_by_id
        .keys()
        .chain(head_by_id.keys())
        .copied()
        .collect();

    let mut entries = Vec::new();
    let mut counts: BTreeMap<Class, usize> = Class::ALL.iter().map(|c| (*c, 0)).collect();
    for id in ids {
        let entry = match (base_by_id.get(id), head_by_id.get(id)) {
            (None, Some(added)) => Entry {
                claim_id: id.to_string(),
                class: Class::Added,
                flags: if flagged(added) {
                    vec![Flag::PinFlagged]
                } else {
                    Vec::new()
                },
                base_address: None,
                head_address: Some(added.derived.address.clone()),
                direction: None,
            },
            (Some(removed), None) => Entry {
                claim_id: id.to_string(),
                class: Class::Removed,
                flags: Vec::new(),
                base_address: Some(removed.derived.address.clone()),
                head_address: None,
                direction: None,
            },
            (Some(before), Some(after)) => classify_pair(before, after),
            (None, None) => continue,
        };
        *counts.entry(entry.class).or_insert(0) += 1;
        if entry.class != Class::Unchanged {
            entries.push(entry);
        }
    }

    // A candidate renumber: an added claim whose address a removed claim had. Reported on the
    // added claim; both stay `added` / `removed`, because the id is the identity (ADR 0001).
    let removed: Vec<(String, String)> = entries
        .iter()
        .filter(|e| e.class == Class::Removed)
        .filter_map(|e| Some((e.claim_id.clone(), e.base_address.clone()?)))
        .collect();
    for entry in entries.iter_mut().filter(|e| e.class == Class::Added) {
        for (removed_id, address) in &removed {
            if entry.head_address.as_ref() == Some(address) {
                entry.flags.push(Flag::SameAddressAs(removed_id.clone()));
            }
        }
    }
    for entry in &mut entries {
        entry.flags.sort();
    }

    Report {
        entries,
        counts,
        registry_changed: base.registry != head.registry,
    }
}

/// The `--fail-on` words that matched at least one claim, and how many claims matched.
fn triggered(report: &Report, fail_on: &BTreeSet<FailOn>) -> (Vec<FailOn>, usize) {
    let matched: Vec<FailOn> = fail_on
        .iter()
        .copied()
        .filter(|word| report.entries.iter().any(|e| word.matches(e)))
        .collect();
    let claims = report
        .entries
        .iter()
        .filter(|e| fail_on.iter().any(|word| word.matches(e)))
        .count();
    (matched, claims)
}

fn label(revision: &Revision) -> String {
    format!("{} ({})", revision.label, revision.short())
}

fn render_text(
    base: &Revision,
    head: Option<&Revision>,
    report: &Report,
    fail_on: Option<&BTreeSet<FailOn>>,
) -> String {
    let head_label = head.map_or_else(|| WORKING_TREE.to_string(), label);
    let mut out = format!("# req diff: {} -> {head_label}\n", label(base));
    out.push_str(if report.registry_changed {
        "registry: changed\n"
    } else {
        "registry: unchanged\n"
    });
    for entry in &report.entries {
        out.push_str(entry.class.as_str());
        out.push(' ');
        out.push_str(&entry.claim_id);
        if let Some(direction) = entry.direction {
            out.push(' ');
            out.push_str(direction.as_str());
        }
        for flag in &entry.flags {
            out.push(' ');
            out.push_str(&flag.text());
        }
        out.push('\n');
    }
    let counts: Vec<String> = Class::ALL
        .iter()
        .map(|class| {
            format!(
                "{} {}",
                class.as_str(),
                report.counts.get(class).copied().unwrap_or(0)
            )
        })
        .collect();
    out.push_str(&format!("counts: {}\n", counts.join(", ")));
    if let Some(fail_on) = fail_on {
        let words: Vec<&str> = fail_on.iter().map(|w| w.as_str()).collect();
        let (matched, claims) = triggered(report, fail_on);
        if matched.is_empty() {
            out.push_str(&format!("fail-on: {} — none triggered\n", words.join(",")));
        } else {
            out.push_str(&format!(
                "fail-on: {} — {claims} triggered\n",
                words.join(",")
            ));
        }
    }
    out
}

fn render_json(
    base: &Revision,
    head: Option<&Revision>,
    report: &Report,
    fail_on: Option<&BTreeSet<FailOn>>,
) -> String {
    let side = |revision: Option<&Revision>| match revision {
        Some(r) => serde_json::json!({ "label": r.label, "sha": r.sha }),
        None => serde_json::json!({ "label": WORKING_TREE, "sha": serde_json::Value::Null }),
    };
    let empty = BTreeSet::new();
    let fail_on = fail_on.unwrap_or(&empty);
    let words: Vec<&str> = fail_on.iter().map(|w| w.as_str()).collect();
    let (matched, _) = triggered(report, fail_on);
    let matched: Vec<&str> = matched.iter().map(|w| w.as_str()).collect();
    let counts: BTreeMap<&str, usize> = Class::ALL
        .iter()
        .map(|class| {
            (
                class.as_str(),
                report.counts.get(class).copied().unwrap_or(0),
            )
        })
        .collect();
    let claims: Vec<serde_json::Value> = report
        .entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "claim_id": e.claim_id,
                "class": e.class.as_str(),
                "flags": e.flags.iter().map(Flag::text).collect::<Vec<_>>(),
                "base_address": e.base_address,
                "head_address": e.head_address,
                "direction": e.direction.map(|d| d.as_str()),
            })
        })
        .collect();
    // `serde_json`'s object is a sorted map, so the keys come out sorted whatever order they are
    // written in; the numbers are integers and no float appears.
    let document = serde_json::json!({
        "schema": SCHEMA_DIFF,
        "generator": "diff",
        "rules_v": canon::RULES_V,
        "base": side(Some(base)),
        "head": side(head),
        "registry_changed": report.registry_changed,
        "fail_on": words,
        "triggered": matched,
        "counts": counts,
        "claims": claims,
    });
    format!("{}\n", format::document(&document))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `req diff` — see the module doc and `DIFF_HELP` in `main.rs`.
pub fn run(args: &[String], root: &Path) -> i32 {
    let options = match parse_options(args) {
        Ok(options) => options,
        Err(code) => return code,
    };
    match produce(&options, root) {
        Ok(output) => {
            let (text, exit) = output;
            print!("{text}");
            exit
        }
        Err(cause) => {
            eprintln!("[UNVERIFIABLE] {cause}");
            3
        }
    }
}

/// The whole report as one string and the exit it earns, or the reason it cannot be produced.
/// Nothing is printed from here: a report that fails half-way must leave stdout empty.
fn produce(options: &Options, root: &Path) -> Result<(String, i32), String> {
    let prefix = work_tree_prefix(root)?;
    let base = resolve(root, &options.base)?;
    let head = options
        .head
        .as_deref()
        .map(|label| resolve(root, label))
        .transpose()?;

    let staging = Staging::new()?;

    let load_revision = |revision: &Revision, slot: &str| -> Result<Side, String> {
        let dir = staging.path().join(slot);
        let written = materialise(
            root,
            revision,
            &prefix,
            &staging.path().join("scratch"),
            &dir,
        )?;
        let display = format!("{}:{prefix}", revision.label);
        if written == 0 {
            return Err(format!(
                "{}: no `claims/` or `aliases/` under {:?} — an empty corpus is indistinguishable \
                 from one whose claims all vanished",
                revision.label,
                prefix.trim_end_matches('/')
            ));
        }
        Side::load(&dir, Some(&display))
    };
    let base_side = load_revision(&base, "base")?;
    let head_side = match &head {
        Some(revision) => load_revision(revision, "head")?,
        None => Side::load(root, None)?,
    };

    let report = compare_sides(&base_side, &head_side);
    let text = match options.format {
        Format::Text => render_text(&base, head.as_ref(), &report, options.fail_on.as_ref()),
        Format::Json => render_json(&base, head.as_ref(), &report, options.fail_on.as_ref()),
    };
    let exit = match &options.fail_on {
        Some(fail_on) if !triggered(&report, fail_on).0.is_empty() => 1,
        _ => 0,
    };
    Ok((text, exit))
}
