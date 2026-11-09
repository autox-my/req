//! `req pin` — a consumer repository pins the commit of an **external corpus** it is gated against
//! (ADR 0033, issue #16).
//!
//! ADR 0012 lets the corpus root be anywhere; nothing said *which commit* of it a green gate was
//! about. The pin is a small file in the consumer's repository naming a full commit SHA. Two modes:
//!
//! * `--verify <file>` — the corpus at the resolved root is exactly that commit, and `claims/` and
//!   `aliases/` have no uncommitted change. A gate run after this passes is a statement about the
//!   pinned policy and about nothing else. 0 verified, 1 the corpus is not the pin, 3 it cannot be told.
//! * `--drift <file> --against <rev>` — how the corpus moved from the pin to `<rev>`: exactly the
//!   report `req diff --base <pin> --head <rev>` prints, per claim with its direction, never a verdict.
//!
//! `req` never fetches. Getting the commit into the checkout is the consumer's CI (`git fetch`,
//! `git checkout <sha>`), the way it already installs everything else; this command only refuses to
//! let a corpus that is not the pin be taken for it. Git is a local process with every `GIT_*`
//! redirection removed (`diff`'s boundary, reused).

use std::path::Path;

use crate::diff;

const USAGE: &str = "usage: req pin --verify <file>\n       req pin --drift <file> --against <rev>";

/// The pin file, parsed: an optional human label for the repository and the full commit SHA.
struct Pin {
    repo: Option<String>,
    rev: String,
}

/// A full, lower-case commit SHA: 40 hex digits. A short SHA is ambiguous and a name moves.
fn is_full_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Parse a pin file. Strict: blank lines and `#` comments are allowed; every other line is
/// `repo: <text>` or `rev: <40 hex>`, each at most once, `rev` required. Anything else is an error.
fn parse(text: &str) -> Result<Pin, String> {
    let (mut repo, mut rev): (Option<String>, Option<String>) = (None, None);
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let place = number + 1;
        let Some((key, value)) = line.split_once(':') else {
            return Err(format!(
                "line {place}: expected `repo: …` or `rev: <40 hex>`, found {line:?}"
            ));
        };
        let value = value.trim();
        let slot = match key.trim() {
            "repo" => &mut repo,
            "rev" => &mut rev,
            other => {
                return Err(format!(
                    "line {place}: unknown key {other:?} (the pin has `repo` and `rev`)"
                ));
            }
        };
        if value.is_empty() {
            return Err(format!("line {place}: `{}` has no value", key.trim()));
        }
        if slot.replace(value.to_string()).is_some() {
            return Err(format!("line {place}: `{}` is given twice", key.trim()));
        }
    }
    let rev = rev.ok_or("no `rev:` line — the pin names the commit")?;
    if !is_full_sha(&rev) {
        return Err(format!(
            "`rev` must be a full 40-digit lower-case commit SHA, not {rev:?} (a short SHA is ambiguous, a branch or tag moves)"
        ));
    }
    Ok(Pin { repo, rev })
}

fn read_pin(path: &str) -> Result<Pin, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read the pin file {path}: {e}"))?;
    parse(&text).map_err(|e| format!("{path}: {e}"))
}

enum Mode {
    Verify { file: String },
    Drift { file: String, against: String },
}

fn parse_args(args: &[String]) -> Result<Mode, String> {
    let usage = |why: &str| Err(format!("{USAGE}\n{why}"));
    let Some(first) = args.first() else {
        return usage("a mode is required");
    };
    match first.as_str() {
        "--verify" => match &args[1..] {
            [file] if !file.starts_with('-') => Ok(Mode::Verify { file: file.clone() }),
            [] => usage("`--verify` needs a pin file"),
            _ => usage("`--verify` takes exactly one pin file"),
        },
        "--drift" => match &args[1..] {
            [file, flag, against] if flag == "--against" && !file.starts_with('-') => {
                if against.is_empty() || against.starts_with('-') {
                    return usage("`--against` needs a revision, not an option");
                }
                Ok(Mode::Drift {
                    file: file.clone(),
                    against: against.clone(),
                })
            }
            [_, flag] if flag == "--against" => usage("`--against` needs a revision"),
            _ => usage("`--drift` needs `<file> --against <rev>`"),
        },
        other => usage(&format!("unrecognized argument: {other}")),
    }
}

fn label(pin: &Pin) -> String {
    match &pin.repo {
        Some(repo) => format!("{repo} @ {}", pin.rev),
        None => format!("@ {}", pin.rev),
    }
}

/// The pinned commit as a revision in this checkout, or why it cannot be one.
fn pinned_commit(root: &Path, pin: &Pin) -> Result<(), String> {
    let resolved = match diff::resolve(root, &pin.rev) {
        Ok(resolved) => resolved,
        Err(_) => {
            // An object that is here but is not a commit (a tree, a blob) is a different mistake from
            // a commit that was never fetched, and the remedy differs.
            return Err(
                match diff::git_output(root, &["cat-file", "-t", &pin.rev]) {
                    Ok(kind) => format!(
                        "{} is not a commit (it is a {}): a pin names a commit",
                        pin.rev,
                        String::from_utf8_lossy(&kind).trim()
                    ),
                    Err(_) => format!(
                        "the pinned commit {} is not in this checkout — fetch it first (`git fetch`, and a shallow clone needs `fetch-depth: 0`); req never fetches",
                        pin.rev
                    ),
                },
            );
        }
    };
    if resolved.sha != pin.rev {
        return Err(format!(
            "{} is not a commit (it resolves to {}): a pin names a commit, not a tag object or a tree",
            pin.rev, resolved.sha
        ));
    }
    Ok(())
}

fn verify(root: &Path, pin: &Pin) -> Result<i32, String> {
    diff::work_tree_prefix(root)?;
    pinned_commit(root, pin)?;
    let head = diff::resolve(root, "HEAD")?.sha;

    let mut findings: Vec<String> = Vec::new();
    if head != pin.rev {
        let counts = diff::git_output(
            root,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("{}...{head}", pin.rev),
            ],
        )?;
        let counts = String::from_utf8_lossy(&counts).into_owned();
        let mut numbers = counts.split_whitespace();
        let (behind, ahead) = (numbers.next().unwrap_or("?"), numbers.next().unwrap_or("?"));
        let mut how = Vec::new();
        if ahead != "0" {
            how.push(format!("{ahead} commit(s) ahead of it"));
        }
        if behind != "0" {
            how.push(format!("{behind} commit(s) behind it"));
        }
        findings.push(format!(
            "the corpus is at {}, the pin says {}: {}",
            &head[..12],
            &pin.rev[..12],
            how.join(" and ")
        ));
    }
    let dirty = diff::git_output(
        root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            "claims",
            "aliases",
        ],
    )?;
    let dirty = String::from_utf8_lossy(&dirty).into_owned();
    if !dirty.trim().is_empty() {
        let files: Vec<String> = dirty
            .lines()
            .map(|l| l.get(3..).unwrap_or(l).trim().to_string())
            .collect();
        findings.push(format!(
            "uncommitted change under claims/ or aliases/ — a gate would judge something other than the pin: {}",
            files.join(", ")
        ));
    }
    if findings.is_empty() {
        println!("corpus pinned: {}", label(pin));
        return Ok(0);
    }
    for finding in &findings {
        println!("[FAIL] {finding}");
    }
    println!("pin: {}", label(pin));
    Ok(1)
}

fn drift(root: &Path, pin: &Pin, against: &str) -> Result<i32, String> {
    diff::work_tree_prefix(root)?;
    pinned_commit(root, pin)?;
    let report = diff::report_text(root, &pin.rev, against)?;
    println!("pin drift: {} -> {against}", label(pin));
    print!("{report}");
    Ok(0)
}

/// `req pin` — see the module doc and `PIN_HELP` in `main.rs`.
pub fn run(args: &[String], root: &Path) -> i32 {
    let mode = match parse_args(args) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    let outcome = match &mode {
        Mode::Verify { file } => read_pin(file).and_then(|pin| verify(root, &pin)),
        Mode::Drift { file, against } => read_pin(file).and_then(|pin| drift(root, &pin, against)),
    };
    match outcome {
        Ok(code) => code,
        Err(cause) => {
            eprintln!("[UNVERIFIABLE] {cause}");
            3
        }
    }
}
