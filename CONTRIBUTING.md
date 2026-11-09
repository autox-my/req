# Contributing to req

Thank you for looking. req is early (`0.1.0`) and maintained by one person, so the process is small and strict.

## Before you start

- **Open an issue first** for anything beyond a typo or a clear bug fix:
  <https://github.com/autox-my/req/issues>. A change to how a requirement is canonicalized moves addresses, so it
  is a decision, not a patch (see *Rules* below).
- **Reporting a bug?** `docs/adopting.md` § *Reporting a bug in req* lists the four things that make a report
  actionable: the statement or file, the exact command, its exit code and deciding line, and `req --version`.
  A **false merge** (two different requirements sharing an address) is the failure this tool exists to prevent;
  say so in the title.
- **A tool defect** (req gave a wrong or misleading answer) has its own issue form, **Tool defect**, which applies the `tool-defect`
  label; a maintainer adds `wrong-verdict` to a verdict that hides a defect. A pull request that fixes one says `Fixes #N` and uses the
  `fix:` prefix, so the release notes list it under *Fixes* (ADR 0031).
- **Security problems** go through [`SECURITY.md`](SECURITY.md), not a public issue.

## Build and test

```bash
cargo install cargo-nextest --version 0.9.149 --locked
cargo build --release
./scripts/check.sh        # the gate: fmt, clippy, nextest, canon, Baseline verification, every corpus
```

`scripts/check.sh` is the one gate; CI (`.github/workflows/ci.yml`) runs the same script. A skipped check is
printed as `SKIP`, never as a pass, and CI fails on any `SKIP`. Run the **full** gate before you open a pull request
(`--fast` is for iterating). `AGENTS.md` has the detail.

## Rules that are not negotiable in a pull request

- **Zero false merges on the corpora is a hard gate.** Never relax it to land a change.
- **Tests first.** Write the failing test, then only enough code; tests use public interfaces and independent
  oracles (labels come from the spec, never from the implementation's own output).
- **A Tier-1 rule change moves addresses.** It needs a superseding ADR, a `RULES_V` bump, deliberately regenerated
  goldens and recorded re-canonicalization candidates. The settled decisions are `docs/adr/` 0001–0009; changing one
  needs a new ADR, not an edit.
- **Nothing derived lives in a claim file** (no `address`, `canonical` block or parse slot), and a pin is never
  edited to fit a parse.
- **Do not overclaim.** Report which corpus produced which number; a seed-corpus pass is not the external-corpora
  result.
- **Docs are claims too:** when a document asserts something the code contradicts, fix one of them in the same
  commit.

## Pull requests

- One feature branch per change, one pull request per branch; `main` is never pushed to directly.
- Commit subjects start with `feat:`, `fix:`, `refactor:`, `test:`, `docs:` or `chore:`.
- **There is no automated review here: a human reads every pull request.** Say what you changed, which tests
  prove it, and what you could not verify.
- Stage files by name (`git add <paths>`); never commit a secret, a `.env` or a local path.
- Contributions are accepted under the repository's licence, Apache-2.0 (`LICENSE`).

## Using an AI agent

Agents work on this repository too. `AGENTS.md` is their instruction file and `CONTEXT.md` the glossary; the same
rules apply, and the person who opens the pull request is responsible for what is in it.
