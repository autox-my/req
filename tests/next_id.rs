//! Issue #21: `req next-id` hands out the next free claim ids, and can reserve a block, so claims
//! authored in parallel worktrees do not collide on `REQ-NNNN`.
//!
//! The command reads **file names**, not parses: `claims/<id>.yml` is the id (`docs/claim-format.md`,
//! *Filename = claim_id*), and an author who needs the next number must get one even while a
//! sibling claim is half-written or fails a pin. It is read-only unless `--reserve` is given.
//!
//! Rules pinned here (and on `req next-id --help`):
//!   * highest existing number + 1; a gap left by a retired or never-used number is **not** refilled;
//!   * `.req/reserved` (`REQ-0010..REQ-0019`, one range or id per line, `#` comments) counts as taken,
//!     so two branches that each reserve a block cannot be handed the same ids;
//!   * `--reserve` appends the allocated block to that file: two branches that reserve at the same
//!     time both edit its end, and the merge conflict is the signal;
//!   * a reservation file that cannot be read cannot be honoured: UNVERIFIABLE (exit 3), never "no
//!     reservations".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn req(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(
        std::env::var_os("CARGO_BIN_EXE_req")
            .map(PathBuf::from)
            .expect("integration tests run with CARGO_BIN_EXE_req set"),
    )
    .args(args)
    .current_dir(cwd)
    .env_remove("REQ_ROOT")
    .output()
    .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

static SEQ: AtomicU64 = AtomicU64::new(0);

struct Corpus(PathBuf);

impl Corpus {
    /// A corpus with **no** claims (the starter is removed): `claims/` exists and is empty.
    fn empty(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "req-next-id-{name}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the fixture directory");
        let (code, out, err) = req(&dir, &["init"]);
        assert_eq!(code, 0, "init: {out}{err}");
        fs::remove_file(dir.join("claims/REQ-0001.yml")).expect("removes the starter claim");
        Corpus(dir)
    }

    /// A file in `claims/`; its content is irrelevant to the command (only the name counts).
    fn claim_file(&self, file: &str) {
        fs::write(self.0.join("claims").join(file), "not even yaml: [\n").expect("writes");
    }

    fn reserved(&self, text: &str) {
        fs::create_dir_all(self.0.join(".req")).expect("creates .req");
        fs::write(self.0.join(".req/reserved"), text).expect("writes the reservation file");
    }

    fn reserved_text(&self) -> Option<String> {
        fs::read_to_string(self.0.join(".req/reserved")).ok()
    }

    fn next(&self, args: &[&str]) -> (i32, String, String) {
        let mut argv = vec!["next-id"];
        argv.extend_from_slice(args);
        req(&self.0, &argv)
    }

    fn ids(&self, args: &[&str]) -> Vec<String> {
        let (code, out, err) = self.next(args);
        assert_eq!(code, 0, "{out}{err}");
        out.lines().map(str::to_string).collect()
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn an_empty_corpus_starts_at_one_and_the_starter_claim_counts() {
    let c = Corpus::empty("start");
    assert_eq!(c.ids(&[]), ["REQ-0001"]);
    c.claim_file("REQ-0001.yml");
    assert_eq!(c.ids(&[]), ["REQ-0002"]);
    assert_eq!(
        c.ids(&["--count", "3"]),
        ["REQ-0002", "REQ-0003", "REQ-0004"]
    );
}

#[test]
fn it_is_the_highest_plus_one_and_never_refills_a_gap() {
    let c = Corpus::empty("gaps");
    for f in ["REQ-0001.yml", "REQ-0002.yml", "REQ-0007.yml"] {
        c.claim_file(f);
    }
    assert_eq!(c.ids(&["--count", "2"]), ["REQ-0008", "REQ-0009"]);
}

#[test]
fn it_reads_names_not_contents_so_a_broken_claim_still_holds_its_number() {
    let c = Corpus::empty("broken");
    c.claim_file("REQ-0004.yml"); // the content is not YAML at all
    c.claim_file("REQ-0005.yaml"); // the other extension
    assert_eq!(c.ids(&[]), ["REQ-0006"]);
}

#[test]
fn files_that_are_not_claims_hold_no_number() {
    let c = Corpus::empty("noise");
    c.claim_file("REQ-0003.yml");
    fs::write(c.0.join("claims/REQ-0900.txt"), "").expect("writes");
    fs::write(c.0.join("claims/notes.yml"), "").expect("writes");
    fs::write(c.0.join("claims/REQ-0800.yml.bak"), "").expect("writes");
    fs::create_dir_all(c.0.join("claims/REQ-0700.yml")).expect("a directory is not a claim");
    assert_eq!(c.ids(&[]), ["REQ-0004"]);
}

#[test]
fn the_number_grows_a_digit_instead_of_wrapping_and_keeps_a_wider_width() {
    let c = Corpus::empty("width");
    c.claim_file("REQ-9999.yml");
    assert_eq!(c.ids(&[]), ["REQ-10000"]);
    let c = Corpus::empty("wide");
    c.claim_file("REQ-00099.yml");
    assert_eq!(c.ids(&[]), ["REQ-00100"], "five digits stay five digits");
}

#[test]
fn the_prefix_is_the_corpus_own_and_a_choice_is_never_guessed() {
    let c = Corpus::empty("prefix");
    c.claim_file("FOO-0003.yml");
    assert_eq!(
        c.ids(&[]),
        ["FOO-0004"],
        "one prefix in the corpus: that one"
    );

    c.claim_file("REQ-0010.yml");
    assert_eq!(c.ids(&[]), ["REQ-0011"], "REQ wins when several exist");
    assert_eq!(c.ids(&["--prefix", "FOO"]), ["FOO-0004"]);

    let d = Corpus::empty("two-prefixes");
    d.claim_file("AAA-0001.yml");
    d.claim_file("BBB-0002.yml");
    let (code, out, err) = d.next(&[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        err.contains("--prefix") && err.contains("AAA") && err.contains("BBB"),
        "{err}"
    );
    assert_eq!(d.ids(&["--prefix", "BBB"]), ["BBB-0003"]);
    assert_eq!(d.ids(&["--prefix", "NEW"]), ["NEW-0001"]);
}

#[test]
fn a_reservation_counts_as_taken_and_comments_are_ignored() {
    let c = Corpus::empty("reserved");
    c.claim_file("REQ-0003.yml");
    c.reserved(
        "# a block for the branch that does imaging\nREQ-0010..REQ-0019  # imaging\n\nREQ-0025\n",
    );
    assert_eq!(c.ids(&["--count", "2"]), ["REQ-0026", "REQ-0027"]);
}

#[test]
fn a_reservation_of_another_prefix_does_not_move_this_one() {
    let c = Corpus::empty("other-prefix");
    c.claim_file("REQ-0003.yml");
    c.reserved("FOO-0100..FOO-0199\n");
    assert_eq!(c.ids(&[]), ["REQ-0004"]);
}

#[test]
fn it_is_read_only_unless_reserve_is_given() {
    let c = Corpus::empty("readonly");
    c.claim_file("REQ-0001.yml");
    let before = fs::read_dir(&c.0).expect("lists").count();
    c.ids(&["--count", "5"]);
    assert!(c.reserved_text().is_none(), "no reservation file appeared");
    assert!(
        !c.0.join(".req").exists() || fs::read_dir(c.0.join(".req")).expect("lists").count() == 0
    );
    assert_eq!(fs::read_dir(&c.0).expect("lists").count(), before);
}

#[test]
fn reserve_prints_the_block_and_appends_it_so_the_next_call_starts_after_it() {
    let c = Corpus::empty("reserve");
    c.claim_file("REQ-0003.yml");
    assert_eq!(
        c.ids(&["--count", "7", "--reserve"]),
        [
            "REQ-0004", "REQ-0005", "REQ-0006", "REQ-0007", "REQ-0008", "REQ-0009", "REQ-0010"
        ]
    );
    assert_eq!(c.reserved_text().as_deref(), Some("REQ-0004..REQ-0010\n"));
    // A second author asking now is handed the next block, not an overlapping one.
    assert_eq!(
        c.ids(&["--count", "2", "--reserve"]),
        ["REQ-0011", "REQ-0012"]
    );
    assert_eq!(
        c.reserved_text().as_deref(),
        Some("REQ-0004..REQ-0010\nREQ-0011..REQ-0012\n")
    );
    // One id is written as an id, not as a one-element range.
    assert_eq!(c.ids(&["--reserve"]), ["REQ-0013"]);
    assert!(c.reserved_text().expect("file").ends_with("\nREQ-0013\n"));
    // And it appends to a file that does not end in a newline, never gluing two lines.
    fs::write(c.0.join(".req/reserved"), "REQ-0001..REQ-0020").expect("rewrites");
    assert_eq!(c.ids(&["--reserve"]), ["REQ-0021"]);
    assert_eq!(
        c.reserved_text().as_deref(),
        Some("REQ-0001..REQ-0020\nREQ-0021\n")
    );
}

#[test]
fn a_reservation_file_that_cannot_be_understood_is_unverifiable_and_writes_nothing() {
    for bad in [
        "REQ-0010..REQ-0005\n", // backwards
        "REQ-0010..FOO-0020\n", // two prefixes
        "ten to twenty\n",
        "REQ-0010..\n",
        "REQ-0010 REQ-0011\n",
    ] {
        let c = Corpus::empty("bad-reserved");
        c.claim_file("REQ-0003.yml");
        c.reserved(bad);
        let (code, out, err) = c.next(&["--reserve"]);
        assert_eq!(code, 3, "{bad:?}: {out}{err}");
        assert!(
            err.contains(".req/reserved") && err.contains("line 1"),
            "{bad:?}: {err}"
        );
        assert_eq!(
            out, "",
            "no id is printed when the answer cannot be trusted"
        );
        assert_eq!(c.reserved_text().as_deref(), Some(bad), "left untouched");
    }
}

#[test]
fn a_corpus_without_a_claims_directory_cannot_answer() {
    let c = Corpus::empty("noclaims");
    fs::remove_dir_all(c.0.join("claims")).expect("removes claims/");
    fs::create_dir_all(c.0.join("claims-not")).expect("keeps a directory");
    // With claims/ gone there is no corpus root to resolve at all: the answer is UNVERIFIABLE.
    let (code, out, err) = c.next(&[]);
    assert_eq!(code, 3, "{out}{err}");
}

#[test]
fn usage_errors_are_exit_2_and_print_no_id() {
    let c = Corpus::empty("usage");
    for args in [
        &["--count", "0"][..],
        &["--count", "x"],
        &["--count", "-1"],
        &["--count", "10001"],
        &["--count"],
        &["--prefix"],
        &["--prefix", "req"],
        &["--prefix", "A B"],
        &["--bogus"],
        &["extra"],
        &["--count", "2", "--count", "3"],
    ] {
        let (code, out, err) = c.next(args);
        assert_eq!(code, 2, "{args:?}: {out}{err}");
        assert_eq!(out, "", "{args:?}");
        assert!(err.contains("usage: req next-id"), "{args:?}: {err}");
    }
}

#[test]
fn the_help_page_is_answered_without_a_corpus_and_states_the_rules() {
    let dir = std::env::temp_dir().join(format!("req-next-id-help-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("creates");
    let (code, out, err) = req(&dir, &["next-id", "--help"]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "{err}");
    for needle in [
        "usage: req next-id",
        "--count",
        "--reserve",
        ".req/reserved",
        "gap",
        "exit:",
    ] {
        assert!(out.contains(needle), "the page lacks {needle:?}:\n{out}");
    }
}
