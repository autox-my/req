# M2-1 — drop the local OCR loop and the meta-checkers; CI review stays

Task id: `M2-1`. Code baseline: `b6ef8bd`. Owner ruling of 2026-11-03 on F6 of the architecture
review: the review that blocks merges is the CI one; the local delegation-mode loop and the Python
that polices the review tool are removed. Nothing here changes an address, a claim or the bet.

## Why

About 1,400 lines of Python (`tools/review/check_rules.py` 709, `check_review_scope.py` 475) and
three `scripts/check.sh` steps police the review tool's configuration, and `AGENTS.md` carries a
local self-review procedure for a review CI already runs. CI's review found two real highs on #80
that the local loop also found: the local loop is duplicated cost. The CI verdict, its gate
(`tools/review/ocr_gate.py`, taken from `origin/main`) and its rule file (`review/rule.json`, taken
from `origin/main`) are the part that blocks merges, and they stay.

**Accepted risk, stated rather than hidden:** `check_rules.py` existed because the review CLI
silently falls back to its built-in rubric for a glob that matches nothing, so a renamed directory
would drop a checklist with every exit code zero. With the checker gone nothing detects that. The
owner accepts it; `AGENTS.md` keeps one sentence naming it.

## What to do

1. **Delete** `tools/review/check_rules.py` and `tools/review/check_review_scope.py`. Keep
   `tools/review/ocr_gate.py` (and whatever runs its `--self-check` today; find it with grep, it is
   in `review.yml`, not in `check.sh`) and `review/rule.json`, `tools/ocr/**`,
   `.forgejo/workflows/review.yml`, `review-measure.yml` untouched.
2. **`scripts/check.sh`:** remove the review-rule self-check step, the "ocr (review tool, not a
   gate)" version/pin report, the `check_rules.py` / `check_review_scope.py` runs and the skip
   messages tied to them, and the two `FAST` lines naming them. Every other step, its order and its
   exit contract stay. `--fast` stays a mode with fewer steps than the full run; its description in
   `AGENTS.md` (item 5) and the banner must still be true.
3. **`tests/baseline.rs`:** the plan/fast tests pin the removed steps by string (the two `FAST the
   review-rule …` lines near line 2515, and `"check_rules.py"` in the full plan near 2562). Update
   those expectations to the new truth; do not delete the assertion that every dropped step in
   `--fast` is *named* as skipped, only the removed names. Write the updated expectations first and
   show them failing at the base (that is the red).
4. **`.forgejo/workflows/ci.yml`:** the gate job installs `ocr` only so the rule-glob check would
   not skip (its own comment says so). Remove that install and version step and nothing else in the
   job. Say in the PR description that a workflow file changed and what it did before/after; the
   owner reviews that diff by eye.
5. **`AGENTS.md`:** replace the sections "Review — delegation mode locally, LLM mode on pull
   requests", "Self-review before merge (a loop, not a pass)", and the `ocr` install / rule-file
   shape material with one **"Review"** section of at most 45 lines that keeps exactly:
   - CI reviews, blocks on critical/high, and the verdict script, the rule file and the trust anchor
     come from `origin/main`, never from the PR;
   - `--no-filter` and its consequence (green means "the reviewer reported nothing"; findings are
     unverified, confirm each against the code);
   - the rule-file facts that still bite: unknown keys are ignored; a custom rule replaces the
     built-in rubric for its paths; **a glob matching nothing silently falls back and nothing
     checks it any more**; markdown and `*.minisig` are outside the reviewer's view, so docs are
     read by hand and a signed change always carries its signature;
   - the "Reading a failed CI review" paragraph from PR #81, verbatim;
   - one line that `tools/ocr` is the CI pin (bump it deliberately; state it in the commit).
   Remove the local `ocr delegate …` commands and the review-coverage steps. Update the `--fast`
   description and any other line that names the removed steps. Do not otherwise shorten
   `AGENTS.md`: the full cut is M2-3.
6. **Stale references:** `docs/design.md` (~line 306) and `docs/adr/0018-…md` (~line 85) cite
   `check_rules.py` as a precedent. Leave the ADR's decision text; add a dated one-paragraph
   *Amended* note to ADR 0018 (the ADR 0019 precedent) saying the tool was removed on 2026-11-03 and
   why; fix the one sentence in `design.md`. `tests/loader.rs` comments that mention the tool's
   "lesson" are history: leave them.
7. The PR description records the `check.sh` step count before/after and the full-gate wall time
   before/after (measure both on your branch; the point of the unit is that the gate gets cheaper).

## Red test

`tests/baseline.rs` expectations updated per item 3, failing at the base.

## Verify

`HOME=<main checkout>/.tools/home bash scripts/check.sh` full run, exit 0, no SKIP; baseline
`[VERIFIED]` (nothing here moves an address). Markdown is outside the OCR selection: the PR says
`AGENTS.md`, `docs/design.md` and the ADR note were read by hand.

## Allowed paths

`tools/review/check_rules.py` and `tools/review/check_review_scope.py` (deleted),
`scripts/check.sh`, `tests/baseline.rs`, `.forgejo/workflows/ci.yml` (the ocr install/version step
only), `AGENTS.md` (the review sections of item 5 and the lines that name removed steps),
`docs/design.md` (one sentence), `docs/adr/0018-documents-are-projections.md` (the Amended note).

Forbidden: `src/**`, `review/**`, `tools/review/ocr_gate.py`, `tools/ocr/**`,
`.forgejo/workflows/review.yml`, `.forgejo/workflows/review-measure.yml`, `claims/**`,
`baselines/**`, `bench/**`, `aliases/**`, every other doc.
