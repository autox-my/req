# V-2: graph pages show declared slices, grouped and laid out in layers (ADR 0050)

Task id: `V-2`. Base: the commit that adds this spec and ADR 0050. ADR 0050 is proposed; the PR that carries this work
ratifies it. Owner go-ahead of 2026-11-13 ("yes", answering the proposal to write the V-2 spec and the ADR 0046
amendment).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contract is `docs/adr/0050-a-graph-page-shows-a-declared-slice.md` (decisions 1–8 and its *Falsification* list),
on top of `docs/adr/0046-the-trace-document-is-the-graph-contract.md`. Where this spec and an ADR disagree, the ADR
wins; stop and report.

This unit changes the converter only. `req` itself, `req.trace/1` and every committed byte outside the allowed paths
stay as they are.

## The external tool

archify `https://github.com/tt-a1i/archify` at commit **`7e45f36`**, Apache-2.0, read in a gitignored `.scratch/`
clone exactly as V-1 did (`docs/specs/V-1-graph-via-archify.md`, *The external tool*). Never commit or copy its files.
Every archify command you run sets `ARCHIFY_UPDATE_CHECK_DISABLED=1`.

The fields this unit uses, all in `archify/schemas/architecture.schema.json` and `common.schema.json`:
- `boundaries` (`kind: region`);
- `meta.views` (at most 5);
- `meta.translations`, with keys from `archify/locales/en.json`: `viewer.kind.*`, `legend.architecture.*`,
  `node.context.architecture`, `viewer.passport.*`;
- `cards`.

Read each definition before you write it.

## Choices fixed here

1. **Usage:** `trace_to_archify.py <trace.json> <out-dir>`. A `-` still reads stdin.
   - It writes `<out-dir>/index.json` only when there are several pages, and one `<out-dir>/<page>.json` per page.
   - Page names: `all` for a single page; otherwise `family-REQ-0001`, `standalone` and `…-part-2`.
   - It refuses an existing non-empty `<out-dir>` (exit 2, naming it), so stale pages never mix with new ones.
   - It writes nothing on any refusal. The existing refusals (schema, malformed JSON, empty `nodes`, bad
     `superseded_by`) keep their messages and exit 2.
2. **Constants** beside the existing ones: `PAGE_LIMIT = 150`, and a fixed sweep count for the median ordering (any
   value from 4 to 24; state it).
3. **Families, layers and parts**, as ADR 0050 decisions 2, 3 and 5 state them. Families are computed once, over the
   whole document, before any page is cut.
4. **Layout** (ADR 0050 decision 5), replacing V-1's grid:
   - median ordering;
   - one waypoint per crossed layer for a long edge, with the route passing through the waypoint coordinates;
   - a layer wider than `WRAP` boxes continues on further bands below, in the same median order (owner decision of
     2026-11-13, which superseded this spec's first wording, "no wrap of a connected layer"; ADR 0050 decision 5);
   - family blocks left to right, a block that would pass `MAX_WIDTH` starting a new row of blocks;
   - standalone claims in a grid at most six wide.

   The V-1 rule stays: every edge is routed explicitly, with its label on routed segment 1, and no route crosses an
   unrelated node. Integers only.
5. **Stubs** (ADR 0050 decision 4) use the existing `missing` style, kept distinct from a dangling edge's target by
   their tag, and their panel context names the page that holds the target. A dangling target stays as V-1 draws it.
6. **Text** (ADR 0050 decisions 1, 6 and 7):
   - Title: `Requirement graph`, or `Requirement graph: <group>` on a group page.
   - Subtitle: `<selection words> · <group or all claims> · <N> of <M> claims · <E> relations · rules_v <n> ·
     archify 7e45f36`. On an index page it reads `index of <G> groups` in place of the group.
   - Translations: the five states, as `meta.legend` already words them, and the panel's labels. "Semantic passport"
     becomes `Claim`, and the default context becomes `Claim`.
   - The reach directions: work out from a fixture which archify direction holds the claims that rest on the focused
     one. Label that one `Rests on this` and the other `This rests on`, and state in the ADR note which archify key
     went where.
   - Cards: a findings card first, only when non-empty, then one card per group on the page. Each card item is
     `REQ-x: <statement>`; a finding item adds its reason in req's words.
7. **No source links, no cross-page links.** archify `7e45f36` has no inter-page link, and the claim panel's extra
   fields are the owner's later archify fork. The index page's cards list each group page's file name instead.

## Tests first (envelope `red`)

Oracles come from ADR 0050 and hand-written fixtures, never from a run. Extend `tests/graph_archify.rs` in the G-1/V-1
style, with fixtures written by hand under `tests/`:

- **one page:** a fixture of about 8 claims in two families plus one standalone claim gives exactly one `all.json`,
  holding two `region` boundaries named by the root ids, two focus views, and the standalone claim outside both
  boundaries;
- **split:** a fixture with more claims than a test-only limit gives an index plus one page per group. Expose the limit
  as an environment override read only when set, for example `REQ_GRAPH_PAGE_LIMIT`, and say so in the README. Check:
  - every document claim appears on exactly one group page (falsification 2);
  - the index has one node per group, and one connection per (group pair, relation) with the right `×count`;
- **a cross-group edge** appears on its source's page, ending in a stub that names the target's page (falsification 3);
- **an oversized family** is cut into parts by layer, no page exceeds the limit, and each part's subtitle says
  `part i of n` (falsification 5);
- **the subtitle and counts** match the fixture (falsification 1);
- **ordering:** on a hand-built three-layer fixture where claim-id order crosses two edges and the median order crosses
  none, the output's x order inside a layer is the median order;
- **determinism:** two runs give byte-identical directories (falsification 4);
- **refusals:** an existing non-empty `<out-dir>` is exit 2 with nothing written, and the V-1 refusals still hold;
- **V-1's** node/edge equality, no-network-import and distinct-marking tests are kept, adapted to the directory output.

Prove them red at the base.

## Verification outside the gate (envelope; not in `check.sh`)

- Render this repository's graph (`.req/variants` line 1) and report the receipt gates. Report
  `routesOverSuggestedBends` and the over-stretch count beside V-1's 34 and 13. Falsification 6 needs fewer bends.
- Generate the synthetic 500- and 1000-claim corpora with a scratch script (ADR 0050 *Context* describes the shape).
  Render every page and report, per corpus: the page count, the largest page, and each page's gates. The 1000-claim
  corpus must now pass on every page.
- Keep all HTML in `.scratch/` and name the paths. Commit none of it. If `finalize` fails, fix the converter, never an
  output file.

## Mutation checks (envelope `mutations`)

- Order a layer by claim id instead of the median sweep → the ordering test fails.
- Drop the stub for a cross-group edge → the cross-group test fails.
- Skip the cut into parts → the oversized-family test fails.
- Put a claim on two pages, for example in both its family and the standalone group → the exactly-once test fails.
- Iterate a set unsorted while ordering → the determinism test fails (show it, then restore).

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
`docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and VERIFIED.

## Docs, one home each

- `examples/graph/README.md`:
  - the directory output, and the `finalize` loop over its files;
  - what a slice is and what the subtitle states;
  - the agent rule unchanged: never add, remove, relabel or fix a node or edge; placement fields only, said aloud.
- ADR 0050: a dated implementation note covering the sweep count, the translation keys used, the reach mapping and the
  verification numbers.
- ADR 0046: one line under its status, "Extended by ADR 0050; *Falsification* item 3 amended there."

## Allowed paths

- `tools/graph/trace_to_archify.py`
- `tests/graph_archify.rs`, and new fixtures under `tests/`
- `examples/graph/README.md`
- `docs/adr/0050-a-graph-page-shows-a-declared-slice.md` (the implementation note)
- `docs/adr/0046-the-trace-document-is-the-graph-contract.md` (the one line)

Forbidden: everything under `src/`, `RULES_V`, `bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`,
`.req/**`, `docs/spec.md`, `docs/trace.md`, `scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`, and any archify
file. If a forbidden file must change, stop and report.
