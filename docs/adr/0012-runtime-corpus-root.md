# 0012. The corpus root is runtime, never compile-time

**Status: accepted** — proposed by the agent, ratified by the maintainer on 2026-10-13. The decision
answers review finding [F6] — "single-repo dev tool, or a tool for other repositories?" — and closes
[F1] with it. Date: 2026-10-13.

## Context

Every claim-reading command locates its corpus through `env!("CARGO_MANIFEST_DIR")` — thirteen call
sites as of this ADR. The path is baked in at *compile* time, so a binary built anywhere reads that
build tree or nothing: install `req` and run it against another repository and it exits 3 — or,
worse, answers from a stale tree that still exists. The failure was reproduced during the review: a
binary compiled in one worktree, cached in a shared `target/` and run from the main checkout, tried
to read a worktree that had already been deleted.

The product question underneath is the one the review asked to be decided explicitly. The review
reached for git as the precedent, and git argues for the other choice: git commands take the
repository from the environment (`cwd/.git`) or explicitly (`--git-dir`, `GIT_DIR`, `-C`) — it was
built for one repository and has never been limited to it. req's own settled decisions agree: ADR
0002 makes git the transport for *the host project's* repository, and a requirements engine whose
corpus is `claims/` in whatever project it manages is a tool that runs inside other projects'
checkouts. ADR 0009 and REQ-0012 commit to an air-gapped single binary, which must manage a
project's claims where no source tree exists.

The rejected alternative — "a single-repo dev tool" — makes `req` a tool for req alone. That
contradicts the product, and it preserves the one defect that makes every installed binary wrong.

## Decision

req is a tool for other repositories. The corpus root is resolved at **runtime**, in this order:

1. `--root <dir>`, when given;
2. `REQ_ROOT`, when set;
3. the nearest ancestor of the working directory that contains `claims/`.

No compiled-in path remains. Dev ergonomics are unchanged: inside a repository with `claims/`,
everything works as before. A root that yields no corpus fails loudly — **UNVERIFIABLE (exit 3)**,
never an empty answer — and whichever root was selected is *printed*, because a silently-chosen root
is the failure mode this ADR exists to remove.

## Consequences

- Thirteen `env!` sites collapse into one resolved `Root`. PR #5 already built the seam
  (`canon::load_claims(root)`, `run_claim_diagnostics_at(root)`); only the CLI plumbing remains.
- [F1] and [F6] close. The shared-`target/` footgun loses its teeth — a binary reads whatever tree
  it is run in. The rule stands anyway: do not share `CARGO_TARGET_DIR` across worktrees.
- The root is an invocation parameter, like a variant (ADR 0003): selection changes no address, and
  neither does the root.
- Tests build fixtures under a temporary root (`tests/loader.rs` is the pattern), never the manifest
  directory.
- **Falsification:** install `req` on a machine with no source tree, point it at a fresh
  repository's `claims/`, and run the read path end to end. If that cannot work, this ADR has
  failed.
