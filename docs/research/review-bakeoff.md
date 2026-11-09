# Review bake-off — result

Experiment: [`docs/specs/review-bakeoff.md`](../specs/review-bakeoff.md) (question, arms, cases and decision rules, all fixed
before any result). Run 2026-11-09 on one model, `deepseek-v4.1-flash` through opencode-go, over 24 historical pull
requests of this repository. Data to recompute every number below: `docs/research/review-bakeoff-data/`.

**Status of the numbers: unaudited.** The labels are one LLM judge's (Opus, blind to the tool); the spec requires a human to
re-read a random 20 % before any figure is repeated, and that has not happened. Read this as a lead, not a finding.

## Result in five lines

1. **OCR stays the by-hand reviewer.** Neither PR-Agent arm meets the replacement rule: their recall of the defects reviewers
   made us fix is 0.15 and 0.25 against OCR's 0.41.
2. **PR-Agent finds real things OCR does not.** Of 60 distinct real findings across all arms, 17 were found only by a PR-Agent
   arm and 28 only by OCR. PR-Agent costs about 4–6 % of OCR's tokens.
3. **OCR is the loudest.** 48 of its 101 findings were judged `minor` (true but trivial): about two per pull request on top
   of the real ones. Its `noise` rate is the lowest (7 %).
4. **All three arms together recovered 34 of the 61 defects** (56 %). Whatever the tool, this model leaves nearly half of
   what a reviewer later demanded unfound.
5. **Nothing here moves ADR 0029 §7.** No arm is a review gate; any output is an unverified lead.

## The table (24 cases per arm: 14 whose PR later needed a review-answering commit, 10 with none identified)

| | OCR (`O1`) | PR-Agent `/review` (`PR`) | PR-Agent `/improve` (`PI`) |
|---|---|---|---|
| runs completed | 24 / 24 | 23 / 24 (`f117`: output budget exhausted, 3 attempts) | 24 / 24 |
| findings | 101 | 27 | 57 |
| `real` | 46 (0.46; 95 % CI 0.36–0.55) | 17 (0.63; 0.44–0.78) | 24 (0.42; 0.30–0.55) |
| `minor` | 48 | 6 | 22 |
| `noise` | 7 (0.07; 0.03–0.14) | 4 (0.15; 0.06–0.32) | 11 (0.19; 0.11–0.31) |
| `real + minor` | 94 (0.93) | 23 (0.85) | 46 (0.81) |
| ground-truth defects found | 25 / 61 (0.41) | 9 / 61 (0.15) | 15 / 61 (0.25) |
| … of the 50 first found by an OCR-family review | 21 | 7 | 11 |
| … of the 11 first found by another reviewer | 4 | 2 | 4 |
| distinct real findings found (of 60) | 43 | 17 | 23 |
| real findings no other arm made | 30 | 7 | 7 |
| clean cases with any finding / with a `real` one (of 10) | 9 / 8 | 6 / 3 | 10 / 2 |
| tokens (all 24 runs) | 30.5 M | 1.2 M | 1.7 M |
| wall time per run (mean) | 442 s | 601 s | 428 s |

Precision here is `real` over judged findings; the intervals are Wilson. With 27 to 101 findings per arm, only large gaps
mean anything (spec, rule 4).

## The pre-registered rules, applied

1. *More than 10 % incomplete runs makes an arm unreliable.* None: `PR` has 1 of 24 (4 %).
2. *A challenger replaces OCR only if its `real` precision is at least OCR's and its recall within 0.10.* **Neither does.**
   `PR` precision is higher (0.63 against 0.46, a gap above the 0.15 that rule 4 allows reading) but its recall is 0.26 lower;
   `PI`'s precision is not distinguishable from OCR's and its recall is 0.16 lower.
3. *A challenger is added as a second opinion if it makes at least 3 distinct real findings OCR missed, at a noise rate no
   worse than OCR's.* Both make far more than three (10 each, 17 between them). Their noise rates are higher on the
   point estimates (0.15 and 0.19 against 0.07) but the gaps (0.08 and 0.12) are under the 0.15 that rule 4 says not to read.
   **So each qualifies narrowly if rule 4 is applied as written, and neither does on point estimates alone.** The rules left
   that ambiguity; it is stated, not resolved in PR-Agent's favour.

## How to read it

- **Different tools see different things.** The overlap is small: 43 distinct real findings from OCR, 17 from `/review`,
  23 from `/improve`, 60 in total. A second pass with PR-Agent is cheap (about 120 k tokens per pull request against OCR's
  1.3 M) and is where the 17 came from.
- **OCR's volume has a price.** 4.2 findings per pull request, 2 of them `minor`. A reviewer who must read all of them pays
  that attention; the single-call PR-Agent arms say much less (`/review` 1.1 per case).
- **Selection bias favours OCR on recall, and the data show it is small.** The ground truth was mostly found by OCR-family
  reviews (50 of 61 items). OCR's recall on those is 21 / 50 (0.42); on the 11 found by other reviewers 4 / 11 (0.36). Eleven
  items cannot settle it.
- **`/improve` is not a second `/review`.** It found 24 real findings to `/review`'s 17 but with 11 `noise`; they overlap only
  partly.
- **Clean cases.** OCR said something real on 8 of 10 pull requests nobody later asked to change; those are not all false alarms
  (the labels were verified against the code), but they are also not defects anyone demanded.

## What this does not show

- **One model, one repository, one run per cell.** Run-to-run variance is unmeasured (the spec's phase 2, three cases twice, is
  not done). A different model could move PR-Agent a lot: `deepseek-v4.1-flash` is not what it is tuned for, and on `f117`
  it exhausted its output budget.
- **The judge is an LLM, blind to the arm but not to style.** It flagged its own uncertain calls (for example whether a stale
  doc comment is `real` or `minor`, and a few ground-truth items it thought overstated or weak). The human re-read is pending.
- **PR-Agent ran at its default 32 000-token context**, so a large diff was compressed; this was not isolated as a cause.
- **Ground truth is the commit's own account** of what a reviewer found (extracted before any tool ran), including a few
  items its extractor itself marked uncertain.
- **Copilot code review is out of scope**, and no workflow was evaluated, only the tools run by hand.

## Data and reproduction

`docs/research/review-bakeoff-data/` holds the extracted ground truth (`gt.json`), the 24 blind packets, the key mapping
each finding to its arm, the judge labels, and each run's normalized findings and metadata. To recompute the table:

```
python3 -I tools/review/bakeoff/bakeoff.py score --out docs/research/review-bakeoff-data/results \
  --keys docs/research/review-bakeoff-data/keys --labels docs/research/review-bakeoff-data/labels \
  --gt docs/research/review-bakeoff-data/gt.json
```

Findings about the retired Forgejo workflows quoted the private host and removed workflow file names. In the committed data
those strings are replaced by `<private-host>`, `<removed forgejo workflow>`, `<removed workflow>` and `<FORGE>` (an upper-case environment-variable prefix); nothing else was
edited, and the score recomputed from the scrubbed data equals the one computed from the originals.
