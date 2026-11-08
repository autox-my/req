//! `req init` — the corpus it writes, and the guarantees that make running it safe.
//!
//! The seam is the **CLI**. Every check drives the compiled binary against a fresh
//! directory under the system temp dir (the pattern `tests/cli_version.rs` established),
//! because what `init` promises is a *file set a reader will use*: `req canon`, `req gate`
//! and the corpus loader all read it back. A private writer unit-tested in
//! isolation would prove nothing about that.
//!
//! The address is checked **twice, because the two checks fail differently**. A *golden*
//! derived outside this crate — the address the pack's prototype corpus produced before
//! `init` existed — is the independent oracle: recomputing the value with the same functions
//! `build` uses (read the claim, parse it, re-derive) asserts only that the two agree, which
//! an implementation that never derived a correct address would also satisfy. Keeping the
//! fresh derivation *beside* it catches the other failure: a loader whose derived address is
//! not the one its own canonical bytes hash to. Since `req.claim/2` (ADR 0022) the file stores
//! neither — the golden is the address the starter claim *derives*, unchanged by the format
//! bump. It is not "the implementation's output written down" — when `RULES_V` moves it is
//! regenerated deliberately, in that commit, as every other golden is.

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
        // in these fixtures (`canon`, `gate`) resolve one, and an inherited
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
// Acceptance 1-3: a fresh directory, a working corpus, a green gate
// ---------------------------------------------------------------------------

#[test]
fn init_creates_a_working_corpus_in_the_current_directory() {
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

    // The snippet is printed **exactly**, not merely present in the const: removing
    // `print!("{CI_SNIPPET}")` (or indenting the body) would leave every equality and
    // fragment pin green while `req init` stopped giving the adopter the CI file.
    assert!(
        out.stdout.contains(req::init::CI_SNIPPET),
        "`init` must print the CI snippet byte for byte:\n{}",
        out.stdout
    );

    // The next steps are printed, not written: a tool must not guess a forge. Every
    // item the pack requires is named — where to report a bug, and the per-variant rule.
    for required in [
        "https://github.com/autox-my/req/issues",
        ".req/variants",
        "req canon",
        "req gate",
    ] {
        assert!(
            out.stdout.contains(required),
            "`init` must print its next steps, including {required:?}:\n{}",
            out.stdout
        );
    }

    // The claim that landed is the authored fields of `req.claim/2` and nothing derived; the
    // loader derives its parse and address, checked here against the golden (the independent
    // recomputation of the corpus' addresses is `tests/address_recompute.rs`).
    let claims = req::canon::load_claims(fx.path()).expect("the corpus `init` wrote loads");
    assert_eq!(claims.len(), 1, "one starter claim, not a seed set");
    let claim = &claims[0].claim;
    let derived = &claims[0].derived;
    assert_eq!(claim.claim_id, "REQ-0001");
    assert_eq!(claim.schema, req::canon::claim_schema());
    assert_eq!(
        claim.statement,
        "The API shall respond to a health check within 200 ms."
    );
    assert_eq!(derived.form.modality_str(), "SHALL");
    assert_eq!(derived.form.subject, "API");
    assert_eq!(derived.form.operator_str(), "LE");
    let pins = claim
        .pins
        .as_ref()
        .expect("the starter claim pins its bound");
    assert_eq!(pins.operator.as_deref(), Some("LE"));
    assert_eq!(
        pins.value.as_deref(),
        Some("0.2"),
        "canonical SI, not 200 ms"
    );
    assert_eq!(pins.unit.as_deref(), Some("s"));
    assert_eq!(
        derived.pins.len(),
        3,
        "the starter claim ships three pins, each judged: {:?}",
        derived.pins
    );
    assert!(
        derived.pins.iter().all(|check| !check.flags()),
        "every pin the starter claim ships holds: {:?}",
        derived.pins
    );
    assert!(
        claim.applicability.is_empty(),
        "the starter claim is unconditional"
    );
    assert!(claim.relations.is_empty(), "no relation to invent");
    assert_iso_date(&claim.rationale.date);

    assert_eq!(
        derived.address, STARTER_ADDRESS,
        "the starter claim's address is a **golden derived outside this crate** — the prototype \
         corpus filled at the commit before `init` existed. A mismatch is either a derivation \
         that moved without a rules bump (a defect) or a deliberate `RULES_V` bump, which \
         regenerates this constant in the same commit"
    );
    let text = fx.read("claims/REQ-0001.yml");
    for key in req::model::REMOVED_V1_KEYS {
        assert!(
            !text
                .lines()
                .any(|line| line.starts_with(&format!("{key}:"))),
            "the starter claim stores nothing derived and no v1 slot (`{key}`):\n{text}"
        );
    }

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
        "req canon", // reports and never gates
        "req gate",
        "pins:",
        "SI",
        "derived",
        "address",
        "tools/migrate_claims.py",
        "tools/adopt.py",
        // The runnable form: required flags, and --write because the default is a dry run.
        "--from",
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

    // `canon` reports the derivation and every pin it holds, in the directory it created.
    let canon = run_at(fx.path(), &["canon"]);
    assert_eq!(canon.code, 0, "{}\n{}", canon.stdout, canon.stderr);
    assert!(
        canon.stdout.contains("pins: 3 on 1 claim(s), 0 flagged"),
        "the starter claim's three pins hold:\n{}",
        canon.stdout
    );

    // The release gate now asks the flagship question too: does a test that proves the claim
    // pass? A fresh corpus has no evidence yet, so the honest first answer is UNVERIFIABLE,
    // naming the claim — not PASS over a link nobody wrote.
    let gate = run_at(fx.path(), &["gate"]);
    assert_eq!(
        gate.code, 3,
        "a corpus with no evidence is UNVERIFIABLE, never a pass:\n{}\n{}",
        gate.stdout, gate.stderr
    );
    assert!(
        gate.stdout.contains("tests_linked") && gate.stdout.contains("no --test-report"),
        "the gate names the policy it cannot exercise:\n{}",
        gate.stdout
    );

    // Supply the evidence: link the starter claim and hand the gate the report that carries it.
    let claim_path = fx.join("claims/REQ-0001.yml");
    let claim = std::fs::read_to_string(&claim_path).expect("reads the starter claim");
    let linked = claim.replace(
        "relations: []\n",
        "relations: []\ntests:\n  - \"init::starter\"\n",
    );
    assert_ne!(linked, claim, "the link was authored");
    std::fs::write(&claim_path, linked).expect("writes the linked claim");
    let report = fx.join("junit.xml");
    std::fs::write(
        &report,
        "<?xml version=\"1.0\"?>\n<testsuites name=\"init\"><testsuite name=\"init\">\
         <testcase name=\"starter\" classname=\"init\"/></testsuite></testsuites>\n",
    )
    .expect("writes the report");
    let evidenced = run_at(
        fx.path(),
        &[
            "gate",
            "--test-report",
            report.to_str().expect("a UTF-8 fixture path"),
        ],
    );
    assert_eq!(
        evidenced.code, 0,
        "a linked passing test is the proof the gate asks for:\n{}\n{}",
        evidenced.stdout, evidenced.stderr
    );
    assert!(
        evidenced.stdout.contains("verdict: PASS"),
        "the gate's verdict line:\n{}",
        evidenced.stdout
    );

    // The pins are part of the gate it passed: `pins_hold` held all three.
    assert!(
        evidenced.stdout.contains("[PASS] pins_hold"),
        "{}",
        evidenced.stdout
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
    let verify = run_at(&child, &["canon"]);
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

    // The source install (ADR 0029 §5): public https, pinned by rev, locked. No ssh key, no
    // libgit2 workaround, and no private host in any spelling.
    for required in [
        "cargo install --git https://github.com/autox-my/req.git",
        "--rev <PIN>",
        "--locked",
        "cargo install --path .",
        "publish = false",
    ] {
        assert!(
            doc.contains(required),
            "docs/adopting.md must carry the source install form ({required:?} missing)"
        );
    }
    for gone in [
        "ssh://",
        "git@",
        "GIT_SSH_COMMAND",
        "CARGO_NET_GIT_FETCH_WITH_CLI",
        "StrictHostKeyChecking",
    ] {
        assert!(
            !doc.contains(gone),
            "the retired SSH install ({gone:?}) must not come back into docs/adopting.md"
        );
    }

    // The release step is `req check` (ADR 0023): the page names the command, the file it reads
    // and the bug URL; the loop that used to be shell is req's, not the page's.
    for required in [
        "if req check --test-report",
        "req check",
        ".req/variants",
        "https://github.com/autox-my/req/issues",
    ] {
        assert!(
            doc.contains(required),
            "docs/adopting.md must carry the CI release step and the bug URL ({required:?} missing)"
        );
    }
}

// ---------------------------------------------------------------------------
// The CI steps: executed, not just compared with the doc
// ---------------------------------------------------------------------------
//
// The snippet's shell is two small programs since ADR 0023: the release step runs `req check` and
// exits with its status, and the JSON-views step captures four documents. The loop over
// `.req/variants`, the fold and the list grammar are `req check`'s and are tested in
// `tests/check.rs`; what is left here is what the shell still owns — that the step's status IS the
// command's, that the summary is written, and that a view that wrote nothing leaves no file.

/// The shell body of the release step, extracted from the snippet `req init` prints — the same
/// bytes an adopter's runner executes. Extracted rather than duplicated: a third copy would be
/// free to drift, and the point of these tests is to run *the* snippet.
#[cfg(unix)]
fn check_script() -> String {
    run_block_containing("if req check --test-report")
}

/// The shell body of the CI **JSON-views** step, extracted by the same positional rule: the
/// anchor is the `req.check/1` capture (the step's own logic), and the opener is the nearest
/// `run: |` above it. A separate test from the release step's, because the two steps are separate
/// shell programs and one running says nothing about the other.
#[cfg(unix)]
fn artifact_script() -> String {
    run_block_containing("capture check check --test-report")
}

/// The shell body of the `run: |` block in the snippet `req init` prints that contains `anchor`.
#[cfg(unix)]
fn run_block_containing(anchor: &str) -> String {
    let lines: Vec<&str> = req::init::CI_SNIPPET.lines().collect();
    // The snippet has several `run: |` blocks (install, PATH, tests, check, JSON views).
    // The block asked for is the one containing the anchor, and the opener is the nearest
    // `run: |` above it — positional, not "the first" or "the last".
    let anchor_at = lines
        .iter()
        .position(|line| line.contains(anchor))
        .unwrap_or_else(|| panic!("the snippet has no line containing {anchor:?}"));
    let at = lines[..anchor_at]
        .iter()
        .rposition(|line| line.trim() == "run: |")
        .expect("the anchor lives inside a `run: |` block");
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

/// Write `.req/variants` with exactly `text`.
#[cfg(unix)]
fn write_variants(fx: &Fixture, text: &str) {
    std::fs::create_dir_all(fx.join(".req")).expect("creates .req/");
    std::fs::write(fx.join(".req/variants"), text).expect("writes .req/variants");
}

/// The environment a runner gives the release step: the binary directory first on PATH, the
/// job-summary file, and a `RUNNER_TEMP` of the fixture's own (the step writes its capture there).
#[cfg(unix)]
fn run_release_step(fx: &Fixture, script: &str, bin_dir: &Path) -> (i32, String, String) {
    let summary = fx.join("step-summary.md");
    // The step appends to the summary; a fresh run must not read the previous case's.
    let _ = std::fs::remove_file(&summary);
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(fx.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("GITHUB_STEP_SUMMARY", &summary)
        .env("RUNNER_TEMP", fx.path())
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the CI release step");
    (
        out.status.code().unwrap_or(-1),
        std::fs::read_to_string(&summary).unwrap_or_default(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// A **stub `req`** that records its argv and answers `code`, printing `emit` (a text, or nothing)
/// on stdout. The stub lives in `<fx>/stub-bin`, and every path interpolated into the generated
/// shell is escaped for a single quote as well as a space (`'''` closes, escapes and reopens it).
#[cfg(unix)]
fn stub_req(fx: &Fixture, emit: Option<&str>, code: i32) -> (PathBuf, PathBuf) {
    let bin = fx.join("stub-bin");
    std::fs::create_dir_all(&bin).expect("creates the stub bin directory");
    let argv_log = fx.join("stub-argv");
    let _ = std::fs::remove_file(&argv_log);
    let argv_sh = argv_log.display().to_string().replace('\'', r"'\''");
    let out_sh = match emit {
        Some(text) => format!("printf '%s\\n' '{}'", text.replace('\'', r"'\''")),
        None => ":".to_string(),
    };
    std::fs::write(
        bin.join("req"),
        format!(
            "#!/bin/sh\n\
             printf '%s\\n' \"$*\" >> '{argv}'\n\
             {out}\n\
             exit {code}\n",
            argv = argv_sh,
            out = out_sh,
            code = code
        ),
    )
    .expect("writes the stub req");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(bin.join("req"), std::fs::Permissions::from_mode(0o755))
        .expect("makes the stub executable");
    (bin, argv_log)
}

/// What the stub was called with, one entry per call.
#[cfg(unix)]
fn argv_of(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The release step's status is `req check`'s, whatever it is. The aggregation lives in `req`, so
/// the shell's whole job is to not disturb the code: 0 stays 0, and 1, 2, 3 — and a code req does
/// not define at all — come out as they went in. The stub makes every code observable; the real
/// binary is exercised in `tests/ci_template.rs`.
#[cfg(unix)]
#[test]
fn the_release_step_exits_with_the_checks_exit_whatever_it_is() {
    let script = check_script();
    for code in [0, 1, 2, 3, 4, 127] {
        let fx = Fixture::new("ci-release-step-codes");
        let (bin, argv) = stub_req(&fx, Some(&format!("verdict: stub {code}")), code);
        let (got, summary, text) = run_release_step(&fx, &script, &bin);
        assert_eq!(
            got, code,
            "the step must exit {code} as `req check` did:\n{summary}{text}"
        );
        assert_eq!(
            argv_of(&argv),
            ["check --test-report target/nextest/ci/junit.xml"],
            "exactly one `req check`, handed the report and nothing narrowing the steps"
        );
        assert!(
            summary.contains(&format!("| {code} | stub {code} |")),
            "the summary names the exit and the verdict line:\n{summary}"
        );
        assert!(
            text.contains(&format!("verdict: stub {code}")),
            "the command's own output reaches the log:\n{text}"
        );
    }

    // A req that prints no `verdict:` line is named, not summarised as a pass.
    let fx = Fixture::new("ci-release-step-no-verdict");
    let (bin, _) = stub_req(&fx, None, 3);
    let (got, summary, _) = run_release_step(&fx, &script, &bin);
    assert_eq!(got, 3);
    assert!(summary.contains("| 3 | no verdict line |"), "{summary}");
}

/// The generated stub interpolates the fixture path into shell, and so does the step (it writes
/// its capture under `RUNNER_TEMP`): a path with a space, and one with a single quote, must
/// survive both. The fixture names carry the character on purpose — and the path is asserted to
/// carry it, so a later normalisation of fixture names cannot turn these into green no-ops.
#[cfg(unix)]
#[test]
fn the_release_step_survives_a_fixture_path_with_a_space_or_a_quote() {
    let script = check_script();
    for (name, needle) in [
        ("ci release step spaced", ' '),
        ("ci release step 'quoted'", '\''),
    ] {
        let fx = Fixture::new(name);
        assert!(
            fx.path().to_string_lossy().contains(needle),
            "the fixture name must reach the path, or this test checks nothing: {}",
            fx.path().display()
        );
        let (bin, _) = stub_req(&fx, Some("verdict: stub 3"), 3);
        let (got, summary, text) = run_release_step(&fx, &script, &bin);
        assert_eq!(got, 3, "the refusal is the step's:\n{summary}{text}");
        assert!(summary.contains("| 3 | stub 3 |"), "one row:\n{summary}");
    }
}

/// The real binary through the real step, on the two list shapes the old shell had to special-case
/// and `req check` now owns: a comment-only list is the default variant and says so, and a
/// directory at the path is a pinned list that became unusable — both end as 3 on a corpus with no
/// report, never as a green step.
#[cfg(unix)]
#[test]
fn the_release_step_runs_the_real_check_against_a_fresh_corpus() {
    let script = check_script();
    let bin_dir = req_bin()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    let fx = Fixture::new("ci-release-step-real");
    let init = run_at(fx.path(), &["init"]);
    assert_eq!(init.code, 0, "{}{}", init.stdout, init.stderr);

    write_variants(&fx, "# nothing declared yet\n");
    let (code, summary, text) = run_release_step(&fx, &script, &bin_dir);
    assert_eq!(
        code, 3,
        "a comment-only list is the default variant, and a corpus with no report is \
         UNVERIFIABLE:\n{summary}{text}"
    );
    assert!(
        text.contains("(.req/variants declares no release: the default variant)"),
        "the default variant is said, not implied:\n{text}"
    );
    assert!(
        !text.contains("verdict: PASS") && !summary.contains("PASS"),
        "an UNVERIFIABLE run never prints PASS:\n{summary}{text}"
    );

    std::fs::remove_file(fx.join(".req/variants")).expect("removes the list");
    std::fs::create_dir_all(fx.join(".req/variants")).expect("a directory at the path");
    let (code, _, text) = run_release_step(&fx, &script, &bin_dir);
    assert_eq!(
        code, 3,
        "a directory is not a release list, and not 'absent':\n{text}"
    );
}

/// The files under `req-views/`, sorted — a capture that wrote nothing must leave nothing, and a
/// `.tmp` left behind is a document nobody uploads.
#[cfg(unix)]
fn view_files(fx: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fx.join("req-views"))
        .expect("the step creates req-views/")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Run the extracted JSON-views step against a stub `req` that records its argv and answers `emit`
/// on stdout with `code`. Returns the step's exit code, its combined output, and the argv of every
/// `req` call in order.
#[cfg(unix)]
fn run_artifact_step(fx: &Fixture, emit: Option<&str>, code: i32) -> (i32, String, Vec<String>) {
    let (bin, argv_log) = stub_req(fx, emit, code);
    let out = Command::new("bash")
        .arg("-c")
        .arg(artifact_script())
        .current_dir(fx.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the JSON-views step");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        argv_of(&argv_log),
    )
}

/// The JSON-views step is the part of the snippet with real shell logic that nothing else
/// executes: the byte/pin checks cannot tell a working `capture` from a broken one. Three
/// behaviours decide whether an agent reads a verdict or a fabrication:
///
/// 1. four documents (render, trace, audit and the ONE `req.check/1`), the flag reaches `req`
///    *after* the command's own args, and no `.tmp` survives;
/// 2. a non-zero command that DID write a document keeps it — the `-s` gate is about bytes, never
///    a proxy for the exit class (a document is complete even when its verdict is open);
/// 3. a command that fails **without writing a document** (the unreadable-corpus shape) leaves no
///    file at all, and neither does a zero-exit command with empty stdout — and the step knows
///    how many documents it expects, so the *set* is red.
#[cfg(unix)]
#[test]
fn the_json_views_step_captures_four_documents_and_never_fabricates_one() {
    let fx = Fixture::new("ci-json-views");
    let (code, text, argv) = run_artifact_step(&fx, Some("{\"verdict\":\"pass\"}"), 0);
    assert_eq!(code, 0, "{text}");
    assert_eq!(
        view_files(&fx),
        ["audit.json", "check.json", "render.json", "trace.json"],
        "one document per view, and ONE for the whole release check:\n{text}"
    );
    assert_eq!(
        argv.iter().map(String::as_str).collect::<Vec<_>>(),
        [
            "render --format json",
            "trace --format json",
            "audit --format json",
            "check --test-report target/nextest/ci/junit.xml --format json",
        ],
        "`--format json` must come after the command's own args, and `check` reads the report"
    );

    let fx = Fixture::new("ci-json-views-open-verdict");
    let (code, text, _) = run_artifact_step(&fx, Some("{\"verdict\":\"unverifiable\"}"), 3);
    assert_eq!(code, 0, "{text}");
    assert!(
        fx.join("req-views/check.json").is_file(),
        "a document written under a non-zero exit is complete and must be kept:\n{text}"
    );

    // A command that writes nothing leaves no file, and the step knows how many documents it
    // expects — so the *set* is red even though each individual exit class was recorded. The
    // upload's `if-no-files-found` would only notice a completely empty directory.
    for (name, code_in) in [("ci-json-views-empty", 3), ("ci-json-views-empty-zero", 0)] {
        let fx = Fixture::new(name);
        let (code, text, _) = run_artifact_step(&fx, None, code_in);
        assert_eq!(
            code, 1,
            "{name}: a missing document set is red, whatever the exit class:\n{text}"
        );
        assert!(
            view_files(&fx).is_empty(),
            "{name}: a view that wrote nothing left files: {:?}\n{text}",
            view_files(&fx)
        );
        assert!(text.contains("NO DOCUMENT"), "{text}");
        assert!(text.contains("incomplete view set"), "{text}");
    }
}
