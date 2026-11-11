# 0039. Structural completeness is reported, never asserted

Extends ADR 0023 (`req check` folds the repository's release lists) and ADR 0015 (an undecided claim is not a pass). Nothing
is superseded. No rules change, no claim-schema change, no Baseline envelope change.

**Status: proposed** — the owner ratifies it by merging the pull request that carries it. Asked for by the owner on 2026-11-11
("do req help to address incomplete requirements and silent omissions; if not should we?" → "completeness checks"). Date
proposed: 2026-11-11. This ADR decides the contract; the code follows in its own pull request, test first.

## Context

req already refuses most **silent** omissions inside one invocation: an undecided claim is listed, never dropped (REQ-0016), and
makes the gate UNVERIFIABLE (REQ-0024); a claim with no linked test is UNVERIFIABLE (REQ-0025); a relation to a claim the corpus
does not hold is UNVERIFIABLE in `req trace`; a statement word the rules cannot place is a lint, not a dropped word (rules v6).

What no command answers is the question **across** invocations and **across** artefacts. Running req on its own repository on
2026-11-10 showed three omissions that every individual check passed over:

1. **A claim in no Baseline.** `baselines/air-gapped.json` records the selection `deployment_mode=air_gapped`, under which
   REQ-0008, REQ-0009, REQ-0010 and REQ-0018 are undecided. They are therefore in no signed manifest — REQ-0008 is the project's
   headline claim (FMR = 0) — while `req audit` prints `VERIFIED` and `req check` is green. The audit is right about what it was
   asked; nothing asks whether the signed set covers what the release lines ship.
2. **A claim examined by no release.** A claim whose predicates no `.req/variants` line satisfies is examined by none of them.
   Today only a repository test (`tests/release_gate.rs::every_claim_applies_in_at_least_one_declared_release`) pins this, and
   only for req's own corpus; an adopter has no check.
3. **A claim with no relation.** 15 of req's 39 claims (REQ-0025…REQ-0030, REQ-0032…REQ-0040) neither refine nor derive from
   anything, and nothing refines them. Some are legitimate top-level obligations; some are probably a missing `derives-from`.
   Nothing says which, because nothing lists them.

**What req cannot do.** "Complete" in the regulated sense is complete *against something*: a hazard analysis, the stakeholder
needs, the clauses of a standard. req holds none of those, so it has no oracle, and a tool that printed "requirements complete"
would be overclaiming in exactly the way `AGENTS.md` forbids. This ADR is limited to omissions req can **prove structurally**
from files it already reads.

Prior art, read at source on 2026-11-11: OpenFastTrace lets each item declare the artefact types that must cover it
(`Needs: dsn, itest`) and reports shallow and deep coverage (`doc/user_guide/reference/xml_tracing_report.md`); Doorstop's
validation warns when "a normative, non-derived item in a child document has no links" (`docs/cli/validation.md`). Both treat a
missing link as a *reported* finding and leave the judgement to the author. req follows that, with its own three-state rule.

## Decision

1. **`req check` gains a `completeness` step**, folded like every other step (ADR 0023). It judges the repository, not one
   selection, because each finding below is a property across release lines, Baselines and claims. It reads only what `req check`
   already reads: `claims/`, `.req/variants`, `baselines/*.json` (the manifests; signatures stay `req baseline --verify`'s answer).
2. **Three findings, each naming the claim:**
   - **`in_no_release`** — the claim applies (`Yes`) in no `.req/variants` line. *Answer: FAIL.* A claim nothing examines is the
     silent omission ADR 0015 exists to prevent, moved one level up. A claim intended for no release is retired, not left.
   - **`in_no_baseline`** — the claim applies in at least one release line and is in no committed Baseline manifest. *Answer:
     UNVERIFIABLE.* It may be new since the last signing, which is normal between releases; it is not a defect, but it is not
     ratified either, and it is named.
   - **`unrelated`** — the claim has no outgoing and no derived incoming relation. *Answer: reported only*, a finding line that
     does not change the step's verdict, unless the repository opts in (decision 3). A top-level obligation is legitimate; the
     list is what lets a reviewer tell the two apart.
3. **Opt-in strictness, declared in a file req already owns.** A new optional file `.req/completeness` lists, one per line,
   the claim ids that are **roots** (allowed to have no parent) and the findings to promote: `strict unrelated` turns every
   unlisted relation-less claim into FAIL. Grammar as `.req/variants` and `.req/projections` (`#` comments, blank lines skipped,
   trailing CR stripped). An unknown word or an id the corpus does not hold is a usage failure naming the line, never ignored.
   An absent file means: no roots declared, nothing promoted.
4. **The audit packet prints the same findings** for its selection, in a section of their own after *Undecided*, so the auditor
   reading `req audit` sees what the signed set leaves out. The packet's exit class is unchanged by `unrelated`; `in_no_baseline`
   is already its `uncovered`.
5. **Wording.** No output says "complete". The step is named `completeness` and its pass line says
   `no structural omission found (3 checks)` — what was checked, not what is true.

## Not decided here (each its own ADR if wanted)

- **Tests no claim links to** (the reverse direction of ADR 0019). Most unit tests are legitimately unlinked; it needs a
  declared scope (a test-id glob) to mean anything.
- **External sources no claim cites** — every object of an imported ReqIF or Sphinx-Needs export (ADR 0027/0028 keep the
  object id in `rationale.source`) answered by a claim. It needs the export kept as a committed input.
- **Required coverage kinds per claim** (OpenFastTrace's `Needs:`), which belongs with design satisfaction, not here.
- **Contradictions between claims** (`docs/design.md` §4 pain 8). A different question: two claims that cannot both hold.

## Consequences

- req's own repository goes red on adoption unless it acts on finding 1: REQ-0008/9/10/18 are in no Baseline. The fix is the
  owner's (a Baseline per release line, or one whose recorded selection decides all four keys), a re-signing, which is why the
  step lands as UNVERIFIABLE (exit 3) rather than FAIL for this finding.
- The 15 relation-less claims are listed on every `req check`; the owner either adds the missing relations or declares the roots.
- `tests/release_gate.rs::every_claim_applies_in_at_least_one_declared_release` becomes a test of the product, not only of this
  corpus; it stays as the dogfood pin.
- The fixtures in `tests/baseline.rs` and `tests/projection.rs` that run `scripts/check.sh` need no new stub: the step lives
  inside `req check`, not in the script.

## Falsification

This decision is wrong if any of these is observed:

1. a claim applying in no release line passes `req check`;
2. a claim in a release line and in no Baseline is reported PASS, or FAIL;
3. a relation-less claim changes the verdict without `strict unrelated`, or a declared root is reported under it;
4. an unknown word, or an id the corpus does not hold, in `.req/completeness` is ignored;
5. any output of this step contains the word "complete" as a verdict.
