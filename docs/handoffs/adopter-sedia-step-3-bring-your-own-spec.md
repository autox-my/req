# Handoff to sedia's agent — adopt `docs/capabilities.md` into a req corpus (draft only)

Prepared 2026-11-04 by req's orchestrator thread from `examples/handoff-brief.md`. Hand the block below to the
coding agent working in the **sedia** repository. It carries one step and stops; the human ratifies. The
step, its commands and its deciding lines are in req's `docs/adopting.md` § *For an agent* (steps 1-4) at
the pinned rev; `req --help` is the command reference. sedia's `AGENTS.md` should already hold the block
from `examples/agents-snippet.md`; if it does not, add it first and say so in the report.

```text
STEP        Adopting req, "Bring your own spec" with the install and init that precede it, ending at
            "Ratify: read the parse": get docs/capabilities.md into a req corpus as DRAFT claims and
            show their derived parse. Do NOT gate, link tests, write pins, render, or sign a Baseline.
REQ PIN     1e5fa60. Precheck, before any other command: `req --version` must show
            claim_schema req.claim/2, rules_v 6 and built from 1e5fa60. If it does not, stop and
            report what it shows. Do not install, rebuild or work around it. (The human installs it:
            docs/adopting.md § Install req.)
INPUT       docs/capabilities.md. Its nine claims are the rows of the "## The claims" table, ids
            REQ-0001 .. REQ-0009, statement in the second column.
DONE WHEN   `req canon` exits 0 and its last line begins `claims: 9 (`; claims/REQ-0001.yml ..
            claims/REQ-0009.yml exist and each one's claim_id equals the id in its table row.
MAY NOT     the prohibitions in examples/agents-snippet.md (req's repo). In particular: no `pins:`
            (a human pins a bound after reading the parse), no edit of a statement to make its parse
            nicer, no `req gate`, no Baseline. An exit 3 is not a pass: report it.
COMMANDS    `req --help` lists them; `req <command> --help` is the contract and its `exit:` line.
REPORT      every req command you ran, its exit code, its first and last output line; the dry run of
            the extractor in full; any exit 3 verbatim; the 9 id->statement pairs as drafted; then
            stop. Do not start the next step.
```

## What the agent needs to know that the generic template cannot say

These are the points where sedia differs from `docs/adopting.md`'s example. Each came from reading
`docs/capabilities.md` and `req init` against `tools/adopt.py`, not from guessing.

1. **The source is a table, and `tools/adopt.py` has no table convention.** It reads `bullets` and
   `req-lines` only. `adopting.md` says to copy the tool out of req's checkout at the pinned rev and adjust
   it: copy it to sedia's `tools/adopt.py` and add a third convention (for example `table`: each row
   under `## The claims` whose first cell is `**REQ-<nnnn>**`; the statement is the second cell). Follow
   sedia's TDD rule (`AGENTS.md`): write the failing fixture test first, with an independent oracle (the
   nine rows counted by hand), and mutation-check it. Do not edit req's copy.
2. **Keep sedia's ids.** sedia's documents cite REQ-0001 .. REQ-0009, and `adopt.py` ignores source ids
   and allocates the next free `REQ-NNNN`. `req init` writes a **starter claim `claims/REQ-0001.yml`** (a
   placeholder about a health check; its rationale source is `docs/requirements.md`, which sedia does not
   have). Delete that one file after `req init` (keep `claims/README.md` and `aliases/subjects.yml`), so
   drafting starts at 0001 (rehearsed in a scratch corpus: the first draft is REQ-0001). Until the first
   draft exists `req canon` on the emptied corpus exits 3, which is correct. Then make the new convention **assert** that each drafted number equals the
   id in its row and fail (exit 1, nothing written) if they differ. A silent renumbering would break every
   cross-reference in sedia's documents.
3. **Strip the markup, keep the words.** A cell like `The copilot **shall read** every submitted
   document before judging it.` becomes `The copilot shall read every submitted document before judging
   it.` Remove only `**` and the surrounding table pipes and spaces; do not paraphrase, shorten or fix
   wording (REQ-0006's four sources and REQ-0009's two halves stay as written). The statement is the
   source of truth and its parse is derived, so the wording is the human's.
4. **The columns `shape` and `status` are not part of the claim.** They are provenance for the human; do
   not copy them into the claim file. The rationale source is `docs/capabilities.md#L<n>` (the tool
   records the line) and the date is the commit date of that file or the explicit `--date` you are given.
   Statuses of "not implemented" or "partly" are expected and are not a defect at this step.
5. **Run a dry run first** and put it in the report; `--write` only if the dry run shows nine drafts, no
   skips you cannot explain, and ids 0001..0009. The tool never overwrites and refuses a document already
   drafted.

## After the report (the human's, not the agent's)

Read the nine derived parses in `req canon`'s output. Where a statement derives something the author did
not mean, re-word the statement. Where a bound is load-bearing (REQ-0006's four sources, for example) add
a pin deliberately, and never edit a pin to fit a parse. Giving each claim a test link, a first
`req gate`, and a Baseline are separate later steps; sedia's own document defers the Baseline until
someone outside asks to prove a rule has not changed, and that deferral stands.
