//! req — the semantic, git-native requirements engine.
//!
//! Subcommands:
//!   req [--root <dir>] bench [corpus.yml] — run the FMR gate (default: bench/corpus/parity.yml)
//!   req [--root <dir>] canon               — parse diagnostics over <root>/claims/REQ-*.yml
//!   req [--root <dir>] canon --fill        — write the derived `canonical.form` + `address`
//!   req [--root <dir>] canon --verify      — check the stored derived fields against the parse
//!   req [--root <dir>] compare A.yml B.yml — what kind of change is this between two claims?
//!   req [--root <dir>] select k=v [...]    — which claims apply in this variant? (tri-state)
//!   req [--root <dir>] render k=v [...]    — the spec view for that selection
//!   req [--root <dir>] trace k=v [...]     — the traceability view, with derived incoming edges
//!   req [--root <dir>] impact <claim-id>   — what depends on that claim, transitively
//!   req [--root <dir>] gate k=v [...]      — may this selection ship? (policies over applying claims)
//!   req [--root <dir>] propose             — the Tier-2 funnel: blocking, then provider adjudication
//!   req [--root <dir>] baseline            — emit the canonical manifest a human signs (ADR 0010)
//!   req [--root <dir>] baseline --verify F — verify a signed Baseline: drift, trust, signature
//!   req [--root <dir>] audit [--baseline F] — the audit packet: coverage, provenance, undecided
//!   req [--root <dir>] decision [--fill]   — the decision log and its hash chain (step 8)
//!
//! The corpus root is resolved at **runtime** (ADR 0012), once, before dispatch: `--root`,
//! then `REQ_ROOT`, then the nearest ancestor of the working directory holding `claims/`.
//! The resolved root is printed to **stderr** — never stdout, which carries the exact bytes
//! a human signs (`req baseline`) — and a root that yields no corpus is UNVERIFIABLE (exit
//! 3), never an empty answer.
//!
//! The Tier-1 canonicalizer (src/canon.rs) is deterministic and offline: no model I/O,
//! no network, no `HashMap` iteration order dependence, no float math anywhere near
//! the address computation.
use req::{
    audit, baseline, bench, canon, compare, decision, fill, funnel, gate, impact, root, select,
    view,
};
use std::process::exit;

const USAGE: &str = "usage: req [--root <dir>] bench [corpus.yml] | req canon [--fill|--verify] | req select k=v [...]";

/// Every subcommand that reads a corpus. Checked before the root is resolved, so a typo is
/// a usage error (exit 2) rather than a root lookup reported as UNVERIFIABLE (exit 3).
const COMMANDS: [&str; 12] = [
    "bench", "select", "render", "trace", "impact", "gate", "propose", "baseline", "audit",
    "decision", "compare", "canon",
];

fn main() {
    // `--root` is the tool's parameter, not any subcommand's: strip it once, in both forms,
    // before anything else parses the remaining arguments.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (cli_root, mut rest) = match root::take_root_arg(&argv) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            exit(2);
        }
    };
    if rest.is_empty() {
        eprintln!("{USAGE}");
        exit(2);
    }
    let cmd = rest.remove(0);
    let args = rest;
    if !COMMANDS.contains(&cmd.as_str()) {
        eprintln!(
            "unrecognized subcommand: {cmd}\nusage: req bench [corpus.yml] | req canon [--fill]"
        );
        exit(2);
    }

    // Resolve ONCE, here: the resolver is pure, so this is the single place the process's
    // working directory and environment are read (ADR 0012).
    let start = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("[UNVERIFIABLE] cannot read the working directory: {e}");
            exit(3);
        }
    };
    let env_req_root = std::env::var("REQ_ROOT").ok();
    let corpus =
        match root::resolve_corpus_root(&start, cli_root.as_deref(), env_req_root.as_deref()) {
            Ok(dir) => dir,
            Err(message) => {
                eprintln!("[UNVERIFIABLE] {message}");
                exit(3);
            }
        };
    // Exactly once per invocation, on stderr: a silently-chosen root is the failure mode
    // ADR 0012 exists to remove, and stdout carries the bytes a human signs (`req baseline`).
    eprintln!("corpus root: {}", corpus.display());

    let code = match cmd.as_str() {
        "bench" => bench::run(args.first().map(String::as_str), &corpus),
        "select" => select::run(&args, &corpus),
        "render" => view::run_render(&args, &corpus),
        "trace" => view::run_trace(&args, &corpus),
        "impact" => impact::run(&args, &corpus),
        "gate" => gate::run(&args, &corpus),
        "propose" => funnel::run(&args, &corpus),
        "baseline" => baseline::run(&args, &corpus),
        "audit" => audit::run(&args, &corpus),
        "decision" => decision::run(&args, &corpus),
        "compare" => match (args.first(), args.get(1)) {
            (Some(a), Some(b)) => compare::run(a, b, &corpus),
            _ => {
                eprintln!("usage: req compare <a.yml> <b.yml>");
                2
            }
        },
        "canon" => match args.first().map(String::as_str) {
            // Only the derived fields are ever written, and only for claims whose
            // parse agrees with their authored slots.
            Some("--fill") => fill::run_fill(&corpus),
            // The check that makes "never hand-write an address" enforceable.
            Some("--verify") => fill::run_verify(&corpus),
            None => canon::run_claim_diagnostics_at(&corpus),
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
