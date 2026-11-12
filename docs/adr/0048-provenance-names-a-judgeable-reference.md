# 0048. A rationale's source may name a judgeable reference, offline

Narrows the unjudged half of `rationale_present` (ADR 0023's policy list). Extends ADR 0013 (audit-surface hashes) by
giving provenance something to be judged against, without adding a surface. No `RULES_V` bump, no claim-schema change,
no Baseline format change, and **no address moves**: `rationale` sits outside the canonical form and is attested by
`rationale_digest` (ADR 0013), so editing a source cannot move an address and editing one after signing is already FAIL.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Raised from a comparative
review of an adopter-surface tool (reqord, `f043777`) that records `origin.feedbackIssue` as a number — machine-readable
and unverifiable — where req records a source it can check but does not check. Date proposed: 2026-11-12.

## Context

`rationale` is required and has four required fields. It is outside identity and inside the signature: a provenance edit
moves no address, and a provenance edit after signing turns a Baseline FAIL rather than passing silently. That is
already the right algebra for provenance, and it is why this decision is cheap.

What is missing is judgement. `rationale_present` checks one thing: a `source` that is a repo path **with a line anchor**
(`docs/plan.md#L390`) must name a file that exists and has that many lines. The contract is explicit that everything else
is not judged — "a source with no anchor, a heading anchor, a URL, prose or a path outside the repository is not judged"
(`docs/claim-format.md`).

So today a claim that exists because of a recorded Decision cannot say so in a way the gate can confirm, and
`source: https://github.com/autox-my/req/issues/22` is inert text. "Why does this claim exist?" is answerable by reading
prose and by nothing else — which also means an agent asked to judge a rationale can only pattern-match it.

## Decision

1. **A source may name a reference in a fixed prefix grammar**, judged offline:
   - `dec:<DEC-id>` — the decision exists in `decisions/` and appears in the verified hash chain;
   - `adr:<number>` — resolves to `docs/adr/<number>-*.md`;
   - `claim:<id>` — the id is a claim in the corpus;
   - `issue:<n>` and `pr:<n>` — **well-formed only, never fetched** (REQ-0012: nothing leaves the machine).
2. **A repo path keeps today's behaviour**, with or without a line anchor, including "a path outside the repository is
   not judged". The prefixes are additions, not a replacement.
3. **Only the shape of a tracker reference is judged.** A malformed reference is a defect — it is an authored
   reference this build cannot read, and the author fixes it. A well-formed one is recorded and unjudged in the strict
   sense: the gate does not claim issue #22 says what the claim says, and says so rather than staying silent.
4. **The judgement stays in `rationale_present`.** Its `checked` count already distinguishes a pass over zero from a pass
   that held some; it now counts references as well, and names the references that failed, by claim and by source.
5. **Still outside identity, still inside the signature, nothing derived stored.** `rationale` does not enter the
   canonical form; the digest and the Baseline format are unchanged, so a corpus whose rationale text does not move needs
   no re-signing. The existence of a reference is computed at gate time and never written into a claim (ADR 0022).
6. **A dangling reference is a red, not UNVERIFIABLE.** The referent is a file the corpus controls, so its absence is a
   defect in the corpus, not a limit of the check — unlike a tracker reference, which the air gap makes unanswerable.

## Consequences

- A claim can say `source: dec:DEC-0007` and have the gate prove that decision exists and is in the chain. A claim whose
  reason was deleted fails the gate instead of reading as justified.
- Adopters whose tracker is not GitHub, or is private, lose one prefix and keep the three internal ones. The internal
  three are the ones that carry the most: they are what makes "the decision that authorised this claim still exists"
  checkable by a machine.
- A tracker reference remains text. req cannot verify that an issue says what a claim says, and the gate now states that
  limit next to the reference instead of leaving the reader to assume otherwise.
- The prefix grammar is a new authored convention, so `docs/claim-format.md` and `req gate --help` both change in the
  same commit that lands the check.

## Rejected

- **Fetching the URL to confirm it exists.** REQ-0012 forbids the network, and a verdict that depends on a socket is
  UNVERIFIABLE in exactly the deployment req is built for.
- **A new `origin:` object or a `req.claim/3` schema.** A schema bump, a second provenance surface, and a migration, for
  judgement the existing required `source` slot can already carry. `rationale` deliberately has one place to say where a
  claim came from.
- **Parsing free prose for a reference.** Deciding which words in a sentence are a reference is the ambiguity class
  claims exist to remove; the author states it in a grammar or it stays prose.
- **Making `issue:`/`pr:` a failure when it cannot be reached.** That is a network check wearing a judgement's clothes.
- **Judging a heading anchor.** A heading can be renamed; the current contract already prefers a path alone, and this
  decision does not widen what a path claim promises.

## Falsification

This decision is wrong if any of these is observed:

1. `dec:` naming a decision that is absent, or present but outside the verified chain, passes;
2. `adr:0043` passes when no such file exists, or fails when `docs/adr/0043-*.md` does;
3. any path in `rationale_present` performs network I/O, or a well-formed tracker reference changes a verdict;
4. editing a rationale moves an address, or leaves a signed Baseline PASS;
5. a well-formed but unresolvable internal reference is reported as PASS instead of named.
