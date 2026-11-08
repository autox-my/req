# #135 — a ReqIF reader

Task id: `135-1` (one unit). Code baseline: current `main`. Issue #135; the decision is **[ADR
0028](../adr/0028-reqif-reader-drafts-claims-and-keeps-the-object-identifier.md)** (proposed; dispatched only
after it merges). Read the ADR first: the contract is there and is not restated here. Read `tools/needs_import.py`,
`tests/needs_import.rs`, `tools/adopt.py` and the *From a Sphinx-Needs `needs.json`* subsection of
`docs/adopting.md`: this unit is `needs_import.py`'s sibling and follows it wherever the ADR is silent (argument
errors, corpus root rules, `--self-check`, id allocation, registry check, `req --version` schema check, claim
template, yaml quoting, report layout).

**Model:** `claude-sonnet-5-5` (high). Reviewed read-only on a different family (`135-1-review-…`): it decides what
becomes a draft and what is named as skipped.

## Why

F8: adoption should not mean retyping. ReqIF is what DOORS, Polarion, Codebeamer, Jama and ProR export and what
OEMs send suppliers. The evidence is **three tiny public files** (`tests/fixtures/reqif/README.md`): the unit
proves the reader's *structure* and fails loud on what it cannot honestly flatten; it proves nothing about
yield on a real specification, and no doc or report it writes may say otherwise.

## Unit 135-1 — `tools/reqif_import.py`

1. **The script** (new, python3 stdlib only, standalone: copy what it needs from `needs_import.py`; do **not**
   import a sibling and do **not** edit one).
   - Arguments: `--from <file.reqif>` (required; relative to the corpus root), `--list`, `--origin <url-or-path>`
     (required unless `--list`; non-empty), `--type <t>` (repeatable), `--where NAME=VALUE` (repeatable; split at the
     **first** `=`; an empty NAME is exit 2), `--text-attr NAME` (required unless `--list`), `--id-attr NAME`
     (optional), `--date YYYY-MM-DD` (required unless `--list`), `--root`, `--write`, `--self-check`. Without
     `--list`, neither `--type` nor `--where` is exit 2. `--list` with `--write` or any drafting argument is exit 2.
     A bad date, an empty `--origin`, or a value beginning with `-` where a value is expected is exit 2.
   - Reading, exit 3 and the message names which: every case in ADR 0028 §2. Parse with `xml.etree.ElementTree`
     on **bytes**; refuse a DOCTYPE or entity declaration **before** parsing (scan the bytes), so no entity is ever
     expanded. Elements are matched by the namespace-qualified name, never by prefix.
   - `--list` output, ADR 0028 §3: a `reqif:` header line (tool id, creation time, ReqIF version, title), one
     `specification <LONG-NAME or (unnamed)>  objects <n>` line per specification (objects reachable through its
     hierarchy), then per object type `type <LONG-NAME or (unnamed)>  <IDENTIFIER>  objects <n>` and an indented
     line per attribute `<name>  <declared kind>  <n> with a value`. `(unnamed)` is display only and is **not** a
     selector.
   - Per object, in **file order** (the order of `SPEC-OBJECT` elements): not selected by `--type`/`--where` → *not
     selected*, counted by type (name, else identifier); otherwise the text attribute's value by ADR 0028 §5, the
     skip order `no text`, `multi-paragraph`, `rich markup (<element>)`; otherwise **drafted**.
   - Output: one `draft REQ-NNNN  <IDENTIFIER>  <id-attr value, or - if none>` line per draft; one `skip
     <IDENTIFIER>  <reason>` line per skip; a `not carried` line (every attribute other than the text attribute,
     with the count of drafted objects carrying it); a `hierarchy` line (the count of drafted objects that sit under
     a parent, and the specification names); a `relations` line (the file's `SPEC-RELATION` count, the drafted
     objects that are the source or target of one, and the relation types' names); a line saying the object
     `IDENTIFIER`s are kept as provenance and are not the `claim_id`; last the reconciling line `objects N = drafted D
     + skipped S + not selected M (by type: …)`. Dry run writes nothing.
   - The draft file is `needs_import.py`'s template with `rationale.source: "<origin>#<IDENTIFIER>"`, the
     `--date`, and a header comment naming the file, the `IDENTIFIER` and the `--id-attr` value. Nothing derived, no
     pin, empty `applicability` and `relations`. A selected, draftable object whose `<origin>#<IDENTIFIER>` is already a
     corpus claim's `rationale.source` is exit 1, nothing written (the report names the claims).
2. **Tests** (`tests/reqif_import.rs`, new; drive the script and then the compiled binary as `tests/needs_import.rs`
   does; a missing `python3` is a named failure, never a skip). The three public files in `tests/fixtures/reqif/` are
   read-only for this unit and are what the expected values below were read from (they are facts about the pinned
   bytes, written into the tests as literals, **not** computed by the script). A shape the files lack is a
   **synthetic** ReqIF the test writes into a temporary directory with a comment saying so; no synthetic file is
   ever described as an export of a tool.
   - (a) **Polarion**: `--type "System Requirement" --text-attr ReqIF.Text --id-attr ReqIF.ForeignID --write` drafts
     exactly one claim whose statement is that object's text (the literal lives in the test, not in a doc),
     `rationale.source` is `<origin>#rmf-1d312f18-fa7c-4de3-b8f3-ab105179fddf`,
     the report shows id `LOREM-818`, the reconciling line is `objects 2 = drafted 1 + skipped 0 + not selected 1`
     with `Heading` in the breakdown, the hierarchy line says one drafted object sits under a parent; `req canon`
     exits 0 and `req render` works. `--type Heading --type "System Requirement"` drafts two (the heading's plain
     `STRING` value is taken: the kind comes from the value element).
   - (b) **Enumeration**: `--where Status=Draft` selects that object and `--where Status=Approved` is exit 1 with
     nothing written, the message listing the `Status` values it saw (`Approved`, `Draft`, `Rejected`, `Reviewed` are
     the file's enumeration; the ones carried by objects are named).
   - (c) **DOORS, unnamed types**: `--list` prints 14 object types, all `(unnamed)`, and the one specification; `--type
     "(unnamed)"` is exit 1 (not a selector). `--type _7f123ed4-98dd-4eed-b96a-edc8828963a8 --text-attr
     ReqIF.ChapterName --id-attr ReqIF.ForeignID` drafts three claims, in file order, with ids 1, 2, 3 and sources
     `<origin>#_1_01ea51ea-ed54-471e-8c08-19e093f930f7`, `…#_2_7e1c200d-5c01-4b93-941d-95fadf8feb0c`,
     `…#_3_a4285dbc-8259-4577-a093-ffd735084332` (an integer id attribute is reported as its decimal text). With
     `--text-attr ReqIF.Text` the same selection skips all three `no text` and is exit 1 (nothing extracted).
   - (d) **RMF**: `--list` prints one type `TC18xx SpecObjectType` with six objects and two specifications;
     `--type "TC18xx SpecObjectType" --text-attr ReqIF.Name` drafts six (the `<p>` wrapper); `--text-attr ReqIF.Text`
     is exit 1 naming that the attribute is defined on none of the selected objects' types.
   - (e) **Dry run is the default**: no file is created; `--write` creates exactly the reported files; claim ids are the
     next free `REQ-NNNN`, never derived from an identifier or a human id.
   - (f) **Provenance**: a second `--write` over the same origin is exit 1 with nothing overwritten; the same file under
     another `--origin` is allowed.
   - (g) **Reconciliation**: the last line adds up on all three files and on synthetic files with an object added;
     not-selected is broken down by type; **N counts every `SPEC-OBJECT`**, including one under no hierarchy node.
   - (h) **No default selection, no default text attribute**: neither `--type` nor `--where` is exit 2; no `--text-attr`
     is exit 2; a `--type` matching nothing is exit 1 listing the types the file has.
   - (i) **Skips are named** (synthetic): a missing text value and a blank one → `no text`; two `div`s, and a plain string with
     a blank line → `multi-paragraph`; a `br`, a `span`, a `sub`, a `ul`, a `table`, an `object` inside the wrapper →
     `rich markup (<element>)` naming the element; a wrapper that is not `div`/`p`. Each is counted; none is silent.
     A `sub` is **never** flattened (`m<sub>2</sub>` is not drafted as `m2`).
   - (j) **Plain-paragraph joining**: a wrapped plain string and a wrapped XHTML text are drafted with their whitespace
     collapsed to single spaces and trimmed (XML entities decoded; an `&amp;` is a `&` in the statement).
   - (k) **Where**: `--where` is ANDed (two clauses narrow; a `--type` and a `--where` both must hold); an enumeration
     object with two values matches either; a `--where` name matching two definitions on one type is exit 3; an
     integer attribute compares by its decimal text.
   - (l) **Not carried**: no attribute value, enumeration, hierarchy or relation appears in any claim file;
     `relations` is `[]`; the report names the attributes and counts them. Synthetic with two `SPEC-RELATION`s: the
     `relations` line counts them and the drafted objects they touch, names their types, and no claim has a relation.
   - (m) **UNVERIFIABLE**, each exit 3 with nothing written: not XML; a DOCTYPE; an entity declaration (and the
     test asserts no entity was expanded: a file whose entity would add a marker string to the output); a root that is not
     `REQ-IF`; the wrong namespace; a `.reqifz` path; an object with no `IDENTIFIER`; two objects with the same
     `IDENTIFIER`; an attribute value whose definition reference is dangling; a dangling `ENUM-VALUE-REF`; a corpus the
     loader rejects; a `req` reporting another claim schema. **A repeated `IDENTIFIER` on non-objects is accepted**
     (the Polarion file has one: assert it does not fail).
   - (n) **Dates**: no `--date` is exit 2, `2026-13-40` is exit 2, and the script never reads the clock, git or the
     header `CREATION-TIME` for one (assert the drafts carry the `--date`, with a header time that differs).
   - (o) **Nothing derived**: the draft's bytes carry no `address`, `canonical`, parse slot or `pins`, and the strict
     loader accepts them (the oracle `tests/needs_import.rs` uses).
   - (p) `--self-check` exits 0 with no file, corpus or `req`, and fails with another argument beside it; `--list` needs no
     corpus.
   - (q) The corpus root rules of the siblings (never an ancestor; `--root`, else `REQ_ROOT`, else the working directory).
   - (r) **Equal text is not collapsed**: two synthetic objects with the same text are two drafts; `req canon` shows the
     same address.
3. **Docs**: `docs/adopting.md`, one short subsection after the Sphinx-Needs one, *From a ReqIF file (DOORS, Polarion, …)*:
   run `--list` first; name your selection and text attribute and why (a DOORS type has no name; the text column
   varies); what is skipped and named (the three reasons), what is not carried (other attributes, hierarchy,
   relations) and why; the reconciling line; that the drafts are proposals; that a `.reqifz` must be unzipped; and
   that **the yield on a real specification is unmeasured** (the reader was built on three small public files). No PDF
   instruction beyond one sentence that a PDF is the rendering of data ReqIF holds with its structure. No text from
   any fixture in the page. `AGENTS.md`: add `reqif_import.py` beside `needs_import.py` in the `tools/` entry of
   *Where things live*.
4. **Mutation checks** (envelope `mutations`):
   - Read the kind from the declaration, not the value → (a) fails (the heading's `STRING`).
   - A default `--text-attr ReqIF.Text` → (h) and (d) fail. A default selection → (h) fails.
   - Treat `(unnamed)` as a selector → (c) fails. Match `--type` by name only → (c) fails.
   - Flatten a `sub`/`span`/`br` (take its text) → (i) fails. Take the first of two `div`s → (i) fails.
   - Parse before the DOCTYPE refusal → (m) fails. Require uniqueness of every `IDENTIFIER`, not just objects' → (m) fails
     on the Polarion file.
   - Take the `claim_id` from the identifier or the human id → (e) fails. Drop the already-drafted refusal → (f) fails.
   - Compute N from the drafts and skips only → (g) fails. Use the header `CREATION-TIME` as the date → (n) fails.
   - Carry an attribute (e.g. `Status`) into the claim → (l) fails. Write on a dry run → (e) fails.
   - Collapse two equal statements into one → (r) fails.
5. **Allowed paths**: `tools/reqif_import.py` (new), `tests/reqif_import.rs` (new), `docs/adopting.md`, `AGENTS.md`.
   Forbidden: `tests/fixtures/**` (committed with this spec; a change to a fixture is a change to evidence),
   `tools/adopt.py`, `tools/needs_import.py`, `tests/adopt.rs`, `tests/needs_import.rs`, `src/**`, `claims/**`,
   `baselines/**`, `bench/**`, `aliases/**`, `decisions/**`, `review/**`, `.forgejo/**`, `examples/**`, `scripts/**`,
   `Cargo.toml`, `Cargo.lock`, `README.md`, any canonicalizer / `RULES_V` / address change, any re-signing of a
   Baseline, any new dependency.

## Real-run evidence (the orchestrator, after merge; not the child)

None exists to run on: the public files are the fixtures. The merged script is run once on each of the three, `--list`
and a drafting run, and the numbers recorded on #135 **as structural checks on tiny files**. The first adopter's real
ReqIF export is the measurement; until one is read, the issue stays open for it and no yield is claimed anywhere.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, **read `$?` before committing**, exit 0, no SKIP,
Baseline `[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline or
`bench/results` change.
