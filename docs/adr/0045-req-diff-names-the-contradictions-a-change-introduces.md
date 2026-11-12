# 0045. `req diff` names the contradictions a change introduces

Extends ADR 0026 (`req diff`) with the follow-up ADR 0043 named in *Not decided here* ("contradictions introduced by a
change"). No class is added or renamed, no `RULES_V` bump, no claim-schema or Baseline change.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it, together with the code (task
D-1; 2026-11-12, GitHub PR #62). Asked for by the owner on 2026-11-12, as the second item after PR #60 merged. Date proposed: 2026-11-12.

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

## Implementation note — 2026-11-12 (D-1: the code)

Decisions 1–6 are implemented in `src/diff.rs`; `src/check.rs` only made its `.req/variants` reader and parser
`pub(crate)`, so there is one parser. `contradict::find` is called, unchanged, on each side's rows per line.

- **"Holds" means outstanding.** Decision 2 reads a contradiction that is *unrecorded* as the one that holds, the same word
  the gate uses (ADR 0043). So *added* is outstanding at head and not outstanding at base under that line, and *resolved* is
  the reverse. Read the other way (a pair "holds" whether or not a Conflict records it), a pair recorded at base and at head
  would be listed as resolved in every diff for ever. The consequence a reviewer should know: a pair recorded at base whose
  `conflicts-with` is deleted at head is **added** (outstanding again), and a pair recorded on both sides is not news.
- **One list, read from head.** With no `--head` the working tree's `.req/variants` is read; with `--head <rev>` that revision's
  blob (so a working-tree edit does not move a `--head` report). A list that cannot be read (malformed, not a regular file, not
  UTF-8) is exit 3 naming `.req/variants:<line>`; a base list is never read. Lines with identical words count once. One
  difference from `req check`: with `--head`, a list committed as a symbolic link is exit 3, not followed (git stores the
  link's target path, and resolving it would read outside what `req diff` extracts); without `--head` the working-tree link
  is followed as `req check` follows it (review D-1-review-1). The default selection carries `req check`'s labels.
- **Order.** Pairs are sorted by the line's position in the list, then by the two ids.
- **The admission is a parameter** (`flagged_claims(delta, admitted)`; `contradictions_block(delta, n, admitted)`), so falsification
  3 is tested while `DETECTOR_ADMITTED` is `true`. `req diff` passes the constant.
- **Cost.** Rows are built with `view::load_rows` per line per side, which reloads the corpus each time: one detector pass and
  one load per line per side. A corpus with many lines pays for each; the loader is the cost, not the detector.
- **Verified.** The three falsification-driven mutations each fail a test: ignoring the base side (falsification 1, and the
  list-from-head test), reading `.req/variants` per side (the list-from-head test and the malformed-list test), flagging
  regardless of admission (the admission seam test).

