//! The build identity: `REQ_BUILD_SHA`, captured from git at build time and reported by
//! `req --version` as the `built from` line.
//!
//! Why a build script at all: semver cannot answer the question this exists for. Two commits
//! both carry the crate's `0.1.0`, so a binary that reports only its version cannot say which
//! revision it is — which is how a worktree binary built before its branch's last commit once
//! produced a confident, detailed and entirely false defect report against green code.
//!
//! The contract, in the order of the traps that decide whether the line is *trustworthy*
//! rather than merely present:
//!
//! 1. **Never fail the build.** No `.git`, no `git`, a shallow clone, a source tarball or a
//!    git that errors all yield `unknown`. A build script that can fail breaks every build for
//!    the sake of one provenance line, so the value is emitted unconditionally — `unknown`
//!    included, never omitted (an absent line reads as "not applicable", a different claim).
//! 2. **A stale identity is worse than none.** A value naming a commit the binary was not
//!    built from is *confidently wrong* — precisely the failure this exists to remove. Cargo
//!    re-runs a build script only for the paths it is told about, so `<gitdir>/HEAD` is named,
//!    resolved with `git rev-parse --git-dir` and **never** a hardcoded `.git`: in a worktree
//!    `.git` is a *file* pointing at the real gitdir, so a hardcoded path would watch nothing
//!    and leave the identity stale forever. Every tracked file is named too, or the `-dirty`
//!    marker would survive the edit that should have set it.
//! 3. **Dirty means "tracked files modified"**: `git status --porcelain --untracked-files=no`.
//!    A plain `--porcelain` marks any checkout holding an untracked file dirty forever, and an
//!    always-on `-dirty` trains the reader to ignore the marker.
//! 4. **No new dependency**: `std::process::Command`, not `git2` — a build-graph dependency for
//!    one version string, in a crate that links no cryptography and needs no network. This
//!    script reads local files and local git state only; it fetches nothing.
//! 5. **It must never reach an identity.** The SHA is *printed*, never hashed: it is not in
//!    `canonical_bytes`, not in an address, not in any digest. Nothing here touches the model,
//!    and `req canon --verify` staying green on the committed corpus is the proof.

use std::process::Command;

/// The honest answer when git cannot supply one.
const UNKNOWN: &str = "unknown";

fn main() {
    // Unconditional, so `env!("REQ_BUILD_SHA")` always resolves in `src/main.rs`.
    println!("cargo:rustc-env=REQ_BUILD_SHA={}", build_identity());
    watch_inputs();
}

/// The short HEAD sha, suffixed `-dirty` when a tracked file is modified, or `unknown`.
/// Never an error: every failure path here is a build that must still succeed.
fn build_identity() -> String {
    let Some(head) = git(&["rev-parse", "--short", "HEAD"]) else {
        return UNKNOWN.to_string();
    };
    if head.is_empty() {
        return UNKNOWN.to_string();
    }
    let dirty =
        git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|out| !out.is_empty());
    if dirty { format!("{head}-dirty") } else { head }
}

/// What cargo must watch for this script to re-run. Where the identity *derives from* git
/// state that does not change when a file does — and vice versa — both are named:
///
/// * the git metadata a commit or a checkout rewrites;
/// * every tracked path, because a working-tree edit changes no git metadata at all, and a
///   `-dirty` marker that lags the edit is the same stale-identity failure wearing a badge.
///
/// Untracked paths are deliberately not watched: they never make the identity dirty, so
/// watching them would only rebuild the world whenever a scratch file is written.
///
/// **`HEAD` alone is not enough, and that was measured.** `<gitdir>/HEAD` is a *symref*
/// (`ref: refs/heads/<branch>`); a commit moves the branch it points at, not the symref, so
/// after `git commit --allow-empty` both `<gitdir>/HEAD` and `<gitdir>/index` keep their mtime
/// while `<gitdir>/logs/HEAD` and the loose ref file move. Relying on `HEAD` therefore leaves
/// `built from` stale after exactly the commits that change no tracked file — an amended
/// message, a re-run of the same tree — which is the confidently-wrong value this script exists
/// to remove. The reflog is what every HEAD-moving operation appends to; the loose ref is
/// watched beside it, because a repository with reflogs switched off still rewrites that.
fn watch_inputs() {
    if let Some(gitdir) = git(&["rev-parse", "--git-dir"])
        && !gitdir.is_empty()
    {
        let gitdir = std::path::Path::new(&gitdir);
        watch_if_present(&gitdir.join("HEAD"));
        watch_if_present(&gitdir.join("logs/HEAD"));
        if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
            && !reference.is_empty()
            && let Some(resolved) = git(&["rev-parse", "--git-path", &reference])
        {
            watch_if_present(std::path::Path::new(&resolved));
        }
    }
    let Some(listing) = git_raw(&["ls-files", "-z"]) else {
        // No git, or no index: nothing else to watch. The default rule (re-run when a file in
        // the package changes) then applies, and the value is `unknown` either way.
        return;
    };
    // `-z`: a path containing a space or a newline is still one path. A path that is not valid
    // UTF-8 cannot be named to cargo as it is written, so it is skipped rather than mangled
    // into a path that does not exist — cargo re-runs the script on **every** build for a path
    // that does not exist, which is why every optional path above goes through the same guard.
    for path in listing.split(|byte| *byte == 0) {
        if let Ok(path) = std::str::from_utf8(path)
            && !path.is_empty()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}

/// Name `path` to cargo only when it is there. A `rerun-if-changed` path that does not exist
/// makes cargo re-run the script on every build, so a path git may or may not have — a reflog, a
/// loose ref — is watched when present and silently skipped when not.
fn watch_if_present(path: &std::path::Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// Run git and return its trimmed stdout. `None` on anything that is not a successful,
/// UTF-8 answer — an absent binary, a non-zero status, output this cannot read.
fn git(args: &[&str]) -> Option<String> {
    git_raw(args).and_then(|raw| String::from_utf8(raw).ok().map(|s| s.trim().to_string()))
}

/// Run git and return its stdout as bytes: `ls-files -z` is deliberately NUL-separated, so it
/// must be split before any UTF-8 conversion.
fn git_raw(args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git").args(args).output().ok()?;
    output.status.success().then_some(output.stdout)
}
