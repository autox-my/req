//! The publishing job's script, **run** (ADR 0025 §5, amended by ADR 0032): the `run:` block of the
//! step "publish the release rev-REV" in `.github/workflows/release.yml`, extracted and executed with
//! a fake `gh` and a directory of assets for both targets.
//!
//! `tests/release_workflow.rs` can only pin the workflow's text. This script is the one that writes to a
//! public release and that no asset may ever be replaced by, so the behaviour that matters is run here:
//! a release has **both targets or none**, every check happens **before the first write**, a name that
//! is already on the release is a refusal, and the tarball of each target goes last. The fake `gh` only
//! records what it is asked and keeps the uploaded names in a state file; the oracle is that record and
//! the exit status, written from the ADR, not read back from the script.
//!
//! What this does not show: that GitHub accepts the calls (the first dispatch is the real test), or the
//! two build jobs (they run `scripts/build-release.sh`, which `tests/release_bundle.rs` runs).
#![cfg(unix)]

use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

const LINUX: &str = "x86_64-unknown-linux-gnu";
const MAC: &str = "aarch64-apple-darwin";
const FULL: &str = "6b74f51e2acd726bcf5eb846d73f5219fa8d5957";

/// The script of the publishing step, de-indented the way the runner hands it to bash.
fn publish_script() -> String {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml"),
    )
    .expect("reads the workflow");
    let at = text
        .find("      - name: publish the release rev-REV")
        .expect("the publishing step exists");
    let rest = &text[at..];
    let run = rest
        .find("        run: |\n")
        .expect("the step has a script")
        + "        run: |\n".len();
    let mut script = String::new();
    for line in rest[run..].lines() {
        if line.is_empty() {
            script.push('\n');
        } else if let Some(body) = line.strip_prefix("          ") {
            script.push_str(body);
            script.push('\n');
        } else {
            break; // the next step or the end of the job
        }
    }
    script
}

const FAKE_GH: &str = r#"#!/bin/bash
echo "gh $*" >> "$SIM/calls.log"
case "$1 $2" in
  "release view")
    case "$*" in
      *"--json isDraft"*)
        if [ -f "$SIM/created" ]; then echo false; exit 0; fi
        echo "release not found" >&2; exit 1 ;;
      *"--json assets"*) [ -f "$SIM/state" ] && cat "$SIM/state"; exit 0 ;;
    esac ;;
  "release create") touch "$SIM/created"; : > "$SIM/state"; exit 0 ;;
  "release upload") basename "$4" >> "$SIM/state"; exit 0 ;;
esac
echo "unexpected gh call: $*" >&2; exit 99
"#;

struct Sim {
    dir: PathBuf,
}

impl Sim {
    /// A clean world: assets for both targets at `FULL`, a notes file, a fake `gh`.
    fn new() -> Sim {
        let dir = std::env::temp_dir().join(format!(
            "req-release-publish-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(dir.join("bin")).expect("creates");
        fs::create_dir_all(dir.join("out")).expect("creates");
        fs::create_dir_all(dir.join("notes")).expect("creates");
        fs::write(dir.join("notes/notes.md"), "notes\n").expect("writes");
        fs::write(dir.join("bin/gh"), FAKE_GH).expect("writes");
        fs::set_permissions(dir.join("bin/gh"), fs::Permissions::from_mode(0o755)).expect("chmod");
        let sim = Sim { dir };
        sim.assets(LINUX, "6b74f51", FULL);
        sim.assets(MAC, "6b74f51", FULL);
        sim
    }

    /// The three assets of one target, as `scripts/build-release.sh` writes them.
    fn assets(&self, target: &str, rev: &str, full: &str) {
        let tarball = format!("req-{rev}-{target}.tar.gz");
        let bytes = format!("tarball of {target} at {rev}").into_bytes();
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let out = self.dir.join("out");
        fs::write(out.join(&tarball), &bytes).expect("writes");
        fs::write(
            out.join(format!("{tarball}.sha256")),
            format!("{digest}  {tarball}\n"),
        )
        .expect("writes");
        fs::write(
            out.join(format!("req-{rev}-{target}.build-info")),
            format!("rev={rev}\nsha={full}\ntarget={target}\n"),
        )
        .expect("writes");
    }

    fn remove(&self, name: &str) {
        fs::remove_file(self.dir.join("out").join(name)).expect("removes");
    }

    /// Pretend the release already exists holding these names.
    fn release_holds(&self, names: &[&str]) {
        fs::write(self.dir.join("created"), "").expect("writes");
        fs::write(self.dir.join("state"), names.join("\n") + "\n").expect("writes");
    }

    fn publish(&self) -> (i32, String) {
        let path = format!(
            "{}:{}",
            self.dir.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let run = Command::new("bash")
            .arg("-c")
            .arg(publish_script())
            .current_dir(&self.dir)
            .env("PATH", path)
            .env("SIM", &self.dir)
            .env("GH_TOKEN", "unused")
            .env("GH_REPO", "owner/name")
            .env("REL_FULL", FULL)
            .env("RELEASE_OUT", self.dir.join("out"))
            .env("NOTES_FILE", self.dir.join("notes/notes.md"))
            .output()
            .expect("runs bash");
        let mut log = String::from_utf8_lossy(&run.stdout).into_owned();
        log.push_str(&String::from_utf8_lossy(&run.stderr));
        (run.status.code().unwrap_or(-1), log)
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn uploads(&self) -> Vec<String> {
        self.calls()
            .iter()
            .filter(|c| c.starts_with("gh release upload"))
            .map(|c| c.rsplit('/').next().unwrap_or("").to_owned())
            .collect()
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn names(rev: &str, target: &str) -> [String; 3] {
    [
        format!("req-{rev}-{target}.build-info"),
        format!("req-{rev}-{target}.tar.gz.sha256"),
        format!("req-{rev}-{target}.tar.gz"),
    ]
}

#[test]
fn both_targets_are_published_together_each_tarball_last() {
    let sim = Sim::new();
    let (code, log) = sim.publish();
    assert_eq!(code, 0, "{log}");
    let want: Vec<String> = names("6b74f51", LINUX)
        .into_iter()
        .chain(names("6b74f51", MAC))
        .collect();
    assert_eq!(
        sim.uploads(),
        want,
        "six uploads, each target's tarball after its other two"
    );
    let creates = sim
        .calls()
        .iter()
        .filter(|c| c.starts_with("gh release create"))
        .count();
    assert_eq!(creates, 1, "one release for the rev, however many targets");
    assert!(
        sim.calls().iter().any(|c| c.contains("--notes-file")),
        "the release is created with the generated notes: {:?}",
        sim.calls()
    );
    assert!(log.contains("released rev-6b74f51"), "{log}");
    for target in [LINUX, MAC] {
        assert!(
            log.contains(&format!("pin ({target}): REV=6b74f51 SHA256=")),
            "a pin line per target for the owner to copy: {log}"
        );
    }
}

#[test]
fn a_release_has_both_targets_or_none_and_nothing_is_written_when_one_is_missing() {
    for (label, missing) in [
        ("macOS tarball", names("6b74f51", MAC)[2].clone()),
        ("Linux sidecar", names("6b74f51", LINUX)[1].clone()),
        ("macOS build-info", names("6b74f51", MAC)[0].clone()),
    ] {
        let sim = Sim::new();
        sim.remove(&missing);
        let (code, log) = sim.publish();
        assert_ne!(code, 0, "{label}: {log}");
        assert!(
            sim.calls().is_empty(),
            "{label}: no gh call at all before a refusal: {:?}",
            sim.calls()
        );
    }
}

#[test]
fn a_name_already_on_the_release_stops_everything_before_the_first_upload() {
    for taken in [
        names("6b74f51", MAC)[0].clone(),
        names("6b74f51", LINUX)[2].clone(),
    ] {
        let sim = Sim::new();
        sim.release_holds(&[taken.as_str()]);
        let (code, log) = sim.publish();
        assert_ne!(code, 0, "{taken}: {log}");
        assert!(
            log.contains("already exists on release rev-6b74f51"),
            "{log}"
        );
        assert!(
            sim.uploads().is_empty(),
            "{taken}: nothing uploaded, not even the other target"
        );
    }
}

#[test]
fn two_targets_that_name_different_revs_publish_nothing() {
    let sim = Sim::new();
    for name in names("6b74f51", MAC) {
        sim.remove(&name);
    }
    // Both revs are prefixes of the resolved commit, so every other check passes: only the agreement
    // between the two targets can refuse this (the tag would be one of two names).
    sim.assets(MAC, "6b74f51e", FULL);
    let (code, log) = sim.publish();
    assert_ne!(code, 0, "{log}");
    assert!(log.contains("name different revs"), "{log}");
    assert!(sim.calls().is_empty(), "{:?}", sim.calls());
}

#[test]
fn a_target_built_from_another_commit_than_the_resolved_one_publishes_nothing() {
    let sim = Sim::new();
    for name in names("6b74f51", MAC) {
        sim.remove(&name);
    }
    sim.assets(MAC, "6b74f51", "0000000000000000000000000000000000000000");
    let (code, log) = sim.publish();
    assert_ne!(code, 0, "{log}");
    assert!(log.contains("not the resolved"), "{log}");
    assert!(sim.calls().is_empty(), "{:?}", sim.calls());
}

#[test]
fn a_sidecar_that_disagrees_with_its_tarball_publishes_nothing() {
    let sim = Sim::new();
    fs::write(
        sim.dir.join("out").join(&names("6b74f51", MAC)[1]),
        "deadbeef  req-6b74f51-aarch64-apple-darwin.tar.gz\n",
    )
    .expect("writes");
    let (code, log) = sim.publish();
    assert_ne!(code, 0, "{log}");
    assert!(sim.calls().is_empty(), "{:?}", sim.calls());
}

#[test]
fn an_empty_notes_file_publishes_nothing() {
    let sim = Sim::new();
    fs::write(sim.dir.join("notes/notes.md"), "").expect("writes");
    let (code, log) = sim.publish();
    assert_ne!(code, 0, "{log}");
    assert!(log.contains("refusing to publish without them"), "{log}");
    assert!(sim.calls().is_empty(), "{:?}", sim.calls());
}
