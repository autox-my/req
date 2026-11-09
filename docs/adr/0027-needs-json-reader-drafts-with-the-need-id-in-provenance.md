# 0027. A Sphinx-Needs `needs.json` reader drafts claims and keeps the need id as provenance

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-05, former Forgejo PR #128); issue #124 (the
second half of the architecture review's finding F8, after `req diff`, ADR 0026). Date proposed: 2026-11-05.
**Adds a copy-and-adjust script; adds no `req` command and changes no address.** It is the *import* mode
ADR 0017 named for `needs.json`, built the way `tools/adopt.py` (issue #44) is built.

## Context

An adopter with a Sphinx-Needs project already has its requirements as `needs` objects with a stable id, a
title, a body (`content`), a status and typed links. Asking that team to retype them into claim files is the
adoption barrier F8 describes. ADR 0017 decided the rule (the external text is a *source*, the claim is the
obligation; an import yields unratified drafts) and left the reader unbuilt.

Real exports exist and were read before this was written (retrieved 2026-11-05): Eclipse S-CORE's
`https://eclipse-score.github.io/score/main/needs.json` (Apache-2.0, 946 needs, sphinx_needs 8.5.0) and the
Sphinx-Needs demo's `…/en/latest/needs.json` (MIT, 362 needs, 8.3.0). What they show, and the reader must
survive:

- **The type names are the project's own** (`stkh_req`, `feat_req`, `comp_req`, `aou_req` in S-CORE; `req`,
  `sysreq`, `fsr`, `swreq` in the demo). Many needs are not requirements at all (documents, architecture,
  test runs, 167 of 946 in S-CORE are `document`).
- **The statement is `content`, and it is reStructuredText, not a sentence.** Of S-CORE's 568 requirement-typed
  needs: 450 are one plain paragraph (some wrapped over several lines), 84 have several paragraphs (a
  requirement followed by a note or a list), 28 contain a directive, 6 have no `content` (empty,
  or the key absent). The demo has code-block bodies and `:ref:`-style markup.
- **Links are typed and name need ids** (`derived_from`, `satisfied_by`, `links`), with a `[version==1]` filter
  suffix in S-CORE. A claim's `relations` name `claim_id`s, which do not exist until req allocates them.
- **Equal text recurs across levels**: `comp_req__feo__act_alloc_error` and `feat_req__feo__act_alloc_error`
  carry the same sentence. That is information for the human, not something to collapse.
- A need has a `status` (`valid`, `draft`, `invalid`, …), tags, safety and security levels, a `reqtype`.

## Decision

1. **A script, not a subcommand.** `tools/needs_import.py`, python3 standard library only, a sibling of
   `tools/adopt.py` that adopters copy and adjust (per-project mapping). `req` gains nothing; the engine stays
   offline and format-agnostic. The reader is a *draft writer*: it writes the same legal `req.claim/2` files
   `adopt.py` writes (statement, provenance, empty `applicability` and `relations`, nothing derived, no pin).
2. **No default `--type`.** The caller names the need types to read (`--type`, repeatable, required). A
   built-in list would be a heuristic about someone else's vocabulary. Types not named are *counted by type* in
   the report, never silently ignored.
3. **The statement is `content` only when it is one plain paragraph.** Its lines are trimmed and joined with a
   single space (as `adopt.py` does for wrapped bullets). Anything else is **skipped and named with its
   reason**: `no content` (empty or the key absent), `multi-paragraph` (whether the second paragraph is part
   of the obligation or a note is a human decision, ADR 0004), `rst directive` (a line starting `..`),
   `rst role` (an inline `:role:` + backticks), `external` (`is_external` true: the file does not own it,
   ADR 0017's reference mode). The reader does not strip markup or truncate to make a need fit.
4. **The need id is provenance and the join key; req allocates the `claim_id`.** The draft's
   `rationale.source` is `<origin>#<need id>` where `<origin>` is what the caller passes as `--origin` (the
   URL or path the export was retrieved from); `docname:lineno` goes in the draft's header comment. A need
   already drafted from the same origin (its `source` is in the corpus) is **refused** (exit 1, nothing
   written), so re-running over the same export does not duplicate and a future drift check can join on the
   id. The source's status, tags, safety, security and `reqtype` are **not carried** (a source status is not a
   ratification, ADR 0004, and applicability is authored): the report names the fields seen and not carried.
5. **Links are not carried, and the report says how many.** `relations` stays `[]`; the report counts the
   drafted needs that had typed links and names the link fields. Turning them into relations needs the
   allocated ids and a human's judgement of the relation kind; it is a separate step if it is wanted at all.
6. **The date is never invented.** `--date` is required (the retrieval date); a `needs.json` has no
   trustworthy one (S-CORE's has no `created`).
7. **Dry run is the default; `--write` writes; counts reconcile.** The report ends with one line that
   must add up: `needs N = drafted D + skipped S + not selected M`, and the not-selected remainder is broken
   down by type. Exit codes are `adopt.py`'s: 0 drafted (dry run or written), 1 a definite problem with nothing
   written (nothing extracted, already drafted, no named type present in the file), 2 usage, 3 UNVERIFIABLE
   (the file or the corpus cannot be read honestly: not JSON, no `versions`, several versions and no
   `current_version`, a registry that does not load). Duplicated statements are drafted as separate claims;
   `req canon` shows the equal addresses.
8. **Test data is a pinned excerpt of a real export.** `tests/fixtures/needs/` holds trimmed excerpts of the
   two exports above, each kept need byte-for-byte as published, with the source URL, retrieval date, licence
   and the full file's sha256 recorded. The excerpts carry the upstream requirement text verbatim as test data
   (attribution in the fixture README); AGENTS.md's rule against quoting external material verbatim into
   *docs* is unaffected, and the owner is asked to ratify this reading by merging.

## What stays outside req

The engine reads claims, not Sphinx-Needs. Nothing here makes a `needs.json` an authority: it is the source
of the drafts, the drafts are proposals, and a human ratifies them. Two-way sync, writing back to a Sphinx
project, ReqIF/CSV/Excel readers, and a `req import` command are not decided here.

## Consequences

- An adopter on Sphinx-Needs runs one command, reads a report that accounts for every need in the file, and
  gets drafts `req canon`, `render` and `gate` accept; the 20% of requirement-typed needs that are not one
  plain paragraph are named for a person instead of being guessed at.
- The measured yield on S-CORE (450 of 568 requirement-typed needs plain) is a property of that project's
  authoring style; it is reported as such, not as the reader's quality.
- Two scripts now share a template and a corpus reader by copy. That is the cost of "copy and adjust" and is
  accepted; neither imports the other.

## Rejected

- **A `req import` subcommand.** The mapping is per project; ADR 0017 and issue #44 chose a script.
- **A default type list** (S-CORE's names would be wrong for the demo, and vice versa). See §2.
- **Stripping reStructuredText to get a sentence.** It would author the statement; the human edits the draft.
- **Using the need id as the `claim_id`.** The `claim_id` allocation is req's; foreign ids carry their own
  regexes and collide across projects (`REQ_1`, `REQ-0001`). The id is kept where it is useful: provenance.
- **Carrying `status` as ratification.** `valid` in Sphinx-Needs is not a signed Decision (ADR 0005).
