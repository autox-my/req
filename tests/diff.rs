//! `req diff` (ADR 0026, spec `docs/specs/123-req-diff.md` unit 123-1): what changed in meaning
//! between two revisions of a corpus.
//!
//! The public seam is the compiled `req`, run against **temporary git repositories this file
//! builds with real commits**. Every expectation is written by hand from the ADR or taken from a
//! labelled corpus pair (`bench/corpus/parity.yml`); none is read back from `diff`'s own output.
//! The lettered tests (a)–(r) are the spec's list, in its order.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A base directory outside any checkout: a fixture under this repository's own `.git` or
/// `claims/` would test the wrong repository (and the "no git work tree" case could not exist).
fn fixture_base() -> PathBuf {
    for base in [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ] {
        if base.is_dir()
            && !base
                .ancestors()
                .any(|d| req::root::is_corpus_root(d) || d.join(".git").exists())
        {
            return fs::canonicalize(&base).expect("the fixture base resolves");
        }
    }
    panic!("no fixture base outside a checkout: TMPDIR, /tmp and /var/tmp are all inside one");
}

/// A fresh directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = fixture_base().join(format!("req-diff-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the fixture directory");
        Dir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `git` with the user's configuration out of the way and an identity that is the fixture's own
/// (it lives in the environment of these child processes only).
fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "req diff test")
        .env("GIT_AUTHOR_EMAIL", "diff@example.invalid")
        .env("GIT_COMMITTER_NAME", "req diff test")
        .env("GIT_COMMITTER_EMAIL", "diff@example.invalid")
        .env("GIT_AUTHOR_DATE", "2020-01-02T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2020-01-02T00:00:00Z")
        .output()
        .expect("runs git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

const REGISTRY: &str = "\
schema: req.subject_registry/0
version: 1
subjects:
  SYSTEM:
    label: The system
    aliases:
      - the system
";

/// A claim file. `tests` and `pins` are authored fields outside the statement.
fn claim_file(id: &str, statement: &str, tests: &[&str], pins: &str) -> String {
    let statement = serde_json::to_string(statement).expect("a JSON string");
    let tests = if tests.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = tests.iter().map(|t| format!("  - \"{t}\"\n")).collect();
        format!("tests:\n{}", items.concat())
    };
    format!(
        "schema: req.claim/2\nclaim_id: {id}\nstatement: {statement}\napplicability: []\n{tests}\
         rationale:\n  source: tests/diff.rs\n  date: 2026-11-05\n  rejected_alternatives: []\n  assumptions: []\nrelations: []\n{pins}"
    )
}

/// A throwaway repository holding one corpus at `prefix` ("" = the repository root).
struct Repo {
    dir: Dir,
    prefix: String,
}

impl Repo {
    fn new(name: &str) -> Self {
        Self::with_prefix(name, "")
    }

    fn with_prefix(name: &str, prefix: &str) -> Self {
        let dir = Dir::new(name);
        git_in(dir.path(), &["init", "-q", "-b", "main"]);
        let repo = Repo {
            dir,
            prefix: prefix.to_string(),
        };
        repo.write("aliases/subjects.yml", REGISTRY);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The corpus root: the repository, or the subdirectory holding `claims/`.
    fn root(&self) -> PathBuf {
        if self.prefix.is_empty() {
            self.path().to_path_buf()
        } else {
            self.path().join(&self.prefix)
        }
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.root().join(rel);
        fs::create_dir_all(path.parent().expect("a parent")).expect("creates the parent");
        fs::write(path, text).expect("writes the file");
    }

    fn claim(&self, id: &str, statement: &str) {
        self.claim_full(id, statement, &[], "");
    }

    fn claim_full(&self, id: &str, statement: &str, tests: &[&str], pins: &str) {
        self.write(
            &format!("claims/{id}.yml"),
            &claim_file(id, statement, tests, pins),
        );
    }

    fn remove(&self, id: &str) {
        fs::remove_file(self.root().join(format!("claims/{id}.yml"))).expect("removes the claim");
    }

    /// Commit everything and tag it, so a test names a revision by `tag`.
    fn commit(&self, tag: &str) {
        git_in(self.path(), &["add", "-A"]);
        git_in(self.path(), &["commit", "-q", "--allow-empty", "-m", tag]);
        git_in(self.path(), &["tag", tag]);
    }

    fn sha(&self, rev: &str) -> String {
        git_in(self.path(), &["rev-parse", rev])
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// `req --root <root> diff <args>` from `cwd`, with a hook to adjust the environment.
fn run_with(cwd: &Path, root: &Path, args: &[&str], adjust: impl FnOnce(&mut Command)) -> Run {
    let mut command = Command::new(req_bin());
    command
        .arg("--root")
        .arg(root)
        .arg("diff")
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT");
    adjust(&mut command);
    let output = command.output().expect("runs the compiled binary");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn diff(repo: &Repo, args: &[&str]) -> Run {
    run_with(repo.path(), &repo.root(), args, |_| {})
}

const CLASSES: &[&str] = &[
    "added",
    "removed",
    "weakened",
    "tightened",
    "reshaped",
    "not-judged",
    "reworded",
    "metadata",
];

/// The listed claims of a text report: id → (class, the remaining words of its line).
fn listed(stdout: &str) -> BTreeMap<String, (String, Vec<String>)> {
    let mut out = BTreeMap::new();
    for line in stdout.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let [class, id, rest @ ..] = words.as_slice()
            && CLASSES.contains(class)
        {
            out.insert(
                (*id).to_string(),
                (
                    (*class).to_string(),
                    rest.iter().map(|w| (*w).to_string()).collect(),
                ),
            );
        }
    }
    out
}

fn class_of(run: &Run, id: &str) -> String {
    listed(&run.stdout)
        .get(id)
        .map(|(class, _)| class.clone())
        .unwrap_or_else(|| format!("(not listed)\n{}\n{}", run.stdout, run.stderr))
}

fn words_of(run: &Run, id: &str) -> Vec<String> {
    listed(&run.stdout)
        .get(id)
        .map(|(_, rest)| rest.clone())
        .unwrap_or_default()
}

fn has_word(run: &Run, id: &str, word: &str) -> bool {
    words_of(run, id).iter().any(|w| w == word)
}

const SHALL_200: &str = "The system shall respond within 200 ms.";

/// Two commits of one corpus: `base` holds `before`, `head` holds `after`, for claim REQ-0001.
fn one_claim_change(name: &str, before: &str, after: &str) -> Repo {
    let repo = Repo::new(name);
    repo.claim("REQ-0001", before);
    repo.commit("base");
    repo.claim("REQ-0001", after);
    repo.commit("head");
    repo
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn identical_revisions_report_counts_only_and_list_nothing() {
    let repo = Repo::new("identical");
    repo.claim("REQ-0001", SHALL_200);
    repo.claim("REQ-0002", "The system shall log each command.");
    repo.commit("base");
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(
        listed(&run.stdout).is_empty(),
        "nothing is listed:\n{}",
        run.stdout
    );
    assert!(run.stdout.contains("registry: unchanged"), "{}", run.stdout);
    assert!(run.stdout.contains("unchanged 2"), "{}", run.stdout);
    assert!(run.stdout.contains("added 0"), "{}", run.stdout);
    let first = run.stdout.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("# req diff: base (") && first.contains(") -> head ("),
        "the header names both labels with their resolved SHAs: {first}"
    );
    assert!(
        first.contains(&repo.sha("base")[..12]) && first.contains(&repo.sha("head")[..12]),
        "{first}"
    );
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn shall_to_should_is_weakened_and_the_reverse_is_tightened() {
    let weaker = one_claim_change(
        "shall-should",
        SHALL_200,
        "The system should respond within 200 ms.",
    );
    let run = diff(&weaker, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened");
    assert!(
        has_word(&run, "REQ-0001", "modality_weakening"),
        "{}",
        run.stdout
    );

    let stronger = one_claim_change(
        "should-shall",
        "The system should respond within 200 ms.",
        SHALL_200,
    );
    let run = diff(&stronger, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "tightened");
    assert!(
        has_word(&run, "REQ-0001", "modality_strengthening"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_guard_added_is_weakened_and_a_guard_removed_is_tightened() {
    // ADR 0041 amends ADR 0026 §2: a condition makes an obligation bind less often.
    let plain = "The system shall stop within 2 s.";
    let guarded = "The system shall stop within 2 s when the tank is full.";
    let run = diff(
        &one_claim_change("guard-added", plain, guarded),
        &["--base", "base", "--head", "head"],
    );
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "scope_narrowing"),
        "{}",
        run.stdout
    );

    let run = diff(
        &one_claim_change("guard-removed", guarded, plain),
        &["--base", "base", "--head", "head"],
    );
    assert_eq!(class_of(&run, "REQ-0001"), "tightened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "scope_widening"),
        "{}",
        run.stdout
    );

    // `--fail-on weakened` catches the added guard: the change this amendment exists for.
    let run = diff(
        &one_claim_change("guard-fail-on", plain, guarded),
        &["--base", "base", "--head", "head", "--fail-on", "weakened"],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
}

#[test]
fn a_guard_on_a_permission_is_reshaped() {
    let run = diff(
        &one_claim_change(
            "guard-may",
            "The system may stop within 2 s.",
            "The system may stop within 2 s when the tank is full.",
        ),
        &["--base", "base", "--head", "head"],
    );
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "reshaped", "{}", run.stdout);
}

// (c) ------------------------------------------------------------------------------------------

#[test]
fn a_looser_upper_bound_is_weakened_and_a_tighter_one_is_tightened() {
    let looser = one_claim_change(
        "bound-up",
        SHALL_200,
        "The system shall respond within 300 ms.",
    );
    let run = diff(&looser, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "numeric_loosening"),
        "{}",
        run.stdout
    );

    let tighter = one_claim_change(
        "bound-down",
        "The system shall respond within 300 ms.",
        SHALL_200,
    );
    let run = diff(&tighter, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "tightened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "numeric_tightening"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_polarity_flip_is_reshaped_and_an_unparsed_side_is_not_judged() {
    let flip = one_claim_change(
        "polarity",
        "The system shall log each command.",
        "The system shall not log each command.",
    );
    let run = diff(&flip, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "reshaped", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "polarity_flip"),
        "{}",
        run.stdout
    );

    // Prose with no modality has no structured form (REQ-0007's fallback): no direction is
    // claimed, and the class says so rather than folding it into "changed".
    let prose = one_claim_change(
        "prose",
        "Requirements are decisions, not documents.",
        "Requirements are claims, not documents.",
    );
    let run = diff(&prose, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "not-judged", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "unclassifiable"),
        "{}",
        run.stdout
    );
}

// (d) (e) --------------------------------------------------------------------------------------

fn parity() -> req::model::Corpus {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/corpus/parity.yml");
    req::model::Corpus::load(path.to_str().expect("a UTF-8 path")).expect("loads the seed corpus")
}

fn pair_repo(name: &str, pairs: &[&req::model::Pair]) -> Repo {
    let repo = Repo::new(name);
    for (i, pair) in pairs.iter().enumerate() {
        repo.claim(&format!("REQ-{:04}", i + 1), &pair.a);
    }
    repo.commit("base");
    for (i, pair) in pairs.iter().enumerate() {
        repo.claim(&format!("REQ-{:04}", i + 1), &pair.b);
    }
    repo.commit("head");
    repo
}

#[test]
fn pairs_labelled_equivalent_by_tier_1_are_reworded_not_changed_in_meaning() {
    let corpus = parity();
    // E-001..E-005 are the pairs Tier-1 collapses (hashing spec T1.2-T1.5); E-006..E-008 are
    // documented false splits (Tier-2 proposals) and are deliberately not asserted here.
    let pairs: Vec<&req::model::Pair> = corpus
        .equivalent
        .iter()
        .filter(|p| ["E-001", "E-002", "E-003", "E-004", "E-005"].contains(&p.id.as_str()))
        .collect();
    assert_eq!(
        pairs.len(),
        5,
        "the five Tier-1 equivalent pairs are in the corpus"
    );
    let repo = pair_repo("equivalent", &pairs);
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    for (i, pair) in pairs.iter().enumerate() {
        let id = format!("REQ-{:04}", i + 1);
        assert_eq!(
            class_of(&run, &id),
            "reworded",
            "{} ({:?} -> {:?}) is the same meaning in other words\n{}",
            pair.id,
            pair.a,
            pair.b,
            run.stdout
        );
    }
}

#[test]
fn pairs_labelled_not_equivalent_are_never_reworded() {
    let corpus = parity();
    let pairs: Vec<&req::model::Pair> = corpus.not_equivalent.iter().collect();
    assert!(!pairs.is_empty(), "the seed corpus has trap pairs");
    let repo = pair_repo("not-equivalent", &pairs);
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    let report = listed(&run.stdout);
    for (i, pair) in pairs.iter().enumerate() {
        let id = format!("REQ-{:04}", i + 1);
        let class = report.get(&id).map(|(c, _)| c.as_str());
        assert!(
            class.is_some() && class != Some("reworded"),
            "{} ({:?} -> {:?}) changed meaning and must be listed under a class that says so, \
             not as a rewording; got {class:?}\n{}",
            pair.id,
            pair.a,
            pair.b,
            run.stdout
        );
    }
    // The value trap N-003 (200 ms -> 250 ms on an upper bound) is hand-classified: a larger
    // upper bound permits more.
    let n003 = pairs
        .iter()
        .position(|p| p.id == "N-003")
        .expect("N-003 is in the corpus");
    assert_eq!(class_of(&run, &format!("REQ-{:04}", n003 + 1)), "weakened");
    // N-002 (LE vs LT) is a tightening and N-001 (shall -> may) a weakening.
    let n002 = pairs.iter().position(|p| p.id == "N-002").expect("N-002");
    assert_eq!(class_of(&run, &format!("REQ-{:04}", n002 + 1)), "tightened");
    let n001 = pairs.iter().position(|p| p.id == "N-001").expect("N-001");
    assert_eq!(class_of(&run, &format!("REQ-{:04}", n001 + 1)), "weakened");
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn added_and_removed_ids_are_listed_by_id_and_sorted() {
    let repo = Repo::new("added-removed");
    repo.claim("REQ-0001", SHALL_200);
    repo.claim("REQ-0002", "The system shall log each command.");
    repo.commit("base");
    repo.remove("REQ-0002");
    repo.claim("REQ-0010", "The system shall retain each log for 30 days.");
    repo.claim("REQ-0003", "The system shall reject malformed input.");
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0002"), "removed");
    assert_eq!(class_of(&run, "REQ-0003"), "added");
    assert_eq!(class_of(&run, "REQ-0010"), "added");
    assert!(
        !listed(&run.stdout).contains_key("REQ-0001"),
        "REQ-0001 is unchanged"
    );
    let order: Vec<String> = run
        .stdout
        .lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let (class, id) = (words.next()?, words.next()?);
            CLASSES.contains(&class).then(|| id.to_string())
        })
        .collect();
    assert_eq!(
        order,
        ["REQ-0002", "REQ-0003", "REQ-0010"],
        "sorted by claim id"
    );
    assert!(
        run.stdout.contains("added 2, removed 1") && run.stdout.contains("unchanged 1"),
        "{}",
        run.stdout
    );
}

// (g) ------------------------------------------------------------------------------------------

#[test]
fn equal_addresses_are_a_candidate_renumber_never_a_rename() {
    let repo = Repo::new("renumber");
    repo.claim("REQ-0005", SHALL_200);
    repo.commit("base");
    repo.remove("REQ-0005");
    // The same meaning in other words, under a new id.
    repo.claim("REQ-0006", "The system shall respond no later than 200 ms.");
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(
        class_of(&run, "REQ-0005"),
        "removed",
        "the id is the identity"
    );
    assert_eq!(
        class_of(&run, "REQ-0006"),
        "added",
        "the id is the identity"
    );
    let words = words_of(&run, "REQ-0006");
    let at = words.iter().position(|w| w == "same-address-as");
    assert_eq!(
        at.and_then(|i| words.get(i + 1)).map(String::as_str),
        Some("REQ-0005"),
        "the added claim names the removed one: {words:?}"
    );
    assert!(
        !words_of(&run, "REQ-0005")
            .iter()
            .any(|w| w == "same-address-as"),
        "the flag is on the added claim only"
    );
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn a_removed_test_link_alone_is_metadata_with_tests_lost() {
    let repo = Repo::new("tests-lost");
    repo.claim_full("REQ-0001", SHALL_200, &["a::one", "a::two"], "");
    repo.commit("base");
    repo.claim_full("REQ-0001", SHALL_200, &["a::one"], "");
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "metadata", "{}", run.stdout);
    assert!(has_word(&run, "REQ-0001", "tests-lost"), "{}", run.stdout);
    assert!(!has_word(&run, "REQ-0001", "tests-added"), "{}", run.stdout);

    // The reverse is `tests-added`, and moving a link (one lost, one gained) carries both.
    let swap = Repo::new("tests-swap");
    swap.claim_full("REQ-0001", SHALL_200, &["a::one"], "");
    swap.commit("base");
    swap.claim_full("REQ-0001", SHALL_200, &["a::renamed"], "");
    swap.commit("head");
    let run = diff(&swap, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "metadata");
    assert!(has_word(&run, "REQ-0001", "tests-lost"), "{}", run.stdout);
    assert!(has_word(&run, "REQ-0001", "tests-added"), "{}", run.stdout);

    // Reordering the same ids changes nothing: the link list is an order-free set.
    let order = Repo::new("tests-order");
    order.claim_full("REQ-0001", SHALL_200, &["a::one", "a::two"], "");
    order.commit("base");
    order.claim_full("REQ-0001", SHALL_200, &["a::two", "a::one"], "");
    order.commit("head");
    let run = diff(&order, &["--base", "base", "--head", "head"]);
    assert!(listed(&run.stdout).is_empty(), "{}", run.stdout);
}

// (i) ------------------------------------------------------------------------------------------

const PINS_HOLD: &str = "pins:\n  modality: SHALL\n  operator: LE\n  value: \"0.2\"\n  unit: s\n";

#[test]
fn an_edited_pin_is_pins_changed_and_one_that_no_longer_holds_is_pin_flagged() {
    // 200 ms parses to 0.2 s: the base pins hold. The head edits the pinned value to 0.3 — the
    // way a flagged parse gets silenced, here pinned to a bound the statement does not say.
    let repo = Repo::new("pins");
    repo.claim_full("REQ-0001", SHALL_200, &[], PINS_HOLD);
    repo.commit("base");
    repo.claim_full(
        "REQ-0001",
        SHALL_200,
        &[],
        "pins:\n  modality: SHALL\n  operator: LE\n  value: \"0.3\"\n  unit: s\n",
    );
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "metadata", "{}", run.stdout);
    assert!(has_word(&run, "REQ-0001", "pins-changed"), "{}", run.stdout);
    assert!(has_word(&run, "REQ-0001", "pin-flagged"), "{}", run.stdout);

    // An edit that still holds is `pins-changed` and not `pin-flagged`.
    let holds = Repo::new("pins-hold");
    holds.claim_full("REQ-0001", SHALL_200, &[], "pins:\n  modality: SHALL\n");
    holds.commit("base");
    holds.claim_full("REQ-0001", SHALL_200, &[], PINS_HOLD);
    holds.commit("head");
    let run = diff(&holds, &["--base", "base", "--head", "head"]);
    assert!(has_word(&run, "REQ-0001", "pins-changed"), "{}", run.stdout);
    assert!(!has_word(&run, "REQ-0001", "pin-flagged"), "{}", run.stdout);

    // The same pins in another key order are the same pins (structural, not file bytes).
    let reordered = Repo::new("pins-reordered");
    reordered.claim_full("REQ-0001", SHALL_200, &[], PINS_HOLD);
    reordered.commit("base");
    reordered.claim_full(
        "REQ-0001",
        SHALL_200,
        &[],
        "pins:\n  unit: s\n  value: \"0.2\"\n  operator: LE\n  modality: SHALL\n",
    );
    reordered.commit("head");
    let run = diff(&reordered, &["--base", "base", "--head", "head"]);
    assert!(
        listed(&run.stdout).is_empty(),
        "a reordered pin block is not a change:\n{}",
        run.stdout
    );
}

#[test]
fn a_pin_that_disagrees_after_a_weakening_is_flagged_beside_the_class() {
    // The statement is loosened to 300 ms and the pin left at 0.2: the class says weakened and
    // the pin flag says the author's own bound no longer matches the parse.
    let repo = Repo::new("pin-after-weakening");
    repo.claim_full("REQ-0001", SHALL_200, &[], PINS_HOLD);
    repo.commit("base");
    repo.claim_full(
        "REQ-0001",
        "The system shall respond within 300 ms.",
        &[],
        PINS_HOLD,
    );
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert!(has_word(&run, "REQ-0001", "pin-flagged"), "{}", run.stdout);
    assert!(
        !has_word(&run, "REQ-0001", "pins-changed"),
        "{}",
        run.stdout
    );
}

// (j) ------------------------------------------------------------------------------------------

#[test]
fn an_edited_registry_is_reported_once_at_the_top() {
    let repo = Repo::new("registry");
    repo.claim("REQ-0001", SHALL_200);
    repo.commit("base");
    repo.write(
        "aliases/subjects.yml",
        &REGISTRY.replace(
            "      - the system\n",
            "      - the system\n      - the unit\n",
        ),
    );
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("registry: changed"), "{}", run.stdout);
    assert_eq!(
        run.stdout.matches("registry:").count(),
        1,
        "reported once:\n{}",
        run.stdout
    );
    // The statement text is byte-identical, so the claim is counted and not listed: the reader
    // must not mistake a registry move for an edited statement.
    assert!(listed(&run.stdout).is_empty(), "{}", run.stdout);
}

#[test]
fn a_registry_edit_that_makes_a_pin_disagree_lists_the_claim_pin_flagged() {
    // The claim file is byte-identical on both sides. The head registry moves the alias the
    // statement's subject resolved through, so the head parse no longer agrees with the pinned
    // subject: an unchanged-looking claim that a CI gate on `pin-flagged` must still see.
    let pinned = "pins:\n  subject: SYSTEM\n";
    let repo = Repo::new("registry-pin");
    repo.write("aliases/subjects.yml", REGISTRY);
    repo.claim_full("REQ-0001", SHALL_200, &[], pinned);
    repo.commit("base");
    repo.write(
        "aliases/subjects.yml",
        &REGISTRY.replace("      - the system\n", "      - the unit\n"),
    );
    repo.commit("head");
    let run = diff(
        &repo,
        &[
            "--base",
            "base",
            "--head",
            "head",
            "--fail-on",
            "pin-flagged",
        ],
    );
    assert!(
        has_word(&run, "REQ-0001", "pin-flagged"),
        "{}{}",
        run.stdout,
        run.stderr
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);

    // A disagreement that was already there on both sides is not a change and is not listed.
    let before = Repo::new("registry-pin-same");
    before.write(
        "aliases/subjects.yml",
        &REGISTRY.replace("      - the system\n", "      - the unit\n"),
    );
    before.claim_full("REQ-0001", SHALL_200, &[], pinned);
    before.commit("base");
    before.claim_full("REQ-0002", "The system shall stay up.", &[], "");
    before.commit("head");
    let run = diff(&before, &["--base", "base", "--head", "head"]);
    assert!(!has_word(&run, "REQ-0001", "pin-flagged"), "{}", run.stdout);
}

// (k) ------------------------------------------------------------------------------------------

#[test]
fn fail_on_turns_a_named_class_into_exit_1_and_nothing_else() {
    let weakening = one_claim_change(
        "fail-weak",
        SHALL_200,
        "The system should respond within 200 ms.",
    );
    let range = ["--base", "base", "--head", "head"];
    let with = |extra: &[&str]| -> Run {
        let mut args = range.to_vec();
        args.extend_from_slice(extra);
        diff(&weakening, &args)
    };

    let run = with(&["--fail-on", "weakened"]);
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("fail-on: weakened — 1 triggered"),
        "{}",
        run.stdout
    );
    assert_eq!(
        class_of(&run, "REQ-0001"),
        "weakened",
        "the report is still printed"
    );

    // The same weakening without `--fail-on` is exit 0: the report is the answer.
    assert_eq!(with(&[]).code, 0);
    // A listed class that does not occur is not a failure.
    let run = with(&[
        "--fail-on",
        "removed,reshaped,not-judged,pins-changed,pin-flagged,tests-lost",
    ]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("none triggered"), "{}", run.stdout);
    // An unlisted class is still printed.
    assert_eq!(class_of(&run, "REQ-0001"), "weakened");

    // A tightening does not trip `weakened`.
    let tightening = one_claim_change(
        "fail-tight",
        "The system should respond within 200 ms.",
        SHALL_200,
    );
    let run = diff(
        &tightening,
        &["--base", "base", "--head", "head", "--fail-on", "weakened"],
    );
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
}

#[test]
fn every_named_class_and_flag_can_fail_the_run() {
    let removal = Repo::new("fail-removed");
    removal.claim("REQ-0001", SHALL_200);
    removal.claim("REQ-0002", "The system shall log each command.");
    removal.commit("base");
    removal.remove("REQ-0002");
    removal.commit("head");
    let run = diff(
        &removal,
        &["--base", "base", "--head", "head", "--fail-on", "removed"],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);

    let reshape = one_claim_change(
        "fail-reshaped",
        "The system shall log each command.",
        "The system shall not log each command.",
    );
    let run = diff(
        &reshape,
        &["--base", "base", "--head", "head", "--fail-on", "reshaped"],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);

    let prose = one_claim_change(
        "fail-notjudged",
        "Requirements are decisions, not documents.",
        "Requirements are claims, not documents.",
    );
    let run = diff(
        &prose,
        &[
            "--base",
            "base",
            "--head",
            "head",
            "--fail-on",
            "not-judged",
        ],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);

    let lost = Repo::new("fail-tests-lost");
    lost.claim_full("REQ-0001", SHALL_200, &["a::one"], "");
    lost.commit("base");
    lost.claim_full("REQ-0001", SHALL_200, &[], "");
    lost.commit("head");
    let run = diff(
        &lost,
        &[
            "--base",
            "base",
            "--head",
            "head",
            "--fail-on",
            "tests-lost",
        ],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    // `tests-added` is not a `--fail-on` spelling (ADR 0026 §4).
    let run = diff(
        &lost,
        &[
            "--base",
            "base",
            "--head",
            "head",
            "--fail-on",
            "tests-added",
        ],
    );
    assert_eq!(run.code, 2);

    let pins = Repo::new("fail-pins");
    pins.claim_full("REQ-0001", SHALL_200, &[], PINS_HOLD);
    pins.commit("base");
    pins.claim_full(
        "REQ-0001",
        SHALL_200,
        &[],
        "pins:\n  modality: SHALL\n  operator: LE\n  value: \"0.3\"\n  unit: s\n",
    );
    pins.commit("head");
    for class in ["pins-changed", "pin-flagged"] {
        let run = diff(
            &pins,
            &["--base", "base", "--head", "head", "--fail-on", class],
        );
        assert_eq!(run.code, 1, "{class}: {}{}", run.stdout, run.stderr);
    }
}

#[test]
fn every_bad_fail_on_spelling_is_a_usage_error() {
    let repo = one_claim_change(
        "fail-bad",
        SHALL_200,
        "The system should respond within 200 ms.",
    );
    for bad in [
        "",
        ",",
        "weakened,",
        ",weakened",
        "weakened,,removed",
        "bogus",
        "WEAKENED",
        " weakened",
        "weakened ",
        "added",
        "tightened",
        "reworded",
        "metadata",
        "same-address-as",
        "-weakened",
        "weakened;removed",
    ] {
        let run = diff(
            &repo,
            &["--base", "base", "--head", "head", "--fail-on", bad],
        );
        assert_eq!(
            run.code, 2,
            "--fail-on {bad:?} is a usage error:\n{}{}",
            run.stdout, run.stderr
        );
        assert!(
            run.stdout.is_empty(),
            "no report on a usage error: {:?}",
            run.stdout
        );
        assert!(
            run.stderr
                .contains("usage: req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]"),
            "the usage line is named: {}",
            run.stderr
        );
    }
    // A missing value, and a missing `--base`, an extra positional, an unknown flag.
    for args in [
        vec!["--base", "base", "--fail-on"],
        vec!["--head", "head"],
        vec!["--base"],
        vec!["--base", "base", "extra"],
        vec!["--base", "base", "--bogus"],
        vec!["--base", "base", "--base", "head"],
        vec!["--base", "base", "--head", "head", "--head", "head"],
        vec![
            "--base",
            "base",
            "--fail-on",
            "weakened",
            "--fail-on",
            "removed",
        ],
        vec!["--base", "", "--head", "head"],
        vec![],
    ] {
        let run = diff(&repo, &args);
        assert_eq!(run.code, 2, "{args:?}:\n{}{}", run.stdout, run.stderr);
        assert!(run.stdout.is_empty(), "{args:?}: {:?}", run.stdout);
    }
}

// (l) ------------------------------------------------------------------------------------------

#[test]
fn head_absent_compares_the_working_tree_including_an_uncommitted_edit() {
    let repo = Repo::new("working-tree");
    repo.claim("REQ-0001", SHALL_200);
    repo.commit("base");
    // Uncommitted: an edited claim and an untracked new one.
    repo.claim("REQ-0001", "The system should respond within 200 ms.");
    repo.claim("REQ-0002", "The system shall log each command.");
    let run = diff(&repo, &["--base", "base"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert_eq!(class_of(&run, "REQ-0002"), "added", "{}", run.stdout);
    let first = run.stdout.lines().next().unwrap_or_default();
    assert!(first.ends_with("-> working-tree"), "{first}");
    // Nothing was written into the work tree or the repository by reading it.
    assert_eq!(
        git_in(repo.path(), &["status", "--porcelain"])
            .lines()
            .count(),
        2
    );
}

// (m) ------------------------------------------------------------------------------------------

fn assert_unverifiable(run: &Run, what: &str) {
    assert_eq!(
        run.code, 3,
        "{what}: exit 3 expected\n{}{}",
        run.stdout, run.stderr
    );
    assert!(
        run.stdout.is_empty(),
        "{what}: no report on stdout, got {:?}",
        run.stdout
    );
    assert!(
        run.stderr.contains("[UNVERIFIABLE]"),
        "{what}: the cause is named on stderr: {}",
        run.stderr
    );
}

#[test]
fn a_report_that_cannot_be_produced_is_exit_3_with_no_stdout() {
    let repo = one_claim_change(
        "unverifiable",
        SHALL_200,
        "The system should respond within 200 ms.",
    );

    let run = diff(&repo, &["--base", "no-such-rev", "--head", "head"]);
    assert_unverifiable(&run, "an unknown base revision");
    assert!(run.stderr.contains("no-such-rev"), "{}", run.stderr);
    let run = diff(&repo, &["--base", "base", "--head", "no-such-rev"]);
    assert_unverifiable(&run, "an unknown head revision");

    // `git` missing from PATH is never a silent empty diff.
    let empty_path = Dir::new("no-git-path");
    let run = run_with(
        repo.path(),
        &repo.root(),
        &["--base", "base", "--head", "head"],
        |c| {
            c.env("PATH", empty_path.path());
        },
    );
    assert_unverifiable(&run, "git is not on PATH");
    assert!(run.stderr.to_lowercase().contains("git"), "{}", run.stderr);
}

#[test]
fn a_corpus_outside_a_git_work_tree_is_exit_3_naming_that() {
    let plain = Dir::new("no-repo");
    fs::create_dir_all(plain.path().join("claims")).expect("claims/");
    fs::create_dir_all(plain.path().join("aliases")).expect("aliases/");
    fs::write(plain.path().join("aliases/subjects.yml"), REGISTRY).expect("registry");
    fs::write(
        plain.path().join("claims/REQ-0001.yml"),
        claim_file("REQ-0001", SHALL_200, &[], ""),
    )
    .expect("claim");
    let run = run_with(plain.path(), plain.path(), &["--base", "HEAD"], |_| {});
    assert_unverifiable(&run, "a directory with no repository");
    assert!(
        run.stderr.contains("git work tree"),
        "the cause is that the corpus is not inside a git work tree: {}",
        run.stderr
    );
}

#[test]
fn a_side_that_declares_another_claim_format_is_exit_3() {
    let repo = Repo::new("claim-v1");
    repo.write(
        "claims/REQ-0001.yml",
        &claim_file("REQ-0001", SHALL_200, &[], "").replace("req.claim/2", "req.claim/1"),
    );
    repo.commit("base");
    repo.claim("REQ-0001", SHALL_200);
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_unverifiable(&run, "a base side declaring req.claim/1");
    assert!(run.stderr.contains("req.claim/1"), "{}", run.stderr);
    // The same on the head side.
    let run = diff(&repo, &["--base", "head", "--head", "base"]);
    assert_unverifiable(&run, "a head side declaring req.claim/1");
}

#[test]
fn an_empty_claims_directory_at_one_side_is_exit_3() {
    let repo = Repo::new("empty-claims");
    repo.write("claims/README.md", "no claims yet\n");
    repo.commit("base");
    repo.claim("REQ-0001", SHALL_200);
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_unverifiable(&run, "an empty claims/ at the base");
    let run = diff(&repo, &["--base", "head", "--head", "base"]);
    assert_unverifiable(&run, "an empty claims/ at the head");

    // A revision with no claims/ directory at all is the same answer.
    let none = Repo::new("no-claims-dir");
    none.write("README.md", "docs only\n");
    none.commit("base");
    none.claim("REQ-0001", SHALL_200);
    none.commit("head");
    let run = diff(&none, &["--base", "base", "--head", "head"]);
    assert_unverifiable(&run, "no claims/ at the base");
}

#[test]
fn an_unreadable_registry_at_one_side_is_exit_3() {
    let repo = Repo::new("bad-registry");
    repo.claim("REQ-0001", SHALL_200);
    repo.commit("base");
    repo.write("aliases/subjects.yml", "this: is: not: a registry\n");
    repo.commit("head");
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_unverifiable(&run, "a registry that does not parse");
}

// (n) ------------------------------------------------------------------------------------------

#[test]
fn a_corpus_in_a_subdirectory_is_read_at_its_prefix() {
    let repo = Repo::with_prefix("subdir", "docs/reqs");
    repo.claim("REQ-0001", SHALL_200);
    // A decoy corpus at the repository root must not be read in its place.
    fs::create_dir_all(repo.path().join("claims")).expect("decoy claims/");
    fs::create_dir_all(repo.path().join("aliases")).expect("decoy aliases/");
    fs::write(repo.path().join("aliases/subjects.yml"), REGISTRY).expect("decoy registry");
    fs::write(
        repo.path().join("claims/REQ-0001.yml"),
        claim_file("REQ-0001", "The system shall log each command.", &[], ""),
    )
    .expect("decoy claim");
    repo.commit("base");
    repo.claim("REQ-0001", "The system should respond within 200 ms.");
    repo.commit("head");

    // Both from the repository top (--root names the subdirectory) and from inside it.
    let run = diff(&repo, &["--base", "base", "--head", "head"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "modality_weakening"),
        "{}",
        run.stdout
    );
    let inside = run_with(
        &repo.root(),
        &repo.root(),
        &["--base", "base", "--head", "head"],
        |_| {},
    );
    assert_eq!(
        inside.stdout, run.stdout,
        "the cwd inside the corpus changes nothing"
    );
}

// (o) ------------------------------------------------------------------------------------------

fn mixed_repo(name: &str) -> Repo {
    let repo = Repo::new(name);
    repo.claim("REQ-0001", SHALL_200);
    repo.claim("REQ-0002", "The system shall log each command.");
    repo.claim("REQ-0003", "The system shall respond within 300 ms.");
    repo.claim_full("REQ-0004", SHALL_200, &["a::one"], "");
    repo.claim("REQ-0005", "The system shall reject malformed input.");
    repo.commit("base");
    repo.claim("REQ-0001", "The system should respond within 200 ms.");
    repo.remove("REQ-0002");
    repo.claim("REQ-0003", SHALL_200);
    repo.claim_full("REQ-0004", SHALL_200, &[], "");
    repo.claim("REQ-0006", "The system shall retain each log for 30 days.");
    repo.commit("head");
    repo
}

#[test]
fn json_is_req_diff_1_and_carries_the_same_classes_as_the_text() {
    let repo = mixed_repo("json");
    let args = [
        "--base",
        "base",
        "--head",
        "head",
        "--fail-on",
        "removed,weakened",
    ];
    let text = diff(&repo, &args);
    let mut json_args = args.to_vec();
    json_args.extend_from_slice(&["--format", "json"]);
    let json = diff(&repo, &json_args);
    assert_eq!(text.code, 1, "{}{}", text.stdout, text.stderr);
    assert_eq!(json.code, text.code, "the format does not move the exit");

    let doc: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("stdout is one JSON document");
    assert_eq!(doc["schema"], "req.diff/1");
    assert_eq!(doc["generator"], "diff");
    assert_eq!(doc["rules_v"], req::canon::RULES_V);
    assert_eq!(doc["registry_changed"], false);
    assert_eq!(doc["base"]["label"], "base");
    assert_eq!(doc["base"]["sha"], repo.sha("base"));
    assert_eq!(doc["head"]["label"], "head");
    assert_eq!(doc["head"]["sha"], repo.sha("head"));
    // In the order ADR 0026 §4 lists the spellings, whatever order they were given in.
    assert_eq!(doc["fail_on"], serde_json::json!(["weakened", "removed"]));
    assert_eq!(doc["triggered"], serde_json::json!(["weakened", "removed"]));

    let claims = doc["claims"].as_array().expect("claims[] is an array");
    let from_json: BTreeMap<String, String> = claims
        .iter()
        .map(|c| {
            (
                c["claim_id"].as_str().expect("claim_id").to_string(),
                c["class"].as_str().expect("class").to_string(),
            )
        })
        .collect();
    let from_text: BTreeMap<String, String> = listed(&text.stdout)
        .into_iter()
        .map(|(id, (class, _))| (id, class))
        .collect();
    assert_eq!(from_json, from_text, "JSON and text carry the same classes");
    // Hand-written expectation, not read from the output.
    let expected: BTreeMap<String, String> = [
        ("REQ-0001", "weakened"),
        ("REQ-0002", "removed"),
        ("REQ-0003", "tightened"),
        ("REQ-0004", "metadata"),
        ("REQ-0006", "added"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(from_json, expected);

    let ids: Vec<&str> = claims
        .iter()
        .map(|c| c["claim_id"].as_str().expect("id"))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "claims[] is ordered by claim id");

    let first = &claims[0];
    assert_eq!(first["direction"], "modality_weakening");
    assert!(first["base_address"].is_string() && first["head_address"].is_string());
    let removed = &claims[1];
    assert!(removed["head_address"].is_null() && removed["base_address"].is_string());
    assert!(removed["direction"].is_null());
    assert_eq!(claims[3]["flags"], serde_json::json!(["tests-lost"]));

    // Counts per class.
    assert_eq!(doc["counts"]["weakened"], 1);
    assert_eq!(doc["counts"]["removed"], 1);
    assert_eq!(doc["counts"]["added"], 1);
    assert_eq!(doc["counts"]["tightened"], 1);
    assert_eq!(doc["counts"]["metadata"], 1);
    assert_eq!(doc["counts"]["unchanged"], 1);
    assert_eq!(doc["counts"]["reworded"], 0);

    // Deterministic, keys sorted, and no float anywhere.
    let again = diff(&repo, &json_args);
    assert_eq!(again.stdout, json.stdout, "two runs are byte-identical");
    fn walk(value: &serde_json::Value) {
        match value {
            serde_json::Value::Number(n) => assert!(!n.is_f64(), "a float in the document: {n}"),
            serde_json::Value::Array(items) => items.iter().for_each(walk),
            serde_json::Value::Object(map) => {
                let keys: Vec<&String> = map.keys().collect();
                let mut sorted = keys.clone();
                sorted.sort();
                assert_eq!(keys, sorted, "keys are sorted");
                map.values().for_each(walk);
            }
            _ => {}
        }
    }
    walk(&doc);
}

#[test]
fn json_for_the_working_tree_names_it_and_a_bad_format_is_a_usage_error() {
    let repo = mixed_repo("json-wt");
    let run = diff(&repo, &["--base", "base", "--format", "json"]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    let doc: serde_json::Value = serde_json::from_str(&run.stdout).expect("JSON");
    assert_eq!(doc["head"]["label"], "working-tree");
    assert!(doc["head"]["sha"].is_null());
    assert_eq!(doc["fail_on"], serde_json::json!([]));
    assert_eq!(doc["triggered"], serde_json::json!([]));

    let run = diff(&repo, &["--base", "base", "--format", "yaml"]);
    assert_eq!(run.code, 2);
    assert!(run.stdout.is_empty());
}

// (p) ------------------------------------------------------------------------------------------

#[test]
fn the_callers_git_environment_does_not_change_the_result() {
    let repo = mixed_repo("environment");
    // A repository attribute and a global configuration that would both rewrite line endings if
    // a blob went through a filter (it must not: blobs are read raw).
    repo.write(".gitattributes", "* text eol=crlf\n");
    repo.commit("attrs");
    let clean = diff(&repo, &["--base", "base", "--head", "attrs"]);
    assert_eq!(clean.code, 0, "{}{}", clean.stdout, clean.stderr);

    let config = Dir::new("env-config");
    let global = config.path().join("gitconfig");
    fs::write(
        &global,
        "[core]\n\tautocrlf = true\n\teol = crlf\n\thooksPath = /nonexistent\n\tfsmonitor = false\n",
    )
    .expect("writes the config");
    let elsewhere = Dir::new("env-elsewhere");
    let args = ["--base", "base", "--head", "attrs"];
    let hostile = run_with(repo.path(), &repo.root(), &args, |c| {
        c.env("GIT_DIR", elsewhere.path().join("not-a-repo"))
            .env("GIT_WORK_TREE", elsewhere.path())
            .env("GIT_INDEX_FILE", elsewhere.path().join("index"))
            .env("GIT_OBJECT_DIRECTORY", elsewhere.path().join("objects"))
            .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", elsewhere.path())
            .env("GIT_CONFIG", &global)
            .env("GIT_CONFIG_GLOBAL", &global)
            .env("GIT_CONFIG_SYSTEM", &global)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.bare")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env("GIT_TEMPLATE_DIR", elsewhere.path());
    });
    assert_eq!(
        hostile.code, clean.code,
        "{}{}",
        hostile.stdout, hostile.stderr
    );
    assert_eq!(hostile.stdout, clean.stdout, "the report is the same bytes");
}

// (q) ------------------------------------------------------------------------------------------

#[test]
fn the_output_never_says_pass_fail_or_unverifiable_on_stdout() {
    let repo = mixed_repo("no-verdict");
    for args in [
        vec!["--base", "base", "--head", "head"],
        vec!["--base", "base", "--head", "head", "--fail-on", "weakened"],
        vec!["--base", "base", "--head", "head", "--fail-on", "reshaped"],
        vec!["--base", "base", "--head", "head", "--format", "json"],
        vec!["--base", "base", "--head", "base"],
        vec!["--base", "base"],
    ] {
        let run = diff(&repo, &args);
        assert!(
            run.stdout.contains("req diff") || run.stdout.contains("req.diff/1"),
            "{args:?}: a report was produced to inspect:\n{}{}",
            run.stdout,
            run.stderr
        );
        for word in ["PASS", "FAIL", "UNVERIFIABLE"] {
            assert!(
                !run.stdout.contains(word),
                "{args:?}: stdout says {word}:\n{}",
                run.stdout
            );
        }
    }
    let run = diff(&repo, &["--base", "no-such-rev"]);
    assert!(run.stdout.is_empty());
    assert!(run.stderr.contains("[UNVERIFIABLE]"), "{}", run.stderr);
}

// (r) ------------------------------------------------------------------------------------------

/// A `git` that records that it was started, then fails.
fn tripwire_git(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("bin/");
    let script = bin.join("git");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}/spawned'\nexit 1\n",
            dir.display()
        ),
    )
    .expect("writes the fake git");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

#[test]
fn a_revision_starting_with_a_dash_is_a_usage_error_before_any_process_runs() {
    let repo = one_claim_change(
        "dash",
        SHALL_200,
        "The system should respond within 200 ms.",
    );
    let trip = Dir::new("tripwire");
    let bin = tripwire_git(trip.path());
    for args in [
        vec!["--base", "--output=/tmp/pwned"],
        vec!["--base", "-x"],
        vec!["--base", "base", "--head", "--upload-pack=evil"],
        vec!["--base", "base", "--head", "-"],
    ] {
        let run = run_with(repo.path(), &repo.root(), &args, |c| {
            c.env("PATH", &bin);
        });
        assert_eq!(run.code, 2, "{args:?}:\n{}{}", run.stdout, run.stderr);
        assert!(run.stdout.is_empty(), "{args:?}");
        assert!(
            run.stderr.contains("usage: req diff --base"),
            "{args:?}: the diff usage line, not an unknown-subcommand error: {}",
            run.stderr
        );
    }
    assert!(
        !trip.path().join("spawned").exists(),
        "no git process was started for a usage error: {:?}",
        fs::read_to_string(trip.path().join("spawned"))
    );
}

// Cleanup ---------------------------------------------------------------------------------------

/// The temp dir is listed before and after; a staging directory left behind fails here.
#[test]
fn the_temporary_directory_is_removed_on_every_exit() {
    let repo = mixed_repo("cleanup");
    let tmp = Dir::new("cleanup-tmp");
    let entries = || -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(tmp.path())
            .expect("lists the temp dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert!(entries().is_empty());
    for args in [
        // report produced
        vec!["--base", "base", "--head", "head"],
        // exit 1
        vec!["--base", "base", "--head", "head", "--fail-on", "weakened"],
        // head side fails to load after the base side was staged
        vec!["--base", "base", "--head", "no-such-rev"],
    ] {
        let run = run_with(repo.path(), &repo.root(), &args, |c| {
            c.env("TMPDIR", tmp.path());
        });
        assert!(
            matches!(run.code, 0 | 1 | 3),
            "{args:?}: {}{}",
            run.stdout,
            run.stderr
        );
        assert_eq!(
            entries(),
            Vec::<String>::new(),
            "{args:?} left a directory behind"
        );
    }
    // A side that fails to load after staging.
    let bad = Repo::new("cleanup-bad");
    bad.write("claims/REQ-0001.yml", "not a claim: [\n");
    bad.commit("base");
    bad.claim("REQ-0001", SHALL_200);
    bad.commit("head");
    let run = run_with(
        bad.path(),
        &bad.root(),
        &["--base", "base", "--head", "head"],
        |c| {
            c.env("TMPDIR", tmp.path());
        },
    );
    assert_eq!(run.code, 3, "{}{}", run.stdout, run.stderr);
    assert_eq!(
        entries(),
        Vec::<String>::new(),
        "a failed load left a directory behind"
    );
}

/// A signal during the read removes the staged directory too.
#[test]
fn the_temporary_directory_is_removed_when_the_process_is_signalled() {
    let repo = mixed_repo("signal");
    let tmp = Dir::new("signal-tmp");
    let fake = Dir::new("signal-git");
    let real_git = String::from_utf8(
        Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .expect("finds git")
            .stdout,
    )
    .expect("utf-8")
    .trim()
    .to_string();
    let bin = fake.path().join("bin");
    fs::create_dir_all(&bin).expect("bin/");
    let script = bin.join("git");
    // Every call is the real git, except the blob read, which hangs: the staging directory
    // exists and the process is parked in the middle of reading a revision.
    fs::write(
        &script,
        format!(
            "#!/bin/sh\ncase \"$*\" in\n  *cat-file*) exec sleep 20 ;;\n  *) exec '{real_git}' \"$@\" ;;\nesac\n"
        ),
    )
    .expect("writes the fake git");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut child = Command::new(req_bin())
        .arg("--root")
        .arg(repo.root())
        .args(["diff", "--base", "base", "--head", "head"])
        .current_dir(repo.path())
        .env_remove("REQ_ROOT")
        .env("PATH", path)
        .env("TMPDIR", tmp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("starts req");
    let started = std::time::Instant::now();
    while fs::read_dir(tmp.path()).expect("lists").next().is_none() {
        assert!(
            started.elapsed().as_secs() < 15,
            "the staging directory never appeared in TMPDIR"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("sends the signal");
    assert!(killed.success());
    let status = child.wait().expect("req exits");
    assert!(!status.success(), "a signalled run is not a success");
    // The process had no chance to clean up after itself; the dead-man's switch it started does
    // (on the pipe closing), moments later.
    let started = std::time::Instant::now();
    loop {
        let left: Vec<_> = fs::read_dir(tmp.path()).expect("lists").collect();
        if left.is_empty() {
            break;
        }
        assert!(
            started.elapsed().as_secs() < 15,
            "a signalled run left {left:?} behind"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

// Help ------------------------------------------------------------------------------------------

#[test]
fn the_help_page_states_the_exit_contract() {
    let run = Command::new(req_bin())
        .args(["diff", "--help"])
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs req");
    let page = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(0));
    assert!(
        page.starts_with(
            "usage: req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]"
        ),
        "{page}"
    );
    let exit_at = page.find("exit:").expect("an exit: line");
    let exit = &page[exit_at..];
    for needle in ["0", "1", "3", "2", "--fail-on"] {
        assert!(
            exit.contains(needle),
            "the exit line states {needle}: {exit}"
        );
    }
}

// ADR 0044 (R-2) -------------------------------------------------------------------------------

/// Decision 3: SHOULD→SHALL strengthens while 10→100 ms loosens tenfold. Classed `tightened` it
/// slipped past `--fail-on weakened`; it is `reshaped`, which a repository can also fail on.
#[test]
fn a_compound_change_whose_axes_disagree_is_reshaped_and_not_missed_by_fail_on() {
    let before = "The system should respond within 10 ms.";
    let after = "The system shall respond within 100 ms.";
    let run = diff(
        &one_claim_change("compound-mixed", before, after),
        &["--base", "base", "--head", "head"],
    );
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "reshaped", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "scope_divergence"),
        "{}",
        run.stdout
    );
    assert!(has_word(&run, "REQ-0001", "also:"), "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "modality_strengthening,")
            && has_word(&run, "REQ-0001", "numeric_loosening"),
        "both axes are named: {}",
        run.stdout
    );

    let run = diff(
        &one_claim_change("compound-mixed-fail", before, after),
        &["--base", "base", "--head", "head", "--fail-on", "reshaped"],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
}

/// Agreeing axes keep their class and effect, and the second axis is named.
#[test]
fn a_compound_change_whose_axes_agree_is_weakened_and_names_both() {
    let run = diff(
        &one_claim_change(
            "compound-agree",
            "The system shall respond within 100 ms.",
            "The system should respond within 200 ms.",
        ),
        &["--base", "base", "--head", "head", "--fail-on", "weakened"],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "modality_weakening"),
        "{}",
        run.stdout
    );
    assert!(
        has_word(&run, "REQ-0001", "numeric_loosening"),
        "{}",
        run.stdout
    );
    assert!(has_word(&run, "REQ-0001", "also:"), "{}", run.stdout);
}

/// Decision 5 through `req diff`: raising the number a prohibition forbids is a weakening.
#[test]
fn a_bound_raised_under_a_prohibition_is_weakened() {
    let run = diff(
        &one_claim_change(
            "prohibition-bound",
            "The system shall not be more than 3 s late.",
            "The system shall not be more than 4 s late.",
        ),
        &["--base", "base", "--head", "head", "--fail-on", "weakened"],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert_eq!(class_of(&run, "REQ-0001"), "weakened", "{}", run.stdout);
    assert!(
        has_word(&run, "REQ-0001", "numeric_loosening"),
        "{}",
        run.stdout
    );
}

/// Decision 6: `req.diff/1` gains `axes`, additively — the single-axis claim lists one class, a
/// compound lists every moved axis in order, and an entry with no changed meaning lists none.
#[test]
fn the_json_report_lists_every_moved_axis() {
    let repo = Repo::new("axes-json");
    repo.claim("REQ-0001", "The system should respond within 10 ms.");
    repo.claim("REQ-0002", "The system shall respond within 200 ms.");
    repo.claim("REQ-0003", "The system shall log each command.");
    repo.commit("base");
    repo.claim("REQ-0001", "The system shall respond within 100 ms.");
    repo.claim("REQ-0002", "The system shall respond within 300 ms.");
    repo.claim("REQ-0004", "The system shall archive each report.");
    repo.commit("head");
    let run = diff(
        &repo,
        &["--base", "base", "--head", "head", "--format", "json"],
    );
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    let doc: serde_json::Value = serde_json::from_str(&run.stdout).expect("one JSON document");
    assert_eq!(doc["schema"], "req.diff/1", "additive: the schema stays");
    let claims = doc["claims"].as_array().expect("claims[]");
    let axes_of = |id: &str| {
        claims
            .iter()
            .find(|c| c["claim_id"] == id)
            .unwrap_or_else(|| panic!("{id} is listed"))["axes"]
            .clone()
    };
    assert_eq!(
        axes_of("REQ-0001"),
        serde_json::json!(["modality_strengthening", "numeric_loosening"])
    );
    assert_eq!(
        axes_of("REQ-0002"),
        serde_json::json!(["numeric_loosening"])
    );
    assert_eq!(
        axes_of("REQ-0004"),
        serde_json::json!([]),
        "an added claim moved no axis"
    );
}
