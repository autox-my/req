# 0057. The formats an adopter writes or signs are stable from `req.claim/4` and envelope v5

Extends ADR 0022 (`req.claim/2`), ADR 0051 decision 6 (an older claim format loads as it is), ADR 0056 (`req.claim/4`,
`req.kind/1`, envelope v5) and ADR 0046 decision 9 (`req.trace/1` changes additively; this extends that rule to every JSON
document). It constrains every later ADR that would change one of the formats named in decision 1. It changes no format
and no address. No `RULES_V` bump, no claim-schema change, no Baseline envelope change. Two promises need a mechanism
that does not exist yet, and decision 5 makes building them part of the implementing change.

**Status: proposed.** The owner ratifies it by merging. Asked for by the owner on 2026-11-18, after an external review
(not quoted here) called req "risky as a dependency" because of format churn. Date proposed: 2026-11-18. The formats it
names shipped in `v0.6.0` (rev `161c336`).

## Context

The claim format moved twice between 2026-11-04 and 2026-11-18: from `req.claim/2` (ADR 0022) to `/3` (ADR 0051, `area`),
then to `/4` (ADR 0056, `kind` and `fields`). Neither move made an adopter edit a file. `/2` and `/3` stay on `READS_AS_IS`
(`src/canon.rs:84`), so a `/4` build loads them as they are.

The Baseline envelope moved from v3 to v4 on 2026-10-22 (ADR 0019). That was before releases carried a version: it shipped
in the `rev-*` releases of 2026-11-09 onward. It moved again to v5 on 2026-11-18 (ADR 0056), and v5 shipped in `v0.6.0`, a minor release whose
notes said identity moved (ADR 0055 decision 2). Each envelope move asked adopters to re-emit and re-sign to gain the new
coverage.

Nothing has said that this will stop. Someone who depends on req today cannot tell a project that has settled its formats
from one that changes them every week.

ADR 0056 was the last planned format change. The repository-checks ADR that follows it adds a new file under `.req/` and
needs no change to a claim, a declaration or a manifest. This is the point to say what is stable.

## Decision

1. **The stable formats.** From `v0.6.0` on, everything an adopter writes into their repository, or signs, is stable.
   - **Claims and declarations:**
     - the claim file, `req.claim/4`, with `/2` and `/3` read as they are;
     - the kind declaration, `req.kind/1`;
     - `aliases/subjects.yml`.
   - **Every file under `.req/`** that a release reads, in the grammar its `--help` page or `docs/claim-format.md` states.
     Today that is `variants`, `projections`, `completeness`, `areas`, `signing`, `design` and `kinds/`, plus `satisfied`,
     which records the human act `req satisfy --record` performs (`schema: req.satisfied/1`, ADR 0040). A file a later
     release adds under `.req/` joins this list when it ships. One file is left out: **`.req/reserved`**. It is
     transient coordination state that only `req next-id` reads, branches edit it concurrently, and no gate or Baseline
     step sees it.
   - **The signed record:**
     - the Baseline envelope, v5, with v3 and v4 verified over the bytes they were signed over;
     - the trust anchor `baselines/trust.yml` and the signature file names beside a manifest (ADR 0010, ADR 0042);
     - the decision records under `decisions/`, and the hash rule that chains them (`decision_chain 1`).
   - **What an adopter puts in their own files for req to read:**
     - the `req-satisfies: <claim-id>` marker in source or design files (ADR 0040);
     - the header req writes into a committed projection and `req render --check` / `req trace --check` judge (ADR 0018);
     - the test id a claim's `tests:` names and a JUnit report carries (`classname::name`, ADR 0019).
   - **Every JSON document a release emits**, `req.*/N`. Today that is `req.render/1`, `req.trace/1`, `req.gate/2`,
     `req.audit/1`, `req.check/1`, `req.diff/1` and `req.satisfy/1`. It extends ADR 0046 decision 9's rule for
     `req.trace/1` to all of them: a key may be **added**, and a key is never removed, renamed, retyped or reordered without
     a new schema number. A document a later release adds joins this list when it ships.
2. **What stable means.** Every later release does all of the following:
   - It loads a `req.claim/4` file, and a `/2` or `/3` file, **as it is**: the same claim, and the same address under the
     same `RULES_V`, with **no message** and no rewrite.
   - It reads a `req.kind/1` declaration as it is.
   - It verifies a v5 manifest over the bytes it was signed over, and v3 and v4 manifests as today. This holds under the
     `RULES_V` the manifest was signed at, against the same trust anchor and decision log, with a `minisign` that verifies
     minisign's signature format. (req calls `minisign` as an external program; its signature format is minisign's
     contract, not req's.)
   - It verifies every decision record written under `decision_chain 1` as untampered.
   - It reads every file named in decision 1 in its current grammar.

   A later format may **add**. Examples: a `req.claim/5` with a new optional key; a `req.kind/2` with a new field type; an
   envelope v6 that attests one more surface; a decision chain v2 for new records. Each one is an ADR, and each keeps the
   version before it readable. A new envelope asks for a re-sign only to gain the new coverage; the old manifest keeps
   verifying.
3. **A break needs a superseding ADR, and a release that says so.** A break is any change that makes a file named in
   decision 1 do one of four things:
   - stop loading as it is;
   - become a migration candidate;
   - stop verifying;
   - be read with a different meaning.

   A break is decided only by an ADR that supersedes this one for that format. That ADR names the break, ships a
   migration tool or the steps, and says why adding was not enough. The release that carries it says "migrate" in its
   notes. A break is not always an identity move or a command change, so the release workflow cannot detect it, and ADR
   0055 decision 2 does not force the number. The bump pull request declares the release **minor** (ADR 0055 decision 4:
   the bump PR declares what history cannot decide), so the release is never a patch. That is why the rule lives in an
   ADR and not in the version number.

   The release notes cannot say "migrate" on their own today. `scripts/release-notes.sh` compares only `RULES_V`, the
   claim schema and the envelope, so a break in another format would ship with notes that say "No identity change". The
   superseding ADR's implementing change must therefore make the notes say it, for example by adding that format's
   version to the identities the script compares. A break whose release notes do not say "migrate" is a falsification
   of this decision.
4. **What this does not freeze.**
   - **`RULES_V`.** A Tier-1 rule change moves addresses on purpose (hashing spec §5). It is the bet being measured, not
     a format. A manifest whose `rules_v` differs from the build's stays UNVERIFIABLE and is re-signed (ADR 0016,
     "`rules_v` mismatch stays UNVERIFIABLE"). That does not change. A rules bump stays a deliberate, recorded event,
     never opportunistic (AGENTS.md).
   - **Commands, flags and text output.** These are contracts on each `req <command> --help` page and move under ADR
     0055's minor rule. Text output is for people; a script reads a JSON document.
   - **Tools copied into an adopter's project** (`tools/adopt.py`, `tools/needs_import.py`, `tools/reqif_import.py`).
     They are copied and adjusted, and they keep drafting the oldest format req reads as it is.
5. **It is enforced by tests and by two mechanisms, built in the implementing change.**
   - **Kind declarations get a read-as-is list.** Today `src/kinds.rs:320-327` accepts only `KIND_SCHEMA_V`. A newer
     declaration is NOT JUDGED (`:324`, `:298`), and an older or malformed one is refused as a malformed declaration
     (exit 2). No older version exists yet, but the first `req.kind/2` build would refuse every existing `req.kind/1`
     file. The implementing change adds the mechanism claims have, a `KIND_READS_AS_IS` list stated once as data, and an
     arm that loads a listed older version as it is. Every later `req.kind/N` puts the one before it on the list.
   - **The decision chain must not reinterpret old records.** A record carries no version today. The chain version is
     part of the hashed bytes (`src/decision.rs:74-75`), and the build fills it in from its own compile-time constant
     (`DECISION_VERSION`, `:36`, used at `:223`). So a build at `decision_chain 2` would report every existing record
     TAMPERED, a false accusation against ADR 0005's authority chain. The implementing change does two things:
     - **The record says its version.** A record with no version key is a v1 record. A later chain version writes a
       key that v1 records lack, so the verifier can tell them apart.
     - **The verifier takes the version as a parameter.** It recomputes a record's hash under the record's own version,
       never the build's constant, as the claim path takes `build_v` (`src/canon.rs:133`). A test verifies a v1 record
       through that seam with a raised version, and it must still be untampered.
   - **A committed fixture**, `tests/fixtures/stable/`, holds:
     - claim files at `/2`, `/3` and `/4`, the `/4` ones carrying `area`, `kind` and fields of every type;
     - a `req.kind/1` declaration, and every `.req/` file named in decision 1;
     - a decision log;
     - a projection with its header;
     - a source file with a `req-satisfies:` marker and its `.req/satisfied` record;
     - a JUnit report naming a linked test;
     - a v5 manifest over the corpus, with its trust anchor and a signature made with a test key held in the fixture.

     Its test requires that every build does each of these:
     - loads each claim with no message;
     - derives the recorded address;
     - reads every `.req/` file;
     - reads the projection's header (`render --check` judges it current or stale, never unreadable);
     - judges the `req-satisfies:` link and the test link as recorded;
     - verifies the decision log and the manifest.

     The recorded addresses and the manifest are regenerated **only** on a `RULES_V` bump, by the rules-change
     protocol, and the commit says so. Editing the fixture any other way is a break (decision 3).
   - **A test requires** that `READS_AS_IS` holds every claim format from 2 up to the one before `CLAIM_SCHEMA_V`, and
     that `KIND_READS_AS_IS` holds every declaration format from 1 up to the one before `KIND_SCHEMA_V`.
   - **One registry of JSON schema strings.** Today the `SCHEMA_*` constants are spread across `src/format.rs:27-32`,
     `src/check.rs:35`, `src/diff.rs:43` and `src/satisfy.rs:45`, so a hand-kept list could miss a new one. The
     implementing change gathers them into one list in `src/format.rs` that every emitter uses. A test requires two
     things:
     - each entry is named in decision 1;
     - a scan of `src/` for `"req.<name>/<n>"` literals finds a positive number of them, and each one is in the registry
       or is a file format named in decision 1.

     A new document fails the test until it is added in both places.
6. **1.0 is not decided here.** Stable formats are a precondition for 1.0, not 1.0 itself. ADR 0055 leaves the timing of
   1.0 to the owner, and ties it to evidence for the bet beyond the seed corpora. This ADR does not change that.

## Consequences

- An adopter can depend on what they wrote or signed at `v0.6.0`: claims, declarations, `.req/` files, markers, test ids,
  projections and manifests stay readable and verifiable across releases. Four things can still ask them to act:
  - a `RULES_V` bump, which means re-signing, and the notes say so;
  - a new envelope they choose to adopt, which means re-signing for more coverage;
  - a change to a projection's text: its header is stable, but its body is text output, which a minor release may
    change (decision 4). `render --check` then reports it stale and the adopter re-renders with `--out`;
  - a break, which needs an ADR and a release that says "migrate".
- Two mechanisms are new code, the kind read-as-is list and the decision-chain rule. Until the implementing change lands,
  this ADR describes the promise and not yet its enforcement.
- The README's *Status* states the stable formats and links here, in the implementing change.
- A future feature that seems to need a breaking format change must first try an additive one, and say in its ADR why
  that was not enough.
- The fixture is one more thing a rules bump regenerates. That cost is deliberate: the diff of the commit that bumps the
  rules shows its effect on an existing manifest and projection.

## Rejected

- **Declare 1.0 now.** ADR 0055 *Rejected* already turned this down: 1.0 needs evidence for the bet beyond the seed
  corpora. Stable formats and a stable bet are different promises.
- **Freeze `RULES_V` too.** That would stop the Tier-1 rules from being corrected, for example R-1's `no more than` →
  LE, which fixed a wrong direction. A rules bump is how a measured mistake gets fixed.
- **Say nothing and keep changing formats additively.** Every claim-format change so far was additive, but nothing said
  so, and a reader cannot tell a habit from a rule.
- **A fixed list of formats.** It would leave out whatever ships next. Decision 1 names categories ("every file under
  `.req/`", "every JSON document") and a test enumerates the members.
- **A deprecation window instead of a superseding ADR.** A window promises a timetable for breaks. This ADR promises that
  a break is an exception that has to be argued for.

## Falsification

This decision is wrong if any of these is observed:

1. a release fails to load a `req.claim/2`, `/3` or `/4` file as it is, prints a message for one, or derives a different
   address for it under the same `RULES_V`;
2. a release refuses a `req.kind/1` declaration, or reads it with a different meaning;
3. a release fails to verify a v5 manifest (or a v3 or v4 one) over the bytes it was signed over, under the `RULES_V` it
   was signed at and the same trust anchor and decision log;
4. a release reports a decision record written under `decision_chain 1` as TAMPERED when its bytes have not changed;
5. a JSON document under an unchanged schema number loses, renames, retypes or reorders a key, or a release emits a
   `req.*/N` that decision 1 does not cover;
6. a `.req/` file, `aliases/subjects.yml`, `baselines/trust.yml`, a `req-satisfies:` marker, a projection header or a
   linked test id that a release accepted stops being accepted, or is read with a different meaning, in a later release;
7. a format named in decision 1 changes incompatibly without an ADR that supersedes this one for it;
8. the stable fixture is edited outside a `RULES_V` bump without such an ADR;
9. a break ships in a release whose notes do not say "migrate".
