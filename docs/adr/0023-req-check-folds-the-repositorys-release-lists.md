# 0023. `req check` reads the repository's release lists and folds them into one verdict

**Status: proposed** — the owner ratifies it by merging the pull request that carries it; issue #63 (opened
from the Sedia evaluation, re-scoped by the owner's comment after ADR 0022). Date proposed: 2026-11-04.
**Amends one clause of [ADR 0018](0018-documents-are-projections.md)** ("req itself never reads
`.req/projections`") and the matching sentence in `.req/variants`, `scripts/check.sh` and
`docs/adopting.md` ("req knows nothing about this file"). Nothing else in 0018 moves.

## Context

An adopter's release gate is `req gate` once per line of `.req/variants`, the test report those gates
read, and, when the project commits documents, `req render --check` once per line of `.req/projections`.
req does not read either list (ADR 0018, ADR 0015), so every adopter re-implements the loop **and the
tri-state fold** in shell. The reference implementation is 262 lines of YAML in
`examples/github-workflows/req.yml`, copied into `docs/adopting.md` and `req init`, and again in
`scripts/check.sh`. That shell is where this project's own review has found, release after release, the
defects that matter: an exit 3 folded into green, a swallowed exit code, a failed input redirection that
leaves the loop unrun, a last-line-without-newline release silently dropped, a bare gate that is a false
red beside green variants. They are the failure modes of **aggregation**, not of any one policy.

The fold is not a judgement call. `req gate` already defines it (Fail outranks Unverifiable outranks Pass,
ADR 0015) and the shell re-derives it by hand. A rule that is one line in Rust and sixty in bash belongs
where it can be tested once.

## Decision

1. **`req check [--test-report <file>] [--format text|json]` exists.** It reads two lists beside the
   corpus (the directory holding `claims/`, ADR 0012) — `.req/variants` and `.req/projections` — runs one
   gate per variant line and one projection check per path, and folds the results into **one** exit with
   the gate's contract: **1** if any step is a defect, else **3** if any step cannot be confirmed, else
   **0**; **2** for a malformed list or a usage error, decided before any step runs.
2. **The grammar is req's now, and unchanged.** It is the grammar both files already document and
   `scripts/check.sh` implements: `#` starts a comment, an empty line is skipped, a trailing CR is
   stripped, a line is split on whitespace with no pathname expansion, a final line without a newline
   counts. A variants line is `key=value` words, never an option; a projections line is exactly one path, relative to the corpus root (an absolute path or a `..`
   component is malformed: the list is repository content and must not point outside it).
   `req check` adds **no** key, no section and no third file. What a line *means* stays what it meant:
   a complete release context for `req gate`, and a path for `req render --check`.
3. **Absence is not silence.** No `.req/variants` declares the default variant — one bare `req gate` — and
   the output says so on its own line; on a corpus carrying a selector that gate is exit 3 (ADR 0015), so
   a deleted list cannot turn green. A file that exists and cannot be read as a regular file (a directory,
   a dangling link, a mode-000 file) is exit **3**: a pinned list that became unusable is never "absent".
   A comment-only list is the default variant, as `docs/adopting.md` already says. An absent
   `.req/projections` checks no projection and says `projections: none pinned`; a listed path that is
   missing is exit 1 (a projection that vanished is silent decay, ADR 0018); a present list that yields no
   path is exit 1.
4. **A step's exit is the command's exit.** A variant step exits exactly as `req gate --test-report <file>
   <line>` would, and a projection step exactly as `req render --check <path>` would; `check` calls the
   same code, it does not re-implement a policy or shell out. **One stated exception:** a listed projection
   path that does not exist or is not a regular file is exit 1 (§3), where `render --check` can only say it
   cannot read the file (3). The listing is what makes its absence a defect, and a FIFO is never opened. Its text and JSON say which step produced
   which exit; the folded exit never hides one.
5. **Its green is a release PASS only because every step ran.** `req check` prints `verdict: PASS` only
   when every declared step ran and passed, and it never prints PASS for a subset: there is no
   `--only`, no `--skip` and no flag that narrows what is examined. (A narrower question is
   `req canon --fail-on-flag`, issue #106, which says it is not a release verdict.)
6. **The JSON view is one document.** `--format json` prints `req.check/1`: the folded verdict, and per
   step its kind, its line or path, its exit and — for a gate step — the whole `req.gate/2` document. A
   consumer reads one document where the adopter template reads one per variant today. It is a stdout view
   only; beside `--out` it is a usage error, as for `gate`.

## What stays outside req

req still does not run tests, choose a test runner, write the JUnit report, sign anything, touch the
network, or read a file the two lists do not name. The CI template keeps what is CI's: install, the stale
report removal, the test run that writes the report, the artifacts. Moving the fold does not move the
evidence.

## Consequences

- The adopter template shrinks to install, tests and one `req check --test-report … ` step whose status is
  the step's status; its aggregation hazards (redirect, last line, `set -e`, option-shaped lines) are
  Rust-tested once instead of reviewed in three copies. `scripts/check.sh` calls `req check` for the same
  two lists, so this repository's own gate and an adopter's are one implementation.
- `.req/variants` and `.req/projections` gain a reader in req, so their grammar is now req's contract and a
  change to it is a change to req. That is the price; the files stay plain text beside the corpus and
  remain the repository's to author.
- The risk the old shell avoided by being dumb: a bug in `check` folds wrongly for everyone. The mitigation
  is the property test of the fold (every combination of step exits), the equality of each step's exit with
  the standalone command's, and `tests/release_gate.rs`, which keeps asserting the per-line exit-0
  property through `req gate` directly.
- Pins: `req check` adds no fingerprint-path code, so no `RULES_V` change and no re-signing.

## Rejected

- **Leave it to the template and fix the shell.** It has been fixed four times in review; each fix lives
  in three copies kept equal by a test, and the next adopter writes the fifth.
- **`req gate --all`.** It would put a release-list reader and a projection checker inside the command
  whose contract is one selection, and `gate`'s exit 0 would start to mean "all lists passed" or "one
  selection passed" depending on a flag.
- **A config file with sections (`req.toml`).** The two plain-text lists exist, are documented and are
  adopted. A new format would be a migration for no new capability.
