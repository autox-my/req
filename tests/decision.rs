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
