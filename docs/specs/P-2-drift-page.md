# P-2: a guided page that shows what drift does (presentation)

Task id: `P-2`. Base: the commit that adds this spec, on top of `58c5be5`, where E-1 (PR #92, the explorer) is merged. The
owner asked on 2026-11-16, after an external review of req's presentation, for "the drift moment" to be impossible to
miss: a page a visitor understands in thirty seconds. P-1 (the README) follows and links this page.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md`: holds, holds (weakened), suspect, and why a loosening
  carries a link (decision 3, the admitted carries);
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: the O-6 drift demonstration and
  the O-7 red frame (no status colour; the owner chose the frame on 2026-11-16);
- `docs/adr/0054-an-explorer-page-navigates-the-coverage-graph.md`: the explorer and its `#focus=<claim id>` address;
- `tools/explorer/build.py` + `tests/explorer_page.rs`: the generated, offline, checked page pattern this follows.

Where this spec and an ADR disagree, the ADR wins; stop and report. **Nothing on the page may claim more than req's real
output shows**: every outcome word, direction and statement on it comes from committed req output.

## What the page shows

One real claim (REQ-0010, the false-split bound, linked from `src/bench.rs`, drawn in the overview's Evidence box) and the
three edits of `docs/architecture/drift/edits.json`, each applied on a throw-away copy (O-6). A visitor picks one of three
buttons; each tells a three-step story:

1. **Before.** The overview diagram as it is (`docs/architecture/req-overview.png`): every part holds.
2. **The change.** The claim's statement before and after, with the changed words marked, and the label "applied to a
   temporary copy of the corpus; req's own claims are not edited".
3. **What req says.** The diagram after the edit, the exact `req satisfy` line from that edit's committed `satisfy.txt`,
   and one plain sentence per outcome:
   - **reword** (`at most 0.30` → `no more than 0.30`): HOLDS. The meaning fingerprint is unchanged, so no alarm.
   - **loosen** (`at most 0.40`): HOLDS (weakened). The obligation got looser; what met the stricter bound meets the
     looser one, so the link is not re-opened, and the loosening is reported by name.
   - **tighten** (`at most 0.20`): SUSPECT, framed in red. The obligation got stricter; the design may no longer meet
     it, so a human must re-check it and re-record the link (`req satisfy --record src/bench.rs REQ-0010`).

   The plain sentences are fixed text in the template, keyed by the outcome word; they must agree with ADR 0040.

Above the buttons, one short paragraph: what req does, in one sentence, and why three cases (a tool that flags every
edit would raise the alarm on all three; req raises it on one). Do not name or compare other tools on this page. Below,
**a definition of SUSPECT** in plain words ("the part was recorded against a claim whose meaning has since tightened or
changed in another direction. It does not mean the part is wrong; it means a person must re-check it and re-record the
link."), and links: the explorer focused on REQ-0010 (`explorer.html#focus=REQ-0010`), the real architecture page, the
drift page (`architecture-drift.html`), and ADR 0040 on GitHub.

## Deliverables

1. **Images.** Before: `docs/architecture/req-overview.png` (exists). Tighten: `docs/architecture/drift/req-drift.png`
   (exists, with the red frame). Loosen: render the loosening overlay with archify `7e45f36` the way O-6 rendered the
   tightening one, and save `docs/architecture/drift/req-loosening.png` (visual-check, light, cropped like the others,
   under ~600 KB); its page is not published. Reword: the diagram is identical to before; say so on the page instead of
   showing a duplicate image.
2. **The builder: `tools/demo/drift_page.py`** (Python standard library, offline), `--out` / `--check` / `--req`, the O-3
   exit contract. It reads `docs/architecture/drift/edits.json`, each edit's committed `satisfy.txt` and overlay (the
   Evidence box's status line), and runs `req compare` on each edit's before/after statements in a temporary copy to
   get the direction word; it embeds one deterministic JSON document in the page (escaped as E-1 does). It never
   edits this checkout's claims.
3. **The template: `tools/demo/drift.html`**, hand-written, plain HTML/CSS/script, no network (the E-1 network test's
   rules apply, including "no remote address in the template"), light and dark themes, keyboard-usable buttons, readable
   at phone width. Images are referenced by relative path.
4. **The page: `docs/drift.html`**, committed, published by Pages.
5. **`docs/index.html`**: this page becomes the **first** item, worded for a newcomer ("See what req does when a
   requirement changes: three edits, three verdicts").

## Tests first (envelope `red`)

`tests/drift_page.rs`, in `tests/explorer_page.rs`'s style:
- `--check docs/drift.html` exits 0; a hand-edited page exits 1;
- the embedded data's three edits equal `edits.json` (ids, before, after), and each edit's `req satisfy` line equals the
  line for `src/bench.rs -> REQ-0010` in that edit's committed `satisfy.txt` (read independently in the test);
- the three outcome words on the page are exactly `holds`, `holds (weakened)` / its JSON word, and `suspect`, matched to
  reword, loosen and tighten (a swap fails);
- the page loads nothing over the network (reuse E-1's detector and its no-remote-address rule);
- the images it references exist; two builds give identical bytes; markup in a statement stays data (as E-1-fix tested).
- Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Swap the loosen and tighten outcome sentences in the template → the outcome test fails.
- Make the loosening show SUSPECT → the outcome test fails.
- Reference an image that does not exist → the image test fails.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Disk is tight (~18 GiB free): one target directory, delete temporary copies and render folders.

## Docs, one home each

- ADR 0052: one dated line in its notes pointing to the guided page and its builder.
- `examples/graph/README.md`: one line.

## Allowed paths

- `tools/demo/**` (new), `docs/drift.html` (new), `tests/drift_page.rs` (new)
- `docs/architecture/drift/req-loosening.png` (new)
- `docs/index.html` (the first item), `examples/graph/README.md` (one line)
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the one line)

Forbidden: `README.md` (P-1 owns it), `src/**`, `tools/graph/**`, `tools/explorer/**` (the `#focus=` address already
exists), `claims/**`, `.req/**`, `baselines/**`, `decisions/**`, `aliases/**`, `bench/**`, every other file under
`docs/architecture*` and `docs/explorer.html`, `scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`, any archify
file, and any third-party JavaScript. If a forbidden file must change, stop and report.
