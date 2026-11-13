# Sphinx-Needs `needs.json` fixtures (issue #124, ADR 0027)

Test data for `tools/needs_import.py`. Each file is a **trimmed excerpt of a real export**: the top-level
keys, the `versions.<v>.creator` object and the `versions.<v>.needs_schema` object (the export's own
declaration of its link types) are kept, the `needs` mapping is cut down to the needs named
below, and **each kept need is byte-for-byte the object the export published** (key order and values
untouched). Nothing was rewritten to suit a test. `needs_amount` and the other count keys are not kept,
because they would no longer be true of an excerpt; the reader must not depend on them.

| file | full export | retrieved | licence | kept needs |
| --- | --- | --- | --- | --- |
| `score-excerpt.needs.json` | `https://eclipse-score.github.io/score/main/needs.json` (Eclipse S-CORE, sphinx_needs 8.5.0), full file sha256 `1dd1a9bb2283e97c3227770d03c43b8815d6e0109a222d2631cdba06d62360eb` | 2026-11-05 | Apache-2.0 (the S-CORE project) | `stkh_req__ai_platform__agent_execution`, `feat_req__ai_platform__agent_execution`, `comp_req__feo__act_alloc_error` (wrapped lines, typed links), `feat_req__feo__act_alloc_error` (same text as the `comp_req`: a real cross-level duplicate), `aou_req__communication__1` (wrapped lines), `aou_req__os__nonroot` and `aou_req__feo__on_target_crates` (several paragraphs; the second is status `invalid`), `stkh_req__dependability__firewall` (no `content` key), `mod_view_sta__os__os` (status `null`, not a requirement type), `doc__ai_platform` (a `document`) |
| `sphinx-needs-demo-excerpt.needs.json` | `https://sphinx-needs-demo.readthedocs.io/en/latest/needs.json` (Sphinx-Needs demo, sphinx_needs 8.3.0, `created` 2026-07-30), full file sha256 `75ccd6db112dd7e09347fc15a28ebbf81a54e080565a94bc5ff38a2ab8ff0a11` | 2026-11-05 | MIT (the sphinx-needs-demo project) | `EX_REQ_001` (reStructuredText body), `SYSREQ_BRAKE_01`, `FSR_BRAKE_CTRL_01` (plain, wrapped lines), `GH_ISSUE_12` (a code-block directive), `ARCH_001` (a `links` list), `REQ_1_1_imp` (`is_import` true) |

The excerpts hold the upstream projects' own requirement text, verbatim, as **test data**, with the
attribution above; they are not quoted in any doc (AGENTS.md: external reference material is read-only and
never quoted verbatim into our docs). The two exports use different need `type` names for the same idea
(`feat_req` against `sysreq`), which is the reason the reader has no default `--type`.

## `areas.needs.json` (task I-1)

Not an excerpt of a real export: a **synthetic** `needs.json` written by hand for the `--areas` tests
(`tests/needs_import.rs`). Six `req` needs and one `doc`
need that is never selected. Four of the six carry a `docname` (three distinct ones), the other two carry none
(one has the key absent, one has it `null`). It is an export of no tool, and nothing is claimed from it about a real project.
