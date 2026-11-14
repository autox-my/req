# O-2: req's own files declare the claims they satisfy (ADR 0052 decision 7, step 2)

Task id: `O-2`. Base: the commit that adds this spec, on top of `c939077`, where O-1 (PR #77) is merged. The owner
ratified ADR 0052 by merging PR #76 and asked for this step by merging PR #77 (2026-11-14).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decisions 1, 5 and 7;
- the ADR 0040 file (`docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md`): decision 1 (a link is declared in
  the element, as a `req-satisfies:` line) and decision 2 (**only a human records a link**).

Where this spec and an ADR disagree, the ADR wins; stop and report.

## What this unit is, and is not

This unit **proposes** links. It adds `req-satisfies:` lines to req's own source files and nothing that makes req read
them. It does **not** create `.req/design` and does **not** write `.req/satisfied`: recording is the owner's act
(ADR 0004, ADR 0040 decision 2). The gate stays green because no design root is declared yet; the owner turns the links
on after merging, with the commands this unit prepares (choice 5).

## Choices fixed here

1. **Which files.** Code req ships or runs: `src/*.rs`, `tools/*.py`, `tools/graph/*.py`, `scripts/*.sh`. Never
   `tests/`, `docs/`, `claims/`, `bench/`, `examples/`, a generated file, or anything git ignores
   (`tools/ocr/node_modules` is ignored and must never become a root).
2. **Which claim, where.** For each of req's claims (`claims/REQ-*.yml`, 40 today), read its statement and find the
   **narrowest** file whose code carries out the obligation. A claim may be satisfied by more than one file, and a file
   may satisfy several claims, but:
   - put the link where the obligation is enforced, not where it is merely called: `src/main.rs` only for a claim about
     the command-line surface itself;
   - a claim no file carries out (a process or evidence claim met by a document or a measurement, not by code) gets
     **no** link. List it, with one line why, in the PR body. Do not stretch a link to cover it;
   - no guessing from names: read the code you link, and be ready to cite the lines.
3. **The line.** In the file's header comment, one line per claim, in the file's comment syntax:
   `//! req-satisfies: REQ-0024 — <why, in at most ~80 characters>` (Rust module docs), `# req-satisfies: REQ-0024 — …`
   (Python, shell). The token stops at the space, so the reason after it is free text (ADR 0040 S-2 note). Lines sorted
   by claim id. No other change to any file: added comment lines only.
4. **No new mentions or unknown claims.** A scratch run with every chosen root declared must show the four existing
   files' mentions only (O-1), no UNKNOWN CLAIM, and every link UNRATIFIED.
5. **The owner's commands, in the PR body.** Exactly what the owner runs after merging, in order:
   - the `.req/design` content (one root per line, the narrowest set of roots covering every linked file);
   - one `req satisfy --record <element> <claim-id>` per link, sorted;
   - `req satisfy` and `scripts/check.sh` as the check, expected PASS with every link HOLDS.
   Also a table: claim, area (from the claim's `area`), file(s), the reason; then the unlinked claims with why.

## Tests first (envelope `red`)

`n/a: comment lines only, no behaviour change`. Instead, the evidence in the envelope's `tests`:
- the scratch run of choice 4, on a copy of the branch with the proposed `.req/design` and every link recorded there:
  `req satisfy` PASS, every link HOLDS, mentions unchanged; then `req check` on that copy, exit 0;
- `git diff <base>...HEAD` adds only lines containing `req-satisfies:` (plus the spec's docs), removes none: show the
  command you ran to check it.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do not set
`CARGO_TARGET_DIR`. Delete the scratch copy when done.

## Docs, one home each

- ADR 0052: a dated implementation note for step 2: how many claims are linked, through how many files, how many are
  left unlinked and why in one sentence, and that the owner records them after merge.

## Allowed paths

- `src/*.rs`, `tools/*.py`, `tools/graph/*.py`, `scripts/*.sh`: added `req-satisfies:` comment lines only
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md` (the note)

Forbidden: `.req/**` (in particular `.req/design` and `.req/satisfied`), `claims/**`, `baselines/**`, `decisions/**`,
`aliases/**`, `bench/**`, `tests/**`, `docs/spec.md`, `docs/trace.md`, `Cargo.toml`, `Cargo.lock`, `.github/**`, and
any line that is not an added `req-satisfies:` comment in a code file. If a forbidden file must change, stop and report.
