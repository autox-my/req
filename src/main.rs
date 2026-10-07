//! req — the semantic, git-native requirements engine.
//!
//! Subcommands (v0 scaffold of design.md build step 3):
//!   req bench   — run the FMR gate over bench/corpus/parity.yml (the falsification step)
//!   req canon   — parse diagnostics over claims/REQ-*.yml (parity-invariant surface, v0)
//!
//! The Tier-1 canonicalizer (src/canon.rs) is deterministic and offline: no model I/O,
//! no network, no `HashMap` iteration order dependence, no float math anywhere near
//! the address computation.
use req::{bench, canon};
use std::process::exit;

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: req bench | req canon");
        exit(2);
    });
    let code = match cmd.as_str() {
        "bench" => bench::run(),
        "canon" => canon::run_claim_diagnostics(),
        other => {
            eprintln!("unrecognized subcommand: {other}\nusage: req bench | req canon");
            2
        }
    };
    exit(code);
}
