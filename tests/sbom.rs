//! `tools/sbom.py`: the CycloneDX bill of materials of the req binary (ADR 0036).
//!
//! Two seams. The generator is driven in a throwaway repository whose `cargo` is a script printing a fixed
//! dependency graph, so the expectations are written by hand from that graph (what ships, what only builds,
//! what is a dev-dependency) and nothing is compared with the tool's own output. And the committed
//! `docs/sbom.cdx.json` is checked against the real graph, the drift alarm the gate runs.
#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn tool() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/sbom.py")
}

fn sum(c: char) -> String {
    c.to_string().repeat(64)
}

/// The fixture's lock: three registry crates with checksums, one without (a path crate has none).
fn lock() -> String {
    let mut text = String::from("version = 4\n\n");
    for (name, version, checksum) in [
        ("a", "1.0.0", Some('a')),
        ("b", "1.0.0", Some('b')),
        ("d", "0.1.0", Some('d')),
        ("e", "3.0.0", Some('e')),
        ("r", "1.0.0", None),
        ("x", "2.0.0", Some('c')),
    ] {
        text.push_str(&format!(
            "[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n"
        ));
        if let Some(c) = checksum {
            text.push_str(&format!(
                "source = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{}\"\n",
                sum(c)
            ));
        }
        text.push('\n');
    }
    text
}

/// What `cargo tree -e normal,build` prints for the fixture: `r` builds with `b`, uses `a` and `x`, and
/// `a` uses a procedural macro `d` that uses `e`. A dev-dependency `t` is in no tree.
const TREE_ALL: &str = "0r v1.0.0 (/somewhere/r)\n1a v1.0.0\n2d v0.1.0 (proc-macro)\n3e v3.0.0\n2x v2.0.0\n1b v1.0.0\n2x v2.0.0 (*)\n";
/// `cargo tree -e normal,no-proc-macro`: what is linked in.
const TREE_SHIPPED: &str = "0r v1.0.0 (/somewhere/r)\n1a v1.0.0\n2x v2.0.0\n";

fn metadata() -> String {
    let package = |name: &str, version: &str, license: Option<&str>, repository: Option<&str>| {
        serde_json::json!({
            "name": name, "version": version, "license": license, "repository": repository,
        })
    };
    serde_json::json!({ "packages": [
        package("r", "1.0.0", Some("Apache-2.0"), Some("https://example.org/r")),
        package("a", "1.0.0", Some("MIT"), Some("https://example.org/a")),
        package("b", "1.0.0", Some("MIT OR Apache-2.0"), None),
        package("d", "0.1.0", Some("MIT"), None),
        package("e", "3.0.0", None, None),
        package("x", "2.0.0", Some("Apache-2.0"), None),
        package("t", "9.9.9", Some("MIT"), None),
    ]})
    .to_string()
}

static N: AtomicU64 = AtomicU64::new(0);

struct Repo {
    root: PathBuf,
    log: PathBuf,
}

impl Repo {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "req-sbom-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bin")).expect("creates the fixture");
        fs::write(root.join("Cargo.lock"), lock()).expect("writes the lock");
        fs::write(root.join("metadata.json"), metadata()).expect("writes the metadata");
        fs::write(root.join("tree-all.txt"), TREE_ALL).expect("writes a tree");
        fs::write(root.join("tree-shipped.txt"), TREE_SHIPPED).expect("writes a tree");
        let log = root.join("cargo.log");
        // A `cargo` that answers `tree` and `metadata` from files and logs what it was asked.
        let script = "#!/bin/sh\n\
            printf '%s\\n' \"$*\" >> \"$CARGO_LOG\"\n\
            [ -z \"${CARGO_FAIL:-}\" ] || { echo 'cargo: simulated failure' >&2; exit 101; }\n\
            case \"$1\" in\n\
              metadata) cat \"$FIXTURE/metadata.json\" ;;\n\
              tree)\n\
                case \"$*\" in\n\
                  *'-e normal,no-proc-macro'*) cat \"$FIXTURE/tree-shipped.txt\" ;;\n\
                  *'-e normal,build'*) cat \"$FIXTURE/tree-all.txt\" ;;\n\
                  *) echo \"fixture cargo: unexpected tree edges: $*\" >&2; exit 64 ;;\n\
                esac ;;\n\
              *) echo \"fixture cargo: unexpected: $*\" >&2; exit 64 ;;\n\
            esac\n";
        let cargo = root.join("bin/cargo");
        fs::write(&cargo, script).expect("writes the fake cargo");
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755))
            .expect("makes it executable");
        Repo { root, log }
    }

    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> (i32, String, String) {
        let mut command = Command::new("python3");
        command
            .arg("-I")
            .arg(tool())
            .args(args)
            .arg("--root")
            .arg(&self.root)
            .env("CARGO", self.root.join("bin/cargo"))
            .env("CARGO_LOG", &self.log)
            .env("FIXTURE", &self.root);
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let out = command
            .output()
            .expect("runs python3 (the SBOM tool needs it; the gate SKIPs without it)");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn generate(&self) -> Value {
        let target = self.root.join("out.json");
        let (code, _, err) = self.run(&["--out", target.to_str().expect("utf-8")], &[]);
        assert_eq!(code, 0, "{err}");
        serde_json::from_str(&fs::read_to_string(target).expect("reads the SBOM"))
            .expect("valid JSON")
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn by_name<'a>(bom: &'a Value, name: &str) -> &'a Value {
    bom["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no component {name}"))
}

#[test]
fn it_is_a_cyclonedx_1_5_document_for_the_root_application() {
    let bom = Repo::new("shape").generate();
    assert_eq!(bom["bomFormat"], "CycloneDX");
    assert_eq!(bom["specVersion"], "1.5");
    assert_eq!(bom["version"], 1);
    let app = &bom["metadata"]["component"];
    assert_eq!(app["type"], "application");
    assert_eq!(app["name"], "r");
    assert_eq!(app["version"], "1.0.0");
    assert_eq!(app["purl"], "pkg:cargo/r@1.0.0");
    assert_eq!(app["licenses"][0]["expression"], "Apache-2.0");
    // The root is the application, not one of its own components.
    assert!(
        bom["components"]
            .as_array()
            .expect("components")
            .iter()
            .all(|c| c["name"] != "r"),
        "the root appears among its components"
    );
}

#[test]
fn what_ships_is_required_and_what_only_builds_is_excluded_and_a_dev_dependency_is_absent() {
    let bom = Repo::new("scope").generate();
    let names: Vec<&str> = bom["components"]
        .as_array()
        .expect("components")
        .iter()
        .map(|c| c["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        ["a", "b", "d", "e", "x"],
        "sorted; the dev-dependency `t` is not listed"
    );
    for (name, scope) in [
        ("a", "required"),
        ("x", "required"),
        // `x` is also a build-dependency's dependency: still required, because `a` links it.
        ("b", "excluded"),
        // A procedural macro runs in the compiler, and so does what it uses.
        ("d", "excluded"),
        ("e", "excluded"),
    ] {
        assert_eq!(by_name(&bom, name)["scope"], scope, "{name}");
    }
}

#[test]
fn each_component_carries_its_purl_licence_and_the_locks_sha256() {
    let bom = Repo::new("fields").generate();
    let a = by_name(&bom, "a");
    assert_eq!(a["purl"], "pkg:cargo/a@1.0.0");
    assert_eq!(a["bom-ref"], "pkg:cargo/a@1.0.0");
    assert_eq!(a["version"], "1.0.0");
    assert_eq!(a["licenses"][0]["expression"], "MIT");
    assert_eq!(a["hashes"][0]["alg"], "SHA-256");
    assert_eq!(a["hashes"][0]["content"], sum('a'));
    assert_eq!(a["externalReferences"][0]["url"], "https://example.org/a");
    // Hashes follow the right crate (x's checksum in the lock is 'c').
    assert_eq!(by_name(&bom, "x")["hashes"][0]["content"], sum('c'));
    // No licence in the metadata: none is asserted, none is invented.
    assert!(by_name(&bom, "e").get("licenses").is_none());
}

#[test]
fn the_dependency_graph_is_the_trees_edges_sorted() {
    let bom = Repo::new("edges").generate();
    let edges: Vec<(String, Vec<String>)> = bom["dependencies"]
        .as_array()
        .expect("dependencies")
        .iter()
        .map(|d| {
            (
                d["ref"].as_str().expect("ref").to_owned(),
                d["dependsOn"]
                    .as_array()
                    .expect("dependsOn")
                    .iter()
                    .map(|r| r.as_str().expect("ref").to_owned())
                    .collect(),
            )
        })
        .collect();
    let want = |from: &str, to: &[&str]| {
        (
            format!("pkg:cargo/{from}"),
            to.iter()
                .map(|t| format!("pkg:cargo/{t}"))
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        edges,
        vec![
            want("a@1.0.0", &["d@0.1.0", "x@2.0.0"]),
            want("b@1.0.0", &["x@2.0.0"]),
            want("d@0.1.0", &["e@3.0.0"]),
            want("e@3.0.0", &[]),
            want("r@1.0.0", &["a@1.0.0", "b@1.0.0"]),
            want("x@2.0.0", &[]),
        ]
    );
}

#[test]
fn the_same_inputs_give_the_same_bytes_and_a_changed_lock_gives_another_serial() {
    let repo = Repo::new("determinism");
    let first = fs::read_to_string({
        repo.generate();
        repo.root.join("out.json")
    })
    .expect("reads");
    repo.generate();
    let second = fs::read_to_string(repo.root.join("out.json")).expect("reads");
    assert_eq!(first, second, "no timestamp, no random serial");
    assert!(
        !first.contains("timestamp"),
        "a clock would make the file differ on every run"
    );

    let serial = |text: &str| -> String {
        let v: Value = serde_json::from_str(text).expect("JSON");
        v["serialNumber"].as_str().expect("serial").to_owned()
    };
    let shape = |s: &str| {
        let parts: Vec<usize> = s
            .strip_prefix("urn:uuid:")
            .expect("urn")
            .split('-')
            .map(str::len)
            .collect();
        parts == [8, 4, 4, 4, 12] && s.as_bytes()["urn:uuid:".len() + 14] == b'5'
    };
    assert!(
        shape(&serial(&first)),
        "a UUID of version 5: {}",
        serial(&first)
    );

    fs::write(
        repo.root.join("Cargo.lock"),
        format!("{}# changed\n", lock()),
    )
    .expect("edits the lock");
    repo.generate();
    let third = fs::read_to_string(repo.root.join("out.json")).expect("reads");
    assert_ne!(
        serial(&first),
        serial(&third),
        "the serial follows the lock"
    );
}

#[test]
fn it_asks_cargo_for_both_release_targets_offline_and_locked() {
    let repo = Repo::new("argv");
    repo.generate();
    let log = fs::read_to_string(&repo.log).expect("cargo was run");
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        assert!(
            log.lines()
                .any(|l| l.starts_with("tree") && l.contains(&format!("--target {target}"))),
            "no tree for {target}:\n{log}"
        );
    }
    for line in log.lines() {
        assert!(
            line.contains("--locked") && line.contains("--offline"),
            "a cargo call that could touch the network or move the lock: {line}"
        );
    }
}

#[test]
fn check_is_current_for_the_file_it_wrote_and_stale_for_any_other() {
    let repo = Repo::new("check");
    repo.generate();
    let file = repo.root.join("out.json");
    let path = file.to_str().expect("utf-8");
    let (code, out, err) = repo.run(&["--check", path], &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("CURRENT"), "{out}");

    // A dependency the graph no longer has, as a stale file shows it.
    let text = fs::read_to_string(&file).expect("reads");
    fs::write(
        &file,
        text.replace("\"name\": \"x\"", "\"name\": \"y\"")
            .replace("pkg:cargo/x@2.0.0", "pkg:cargo/y@2.0.0"),
    )
    .expect("edits");
    let (code, _, err) = repo.run(&["--check", path], &[]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("STALE"), "{err}");
    assert!(
        err.contains("added   pkg:cargo/x@2.0.0") && err.contains("removed pkg:cargo/y@2.0.0"),
        "the stale file is told what moved: {err}"
    );
    assert!(
        err.contains("tools/sbom.py --out"),
        "and how to fix it: {err}"
    );

    fs::remove_file(&file).expect("removes");
    let (code, _, err) = repo.run(&["--check", path], &[]);
    assert_eq!(code, 1, "a missing file is stale, not a pass: {err}");
}

#[test]
fn a_graph_that_cannot_be_read_is_unverifiable_never_a_match() {
    let repo = Repo::new("unverifiable");
    let file = repo.root.join("out.json");
    repo.generate();
    let path = file.to_str().expect("utf-8");

    let (code, _, err) = repo.run(&["--check", path], &[("CARGO_FAIL", "1")]);
    assert_eq!(code, 3, "cargo failing is UNVERIFIABLE: {err}");
    assert!(err.contains("UNVERIFIABLE"), "{err}");

    fs::write(repo.root.join("Cargo.lock"), "this is = not [toml").expect("breaks the lock");
    let (code, _, err) = repo.run(&["--check", path], &[]);
    assert_eq!(code, 3, "an unreadable lock is UNVERIFIABLE: {err}");

    fs::remove_file(repo.root.join("Cargo.lock")).expect("removes the lock");
    let (code, _, err) = repo.run(&["--out", path], &[]);
    assert_eq!(code, 3, "a missing lock is UNVERIFIABLE: {err}");
}

#[test]
fn a_package_in_the_tree_but_not_in_the_metadata_is_unverifiable() {
    let repo = Repo::new("missing-package");
    let md = fs::read_to_string(repo.root.join("metadata.json")).expect("reads");
    fs::write(
        repo.root.join("metadata.json"),
        md.replace("\"name\":\"x\"", "\"name\":\"xx\""),
    )
    .expect("edits");
    let (code, _, err) = repo.run(
        &["--out", repo.root.join("o.json").to_str().expect("utf-8")],
        &[],
    );
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("not in `cargo metadata`"), "{err}");
}

#[test]
fn usage_errors_are_exit_2() {
    let repo = Repo::new("usage");
    for args in [
        vec![],
        vec!["--out"],
        vec!["--out", "a", "--check", "b"],
        vec!["--no-such-flag"],
    ] {
        let (code, _, err) = repo.run(&args, &[]);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
}

#[test]
fn the_committed_sbom_is_current_for_this_repository() {
    // The drift alarm the gate runs, on the real graph: change a dependency or Cargo.lock and this fails
    // until `python3 tools/sbom.py --out docs/sbom.cdx.json` is run in the same commit.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg("-I")
        .arg(tool())
        .arg("--check")
        .arg(root.join("docs/sbom.cdx.json"))
        .arg("--root")
        .arg(root)
        .env(
            "CARGO",
            std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
        )
        .output()
        .expect("runs python3");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_real_sbom_lists_nothing_the_binary_does_not_link_that_the_network_ban_names() {
    // REQ-0012 from the other side: the bill of materials, read independently as JSON, has no network
    // client or async runtime among the components that ship.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bom: Value =
        serde_json::from_str(&fs::read_to_string(root.join("docs/sbom.cdx.json")).expect("reads"))
            .expect("JSON");
    let required: Vec<&str> = bom["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter(|c| c["scope"] == "required")
        .map(|c| c["name"].as_str().expect("name"))
        .collect();
    for banned in [
        "reqwest", "hyper", "tokio", "openssl", "rustls", "mio", "socket2", "bytes",
    ] {
        assert!(!required.contains(&banned), "{banned} ships: {required:?}");
    }
    for expected in [
        "serde",
        "serde_json",
        "yaml_serde",
        "rust_decimal",
        "sha2",
        "unicode-normalization",
    ] {
        assert!(
            required.contains(&expected),
            "the direct dependency {expected} is missing: {required:?}"
        );
    }
}
