//! req — the semantic, git-native requirements engine.
//!
//! Subcommands:
//!   req bench [corpus.yml]  — run the FMR gate (default: bench/corpus/parity.yml)
//!   req canon               — parse diagnostics over claims/REQ-*.yml
//!   req canon --fill        — write the derived `canonical.form` + `address`
//!   req canon --verify      — check the stored derived fields against the parse
//!   req compare A.yml B.yml — what kind of change is this between two claims?
//!   req select k=v [...]    — which claims apply in this variant? (tri-state)
//!   req render k=v [...]    — the spec view for that selection
//!   req trace k=v [...]     — the traceability view, with derived incoming edges
//!   req impact <claim-id>   — what depends on that claim, transitively
//!   req gate k=v [...]      — may this selection ship? (policies over applying claims)
//!   req propose             — the Tier-2 funnel: blocking, then provider adjudication (if any)
//!   req baseline            — emit the canonical manifest a human signs (ADR 0010)
//!   req baseline --verify F — verify a signed Baseline: drift, trust, signature
//!   req audit [--baseline F] — the audit packet: coverage, provenance, undecided
//!   req decision [--fill]   — the decision log and its hash chain (step 8)
//!
//! The Tier-1 canonicalizer (src/canon.rs) is deterministic and offline: no model I/O,
//! no network, no `HashMap` iteration order dependence, no float math anywhere near
//! the address computation.
use req::{
    audit, baseline, bench, canon, compare, decision, fill, funnel, gate, impact, select, view,
};
use std::process::exit;

fn main() {
    let mut args = std::env::args();
    let cmd = args.nth(1).unwrap_or_else(|| {
        eprintln!("usage: req bench [corpus.yml] | req canon [--fill]");
        exit(2);
    });
    let code = match cmd.as_str() {
        "bench" => bench::run(args.next().as_deref()),
        "select" => select::run(&args.collect::<Vec<String>>()),
        "render" => view::run_render(&args.collect::<Vec<String>>()),
        "trace" => view::run_trace(&args.collect::<Vec<String>>()),
        "impact" => impact::run(&args.collect::<Vec<String>>()),
        "gate" => gate::run(&args.collect::<Vec<String>>()),
        "propose" => funnel::run(&args.collect::<Vec<String>>()),
        "baseline" => baseline::run(&args.collect::<Vec<String>>()),
        "audit" => audit::run(&args.collect::<Vec<String>>()),
        "decision" => decision::run(&args.collect::<Vec<String>>()),
        "compare" => match (args.next(), args.next()) {
            (Some(a), Some(b)) => compare::run(&a, &b),
            _ => {
                eprintln!("usage: req compare <a.yml> <b.yml>");
                2
            }
        },
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
