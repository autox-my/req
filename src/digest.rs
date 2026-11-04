//! The one SHA-256 → lowercase-hex encoder.
//!
//! Four call sites across the crate hashed and then re-encoded the digest bytes, each with its
//! own loop — three of them allocating a temporary `String` per byte. One helper here means the
//! bytes cannot drift between the Tier-1 identity path, the decision hash chain and the
//! audit-surface digests, and the encoding allocates once.

use sha2::{Digest, Sha256};

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// `sha256(bytes)` as lowercase hex. Deterministic: no float, no locale, and no allocation per
/// byte — the digest is written straight into one preallocated buffer.
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// `sha256(prefix ‖ bytes)` as lowercase hex — the domain-separated form taken by the
/// audit-surface digests (`model::relations_digest` / `model::rationale_digest`) and by the
/// Tier-1 identity (`canon::address_hex`, the same shape over `ADDRESS_TAG`). The decision
/// chain does *not* take this: it builds a multi-part input and calls the plain
/// [`sha256_hex`]. The prefix **must be NUL-delimited** — every real tag is
/// (`req.claim.v1\0`, `req.relations.v1\0`, `req.rationale.v1\0`) — which is what actually
/// stops one surface's prefix from straddling another's; a bare domain name would let
/// `req.relations.v1` match `req.relations.v1extra`. That contract is caller-side, so it is
/// checked here rather than only documented.
pub fn sha256_hex_prefixed(prefix: &[u8], bytes: &[u8]) -> String {
    assert!(
        prefix.ends_with(b"\0"),
        "digest prefix must be NUL-delimited so domains cannot straddle: {:?}",
        String::from_utf8_lossy(prefix)
    );
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

fn to_hex(digest: &[u8]) -> String {
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinned against the shell, independently of this code:
    /// `printf 'req.claim.v1\0[]' | shasum -a 256` (the `\0` is a literal NUL byte).
    #[test]
    fn the_prefixed_digest_matches_an_independent_oracle() {
        let expected = "6261adc8a2581b0d5ca022cbd593d42f44f3a63363459b947e8b591bf574754a";
        assert_eq!(sha256_hex_prefixed(b"req.claim.v1\0", b"[]"), expected);
        // Same bytes without the prefix hash differently: the prefix is load-bearing.
        assert_ne!(
            sha256_hex_prefixed(b"req.claim.v1\0", b"[]"),
            sha256_hex(b"[]")
        );
    }

    #[test]
    fn the_empty_input_hashes_to_the_known_sha256() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// A bare domain name would let `req.relations.v1` straddle `req.relations.v1extra`, so the
    /// guard refuses it rather than silently weakening the separation.
    #[test]
    #[should_panic(expected = "NUL-delimited")]
    fn a_bare_domain_prefix_is_refused() {
        let _ = sha256_hex_prefixed(b"req.relations.v1", b"[]");
    }
}
