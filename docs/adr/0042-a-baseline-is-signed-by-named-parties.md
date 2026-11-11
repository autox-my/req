# 0042. A Baseline is signed by named parties, each key carrying one party and one role

Extends ADR 0005 (authority is a signed Decision), ADR 0010 (minisign over a committed trust file), ADR 0016 (the signing
time) and ADR 0033 (a consumer pins an external corpus). Nothing is superseded. **No manifest envelope change**: every party
signs the same manifest bytes. No rules change, no claim-schema change.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-11, GitHub PR #52). Asked for by the owner on
2026-11-11 ("Multi-party collaboration: role-based access, contractual sign-off workflows, and cross-organization visibility
— the features needed for OEM↔Tier-1 co-development"), and sequenced by the owner after the unrelated claims were resolved.
Date proposed: 2026-11-11. This ADR decides the contract; the code follows in its own pull request, test first.

**Implementation note (2026-11-12, task A-1).** Decisions 1–6 are implemented: `party`/`role` on a trust key; keyed
signature files judged beside the default `.minisig`; `.req/signing`; `req baseline --verify` naming every signature
judged; `req decision --verify` judging `DEC-NNNN.yml.<key-id>.minisig` over `decision::canonical_bytes`; the
completeness step counting a manifest with any signature file. Choices the ADR left open: a key id in a signature file
name is `[A-Za-z0-9._-]+`; a signed Decision's key is judged at the record's own `date`, and a key that holds no `role`
cannot sign a Decision (FAIL); a key expired at that date is FAIL as at a Baseline's signing time; a Decision key that is
unknown or whose verifier cannot answer is UNVERIFIABLE. Falsification 1–5 are tests (`tests/party_signatures.rs`); 6 is
the same test run with the network refused. No manifest, claim-schema or rules change.

## Context

ADR 0005 ends: *"The tool must model roles and authority."* It does not yet (`docs/design.md` §4 row 27, §6 item 8):

- a Decision's `role` is free text (`decisions/DEC-0001.yml`: `role: maintainer`), and its `actor` is a name nobody checks;
- `baselines/trust.yml` lists keys with a validity window and nothing about who holds them or in what capacity;
- a Baseline has exactly one signature, `<manifest>.minisig`, checked against one key (`req baseline --verify --key`).

OEM↔supplier co-development needs a release that *both* sides have signed, each in a named capacity: an OEM's
requirements owner approves the claim set, a supplier's safety manager accepts it, and either side can show the other's
signature offline, years later, to an assessor.

**Prior art.** *Read at source:* the TUF specification (`theupdateframework/specification`, `tuf-spec.md` at commit
`e2bc7d83d074`) gives every role one or more keys and a **threshold** of signatures, and says a signature list should hold
one signature per key id so that one key "being counted erroneously towards the minimum threshold" cannot happen. *From
memory, not re-read (leads):* in-toto layouts name the functionaries allowed to sign each supply-chain step; 21 CFR Part 11
§11.50 asks that a signature show the signer's name, the time and the **meaning** (review, approval, responsibility); ALM
tools (Polarion, Jama, DOORS Next) implement e-signatures as server-side workflow states bound to user accounts.

**What separates req from the ALM tools.** They hold the signature in a server's database; the auditor trusts the server.
req's Baseline is a file that verifies offline (ADR 0010), so the only design that keeps that property is one where each
party's authority is a key in a committed trust file and each signature is a file beside the manifest.

## Decision

1. **A key carries one party and one role.** A `baselines/trust.yml` entry gains two optional fields: `party` (an
   organisation or function, e.g. `oem`, `acme-tier1`) and `role` (the capacity, e.g. `requirements-owner`,
   `safety-manager`). The role **is the meaning** of every signature the key makes (Part 11's "meaning"): a person who acts
   in two capacities holds two keys. This is ADR 0010's purpose-specific key carried one step further, and it keeps
   signing authority separate from push authority (the git host's access control, which req does not replace).
2. **A Baseline may carry one signature per key.** Beside `<manifest>.json`, a party's signature is
   `<manifest>.json.<key-id>.minisig`. Every party signs **the same bytes**, so a signature can be added later without
   re-emitting the manifest and without invalidating any signature already there — a supplier countersigns what the OEM
   signed. The existing `<manifest>.json.minisig` stays valid and is read as a signature by the `--key` key (today's
   default), so every committed Baseline verifies unchanged.
3. **A repository may declare a signing policy** in a new optional `.req/signing` (grammar as the other `.req/` lists): one
   line per required signature, `<party> <role>`. A line repeated *n* times needs *n* signatures from distinct keys holding
   that party and role (TUF's threshold, with TUF's rule that one key counts once).
4. **Verification judges every signature, then the policy.**
   - Each present signature is verified against its key under ADR 0010/0016: a signature that does not verify, or a key
     revoked at the signing time, is **FAIL** — exactly as today. A signature file naming a key the trust file does not hold
     is UNVERIFIABLE, never ignored.
   - With a policy: every line satisfied by a distinct verified signature is **verified**; any line unsatisfied is
     **UNVERIFIABLE**, naming the missing party and role ("awaiting `acme-tier1 safety-manager`"). Awaiting a party is not a
     defect and not a pass (REQ-0041).
   - With no policy: one verified signature, as today. No existing repository changes behaviour.
5. **A Decision may be signed, and then its role is checked.** A Decision record may carry
   `decisions/DEC-NNNN.yml.<key-id>.minisig` over its canonical bytes (ADR 0010 already names per-Decision signatures as the
   opt-in for regulated flows). When it does, the record's `role` must equal the key's `role` and its `actor` is reported
   beside the key's `party`; a mismatch is **FAIL**. An unsigned Decision stays valid and is reported as *asserted* — its
   role is a claim, not a proof. This is what meets ADR 0005's "must model roles": a role becomes a property of a key
   someone was trusted with, and a role claimed without one is labelled as such.
6. **Exchange across organisations is a pinned corpus plus keys, offline.** A supplier pins the OEM's corpus at a commit
   (ADR 0033) and commits the OEM's public key to its own trust file with `party: oem`. The OEM's Baseline and signatures
   then verify in the supplier's checkout with no network (REQ-0012). The supplier's countersignature over the same manifest
   travels back as one file. Transport is git or a file; req has no server and gains none.

## Not decided here

- **Partial visibility** — giving a supplier a projection of only the claims allocated to it, with the rest withheld but
  still attested (a redacted manifest whose hidden entries are committed as digests). It needs its own ADR: it changes the
  manifest and the projection contracts.
- **Access control** — who may read or push stays with the git host. req verifies authority; it does not grant access.
- **Signing ceremony and key custody** — HSMs, OpenPGP/X.509 schemes (ADR 0010 left them pluggable), key generation UX.
- **ReqIF round-trip of signatures** — the ReqIF readers (`tools/reqif_import.py`) import claims, not authority.

## Consequences

- A release can now be shown to be accepted by both sides of a contract, each in a named capacity, from files in git, by
  anyone with the public keys and `minisign`.
- `baselines/trust.yml` becomes the register of who may sign in what capacity — a reviewed surface whose diff is a change of
  authority. Adding a party is a pull request that the owner merges, as rotating a key already is (AGENTS.md).
- `req check`'s Baseline step can be UNVERIFIABLE for a reason that is not a defect ("awaiting a party"); `docs/adopting.md`
  must say how to tell that apart from a broken signature, and the CI step stays blocking (a release awaiting a party is
  not ready to ship).
- Cost: N signature files per Baseline instead of one, and a policy line per required party; a repository with one signer
  writes neither.

## Rejected

- **Roles in a separate file from keys.** It lets a key's meaning drift without the key entry changing in the same diff.
  The key is the authority; its meaning belongs on the same record.
- **One signature over a list of signers (an envelope of signatures).** It changes the manifest bytes whenever a party
  signs, so a countersignature would invalidate the first signature. Detached signatures over identical bytes keep each
  party's act independent.
- **Inferring the role from the git commit author.** ADR 0005 rejected the commit as authority: the forge account proves
  push access, not capacity.
- **A server-side workflow (states, approvers, notifications).** It would put authority in a database the auditor must
  trust, and a network dependency in the core.

## Falsification

This decision is wrong if any of these is observed:

1. a Baseline verifies under a signing policy while a required party/role has no verified signature;
2. one key's signature is counted twice towards a repeated policy line;
3. adding a second party's signature changes the manifest bytes or invalidates the first signature;
4. a repository with no `.req/signing` and no `party`/`role` fields verifies differently than before this decision;
5. a signed Decision whose `role` differs from its key's role verifies;
6. verifying another organisation's Baseline needs any network access.
