# 0051. A claim may name the area it belongs to

Extends ADR 0022 (`req.claim/2`: claims are authored, fingerprints are derived). It proposes one optional authored key.
No `RULES_V` bump and no address moves: the key is outside identity. Some loader rules change, and decision 6 is a
claim-schema question the owner rules on.

**Status: proposed.** The owner ratifies it by merging; no code is attached yet. Asked for by the owner on
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
6. **The format version.** `req.claim/2` refuses an unknown key, so an older build reading a file that carries
   `area` reports a parse error, not a newer-format question. This decision proposes **`req.claim/3`** as `/2` plus
   the optional `area`, and proposes that a `/3` build go on loading `/2` files as they are, as `/3` files with no
   area, with no migration. That is a change to the rule that an older declaration is a migration candidate
   (`docs/claim-format.md`, *Version axes*), and the owner rules on it. The alternative is to accept `area` inside
   `/2` and live with old builds misreporting it.

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
4. after the format decision, a file carrying `area` is read by an older build without that build naming the cause;
5. a real adopter's sections cannot be expressed as one path of stable keys per requirement;
6. a claim's address, applicability or derived parse changes because of its area.
