# I-1: the ReqIF and Sphinx-Needs readers file their drafts under an area (ADR 0051 decision 2)

Task id: `I-1`. Base: the commit that adds this spec, on top of `01dc089`, where A-1 (PR #72), A-2 (PR #73) and the
filing of req's own claims (PR #74) are merged. Owner go-ahead of 2026-11-13: "importers mapping a source's sections
onto area, starting with ReqIF and Sphinx-Needs".

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `docs/adr/0051-a-claim-may-name-the-area-it-belongs-to.md`: decision 2 ("an importer derives keys from the
  source's stable identifiers"), decisions 3, 4 and 6, and *Falsification* 5;
- `docs/adr/0028-reqif-reader-drafts-claims-and-keeps-the-object-identifier.md` and
  `docs/adr/0027-needs-json-reader-drafts-with-the-need-id-in-provenance.md`: the readers refuse to guess, account
  for every object, and keep the three exit states;
- `docs/claim-format.md`, *Area*: the path grammar and the registry grammar.

Where this spec and an ADR disagree, the ADR wins; stop and report.

## Choices fixed here

1. **Opt-in, and nothing moves without it.** Both readers gain `--areas`. Without it every byte of today's output
   stays as it is: the drafts, the report and the exit code. The existing tests pass unchanged.
2. **The key rule, one rule in both readers.** A segment is made from a source identifier this way:
   - lowercase it;
   - replace each run of characters outside `[a-z0-9]` with one `-`;
   - strip `-` from both ends.
   - An identifier that leaves nothing is a problem (exit 1) naming the identifier.
   - Two different identifiers that give the same path are a problem (exit 1) naming both. They are never merged.
   - A heading's text and a section number are never a key: both move when the source is edited (ADR 0051 context).
3. **ReqIF: the area is the chain of ancestors.**
   - A drafted object's area is the keys of its ancestors in the specification hierarchy, outermost first, joined by
     `/`. Each key comes from the ancestor object's `IDENTIFIER`.
   - The specification itself is not a segment.
   - An ancestor is a segment whether or not it was selected or drafted. An object that is itself drafted and has
     children is filed under its own ancestors, and its children name it as a segment.
   - A drafted object with no ancestor (directly under a specification, or under no hierarchy node) gets no area.
   - A drafted object the hierarchy reaches more than once (two specifications, or two places in one) gets no area.
     The reader does not pick one: a claim has exactly one place.
   - A drafted object under a hierarchy node that names no object (no `SPEC-OBJECT-REF`) gets no area either, and an
     ancestor reference the file does not declare is exit 3 (amended in I-1-fix). Every ancestor chain of every drafted
     object is walked, each place of a several-places object included and filed or not, with the same strictness and the
     same depth cut: an undeclared ancestor is exit 3, an identifier that leaves no key and a collision are exit 1
     (choice 2), and only a node that names no object ends a chain without error (amended in I-1-fix-2).
   - `--area-depth N` (N ≥ 1) keeps only the outermost N segments. Default: all of them.
   - **Label.** `--area-label-attr NAME` names the attribute that holds a heading's text. The label of a path is that
     attribute's value on the path's last ancestor, as one line, when it has one plain value; otherwise the `--id-attr`
     value when given and present; otherwise the path's last segment. The name must match one definition, as
     `--text-attr` must (`check_unambiguous`).
4. **Sphinx-Needs: the area is the document.**
   - A drafted need's area is its `docname`, split on `/`, each part made a segment by choice 2.
   - A need with no `docname` gets no area.
   - `--area-depth N` as in choice 3.
   - **Label.** The `docname` as the export writes it, cut to the kept depth. Section titles are not used: they are
     not stable, and several of them share one document.
   - With `--areas`, `docname` is still listed as used, not as "not carried".
5. **The draft.**
   - A draft with an area is `schema: req.claim/3` with `area: <path>` on the line after `claim_id`
     (`docs/claim-format.md`, *Field reference*). Nothing else in the template changes.
   - A draft with no area stays `req.claim/2`, as today.
   - `--areas` needs a `req` that reports `claim_schema req.claim/3`. Any other value is exit 3, with today's message
     shape.
   - Area supplies no meaning (ADR 0051 decision 4): the statement is exactly what it is without `--areas`. No
     heading text is added to it.
6. **The registry.** The reader proposes lines for `.req/areas`; the human owns the file.
   - A dry run prints the lines it would add, each as `<path> <label>`.
   - `--write` appends the lines for paths `.req/areas` does not declare, in the order the source first shows them.
     It creates the file when there is none.
   - A path the registry already declares is left alone, whatever its label. The report counts these.
   - A label is one line and never empty. A registry that is malformed before the run is exit 3, naming what
     `req` names.
   - The loader check that runs today on the drafts also covers the registry the run would leave.
   - The write is all-or-nothing with the claims: if anything fails after the registry changed, its earlier bytes
     come back, or the file is removed when the run created it.
7. **The report.** With `--areas` the report gains lines; the reconcile line is unchanged and must still add up.
   - `areas: <P> paths; filed <F> of <D> drafted`
   - `no area: <K>` with the count per reason, only when K > 0. The reasons are `no ancestor`, `several places`
     and `unnamed ancestor` (ReqIF), and `no docname` (Sphinx-Needs) (`unnamed ancestor` added in I-1-fix).
   - `registry: <A> lines to add, <E> already declared`
   - ReqIF's `hierarchy` line ends `the ancestors of N drafted objects are carried as the area (--areas)` when N > 0
     drafts were filed (`1 drafted object` for N = 1), else `the structure is not carried` (amended in I-1-fix-2).
   - The header comment of a draft is unchanged.

## Tests first (envelope `red`)

Oracles come from ADR 0051, this spec and hand-written fixtures, never from a run. Fixtures are small files written
by hand under `tests/fixtures/`; reuse an existing one where it already has the shape.
- `tests/reqif_import.rs`:
  - **off by default:** a run without `--areas` on a fixture with a hierarchy is byte-identical to today's;
  - **two levels:** an object under heading `H-1` under heading `H-0` is drafted `/3` with `area: h-0/h-1`;
  - **a drafted parent:** a requirement with a child requirement is filed under its own ancestors, and the child's
    path ends in the parent's key;
  - **no ancestor** and **several places:** each draft is `/2` with no `area`, and the report counts it by reason;
  - **depth:** `--area-depth 1` gives `h-0`;
  - **key rule:** an identifier such as `_Sec.A 12` gives `sec-a-12`; two identifiers that collide are exit 1 naming
    both, and nothing is written;
  - **label:** from `--area-label-attr`, then `--id-attr`, then the segment; a non-ASCII label survives byte for
    byte;
  - **registry:** `--write` creates `.req/areas` in source order; a second fixture run against a registry that
    already declares one path leaves that line alone and appends the rest; a failure after the registry write
    restores it;
  - **loads:** `req canon` exits 0 on the corpus the run leaves, and `req check`'s completeness step reports no
    `undeclared_area`;
  - **older req:** a `req` reporting `req.claim/2` with `--areas` is exit 3.
- `tests/needs_import.rs`: the same list where it applies. `docname` `requirements/Diag_Startup` gives
  `requirements/diag-startup`; a need with no `docname` is `/2`; depth; collision; registry; loads; off by default.
- Each tool's own `--self-check` gains the key rule's cases.

Prove them red at the base.

## Mutation checks (envelope `mutations`)

- Use the heading text as the key → the two-levels test fails.
- File a several-places object under its first place → that test fails.
- Merge two colliding identifiers → the collision test fails.
- Overwrite an already declared registry line → the registry test fails.
- Write `area` into a `/2` draft → the loads test fails.
- Skip the registry rollback → the restore test fails.

## Verify

Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run. Do **not**
set `CARGO_TARGET_DIR`: two plan tests in `tests/baseline.rs` fail under it. `docs/spec.md`, `docs/trace.md` and the
three Baselines are unchanged and VERIFIED. Disk is limited: use one target directory and no extra copies.

## Docs, one home each

- Each tool's module docstring and `--help`: the flags, the key rule, the report lines.
- `docs/adopting.md`: the two "Not carried" paragraphs (the Sphinx-Needs one and the ReqIF one) say what `--areas`
  carries.
- ADR 0051: a dated implementation note. It replaces "Mapping a source's hierarchy onto `area` is still a later
  unit" for these two readers, and says `tools/adopt.py` is not covered.
- ADR 0027 and ADR 0028: one dated line each, pointing to that note.

## Allowed paths

- `tools/reqif_import.py`, `tools/needs_import.py`
- `tests/reqif_import.rs`, `tests/needs_import.rs`, and new fixtures under `tests/fixtures/`
- `docs/adopting.md` (the two paragraphs only)
- `docs/adr/0051-a-claim-may-name-the-area-it-belongs-to.md` (the note)
- `docs/adr/0027-needs-json-reader-drafts-with-the-need-id-in-provenance.md` and
  `docs/adr/0028-reqif-reader-drafts-claims-and-keeps-the-object-identifier.md` (one line each)

Forbidden: everything under `src/`, `tools/adopt.py`, `RULES_V`, `bench/**`, `claims/**`, `baselines/**`,
`decisions/**`, `aliases/**`, `.req/**`, `docs/spec.md`, `docs/trace.md`, `docs/claim-format.md`, `scripts/check.sh`,
`Cargo.toml`, `Cargo.lock`, `.github/**`. If a forbidden file must change, stop and report.
