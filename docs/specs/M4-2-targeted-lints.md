# M4-2 — a few targeted lints, so the determinism rules are checked by the compiler

Task id: `M4-2`. Code baseline: current `main`. Owner ruling of 2026-11-04 (after reading
BeautifulRust, `morganlinton/BeautifulRust`: its skills are not adopted; a handful of its lints are,
because they enforce rules AGENTS.md already states): add **six** targeted lints and fix or
justify what they flag. Nothing else from that repository is taken.

**Model:** `deepseek-v4.1-flash` via the `opencode` provider (cheap; the work is mechanical and the
gate is the check). If it comes back `blocked`, or its envelope fails the checks, the orchestrator
re-dispatches on `claude-sonnet-5-5`.

## Why

AGENTS.md says: Tier-1 determinism ("no `f32`/`f64` math … no locale-dependent formatting"), "silent
loss" casts are how an address could drift on a platform, and a suppression should say why. Today
only clippy's default set runs (`cargo clippy --all-targets -- -D warnings`). Measured on `main` at
the time of writing (clippy of the pinned toolchain, `--all-targets`): `as_conversions` 28,
`unwrap_used` 30, `float_arithmetic` 3, `cast_sign_loss` 3, `cast_possible_truncation` 2,
`allow_attributes_without_reason` 1.

## What to do

1. **Configure** in the root `Cargo.toml` a `[lints.clippy]` table at `warn` (the gate's
   `-D warnings` makes it an error) with exactly these six: `as_conversions`,
   `cast_possible_truncation`, `cast_sign_loss`, `allow_attributes_without_reason`, `unwrap_used`,
   `float_arithmetic`; and add `clippy.toml` with `allow-unwrap-in-tests = true`,
   `allow-expect-in-tests = true`. **No other lint, no `pedantic`, no dependency, no change to
   `[package]`, `[dependencies]` or `Cargo.lock`.** The experiments crates (`experiments/**`) are
   not touched.
2. **Fix what the lints flag, in this order of preference:** (a) a conversion that has a lossless
   form (`u64::from`, `usize::try_from(..)` with the error handled or `expect("why it cannot fail")`)
   becomes that; (b) a `unwrap()` on a value that cannot be absent becomes `expect("<why>")` (the
   `expect_used` lint is not enabled), or `?`; (c) what genuinely must stay gets
   `#[expect(clippy::<lint>, reason = "<why, one sentence>")]` on the smallest item — never
   `#[allow]`, never a blanket crate-level attribute. The three `float_arithmetic` hits are the rate
   reporting in `src/bench.rs` (a printed ratio, never part of an address): each is an `#[expect]`
   whose reason says so.
3. **The fingerprint path is not to move.** `src/canon.rs` and anything feeding `canonical_json` /
   `address_hex` change **behaviour-preservingly or not at all**. After the change: the 21 recorded
   addresses equal the list in `tests/claim_v2.rs` (with its re-worded override), `req baseline
   --verify baselines/air-gapped.json` is `[VERIFIED]` under the unchanged `.minisig`, `RULES_V`
   is 6, and every `bench/results` record is unchanged. **If any of these moves, stop and report
   `blocked`**: a lint fix that changes an address is a rules change and not this unit's.
4. **Record the honest remainder.** The PR description lists, per lint, how many sites were fixed and
   how many are `#[expect]` with their reasons (one line each), and the count of `clippy` warnings
   before and after (before: the table above).
5. **Docs.** One sentence in `AGENTS.md`'s Correctness rules (the determinism bullet) naming that
   the six lints enforce it, and nowhere else; no new document.

## Mutation checks (envelope `mutations`)

- Add `let _ = 1u64 as u32;` to a scratch copy of `src/canon.rs` → `scripts/check.sh` (clippy step)
  fails; show it.
- Remove one `#[expect(..)]` you added → clippy fails on the site it covered; show it.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, exit 0, no SKIP, Baseline
`[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present.

## Allowed paths

`Cargo.toml` (the `[lints.clippy]` table only), `clippy.toml` (new), `src/**`, `tests/**` (only where a
lint flags a test and `allow-*-in-tests` does not cover it), `AGENTS.md` (the one sentence).

Forbidden: `Cargo.lock`, every other part of `Cargo.toml`, `rust-toolchain.toml`, `baselines/**`,
`bench/**`, `aliases/**`, `decisions/**`, `claims/**`, `review/**`, `.forgejo/**`, `tools/**`,
`experiments/**`, `docs/**` (except none), and any change to the canonicalizer's rules or `RULES_V`.
