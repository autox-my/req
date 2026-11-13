# 0051. A claim may name the area it belongs to

Extends ADR 0022 (`req.claim/2`: claims are authored, fingerprints are derived). It proposes one optional authored key.
No `RULES_V` bump and no address moves: the key is outside identity. The claim file format moves to `req.claim/3`
(decision 6, the owner's ruling of 2026-11-13).

**Status: proposed.** The owner ratifies it by merging; the first implementation (decisions 1–4 and 6) is
recorded in the note at the end. Asked for by the owner on
2026-11-13. Date proposed: 2026-11-13.

## Context

Requirements arrive in sections. A customer specification, a Sphinx-Needs project, a ReqIF export and a Markdown
requirements page each place a requirement under a heading or a hierarchy: interfaces, diagnostics, start-up. That
place is how a product owner finds a requirement and how a reviewer splits the work.

req keeps no such place:
- `claims/` is flat, and a file's name is its `claim_id`;
- `tools/needs_import.py` reports `tags` as not carried;
- `tools/adopt.py` uses a heading only to choose which lines to read, and does not record it;
- `req render` orders claims by id.

ADR 0050 had to group the graph by derivation **families**, the only grouping the corpus already holds. A family
follows `derives-from`, which is a design relation, not the section a reader expects.

How a specification places a requirement has properties a design must respect:
- **Section numbers move.** Inserting a section renumbers everything after it, and a requirement's visible number is
  often just its position under a heading. What stays fixed is the object identifier the source tool assigns.
- **Headings carry context.** A requirement written as a bare phrase, or as one item of a list whose lead-in sits in
  the object before it, means something only under its heading.
- **Placement is not the only classification.** Object type (requirement, information, heading), allocation to a
  discipline, safety level, a supplier's response and the variant all cut across sections, each with several values.
- **A source may hold no links between its requirements.** Trace links often run between levels (customer, system,
  software, test), not inside one specification. Derivation families then group nothing: every claim is standalone.

A grouping cannot be borrowed from anything req already has:
- **subject** names who a statement is about, for the fingerprint (33 of 40 claims here parse to `REQ_TOOL`);
- **`rationale.source`** is free text, so two spellings of one source would split a group;
- **relations** say what a claim rests on, not where it lives.

## Decision

1. **An optional key, `area`.** A claim may carry `area: <path>`. It is authored, optional, and absent by default. It
   is the one place a claim belongs in the reader's layout.
2. **One area per claim, as a path of stable keys.** A path is segments joined by `/` (for example
   `diagnostics/startup`); each segment is `[a-z0-9][a-z0-9-]*`.
   - A segment is a **key**, not a section number and not a heading: it must survive the source inserting or
     renumbering a section. An importer derives keys from the source's stable identifiers; a human chooses short
     words.
   - The reader-facing name of an area, in any language, is its label in the registry (decision 3), never the key.
   - The path gives the hierarchy: `diagnostics` holds `diagnostics/startup`.
   - A claim has exactly one area, so it has exactly one place, and a page, a section or a review split never shows it
     twice. Tags, where a claim can carry many, are rejected below.
3. **The areas are declared.** An optional registry, `.req/areas`, holds one line per area: `<path> <label>`. The order
   of its lines is the reading order.
   - A claim naming an area the registry does not declare is a **finding** that names the claim and the area, not a
     load error. Authoring stays friction-free (ADR 0006). Which command reports it is left to the implementation
     spec.
   - With no registry, any well-formed path is accepted, and the order is the path's sort order.
   - A malformed path is a load error, naming the claim and the key, the way every malformed key already is.
4. **Placement only, outside identity and outside the Baseline.** `area` is not in the canonical form and not in any attested hash.
   Moving a claim to another section moves no address and needs no re-signature: where a requirement is filed is not
   what it means.
   - Area never supplies meaning. A statement must stand alone; context its source left to a heading or a lead-in is
     written into the statement by the author or the importer, or the claim derives unstructured and says so.
   - The classifications that cut across sections are **not** areas. The variant is already applicability (ADR 0003);
     the others are not decided here.
5. **The views that group, group by it.**
   - The graph converter groups by area when any claim carries one, in place of families (ADR 0050 decision 2).
     Claims with no area form a `no area` group.
   - A grouped `req render` is a natural next step, but it moves committed projection bytes. It is **not** decided
     here.
6. **The format moves to `req.claim/3`, and `/2` keeps loading.** Owner's ruling, 2026-11-13.
   - `req.claim/3` is `req.claim/2` plus the optional `area`, and nothing else.
   - A `/3` build loads a `/2` file as it is, as a `/3` file with no area: no migration, no rewrite, and no address
     moves. This amends the rule that an older declaration is a migration candidate (`docs/claim-format.md`,
     *Version axes*) for `/2` alone. `req.claim/1` is still a migration candidate.
   - A `/2` file carrying `area` is a parse error naming the key, as an unknown key is today: `area` needs the `/3`
     declaration.
   - An older `/2` build reading a `/3` file reports a newer format (NOT JUDGED), the existing answer for a reader
     that is too old.
   - *Rejected:* accept `area` inside `/2`. Old builds would report a valid file as broken, with a parse error, not a
     version question.

## Consequences

- An adopter files requirements the way their specification already does, and the graph's pages follow those sections.
- An importer can carry its source's place. Each mapping is its own follow-up:
  - the ReqIF specification hierarchy;
  - a Sphinx-Needs field the adopter names;
  - an adopt.py heading.
- An importer has two jobs per requirement: carry its place as an area, and fold any heading or lead-in context
  into its statement. Doing only the first produces claims that read correctly in the graph and wrongly anywhere else.
- A corpus whose sources hold no links between requirements needs areas for the graph to group at all (ADR 0050).
- A wrong area is cheap to fix: it moves no address and needs no signature.
- Because `area` is unsigned, a reviewer cannot rely on a Baseline to show where a claim was filed when it was signed.
  That is accepted: placement is navigation, not meaning.

## Rejected

- **Tags (many per claim).** A claim with two tags has two places, so a grouped page or section shows it twice or must
  pick one. Tags may come later as a separate key for filtering; they do not give a layout.
- **Directories under `claims/`.** The file name is the `claim_id` and `claims/` is flat (`docs/claim-format.md`,
  *Layout*). Moving a file to re-file a claim would churn git history for a navigation change.
- **Reuse `subject`.** It is a Tier-1 input; filing would then move addresses.
- **Derive it from `rationale.source` or from relations.** Both are covered in *Context*: free text, and a design
  relation.
- **Put it in the canonical form.** Re-filing would then be a meaning change that needs re-ratification.

## Not decided here

- A grouped `req render`, and whether `docs/spec.md` would adopt it.
- Gate or `req check` policies per area, such as a strictness per section.
- Mapping each importer's hierarchy onto `area`.
- Tags as a filtering key, and where the cross-cutting classifications (object type, discipline, safety level, a
  supplier's response) live.

## Falsification

This decision is wrong if any of these is observed:

1. adding, changing or removing a claim's `area` moves its address or breaks a Baseline's verification;
2. a claim is shown in two groups of a view grouped by area;
3. a claim naming an undeclared area fails to load, when it should be reported as a finding;
4. a `/3` build refuses or rewrites a `/2` file, or a `/2` build reading a `/3` file reports anything other than a
   newer format;
5. a real adopter's sections cannot be expressed as one path of stable keys per requirement;
6. a claim's address, applicability or derived parse changes because of its area.

## Implementation note — 2026-11-13 (task A-1)

Decisions 1–4 and 6 are implemented (`docs/specs/A-1-area-key.md`); decision 5 (the graph) is A-2, after V-2.

- **Format.** `CLAIM_SCHEMA_V` is 3. The older formats a build loads as they are are data, `READS_AS_IS` in
  `src/canon.rs`, holding `/2`; the loader judges one `ClaimSchema::ReadAsIs` (no message, no rewrite). `/1` is
  still a migration candidate and `/4` a newer format. A `/2` file carrying `area` is a parse error naming the
  key and `req.claim/3`. This repository's own claims stay `/2`, which is the live proof of decision 6.
- **Where the finding lives (decision 3 left it to the implementation).** `req check`'s completeness step
  (ADR 0039) reports `undeclared_area`, naming the claim and the area. It is reported and does not fail, and
  becomes FAIL under `strict areas` in `.req/completeness`. With no `.req/areas` there is no finding; a malformed
  one is refused as `.req/completeness` is (exit 2, `<file>:<line>`, every bad line in one run), an unreadable
  one is the step's 3.
- **Registry grammar.** `<path> <label>`, the label the rest of the line in any language. Only a line that
  *starts* with `#` is a comment: unlike the other `.req/` lists a `#` inside a line is label text, because a
  label is free text and `C# support` must not be cut to `C` without a word. The path grammar is the claim key's,
  one function (`model::validate_area`), so the two cannot disagree.
- **Outside identity, checked.** `area` is in no canonical form and no attested hash; a Baseline emitted over this
  repository's corpus is byte-identical with and without an area on a claim, and so are the addresses
  (`tests/area.rs`). `RULES_V` did not move and `req selftest`'s known answers did not change.
- **Views.** `Row` carries `area`; every `req.*/1` document that serialises a row gains `area` after
  `superseded_by` (`null` when absent), and `req.trace/1`'s nodes do too. No text projection changes, so
  `docs/spec.md` and `docs/trace.md` pass `--check` unchanged. `req canon` prints the area at the end of a
  claim's line.
- **Importers.** `tools/adopt.py`, `tools/needs_import.py` and `tools/reqif_import.py` keep drafting
  `req.claim/2` (their drafts carry no area) and accept a `req` that reports `/2` or `/3`. Mapping a source's
  hierarchy onto `area` is still a later unit.
- **Migration message.** The `/1` migration candidate's message now names the schema `tools/migrate_claims.py`
  writes (`req.claim/2`) and says this build loads it as it is, since the tool stops at `/2`.

