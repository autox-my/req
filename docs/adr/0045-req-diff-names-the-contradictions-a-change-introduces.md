# 0045. `req diff` names the contradictions a change introduces

Extends ADR 0026 (`req diff`) with the follow-up ADR 0043 named in *Not decided here* ("contradictions introduced by a
change"). No class is added or renamed, no `RULES_V` bump, no claim-schema or Baseline change.

**Status: proposed.** The owner ratifies it by merging the pull request that carries it, together with the code (task
D-1). Asked for by the owner on 2026-11-12, as the second item after PR #60 merged. Date proposed: 2026-11-12.

## Context

ADR 0043 judges contradictions at the gate, over a **state** of the corpus. A pull request that introduces one is caught
only when `req gate` or `req check` runs on the merged state. On a repository that runs only `req diff --fail-on` on pull
requests (the ADR 0026 wedge), it is not caught at all. The reviewer reads "REQ-0042 added" with no sign that it cannot
hold beside REQ-0007.

`req diff` has no selection, but contradiction is a property of one (ADR 0043 *Rejected*: never the 150% model). So the
diff must say which selection a contradiction belongs to.

## Decision

1. **Selections.** `req diff` judges contradictions once per line of `.req/variants`, read from the **head** side (the
   working tree, or the `--head` revision's blob). Each line is evaluated on both sides with that side's claims. With no
   list, or an empty one, it uses the default bare selection, as `req check` does (ADR 0023). One list for both sides,
   because a contradiction is "new" only against the same release.
2. **New and resolved.** A contradiction is an unordered claim-id pair, a variants line, and ADR 0043's Tier-1 verdict.
   - It is **added** when it holds at head and is not recorded as a Conflict there, and the same (pair, line) did not hold
     at base.
   - It is **resolved** when it held at base and does not hold at head, or is recorded as a Conflict at head.
   Both are listed in a `contradictions:` section after the per-claim classes, with both ids, the line, the slot and the
   two values.
3. **A flag, not a class.** Each claim in an added contradiction carries the flag `contradiction-added <other-id>`, and
   `--fail-on contradiction-added` turns the run into exit 1. A claim's class is about its own change; a contradiction is
   about a pair, and either side may be unchanged (an applicability edit, or the other claim added).
4. **Only an admitted detector raises the flag.** While `contradict::DETECTOR_ADMITTED` is false, added contradictions are
   listed as *reported, not judged*. No flag is raised and `--fail-on` does not fire on them, as at the gate (ADR 0043
   decision 6).
5. **JSON.** `req.diff/1` gains `contradictions: {added: [...], resolved: [...]}`. The flag appears in an entry's `flags`
   like the others. Both changes are additive.
6. **Still a description.** `req diff` never says PASS or FAIL (ADR 0026). The section says what changed, and the gate
   stays the judge of a state.

## Consequences

- A pull request that adds a claim contradicting an existing one is named on the pull request, with the other claim and
  the release line.
- `req diff` now reads `.req/variants` and runs the selection evaluator on both sides, so it costs one detector pass per
  line per side. The bucketing keeps that near-linear (ADR 0043 decision 1).
- A corpus with selectors and no `.req/variants` has no applying claim under the bare selection, so no contradiction is
  ever reported. That matches the gate, which is UNVERIFIABLE there.

## Rejected

- **A new class (`contradicting`).** It would hide the claim's own direction (a `weakened` that also contradicts).
  Classes are per claim; this is per pair.
- **Comparing the whole corpus with no selection.** This is the 150% model ADR 0043 rejects.
- **Reading `.req/variants` from each side separately.** A line renamed or added in the pull request would make every
  contradiction under it "new".

## Falsification

This decision is wrong if any of these is observed:

1. a contradiction that already held at base, under the same line, is reported as added;
2. a pair whose two claims never apply in the same line is reported;
3. the flag is raised, or `--fail-on contradiction-added` fires, while the detector is not admitted;
4. a contradiction recorded as a Conflict at head is reported as added.
