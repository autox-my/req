# A-2: the graph groups by area (ADR 0051 decision 5, ADR 0050 decision 2)

Task id: `A-2`. Base: the commit that adds this spec, on top of `a96ccf0`, where V-2 (PR #70) and A-1 (PR #72) are
merged. Owner go-ahead of 2026-11-13: A-2 was planned as "group the graph by area, after V-2 and A-1 merge", and both
merged that day.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0051-a-claim-may-name-the-area-it-belongs-to.md`: decisions 2, 3 and 5, and *Falsification* 2;
- `docs/adr/0050-a-graph-page-shows-a-declared-slice.md`: decision 2's last bullet ("an author-chosen grouping
  replaces the family as the key; the page rules here do not change") and its whole *Falsification* list;
- `docs/adr/0046-the-trace-document-is-the-graph-contract.md`: the converter reads the trace document and nothing
  else, and `req.trace/1` grows only additively (decision 7).

Where this spec and an ADR disagree, the ADR wins; stop and report.

## Choices fixed here

1. **The labels travel in the document.** An area's reader-facing name is its label in `.req/areas`, never its key
   (ADR 0051 decision 2). The converter reads only `req.trace/1` (ADR 0046), so the document carries the registry.
   - `req.trace/1` gains a top-level `areas`: an array of `{"path": …, "label": …}` in the registry's line order. It
     is `[]` when there is no `.req/areas`.
   - The key sits after `nodes` and before `edges`, and each entry keeps the field order above. The change is additive
     under `/1`.
   - The registry is parsed with `completeness::parse_areas`, the one parser. A malformed or unreadable `.req/areas`
     makes `req trace --format json` exit 2, naming every bad `<file>:<line>` as `req check` does. Nothing is printed
     to stdout.
   - Text `req trace` does not read the registry. `docs/trace.md` passes `--check` unchanged.
   - `req trace --help` names the field.
2. **The group key.**
   - If **any** node carries an `area`, the groups are areas, not families (ADR 0051 decision 5). Otherwise every
     byte of today's output stays as it is, and V-2's tests pass unchanged.
   - An area group holds the claims whose `area` equals its path exactly. `diagnostics` and `diagnostics/startup` are
     two groups, and a claim is in exactly one (ADR 0051 *Falsification* 2).
   - Claims with no area form one group named `no area`.
   - **Order:** areas the registry declares, in its line order; then undeclared areas, in path sort order; then
     `no area`, last.
   - **Name:** the registry label, or the path when the area is undeclared. `no area` is the name of the no-area group.
   - **Page file:** `area-<path with each / as .>` (for example `area-diagnostics.startup.json`). `.` is not a key
     character, so distinct paths never share a file. The no-area page is `no-area`. Parts keep V-2's `-part-<i>`
     suffix.
3. **The page rules do not change** (ADR 0050 decisions 1 and 3–8).
   - `PAGE_LIMIT`, the index, stubs, the cut into parts by layer, and the subtitle all apply as for families.
   - The subtitle's group words are the group's name.
   - On a single page, each area gets a `region` frame labelled with its name, and focus views when there are at most
     five groups.
   - The findings card comes first, then one card per area group.
4. **Inside a group.** A group's claims are laid out with V-2's layered method over the `derives-from`/`refines` edges
   whose two ends are both in the group.
   - A claim with no such edge sits in the group's own grid, inside the group's frame.
   - An edge to a claim in another group is a cross-group edge: a stub when the target is on another page, a drawn
     edge when it is on the same page. The rules are V-2's.
   - Families are not computed and not shown while areas are the key.
5. **Fit.** archify refuses a label wider than its box (`component-label-overflow`). A registry label is free text in
   any language, so the converter never puts a label it cannot fit into a node or a frame title.
   - On the index, a node is labelled with the path's last segment. The full name goes in its `tag` or panel context,
     whichever archify draws unclipped.
   - Read the box rule in archify's scratch clone and state it in the ADR note. Never copy archify's code.
   - A test pins a long, non-ASCII label, for example `Démarrage du système et journal des défauts`, rendering
     through the index and a group page without overflow.

## Tests first (envelope `red`)

Oracles come from ADR 0051, ADR 0050 and hand-written fixtures, never from a run.
- `tests/trace_nodes.rs` and `tests/json_format.rs`, for the document:
  - `areas` in registry order, `[]` without a registry;
  - its position between `nodes` and `edges`;
  - a malformed registry gives exit 2 naming `<file>:<line>`, with nothing on stdout;
  - text `trace` unaffected.
- `tests/graph_archify.rs`, for the converter, in the V-2 style, with fixtures written by hand under `tests/`:
  - **no area anywhere:** output byte-identical to the family grouping (reuse a V-2 fixture);
  - **one page by area:** about 8 claims over three areas plus two with no area give one `all.json` with three frames
    named by the labels, in registry order, then `no area`;
  - **parent and child are separate groups:** a claim at `diagnostics` and one at `diagnostics/startup` sit in two
    groups, each claim exactly once (ADR 0051 *Falsification* 2);
  - **undeclared area:** it is named by its path and ordered after the declared areas;
  - **split by area:** with the test-only `REQ_GRAPH_PAGE_LIMIT`, one page per area plus an index. Every claim
    appears on exactly one group page, and a cross-area edge ends in a stub naming the target's page;
  - **an oversized area** is cut into parts by layer, and no page exceeds the limit;
  - **long non-ASCII label** (choice 5);
  - **determinism:** two runs give byte-identical directories.

Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Group by the path's first segment instead of the whole path → the parent-and-child test fails.
- Order areas by path instead of registry order → the one-page test fails.
- Fall back to families when some claims have no area → the one-page test fails.
- Put the full label on an index node → the long-label test fails.
- Omit `areas` from the document → the converter's labelled-frame test fails.

## Verification outside the gate (envelope; not in `check.sh`)

- Run archify `7e45f36` (`.scratch/graph/archify`, `ARCHIFY_UPDATE_CHECK_DISABLED=1`) `finalize` on every page of:
  - this repository's graph, which has no areas, so it is the family output and must match V-2's numbers (23 bends,
    over-stretch 10);
  - a scratch copy of the 500-claim synthetic corpus with areas assigned over about 12 paths at two depths and a
    `.req/areas` with long labels;
  - the 1000-claim one, likewise.
- Report the page count, the largest page and every page's gates. Keep all HTML in `.scratch/` and commit none of it.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
`docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and VERIFIED. Disk is limited: use one target
directory and no extra copies.

## Docs, one home each

- `req trace --help`: the `areas` field.
- `examples/graph/README.md`: grouping by area when claims carry one, and families otherwise.
- ADR 0051: a dated implementation note for decision 5.
- ADR 0050: one line, saying the author-chosen grouping of decision 2 now exists (ADR 0051).

## Allowed paths

- `tools/graph/trace_to_archify.py`
- `src/view.rs` (the `areas` key of the trace document only)
- `src/main.rs` (the trace help page, and passing the registry to the document only)
- `src/completeness.rs` (only to expose what `parse_areas` already returns)
- `tests/graph_archify.rs`, `tests/trace_nodes.rs`, `tests/json_format.rs`, and new fixtures under `tests/`
- `examples/graph/README.md`
- `docs/adr/0051-a-claim-may-name-the-area-it-belongs-to.md` (the implementation note)
- `docs/adr/0050-a-graph-page-shows-a-declared-slice.md` (the one line)

Forbidden: `RULES_V` and any change to the canonical form or an attested hash, `src/canon.rs`, `src/model.rs`,
`src/baseline.rs`, `bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`, `docs/spec.md`,
`docs/trace.md`, `scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`, and any archify file. If a forbidden
file must change, stop and report.
