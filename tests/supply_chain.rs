//! The dependency checks of ADR 0036: `deny.toml`, the pinned cargo-deny in CI and in the scheduled
//! advisory workflow, and the gate steps that run them.
//!
//! What a test can show about a policy file and two workflows is what a careless edit would silently lose,
//! so these are pins, in the style of `tests/release_workflow.rs`: the text, with whole-line comments
//! removed. Each was mutation-checked by making the edit that must turn it red. **None of it shows a
//! green GitHub run**, and none of it runs `cargo deny`; the policy itself was exercised by hand, the way
//! the ADR records (ban a crate that is in the graph, drop a licence that is needed).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CI: &str = ".github/workflows/ci.yml";
const AUDIT: &str = ".github/workflows/audit.yml";
const DENY: &str = "deny.toml";
const CHECK: &str = "scripts/check.sh";

fn read(rel: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{rel} must exist and be readable: {error}"))
}

/// The file without its whole-line comments.
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `NAME=value` as the pin steps write it (`VERSION=0.20.2`), the first occurrence.
fn assignment(text: &str, name: &str) -> String {
    let key = format!("{name}=");
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&key))
        .unwrap_or_else(|| panic!("no `{key}` line"));
    line[key.len()..].trim_matches('"').to_owned()
}

fn quoted_after(text: &str, marker: &str) -> BTreeSet<String> {
    let start = text.find(marker).unwrap_or_else(|| panic!("no `{marker}`"));
    let rest = &text[start..];
    let end = rest
        .find("];")
        .or_else(|| rest.find(']'))
        .expect("a closing bracket");
    rest[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_advisory_workflow_is_scheduled_or_manual_never_a_pull_request_and_holds_no_secret() {
    let text = code(AUDIT);
    assert!(
        text.contains("schedule:") && text.contains("workflow_dispatch:"),
        "{text}"
    );
    for forbidden in [
        "pull_request",
        "pull_request_target",
        "push:",
        "workflow_run",
        "secrets.",
        "continue-on-error",
        "|| true",
        "permissions:\n  contents: write",
    ] {
        assert!(
            !text.contains(forbidden),
            "audit.yml must not contain `{forbidden}`"
        );
    }
    assert!(
        text.contains("permissions:\n  contents: read"),
        "read-only: {text}"
    );
    assert_eq!(
        text.matches("contents:").count(),
        1,
        "one permission block, read-only, for the whole workflow"
    );
    assert!(
        text.contains("cargo deny --locked check advisories"),
        "the one question this workflow asks"
    );
    assert!(
        text.contains("RUSTUP_TOOLCHAIN: stable"),
        "the pinned channel is not downloaded just to read a lock"
    );
}

#[test]
fn the_gate_never_runs_advisories_so_it_stays_offline() {
    let check = code(CHECK);
    let line = check
        .lines()
        .find(|l| l.trim_start().starts_with("run cargo deny"))
        .expect("the gate runs cargo deny");
    assert!(
        line.contains("--offline") && line.contains("check bans licenses sources"),
        "{line}"
    );
    assert!(
        !check.lines().any(|l| l.contains("cargo deny")
            && !l.contains("--version")
            && l.contains("advisories")),
        "advisories read a database over the network: they belong in audit.yml (REQ-0012)"
    );
}

#[test]
fn the_pin_is_one_version_and_one_hash_in_ci_the_audit_and_the_gate() {
    let (ci, audit, check) = (code(CI), code(AUDIT), code(CHECK));
    for name in ["VERSION", "SHA256", "TARGET", "api"] {
        let in_ci = {
            let from = ci.find("cargo-deny").expect("CI installs cargo-deny");
            assignment(&ci[from.saturating_sub(400)..], name)
        };
        assert_eq!(
            in_ci,
            assignment(&audit, name),
            "{name} differs between ci.yml and audit.yml"
        );
    }
    let version = assignment(&audit, "VERSION");
    assert_eq!(
        assignment(&check, "DENY_PIN"),
        version,
        "the gate checks the version CI installs"
    );
    let sha = assignment(&audit, "SHA256");
    assert!(
        sha.len() == 64 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "{sha}"
    );
    assert!(
        audit.contains("sha256sum -c -"),
        "the hash is checked, not just recorded"
    );
}

#[test]
fn ci_fetches_the_whole_lock_before_the_gate_reads_the_graph_offline() {
    let ci = code(CI);
    let fetch = ci
        .find("cargo fetch --locked")
        .expect("CI fetches every crate in the lock");
    let gate = ci.find("bash scripts/check.sh").expect("CI runs the gate");
    assert!(fetch < gate, "the fetch must come first");
    assert!(
        ci.find("cargo-deny\" --version")
            .expect("the tool is installed")
            < gate,
        "and so must the install"
    );
}

#[test]
fn the_policy_bans_every_network_crate_the_direct_dependency_test_names() {
    let deny = read(DENY);
    let banned: BTreeSet<String> = deny
        .lines()
        .filter_map(|l| l.trim().strip_prefix("{ crate = \""))
        .filter_map(|l| l.split('"').next())
        .map(str::to_owned)
        .collect();
    let evidence = read("tests/evidence.rs");
    let named = quoted_after(&evidence, "const NETWORK_OR_ASYNC");
    assert!(named.len() >= 15, "the reader found the list: {named:?}");
    let missing: Vec<_> = named.difference(&banned).collect();
    assert!(
        missing.is_empty(),
        "tests/evidence.rs bans these in the direct dependencies; deny.toml must ban them anywhere in the graph: {missing:?}"
    );
}

#[test]
fn the_policy_judges_the_shipped_targets_trusts_only_crates_io_and_allows_no_copyleft() {
    let deny = code(DENY);
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        assert!(
            deny.contains(&format!("\"{target}\"")),
            "{target} is a release target (ADR 0025)"
        );
    }
    assert!(
        deny.contains("unknown-registry = \"deny\"") && deny.contains("unknown-git = \"deny\""),
        "{deny}"
    );
    assert!(
        deny.contains("https://github.com/rust-lang/crates.io-index"),
        "{deny}"
    );
    let allowed = quoted_after(&deny, "allow = [");
    for licence in ["Apache-2.0", "MIT"] {
        assert!(
            allowed.contains(licence),
            "{licence} must be allowed: {allowed:?}"
        );
    }
    for copyleft in ["GPL", "AGPL", "LGPL", "MPL", "EPL", "CDDL", "SSPL"] {
        assert!(
            !allowed.iter().any(|l| l.contains(copyleft)),
            "{copyleft} is not compatible with the simple permissive posture of ADR 0036: {allowed:?}"
        );
    }
    assert!(
        deny.contains("yanked = \"deny\""),
        "a yanked crate is a failure"
    );
}

#[test]
fn the_gate_checks_the_bill_of_materials_like_a_projection() {
    let check = code(CHECK);
    assert!(
        check.contains("tools/sbom.py\" --check docs/sbom.cdx.json"),
        "the SBOM is drift-checked in every run"
    );
    assert!(
        check.contains("skip \"python3 or tools/sbom.py missing"),
        "a missing python3 is a SKIP, never a pass"
    );
}
