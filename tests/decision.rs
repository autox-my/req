//! The decision log: the chain, its three outcomes, and the boundaries between them.
//!
//! Two seams. The first is the library (`verify_chain`, `fill`, `canonical_bytes`) plus the CLI
//! `req decision` over a corpus root, at the bottom of this file's `cli` module: what the log
//! *is*. The second is `scripts/check.sh decision`, after them: that the **gate** re-verifies the
//! log on every run, so a record edited after it was recorded fails in the gate rather than in an
//! auditor's terminal.

mod common;

use req::decision::{
    Chain, Decision, GENESIS, canonical_bytes, compute_hash, fill, head, upsert_line, verify_chain,
};

fn decision(id: &str, address: &str) -> Decision {
    Decision {
        id: id.to_string(),
        kind: "accept".to_string(),
        claim_id: "REQ-0001".to_string(),
        address: address.to_string(),
        actor: "fdx".to_string(),
        role: "maintainer".to_string(),
        date: "2026-10-08".to_string(),
        note: "rejected: leaving the wording unstated".to_string(),
        prev_hash: None,
        hash: None,
    }
}

fn filled(count: usize) -> Vec<Decision> {
    let mut log: Vec<Decision> = (1..=count)
        .map(|i| decision(&format!("DEC-{i:04}"), &format!("{i:064x}")))
        .collect();
    fill(&mut log);
    log
}

/// A log with nothing in it verifies nothing. Reporting VALID here would be a check passing on no
/// evidence — the failure mode this project is organised against.
#[test]
fn an_empty_log_verifies_nothing() {
    let chain = verify_chain(&[]);
    assert!(matches!(chain, Chain::Unverifiable(_)), "{chain:?}");
    assert_eq!(chain.exit_code(), 3);
    assert!(chain.reasons().join(" ").contains("empty"));
}

#[test]
fn a_filled_chain_is_valid() {
    let log = filled(3);
    let chain = verify_chain(&log);
    assert_eq!(chain, Chain::Valid, "{:?}", chain.reasons());
    assert_eq!(chain.exit_code(), 0);
}

#[test]
fn an_unfilled_entry_is_unverifiable_never_valid() {
    let mut log = filled(3);
    log[1].hash = None;
    let chain = verify_chain(&log);
    assert!(matches!(chain, Chain::Unverifiable(_)), "{chain:?}");
    assert!(chain.reasons().join(" ").contains("DEC-0002"));
}

/// The property the log exists for: a recorded decision edited afterwards is caught, not blessed.
#[test]
fn editing_a_recorded_decision_is_tampering() {
    let mut log = filled(3);
    log[1].address = "edited-after-recording".to_string();
    let chain = verify_chain(&log);
    assert!(matches!(chain, Chain::Tampered(_)), "{chain:?}");
    assert!(chain.reasons().join(" ").contains("does not recompute"));
    assert_eq!(chain.exit_code(), 1);
}

/// Order is part of the record: each entry commits to the one before it.
#[test]
fn moving_an_entry_is_tampering() {
    let mut log = filled(3);
    log.swap(1, 2);
    let chain = verify_chain(&log);
    assert!(matches!(chain, Chain::Tampered(_)), "{chain:?}");
    assert!(chain.reasons().join(" ").contains("chains from"));
}

/// A truncated history and a forged first entry are indistinguishable from the file set alone, so
/// the answer is that the question cannot be settled — deliberately **not** TAMPERED.
#[test]
fn a_log_that_does_not_begin_at_genesis_is_unverifiable() {
    let log = filled(3);
    let truncated: Vec<Decision> = log[1..].to_vec();
    let chain = verify_chain(&truncated);
    assert!(matches!(chain, Chain::Unverifiable(_)), "{chain:?}");
    assert!(chain.reasons().join(" ").contains("genesis"));
    assert_eq!(chain.exit_code(), 3);
}

/// `head` answers `None` for a log that attests nothing: an empty log, or one whose **last**
/// record carries no hash. Its single production caller (Baseline emission) reaches it only
/// after `Chain::Valid`, so these arms are exercised through the public API alone — and a `Some`
/// returned here would attest an authority the log cannot supply, which is the refusal emission
/// exists to make.
#[test]
fn head_is_none_for_a_log_that_attests_nothing() {
    assert_eq!(head(&[]), None, "an empty log has no head");

    // The head is the *last* record: a filled earlier record does not rescue an unfilled last one.
    let mut unfilled_last = filled(2);
    unfilled_last[1].hash = None;
    assert_eq!(head(&unfilled_last), None);

    // ...and where it answers, it answers with the stored field and the record count.
    let log = filled(2);
    let answered = head(&log).expect("a filled log has a head");
    assert_eq!(answered.hash, log[1].hash.clone().expect("filled"));
    assert_eq!(answered.entries, 2);
}

/// Re-filling a reordered log yields a *different* internally consistent log: the point is that its
/// hashes differ from the recorded ones, so the rewrite cannot masquerade as the original.
#[test]
fn position_is_part_of_the_hash() {
    let log = filled(2);
    let before: Vec<String> = log.iter().map(|d| d.hash.clone().unwrap()).collect();
    let mut reordered = log.clone();
    reordered.reverse();
    fill(&mut reordered);
    let after: Vec<String> = reordered.iter().map(|d| d.hash.clone().unwrap()).collect();
    assert_ne!(before, after);
    assert_eq!(verify_chain(&reordered), Chain::Valid);
}

#[test]
fn canonical_bytes_are_stable_and_the_hash_covers_the_road_not_taken() {
    let a = decision("DEC-0001", "abc");
    let b = decision("DEC-0001", "abc");
    assert_eq!(canonical_bytes(&a), canonical_bytes(&b));
    assert!(canonical_bytes(&a).contains("\"address\":\"abc\""));
    // The predecessor is part of the hash, so one decision in two positions differs...
    assert_ne!(
        compute_hash(Some(GENESIS), &a),
        compute_hash(Some("deadbeef"), &a)
    );
    // ...and so is the note: a record that omitted the road not taken would not be auditable.
    let mut other = a.clone();
    other.note = "different".to_string();
    assert_ne!(compute_hash(None, &a), compute_hash(None, &other));
}

/// A derived field is written *into* the record, never over it: comments and the human's note
/// survive the fill, and re-filling updates in place instead of accumulating lines.
#[test]
fn filling_preserves_everything_else_in_the_file() {
    let text = "# a decision a human wrote\nid: \"DEC-0001\"\nkind: \"accept\"\nnote: \"kept\"\n";
    let once = upsert_line(text, "prev_hash", GENESIS);
    let twice = upsert_line(&once, "hash", "abc123");
    assert!(twice.starts_with("# a decision a human wrote\n"));
    assert!(twice.contains("note: \"kept\""));
    assert!(twice.contains(&format!("prev_hash: \"{GENESIS}\"")));
    assert!(twice.contains("hash: \"abc123\""));
    let thrice = upsert_line(&twice, "hash", "def456");
    assert_eq!(
        thrice
            .lines()
            .filter(|line| line.starts_with("hash:"))
            .count(),
        1
    );
    assert!(thrice.contains("hash: \"def456\""));
    assert!(!thrice.contains("abc123"));
}

// ---------------------------------------------------------------------------------------------
// The public seam is the CLI: `req decision --fill` over a corpus root, observed through its exit
// code and the bytes on disk. Judging *before* writing is invisible to the unit tests above,
// because `fill` never touches a file — only the invocation can launder an edit.
// ---------------------------------------------------------------------------------------------

mod cli {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// A fresh corpus root under the build tree Cargo guarantees is writable, removed when the
    /// test ends — including when an assertion panics, which a trailing cleanup call would skip.
    pub struct Fixture(PathBuf);

    impl Fixture {
        pub fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            // A process-local counter carries uniqueness rather than the clock, whose resolution
            // is not one; the pid keeps two concurrent runs apart.
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
                "decision-{tag}-{}-{nanos}-{serial}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            // `--root <dir>` is only accepted when the directory is a corpus root (ADR 0012), so
            // every fixture carries an empty `claims/` beside its `decisions/`.
            std::fs::create_dir_all(dir.join("claims")).expect("creates claims/");
            std::fs::create_dir_all(dir.join("decisions")).expect("creates decisions/");
            Fixture(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        pub fn write(&self, name: &str, body: &str) {
            std::fs::write(self.0.join("decisions").join(name), body).expect("writes a record");
        }

        pub fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.0.join("decisions").join(name)).expect("reads a record")
        }

        pub fn bytes(&self, name: &str) -> Vec<u8> {
            std::fs::read(self.0.join("decisions").join(name)).expect("reads a record's bytes")
        }

        /// One unfilled record, with its own id and a distinct address so two cannot collide.
        pub fn put(&self, name: &str, id: &str, seed: char) {
            self.write(name, &unfilled(id, seed));
        }

        /// `req --root <fixture> decision <args>`, with stdout and stderr in one string: a refusal
        /// is an error, and the question is what the invocation said either way.
        pub fn decision(&self, args: &[&str]) -> Outcome {
            let output = Command::new(req_bin())
                .arg("--root")
                .arg(self.path())
                .arg("decision")
                .args(args)
                .output()
                .expect("runs the compiled binary");
            Outcome {
                code: output.status.code().unwrap_or(-1),
                text: format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub struct Outcome {
        pub code: i32,
        pub text: String,
    }

    /// The absolute path of the compiled binary, so the tests do not depend on `$PATH`.
    pub fn req_bin() -> PathBuf {
        std::env::var_os("CARGO_BIN_EXE_req")
            .map(PathBuf::from)
            .expect("integration tests run with CARGO_BIN_EXE_req set")
    }

    /// An **unfilled** decision record. No derived field is hand-written here: a hand-written hash
    /// is not evidence, and `--fill` is the only thing that may write one.
    pub fn unfilled(id: &str, seed: char) -> String {
        format!(
            "# a decision a human wrote\nid: \"{id}\"\nkind: \"accept\"\nclaim_id: \"REQ-0001\"\n\
             address: \"{}\"\nactor: \"fdx\"\nrole: \"maintainer\"\ndate: \"2026-10-08\"\n\
             note: \"rejected: leaving the wording unstated\"\n",
            seed.to_string().repeat(64)
        )
    }

    /// Strip the derived lines back out, producing a deliberately unfilled record again without
    /// hand-writing a replacement value.
    pub fn strip_derived(text: &str) -> String {
        let mut out: String = text
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("hash:") && !trimmed.starts_with("prev_hash:")
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out
    }
}

/// The property the log exists for, seen through the CLI: `--fill` must not be the one path that
/// turns a definite finding into a clean bill of health. Recomputing over an edited record would
/// write the edit's own hash over the evidence, so the refusal (exit 1) and the untouched bytes
/// *are* the test.
#[test]
fn filling_refuses_to_launder_an_edited_record() {
    let fx = cli::Fixture::new("launder");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    // Setup only: derive the chain, then confirm validity through `--verify` — never through the
    // exit code of the behaviour under test.
    let _ = fx.decision(&["--fill"]);
    let clean = fx.decision(&["--verify"]);
    assert_eq!(
        clean.code, 0,
        "--verify of the freshly filled log:\n{}",
        clean.text
    );

    // Edit the *content* and leave the stored hash standing: the edit a human makes, and exactly
    // the record the chain is for.
    let edited = fx.read("DEC-0002.yml").replace("\"accept\"", "\"reject\"");
    assert!(edited.contains("\"reject\""), "the fixture edit applied");
    fx.write("DEC-0002.yml", &edited);
    let before = fx.bytes("DEC-0002.yml");

    let filled = fx.decision(&["--fill"]);
    assert_eq!(
        filled.code, 1,
        "--fill must refuse to launder an edit --verify calls TAMPERED:\n{}",
        filled.text
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before,
        "the refusal left the record byte-identical"
    );
    assert!(
        !filled.text.contains("filled DEC-0002"),
        "nothing was filled, so nothing may be announced:\n{}",
        filled.text
    );
    // The positive pin for the three `!contains("does not recompute")` guards: this *is* the
    // content-edit case, so the phrase must be there for their absence elsewhere to mean anything.
    assert!(
        filled.text.contains("does not recompute"),
        "the content edit is reported as a hash that no longer recomputes:\n{}",
        filled.text
    );

    let after = fx.decision(&["--verify"]);
    assert_eq!(
        after.code, 1,
        "--verify still reports the edit after the attempted fill:\n{}",
        after.text
    );
    assert!(after.text.contains("TAMPERED"), "{}", after.text);
}

/// An unfilled record in an otherwise clean log is filled, and the invocation's own verdict
/// describes the log as it now stands on disk — VALID, exit 0 — rather than the stale records it
/// read on the way in.
#[test]
fn an_unfilled_record_is_filled_and_the_fill_reports_valid() {
    let fx = cli::Fixture::new("fills");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');

    let filled = fx.decision(&["--fill"]);
    assert_eq!(filled.code, 0, "{}", filled.text);
    assert!(filled.text.contains("filled DEC-0001"), "{}", filled.text);
    assert!(filled.text.contains("filled DEC-0002"), "{}", filled.text);
    assert!(filled.text.contains("[VALID]"), "{}", filled.text);
    let first = fx.read("DEC-0001.yml");
    assert!(first.contains("prev_hash: \"req.genesis.v1\""), "{first}");
    assert!(first.contains("hash: \""), "{first}");

    let verify = fx.decision(&["--verify"]);
    assert_eq!(verify.code, 0, "{}", verify.text);
    assert!(verify.text.contains("[VALID]"), "{}", verify.text);
}

/// Order is part of the record. A record filled at position 0 — its declared `prev_hash` is
/// `GENESIS` and its own hash recomputes from that — placed second is a moved entry, and only the
/// position can catch it: `--fill` must refuse rather than chain it in at its new place.
#[test]
fn an_entry_moved_out_of_position_refuses_and_is_left_alone() {
    let first = cli::Fixture::new("moved-first");
    first.put("DEC-0001.yml", "DEC-0001", '1');
    let _ = first.decision(&["--fill"]);
    assert_eq!(
        first.decision(&["--verify"]).code,
        0,
        "the first record is clean"
    );

    let second = cli::Fixture::new("moved-second");
    second.put("DEC-0002.yml", "DEC-0002", '2');
    let _ = second.decision(&["--fill"]);
    assert_eq!(
        second.decision(&["--verify"]).code,
        0,
        "a one-record log fills at position 0"
    );

    let fx = cli::Fixture::new("moved-log");
    fx.write("DEC-0001.yml", &first.read("DEC-0001.yml"));
    fx.write("DEC-0002.yml", &second.read("DEC-0002.yml"));
    let before = fx.bytes("DEC-0002.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 1, "{}", outcome.text);
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before,
        "the refusal left the moved record byte-identical"
    );
    assert!(
        !outcome.text.contains("filled DEC-0002"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("DEC-0002") && outcome.text.contains("chains from"),
        "the finding names the record and its position:\n{}",
        outcome.text
    );
}

/// A stored `prev_hash` that disagrees with its chain position is a definite finding even when the
/// record's own `hash` is still absent: the walk must not accept it so that `--fill` can derive a
/// clean chain over it.
#[test]
fn a_stale_prev_hash_refuses_and_is_left_alone() {
    let anchor = cli::Fixture::new("stale-anchor");
    anchor.put("DEC-0001.yml", "DEC-0001", '1');
    let _ = anchor.decision(&["--fill"]);
    assert_eq!(
        anchor.decision(&["--verify"]).code,
        0,
        "the anchor is a clean first record"
    );

    let fx = cli::Fixture::new("stale-prev");
    fx.write("DEC-0001.yml", &anchor.read("DEC-0001.yml"));
    // Deliberately *wrong*, not a digest under test: all-zero is not the predecessor's hash, so
    // the record declares a position it does not occupy. No `hash` line is written at all.
    fx.write(
        "DEC-0002.yml",
        &format!(
            "{}prev_hash: \"{}\"\n",
            cli::unfilled("DEC-0002", '2'),
            "0".repeat(64)
        ),
    );
    let before = fx.bytes("DEC-0002.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 1, "{}", outcome.text);
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before,
        "the refusal left the record byte-identical"
    );
    assert!(
        !outcome.text.contains("filled DEC-0002"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("DEC-0002") && outcome.text.contains("chains from"),
        "{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("does not recompute"),
        "the chain alone is the finding; no hash was stored to recompute:\n{}",
        outcome.text
    );
}

/// The field being derived here is the `hash`, and the record already declares which anchor it
/// chains from. Deriving the absent hash over the chain's genesis anchor instead of the anchor the
/// record actually carries would make the *proposed* record self-inconsistent and then blame a
/// stored hash that does not exist: the finding is the wrong anchor — an open question at position
/// 0 — not a digest that fails to recompute.
#[test]
fn a_wrong_anchor_with_no_stored_hash_is_unverifiable_not_tampered() {
    let fx = cli::Fixture::new("wrong-anchor");
    fx.write(
        "DEC-0001.yml",
        &format!(
            "{}prev_hash: \"{}\"\n",
            cli::unfilled("DEC-0001", '1'),
            "0".repeat(64)
        ),
    );
    let before = fx.bytes("DEC-0001.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 3, "{}", outcome.text);
    assert!(outcome.text.contains("genesis"), "{}", outcome.text);
    assert!(
        !outcome.text.contains("does not recompute"),
        "no stored hash exists to fail to recompute:\n{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("filled DEC-0001"),
        "{}",
        outcome.text
    );
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before,
        "the refusal left the record byte-identical"
    );
}

/// The `  - reason` lines of a report: one finding each, which is what "exactly one reason" counts.
fn reasons(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("  - "))
        .collect()
}

/// F2a: a wrong anchor at position 0 followed by an unfilled record. The successor is innocent — it
/// did not move — so the judgement must not chain it from a genesis-anchored derivation that ignores
/// the anchor the first record actually carries. The open question at position 0 is the whole
/// finding, exactly as `--verify` classes it.
#[test]
fn a_wrong_anchor_before_an_unfilled_record_is_not_blamed_on_the_successor() {
    let fx = cli::Fixture::new("f2a");
    fx.write(
        "DEC-0001.yml",
        &format!(
            "{}prev_hash: \"{}\"\n",
            cli::unfilled("DEC-0001", '1'),
            "0".repeat(64)
        ),
    );
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 3, "{}", outcome.text);
    assert!(outcome.text.contains("genesis"), "{}", outcome.text);
    assert!(
        !outcome.text.contains("chains from"),
        "no entry was moved:\n{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("DEC-0002"),
        "the unfilled successor is not the finding:\n{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("filled DEC-0001"),
        "{}",
        outcome.text
    );
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before_first,
        "the refusal wrote nothing"
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the refusal wrote nothing"
    );
}

/// F2c: the truncated-log shape — a *self-consistent* record whose anchor is not GENESIS, followed
/// by an unfilled record. `--verify` classes the log open; `--fill` must too, rather than blaming the
/// unfilled record for an anchor that position 0 already declares.
#[test]
fn a_truncated_log_before_an_unfilled_record_is_not_blamed_on_the_successor() {
    let fx = cli::Fixture::new("f2c");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    let _ = fx.decision(&["--fill"]);
    assert_eq!(
        fx.decision(&["--verify"]).code,
        0,
        "the two-record log is clean"
    );

    // Truncate: drop the first entry, and an unfilled successor arrives afterwards.
    std::fs::remove_file(fx.path().join("decisions/DEC-0001.yml"))
        .expect("removes the first entry");
    fx.put("DEC-0003.yml", "DEC-0003", '3');
    let before_second = fx.bytes("DEC-0002.yml");
    let before_third = fx.bytes("DEC-0003.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 3, "{}", outcome.text);
    assert!(outcome.text.contains("genesis"), "{}", outcome.text);
    assert!(
        !outcome.text.contains("chains from"),
        "no entry was moved:\n{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("DEC-0003"),
        "the unfilled record is not the finding:\n{}",
        outcome.text
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the refusal wrote nothing"
    );
    assert_eq!(
        fx.bytes("DEC-0003.yml"),
        before_third,
        "the refusal wrote nothing"
    );
}

/// F2b: a wrong anchor at position i>0 followed by an unfilled record. The wrong anchor is the
/// finding — one reason, naming DEC-0002 — not two, the second blaming the unfilled DEC-0003 for a
/// chain it was never part of.
#[test]
fn a_wrong_anchor_before_an_unfilled_record_gives_one_reason() {
    let anchor = cli::Fixture::new("f2b-anchor");
    anchor.put("DEC-0001.yml", "DEC-0001", '1');
    let _ = anchor.decision(&["--fill"]);
    assert_eq!(
        anchor.decision(&["--verify"]).code,
        0,
        "the anchor is clean"
    );

    let fx = cli::Fixture::new("f2b");
    fx.write("DEC-0001.yml", &anchor.read("DEC-0001.yml"));
    fx.write(
        "DEC-0002.yml",
        &format!(
            "{}prev_hash: \"{}\"\n",
            cli::unfilled("DEC-0002", '2'),
            "0".repeat(64)
        ),
    );
    fx.put("DEC-0003.yml", "DEC-0003", '3');
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");
    let before_third = fx.bytes("DEC-0003.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 1, "{}", outcome.text);
    assert_eq!(
        reasons(&outcome.text).len(),
        1,
        "exactly one finding:\n{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("DEC-0002") && outcome.text.contains("chains from"),
        "{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("DEC-0003"),
        "the unfilled successor is not the finding:\n{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("does not recompute"),
        "no hash was stored to recompute:\n{}",
        outcome.text
    );
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before_first,
        "the refusal wrote nothing"
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the refusal wrote nothing"
    );
    assert_eq!(
        fx.bytes("DEC-0003.yml"),
        before_third,
        "the refusal wrote nothing"
    );
}

/// F2d, the control: a wrong anchor mid-chain whose successor is fully filled. `--verify` and
/// `--fill` must agree — same exit code, the same single reason — because the filled successor pins
/// the chain and the wrong anchor is the only finding.
#[test]
fn a_wrong_anchor_with_a_filled_successor_agrees_with_verify() {
    // DEC-0002 filled at position 0 is GENESIS-anchored: self-consistent, but in the wrong place...
    let twins = cli::Fixture::new("f2d-twins");
    twins.put("DEC-0002.yml", "DEC-0002", '2');
    let _ = twins.decision(&["--fill"]);
    assert_eq!(
        twins.decision(&["--verify"]).code,
        0,
        "a one-record log is clean at position 0"
    );
    // ...and a fully filled successor that chains from it.
    twins.put("DEC-0003.yml", "DEC-0003", '3');
    let _ = twins.decision(&["--fill"]);
    assert_eq!(
        twins.decision(&["--verify"]).code,
        0,
        "the pair is clean at those positions"
    );

    let first = cli::Fixture::new("f2d-first");
    first.put("DEC-0001.yml", "DEC-0001", '1');
    let _ = first.decision(&["--fill"]);
    assert_eq!(first.decision(&["--verify"]).code, 0);

    let fx = cli::Fixture::new("f2d");
    fx.write("DEC-0001.yml", &first.read("DEC-0001.yml"));
    fx.write("DEC-0002.yml", &twins.read("DEC-0002.yml"));
    fx.write("DEC-0003.yml", &twins.read("DEC-0003.yml"));
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");
    let before_third = fx.bytes("DEC-0003.yml");

    let verify = fx.decision(&["--verify"]);
    let fill = fx.decision(&["--fill"]);
    assert_eq!(verify.code, 1, "{}", verify.text);
    assert_eq!(fill.code, 1, "{}", fill.text);
    assert_eq!(
        reasons(&verify.text),
        reasons(&fill.text),
        "the same finding either way"
    );
    assert_eq!(reasons(&verify.text).len(), 1, "{}", verify.text);
    assert!(
        verify.text.contains("DEC-0002") && verify.text.contains("chains from"),
        "{}",
        verify.text
    );
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before_first,
        "the control wrote nothing"
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the control wrote nothing"
    );
    assert_eq!(
        fx.bytes("DEC-0003.yml"),
        before_third,
        "the control wrote nothing"
    );
}

/// The row only the "as the write would leave it" judgement can see: `--verify` stops at the
/// unfilled record and can judge nothing after it, but `--fill` would chain a fresh hash into
/// position 0 — and the successor declares a predecessor that no longer follows from it. Refuse,
/// write nothing.
#[test]
fn a_successor_chained_from_a_hash_that_no_longer_exists_refuses() {
    let fx = cli::Fixture::new("stale-successor");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    let _ = fx.decision(&["--fill"]);
    assert_eq!(fx.decision(&["--verify"]).code, 0, "the log starts clean");

    // The first record's content moves and its derived lines go: the chain cannot be walked past
    // it, and the second record still points at the hash position 0 used to have.
    let edited =
        cli::strip_derived(&fx.read("DEC-0001.yml")).replace(&"1".repeat(64), &"3".repeat(64));
    assert!(edited.contains(&"3".repeat(64)), "the fixture edit applied");
    fx.write("DEC-0001.yml", &edited);
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");

    // The stored log alone cannot settle it: the walk stops at the unfilled record.
    assert_eq!(
        fx.decision(&["--verify"]).code,
        3,
        "--verify reaches only the unfilled record"
    );

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 1, "{}", outcome.text);
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before_first,
        "the unfilled record was not filled"
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the successor was not rewritten"
    );
    assert!(
        !outcome.text.contains("filled DEC-0001"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("DEC-0002") && outcome.text.contains("chains from"),
        "the finding names the successor, which --verify cannot reach:\n{}",
        outcome.text
    );
}

/// The log does not begin at the anchor. A truncated history and a forged first entry look the
/// same from the file set alone, so the question is open — exit 3, never VALID — and `--fill` must
/// not settle it by writing.
#[test]
fn a_log_that_does_not_begin_at_genesis_refuses_as_unverifiable() {
    let fx = cli::Fixture::new("truncated");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    let _ = fx.decision(&["--fill"]);
    assert_eq!(
        fx.decision(&["--verify"]).code,
        0,
        "the two-record log is clean"
    );

    // Drop the first entry: the survivor now declares a predecessor that is not there.
    std::fs::remove_file(fx.path().join("decisions/DEC-0001.yml"))
        .expect("removes the first entry");
    let before = fx.bytes("DEC-0002.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 3, "{}", outcome.text);
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before,
        "the refusal left the record byte-identical"
    );
    assert!(outcome.text.contains("UNVERIFIABLE"), "{}", outcome.text);
    assert!(outcome.text.contains("genesis"), "{}", outcome.text);
    assert!(
        !outcome.text.contains("filled DEC-0002"),
        "{}",
        outcome.text
    );
}

/// An empty log verifies nothing, and filling nothing cannot make it valid: never `[VALID]`.
#[test]
fn an_empty_log_is_still_unverifiable() {
    let fx = cli::Fixture::new("empty");
    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 3, "{}", outcome.text);
    assert!(outcome.text.contains("UNVERIFIABLE"), "{}", outcome.text);
    assert!(!outcome.text.contains("[VALID]"), "{}", outcome.text);
    assert!(outcome.text.contains("nothing to fill"), "{}", outcome.text);
}

/// All-or-nothing: a definite finding anywhere means no file is written anywhere, so an unfilled
/// earlier record stays unfilled rather than leaving the log in a state that neither the old nor
/// the new code describes.
#[test]
fn a_definite_finding_anywhere_writes_nothing_anywhere() {
    let fx = cli::Fixture::new("all-or-nothing");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    let _ = fx.decision(&["--fill"]);
    assert_eq!(fx.decision(&["--verify"]).code, 0, "the log starts clean");

    // The first record goes back to unfilled (its derived lines removed, no value hand-written),
    // and the second is edited after it was recorded with its stale hash left standing.
    let unfilled_first = cli::strip_derived(&fx.read("DEC-0001.yml"));
    assert!(
        !unfilled_first.contains("hash:"),
        "the first record is unfilled again"
    );
    fx.write("DEC-0001.yml", &unfilled_first);
    fx.write(
        "DEC-0002.yml",
        &fx.read("DEC-0002.yml").replace("\"accept\"", "\"reject\""),
    );
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 1, "{}", outcome.text);
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before_first,
        "the unfilled record was not filled"
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the tampered record was not rewritten"
    );
    assert!(
        !outcome.text.contains("filled DEC-0001"),
        "{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("filled DEC-0002"),
        "{}",
        outcome.text
    );
}

/// Nothing unfilled, nothing tampered: the existing line, exit 0, and not one byte rewritten.
#[test]
fn a_clean_filled_log_reports_nothing_to_fill_and_writes_nothing() {
    let fx = cli::Fixture::new("clean");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    // Setup: the first fill derives the chain, and its verdict is asserted rather than discarded.
    let setup = fx.decision(&["--fill"]);
    assert_eq!(setup.code, 0, "setup: the first fill:\n{}", setup.text);
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 0, "{}", outcome.text);
    assert!(outcome.text.contains("nothing to fill"), "{}", outcome.text);
    assert_eq!(fx.bytes("DEC-0001.yml"), before_first, "bytes unchanged");
    assert_eq!(fx.bytes("DEC-0002.yml"), before_second, "bytes unchanged");
}

/// An unfilled record appended after a clean prefix: the absent field is filled, the prefix is
/// untouched byte for byte, and the verdict describes the log as it now stands on disk.
#[test]
fn an_unfilled_record_appended_after_a_clean_prefix_is_filled_alone() {
    let fx = cli::Fixture::new("appended");
    fx.put("DEC-0001.yml", "DEC-0001", '1');
    fx.put("DEC-0002.yml", "DEC-0002", '2');
    // Setup: the prefix is filled and clean, and the setup's verdict is asserted rather than
    // discarded.
    let setup = fx.decision(&["--fill"]);
    assert_eq!(setup.code, 0, "setup: the clean prefix:\n{}", setup.text);
    let before_first = fx.bytes("DEC-0001.yml");
    let before_second = fx.bytes("DEC-0002.yml");
    fx.put("DEC-0003.yml", "DEC-0003", '3');

    let outcome = fx.decision(&["--fill"]);
    assert_eq!(outcome.code, 0, "{}", outcome.text);
    assert!(outcome.text.contains("filled DEC-0003"), "{}", outcome.text);
    assert!(
        !outcome.text.contains("filled DEC-0001"),
        "{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("filled DEC-0002"),
        "{}",
        outcome.text
    );
    assert_eq!(
        fx.bytes("DEC-0001.yml"),
        before_first,
        "the prefix is byte-identical"
    );
    assert_eq!(
        fx.bytes("DEC-0002.yml"),
        before_second,
        "the prefix is byte-identical"
    );
    assert_eq!(
        fx.decision(&["--verify"]).code,
        0,
        "the appended record is now chained in"
    );
}

// ---------------------------------------------------------------------------------------------
// The gate step: `scripts/check.sh decision` re-verifies the log on every run.
//
// The observable contract is that step — its **exit code** and its printed verdict — driven
// through the `REQ_GATE_BIN` seam `tests/baseline.rs` uses for the Baseline step, and copying its
// shape: the step is separate so its failure modes are testable without the full run, which
// invokes `cargo test` and would recurse; `plan` pins its default-run call site and executes no
// check. Tooling, deliberately with no skip path: `bash`, `git`, and a `$HOME/.rustup/toolchains`
// layout `scripts/check.sh` resolves. `minisign` is *not* needed — this step consults no
// signature. Fixtures are `cli::Fixture`s, a real `claims/` directory beside `decisions/`, and
// the gate is handed the binary this test run already built, so no second cargo runs inside
// `cargo test`.
// ---------------------------------------------------------------------------------------------

/// Runs `scripts/check.sh` with `args` from the tree under test, handing the named step the binary
/// this test run already built, plus any environment a fixture needs.
#[cfg(unix)]
fn check_sh_env(
    args: &[&std::ffi::OsStr],
    req_gate_bin: &std::path::Path,
    env: &[(&str, &std::ffi::OsStr)],
) -> std::process::Output {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = std::process::Command::new("bash");
    command
        .arg(root.join("scripts/check.sh"))
        .args(args)
        .current_dir(&root)
        .env("REQ_GATE_BIN", req_gate_bin)
        // The resolver reads `REQ_ROOT` before the walk-up, so an inherited value would make the
        // no-override tests check a different corpus than the tree under test. A fixture sets it
        // explicitly.
        .env_remove("REQ_ROOT");
    for (key, value) in env {
        command.env(key, value);
    }
    common::point_home_at_toolchain(&mut command);
    command.output().expect("runs scripts/check.sh")
}

/// `scripts/check.sh <step>`, with the test-built binary behind `REQ_GATE_BIN`.
#[cfg(unix)]
fn check_sh_step(step: &str, env: &[(&str, &std::ffi::OsStr)]) -> std::process::Output {
    check_sh_env(&[std::ffi::OsStr::new(step)], &cli::req_bin(), env)
}

/// `REQ_ROOT` pointing at `fixture`, which is how the gate drives the step against a corpus that
/// is not the working tree.
#[cfg(unix)]
fn req_root(fixture: &cli::Fixture) -> Vec<(&'static str, &std::ffi::OsStr)> {
    vec![("REQ_ROOT", fixture.path().as_os_str())]
}

/// The green half: the committed log passes the step, and one check is not the gate. A step that
/// failed on a good log would not be the step.
#[cfg(unix)]
#[test]
fn the_gate_verifies_the_committed_decision_log() {
    let output = check_sh_step("decision", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the committed decision log must pass the step (exit {:?}):\n{stdout}",
        output.status.code()
    );
    assert!(stdout.contains("[VALID]"), "{stdout}");
    assert!(stdout.contains("DECISION STEP: PASS"), "{stdout}");
    assert!(
        !stdout.contains("GATE: PASS"),
        "one check is not the gate:\n{stdout}"
    );
}

/// The red test of the wiring: a recorded decision edited afterwards fails the gate step, and the
/// finding names the record. Before the step existed the gate stayed green while the log no longer
/// verified — the blind spot the Baseline step files, on the other recorded artifact.
///
/// The break is **stated** by the fixture, not discovered by re-deriving the chain: the body is
/// edited and the stored `hash` is left exactly where it was, so the finding is "the content was
/// edited after it was recorded" rather than "the derived field is absent" (which answers
/// UNVERIFIABLE, exit 3 — and would make a bare non-zero assertion pass for the wrong reason).
#[cfg(unix)]
#[test]
fn the_gate_fails_when_a_decision_was_edited_after_it_was_recorded() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("decisions/DEC-0001.yml"))
        .expect("the committed decision record reads");
    let tampered = text.replace("I accept REQ-0016", "I accept REQ-0017");
    assert_ne!(
        tampered, text,
        "the fixture must actually edit the record's content, or it exercises nothing"
    );
    let fx = cli::Fixture::new("gate-tampered");
    fx.write("DEC-0001.yml", &tampered);

    let output = check_sh_step("decision", &req_root(&fx));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "a tampered decision log must fail the step (exit {:?}):\n{stdout}",
        output.status.code()
    );
    assert!(
        stdout.contains("[TAMPERED]"),
        "a stored hash that does not recompute is a definite finding, not an open question:\n{stdout}"
    );
    assert!(
        stdout.contains("DEC-0001: the stored hash does not recompute"),
        "the finding must name the TAMPERED record:\n{stdout}"
    );
    assert!(
        !stdout.contains("[UNVERIFIABLE]"),
        "a definite finding must not be reported as an open question:\n{stdout}"
    );
    assert!(stdout.contains("(exit 1)"), "{stdout}");
    assert!(stdout.contains("DECISION STEP: FAIL"), "{stdout}");
}

/// The other side of the same decision: an empty log **blocks** with req's exit-3 class and no
/// SKIP. Pinned because the tempting "fix" for an empty log is exactly the SKIP this step
/// deliberately has none of — "we could not tell" must never be a pass, and a skipped check is
/// never a pass. A missing `decisions/` directory is that case, not an error, and it is the state
/// a consumer adopting this wiring starts from.
#[cfg(unix)]
#[test]
fn the_gate_blocks_an_empty_decision_log_without_a_skip() {
    let fx = cli::Fixture::new("gate-empty");
    // No decisions/ directory at all: the resolver has its claims/, so the root is accepted and
    // the log it finds is empty.
    std::fs::remove_dir(fx.path().join("decisions")).expect("removes the empty decisions/");

    let output = check_sh_step("decision", &req_root(&fx));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "an empty log verifies nothing, so it must block (exit {:?}):\n{stdout}",
        output.status.code()
    );
    assert!(stdout.contains("[UNVERIFIABLE]"), "{stdout}");
    assert!(
        stdout.contains("the decision log is empty"),
        "the fixture must be the empty-log case, not a missing corpus:\n{stdout}"
    );
    assert!(
        stdout.contains("(exit 3)"),
        "the step consumes req's exit-3 class:\n{stdout}"
    );
    assert!(
        !stdout.contains("SKIP"),
        "an empty log is UNVERIFIABLE, never a skip:\n{stdout}"
    );
    assert!(stdout.contains("DECISION STEP: FAIL"), "{stdout}");
}

/// The default run's own call site — the wiring that re-verifies the log on every gate invocation.
/// The standalone-step tests above cannot reach it: `decision` takes its own branch and returns
/// before the default flow. The `plan` step prints the default run's commands from the same call
/// sites and executes none, so this pins the wiring without running the gate. It is UNVERIFIABLE
/// (exit 3): a plan is not a pass.
#[cfg(unix)]
#[test]
fn the_default_gate_run_verifies_the_decision_log() {
    let plan = check_sh_step("plan", &[]);
    let stdout = String::from_utf8_lossy(&plan.stdout);
    assert_eq!(
        plan.status.code(),
        Some(3),
        "a plan executes nothing, so it is UNVERIFIABLE, never a pass:\n{stdout}"
    );
    // Compared on normalized lines: the plan's content is the contract, its column padding is
    // not, so a cosmetic reformat of check.sh must not fail this test.
    let planned: Vec<String> = stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    assert!(
        planned
            .iter()
            .any(|line| line.starts_with("PLAN ") && line.contains("decision --verify")),
        "the default run must verify the decision log:\n{stdout}"
    );
    // ...and it is the default run being planned, not the single step.
    assert!(
        planned.iter().any(
            |line| line.starts_with("PLAN ") && line.contains("cargo nextest run --profile ci")
        ),
        "{stdout}"
    );
    // The override is the *standalone* step's seam: the full gate never honours it, so a stray
    // REQ_GATE_BIN — set for this very invocation — must not appear in any of the planned calls.
    assert!(
        !planned
            .iter()
            .any(|line| line.starts_with("PLAN ") && line.contains("REQ_GATE_BIN")),
        "the full gate must not honour REQ_GATE_BIN:\n{stdout}"
    );
    assert!(stdout.contains("GATE: UNVERIFIABLE"), "{stdout}");
}

/// The binary seam refuses what it cannot mean, by name and with exit 2: a value that is not a
/// regular executable file. A directory is the interesting case — `-x` is true for it, so testing
/// only executability would send it to a generic gate failure instead, and a stray environment
/// value must not be able to redirect the verification the gate exists to perform.
#[cfg(unix)]
#[test]
fn the_gate_refuses_a_binary_override_that_is_not_an_executable_file() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // A regular file with the execute bit cleared is the only value that reaches the second half
    // of the guard: a directory and a missing path already fail its regular-file test.
    let fx = cli::Fixture::new("gate-binary-override");
    let plain = fx.path().join("not-executable");
    std::fs::write(&plain, b"#!/bin/sh\nexit 0\n").expect("writes a plain file");
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644))
        .expect("clears the execute bit");

    for bad in [root.clone(), root.join("nowhere-at-all"), plain.clone()] {
        let refused = check_sh_env(&[std::ffi::OsStr::new("decision")], &bad, &[]);
        assert_eq!(
            refused.status.code(),
            Some(2),
            "{} must be refused as a usage error:\n{}",
            bad.display(),
            String::from_utf8_lossy(&refused.stdout)
        );
        assert!(
            String::from_utf8_lossy(&refused.stderr)
                .contains("REQ_GATE_BIN is not an executable file"),
            "{}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }
}
