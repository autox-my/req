# 0011. Tier-2's budget, and where its cost lands

**Status: proposed** — for the human to ratify. An ADR records a decision, and this one is written by
the agent that proposes it, because the rule that governs the model governs this document too: the model
proposes, a human disposes (ADR 0004).

Date: 2026-10-10

## Context

Tier-2 hands candidate pairs to a model. Almost everything about that is measured now: five providers
through one wrapper, a 29-pair authored corpus with two human readings, eleven records, and a sample
protocol whose fourth amendment records a hypothesis that failed. What has never been decided is what a
Tier-2 run is *allowed to cost* — and that decision governs which of the measured configurations is
admissible at all.

The measurements that bear on it:

- **Strands v21** (2B decision model, CPU) is the only candidate whose confidence orders the classes:
  stable floor 77 on the two-fold criterion, no false merge, 13 of 15 paraphrases. About 12 s per pair.
- **The two-stage pipeline** (a 4B GGUF decomposer naming structural differences, then Strands) costs
  about 40 s per pair and changed one outcome in 29 pairs — one alteration declined instead of none.
- **Blocking** proposes 92 candidate pairs from this repository's 15 applying claims, and reaches 15 of
  15 authored paraphrases at its threshold of two shared content terms.
- **Human time, measured rather than assumed**: authoring 15 rows took about an hour (roughly 4 minutes
  per row), and labelling 29 pairs twice took two people an hour or so each.
- **Provider failure is cheap to discover and expensive to miss**: five provider defects in one session,
  each found mid-run, one of which cost an hour of compute and several of which spent a full run
  answering from a malformed input.

## Decision

**Tier-2 is a batch survey, not an interactive service.**

1. **Advisory, never blocking.** Its output is proposals with confidences and the addresses they judged;
   a human ratifies. It takes no part in a merge decision, so its latency gates nobody's work.
2. **Triggered by a change, scoped by the change.** Because identity is a content address, the pairs
   worth re-adjudicating after an edit are exactly those involving an address that moved. Cost is
   proportional to the diff, not to the corpus — which is the property that makes a batch design
   practical on a growing model.
3. **The budget that binds is the reviewer's attention.** Machine minutes are cheap: a run may take
   fifteen minutes or an hour and finish whenever it finishes. A reviewer's minutes are scarce, at
   roughly 2–4 minutes per pair of careful reading. A run therefore must not propose more than a person
   can review in one sitting: the proposal count per run is capped and the remainder deferred to the next
   run rather than dumped on a human in one list.
4. **The confidence floor protects that budget, not the machine's.** It trades recall for review
   precision: at floor 0 Strands offers 30 false merges to wade through; at floor 77, none, at the cost
   of two paraphrases. Reviewing only correct proposals is the best economics available, and the missed
   duplicates are the affordable direction — the same asymmetry the whole project runs on.
5. **Interactive is not foreclosed.** The seam is a command with a JSON answer on stdout. If a model ever
   answers in milliseconds on this hardware, the same wrapper and the same capture become interactive
   with a pair-keyed cache and no change to the core. This decision does not rule that out; it declines
   to depend on it.

## Where the cost actually goes

**Blocking is the cost lever**, because cost scales with proposed pairs and blocking decides which pairs
are proposed. Its threshold is an asymmetric control like everything else: a pair sharing too few content
terms is *never* proposed, which is a duplicate silently dropped, measured against the corpus labels and
pinned at zero by a test. At two shared terms, 15 of 15 authored paraphrases are reachable and 92 pairs
are proposed from 15 claims; raising the threshold would cut the run's cost and the recall together. That
trade is now measured rather than assumed, and any change to the threshold must re-measure both sides.

**Provider selection is not where the cost goes**, which is worth stating because it is counter-intuitive:
the only candidate that passes the gate is the slowest one measured, and the fast ones (GLiNER at ~2 s,
THX-01 at ~1 s) are the ones whose confidence carries no signal at all. Optimising for latency would have
selected a provider with no usable operating point.

## What to measure before this is ratified or revised

1. **The diff-scoped pair count**: how many candidate pairs a typical claim edit produces. This is the
   number the batch design depends on and it has not been measured.
2. **Review time per proposal**, at the scale a real run produces, rather than the 4 minutes per row
   measured while authoring.
3. **Held-out validation** of any provider named as a default. Strands at floor 77 is in-sample on 29
   pairs with informed labels; until it survives a held-out split, `NoProvider` remains the honest
   default.

## Assumptions this decision makes visible

- **Claims are authored into git.** If requirements ever arrive from the systems that already hold them —
  Jira, Notion, Slack — then the boundary is a live query with the user's own permissions, not a bulk
  sync. External terms already forbid bulk export in at least one widely used case, a copy is a copy that
  can drift, and every such system ranks its own results. That is a future design, recorded here so the
  assumption is not silent.
- **A run's proposals are reviewable as a list.** If ratification turns out to need constant
  back-and-forth rather than periodic review, this ADR is the thing to supersede.
