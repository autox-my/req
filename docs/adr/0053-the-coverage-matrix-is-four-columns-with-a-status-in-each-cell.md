# 0053. The coverage matrix is four columns, with a status in each cell

Amends ADR 0052 decision 3 (the claims × components matrix). Extends ADR 0046 decision 2 (`req.trace/1` carries its
nodes) additively. Nothing else is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: proposed.** The owner ratifies it by merging. Asked for by the owner on 2026-11-14, after evaluating a
"hybrid view" concept (a matrix for overview, a graph for navigation, over one typed graph) and agreeing to adopt its
matrix half. Date proposed: 2026-11-14.

## Context

**The matrix ADR 0052 decided does not read.** Decision 3 fixed it as one row per claim and **one column per
component**. On req's own architecture that is 40 rows by 27 columns (the detailed view) or 12 (the overview), almost
every cell empty (`docs/architecture/matrix.md`, `overview-matrix.md`). A reader cannot see which claims are covered,
by what, or what is missing.

**The concept the owner evaluated.** A local research note and demo (not committed; generic, no customer material)
describe the matrix most requirements tools converge on: rows are requirements, and a few **fixed columns** name the
relation in the heading: *Source*, *Requirement*, *Satisfied by*, *Verified by*. A missing link is printed as a gap
("no design element"), never a blank cell. The note's claims about DOORS link types, Sphinx-Needs `needflow` and Jama's
Trace View are uncited, and its "study with 24 participants" names no paper: **leads, not findings**. The shape stands
on its own merits, not on that evidence.

**req already holds all four layers**, counted on its own corpus at `049f4ee`:

| column | where req keeps it | req's own 40 claims |
| --- | --- | --- |
| Source | `rationale.source` (an importer keeps the source tool's identifier there too, ADR 0027/0028) | 40 |
| Requirement | the claim | 40 |
| Satisfied by | `req-satisfies:` links, judged by `req satisfy` (ADR 0040) | 28 |
| Verified by | the claim's `tests:` (ADR 0019), judged against a JUnit report | 38 |

**What the concept lacks and req has.** The concept checks that a link *exists*, as the tools it cites do. req can say
whether it is still *true*: a design link HOLDS, or is SUSPECT because the claim tightened since a human recorded it
(ADR 0040); a linked test passed, or was skipped, which is not a pass (REQ-0026). A cell that says *linked, but suspect
since the claim tightened* is what a presence check cannot show.

## Decision

1. **The coverage matrix has four fixed columns.** It is a view (`CONTEXT.md`): coverage of each claim by source, design and
   tests, compiled from the corpus, never maintained by hand. One row per claim of the selection, ordered by claim id:
   - **Source**: the claim's `rationale.source`, verbatim;
   - **Claim**: its id and statement;
   - **Satisfied by**: each design element (ADR 0040) or, on an architecture overlay, each component (ADR 0052) that
     satisfies it, **with the link's outcome** (`holds`, `holds (weakened)`, `suspect`, `unratified`, …);
   - **Verified by**: the tests it links (ADR 0019): their count and ids, and **their outcome when a test report is
     given** (passed, failed, skipped, not found).
2. **A gap is printed, never blank.** An empty column says why in words: `no source`, `no design element`, `no test`.
   A claim whose evidence is not machine evidence (`evidence:` other than `machine`) says so in *Verified by* instead
   of `no test`, because the gate does not ask it for one (ADR 0019).
3. **This replaces ADR 0052 decision 3's matrix.** The architecture overlay's `matrix.md` takes this shape, with
   *Satisfied by* naming components. It is the only change to ADR 0052; its other decisions stand.
4. **The data comes from req, the layout from the converter.** `req.trace/1`'s nodes gain `source` and `tests` (and
   `evidence`), additively under decision 7 of ADR 0046. The converter (`tools/graph/arch_overlay.py`) already reads
   `req.trace/1` and `req.satisfy/1`; it writes the matrix from them.
5. **The committed matrix is a function of the repository, so test outcomes stay out of it.** A test outcome depends on
   a run, not on the corpus. A committed matrix (`docs/architecture/*matrix.md`, checked by
   `tools/graph/build_architecture.py --check`) shows the linked tests and no outcome; a run given a JUnit report
   (the converter's optional `--test-report`) shows the outcomes and is not committed. Design outcomes are a function
   of the repository (the claims, `.req/satisfied` and the design files), so they are committed.
6. **Cells link to what they name**, where the output format allows: a claim to its file, a design element to its
   lines on GitHub at the pinned revision, a component to the architecture page. The matrix is Markdown, read on
   GitHub (the Pages site serves `docs/` without rendering Markdown).

## Not decided here

- **An interactive explorer** (focus on a node, filter by type, "gaps only"), the concept's graph half. It would be a
  second renderer beside archify, against ADR 0046's choice, and it must stay readable at the scale of a real
  specification (thousands of claims, tests grouped per claim), which the concept's demo (22 nodes) does not show.
  Revisit after this matrix ships.
- **A source as its own node**, so that two claims from one section share a source. Today a source is free text.
- **A coverage section in `req trace`'s text projection** (`docs/trace.md`). `req trace` stays the claim-to-claim matrix;
  a coverage section there would move committed projection bytes and is a decision of its own.

## Consequences

- The architecture page's matrix becomes four columns wide whatever the number of components: readable at 40 claims
  and at 4000.
- A tightened claim shows in one row: *Satisfied by* turns `suspect`, with the element named. The drift demonstration
  (ADR 0052 decision 7 step 4) shows it in the matrix and on the diagram at once.
- `req.trace/1` grows three node fields. A renderer that ignores unknown fields is unaffected.

## Falsification

This decision is wrong if any of these is observed:

1. a claim with no design link, no test or no source shows a blank cell instead of a named gap;
2. a design link whose outcome is not `holds` or `holds (weakened)` is shown as covered without its outcome;
3. a committed matrix changes when only a test report changes (it must be a function of the repository);
4. two runs over the same inputs give different matrix bytes;
5. the matrix grows a column when a component is added.
