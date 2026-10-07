# Identity collapses only on meaning-preserving rules

A content address must be a pure, deterministic function of a claim's meaning.
Deciding identity by probabilistic judgment — similarity thresholds, or model outputs,
however calibrated — means two different claims may become "the same claim" between two
runs. In safety work a false merge silently deletes a requirement, and an address that
varies between runs makes every signed Baseline unauditable.

The rules are split into tiers: **Tier-1** collapses (provable transformations —
encoding, casing, exact SI unit conversion, a closed comparator table, the moderated
modality table, the reviewed subject registry, condition extraction); **Tier-2**
(generated suggestions: similarity, embeddings, external model judgments) only ever
*proposes* reviewable edges — `may_be_equivalent`, `contradiction`, `impact` — with
confidence and a recorded source-version, and never changes an address.

## Considered Options

- **Model judgment inside the hash path** — rejected: nondeterministic identity.
- **Embedding thresholds inside the hash path** — rejected: a threshold makes two claims
  equal, which is exactly a silent false merge.

## Consequences

The asymmetry is structural, not a policy promise: false splits (noise) are always
survivable and always recoverable; false merges (deletion) are unreachable without human
ratification. It also fixes what an external semantic assistant may do: propose, never
decide (docs/adr/0004-model-proposes-human-disposes.md).
