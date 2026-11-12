# G-1 — PURE as a fourth source for the direction gold set (direction-v4)

Task id: `G-1`. Base: the commit that adds this spec. Owner go-ahead of 2026-11-12: add PURE to the direction gold set,
taking statements from all its PDFs through `pdftotext`. The aim is enough public numeric statements to measure
`numeric_loosening`'s false-carry support (9 at v2). That support decides whether ADR 0040 may admit the class.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

## The source (read at named provenance, AGENTS.md)

- **PURE**: A. Ferrari, G. O. Spagnolo, S. Gnesi, "PURE: A Dataset of Public Requirements Documents", RE 2017.
  - Zenodo record 1414117, DOI `10.5281/zenodo.1414117`, version 1.0, licence **CC BY 4.0** (the record's `license` field).
  - The PDFs are in `requirements.zip`: 33,623,628 bytes, md5 `bc319fe28619f6290badff328ca159dd`. The generator also
    records the archive's sha256.
- **Attribution.** CC BY needs it, so the corpus header names the authors, the title, the DOI and the licence, and so does
  `docs/canonical-hashing.md` §6.1.
- **Text extraction.** `pdftotext` (poppler) **26.11.0**, the version on the owner's machine on 2026-11-12. It is
  generator provenance, as the OCR pin is review provenance. The generator refuses to run with any other version, naming
  both versions, and the header records the version. The core never runs it, and `scripts/check.sh` never regenerates.
- **What was measured before this spec** (a regex count, not req's parser; for scale only):
  - 62 PDFs extract text, and 41 of them contain "shall";
  - 4,280 "shall" sentences of 300 characters or fewer;
  - **124** of those carry a comparator phrase followed by a number;
  - 9 carry "one of";
  - the numeric ones are concentrated: 47 come from `2007 - nde`, 20 from `1999 - tcs`, 13 from `2004 - rlcs`, and 11
    each from the two EIRENE documents.

  PURE cannot move `membership_widening` (support 0): say so, and do not synthesise enumerations.

## What to do

1. **Tests first** (envelope `red`), where they belong: the generator's reading is checked by regeneration, and the
   corpus by `req bench`.
   - A Python test of the PURE sentence filter and the numeric-mutation labelling, run from `tests/` the way existing
     tests drive `tools/` (if none does, a Rust test that runs `python3 -I tools/corpus/build_direction.py --self-test`).
     Oracles from English:
     - "within 180 days" → upper bound;
     - "for at least 24 hours" → lower bound;
     - "no more than 4 hours" → upper (R-1's longest match);
     - "shall not be interrupted for more than 2 hours" → a native prohibition, whose numeric mutation is labelled with
       the swapped sense (ADR 0044 decision 5).
   - `tests/baseline.rs`'s plan list and the `check.sh` fixtures gain the v4 bench step, as S-1 did for v2.
2. **The generator** (`tools/corpus/build_direction.py --version 4`):
   - v4 = every v3 pair, plus pairs built from PURE statements by the **existing** families and mutations (numeric
     loosening and tightening with mirrors, modality, compound, prohibition). Labels come from the mutation as today,
     never from req.
   - The PURE pool is every sentence that contains "shall", is 300 characters or fewer after whitespace normalisation,
     and carries a comparator phrase with a number. Include native "shall not" sentences with one bound in the
     prohibition family, labelled per ADR 0044 decision 5.
   - Sentences are taken in a deterministic order (document file name, then position).
   - Drop extraction noise by named rules: a replacement character, a page-header or footer fragment, or a sentence
     broken by a hyphenated line end that cannot be rejoined. The header counts each rule's drops. Never hand-edit a
     sentence.
   - Ids are `D-PU-<doc-slug>-<n>…`, and `source:` is `pure@zenodo-1414117 <file> #<n>`.
   - **v1, v2 and v3 regenerate byte-identically** (show it).
3. **The record.**
   - Record `bench/results/direction-v4-rules-v9.json` and add its bench to the full `scripts/check.sh` beside v1–v3.
   - Report the v4 numbers: FMR and FSR, dangerous confusions (must stay 0, a hard stop), accuracy, the per-family counts,
     and the `false_carry` block.
   - Report **`numeric_loosening`'s count and support on v4**, and how many PURE statements req's parser reads with a
     value slot. If the parser reads too few for the regex count to matter, say so plainly.
4. **Admission is not changed here.** `ADMITTED_CARRIES` stays pinned to `direction-v2-rules-v<RULES_V>.json` (S-2 choice
   6). Moving the measurement to v4, and admitting any class it supports, is an owner decision for a later unit.
   - If v4 shows `numeric_loosening` with count 0 and support ≥ 100, stop with status `done` and say so prominently.
   - If v4 shows a false carry on `modality_weakening`, the admitted class, **stop with status `blocked`**: that is
     evidence against an admitted carry.
5. **Docs, one home each.**
   - `docs/canonical-hashing.md` §6.1: the PURE source, its attribution and the pdftotext pin.
   - `docs/canonical-hashing.md` *Recorded runs*: the v4 numbers.
   - ADR 0040: one dated implementation-note line with v4's false-carry numbers.
   - ADR 0044: one dated line if the native prohibition family now measures decision 5 on native `shall not` statements
     (ADR 0044's note says the v3 bases were all flipped SHALLs).

## Mutation checks (envelope `mutations`)

- Label a PURE numeric mutation without the prohibition swap → the generator test fails and the regenerated v4 differs.
- Drop the pdftotext version check → the generator test that runs it with a fake version fails.
- Feed v1–v3 through the PURE path → their regeneration is no longer byte-identical.

## Verify

Full `scripts/check.sh` (HOME=<main checkout>/.tools/home): exit 0, no SKIP, tree clean after the run. v1–v3 corpora and
every existing record are byte-identical.

## Allowed paths

`tools/corpus/build_direction.py`, `bench/corpus/direction-v4.yml` (new), `bench/results/direction-v4-rules-v9.json`
(new), `scripts/check.sh` (the v4 bench step only), `tests/**`, `docs/canonical-hashing.md` (§6.1 and *Recorded runs*),
`docs/adr/0040-a-design-satisfies-a-claim-at-a-meaning.md` and `docs/adr/0044-…` (one line each).

Forbidden: `src/**`, `RULES_V`, `bench/corpus/direction-v{1,2,3}.yml`, every existing `bench/results/*` record,
`claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`, `Cargo.toml`, `Cargo.lock`, `.github/**`. Never
commit the downloaded archive or the extracted text: they belong in a scratch directory outside the repository, or under
the gitignored `.scratch/`.
