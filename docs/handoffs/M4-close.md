# M4 close

Date: 2026-11-05
Code baseline (main sha): `1255a45` (PR #113 merged)
Gate: **0 pass** — full `bash scripts/check.sh` on `1255a45` with the `.tools` HOME: no SKIP, Baseline
`[VERIFIED]` (18 claims, rules v6, signed 2026-11-04T05:27:16Z), `variants gated: 2`, `projections checked: 1`,
`QUICKSTART 0s`.

This file covers M3 → M4 (the `req.claim/2` format, its clean-up, the three M4 units) **and** the adopter-driven
work that followed it (#106, #63, #64), because no handoff was written between them. The next orchestrator thread
starts from this file, not from the previous thread's transcript.

## Shipped

M3 / M4 (the claim is authored, the parse and address are derived):

- #90, #91 — M3 spec and `req.claim/2` (ADR 0022): nothing derived lives in a claim file; pins are the only
  load-bearing slot.
- #92 — review rules stop describing the removed fill/verify mechanism. #94 — `--max-tools 150`,
  `--timeout 30` so the largest files finish. #93 — M4-1/M4-2 specs.
- #95 — M4-2: six targeted clippy lints enforce the Tier-1 determinism rules on every path.
- #96/#97 — M4-3: `tools/migrate_claims.py` proves its output loads and its pins hold before writing.
- #98 — M4-1: REQ-0007 split into its two obligations (REQ-0007 + REQ-0023); REQ-0015's rationale re-worded;
  the owner re-signed the Baseline.
- #99 — close-out from the final Opus 5.5 read-only review of `f3c8600..e70187a` (no high findings): honest
  `signed_at`, "pins are not attested" stated where adopters look (`docs/claim-format.md`, `docs/adopting.md`,
  `req baseline --help`), REQ-0007 assumption names the right lint, stale counts. The 0-byte manifest the
  re-sign left was restored in the same PR and re-signed by the owner.

After M4 (adopter-driven, each an ADR-first, cross-family-reviewed unit):

- #100 agent handoff precheck + brief; #101/#102 sedia step 3 brief (fifteen claims); #104 two overview views;
  #105 `baseline --out` writes atomically.
- #106 → #107 — `req canon --fail-on-flag` (exit 1 on any flagged pin; not a release verdict).
- #63 → #108 (ADR 0023 + spec), #109 (`req check` itself), #110 (CI template, adopting docs, this repository's
  own gate call it) — issue closed.
- #64 → #111 (ADR 0024 + spec + real pytest fixtures), #112 (`--test-report` repeatable, the union judged,
  pytest `--junitxml` accepted, collection errors named), #113 (`tools/junit_case.py`, adopting docs) — issue
  closed.

## Evidence

Never pooled across corpora. All from the gate run above, rules v6:

| corpus | FMR hard gate | collapses | not-equivalent pairs |
|---|---|---|---|
| parity (seed) | PASS | 0 | 9 |
| external-v1 | PASS | 0 | 160 |
| external-v2 | PASS | 0 | 227 |
| external-v3 | PASS | 0 | 625 |
| external-v4 | PASS | 0 | 1595 |
| direction-v1 (direction gold) | PASS | 0 | 880 |

- The parity corpus carries no direction labels, so its direction block is `UNVERIFIABLE` (checked 0) by design;
  direction is measured on `direction-v1` only.
- A seed-corpus pass is not the external-corpora falsification, and neither is validation of the bet on
  seed evidence (AGENTS.md "Do not overclaim").
- nextest: 591 tests. `req canon`: 22 claims (1 unstructured), 8 pins on 4 claims, 0 flagged.
- Not measured here: the Tier-2 research crate (nothing in it gates a release), real-world adopter FMR.

## Decisions made (ratified by the owner)

- ADR 0020 (address is a meaning fingerprint), ADR 0021 (Tier-2 leaves the core), ADR 0022 (`req.claim/2`),
  ADR 0023 (`req check` folds the repository's release lists; amends one clause of ADR 0018), ADR 0024 (test
  evidence is a set of reports) — each ratified by the owner's merge of its PR.
- REQ-0007 split; REQ-0015 re-worded; lints after M3; migrate-then-load after M4-2.
- `--fail-on-flag` spelled exactly so (owner confirmed).
- Review policy: CI reviews, local self-review retired (2026-11-03). Fix every high; a medium only with a named
  failing input; do not chase lows; the owner merges.
- ADR 0024 §4 consequence stated and not overruled: an adapter exit 3 becomes `skipped`, which ADR 0019 §5 reads
  as FAIL, never as a pass.

## Open / carried forward

| Item | Owner | Why it blocks |
|---|---|---|
| sedia agent's report on `docs/handoffs/adopter-sedia-step-3-…md` | owner / sedia | Not blocking. The brief pins req `1e5fa60`; `main` is now `1255a45`. A re-run needs the human to reinstall req first (precheck stops on a mismatch). |
| `pins_hash` in the Baseline envelope (v5) | owner | Not blocking. Closes ADR 0022's named loss (pins are not attested); a new envelope version means an owner re-sign. |
| libtest is a named gap in test evidence (ADR 0024) | owner | Not blocking: adopt nextest, or wrap `cargo test` as one coarse case via `junit_case.py`. |
| 70 remote branches, nearly all merged | owner | Not blocking. Deleting a remote branch is the owner's call; none is needed by an open PR. |
| Deferred review lows: `junit_case.py` SIGTERM window, `suite.tail`, a `format.rs` "four commands" comment, message-wording pins | — | Not blocking; do not chase (policy above). |
| Installed `req` is a local build of `1255a45`; `rust-objcopy` cannot find `libLLVM.dylib` so debug info is not stripped | owner | Cosmetic: the build warns and the binary works. |

## Entry conditions for the next milestone

- [x] Gate green on `main` (`1255a45`, full run, no SKIP)
- [x] Open owner-side items above resolved or explicitly deferred (all listed as not blocking)
- [ ] Next specs written and committed under `docs/specs/` — **none exist**; the owner names the next milestone
- [x] Branch point recorded: `1255a45`

Read first: `AGENTS.md` (orchestration, review), `docs/adopting.md` (the adopter's loop), ADRs 0022–0024, and
`req --version` (built from must equal `git rev-parse --short HEAD`; `rules_v` 6).
