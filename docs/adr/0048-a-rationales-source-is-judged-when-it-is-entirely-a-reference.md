# 0048. A rationale's source is judged when it is entirely a reference

Narrows the unjudged half of `rationale_present` (ADR 0023's policy list), and gives ADR 0013's provenance surface
something to be judged against without adding a surface. No `RULES_V` bump, no claim-schema change, no Baseline format
change, and **no address moves**: `rationale` sits outside the canonical form and is attested by `rationale_digest`
(ADR 0013), so editing a source cannot move an address and editing one after signing is already FAIL.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Raised from a comparative
review of an adopter-surface tool (reqord, `f043777`) that records `origin.feedbackIssue` as a number —
machine-readable and unverifiable — where req records a source it can check but does not. Revised 2026-11-13 after
review (PR #66): the first version proposed a `dec:` / `adr:` / `claim:` prefix grammar, which duplicates a form
`docs/claim-format.md` already documents and which **no claim in this corpus uses**, while a plain one-token path rule
covers all 40. Date proposed: 2026-11-12.

## Context

`rationale` is required and has four required fields. It is outside identity and inside the signature: a provenance
edit moves no address, and a provenance edit after signing turns a Baseline FAIL rather than passing silently. That is
already the right algebra for provenance, and it is why this decision is cheap.

What is missing is judgement. `rationale_present` checks one thing: a `source` that is a repo path **with a line
anchor** must name a file that exists and has that many lines. The contract is explicit that everything else is not
judged — "a source with no anchor, a heading anchor, a URL, prose or a path outside the repository is not judged"
(`docs/claim-format.md`).

The gap is not hypothetical. **All 40 claims in this corpus give a `source` that is a single token with no anchor**, and
**none is judged**: `docs/canonical-hashing.md` (7), `docs/design.md` (4), `docs/adr/0004-…` (4), `docs/claim-format.md`
(3), and so on down to `AGENTS.md` and `src/main.rs`. The gate cannot say whether a single one of them still exists. A
claim whose justifying document was deleted reads exactly like one whose source is current.

Meanwhile `source: https://github.com/autox-my/req/issues/22` is inert text, and "why does this claim exist?" is
answerable by reading prose and by nothing else — which means an agent asked to judge a rationale can only pattern-match
it.

## Decision

1. **A source that is entirely one token is a reference, and is judged.** One token means no whitespace and no `:`. So
   `docs/adr/0004-….md`, `AGENTS.md` and `src/main.rs` are references; a URL (`https://…`) and any prose sentence are
   not. This is a syntactic class, not a guess about intent.
2. **Such a token is a repo-relative path, and must exist.** With a line anchor, today's rule applies unchanged: the
   file exists and has that many lines. Without one, existence is the whole claim, and a dangling path is a defect.
3. **A one-token claim id or address is judged as a member of the corpus.** `docs/claim-format.md` already says reference
   types — `source` included — use "`claim_id`s, addresses, or repo paths". This decision is what makes that sentence
   true at the gate.
4. **Everything else stays unjudged, and the gate says so.** A URL, a prose sentence, a tracker reference (`pr:64`,
   `issue:#22`, which contain a `:`), a path outside the repository, a heading anchor: each is recorded and not judged.
   No network call, ever (REQ-0012). req never claims that issue #22 says what the claim says.
5. **No prefix grammar and no new authoured convention.** A source that was prose yesterday is prose today, because it
   contains whitespace or a colon, so the upgrade cannot turn a valid corpus red for a form it did not change.
6. **Chain membership stays `req decision`'s question, not the rationale's.** A `decisions/DEC-*.yml` cited as a path is
   judged for existence like any other file. Whether that entry is in the verified hash chain is a question with one
   home (ADR 0005), and duplicating it here would give one rule two answers.
7. **The judgement stays in `rationale_present`.** Its `checked` count already distinguishes a pass over zero from a
   pass that held some; it now counts references as well, and names every reference that failed, by claim and by source.
8. **Three states, as everywhere.** A dangling path, an absent claim id, a source outside the repository that was
   written as a path — those are **FAIL**, because the corpus controls them. A corpus or registry that cannot be read is
   **UNVERIFIABLE** (exit 3). An unjudged form changes no verdict and is named as unjudged, never folded into PASS.
9. **Still outside identity, still inside the signature, nothing derived stored.** `rationale` does not enter the
   canonical form; the digest and the Baseline format are unchanged. Existence is computed at gate time and never
   written into a claim (ADR 0022).

## Consequences

- On the next gate run, every one of the 40 sources in this repository becomes judged. A claim whose justifying document
  was deleted or renamed now fails, where today it reads as justified.
- An adopter who cites a document by path gets the same protection for free, with no new convention to learn.
- A tracker reference remains text. That limit is now stated next to the reference instead of being left to the reader:
  the gate can confirm an in-repo document exists, and cannot confirm what a URL says.
- `docs/claim-format.md` and `req gate --help` both change in the same commit that lands the check, since the judged set
  is part of the policy's contract.
- No address moves, no `RULES_V` bump, no re-signing for a corpus whose rationale text does not change.

## Rejected

- **The `dec:` / `adr:` / `claim:` prefix grammar (this ADR's first version).** `claim:` duplicates a form the claim
  format already documents; `adr:` covers 18 of the 40 sources where the plain rule covers 40; `dec:` has **zero** users
  in this corpus. A new authored convention that judges a strict subset of what a syntactic class already reaches.
- **Judging `decisions/` paths against the hash chain here.** Zero sources cite a decision today, so it would be
  invented demand, and ADR 0005 already owns that question.
- **Fetching the URL to confirm it exists.** REQ-0012 forbids the network, and a verdict that depends on a socket is
  UNVERIFIABLE in exactly the deployment req is built for.
- **A new `origin:` object or a `req.claim/3` schema.** A schema bump, a second provenance surface and a migration, for
  judgement the required `source` slot can already carry.
- **Parsing free prose for a reference.** Deciding which words in a sentence are a reference is the ambiguity class
  claims exist to remove; the author writes one token or it stays prose.
- **Judging a heading anchor.** A heading can be renamed, and failing a claim for a documentation edit is the wrong
  coupling. The current contract already prefers a path alone.

## Falsification

This decision is wrong if any of these is observed:

1. a source that is one token, is a path, and does not exist is reported as PASS;
2. a source that is prose, a URL, or a tracker reference (`pr:64`) changes any verdict;
3. a one-token claim id that the corpus does not hold is reported as PASS;
4. editing a rationale moves an address, or leaves a signed Baseline PASS;
5. an unjudged form is folded into PASS instead of being named as unjudged.
