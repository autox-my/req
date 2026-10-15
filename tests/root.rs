//! The corpus-root seam (ADR 0012): the root is resolved at runtime, and the resolver is
//! **pure** — the working directory, the `--root` value and `REQ_ROOT` are parameters, not
//! ambient state — so precedence is testable without `set_current_dir` or env mutation.
//! Both are process-global, and either would make this file race every other test in a run.

use req::root::{resolve_corpus_root, take_root_arg};
use std::path::{Path, PathBuf};

/// A throwaway tree. `tests/loader.rs` is the fixture pattern; a `Tree` under
/// `env!("CARGO_TARGET_TMPDIR")` is used where the walk-up must find the fixture (the
/// repository itself has `claims/`, and it is an ancestor of `target/`), and a system temp
/// dir where the walk-up must fail — there, no ancestor may hold a `claims/` directory.
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

fn resolve(start: &Path, cli: Option<&Path>, env: Option<&str>) -> Result<PathBuf, String> {
    resolve_corpus_root(start, cli, env)
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
