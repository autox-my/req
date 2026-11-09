# Worker brief — pointer-only template

The brief is a pointer, not a restatement. The worker reads the spec, `AGENTS.md` and nothing
else it is not pointed at.

```text
TASK        <task id, e.g. M1-1-fix>
SPEC        docs/specs/<file>.md
BRANCH      <branch point sha>  → work on feature/<name> in your own worktree
REPORT      docs/specs/task-envelope.schema.json  (check: python3 tools/orchestration/validate_envelope.py)
RED         <test id | n/a: reason>
VERIFY      scripts/check.sh   (full run before every commit; --fast only while iterating)
FORBIDDEN   paths outside the spec's allowed list
NOTE        <only facts not in the spec>
```

Rules:

- `RED` names the test the unit must turn green, proven red at the base before the fix is
  written, or it says `n/a: <reason>` (a docs-only unit). It is never blank. The same evidence
  goes in the envelope's `red`; the mutation checks go in `mutations`.
- Do not restate goal, scope, acceptance or context from the spec: that is duplicate tokens and a
  second source of truth.
- `NOTE` is for facts the spec cannot know: a reviewer's finding, a changed base commit.
- The report contract is a link, never an inlined schema.
- A worker that writes pushes its branch and opens a pull request (`gh pr
  create` against `autox-my/req`); it never pushes `main`, and the human merges. The envelope's
  `pr` carries the URL.
- A reviewer makes no edits, no commits and no worktree; its envelope's `task_id` contains
  `-review-`, `files_changed` lists what it flagged (may be empty), `committed` is `false`, and
  `summary` carries the findings (at most 10 lines, severity first).
