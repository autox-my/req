# 0050. A graph page shows a declared slice, grouped, and says what it leaves out

Extends ADR 0046 (the trace document is the graph contract, and archify renders it) and amends its *Falsification* item 3.
No command, flag or projection is added. No `RULES_V` bump, no claim-schema or Baseline change, no address moves.
`req.trace/1` gains no field.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it, together with the code (task
V-2). Asked for by the owner on 2026-11-13, after seeing the first rendered page. Date proposed: 2026-11-13.

## Context

V-1 delivered one page for the whole selection. This repository's own graph rendered and passed every archify gate.
The owner then read the page and found five problems.

**The layout reads badly.** archify's receipt counted 34 routes with more bends than it suggests and 13 longer than
they need to be. The causes:
- inside a layer, columns follow claim id, not the graph, so a child often sits far from its parent;
- an edge that skips layers detours through a gutter;
- a layer wraps after six boxes, so the picture stops matching the layers.

**It does not scale.** This was measured on 2026-11-13 against synthetic corpora, not adopter data. Each corpus was a
generated tree of claims with derives-from relations, a refines cross-link on every seventh claim, and every tenth claim
standing alone. Each went through V-1's three commands with archify `7e45f36` on one Apple-silicon laptop.

| claims | archify `finalize` | gates | page | canvas |
| --- | --- | --- | --- | --- |
| 40 (this repository) | about 15 s | all pass | 0.9 MB | one screen |
| 200 | 11 s | all pass | 1.3 MB | 1208 × 6122 |
| 500 | 35 s | all pass | 1.9 MB | 1208 × 14622 |
| 1000 | exit 1 | `validate` **not judged** | none | none |

At 1000 claims archify's checker overran its 64 MiB output buffer and judged nothing. Well before that, a 500-claim
page is twelve screens of scrolling. No layout makes that one page readable.

**The page's own text is not req's.**
- The title runs the selection words into one heading.
- The claim panel says "BACKEND", "Architecture component", "Semantic passport" and "Upstream". These are archify's
  words for its own diagram types.
- Below the graph sit one card per claim, repeating each statement, so the cards add little.

**archify already has the seams.** At `7e45f36`:
- `meta.translations` overrides any reader string, including the panel's labels and a component type's display name;
- `boundaries` draws a labelled frame around a set of components, and the panel shows that label as the component's
  context;
- `meta.views` offers up to five focus views;
- `cards` are free-text lists.

What archify cannot do without a change of its own: show fields beyond its fixed slots (a claim's tests, rationale or
pins), change the panel's layout, or link one page to another. The owner's direction is to fork archify for those
later. They are not part of this decision.

**Subjects are not groups.** The subject registry names who a statement is about, for the meaning fingerprint (ADR
0020). In this repository, 33 of 40 claims parse to `REQ_TOOL`. Grouping by subject gives one group, and it would give
a Tier-1 input a second job. A category an author chooses is a separate proposal (ADR 0051).

## Decision

1. **A page shows a declared slice.** Every page states its slice in its subtitle:
   - the selection words;
   - the group it shows, or `all claims`;
   - `N of M claims` and the number of relations drawn;
   - the rules version and the archify commit.

   A page never presents a slice as the whole graph.
2. **Groups are families**, computed from the document alone.
   - A **root** is a claim with no outgoing `derives-from` or `refines` edge to a claim the document holds.
   - A claim's **family** is the lowest-id root reachable from it over those edges, following the same cycle rule as
     the layout (ADR 0046 implementation note).
   - A claim with no edge at all, in or out, is **standalone**. Standalone claims form one group of their own.
   - A family is named by its root's id. Families are ordered by root id, and the standalone group comes last.
   - If a corpus later declares an author-chosen grouping (ADR 0051, if ratified), that replaces the family as the key.
     The page rules here do not change.
3. **One page or several, by size.**
   - If the document holds at most `PAGE_LIMIT` claims, there is one page. Each family gets a `boundaries` frame, and
     when there are five families or fewer, each also gets a focus view.
   - Otherwise there is one page per group plus an **index page**. The index page has one node per group, and one
     connection per ordered pair of groups and relation, labelled `<relation> ×<count>`.
   - A group larger than `PAGE_LIMIT` is cut into consecutive parts by layer, and each part declares itself
     (`part 2 of 3`).
   - `PAGE_LIMIT` is 150, a named constant beside the converter. It sits well inside the measured 500-claim pass, and
     it is about what one page stays readable at.
4. **An edge leaving the slice is still drawn.** It ends on a stub node, labelled with the target's id, whose panel
   names the group page that holds it. A reader can see that a dependency exists, and where it lives.
5. **Layout is the layered method, still deterministic.** The rows stay as ADR 0046 set them: a node's layer is its
   longest `derives-from`/`refines` path to a root. Within a page:
   - each layer is ordered to reduce crossings, by repeated median sweeps down and up, a fixed number of passes, with
     ties broken by claim id;
   - an edge that skips layers runs through a reserved waypoint in each layer it crosses;
   - a connected layer is never wrapped;
   - each family is laid out as its own block, blocks run left to right in family order, and standalone claims sit in
     a grid of their own;
   - every coordinate is an integer, with no randomness and no clock.
6. **The page speaks req's words.**
   - `meta.translations` relabels the component types as the claim states (applies, undecided, does not apply,
     withdrawn, not in the corpus).
   - It relabels the panel's own words for a claim, and its reach directions as req says them.
   - The title is `Requirement graph`, followed by the group on a group page; the slice goes in the subtitle.
7. **The cards say what a reader needs.**
   - The first card holds the findings: every claim that is withdrawn, undecided, not applying or dangling, each with
     its reason.
   - It is followed by one card per group on the page, listing `claim id: statement` in claim-id order.
   - No card is written per claim.
8. **ADR 0046's *Falsification* item 3 now reads:** "a claim, an edge or a relation label in the page's declared slice
   is missing from that page, or one is present there that the document does not hold". A stub node (decision 4) and
   an index connection (decision 3) are counted from the document and are not inventions. The union of a run's group
   pages holds every claim and every edge in the document.

## Consequences

- A reader opens the index, then the group they care about, and each page fits on a screen or two.
- A corpus of any size renders, because no page grows past `PAGE_LIMIT`. Whether archify gives a pass for every page
  is still measured, not assumed.
- The converter's output becomes a directory of diagram files, not one file. The adopter loop in
  `examples/graph/README.md` runs `finalize` once per file.
- A family can be a poor grouping. A corpus with one deep root puts nearly everything in one family, which is then cut
  into parts by layer. Author-chosen grouping (ADR 0051) is the remedy that is on offer.

## Rejected

- **Group by subject.** It gives one group here, and it reuses a fingerprint input.
- **Group by `rationale.source`.** It is free text, so two spellings of one source would split a group.
- **One page with a zoom.** It is the 500-claim page again, and archify judged nothing at 1000.
- **Collapse and expand inside one page.** archify has no collapse, and adding one would fork archify for something
  separate pages already do.
- **Drop the edges that leave a slice.** A page would then claim a claim has no dependency when it has one in another
  group.
- **A layout library (ELK or similar).** It is a new pinned dependency for a layout that about 150 lines of standard
  library code compute deterministically.

## Falsification

This decision is wrong if any of these is observed:

1. a page's subtitle does not state its slice, or a group page's counts do not match the document;
2. a claim in the document appears on no group page, or on two;
3. an edge with an end on a page is missing from that page, either as a drawn edge or as a stub;
4. two runs over one document give different bytes in any output file;
5. a page holds more than `PAGE_LIMIT` claims;
6. on this repository's own graph, archify's `routesOverSuggestedBends` is not lower than V-1's 34.

## Implementation note (2026-11-13, unit V-2)

The converter (`tools/graph/trace_to_archify.py`) now writes a directory, as decisions 1–8 say, with these choices the
decisions left open.

- **Sweep count.** `SWEEPS = 8`: eight median passes over the layers, alternately down and up, ties broken by claim id.
  A coordinate pass then runs `ALIGN_PASSES = 4` times (each slot goes towards the mean x of its neighbours, kept in order
  and at least a slot apart by a least-squares fit, floor division throughout). Both are fixed counts, so one document gives
  one layout.
- **Waypoints.** A long edge reserves one slot in each layer it crosses; the slot takes part in the median ordering, and
  the route's `via` list passes through the slot's centre in that layer's row.
- **Part cutting.** A group is cut by layer: layers fill a part whole until the next would not fit. A single layer still
  over `PAGE_LIMIT` is cut in **claim-id order** into consecutive parts of at most `PAGE_LIMIT`, because a group can be one
  layer wide (the standalone group always is, and a corpus with no relations puts every claim there). Part 1 keeps the
  group's page name; later parts are `<name>-part-2` and so on.
- **Edges and stubs.** An edge is drawn on every page that holds either end, so the union of the group pages holds every
  edge, some twice (falsification 3 reads "an edge with an end on a page"). The far end is a stub when the document holds
  it and a dangling target when it does not; both use the `cloud` (missing) type, told apart by their tag (`on another page`
  and `not in the corpus`). A stub sits in its own `region` frame labelled `page <name>`: that frame is the panel's context,
  and archify offers no per-node context field.
- **Index page.** Nodes sit in V-1's grid, six wide, in group order. A group's node is named by its first page; its tag
  counts claims and, past one page, pages. The subtitle counts the relations that run between groups, and every claim
  as drawn (`M of M claims`).
- **Translations** (keys of archify's `locales/en.json`): `viewer.kind.*` and `legend.architecture.*` (the states, in the
  words `meta.legend` already used), `node.context.architecture` and `viewer.passport.eyebrow` (`Claim`),
  `viewer.passport.reach`, `.upstream`, `.downstream`, their `.trace` forms, `viewer.passport.reach.{upstream,downstream}`
  `.one`/`.other`, `.noUpstream`/`.noDownstream`, `viewer.passport.close` and `.move`. The index page says `Group`.
  **archify applies `meta.translations` only to a document that names `meta.locale`**; without it every override is
  silently ignored and the receipt reports nothing, so the converter writes `locale: "en"`.
- **Reach mapping.** archify's `upstream` follows a connection backwards (`assets/template.html`, `computeReachability`: for
  `upstream`, `relationship.to === current` moves to `relationship.from`). An edge runs from the claim that derives or
  refines to the claim it rests on, so `upstream` holds the claims that rest on the focused one: `viewer.passport.upstream` is
  `Rests on this`, `viewer.passport.downstream` is `This rests on`. Confirmed in a rendered page's i18n data.

**Verification outside the gate** (archify `7e45f36`, `ARCHIFY_UPDATE_CHECK_DISABLED=1`, `--quality standard`, one
Apple-silicon laptop; HTML kept in `.scratch/`, not committed).

- **This repository's graph** (`.req/variants` line 1, 40 claims, two families and no standalone claim, one page):
  `validate`, `deliver` and `check` pass; `browser-check` **fails**. `routesOverSuggestedBends` is **4** (V-1: 34), over-stretch
  **1** (V-1: 13), so falsification 6 holds. The `browser-check` failure is `viewer/projected-text-readability`, at the
  floor described below.
- **Synthetic 500- and 1000-claim corpora** (a heap-shaped tree of `derives-from`, a `refines` cross-link on every seventh
  claim, every tenth claim standalone; generated by a scratch script, not committed). 500: 6 pages, largest 150 claims;
  1000: 9 pages, largest 150. In both, the index and the standalone page pass every gate, and **every page of the family
  fails**: `validate` does not pass (archify refuses a frame label past its canvas edge on a canvas 10 000 to 80 000 px
  wide), so `browser-check` does not run. The 1000-claim corpus does render, with no buffer overrun, which V-1 could not do,
  but it does **not** pass on every page.

**What the measurement found.** `browser-check` shrinks the drawing to the reader's window and fails any text it would
project below 6 px. Edge labels are 9 px at source, so on a 1440 px window the canvas may be about 1700 px wide. A row of 7
claims at this layout (1610 px) passes; a row of 8 (1830 px) fails. Decision 5's rule that a connected layer is never
wrapped, and decision 3's limit of 150 claims, cannot both hold with that floor: a heap-shaped tree puts 128 claims in one
layer, and this repository's widest layer is 11 wide. The limit is on claims, but the readable page is limited by the
widest row. This is a finding about the decisions, not a defect of the converter, and the decisions are unchanged here.
Two remedies are open to the owner and neither is taken in this unit: cut parts by row width as well as by claim count, or
wrap a row past a width (which returns to the problem that decision 5 set out to remove).
