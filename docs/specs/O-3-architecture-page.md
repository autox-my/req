# O-3: req's architecture, with the claims each part satisfies, on the project's pages (ADR 0052 decision 7, step 3)

Task id: `O-3`. Base: the commit that adds this spec, on top of `b0dbee4`, where the owner's records (PR #79) are merged:
`.req/design` lists `src`, and `.req/satisfied` holds 30 links, all HOLDS.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decisions 1–5 and 7 (step 3),
  and its O-1 note (the converter, its exit codes, the tag width rule);
- `docs/adr/0046-the-trace-document-is-the-graph-contract.md`: decision 7 (archify pinned at `7e45f36`, offline,
  `ARCHIFY_UPDATE_CHECK_DISABLED=1`);
- `tools/overview/build.py` and `tests/overview.rs`: the precedent for a committed, generated page with a `--check`.

Where this spec and an ADR disagree, the ADR wins; stop and report. This page is the project's showcase: it must be
correct first and handsome second, and it must never show anything the tool did not compute.

## Deliverables

1. **The diagram, authored: `docs/architecture/req.archify.json`.** An archify `architecture` diagram of req, drawn in
   archify's repository mode by following its `references/repository-authoring.md` in the scratch clone
   (`.scratch/graph/archify/archify`; read it, never copy it into this repository).
   - `meta.repository`: `https://github.com/autox-my/req`, provider `github`, `link_mode: "web"`, and the revision of
     this spec's base commit **as pushed to origin** (`b0dbee4…`, full 40 hex), so every source link opens on GitHub.
   - Components are req's real parts, each citing one to three files in `sources` with line ranges you read. Every one
     of the 14 files that declares a `req-satisfies:` link (`git grep -l 'req-satisfies: REQ-' -- src`) is cited by
     the component that owns it. Also show the stores (`claims/`, `decisions/`, `baselines/`, `.req/`), the importers
     (`tools/*_import.py`, `tools/adopt.py`) and the CLI entry. A component that would need more than three files is
     split along a real seam, never padded or merged to fit.
   - Connections are the calls and reads/writes the source shows (repository-authoring step 3), not decoration.
   - Boundaries group the parts the way a reader thinks about req: the 8 areas of `.req/areas` are a good guide for
     the components that serve claims; the stores and the importers sit outside them.
   - The layout passes archify `validate` and every `finalize` gate at `--quality standard`, with no overflow.
2. **The overlay, generated: `docs/architecture/overlay.json` and `docs/architecture/matrix.md`**, by
   `tools/graph/arch_overlay.py` from the diagram, `req satisfy --format json` and `req trace --format json` of this
   repository. Byte-for-byte what the converter writes; never hand-edited.
3. **The build script: `tools/graph/build_architecture.py`.** Python standard library only, offline.
   - `--out <dir>` runs `req satisfy --format json`, `req trace --format json` and the converter, and writes the two
     generated files into `<dir>` (`docs/architecture`).
   - `--check <dir>` regenerates into a temporary directory and compares: exit 0 current, 1 stale (naming the file),
     3 `req` cannot be run (UNVERIFIABLE), 2 usage. The converter's own exit 3 (a stale citation or an unknown
     claim) is a failure of the page, so `--check` reports it as 1, naming the reason.
   - It takes the `req` binary from `--req <path>` or `PATH`, like `tools/overview/build.py`.
4. **The page: `docs/architecture.html`**, rendered by archify `7e45f36` `finalize` from `overlay.json`, standalone and
   offline, published by the project's Pages site (which serves `docs/` from `main`). It is generated and committed, like
   `docs/overview.html`; it carries the req rules version and the diagram's revision in its subtitle (the converter
   puts them there). The gate cannot run archify (no Node, no browser in `check.sh`), so:
   - if archify's HTML embeds the diagram document in a form a test can read back, `tests/architecture_page.rs`
     asserts it equals `overlay.json`; state which you found;
   - if it does not, say so in ADR 0052's note as the named gap: the HTML's currency is the regenerating author's
     responsibility, while `overlay.json` and `matrix.md` are checked.
5. **An image for the README: `docs/architecture/req.png`** (or `.svg`), exported by archify's own export (see its
   `references/delivery-contract.md`), of the overlay. If archify cannot export offline at `7e45f36`, say so and skip it.
6. **The README and the Pages index.**
   - `README.md`: a short section near the top, *Requirements on the architecture*: the image (when it exists), one
     sentence on what it shows (each part of req, coloured by whether the code still satisfies the requirements it
     was recorded against), the counts the matrix shows (claims linked, links holding), and links to the page, the
     matrix and ADR 0052. Numbers come from the generated files, never typed from memory.
   - `docs/index.html`: one list item linking `architecture.html`, in the voice of the existing items.

## Tests first (envelope `red`)

- `tests/architecture_page.rs`, in `tests/overview.rs`'s style, with `CARGO_BIN_EXE_req`:
  - `build_architecture.py --check docs/architecture` exits 0 on the committed files;
  - a copy of the repository where one linked claim's statement is tightened (a scratch copy, never this checkout)
    makes `--check` exit 1 naming `overlay.json` (the overlay would now show SUSPECT): the page cannot silently go
    stale;
  - deleting a cited file in a copy makes `--check` exit 1 with the stale reason;
  - if deliverable 4 found an embedded document, the HTML-equals-overlay assertion.
- Prove the first test red at the base (the files do not exist).

## Mutation checks (envelope `mutations`)

- Hand-edit one cell of `matrix.md` → the current test fails.
- Make `--check` ignore the converter's exit 3 → the deleted-file test fails.

## Verification outside the gate (envelope)

- archify `validate` and `finalize --quality standard --json` on `overlay.json`: every gate's result.
- Open `docs/architecture.html` in archify's browser check and confirm the source links point at
  `github.com/autox-my/req/blob/<revision>/…`.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Disk is limited: one target directory; delete scratch copies.

## Docs, one home each

- ADR 0052: a dated note for step 3: the files, how to regenerate (one command), what the gate checks and the named gap
  if any.
- `examples/graph/README.md`: one line pointing at `docs/architecture/` as a worked example.

## Allowed paths

- `docs/architecture/**` (new), `docs/architecture.html` (new), `docs/index.html` (the one item)
- `tools/graph/build_architecture.py` (new)
- `tests/architecture_page.rs` (new)
- `README.md` (the one section)
- `examples/graph/README.md` (the one line)
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the note)

Forbidden: `src/**`, `tools/graph/arch_overlay.py`, `tools/graph/trace_to_archify.py`, `claims/**`, `.req/**`,
`baselines/**`, `decisions/**`, `aliases/**`, `bench/**`, `docs/spec.md`, `docs/trace.md`, `docs/overview.html`,
`scripts/check.sh`, `Cargo.toml`, `Cargo.lock`, `.github/**`, and any archify file. If the converter needs a change to
make this page work, stop and report it instead of changing it.
