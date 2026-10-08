# The Tier-2 sample: a protocol, not a corpus

Tier-2 asks a model whether two requirements mean the same thing. Every number the funnel has
produced for that question so far came from `bench/corpus/tier2-v1.yml`, which is generated: its
pairs are transformations the generator applied, so the labels are by construction and the pair
distribution is the generator's, not the world's. Two consequences were measured rather than
assumed.

First, the confidence floor fitted: chosen on one half of that corpus it gave zero false merges,
and on the other half one false merge — the halves disagreed about where the boundary lay. Second,
the prompt variant that named the differing content terms collapsed the margin between true
positives and false merges from +19 points to 0. Both are recorded under `bench/results/`. A corpus
whose pairs were built by one generator can be out-grown by a model that learns that generator.

So the evaluation corpus has to be independent of the generator, and independent of the provider.
This document is the protocol for building it. It is deliberately written before the labels exist,
because a rule chosen after seeing results is a fitted rule.

## What a pair must satisfy

1. **Real prose, pinned provenance.** Statements come from other projects' repositories, with the
   repository and commit recorded per pair. The pool is `bench/corpus/external-v3.yml`, whose
   sources are pinned by commit SHA.
2. **Crossing requirement ids.** The two statements must come from *different* requirement ids of
   the same repository. Pairing within one requirement id reproduces that corpus's own construction
   transforms — modality synonyms, case, unit scaling — which are Tier-1 cases the funnel resolves
   before Tier-2 is reached. Mining a mutation corpus within a requirement finds the mutations.
3. **Plausible candidates only.** Candidates share at least two content terms, so the sample is not
   polluted by pairs no funnel would ever propose. The tokenizer used for candidate generation is
   approximate; it shapes which pairs are shown and cannot bias a label, because the label is
   written by a person reading the statements.
4. **No model in the loop.** No provider proposes, filters or ranks the pairs. Candidates come from
   the deterministic rule above, and the labels come from humans. A provider's suggestions would
   make the sample a measurement of the provider's own taste.

## Stratification

The cross-id pool is enormous and mostly easy: 128,185 pairs share two or more content terms, and a
uniform sample of it is dominated by weak intersections that share a word or two and nothing else.
Spending a labelling budget there measures the obvious. The pilot therefore draws equal thirds:

| tier | rule | what it is for |
|---|---|---|
| A | 4 or more shared content terms | the danger zone: near-identical wording a qualifier apart |
| B | exactly 3 | the related boundary: overlapping logic, different actor, scope or bound |
| C | exactly 2 | the blocker's floor: the weakest pair the funnel would still propose |

Each pair records `jaccard_pm`, the intersection over union in integer per-mille. This is not
decoration. Raw shared-term count is length-biased — a long statement accumulates shared terms — and
in the pilot's tier A the ratio spans 105 to 700 per-mille, meaning several tier-A pairs qualify only
by being long. The full corpus should add a ratio floor (250 per-mille is the current guess, to be
settled by the pilot) and keep the count tiers as the coarse first cut.

## The labelling act

A person reads the two statements and writes, per pair:

- `label` — `equivalent`, `related`, or `unrelated`;
- `confidence` — an integer 0 to 100, how sure they are;
- `note` — what differed, or what made it the same.

Two rules for the labeller. **Upstream text is preserved, not repaired**: the pinned PROMISE data
contains mojibake such as a mangled Windows-1252 curly quote, and a labeller should read through it
rather than treat it as a typo to fix. **A pair that cannot be decided is worth recording as such**:
"could not decide" is an outcome here, not a failure, and it is the raw material for the ceiling.

The agent that wrote the funnel does not label. On this task a label produced by a model is the
thing under test; a corpus labelled by a model could not falsify anything about models.

## The ceiling, and why it comes first

Two labellers label all thirty pilot pairs, exhaustively, and the protocol reports their agreement.
That number is the **ceiling**: if two people who read the statements carefully disagree about
whether a marginal pair is equivalent, then no model should be measured against 100%, and the
honest target becomes the agreement rate. No table in this project has carried that number, which
means every precision figure so far was quoted without knowing what perfect would look like.

## Scale, and what it gates

Thirty pairs is a pilot, not an evaluation. It yields three things cheaply: minutes per pair, the
agreement ceiling, and whether the marginal cases are decidable at all. Only then is the full
corpus committed to — realistically 150 to 300 pairs for a precision claim with usable intervals,
with the ratio floor applied, which at a minute or two per pair is most of a day of reading.

That corpus is the gate on fine-tuning, which is the only remaining lever able to move a model's
internal decision boundary. Training on the 68 generated pairs would fit the generator; training on
this sample is the first thing that could be measured against something not built by a template.

## Current state

`bench/corpus/tier2-pilot-v0.yml` holds the thirty drawn pairs with full provenance and **empty
labels**. It is the reviewable surface: its header records the pool, the pairing rule, the
stratification and the sampling stride, so a reader can reproduce the draw rather than trust it.
The ADR that fixes the protocol and the ceiling comes after the pilot reports them.

## Amendment 1: what the pilot found

Two labellers filled all thirty pairs, independently. Both fillings validated clean. What they
produced:

```
agreed 22   disagreed 8   undecided 0        ceiling 73%
tier A (high overlap) 6 agreed / 4 disagreed
tier B (3 shared)     7 agreed / 3 disagreed
tier C (2 shared)     9 agreed / 1 disagreed
```

Four findings, and the first changes what the ceiling means.

**1. Neither labeller wrote `equivalent` — not once, in sixty judgements — and all eight
disagreements are `related` versus `unrelated`.** The sample therefore contains no true duplicates,
and two consequences point in opposite directions.

For the question the gate asks — are these the same requirement? — the two labellers agreed on all
thirty pairs. Three labels collapse to two for that question, and the ceiling on it is unanimity, not
73%. This corroborates the provider verdict rather than complicating it: every pair d1 called
equivalent, both humans independently called not-equivalent, so those false merges are unambiguous
errors rather than boundary calls. The 73% is disagreement about *which kind* of difference a pair
has — a distinction the record should carry and the gate does not use.

For measuring a provider's precision **on equivalents**, the sample is useless: zero positives in
thirty pairs. The mechanism is structural, not luck. Pairing across differing requirement ids
correctly excludes the source corpus's own transform pairs, but what remains is, by construction,
different requirements that happen to share vocabulary. In real prose the base rate of two distinct
requirement ids stating the same requirement is near zero, so **mining cannot produce a positive
sample**. The first version of this protocol assumed it could.

**2. The draw clustered by project.** Every tier-A pair is a StrictDoc "Requirements Tool" statement,
because the sort key `(repo, a, b)` followed by a stride walks alphabetically within one repository.
Stratifying by lexical overlap worked; stratifying by source was never in the rule.

**3. Confidence does not travel.** One labeller's median confidence was 100, minimum 90, while
disagreeing with the other on four of the ten hardest pairs. The other's median was 86. A stated
confidence of 100 is a statement about the labeller, not about the pair, and the field is recorded for
the record's sake rather than as evidence.

**4. The related/unrelated boundary is where human ambiguity lives.** It is not the boundary the
provider was failing on: d1 called those pairs equivalent, which both humans reject outright. A
provider's error and a labeller's doubt are different phenomena and should not be reported as one.

## Rules from here

1. **Equivalents are sourced deliberately, never mined.** A positive sample needs pairs *known* to
   restate one requirement: the same requirement reworded across releases, restated in a second
   document of the same project, or written as a paraphrase by a person. Each pair records which
   route produced it, so any precision figure can say where its positives came from.
2. **Stratify by repository as well as by lexical overlap.** No tier may be one project.
3. **Report the ceiling twice** — once over the three-way labels, once over the binary question the
   gate asks. A provider is judged against the second.
4. **Keep `undecided`.** It went unused here. A pair nobody can decide is data, and the vocabulary
   must keep admitting it.

The thirty pairs and both fillings stand as the pilot's record: the worksheet is committed, the ceiling
is recorded under `bench/results/`, and the fillings remain session scratch because they are two
people's working notes rather than repository content.
