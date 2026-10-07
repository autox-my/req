//! req — the semantic, git-native requirements engine.
//!
//! Subcommands:
//!   req bench [corpus.yml]  — run the FMR gate (default: bench/corpus/parity.yml)
//!   req canon               — parse diagnostics over claims/REQ-*.yml
//!
//! The Tier-1 canonicalizer (src/canon.rs) is deterministic and offline: no model I/O,
//! no network, no `HashMap` iteration order dependence, no float math anywhere near
//! the address computation.
use req::{bench, canon};
use std::process::exit;

fn main() {
    let mut args = std::env::args();
    let cmd = args.nth(1).unwrap_or_else(|| {
        eprintln!("usage: req bench [corpus.yml] | req canon");
        exit(2);
    });
    let code = match cmd.as_str() {
        "bench" => bench::run(args.next().as_deref()),
        "canon" => canon::run_claim_diagnostics(),
        other => {
            eprintln!(
                "unrecognized subcommand: {other}\nusage: req bench [corpus.yml] | req canon"
            );
            2
        }
    };
    exit(code);
}
