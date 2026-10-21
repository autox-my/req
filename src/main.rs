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
//!   req [--root <dir>] baseline --at <ts>   — emit the canonical manifest a human signs (ADR 0016)
//!   req [--root <dir>] baseline --verify F — verify a signed Baseline: drift, authority, trust,
//!                                             signature, with the key judged at its signed_at
//!   req [--root <dir>] audit [--baseline F] — the audit packet: coverage, provenance, undecided
//!   req [--root <dir>] decision [--fill]   — the decision log and its hash chain (step 8)
//!   req init                               — create a corpus in the working directory
//!
//! The corpus root is resolved at **runtime** (ADR 0012), once, before dispatch: `--root`,
//! then `REQ_ROOT`, then the nearest ancestor of the working directory holding `claims/`.
//! The resolved root is printed to **stderr** — never stdout, which carries the exact bytes
//! a human signs (`req baseline`) — and a root that yields no corpus is UNVERIFIABLE (exit
//! 3), never an empty answer.
//!
//! `--version`, `--help`, `<command> --help` and `init` are answered **before** that
//! resolution, in the position where the subcommand is looked up. For the flags the reason is
//! that they must work in any directory — and a command's help may be read to find out how to
//! create the corpus that command reads; for `init` it is that `init` is what *creates* the
//! root, so resolving one would be answering a question it is about to make true (and `--root`
//! is a usage error there rather than a second, silently different anchor).
//!
//! The Tier-1 canonicalizer (src/canon.rs) is deterministic and offline: no model I/O,
//! no network, no `HashMap` iteration order dependence, no float math anywhere near
//! the address computation.
use req::{
    audit, baseline, bench, canon, compare, decision, fill, funnel, gate, impact, init, root,
    select, view,
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
  init                            create a corpus in the current directory (no root lookup)
  propose                         the Tier-2 funnel: blocking, then provider adjudication
  baseline --at <ts>              the manifest a human signs (--at is the signing time)
  baseline --verify <file>        verify one (a v3 manifest's signed_at judges the key)
  audit [--baseline <file>]       the audit packet: coverage, provenance, undecided
  decision [--fill | --verify]    the decision log and its hash chain
  --version | -V                  the versions that decide a verdict, and the build identity
  --help | -h                     this text, or <command> --help for one command's page

The corpus root is resolved at runtime: --root, else REQ_ROOT, else the nearest ancestor of
the working directory holding claims/ (ADR 0012).";

/// What `--version` prints: the crate's version on the first line — single-line and greppable,
/// so `req --version | head -1` behaves like any other tool's — then every version axis this
/// build *judges* by, then the build identity.
///
/// Every value is read from the constant that carries it (`env!("CARGO_PKG_VERSION")`,
/// [`canon::RULES_V`], [`baseline::BASELINE_VERSION`], [`decision::DECISION_VERSION`],
/// [`canon::claim_schema()`]), never re-typed here: a literal `5` in this file would be a second
/// copy of `RULES_V`, free to drift from the value that decides whether a stored address is
/// current.
///
/// `rules_v` comes first because it is that axis and the first thing to check when a verdict
/// surprises the reader. The build line is printed even when the identity cannot be established
/// (`unknown`, from `build.rs`): an absent line reads as "not applicable", a different claim.
fn version_text() -> String {
    let axes = [
        ("rules_v", canon::RULES_V.to_string()),
        ("baseline_envelope", baseline::BASELINE_VERSION.to_string()),
        ("decision_chain", decision::DECISION_VERSION.to_string()),
        ("claim_schema", canon::claim_schema()),
        ("built from", env!("REQ_BUILD_SHA").to_string()),
    ];
    // Two-space indent and a 19-wide label column: the values line up, and the block stays one
    // fact per line for a reader and for a grep.
    let mut out = format!("req {}\n", env!("CARGO_PKG_VERSION"));
    for (label, value) in axes {
        out.push_str(&format!("  {label:<19}{value}\n"));
    }
    out
}

/// A subcommand's handler: its own arguments (`--root` already stripped) and the corpus.
type Handler = fn(&[String], &Path) -> i32;

/// Whether these arguments ask for a page: `--help` and `-h` are one request.
///
/// One definition rather than a comparison repeated at each call site: the table's answer and
/// `init`'s must not be able to disagree about what counts as asking for help.
fn help_requested(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

/// A subcommand: its name, the handler that runs it, and the page `<name> --help` prints.
///
/// The page is a **field of the dispatch entry**, not a second table keyed by the same string:
/// a verb added without a page does not compile, so completeness is structural rather than
/// something a test has to remember. It is also the reason the two cannot drift — there is no
/// pair of lists to keep in step.
struct Command {
    name: &'static str,
    run: Handler,
    help: &'static str,
}

// The pages. Each is a map, not the territory: the exit contract is the payload a usage line
// cannot carry, and the long semantics stay in `AGENTS.md` §Commands (the anchor closes every
// page). The pages and that reference are both claims: if one cannot agree with the other, the
// wrong one is fixed in the same commit.

const BENCH_HELP: &str = "\
usage: req bench [corpus.yml]

Runs the benchmark gate over a corpus — the seed parity corpus by default — and records the
result under bench/results/. Each labelled pair is the oracle: a false merge is the failure
the gate exists to catch, so the record is written even when the run fails.

exit: 0 the gate passed (no false merge), 1 a false merge or a direction confusion, 3
UNVERIFIABLE (an unreadable corpus, a degraded run that found nothing, or a vacuous
denominator). A run that found a defect is 1 even when it was degraded.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const CANON_HELP: &str = "\
usage: req canon [--fill | --verify]

Reports the parity invariant per claim: whether the authored slots agree with a fresh parse
of `statement`. A contradiction or an omission is a flag; a qualitative attribute/value
triple is an annotation, which identity does not carry. `--fill` writes the derived
`canonical.form` and `address`; `--verify` checks the stored derived fields.

exit: 0 diagnostics reported — not a pass: it reports, and it never gates; `--verify` is what
gates: 0 verified, 1 a stored digest that does not recompute, 3 a re-canonicalization candidate
(unfilled, or filled under other rules) or a claim file this build does not write. `--fill`:
0 filled, 3 refused (a contradiction or an omission). 3 for a registry or claim file that
cannot be read, and 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const COMPARE_HELP: &str = "\
usage: req compare <a.yml> <b.yml>

Classifies the direction of change between two claims — modality weakening, numeric
tightening or loosening, polarity flip, scope divergence — from their canonical forms. A
classification is for a human to ratify, never an approval.

exit: 0 classified, 3 UNVERIFIABLE (a side that will not load, or that declares a file
format this build does not write, so its parse is NOT JUDGED; a pair with no structured form
to compare), 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const SELECT_HELP: &str = "\
usage: req select key=value [key=value ...]

Reports which claims apply in a variant, tri-state: a claim whose applicability predicate
the context does not decide is UNKNOWN, never excluded — that would drop a safety claim out
of a variant silently. Selection changes no address: a variant is a projection over one
model.

exit: 0 every claim decided, 3 at least one claim left undecided, 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const RENDER_HELP: &str = "\
usage: req render [key=value ...]

Prints the spec view for a selection: every applying claim's address, modality, statement,
selector and outgoing relations, with the undecided claims in their own section and the
outside-the-selection ones named. It is a projection: it reads claims, never writes one.

exit: 0 settled, 3 anything an undecided claim leaves open, 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const TRACE_HELP: &str = "\
usage: req trace [key=value ...]

Prints the traceability matrix for a selection: the outgoing edges, the **derived** incoming
edges (each relation is stored once, on the outgoing side), and the findings. Every edge that
leaves the selection is named — to an excluded claim, or to one this variant leaves undecided
— and a relation to a claim the corpus does not hold is named UNVERIFIABLE, never ignored.
It is a projection: it reads claims, never writes one.

exit: 0 the edge graph is settled, 3 an edge into a claim the corpus does not hold or into
one the variant leaves undecided leaves the matrix open, 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const GATE_HELP: &str = "\
usage: req gate [key=value ...]

Runs the release policies over the claims a variant leaves applying — addresses current,
relations resolve, dependencies decided, rationale present — and confirms the invocation
itself. A claim the invocation leaves undecided was not examined, and that is not a pass: a
bare gate on a corpus carrying a selector is UNVERIFIABLE.

exit: 0 every policy passed and every claim decided, 1 a defect, 3 something that cannot be
confirmed (an unfilled claim, a relation that does not resolve, a dependency left undecided,
an entry the corpus cannot confirm, an empty selection) or any claim left undecided — named
with the remedy; 2 usage. Fail outranks Unverifiable, and an undecided claim outranks a pass.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const IMPACT_HELP: &str = "\
usage: req impact <claim-id> [key=value ...]

Walks the relations backwards (derived, never stored twice) and reports every dependent of a
claim nearest-first, with its state in the selection.

exit: 0 complete and settled, 3 UNVERIFIABLE (a missing target, an undecided target or
dependent, or a relation that does not resolve — an answer a broken relation could have
shortened says so instead of reading as complete), 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const PROPOSE_HELP: &str = "\
usage: req propose [--min-shared N] [--max-pairs N] [--gold <file>] [--provider-cmd <cmd>]
                   [--repeats N] [--min-confidence N] [--pairs] [key=value ...]

Runs the Tier-2 funnel over the claims: skips pairs the address already resolves, blocks the
rest on shared content terms, then puts the candidates to a provider. The provider proposes;
a human disposes. Without `--provider-cmd` the offline provider answers Unverifiable — never
\"unrelated\" — so an offline run refuses to guess.

exit: 0 every candidate adjudicated, 1 a gold-set FAIL (a false merge, an unreachable
equivalent, or a flip rate above 2%), 3 anything unadjudicated or deferred (no provider, a
provider that declined, or `--max-pairs` stopping short), 2 usage. Under `--gold` the gold
result is the verdict; an unmeasured flip rate is UNVERIFIABLE, never a pass.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const BASELINE_HELP: &str = "\
usage: req baseline --at <RFC 3339 UTC timestamp> [key=value ...]
       req baseline --verify <file> [--signature <file>] [--key <id>] [--now <date>]

Emits the canonical manifest — the exact byte string a human signs — or verifies one: drift,
authority, trust and signature, with the key judged at the manifest's `signed_at` for a v3
envelope (today for v2, and `--now` sets the judgement time explicitly). The signing time is
authored, never invented, and req never signs.

exit: 0 emitted or VERIFIED, 1 FAIL (drift, a signature or authority that does not verify),
3 UNVERIFIABLE (no verifier, an unknown key, no signature, a `rules_v` mismatch, or an absent
decision log), 2 usage (emitting without `--at`, an unreadable timestamp, `--at` on the
verification path).

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const AUDIT_HELP: &str = "\
usage: req audit [--baseline <file>] [key=value ...]

Prints the audit packet for a Baseline: every applying claim's address, coverage state,
provenance and outgoing relations, then the undecided claims in their own section, then the
findings. Coverage is derived from the Baseline's drift; whether a signature is valid belongs
to `req baseline --verify`, and this packet never asserts authority it did not check.

exit: 0 every applying claim covered and still matching, 1 a defect (drift, or no
provenance), 3 nothing ratified, something uncovered, or coverage NOT JUDGED because the
manifest's `rules_v` disagrees with this build's, 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const DECISION_HELP: &str = "\
usage: req decision [--fill | --verify]

Walks the decision log's hash chain — one record per file, each committing to its
predecessor — or with `--fill` writes the derived `prev_hash`/`hash` into the records,
preserving comments and the note a human wrote. It never overwrites a record, and it refuses
to recompute over a stored field that disagrees with the content.

exit: 0 VALID, 1 TAMPERED (a stored hash that does not recompute, or a `prev_hash` that
disagrees with the entry before it), 3 UNVERIFIABLE (an unfilled hash, an unparsable file, an
empty log, or a log that does not begin at the genesis anchor), 2 usage.

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

const INIT_HELP: &str = "\
usage: req init

Creates a corpus in the current directory: a starter claim filled and verified through the
same derivation `req canon --fill` uses, the claims README, and the subject registry. It
refuses rather than overwrite an existing corpus, and it reads no root — it creates one.

exit: 0 created, 1 refused (`claims/` or `aliases/subjects.yml` already exists, or `aliases`
is a symlink), 3 UNVERIFIABLE (the starter claim could not be verified, or the target cannot
be written — whatever it wrote is removed), 2 usage (an argument, or `--root`: run `init`
where the corpus should live).

`AGENTS.md` §Commands for the reference, `docs/adopting.md` for the loop.";

/// Every subcommand that reads a corpus, with the handler that runs it and the page it prints —
/// **one** table.
///
/// The allowlist that turns a typo into a usage error, the dispatch that runs the command and
/// the help page it answers with are the same list, so a name cannot be accepted and then fall
/// through to "unrecognized", no arm can exist that the allowlist would never reach, and no
/// verb can exist without a page. A `match` beside a separate `COMMANDS.contains` check was two
/// sources of truth for one fact, free to drift.
const COMMANDS: &[Command] = &[
    Command {
        name: "bench",
        run: |args, corpus| bench::run(args.first().map(String::as_str), corpus),
        help: BENCH_HELP,
    },
    Command {
        name: "select",
        run: |args, corpus| select::run(args, corpus),
        help: SELECT_HELP,
    },
    Command {
        name: "render",
        run: |args, corpus| view::run_render(args, corpus),
        help: RENDER_HELP,
    },
    Command {
        name: "trace",
        run: |args, corpus| view::run_trace(args, corpus),
        help: TRACE_HELP,
    },
    Command {
        name: "impact",
        run: |args, corpus| impact::run(args, corpus),
        help: IMPACT_HELP,
    },
    Command {
        name: "gate",
        run: |args, corpus| gate::run(args, corpus),
        help: GATE_HELP,
    },
    Command {
        name: "propose",
        run: |args, corpus| funnel::run(args, corpus),
        help: PROPOSE_HELP,
    },
    Command {
        name: "baseline",
        run: |args, corpus| baseline::run(args, corpus),
        help: BASELINE_HELP,
    },
    Command {
        name: "audit",
        run: |args, corpus| audit::run(args, corpus),
        help: AUDIT_HELP,
    },
    Command {
        name: "decision",
        run: |args, corpus| decision::run(args, corpus),
        help: DECISION_HELP,
    },
    Command {
        name: "compare",
        run: |args, corpus| match (args.first(), args.get(1)) {
            (Some(a), Some(b)) => compare::run(a, b, corpus),
            _ => {
                eprintln!("usage: req compare <a.yml> <b.yml>");
                2
            }
        },
        help: COMPARE_HELP,
    },
    Command {
        name: "canon",
        run: |args, corpus| match args.first().map(String::as_str) {
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
        },
        help: CANON_HELP,
    },
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
    // The tool's own flags, answered in the same position as the subcommand lookup below and
    // for the same reason: they belong to no subcommand, and they must work **anywhere** —
    // including a directory with no `claims/` ancestor — so they are answered *before* the
    // corpus root is resolved and never print a `corpus root:` line. `--help` is deliberately
    // not folded into the empty-`rest` path above: saying nothing is a usage error, whereas a
    // request for help is a request that was answered.
    match rest[0].as_str() {
        "--version" | "-V" => {
            print!("{}", version_text());
            exit(0);
        }
        "--help" | "-h" => {
            println!("{USAGE}");
            exit(0);
        }
        // `init` *creates* the root, so it is answered here beside the two flags and for the
        // stronger version of their reason: resolving a corpus would answer a question this
        // command is about to make true, and in a directory with no corpus it would fail
        // before doing the one thing it was asked to do. It is deliberately not an entry in
        // `COMMANDS`, whose handlers all take a resolved root — which is why its page is
        // answered here too, with the same mechanism and for the same reason: reaching
        // `init::run` with `--help` would answer a help request as a usage error.
        "init" => {
            rest.remove(0);
            // `init` answers `--help` only for an invocation that is otherwise valid: a
            // positional argument and `--root` are usage errors, help flag or not — `init`
            // takes neither, the page says so, and a user who got the invocation wrong is
            // exactly the one who needs the error rather than a page.
            let only_help =
                !rest.is_empty() && rest.iter().all(|arg| arg == "--help" || arg == "-h");
            if cli_root.is_none() && only_help {
                println!("{}", INIT_HELP);
                exit(0);
            }
            exit(init::run(&rest, cli_root.as_deref()));
        }
        _ => {}
    }
    let cmd = rest.remove(0);
    let args = rest;
    // Looked up before the root is resolved, so a typo is a usage error (exit 2) rather than
    // a root lookup reported as UNVERIFIABLE (exit 3) — and looked up in the same table that
    // dispatches below, so the two cannot disagree about which subcommands exist.
    let Some(command) = COMMANDS.iter().find(|command| command.name == cmd) else {
        eprintln!("unrecognized subcommand: {cmd}\n{USAGE}");
        exit(2);
    };

    // A help request is answered here, beside `--version`/`--help` and before the root is
    // resolved, because reading help must not require a corpus — it is exactly what you read
    // to learn how to create one. This is also where the page is *looked up*: the verb and its
    // help are one entry, so a verb cannot exist without a page, and the answer cannot describe
    // a different command from the one that will run.
    if help_requested(&args) {
        println!("{}", command.help);
        exit(0);
    }

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
    exit((command.run)(&args, &corpus));
}
