# 0020. The address is a meaning fingerprint; `claim_id` is the identity

**Status: accepted** — drafted by the agent on 2026-11-02 and ratified by the owner on 2026-11-03.
It supersedes only the *identity wording* of [ADR 0001](0001-the-unit-is-a-claim.md); the decision
that the unit is a claim stands. Date proposed: 2026-11-02. Date accepted: 2026-11-03.

## Context

ADR 0001 says a claim is "content-addressed by its canonical meaning" and that "canonical identity" is
what makes dedup, contradiction detection, semantic merge and impact analysis possible. The README,
`CONTEXT.md` (**Address**: "A claim's identity") and `docs/canonical-hashing.md` carry the same
wording: the address *is* the claim's identity.

An architecture review of 2026-11-02 found that the word is wrong for what the address does, in two
ways.

**What everything else already refers to.** Relations (`relations[].to`), test links (`tests:`,
ADR 0019), the Decision log, the rationale and the people who talk about a requirement all name the
`claim_id`. The address appears in none of those places as a referent: a Decision records the address it
ratified, but only to detect that the claim has since *moved* (`req audit`, `req baseline --verify`).
A requirement whose bound tightens is still the same requirement — `REQ-0007`, with the same relations,
tests and history — carrying a new meaning and so a new address. If the address were its identity, an
edit that changes an obligation would create a different requirement and orphan every reference to the
old one. The model does not behave that way, and should not say it does.

**What the address is for.** It is a comparison primitive over *canonical meaning*: equal addresses mean
the Tier-1 rules judged two statements the same; a changed address against a signed Baseline means the
meaning moved since a human signed it. That is a **fingerprint** of the claim's meaning, derived from the
statement by rules at a stated `RULES_V` — not a name.

The same review found two **false merges** in req's own dogfood corpus (REQ-0020 "outgoing/incoming
side" and REQ-0004 "semantic/audit layer"). They were *found* by that review and are *fixed* at rules v6
by the positional coverage invariant (`docs/canonical-hashing.md` T1.10) and whole-span subject matching
(T1.7), merged as PR #73 (task M1-1, `docs/specs/M1-1-identity-coverage.md`). This ADR is the
framing half of the same review; it changes no rule and moves no address.

A second framing point belongs here because it bounds what the fingerprint has been shown to do. The
external corpora (`bench/corpus/external-v1…v4.yml`) generate their *equivalent* pairs with the
transforms Tier-1 itself implements, so a 0% false-split rate there is expected by construction and says
nothing about independent paraphrase. Their *not-equivalent* pairs are a small set of mutation classes
(shall→may, shall→shall not, a first-verb swap, and at rules v6 the any-position and possessive-head
classes); v4 widened the false-merge side only. What has been measured is therefore: zero false merges on
the parity corpus and on those mutation classes, at the rules version each record names
(`bench/results/<corpus>-rules-v<N>.json`). Measurement of independent, human-written paraphrase — the
false-split side, and the meaning-preservation claim in its broad form — is still open. **This ADR does
not claim the bet validated, and it does not claim it invalidated.**

## Decision

1. **`claim_id` is the identity of a claim.** It is stable across rewording and across edits that change
   meaning. Relations, test links, Decisions, rationale and human reference attach to it.
2. **The address is a derived *meaning fingerprint*** (see `CONTEXT.md`): a digest of the canonical form,
   computed by the Tier-1 rules at a given `RULES_V`. It is used for exactly two things — **drift**
   against a signed Baseline (has this claim's meaning moved since it was signed?) and **duplicate
   hints** (two claims with equal fingerprints are candidates for a human to ratify as one; req never
   merges them itself, ADR 0004).
3. The asymmetric cost is unchanged. A false merge — two different meanings sharing a fingerprint —
   still silently deletes a requirement, is always a failure, and `FMR = 0` remains a hard gate. Calling
   the address a fingerprint changes what it is *named*, not what a collision costs.
4. Nothing about the on-disk format, the digest tag (`req.claim.v1`), the Baseline envelope or any
   command changes with this ADR.

## Consequences

**Enabled, not performed.** If the address is a derived fingerprint, a later claim format could stop
storing `canonical.form` and `address` in claim files, compute them at load, and store fingerprints only
in Baseline manifests. A `RULES_V` bump would then rewrite no claim file: today it makes every stored
address a *re-canonicalization candidate* (`docs/canonical-hashing.md` §5) and a fill-and-commit churn.
That change is **not** made here. It needs its own ADR, a claim-schema bump (`req.claim/<n>`), and a
decision about `req canon --verify`; until then claim files keep storing both fields and
`req canon --fill` remains their only author.

**The loss that move names.** Today "a hand-edited digest is a critical finding" (`AGENTS.md`;
`req canon --verify` exits 1 on mismatch) precisely because a digest is *stored* beside the statement it
claims to summarise. Without a stored digest there is nothing to hand-edit, and that finding disappears
with it. What replaces it is per-claim **drift against a signed manifest**: the Baseline attests each
applying claim's fingerprint, and `req baseline --verify` / `req audit` re-derive and compare. That is a
weaker guarantee for claims not yet in any signed Baseline — a claim edited between signings has no
stored digest to disagree with — so the move trades an authoring-time check for a signing-time one. It
is named here so that the future format ADR has to answer it, not rediscover it.

**Vocabulary.** `CONTEXT.md` gains **Meaning fingerprint** and points **Address** at it; prose that
calls the address "identity" is corrected where it is found (README bet section; the identity sentences
in `docs/design.md` §1 and `docs/canonical-hashing.md` §1 point here). Older ADRs keep their text; ADR
0001 gains a status line, following the 0008 → 0009 precedent.

## Considered Options

- **Keep the address as identity** (the status quo, ADR 0001's wording). Rejected: every reference in the
  model already uses `claim_id`, and an address-as-identity reading makes a tightened bound a *new*
  requirement that orphans its relations, tests and Decisions. It also invites the over-reading that two
  claims with the same address are "the same requirement" in a sense stronger than the rules licence —
  the one claim the measurement can least support (see Context).
- **Drop the address, compare statements textually.** Rejected: ADR 0001's reason for content-addressing
  stands; drift detection and duplicate hints need a meaning-level comparison, and the Baseline already
  attests addresses.
- **Perform the stop-storing change in this ADR.** Rejected as scope: it changes the claim format, the
  `--verify` contract and the gate, and deserves its own decision once the owner has ratified the
  framing.
