//! `scripts/build-release.sh` — the build-and-bundle half of a prebuilt, checksummed binary per
//! pinned rev (ADR 0025, unit 65-1 of `docs/specs/65-prebuilt-binary.md`).
//!
//! The public seam is the script run as a process (`bash scripts/build-release.sh --rev … --out …`),
//! exactly what the owner types, against a throwaway git repository whose one commit holds a tiny
//! corpus and a stub `req` (a shell script whose answers the test dials through environment
//! variables). The stub makes the packaging logic testable without a compile; every check the
//! script applies to a `--bin` it applies to a built binary alike, which is why the flag is safe.
//!
//! The oracles are independent of the script: `tar -tzf` and `tar -xzf` for the archive, the `sha2`
//! crate for the digest (the script uses `shasum`/`sha256sum`), and hand-written expected file
//! names, messages and exit codes from the ADR and the spec. Nothing is compared with the script's
//! own output except that the script must print what the files hold.
//!
//! The `PATH` the script sees is a directory of symlinks to the few tools it needs plus shims for
//! `rustc`, `cargo` and `minisign`, so a test never depends on the host's toolchain, on its target
//! triple, or on `minisign` being installed — and case (e) can take `minisign` away.
//!
//! Exit contract under test: 0 released; 1 a check failed or an output file already exists; 2 usage,
//! an unresolvable rev or an unsupported host; 3 UNVERIFIABLE (no `minisign`). In every non-zero case
//! **nothing is written to `--out`** (it is not even created) and no temporary directory is left.
#![cfg(unix)]

use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// The target the `rustc` shim reports. One of the two the ADR supports.
const TARGET: &str = "x86_64-unknown-linux-gnu";

/// The committed text of the Baseline file in the fixture repository. A run that sees anything
/// else is not looking at the commit it was asked to release.
const COMMITTED_BASELINE: &str = "COMMITTED\n";

/// The stub `req`. Configured by the environment of each run: `STUB_BUILT_FROM` (the value after
/// `built from`), `STUB_VERIFY` (what `baseline --verify` does), `STUB_BENCH` (what `bench` does),
/// `STUB_SELFTEST` (what `selftest` does),
/// `STUB_LOG` (a file it appends its working directory to). It insists on being invoked the way the
/// ADR says (`baseline --verify baselines/air-gapped.json`, `bench` with no argument) and on being
/// run in the clean copy: the committed Baseline text present, no stray file from the caller's tree.
const STUB_REQ: &str = r##"#!/bin/sh
log() { [ -z "${STUB_LOG:-}" ] || printf '%s\n' "$*" >> "$STUB_LOG"; }
emit_bench() {
  printf 'E-001    equivalent      collapsed=true  structured=true\nN-001    not_equivalent  collapsed=false\n'
  printf '{\n  "corpus": "bench/corpus/parity.yml",\n  "corpus_sha256": "4a1727af8631325b190fda8921390b7c559215bb0edc9f00079edf1c2f0c9889",\n  "degraded": [],\n  "direction": {\n    "accuracy": null,\n    "hard_gate": "%s"\n  },\n  "fmr": {\n    "collapses": 0,\n    "hard_gate": "%s",\n    "rate": 0.0\n  }\n}\n' "$1" "$2"
}
tree_is_the_clean_copy() {
  [ -f baselines/air-gapped.json ] && [ "$(cat baselines/air-gapped.json)" = COMMITTED ] && [ ! -e stray.txt ]
}
case "${1:-}" in
  --version)
    printf 'req 0.1.0\n  rules_v            6\n  baseline_envelope  4\n  decision_chain     1\n  claim_schema       req.claim/2\n  built from         %s\n' "${STUB_BUILT_FROM:-unknown}"
    exit 0
    ;;
  baseline)
    log "baseline req_root=${REQ_ROOT:-} cwd=$(pwd -P)"
    if [ "${2:-}" != "--verify" ] || [ "${3:-}" != "baselines/air-gapped.json" ] || [ $# -ne 3 ]; then
      echo "stub: wrong baseline invocation: $*" >&2; exit 64
    fi
    if ! tree_is_the_clean_copy; then
      echo "[FAIL] stub saw a tree that is not the committed one"; exit 1
    fi
    case "${STUB_VERIFY:-ok}" in
      ok) echo "# Baseline baselines/air-gapped.json: 18 claim(s)"; echo "[VERIFIED]"; exit 0 ;;
      fail) echo "[FAIL]"; exit 1 ;;
      unverifiable) echo "[UNVERIFIABLE]"; exit 3 ;;
      verified-but-exit1) echo "[VERIFIED]"; exit 1 ;;
      no-brackets) echo "VERIFIED"; exit 0 ;;
      silent) exit 0 ;;
    esac
    ;;
  selftest)
    log "selftest ran"
    if [ $# -ne 1 ]; then echo "stub: wrong selftest invocation: $*" >&2; exit 64; fi
    case "${STUB_SELFTEST:-pass}" in
      pass) echo "PASS digest: sha256(\"abc\")"; echo "selftest: PASS (26 checks)"; exit 0 ;;
      fail) echo "FAIL address: x"; echo "selftest: FAIL (1 of 26 checks failed)"; exit 1 ;;
      fail-exit0) echo "selftest: FAIL (1 of 26 checks failed)"; exit 0 ;;
      pass-but-exit1) echo "selftest: PASS (26 checks)"; exit 1 ;;
      unverifiable) echo "selftest: UNVERIFIABLE"; exit 3 ;;
      silent) exit 0 ;;
    esac
    ;;
  bench)
    log "bench req_root=${REQ_ROOT:-} cwd=$(pwd -P)"
    if [ $# -ne 1 ]; then echo "stub: wrong bench invocation: $*" >&2; exit 64; fi
    if ! tree_is_the_clean_copy; then echo "stub saw a tree that is not the committed one" >&2; exit 1; fi
    case "${STUB_BENCH:-pass}" in
      pass) emit_bench UNVERIFIABLE PASS; exit 0 ;;
      fail) emit_bench UNVERIFIABLE FAIL; exit 1 ;;
      fail-exit0) emit_bench UNVERIFIABLE FAIL; exit 0 ;;
      direction-pass-fmr-fail) emit_bench PASS FAIL; exit 0 ;;
      pass-but-exit1) emit_bench UNVERIFIABLE PASS; exit 1 ;;
      unverifiable) emit_bench UNVERIFIABLE UNVERIFIABLE; exit 3 ;;
      no-json) echo "no record"; exit 0 ;;
    esac
    ;;
esac
echo "stub: unhandled: $*" >&2
exit 64
"##;

/// A shim that answers the two questions the script asks of `rustc`.
fn rustc_shim(host: &str) -> String {
    format!(
        "#!/bin/sh\ncase \"$1\" in\n  -vV) printf 'rustc 1.99.0 (shim 2026-01-01)\\nbinary: rustc\\ncommit-hash: 0000000000000000000000000000000000000000\\nhost: {host}\\nrelease: 1.99.0\\n' ;;\n  -V) echo 'rustc 1.99.0 (shim 2026-01-01)' ;;\n  *) echo \"rustc shim: $*\" >&2; exit 64 ;;\nesac\n"
    )
}

/// A `cargo` shim: `-V` is a version line; `build` records its argv and working directory and
/// "compiles" by copying the tracked `stub-req` of the tree it runs in to `target/release/req`.
const CARGO_SHIM: &str = r#"#!/bin/sh
case "$1" in
  -V) echo 'cargo 1.99.0 (shim 2026-01-01)' ;;
  build)
    [ -z "${CARGO_SHIM_LOG:-}" ] || { printf 'argv=%s\ncwd=%s\nencoded_rustflags=%s\nrustflags=%s\nsource_date_epoch=%s\n' "$*" "$(pwd -P)" "$(printf '%s' "${CARGO_ENCODED_RUSTFLAGS:-}" | tr '\037' '|')" "${RUSTFLAGS:-}" "${SOURCE_DATE_EPOCH:-}" >> "$CARGO_SHIM_LOG"; }
    mkdir -p "${CARGO_TARGET_DIR:-target}/release"
    cp stub-req "${CARGO_TARGET_DIR:-target}/release/req"
    [ -z "${CARGO_SHIM_NOISE:-}" ] || printf '# %s\n' "$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')" >> "${CARGO_TARGET_DIR:-target}/release/req"
    chmod 755 "${CARGO_TARGET_DIR:-target}/release/req"
    ;;
  *) echo "cargo shim: $*" >&2; exit 64 ;;
esac
"#;

/// Tools the script may use, linked into the shim `PATH` from wherever the host keeps them.
/// `minisign` is deliberately not here: it is a shim, or absent (case e).
const HOST_TOOLS: &[&str] = &[
    "awk",
    "basename",
    "bash",
    "cat",
    "chmod",
    "cmp",
    "cp",
    "cut",
    "date",
    "dirname",
    "egrep",
    "env",
    "find",
    "git",
    "grep",
    "gunzip",
    "gzip",
    "head",
    "id",
    "ldd",
    "ln",
    "ls",
    "mkdir",
    "mktemp",
    "mv",
    "od",
    "readlink",
    "realpath",
    "rm",
    "rmdir",
    "sed",
    "sh",
    "sha256sum",
    "shasum",
    "sleep",
    "sort",
    "sw_vers",
    "tail",
    "tar",
    "tee",
    "touch",
    "tr",
    "uname",
    "uniq",
    "wc",
    "xargs",
];

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/build-release.sh")
}

fn which(tool: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(tool))
            .find(|candidate| candidate.is_file())
    })
}

fn write_executable(path: &Path, text: &str) {
    fs::write(path, text).expect("writes the executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("marks it executable");
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .expect("runs git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// What a run of the script left behind and said.
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn from(output: &Output) -> Self {
        Run {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// One throwaway world: a git repository holding one commit, a shim `PATH`, a temp dir the script
/// must leave empty, an `--out` directory path (not created), and the stub `req` outside the repo.
struct World {
    root: PathBuf,
    repo: PathBuf,
    shims: PathBuf,
    tmp: PathBuf,
    out: PathBuf,
    stub_log: PathBuf,
    cargo_log: PathBuf,
    bin: PathBuf,
    full_sha: String,
    short: String,
}

impl World {
    fn new(name: &str, with_minisign: bool, host: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "req-release-bundle-{}-{}-{name}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let shims = root.join("shims");
        let tmp = root.join("tmp");
        for dir in [&repo, &shims, &tmp, &root.join("home"), &root.join("bin")] {
            fs::create_dir_all(dir).expect("creates the fixture dir");
        }

        for tool in HOST_TOOLS {
            if let Some(real) = which(tool) {
                std::os::unix::fs::symlink(real, shims.join(tool)).expect("links a host tool");
            }
        }
        write_executable(&shims.join("rustc"), &rustc_shim(host));
        write_executable(&shims.join("cargo"), CARGO_SHIM);
        if with_minisign {
            write_executable(&shims.join("minisign"), "#!/bin/sh\nexit 0\n");
        }

        git(&repo, &["init", "-q"]);
        fs::create_dir_all(repo.join("baselines")).expect("creates baselines/");
        fs::create_dir_all(repo.join("claims")).expect("creates claims/");
        fs::write(repo.join("baselines/air-gapped.json"), COMMITTED_BASELINE)
            .expect("writes the baseline");
        fs::write(
            repo.join("claims/REQ-0001.yml"),
            "id: REQ-0001\nstatement: The system shall respond within 200 ms.\n",
        )
        .expect("writes a claim");
        write_executable(&repo.join("stub-req"), STUB_REQ);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "fixture"]);
        let full_sha = git(&repo, &["rev-parse", "HEAD"]);

        let bin = root.join("bin/req");
        write_executable(&bin, STUB_REQ);

        World {
            out: root.join("out"),
            stub_log: root.join("stub.log"),
            cargo_log: root.join("cargo.log"),
            root,
            repo,
            shims,
            tmp,
            bin,
            short: full_sha[..7].to_string(),
            full_sha,
        }
    }

    fn standard(name: &str) -> Self {
        World::new(name, true, TARGET)
    }

    /// The seven-character prefix a faithful `req --version` reports for the fixture commit.
    fn short(&self) -> String {
        self.short.clone()
    }

    /// Run the script with the given arguments and stub dials, in a cleared environment.
    fn run(&self, args: &[&str], dials: &[(&str, &str)]) -> Run {
        self.run_script(&script_path(), args, dials)
    }

    fn run_script(&self, script: &Path, args: &[&str], dials: &[(&str, &str)]) -> Run {
        let mut command = Command::new("/bin/bash");
        command
            .arg(script)
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", &self.shims)
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", &self.tmp)
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("STUB_LOG", &self.stub_log)
            .env("CARGO_SHIM_LOG", &self.cargo_log);
        for (key, value) in dials {
            command.env(key, value);
        }
        let output = command.output().expect("runs the script");
        let run = Run::from(&output);
        let left: Vec<_> = fs::read_dir(&self.tmp)
            .expect("reads the temp dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert!(
            left.is_empty(),
            "the script left temporary files behind in TMPDIR: {left:?}"
        );
        run
    }

    /// The standard invocation: this repo's `HEAD`, the stub as `--bin`, the fixture `--out`.
    fn release(&self, dials: &[(&str, &str)]) -> Run {
        let bin = self.bin.to_string_lossy().into_owned();
        let repo = self.repo.to_string_lossy().into_owned();
        let out = self.out.to_string_lossy().into_owned();
        self.run(
            &[
                "--rev", "HEAD", "--repo", &repo, "--out", &out, "--bin", &bin,
            ],
            dials,
        )
    }

    fn built_from(&self) -> (&'static str, &str) {
        ("STUB_BUILT_FROM", &self.short)
    }

    fn asset(&self, suffix: &str) -> PathBuf {
        self.out
            .join(format!("req-{}-{TARGET}{suffix}", self.short()))
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// `--out` holds nothing: absent, or present and empty. Nothing was written.
fn assert_nothing_written(out: &Path) {
    match fs::read_dir(out) {
        Err(_) => assert!(!out.exists(), "--out exists but cannot be read"),
        Ok(entries) => {
            let names: Vec<_> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect();
            assert!(names.is_empty(), "--out must stay empty, holds {names:?}");
        }
    }
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("reads the directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn tar_output(args: &[&str], archive: &Path) -> String {
    let out = Command::new("tar")
        .args(args)
        .arg(archive)
        .output()
        .expect("runs tar");
    assert!(out.status.success(), "tar {args:?} failed");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ---------------------------------------------------------------------------------------------
// (a) the happy path
// ---------------------------------------------------------------------------------------------

#[test]
fn a_release_writes_exactly_the_tarball_its_checksum_and_the_build_info() {
    let world = World::standard("happy");
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);

    let short = world.short();
    let expected = vec![
        format!("req-{short}-{TARGET}.build-info"),
        format!("req-{short}-{TARGET}.tar.gz"),
        format!("req-{short}-{TARGET}.tar.gz.sha256"),
    ];
    assert_eq!(
        file_names(&world.out),
        expected,
        "exactly the three assets, no temp leftovers"
    );

    // The archive: exactly one member `req`, at the root, mode 0755, the stub's own bytes.
    let tarball = world.asset(".tar.gz");
    assert_eq!(tar_output(&["-tzf"], &tarball), "req\n");
    let listing = tar_output(&["-tvzf"], &tarball);
    assert!(
        listing.starts_with("-rwxr-xr-x"),
        "the member is a regular 0755 file: {listing}"
    );
    let unpacked = world.root.join("unpacked");
    fs::create_dir_all(&unpacked).expect("creates the unpack dir");
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&tarball)
        .arg("-C")
        .arg(&unpacked)
        .status()
        .expect("runs tar");
    assert!(status.success());
    assert_eq!(
        fs::read_to_string(unpacked.join("req")).expect("reads the member"),
        STUB_REQ,
        "the member is the binary that was proven, byte for byte"
    );
    assert_eq!(file_names(&unpacked), vec!["req".to_string()]);

    // The checksum: one line, an independently computed hash, naming the asset.
    let digest = hex_sha256(&fs::read(&tarball).expect("reads the tarball"));
    let asset_name = format!("req-{short}-{TARGET}.tar.gz");
    assert_eq!(
        fs::read_to_string(world.asset(".tar.gz.sha256")).expect("reads the checksum"),
        format!("{digest}  {asset_name}\n")
    );

    // The build info: informational, but it must say what the ADR says it says.
    let info = fs::read_to_string(world.asset(".build-info")).expect("reads the build info");
    for needle in [
        short.as_str(),
        world.full_sha.as_str(),
        TARGET,
        "rustc 1.99.0 (shim 2026-01-01)",
        "cargo 1.99.0 (shim 2026-01-01)",
    ] {
        assert!(
            info.contains(needle),
            "build-info lacks {needle:?}:\n{info}"
        );
    }
    let stamp = info
        .lines()
        .find_map(|line| line.strip_prefix("built_utc="))
        .expect("build-info carries built_utc=");
    let b = stamp.as_bytes();
    assert!(
        stamp.len() == 20
            && b[4] == b'-'
            && b[7] == b'-'
            && b[10] == b'T'
            && b[13] == b':'
            && b[16] == b':'
            && b[19] == b'Z',
        "UTC time as %Y-%m-%dT%H:%M:%SZ, got {stamp:?}"
    );

    // What it prints: the three paths, and the one line an adopter pastes.
    for suffix in [".tar.gz", ".tar.gz.sha256", ".build-info"] {
        let path = world.asset(suffix);
        assert!(
            run.stdout.contains(&path.to_string_lossy().into_owned()),
            "stdout names {path:?}:\n{}",
            run.stdout
        );
    }
    let pin = format!("pin: REV={short} SHA256={digest} TARGET={TARGET}");
    assert!(
        run.stdout.lines().any(|line| line == pin),
        "stdout has the line {pin:?}:\n{}",
        run.stdout
    );
}

#[test]
fn the_checks_run_with_the_clean_copy_as_the_working_directory() {
    let world = World::standard("cwd");
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let log = fs::read_to_string(&world.stub_log).expect("the stub logged where it ran");
    let repo = fs::canonicalize(&world.repo).expect("canonicalizes the repo");
    let cwds: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_once("cwd=").map(|(_, cwd)| cwd))
        .collect();
    assert_eq!(cwds.len(), 2, "baseline --verify and bench both ran: {log}");
    assert_eq!(cwds[0], cwds[1], "both checks ran in the same copy");
    assert!(
        log.lines().any(|line| line == "selftest ran"),
        "the binary's selftest ran too: {log}"
    );
    assert!(
        !Path::new(cwds[0]).starts_with(&repo),
        "the checks must not run in the caller's repository ({repo:?}): {cwds:?}"
    );
}

#[test]
fn an_exported_req_root_cannot_move_the_proofs_off_the_clean_copy() {
    // `req` resolves its corpus from REQ_ROOT before the working directory, so a caller's export
    // would make both proofs judge (and `bench` write into) some other checkout.
    let world = World::standard("req-root");
    let elsewhere = world.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("creates the other root");
    let elsewhere = elsewhere.to_string_lossy().into_owned();
    let run = world.release(&[world.built_from(), ("REQ_ROOT", &elsewhere)]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let log = fs::read_to_string(&world.stub_log).expect("the stub logged its environment");
    let roots: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_once("req_root=").map(|(_, rest)| rest))
        .map(|rest| rest.split(" cwd=").next().unwrap_or(rest))
        .collect();
    assert_eq!(roots.len(), 2, "both proofs ran: {log}");
    assert!(
        roots.iter().all(|root| root.is_empty()),
        "REQ_ROOT must not reach the proofs: {roots:?}"
    );
}

#[test]
fn the_callers_git_configuration_cannot_change_the_clean_copy() {
    // A global `core.autocrlf = true` checks files out with CRLF endings: bytes that are not the
    // commit's. The stub's `tree_is_the_clean_copy` compares the committed Baseline text exactly.
    let world = World::standard("git-config");
    let config = world.root.join("gitconfig");
    fs::write(&config, "[core]\n\tautocrlf = true\n").expect("writes a git config");
    let config = config.to_string_lossy().into_owned();
    let run = world.release(&[world.built_from(), ("GIT_CONFIG_GLOBAL", &config)]);
    assert_eq!(
        run.code,
        Some(0),
        "the proofs must see the commit's bytes whatever the caller's git config says; stderr: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------------------------
// (b) the binary must prove which rev it is
// ---------------------------------------------------------------------------------------------

#[test]
fn a_binary_that_does_not_report_a_bare_prefix_of_the_rev_is_refused() {
    let world = World::standard("built-from");
    let wrong_prefix = if world.full_sha.starts_with("0000000") {
        "1111111"
    } else {
        "0000000"
    };
    let cases = [
        format!("{}-dirty", world.short()),
        format!("{}-cleanliness-unknown", world.short()),
        "unknown".to_string(),
        wrong_prefix.to_string(),
        String::new(),
        // A prefix of the full SHA, but too short to name a commit: refused on length alone.
        world.full_sha[..6].to_string(),
        world.full_sha[..1].to_string(),
    ];
    for built_from in &cases {
        let run = world.release(&[("STUB_BUILT_FROM", built_from)]);
        assert_eq!(
            run.code,
            Some(1),
            "built from {built_from:?} must be refused; stderr: {}",
            run.stderr
        );
        assert!(
            run.stderr.contains("built-from"),
            "the failure names the check (built-from): {}",
            run.stderr
        );
        assert_nothing_written(&world.out);
    }
}

#[test]
fn the_full_sha_is_a_bare_sha_too() {
    let world = World::standard("full-sha");
    let run = world.release(&[("STUB_BUILT_FROM", &world.full_sha)]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let names = file_names(&world.out);
    assert!(
        names.contains(&format!("req-{}-{TARGET}.tar.gz", world.full_sha)),
        "<REV> is the string the binary printed: {names:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (c) the signed Baseline must verify with this binary
// ---------------------------------------------------------------------------------------------

#[test]
fn a_baseline_that_is_not_verified_blocks_the_release() {
    let world = World::standard("baseline");
    for verify in [
        "fail",
        "unverifiable",
        "verified-but-exit1",
        "no-brackets",
        "silent",
    ] {
        let run = world.release(&[world.built_from(), ("STUB_VERIFY", verify)]);
        assert_eq!(
            run.code,
            Some(1),
            "baseline --verify {verify} must block; stderr: {}",
            run.stderr
        );
        assert!(
            run.stderr.contains("baseline-verify"),
            "the failure names the check (baseline-verify): {}",
            run.stderr
        );
        assert_nothing_written(&world.out);
    }
}

// ---------------------------------------------------------------------------------------------
// (d) the FMR gate must pass with this binary
// ---------------------------------------------------------------------------------------------

#[test]
fn a_bench_whose_fmr_gate_does_not_pass_blocks_the_release() {
    let world = World::standard("bench");
    for bench in [
        "fail",
        "fail-exit0",
        // The `direction` block says PASS; only `fmr.hard_gate` is judged, and it says FAIL.
        "direction-pass-fmr-fail",
        "pass-but-exit1",
        "unverifiable",
        "no-json",
    ] {
        let run = world.release(&[world.built_from(), ("STUB_BENCH", bench)]);
        assert_eq!(
            run.code,
            Some(1),
            "bench {bench} must block; stderr: {}",
            run.stderr
        );
        assert!(
            run.stderr.contains("bench-gate"),
            "the failure names the check (bench-gate): {}",
            run.stderr
        );
        assert_nothing_written(&world.out);
    }
}

#[test]
fn the_direction_block_being_unverifiable_does_not_block_a_passing_fmr_gate() {
    // The seed parity corpus's `direction` block is UNVERIFIABLE by design; the happy path is
    // exactly that shape (`STUB_BENCH=pass`), and (a) proves it releases. This pins it alone.
    let world = World::standard("direction-unverifiable");
    let run = world.release(&[world.built_from(), ("STUB_BENCH", "pass")]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
}

// ---------------------------------------------------------------------------------------------
// (d2) the binary's own known answers must hold (ADR 0034, ADR 0037)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_selftest_that_does_not_pass_blocks_the_release() {
    let world = World::standard("selftest");
    for selftest in [
        "fail",
        "fail-exit0",
        "pass-but-exit1",
        "unverifiable",
        "silent",
    ] {
        let run = world.release(&[world.built_from(), ("STUB_SELFTEST", selftest)]);
        assert_eq!(
            run.code,
            Some(1),
            "selftest {selftest} must block; stderr: {}",
            run.stderr
        );
        assert!(
            run.stderr.contains("selftest"),
            "the failure names the check (selftest): {}",
            run.stderr
        );
        assert_nothing_written(&world.out);
    }
}

#[test]
fn the_build_info_is_a_record_of_the_proofs_that_ran_and_says_it_is_self_attestation() {
    let world = World::standard("record");
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let info = fs::read_to_string(world.asset(".build-info")).expect("reads the build info");
    let value = |key: &str| -> String {
        let prefix = format!("{key}=");
        let hits: Vec<&str> = info
            .lines()
            .filter_map(|line| line.strip_prefix(prefix.as_str()))
            .collect();
        assert_eq!(hits.len(), 1, "exactly one {key}= line in:\n{info}");
        hits[0].to_string()
    };
    assert_eq!(value("record"), "req.release-record/1");
    // The binary that passed, by an independent hash of the member (not of the archive).
    let unpacked = world.root.join("record-unpacked");
    fs::create_dir_all(&unpacked).expect("creates the unpack dir");
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(world.asset(".tar.gz"))
        .arg("-C")
        .arg(&unpacked)
        .status()
        .expect("runs tar");
    assert!(status.success());
    assert_eq!(
        value("binary_sha256"),
        hex_sha256(&fs::read(unpacked.join("req")).expect("reads the member"))
    );
    // Every proof the script ran, by name, as the script judged it.
    for proof in [
        "proof_built_from",
        "proof_baseline_verify",
        "proof_bench_fmr",
        "proof_selftest",
    ] {
        assert_eq!(value(proof), "pass", "{proof}");
    }
    // The numbers an assessor reads, taken from the binary's own output, not typed here.
    assert_eq!(value("rules_v"), "6");
    assert_eq!(value("claim_schema"), "req.claim/2");
    assert_eq!(
        value("baseline"),
        "# Baseline baselines/air-gapped.json: 18 claim(s)"
    );
    assert_eq!(
        value("bench_corpus_sha256"),
        "4a1727af8631325b190fda8921390b7c559215bb0edc9f00079edf1c2f0c9889"
    );
    assert_eq!(value("selftest"), "selftest: PASS (26 checks)");
    // What the record is NOT: said in the record itself, where an assessor reads it.
    let attestation = value("attestation");
    for needle in ["self", "not independent"] {
        assert!(
            attestation.contains(needle),
            "the attestation says {needle:?}: {attestation}"
        );
    }
    // And it records no claim about what did not run here.
    for absent in [
        "proof_gate",
        "proof_tests",
        "proof_reproduce",
        "proof_mutation",
    ] {
        assert!(
            !info.contains(absent),
            "{absent} did not run in this script, so the record cannot claim it:\n{info}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (e) no minisign is UNVERIFIABLE, not a skip
// ---------------------------------------------------------------------------------------------

#[test]
fn without_minisign_the_release_is_unverifiable_and_writes_nothing() {
    let world = World::new("no-minisign", false, TARGET);
    assert!(
        !world.shims.join("minisign").exists(),
        "the fixture PATH holds no minisign"
    );
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(3), "stderr: {}", run.stderr);
    assert!(
        run.stderr.contains("minisign"),
        "the failure names minisign: {}",
        run.stderr
    );
    assert_nothing_written(&world.out);
}

// ---------------------------------------------------------------------------------------------
// (f) never overwrite
// ---------------------------------------------------------------------------------------------

#[test]
fn an_existing_output_file_is_refused_and_left_untouched() {
    for suffix in [".tar.gz", ".tar.gz.sha256", ".build-info"] {
        let world = World::standard("collision");
        fs::create_dir_all(&world.out).expect("creates --out");
        let existing = world.asset(suffix);
        fs::write(&existing, "pinned elsewhere\n").expect("writes the existing asset");

        let run = world.release(&[world.built_from()]);
        assert_eq!(run.code, Some(1), "{suffix} exists; stderr: {}", run.stderr);
        assert_eq!(
            fs::read_to_string(&existing).expect("reads the existing asset"),
            "pinned elsewhere\n",
            "the existing {suffix} is untouched"
        );
        assert_eq!(
            file_names(&world.out),
            vec![
                existing
                    .file_name()
                    .expect("has a name")
                    .to_string_lossy()
                    .into_owned()
            ],
            "nothing else was written beside it"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (g) usage
// ---------------------------------------------------------------------------------------------

#[test]
fn usage_errors_are_exit_2_and_write_nothing() {
    let world = World::standard("usage");
    let out = world.out.to_string_lossy().into_owned();
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("no arguments", vec![]),
        ("no --rev", vec!["--out", &out]),
        ("--rev without a value", vec!["--out", &out, "--rev"]),
        ("no --out", vec!["--rev", "HEAD"]),
        ("--out without a value", vec!["--rev", "HEAD", "--out"]),
        (
            "an unknown flag",
            vec!["--rev", "HEAD", "--out", &out, "--frobnicate"],
        ),
        (
            "an extra positional",
            vec!["--rev", "HEAD", "--out", &out, "stray"],
        ),
    ];
    for (what, args) in &cases {
        let run = world.run(args, &[world.built_from()]);
        assert_eq!(run.code, Some(2), "{what}; stderr: {}", run.stderr);
        assert!(
            run.stderr.contains("usage:"),
            "{what}: the message shows the usage line: {}",
            run.stderr
        );
        assert_nothing_written(&world.out);
    }
}

#[test]
fn an_unresolvable_rev_is_exit_2_and_writes_nothing() {
    let world = World::standard("bad-rev");
    let repo = world.repo.to_string_lossy().into_owned();
    let out = world.out.to_string_lossy().into_owned();
    let bin = world.bin.to_string_lossy().into_owned();
    let run = world.run(
        &[
            "--rev",
            "no-such-rev",
            "--repo",
            &repo,
            "--out",
            &out,
            "--bin",
            &bin,
        ],
        &[world.built_from()],
    );
    assert_eq!(run.code, Some(2), "stderr: {}", run.stderr);
    assert!(
        run.stderr.contains("no-such-rev"),
        "names the rev: {}",
        run.stderr
    );
    assert_nothing_written(&world.out);
}

#[test]
fn an_unsupported_host_is_exit_2_naming_the_host() {
    let world = World::new("host", true, "x86_64-pc-windows-msvc");
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(2), "stderr: {}", run.stderr);
    assert!(
        run.stderr.contains("x86_64-pc-windows-msvc"),
        "names the host: {}",
        run.stderr
    );
    assert_nothing_written(&world.out);
}

#[test]
fn the_other_supported_target_is_named_in_the_assets() {
    let world = World::new("darwin", true, "aarch64-apple-darwin");
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    assert!(
        file_names(&world.out).contains(&format!(
            "req-{}-aarch64-apple-darwin.tar.gz",
            world.short()
        )),
        "{:?}",
        file_names(&world.out)
    );
}

// ---------------------------------------------------------------------------------------------
// (h) a clean copy of the commit, never the caller's tree
// ---------------------------------------------------------------------------------------------

#[test]
fn a_modified_tracked_file_in_the_callers_checkout_cannot_reach_the_artifact() {
    let world = World::standard("dirty-caller");
    // The caller's checkout is not the commit: a tracked file edited, an untracked file added.
    fs::write(world.repo.join("baselines/air-gapped.json"), "DIRTY\n").expect("dirties a file");
    fs::write(world.repo.join("stray.txt"), "untracked\n").expect("adds a stray file");

    let run = world.release(&[world.built_from()]);
    assert_eq!(
        run.code,
        Some(0),
        "the stub accepts only the committed tree, so a run in the caller's tree fails; stderr: {}",
        run.stderr
    );
}

#[test]
fn without_bin_the_build_runs_in_the_clean_copy_and_packages_the_committed_binary() {
    let world = World::standard("build");
    // Dirty the tracked source of the "compiled" binary in the caller's checkout.
    fs::write(
        world.repo.join("stub-req"),
        format!("{STUB_REQ}# modified\n"),
    )
    .expect("modifies the tracked file");

    let repo = world.repo.to_string_lossy().into_owned();
    let out = world.out.to_string_lossy().into_owned();
    let run = world.run(
        &["--rev", "HEAD", "--repo", &repo, "--out", &out],
        &[world.built_from()],
    );
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);

    let log = fs::read_to_string(&world.cargo_log).expect("cargo build ran");
    assert!(
        log.lines()
            .any(|line| line == "argv=build --release --locked"),
        "the build is `cargo build --release --locked`: {log}"
    );
    let canonical_repo = fs::canonicalize(&world.repo).expect("canonicalizes the repo");
    let cwd = log
        .lines()
        .find_map(|line| line.strip_prefix("cwd="))
        .expect("cargo logged its working directory");
    assert!(
        !Path::new(cwd).starts_with(&canonical_repo),
        "cargo ran in the caller's repository: {cwd}"
    );

    let unpacked = world.root.join("unpacked");
    fs::create_dir_all(&unpacked).expect("creates the unpack dir");
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(world.asset(".tar.gz"))
        .arg("-C")
        .arg(&unpacked)
        .status()
        .expect("runs tar");
    assert!(status.success());
    assert_eq!(
        fs::read_to_string(unpacked.join("req")).expect("reads the member"),
        STUB_REQ,
        "the artifact holds the committed binary, not the caller's edit"
    );
}

// ---------------------------------------------------------------------------------------------
// reproducibility (ADR 0035): the same rev and the same bytes give the same tarball, wherever and
// whenever it is made
// ---------------------------------------------------------------------------------------------

/// A second `--out` beside the world's own, so one fixture can be released twice.
fn release_into(world: &World, out: &Path, dials: &[(&str, &str)]) -> Run {
    let bin = world.bin.to_string_lossy().into_owned();
    let repo = world.repo.to_string_lossy().into_owned();
    let out = out.to_string_lossy().into_owned();
    world.run(
        &[
            "--rev", "HEAD", "--repo", &repo, "--out", &out, "--bin", &bin,
        ],
        dials,
    )
}

/// The first 512 bytes of the tar inside a `.tar.gz`: one member's header, decoded by hand from the
/// ustar layout (an independent oracle: the script writes it with `tar`, this reads the bytes).
fn first_tar_header(tarball: &Path) -> Vec<u8> {
    let out = Command::new("gzip")
        .args(["-dc"])
        .arg(tarball)
        .output()
        .expect("runs gzip");
    assert!(out.status.success(), "gzip -dc failed");
    out.stdout[..512].to_vec()
}

/// A NUL- or space-terminated octal field of a ustar header.
fn octal_field(header: &[u8], from: usize, len: usize) -> u64 {
    let text: String = header[from..from + len]
        .iter()
        .take_while(|b| **b != 0 && **b != b' ')
        .map(|b| char::from(*b))
        .collect();
    u64::from_str_radix(text.trim(), 8).unwrap_or_else(|_| panic!("not octal: {text:?}"))
}

fn text_field(header: &[u8], from: usize, len: usize) -> String {
    header[from..from + len]
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| char::from(*b))
        .collect()
}

#[test]
fn one_rev_and_one_binary_give_byte_identical_tarballs_at_any_time_and_in_any_directory() {
    let world = World::standard("repro-bytes");
    let first = release_into(&world, &world.root.join("out-a"), &[world.built_from()]);
    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);
    // Clock moves, the temp dir and the output directory change, and the binary is a new file with
    // a new modification time: none of it may reach the archive.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let other_tmp = world.root.join("a-much-longer-temporary-directory-name");
    fs::create_dir_all(&other_tmp).expect("creates the second temp dir");
    let bin = world.bin.to_string_lossy().into_owned();
    let repo = world.repo.to_string_lossy().into_owned();
    let out_b = world
        .root
        .join("elsewhere/out-b")
        .to_string_lossy()
        .into_owned();
    let mut command = Command::new("/bin/bash");
    command
        .arg(script_path())
        .args([
            "--rev", "HEAD", "--repo", &repo, "--out", &out_b, "--bin", &bin,
        ])
        .current_dir(&world.root)
        .env_clear()
        .env("PATH", &world.shims)
        .env("HOME", world.root.join("home"))
        .env("TMPDIR", &other_tmp)
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("STUB_BUILT_FROM", &world.short);
    let second = command.output().expect("runs the script");
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );

    let name = format!("req-{}-{TARGET}.tar.gz", world.short());
    let a = fs::read(world.root.join("out-a").join(&name)).expect("reads the first tarball");
    let b = fs::read(Path::new(&out_b).join(&name)).expect("reads the second tarball");
    assert_eq!(
        hex_sha256(&a),
        hex_sha256(&b),
        "the same rev and binary must give the same archive bytes"
    );
    let sum_a = fs::read_to_string(world.root.join("out-a").join(format!("{name}.sha256")))
        .expect("reads the checksum");
    assert_eq!(sum_a, format!("{}  {name}\n", hex_sha256(&a)));
}

#[test]
fn the_member_header_holds_no_clock_no_user_and_no_group_of_the_builder() {
    let world = World::standard("repro-header");
    let run = world.release(&[world.built_from()]);
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let header = first_tar_header(&world.asset(".tar.gz"));

    assert_eq!(text_field(&header, 0, 100), "req");
    assert_eq!(octal_field(&header, 100, 8), 0o755, "mode");
    assert_eq!(octal_field(&header, 108, 8), 0, "uid is not the builder's");
    assert_eq!(octal_field(&header, 116, 8), 0, "gid is not the builder's");
    for (what, name) in [
        ("uname", text_field(&header, 265, 32)),
        ("gname", text_field(&header, 297, 32)),
    ] {
        assert!(
            name.is_empty() || name == "root",
            "{what} is the builder's ({name:?}); one name for every builder is empty or root"
        );
    }
    // The modification time is the commit's own (SOURCE_DATE_EPOCH's convention): a fact of the rev.
    let commit_time: u64 = git(&world.repo, &["log", "-1", "--format=%ct", "HEAD"])
        .parse()
        .expect("a commit time");
    assert_eq!(
        octal_field(&header, 136, 12),
        commit_time,
        "mtime is the commit time, not the build time"
    );
    // ustar, so no per-run extended header (pax) can carry a sub-second clock or a path.
    assert_eq!(&header[257..262], b"ustar", "the archive is ustar");
}

#[test]
fn the_build_maps_the_builders_paths_away_and_ignores_a_rustflags_in_the_environment() {
    let world = World::standard("repro-flags");
    let repo = world.repo.to_string_lossy().into_owned();
    let out = world.out.to_string_lossy().into_owned();
    let run = world.run(
        &["--rev", "HEAD", "--repo", &repo, "--out", &out],
        &[
            world.built_from(),
            ("RUSTFLAGS", "-Cdebuginfo=2"),
            ("CARGO_ENCODED_RUSTFLAGS", "-Cdebuginfo=2"),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr: {}", run.stderr);
    let log = fs::read_to_string(&world.cargo_log).expect("cargo build ran");
    let encoded = log
        .lines()
        .find_map(|line| line.strip_prefix("encoded_rustflags="))
        .expect("the build logged its flags");
    let flags: Vec<&str> = encoded.split('|').collect();
    assert!(
        !flags.iter().any(|flag| flag.contains("debuginfo")),
        "an exported RUSTFLAGS changed the codegen of a released binary: {flags:?}"
    );
    let home = world.root.join("home").join(".cargo");
    let home = format!("--remap-path-prefix={}=/cargo", home.display());
    assert!(
        flags.contains(&home.as_str()),
        "the registry path ({home}) must be mapped, or every dependency's source path carries the builder's home: {flags:?}"
    );
    assert!(
        flags
            .iter()
            .any(|flag| flag.starts_with("--remap-path-prefix=") && flag.ends_with("/src=/req")),
        "the clean copy's own path must be mapped: {flags:?}"
    );
    assert_eq!(
        log.lines().find_map(|l| l.strip_prefix("rustflags=")),
        Some(""),
        "plain RUSTFLAGS must not be set (the encoded form is used, so a path with a space survives)"
    );
}

// ---------------------------------------------------------------------------------------------
// scripts/reproduce.sh: rebuild and compare (ADR 0035)
// ---------------------------------------------------------------------------------------------

fn reproduce_path() -> PathBuf {
    script_path().with_file_name("reproduce.sh")
}

impl World {
    fn reproduce(&self, extra: &[&str], dials: &[(&str, &str)]) -> Run {
        let repo = self.repo.to_string_lossy().into_owned();
        let mut args = vec!["--rev", "HEAD", "--repo", repo.as_str()];
        args.extend_from_slice(extra);
        self.run_script(&reproduce_path(), &args, dials)
    }
}

#[test]
fn two_builds_of_one_rev_are_reproduced_and_the_binary_hash_is_the_independent_one() {
    let world = World::standard("rep-ok");
    let run = world.reproduce(&[], &[world.built_from()]);
    assert_eq!(
        run.code,
        Some(0),
        "stdout: {} stderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(run.stdout.starts_with("REPRODUCED"), "{}", run.stdout);
    let expected = hex_sha256(STUB_REQ.as_bytes());
    assert!(
        run.stdout.contains(&format!("binary   {expected}")),
        "the hash printed for the binary is the SHA-256 of the binary's bytes: {}",
        run.stdout
    );
    assert!(
        !world.out.exists(),
        "reproduce.sh writes nothing the caller has to clean up"
    );
}

#[test]
fn a_builder_whose_output_varies_is_reported_as_differing_with_both_hashes() {
    let world = World::standard("rep-noisy");
    let run = world.reproduce(&[], &[world.built_from(), ("CARGO_SHIM_NOISE", "1")]);
    assert_eq!(
        run.code,
        Some(1),
        "stdout: {} stderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(run.stdout.starts_with("DIFFERS"), "{}", run.stdout);
    let hashes: Vec<&str> = run
        .stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("binary   "))
        .collect();
    assert_eq!(hashes.len(), 2, "both binaries are named: {}", run.stdout);
    assert_ne!(hashes[0], hashes[1], "{}", run.stdout);
    assert!(
        hashes.iter().all(|h| h.len() == 64),
        "full SHA-256 values: {hashes:?}"
    );
}

#[test]
fn a_release_is_reproduced_against_when_its_binary_is_the_one_the_rev_builds() {
    let world = World::standard("rep-against");
    let released = world.release(&[world.built_from()]);
    assert_eq!(released.code, Some(0), "stderr: {}", released.stderr);
    let tarball = world.asset(".tar.gz").to_string_lossy().into_owned();
    let run = world.reproduce(&["--against", &tarball], &[world.built_from()]);
    assert_eq!(
        run.code,
        Some(0),
        "stdout: {} stderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(run.stdout.starts_with("REPRODUCED"), "{}", run.stdout);
}

#[test]
fn a_tarball_holding_another_binary_is_not_reproduced() {
    let world = World::standard("rep-tamper");
    let dir = world.root.join("other");
    fs::create_dir_all(&dir).expect("creates the dir");
    write_executable(
        &dir.join("req"),
        &format!("{STUB_REQ}# not the built one\n"),
    );
    let tarball = world.root.join("other.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(&dir)
        .arg("req")
        .status()
        .expect("runs tar");
    assert!(status.success());
    let tarball = tarball.to_string_lossy().into_owned();
    let run = world.reproduce(&["--against", &tarball], &[world.built_from()]);
    assert_eq!(
        run.code,
        Some(1),
        "stdout: {} stderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(run.stdout.starts_with("DIFFERS"), "{}", run.stdout);
    assert!(
        run.stdout.contains(&hex_sha256(STUB_REQ.as_bytes())),
        "the hash this machine built is shown: {}",
        run.stdout
    );
    // How far apart the two are: the given binary has the build's bytes plus 24 more.
    assert!(
        run.stdout.contains(&format!(
            "built {} bytes, given {} bytes; 0 bytes differ in the common prefix",
            STUB_REQ.len(),
            STUB_REQ.len() + "# not the built one\n".len()
        )),
        "the distance between the two binaries is shown: {}",
        run.stdout
    );
}

#[test]
fn without_minisign_reproduction_is_unverifiable_never_a_pass() {
    let world = World::new("rep-nominisign", false, TARGET);
    let run = world.reproduce(&[], &[world.built_from()]);
    assert_eq!(
        run.code,
        Some(3),
        "stdout: {} stderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(!run.stdout.contains("REPRODUCED"), "{}", run.stdout);
    assert!(run.stderr.contains("UNVERIFIABLE"), "{}", run.stderr);
}

#[test]
fn reproduce_usage_errors_are_exit_2() {
    let world = World::standard("rep-usage");
    for args in [
        vec![],
        vec!["--rev"],
        vec!["--rev", "HEAD", "--against", "/no/such/file.tar.gz"],
        vec!["--rev", "HEAD", "stray"],
        vec!["--rev", "HEAD", "--no-such-flag"],
    ] {
        let run = world.run_script(&reproduce_path(), &args, &[]);
        assert_eq!(run.code, Some(2), "{args:?}: {} {}", run.stdout, run.stderr);
    }
}
