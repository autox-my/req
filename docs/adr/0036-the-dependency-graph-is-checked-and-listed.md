# 0036. The dependency graph is checked in the gate, audited on a schedule, and listed as a bill of materials

**Status: proposed** — the owner ratifies it by merging the pull request that carries it; asked for by the owner on 2026-11-10
(item 6 of the qualification-readiness package; no issue). Date proposed: 2026-11-10. Adds checks and a generated file; changes no
engine behaviour, no `RULES_V`, no address, and adds no dependency to the crate.

## Context

req's own dependency graph is small (six direct dependencies, 33 crates in the build graph of the two release targets) and its core promises no network
dependency (REQ-0012). Three questions about that graph had no mechanical answer:

1. **Is anything in it that the promise forbids?** `tests/evidence.rs` bans network clients and async runtimes among the crate's
   *direct* dependencies. A transitive one is invisible to it.
2. **Is anything in it known to be vulnerable?** Nothing looked.
3. **What exactly is in the binary?** An assessor, or an adopter's supplier-management process, asks for a bill of materials. The
   honest answer today was "read `Cargo.lock`", which lists 66 packages including ones no build of ours links.

That last point is a measured finding, not a hypothetical. `cargo metadata` reports `bytes`, `borsh`, `rand`, `getrandom` and others as
dependencies of `rust_decimal`, because they are *optional* dependencies the lock must resolve; `cargo tree`, which applies the
features Cargo actually activates, shows none of them in either release target. A bill of materials built from `cargo metadata` would
have listed a networking-ecosystem crate (`bytes`) in a binary that does not contain it.

## Decision

1. **`deny.toml`, three checks in every gate run.** `cargo deny --offline check bans licenses sources`:
   - *bans*: the network and async crates `tests/evidence.rs` names are denied **anywhere in the graph** (REQ-0012 from the transitive side);
     a wildcard version requirement is denied; duplicate versions are a warning, not a failure;
   - *licenses*: the smallest allow-list the graph needs today (`Apache-2.0`, `MIT`, `Unicode-3.0`; every dependency offers one of
     them). A new dependency under another licence fails the gate before it is reviewed, and widening the list is a reviewed change;
   - *sources*: crates.io only; a git or alternate-registry dependency is a different trust decision and needs its own ADR.
   The graph is judged for the two release targets (ADR 0025), not for every platform a dependency supports.
2. **`advisories` is not in the gate.** It reads the RustSec database over the network, which would make the gate non-offline and would turn
   an unrelated pull request red on the day an advisory is filed. It runs **weekly and on demand** in `.github/workflows/audit.yml`
   (read-only, no secret, no pull-request trigger). A failing run is the notification; the response is a human decision (update, accept,
   or `ignore` with a reason in `deny.toml`). `yanked` crates are denied.
3. **cargo-deny is pinned like nextest** (ADR 0019): version `0.20.2` and the SHA-256 of the prebuilt tarball, in CI and in the audit
   workflow, and `scripts/check.sh` fails naming both versions if another is installed. The hash was taken by downloading the release on
   2026-11-10 and checked against the digest GitHub shows for the asset: it is provenance, **trust on first use of the maintainer's
   release**, not authentication. Not a `Cargo.lock` change, and no `RULES_V` bump.
4. **`tools/sbom.py` writes `docs/sbom.cdx.json`**, a CycloneDX 1.5 bill of materials of the binary, with no tool added to the
   toolchain (Python 3.11+ standard library, `cargo`). The graph is `cargo tree` (activated features, per target); licence and
   repository come from `cargo metadata`; the SHA-256 of each registry crate from `Cargo.lock`. Each component has a `scope`:
   `required` (linked into the binary) or `excluded` (used to build it: build scripts, procedural macros and what they use). A
   dev-dependency is not listed. The file is **deterministic** (no timestamp; the serial is a UUIDv5 of the lock's hash), so it is a
   **projection checked like `docs/spec.md`**: the gate runs `--check`, and a change to a dependency, a version or `Cargo.lock` makes it
   STALE until regenerated in the same commit. A graph that cannot be read is UNVERIFIABLE (exit 3), never a match.
5. **The bill of materials is not a release asset.** It is the file at the release's rev: `git show <rev>:docs/sbom.cdx.json`. Adding
   it to the asset set would change the release workflow's six-name rule (ADR 0032) and its tests for a file that is already
   pinned by the rev.
6. **The tool has exactly one kind of evidence and says so.** The generated file was validated **once, by hand,** against the
   CycloneDX 1.5 JSON schema (0 errors; a deliberately broken copy was rejected, so the validator is not vacuous). That validation is
   not in the gate; the gate checks that the file is current, not that it is a valid document.

## Consequences

- A dependency change now touches `Cargo.lock`, `docs/sbom.cdx.json` and possibly `deny.toml`, and a lock change is also a Tier-1
  rules question where the dependency is on the fingerprint path (AGENTS.md: `Cargo.lock` is a version pin). The SBOM makes the
  change visible in review as a diff of components, not of a lock file.
- The gate reads the whole graph **offline**. A checkout that has built only for its own platform lacks the other target's crates;
  CI runs `cargo fetch --locked` first for that reason, and a developer sees `UNVERIFIABLE` naming cargo's own message until they do.
- Both new gate steps take about a second and stay in `--fast`.
- **What this does not show.** An advisory database lists what has been reported, not what is safe. A licence allow-list reads the
  SPDX expression each crate *declares*, not its source. A bill of materials lists crates, not the C code inside `libyaml-rs` or the
  compiler and linker that build the binary (the toolchain is pinned in `rust-toolchain.toml` and recorded in `build-info`). The
  policy was exercised by hand (banning `serde`, which is in the graph, fails `bans`; the pins are mutation-checked in
  `tests/supply_chain.rs`), and `cargo deny` itself is not run by a test.

## Rejected

- **`cargo audit` as well.** It answers the question `cargo deny check advisories` already answers from the same database; two tools
  would be two pins for one question.
- **Advisories in the gate or on every pull request.** Stated in §2.
- **An SBOM from `cargo-cyclonedx`.** Not evaluated in depth, and not run. It would add a pinned build-time tool to a project that
  pins few; the generator we have needs only `cargo` and the Python standard library, takes its graph from `cargo tree` for the reason in
  *Context*, and is small enough to read (see `tools/sbom.py`). Whoever finds the standard tool better is invited to say what it does that
  this does not.
- **Committing no SBOM and generating it per release.** Then it could not be reviewed in the diff that changes a dependency.
