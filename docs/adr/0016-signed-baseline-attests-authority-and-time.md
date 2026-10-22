# 0016. The signed Baseline attests its authority and its signing time

**Status: accepted** — proposed by the agent, ratified by the maintainer on 2026-10-20. The decision answers the
second half of review finding [F4] and the limitation ADR 0010 states plainly ("a Baseline is historical
and no trusted signing time exists"). Date proposed: 2026-10-20.

**Extended by [ADR 0019](0019-claims-carry-authored-test-links.md) (proposed)** — envelope **v4** adds
`tests_hash` to each claim record, beside `relations_hash`/`rationale_hash`. Nothing in this ADR is
superseded: `signed_at`, the authority anchor and the `rules_v`-mismatch rule are unchanged. A **v3**
manifest still verifies on its historical signed bytes and reports **once** that it predates v4, so its
tests are **not attested** — never a FAIL, because it never claimed to attest them.

## Context

A Baseline today is a signature over **which claims** were ratified. The canonical manifest (envelope v2,
`BASELINE_VERSION = 2`) is:

```json
{"version":2,"rules_v":5,"selection":{"deployment_mode":"air_gapped"},
 "claims":[{"claim_id":"REQ-0001","address":"f559f5a9…","relations_hash":"479fcec2…","rationale_hash":"ae05ba61…"}, …]}
```

It attests the selection, the rules version, each claim's address, and — since ADR 0013 — its audit
surface. It attests **nothing about the authority that ratified those addresses, and nothing about when**.
Measured on `e9506a6`:

- `grep -c decision src/baseline.rs` → **0**. `req baseline --verify` never reads `decisions/`. A Baseline
  can verify while the Decision log is absent, truncated, or rebuilt.
- `decisions/DEC-0001.yml` exists and `req decision --verify` reports **[VALID]** — but that is the *log's*
  opinion of itself. F4 established that the chain carries **no signature and no HMAC** (`sha2` is a
  dependency, so hashing is available; an HMAC would need a key and the repository holds none), so its
  integrity rests on git history and on `--fill`'s refusal to rewrite a filled hash. That refusal landed;
  **the anchor did not**.
- `baselines/air-gapped.json` is v2 and signed (`air-gapped.json.minisig`, 310 bytes) with the key in
  `baselines/trust.yml`.

Two consequences follow, and both are load-bearing for the product's claim that authority is a signed
Decision (ADR 0005):

1. **"Ratified by a human" is asserted, not attested.** The file's existence proves that *a* signed
   Baseline exists; it does not tie that signature to the Decision chain whose head it was meant to
   anchor. Truncate the log to its genesis entry and every Baseline still verifies.
2. **Key history cannot be judged.** ADR 0010 rules an expired key UNVERIFIABLE because no trusted
   signing time exists, and a revoked key FAIL even for signatures made before revocation. Both are the
   only answers available without the missing field — and the second is wrong in the direction that
   matters: revocation is normally *prospective*, and a Baseline signed in good standing should not rot
   because the key was retired afterwards.

## Decision

### Envelope v3

`BASELINE_VERSION = 3`. The manifest gains two fields, both **inside the signed bytes**:

```json
{"version":3,"signed_at":"2026-10-20T18:30:00Z","authority":{"head":"<sha256>","entries":1},
 "rules_v":5,"selection":{…},"claims":[…]}
```

- **`authority.head`** — the Decision log's chain head: the last entry's `hash` (`src/decision.rs:65`;
  there is no accessor for it yet, so this ADR adds one), the value every earlier entry chains into. `entries` is a count, included because a head alone cannot distinguish "one entry" from "a
  log rebuilt to one entry with the same head", which is impossible in principle and cheap to state in
  fact. `req baseline --verify` **re-derives** the head from `decisions/` and compares: a mismatch is
  **FAIL** (a definite finding, the same class as a hand-edited digest), a missing or unparsable log is
  **UNVERIFIABLE**.
- **`signed_at`** — an RFC 3339 UTC timestamp the **signer supplies** (`req baseline --at <ts>`), not a
  clock req invents. It is an authored field, and req refuses to emit a manifest without one, because a
  time the tool guessed would be a time the signer never asserted. Being inside the signed bytes, it
  cannot be backdated without breaking the signature.

### What `signed_at` is used for, and what it is not

It adjudicates the **key**, never the claims: a key whose `expires` is at or after `signed_at` verifies
for that Baseline even if today is later, and a key **revoked after** `signed_at` no longer invalidates
it. Whether a claim is *current* remains `canon --verify`'s and `audit`'s business, which re-derive
addresses; `signed_at` never substitutes for that.

### `rules_v` mismatch stays UNVERIFIABLE

F4 listed "rules_v enforcement" as an open question; this ADR answers it **no change**. An address is
derived from the rules version, so a manifest recorded under a different one cannot be compared at all —
the comparison is *void*, not failed. FAIL would accuse a claim of drift on evidence the build cannot
produce. The existing behaviour (UNVERIFIABLE, with the re-sign remedy) stays.

## Consequences

- **The committed Baseline must be re-signed.** `baselines/air-gapped.json` is v2; under v3 `--verify`
  reports **UNVERIFIABLE**, never FAIL — measured on `e9506a6`, an envelope this build does not write is
  already handled in both directions, exit 3 with a re-issue remedy: a newer one says *"manifest version 9
  is newer than the version this build writes (2) — verify with a build that writes it"*, an older one
  *"manifest version 1 predates the version this build writes (2) — re-issue and re-sign (ADR 0013)"*. A v3
  build therefore reports today's v2 manifest as *"predates … re-issue and re-sign"* with no new message.
  The
  new manifest is emitted with the chain head and the signer's timestamp and signed with the maintainer's
  minisign secret key, which req cannot reach. **The gate cannot go green in between**: the Baseline step
  has no SKIP and UNVERIFIABLE blocks, so a v3 implementation PR stays red until the human signs, inside
  that same PR.
- **Revocation becomes prospective**, and expiry becomes judgeable at signing time — the reason the field
  exists. This changes what `trust.yml`'s `revoked`/`expires` mean for *existing* signatures, and is the
  one consequence a reader should not skim.
- **Anchoring starts at the first v3 signature.** Baselines signed before it remain unanchored; that is
  recorded rather than hidden, and the first v3 re-sign is the moment the chain is tied to a signature.
- **v2 manifests keep working.** A manifest with `version: 2` and no `authority` must not be FAILed for
  its absence: the envelope-version mismatch is reported first (UNVERIFIABLE), so old Baselines degrade to
  "re-sign", never to "tampered".
- Effort **S/M**: two serde fields, a `chain_head` accessor in `decision.rs`, the comparison in
  `baseline::verify`, the `--at` flag with its refusal, and the tests below.

## Falsification

Sign a v3 Baseline with head `H` and `signed_at` `T`, then:

1. edit `decisions/DEC-0001.yml` → `req baseline --verify` must **FAIL** on the authority mismatch;
2. delete `decisions/` → **UNVERIFIABLE**, never FAIL (an absent log is "cannot tell", not "tampered");
3. with a trust entry whose `expires` is after `T` and today's date later than it → must **verify**;
4. with a key revoked at a date after `T` → must **verify** (prospective revocation);
5. `req baseline` without `--at` → usage error (exit 2), never a guessed timestamp.

If any of 1–5 behaves otherwise, this ADR has not landed.

## Alternatives rejected

- **Attest the head in minisign's trusted comment.** It is not re-derived by req, it is a second place to
  keep in sync with the manifest, and minisign's comment is optional — a signature without one would be
  indistinguishable from an older signature. ADR 0013's principle applies: one manifest, so one signature
  covers both facts.
- **A separate `authority.json` beside the manifest, with its own signature.** Two signatures for one
  ratification is the drift this project exists to remove, and it doubles the human's signing act.
- **HMAC the chain with a key in the repository.** A key committed to the repo is not a secret; it would
  add ceremony and no adversarial protection. The signature over the head is the anchor, and it is the
  human's.
- **Keep v2 and rely on git history.** Git answers *what changed*; it cannot answer *which authority was
  signed under*, and it cannot answer expiry at signing time. This is the status quo the ADR exists to
  replace.

## Implementation note (2026-10-21): the head names a prefix

Verification locates the attested head at **position `entries`** in the log and checks that
record's stored hash — it does not compare it with the log's present head. `entries` exists
precisely to say which record the head names, and equality with the present head would make the
ordinary act of recording a Decision (ADR 0005) invalidate every Baseline signed before it;
history growth is not tampering. A log shorter than the attested prefix, a rewritten attested
prefix, or a record whose stored hash no longer recomputes is still FAIL. Decisions appended
afterwards do not stop an earlier Baseline verifying, and `req decision --verify` remains the
judge of the log as it now stands.
