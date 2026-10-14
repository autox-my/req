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

/// `sha256(prefix ‖ bytes)` as lowercase hex — the domain-separated form the identity path, the
/// decision chain and the audit-surface digests all take, so two surfaces can never trade
/// hashes.
pub fn sha256_hex_prefixed(prefix: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

fn to_hex(digest: &[u8]) -> String {
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(HEX_DIGITS[(byte >> 4) as usize] as char);
        hex.push(HEX_DIGITS[(byte & 0x0f) as usize] as char);
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
}
