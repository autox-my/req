# 0048. A rationale's source is judged when it is a path-shaped reference

Narrows the unjudged half of `rationale_present` (ADR 0023's policy list), and gives ADR 0013's provenance surface
something to be judged against without adding a surface. No `RULES_V` bump, no claim-schema change, no Baseline format
change, and **no address moves**: `rationale` sits outside the canonical form and is attested by `rationale_digest`
(ADR 0013), so editing a source cannot move an address and editing one after signing is already FAIL.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Raised from a comparative
review of an adopter-surface tool (reqord, `f043777`) that records `origin.feedbackIssue` as a number —
machine-readable and unverifiable — where req records a source it can check but does not.

Revised twice after review (PR #66). The first version proposed a `dec:` / `adr:` / `claim:` prefix grammar, which
duplicates a form `docs/claim-format.md` already documents and which **no claim in this corpus uses**, while a
path-shaped rule covers all 40. The second version said "one token", which makes `TBD`, `N/A` and `interview` dangling
paths, and turned `req init`'s own template red. Date proposed: 2026-11-12; revised 2026-11-13.

## Context

`rationale` is required and has four required fields. It is outside identity and inside the signature: a provenance
edit moves no address, and a provenance edit after signing turns a Baseline FAIL rather than passing silently. That is
already the right algebra for provenance, and it is why this decision is cheap.

What is missing is judgement. `rationale_present` checks one thing: a `source` that is a repo path **with a line
anchor** must name a file that exists and has that many lines. The contract is explicit that everything else is not
judged — "a source with no anchor, a heading anchor, a URL, prose or a path outside the repository is not judged"
(`docs/claim-format.md`).

The gap is not hypothetical. **All 40 claims in this corpus give a `source` that is a relative path with a file
extension and no anchor**, and **none is judged**: `docs/canonical-hashing.md` (7), `docs/design.md` (4),
`docs/adr/0004-…` (4), `docs/claim-format.md` (3), and so on down to `AGENTS.md` and `src/main.rs`. The gate cannot say
whether a single one of them still exists. A claim whose justifying document was deleted reads exactly like one whose
source is current.

**The experiment was run, not proposed.** Applying decision 2 by hand to this corpus — every path-shaped source, tested
for existence — reports exactly one dangling reference out of forty: `claims/REQ-0034.yml` cites
`docs/adr/0026-req-diff-what-changed-in-meaning-between-two-revisions.md`, and the file on disk is
`0026-req-diff-what-changed-in-meaning-between-two-revs.md`. Nothing in the gate can see that today.

## Decision

1. **A source is judged only when it is one path-shaped token.** Path-shaped means: no whitespace, no `:`, not absolute,
   and the final segment carries a file extension (`docs/adr/0004-….md`, `AGENTS.md`, `src/main.rs`). One-word notes
   (`TBD`, `N/A`, `interview`, `customer`) are **not** path-shaped and are never judged, because a note is not a
   citation and req cannot tell a filename from a word.
2. **A path-shaped token must exist.** It is a repo-relative path and must resolve to an existing file; with a line
   anchor (`#L390`, `#L390-L395`, `#L390-395`) today's rule applies unchanged, so the file must exist and have that many
   lines.
3. **A source that is a claim id is judged as a member of the corpus.** The claim-id class is tried first; the two
   classes cannot collide, since a claim id carries no extension. `docs/claim-format.md` already names claim ids as a
   reference type for `source`; this decision is what makes that sentence true at the gate.
4. **An anchor other than a line anchor makes the whole source unjudged.** `docs/x.md#Heading` is a heading anchor, and
   `needs.json#REQ_001` is what `tools/needs_import.py` writes for provenance — the origin is often outside the corpus
   or not committed. Judging either would fail a corpus for a reference it did not make. This keeps the behaviour
   `docs/claim-format.md` describes today and closes the gap between decisions 2 and 4 in the previous version.
5. **A path outside the repository is never judged, and never FAIL.** req cannot see outside the corpus, so it cannot
   tell a dangling path from a path it is not permitted to read. `docs/claim-format.md` already says so; this decision
   does not strengthen it.
6. **Everything else is unjudged and named as unjudged:** a URL, a prose sentence, a one-word note, a heading anchor, an
   external origin, and **an address** — a full address carries a `sha256:` prefix, so it is never one path-shaped token
   and cannot reach decision 3. No network call, ever (REQ-0012). req never claims that issue #22 says what the claim
   says.
7. **The upgrade is stated, not promised away.** This rule turns a corpus red exactly where a source names an in-repo
   path that does not exist — which is the rule working. It cannot turn red for prose, a one-word note, a URL, an
   external origin or any anchor that is not a line anchor.
8. **`req init`'s claim template changes in the same commit.** It currently writes `source: docs/requirements.md`
   (`src/init.rs:328`), a file `init` does not create, so under decision 2 every fresh corpus would FAIL its own gate and
   `tools/quickstart.sh` would break. The template's source becomes `claims/README.md`, which `init` writes in the same
   run (`src/init.rs:211`).
9. **The judgement stays in `rationale_present`.** Its `checked` count already distinguishes a pass over zero from a
   pass that held some; it now counts references as well and names every reference that failed, by claim and by source.
10. **Three states, as everywhere.** A dangling in-repo path or a claim id the corpus does not hold is **FAIL**. A
    corpus or registry the build cannot read is **UNVERIFIABLE** (exit 3). An unjudged form changes no verdict and is
    named as unjudged, never folded into PASS.
11. **Still outside identity, still inside the signature, nothing derived stored.** `rationale` does not enter the
    canonical form; the digest and the Baseline format are unchanged. Existence is computed at gate time and never
    written into a claim (ADR 0022).

## Consequences

- On the next gate run, all 40 sources in this repository become judged. A claim whose justifying document was deleted
  or renamed now fails, where today it reads as justified.
- **The first such run is red, and the repair is not free.** The dangling reference above is repaired by editing
  `claims/REQ-0034.yml`, which changes its `rationale_hash`, which is in the signed manifest (`src/baseline.rs:72`) — so
  the Baseline FAILs until the owner re-signs it, and req never signs (ADR 0010). The repair, the re-signing and this
  rule land as one owner-acted sequence; landing the rule first would put CI red on `main` for a defect it correctly
  found.
- A fresh corpus created by `req init` is green under the rule, because decision 8 points its template at a file the
  same run writes.
- An adopter who cites a document by path gets the same protection for free. An adopter who writes `TBD` keeps writing
  `TBD`, and the gate says it did not judge it.
- A tracker reference remains text, and the gate now states that limit next to the reference instead of leaving the
  reader to assume otherwise.
- `docs/claim-format.md` and `req gate --help` change in the same commit that lands the check, since the judged set is
  part of the policy's contract.

## Rejected

- **The `dec:` / `adr:` / `claim:` prefix grammar (this ADR's first version).** `claim:` duplicates a form the claim
  format already documents; `adr:` covers 18 of the 40 sources where the path-shaped rule covers 40; `dec:` has **zero**
  users. A new authored convention that judges a strict subset of what a syntactic class already reaches.
- **"One token" with no shape test (this ADR's second version).** It makes `TBD` and `N/A` dangling paths and reddens
  `req init`'s own template. A token is a word as often as it is a path.
- **Judging addresses.** A full address carries a `sha256:` prefix, so it is not path-shaped; reaching it would need a
  second grammar for a form no claim uses as a source today.
- **Judging a path outside the repository.** req cannot read it, so the verdict would be about the machine, not the
  corpus.
- **Judging `decisions/` paths against the hash chain here.** Zero sources cite a decision today, and ADR 0005 already
  owns that question.
- **Fetching the URL to confirm it exists.** REQ-0012 forbids the network, and a verdict that depends on a socket is
  UNVERIFIABLE in exactly the deployment req is built for.
- **A new `origin:` object or a `req.claim/3` schema.** A schema bump, a second provenance surface and a migration, for
  judgement the required `source` slot can already carry.
- **Parsing free prose for a reference.** Deciding which words in a sentence are a reference is the ambiguity class
  claims exist to remove.

## Falsification

This decision is wrong if any of these is observed:

1. a path-shaped source that does not exist in the repository is reported as PASS;
2. a source that is a URL, a one-word note, a heading anchor, an external origin or an address changes any verdict —
   including `N/A` becoming a dangling path, or `needs.json#REQ_001` failing;
3. a claim id the corpus does not hold is reported as PASS;
4. a corpus created by `req init` FAILs `rationale_present`;
5. editing a rationale moves an address, or leaves a signed Baseline PASS;
6. an unjudged form is folded into PASS instead of being named as unjudged.
