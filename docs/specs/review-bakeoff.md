# Review bake-off — which review tool finds what a reviewer later made us fix

Task id: `rb-1` (one unit, research; nothing in it gates a release). Code baseline: `main` after the pull request
that carries this file merges.

Why now: ADR 0029 §7 ended "CI reviews" and left a human reading every pull request until a review workflow is
chosen for GitHub. That choice needs numbers on **this** repository, not a vendor benchmark. The candidates are
the tool already pinned (`tools/ocr`, OpenCodeReview 1.13.3) and PR-Agent (0.50.0). GitHub Copilot code review is
out of scope: the Free plan does not include it.

## The question

Given the model this project already uses (`deepseek-v4.1-flash`, through the opencode-go endpoint), which tool,
run on the diffs we actually reviewed, **finds the defects a reviewer made us fix**, at what **noise**, **time** and
**token** cost?

It is not a question about which model is best, and it is not a verdict on a CI workflow: a workflow that holds an
LLM key stays a decision of its own (ADR 0029 §7). This experiment only decides which tool a maintainer runs by
hand, and whether a later ADR for a workflow is worth writing.

## Arms (one model, one endpoint, one day)

| arm | tool | how it runs |
|---|---|---|
| `O1` | OpenCodeReview 1.13.3 (the `tools/ocr` pin) | exactly the flags the retired Forgejo review ran: `--effort low --no-filter --max-tools 150 --timeout 30`, `--rule review/rule.json` (today's), `--exclude 'bench/corpus/**,bench/results/**'`, three attempts |
| `PR` | PR-Agent 0.50.0 `/review` | `git_provider=local` (the same `base..head` range on a checkout of `head`), defaults except the items below |
| `PI` | PR-Agent 0.50.0 `/improve` | same, so the suggestions it makes count as findings |

What is **not** the tool's default, and why (recorded so nobody reads them as tuning):

- the model, key, `api_base` and the `x-opencode-session` header the endpoint requires (every arm);
- `ai_timeout` 120 s → 600 s: the smoke run timed the reasoning model out at 120 s and PR-Agent retried;
- `custom_model_max_tokens` 128000 so an unknown model id is accepted; `max_model_tokens` stays at its default
  of 32000, so a large diff is compressed the way PR-Agent compresses it. Each run's log says when it was;
- the same two path classes excluded as the production review excluded them (`IGNORE__GLOB`).

PR-Agent's default `repo_context_files = ["AGENTS.md"]` is read through the git provider; whether the `local`
provider supplies it is recorded per run, not assumed. OCR reads the repository through its own tools; that is a
design difference between the products and is part of what is being compared.

## Cases (24, `tools/review/bakeoff/cases.json`)

- **14 `fix` cases.** A merged pull request that has a commit whose subject says it answers a review (`review`,
  `OCR`). The reviewed state is the pull request **just before its first such commit**
  (`merge-base(main, PR)..<parent of that commit>`); the commit itself is the ground truth. One PR, one case.
  Chosen for a code, test or workflow defect a reader of the diff could plausibly find; the review-tooling-only
  PRs (rule text, `check_rules.py`) are left out because their defects are about the tool, not the code.
- **10 `clean` cases.** Merged code pull requests from after the review gate existed (2026-10-15) with **no**
  review-answering commit identified. "Unlabelled" is the honest word: a finding there may be real. They exist to
  measure what each arm says when nobody later asked for a change.

The case numbers (`#NN`) are Forgejo numbers (ADR 0029 §8). Sizes run from 70 to 1,361 changed lines
(`bench/` excluded).

## Ground truth

For each `fix` case one agent reads the fix commit's **message and diff** and writes the atomic defects the
reviewer found and the author accepted (`ground_truth`), the findings the author **refuted** (known false
positives) and the ones acknowledged but left (`declined_true`). It is written **before** any tool output exists
and from the commit alone; it never sees a result file. Paraphrased, never copied from the message.

## Judging

One judge per case, on a different model family from the reviewing model, **read-only**, with the checkout of the
reviewed state:

1. The packet holds the ground truth and every finding from every arm in a seeded shuffle, with neutral ids
   (`F01…`), the file, lines and text. No arm, no tool severity, no tool score. The key that maps ids to arms is
   kept outside the judge's reach and joined only by `bakeoff.py score`.
2. For each finding the judge reads the code and labels it `real` (a defect or risk the author would want fixed
   before merge, **verified in the code**), `minor` (true, trivial), `noise` (wrong, misreads the code, restates a
   documented decision, or has no concrete trigger) or `unverifiable`. It lists the ground-truth items the finding
   substantively matches (`gt`) and a `cluster` id shared by findings that describe the same defect.
3. A finding that matches a ground-truth item is `real` by construction.

Limit: the judge is blind to the arm, **not to style**: OCR and PR-Agent write differently.

## Measures

Per arm, over the cases it completed:

- **recall of ground truth** (fix cases): matched items / items;
- **precision**: `real` / judged findings, and also `real+minor` / judged;
- **unique real**: `real` clusters no other arm produced;
- **clean-case behaviour**: cases with any finding, cases with a `real` finding;
- **completeness**: runs that did not complete (a failed status, `coverage.failed`, no result file) are counted
  and never read as "no findings";
- **cost**: wall seconds and tokens per run (OCR reports `total_tokens`; PR-Agent reports `usage`).

## Decision rules, fixed before any result

1. An arm with more than 10 % incomplete runs is **unreliable** and cannot be adopted, whatever else it scores.
2. A challenger **replaces** OCR as the by-hand reviewer only if its `real` precision is at least OCR's **and**
   its recall is within 0.10 of OCR's or higher.
3. A challenger is **added** as a second opinion if it produces at least **3 distinct `real` clusters** OCR
   missed across the 24 cases at a noise rate no worse than OCR's.
4. Differences under 0.15 on a proportion are **not read**: with 14 and 10 cases a single case moves a rate by
   0.07–0.10. The report gives counts beside every rate.
5. Nothing here moves the review gate: the output of any arm stays an unverified lead (AGENTS.md, Review).

## Limits, stated now

- **Selection bias favours OCR on recall.** Most ground truth was found by OCR-family reviews (the commit
  messages say so; each case records `finder`). OCR's recall is therefore an upper bound and a challenger's a
  lower one; the report gives recall split by `finder` where the numbers allow.
- One model, one repository, one run per cell: run-to-run variance is **not** measured. (A second phase that
  repeated three cases twice per arm was planned and dropped: the owner asked for a verdict, not more measurement.)
- The judge is an LLM. A random 20 % of its labels is re-read by a human before any number is repeated.
- The earlier reviews that produced the ground truth ran on other models and an older rule file; today's
  `review/rule.json` is used for all `O1` runs.
- The reviewing model is a reasoning model with no seed (OCR sends no temperature or seed). Two runs differ.
- `deepseek-v4.1-flash` is not what PR-Agent is tuned for. A different model could change its standing.

## Files

`tools/review/bakeoff/bakeoff.py` (`run`, `pack`, `score`, `--self-check`) and `cases.json`. Outputs, packets,
keys and labels live outside the tree (a scratch directory); only the report is committed.
