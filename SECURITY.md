# Security policy

## Supported versions

req is pre-release (`0.1.0`). Only the tip of `main` is supported; there are no backported fixes.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Use GitHub's private reporting instead:
the repository's **Security** tab, then **Report a vulnerability**
(<https://github.com/autox-my/req/security/advisories/new>). If that option is not shown, open an issue that says
only "security contact needed", with no details, and a private channel will be arranged.

Include what you did, what you expected, what happened, `req --version` (its `built from` and `rules_v` lines) and
the smallest corpus or claim file that reproduces it. This is a one-person project: there is no guaranteed response time,
and any fix timeline is stated in the reply.

## What counts

req is an offline command-line tool that reads a corpus of claim files and git history. These are in scope:

- a claim, corpus, registry, Baseline or decision file that makes req execute code, write outside the corpus
  root, or read a file it was not pointed at;
- a way to make `req baseline --verify` report `[VERIFIED]` for a manifest that was not signed by a key in
  `baselines/trust.yml`, or to bypass the trust anchor;
- a way to tamper with the decision log's hash chain without `TAMPERED` being reported;
- the install script (`examples/install-req.sh`) accepting a binary that does not match its pinned SHA-256 or
  `built from` rev;
- a weakness in the GitHub workflows of this repository (`.github/workflows/`) that exposes a secret or a
  write token to a pull request from a fork.

These are **not** security issues; report them as ordinary issues: a false merge or a wrong direction
classification (a correctness bug, and the one this tool most wants to hear about), a wrong exit code, or a
documentation error.

The dependency graph is checked in every gate run for forbidden network crates, unreviewed licences and non-crates.io sources, and weekly against the RustSec advisory database (`.github/workflows/audit.yml`); the bill of materials of the binary is `docs/sbom.cdx.json` (ADR 0036).

req never signs anything and has no network dependency in its core; the optional `req propose` provider is a
separate process you configure.
