# 0052. An architecture diagram shows the claims its components satisfy, through the files they cite

Extends ADR 0040 (a design satisfies a claim at a meaning) and ADR 0046 (req writes archify's input; archify only renders
it). Nothing is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: proposed.** The owner ratifies it by merging. Asked for by the owner on 2026-11-14 ("we should also use archify to
generate architecture diagram and most importantly to display the relation to the requirement"); the route in decision 1
was chosen by the owner on 2026-11-14 over a requirement field in an archify fork. Date proposed: 2026-11-14.

## Context

**req can judge a design link, but only for a whole file, and it does not do so for itself.** ADR 0040 lets any file under
a `.req/design` root declare `req-satisfies: <claim-id>`. A human records the link, and `req satisfy` judges it by the
direction of the claim's change since then. This repository lists no `.req/design`, so its own 40 claims have no design
link. Process standards ask for traceability between requirements and architecture elements (ADR 0040's *Context* names
DO-178C and ASPICE SWE.2 as leads, from memory).

**An architecture diagram has no place for a requirement.** archify (`tt-a1i/archify`, pinned at `7e45f36` by ADR 0046)
draws architecture diagrams from a JSON file. Read at that commit:
- its schema is closed: a component holds `id`, `type`, `label`, `sublabel`, `tag`, `icon`, `brand`, `sources`, `row`,
  `col`, `pos` and `size`, and any other key is refused;
- in its repository mode (`references/repository-authoring.md`), every component cites the code it was drawn from in
  `sources`: one to three repository-relative paths with line ranges, verified against a forty-character revision pinned
  in `meta.repository`.

**A diagram is redrawn.** An agent draws an archify diagram, and a second drawing can name the same component differently.
A link keyed on a component id would then break on every redraw.

**A probe, 2026-11-14.** With `.req/design` listing `src` on a copy of this repository, `req satisfy` reports four links to
the claim `""` (UNKNOWN CLAIM, UNVERIFIABLE): `src/audit.rs`, `src/completeness.rs`, `src/main.rs` and `src/satisfy.rs`
mention the marker in help text and comments with no claim id after it. A tool that documents its own marker cannot list
its own source as a design root today.

## Decision

1. **A component inherits the links of the files it cites.** The link stays where ADR 0040 puts it: in the file, as a
   `req-satisfies:` line, recorded by a human. A component of an archify architecture diagram satisfies the union of the
   claims declared by the files in its `sources`.
   - The element is still a file. A line range in `sources` locates evidence for the diagram; it does not narrow a link.
   - A cited file outside every `.req/design` root contributes no link, and the overlay says so for that component.
   - The diagram stores no link. Nothing in archify's schema changes, and no fork is needed.
2. **A component's status is the worst of its links' outcomes.** The order, worst first: UNKNOWN CLAIM, ORPHANED, SUSPECT,
   UNRATIFIED, HOLDS (weakened), HOLDS. A component whose files declare no link is drawn as *no claim*; that is not a
   finding here, because whether every claim needs a design is ADR 0040's `need design`, not the diagram's.
3. **req writes the overlay; the user's diagram is never edited.** A converter beside `tools/graph/trace_to_archify.py`
   reads the user's archify architecture file and the output of `req satisfy --format json` (`req.satisfy/1`). It writes
   **a new** archify file.
   - **Deterministic**, as ADR 0046 decision 5: two runs over the same inputs give identical bytes. Python standard
     library only, offline. The core never runs it, and `scripts/check.sh` never needs archify or a browser.
   - It changes **presentation only**: each component shows its status and claim ids in archify's existing fields,
     keeping the author's label. It never adds, removes or moves a component or a connection.
   - It also writes the **claims × components matrix**: one row per claim, one column per component, each cell the
     link's outcome. A claim no component reaches is listed, so the page shows the gap as well as the coverage.
   - Which archify fields carry the status, and the matrix's file format, are left to the implementation spec.
4. **A stale diagram is shown as stale.** A cited path that does not exist in the working tree is drawn distinctly and
   reported, and the converter's exit status says so; the overlay never silently drops it. The overlay page carries the
   req commit and the diagram's pinned revision, so it never claims to be current (ADR 0046, *Consequences*).
5. **Who authors what.**
   - The **diagram** is an architect's or an agent's reading of the code, reviewed by a human like any document. This is
     unlike ADR 0046's trace graph, where the agent may not author anything: an architecture is a judgement about the
     code, not a projection of the corpus.
   - The **links** are never authored in the diagram. An agent may propose a `req-satisfies:` line in a pull request;
     only a human records it (`req satisfy --record`, ADR 0004).
   - The **status** comes from `req satisfy` alone. An agent may not edit the overlay's output.
6. **A marker with no claim id is a mention, not a link.** `req-satisfies:` followed by no token is not a declaration.
   `req satisfy` counts mentions in its report (with the file and line), so they are visible but never a link and never
   UNVERIFIABLE. A token that is present but malformed is still named, as today.
7. **req uses it on itself.** In this order, each a separate pull request:
   1. decision 6 and the converter, test first;
   2. `.req/design` lists req's source roots; modules declare the claims they satisfy; the owner records the links;
   3. a committed archify architecture diagram of req in repository mode, and its overlay published on the project's
      Pages site and in the README, with the req commit it was made from;
   4. a before-and-after demonstration of drift: a claim's meaning changes on a **copy** of a corpus, and the components
      that satisfy it turn SUSPECT. Never on req's own claims.

   `need design` stays off for req until step 2 has covered every claim; turning it on is a later owner decision.
8. **archify stays pinned and offline**, as ADR 0046 decision 7: a named commit, and `ARCHIFY_UPDATE_CHECK_DISABLED=1` on
   every invocation (REQ-0012).

## Alternatives rejected

- **A `requirements` field on a component, in an archify fork or upstream.** It would store every link twice, once in
  the diagram and once in the code, and the two would drift on every redraw. A fork is also a dependency req would have
  to maintain.
- **The marker in a component's visible text** (`sublabel`, `tag`). It works today (probed 2026-11-14: the whole diagram
  file holds, after `--record`), but it links the whole file, not a component, and puts bookkeeping on the drawing.
- **A component-level element id, `<file>#<component-id>`.** It keys the link on the one thing a redraw changes.
- **A req-owned mapping file, component to claims.** It contradicts ADR 0040 decision 1 (the element declares its own
  links) and drifts like the field.
- **Generating the diagram from the code** (an import graph, or graphify's code extraction, whose node ids are file path
  plus symbol). It is deterministic but draws modules, not an architecture. It may later suggest connections; it is not
  needed for the link, because the file path is already the stable key.
- **Keeping a bare marker an error** (decision 6), and listing design roots file by file to avoid the four mentions. It
  leaves every tool that documents the marker unable to name its own source as a root.

## Not decided here

- Diagrams with no `sources` (sketched by hand, not drawn from a repository): they get no overlay.
- Other archify diagram types (dataflow, sequence) whose elements also carry `sources`.
- A requirement field upstream in archify, if diagrams without `sources` turn out to matter.
- `need design` for this repository (decision 7).

## Consequences

- Anyone already drawing archify diagrams from a repository gets the overlay without changing a diagram: add the
  `req-satisfies:` lines, record them, run the converter.
- A redrawn diagram keeps its links when it cites the same files, whatever its component ids.
- A broad file cited by many components (an entry point such as `src/main.rs`) lends its claims to each of them. The
  overlay shows that as it is; it is a reason to put a link in the narrowest file that satisfies the claim.
- archify allows three `sources` per component, so a component is linked through at most three files.

## Falsification

This decision is wrong if any of these is observed:

1. two runs of the converter over the same inputs give different bytes;
2. the converter, or any invocation it documents, writes the user's diagram or the corpus, or needs the network;
3. a component is drawn HOLDS while a link from one of its cited files has any other outcome than HOLDS or HOLDS
   (weakened);
4. renaming a component id, with its `sources` unchanged, changes the claims the component shows;
5. a cited file that no longer exists is drawn without being flagged;
6. a bare marker is reported as a link, or a malformed claim id is reported as a mention.
