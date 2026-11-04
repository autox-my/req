//! `req baseline --at <ts> --out <file>`: emit the manifest to a file, atomically and only on success.
//!
//! The failure this exists for: `req baseline … > baselines/x.json` truncates the target BEFORE `req`
//! runs, so any refusal (an old binary, an absent decision log, a usage error) leaves a 0-byte
//! manifest, and `minisign -Sm` will happily sign it. With `--out` the file is written only after the
//! whole manifest exists, via a temp file and a rename, so a refusal leaves whatever was there.
//! Expectations come from the plain emission (stdout) and from the files' bytes, not from req's own
//! description of what it wrote.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "req-baseline-out-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
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
    code: i32,
    stdout: Vec<u8>,
    stderr: String,
}

fn run(root: &Path, args: &[&str]) -> Run {
    let output = Command::new(req_bin())
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

const AT: &str = "2026-11-04T05:00:00Z";

#[test]
fn out_writes_exactly_the_bytes_the_plain_emission_prints() {
    let plain = run(
        repo(),
        &["baseline", "--at", AT, "deployment_mode=air_gapped"],
    );
    assert_eq!(plain.code, 0, "{}", plain.stderr);
    assert!(!plain.stdout.is_empty());

    let scratch = Scratch::new("equal");
    let target = scratch.0.join("manifest.json");
    let target_str = target.to_str().expect("utf-8 path");
    let written = run(
        repo(),
        &[
            "baseline",
            "--at",
            AT,
            "--out",
            target_str,
            "deployment_mode=air_gapped",
        ],
    );
    assert_eq!(written.code, 0, "{}", written.stderr);
    assert!(
        written.stdout.is_empty(),
        "--out prints no manifest on stdout"
    );
    assert_eq!(fs::read(&target).expect("the file exists"), plain.stdout);
    let leftovers: Vec<_> = fs::read_dir(&scratch.0)
        .expect("lists the dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "no temp file left behind: {leftovers:?}"
    );
}

#[test]
fn a_refused_emission_leaves_an_existing_file_untouched_and_creates_none() {
    // A starter corpus has no decision log, so emission is refused (exit 3).
    let corpus = Scratch::new("corpus");
    let init = Command::new(req_bin())
        .arg("init")
        .current_dir(&corpus.0)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req init");
    assert_eq!(init.status.code(), Some(0));

    let out_dir = Scratch::new("refused");
    let existing = out_dir.0.join("signed-before.json");
    fs::write(&existing, b"PREVIOUS MANIFEST BYTES").expect("seeds a file");
    let absent = out_dir.0.join("never-created.json");

    for target in [&existing, &absent] {
        let done = run(
            &corpus.0,
            &[
                "baseline",
                "--at",
                AT,
                "--out",
                target.to_str().expect("utf-8"),
            ],
        );
        assert_eq!(
            done.code, 3,
            "a refused emission is UNVERIFIABLE: {}",
            done.stderr
        );
    }
    assert_eq!(
        fs::read(&existing).expect("still there"),
        b"PREVIOUS MANIFEST BYTES",
        "a refusal must not truncate or replace the existing manifest"
    );
    assert!(!absent.exists(), "a refusal must not create the file");
}

#[test]
fn usage_errors_leave_the_target_alone() {
    let scratch = Scratch::new("usage");
    let target = scratch.0.join("kept.json");
    fs::write(&target, b"KEEP").expect("seeds a file");
    let target_str = target.to_str().expect("utf-8");

    // --out needs a value.
    let no_value = run(repo(), &["baseline", "--at", AT, "--out"]);
    assert_eq!(no_value.code, 2, "{}", no_value.stderr);
    // --out is an emission flag: it does not mix with --verify.
    let mixed = run(
        repo(),
        &[
            "baseline",
            "--verify",
            "baselines/air-gapped.json",
            "--out",
            target_str,
        ],
    );
    assert_eq!(mixed.code, 2, "{}", mixed.stderr);
    // An option-shaped value is not a path.
    let shaped = run(repo(), &["baseline", "--at", AT, "--out", "--verify"]);
    assert_eq!(shaped.code, 2, "{}", shaped.stderr);
    // A missing --at is still refused, and still touches nothing.
    let no_at = run(
        repo(),
        &[
            "baseline",
            "--out",
            target_str,
            "deployment_mode=air_gapped",
        ],
    );
    assert_eq!(no_at.code, 2, "{}", no_at.stderr);
    assert_eq!(fs::read(&target).expect("still there"), b"KEEP");
}

#[test]
fn an_unwritable_target_is_unverifiable_not_a_silent_success() {
    let scratch = Scratch::new("unwritable");
    let target = scratch.0.join("no-such-dir").join("m.json");
    let done = run(
        repo(),
        &[
            "baseline",
            "--at",
            AT,
            "--out",
            target.to_str().expect("utf-8"),
            "deployment_mode=air_gapped",
        ],
    );
    assert_eq!(done.code, 3, "{}", done.stderr);
    assert!(!target.exists());
}
