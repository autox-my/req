# 0028. A ReqIF reader drafts claims and keeps the object IDENTIFIER as provenance

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-08, former Forgejo PR #136); issue #135, the third
part of the architecture review's finding F8 (after `req diff`, ADR 0026, and the Sphinx-Needs reader, ADR
0027). Date proposed: 2026-11-08. **Adds a copy-and-adjust script; adds no `req` command and changes no
address.** It is the ReqIF counterpart of `tools/needs_import.py`, built the same way.

## Context

ReqIF (OMG, XML) is the exchange format between requirements tools: IBM DOORS, Polarion, Codebeamer, Jama and
Eclipse ProR all read and write it, and customers in this niche receive requirement sets from OEMs as ReqIF or
as a PDF rendering of one. A team whose requirements live in such a tool can already produce the file; asking
it to retype them into claims is the adoption barrier F8 describes. ADR 0017's rule applies unchanged: the
external text is a *source*, the claim is the obligation, an import yields unratified drafts.

**The evidence is public samples only, and it is thin.** Three public files were read before this was written
(retrieved 2026-11-08, fixtures README for provenance and the licence caveat): a DOORS export, a Polarion export
and an Eclipse RMF export. Together they hold 11 objects, **one** of which is a requirement with body text, and
no relation, table, list or multi-paragraph body. What they show, and what the reader must survive:

- **DOORS object types carry no name** (`LONG-NAME` absent; only an opaque `IDENTIFIER`). Selecting objects by
  type name is impossible there; Polarion and RMF do name their types.
- **The text attribute is not at a fixed place.** `ReqIF.Text` is the usual name (DOORS, Polarion) but RMF has
  none and uses `ReqIF.Name`; a DOORS module that was exported with its own column names uses those. In the
  DOORS sample the three objects are chapter headings (`ReqIF.ChapterName`) with no `ReqIF.Text` at all.
- **The value's kind is on the value, not the declaration**: a Polarion `ReqIF.Text` is declared XHTML but one
  object's value is a plain `STRING`.
- **The human id differs in kind**: `ReqIF.ForeignID` is an integer in DOORS (the absolute number) and a string
  (`LOREM-818` style) in Polarion; RMF has none. The `IDENTIFIER` is a generated string (`_<n>_<guid>` in DOORS,
  a UUID in Polarion).
- **`IDENTIFIER`s are not unique across the file**: the Polarion sample repeats one on two `SPEC-HIERARCHY`
  nodes (never on objects). Uniqueness can only be required of the objects.
- **Status-like data is an enumeration** whose value is a reference to an `ENUM-VALUE` with a `LONG-NAME`
  (`Draft`, `Approved`); selecting on it needs the reference resolved.
- Rich text is XHTML in the ReqIF namespace's own prefix (`reqif-xhtml:div` in DOORS). A real requirement body
  may hold paragraphs, lists, tables, `<sub>`/`<sup>`, images or embedded OLE objects; **none of that is in the
  public samples.**

The PDF renderings of DOORS-style modules the maintainer holds (confidential, not committed, never quoted)
print each object as an `ID: …` / `Object Type: Requirement` block: an id text and an object-type attribute.
That is why selection by *attribute value* is part of the design, though no public file exercises it beyond
the Polarion `Status`.

## Decision

1. **A script, not a subcommand.** `tools/reqif_import.py`, python3 standard library only (`xml.etree`), a
   sibling of `tools/adopt.py` and `tools/needs_import.py` that adopters copy and adjust. `req` gains nothing. It
   writes the same legal `req.claim/2` drafts the siblings write: statement, provenance, empty `applicability` and
   `relations`, nothing derived, no pin.
2. **Reading is strict and local.** Only a `.reqif` file (XML) is read. These are **UNVERIFIABLE** (exit 3), each
   named: not well-formed XML; any `<!DOCTYPE` or entity declaration (a ReqIF file needs neither, and a
   declared entity is how an XML file becomes a bomb or a file read); a root other than `REQ-IF` in the ReqIF
   `20110401` namespace; a `.reqifz` (a zip: unzip it first, the script opens no archive); a `SPEC-OBJECT`
   `IDENTIFIER` that is missing or occurs twice among objects (it is the join key); an attribute value whose
   definition or an enumeration reference does not resolve; a `--where`/`--text-attr`/`--id-attr` name matching
   two attribute definitions on one object type. The script never fetches anything and never writes outside the
   corpus.
3. **`--list` shows what is there, and writes nothing.** With only `--from` (and optionally `--root`), it prints
   the header (tool id, creation time, title, ReqIF version), each specification and its object count, and per
   object type: its `LONG-NAME` (or `(unnamed)`), its `IDENTIFIER`, the object count, and each attribute with
   the kind the declaration gives and how many objects of that type carry a value. It exists because a DOORS
   type has no name to guess; exit 0, or 3 as §2.
4. **No default selection and no default text attribute.** The caller names what to read:
   `--type <t>` (repeatable; matches an object type's `LONG-NAME`, else its `IDENTIFIER`; any one matches),
   `--where NAME=VALUE` (repeatable; all must hold; `NAME` an attribute's `LONG-NAME`; an enumeration compares
   the resolved `LONG-NAME`s, any one matching; other kinds compare the value's text, an XHTML value after the §5
   flattening), and **`--text-attr NAME`** (required: the attribute whose value is the statement). At least one of
   `--type`/`--where` is required (exit 2): a built-in selection would be a heuristic about someone else's
   vocabulary. Objects not selected are **counted by type** (name, else identifier), never silently ignored.
5. **The statement is the text attribute only when it is one plain paragraph.** The kind is read from the
   *value element*, not the declaration. A plain string: its lines are trimmed and joined with one space (the
   siblings' rule). An XHTML value: its one wrapper element, `div` or `p` in the XHTML namespace, whose content is
   **text only**, with whitespace collapsed and trimmed. Anything else is **skipped and named with its reason**,
   checked in this order: `no text` (the attribute absent, or blank), `multi-paragraph` (a blank line in a plain
   string; more than one block element in XHTML), `rich markup` (any element inside the wrapper, or a wrapper
   that is not `div`/`p`, the first one named: `br`, `span`, `sub`, `ul`, `table`, `object`…). The reader never
   strips markup to make an object fit: `m<sub>2</sub>` flattened to `m2` is a changed meaning, which is the
   failure this project exists to prevent. If real exports turn out to be mostly `rich markup`, the answer is
   a measured, per-element opt-in in a later ADR, not a looser default here.
6. **The object `IDENTIFIER` is provenance and the join key; req allocates the `claim_id`.**
   `rationale.source` is `<origin>#<IDENTIFIER>` (`--origin` required, non-empty: the URL or path the file came
   from). `--id-attr NAME` (optional) names the attribute carrying the project's own human id (DOORS
   `ReqIF.ForeignID`, Polarion `ReqIF.ForeignID`, a module's `ID` column); its value is shown in the report and
   the draft's header comment, **not** in `rationale` (a rationale has four fixed keys) and not as the
   `claim_id`. A selected, draftable object whose `<origin>#<IDENTIFIER>` is already some claim's
   `rationale.source` is refused (exit 1, nothing written), as ADR 0027 §4. **Whether an `IDENTIFIER` survives a
   second export of the same module is not verified here**: some tools regenerate identifiers per export.
   A re-sync that joins on it is a separate ADR and must be decided on two real exports of one module first.
7. **Not carried, and the report says so.** Every attribute other than the text attribute (the report names them
   with the count of drafted objects carrying each, so a `Status` or `Object Type` the caller did not select on
   is visible); the specification **hierarchy** (chapter structure: the report counts drafted objects that sit
   under a parent and names the specification); and **relations**: `relations` stays `[]`; the report counts the
   file's `SPEC-RELATION`s and the drafted objects that are the source or target of one, with the relation
   types' names. Turning them into claim relations needs the allocated ids and a human's judgement of the kind.
   (No public sample has a relation: this path is tested on synthetic ReqIF only.)
8. **The date is never invented.** `--date` is required. The header's `CREATION-TIME` is the export's time, shown
   in the report and never used as the date.
9. **Dry run is the default; `--write` writes; counts reconcile.** The last line adds up:
   `objects N = drafted D + skipped S + not selected M (by type: …)`, N counting **every** `SPEC-OBJECT` in the
   file (a ReqIF file carries no trustworthy count of its own). Exit codes are the siblings': 0 drafted, 1 a
   definite problem with nothing written (nothing selected: it lists the types and the `--where` values it
   saw; nothing extracted; `--text-attr` is defined on none of the selected objects' types; already drafted), 2
   usage, 3 UNVERIFIABLE (§2, or the corpus/registry/`req` cannot be read honestly). Equal statements are drafted as
   separate claims; `req canon` shows the equal addresses.
10. **Test data is the three public files plus synthetic ones.** `tests/fixtures/reqif/` holds the public files
    whole, byte-for-byte, with provenance, retrieval date, hashes and the licence caveat (its README). The shapes
    they lack — requirement text in DOORS style, an object-type enumeration, multi-paragraph, rich markup,
    relations, a DOCTYPE, a duplicate object identifier — are written by the tests into a temporary directory and
    labelled synthetic. The owner is asked to accept the fixture reading by merging.

## Amendment (review of #137, 2026-11-08)

Two behaviours the writer chose and the review accepted, recorded here so the ADR and the tool agree:
(a) a selected object whose text attribute's value is not text at all (an integer, a date, an enumeration) is
skipped and named `not text (<KIND>)`, after the three reasons in §5; it is never turned into text.
(b) `--id-attr` naming an attribute that none of the selected objects' types defines is exit 1 (nothing
written), as `--text-attr` is in §9. Also clarified: a file may split its objects, types, specifications or
relations over several containers of the same name; every one is read, and N still counts every `SPEC-OBJECT`.
ReqIF elements are matched by their namespace-qualified name, so a foreign-namespace element is never a value.

## What stays outside req

The engine reads claims, not ReqIF. The file is the source of the drafts; the drafts are proposals; a human
ratifies them. Not decided here: a ReqIF **writer** (pushing claims back into a tool), a re-sync that updates
existing claims from a later export (§6), reading `.reqifz`, hierarchy or relation import, a PDF reader (the
PDF is a rendering of data that ReqIF holds with its structure), and a `req import` command.

## Consequences

- An adopter on any ReqIF-speaking tool runs `--list`, names a selection and a text attribute, and reads a
  report that accounts for every object in the file. What is not one plain paragraph is named for a person.
- **The yield on real requirement specifications is unknown.** The public samples hold one requirement with
  text. The first adopter run is the measurement; until then no document may state a yield, and the page that
  documents this tool says "unmeasured".
- Three importers now share a template by copy; accepted, as in ADR 0027.

## Rejected

- **Reading the PDF instead.** It is the lossy rendering of the same data; ReqIF keeps ids, types, hierarchy and
  relations. A PDF reader stays a last resort for documents that exist nowhere else.
- **Default `--text-attr ReqIF.Text`.** It is the common name, but RMF lacks it and DOORS modules rename their
  columns; a default would silently draft nothing, or the wrong attribute.
- **Flattening rich text.** See §5.
- **A ReqIF library dependency.** The core and the tools have no network or package dependency; the subset
  read here is small, and `xml.etree` is standard library.
- **`IDENTIFIER` as `claim_id`**, and **`ReqIF.ForeignID` as the join key**: the first is a foreign generated
  string, the second is absent from RMF and an integer in one tool and a string in another.

*2026-11-13 (task I-1):* `--areas` files each draft under an area made from the specification hierarchy; see ADR 0051, note *task I-1*.
