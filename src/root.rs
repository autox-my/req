//! The corpus root, resolved at runtime (ADR 0012).
//!
//! req manages *any* repository's claims: the root is an invocation parameter, like a
//! variant, and never a compile-time path. The rule is short — `--root`, then `REQ_ROOT`,
//! then the nearest ancestor of the working directory holding a `claims/` directory — and
//! it lives here as a pure function of its arguments, so precedence is testable without
//! `set_current_dir` or env mutation (both process-global, and both fatal to a parallel
//! test run).

use std::path::{Path, PathBuf};

/// The name that marks a corpus root. A **directory**: a file called `claims` is not a
/// corpus, and a root that yielded no corpus must fail loudly rather than read nothing.
const CLAIMS_DIR: &str = "claims";

/// The remedy named by every failure, so a caller who lands here knows the two ways out.
const REMEDY: &str = "pass --root or set REQ_ROOT";

/// Split `--root <dir>` / `--root=<dir>` out of argv, leaving the subcommand and its own
/// arguments otherwise untouched.
///
/// The root is the *tool's* parameter, not any one subcommand's, so it is stripped once
/// here instead of being taught to thirteen call sites — and a subcommand's own flag parse
/// keeps saying `usage:` for anything it does not know.
pub fn take_root_arg(args: &[String]) -> Result<(Option<PathBuf>, Vec<String>), String> {
    let mut root: Option<PathBuf> = None;
    let mut rest: Vec<String> = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        index += 1;
        if arg == "--root" {
            let value = args
                .get(index)
                .ok_or_else(|| format!("usage: --root <dir> requires a directory; {REMEDY}"))?;
            index += 1;
            if value.is_empty() {
                return Err(format!(
                    "usage: --root <dir> requires a directory; {REMEDY}"
                ));
            }
            root = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--root=") {
            if value.is_empty() {
                return Err(format!(
                    "usage: --root=<dir> requires a directory; {REMEDY}"
                ));
            }
            root = Some(PathBuf::from(value));
        } else {
            rest.push(arg.clone());
        }
    }
    Ok((root, rest))
}

/// Resolve the corpus root, in ADR 0012's order: `cli`, then `env_req_root`, then the
/// nearest ancestor of `start()` containing a `claims/` directory.
///
/// Pure: the working directory and the environment are parameters, never read here. `start`
/// is a **thunk**, not a path, because the walk-up is the only branch that needs it: a root
/// that was named must resolve without a readable working directory, and reading it eagerly
/// made an unusable CWD fatal before `--root` or `REQ_ROOT` had been consulted at all.
///
/// A root that was *named* (`--root` or `REQ_ROOT`) and holds no `claims/` is an error,
/// never a silent fallback to the walk-up: a root that was asked for and cannot serve the
/// corpus must fail loudly, which is the failure mode this ADR exists to remove.
pub fn resolve_corpus_root(
    start: impl FnOnce() -> std::io::Result<PathBuf>,
    cli: Option<&Path>,
    env_req_root: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(cli) = cli {
        return validated(cli, &format!("--root {}", cli.display()));
    }
    // An empty `REQ_ROOT` is *unset*, not a root: `Path::new("").join("claims")` collapses to
    // a bare `claims/`, which is a path against whatever the working directory happens to be —
    // a corpus nobody asked for, and an empty `corpus root:` line. Unset falls through to the
    // walk-up, which is what an absent value does.
    if let Some(value) = env_req_root.filter(|value| !value.is_empty()) {
        return validated(Path::new(value), &format!("REQ_ROOT={value}"));
    }
    match start() {
        Ok(start) => match start.ancestors().find(|dir| is_corpus_root(dir)) {
            Some(found) => Ok(found.to_path_buf()),
            None => Err(format!(
                "no claims/ directory at or above {} — {REMEDY}",
                start.display()
            )),
        },
        // Nothing named a root and there is no working directory to walk up from: the
        // question is unanswerable, and the remedy is the one every failure names.
        Err(e) => Err(format!(
            "cannot read the working directory to search for claims/: {e} — {REMEDY}"
        )),
    }
}

/// Whether `dir` is a corpus root: it holds a `claims/` directory.
pub fn is_corpus_root(dir: &Path) -> bool {
    dir.join(CLAIMS_DIR).is_dir()
}

fn validated(dir: &Path, source: &str) -> Result<PathBuf, String> {
    if is_corpus_root(dir) {
        Ok(dir.to_path_buf())
    } else {
        Err(format!(
            "{source} is not a corpus root: {} has no claims/ directory — {REMEDY}",
            dir.display()
        ))
    }
}
