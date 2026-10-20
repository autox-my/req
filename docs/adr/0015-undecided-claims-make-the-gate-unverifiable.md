# 0015. An undecided claim makes the release gate UNVERIFIABLE

**Status: accepted** — proposed by the agent, ratified by the maintainer on 2026-10-20. The decision answers review finding [F5]. Date proposed: 2026-10-20.
 The decision answers review
finding [F5]. Date proposed: 2026-10-20.

## Context

`req gate` answers one question — *may this variant be released?* — and it keeps the tri-state honestly
almost everywhere: a claim the variant **excludes** is not the variant's problem, and an undecided claim
is never treated as absent. The banner says so on every run:

```
# Release gate: 15 applying claim(s), 6 undecided (an undecided claim is never treated as absent)
```

But the **exit code does not say so**. Measured on our own corpus before `.req/variants` existed: a bare
`req gate` printed the line above and exited **0**. The six variant-scoped claims (REQ-0008/9/10/11/12/18)
were never examined, and no caller could tell that run from one with nothing undecided. `req select` and
`req render` exit **3** on the same input; the gate alone treats "I could not decide" as a pass.

That is not a theory in this repository: the hole it describes was real until this week. `scripts/check.sh`
never invoked `req gate` at all, and the reason it took a review to notice is that a `gate` run saying
PASS-with-six-unexamined looks exactly like a gate run saying PASS.

The disclosure is honest and useless to a machine. A CI job keys on the exit code; a human reading a
banner is not a check. The asymmetry the rest of the project keeps — FAIL outranks UNVERIFIABLE, an
undecided claim is never absent — argues that the gate's own answer should not be "pass".

## Decision

A bare `req gate` whose invocation leaves **any claim undecided** exits **3 (UNVERIFIABLE)**, with the
undecided claims named and the remedy stated (pass the variant context that decides them). The exit
contract becomes:

| the invocation | exit |
|---|---|
| every claim decided, every policy settled in the affirmative | 0 |
| any claim undecided — the variant does not decide what it claims to release | 3 |
| a policy settled negatively (a defect) | 1 |
| usage error | 2 |

A context that decides every claim is unaffected: `.req/variants` in this repository declares two
releases, each `20 applying claim(s), 0 undecided`, so the gate's own step stays green.

## Consequences

- **The banner stops being the only signal.** `scripts/check.sh` gates per declared release and each line
  is complete, so the repository's gate is unchanged in behaviour and stronger in meaning: the lines it
  runs are now the *only* way to get a 0.
- **A bare `req gate` on this corpus becomes exit 3** (currently 0 with six undecided). That is the
  decision working, and it is worth stating plainly because it changes what a human sees when they run
  the command by hand.
- **Adopters must pass a context.** `docs/adopting.md` already tells them to gate per variant, and it
  gains the sentence that a bare `gate` is UNVERIFIABLE on any corpus with a selector. The friction is
  the kind ADR 0006 sanctions: it belongs at the decision boundary, not in a silent skip.
- **The sedia hand-off pack needs one edit.** `prompts/pack-sedia.md:199` (in that repository) emits
  `scripts/req-gate.sh`, whose first line is `req gate || rc=$?` — before the per-variant loop. Under
  this decision that reports a false UNVERIFIABLE for a corpus whose variants are all green. That pack
  has not been executed yet, so the edit lands before it can bite. The loop *is* the gate; the bare run either goes, or its status is
  discarded deliberately with a comment saying why. This is a good forcing function: the pack was written
  from the old contract, and nothing else would have found that as cheaply.
- Consistent with `selectors_confirmed`, which already refuses an invocation that decided nothing: the
  gate does not clear what the model does not describe. This extends the same rule from "nothing was
  decided" to "something was not".
- **Falsification:** on the committed corpus, `req gate` with no context exits 3 and names the six
  scoped claims; `req gate <either line of .req/variants>` exits 0 with `0 undecided`; and the full gate
  (`scripts/check.sh`) stays green. If a bare `gate` can still exit 0 while the banner counts an
  undecided claim, this ADR has not landed.

## Alternatives rejected

- **Keep exit 0 with the disclosure** (status quo): the disclosure is not a signal, and this repository
  has already paid for the difference once — six claims unexamined in CI, found by a review rather than
  by the gate.
- **An opt-in `--strict`**: a flag no shipped script sets is the status quo with a longer command line.
  If the answer is right, it should be the answer; if it is wrong, a flag would hide that.
- **Exit 0 but print the undecided claims to stderr**: a machine still reads the exit code, and the
  banner already names the count. This changes nothing that was wrong.
