# Variants are selectors over a 150% model, not branches

The target market's dominant pain is *permanent* variation: a platform with 40 variants
sharing 80% of its requirements, a device family with per-region regulatory variants.
These coexist forever. Git branches are transient and merge back, and long-lived
divergence is git's most notorious weakness — so a branch-per-variant design inherits that
weakness exactly where the market hurts most. Claims carry applicability predicates, and a
variant is a selection over the 150% model; a variant's specification is a query, and
cross-variant consistency is machine-checkable.

## Considered Options

- **Branch per variant** — git's own model. Rejected: transience is the wrong shape for
  permanence.
- **Copy-paste per variant** — what teams actually do today. Rejected: this is the disease.
- **Inheritance hierarchy per variant** — fragile and order-dependent; an override deep in
  the chain is invisible.

## Consequences

Applicability must be **tri-state** — UNKNOWN, never silently FALSE — because FALSE would
drop a safety claim out of a variant without anyone noticing.
