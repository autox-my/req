# 0052. An architecture diagram shows the claims its components satisfy, through the files they cite

Extends ADR 0040 (a design satisfies a claim at a meaning) and ADR 0046 (req writes archify's input; archify only renders
it). Nothing is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: accepted** (ratified by the owner's merge of PR #76, 2026-11-14). Asked for by the owner on 2026-11-14 ("we should
also use archify to generate architecture diagram and most importantly to display the relation to the requirement"); the
route in decision 1 was chosen by the owner on 2026-11-14 over a requirement field in an archify fork. Date proposed:
2026-11-14.

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

## Implementation note — 2026-11-14 (O-1: the mention rule and the converter)

Not an edit of the decisions above; where decisions 1–6 now live (decision 7 step 1).

- **Decision 6 is `src/satisfy.rs`.** `scan` returns `mentions` beside `declarations`: a `req-satisfies:` whose token (the
  existing run of ASCII alphanumerics, `-` and `_`) is empty, as `(element, line)`, sorted and de-duplicated. `req satisfy`
  lists each as `[MENTION] <element>:<line>: req-satisfies: with no claim id — not a link`, its header counts them, and
  `req.satisfy/1` gains a top-level `mentions` between `links` and `unreadable` (additive under `/1`). A mention changes no
  verdict and no exit code, and `gate`, `completeness` and `audit` read declarations only. A non-empty token that is no claim
  stays UNKNOWN CLAIM.
- **The converter is `tools/graph/arch_overlay.py`** (standard library, offline), run as `python3 -I
  tools/graph/arch_overlay.py [--root <dir>] <diagram.json> <satisfy.json> <trace.json> <out-dir>`; it writes
  `overlay.json` and `matrix.md`. A component's links are the `req.satisfy/1` links whose `element` equals one of its
  `sources[].path` exactly (line ranges ignored; the id is never read). Exit `0` written; `3` written, with a component
  `STALE`, a link `unknown_claim` or an author's tag the status cannot fit beside (reasons on stderr); `2` refused (a wrong
  schema or diagram type, malformed JSON, an unreadable input, an existing non-empty out-dir), nothing written.
- **The archify field is `tag`.** Read at `7e45f36`: a component's `tag` is one unwrapped line at the foot of its box, in the
  accent colour, shrunk to a 6 px floor inside `(width - 8) / 3.6` columns, and rejected (`component-tag-overflow`) only past
  that. The width is the component's `size[0]`, else archify's `layout.defaultW`, 120 px (31 columns); the grid's `cellW` is
  the step between boxes and never a box's width. An author's tag is kept even when the status cannot fit beside it: the
  overlay is still written, the component and the reason go to stderr, and the exit is 3 (archify `validate` would refuse
  it); the `label` is the author's name, and the `sublabel` is the author's description of the component, so both stay
  untouched. The overlay's `tag` keeps any text the author wrote and adds ` | <STATUS> · <claim ids>`; when the ids do not
  fit the box they are cut to the first few and `+N` (then `N claims`), and the **status card lists every id**. `cards` carry
  the rest: components by status (worst first), claims no component reaches, and cited paths outside every `.req/design`
  root. `meta.subtitle` (kept, and added to) names the diagram's pinned `meta.repository.revision` (or says there is none),
  the satisfy document's `rules_v` and archify `7e45f36`. archify's file keys are `schema_version` and `diagram_type`; the
  converter refuses anything but `architecture` at `schema_version` 1.
- **A root is normalised by segments**: empty and `.` segments are dropped, so `src`, `src/.`, `./src/./` and `src//` are one
  root, and `.` or `./` (nothing left) is the repository itself, held as the empty root. An element id is therefore `src/x.rs`
  and never carries a dot segment (ADR 0040), and the converter treats the empty root as covering every cited path. A leading
  `./` is taken off before the absolute-path check, so `.//tmp/x` is refused like `/tmp/x` (exit 2 naming the line); the
  converter, which does not validate, lets such a line cover nothing.
- **Tag columns** follow archify's `textUnits` rule, not the Unicode East Asian Width table: a variation selector adds none, a
  character followed by U+FE0F takes two, a character in archify's list of wide ranges takes two, any other takes one. The
  converter was checked against archify over every code point, alone and with U+FE0E and U+FE0F.
- **Status** is the worst outcome, in decision 2's order, with two states that are not outcomes: `STALE` (a cited path is
  absent under `--root`, or is not a repository-relative path; ranks worst of all, decision 4) and `NO CLAIM` (no link: a
  component without `sources`, or whose files declare nothing). The matrix's rows are the claims of `req.trace/1` whose
  `applies` is not `does_not_apply`; a cell is the worst outcome of that component's links to that claim.
- **Falsification 1–6** are tests: `tests/graph_overlay.rs` (1–5, with the mutations that delete the worst-of rule, key a link
  on the id, skip the existence check and replace the author's tag) and `tests/satisfy.rs` (6).

## Implementation note — 2026-11-14 (O-2: req's own files declare the claims they satisfy)

Not an edit of the decisions above; decision 7 step 2, the proposal half (`docs/specs/O-2-own-design-links.md`).

- **28 of req's 40 claims are linked**, 30 `req-satisfies:` lines in the header comments of 14 files under `src/`
  (`baseline`, `bench`, `canon`, `check`, `decision`, `diff`, `gate`, `junit`, `main`, `model`, `pin`, `propose`,
  `select`, `view`). Only comment lines were added; no behaviour changed.
- **12 claims have no link.** Eleven because no file under `src/` carries the obligation out: two are processes
  (REQ-0013, REQ-0014), five are met by the absence of a capability (REQ-0005, REQ-0006, REQ-0012, REQ-0015, REQ-0021:
  nothing merges, constrains, synchronises or reaches the network), and four are a quality, a corpus shape or a property
  spread over every command (REQ-0007, REQ-0022, REQ-0023, REQ-0041).
- **REQ-0039 is carried out by `examples/install-req.sh`** (it refuses a tarball whose hash differs from the pin), and is
  still not linked: that file is copy-ready for adopters, so a `req-satisfies:` line in it would travel into their
  repositories and name a claim in their corpus, not req's.
- **REQ-0009 and REQ-0010 link the harness that measures the bound** (`src/bench.rs`); the bounds themselves (0.95, 0.30)
  are asserted in `tests/evidence.rs`, which is not a design element.
- **This repository still declares no `.req/design` and holds no `.req/satisfied`.** The owner creates the first and
  records the second after merging (ADR 0040 decision 2); until then the gate is unchanged. The commands are in the pull
  request that carries this note.


## Implementation note — 2026-11-14 (O-3: req's architecture on the project's pages)

Not an edit of the decisions above; decision 7 step 3 (`docs/specs/O-3-architecture-page.md`).

- **The files.** `docs/architecture/req.archify.json` is the diagram, drawn by an agent from the source and reviewed like a
  document (decision 5): 27 components, 50 connections that are calls and reads or writes the code shows, and four boundaries.
  24 components cite one to three files with line ranges read (41 citations; the 14 files that declare a `req-satisfies:` link
  are each cited by the component that owns them, at the range that enforces the claim: a component may cite one file twice,
  as the converter reads paths and archify allows three entries). Three components cite nothing: `git`, `minisign` and the
  provider are processes outside the repository, and a source cited for them would lend them the claims of the file that
  calls them. It pins `meta.repository` at `b0dbee4…`, the revision of PR #79, with web links into `github.com/autox-my/req`.
  `overlay.json` and `matrix.md` are what `tools/graph/arch_overlay.py` writes from it, from `req satisfy --format json` and
  from `req trace --format json`; `docs/architecture.html` is archify `7e45f36`'s rendering of `overlay.json`, committed like
  `docs/overview.html` and served by the Pages site; `docs/architecture/req.png` is its screenshot. The README and
  `docs/index.html` link the page.
- **Regenerate** with one command, then render the page again from the Node side (`examples/graph/README.md`, the
  `finalize … --quality standard` line, with `--repo-root` at a checkout whose `origin` is the diagram's URL):
  `python3 tools/graph/build_architecture.py --out docs/architecture`. `--check docs/architecture` is the same run compared:
  exit 0 current, 1 stale (the file is named) or the converter reported a stale citation, an unknown claim or an
  overflowing tag (its exit 3: a failure of the page, with its reason), 2 usage, 3 UNVERIFIABLE (`req` cannot be run, did not
  print the complete `req.satisfy/1` and `req.trace/1` documents, or the converter refused its input, which regenerating cannot
  fix). `trace` exits 3 for an undecided edge and `satisfy` exits non-zero for a suspect link, and both still print the complete
  document, so the build judges the document (its schema, and no `unreadable` entry in `satisfy`), not the exit code: a file
  `req satisfy` could not read has no links in its report, and the overlay would draw its parts as NO CLAIM.
- **What the gate checks** (`tests/architecture_page.rs`, in `scripts/check.sh` through nextest): `overlay.json` and `matrix.md`
  equal what the tool computes now, so a claim whose meaning moved (the overlay would say SUSPECT) or a cited file that was
  deleted fails the gate, and an unreadable source or another schema is UNVERIFIABLE, never current; the README quotes the
  counts the matrix shows; the page's `archify-source-evidence-data` (the repository and every node's sources, which archify
  verified against the pinned revision) equals the overlay's `meta.repository` and component `sources`; and the page's
  attributes carry every node's id, label and tag, the subtitle and every source link.
- **The named gap.** The page embeds no copy of the diagram document, only that evidence. What the gate cannot read back is
  connections, geometry, boundaries and cards: a page rendered from an older overlay that differs only there passes. Rendering
  it again after any change to `overlay.json` is the regenerating author's responsibility, and the gate cannot run archify (no
  Node, no browser in `check.sh`).
- **Not drawn, said on the page** (the "Not drawn" card). The CLI entry's dispatch to every command (`COMMANDS` in `src/main.rs`)
  has no arrows: each crossed the parts it passes and archify's `validate` refused the layout (`edge-through-node`). Four calls
  the layout refused are named: Views to Repository check (`read_list`) and to Completeness (`parse_areas`), Diff and pin to
  Baseline (`staging_dir`), Completeness to Baseline (`from_json`). Shared constants, the `--format` printer and types handed
  from one part to another are not joined. 12 of the 40 claims have no part (the list is in the matrix), as the O-2 note says.
- **The image** `docs/architecture/req.png` is not archify's export (that is a button in the page's viewer; `7e45f36` has no
  command that exports without a browser session). It is the light-theme 2048×1320 screenshot `archify visual-check` writes
  (`visual-check <page> --summary --out-dir <dir>`, real Chrome), cropped to the diagram with `sips -c 1070 1680 --cropOffset
  132 182`. It is a picture of the page at that revision and is not checked by the gate; the README links the page, where the
  labels are readable.
- **Run on this repository, 2026-11-14:** archify `validate` and `finalize --quality standard --json` on `overlay.json`: all four
  gates (validate, deliver, check, browser-check) pass with no diagnostics; `visual-check` passes containment, readability,
  viewer chrome, themes and captures at 1440×900 and 2048×1320. archify's route-readability signal reports 28 crossings, 16
  routes over the suggested bends: the picture is dense, and the page's Route and Lens controls are how to read one part.

## Implementation note — 2026-11-14 (O-4: the architecture page shows the requirements at first sight)

Not an edit of the decisions above; decision 3 leaves which archify fields carry the status to the implementation, and this
unit changes that one choice (`docs/specs/O-4-architecture-readable.md`). The owner merged O-3 as it was and asked for two
improvements.

- **The field.** The status moves from a component's `tag` to its `sublabel`, in the form `HOLDS · 6 claims`,
  `SUSPECT · 1 of 3 claims`, `STALE`, `no claim`. Why: archify `7e45f36` draws a `tag` only at its finest zoom
  (`data-detail="fine"`) and a `sublabel` at the overview zoom (`data-detail="context"`), so the status was invisible on the
  published page and in the README image. The author's own `sublabel` is not lost: it moves to the `tag`, after the author's tag,
  where it shows at the fine zoom and in the passport panel. The claim ids are on the status card, no longer in the box. Both
  fields share archify's 6 px floor, so the box holds `(width - 8) / 3.6` columns of either; a status line or a moved text
  that cannot fit is still written whole, named on stderr, and the converter exits 3, as for the tag in O-1.
- **Two views.** `docs/architecture/req-overview.archify.json` is a second diagram of the same revision (`b0dbee4…`): 12
  components (one per area of `.req/areas` with code, merged where the code is one thing, plus three stores, `minisign` and
  the CLI entry) and 13 connections that are calls and reads or writes, only those that carry a claim from authoring to a
  signed Baseline. Every file that declares a `req-satisfies:` link is cited by one of its boxes, so it leaves no linked file
  to the detail; what it leaves out (Direction, Contradictions, Design links, Completeness, the Audit packet, self-test,
  the provider and git) holds no link. `docs/architecture.html` is now the overview; the detailed diagram, unchanged but for
  a card that names the overview and its `meta.output`, is `docs/architecture-detail.html`. A card is plain text in archify
  `7e45f36`, so each page names the other by file name. `docs/architecture/req.png` (the detail) is replaced by
  `req-overview.png`, the README image (light theme, cropped, 1400×1060, 136 KB).
- **Regenerate** both: `python3 tools/graph/build_architecture.py --out docs/architecture` writes `overview-overlay.json`,
  `overview-matrix.md`, `overlay.json` and `matrix.md`; `--check docs/architecture` compares all four with the O-3 exit
  contract; then render each overlay again with `finalize … --quality standard` (`examples/graph/README.md`) to
  `docs/architecture.html` and `docs/architecture-detail.html`.
- **Run on this repository, 2026-11-14:** archify `validate` and `finalize --quality standard --json` on both overlays: all four
  gates pass with no diagnostics; `visual-check` passes at 1440×900 and 2048×1320. The overview reports 0 proper crossings and
  0 resolved crossovers; the detail 3 and 28, as before. The gate's named gap is unchanged: it reads back what a page carries
  as attributes and as embedded source evidence (now for both pages, including each status line), not the picture.
- **The matrix (ADR 0053, O-5).** Decision 3's claims × components matrix is replaced by the four-column coverage matrix of ADR
  0053; the overlay JSON, the status line and the cards of this note are unchanged.

## Implementation note — 2026-11-14 (O-6: what drift looks like on the architecture)

Not an edit of the decisions above; decision 7 step 4 (`docs/specs/O-6-drift-demo.md`). **With it, decision 7 is complete**
(steps 1 to 4); `need design` for this repository stays the owner's later decision.

- **Three edits, one claim, always on a copy.** `docs/architecture/drift/edits.json` (`req.drift-edits/1`) names, for REQ-0010
  (the false-split bound, linked from `src/bench.rs`, drawn in the Evidence box of the overview), the statement before and
  after and the outcome `req satisfy` is expected to give: a **cosmetic** re-word (`at most 0.30` written `at most 0.3`:
  HOLDS, the same meaning fingerprint), a **loosening** (`at most 0.40`: HOLDS (weakened), `numeric_loosening`, an admitted
  carry) and a **tightening** (`at most 0.20`: SUSPECT, `numeric_tightening`). They were probed with `req satisfy` first; a
  re-wording that adds words (`for the parity corpus`, a comma, `no greater than`) is not cosmetic to req: it classified
  `scope_divergence` or `unrelated`, SUSPECT, which is the asymmetric cost of ADR 0007 and not a defect of the demonstration.
  The pin (`pins: value 0.3`) is left as the claim has it: a statement edited on a copy is the copy's own, and `canon` would
  flag the pin there (the statement is the source of truth, never the pin).
- **The build** is `python3 tools/graph/build_architecture.py --drift docs/architecture/drift` (`--check-drift` is the same
  run compared: exit 0 current, 1 stale with the file named or an edit whose outcome is not the one it expects, 2 usage, 3
  UNVERIFIABLE: `req` cannot be run, did not print the complete documents, or the converter refused its input). For each edit
  it makes a **fresh temporary copy** (`claims`, `aliases`, `.req`, `src`, `decisions`, `baselines`), changes the claim's
  folded statement there (the build refuses an edit whose `before` is not the statement as committed), runs `req satisfy`
  and `req trace --format json` with `--root .` in the copy (no machine path reaches an output) and runs
  `tools/graph/arch_overlay.py` on the overview diagram, which is read, never written. The diagram handed to the converter
  is the architect's with a title, a subtitle and one card added (a demonstration, on a copy, the statement before and
  after, req's own line for the edited link); components, connections and geometry are untouched. It writes
  `<edit-id>/overlay.json`, `matrix.md` and `satisfy.txt` (the exact `req satisfy` text). The copies are deleted. This checkout's
  corpus is never opened for writing.
- **The page** `docs/architecture-drift.html` is archify `7e45f36`'s rendering of `tightening/overlay.json` (the Evidence box
  reads `SUSPECT · 1 of 4 claims`, every other box holds), made as the other pages are (`finalize … --quality standard`, with
  `--repo-root` at a checkout whose `origin` is the diagram's URL); `docs/architecture/drift/req-drift.png` is its light
  1680×1070 crop. archify draws no colour per status, so the SUSPECT box differs by its line, not its fill.
- **What the gate checks** (`tests/drift_demo.rs`, through nextest): `--check-drift` on the committed directory; each edit's
  `satisfy.txt` carries the link line its direction gives and the Evidence box the matching status (the oracles are
  written in the test, not read from `edits.json`); the cosmetic edit's whole verdict is PASS; a copy-based run changes
  nothing outside the drift directory and leaves the claim byte-for-byte; a tightening edited into a loosening, or a
  hand-edited `satisfy.txt`, is stale and named; no `req` is UNVERIFIABLE; the page carries the tightening overlay's
  subtitle and every node's id, label and status; the README quotes req's words and the generated status for each edit.
  The named gap of this note's step 3 is unchanged: the page's picture is not read back.
