# P-1: the README leads with what req does (presentation)

Task id: `P-1`. Base: the commit that adds this spec, on top of `e7d882e`, where P-2 (PR #93, the guided drift page) is
merged. The owner accepted an external review of req's presentation on 2026-11-16, with corrections (below), and chose:
the hero links the live demo at `https://fadasr.github.io/req-demo/` first and req's own guided page second ("if it does
not look right, we will swap"); the red frame, not status colours.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are: `AGENTS.md` (the README is the front page and stays consistent with it: "Do not overclaim", "Never
claim the bet is validated on seed evidence", the differentiators and the README's *Status* section are referred to by
name), ADR 0040 (holds / holds (weakened) / suspect), ADR 0052 (the overlay, the red frame), ADR 0054 (the explorer), and
every test that reads `README.md` (16 files under `tests/`: they pin counts, links and phrases; keep them passing, and
change a test only where the README's restructure moves a phrase, saying which and why).

## The problem

The first screen is the mechanism (`claim_id`, canonical meaning, content addresses); the outcome, a requirement changing
meaning and the design part that depends on it being framed for re-check, is about line 35 of 185, under the fourth
heading. A newcomer leaves before seeing it.

## The new order

1. **Title and one plain sentence**, no jargon, naming the problem and the outcome, e.g. "When a requirement's meaning
   changes, req tells you which parts of your design need a second look, and only those." Then two links on one line:
   **Live demo** (`https://fadasr.github.io/req-demo/`, "135 requirements from a public Sphinx-Needs project, put through
   req") and **What drift looks like** (`https://autox-my.github.io/req/drift.html`).
2. **Before and after, side by side**: `docs/architecture/drift/req-before.png` and `req-tightening.png` (the matched pair
   P-2 made; same size, the only difference is the Evidence box and its red frame), in a two-cell table, each linking to
   the guided page. A one-line caption: one requirement (REQ-0010) tightened on a copy; the part that implements it is
   framed for re-check; nothing else changes.
3. **Three edits, three verdicts**: a three-row table in req's real output words, read from
   `docs/architecture/drift/edits.json` and each edit's committed `satisfy.txt` (never typed from memory): reword → HOLDS,
   loosen → HOLDS (weakened), tighten → SUSPECT. One sentence under it: a tool that flags every edit would raise the
   alarm three times; req raises it once, because it judges the *direction* of the change (ADR 0040).
4. **What SUSPECT means**, in two plain sentences (as on the guided page).
5. **Try it in three commands**: clone, build (`cargo build --release`, since the latest published release predates
   `req.claim/3` and cannot read today's claims; say so in one clause), and one script, `tools/demo/try_drift.py`, that
   copies the repository to a temporary directory, applies the tightening edit from `edits.json`, runs `req satisfy`
   there, prints its lines, and deletes the copy. It never edits this checkout. The script is new in this unit.
6. **How it works**, as a Mermaid diagram (renders on GitHub): requirement edited → meaning fingerprint → direction of the
   change → each recorded design link judged → the part holds or is framed. Then two or three sentences.
7. **Where to look next**: the architecture pages, the coverage explorer, the coverage matrix, the ADRs. These move here
   from today's *Requirements on the architecture* section; keep every number the tests pin, read from the generated files.
8. **Then the existing depth, in this order**: what a claim is (today's opening paragraphs, moved, not rewritten beyond
   joining them up), *The bet, stated so it can fail*, *What it does*, *The two differentiators*, *Status* (unchanged in
   substance: the honest limits stay), *Install*, *Development*, *Licence*.

## What the README must not say

- No claim that no other tool does this, and no comparison table against named tools: neither is sourced here. What it may
  say, because ADR 0040 read OpenFastTrace and Doorstop at source, is the narrower point: req judges the direction of a
  change, so a re-wording raises no alarm and an admitted loosening does not re-open the link.
- No "colour flips": the part is framed in red (ADR 0052, O-7).
- No invented example: the example is REQ-0010 and the three committed edits.
- Nothing that contradicts *Status*.

## Images

- Use `req-before.png` and `req-tightening.png`. `docs/architecture/drift/req-drift.png` (the older, differently sized
  tightened picture) is then unused: delete it, remove it from `DRIFT_ROOT_FILES` in `tools/graph/build_architecture.py`
  and from the leftovers filter in `tests/drift_demo.rs`, and remove any other reference to it (grep).
- The pictures' 1,070-pixel crop clips the top of the legend at the bottom edge (P-2 review 2). If you can, re-crop all
  three P-2 pictures identically so the legend is whole (same width, the same crop offset, a taller height), re-run the
  pixel check (same size, the only differing region the Evidence box and frame), and update each `.png.json` record's
  `png_sha256` and `recipe`. If that cannot be done cleanly from the existing renders, leave them and say so.

## Tests first (envelope `red`)

- `tests/readme_front.rs` (new): the first non-empty paragraph after the title has no `claim_id`, `canonical` or
  `content address`; the two hero links are present in that order; the two hero images exist and have identical pixel
  dimensions; the three-row table's outcome words equal the committed `satisfy.txt` lines (read independently); the
  Mermaid block is present; `try_drift.py` is mentioned; *Status* is still a heading.
- `tests/try_drift.rs` (new): running `tools/demo/try_drift.py --req <bin>` prints the SUSPECT line for `src/bench.rs ->
  REQ-0010` and exits 0 when it did that (say which exit you choose and why), and the checkout is byte-identical
  afterwards (`git status --porcelain` empty in a copy-based test).
- Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Put the mechanism paragraph back first → the front test fails.
- Swap loosen's and tighten's verdicts in the README table → the front test fails.
- Make `try_drift.py` edit the real claim file → the untouched test fails.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Disk is tight (~18 GiB free): one target directory, delete temporary copies.

## Docs, one home each

- The quickstart is a demonstration, not the adopter's loop: link `docs/adopting.md` for adopting req, do not restate it.
- ADR 0052: one dated line saying `req-drift.png` was retired for the matched pair.

## Allowed paths

- `README.md`, `tools/demo/try_drift.py` (new), `tests/readme_front.rs` (new), `tests/try_drift.rs` (new)
- existing tests that read `README.md`, only where a moved phrase requires it (name each in the envelope)
- `docs/architecture/drift/req-drift.png` (delete), `docs/architecture/drift/req-before.png`, `req-loosening.png`,
  `req-tightening.png` and their `.png.json` records (only the re-crop), `docs/drift.html` (only if the re-crop changes it,
  via `tools/demo/drift_page.py --out`)
- `tools/graph/build_architecture.py` and `tests/drift_demo.rs` (only `req-drift.png`'s allow-list entry)
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the one line)

Forbidden: `src/**`, `claims/**`, `.req/**`, `baselines/**`, `decisions/**`, `aliases/**`, `bench/**`, `docs/spec.md`,
`docs/trace.md`, `AGENTS.md`, `CONTEXT.md`, `docs/adopting.md`, every other generated page, `scripts/check.sh`,
`Cargo.toml`, `Cargo.lock`, `.github/**`, any archify file. If a forbidden file must change, stop and report.
