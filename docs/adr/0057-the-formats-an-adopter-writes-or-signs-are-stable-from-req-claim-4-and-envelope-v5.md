# 0057. The formats an adopter writes or signs are stable from `req.claim/4` and envelope v5

Extends ADR 0022 (`req.claim/2`), ADR 0051 decision 6 (an older claim format loads as it is), ADR 0056 (`req.claim/4`,
`req.kind/1`, envelope v5) and ADR 0046 decision 9 (`req.trace/1` changes additively; this extends that rule to every JSON
document); constrains every later ADR that would change one of the formats named in decision 1. It
changes no code, no format and no address. No `RULES_V` bump, no claim-schema change, no Baseline envelope change.

**Status: proposed.** The owner ratifies it by merging. Asked for by the owner on 2026-11-18, after an external review
(not quoted here) called req "risky as a dependency" because of format churn. Date proposed: 2026-11-18. The formats it
names shipped in `v0.6.0` (rev `161c336`).

## Context

Between 2026-11-04 and 2026-11-18 the claim format moved twice: from `req.claim/2` (ADR 0022) to `/3` (ADR 0051, `area`)
and then to `/4` (ADR 0056, `kind` and `fields`). The Baseline envelope moved from v4 (ADR 0019, 2026-10-22) to v5 on
2026-11-18. None of the claim-format moves made an
adopter edit a file: `/2` and `/3` stay on `READS_AS_IS` (`src/canon.rs:79`), so a `/4` build loads them as they are. The
envelope moves did ask every adopter to re-emit and re-sign to gain the new coverage, and each one shipped in a minor
release that the notes called identity-moved (ADR 0055 decision 2). Nothing has said that this will stop. Someone who
depends on req today cannot tell a project that has settled its formats from one that changes them every week.

ADR 0056 was the last planned format change: the repository-checks ADR that follows it adds a new file under `.req/` and
needs no change to a claim, a declaration or a manifest. This is the point to say what is stable.

## Decision

1. **The stable formats.** From `v0.6.0` on, these are stable, with the version each one is at today (`req --version`
   reports the first, third and fourth):
   - the claim file, `req.claim/4`, with `/2` and `/3` read as they are;
   - the kind declaration, `req.kind/1`;
   - the Baseline envelope, v5, with v3 and v4 verified over the bytes they were signed over;
   - the decision log, `decision_chain 1`;
   - the line files under `.req/` (`variants`, `projections`, `completeness`, `areas`, `signing`, `reserved`) and
     `aliases/subjects.yml`, in the grammars their `--help` pages and `docs/claim-format.md` state;
   - the JSON documents (`req.render/1`, `req.trace/1`, `req.gate/2`, `req.audit/1`, `req.diff/1`, `req.satisfy/1`, and
     every later `req.*/N`), under the rule ADR 0046 decision 9 states for `req.trace/1`, extended here to all of
     them: a key may be **added**, and a key is never removed, renamed, retyped or reordered without a new schema
     number.
2. **What stable means.** Every later release:
   - loads a `req.claim/4` file, and a `/2` or `/3` file, **as it is**: the same claim, and the same address under the
     same `RULES_V`, with no message and no rewrite;
   - reads a `req.kind/1` declaration as it is;
   - verifies a v5 manifest over the bytes it was signed over, and v3 and v4 manifests as today;
   - verifies the decision log at `decision_chain 1`.

   A later format may **add**: a `req.claim/5` with a new optional key, a `req.kind/2` with a new field type, an
   envelope v6 that attests one more surface. Each one is an ADR, and each puts the version before it on the read-as-is
   list. A new envelope asks for a re-sign only to gain the new coverage; the old manifest keeps verifying.
3. **A break needs a superseding ADR, and a release that says so.** A break is any change that makes an existing stable
   file stop loading as it is, become a migration candidate, or stop verifying. It is decided only by an ADR that
   supersedes this one for that format. The ADR names the break, ships a migration tool or the steps, and says why
   adding was not enough. The release that carries it says "migrate" in its notes. Below 1.0 that release is a minor bump
   (ADR 0055 decision 2), the same number an additive identity move takes. That is why the rule lives here, in an ADR,
   and not in the version number.
4. **What this does not freeze.**
   - **`RULES_V`.** A Tier-1 rule change moves addresses on purpose (hashing spec §5). It is the bet being measured, not
     a format. A manifest signed under one rules version is UNVERIFIABLE under another and is re-signed (ADR 0013); that
     does not change. A rules bump stays a deliberate, recorded event, never opportunistic (AGENTS.md).
   - **Commands, flags and text output.** These are contracts on each `req <command> --help` page and move under ADR
     0055's minor rule. Text output is for people, and a script reads a JSON document.
   - **Tools copied into an adopter's project** (`tools/adopt.py`, `tools/needs_import.py`, `tools/reqif_import.py`):
     they are copied and adjusted, and they keep drafting the oldest format req reads as it is.
5. **It is enforced by a test, not only by this text.** The implementing change adds a committed fixture under
   `tests/fixtures/stable/`:
   - claim files at `/2`, `/3` and `/4`, the `/4` ones carrying `area`, `kind` and fields of every type;
   - a `req.kind/1` declaration;
   - the decision log;
   - a v5 manifest over them, signed with a test key held in the fixture.

   Its test requires that every build loads each claim with no message, derives the recorded address, and verifies the
   manifest under the fixture's trust file. The recorded addresses and the manifest are regenerated **only** on a
   `RULES_V` bump, by the rules-change protocol, and the commit says so. A test also requires that `READS_AS_IS` holds
   every claim format from 2 up to the one before `CLAIM_SCHEMA_V`. Editing the fixture any other way is a break
   (decision 3).
6. **1.0 is not decided here.** Stable formats are a precondition for 1.0, not 1.0 itself. ADR 0055 leaves the timing of
   1.0 to the owner and ties it to evidence for the bet beyond the seed corpora. This ADR does not change that.

## Consequences

- An adopter can depend on a `/4` claim, a `req.kind/1` declaration and a signed v5 manifest staying readable and
  verifiable across releases. What can still ask them to act is a `RULES_V` bump (re-sign; the notes say so), a new
  envelope they choose to adopt (re-sign for more coverage), or a break, which needs an ADR and says "migrate".
- The README's *Status* states the stable formats and links here, in the implementing change.
- A future feature that seems to need a breaking format change must first try an additive one, and say in its ADR why
  that was not enough.
- The fixture is one more thing a rules bump regenerates. That cost is deliberate: it makes the effect of a rules bump on
  existing manifests visible in the diff of the commit that makes it.

## Rejected

- **Declare 1.0 now.** ADR 0055 *Rejected* already turned this down: 1.0 needs evidence for the bet beyond the seed
  corpora. Stable formats and a stable bet are different promises.
- **Freeze `RULES_V` too.** That would stop the Tier-1 rules from being corrected, for example R-1's `no more than` →
  LE, which fixed a wrong direction. A rules bump is how a measured mistake gets fixed.
- **Say nothing and keep changing formats additively.** Every change so far was additive for claims, but nothing said
  so, and a reader cannot tell a habit from a rule.
- **A deprecation window instead of a superseding ADR.** A window promises a timetable for breaks. This ADR promises that
  a break is an exception that has to be argued for.

## Falsification

This decision is wrong if any of these is observed:

1. a release fails to load a `req.claim/2`, `/3` or `/4` file as it is, or derives a different address for it under the
   same `RULES_V`;
2. a release fails to verify a v5 manifest (or a v3 or v4 one) over the bytes it was signed over, under the `RULES_V` it
   was signed at;
3. a JSON document under an unchanged schema number loses, renames, retypes or reorders a key;
4. a format named in decision 1 changes incompatibly without an ADR that supersedes this one for it;
5. the stable fixture is edited outside a `RULES_V` bump without such an ADR.
