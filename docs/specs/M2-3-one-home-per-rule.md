# M2-3 — one home per rule: cut AGENTS.md, make `--help` the command reference, merge the adopter pages

Task id: `M2-3`. Run after M2-1 (and M2-2 if it is decided); the invariants below hold whatever
those left in `AGENTS.md`. Owner ruling of 2026-11-03 on F6.

## Why

`AGENTS.md` is ~990 lines. Its "Commands" section is a second man page: every exit-code nuance is
written there, again in `README.md`, `docs/adopting.md` (742 lines), `docs/agentic.md`, and
`docs/claim-format.md`, and the repo's own rule is that where two disagree one is wrong. Meanwhile
every `req <cmd> --help` page already carries a usage line, a paragraph and an `exit:` contract, and
ends with "`AGENTS.md` §Commands for the reference" — a pointer into the copy that drifts. The
aim: **each rule has exactly one home**, the contract lives where the code and its tests are, and
`AGENTS.md` is what an agent needs at the start of a task.

## What to do

1. **Ledger first.** Before cutting, list every rule/contract sentence in `AGENTS.md`'s Commands
   section and "Where things live" (a table in the PR description: rule → *kept in AGENTS.md* /
   *moves to `<cmd> --help`* / *already in `<doc>`* / *already pinned by `<test>`* / *deleted:
   reason*). A rule with no row is a finding. Contract text that exists only in `AGENTS.md` moves
   into the matching help page in `src/main.rs` (the exit-code rule, tri-state outcomes,
   "never pipe a check through tail/grep" stays in `AGENTS.md`).
2. **`--help` pages are the command reference.** Replace each page's footer "`AGENTS.md`
   §Commands for the reference, `docs/adopting.md` for the loop." by a pointer that is true after
   the cut (`docs/adopting.md` for the loop). Pin the new footer and each moved contract in
   `tests/cli_version.rs` (or the test that already pins that page) — red first.
3. **`AGENTS.md` ≤ 150 lines.** Keep: what req is (≤8 lines); sources of truth; agent rules
   (vocabulary, never hand-write `address`/`canonical.form`, asymmetric cost, git identity, no
   network, pins as provenance, do not overclaim, external material); the correctness rules; the
   project discipline (settled ADRs, the bet); the short commands table (one line per command,
   pointing at `--help`); the gate (`scripts/check.sh`, `--fast` in two sentences, the nextest
   profile and "never pipe a check"); where things live as a one-line-each list; orchestration and
   the SDLC (worktree, TDD, commit conventions, PR flow) as short paragraphs; the Review section
   M2-1 wrote; the phases paragraph (CI). Delete the stale "Merge locally (no PR)" section.
4. **Merge `docs/agentic.md` into `docs/adopting.md`.** One adopter page: the human path first,
   then a "For an agent" section with the five steps, what the agent may not do, what it can read
   (`--format json`, the exit contract, `render --check`), and the honest limit of the test link.
   Delete `agentic.md`. Update every reference (`README.md`, `AGENTS.md`, `tools/quickstart.sh`,
   `scripts/check.sh`, `init`'s printed next steps if it names it). `tests/ci_template.rs` pins the
   CI snippet three ways (the `init` const, the adopting page's fenced block, the example file):
   the block must stay byte-equal. `tools/quickstart.sh` still rehearses the page's commands and
   prints `QUICKSTART <n>s`; it must stay green and under its bound.
5. **`README.md`**: one pass so it neither repeats an exit contract nor contradicts `AGENTS.md`;
   keep the bet section exactly as ADR 0020's PR left it.
6. **No behaviour change.** The only `src/` edits are help-page text in `src/main.rs` (and the
   tests that pin it). Anything else is out of scope; report it instead.

## Red test

The updated help-page pins from item 2, failing at the base.

## Verify

Full `scripts/check.sh`, exit 0, no SKIP, `QUICKSTART` line present, baseline `[VERIFIED]`. The PR
description carries the ledger, the line counts before/after for `AGENTS.md`, `adopting.md`,
`agentic.md`, and says the markdown was read by hand (list the files).

## Allowed paths

`AGENTS.md`, `README.md`, `docs/adopting.md`, `docs/agentic.md` (deleted), `docs/claim-format.md`
and `docs/canonical-hashing.md` (only to host a rule the ledger moves there), `src/main.rs`
(help text only), `tests/cli_version.rs` and the tests that pin the touched help pages,
`tools/quickstart.sh`, `scripts/check.sh` (only references to the renamed page), `examples/`
(only a reference fix; the CI snippet bytes do not change).

Forbidden: everything else under `src/**`, `claims/**`, `baselines/**`, `bench/**`, `aliases/**`,
`review/**`, `.forgejo/**`, `docs/adr/**`.
