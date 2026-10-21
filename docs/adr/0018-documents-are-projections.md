# 0018. Documents are projections, and a committed projection is checked for staleness

**Status: proposed** — drafted by the agent on 2026-10-21. Acceptance is the maintainer's act. Date proposed:
2026-10-21.

## Context

ADR 0001 made the **claim** the unit of work, not the document. The views followed: `render` (the spec view),
`trace` (the traceability matrix) and `audit` (the packet) are **projections** — they read claims and never
write one (design step 9). That is the right model, and it has left one question unanswered since the
beginning: **what happens when a project wants a document.**

Two answers exist and both are wrong:

- **Write the document.** Then it is a second source of truth, it drifts, and `req render` is decoration.
  This is the failure an adoption review found in another project ([issue #44]'s author: a prose
  capabilities table that no tool could check and that quietly stopped matching the product), and it is the
  failure agentic projects walk into first: the agent writes `spec.md`, the corpus is initialized *after*
  it, and from then on there are two specs.
- **Say "generated, do not edit".** A declaration with nothing behind it. The review-gate work already
  taught this repository that lesson twice: a marker that can vanish silently is worth nothing, and a
  check that cannot go red for the right reason is worse than no check (the marker convention, then the
  spelling-not-effect pins).

A third answer is available, and one measurement makes it cheap: **`req render` is byte-identical across
runs** (measured on `8e0f636`…`ba739c1`: two invocations, `diff -q`, empty). A document that is a pure
function of the corpus can therefore be **committed as an artefact and proven current by regeneration** —
not by a comment, not by a manifest of file hashes, but by asking the corpus again and comparing.

## Decision

### 1. A projection is derived; the corpus is the only source

A **projection** is any output that is a function of the corpus and writes nothing: `render`, `trace`,
`audit`, and anything later added in that shape. Its content is determined by:

- the rules version (`RULES_V`) — because addresses are derived from it;
- the **selection** (`key=value` context, ADR 0003) — because the applying set is a projection choice;
- and the corpus itself: the addresses, statements, relations and coverage of the claims that apply.

A projection may be **committed** to the repository as a deliverable (a `spec.md` a human reads, a
matrix an auditor reads). It is then a **checked-in build artefact**, and it must be provably current.

### 2. A committed projection carries a header naming what it was built from

The first lines of any committed projection state, in a machine-readable form:

- the command that produced it (`req render`, `req trace`, …);
- `rules_v`;
- the selection (verbatim, or the explicit "no context" when nothing was decided);
- the number of applying claims;
- an **identity digest**: `sha256` over the sorted addresses of the applying claims.

The header is not the check — it is what makes a stale file *self-evidently* stale to a reader and what
lets the check refuse to compare two different things (§3, `3` below). An artefact without a header is not
a projection this decision covers; it is an authored document, and it is reviewed as one.

### 3. Staleness is decided by regeneration, not by the header

A committed projection is verified by re-deriving it and comparing bytes:

```
req render --check docs/spec.md      # 0 current, 1 stale or edited, 3 cannot judge, 2 usage
```

Equivalently, where a `--check` mode does not exist: regenerate over the file and let git report the
difference (`req render > docs/spec.md && git diff --exit-code docs/spec.md`). Either way the **corpus is
the oracle**; the file is never trusted about itself.

The exit classes follow the repository's rule, and the third one matters:

| exit | meaning |
|---|---|
| **0** | the file is byte-identical to a fresh projection of the corpus |
| **1** | the file differs — stale (the corpus moved) or edited (the file moved); both are defects, and the remedy is identical: change the corpus or regenerate, never hand-edit |
| **3** | the check cannot judge: the file's `rules_v` is not this build's (its addresses mean something else — the same class as a Baseline's `rules_v` mismatch, ADR 0016), its selection cannot be reproduced, or the corpus cannot be read |
| **2** | usage |

A projection whose `rules_v` differs is **UNVERIFIABLE**, never FAIL: a projection is not wrong because
the reader is a different build. It is re-generated, exactly as a v2 manifest is re-issued.

### 4. The checked set is a pinned list, and it is small

Only paths whose content is a projection are checked. The list lives with the check and is pinned the way
`tools/review/check_rules.py` pins its allow-list: **adding a path is a decision, not an edit.** Authored
documents — `docs/design.md`, the ADRs, `AGENTS.md`, `CONTEXT.md`, `docs/adopting.md`, `claims/README.md`
— are sources of the project and are never in the list. A projection inside `docs/` must say which command
produced it, so a reader (and an agent) can tell a source from an artefact at a glance.

### 5. Never edited, always regenerated

Editing a projection is a defect of the same family as a hand-written `address`: it makes the artefact
disagree with the corpus while looking authoritative. The check reports it as a difference (§3, `1`); the
remedy is to change the claim and regenerate. This holds for agents and humans alike — an agent that
edits `spec.md` has changed a document, not a requirement.

## Consequences

- A CI step per committed projection; the repository's own gate gains it when this repository adopts one.
  The natural first instance is this repository's own corpus (`req render` over 21 claims), which gives
  the check a live user rather than a fixture.
- `spec.md` cannot go stale **by construction**: the only way to change it is to change the corpus, and the
  corpus has its own gates (`canon --verify`, `baseline --verify`).
- A committed projection makes the audit trail visible: the packet an auditor reads is in the tree and
  provably current, not a screenshot in a ticket.
- The header digest lets a reader or an agent detect staleness *without* running anything — a cheap signal
  in front of the expensive answer.
- Cost: a regeneration on every claim change means diff noise in the committed file. That is why §4 says
  the list is small and pinned: a projection is committed where it is a **deliverable**, not as a cache of
  every view.
- No address, claim, Baseline or `rules_v` change. Nothing here moves a verdict.
- Not implemented by this decision: `--check`, `--format markdown`, `--out`. Today `render`'s stdout is
  already markdown-shaped and `>` captures it; the decision names the contract, the implementation follows.

## Implementation note (2026-10-21, pack A)

The bullet above is what the decision did *not* build; this note records what was built on
`feature/projection-check`, for `render` and `trace` (both, as §1 requires). `audit` is the third
projection and gains no flag here: the pack's scope is the two views, and the audit packet's
selection comes from a Baseline, which is a different question.

**CLI.** `req render|trace [--out <path>] [--check <path>] [key=value ...]`. `--out` and `--check`
are exclusive (2), as are a flag without a path and a path that is option-shaped. `--out` writes the
header plus the body and exits **0** written / **3** cannot write — including when the plain
command's verdict would be 3, because the artefact is complete (an undecided claim keeps its own
section) and the exit class belongs to the verdict. `--check` writes nothing: **0** byte-identical,
**1** stale or hand-edited (the first differing line is named, with the `--out` remedy), **3**
cannot judge. Plain `render`/`trace` are unchanged, byte for byte and exit class for exit class.

**Header** (one line, the file's first line, invisible when rendered):

```
<!-- req projection: generator=req render | rules_v=5 | selection=field=value runs=25 | claims=15 | identity=sha256:<64 lowercase hex> -->
```

`selection` is the canonical context — keys sorted, `k=v` pairs joined by single spaces, the literal
`none` when the caller decided nothing — and the check reproduces the context from it. A context
whose canonical spelling would not parse back (a key or value carrying whitespace) is refused by
`--out` (**3**) rather than written into an artefact that cannot be checked.

**Identity** is `sha256` over the sorted addresses of the **applying** claims, each followed by
`\n`, lowercase hex: `sha256(concat(sorted(addresses).map(|a| a + "\n")))`. An address is the bare
64-hex string `Row.address` carries (the digest of `canonical.form`), not the header's
`sha256:`-prefixed spelling. The fixture
`sha256("a"*64 + "\n" + "b"*64 + "\n") = 913f9338fb6c17253f3a14816fc08d52522454bfc19a8c99538a197cfb23fb41`
was computed independently of the implementation, and a test pins the sort and the separator through
it. The digest is an artefact of the projection, never a claim address: no identity and no `RULES_V`
moves.

**Check order** (an earlier failure outranks a later one): header grammar (**3**); `rules_v` not this
build's (**3**, the ADR 0016 precedent — it is regenerated, never FAIL); another generator's
projection (**3** — two commands are two artefacts, so `render --check` on a `trace` file names
`trace --check` instead of calling it stale); a selection that does not reproduce (**3**); then
regeneration and a **byte** comparison (**1** on any difference). The header's identity digest is
never compared alone. An explicit context beside `--check` must name the same canonical selection as
the header, or it is **3**: the artefact records what it was built from, and a second selection is a
second question.

**Pinned list.** `.req/projections` (one repo-relative path per line, `#` comments, blank lines
skipped — the `.req/variants` grammar) is read by `scripts/check.sh`, which runs
`req render --check` per path, fails on a listed path that is missing and on a list that checked
nothing, and prints `projections checked: N`. This repository's first instance is `docs/spec.md`,
generated with no context (15 applying, 6 undecided). req itself never reads `.req/projections`: the
list and its reader belong to the repository, like `.req/variants`.

**Deliberately not here.** `--format json` waits for workstream C — markdown is the only format, and
the header names the generator so a JSON artefact can arrive without re-reading this decision.
`audit` gains no `--out` (above). And `--check` never repairs, rewrites or normalizes the file: it
answers a question.

## Falsification

The decision is wrong if any of these is observed:

1. a projection passes the check while the corpus changed — the check compared the header, not the bytes;
2. two runs of the same projection differ — it was never a function, and a committed one cannot be checked;
3. a hand-edited projection passes the check;
4. a projection built under another `rules_v` is reported **FAIL** instead of **UNVERIFIABLE**;
5. a projection generated under a different selection passes — the header did not name the selection;
6. the checked list grows to include an authored document — a source became an artefact, or vice versa;
7. an agent's `spec.md` is edited by hand and the gate stays green.

## Alternatives rejected

1. **A "generated, do not edit" comment with no check.** A declaration that can vanish silently — the
   failure this repository has now caught three times (the sedia marker convention, the review-gate
   exclusion, the spelling-only pins).
2. **Never commit projections.** Then the artefact an auditor or an agent reads is not in the tree, a
   release-time regeneration leaves no evidence trail, and "what the spec said last month" is answerable
   only by re-running against a past commit.
3. **Make the document the source** (the docs-as-code model). Identity would be prose, addresses would be
   file hashes, and a restatement would be a new requirement — which is the model ADR 0001 rejected.
4. **Check by recording file hashes in a manifest.** That is the same check with the oracle removed: the
   manifest would attest what the file *was*, never that it is still what the corpus *says*.

## References

- ADR 0001 (the unit is a claim), 0003 (variants are selectors — the selection is part of the header),
  0007 (collapse tiers — why `rules_v` is in the header), 0012 (runtime corpus root), 0016 (a `rules_v`
  mismatch is UNVERIFIABLE, the precedent §3 follows), 0017 (external sources are cited, never copied —
  the same "never own a copy" rule applied to a projection).
- Design step 9 / REQ-0020 — the views are projections.
- [Issue #44] — the adopter whose prose table no tool could check, and the reason a document needs an
  identity underneath it.

[Issue #44]: https://git.xantu-barb.ts.net/fdx/req/issues/44
