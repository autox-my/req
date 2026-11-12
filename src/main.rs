//! req — the semantic, git-native requirements engine.
//!
//! Subcommands:
//!   req [--root <dir>] bench [corpus.yml] — run the FMR gate (default: bench/corpus/parity.yml)
//!   req [--root <dir>] canon               — the derived parse and each pin's verdict, per claim
//!   req [--root <dir>] compare A.yml B.yml — what kind of change is this between two claims?
//!   req [--root <dir>] diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]
//!                                          — what changed in meaning between two revisions (ADR 0026)
//!   req [--root <dir>] pin --verify <file> | --drift <file> --against <rev>
//!                                          — the corpus is exactly the commit a consumer pinned, or how
//!                                            it moved since (ADR 0033)
//!   req [--root <dir>] select k=v [...]    — which claims apply in this variant? (tri-state)
//!   req [--root <dir>] render [--format text|json] k=v [...]
//!                                          — the spec view for that selection
//!   req [--root <dir>] trace [--format text|json] k=v [...]
//!                                          — the traceability view, with derived incoming edges
//!   req [--root <dir>] impact <claim-id>   — what depends on that claim, transitively
//!   req [--root <dir>] gate [--format text|json] k=v [...]
//!                                          — may this selection ship? (policies over applying claims)
//!   req [--root <dir>] check [--test-report F]... [--format text|json]
//!                                          — may this repository ship? every declared variant and
//!                                            projection, folded into one verdict (ADR 0023)
//!   req [--root <dir>] propose <a.yml> <b.yml> [--provider-cmd <cmd>]
//!                                          — one pair put to a provider: an unratified proposal
//!   req [--root <dir>] baseline --at <ts> [--out F] — emit the canonical manifest a human signs (ADR 0016)
//!   req [--root <dir>] baseline --verify F — verify a signed Baseline: drift, authority, trust,
//!                                             signature, with the key judged at its signed_at
//!   req [--root <dir>] audit [--baseline F] [--format text|json]
//!                                          — the audit packet: coverage, provenance, undecided
//!   req [--root <dir>] decision [--fill]   — the decision log and its hash chain (step 8)
//!   req [--root <dir>] satisfy [--format text|json] | --record <element> <claim-id>
//!                                          — design satisfaction links, judged by direction (ADR 0040)
//!   req init                               — create a corpus in the working directory
//!   req selftest                           — known-answer checks of this binary; reads no corpus (ADR 0034)
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
    audit, baseline, bench, canon, check, compare, decision, diff, gate, impact, init, next_id,
    pin, propose, root, satisfy, select, selftest, view,
};
use std::path::Path;
use std::process::exit;

/// One line per shape of invocation, matching the module doc above. Printed both when no
/// subcommand was given and when one was not recognized, so the two cannot describe
/// different tools — the old fallback listed three subcommands and never mentioned `--root`.
const USAGE: &str = "\
usage: req [--root <dir>] <command> [args]

  bench [corpus.yml]              the FMR gate (default: bench/corpus/parity.yml)
  canon                           the derived parse and each pin's verdict (diagnostics)
  compare <a.yml> <b.yml>         what kind of change is this between two claims?
  diff --base <rev> [...]         what changed in meaning between two revisions (ADR 0026)
  pin --verify <file>             the corpus is exactly the commit a consumer pinned (ADR 0033)
  pin --drift <file> [...]        how the corpus moved since the pin, per claim and direction
  select | render | trace | gate k=v [...]
                                  the variant projections and the release gate
  check [--test-report <file>]    may this repository ship: every declared variant and projection,
                                  folded into one verdict (ADR 0023)
  impact <claim-id>               what depends on that claim, transitively
  satisfy [--record <el> <id>]    do the design elements still satisfy their claims (ADR 0040)
  next-id [--count N]             the next free claim id(s); --reserve holds them (parallel authors)
  init                            create a corpus in the current directory (no root lookup)
  selftest                        known-answer checks of this binary; reads no corpus (ADR 0034)
  propose <a.yml> <b.yml>         one pair put to a provider: an unratified proposal (ADR 0021)
  baseline --at <ts>              the manifest a human signs (--at is the signing time)
  baseline --verify <file>        verify one (a v3 manifest's signed_at judges the key)
  audit [--baseline <file>]       the audit packet: coverage, provenance, undecided
  decision [--fill | --verify]    the decision log and its hash chain
  --version | -V                  the versions that decide a verdict, and the build identity
  --help | -h                     this text, or <command> --help for one command's page

The corpus root is resolved at runtime: --root, else REQ_ROOT, else the nearest ancestor of
the working directory holding claims/ (ADR 0012). The chosen root is printed once to stderr;
with no corpus the answer is [UNVERIFIABLE] and exit 3 — never an empty result.

A help flag wins wherever it appears among a command's arguments, and --version, --help and
<command> --help are answered before any root is resolved: exit 0, no root line, in any
directory. Bare `req`, an unknown subcommand and an unknown flag are exit 2.

Exit codes are tri-state everywhere, plus usage: 0 settled affirmatively, 1 a definite finding
(a defect, a mismatch, a stale projection), 3 UNVERIFIABLE — it cannot be told, which blocks
and is never folded into a pass — and 2 usage. A command's page carries its own `exit:` line.

`docs/adopting.md` for the loop.";

/// What `--version` prints: the crate's version on the first line — single-line and greppable,
/// so `req --version | head -1` behaves like any other tool's — then every version axis this
/// build *judges* by, then the build identity.
///
/// Every value is read from the constant that carries it (`env!("CARGO_PKG_VERSION")`,
/// [`canon::RULES_V`], [`baseline::BASELINE_VERSION`], [`decision::DECISION_VERSION`],
/// [`canon::claim_schema()`]), never re-typed here: a literal `5` in this file would be a second
/// copy of `RULES_V`, free to drift from the value that decides every derived address.
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

/// Whether one argument asks for a page: `--help` and `-h` are one request.
///
/// One definition rather than the literal spellings repeated at each call site, so a future
/// spelling cannot reach the table's answer and miss `init`'s (or the reverse).
fn is_help_flag(arg: &str) -> bool {
    arg == "--help" || arg == "-h"
}

/// Whether any argument asks for a page.
fn help_requested(args: &[String]) -> bool {
    args.iter().any(|arg| is_help_flag(arg))
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

// The pages. Each is the command's reference and the one home of its contract: the exit codes
// and the rules that decide them are the payload a usage line cannot carry. There is no second
// copy to agree with (M2-3 retired `AGENTS.md` §Commands), so a rule changed here is changed
// everywhere; `tests/cli_version.rs` pins the footer and each contract that moved onto a page.

const BENCH_HELP: &str = "\
usage: req bench [corpus.yml]

Runs the benchmark gate over a corpus — the seed parity corpus by default — and records the
result under bench/results/<corpus-stem>-rules-v<N>.json. Each labelled pair is the oracle: a
false merge is the failure the gate exists to catch, so the record is written even when the run
fails, and results are append-only — one record per corpus and rules version.

`req bench bench/corpus/direction-v1.yml` scores the direction gold set (`req compare`): its
hard gate is dangerous confusions = 0 (a loosening reported as no change, or as the opposite
direction — the direction analogue of a false merge), accuracy is reported against a 0.95 soft
target, and a misspelled `direction:` label fails the gate rather than switching the check off.
The record's `direction.false_carry` reports, per weakening class (modality_weakening,
numeric_loosening, membership_widening), the pairs labelled a non-weakening class but reported as
it (ADR 0040), the support behind that count, and `admitted` (count 0 and support >= 100) — reported, never a verdict.

exit: 0 the gate passed (no false merge), 1 a false merge or a direction confusion, 3
UNVERIFIABLE (an unreadable corpus, a degraded run that found nothing, or a vacuous
denominator). A run that found a defect is 1 even when it was degraded.

`docs/adopting.md` for the loop.";

const CANON_HELP: &str = "\
usage: req canon [--fail-on-flag]

Prints, per claim, the parse its `statement` derives — structured or unstructured, modality,
subject, operator, value, unit, and the lints the canonicalizer raised — and each pin's
verdict. Nothing derived is stored in a claim file (`req.claim/2`, ADR 0022): the parse, the
canonical form and the address are computed when the claim is loaded, so there is nothing to
fill and nothing to verify; `--fill` and `--verify` are removed and answer a usage error.

A pin (`pins:` — modality, subject, operator, value, unit) states what the statement must parse
to. It agrees, or it is a flag: a *contradiction* (the parse says something else, named) or
*not carried* (the parse holds no such value). There is no third, non-flagging category. A
flagged pin moves no address — it reports here, and `req gate` fails on it (`pins_hold`). A
file whose declared `schema:` is not the one this build writes (`req --version` reports it as
`claim_schema`) is NOT JUDGED: an older one is a migration candidate
(`tools/migrate_claims.py`), a newer one is a question for a build that writes it, and a value
of no `req.claim/<n>` shape is a broken file. A `req.claim/2` file carrying a removed
`req.claim/1` key is a parse error that names the key.

`--fail-on-flag` is the opt-in for the window between \"pinned\" and \"every claim has a linked
test\", when `req gate` cannot yet be green: the output is unchanged, and the exit is 1 when any
pin is flagged. It looks at every claim, not only the ones a variant applies, so a wrong pin
behind a selector cannot hide. Its green is not a release verdict: tests, relations and
decisions were not judged, and `req gate` still decides the release.

exit: 0 diagnostics reported — not a pass: it reports, and it never gates; with `--fail-on-flag`,
0 means no pin is flagged and 1 means at least one is; 3 a registry or claim file that cannot be
read (or is NOT JUDGED); 2 usage, including `--fill` and `--verify`.

`docs/adopting.md` for the loop.";

const COMPARE_HELP: &str = "\
usage: req compare <a.yml> <b.yml>

Classifies the direction of change between two claims — modality weakening, numeric
tightening or loosening, polarity flip, scope divergence — from their canonical forms, and
says whether the change permits more or less. A condition added (`scope_narrowing`) makes a
SHALL, SHOULD or SHALL NOT bind less often, so it permits more; on a MAY no direction is
claimed. A change on several axes has a direction only when every axis that moved agrees; the
others are named after `also:`, and a change whose axes point different ways is `scope_divergence`.
Under a SHALL NOT the bound's senses swap (ADR 0044). A classification is for a human to ratify,
never an approval.

exit: 0 classified, 3 UNVERIFIABLE (a side that will not load, or that declares a file
format this build does not write, so its parse is NOT JUDGED; a pair with no structured form
to compare), 2 usage.

`docs/adopting.md` for the loop.";

const SELFTEST_HELP: &str = "\
usage: req selftest

A set of known-answer checks of this binary, the first thing to run after installing it or on a
machine nobody has run req on. The answers are compiled in: a SHA-256 test vector; what a handful of statements must
derive (written from the hashing spec: `within 200 ms` is LE, 0.2, s; `less than` is LT, a different
claim; a prohibition; an enumerated set); pairs that must share an address and pairs that must not;
address pins frozen at this build's rules version; the direction of six hand-labelled changes; and the
seed parity corpus through the false-merge gate. It reads no corpus, resolves no root and needs no
network, so it answers in any directory, and `--root` is a usage error (there is nothing for it to name).

What it tells you: this build, here, derives what the project's reference build derives. What it does
not: it is not a defence against a tampered binary (one that lies about its own checks passes them), and
it does not show the rules are right, only that they are the ones the reference answers were taken from.
After a change of the Tier-1 rules, the address pins are regenerated in the same commit; a build whose
pins belong to another `rules_v` does not compile.

It prints the build it checked (`rules_v`, `baseline_envelope`, `claim_schema`, `built from`), one
`PASS`/`FAIL` line per check (a failing line says what was derived and what was expected), then
`info:` lines for `git` and `minisign`, which the revision and Baseline commands need; these are
notes, never counted as checks.

exit: 0 every check passed; 1 at least one failed; 3 UNVERIFIABLE (the answers compiled into this build
cannot be read: a build defect); 2 usage (any argument, or `--root`).

`docs/adopting.md` for the loop.";

const PIN_HELP: &str = "\
usage: req pin --verify <file>
       req pin --drift <file> --against <rev>

A repository that is gated against a corpus kept elsewhere pins the commit it means. The pin file is
two lines, `repo: <label>` (optional, printed as given) and `rev: <40 hex>` (the full commit SHA of
the corpus; a short SHA is ambiguous and a branch or tag moves); blank lines and `#` comments are
allowed and anything else is refused. `--root` (or `REQ_ROOT`) names the corpus checkout.

`--verify` says whether the corpus at the root is exactly that commit: `HEAD` equals the pin, and
`claims/` and `aliases/` hold no uncommitted or untracked change (a gate would judge something other
than the pin). It prints `corpus pinned: <repo> @ <sha>` first, so a gate log names the policy it
judged; run it before `req gate`. req never fetches: the consumer's CI gets the commit into the
checkout (`git fetch`, `git checkout <sha>`), and a pinned commit that is not there is UNVERIFIABLE.

`--drift` is `req diff --base <pin> --head <rev>` in the corpus's repository, introduced by a
`pin drift:` line: each claim that changed gets its class (added, removed, weakened, tightened,
reshaped, …) and flags, and identical claims are counted. It is a description, never a verdict, and
it does not need the checkout to be at the pin; `<rev>` is any revision of that repository
(`origin/main` after a `git fetch`). See `req diff --help` for the classes and what is read.

`git` is run as a local process with every `GIT_*` redirection, user and system configuration
removed; nothing is fetched and no hook runs.

exit: 0 verified, or a drift report was produced; 1 only with `--verify`, when the corpus is not
the pinned commit (ahead, behind or diverged) or `claims/` or `aliases/` has an uncommitted change;
3 UNVERIFIABLE (the pin file is unreadable or malformed, the pinned commit is not in the checkout or
is not a commit, `git` is missing or failing, the corpus is not inside a git work tree, or a drift
revision does not resolve or a side does not load); 2 usage.

`docs/adopting.md` for the loop.";

const DIFF_HELP: &str = "\
usage: req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]

What changed in meaning between two revisions of this corpus? `--base` is a git revision;
`--head` is a git revision, or the working tree when absent (an uncommitted edit is read).
Claims are matched by claim_id; each id on either side gets one class: `added`, `removed`,
`weakened` (strictly more complies: modality weakening, numeric loosening, membership
widening, a condition added to a SHALL, SHOULD or SHALL NOT), `tightened` (the reverse),
`reshaped` (a different obligation: polarity flip, scope divergence, unrelated, a condition
changed on a MAY, a change on several axes that point different ways), `not-judged` (the meaning changed but a side has no structured
form), `reworded` (the text changed, the meaning fingerprint did not) or `metadata` (only
relations, rationale, tests, evidence or pins changed). Identical claims are counted, not
listed. A claim also carries flags: `pins-changed` (the authored pins differ, structurally),
`pin-flagged` (a head-side pin disagrees with the parse), `tests-lost` and `tests-added`
(linked test ids), and `same-address-as <id>` on an added claim whose meaning fingerprint a
removed claim had (a candidate renumber, never inferred as a rename). `registry: changed`
is reported once, at the top: a changed `aliases/subjects.yml` can move many addresses. A change
on several axes is named beside its class (`also: ...`), and `--format json` lists every moved axis
in `axes` (ADR 0044).

A `contradictions:` section follows the claims (ADR 0045): the structural contradictions the
change added or resolved, each with its `.req/variants` line, both ids, the slot and the two
values. Contradiction belongs to a selection, so it is judged once per line of the **head**
side's `.req/variants` (the working tree's, or `--head`'s blob; no list is the bare selection),
on both sides with that line, so a renamed line does not make old pairs new. *Added* is
unrecorded at head and not unrecorded at base; *resolved* is the reverse (gone, or recorded as
a `conflicts-with` Conflict at head). Each claim in an added pair carries the flag
`contradiction-added <other-id>`, and an otherwise unchanged one is then listed. While the
detector is not admitted the pairs are marked `(reported, not judged)` and no flag is raised.
`--format json` adds `contradictions: {added, resolved}`.

Each side is read from git without checking anything out: only `claims/` and `aliases/`, as
raw blobs, into a temporary directory that is removed on every exit, then loaded by the same
loaders every command uses and derived under this binary's rules and that side's own
registry. `git` is run as a local process with `GIT_*` redirections, user and system
configuration removed; nothing is fetched, no hook runs, and the base revision must be in
the checkout (a shallow clone needs `fetch-depth: 0`). The corpus must be inside a git work
tree; its direction quality is exactly `req compare`'s.

`--fail-on <list>` names classes that turn the run into exit 1: a comma-separated subset of
`weakened`, `removed`, `reshaped`, `not-judged`, `pins-changed`, `pin-flagged`, `tests-lost`,
`contradiction-added`. Nothing is on by default; an unlisted class is still printed. `--format json` prints one
`req.diff/1` document, the same classes as the text.

The output is a description, not a verdict: it never says PASS or FAIL, and `weakened` is a
prompt for a human, not a rejection.

exit: 0 the report was produced (whatever it lists); 1 only with `--fail-on`, when a named
class or flag is present; 3 UNVERIFIABLE (`git` missing or failing, the corpus not inside a
git work tree, a revision that does not resolve, a side whose claims or registry do not
load, including a claim declaring a file format this build does not write, or an empty
`claims/`, or a head `.req/variants` that cannot be read, naming its line); 2 usage (a missing
`--base`, a revision beginning with `-`, an empty or unknown `--fail-on` class).

`docs/adopting.md` for the loop.";

const SELECT_HELP: &str = "\
usage: req select key=value [key=value ...]

Reports which claims apply in a variant, tri-state: a claim whose applicability predicate
the context does not decide is UNKNOWN, never excluded — that would drop a safety claim out
of a variant silently. Selection changes no address: a variant is a projection over one
model. The predicate grammar is deliberately tiny — `key == value` and `key >= number`;
anything else is UNKNOWN, with its text shown.

exit: 0 every claim decided, 3 at least one claim left undecided, 2 usage.

`docs/adopting.md` for the loop.";

const RENDER_HELP: &str = "\
usage: req render [--out <path>] [--check <path>] [--format text|json] [key=value ...]

Prints the spec view for a selection: every applying claim's address, modality, statement,
selector and outgoing relations, with the undecided claims in their own section and the
outside-the-selection ones named. It is a projection: it reads claims, never writes one.

`--out <path>` commits the projection: the same view, preceded by an HTML-comment header naming
the generator, rules_v, selection, applying-claim count and an identity digest over the sorted
addresses. `--check <path>` regenerates the projection and compares bytes — it answers a
question and writes nothing; a committed projection is checked, never hand-edited. The two are
one question each: give one of them, once. The file is complete even when the plain command's
verdict is open: the exit class of `--out` belongs to the write, not to the verdict. A stale
`--check` names the first differing line (regenerate with `--out`, never edit), and an explicit
context beside it must name the selection the header records or it is a second question (3).

`--format text|json` selects the rendering (default `text`). `--format json` prints the same view
as a `req.render/1` document on stdout: schema, generator, `rules_v`, the selection, the three
sections and the identity digest. It is stdout only — `--out`/`--check` are text projections, and
pairing either with `--format json` is a usage error (exit 2).

exit: 0 settled (plain), written (--out), or current (--check); 1 a committed projection is
stale or was hand-edited (--check); 3 anything an undecided claim leaves open (plain), a
projection that cannot be written (--out), or one this build cannot judge — no header, another
build's rules_v, an unreproducible selection, or another command's projection (--check); 2 usage.

`docs/adopting.md` for the loop.";

const TRACE_HELP: &str = "\
usage: req trace [--out <path>] [--check <path>] [--format text|json] [key=value ...]

Prints the traceability matrix for a selection: the outgoing edges, the **derived** incoming
edges (each relation is stored once, on the outgoing side), and the findings. Every edge that
leaves the selection is named — to an excluded claim, or to one this variant leaves undecided
— and a relation to a claim the corpus does not hold is named UNVERIFIABLE, never ignored.
It is a projection: it reads claims, never writes one.

`--out <path>` commits the projection with the same header `render --out` writes, naming this
generator (`req trace`). `--check <path>` regenerates and compares bytes, and writes nothing; a
committed projection is checked, never hand-edited. A projection made by the other command is
UNVERIFIABLE here rather than stale: two commands are two artefacts. The file is complete even
when the plain command's verdict is open: the exit class of `--out` belongs to the write, not
to the verdict. A stale `--check` names the first differing line, and an explicit context beside
it must name the selection the header records or it is a second question (3).

`--format text|json` selects the rendering (default `text`). `--format json` prints the same
matrix as a `req.trace/1` document on stdout: schema, generator, `rules_v`, the selection, the
edges, the derived incoming edges and the findings. It is stdout only — `--out`/`--check` are
text projections, and pairing either with `--format json` is a usage error (exit 2).

exit: 0 the edge graph is settled (plain), written (--out), or current (--check); 1 a committed
projection is stale or was hand-edited (--check); 3 an edge into a claim the corpus does not
hold or into one the variant leaves undecided leaves the matrix open (plain), a projection that
cannot be written (--out), or one this build cannot judge — no header, another build's rules_v,
an unreproducible selection, or another command's projection (--check); 2 usage.

`docs/adopting.md` for the loop.";

const GATE_HELP: &str = "\
usage: req gate [--format text|json] [--test-report <file>]... [key=value ...]

Runs the eight release policies over the claims a variant leaves applying — pins hold, relations
resolve, dependencies decided, rationale present, design satisfied (ADR 0040), contradictions
(ADR 0043), and (with `--test-report <file>`) tests linked — and confirms the invocation itself. A claim the invocation leaves undecided was
not examined, and that is not a pass: a bare gate on a corpus carrying a selector is
UNVERIFIABLE.

`pins_hold` fails on any pin of an applying claim that the parse derived at load does not hold
— a contradiction, or a value the parse does not carry (ADR 0022). With no pin in the selection
it passes and says so: it ran over zero pins, it was not skipped.

`selectors_confirmed` judges the invocation: a `key=value` entry no applicability predicate in
the corpus mentions decided nothing, and an `==` entry whose value the corpus never declares is
refused, naming the values it does; a `>=` key is confirmed by comparison. A real variant the
corpus does not describe is refused on purpose — the gate clears only what the model describes,
and the friction belongs at the decision boundary (ADR 0006).

`rationale_present` also checks a `rationale.source` that is a repository path with a line anchor
(`docs/plan.md#L390`, `#L390-L395`): the file must exist, be a file and have that many lines, else the
policy FAILS naming the claim; a file that exists but cannot be read is UNVERIFIABLE. Prose, URLs, absolute
or `..` paths, heading anchors and a path without an anchor are not judged (a line count cannot tell whether a
claim still matches the text it was written from; that stays a human reading). Paths are looked up from the
corpus root and, if it sits in a subdirectory of a git checkout, from the checkout's root.

`design_satisfied` (ADR 0040) judges the satisfaction links `req satisfy` lists — a design element's
`req-satisfies: <claim-id>` against the statement a human recorded in `.req/satisfied` — for the applying
claims, plus any link naming a claim the corpus does not hold. PASS when every link holds or holds
weakened under an admitted class (the line names the counts: links judged, holding, weakened); UNVERIFIABLE
when one is suspect (the claim changed in a direction that is not carried), unratified (never recorded) or
names an unknown claim. It never FAILs — a suspect link may still comply; nobody has shown it — and with no
`.req/design` it passes over zero links and says so.

`contradictions` (ADR 0043) judges the applying claims of the selection against each other, and the
Conflicts a human recorded between them. Two claims are *comparable* when they share subject, action,
qualifier, unit, exception and temporal, one's conditions are inside the other's, and both are `SHALL` /
`SHALL NOT`; they *contradict* when no value satisfies both (a `SHALL` against a `SHALL NOT` of the same
obligation, disjoint bounds — strictness at the meeting point counts — or disjoint or forbidden membership), decided on exact
decimals. FAIL names both claims, the slot and the two values, unless a human recorded the pair as a
Conflict (`relations: [{type: conflicts-with, to: REQ-X}]`, on either claim): a recorded Conflict between two
applying claims is UNVERIFIABLE, not a defect — information, but a release cannot be shown to satisfy both
sides — and so is a `conflicts-with` naming a claim the corpus does not hold. Resolve one by an edit, a
retirement or an applicability predicate that keeps the two out of the same release, never by a flag. PASS
says what was examined (\"no structural contradiction among *k* comparable pairs; *m* pairs not comparable\")
and never that the corpus is consistent: a claim the parser leaves unstructured, a `SHOULD` or `MAY`, or a pair
whose slots differ is counted as not comparable, never guessed. `checked` is *k*. Claims that apply in different
`.req/variants` lines are never compared.

`tests_linked` answers four ways over machine-evidenced claims, and there is no fifth: PASS (a
linked test passed), FAIL (every linked test is in the report and none passed — a skipped test
is not a pass, and a failed one outranks a passing one on the same claim), UNVERIFIABLE *no
linked test*, and UNVERIFIABLE *linked test not in the report* (a renamed or deleted test is
never a pass). An absent, unparsable or zero-testcase report is UNVERIFIABLE once, not per
claim. Without `--test-report` the policy cannot be exercised and the gate is UNVERIFIABLE. A
claim declaring `evidence: process` carries no `tests:` and is answered PASS as declared,
listed under `process_evidenced` — a reported fact, not machine proof and not a fifth outcome;
an empty `tests:` never implies it, and when every applying claim declares it no report is
needed.

`--test-report` may be given more than once (a project with several stacks passes one report each,
`--test-report a.xml --test-report b.xml`): the gate judges the **union** of the reports. A test
id in two reports with the same outcome counts once; with different outcomes the evidence is
unusable and the message names the id and both files. One report that cannot be read or parsed
(a pytest collection error among them) makes `tests_linked` UNVERIFIABLE, once, naming that file,
because a test absent from the union might be in the one that failed to load.

`--format text|json` selects the rendering (default `text`). `--format json` prints the same
answers as a `req.gate/2` document on stdout: schema, generator, `rules_v`, the selection, the
folded verdict, the eight policies (`pins_hold` also carries `checked`, the number of pins it judged,
so a pass over zero pins is told from one that held some; `rationale_present` carries it too, as the number of source line anchors it judged; `design_satisfied` carries it as the number of links; `contradictions` carries it as the comparable pairs examined; `tests_linked` carries `counts`, below) and the undecided claims with what each is missing. The
verdict is the same fold the exit code applies, so the document and the process cannot disagree.

`tests_linked`'s `counts` (also printed as a `counts:` line under the policy in the text report) are the
applying claims by evidence: `applying`, of which `process_evidenced` (declared) and the rest machine-evidenced,
split into `linked` (at least one `tests:` entry) and `unlinked`, so `applying = linked + unlinked +
process_evidenced`. A linked claim is `passed`, `failed` (a linked test failed, or every linked test was skipped)
or `not_in_report` (a linked test the report lacks, with no failure), once each. Those three are `null`, and
the text says `not judged`, when no usable report was supplied: a claim that was not judged is not counted as
judged.

exit: 0 every policy passed and every claim decided, 1 a defect (a flagged pin among them), 3
something that cannot be confirmed (a relation that does not resolve, a dependency left
undecided, an entry the corpus cannot confirm, an empty selection) or any claim left undecided — named
with the remedy; 2 usage. Fail outranks Unverifiable, and an undecided claim outranks a pass.

`docs/adopting.md` for the loop.";

const CHECK_HELP: &str = "\
usage: req check [--test-report <file>]... [--format text|json]

May this repository ship? Reads the two release lists beside the corpus — `.req/variants` and
`.req/projections` — runs one step per line and folds them into ONE verdict, so an adopter's
gate is this command and not a shell loop that has to get the fold right.

`.req/variants`: one complete release context per line, `key=value` words, run as `req gate
--test-report <file> <line>`; the test reports are read once and shared. `--test-report` may be
repeated, and every gate step judges the union of the reports (the gate's rule: an id with
different outcomes in two, or one report that cannot be read, is UNVERIFIABLE, naming the
file). `.req/projections`: one
repo-relative path per line, run as `req render --check <path>` (or `req trace --check <path>` for a trace projection: the
generator the file's header names decides, and each is judged under the selection its header records). The grammar is the same for
both: `#` starts a comment, an empty line is skipped, a trailing CR is stripped, a line is split
on blanks with no pathname expansion (a `*` in a value is data), and a final line without a
newline counts. A variants line is never an option, a word without `=` or an empty key; a
projections line is exactly one repo-relative path (no absolute path, no `..`). Both lists are validated before anything runs: a
malformed line is exit 2 naming `<file>:<line>`, and nothing was gated.

A step's exit is the command's exit — the same code runs, no policy is copied and nothing is
shelled out — and the output says which step produced which exit. The fold is the gate's:
a defect (1) outranks something that cannot be confirmed (3), which outranks a pass (0). Each variants
line therefore carries the gate's eight policies, `contradictions` among them, over **that line's** applying claims (ADR 0043):
two claims that apply in different lines are never compared.

Absence is not silence. No `.req/variants`, or one with no line in it, is the default variant
(one bare gate) and says so on its own line; on a corpus carrying a selector that gate is 3, so
a deleted list cannot turn green. A list that exists and cannot be read as a regular file (a
directory, a dangling link, a FIFO, a mode-000 file) is 3, never \"absent\". No
`.req/projections` checks no projection and says `projections: none pinned`; a listed path that
does not exist or is not a regular file is 1 (a projection that vanished is silent decay; this is the
one step whose exit differs from `render --check`'s, which can only say 3), and a present list that yields
no path is 1.

One more step always runs: `completeness` (ADR 0039), the structural omissions req can prove from
these files. `in_no_release` — a claim every declared release excludes — is 1. `in_no_baseline` — a
claim a release ships that no signed Baseline under `baselines/` names (a manifest counts only with a
signature beside it, `.minisig` or a party's `.<key-id>.minisig`; verifying it is `req baseline --verify`'s) — is 3, so a repository
with no signed Baseline is 3. `unrelated` — a claim with no relation in or out — is listed and changes
nothing, unless `.req/completeness` says `strict unrelated` (then 1). `need design` in that file adds
`unsatisfied` (ADR 0040) — a claim a release applies that no design element declares `req-satisfies:`
for — which is 3. That optional file lists, one per line, root claim ids `unrelated` spares,
`strict unrelated` and `need design`; an unknown word or an id the corpus does not hold is exit 2 naming
`<file>:<line>`. A malformed `.req/design` line is exit 2 too. A claim a line leaves undecided is that gate's 3, never named
here. Nothing says the requirements are complete: req holds nothing to be complete against.

`verdict: PASS` is printed only when every declared step ran and passed. There is no `--only`,
no `--skip` and no flag that narrows what is examined. `--format json` prints one `req.check/1`
document on stdout — the folded verdict and, per step, its kind, its line or path, its exit and,
for a gate step, the whole `req.gate/2` document, and for the completeness step its lists (three, and `unsatisfied` with `need design`) —
with the same fold and the same exit as the text.

exit: 0 every declared step ran and passed; 1 a defect in any step (a gate that fails, a stale or
missing projection, a list that pins nothing, a claim no release examines); 3 UNVERIFIABLE (a step
that cannot be confirmed — an undecided claim, a missing test report, a list that exists and cannot
be read, a projection `render --check` cannot judge, a shipped claim no signed Baseline names); 2
usage or a malformed list, decided before any step runs.
Fail outranks Unverifiable.

`docs/adopting.md` for the loop.";

const SATISFY_HELP: &str = "\
usage: req satisfy [--format text|json]
       req satisfy --record <element> <claim-id>

Do the design elements still satisfy the claims they were recorded against (ADR 0040)? A design
element is any file under a root listed in `.req/design` (optional; one repo-relative directory or
file per line, `#` comments and blank lines skipped, an absolute path or `..` is exit 2 naming
`<file>:<line>`; directories are read recursively, symlinks and `.git` are not followed). It
declares a link with a line `req-satisfies: <claim-id>` — in a comment or front matter; req reads the
token, not the file format. The element's id is its repo-relative path.

`req satisfy` lists every declared link with its outcome. A link is judged by the *direction* of the
claim's change since it was recorded, both statements re-derived under the current rules:
HOLDS (same meaning fingerprint: a cosmetic edit, or none); HOLDS (weakened) (the change is a class
the measurement admitted — today `modality_weakening` and `numeric_loosening` — named, and the link is not re-opened);
SUSPECT (any other direction: a strengthening, a tightening, a polarity flip, a scope change, an
unclassifiable change, or a weakening class not admitted; UNVERIFIABLE, naming the element, the claim
and the direction); UNRATIFIED (declared, never recorded: UNVERIFIABLE); UNKNOWN CLAIM (a declared id
the corpus does not hold: UNVERIFIABLE); ORPHANED (recorded, no longer declared: listed, not a
finding). A file under a root that is not UTF-8, or a root that does not exist, is UNVERIFIABLE and
named. Scope changes are never carried (adding a guard and removing one are both suspect).

`req satisfy --record <element> <claim-id>` is the human act (ADR 0004): it writes or replaces that one
link in `.req/satisfied` (`schema: req.satisfied/1`, `links:` sorted by element and claim), storing the
claim's statement **verbatim** — no address, no rules version, no date, so a record survives a rules bump
and needs no git history. req never records a link on its own. It refuses a link no element declares and
a claim the corpus does not hold (exit 2), and an unreadable record (exit 3).

`--format json` prints a `req.satisfy/1` document: the links, each with its element, claim, outcome
(`holds`, `holds_weakened`, `suspect`, `unratified`, `unknown_claim`, `orphaned`), direction and line.
The release gate's `design_satisfied` policy and `.req/completeness`'s `need design` build on this.

exit: 0 every declared link holds (or there are none, said so), 3 UNVERIFIABLE (a suspect, unratified
or unknown-claim link, an unreadable design file or record), 2 usage or a malformed `.req/design`.

`docs/adopting.md` for the loop.";

const IMPACT_HELP: &str = "\
usage: req impact <claim-id> [key=value ...]

Walks the relations backwards (derived, never stored twice) and reports every dependent of a
claim nearest-first, with its state in the selection.

exit: 0 complete and settled, 3 UNVERIFIABLE (a missing target, an undecided target or
dependent, or a relation that does not resolve — an answer a broken relation could have
shortened says so instead of reading as complete), 2 usage.

`docs/adopting.md` for the loop.";

const NEXT_ID_HELP: &str = "\
usage: req next-id [--count N] [--prefix PREFIX] [--reserve]

Prints the next N free claim ids (default 1), one per line, so claims authored in parallel
worktrees do not collide. It is the highest existing number + 1: a gap left by a retired or never-used
number is not refilled (a number once seen in a review or a Baseline must not come to mean another
claim). It reads file names, not claim contents: `claims/<id>.yml` is the id, so it answers while a
sibling claim is half-written or fails a pin. The prefix is the corpus's own (`REQ` when it has
several, or none); `--prefix` chooses, and several prefixes without `--prefix` or `REQ` is a usage error.

`.req/reserved` lists blocks taken but not yet written, one id or one range per line
(`REQ-0010..REQ-0019`, `#` starts a comment); they count as taken. `--reserve` appends the block it
hands out, so the next author is given the next one. Two branches that reserve at the same time both
edit the end of that file, and the merge conflict is the signal. There is no locking across machines;
a reservation is a file in git. A file that cannot be read or understood is UNVERIFIABLE and prints no id.
Without `--reserve` nothing is written.

exit: 0 ids printed, 2 usage (including `--count` outside 1 to 10000), 3 UNVERIFIABLE (no `claims/`,
or `.req/reserved` unreadable, malformed or not writable).

`docs/adopting.md` for the loop.";

const PROPOSE_HELP: &str = "\
usage: req propose <a.yml> <b.yml> [--provider-cmd <cmd>]

Puts ONE explicit pair of claims to a Tier-2 provider and prints its answer as `proposal
(unratified): <label>`, with the provider command recorded verbatim. The provider proposes; a
human disposes (ADR 0004): it never writes a file and never changes an address. The command
(run through `sh -c`) is given `ID:`, `A:`, `B:`, `DIFF:` and `CHOOSE:` lines on stdin and prints
a label (equivalent, related, unrelated) or a JSON object with `choice` and `confidence`. Without
`--provider-cmd` there is no provider and the answer is UNVERIFIABLE — never \"unrelated\". The
corpus-wide funnel (blocking, gold set, flip rate) is research in `experiments/tier2/`, ADR 0021.

exit: 0 a proposal was produced, 3 UNVERIFIABLE (no provider, or one that failed, declined or
answered with no readable label, or a claim that will not load), 2 usage.

`docs/adopting.md` for the loop.";

const BASELINE_HELP: &str = "\
usage: req baseline --at <RFC 3339 UTC timestamp> [--out <file>] [key=value ...]
       req baseline --verify <file> [--signature <file>] [--key <id>] [--now <date>]

Emits the canonical manifest — the exact byte string a human signs — or verifies one: drift,
authority, trust and signature, with the key judged at the manifest's `signed_at` for a v3
envelope (today for v2, and `--now` sets the judgement time explicitly). The signing time is
authored, never invented, and req never signs.

`--out <file>` writes the manifest to a file instead of stdout, only after it is complete (a temp file,
then a rename): a refusal leaves any existing file untouched. Prefer it to a shell redirect, which
truncates the target before req runs, so a refusal leaves an empty file that `minisign -Sm` will sign.

Emission refuses (3) when the decision log is absent, unparsable or empty, and exits 1 when it
is tampered: a chain head that attests nothing must not be signed. Verification re-derives
every address under the selection the manifest records, so a claim edited after signing is
FAIL; an explicit context that contradicts the recorded selection is a usage error (exit 2),
never drift. A v3-or-later manifest also attests the decision log's chain head at a position:
a rewritten prefix or a truncated log is FAIL, decisions appended after it are not. A v4
manifest attests each claim's `tests_hash` (its evidence declaration and links); a v3 one says
once that it predates v4, so its tests are not attested — not a FAIL. A claim's `pins:` are not
attested by any version (ADR 0022): deleting one after signing still verifies. A manifest whose `rules_v`
differs from this build's is UNVERIFIABLE, not judged: an older one is re-signed, a newer one
is verified with a build that writes it. Trust is read from `baselines/trust.yml` (ADR 0010),
by calendar date: a key expired at the signing time, or revoked at or before it, is FAIL; one
revoked after it still verifies (revocation is prospective). The signing time is
signer-asserted, so revocation bounds newly asserted times only, and `valid_from` is recorded
but not enforced. A newer envelope's dated revocation stays UNVERIFIABLE, while an anchor whose
own `revoked_at` cannot be read fails closed on every envelope. The signature check is a process boundary to a pinned verifier.

Several parties may sign the same bytes (ADR 0042). `<file>.minisig` is the `--key` key's signature, as
always; every `<file>.<key-id>.minisig` beside it is that key's. A trust-file key may carry a `party`
and a `role` (the meaning of its signatures), and an optional `.req/signing` lists what a release
needs, one `<party> <role>` per line (a line repeated n times needs n distinct keys; `#` comments;
a malformed line is exit 2 naming `.req/signing:<line>`). Every signature present is judged as above
(one that does not verify, or a key revoked at the signing time, is FAIL; a file naming a key the
trust file lacks is UNVERIFIABLE, never ignored). With a policy every line must be met by a distinct
verified key holding that party and role: an unmet line is UNVERIFIABLE, `awaiting <party> <role>`
— not a defect and not a pass. With no policy one verified signature suffices, and a repository
with no `.req/signing` and no keyed file verifies exactly as before.

exit: 0 emitted or VERIFIED, 1 FAIL (drift, a signature or authority that does not verify),
3 UNVERIFIABLE (no verifier, an unknown key, no signature, a `rules_v` mismatch, an absent
decision log, or an `--out` target that cannot be written), 2 usage (emitting without `--at`, an
unreadable timestamp, `--at` on the verification path, `--out` on the verification path or with no
file, any other option-shaped argument such as `--out=file`).

`docs/adopting.md` for the loop.";

const AUDIT_HELP: &str = "\
usage: req audit [--baseline <file>] [--format text|json] [key=value ...]

Prints the audit packet for a Baseline: every applying claim's address, coverage state,
provenance and outgoing relations, then the undecided claims in their own section, then the
findings. Coverage is derived from the Baseline's drift; whether a signature is valid belongs
to `req baseline --verify`, and this packet never asserts authority it did not check. The
selection comes from the Baseline being audited. With no `--baseline` it still prints, and says
plainly that nothing is ratified — an audit that passed on an unsigned model would be worse
than none.

`--format text|json` selects the rendering (default `text`). `--format json` prints the same
packet as a `req.audit/1` document on stdout: schema, generator, `rules_v`, the selection,
whether anything is ratified, the coverage lists (`not_judged` as its reason string), the
findings in order, each carrying the class the text packet prints, and `unrelated`.

An applying claim with no relation in or out (minus the roots `.req/completeness` lists) is printed
under *Structural omissions* (ADR 0039), and so is an applying claim no design element declares a link
for when `.req/completeness` says `need design` (`unsatisfied`, ADR 0040; `req check` judges it
UNVERIFIABLE). It is reported and never part of the verdict; the release-wide omissions are `req check`'s.

exit: 0 every applying claim covered and still matching, 1 a defect (drift, or no
provenance), 3 nothing ratified, something uncovered, or coverage NOT JUDGED because the
manifest's `rules_v` disagrees with this build's, 2 usage.

`docs/adopting.md` for the loop.";

const DECISION_HELP: &str = "\
usage: req decision [--fill | --verify]

Walks the decision log's hash chain — one record per file, each committing to its
predecessor — or with `--fill` writes the derived `prev_hash`/`hash` into the records,
preserving comments and the note a human wrote. It never overwrites a record: a stored field
that disagrees with the content makes it refuse: it writes nothing at all, because recomputing
would launder the edit `--verify` reports as TAMPERED. A decision records the address it
ratified; an edit that moves the address is reported by `req audit` and `baseline --verify`,
not by this log, because a log is history and superseding an address later is not tampering.

A record may carry `decisions/DEC-NNNN.yml.<key-id>.minisig`, a signature over its canonical bytes
(ADR 0042). A verified one makes the listing say `signed by <id> (<party> <role>)`, and the record's
`role` must equal the key's: a mismatch, a signature that does not verify or a key revoked at the
record's date is FAIL (exit 1); an unknown key or no verifier is UNVERIFIABLE. A record with no
signature file is listed as *asserted*: its role is a claim, not a proof, and the verdict is unchanged.

exit: 0 VALID, 1 TAMPERED (a stored hash that does not recompute, or a `prev_hash` that
disagrees with the entry before it) or a signed Decision that FAILS, 3 UNVERIFIABLE (an unfilled
hash, an unparsable file, an empty log, a log that does not begin at the genesis anchor, or a
Decision signature that cannot be judged), 2 usage.

`docs/adopting.md` for the loop.";

const INIT_HELP: &str = "\
usage: req init

Creates a corpus in the current directory: a `req.claim/2` starter claim (authored fields and
pins, nothing derived) read back through the loader every command uses with its pins held, the
claims README, and the subject registry. It
refuses rather than overwrite an existing corpus, and it reads no root — it creates one, so it
announces none. It prints the next steps to stdout: the CI snippet, the
per-variant rule, and where a req bug is reported.

exit: 0 created, 1 refused (`claims/` or `aliases/subjects.yml` already exists, or `aliases`
is a symlink), 3 UNVERIFIABLE (the starter claim's pins do not hold, or the target cannot
be written — whatever it wrote is removed), 2 usage (an argument, or `--root`: run `init`
where the corpus should live).

`docs/adopting.md` for the loop.";

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
        name: "satisfy",
        run: |args, corpus| satisfy::run(args, corpus),
        help: SATISFY_HELP,
    },
    Command {
        name: "gate",
        run: |args, corpus| gate::run(args, corpus),
        help: GATE_HELP,
    },
    Command {
        name: "check",
        run: |args, corpus| check::run(args, corpus),
        help: CHECK_HELP,
    },
    Command {
        name: "next-id",
        run: |args, corpus| next_id::run(args, corpus),
        help: NEXT_ID_HELP,
    },
    Command {
        name: "propose",
        run: |args, corpus| propose::run(args, corpus),
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
        name: "diff",
        run: |args, corpus| diff::run(args, corpus),
        help: DIFF_HELP,
    },
    Command {
        name: "pin",
        run: |args, corpus| pin::run(args, corpus),
        help: PIN_HELP,
    },
    Command {
        name: "canon",
        run: |args, corpus| match args.first().map(String::as_str) {
            None => canon::run_claim_diagnostics_at(corpus),
            // The opt-in (issue #106): same diagnostics, exit 1 on a flagged pin. Exactly this one
            // argument — every other spelling or combination falls to the usage error below.
            Some("--fail-on-flag") if args.len() == 1 => {
                canon::run_claim_diagnostics_with(corpus, true)
            }
            // Removed in `req.claim/2` (ADR 0022): there is nothing stored to write or check.
            // A usage error that says why, rather than "unrecognized flag", because a script or
            // a habit written against `req.claim/1` is the likeliest caller.
            Some("--fill" | "--verify") => {
                eprintln!(
                    "{}\nusage: req canon [--fail-on-flag]",
                    canon::FILL_VERIFY_REMOVED
                );
                2
            }
            Some(other) => {
                eprintln!("unrecognized flag: {other}\nusage: req canon [--fail-on-flag]");
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
        // The flags are matched through the same predicate every subcommand's page uses, so a
        // spelling added there cannot reach `req gate <flag>` and miss `req <flag>`.
        flag if is_help_flag(flag) => {
            println!("{USAGE}");
            exit(0);
        }
        // `selftest` reads no corpus, so like `init` it is answered before the root is resolved: it must
        // work in a directory with no `claims/` above it, which is where a fresh install is. It takes no
        // argument and no `--root` (a root would name a corpus it does not read), and a help request
        // is answered only for an otherwise valid invocation, as `init` does.
        "selftest" => {
            rest.remove(0);
            let only_help = !rest.is_empty() && rest.iter().all(|arg| is_help_flag(arg));
            if cli_root.is_none() && only_help {
                println!("{}", SELFTEST_HELP);
                exit(0);
            }
            if cli_root.is_some() {
                eprintln!(
                    "usage: req selftest\n`--root` names no corpus here: selftest reads none"
                );
                exit(2);
            }
            rest.retain(|arg| !is_help_flag(arg));
            exit(selftest::run(&rest));
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
            let only_help = !rest.is_empty() && rest.iter().all(|arg| is_help_flag(arg));
            if cli_root.is_none() && only_help {
                println!("{}", INIT_HELP);
                exit(0);
            }
            // The help flag is not an argument `init` should blame: when the invocation is
            // otherwise invalid, drop it so `init::run` names the actual offender (`junk` in
            // `init --help junk`), not the request for a page.
            rest.retain(|arg| !is_help_flag(arg));
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
