# M4-3 — `migrate_claims.py` proves its own output loads, and that its pins hold

Task id: `M4-3`. Code baseline: current `main` (after M3 and, when this is dispatched, M4-2). Owner
request of 2026-11-04, after the CI review of #91 caught the same class of defect in
`tools/migrate_claims.py` four times (a unit quoted wrongly, a numeric-looking unit, a non-comparator
operator, a rescaled unit that can never agree): the tool wrote a file `req` then refused or flagged,
and printed `migrated N file(s)`. Each was found by a model reading the diff; one deterministic check
would have found all of them. This unit adds it.

**Model:** `claude-sonnet-5-5` (high); the orchestrator may use `deepseek-v4.1-flash` if M4-2 landed
clean on it. The gate is the check either way.

## Why

`tools/migrate_claims.py` is the documented migration path for every adopter of `req.claim/1`. It
parses YAML by hand (no library is installed or wanted), so its output is the one place a quoting or
mapping slip silently produces a corpus `req` cannot load. The `--self-check` table asserts emitted
*text*; it cannot say whether the text loads. `tools/adopt.py` already closes the same hole with
`loader_check`: it runs the corpus plus its drafts through `req canon` in a scratch copy before
writing anything. The migrator needs the equivalent, plus one thing `adopt.py` does not need: that
every pin it created **holds** against the parse.

## What to do

1. **A load check in `run()`, before anything is written (dry run and `--write` alike).** Copy the
   corpus root's `aliases/` (and nothing else) into a scratch directory, put each migrated file in its
   `claims/`, and run the real binary over it:
   - `req --root <scratch> canon` must exit 0 (the loader accepted every migrated file); any other
     exit is a refusal that prints the loader's own message and **writes nothing** (exit 1, like any
     refusal);
   - `req --root <scratch> gate --format json` is read for the `pins_hold` policy only (the gate's
     other policies and exit code are irrelevant here: a bare gate may be UNVERIFIABLE for unrelated
     reasons): its `outcome` must be `pass`. Otherwise the migrated pin set is refused, naming the
     offenders from the document's `details`. A pin the parse contradicts is exactly the defect class
     this unit exists to stop.
   The corpus root is `--root <dir>` (new flag; default: the parent of the first path's `claims/`,
   else the current directory). `req` is found on `PATH` (as `adopt.py` does).
2. **No `req`, no silent pass.** If `req` is not on `PATH`, the tool exits **3** (UNVERIFIABLE) saying
   that it cannot prove the output loads and how to proceed; a new `--no-verify` flag migrates
   without the check and says so on stdout and in its final line ("NOT VERIFIED"). `--self-check`
   needs no binary and is unchanged in that respect.
3. **Tests** (`tests/migrate.rs`, new; public seam, the compiled `req` and `python3
   tools/migrate_claims.py`; a fixture corpus built in a temp dir from a small `req.claim/1` text
   written in the test, with `aliases/subjects.yml` copied from the repository; expectations from
   independent oracles, never from the tool's own output):
   (a) a valid v1 corpus migrates (`--write`), then `req canon` exits 0 and the gate's `pins_hold`
       passes with the pins the file's slots implied (value, operator, unit);
   (b) a v1 file whose authored value contradicts its statement (`value: "0.5"` against a
       `within 200 ms` statement) is **refused**, exit 1, nothing written, the message names the
       pin;
   (c) a v1 file that migrates to something the loader rejects (build one the existing refusals
       do not catch; if none exists, assert the loader-failure path with a stub `req` on `PATH`
       that exits 3 and prints a message, and say so in the PR) is refused with the loader's message;
   (d) no `req` on `PATH` → exit 3, and with `--no-verify` the migration proceeds and says
       "NOT VERIFIED";
   (e) a dry run writes nothing (as today) and also runs the check.
4. **Docs.** `docs/claim-format.md`, in the "Migrating from req.claim/1" section: one short paragraph
   that the tool proves its output loads and that its pins hold before writing, what exit 3 and
   `--no-verify` mean. Update the tool's module docstring and `--help` text to match.

## Mutation checks (envelope `mutations`)

- Make the load check a no-op (return success without running `req`) → test (b) fails.
- Make the `pins_hold` read ignore a `fail` outcome → test (b) fails.
- Remove the `--no-verify` path's "NOT VERIFIED" line → test (d) fails.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP, Baseline
`[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present, `migrate_claims.py --self-check` green.
No claim, address, Baseline or `bench/results` change.

## Allowed paths

`tools/migrate_claims.py`, `tests/migrate.rs` (new), `docs/claim-format.md` (the migration section),
`scripts/check.sh` only if a step must name the new test (it should not: nextest runs it).

Forbidden: `src/**`, `claims/**`, `baselines/**`, `bench/**`, `aliases/**`, `decisions/**`,
`review/**`, `.forgejo/**`, `tools/ocr/**`, `tools/adopt.py`, root `Cargo.toml` and `Cargo.lock`, and any
change to the canonicalizer's rules or `RULES_V`.
