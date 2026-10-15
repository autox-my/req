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
use std::path::Path;
use std::process::exit;

/// One line per shape of invocation, matching the module doc above. Printed both when no
/// subcommand was given and when one was not recognized, so the two cannot describe
/// different tools — the old fallback listed three subcommands and never mentioned `--root`.
const USAGE: &str = "\
usage: req [--root <dir>] <command> [args]

  bench [corpus.yml]              the FMR gate (default: bench/corpus/parity.yml)
  canon [--fill | --verify]       parse diagnostics, or write/check the derived address
  compare <a.yml> <b.yml>         what kind of change is this between two claims?
  select | render | trace | gate k=v [...]
                                  the variant projections and the release gate
  impact <claim-id>               what depends on that claim, transitively
  propose                         the Tier-2 funnel: blocking, then provider adjudication
  baseline [--verify <file>]      the manifest a human signs, or verify one
  audit [--baseline <file>]       the audit packet: coverage, provenance, undecided
  decision [--fill | --verify]    the decision log and its hash chain

The corpus root is resolved at runtime: --root, else REQ_ROOT, else the nearest ancestor of
the working directory holding claims/ (ADR 0012).";

/// A subcommand's handler: its own arguments (`--root` already stripped) and the corpus.
type Handler = fn(&[String], &Path) -> i32;

/// Every subcommand that reads a corpus, with the handler that runs it — **one** table.
///
/// The allowlist that turns a typo into a usage error and the dispatch that runs the command
/// are the same list, so a name cannot be accepted and then fall through to "unrecognized",
/// and no arm can exist that the allowlist would never reach. A `match` beside a separate
/// `COMMANDS.contains` check was two sources of truth for one fact, free to drift.
const COMMANDS: &[(&str, Handler)] = &[
    ("bench", |args, corpus| {
        bench::run(args.first().map(String::as_str), corpus)
    }),
    ("select", |args, corpus| select::run(args, corpus)),
    ("render", |args, corpus| view::run_render(args, corpus)),
    ("trace", |args, corpus| view::run_trace(args, corpus)),
    ("impact", |args, corpus| impact::run(args, corpus)),
    ("gate", |args, corpus| gate::run(args, corpus)),
    ("propose", |args, corpus| funnel::run(args, corpus)),
    ("baseline", |args, corpus| baseline::run(args, corpus)),
    ("audit", |args, corpus| audit::run(args, corpus)),
    ("decision", |args, corpus| decision::run(args, corpus)),
    ("compare", |args, corpus| {
        match (args.first(), args.get(1)) {
            (Some(a), Some(b)) => compare::run(a, b, corpus),
            _ => {
                eprintln!("usage: req compare <a.yml> <b.yml>");
                2
            }
        }
    }),
    ("canon", |args, corpus| {
        match args.first().map(String::as_str) {
            // Only the derived fields are ever written, and only for claims whose parse agrees
            // with their authored slots.
            Some("--fill") => fill::run_fill(corpus),
            // The check that makes "never hand-write an address" enforceable.
            Some("--verify") => fill::run_verify(corpus),
            None => canon::run_claim_diagnostics_at(corpus),
            Some(other) => {
                eprintln!("unrecognized flag: {other}\nusage: req canon [--fill|--verify]");
                2
            }
        }
    }),
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
    // Looked up before the root is resolved, so a typo is a usage error (exit 2) rather than
    // a root lookup reported as UNVERIFIABLE (exit 3) — and looked up in the same table that
    // dispatches below, so the two cannot disagree about which subcommands exist.
    let Some(run) = COMMANDS
        .iter()
        .find(|(name, _)| *name == cmd)
        .map(|(_, run)| *run)
    else {
        eprintln!("unrecognized subcommand: {cmd}\n{USAGE}");
        exit(2);
    };

    // Resolve ONCE, here: the resolver is pure, so this is the single place the process's
    // environment is consulted and the only place the working directory is read (ADR 0012).
    // It is passed as a thunk because the walk-up is the only branch that wants it: a root
    // named by `--root` or `REQ_ROOT` must resolve even when the CWD cannot be read.
    let env_req_root = std::env::var("REQ_ROOT").ok();
    let corpus = match root::resolve_corpus_root(
        std::env::current_dir,
        cli_root.as_deref(),
        env_req_root.as_deref(),
    ) {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("[UNVERIFIABLE] {message}");
            exit(3);
        }
    };
    // Exactly once per invocation, on stderr: a silently-chosen root is the failure mode
    // ADR 0012 exists to remove, and stdout carries the bytes a human signs (`req baseline`).
    eprintln!("corpus root: {}", corpus.display());

    // The handler found above, run from the one table: there is no second dispatch to keep
    // in step with it.
    exit(run(&args, &corpus));
}
