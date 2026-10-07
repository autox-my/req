# Git is transport and serialization, not identity or merge

Git-native storage is table stakes and worth having: it gives atomic co-evolution of spec
and code (`git checkout` moves both), offline and air-gapped operation for free, and
tamper-evidence. But git's identity is *bytes* and its merge is *line-based* — both wrong
for natural language, where "shall" becoming "may" is a one-word diff and a reworded
requirement is a false conflict. We keep the repository as a deterministic serialization
of the claim graph — one claim per file — and merge in the graph, never with `git merge`
over requirement text.

## Considered Options

- **Requirements as ordinary Markdown merged by git** — the Doorstop/StrictDoc path.
  Rejected: line merge is semantically blind, so meaning is reconciled by hand.
- **A database with git export** — rejected: breaks atomic spec/code coupling, which is
  the one thing git storage does better than anything else.

## Consequences

A reader who expects `git merge` to reconcile requirements is wrong, and this must be
documented or someone will "fix" it back. Commits carry baselines; they do not carry the
identity model.
