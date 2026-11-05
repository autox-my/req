# #124 — a Sphinx-Needs `needs.json` reader

Task id: `124-1` (one unit). Code baseline: current `main`. Issue #124; the decision is **[ADR
0027](../adr/0027-needs-json-reader-drafts-with-the-need-id-in-provenance.md)** (proposed; dispatched only after
it merges). Read the ADR first: the contract is there and is not restated here. Read `tools/adopt.py`,
`tests/adopt.rs` and the *Adopting from documents you already have* section of `docs/adopting.md` too: this unit is
`adopt.py`'s sibling and follows it wherever the ADR is silent.

**Model:** `claude-sonnet-5-5` (high). Reviewed read-only on a different family (`124-1-review-…`): it decides what
becomes a draft and what is named as skipped.

## Why

F8 of the architecture review: adoption should not mean retyping. The format with customers in this niche is
Sphinx-Needs, and its export is `needs.json`. ADR 0017 decided the rule; nothing reads the file.

## Unit 124-1 — `tools/needs_import.py`

1. **The script** (new, python3 stdlib only, standalone: copy the corpus reader, id allocation, registry check, `req
   --version` schema check and claim template from `adopt.py`; do **not** import `adopt.py` and do **not** edit it).
   - Arguments: `--from <needs.json>` (required; relative to the corpus root), `--origin <url-or-path>` (required,
     non-empty: recorded in each draft's `rationale.source` as `<origin>#<need id>`), `--type <t>` (required,
     repeatable), `--status <s>` (optional, repeatable: only needs with one of these statuses are considered; the rest
     count as *not selected*), `--date YYYY-MM-DD` (required), `--version-key <v>` (default: the file's
     `current_version`), `--root`, `--write`, `--self-check` (as `adopt.py`). A missing required argument, a bad date,
     an empty `--origin`, or a value beginning with `-` where a value is expected is exit 2.
   - Reading: `versions[<version>].needs`, a mapping of need id → object. Not JSON, no `versions`, the requested
     version absent, or several versions and no `current_version` is exit 3 naming which. Keys the script does not
     use are ignored; `needs_amount` and the like are never trusted.
   - Per need, in file order: not in `--type` (or not in `--status`) → *not selected*, counted by type; `is_external`
     true → skip `external`; `content` absent, null or blank → skip `no content`; a line starting `..` → skip `rst
     directive`; a blank line inside the trimmed content → skip `multi-paragraph`; an inline `:role:` followed by a
     backtick → skip `rst role`; otherwise **drafted**, statement = the trimmed lines joined with one space. Check the
     skips in that order and name the first that applies.
   - Output and exit codes exactly as ADR 0027 §4–§7: one `draft REQ-NNNN  <need id>  <docname>:<lineno>` line per
     draft, one `skip <need id>  <reason>` line per skip, the `not carried` line (fields seen on the drafted needs and
     not carried; the count of drafted needs that had typed links, and the link field names), a line saying the source
     ids are ignored for the `claim_id` and kept as provenance, and last the reconciling line `needs N = drafted D +
     skipped S + not selected M` with M broken down by type. Dry run writes nothing.
   - The draft file is `adopt.py`'s template with `rationale.source: "<origin>#<need id>"`, the `--date`, and the header
     comment naming the export and `docname:lineno`. Nothing derived, no pin, empty `applicability` and `relations`.
   - A selected, draftable need whose `<origin>#<id>` is already some corpus claim's `rationale.source` is exit 1,
     nothing written (the report names the claims).
2. **Tests** (`tests/needs_import.rs`, new; drive the script and then the compiled binary, as `tests/adopt.rs` does; a
   missing `python3` is a named failure, never a skip). Fixtures are the two committed excerpts in
   `tests/fixtures/needs/` (read-only for this unit); a test that needs another shape writes a modified copy of an
   excerpt into a temporary directory and says what it changed.
   - (a) **S-CORE end to end**: `--type stkh_req --type feat_req --type comp_req --type aou_req --write`, then `req
     canon` exits 0, `req render` works; the draft count and each skip reason and count equal the values the test
     computes **independently** from the fixture JSON (an oracle written in the test, not copied from the script's
     output).
   - (b) **Demo end to end** with `--type req --type sysreq --type fsr --type swreq`; same oracle; `EX_REQ_001` and
     `GH_ISSUE_12` skipped with their reasons, `SYSREQ_BRAKE_01` drafted with its wrapped lines joined.
   - (c) **Dry run is the default**: no file is created; `--write` creates exactly the reported files; the claim ids are
     the next free `REQ-NNNN`, never derived from the need id.
   - (d) **Provenance**: each draft's `rationale.source` is `<origin>#<need id>` and a second `--write` over the same
     origin is exit 1 with nothing overwritten; the same file under a different `--origin` is allowed.
   - (e) **Reconciliation**: the last line adds up on both fixtures and on a copy with a need removed; not-selected is
     broken down by type (`document`, `mod_view_sta`, `arch`, …).
   - (f) **No default type**: no `--type` is exit 2; a `--type` absent from the file is exit 1 and lists the types
     the file has.
   - (g) **Skips are named**: `no content` for both the empty and the absent-key need (`stkh_req__dependability__firewall`),
     `multi-paragraph` (`aou_req__os__nonroot`), `rst directive` (`EX_REQ_001`), `rst role` (a modified copy),
     `external` (a modified copy with `is_external: true`). Each is counted; none is silent.
   - (h) **Equal text is not collapsed**: `comp_req__feo__act_alloc_error` and `feat_req__feo__act_alloc_error` are
     two drafts, and `req canon` shows them with the same address.
   - (i) **Not carried**: status, tags, safety, security, `reqtype` and links appear in no claim file; `relations` is
     `[]`; the report names the link fields and the count of drafted needs that had links.
   - (j) **`--status`** narrows to the named statuses and counts the rest as not selected; no status is written to a
     claim.
   - (k) **Dates**: no `--date` is exit 2, `2026-13-40` is exit 2, and the script never reads the clock or git for one.
   - (l) **UNVERIFIABLE**: not JSON, a JSON array, no `versions`, two versions and no `current_version`, an unknown
     `--version-key`, a corpus the loader rejects, a `req` reporting another claim schema: exit 3, nothing written.
   - (m) **Nothing derived**: the draft's bytes carry no `address`, `canonical`, parse slot or `pins`, and the strict
     loader accepts them (the oracle `tests/adopt.rs` uses).
   - (n) `--self-check` exits 0 with no document, no corpus and no `req`, and fails with another argument beside it.
   - (o) The corpus root rules of `adopt.py` (never an ancestor; `--root`, else `REQ_ROOT`, else the working directory).
3. **Docs**: `docs/adopting.md`, one short subsection after the `adopt.py` text, *From a Sphinx-Needs `needs.json`*:
   the command, the "name your types" rule and why, what is skipped and named (the five reasons), what is not
   carried and why, the reconciling line, and that the drafts are proposals. No S-CORE requirement text in it; the
   measured S-CORE yield is **not** stated in the page (it is one project's style). `AGENTS.md`: add
   `needs_import.py` beside `adopt.py` in the `tools/` entry of *Where things live*.
4. **Mutation checks** (envelope `mutations`):
   - A default `--type` list → (f) fails. Take the `claim_id` from the need id → (c) fails.
   - Draft a `multi-paragraph` need by taking its first paragraph → (g) and (a) fail. Draft an empty-content need → (g) fails.
   - Write on a dry run → (c) fails. Drop the already-drafted refusal → (d) fails.
   - Compute the reconciling line from the drafts alone (not selected omitted) → (e) fails. Invent a date → (k) fails.
   - Carry `status` into the claim → (i) and (j) fail. Collapse the two equal statements into one → (h) fails.
   - Trust `needs_amount` → a test with a modified count fails (add it to (e)).
5. **Allowed paths**: `tools/needs_import.py` (new), `tests/needs_import.rs` (new), `docs/adopting.md`, `AGENTS.md`.
   Forbidden: `tests/fixtures/**` (committed with this spec; a change to a fixture is a change to evidence), `tools/adopt.py`,
   `tests/adopt.rs`, `src/**`, `claims/**`, `baselines/**`, `bench/**`, `aliases/**`, `decisions/**`, `review/**`,
   `.forgejo/**`, `examples/**`, `scripts/**`, `Cargo.toml`, `Cargo.lock`, `README.md`, any canonicalizer / `RULES_V` /
   address change, any re-signing of a Baseline.

## Real-run evidence (the orchestrator, after merge; not the child)

Dry runs of the merged script on the **full** exports (S-CORE `score` `main`, the demo), with the type names a reader
of each project would choose, then `--write` into a scratch corpus and `req canon` over it. The table (needs, drafted,
each skip reason, not selected, exit code of `req canon`) goes in the pull request that closes #124, with the retrieval
date and the file's sha256. This is one project's authoring style measured once; it is reported as that.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, **read `$?` before committing**, exit 0, no SKIP,
Baseline `[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline or `bench/results`
change.
