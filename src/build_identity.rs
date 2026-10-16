//! The build identity's decision, extracted so it can be *tested*: a pure function over the two
//! facts the identity is built from.
//!
//! Why this is a lib module and not only build-script code: `build.rs` decides the value
//! `req --version` reports as `built from`, and a build script cannot be unit-tested by
//! `cargo test` — it is compiled and run by cargo, not by the test harness. The build script pulls
//! this file in with `#[path = "src/build_identity.rs"] mod build_identity;`, so **one**
//! implementation is compiled by both and the four states are exercised by the unit tests at the
//! bottom of this file. (`#[path]` rather than `include!`: it keeps this a normal module with its
//! own imports and no textual splicing into `build.rs`'s item scope.)
//!
//! Four states, from two facts that are independent:
//!
//! * `<sha>` — `rev-parse` resolved a commit and the cleanliness probe answered "clean".
//! * `<sha>-dirty` — resolved, and a tracked file is modified.
//! * `<sha>-cleanliness-unknown` — resolved, and the cleanliness probe could not answer at all (a
//!   locked index, an invocation this git rejects, a killed process). The commit is a fact git
//!   gave us; discarding it would also make `unknown` mean two different things with two
//!   different remedies.
//! * `unknown` — the commit itself could not be resolved: no `.git`, no `git`, an empty answer.
//!
//! A bare `<sha>` therefore keeps meaning *clean* — the round-1 bug was emitting it for a probe
//! that had established nothing, and the remedy is the suffix, not the loss of attribution.

/// The honest answer when git cannot resolve the commit at all.
pub const UNKNOWN: &str = "unknown";

/// The suffix for a resolved commit whose cleanliness was not established. Deliberately spelled
/// out rather than shortened to a token like `-?`: a bare `<sha>` must keep meaning *clean*, and a
/// provenance field is the wrong place to save bytes at the cost of a reader having to guess.
pub const CLEANLINESS_UNKNOWN: &str = "-cleanliness-unknown";

/// What the cleanliness probe answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleanliness {
    /// `git status --porcelain --untracked-files=no` answered with no entries.
    Clean,
    /// ...answered with entries: tracked files are modified.
    Dirty,
    /// ...could not answer at all.
    Unestablished,
}

/// The identity from the two facts: the resolved commit (if any) and the cleanliness answer.
pub fn identity(commit: Option<&str>, cleanliness: Cleanliness) -> String {
    match commit.filter(|sha| !sha.is_empty()) {
        None => UNKNOWN.to_string(),
        Some(sha) => match cleanliness {
            Cleanliness::Clean => sha.to_string(),
            Cleanliness::Dirty => format!("{sha}-dirty"),
            Cleanliness::Unestablished => format!("{sha}{CLEANLINESS_UNKNOWN}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three reachable-and-correct states, and the fourth the old code collapsed away. A state
    /// no test reaches is a state nobody has verified: the `Unestablished` arm cannot be triggered
    /// end-to-end (it needs a `git` that fails), which is exactly why the decision lives here.
    #[test]
    fn a_resolved_clean_commit_is_a_bare_sha() {
        assert_eq!(identity(Some("abc1234"), Cleanliness::Clean), "abc1234");
    }

    #[test]
    fn a_resolved_dirty_commit_is_suffixed_dirty() {
        assert_eq!(
            identity(Some("abc1234"), Cleanliness::Dirty),
            "abc1234-dirty"
        );
    }

    /// The round-2 decision: the commit and the cleanliness are **separate** facts, and only the
    /// unresolved commit makes the whole value `unknown`. Throwing the commit away here would
    /// discard attribution exactly in the anomalous state where a bug report needs it most.
    #[test]
    fn a_resolved_commit_with_an_unanswerable_probe_keeps_the_commit() {
        assert_eq!(
            identity(Some("abc1234"), Cleanliness::Unestablished),
            "abc1234-cleanliness-unknown"
        );
        // ...and the suffix is not the dirty one: a reader must not mistake an unestablished
        // cleanliness for a known modification (or the reverse).
        assert!(!identity(Some("abc1234"), Cleanliness::Unestablished).ends_with("-dirty"));
    }

    /// Only the resolved commit makes the whole value `unknown` — whatever the probe said, and
    /// including a `git` that answered with nothing.
    #[test]
    fn an_unresolved_commit_is_unknown_whatever_the_probe_said() {
        for cleanliness in [
            Cleanliness::Clean,
            Cleanliness::Dirty,
            Cleanliness::Unestablished,
        ] {
            assert_eq!(identity(None, cleanliness), UNKNOWN);
        }
        assert_eq!(identity(Some(""), Cleanliness::Clean), UNKNOWN);
    }

    /// The four states are mutually distinguishable, which is the property the suffix exists for:
    /// no two of them render to the same string.
    #[test]
    fn the_four_states_render_distinctly() {
        let states = [
            identity(None, Cleanliness::Unestablished),
            identity(Some("abc1234"), Cleanliness::Clean),
            identity(Some("abc1234"), Cleanliness::Dirty),
            identity(Some("abc1234"), Cleanliness::Unestablished),
        ];
        assert_eq!(
            states,
            [
                UNKNOWN,
                "abc1234",
                "abc1234-dirty",
                "abc1234-cleanliness-unknown"
            ]
        );
        for (i, a) in states.iter().enumerate() {
            for b in states.iter().skip(i + 1) {
                assert_ne!(a, b, "two identity states render alike: {a:?}");
            }
        }
    }
}
