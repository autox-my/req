//! req — the semantic, git-native requirements engine.
//!
//! Subcommands:
//!   req bench [corpus.yml]  — run the FMR gate (default: bench/corpus/parity.yml)
//!   req canon               — parse diagnostics over claims/REQ-*.yml
//!   req canon --fill        — write the derived `canonical.form` + `address`
//!   req canon --verify      — check the stored derived fields against the parse
//!
//! The Tier-1 canonicalizer (src/canon.rs) is deterministic and offline: no model I/O,
//! no network, no `HashMap` iteration order dependence, no float math anywhere near
//! the address computation.
use req::{bench, canon, fill};
use std::process::exit;

fn main() {
    let mut args = std::env::args();
    let cmd = args.nth(1).unwrap_or_else(|| {
        eprintln!("usage: req bench [corpus.yml] | req canon [--fill]");
        exit(2);
    });
    let code = match cmd.as_str() {
        "bench" => bench::run(args.next().as_deref()),
        "canon" => match args.next().as_deref() {
            // Only the derived fields are ever written, and only for claims whose
            // parse agrees with their authored slots.
            Some("--fill") => fill::run_fill(),
            // The check that makes "never hand-write an address" enforceable.
            Some("--verify") => fill::run_verify(),
            None => canon::run_claim_diagnostics(),
            Some(other) => {
                eprintln!("unrecognized flag: {other}\nusage: req canon [--fill|--verify]");
                2
            }
        },
        other => {
            eprintln!(
                "unrecognized subcommand: {other}\nusage: req bench [corpus.yml] | req canon [--fill]"
            );
            2
        }
    };
    exit(code);
}
