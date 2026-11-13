# 0047. `req init` offers an agent scaffold of the snippet it already ships, and only of that

Extends ADR 0012 (`init` resolves no corpus) and amends `req init`'s documented contract. No `RULES_V` bump, no address
moves, no Baseline change: `init` still writes authored fields only, and nothing it writes is derived (ADR 0022).

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Raised from a comparative
review of an adopter-surface tool (reqord, `f043777`) that persists an agent context directory (`product.yaml`,
`technical.yaml`, `domain/*.md`). Revised three times after review (PR #66): decision 3 judged a file the scaffold tells
the adopter to fill; decision 5 shut out a repository that already has an `AGENTS.md`; and the context premise was wrong
— the rules are **already shipped** in `examples/agents-snippet.md`, not only as prose in `docs/adopting.md`. Date
proposed: 2026-11-12; revised 2026-11-13.

## Context

`req init` writes exactly three things — `claims/`, `aliases/subjects.yml`, and `claims/README.md` with one claim
template — with exit **0** created, **1** refused, **2** usage, **3** UNVERIFIABLE, and writes that go through exclusive
creation so a path planted between the check and the write is never truncated (`src/init.rs:252`).

An adopter's agent therefore gets a corpus and no rules, and has to be told to paste them. **The rules already exist in
one authored place**: `examples/agents-snippet.md` is "the invariant set from req's own agent rules, cut to what an
adopter's agent needs", linked from `README.md:143` and `docs/adopting.md:1441`. So this decision is not about inventing
agent guidance — it is about the paste. A scaffold that shipped a second copy of those rules would create two homes for
one rule, against AGENTS.md's own "each rule has one home".

A comparable tool ships a persisted context directory for the same cost. The problem is right and the contents are
wrong: product vision and tech stack are not requirement meaning, req would never check them, and hosting them would
make this a second wiki with a requirements engine attached.

## Decision

1. **One authored home, and it is the snippet.** `examples/agents-snippet.md` stays the single home of the agent rules.
   `req init --agent` writes **that block**, wrapped in markers — it does not restate, paraphrase or extend it.
2. **Two files, and only two.** `AGENTS.md` (the marked block) and `CONTEXT.md` (the six glossary terms — *Claim*,
   *Requirement*, *View*, *Variant*, *Decision*, *Baseline* — with empty definitions).
3. **No product, technical or domain context.** req stores no vision, no stack, no security policy, no API standards.
   Those belong to the adopter's repository, where their own tools can read them.
4. **Adoption is recorded by the block, and the block is what `--check` judges.** The block is delimited by
   `<!-- req:agent:begin -->` and `<!-- req:agent:end -->`. `req init --agent --check` diffs **only that region** against
   the current snippet: **0** current, **1** drifted or edited, and **3** when the markers are absent — a repository that
   never adopted the scaffold has not drifted from it, and a foreign `AGENTS.md` is not a defect. Nothing outside the
   markers is ever read or judged. `CONTEXT.md` is never judged: it ships empty precisely so the adopter fills it, and a
   check that reddened on the instruction it gave would be worse than no check.
5. **An existing `AGENTS.md` is extended, not refused.** With no markers, the block is **appended** and the rest of the
   file is left byte-for-byte alone; the run refuses (exit 1, naming the file) only when the markers are already present,
   because that repository has adopted the scaffold and `--check` is the tool from then on. Appending never truncates,
   never rewrites an existing byte, and a symlinked `AGENTS.md` is refused rather than written through — the guarantee
   exclusive creation gives a new file (`src/init.rs:252`), stated for the append path.
6. **An existing `CONTEXT.md` is left untouched and the run says so.** It is the adopter's glossary; requiring them to
   delete it before running `init --agent` would be worse than leaving it. `CONTEXT.md` is created only when absent, by
   exclusive creation, and the run prints the one line naming the file it left alone.
7. **The snippet carries no literal `rules_v`.** It currently says `rules_v 9` (line 7), which would make `--check`
   answer 1 after every `RULES_V` bump — a red caused by the build moving, not by the adopter. It becomes "the `rules_v`
   your project pins, read from `req --version`". `docs/adopting.md` and `README.md` link the snippet once and do not
   restate it.
8. **Drift of an adopted block is not a policy.** The answer is printed by the check, never folded into `PolicyResult`:
   the fold has exactly three outcomes (ADR 0023), and an advisory fourth for a cosmetic concern would dilute the one
   thing the gate is for.
9. **The flag is opt-in.** Plain `req init` stays three paths. The quickstart rehearsal in `tools/quickstart.sh` and its
   `QUICKSTART <n>s` claim rehearse the plain form, so their timing statement stays true without re-measuring.
10. **The templates are the contract, not a copy of this repository's files.** The block carries the rules an adopter's
    agent needs to operate a corpus, and deliberately not the rules of developing req itself.

## Consequences

- An adopter's agent starts with the rules that keep a corpus honest, in the file it already reads, with no paste.
- `CONTEXT.md` in an adopter's repository starts empty and stays the one home for vocabulary, as it is here.
- A repository that already has an `AGENTS.md` keeps every line of it; the block is added between markers and is the only
  part req will ever look at.
- Two more tracked files exist in an adopter's repository that req can read but does not own. Nothing in the gate
  depends on them being present, current, or absent.
- `docs/adopting.md` § *For an agent* and its "exactly three files" line (`:1416`), and the `req init --help` page, change
  in the same commit that lands the flag.

## Rejected

- **A second copy of the rules inside `init`.** Two homes for one rule is the failure AGENTS.md names; the snippet is
  already the home and is already tested by `tests/agent_handoff.rs`.
- **Refusing an existing `AGENTS.md` (this ADR's second version).** It shuts out the repository most likely to use the
  flag, and with no marker it leaves `--check` answering 1 for ever on a file req never wrote.
- **A recorded adoption state file.** The markers already record adoption, in the file the adopter's agent reads; a
  second state file could disagree with it.
- **Writing `CONTEXT.md` with this repository's glossary populated.** An adopter's domain differs. A copied glossary is
  how a declaration starts to read as a proof.
- **Hosting `product.yaml` / `technical.yaml` / `domain/`.** The contents belong to another tool's problem; req would
  store facts it never checks, which is the "docs are claims too" failure with no claim.
- **A `req check` policy for template drift.** It would add a fourth fold outcome for a file req does not own.
- **Making `--agent` the default.** It changes every existing `init` test, the documented three-path contract, and the
  quickstart's timing claim, for a benefit only agent-operated corpora receive.

## Falsification

This decision is wrong if any of these is observed:

1. `--agent` **additionally** writes anything beyond `AGENTS.md` and `CONTEXT.md` (init always writes its three paths);
2. a re-render with no edit reports drift, or an edit inside the markers reports current;
3. an edit to `CONTEXT.md` is reported as drift — the check would punish the fill-in its own scaffold asked for;
4. a foreign `AGENTS.md` with no markers reports 1 or 0 instead of 3;
5. appending changes a byte outside the markers, truncates the file, or follows a symlink to write through it;
6. an existing `CONTEXT.md` is overwritten, or its presence makes the run fail;
7. the adopted block differs from `examples/agents-snippet.md`, or `--check` answers 1 after a `RULES_V` bump;
8. drift makes `req check` fail, or adds a policy result to the fold;
9. the plain `req init` output, exit codes or quickstart timing change as a side effect.
