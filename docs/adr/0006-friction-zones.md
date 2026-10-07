# Friction is deliberate at the decision boundary, absent in authoring

Regulated work requires auditable approval, and Linear-style friction elimination breaks
21 CFR Part 11 and ISO 26262. But every plain-text requirements tool has died because
non-engineers abandoned it, so friction cannot be spread across the whole tool. We split
the product into two zones: **authoring** is friction-free and produces Proposals, and the
**decision boundary** is deliberately high-friction, demanding role, meaning, and evidence.

## Considered Options

- **Uniform friction** — the "compliance tool" default. Rejected: kills adoption, which is
  how Doorstop and StrictDoc stayed niche for a decade.
- **Uniform ease** — the Linear default. Rejected: indefensible to an auditor.

## Consequences

Two interfaces over one engine, and a non-engineer should be able to work a full day
without ever meeting git.
