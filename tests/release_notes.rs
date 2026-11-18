//! `scripts/release-notes.sh` — the release notes of one rev (ADR 0031, amending ADR 0025 §5).
//!
//! The public seam is the script run as a process, exactly what `release.yml`'s build job runs,
//! against a throwaway git repository whose history the test writes by hand. The oracles are
//! independent of the script: the headings, the arrows and the wording below come from the ADR,
//! and the expected PR lines are typed here, never read back from the script's output.
//!
//! What the notes must do, in the order the ADR states it: open with the version, the rev and the bump class
//! (ADR 0055), say what the previous release is (the nearest `rev-*` or `v<N>…` tag strictly before the rev,
//! and nothing else), say whether req's identity moved
//! (`RULES_V`, the claim schema, the Baseline envelope) and what an adopter must then do, list the
//! pull requests merged since, grouped by their conventional-commit prefix, and treat every title
//! as hostile text. A value it cannot read is "could not be determined", never "unchanged".
//!
//! Beside the notes, `--facts <file>` writes `key=value` lines for the release workflow (ADR 0055 decision 4):
//! the previous release's tag, its version (a `rev-*` release counts as 0.1.0; a `v*` release is read from
//! `<tag>:Cargo.toml`) and identity as `moved`, `unmoved` or `unknown`.
//!
//! Exit contract: 0 written; 1 `--out` or `--facts` exists (never overwritten); 2 usage, an unresolvable rev, a
//! repository name outside `owner/name`. In every non-zero case both files are left as they were.
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

    /// Run with `--facts`, failing the test on a non-zero exit; returns the facts as `key=value` lines.
    fn facts(&self, rev: &str) -> String {
        let facts = self.dir.join("facts.txt");
        let _ = fs::remove_file(&facts);
        let _ = fs::remove_file(self.dir.join("notes.md"));
        let run = Command::new("bash")
            .arg(script())
            .args(["--rev", rev, "--repo", "owner/name", "--out"])
            .arg(self.dir.join("notes.md"))
            .arg("--facts")
            .arg(&facts)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("runs the script");
        assert!(
            run.status.success(),
            "exit {:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
        fs::read_to_string(facts).expect("the facts file")
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

/// A `Cargo.toml` whose `[package]` says `version`, with a dependency table that has a `version` of its own first.
fn cargo(version: &str) -> (&'static str, String) {
    (
        "Cargo.toml",
        format!(
            "[dependencies.other]\nversion = \"9.9.9\"\n\n[package]\nname = \"req\"\nversion = \"{version}\"\n"
        ),
    )
}

/// `constants`, and the `READS_AS_IS` list of `src/canon.rs` as the released rev writes it
/// (`pub const READS_AS_IS: &[u32] = &[2, 3];`, ADR 0056 *Consequences*).
fn constants_reading(
    rules: u32,
    schema: u32,
    envelope: u32,
    reads_as_is: &[u32],
) -> [(&'static str, String); 2] {
    let list: Vec<String> = reads_as_is.iter().map(u32::to_string).collect();
    let [(canon, canon_text), baseline] = constants(rules, schema, envelope);
    [
        (
            canon,
            format!(
                "{canon_text}pub const READS_AS_IS: &[u32] = &[{}];\n",
                list.join(", ")
            ),
        ),
        baseline,
    ]
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
fn the_previous_release_is_the_nearest_release_tag_that_is_an_ancestor() {
    let w = World::new();
    w.set(&constants(6, 2, 4));
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
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

/// ADR 0055 decision 3. Changed from the assertion of ADR 0031, which held that a `v`-prefixed tag is not a release:
/// a `v<VERSION>` tag is a release now, and the nearest one of either kind is the previous.
#[test]
fn a_v_tag_counts_as_a_release_and_the_nearest_of_either_kind_is_the_previous() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.set(&[cargo("0.1.0")]);
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.set(&[cargo("0.2.0")]);
    w.commit("b");
    w.tag("v0.2.0");
    let head = w.commit("tip");
    let text = w.text(&head);
    assert!(
        text.contains("Previous release: `v0.2.0`"),
        "a v tag is a release, and the nearer one: {text}"
    );
    assert!(
        text.contains(&format!(
            "https://github.com/owner/name/compare/v0.2.0...{}",
            &head[..7]
        )),
        "the compare link names the v tag: {text}"
    );
    assert!(!text.contains(&format!("rev-{}", &a[..7])), "{text}");
    // The other way round: a rev tag nearer than a v tag is the previous.
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.set(&[cargo("0.2.0")]);
    w.commit("a");
    w.tag("v0.2.0");
    let b2 = w.commit("b");
    w.tag(&format!("rev-{}", &b2[..7]));
    let head = w.commit("tip");
    assert!(
        w.text(&head)
            .contains(&format!("Previous release: `rev-{}`", &b2[..7])),
        "{}",
        w.text(&head)
    );
}

#[test]
fn the_previous_release_is_strictly_before_the_rev_so_a_releases_own_tag_is_never_its_predecessor()
{
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.set(&[cargo("0.2.0")]);
    w.commit("a");
    w.tag("v0.2.0");
    w.set(&[cargo("0.3.0")]);
    let rev = w.commit("the rev being released");
    // The workflow notes a rev that is already tagged when a dispatch is retried: its own tag must not count.
    w.tag("v0.3.0");
    let text = w.text(&rev);
    assert!(text.contains("Previous release: `v0.2.0`"), "{text}");
    assert!(!text.contains("Previous release: `v0.3.0`"), "{text}");
    let facts = w.facts(&rev);
    assert!(facts.lines().any(|l| l == "previous_tag=v0.2.0"), "{facts}");
}

#[test]
fn the_notes_open_with_the_version_the_rev_and_the_bump_class() {
    for (previous, new, class) in [
        ("0.1.0", "0.2.0", "minor"),
        ("0.1.0", "0.1.1", "patch"),
        ("0.1.0", "1.0.0", "major"),
        ("0.1.0", "0.1.0", "none"),
    ] {
        let w = World::new();
        w.set(&constants(7, 2, 4));
        w.set(&[cargo(previous)]);
        let a = w.commit("a");
        w.tag(&format!("rev-{}", &a[..7]));
        w.set(&[cargo(new)]);
        let head = w.commit("tip");
        let text = w.text(&head);
        assert_eq!(
            text.lines().next(),
            Some(format!("Version {new} · rev `{}` · bump: {class}", &head[..7]).as_str()),
            "{previous} -> {new}:\n{text}"
        );
        assert!(
            text.contains("## Identity"),
            "ADR 0031's sections follow, in their order: {text}"
        );
    }
}

#[test]
fn the_opening_line_says_when_there_is_nothing_to_compare() {
    // A first release.
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.set(&[cargo("0.1.0")]);
    let head = w.commit("root");
    assert_eq!(
        w.text(&head).lines().next(),
        Some(format!("Version 0.1.0 · rev `{}` · bump: first release", &head[..7]).as_str())
    );
    // A rev whose Cargo.toml has no readable version: said, never guessed.
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    assert_eq!(
        w.text(&head).lines().next(),
        Some(
            format!(
                "Version unknown · rev `{}` · bump: not comparable",
                &head[..7]
            )
            .as_str()
        )
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
    // `/2 -> /3`: the released rev reads `/2` as it is (ADR 0051), so the line says that and not NOT JUDGED.
    let (w, head) = released_reading((7, 2, 4), &[], (7, 3, 5), &[2]);
    let text = w.text(&head);
    assert!(
        text.contains("claim schema req.claim/2 → req.claim/3"),
        "{text}"
    );
    assert!(text.contains("loaded as it is"), "{text}");
    assert!(text.contains("baseline envelope 4 → 5"), "{text}");
    assert!(
        !text.contains("rules_v 7 → 7"),
        "an unmoved value is not an arrow: {text}"
    );
}

/// A world whose previous release declares `old` and whose new rev declares `new`, with the list of
/// formats the **new** rev loads as they are (the released rev's list, never the previous one's).
fn released_reading(
    old: (u32, u32, u32),
    old_list: &[u32],
    new: (u32, u32, u32),
    new_list: &[u32],
) -> (World, String) {
    let w = World::new();
    w.set(&constants_reading(old.0, old.1, old.2, old_list));
    let first = w.commit("initial");
    w.tag(&format!("rev-{}", &first[..7]));
    w.set(&constants_reading(new.0, new.1, new.2, new_list));
    let head = w.commit("the new rev");
    (w, head)
}

#[test]
fn a_previous_schema_the_released_rev_reads_as_is_is_not_called_not_judged() {
    // ADR 0056, falsification 13: `/3 -> /4` with `READS_AS_IS = [2, 3]` at the released rev says "loaded as it
    // is", and never that the old schema is NOT JUDGED.
    let (w, head) = released_reading((9, 3, 4), &[2], (9, 4, 5), &[2, 3]);
    let text = w.text(&head);
    assert!(
        text.contains("claim schema req.claim/3 → req.claim/4"),
        "{text}"
    );
    assert!(text.contains("loaded as it is"), "{text}");
    assert!(
        !text.contains("NOT JUDGED"),
        "a /3 file is loaded as it is by this build: {text}"
    );
}

#[test]
fn a_previous_schema_the_released_rev_does_not_read_as_is_is_not_judged() {
    let (w, head) = released_reading((9, 1, 4), &[], (9, 3, 4), &[2]);
    let text = w.text(&head);
    assert!(
        text.contains("claim schema req.claim/1 → req.claim/3"),
        "{text}"
    );
    assert!(text.contains("NOT JUDGED"), "{text}");
    assert!(!text.contains("loaded as it is"), "{text}");
}

#[test]
fn the_list_is_read_at_the_released_rev_not_at_the_previous_one() {
    // The previous rev lists `[3]`, the released rev lists `[2]`: a `/3` file is NOT JUDGED by the released
    // build. Reading the previous rev's list would print the false "loaded as it is".
    let (w, head) = released_reading((9, 3, 4), &[3], (9, 5, 4), &[2]);
    let text = w.text(&head);
    assert!(text.contains("NOT JUDGED"), "{text}");
    assert!(!text.contains("loaded as it is"), "{text}");
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

// --- the facts file (ADR 0055 decision 4) ----------------------------------------------------------------

#[test]
fn the_facts_name_the_previous_tag_its_version_and_identity_moved() {
    let (w, head) = released((6, 2, 4), (7, 2, 4));
    let first = git(
        &w.dir,
        &["describe", "--tags", "--abbrev=0", &format!("{head}^")],
    );
    assert_eq!(
        w.facts(&head),
        format!("previous_tag={first}\nprevious_version=0.1.0\nidentity=moved\n"),
        "a rev-* release counts as 0.1.0 whatever its files say"
    );
}

#[test]
fn the_facts_say_unmoved_when_nothing_that_decides_identity_moved() {
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    assert!(w.facts(&head).lines().any(|l| l == "identity=unmoved"));
}

#[test]
fn each_constant_that_moved_makes_identity_moved() {
    for (old, new, what) in [
        ((6, 2, 4), (7, 2, 4), "rules_v"),
        ((7, 2, 4), (7, 3, 4), "the claim schema"),
        ((7, 2, 4), (7, 2, 5), "the baseline envelope"),
    ] {
        let (w, head) = released(old, new);
        assert!(
            w.facts(&head).lines().any(|l| l == "identity=moved"),
            "{what} moved"
        );
    }
}

#[test]
fn the_facts_say_unknown_when_identity_could_not_be_compared_and_never_unmoved() {
    let w = World::new();
    w.set(&[("README.md", "x\n".to_owned())]);
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.set(&constants(7, 2, 4));
    let head = w.commit("tip");
    let facts = w.facts(&head);
    assert!(facts.lines().any(|l| l == "identity=unknown"), "{facts}");
    // Each constant on its own: an unreadable one is unknown even when the others are equal.
    let w = World::new();
    w.set(&[("src/canon.rs", "pub const RULES_V: u32 = 7;\n".to_owned())]);
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.set(&constants(7, 2, 4));
    let head = w.commit("tip");
    assert!(w.facts(&head).lines().any(|l| l == "identity=unknown"));
}

#[test]
fn a_known_move_outranks_an_unreadable_constant() {
    // rules_v moved, and the envelope could not be read at the previous release: it moved, which asks more of the bump.
    let w = World::new();
    w.set(&[(
        "src/canon.rs",
        "pub const RULES_V: u32 = 6;\npub const CLAIM_SCHEMA_V: u32 = 2;\n".to_owned(),
    )]);
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    w.set(&constants(7, 2, 4));
    let head = w.commit("tip");
    assert!(w.facts(&head).lines().any(|l| l == "identity=moved"));
}

#[test]
fn a_v_release_has_its_version_read_from_its_own_cargo_toml() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.set(&[cargo("0.2.0")]);
    w.commit("a");
    w.tag("v0.2.0");
    // The file moves on after the tag; the tag's own copy is the previous release's version.
    w.set(&[cargo("0.4.0")]);
    let head = w.commit("tip");
    assert_eq!(
        w.facts(&head),
        "previous_tag=v0.2.0\nprevious_version=0.2.0\nidentity=unmoved\n"
    );
}

#[test]
fn a_rev_release_counts_as_0_1_0_even_when_its_cargo_toml_says_otherwise() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.set(&[cargo("0.7.0")]);
    let a = w.commit("a");
    w.tag(&format!("rev-{}", &a[..7]));
    let head = w.commit("tip");
    assert!(
        w.facts(&head)
            .lines()
            .any(|l| l == "previous_version=0.1.0"),
        "{}",
        w.facts(&head)
    );
}

#[test]
fn a_v_release_whose_version_cannot_be_read_leaves_its_version_empty_for_the_check_to_refuse() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    w.commit("a");
    w.tag("v0.2.0");
    let head = w.commit("tip");
    let facts = w.facts(&head);
    assert!(facts.lines().any(|l| l == "previous_tag=v0.2.0"), "{facts}");
    assert!(facts.lines().any(|l| l == "previous_version="), "{facts}");
}

#[test]
fn a_first_release_has_no_previous_and_identity_unknown() {
    let w = World::new();
    w.set(&constants(7, 2, 4));
    let head = w.commit("root");
    assert_eq!(
        w.facts(&head),
        "previous_tag=\nprevious_version=\nidentity=unknown\n"
    );
}

#[test]
fn the_facts_file_is_optional_and_never_overwritten() {
    let (w, head) = released((7, 2, 4), (7, 2, 4));
    let (run, _) = w.notes(&head, "owner/name");
    assert!(run.status.success());
    assert!(
        !w.dir.join("facts.txt").exists(),
        "no --facts, no facts file"
    );
    // An existing facts file: exit 1, the facts untouched, and the notes not written either.
    fs::remove_file(w.dir.join("notes.md")).expect("removes");
    fs::write(w.dir.join("facts.txt"), "precious").expect("writes");
    let run = Command::new("bash")
        .arg(script())
        .args(["--rev", &head, "--repo", "owner/name", "--out"])
        .arg(w.dir.join("notes.md"))
        .arg("--facts")
        .arg(w.dir.join("facts.txt"))
        .current_dir(&w.dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("runs the script");
    assert_eq!(
        run.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(w.dir.join("facts.txt")).expect("reads"),
        "precious"
    );
    assert!(!w.dir.join("notes.md").exists(), "nothing half-written");
    // A usage error writes no facts either.
    fs::remove_file(w.dir.join("facts.txt")).expect("removes");
    let run = Command::new("bash")
        .arg(script())
        .args(["--rev", &head, "--repo", "not-a-repo-name", "--out"])
        .arg(w.dir.join("notes.md"))
        .arg("--facts")
        .arg(w.dir.join("facts.txt"))
        .current_dir(&w.dir)
        .output()
        .expect("runs the script");
    assert_eq!(run.status.code(), Some(2));
    assert!(!w.dir.join("facts.txt").exists());
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

/// ADR 0055 decision 3 names `v*`, and the check must fail closed. A `v` tag that is not a plain `v<N>.<N>.<N>` (a
/// stray `v1.0`, a `vendor-snapshot`) is still found as the previous release, and the version check refuses it by its
/// name, whatever its `Cargo.toml` says (review 1 of PR #97). A narrower pattern would skip it silently and, with no
/// other release before the rev, call the dispatch a first release and accept any version.
#[test]
fn a_stray_v_tag_is_found_as_the_previous_release_and_the_version_check_refuses_it() {
    for stray in ["v1.0", "vendor-snapshot"] {
        let w = World::new();
        let mut files = constants(7, 2, 4).to_vec();
        files.push(cargo("0.1.0"));
        w.set(&files);
        w.commit("a");
        w.tag(stray);
        let head = w.commit("tip");
        let facts = w.facts(&head);
        assert!(
            facts.lines().any(|l| l == format!("previous_tag={stray}")),
            "{stray} is found as the previous release:\n{facts}"
        );
        let facts_file = w.dir.join("facts-for-check.txt");
        fs::write(&facts_file, &facts).expect("writes the facts");
        let check = Command::new("bash")
            .arg(script().with_file_name("release-version-check.sh"))
            .args(["check", "--version", "0.2.0", "--facts"])
            .arg(&facts_file)
            .output()
            .expect("runs the version check");
        assert_ne!(
            check.status.code(),
            Some(0),
            "{stray}: the version check must refuse, never accept it as a predecessor"
        );
        assert!(
            String::from_utf8_lossy(&check.stderr).contains(&format!("'{stray}' is neither")),
            "{stray}: refused by its name:\n{}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}
