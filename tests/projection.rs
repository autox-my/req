//! Committed projections: a document that is a function of the corpus, proven current by
//! regeneration (ADR 0018, first instance).
//!
//! The oracle is the corpus, never the file: every check here regenerates the projection and
//! compares **bytes**, so the failure mode each test pins is a comparison that is cheaper than
//! the truth — the header alone, the claim count alone, a default context in place of the one
//! the header names. Those are exactly the mutations the pack's table requires this suite to
//! redden.
//!
//! The helpers follow `tests/cli_version.rs`: `req_bin()` runs the compiled binary so the
//! assertions are about the CLI contract, and a fixture corpus is created with `req init`
//! under the system temp dir (never `CARGO_TARGET_TMPDIR`, whose ancestors hold this
//! repository's own `claims/`). No minisign, no signed Baseline, no network.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

mod common;

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
        // The resolver reads `REQ_ROOT` before the walk-up, so an inherited value would name
        // a corpus and make each fixture check something other than what it says.
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Distinguishes fixtures within one process; the process id extends it across processes.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir(name: &str) -> PathBuf {
    let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "req-projection-{name}-{}-{seq}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creates the fixture directory");
    dir
}

/// A directory with **no `claims/` ancestor**, for the help pages (which must not need a
/// corpus). The absence is asserted rather than assumed within the lifecycle of the fixture.
struct NoCorpus(PathBuf);

impl NoCorpus {
    fn new(name: &str) -> Self {
        let dir = unique_dir(name);
        if let Some(found) = dir.ancestors().find(|d| req::root::is_corpus_root(d)) {
            panic!(
                "the fixture assumes no ancestor holds claims/, but {} does",
                found.display()
            );
        }
        NoCorpus(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for NoCorpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A two-claim corpus created by the real bootstrap (`req init`), so the fixture's claims carry
/// derived fields the derivation wrote — never a hand-written address.
struct Corpus(PathBuf);

impl Corpus {
    fn new(name: &str) -> Self {
        let dir = unique_dir(name);
        let out = run_at(&dir, &["init"]);
        assert_eq!(
            out.code, 0,
            "`req init` must create the fixture corpus:\n{}\n{}",
            out.stdout, out.stderr
        );
        let corpus = Corpus(dir);
        corpus.add_second_claim();
        corpus
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn claim_path(&self, id: &str) -> PathBuf {
        self.path().join("claims").join(format!("{id}.yml"))
    }

    /// The pack asks for a two-claim fixture: a projection whose claim count is unchanged by
    /// the edit in `changing_a_claim_makes_the_old_projection_stale` is what proves a
    /// count-only comparison cannot pass.
    fn add_second_claim(&self) {
        let first = fs::read_to_string(self.claim_path("REQ-0001")).expect("the starter claim");
        let second = first
            .replace("claim_id: REQ-0001", "claim_id: REQ-0002")
            .replace(
                "statement: \"The API shall respond to a health check within 200 ms.\"",
                "statement: \"The API shall log a health check within 200 ms.\"",
            );
        assert_ne!(second, first, "the second claim must be a real edit");
        assert!(
            second.contains("The API shall log a health check within 200 ms."),
            "the fixture must actually move the statement, not only the claim id"
        );
        fs::write(self.claim_path("REQ-0002"), second).expect("writes the second claim");
        // The derived fields are the derivation's to write: the statement moved, so the
        // address must be re-derived exactly as a human would do it.
        let out = run_at(self.path(), &["canon", "--fill"]);
        assert_eq!(
            out.code, 0,
            "the fixture corpus must be filled and green:\n{}\n{}",
            out.stdout, out.stderr
        );
    }

    /// A claim the empty context leaves **undecided** — the projection must still be written
    /// (the exit class belongs to the verdict, not to the artefact), and the claim must appear
    /// in its own section.
    fn make_second_claim_undecided(&self) {
        let path = self.claim_path("REQ-0002");
        let text = fs::read_to_string(&path).expect("the second claim");
        let replaced = text.replace(
            "applicability: []",
            "applicability:\n  - deployment_mode == cloud",
        );
        assert_ne!(replaced, text, "the fixture must actually add a predicate");
        fs::write(&path, replaced).expect("writes the predicate");
        let out = run_at(self.path(), &["canon", "--fill"]);
        assert_eq!(
            out.code, 0,
            "the scoped claim must be filled and green:\n{}\n{}",
            out.stdout, out.stderr
        );
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path().join(name);
        fs::write(&path, text).expect("writes the fixture file");
        path
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path().join(name)).expect("reads the fixture file")
    }

    fn out(&self, args: &[&str]) -> Outcome {
        run_at(self.path(), args)
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The projection's first line as written, or a failure that shows the whole artefact.
fn header_of(projection: &str) -> &str {
    projection
        .lines()
        .next()
        .expect("a projection has a header")
}

/// `identity=sha256:<64 lowercase hex>`, taken from the written header. The closing ` -->` is
/// part of the last field's spelling, so it is stripped here rather than in the assertion.
fn identity_field(projection: &str) -> &str {
    let header = header_of(projection);
    let fields = header.strip_suffix(" -->").unwrap_or(header);
    fields
        .split(" | ")
        .find_map(|field| field.strip_prefix("identity="))
        .expect("the header carries an identity field")
}

/// The hand-computed fixture: `sha256` over the sorted addresses, each followed by `\n`.
///
/// The expected digest was computed independently of this crate (the bytes are
/// `"a"*64 + "\n" + "b"*64 + "\n"` and the value comes from `sha256sum`/`hashlib`, not from the
/// implementation). The addresses are passed **reversed** on purpose: the fixture reddens if
/// the sort is dropped, and a changed separator reddens it too.
#[test]
fn the_identity_digest_definition_is_pinned_by_a_hand_computed_fixture() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    assert_eq!(
        req::view::identity_digest(&[b.clone(), a.clone()]),
        "913f9338fb6c17253f3a14816fc08d52522454bfc19a8c99538a197cfb23fb41",
        "sha256 over sorted(address + \"\\n\") — sort must be applied, separator must be `\\n`"
    );
    assert_eq!(
        req::view::identity_digest(&[]),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "the empty applying set hashes the empty string"
    );
    // Two orderings of the same addresses are one identity.
    assert_eq!(
        req::view::identity_digest(&[a.clone(), b.clone()]),
        req::view::identity_digest(&[b, a.clone()])
    );
    // The digest is over the addresses, not their count: an extra address moves it.
    assert_ne!(
        req::view::identity_digest(std::slice::from_ref(&a)),
        req::view::identity_digest(&[a.clone(), a])
    );
}

/// Fact 4, at the CLI seam a committed projection relies on: two invocations are one
/// document. A `HashMap` iteration anywhere in the view's ordering reddens this, because each
/// invocation is a separate process with its own hash seed — so the assertion runs over this
/// repository's 21-claim corpus, where two hash orders nearly never coincide (the two-claim
/// fixture would let an ordering defect through half the time).
#[test]
fn render_twice_is_byte_identical() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let first = run_at(root, &["render"]);
    let second = run_at(root, &["render"]);
    assert_eq!(
        first.stdout, second.stdout,
        "the spec view must be a function of the corpus"
    );
    assert_eq!(first.code, second.code);
    assert!(
        first.stdout.ends_with('\n'),
        "the view ends with a newline, so two runs cannot differ only in their tail"
    );

    // The same property on a small corpus, so the test does not depend on this repository's
    // claim count.
    let corpus = Corpus::new("render-twice");
    let first = corpus.out(&["render"]);
    let second = corpus.out(&["render"]);
    assert_eq!(first.stdout, second.stdout);
}

/// Fact 5 / R1: the plain command keeps today's stdout and exit class exactly. The header is
/// written by `--out` only, so a plain print must not carry it.
#[test]
fn plain_render_keeps_its_stdout_and_exit_class() {
    let corpus = Corpus::new("plain-render");
    let plain = corpus.out(&["render"]);
    assert_eq!(plain.code, 0, "a settled selection is 0:\n{}", plain.stderr);
    assert!(
        plain.stdout.starts_with("# Selection: "),
        "plain render starts with the readable selection line:\n{}",
        plain.stdout
    );
    assert!(
        !plain.stdout.contains("<!-- req projection:"),
        "the header belongs to `--out`, never to a print for a human:\n{}",
        plain.stdout
    );

    corpus.make_second_claim_undecided();
    let undecided = corpus.out(&["render"]);
    assert_eq!(
        undecided.code, 3,
        "an undecided claim keeps the plain command exit 3:\n{}",
        undecided.stderr
    );
    assert!(
        !undecided.stdout.contains("<!-- req projection:"),
        "still no header on plain stdout"
    );

    // The same corpus writes a projection at exit 0: the artefact is complete and honest, and
    // the exit class belongs to the verdict rather than to the file.
    let out = corpus.out(&["render", "--out", "spec.md"]);
    assert_eq!(
        out.code, 0,
        "writing a projection is 0 even when a claim is undecided:\n{}",
        out.stderr
    );
    let text = corpus.read("spec.md");
    assert!(
        text.contains("## Undecided (1)"),
        "the projection is honest about the undecided claim:\n{text}"
    );
}

/// R2: the header names the generator, the build, the selection, the applying-claim count and
/// the identity digest, on one line, as an HTML comment so it is invisible when rendered.
#[test]
fn the_header_names_what_the_projection_was_built_from() {
    let corpus = Corpus::new("header");
    let out = corpus.out(&["render", "--out", "spec.md"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = corpus.read("spec.md");
    let header = header_of(&text);

    assert!(
        header.starts_with("<!-- req projection: "),
        "the header is an HTML comment: {header}"
    );
    assert!(
        header.ends_with(" -->"),
        "the header closes its comment: {header}"
    );
    assert!(
        header.contains("generator=req render"),
        "the generator is the command that produced it: {header}"
    );
    assert!(
        header.contains(&format!("rules_v={}", req::canon::RULES_V)),
        "the rules version is this build's: {header}"
    );
    assert!(
        header.contains("selection=none"),
        "no context is the explicit `none`: {header}"
    );
    assert!(
        header.contains("claims=2"),
        "the count is the applying claims: {header}"
    );
    let identity = identity_field(&text);
    let hex = identity
        .strip_prefix("sha256:")
        .expect("the identity is sha256-prefixed");
    assert_eq!(hex.len(), 64, "the digest is 64 hex characters: {identity}");
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "the digest is lowercase hex: {identity}"
    );
    assert_eq!(
        text.matches("<!-- req projection:").count(),
        1,
        "one header line per projection"
    );
}

/// R2: the body is exactly what the plain command prints today — the header is prepended,
/// nothing else moves.
#[test]
fn the_projection_body_is_the_plain_commands_stdout() {
    let corpus = Corpus::new("body");
    let out = corpus.out(&["render", "--out", "spec.md"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let plain = corpus.out(&["render"]);
    let text = corpus.read("spec.md");
    let body = text
        .split_once('\n')
        .map(|(_, body)| body)
        .expect("the projection has a body");
    assert_eq!(
        body, plain.stdout,
        "the header is the only difference between the artefact and the print"
    );
    assert!(
        text.ends_with('\n'),
        "the artefact ends with a newline, like the print it wraps"
    );
}

/// The base case: `--out` then `--check` is current, and the check wrote nothing.
#[test]
fn out_then_check_is_current_and_check_writes_nothing() {
    let corpus = Corpus::new("out-check");
    let written = corpus.out(&["render", "--out", "spec.md"]);
    assert_eq!(written.code, 0, "{}", written.stderr);
    let before = corpus.read("spec.md");

    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 0,
        "a projection just written is current:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    assert!(
        checked.stdout.contains("[CURRENT]"),
        "the verdict says what it found:\n{}",
        checked.stdout
    );
    assert_eq!(
        corpus.read("spec.md"),
        before,
        "`--check` answers a question; it never rewrites the artefact"
    );
}

/// R3.5 and the pack's falsification case 1: a difference in the **body** is stale, even
/// though the header still agrees. Comparing the header alone would pass.
#[test]
fn a_hand_edited_projection_is_stale() {
    let corpus = Corpus::new("hand-edit");
    assert_eq!(corpus.out(&["render", "--out", "spec.md"]).code, 0);
    let text = corpus.read("spec.md");
    let edited = text.replace("applies 2,", "applies 3,");
    assert_ne!(edited, text, "the edit must touch the body");
    corpus.write("spec.md", &edited);

    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 1,
        "an edited projection is a defect, not a pass:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    assert!(
        checked.stderr.contains("line 3"),
        "the message names the first differing line:\n{}",
        checked.stderr
    );
    assert!(
        checked.stderr.contains("--out"),
        "the remedy is regeneration:\n{}",
        checked.stderr
    );
}

/// R3.5: the corpus moved under an old projection. The claim count is **unchanged** (2), so a
/// count-only comparison would pass; the bytes are what decides.
#[test]
fn changing_a_claim_makes_the_old_projection_stale() {
    let corpus = Corpus::new("corpus-moved");
    assert_eq!(corpus.out(&["render", "--out", "spec.md"]).code, 0);
    let before = corpus.read("spec.md");
    assert!(
        before.contains("claims=2"),
        "the count this test keeps constant: {before}"
    );

    let path = corpus.claim_path("REQ-0002");
    let claim = fs::read_to_string(&path).expect("the second claim");
    let edited = claim.replace(
        "The API shall log a health check within 200 ms.",
        "The API shall archive a health check within 200 ms.",
    );
    assert_ne!(claim, edited, "the claim must actually move");
    fs::write(&path, edited).expect("writes the edited claim");
    let filled = run_at(corpus.path(), &["canon", "--fill"]);
    assert_eq!(filled.code, 0, "{}", filled.stderr);

    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 1,
        "the corpus moved, so the old projection is stale:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    // The header still names two applying claims; only the body moved.
    assert!(corpus.read("spec.md").contains("claims=2"));
}

/// R3.2 / ADR 0018 §3: another build's `rules_v` is UNVERIFIABLE, never FAIL — the addresses
/// mean something else, so the comparison is void rather than failed.
#[test]
fn a_projection_under_another_rules_version_is_unverifiable() {
    let corpus = Corpus::new("rules-mismatch");
    assert_eq!(corpus.out(&["render", "--out", "spec.md"]).code, 0);
    let text = corpus.read("spec.md");
    let current = format!("rules_v={}", req::canon::RULES_V);
    assert!(
        text.contains(&current),
        "the fixture starts current: {text}"
    );
    let older = req::canon::RULES_V - 1;
    let edited = text.replacen(&current, &format!("rules_v={older}"), 1);
    corpus.write("spec.md", &edited);

    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 3,
        "a rules mismatch is UNVERIFIABLE, not stale:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    assert!(
        checked.stderr.contains("rules_v"),
        "the message names the axis:\n{}",
        checked.stderr
    );
    assert!(
        checked.stderr.contains("--out"),
        "with the regenerate remedy:\n{}",
        checked.stderr
    );
}

/// R2/R3.1: a file produced by `render > file` has no header. It is not a projection this
/// decision covers, and saying so (3) is the useful answer — treating it as current is the
/// failure. Both shapes are covered: a body with no header line at all (grammar error), and an
/// empty file (nothing to parse).
#[test]
fn a_file_without_a_header_is_not_a_projection() {
    let corpus = Corpus::new("bare-print");
    let plain = corpus.out(&["render"]);
    corpus.write("spec.md", &plain.stdout);

    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 3,
        "a bare print is not a committed projection:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    // On the message, not the whole of stderr: the resolved `corpus root:` line carries the
    // fixture's own name, so a substring assert over stderr could be satisfied by a directory
    // called `no-header` while the diagnostic says nothing about a header (that false pass is
    // exactly what the tag rename above prevents).
    let message = checked
        .stderr
        .lines()
        .find(|line| line.contains("is not a projection"))
        .unwrap_or_else(|| panic!("the verdict names the problem:\n{}", checked.stderr));
    assert!(
        message.contains("header"),
        "the message names what is missing:\n{}",
        checked.stderr
    );

    corpus.write("empty.md", "");
    let empty = corpus.out(&["render", "--check", "empty.md"]);
    assert_eq!(
        empty.code, 3,
        "an empty file has no header to judge:\n{}\n{}",
        empty.stdout, empty.stderr
    );
    let empty_message = empty
        .stderr
        .lines()
        .find(|line| line.contains("is not a projection"))
        .unwrap_or_else(|| panic!("the verdict names the problem:\n{}", empty.stderr));
    assert!(
        empty_message.contains("header"),
        "the message names what is missing:\n{}",
        empty.stderr
    );
}

/// R3.3: a header whose selection cannot be reproduced is UNVERIFIABLE. Falling back to the
/// empty context and comparing would report stale/current about a different question.
#[test]
fn an_unparsable_selection_is_unverifiable() {
    let corpus = Corpus::new("unparsable-pair");
    assert_eq!(corpus.out(&["render", "--out", "spec.md"]).code, 0);
    let text = corpus.read("spec.md");
    let edited = text.replacen("selection=none", "selection=garbage", 1);
    assert_ne!(text, edited, "the fixture must edit the selection");
    corpus.write("spec.md", &edited);

    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 3,
        "an unreproducible selection cannot be judged:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    assert!(
        checked.stderr.contains("selection"),
        "the message names the field:\n{}",
        checked.stderr
    );
}

/// R1: one invocation, one question.
#[test]
fn out_and_check_together_are_a_usage_error() {
    let corpus = Corpus::new("both-flags");
    let out = corpus.out(&["render", "--out", "a.md", "--check", "b.md"]);
    assert_eq!(
        out.code, 2,
        "`--out` and `--check` together are usage:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        !corpus.path().join("a.md").exists(),
        "a refused invocation writes nothing"
    );
    // A missing value is usage too, rather than a silent read of the next token.
    let missing = corpus.out(&["render", "--out"]);
    assert_eq!(missing.code, 2, "{}", missing.stderr);
}

/// An option-shaped argument is never a context pair. Without this, `--out=spec.md` becomes the
/// selection `--out == spec.md`, the command prints instead of writing, and exit 0 reads as
/// "written" — a false green on exactly the question the flag was asking.
#[test]
fn an_equals_spelled_flag_is_a_usage_error_not_a_selection() {
    let corpus = Corpus::new("equals-flag");
    // An argument shaped like a flag is an unknown flag, and the error names it.
    for argv in [
        &["render", "--out=spec.md"][..],
        &["render", "--check=spec.md"][..],
        &["render", "--bogus=1"][..],
        &["render", "-x"][..],
    ] {
        let out = corpus.out(argv);
        assert_eq!(
            out.code, 2,
            "`{argv:?}` is usage, not a context pair:\n{}\n{}",
            out.stdout, out.stderr
        );
        assert!(
            out.stderr.contains("unrecognized flag"),
            "the error names the offending argument:\n{}",
            out.stderr
        );
    }
    // A value that is shaped like a flag is a missing value, not a file to write.
    for argv in [
        &["render", "--out", "-x"][..],
        &["render", "--check", "-check"][..],
    ] {
        let out = corpus.out(argv);
        assert_eq!(out.code, 2, "`{argv:?}` is usage:\n{}", out.stderr);
        assert!(
            out.stderr.contains("needs a path"),
            "the error says the value is the problem:\n{}",
            out.stderr
        );
    }
    assert!(
        !corpus.path().join("spec.md").exists() && !corpus.path().join("-x").exists(),
        "a refused `--out` writes nothing"
    );
}

/// R1: `--out` to a path that cannot be written is UNVERIFIABLE — not a silent success.
#[test]
fn out_to_an_unwritable_path_is_unverifiable() {
    let corpus = Corpus::new("unwritable");
    let out = corpus.out(&["render", "--out", "no-such-dir/spec.md"]);
    assert_eq!(
        out.code, 3,
        "an unwritable target is UNVERIFIABLE:\n{}\n{}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains("cannot write"),
        "the message names the failure:\n{}",
        out.stderr
    );
}

/// R2: the header's selection is the *canonical* context — two orderings of one context are
/// one header and one artefact. Keeping the argument order in the header would redden this.
#[test]
fn two_argument_orderings_produce_one_header_and_one_artefact() {
    let corpus = Corpus::new("ordering");
    let a = corpus.out(&["render", "--out", "a.md", "field=value", "runs=25"]);
    let b = corpus.out(&["render", "--out", "b.md", "runs=25", "field=value"]);
    assert_eq!(a.code, 0, "{}", a.stderr);
    assert_eq!(b.code, 0, "{}", b.stderr);
    assert_eq!(
        corpus.read("a.md"),
        corpus.read("b.md"),
        "argument order is not part of the projection"
    );
    assert!(
        header_of(&corpus.read("a.md")).contains("selection=field=value runs=25"),
        "keys are sorted: {}",
        header_of(&corpus.read("a.md"))
    );

    for path in ["a.md", "b.md"] {
        let checked = corpus.out(&["render", "--check", path]);
        assert_eq!(checked.code, 0, "{path}: {}", checked.stderr);
        // The same selection, spelled in either order, still matches the header.
        let reordered = corpus.out(&["render", "--check", path, "runs=25", "field=value"]);
        assert_eq!(reordered.code, 0, "{path}: {}", reordered.stderr);
    }
    // A different selection from the invocation is a different question than the header's:
    // UNVERIFIABLE, never a silent fallback to the header (or to the invocation).
    let different = corpus.out(&["render", "--check", "a.md", "field=other"]);
    assert_eq!(different.code, 3, "{}", different.stderr);
    assert!(
        different.stderr.contains("selection"),
        "{}",
        different.stderr
    );
}

/// R1/R2: a projection built under a selection is checked under **that** selection, and a
/// hand-edited header selection is caught by the body rather than trusted.
#[test]
fn editing_only_the_headers_selection_is_stale() {
    let corpus = Corpus::new("header-selection");
    assert_eq!(corpus.out(&["render", "--out", "spec.md"]).code, 0);
    let text = corpus.read("spec.md");

    // A header field that leaves the body untouched must still be part of the compared artefact.
    // `selection=` changes the body's own `# Selection:` line too, so on its own it does not
    // prove the header is compared: an implementation that compared `fresh_view` with the stored
    // body (while honouring the header's selection) would pass. `claims=` and `identity=` do not
    // move the body at all — each hand-edit must be reported as stale (1).
    let claims_edit = text.replacen("claims=2", "claims=3", 1);
    assert_ne!(claims_edit, text);
    corpus.write("spec.md", &claims_edit);
    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 1,
        "an edited `claims=` is a hand-edit, not a current projection:\n{}\n{}",
        checked.stdout, checked.stderr
    );

    let (head, tail) = text
        .split_once("identity=sha256:")
        .expect("the header carries the digest");
    let mut flipped: Vec<char> = tail.chars().take(64).collect();
    assert_eq!(flipped.len(), 64);
    flipped[0] = if flipped[0] == '0' { '1' } else { '0' };
    let identity_edit = format!(
        "{head}identity=sha256:{}{}",
        flipped.into_iter().collect::<String>(),
        &tail[64..]
    );
    assert_ne!(identity_edit, text);
    corpus.write("spec.md", &identity_edit);
    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 1,
        "an edited `identity=` is a hand-edit, not a current projection:\n{}\n{}",
        checked.stdout, checked.stderr
    );

    // The selection edit: the header names one context while the body was written under `none`.
    let edited = text.replacen("selection=none", "selection=field=value", 1);
    corpus.write("spec.md", &edited);
    let checked = corpus.out(&["render", "--check", "spec.md"]);
    assert_eq!(
        checked.code, 1,
        "the header is not the oracle: the body written under `none` is:\n{}\n{}",
        checked.stdout, checked.stderr
    );
}

/// R1: `trace` is a projection too — same two flags, same classes.
#[test]
fn trace_has_the_same_projection_flags() {
    let corpus = Corpus::new("trace");
    let out = corpus.out(&["trace", "--out", "trace.md"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = corpus.read("trace.md");
    assert!(
        header_of(&text).contains("generator=req trace"),
        "the trace projection names its generator: {}",
        header_of(&text)
    );
    assert!(
        text.contains("# Traceability:"),
        "the trace body is the trace view:\n{text}"
    );
    assert_eq!(corpus.out(&["trace", "--check", "trace.md"]).code, 0);
}

/// Two commands, two projections: checking a `trace` artefact with `render` would compare
/// two different things, so it is UNVERIFIABLE with the command that can judge it named.
#[test]
fn another_generators_projection_cannot_be_judged() {
    let corpus = Corpus::new("other-generator");
    assert_eq!(corpus.out(&["trace", "--out", "trace.md"]).code, 0);
    let checked = corpus.out(&["render", "--check", "trace.md"]);
    assert_eq!(
        checked.code, 3,
        "a trace projection is not a render projection:\n{}\n{}",
        checked.stdout, checked.stderr
    );
    assert!(
        checked.stderr.contains("trace --check"),
        "the message names the command that can judge it:\n{}",
        checked.stderr
    );
}

/// R3: an unreadable path cannot be judged — never "current" and never "stale".
#[test]
fn checking_a_missing_file_is_unverifiable() {
    let corpus = Corpus::new("missing");
    let checked = corpus.out(&["render", "--check", "no-such-file.md"]);
    assert_eq!(
        checked.code, 3,
        "a missing projection cannot be judged:\n{}\n{}",
        checked.stdout, checked.stderr
    );
}

/// R5: the pages are claims too, and this pack changes them.
#[test]
fn the_help_pages_carry_the_projection_flags_and_classes() {
    let empty = NoCorpus::new("help");
    for command in ["render", "trace"] {
        let page = run_at(empty.path(), &[command, "--help"]);
        assert_eq!(page.code, 0, "{}", page.stderr);
        for needle in ["--out <path>", "--check <path>", "stale"] {
            assert!(
                page.stdout.contains(needle),
                "`req {command} --help` must document `{needle}`:\n{}",
                page.stdout
            );
        }
        // The clause, not a bare `1 `: a substring that loose survives deleting the class.
        let normalized = page.stdout.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            normalized.contains("1 a committed projection is stale"),
            "the page carries the `--check` exit 1 class:\n{}",
            page.stdout
        );
    }
}

/// R4: the gate actually runs the check for every pinned path. The oracle is `scripts/check.sh
/// plan` — the same one `tests/baseline.rs` uses for the release-gate wiring — so deleting the
/// step from the default run reddens here even though `render --check` itself still works.
#[cfg(unix)]
#[test]
fn the_gate_plans_the_committed_projection_check() {
    // HOME is pointed at the toolchain the gate resolves, the same way every other gate test in
    // this suite does it (`tests/common::point_home_at_toolchain`): with the ambient HOME the
    // script exits 2 on a checkout whose toolchain lives in `.tools/home`, and the failure would
    // look like a missing projection step.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new("bash");
    command
        .arg("scripts/check.sh")
        .arg("plan")
        .current_dir(root)
        .env_remove("REQ_ROOT");
    common::point_home_at_toolchain(&mut command);
    let planned = command.output().expect("runs the gate's plan");
    let stdout = String::from_utf8_lossy(&planned.stdout);
    assert_eq!(
        planned.status.code(),
        Some(3),
        "a plan executes nothing, so it is UNVERIFIABLE and never a pass:\n{stdout}"
    );
    assert!(
        stdout.contains("render --check docs/spec.md"),
        "the default gate must check the pinned projection:\n{stdout}"
    );
    assert!(
        stdout.contains("projections planned: 1 (nothing executed)"),
        "the step reports what it would check, and says nothing ran:\n{stdout}"
    );
}

/// A throwaway root holding a copy of `scripts/check.sh`, the repository's `.req/variants` (so the
/// release-gate step is not the one failing), and a **fake toolchain whose `cargo` exits 0** — the
/// same fixture shape `tests/baseline.rs` uses. Every `cargo …` step "passes", so whatever flips
/// the banner is the branch under test; an invocation carrying `--format json` prints an empty
/// document, because the gate's JSON smoke step parses one rather than echoing it. Without this,
/// deleting either projection FAIL branch would leave the suite green: the release-gate lesson,
/// applied to R4.
#[cfg(unix)]
struct ScriptFixture(PathBuf);

#[cfg(unix)]
impl ScriptFixture {
    fn new(tag: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = unique_dir(tag);
        fs::create_dir_all(dir.join("scripts")).expect("creates scripts/");
        fs::copy(
            manifest.join("scripts/check.sh"),
            dir.join("scripts/check.sh"),
        )
        .expect("copies check.sh");
        fs::create_dir_all(dir.join(".req")).expect("creates .req/");
        fs::copy(manifest.join(".req/variants"), dir.join(".req/variants"))
            .expect("copies .req/variants");
        let bin = dir.join("home/.rustup/toolchains/fake/bin");
        fs::create_dir_all(&bin).expect("creates the fake toolchain");
        // Every cargo step passes, except the projection check: its verdict is read from
        // `PROJECTION_STUB_EXIT`, so the fixture can drive the step's wiring for 0/1/3 without a
        // second `cargo` invocation (the real binary is exercised by the test beside this one).
        let stub = "#!/bin/sh\n\
                    case \"$*\" in\n\
                      *\" render --check \"*) exit \"${PROJECTION_STUB_EXIT:-0}\" ;;\n\
                      *\" --format json\") printf '{}\\n' ;;\n\
                    esac\n\
                    exit 0\n";
        fs::write(bin.join("cargo"), stub).expect("writes the stub cargo");
        fs::set_permissions(bin.join("cargo"), fs::Permissions::from_mode(0o755))
            .expect("makes the stub executable");
        ScriptFixture(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("creates the fixture directory");
        }
        fs::write(path, text).expect("writes the fixture file");
    }

    /// Run the copied script to its banner, with HOME and PATH the fixture's own: the fake
    /// toolchain is what it resolves, and nothing ambient leaks in.
    fn banner(&self) -> std::process::Output {
        self.banner_with(None)
    }

    /// The same, with the stub `cargo`'s projection verdict set to `exit_code` (0 when `None`).
    fn banner_with(&self, projection_exit: Option<&str>) -> std::process::Output {
        let bin = self.path().join("home/.rustup/toolchains/fake/bin");
        let mut command = Command::new("bash");
        command
            .arg(self.path().join("scripts/check.sh"))
            .current_dir(self.path())
            .env("HOME", self.path().join("home"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
            )
            .env_remove("REQ_ROOT");
        if let Some(code) = projection_exit {
            command.env("PROJECTION_STUB_EXIT", code);
        }
        command.output().expect("runs the copied scripts/check.sh")
    }
}

#[cfg(unix)]
impl Drop for ScriptFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn banner_text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// R4: a listed projection that no longer exists is a FAIL, and the healthy direction is a PASS
/// with the count — so the failure is the branch's doing, not the fixture's.
#[cfg(unix)]
#[test]
fn a_missing_listed_projection_fails_the_gate() {
    let fixture = ScriptFixture::new("projection-missing");
    fixture.write(".req/projections", "docs/spec.md\n");
    let missing = fixture.banner();
    let text = banner_text(&missing);
    assert!(
        text.contains("FAIL  .req/projections lists a projection that is missing: docs/spec.md"),
        "a listed projection that vanished is silent decay, never skipped:\n{text}"
    );
    assert_eq!(missing.status.code(), Some(1), "the gate fails:\n{text}");
    assert!(text.contains("GATE: FAIL"), "{text}");
    // The reason is named once: the missing-file branch already reported it.
    assert!(
        !text.contains("pins no projection"),
        "the empty-list reason is not also blamed for a missing path:\n{text}"
    );

    fixture.write("docs/spec.md", "");
    let healthy = fixture.banner();
    let healthy_text = banner_text(&healthy);
    assert!(
        healthy_text.contains("projections checked: 1"),
        "the healthy list reports its count:\n{healthy_text}"
    );
    assert!(healthy_text.contains("GATE: PASS"), "{healthy_text}");
    assert_eq!(healthy.status.code(), Some(0), "{healthy_text}");

    // The wiring R4 is actually about: a projection that is stale (1) or one the check cannot
    // judge (3) must make the *gate* fail, not merely the command. The stub `cargo` answers the
    // configured verdict for the `render --check` invocation only.
    for code in ["1", "3"] {
        let failed = fixture.banner_with(Some(code));
        let failed_text = banner_text(&failed);
        assert!(
            failed_text.contains(&format!("render --check docs/spec.md (exit {code})")),
            "the step must mark the check's exit {code} as a failure:\n{failed_text}"
        );
        assert!(
            failed_text.contains("GATE: FAIL"),
            "exit {code} from a pinned projection makes the gate red:\n{failed_text}"
        );
        assert_eq!(failed.status.code(), Some(1), "{failed_text}");
    }
}

/// R4: a pinned list that checks nothing is a FAIL, never a pass, and a line with more than one
/// word is named rather than half-checked.
#[cfg(unix)]
#[test]
fn a_projection_list_that_checks_nothing_fails_the_gate() {
    let fixture = ScriptFixture::new("projection-empty");
    fixture.write(".req/projections", "# nothing pinned yet\n\n");
    let empty = fixture.banner();
    let text = banner_text(&empty);
    assert!(
        text.contains("FAIL  .req/projections pins no projection"),
        "a comment-only list examined nothing, and that is not a pass:\n{text}"
    );
    assert!(text.contains("GATE: FAIL"), "{text}");
    assert_eq!(empty.status.code(), Some(1), "{text}");

    fixture.write(".req/projections", "docs/spec.md docs/other.md\n");
    let two_words = fixture.banner();
    let two_words_text = banner_text(&two_words);
    assert!(
        two_words_text.contains("names more than one word"),
        "a path with whitespace is named, not truncated to its first word:\n{two_words_text}"
    );
    assert!(two_words_text.contains("GATE: FAIL"), "{two_words_text}");
    assert_eq!(two_words.status.code(), Some(1), "{two_words_text}");
}

/// R4: a pinned list that is **present but unusable** is not the tolerated "absent" state. A
/// dangling symlink is the portable way to be in that state (a mode-000 file depends on the
/// runner's uid): `-e` cannot see through it and `-r` is false, so a `[ -r ]`-only test would
/// report "nothing pinned" about a file that is right there and leave the gate green.
#[cfg(unix)]
#[test]
fn a_pinned_list_that_cannot_be_read_fails_the_gate() {
    let fixture = ScriptFixture::new("projection-dangling");
    let path = fixture.path().join(".req/projections");
    std::os::unix::fs::symlink("no-such-target", &path).expect("creates the dangling link");
    let out = fixture.banner();
    let text = banner_text(&out);
    assert!(
        text.contains("FAIL  .req/projections is present but is not a readable regular file"),
        "a pinned list that cannot be examined is a failure, not 'nothing pinned':\n{text}"
    );
    assert!(text.contains("GATE: FAIL"), "{text}");
    assert_eq!(out.status.code(), Some(1), "{text}");

    // A directory is the other shape of "present but unusable": `-r` alone accepts it, the loop's
    // redirection then fails at the shell, and the step would blame "pins no projection".
    let fixture = ScriptFixture::new("projection-directory");
    std::fs::create_dir_all(fixture.path().join(".req/projections")).expect("makes a directory");
    let out = fixture.banner();
    let text = banner_text(&out);
    assert!(
        text.contains("FAIL  .req/projections is present but is not a readable regular file"),
        "a directory is not a pinned list:\n{text}"
    );
    assert!(text.contains("GATE: FAIL"), "{text}");
    assert_eq!(out.status.code(), Some(1), "{text}");
}

/// R4: this repository's own adoption — `docs/spec.md` is committed and current. A corpus
/// change that was not regenerated reddens here as well as in the gate, so the first live
/// user of the check is pinned by a test and not only by CI wiring.
#[test]
fn this_repositories_committed_projection_is_current() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        req::root::is_corpus_root(root),
        "the test runs in this repository's checkout: {}",
        root.display()
    );
    let checked = run_at(root, &["render", "--check", "docs/spec.md"]);
    assert_eq!(
        checked.code, 0,
        "docs/spec.md is generated from this corpus; regenerate with \
         `cargo run -q -- render --out docs/spec.md`, never edit it:\n{}\n{}",
        checked.stdout, checked.stderr
    );
}
