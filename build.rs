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
//! 1. **Never fail the build, and only an unresolved commit makes the whole value `unknown`.** No
//!    `.git`, no `git`, a shallow clone, a source tarball or a git that errors on `rev-parse` all
//!    yield `unknown`. The **commit and the cleanliness are separate facts**, though: when
//!    `rev-parse --short HEAD` resolves but the cleanliness probe cannot answer at all — an old git
//!    that rejects `--no-optional-locks`, a corrupt or locked index, a killed process — the
//!    resolved commit is kept and only the cleanliness is marked unestablished, as
//!    `<sha>-cleanliness-unknown`. (Round 1 of this script's review collapsed a *failed* probe into
//!    "clean" and emitted a bare SHA — asserting cleanliness the probe never established; that was
//!    the defect, and it is fixed by the suffix, not by discarding the commit.) Collapsing the
//!    unanswerable probe into `unknown` would make the value mean two different things with two
//!    different remedies — *this is not a git checkout* and *run `git status` yourself and find out
//!    why* — and would throw away attribution exactly in the anomalous state a bug report needs it
//!    most. A build script that can fail breaks every build for the sake of one provenance line, so
//!    the value is emitted unconditionally — `unknown` included, never omitted (an absent line reads
//!    as "not applicable", a different claim). The four values are decided by `identity` in
//!    `src/build_identity.rs`, which is compiled into this script below and unit-tested by
//!    `cargo test`: a build script itself cannot be tested.
//! 2. **A stale identity is worse than none.** A value naming a commit the binary was not
//!    built from is *confidently wrong* — precisely the failure this exists to remove. Cargo
//!    re-runs a build script only for the paths it is told about, so `<gitdir>/HEAD` is named,
//!    resolved with `git rev-parse --git-dir` and **never** a hardcoded `.git`: in a worktree
//!    `.git` is a *file* pointing at the real gitdir, so a hardcoded path would watch nothing
//!    and leave the identity stale forever. Every tracked file is named too, or the `-dirty`
//!    marker would survive the edit that should have set it — and **so is `<gitdir>/index`**,
//!    because staging is exactly the edit nothing else records: `git add <untracked>`,
//!    `git reset` and `git rm --cached` move `HEAD`-relative state without changing `HEAD`, the
//!    reflog, the ref, or any working-tree file, so a staged-only change left `built from`
//!    confidently clean while `git status` reported it.
//! 3. **Dirty means "tracked files modified"**: `git --no-optional-locks status --porcelain
//!    --untracked-files=no`. A plain `--porcelain` marks any checkout holding an untracked file
//!    dirty forever, and an always-on `-dirty` trains the reader to ignore the marker. The
//!    probe must also not *write*: `git status` otherwise refreshes (and rewrites) the index as
//!    a side effect, and now that item 2 watches that file, a probe that rewrites its own watch
//!    input makes the script re-run itself — **measured, not argued**, by dropping the flag on a
//!    deliberately stale index: the rewrite happened, and it cost one extra build-script
//!    execution per staleness event before settling (1, 1, 0, 0, 0), with one measured window at
//!    three consecutive re-runs (1, 1, 1). So the flag is load-bearing on every build it is
//!    missing, and an unbounded loop is plausible rather than measured. `--no-optional-locks` is
//!    what keeps the probe a reader.
//! 4. **No new dependency**: `std::process::Command`, not `git2` — a build-graph dependency for
//!    one version string, in a crate that links no cryptography and needs no network. This
//!    script reads local files and local git state only; it fetches nothing.
//! 5. **It must never reach an identity.** The SHA is *printed*, never hashed: it is not in
//!    `canonical_bytes`, not in an address, not in any digest. Nothing here touches the model,
//!    and `req canon --verify` staying green on the committed corpus is the proof.

use std::process::Command;

// The identity's decision, as a pure function over the two facts it is built from. Pulled in by
// path so the build script and the library compile the **same file**: one implementation, and the
// four states are unit-tested in `cargo test` (`src/build_identity.rs`), which a build script
// cannot be. `#[path]` over `include!`: this stays a normal module with its own imports, with no
// textual splicing into this file's item scope.
#[path = "src/build_identity.rs"]
mod build_identity;

use build_identity::Cleanliness;

fn main() {
    // Unconditional, so `env!("REQ_BUILD_SHA")` always resolves in `src/main.rs`.
    println!("cargo:rustc-env=REQ_BUILD_SHA={}", resolve_identity());
    watch_inputs();
}

/// The build identity: the resolved commit, carrying what the cleanliness probe established about
/// it. Never an error: every failure path here is a build that must still succeed.
///
/// The two facts are resolved separately, because they are separate: `rev-parse` answering gives a
/// commit worth reporting even when the cleanliness probe cannot answer, and only an unresolved
/// commit makes the whole value `unknown`. The four renderings are `identity`'s
/// (`src/build_identity.rs`); this function's job is to resolve the two inputs without failing.
fn resolve_identity() -> String {
    match git(&["rev-parse", "--short", "HEAD"]) {
        Some(commit) if !commit.is_empty() => {
            let cleanliness = match git(&[
                "--no-optional-locks",
                "status",
                "--porcelain",
                "--untracked-files=no",
            ]) {
                Some(status) if status.is_empty() => Cleanliness::Clean,
                Some(_) => Cleanliness::Dirty,
                // Not "clean": `None` is git failing to answer at all (see contract item 1).
                None => Cleanliness::Unestablished,
            };
            build_identity::identity(Some(&commit), cleanliness)
        }
        // No commit, or an answer that resolved nothing: there is nothing to attribute, and the
        // cleanliness probe cannot change that, so it is not run.
        _ => build_identity::identity(None, Cleanliness::Unestablished),
    }
}

/// What cargo must watch for this script to re-run. Where the identity *derives from* git
/// state that does not change when a file does — and vice versa — both are named:
///
/// * the git metadata a commit or a checkout rewrites;
/// * `<gitdir>/index`, because the `-dirty` probe compares the index to `HEAD` and staging is
///   the one way that comparison moves without moving anything else — `git add <untracked>`,
///   `git reset` and `git rm --cached` rewrite no ref, no reflog, and no working-tree file;
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
        // The index is the only input that moves for a staged-only change: `git add <untracked>`
        // rewrites it and nothing else this function names, so without it cargo never re-runs
        // the script and `-dirty` stays silently absent. The probe above must therefore not
        // write it — `--no-optional-locks` (contract item 3).
        watch_if_present(&gitdir.join("index"));
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
