//! `req init` — the corpus it writes, and the guarantees that make running it safe.
//!
//! The seam is the **CLI**. Every check drives the compiled binary against a fresh
//! directory under the system temp dir (the pattern `tests/cli_version.rs` established),
//! because what `init` promises is a *file set a reader will use*: `req canon --verify`,
//! `req gate` and the corpus loader all read it back. A private writer unit-tested in
//! isolation would prove nothing about that.
//!
//! The address check is deliberately a **fresh derivation** — read the claim the run
//! produced, parse it, and re-derive the address from the parse with the same public
//! functions `canon --fill` uses — never a digest literal. A literal would pass today and
//! go stale on the next `RULES_V` bump, which is exactly the failure the starter claim
//! must not ship with.

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
        Some(derived.as_str()),
        "the stored address must be the one a fresh derivation produces — a literal digest \
         passes here only by accident and goes stale on a RULES_V bump"
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

    // The README carries the authoring loop and its traps, in the adopter's words.
    let readme = fx.read("claims/README.md");
    for required in [
        "req canon --fill",
        "req canon --verify",
        "req canon", // reports and always exits 0
        "SI",
        "refused",
        "address",
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

    // The gate step's fail-closed assertion: an absent `.req/variants` must be red, because
    // a failed input redirection does not stop a `set -e` script.
    for required in [
        "[ -f .req/variants ]",
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
