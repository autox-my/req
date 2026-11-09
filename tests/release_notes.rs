//! `scripts/release-notes.sh` — the release notes of one rev (ADR 0031, amending ADR 0025 §5).
//!
//! The public seam is the script run as a process, exactly what `release.yml`'s build job runs,
//! against a throwaway git repository whose history the test writes by hand. The oracles are
//! independent of the script: the headings, the arrows and the wording below come from the ADR,
//! and the expected PR lines are typed here, never read back from the script's output.
//!
//! What the notes must do, in the order the ADR states it: say what the previous release is (the
//! nearest `rev-*` tag that is an ancestor, and nothing else), say whether req's identity moved
//! (`RULES_V`, the claim schema, the Baseline envelope) and what an adopter must then do, list the
//! pull requests merged since, grouped by their conventional-commit prefix, and treat every title
//! as hostile text. A value it cannot read is "could not be determined", never "unchanged".
//!
//! Exit contract: 0 written; 1 `--out` exists (never overwritten); 2 usage, an unresolvable rev, a
//! repository name outside `owner/name`. In every non-zero case `--out` is left untouched.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);
/// Every commit gets a later date than the one before, in the whole process: tag order by date must
/// never be a tie, or a script that picks "the newest tag" instead of "the nearest ancestor" survives.
static CLOCK: AtomicU64 = AtomicU64::new(1_700_000_000);

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/release-notes.sh")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let when = format!("{} +0000", CLOCK.fetch_add(100, Ordering::SeqCst));
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", &when)
        .env("GIT_COMMITTER_DATE", &when)
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
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_owned()
}

/// A repository whose `src/canon.rs` and `src/baseline.rs` carry the three constants the notes read.
struct World {
    dir: PathBuf,
}

fn constants(rules: u32, schema: u32, envelope: u32) -> [(&'static str, String); 2] {
    [
        (
            "src/canon.rs",
            format!(
                "pub const RULES_V: u32 = {rules};\npub const CLAIM_SCHEMA_V: u32 = {schema};\n"
            ),
        ),
        (
            "src/baseline.rs",
            format!("pub const BASELINE_VERSION: u32 = {envelope};\n"),
        ),
    ]
}

impl World {
    fn new() -> World {
        let dir = std::env::temp_dir().join(format!(
            "req-release-notes-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(dir.join("src")).expect("creates the fixture");
        git(&dir, &["init", "-q", "-b", "main"]);
        World { dir }
    }

    fn set(&self, files: &[(&str, String)]) {
        for (rel, text) in files {
            fs::write(self.dir.join(rel), text).expect("writes");
        }
    }

    fn commit(&self, message: &str) -> String {
        git(&self.dir, &["add", "-A"]);
        git(&self.dir, &["commit", "-q", "--allow-empty", "-m", message]);
        git(&self.dir, &["rev-parse", "HEAD"])
    }

    /// A pull request merged the way GitHub does it: a merge commit whose body is the title.
    fn merge_pr(&self, number: u32, title: &str) {
        let branch = format!("pr{number}");
        git(&self.dir, &["checkout", "-q", "-b", &branch]);
        self.commit(&format!("work for {number}"));
        git(&self.dir, &["checkout", "-q", "main"]);
        git(
            &self.dir,
            &[
                "merge",
                "-q",
                "--no-ff",
                &branch,
                "-m",
                &format!("Merge pull request #{number} from someone/{branch}\n\n{title}"),
            ],
        );
    }

    /// The same, in the former tracker's shape: the title is in the subject, quoted, and the body is empty.
    fn merge_quoted(&self, number: u32, title: &str) {
        let branch = format!("old{number}");
        git(&self.dir, &["checkout", "-q", "-b", &branch]);
        self.commit(&format!("work for {number}"));
        git(&self.dir, &["checkout", "-q", "main"]);
        git(
            &self.dir,
            &[
                "merge",
                "-q",
                "--no-ff",
                &branch,
                "-m",
                &format!("Merge pull request '{title}' (#{number}) from {branch} into main"),
            ],
        );
    }

    fn tag(&self, name: &str) {
        git(&self.dir, &["tag", name]);
    }

    fn notes(&self, rev: &str, repo: &str) -> (Output, PathBuf) {
        let out = self.dir.join("notes.md");
        let run = Command::new("bash")
            .arg(script())
            .args(["--rev", rev, "--repo", repo, "--out"])
            .arg(&out)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("runs the script");
        (run, out)
    }

    /// Run and return the notes text, failing the test on a non-zero exit.
    fn text(&self, rev: &str) -> String {
        let (run, out) = self.notes(rev, "owner/name");
        assert!(
            run.status.success(),
            "exit {:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
        fs::read_to_string(out).expect("the notes file")
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A world with a first release tagged, then `rules` etc. at the new rev. Returns (world, new full sha).
fn released(old: (u32, u32, u32), new: (u32, u32, u32)) -> (World, String) {
    let w = World::new();
    w.set(&constants(old.0, old.1, old.2));
    let first = w.commit("initial");
    w.tag(&format!("rev-{}", &first[..7]));
    w.set(&constants(new.0, new.1, new.2));
    let head = w.commit("the new rev");
    (w, head)
}

#[test]
fn the_first_release_says_so_and_lists_nothing() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.commit("root");
    w.merge_pr(3, "feat: something");
    let head = w.commit("tip");
    let text = w.text(&head);
    assert!(text.contains("First release"), "{text}");
    assert!(
        !text.contains("something"),
        "no earlier tag, so no list: {text}"
    );
    assert!(
        text.contains("rules_v 7") && text.contains("claim schema req.claim/2"),
        "the identity facts of this rev are still stated: {text}"
    );
}

#[test]
fn the_previous_release_is_the_nearest_rev_tag_that_is_an_ancestor() {
    let w = World::new();
    w.set(&constants(6, 2, 4));
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.tag("v-not-a-release");
    let b = w.commit("b");
    w.tag(&format!("rev-{}", &b[..7]));
    // A release tag on a side branch is not an ancestor of the rev and must not be picked.
    git(&w.dir, &["checkout", "-q", "-b", "side"]);
    let side = w.commit("side");
    w.tag(&format!("rev-{}", &side[..7]));
    git(&w.dir, &["checkout", "-q", "main"]);
    let head = w.commit("tip");
    let text = w.text(&head);
    assert!(
        text.contains(&format!("Previous release: `rev-{}`", &b[..7])),
        "{text}"
    );
    assert!(!text.contains(&format!("rev-{}", &side[..7])), "{text}");
    assert!(
        text.contains(&format!(
            "https://github.com/owner/name/compare/rev-{}...{}",
            &b[..7],
            &head[..7]
        )),
        "a compare link from the previous tag to this rev: {text}"
    );
}

#[test]
fn when_nothing_that_decides_identity_moved_it_says_unchanged() {
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    let text = w.text(&head);
    assert!(text.contains("No identity change"), "{text}");
    assert!(text.contains("unchanged"), "{text}");
    assert!(!text.contains("re-sign"), "nothing to re-sign: {text}");
}

#[test]
fn a_rules_version_bump_says_what_an_adopter_must_do() {
    let (w, head) = released((6, 2, 4), (7, 2, 4));
    let text = w.text(&head);
    assert!(text.contains("rules_v 6 → 7"), "{text}");
    assert!(text.contains("**changed**"), "{text}");
    assert!(
        text.contains("re-sign") && text.contains("req baseline --verify"),
        "a bump means re-emit and re-sign, then verify: {text}"
    );
    assert!(!text.contains("No identity change"), "{text}");
}

#[test]
fn a_claim_schema_or_envelope_change_is_named_separately() {
    let (w, head) = released((7, 2, 4), (7, 3, 5));
    let text = w.text(&head);
    assert!(
        text.contains("claim schema req.claim/2 → req.claim/3"),
        "{text}"
    );
    assert!(text.contains("baseline envelope 4 → 5"), "{text}");
    assert!(
        !text.contains("rules_v 7 → 7"),
        "an unmoved value is not an arrow: {text}"
    );
}

#[test]
fn a_value_that_cannot_be_read_is_never_reported_as_unchanged() {
    let w = World::new();
    // The previous release has no constants at all (a rev from before the files existed).
    w.set(&[("README.md", "x\n".to_owned())]);
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.set(&constants(7, 2, 4));
    let head = w.commit("tip");
    let text = w.text(&head);
    assert!(text.contains("could not be determined"), "{text}");
    assert!(!text.contains("No identity change"), "{text}");
    assert!(
        text.contains("read the compare link"),
        "an unreadable comparison sends the reader to the diff: {text}"
    );
}

#[test]
fn each_constant_that_cannot_be_read_is_named_on_its_own() {
    // The previous release has exactly one of the three constants; the other two must each say so.
    struct Case {
        label: &'static str,
        file: &'static str,
        text: &'static str,
        unreadable: [&'static str; 2],
    }
    let cases = [
        Case {
            label: "rules_v only",
            file: "src/canon.rs",
            text: "pub const RULES_V: u32 = 7;\n",
            unreadable: [
                "claim schema could not be determined",
                "baseline envelope could not be determined",
            ],
        },
        Case {
            label: "schema only",
            file: "src/canon.rs",
            text: "pub const CLAIM_SCHEMA_V: u32 = 2;\n",
            unreadable: [
                "rules_v could not be determined",
                "baseline envelope could not be determined",
            ],
        },
        Case {
            label: "envelope only",
            file: "src/baseline.rs",
            text: "pub const BASELINE_VERSION: u32 = 4;\n",
            unreadable: [
                "rules_v could not be determined",
                "claim schema could not be determined",
            ],
        },
    ];
    for case in cases {
        let w = World::new();
        w.set(&[(case.file, case.text.to_owned())]);
        let a = w.commit("a");
        w.tag(&format!("rev-{}", &a[..7]));
        w.set(&constants(7, 2, 4));
        let head = w.commit("tip");
        let text = w.text(&head);
        for needle in case.unreadable {
            assert!(
                text.contains(needle),
                "{}: expected `{needle}` in:\n{text}",
                case.label
            );
        }
        assert!(
            !text.contains("No identity change"),
            "{}: {text}",
            case.label
        );
    }
}

#[test]
fn merged_pull_requests_are_listed_under_their_prefix_with_the_number() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.merge_pr(11, "feat(canon): a new thing");
    w.merge_pr(12, "fix(install): a broken thing");
    w.merge_pr(13, "docs: a page");
    w.merge_pr(14, "chore(review): housekeeping");
    w.merge_pr(15, "Something with no prefix");
    w.merge_quoted(9, "feat: the old tracker's title (#1)");
    let head = w.commit("tip");
    let text = w.text(&head);
    for (heading, line) in [
        ("### Features", "- feat(canon): a new thing (#11)"),
        ("### Features", "- feat: the old tracker's title (#1) (#9)"),
        ("### Fixes", "- fix(install): a broken thing (#12)"),
        ("### Documentation", "- docs: a page (#13)"),
        ("### Other", "- chore(review): housekeeping (#14)"),
        ("### Other", "- Something with no prefix (#15)"),
    ] {
        let at = text
            .find(heading)
            .unwrap_or_else(|| panic!("no {heading}: {text}"));
        let section = &text[at..];
        let end = section[heading.len()..]
            .find("\n### ")
            .map_or(section.len(), |e| e + heading.len());
        assert!(
            section[..end].contains(line),
            "{heading} lacks `{line}`:\n{text}"
        );
    }
    assert!(
        !text.contains("work for"),
        "only merges are listed, never the commits inside them: {text}"
    );
}

#[test]
fn a_range_with_no_merge_says_so() {
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    assert!(
        w.text(&head).contains("No pull request was merged"),
        "{}",
        w.text(&head)
    );
}

#[test]
fn a_title_is_treated_as_hostile_text() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    let long = "x".repeat(400);
    w.merge_pr(
        21,
        &format!("fix: ping @someone <script>alert(1)</script> [click](http://e.invalid) {long}"),
    );
    let head = w.commit("tip");
    let text = w.text(&head);
    let line = text
        .lines()
        .find(|l| l.contains("(#21)"))
        .unwrap_or_else(|| panic!("no #21 line: {text}"));
    assert!(
        !line.contains("@someone"),
        "a mention would notify a person: {line}"
    );
    assert!(
        line.contains("@\u{200b}someone"),
        "defused, not deleted: {line}"
    );
    assert!(
        !line.contains('<') && !line.contains('>'),
        "no HTML: {line}"
    );
    assert!(
        !line.contains("](http"),
        "no live link from a title: {line}"
    );
    assert!(
        line.chars().count() < 260,
        "a long title is cut: {} chars",
        line.chars().count()
    );
}

#[test]
fn a_repo_name_outside_owner_slash_name_is_refused() {
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    for bad in ["", "owner", "a/b/c", "owner/na me", "owner/$(x)", "../x"] {
        let (run, out) = w.notes(&head, bad);
        assert_eq!(run.status.code(), Some(2), "repo {bad:?}");
        assert!(!out.exists(), "nothing written for repo {bad:?}");
    }
}

#[test]
fn an_unresolvable_rev_is_exit_2_and_writes_nothing() {
    let (w, _) = released((7, 2, 4), (7, 2, 4));
    let (run, out) = w.notes("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "owner/name");
    assert_eq!(
        run.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn an_existing_output_file_is_refused_and_left_untouched() {
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    fs::write(w.dir.join("notes.md"), "precious").expect("writes");
    let (run, out) = w.notes(&head, "owner/name");
    assert_eq!(
        run.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read_to_string(out).expect("reads"), "precious");
}
