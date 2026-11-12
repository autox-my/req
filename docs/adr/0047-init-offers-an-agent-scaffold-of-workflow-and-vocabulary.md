# 0047. `req init` offers an agent scaffold of workflow and vocabulary, and only those

Extends ADR 0012 (`init` resolves no corpus) and ADR 0029 (the agent rules), and amends `req init`'s documented
contract. No `RULES_V` bump, no address moves, no Baseline change: `init` still writes authored fields only, and nothing
it writes is derived (ADR 0022).

**Status: proposed.** The owner ratifies it by merging the pull request that carries it. Raised from a comparative
review of an adopter-surface tool (reqord, `f043777`) that solves the same problem this repository solves internally
with `AGENTS.md` and `CONTEXT.md`, and solves it for adopters. Date proposed: 2026-11-12.

## Context

`req init` writes exactly three things — `claims/`, `aliases/subjects.yml`, and `claims/README.md` with one claim
template — with exit **0** created, **1** refused, **2** usage, **3** UNVERIFIABLE, and a rollback that removes only what
the run created (`src/init.rs`).

An adopter's agent therefore gets a corpus and no rules. The workflow rules — one claim per file, tests first, never
edit a pin to fit a parse, stage by name, nothing leaves the machine, never push `main` — live in this repository's
`AGENTS.md`, and the vocabulary in its `CONTEXT.md`. For an adopter they exist only as prose in `docs/adopting.md`
§ *For an agent*, which is read once and then not carried by the session.

A comparable tool ships a persisted context directory (`product.yaml`, `technical.yaml`, `domain/*.md`) for exactly this
cost. The problem is right and the contents are wrong: product vision and tech stack are not requirement meaning, req
would never check them, and hosting them would make this a second wiki with a requirements engine attached.

## Decision

1. **Two files, and only two.** `req init --agent` additionally writes `AGENTS.md` — the workflow rules above, as
   instructions to the adopter's agent — and `CONTEXT.md` — the six glossary terms (*Claim*, *Requirement*, *View*,
   *Variant*, *Decision*, *Baseline*) with empty definitions for the adopter to fill.
2. **No product, technical or domain context.** req stores no vision, no stack, no security policy, no API standards.
   Those belong to the adopter's repository, where their own tools can read them, and where no requirement address
   depends on them.
3. **Templates are versioned and re-renderable.** `req init --agent --check` re-renders both templates to stdout and
   diffs against the files on disk: **0** current, **1** drifted or edited, **3** cannot judge. It writes nothing and
   records nothing.
4. **Drift of an adopted document is not a policy.** The answer is printed by the check, never folded into
   `PolicyResult`: the fold has exactly three outcomes (ADR 0023), and an advisory fourth for a cosmetic concern would
   dilute the one thing the gate is for.
5. **The refusal contract is unchanged.** Existing paths are still refused with exit 1, and `--agent` adds its files to
   the same `Created` ledger, so a failure mid-run removes only what this run created.
6. **The flag is opt-in.** Plain `req init` stays three paths. The quickstart rehearsal in `tools/quickstart.sh` and its
   `QUICKSTART <n>s` claim rehearse the plain form, so their timing statement stays true without re-measuring.
7. **The templates are the contract, not a copy of this repository's files.** They carry the rules an adopter's agent
   needs to operate a corpus, and deliberately not the rules of developing req itself.

## Consequences

- An adopter's agent starts with the rules that keep a corpus honest, instead of re-deriving them from a page.
- `CONTEXT.md` in an adopter's repository starts empty and stays the one home for vocabulary, as it is here.
- Two more tracked files exist in an adopter's repository that req can read but does not own. Nothing in the gate
  depends on them being present, current, or absent.
- `docs/adopting.md` § *For an agent* gains the flag; the five-step checklist is unchanged, because the scaffold points
  at it rather than restating it.

## Rejected

- **Writing `CONTEXT.md` with this repository's glossary populated.** An adopter's domain differs. A copied glossary is
  how a declaration starts to read as a proof.
- **Hosting `product.yaml` / `technical.yaml` / `domain/`.** The contents belong to another tool's problem; req would
  store facts it never checks, which is the "docs are claims too" failure with no claim.
- **A `req check` policy for template drift.** It would add a fourth fold outcome for a file req does not own.
- **Making `--agent` the default.** It changes every existing `init` test, the documented three-path contract, and the
  quickstart's timing claim, for a benefit only agent-operated corpora receive.
- **Generating the scaffold from `AGENTS.md` at runtime.** A template that a build reads from the source tree ties a
  shipped behaviour to a file that is edited for other reasons.

## Falsification

This decision is wrong if any of these is observed:

1. `--agent` writes anything beyond `AGENTS.md` and `CONTEXT.md`;
2. a re-render with no edit reports drift, or an edit reports current;
3. drift makes `req check` fail, or adds a policy result to the fold;
4. an existing path is overwritten, or a rollback removes a file the run did not create;
5. the plain `req init` output, exit codes or quickstart timing change as a side effect.
