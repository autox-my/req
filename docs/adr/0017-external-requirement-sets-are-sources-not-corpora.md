# 0017. External requirement sets are sources, not corpora

**Status: proposed** — drafted by the agent on 2026-10-21 in answer to [issue #44]. Acceptance is the
maintainer's act; nothing here is implemented. Date proposed: 2026-10-21.

## Context

Adopting req on an existing project should be able to start from the documents that project already has.
[Issue #44] asks for it, and its author is a real adopter: eight ratified capability claims and a backlog
of roughly fifteen items, all prose, with the only path into a corpus being transcription.

`req init` writes one starter claim and a README; `req --help` has no import, adopt or migrate verb. The
question is whether the missing piece is a parser or a decision.

It is a decision. A claim's identity is a content address over the canonical meaning of its **statement**
(`address = sha256("req.claim.v1\0" ‖ canonical_form_bytes)`, `docs/canonical-hashing.md` §3), and its
structured slots must be an authored parse of that statement. A document is prose: it has no claim
boundaries, no slots and no selectors. Deciding *"this bullet is one requirement, not three"*, *"this
sentence is not decidable"*, *"this applies to customer_facing only"* is a ratification act, which ADR 0004
reserves for a human. A bulk import that decided those would be the auto-merge this project exists to
forbid; one that filled `canonical.form` itself would author what must stay derived.

Two facts make a boundary possible without breaking either rule:

1. **Provenance is outside identity.** The address hashes the canonical form bytes, not the claim file, so
   attaching a citation to a claim moves no address. Without this, every source reference would re-ratify
   the claim it cites, and the whole idea would collapse.
2. **The ecosystem already has the vocabulary, and it splits the same way.** Sphinx-Needs — the closest
   neighbour, and the one used by safety-critical teams — names two modes: `needimport` *creates* local need
   objects from a `needs.json`, while `needs_external_needs` *references* external needs "without having
   their representation in your own documentation" ([configuration]). It states the identifier rule as a
   namespace: *"the file owns the ids it contains"*, and its id regex is not applied to imported or external
   needs ([needimport]). Its `needs.json` is **versioned**, keeping every version side by side with a
   `creator` and its own schema ([builders]), and the file's reading rules live in a **normative exchange
   contract** with a conformance corpus ([exchange contract]).

The automotive case is the hardest consumer of the same question: specs arrive as PDFs, but behind them are
ReqIF exports from DOORS, Polarion, Jama or Codebeamer, or a traceability matrix in Excel. ReqIF is an OMG
specification, and OMG's own page states that *"almost all RM and SysML modeling tools today support ReqIF
import and export"* ([ReqIF]).
Requirement identifiers are stable across revisions, which makes them the join key.

What Sphinx-Needs does *not* do is the reason this ADR exists rather than an import command: it re-reads
sources at build time, reports a missing source as a warning, and continues; it has no signature. Its model
optimises freshness. req's model optimises authority and reproducibility. Both are legitimate, and req can
have both — the manifest pins the revision, the check reports the drift.

## Decision

### 1. Two modes, and they are named for what they do

- **Reference.** A claim cites a source coordinate and the source's text is **never copied** into the
  corpus. `rationale.source` carries the coordinate, the revision, and the hash of the source text the claim
  was ratified against.
- **Import.** A draft: claims whose `statement` (and provenance) came from an export — `needs.json`, a ReqIF
  file, a CSV/Excel matrix, or PDF text segmented by identifier. A draft is **unsigned and unratified**; it
  becomes a corpus only through `req canon --fill`, `req canon`'s divergence report, and a human.

The distinction is the decision: **the external text is a source, the claim is the obligation.** You do not
own the customer's paragraph, so you do not store it as if you did — the same argument ADR 0011 makes about
a live sync with Jira or Notion. A copy in a corpus is a copy that drifts.

### 2. The source owns its identifiers

The source's own identifier (a requirement ID, an object ID) is the **join key**: it is recorded on the
claim, never re-issued locally, and prefixed when two sources could collide. A claim's own `REQ-NNNN` is
req's numbering and has nothing to do with the source's; the manifest keeps the mapping. A renumbered source
identifier is reported as a **withdrawal and an addition**, never inferred as a rename — the same honesty the
Sphinx-Needs exchange contract insists on when a `needs.json` key and its record's `id` disagree.

### 3. A source manifest is a committed artefact

`sources/<source-id>.yml` records, per source:

- the source's identity (document name, and the tool that produced the export);
- the **revision** (a version label, and the export's `sha256`);
- the locator → claim mapping (source identifier → claim id), and each claim's `source.text_sha`.

It is committed and, like a `needs.json` archived per documentation version, its history in git is the
record of *what the customer said at the revision we ratified*. That record is what an audit asks for and
what no warning can supply.

### 4. Drift is a tri-state verdict, never a warning

A check against a current export reports, per claim:

| verdict | meaning | what follows |
|---|---|---|
| **unchanged** | the text at the claim's locator still hashes to `text_sha` | nothing |
| **amended** | the text changed | the claim is stale; a Decision (ADR 0005) is written |
| **withdrawn** | the identifier is gone | the claim cites nothing; a Decision retires or re-points it |
| **new** | the export has an identifier no claim cites | a candidate for a draft claim |

Exit classes follow the repository's rule: `0` all unchanged, `1` an amendment or a withdrawal (a definite
finding: the corpus no longer matches the source it cites), `3` the export cannot be read (absence is not
evidence of an amendment), `2` usage. A source that cannot be read **blocks**; it is not a warning that the
build may continue past.

### 5. Mechanism and policy are separate, and the contract says so

A reader takes no project configuration and no logger; it returns **codes and paths**, and its messages are
prose for humans. This is copied deliberately from the exchange-contract discipline: *"codes and paths are
the contract; messages are not"*, *"mechanisms and no policy"*, and rules nothing enforces marked
**(not pinned)**. Any reader req ships is paired with a conformance corpus, and the contract document, the
corpus and the reader change in one commit.

### 6. The boundary is a projection in both directions

- **Inbound** — read `needs.json`, ReqIF, CSV/Excel, or PDF text segmented by identifier, into drafts.
- **Outbound** — `req export --format needs-json` emits a **projection of a signed Baseline**: derived from
  the corpus, never authored into it, never a second source of truth (the rule `render`, `trace` and `audit`
  already obey). This is the adoption play: tools in the ecosystem — and the humans who audit in them — see
  exactly the set that was ratified, with req's signature behind it.

### 7. What this decision deliberately does not do

- **No live re-read.** A corpus is a snapshot; freshness is the check's job, not the build's. What you did
  not snapshot you cannot sign.
- **No warnings as verdicts.** A missing source is UNVERIFIABLE, never a silent continuation.
- **No copied customer text as owned claims** (mode 1 is reference; mode 2 drafts, and drafts are unsigned).
- **No model and no default OCR.** A model-based extraction is not reproducible and cannot be reviewed; the
  default provider is offline by design (REQ-0012). OCR is a last resort, carries per-page provenance, and
  the drafts it produces are marked as OCR-derived.
- **No core `req adopt` verb.** The mapping is per-project — a capabilities table in one project is not a
  backlog in the next — so it lives in `tools/`, the reviewable surface, as the corpus generators already do.
- **No address from a source's identity.** Only a claim's own canonical meaning moves its address.

## Consequences

- A new artefact type (`sources/`) and at least one new `tools/` reader with a conformance corpus.
- Citations move no address (Context fact 1), so an existing corpus can gain provenance for free — including
  this repository's own claims, if a source manifest is ever wanted for them.
- Adoption becomes **measured**: *N* source identifiers → *M* claims → *K* skipped with reasons, rather than
  a feeling that transcription is long.
- Two costs are stated rather than hidden: **normalisation** (`text_sha` must ignore whitespace and layout,
  or every reformat reads as an amendment — the rule has to be written down, not tuned), and **table
  extraction** (wrapped cells and multi-page rows are why ReqIF and the Excel matrix outrank PDF text).
- Nothing here moves a claim, a Baseline or a `rules_v`; the committed Baseline stays `[VERIFIED]`.

## Falsification

The decision is wrong if any of these is observed:

1. a citation moves an address — provenance leaked into identity;
2. a formatting-only revision of a source yields **amended** — the normalisation rule is insufficient;
3. a renumbered identifier yields **amended** instead of withdrawal + addition — the join key is not the
   identifier;
4. an outbound export is edited by hand and becomes what people ratify — the projection became a corpus;
5. an adopter's real requirement set cannot be expressed in either mode — the two-mode split is wrong, not
   the adopter.

## Alternatives rejected

1. **A `req adopt` core verb.** The mapping is per-project; it would put one project's conventions in the
   CLI, and the parity invariant forbids the only version that could run unattended.
2. **Live re-read at build time** (the Sphinx-Needs model). Excellent freshness, no record of what was
   ratified, and nothing to sign.
3. **Copying the source text into claims.** The copy drifts away from the original and becomes the thing
   people read; ADR 0011's argument against a bulk sync applies unchanged.
4. **Warnings plus `-W` as the failure mode.** It fails open: a build can succeed without the source, and
   the tri-state exists precisely to refuse that.
5. **PDF-first extraction.** The fidelity ladder is ReqIF (or the exporter) → Excel/CSV traceability matrix
   → PDF text segmented by identifier → OCR. Starting at the bottom produces drafts that cannot be audited.

## References

- [Issue #44] — the adopter's request, and the comment that settles its scope.
- Sphinx-Needs: [`needimport`][needimport], [`needs_external_needs`][configuration], the [`needs` builder][builders],
  [services][services], and the [`needs.json` exchange contract][exchange contract].
- [ReqIF] — the OMG Requirements Interchange Format, v1.2.
- ADRs 0004 (model proposes, human disposes), 0005 (authority is a signed Decision), 0007 (collapse tiers),
  0011 (external sources are not copied), 0016 (a signed Baseline attests its authority and time).
- `docs/canonical-hashing.md` §3 — the address definition the whole decision rests on.

[Issue #44]: https://git.xantu-barb.ts.net/fdx/req/issues/44
[needimport]: https://sphinx-needs.readthedocs.io/en/latest/directives/needimport.html
[configuration]: https://sphinx-needs.readthedocs.io/en/latest/configuration.html#needs-external-needs
[builders]: https://sphinx-needs.readthedocs.io/en/latest/builders.html
[services]: https://sphinx-needs.readthedocs.io/en/latest/services/index.html
[exchange contract]: https://github.com/useblocks/sphinx-needs/blob/master/packages/ub-needs-exchange/design/exchange-contract.md
[ReqIF]: https://www.omg.org/spec/ReqIF/
