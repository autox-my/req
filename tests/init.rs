//! `req init` — the corpus it writes, and the guarantees that make running it safe.
//!
//! The seam is the **CLI**. Every check drives the compiled binary against a fresh
//! directory under the system temp dir (the pattern `tests/cli_version.rs` established),
//! because what `init` promises is a *file set a reader will use*: `req canon --verify`,
//! `req gate` and the corpus loader all read it back. A private writer unit-tested in
//! isolation would prove nothing about that.
//!
//! The address is checked **twice, because the two checks fail differently**. A *golden*
//! derived outside this crate — the address the pack's prototype corpus produced before
//! `init` existed — is the independent oracle: recomputing the value with the same functions
//! `build` uses (read the claim, parse it, re-derive) asserts only that the two agree, which
//! an implementation that never derived a correct address would also satisfy. Keeping the
//! fresh derivation *beside* it catches the other failure: a file whose stored form no longer
//! matches its own parse. The golden is not "the implementation's output written down" — when
//! `RULES_V` moves it is regenerated deliberately, in that commit, as every other golden is.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// The absolute path of the compiled binary, so the test does not depend on `$PATH`.
fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_at(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        // `init` creates a root instead of reading one, but the commands driven *after* it
        // in these fixtures (`canon --verify`, `gate`) resolve one, and an inherited
        // `REQ_ROOT` would point them at another tree and make the check meaningless.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Distinguishes fixtures within one process; the process id extends that across processes
/// (two checkouts sharing a `TMPDIR` must not collide on a directory one is about to delete).
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A directory with **no `claims/` ancestor**, so a fixture that starts empty really is a
/// directory with no corpus: the resolver's walk-up would otherwise find one by accident and
/// the test would pass while checking nothing.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("{name}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the fixture directory");
        if let Some(found) = dir.ancestors().find(|d| req::root::is_corpus_root(d)) {
            panic!(
                "the fixture assumes no ancestor holds claims/, but {} does",
                found.display()
            );
        }
        Fixture(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn join(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.join(rel)).unwrap_or_else(|e| panic!("reads {rel}: {e}"))
    }

    fn bytes(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.join(rel)).unwrap_or_else(|e| panic!("reads {rel}: {e}"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The three files `init` promises, in the order the pack lists them.
const WRITTEN: [&str; 3] = [
    "claims/REQ-0001.yml",
    "claims/README.md",
    "aliases/subjects.yml",
];

/// The starter claim's address, **derived outside this crate**: the pack's prototype corpus
/// (`/tmp/newapp`, fact 5) produced exactly this value at `2afbd68`, before `init` existed,
/// and `init`'s first run reproduced it byte for byte. It is an oracle, not a recording of the
/// implementation — which is why the test asserts it *and* a fresh derivation, and why a
/// `RULES_V` bump regenerates it deliberately in that commit.
const STARTER_ADDRESS: &str = "cbece4455e74d01cac9a6b8be0a551382e1800dfca44b0921a210040f441a7e8";

fn assert_written(dir: &Path) {
    for rel in WRITTEN {
        assert!(
            dir.join(rel).is_file(),
            "`req init` must write {rel}; the directory holds {:?}",
            list_dir(dir)
        );
    }
}

fn list_dir(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// `YYYY-MM-DD`, asserted by shape rather than by the value this machine's clock happens to
/// hold: the conversion's *correctness* is pinned in `src/init.rs` against known epoch dates,
/// where the oracle can be a date and not another copy of the arithmetic.
fn assert_iso_date(value: &str) {
    let bytes = value.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    assert!(
        shaped,
        "the rationale date must be YYYY-MM-DD, not {value:?}"
    );
}

// ---------------------------------------------------------------------------
// Acceptance 1-3: a fresh directory, a filled corpus, a green gate
// ---------------------------------------------------------------------------

#[test]
fn init_creates_a_filled_corpus_in_the_current_directory() {
    let fx = Fixture::new("req-init-fresh");
    let out = run_at(fx.path(), &["init"]);

    assert_eq!(
        out.code, 0,
        "`req init` in an empty directory must create a corpus:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        !out.stderr.contains("corpus root:"),
        "`init` creates the root instead of resolving one, so it must never name one:\n{}",
        out.stderr
    );
    assert_written(fx.path());

    // The next steps are printed, not written: a tool must not guess a forge. Every
    // item the pack requires is named — where to report a bug, and the per-variant rule.
    for required in [
        "https://git.xantu-barb.ts.net/fdx/req/issues",
        ".req/variants",
        "req canon --verify",
        "req gate",
    ] {
        assert!(
            out.stdout.contains(required),
            "`init` must print its next steps, including {required:?}:\n{}",
            out.stdout
        );
    }

    // The claim that landed is a real parse of the authored statement, filled by the same
    // path `canon --fill` uses — checked by re-deriving the address from what is on disk.
    let registry = req::canon::load_registry(fx.path()).expect("the registry `init` wrote loads");
    let claims = req::canon::load_claims(fx.path()).expect("the corpus `init` wrote loads");
    assert_eq!(claims.len(), 1, "one starter claim, not a seed set");
    let claim = &claims[0].claim;
    assert_eq!(claim.claim_id, "REQ-0001");
    assert_eq!(
        claim.statement,
        "The API shall respond to a health check within 200 ms."
    );
    assert_eq!(claim.modality, "SHALL");
    assert_eq!(claim.subject, "API");
    assert_eq!(claim.operator.as_deref(), Some("LE"));
    assert_eq!(
        claim.value.as_deref(),
        Some("0.2"),
        "canonical SI, not 200 ms"
    );
    assert_eq!(claim.unit.as_deref(), Some("s"));
    assert!(
        claim.applicability.is_empty(),
        "the starter claim is unconditional"
    );
    assert!(claim.relations.is_empty(), "no relation to invent");
    assert_iso_date(&claim.rationale.date);

    let derived = req::canon::address_hex(&req::canon::canonical_json(
        &req::canon::evaluate_claim(claim, &registry).form,
    ));
    assert_eq!(
        claim.address.as_deref(),
        Some(STARTER_ADDRESS),
        "the starter claim's address is a **golden derived outside this crate** — the prototype \
         corpus filled at the commit before `init` existed. A mismatch is either a derivation \
         that moved without a rules bump (a defect) or a deliberate `RULES_V` bump, which \
         regenerates this constant in the same commit"
    );
    assert_eq!(
        claim.address.as_deref(),
        Some(derived.as_str()),
        "and the stored address must be the one its own stored parse derives, so a file whose \
         form and address disagree cannot pass on the golden alone"
    );
    assert_eq!(
        claim.canonical.rules_v,
        Some(req::canon::RULES_V),
        "the starter claim carries this build's rules version"
    );
    assert!(
        claim
            .canonical
            .form
            .as_deref()
            .is_some_and(|form| !form.is_empty()),
        "the form is filled, not an anchor left for a human"
    );

    // The registry declares the surface the statement uses, or the parse would flag the
    // subject as literal and the claim would not be green.
    let registry_text = fx.read("aliases/subjects.yml");
    for required in ["schema: req.subject_registry/0", "version: 1", "the api"] {
        assert!(
            registry_text.contains(required),
            "the registry must contain {required:?}:\n{registry_text}"
        );
    }

    // The README carries the authoring loop and its traps, in the adopter's words — and points
    // an adopter with a backlog at the extractor, which is the one path into a corpus that does
    // not start by transcribing.
    let readme = fx.read("claims/README.md");
    for required in [
        "req canon --fill",
        "req canon --verify",
        "req canon", // reports and always exits 0
        "SI",
        "refused",
        "address",
        "tools/adopt.py",
        // The runnable form: required flags, and --write because the default is a dry run.
        "--from",
        "--subject",
        "--write",
    ] {
        assert!(
            readme.contains(required),
            "claims/README.md must state the authoring loop ({required:?} missing):\n{readme}"
        );
    }
}

#[test]
fn the_fresh_corpus_is_green() {
    let fx = Fixture::new("req-init-green");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "{}\n{}", init.stdout, init.stderr);

    let verify = run_at(fx.path(), &["canon", "--verify"]);
    assert_eq!(
        verify.code, 0,
        "the claim `init` ships must verify in the directory it created:\n{}\n{}",
        verify.stdout, verify.stderr
    );
    assert!(
        verify.stdout.contains("verified 1"),
        "the verify line must report the claim it checked:\n{}",
        verify.stdout
    );

    let gate = run_at(fx.path(), &["gate"]);
    assert_eq!(
        gate.code, 0,
        "the adopter's first commit must have a working gate, not a red one:\n{}\n{}",
        gate.stdout, gate.stderr
    );
    assert!(
        gate.stdout.contains("verdict: PASS"),
        "the gate's verdict line:\n{}",
        gate.stdout
    );

    // Diagnostics settle too: a claim that ships with flags would redden the adopter's
    // `req canon` on day one even though it does not gate.
    let canon = run_at(fx.path(), &["canon"]);
    assert_eq!(canon.code, 0, "{}\n{}", canon.stdout, canon.stderr);
    assert!(
        canon.stdout.contains("discrepancy flags: 0"),
        "the starter claim must be a clean parse:\n{}",
        canon.stdout
    );
}

// ---------------------------------------------------------------------------
// Acceptance 4: a corpus is never overwritten
// ---------------------------------------------------------------------------

#[test]
fn a_second_init_refuses_and_leaves_every_byte_alone() {
    let fx = Fixture::new("req-init-refuse");
    let first = run_at(fx.path(), &["init"]);
    assert_eq!(first.code, 0, "{}\n{}", first.stdout, first.stderr);
    let before: Vec<Vec<u8>> = WRITTEN.iter().map(|rel| fx.bytes(rel)).collect();

    let second = run_at(fx.path(), &["init"]);
    assert_eq!(
        second.code, 1,
        "a second `init` is a refusal (exit 1), never a rewrite:\n{}\n{}",
        second.stdout, second.stderr
    );
    assert!(
        second.stderr.contains("refus"),
        "the refusal must say what it refused:\n{}",
        second.stderr
    );
    for (rel, was) in WRITTEN.iter().zip(before) {
        assert_eq!(
            fx.bytes(rel),
            was,
            "{rel} must be byte-identical after a refused `init`"
        );
    }
}

#[test]
fn a_partial_corpus_is_refused_rather_than_completed() {
    // Each of these is "a corpus is already here" in the pack's sense: the two paths that
    // decide whether `init` may run. Creating the rest around them would overwrite a human's
    // file — `never overwrite` is the whole rule.
    let cases: [(&str, &str); 3] = [
        ("empty-claims-dir", "claims"),
        ("registry-only", "aliases/subjects.yml"),
        ("claims-is-a-file", "claims"),
    ];
    for (name, marker) in cases {
        let fx = Fixture::new(&format!("req-init-partial-{name}"));
        let path = fx.join(marker);
        if name == "empty-claims-dir" {
            std::fs::create_dir_all(&path).expect("creates the claims directory");
        } else {
            std::fs::create_dir_all(path.parent().unwrap()).expect("creates the parent");
            std::fs::write(&path, "mine\n").expect("plants the marker");
        }

        let out = run_at(fx.path(), &["init"]);
        assert_eq!(
            out.code, 1,
            "`{marker}` already exists, so `init` must refuse:\n{}\n{}",
            out.stdout, out.stderr
        );
        assert_eq!(
            std::fs::read(&path)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned()),
            if name == "empty-claims-dir" {
                None
            } else {
                Some("mine\n".to_string())
            },
            "a refused `init` must not touch the file it found"
        );
        // Nothing was completed around the obstruction.
        for rel in WRITTEN {
            if rel != marker {
                assert!(
                    !fx.join(rel).exists(),
                    "a refused `init` wrote {rel} anyway"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptance: the UNVERIFIABLE outcome, and the cleanup it must leave behind
// ---------------------------------------------------------------------------

/// A mode the fixture held before a case made it read-only, restored on drop so the
/// fixture can remove its own tree.
#[cfg(unix)]
struct RestoreMode(PathBuf);

#[cfg(unix)]
impl Drop for RestoreMode {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        }
    }
}

#[test]
fn an_obstructed_write_is_unverifiable_and_writes_nothing() {
    // `aliases` as a *regular file*: `Created::dir` skips it (`exists()` is true) and the
    // registry write then fails — ENOTDIR, on any uid, which is why this case does not
    // depend on permission bits (root ignores those).
    let fx = Fixture::new("req-init-unverifiable-obstruction");
    std::fs::write(fx.join("aliases"), "not a directory\n").expect("plants the obstruction");

    let out = run_at(fx.path(), &["init"]);
    assert_eq!(
        out.code, 3,
        "a write that cannot happen is UNVERIFIABLE, never a creation and never a refusal:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("[UNVERIFIABLE] init:"),
        "the outcome must be named as UNVERIFIABLE:\n{}",
        out.stderr
    );
    for rel in ["claims", "claims/REQ-0001.yml", "aliases/subjects.yml"] {
        assert!(!fx.join(rel).exists(), "a failed `init` wrote {rel} anyway");
    }
}

#[cfg(unix)]
#[test]
fn a_failure_after_a_write_removes_what_it_wrote() {
    // The case `Created::abort` exists for: a *partial* corpus. The fixture directory is made
    // read-only, but `aliases/` is planted first and stays writable, so the registry write
    // **succeeds** and the `claims/` directory creation that follows fails. `abort` must then
    // remove the registry it wrote — a half-corpus would be red and would make the next
    // `init` refuse with exit 1.
    //
    // A uid that ignores directory permission bits (root) cannot produce this state at all:
    // `claims/` would be created and `init` would exit 0. So the case probes for that and
    // says it was not exercised, rather than passing without having run anything.
    let fx = Fixture::new("req-init-unverifiable-readonly");
    std::fs::create_dir_all(fx.join("aliases")).expect("pre-creates the writable aliases/");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(fx.path(), std::fs::Permissions::from_mode(0o500))
            .expect("makes the fixture directory read-only");
    }
    let _restore = RestoreMode(fx.path().to_path_buf());

    if std::fs::File::create(fx.join("probe")).is_ok() {
        let _ = std::fs::remove_file(fx.join("probe"));
        eprintln!(
            "NOT EXERCISED: this uid ignores directory permission bits (running as root?), so a \
             failure after the registry write cannot be induced — the abort cleanup is \
             unverified here"
        );
        return;
    }

    let out = run_at(fx.path(), &["init"]);
    assert_eq!(
        out.code, 3,
        "an un-creatable `claims/` is UNVERIFIABLE:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("[UNVERIFIABLE] init:"),
        "the outcome must be named as UNVERIFIABLE:\n{}",
        out.stderr
    );
    assert!(
        !fx.join("aliases/subjects.yml").exists(),
        "abort must remove the registry this run wrote — otherwise the next `init` refuses"
    );
    assert!(!fx.join("claims").exists(), "no claims directory was left");
}

#[cfg(unix)]
#[test]
fn a_broken_symlink_at_a_refusal_path_is_something_that_is_there() {
    // The refusal check uses `symlink_metadata`, not `exists()`, precisely because `exists()`
    // follows the link and answers "nothing here" for a dangling one — which would let
    // `init` write through it. Every other case plants a real directory or a regular file,
    // for which the two answers agree, so swapping them leaves them all green; this is the
    // case that does not.
    for name in ["claims", "aliases/subjects.yml"] {
        let fx = Fixture::new(&format!("req-init-dangling-{}", name.replace('/', "-")));
        let link = fx.join(name);
        std::fs::create_dir_all(link.parent().expect("the path has a parent"))
            .expect("creates the parent directory");
        std::os::unix::fs::symlink("nowhere-at-all", &link).expect("plants a dangling symlink");
        assert!(
            !link.exists(),
            "a dangling symlink answers `exists() == false`"
        );

        let out = run_at(fx.path(), &["init"]);
        assert_eq!(
            out.code, 1,
            "a dangling symlink at {name} is a thing that is there, so `init` must refuse:\n{}\n{}",
            out.stdout, out.stderr
        );
        // The refusal must explain *this* case for every path it covers, not only for the one
        // that got its own check: a dangling link at `claims` does not resolve, so "already
        // exists — edit it in place" names a path that is not there and offers the remedy for
        // a corpus that does not exist.
        assert!(
            out.stderr.contains("is a symlink"),
            "a dangling link at {name} must be refused as a link, not as something that exists:\n{}",
            out.stderr
        );
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "a refused `init` must leave the link it found alone"
        );
        for rel in WRITTEN {
            assert!(
                !fx.join(rel).exists() || rel == name,
                "a refused `init` wrote {rel} anyway"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_aliases_directory_is_refused_rather_than_written_through() {
    // The refusal check above reads `claims` and `aliases/subjects.yml`, but the registry
    // write goes *through* the `aliases` directory component — and `Created::dir` answers
    // `exists()`, which follows a link and reports "nothing to create". Pointed at another
    // tree, `init` therefore writes a corpus's registry into a directory it was never asked
    // to touch, and still exits 0. The dangling form is the worse one: the refusal is
    // skipped, `create_dir_all` then fails on the link, and the outcome is UNVERIFIABLE (3)
    // instead of the refusal (1) whose message says what to do about it.
    for dangling in [false, true] {
        let fx = Fixture::new(if dangling {
            "req-init-aliases-dangling"
        } else {
            "req-init-aliases-elsewhere"
        });
        // The tree the link points at, when it points at one. The dangling case has none, so
        // the assertion below is written against the *link target*: an assertion about a path
        // this run never touches is true whatever `init` does, which is how an unused fixture
        // made the first version of this test vacuous in exactly the iteration it called the
        // worse one.
        let target = (!dangling).then(|| Fixture::new("req-init-aliases-target"));
        let link_target = match &target {
            Some(tree) => tree.path().to_path_buf(),
            // Deliberately nothing: the link resolves to nowhere.
            None => fx.join("nowhere-at-all"),
        };
        std::os::unix::fs::symlink(&link_target, fx.join("aliases"))
            .expect("plants the link at the directory component");

        let out = run_at(fx.path(), &["init"]);
        assert_eq!(
            out.code, 1,
            "a link at `aliases` (dangling: {dangling}) is a thing that is there and the \
             corpus is not where it points, so `init` must refuse:\n{}\n{}",
            out.stdout, out.stderr
        );
        assert!(
            !link_target.join("subjects.yml").exists(),
            "the registry must not be written through the link, whatever it points at"
        );
        assert!(
            !fx.join("claims").exists(),
            "a refused `init` writes nothing"
        );
        // The refusal must explain *this* case. A link is refused because writing through it
        // would put the corpus where this run was not asked to create one — not because
        // something is there to overwrite, which is the reason for the case above and says
        // nothing about a link that may not even resolve.
        assert!(
            out.stderr.contains("is a symlink"),
            "the refusal must name the link:\n{}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("already exists"),
            "`aliases` need not exist at all — a dangling link is refused too, so the \
             existing-corpus explanation is wrong here:\n{}",
            out.stderr
        );
        assert!(
            std::fs::symlink_metadata(fx.join("aliases")).is_ok_and(|m| m.file_type().is_symlink()),
            "a refused `init` must leave the link it found alone"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_hiding_a_registry_is_refused_as_a_link_not_as_an_existing_corpus() {
    // The refusal loop reads `aliases/subjects.yml`, and `symlink_metadata` follows symlinks in
    // *intermediate* components — so when `aliases` is a link whose target already holds a
    // registry, the loop resolves through the link, finds that file and answers first, telling
    // the operator to "edit it in place": a path in a tree this run never named. The link is the
    // thing to report, and it is the reason the check belongs **before** the loop.
    let fx = Fixture::new("req-init-aliases-shadowed");
    let target = Fixture::new("req-init-aliases-shadow-target");
    std::fs::write(
        target.path().join("subjects.yml"),
        "schema: req.subject_registry/0\n",
    )
    .expect("plants a registry in the other tree");
    std::os::unix::fs::symlink(target.path(), fx.join("aliases")).expect("plants the link");

    let out = run_at(fx.path(), &["init"]);
    assert_eq!(
        out.code, 1,
        "the link must be refused:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("is a symlink"),
        "the link is what makes the path mean somewhere else, so it must be the reason given:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("already exists"),
        "the registry it found is in another tree; `aliases/subjects.yml` is not this run's to \
         edit:\n{}",
        out.stderr
    );
    assert_eq!(
        std::fs::read_to_string(target.path().join("subjects.yml")).unwrap(),
        "schema: req.subject_registry/0\n",
        "the other tree must be untouched"
    );
}

#[test]
fn a_real_aliases_directory_is_used_rather_than_refused() {
    // The check above is about *links*. A real `aliases/` directory is under the root the
    // caller named, so `init` writes its registry into it and leaves everything else there
    // alone. Refusing any pre-existing `aliases/` would be the easy generalisation and would
    // block a project that happens to keep one for something unrelated — this is the case
    // that keeps the check narrow.
    let fx = Fixture::new("req-init-aliases-existing");
    std::fs::create_dir_all(fx.join("aliases")).expect("pre-creates a real aliases/");
    std::fs::write(fx.join("aliases/notes.txt"), "not ours\n")
        .expect("puts an unrelated file in it");

    let out = run_at(fx.path(), &["init"]);
    assert_eq!(
        out.code, 0,
        "a real `aliases/` directory is where the caller asked for the corpus:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert_written(fx.path());
    assert_eq!(
        fx.read("aliases/notes.txt"),
        "not ours\n",
        "the adopter's own file must be left exactly as it was"
    );
}

// ---------------------------------------------------------------------------
// Acceptance: the argument contract, and the root `init` is answered before
// ---------------------------------------------------------------------------

#[test]
fn init_takes_no_arguments_and_no_root() {
    let fx = Fixture::new("req-init-usage");
    let elsewhere = fx.join("elsewhere");

    for argv in [
        &["init", "extra"][..],
        &["init", "--fill"][..],
        &["init", "--root", elsewhere.to_str().unwrap()][..],
    ] {
        let out = run_at(fx.path(), argv);
        assert_eq!(
            out.code, 2,
            "{argv:?} is a usage error: `init` writes the corpus here and takes no further \
             input, and a silently ignored `--root` would create a corpus somewhere the \
             caller did not ask for:\n{}\n{}",
            out.stdout, out.stderr
        );
    }
    assert!(
        !elsewhere.exists(),
        "a usage error must write nothing, including a --root target"
    );
    for rel in WRITTEN {
        assert!(!fx.join(rel).exists(), "a usage error wrote {rel}");
    }
}

#[test]
fn init_creates_its_own_root_and_never_walks_up_to_an_ancestor() {
    // A corpus at the parent: `init` in the child must create the child's own root rather
    // than refuse because of the ancestor, and must never write into the ancestor.
    let parent = Fixture::new("req-init-ancestor");
    let parent_init = run_at(parent.path(), &["init"]);
    assert_eq!(
        parent_init.code, 0,
        "{}\n{}",
        parent_init.stdout, parent_init.stderr
    );
    let parent_before: Vec<Vec<u8>> = WRITTEN.iter().map(|rel| parent.bytes(rel)).collect();

    let child = parent.join("nested/deeper");
    std::fs::create_dir_all(&child).expect("creates the child directory");
    let out = run_at(&child, &["init"]);
    assert_eq!(
        out.code, 0,
        "the ancestor's corpus is not this directory's, and refusing on it would make \
         `init` unable to start a project inside another one:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert_written(&child);

    // The child is now the resolved root, and the ancestor is untouched.
    let verify = run_at(&child, &["canon", "--verify"]);
    assert_eq!(verify.code, 0, "{}\n{}", verify.stdout, verify.stderr);
    assert!(
        verify
            .stderr
            .contains(&child.to_string_lossy().into_owned()),
        "in the child, the child is the corpus root:\n{}",
        verify.stderr
    );
    for (rel, was) in WRITTEN.iter().zip(parent_before) {
        assert_eq!(
            parent.bytes(rel),
            was,
            "the ancestor's {rel} must be untouched"
        );
    }
}

#[test]
fn help_lists_init_and_version_still_answers_with_no_corpus() {
    let fx = Fixture::new("req-init-help");

    let help = run_at(fx.path(), &["--help"]);
    assert_eq!(help.code, 0, "{}\n{}", help.stdout, help.stderr);
    assert!(
        help.stdout.contains("init"),
        "USAGE must list a subcommand that exists:\n{}",
        help.stdout
    );
    assert!(
        !help.stderr.contains("corpus root:"),
        "--help resolves no root:\n{}",
        help.stderr
    );

    // `init` was added to the pre-root dispatch beside these two; their contract is the
    // thing that must not have moved with it.
    let version = run_at(fx.path(), &["--version"]);
    assert_eq!(version.code, 0, "{}\n{}", version.stdout, version.stderr);
    assert!(
        version
            .stdout
            .lines()
            .next()
            .is_some_and(|l| l.starts_with("req ")),
        "the first line stays the greppable version:\n{}",
        version.stdout
    );
    assert!(
        !version.stderr.contains("corpus root:"),
        "--version resolves no root:\n{}",
        version.stderr
    );
}

// ---------------------------------------------------------------------------
// Acceptance 5: the adoption doc cannot drift to the form that fails
// ---------------------------------------------------------------------------

#[test]
fn the_adoption_doc_carries_the_verified_install_form_and_the_ci_gate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/adopting.md");
    let doc =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reads {}: {e}", path.display()));

    // The install form that was measured to work — the scp-style URL is rejected
    // (`relative URL without a base`) and libgit2 ignores `GIT_SSH_COMMAND`.
    for required in [
        "ssh://git@git.xantu-barb.ts.net/fdx/req.git",
        "CARGO_NET_GIT_FETCH_WITH_CLI=true",
        "--rev <PIN>",
        "--locked",
        "cargo install --path .",
        "publish = false",
    ] {
        assert!(
            doc.contains(required),
            "docs/adopting.md must carry the verified install form ({required:?} missing)"
        );
    }
    assert!(
        !doc.contains("git@git.xantu-barb.ts.net:"),
        "the scp-style URL is the form that fails, and the doc must not offer it"
    );

    // The gate step's fail-closed assertion: an absent **or unreadable** `.req/variants` must
    // be red, because a failed input redirection does not stop a `set -e` script.
    for required in [
        "[ -r .req/variants ]",
        "req canon --verify",
        "req gate",
        ".req/variants",
        "https://git.xantu-barb.ts.net/fdx/req/issues",
    ] {
        assert!(
            doc.contains(required),
            "docs/adopting.md must carry the CI gate and the bug URL ({required:?} missing)"
        );
    }
}

// ---------------------------------------------------------------------------
// The CI gate step: executed, not just compared with the doc
// ---------------------------------------------------------------------------

/// The shell body of the CI gate step, extracted from the snippet `req init` prints — the same
/// bytes an adopter's runner executes. Extracted rather than duplicated: a third copy would be
/// free to drift, and the point of this test is to run *the* snippet.
#[cfg(unix)]
fn gate_script() -> String {
    let lines: Vec<&str> = req::init::CI_SNIPPET.lines().collect();
    // The snippet has several `run: |` blocks (install, verify, gate). The gate one is the block
    // that contains the variants loop, so the loop is the anchor and the opener is the nearest
    // `run: |` above it — positional, not "the first" or "the last".
    // Anchored on the redirection that closes the loop, not on a variable name: a
    // behaviour-preserving rename of the loop variable must not break the extractor.
    let loop_at = lines
        .iter()
        .position(|line| line.trim() == "done < .req/variants")
        .expect("the gate step loops over .req/variants");
    let at = lines[..loop_at]
        .iter()
        .rposition(|line| line.trim() == "run: |")
        .expect("the loop lives inside a `run: |` block");
    let run_indent = lines[at].len() - lines[at].trim_start().len();
    let mut body: Vec<&str> = Vec::new();
    for line in &lines[at + 1..] {
        let blank = line.trim().is_empty();
        let indent = line.len() - line.trim_start().len();
        if !blank && indent <= run_indent {
            break;
        }
        body.push(line);
    }
    let dedent = body
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .expect("the run block has a body");
    let mut script = String::new();
    for line in body {
        if line.len() > dedent {
            script.push_str(&line[dedent..]);
        }
        script.push('\n');
    }
    script
}

/// Write `.req/variants` with exactly `text`, so a test can control whether the last line is
/// newline-terminated.
#[cfg(unix)]
fn write_variants(fx: &Fixture, text: &str) {
    std::fs::create_dir_all(fx.join(".req")).expect("creates .req/");
    std::fs::write(fx.join(".req/variants"), text).expect("writes .req/variants");
}

/// Run the extracted CI step in `fx`; return its exit code and the job summary it wrote.
#[cfg(unix)]
fn run_gate_step(fx: &Fixture, script: &str) -> (i32, String) {
    let summary = fx.join("step-summary.md");
    // The step appends its rows; a fresh run must not read the previous case's.
    let _ = std::fs::remove_file(&summary);
    let bin_dir = req_bin()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(fx.path())
        .env("PATH", path)
        .env("FORGEJO_STEP_SUMMARY", &summary)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the CI gate step");
    let code = out.status.code().unwrap_or(-1);
    let rows = std::fs::read_to_string(&summary).unwrap_or_default();
    let text = format!(
        "{rows}{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (code, text)
}

/// Run the extracted CI step with a **stub `req`**: a script that answers the codes it is given, in
/// order. The loop's aggregation is shell logic, and a real corpus cannot produce every ordering
/// (a FAIL and an UNVERIFIABLE in the order that used to mask the FAIL), so the stub is what makes
/// the precedence observable.
#[cfg(unix)]
fn run_gate_step_with_codes(fx: &Fixture, script: &str, codes: &[i32]) -> (i32, String) {
    let bin = fx.join("stub-bin");
    std::fs::create_dir_all(&bin).expect("creates the stub bin directory");
    let queue = fx.join("stub-codes");
    std::fs::write(
        &queue,
        codes
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("writes the stub's answers");
    // The path is interpolated into generated shell, so it must be shell-escaped for a path
    // containing a single quote as well as a space: `'''` closes, escapes and reopens the quote.
    let queue_sh = queue.display().to_string().replace('\'', r"'\''");
    // Pop the next code; when the queue is empty, answer 0.
    std::fs::write(
        bin.join("req"),
        format!(
            "#!/bin/sh\n\
             queue='{queue}'\n\
             # Consume stdin the way any command might. The step must hand `req` its own\n\
             # stdin: a gate that read the loop's file would silently shorten the release list.\n\
             cat >/dev/null 2>&1 || true\n\
             code=0\n\
             if [ -s \"$queue\" ]; then\n\
               code=$(head -1 \"$queue\")\n\
               tail -n +2 \"$queue\" > \"$queue.tmp\" && mv \"$queue.tmp\" \"$queue\"\n\
             fi\n\
             echo \"verdict: stub $code\"\n\
             exit $code\n",
            queue = queue_sh
        ),
    )
    .expect("writes the stub req");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(bin.join("req"), std::fs::Permissions::from_mode(0o755))
        .expect("makes the stub executable");
    let summary = fx.join("step-summary.md");
    let _ = std::fs::remove_file(&summary);
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(fx.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("FORGEJO_STEP_SUMMARY", &summary)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the CI gate step");
    let rows = std::fs::read_to_string(&summary).unwrap_or_default();
    (
        out.status.code().unwrap_or(-1),
        format!("{rows}{}", String::from_utf8_lossy(&out.stderr)),
    )
}

/// The summary rows the step wrote, one per `gate()` call.
#[cfg(unix)]
fn summary_rows(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("| `"))
        .collect()
}

/// The CI step's loop is load-bearing for every adopter, and until this test nothing executed it:
/// the snippet was only pinned to `docs/adopting.md`, which cannot catch a broken loop. Four
/// behaviours decide whether the step protects a release or lies about it:
/// a comment-only file falls back to the default variant; a declared line is gated *instead*, and
/// its non-zero exit propagates — even when it is the file's unterminated last line; blank and
/// comment lines never become a bare run; and an absent file is red.
#[cfg(unix)]
#[test]
fn the_ci_gate_step_script_behaves_as_an_adopter_needs() {
    let script = gate_script();
    assert!(
        script.contains("done < .req/variants"),
        "the snippet still loops over .req/variants:\n{script}"
    );
    assert!(
        script.contains("[ -r .req/variants ]"),
        "and still fails closed on an absent or unreadable file:\n{script}"
    );

    // A corpus, exactly as `req init` leaves it.
    let fx = Fixture::new("ci-gate-step");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "{}{}", init.stdout, init.stderr);

    // (1) A comment-only file declares no release, so the bare invocation is the gate.
    write_variants(&fx, "# nothing declared yet\n");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(
        code, 0,
        "a comment-only file falls back to the default variant: {text}"
    );
    assert_eq!(summary_rows(&text).len(), 1, "exactly one gate run: {text}");
    assert!(
        text.contains("`<none>`"),
        "the bare run is labelled: {text}"
    );

    // (2) A declared line is gated *instead*, and its exit propagates — here as the file's last
    // line with **no trailing newline**, which `read` reports as non-zero while assigning it.
    write_variants(&fx, "deployment_mode=air_gapped");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(
        code, 3,
        "the declared line is gated and its refusal is the step's: {text}"
    );
    assert_eq!(
        summary_rows(&text).len(),
        1,
        "the loop is the gate; the bare fallback must not also run: {text}"
    );
    assert!(text.contains("deployment_mode=air_gapped"), "{text}");

    // (2b) A CRLF checkout must not leave a `\r` on the value: `read -a` does not split on carriage
    // return, so without the strip the gate would receive `deployment_mode=air_gapped\r`, refuse it
    // for the wrong reason, and write the CR into the job summary where nobody reads it.
    write_variants(&fx, "deployment_mode=air_gapped\r\n");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(
        code, 3,
        "the declared line is gated on a CRLF file too: {text}"
    );
    assert!(
        !text.contains('\r'),
        "no carriage return reaches the gate or the summary: {text:?}"
    );
    assert!(text.contains("deployment_mode=air_gapped"), "{text}");

    // (2c) More than one declared line: the loop must gate **each** of them. Every other case
    // writes one line, so a stray `break` (or a body that ran once) would pass them all.
    write_variants(&fx, "deployment_mode=air_gapped\nruns=5\n");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(code, 3, "both declared lines are gated: {text}");
    let rows = summary_rows(&text);
    assert_eq!(rows.len(), 2, "one row per declared line: {text}");
    assert!(rows[0].contains("deployment_mode=air_gapped"), "{text}");
    assert!(
        rows[1].contains("runs=5"),
        "the second line is gated too: {text}"
    );

    // (2d) Precedence across lines: a FAIL on one line must not be masked by an UNVERIFIABLE on a
    // later one. `rc` used to keep the *last* non-zero code, which reported 3 beside a real 1 — the
    // asymmetry the whole project keeps, inverted by the step adopters run.
    for (codes, expected, why) in [
        (
            vec![1, 3],
            1,
            "a later UNVERIFIABLE must not mask an earlier FAIL",
        ),
        (vec![3, 1], 1, "and the order must not matter"),
        (vec![3, 3], 3, "all undecided is still UNVERIFIABLE"),
        (
            vec![0, 3],
            3,
            "a pass beside an undecided line is UNVERIFIABLE",
        ),
        (vec![0, 0], 0, "all green is green"),
        // Anything non-zero that is not 1 or 3 must still fail the step: a usage error (2) or a
        // missing binary (127) reported as success is worse than the masked FAIL this rule fixed.
        (vec![2], 2, "a usage error is propagated"),
        (vec![127], 127, "a missing binary is propagated"),
        (vec![1, 2], 1, "and FAIL still outranks it"),
        (vec![2, 1], 1, "in either order"),
        (
            vec![0, 2],
            2,
            "a pass beside a usage error is the usage error",
        ),
        // Among non-FAIL codes the rule is the **first** failure, not the last: a missing
        // binary (127) followed by an UNVERIFIABLE (3) used to report 3, which reads as a
        // corpus question when the tool never ran.
        (
            vec![127, 3],
            127,
            "a missing binary is not reported as UNVERIFIABLE",
        ),
        (vec![3, 2], 3, "the first non-FAIL failure is reported"),
        (vec![2, 3], 2, "in either order"),
        (vec![3, 0], 3, "a later pass does not clear it"),
    ] {
        write_variants(&fx, "deployment_mode=air_gapped\nruns=5\n");
        let (code, text) = run_gate_step_with_codes(&fx, &script, &codes);
        assert_eq!(code, expected, "{why} ({codes:?}): {text}");
        assert_eq!(
            summary_rows(&text).len(),
            2,
            "one row per line ({codes:?}): {text}"
        );
    }

    // (3) Blank and comment-only lines are skipped, never turned into a bare run.
    write_variants(&fx, "\n   \n# a comment\n\n");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(code, 0, "{text}");
    assert_eq!(summary_rows(&text).len(), 1, "one run: {text}");
    assert!(
        text.contains("`<none>`"),
        "still the default variant: {text}"
    );

    // (4) An absent variants file is a failure, not a silently green step.
    std::fs::remove_file(fx.join(".req/variants")).expect("removes the variants file");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(code, 1, "an absent variants file is red: {text}");
}

/// A file that exists but cannot be read is not a file the step can gate: the redirection at
/// `done <` fails, the loop never runs, and the fallback would gate the bare variant instead of
/// the declared releases. `[ -f ]` proves existence, not readability — `[ -r ]` is the assertion.
#[cfg(unix)]
#[test]
fn an_unreadable_variants_file_is_red_not_a_fallback_to_the_bare_gate() {
    use std::os::unix::fs::PermissionsExt;

    let script = gate_script();
    let fx = Fixture::new("ci-gate-step-unreadable");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "{}{}", init.stdout, init.stderr);
    write_variants(&fx, "deployment_mode=air_gapped\n");
    let path = fx.join(".req/variants");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000))
        .expect("makes the variants file unreadable");
    // `access()` grants R_OK to uid 0 even on a 0o000 regular file, so a root container would
    // pass the guard and fail this test for an environment reason rather than a product one.
    // Probe the effect of the chmod and say NOT EXERCISED, as the abort test does.
    if std::fs::File::open(&path).is_ok() {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("restores the variants file");
        eprintln!(
            "NOT EXERCISED: mode 0o000 is still readable here (running as root?) — the \
             unreadable-variants guard was not exercised"
        );
        return;
    }
    let (code, text) = run_gate_step(&fx, &script);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .expect("restores the variants file so the fixture can be removed");
    assert_eq!(
        code, 1,
        "an unreadable variants file is red, never a green bare fallback: {text}"
    );
    assert!(
        text.contains("missing, unreadable, or not a regular file .req/variants"),
        "the step says which file and why: {text}"
    );
}

/// `[ -r ]` alone passes for a readable **directory**: the redirection then fails, the loop never
/// runs and the fallback gates the bare variant — the same false green the assertion exists to
/// prevent. Both halves (`-f` and `-r`) are asserted.
#[cfg(unix)]
#[test]
fn a_directory_at_the_variants_path_is_red() {
    let script = gate_script();
    let fx = Fixture::new("ci-gate-step-variants-dir");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "{}{}", init.stdout, init.stderr);
    std::fs::create_dir_all(fx.join(".req/variants")).expect("creates a directory at the path");
    let (code, text) = run_gate_step(&fx, &script);
    assert_eq!(code, 1, "a directory is not a release list: {text}");
    assert!(
        text.contains("not a regular file"),
        "the step says what the path is: {text}"
    );
}

/// The generated stub interpolates the queue path into shell; a fixture (or `TMPDIR`) whose path
/// contains a space must survive it. This test's fixture name carries one on purpose — and the
/// path is asserted to carry it, so a later normalisation of fixture names cannot turn this test
/// into a green no-op.
#[cfg(unix)]
#[test]
fn the_stub_survives_a_fixture_path_with_a_space() {
    let script = gate_script();
    let fx = Fixture::new("ci gate step spaced");
    assert!(
        fx.path().to_string_lossy().contains(' '),
        "the fixture name must reach the path, or this test checks nothing: {}",
        fx.path().display()
    );
    write_variants(&fx, "deployment_mode=air_gapped\n");
    let (code, text) = run_gate_step_with_codes(&fx, &script, &[3]);
    assert_eq!(code, 3, "one row, and its refusal is the step's: {text}");
    assert_eq!(summary_rows(&text).len(), 1, "one gate run: {text}");
}

/// The same pin for the escaping branch: a path carrying a single quote must not close the quote
/// in the generated stub. No other fixture (or `TMPDIR`) carries one, so without this the
/// `'\''` in `run_gate_step_with_codes` would be unpinned.
#[cfg(unix)]
#[test]
fn the_stub_survives_a_fixture_path_with_a_single_quote() {
    let script = gate_script();
    let fx = Fixture::new("ci gate step 'quoted'");
    assert!(
        fx.path().to_string_lossy().contains('\''),
        "the fixture name must reach the path: {}",
        fx.path().display()
    );
    write_variants(&fx, "deployment_mode=air_gapped\n");
    let (code, text) = run_gate_step_with_codes(&fx, &script, &[3]);
    assert_eq!(code, 3, "one row, and its refusal is the step's: {text}");
    assert_eq!(summary_rows(&text).len(), 1, "one gate run: {text}");
}
