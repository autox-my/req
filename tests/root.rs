//! The corpus-root seam (ADR 0012): the root is resolved at runtime, and the resolver is
//! **pure** — the working directory, the `--root` value and `REQ_ROOT` are parameters, not
//! ambient state — so precedence is testable without `set_current_dir` or env mutation.
//! Both are process-global, and either would make this file race every other test in a run.
//! The working directory is a *thunk*, so a test can prove it is never read when a root was
//! named, and so an unreadable one can be simulated without touching the process.

use req::root::{is_corpus_root, resolve_corpus_root, take_root_arg};
use std::path::{Path, PathBuf};

/// A throwaway tree. `tests/loader.rs` is the fixture pattern; a `Tree` under
/// `env!("CARGO_TARGET_TMPDIR")` is used where the walk-up must find the fixture (the
/// repository itself has `claims/`, and it is an ancestor of `target/`), and a system temp
/// dir where the walk-up must fail — there, no ancestor may hold a `claims/` directory, and
/// that is *asserted* at each such fixture rather than assumed (see
/// `assert_no_ancestor_is_a_corpus_root`).
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn under(base: &Path, name: &str) -> Self {
        let root = base.join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("tree root");
        Tree { root }
    }

    fn new(name: &str) -> Self {
        Tree::under(&PathBuf::from(env!("CARGO_TARGET_TMPDIR")), name)
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let path = self.root.join(rel);
        std::fs::create_dir_all(&path).expect("fixture dir");
        path
    }

    /// A directory that *is* a corpus root: it contains a `claims/` **directory**.
    fn corpus(&self, rel: &str) -> PathBuf {
        let path = self.dir(rel);
        std::fs::create_dir_all(path.join("claims")).expect("claims dir");
        path
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The nearest ancestor of `start` that is a corpus root — the production walk-up, mirrored
/// so a fixture can ask the resolver's own question rather than a restatement of it.
fn ancestor_corpus_root(start: &Path) -> Option<&Path> {
    start.ancestors().find(|dir| is_corpus_root(dir))
}

/// Negative fixtures walk up from a system temp dir and must find **nothing**. That is an
/// assumption about the machine, so it is asserted where the fixture is built: if some
/// ancestor ever holds a `claims/` — a `TMPDIR` inside a checkout, a temp dir that grew one —
/// the walk-up resolves by accident and the test passes while checking nothing.
fn assert_no_ancestor_is_a_corpus_root(start: &Path) {
    if let Some(found) = ancestor_corpus_root(start) {
        panic!(
            "the fixture assumes no ancestor holds claims/, but {} does",
            found.display()
        );
    }
}

/// The working directory a caller passes when it must not be read at all: using it is a bug,
/// so it panics rather than returning a path.
fn unreadable_cwd() -> std::io::Result<PathBuf> {
    panic!("the working directory must not be read when a root was named")
}

fn resolve(start: &Path, cli: Option<&Path>, env: Option<&str>) -> Result<PathBuf, String> {
    resolve_corpus_root(|| Ok(start.to_path_buf()), cli, env)
}

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------------------
// (a) precedence: cli beats env beats walk-up
// ---------------------------------------------------------------------------

#[test]
fn cli_beats_env_beats_walk_up() {
    let tree = Tree::new("root-precedence");
    let cli = tree.corpus("cli");
    let env = tree.corpus("env");
    let walk = tree.corpus("walk");
    let start = tree.dir("walk/deep/start");

    assert_eq!(
        resolve(&start, Some(&cli), Some(env.to_str().unwrap())).unwrap(),
        cli,
        "an explicit --root wins over REQ_ROOT and the walk-up"
    );
    assert_eq!(
        resolve(&start, None, Some(env.to_str().unwrap())).unwrap(),
        env,
        "REQ_ROOT wins over the walk-up"
    );
    assert_eq!(
        resolve(&start, None, None).unwrap(),
        walk,
        "with neither given, the nearest ancestor holding claims/ wins"
    );
}

// ---------------------------------------------------------------------------
// (b) walk-up: nearest ancestor with claims/ — a directory, not a file
// ---------------------------------------------------------------------------

#[test]
fn walk_up_finds_the_nearest_ancestor_with_a_claims_dir() {
    let tree = Tree::new("root-walk-up");
    let outer = tree.corpus("outer");
    let start = tree.dir("outer/a/b");
    assert_eq!(
        resolve(&start, None, None).unwrap(),
        outer,
        "claims/ two levels up from start"
    );

    let inner = tree.corpus("outer/a/inner");
    let deep = tree.dir("outer/a/inner/c");
    assert_eq!(
        resolve(&deep, None, None).unwrap(),
        inner,
        "the nearest ancestor wins, not the farthest"
    );

    // A *file* named claims is not a corpus root: the ADR says a `claims/` directory.
    // (Outside the repository, so the walk-up cannot reach this checkout's own corpus.)
    let file_tree = Tree::under(&std::env::temp_dir(), "req-corpus-root-claims-file");
    std::fs::write(file_tree.root.join("claims"), "not a directory").expect("claims file");
    let under = file_tree.dir("sub");
    assert_no_ancestor_is_a_corpus_root(&under);
    let err = resolve(&under, None, None).expect_err("a claims file is not a claims dir");
    assert!(err.contains(&under.display().to_string()), "{err}");
}

// ---------------------------------------------------------------------------
// (c) nothing found: the error names the walked path and the remediation
// ---------------------------------------------------------------------------

#[test]
fn nothing_found_names_the_walked_path_and_the_remediation() {
    let tree = Tree::under(&std::env::temp_dir(), "req-corpus-root-not-found");
    let start = tree.dir("a/b");
    assert_no_ancestor_is_a_corpus_root(&start);

    let err = resolve(&start, None, None).expect_err("no claims/ at or above the temp tree");
    assert!(
        err.contains(&start.display().to_string()),
        "the error must name the path walked: {err}"
    );
    assert!(
        err.contains("pass --root"),
        "the error must name the remedy: {err}"
    );
    assert!(
        err.contains("REQ_ROOT"),
        "the error must name the env remedy: {err}"
    );
}

// ---------------------------------------------------------------------------
// (d) validation: a named root that holds no claims/ is an error, never a fallback
// ---------------------------------------------------------------------------

#[test]
fn a_named_root_without_a_claims_dir_is_an_error() {
    let tree = Tree::new("root-validation");
    let bare = tree.dir("bare");
    let start = tree.corpus("start");

    let err = resolve(&start, Some(&bare), None).expect_err("--root must hold claims/");
    assert!(
        err.contains(&bare.display().to_string()),
        "names the given root: {err}"
    );
    assert!(err.contains("pass --root"), "names the remedy: {err}");

    let err = resolve(&start, None, Some(bare.to_str().unwrap()))
        .expect_err("REQ_ROOT must hold claims/");
    assert!(
        err.contains(&bare.display().to_string()),
        "names the given root: {err}"
    );
    assert!(err.contains("REQ_ROOT"), "names the remedy: {err}");
}

// ---------------------------------------------------------------------------
// (e) REQ_ROOT set to nothing is *unset*, not a path that collapses
// ---------------------------------------------------------------------------

#[test]
fn an_empty_req_root_is_unset_not_a_path_that_collapses() {
    // `Path::new("").join("claims")` is a bare `claims/`, resolved against whatever the
    // working directory happens to be — which for this test run is the crate root, and that
    // one holds a corpus. So an empty value would resolve to "" and print an empty
    // `corpus root:` line. Unset means the walk-up decides.
    let tree = Tree::new("root-empty-env");
    let walk = tree.corpus("walk");
    let start = tree.dir("walk/deep/start");

    assert_eq!(
        resolve(&start, None, Some("")).unwrap(),
        walk,
        "REQ_ROOT= is unset, so the nearest ancestor holding claims/ wins"
    );
}

// ---------------------------------------------------------------------------
// --root is stripped from argv in both forms, before any subcommand parses
// ---------------------------------------------------------------------------

#[test]
fn root_is_stripped_from_argv_in_both_forms() {
    let (root, rest) = take_root_arg(&argv(&["--root", "/x", "canon"])).expect("both forms parse");
    assert_eq!(root, Some(PathBuf::from("/x")));
    assert_eq!(rest, argv(&["canon"]));

    let (root, rest) =
        take_root_arg(&argv(&["--root=/y", "select", "a=b"])).expect("both forms parse");
    assert_eq!(root, Some(PathBuf::from("/y")));
    assert_eq!(rest, argv(&["select", "a=b"]));

    assert!(
        take_root_arg(&argv(&["canon"])).unwrap().0.is_none(),
        "absent is not an error"
    );
    assert!(
        take_root_arg(&argv(&["--root"])).is_err(),
        "--root needs a directory"
    );
    assert!(
        take_root_arg(&argv(&["--root="])).is_err(),
        "--root= needs a directory"
    );
}

// ---------------------------------------------------------------------------
// (f) a named root never reads the working directory; the negative fixtures assert
//     the environment they depend on
// ---------------------------------------------------------------------------

#[test]
fn a_named_root_never_reads_the_working_directory() {
    let tree = Tree::new("root-lazy-cwd");
    let cli = tree.corpus("cli");
    let env = tree.corpus("env");

    // `unreadable_cwd` panics, so reaching it fails the test: a root named either way must be
    // answered without the working directory at all.
    assert_eq!(
        resolve_corpus_root(unreadable_cwd, Some(&cli), None).unwrap(),
        cli,
        "--root answers without the working directory"
    );
    assert_eq!(
        resolve_corpus_root(unreadable_cwd, None, Some(env.to_str().unwrap())).unwrap(),
        env,
        "REQ_ROOT answers without the working directory"
    );

    // An *empty* REQ_ROOT names nothing, so the walk-up runs and does need one: with an
    // unreadable CWD the failure must say so, rather than resolving "" by accident.
    let err = resolve_corpus_root(
        || Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
        None,
        Some(""),
    )
    .expect_err("nothing named a root and there is no working directory");
    assert!(
        err.contains("working directory") && err.contains("pass --root"),
        "names what failed and the remedy: {err}"
    );
}

#[test]
#[should_panic(expected = "the fixture assumes no ancestor holds claims/")]
fn the_negative_fixture_guard_fires_when_an_ancestor_is_a_corpus_root() {
    // The guard must not be vacuous. Self-contained: `corpus/` holds a `claims/` directory,
    // so the walk-up from `corpus/deep` finds one and the assertion has to fail loudly.
    let tree = Tree::new("root-guard-fires");
    let start = tree.corpus("corpus/deep");
    assert_no_ancestor_is_a_corpus_root(&start);
}
