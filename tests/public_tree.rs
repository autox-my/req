//! The tree is publishable (ADR 0029, unit 138-1): what `git ls-files` lists says nothing a public
//! reader should not see, and the licence files a public repository owes are there.
//!
//! Independent of every other test: the oracle is the list of strings written below, not anything
//! read from the workflows, the docs or the code. It looks at the **tracked** tree, because that is
//! what a push publishes; an untracked scratch file is not its business.
//!
//! The history-bearing places are exempt from the string scan, and only those: `docs/adr/`,
//! `docs/specs/` and `docs/handoffs/` (old numbers and old URLs are history, ADR 0029 §8),
//! `bench/results/` (append-only), `experiments/`, `prompts/`, `bench/corpus/` (generated data),
//! and `tests/` (tests assert the absence of these strings, so they have to name them).

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every tracked path, from git itself. A failure to ask git is a failure of this test: a tree that
/// cannot be listed cannot be called clean.
fn tracked() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repo_root())
        .output()
        .expect("runs git ls-files");
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("tracked paths are UTF-8")
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The places allowed to carry the old host, the old paths and the like.
const EXEMPT: [&str; 8] = [
    "docs/adr/",
    "docs/specs/",
    "docs/handoffs/",
    "bench/results/",
    "experiments/",
    "prompts/",
    "tests/",
    "bench/corpus/",
];

/// The literal strings no live file may contain.
const FORBIDDEN: [&str; 4] = ["xantu-barb", ".ts.net", "pct exec", "/Users/"];

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|error| panic!("{rel} must exist and be readable: {error}"))
}

/// Does `text` hold an IPv4 address `100.x.y.z` in `100.64.0.0/10` (the tailnet range, 100.64.0.0
/// to 100.127.255.255)? Written out by hand: no regex dependency, and no float.
fn holds_cgnat_address(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(at) = text[from..].find("100.") {
        let start = from + at;
        from = start + 1;
        // Not the tail of a longer number or a longer dotted run (`2100.`, `1.100.`).
        if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'.') {
            continue;
        }
        let rest = &text[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim_end_matches('.');
        let octets: Vec<&str> = candidate.split('.').collect();
        if octets.len() != 4 {
            continue;
        }
        let numbers: Option<Vec<u16>> = octets
            .iter()
            .map(|octet| {
                if octet.is_empty() || octet.len() > 3 {
                    None
                } else {
                    octet.parse::<u16>().ok()
                }
            })
            .collect();
        if let Some(numbers) = numbers
            && numbers.iter().all(|n| *n <= 255)
            && (64..=127).contains(&numbers[1])
        {
            return Some(candidate.to_owned());
        }
    }
    None
}

#[test]
fn the_address_scan_knows_the_tailnet_range() {
    // The scan's own oracle: a check that cannot fire is not a check.
    for hit in ["100.64.0.1", "ssh 100.101.102.103 x", "at 100.127.255.254."] {
        assert!(holds_cgnat_address(hit).is_some(), "{hit} is in 100.64/10");
    }
    for miss in [
        "100.63.255.255",
        "100.128.0.1",
        "1100.64.0.1",
        "1.100.64.0.1",
        "100.64.0",
        "100.64.0.256",
        "version 100.0.0.1",
    ] {
        assert!(
            holds_cgnat_address(miss).is_none(),
            "{miss} is not in 100.64/10"
        );
    }
}

#[test]
fn no_tracked_path_is_a_forgejo_workflow() {
    let offenders: Vec<String> = tracked()
        .into_iter()
        .filter(|path| path.starts_with(".forgejo/"))
        .collect();
    assert!(
        offenders.is_empty(),
        "the public tree carries no .forgejo/ (ADR 0029 §7): {offenders:?}"
    );
}

#[test]
fn no_live_file_names_the_private_host_the_runner_or_a_local_path() {
    let mut hits = Vec::new();
    for path in tracked() {
        if EXEMPT.iter().any(|prefix| path.starts_with(prefix)) {
            continue;
        }
        // A tracked file that is not there (deleted, not yet committed) or not text has no words.
        let Ok(bytes) = std::fs::read(repo_root().join(&path)) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for needle in FORBIDDEN {
            if text.contains(needle) {
                hits.push(format!("{path}: {needle:?}"));
            }
        }
        if text.contains("FORGEJO") {
            hits.push(format!("{path}: \"FORGEJO\""));
        }
        if let Some(address) = holds_cgnat_address(&text) {
            hits.push(format!("{path}: tailnet address {address}"));
        }
    }
    assert!(
        hits.is_empty(),
        "a public reader would see what only the owner's setup should know (ADR 0029):\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_licence_files_a_public_repository_owes_are_there() {
    let licence = read("LICENSE");
    assert!(
        licence.trim_start().starts_with("Apache License"),
        "LICENSE starts with the Apache-2.0 title"
    );
    for path in ["LICENSES/LGPL-3.0.txt", "LICENSES/GPL-3.0.txt", ".mailmap"] {
        assert!(
            repo_root().join(path).is_file(),
            "{path} must exist (NOTICE ships the LGPL and the GPL it supplements)"
        );
    }
    let notice = read("NOTICE");
    for name in ["PROMISE", "StrictDoc", "Doorstop"] {
        assert!(
            notice.contains(name),
            "NOTICE must name {name}, the third-party data the repository keeps"
        );
    }
    let manifest = read("Cargo.toml");
    assert!(
        manifest
            .lines()
            .any(|line| line.trim() == "license = \"Apache-2.0\""),
        "Cargo.toml says license = \"Apache-2.0\""
    );
}
