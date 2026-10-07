# The unit of requirements work is a claim, not a document

Every incumbent models the wrong thing. ALM suites make the document the database, so
every query, variant, and trace link is a hand-built artifact; issue trackers make the
ticket the unit, so there is no rationale, no graph, and no real versioning; plain-text
tools make the Markdown file the unit, so identity is a string that drifts silently and
merge stays textual. We model a **claim** — content-addressed by its canonical meaning —
and treat every document as a compiled view. The rest of the architecture hangs on this:
canonical identity is what makes dedup, contradiction detection, semantic merge, and
impact analysis possible at all.

## Considered Options

- **Document as unit** — DOORS Next, Jama, Polarion. Rejected: documents as storage make
  every relation a manual artifact.
- **File as unit** — Doorstop, StrictDoc, Sphinx-Needs. Rejected: versioning without
  semantic identity, so variants stay manual and merge stays textual.
- **Ticket as unit** — Jira, Linear, Azure Boards. Rejected: no rationale, no graph, no
  audit.

## Consequences

A canonicalization layer exists whose errors are *semantic* rather than syntactic. A false
merge silently deletes a requirement, so it must never occur without human ratification.
