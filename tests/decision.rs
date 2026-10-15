//! The decision log: the chain, its three outcomes, and the boundaries between them.

use req::decision::{
    Chain, Decision, GENESIS, canonical_bytes, compute_hash, fill, upsert_line, verify_chain,
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
    let _ = fx.decision(&["--fill"]);
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
    let _ = fx.decision(&["--fill"]);
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
