# A-1 — party signatures (ADR 0042, the code)

Task id: `A-1`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-12 ("next build ADR 0042 (party
signatures)").

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0042-a-baseline-is-signed-by-named-parties.md` (decisions 1–6 and its *Falsification* list).
Where this spec and the ADR disagree, the ADR wins; stop and report. The code is `src/baseline.rs` (`TrustedKey`,
`TrustFile`, `Verification`, `verify`, the `--verify` CLI path), `src/decision.rs` (`canonical_bytes`, the `--verify`
path) and `src/completeness.rs` (`signed_coverage`).

## Choices fixed here

1. **Trust file.** `TrustedKey` gains optional `party` and `role` (strings). A key without them behaves exactly as today.
   A policy line can only be met by a key that has both.
2. **Signature files.** Beside `<manifest>.json`: the existing `<manifest>.json.minisig` is a signature by the `--key` key
   (default `req-baseline-1`), unchanged; every `<manifest>.json.<key-id>.minisig` is a signature by `<key-id>`. A key id
   in a file name must match `[A-Za-z0-9._-]+`; a file whose id is not in the trust file is UNVERIFIABLE, named, never
   ignored. Every present signature is judged with today's per-key rules (scheme, ADR 0010/0016 trust state at the judged
   time, signature validity): a signature that does not verify, or a key revoked at the signing time, is FAIL, as now.
   `--signature <file>` keeps its meaning (it replaces the default `.minisig` for the `--key` key).
3. **Policy.** `.req/signing`, optional, the grammar of the other `.req/` lists (`#` comments, blank lines skipped); a line
   is `<party> <role>` (two tokens, `[A-Za-z0-9._-]+` each); anything else is exit 2 naming `<file>:<line>`, every bad line
   in one run. Repeating a line *n* times needs *n* signatures from **distinct** keys holding that party and role; one key
   counts towards one line only (TUF's rule).
4. **Outcome.** Drift, rules version and authority are judged exactly as today. Then: any present signature FAIL → FAIL.
   With a policy: every line met by a distinct verified signature → the signature part verifies; an unmet line →
   UNVERIFIABLE, `awaiting <party> <role>` (one reason per unmet line). With no policy: at least one verified signature →
   verifies; none → today's "no signature" UNVERIFIABLE. **A repository with no `.req/signing` and no keyed signature files
   verifies byte-for-byte as before** (same exit, same text) — `tests/baseline.rs` must pass unchanged, and the three
   committed Baselines must verify.
5. **Output.** The `--verify` header names every signature judged (`key <id> (<party> <role>)` or `key <id>`), and each
   reason line says which key it concerns.
6. **Completeness (ADR 0039).** `signed_coverage` counts a manifest when **any** signature file lies beside it (the
   default `.minisig` or a keyed one), and its "has no signature beside it" note names both spellings.
7. **Signed Decisions (ADR decision 5).** `decisions/DEC-NNNN.yml.<key-id>.minisig` is a signature over
   `decision::canonical_bytes` of that record. `req decision --verify` (and the bare walk) judges each one: verified and
   the record's `role` equals the key's `role` → the record is reported *signed by <id> (<party> <role>)*; signature
   invalid, key revoked at the record's `date`, or `role` mismatch → **FAIL** (exit 1, as TAMPERED is); key unknown or no
   verifier → UNVERIFIABLE; no signature file → the record is reported *asserted* (no change to the verdict). The chain
   verdict is unchanged by any of this when no Decision is signed.

## Tests first (envelope `red`)

Through the existing seams (`Verification` with a fixture `SignatureVerifier`, the trust file as YAML text, the CLI where
the repository's tests already drive it). At minimum one test per ADR 0042 *Falsification* item (1–6), plus: a keyed
signature file naming an unknown key is UNVERIFIABLE and named; a malformed `.req/signing` line is exit 2 naming the
line; a repeated policy line needs two distinct keys; completeness counts a manifest with only a keyed signature; a signed
Decision with a matching role is reported signed, with a mismatching role FAIL, unsigned *asserted*. Prove them red at the
base.

## Mutation checks (envelope `mutations`)

- Let one key count towards two lines of a repeated policy line → a test fails.
- Ignore the policy when it exists (one verified signature suffices) → a test fails.
- Skip the role comparison for a signed Decision → a test fails.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home), exit 0, no SKIP, the three committed Baselines VERIFIED, tree
clean after the run. If the verify output's text changes for the no-policy case, that is a regression, not a fixture to
update.

## Allowed paths

`src/baseline.rs`, `src/decision.rs`, `src/completeness.rs`, `src/main.rs` (help pages), `tests/**`, `docs/adopting.md`
(a short "Signing by several parties" section), `baselines/trust.example.yml` (document `party`/`role`), `AGENTS.md` (the
*Where things live* line on `baselines/` and `.req/` only), `docs/adr/0042-a-baseline-is-signed-by-named-parties.md` (a
dated implementation note only), `examples/**` (a copy-ready `.req/signing` only if the adopter page uses one).

Forbidden: `baselines/trust.yml`, every committed manifest and `.minisig`, `decisions/**`, `claims/**`, `.req/**` of this
repository (no dogfood policy in this unit), `src/canon.rs`, `src/compare.rs`, `RULES_V`, `bench/**`, `Cargo.toml`,
`Cargo.lock`, `.github/**`.
