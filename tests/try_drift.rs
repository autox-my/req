//! `tools/demo/try_drift.py` shows what drift does, on a throw-away copy, and never touches the checkout it is run from
//! (unit P-1; ADR 0040, ADR 0052).
//!
//! The script copies what `req satisfy` reads to a temporary directory, applies the tightening edit from
//! `docs/architecture/drift/edits.json` there, runs `req satisfy` there, prints its lines and deletes the copy. The README
//! sends a newcomer to it as the third of three commands, so what it must keep true is tested here: it prints the SUSPECT line
//! for `src/bench.rs -> REQ-0010`; it leaves its own checkout byte-identical; it leaves no copy behind; and a run that did
//! not show the demonstration says so by its exit code.
//!
//! Exit contract (the script's docstring is the one home): 0 the demonstration ran and `req satisfy` gave the outcome the edit
//! expects (SUSPECT, so the exit is about the demonstration, not about req's verdict, which is 3 for any suspect link); 1 it
//! ran and `req satisfy` gave a different outcome for the link (the demonstration did not show what it claims to); 2 usage;
//! 3 UNVERIFIABLE, nothing was shown: `req` cannot be run, the edit does not apply, or the copy cannot be made.
//!
//! The oracles are written here, from the committed `edits.json` and ADR 0040, never taken from the script. The script is run
//! from a scratch copy of the repository, because it finds its corpus from its own location: a script that edited "the real
//! claim file" would edit the copy, and the byte comparison below would catch it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const SCRIPT: &str = "tools/demo/try_drift.py";
/// What a checkout needs for the script to run in it: the corpus and design files `req satisfy` reads, the edit, the script.
const TREES: [&str; 6] = ["claims", "aliases", ".req", "src", "decisions", "baselines"];
const EDITS: &str = "docs/architecture/drift/edits.json";
/// The line `req satisfy` prints for the tightened link; its prefix is written here from ADR 0040 decision 3.
const SUSPECT_LINE: &str = "[SUSPECT] src/bench.rs -> REQ-0010: numeric_tightening";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

/// `python3` as an absolute path, resolved before the child's environment is rewritten.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/demo/try_drift.py - install it first");
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

/// Every file under `dir` (relative path -> bytes): two snapshots are equal only when nothing was added, removed or edited.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let entry = entry.expect("a dir entry");
            if entry.file_type().expect("a file type").is_dir() {
                walk(base, &entry.path(), into);
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(base)
                    .expect("under the base")
                    .to_string_lossy()
                    .into_owned();
                into.insert(relative, fs::read(entry.path()).expect("reads a file"));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

/// A scratch checkout (the trees the script reads, the edits and the script) and a scratch `TMPDIR` the script must leave empty.
struct Checkout {
    base: PathBuf,
    repo: PathBuf,
    tmp: PathBuf,
}

impl Checkout {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "req-try-drift-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let tmp = base.join("tmp");
        fs::create_dir_all(&tmp).expect("creates the scratch TMPDIR");
        for tree in TREES {
            copy_tree(&root().join(tree), &repo.join(tree));
        }
        for file in [EDITS, SCRIPT] {
            let to = repo.join(file);
            fs::create_dir_all(to.parent().expect("a parent")).expect("makes the dir");
            fs::copy(root().join(file), &to)
                .unwrap_or_else(|e| panic!("{file} must exist (the script is new in P-1): {e}"));
        }
        Checkout { base, repo, tmp }
    }

    fn run(&self, args: &[&str]) -> Run {
        let done = Command::new(python3())
            .arg(self.repo.join(SCRIPT))
            .args(args)
            .current_dir(&self.repo)
            .env("TMPDIR", &self.tmp)
            .env_remove("REQ_ROOT")
            .output()
            .expect("python3 runs");
        Run {
            code: done.status.code(),
            out: String::from_utf8_lossy(&done.stdout).into_owned(),
            err: String::from_utf8_lossy(&done.stderr).into_owned(),
        }
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

struct Run {
    code: Option<i32>,
    out: String,
    err: String,
}

/// A stand-in for `req` that prints one fixed `req satisfy` answer, so the script's reading of an outcome it did not expect
/// is tested without changing a claim.
fn stub_req(dir: &Path, printed: &str, code: i32) -> PathBuf {
    let path = dir.join("stub-req");
    fs::write(
        &path,
        format!("#!/bin/sh\nprintf '%s\\n' '{printed}'\nexit {code}\n"),
    )
    .expect("writes a stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("makes it executable");
    }
    path
}

/// The demonstration prints req's own SUSPECT line for the tightened link and exits 0: the exit says the demonstration ran
/// and showed what it claims, not that req's verdict was clean (req satisfy itself exits 3 for a suspect link).
#[test]
fn it_prints_the_suspect_line_for_the_tightened_link_and_exits_0() {
    let checkout = Checkout::new("suspect");
    let run = checkout.run(&["--req", req_bin().to_str().expect("a utf-8 path")]);
    assert_eq!(
        run.code,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        run.out,
        run.err
    );
    assert!(
        run.out.lines().any(|l| l.starts_with(SUSPECT_LINE)),
        "the SUSPECT line for src/bench.rs -> REQ-0010 must be printed:\n{}",
        run.out
    );
    // The rest of the corpus still holds: the demonstration is one requirement, one part.
    assert!(
        !run.out
            .lines()
            .any(|l| l.starts_with("[SUSPECT]") && !l.starts_with(SUSPECT_LINE)),
        "only the tightened link is suspect:\n{}",
        run.out
    );
}

/// The checkout the script is run from is byte-identical afterwards (no claim edited, nothing added), and the copy it made is
/// gone from `TMPDIR`. The README promises both ("it never edits this checkout").
#[test]
fn it_leaves_the_checkout_untouched_and_deletes_its_copy() {
    let checkout = Checkout::new("untouched");
    let before = snapshot(&checkout.repo);
    let run = checkout.run(&["--req", req_bin().to_str().expect("a utf-8 path")]);
    assert_eq!(
        run.code,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        run.out,
        run.err
    );
    let after = snapshot(&checkout.repo);
    let changed: Vec<&String> = after
        .iter()
        .filter(|(path, bytes)| before.get(*path) != Some(*bytes))
        .map(|(path, _)| path)
        .chain(before.keys().filter(|path| !after.contains_key(*path)))
        .collect();
    assert!(
        changed.is_empty(),
        "the script changed its own checkout: {changed:?}"
    );
    let left: Vec<_> = fs::read_dir(&checkout.tmp)
        .expect("reads the scratch TMPDIR")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert!(left.is_empty(), "the copy was not deleted: {left:?}");
}

/// A `req` that cannot be run shows nothing: exit 3 (UNVERIFIABLE), no SUSPECT line, nothing left behind.
#[test]
fn an_unrunnable_req_is_unverifiable_exit_3() {
    let checkout = Checkout::new("unrunnable");
    let missing = checkout.base.join("no-such-req");
    let run = checkout.run(&["--req", missing.to_str().expect("a utf-8 path")]);
    assert_eq!(
        run.code,
        Some(3),
        "stdout:\n{}\nstderr:\n{}",
        run.out,
        run.err
    );
    assert!(
        !run.out.contains("SUSPECT"),
        "nothing was shown:\n{}",
        run.out
    );
    assert!(!run.err.is_empty(), "the reason is said on stderr");
    let left = fs::read_dir(&checkout.tmp).expect("reads TMPDIR").count();
    assert_eq!(left, 0, "no copy is left behind");
}

/// A `req` that answers something other than the outcome the edit expects did not show the demonstration: exit 1, and the
/// script does not pretend the link is suspect.
#[test]
fn an_unexpected_outcome_is_exit_1() {
    let checkout = Checkout::new("unexpected");
    let stub = stub_req(&checkout.base, "[HOLDS] src/bench.rs -> REQ-0010", 0);
    let run = checkout.run(&["--req", stub.to_str().expect("a utf-8 path")]);
    assert_eq!(
        run.code,
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        run.out,
        run.err
    );
    assert!(
        !run.out.lines().any(|l| l.starts_with("[SUSPECT]")),
        "no SUSPECT line was printed:\n{}",
        run.out
    );
}

/// An argument it does not know is a usage error (2), not a run.
#[test]
fn an_unknown_argument_is_usage_exit_2() {
    let checkout = Checkout::new("usage");
    let run = checkout.run(&["--no-such-flag"]);
    assert_eq!(
        run.code,
        Some(2),
        "stdout:\n{}\nstderr:\n{}",
        run.out,
        run.err
    );
}
