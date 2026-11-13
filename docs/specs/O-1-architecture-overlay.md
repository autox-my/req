# O-1: the mention rule and the architecture overlay (ADR 0052 decisions 1–6)

Task id: `O-1`. Base: the commit that adds this spec, on top of `c151484`, where ADR 0052 (PR #76) is merged. The
owner ratified ADR 0052 on 2026-11-14 by merging it; decision 7 step 1 is this unit.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`: decisions 1–6 and 8, and its whole
  *Falsification* list;
- `docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning-and-a-change-of-meaning-is-judged-by-its-direction.md` (the
  file whose name starts `0040-`): the outcomes, and the S-2 note for `.req/design`, `.req/satisfied` and `req.satisfy/1`;
- `docs/adr/0046-the-trace-document-is-the-graph-contract.md`: decisions 5 and 7 (a deterministic, offline, standard
  library converter beside the importers; archify pinned at `7e45f36`, `ARCHIFY_UPDATE_CHECK_DISABLED=1`).

Where this spec and an ADR disagree, the ADR wins; stop and report.

## Part A: a marker with no claim id is a mention (ADR 0052 decision 6)

1. In `src/satisfy.rs`, a `req-satisfies:` whose token (the existing `tokens()` rule) is **empty** is a **mention**, not
   a declaration. A non-empty token that is not a claim the corpus holds stays UNKNOWN CLAIM, named as today.
2. `Scan` gains the mentions, as `(element, line)`, sorted and de-duplicated like the declarations. Nothing that reads
   declarations (`gate`'s `design_satisfied`, `completeness`'s `unsatisfied`, `audit`) sees a mention.
3. Reporting, so a mention is visible and never silent:
   - text `req satisfy`: after the links, one line per mention, `[MENTION] <element>:<line>: req-satisfies: with no claim
     id — not a link`, and the header counts them;
   - `req.satisfy/1` gains a top-level `mentions`: an array of `{"element": …, "line": …}`, `[]` when there are none,
     after `links` and before `unreadable`. Additive under `/1`.
   - A mention changes no verdict and no exit code.
4. `req satisfy --help` states the rule in one sentence.

## Part B: the overlay converter (ADR 0052 decisions 1–5)

`tools/graph/arch_overlay.py`, Python standard library only, offline, run as
`python3 -I tools/graph/arch_overlay.py [--root <dir>] <diagram.json> <satisfy.json> <trace.json> <out-dir>`.

1. **Inputs.**
   - `<diagram.json>`: an archify `architecture` file (`$schema`/`diagram` as archify writes it). Components without
     `sources` are kept and drawn as *no claim*.
   - `<satisfy.json>`: `req satisfy --format json` (`req.satisfy/1`).
   - `<trace.json>`: `req trace --format json` (`req.trace/1`), for the list of claims and their statements. The matrix
     rows are its `nodes` whose `applies` is not `false`.
   - A wrong schema, malformed JSON, or an existing non-empty `<out-dir>` is exit 2, naming what is wrong, as
     `trace_to_archify.py` refuses. It never writes outside `<out-dir>` and never opens an input for writing.
2. **The link of a component** is every `req.satisfy/1` link whose `element` equals one of the component's
   `sources[].path` exactly (both are repository-relative, `/`-separated). Line ranges are ignored (decision 1).
3. **Status** is the worst outcome, in the order `unknown_claim`, `orphaned`, `suspect`, `unratified`, `holds_weakened`,
   `holds` (decision 2). Two more states, which are not outcomes: `stale` (a cited path does not exist under the corpus
   root given by `--root`, default the current directory) and `no claim` (no link). `stale` ranks worst of all.
4. **Writes `<out-dir>/overlay.json`**, an archify `architecture` file:
   - every component and connection of the input, same ids, same order, same geometry fields;
   - the author's `label` unchanged. Status and claim ids go in archify's existing fields; choose which (`tag`,
     `sublabel`, `cards`) by reading archify's schema and its rendering rules in the scratch clone
     (`.scratch/graph/archify`, never copied into this repo), and state the choice and the reason in ADR 0052's note.
     If the input already uses a field you take, keep its text and add to it, never replace it;
   - one card listing, per state, the components in it; one card listing claims no component reaches; one card naming
     cited paths outside every `.req/design` root (the `req.satisfy/1` document holds no link for them; compute
     "outside" from `.req/design` under `--root`);
   - `meta.subtitle` (or the field archify offers) carries the diagram's `meta.repository.revision` when present, the
     `rules_v` of the satisfy document, and archify `7e45f36`, so the page never claims to be current.
5. **Writes `<out-dir>/matrix.md`**: a Markdown table, rows = claims (by claim id), columns = components (in diagram
   order, by label), cell = the outcome word or empty. Then the list of claims no component reaches. Deterministic.
6. **Exit status:** 0 written; 3 written, but at least one component is `stale` or one link is `unknown_claim` (the page
   is still written, and the reason is printed); 2 refused.
7. **Deterministic:** two runs over the same inputs give byte-identical files. Sorted keys where order is not the
   diagram's, no timestamp, `ensure_ascii=False`, a trailing newline.

## Tests first (envelope `red`)

Oracles come from ADR 0052, ADR 0040 and hand-written fixtures, never from a run.
- `tests/satisfy.rs`, Part A, over a temp corpus:
  - a file holding `req-satisfies:` with nothing after it, one holding `` `req-satisfies: <claim-id>` ``, and one
    holding `req-satisfies: REQ-0001` give exactly one link and two mentions (text and JSON);
  - `req-satisfies: REQ-12x` (a non-empty token that is no claim) is still UNKNOWN CLAIM;
  - mentions change neither the verdict nor the exit code; `gate`'s `design_satisfied` ignores them;
  - `mentions` sits between `links` and `unreadable`.
- `tests/graph_overlay.rs`, Part B, in `tests/graph_archify.rs`'s style (the script run as an adopter runs it), with
  diagrams and documents written by hand:
  - a component citing two files whose links are `holds` and `suspect` is `suspect` (*Falsification* 3);
  - each state of the order in choice 3 wins over the next;
  - a component with no `sources`, and one whose files declare nothing, are `no claim`;
  - renaming a component id with the same `sources` gives the same claims and status (*Falsification* 4);
  - a cited path that does not exist is `stale`, the page is written, exit 3 (*Falsification* 5);
  - a path outside the design roots is named in its card;
  - geometry, ids, connections and labels are unchanged from the input; existing `tag`/`sublabel` text is kept;
  - the matrix lists a claim no component reaches;
  - the inputs are byte-identical after the run, and nothing is written outside `<out-dir>` (*Falsification* 2);
  - determinism: two runs give byte-identical `<out-dir>`s (*Falsification* 1);
  - each refusal of choice B1 is exit 2.

Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Make an empty token a declaration again → the Part A mention test fails.
- Make a malformed non-empty token a mention → the `REQ-12x` test fails (*Falsification* 6).
- Pick the best outcome instead of the worst → the `holds`+`suspect` test fails.
- Key a component's links on its id instead of its `sources` → the rename test fails.
- Skip the existence check → the stale test fails.
- Replace an existing `tag` instead of adding to it → the keep-text test fails.

## Verification outside the gate (envelope; not in `check.sh`)

- On a scratch copy of this repository with `.req/design` listing `src`, `req satisfy` now reports the four marker
  mentions (`src/audit.rs`, `src/completeness.rs`, `src/main.rs`, `src/satisfy.rs`) as mentions and no UNKNOWN CLAIM.
- Hand-write a small archify architecture diagram (about 6 components citing files of that copy, two of them carrying a
  `req-satisfies:` line you added on the copy and recorded with `--record`, one changed afterwards so it is suspect).
  Run the converter, then archify `7e45f36` `validate` and `finalize` with `ARCHIFY_UPDATE_CHECK_DISABLED=1`. Report the
  gates. Keep every file in `.scratch/` and commit none of it.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
`docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and VERIFIED. Disk is limited: use one target
directory and no extra copies; do not set `CARGO_TARGET_DIR`.

## Docs, one home each

- `req satisfy --help`: the mention rule (Part A 4).
- `examples/graph/README.md`: a short section, *Architecture overlay*: the command line, the three inputs, the exit
  codes, the archify invocation.
- ADR 0052: **Status** becomes accepted (ratified by the owner's merge of PR #76, 2026-11-14), and a dated
  implementation note: the module, the fields chosen and why, the exit codes.
- ADR 0040: one line in the S-2 note, saying a bare marker is now a mention (ADR 0052 decision 6).
- `CONTEXT.md`: **Mention**, a `req-satisfies:` with no claim id, counted and never a link.

## Allowed paths

- `src/satisfy.rs`, `src/main.rs` (the satisfy help page only)
- `tools/graph/arch_overlay.py` (new)
- `tests/satisfy.rs`, `tests/graph_overlay.rs` (new), `tests/json_format.rs` (only if it pins `req.satisfy/1`'s keys),
  and new fixtures under `tests/fixtures/`
- `examples/graph/README.md`
- `docs/adr/0052-an-architecture-diagram-shows-the-claims-its-components-satisfy.md`
- the ADR 0040 file (the one line)
- `CONTEXT.md` (the one term)

Forbidden: `RULES_V` and any change to the canonical form or an attested hash, `src/canon.rs`, `src/model.rs`,
`src/baseline.rs`, `src/gate.rs`, `src/completeness.rs`, `src/audit.rs`, `bench/**`, `claims/**`, `baselines/**`,
`decisions/**`, `aliases/**`, `.req/**`, `docs/spec.md`, `docs/trace.md`, `scripts/check.sh`, `Cargo.toml`,
`Cargo.lock`, `.github/**`, `tools/graph/trace_to_archify.py`, and any archify file. If a forbidden file must change,
stop and report.
