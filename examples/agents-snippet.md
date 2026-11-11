# Agents snippet

Paste the block below into your project's `AGENTS.md` (or whatever instruction file your agent
reads). It is the invariant set from req's own agent rules, cut to what an adopter's agent needs.

```markdown
- Use the `req` on `PATH` only after `req --version` shows `claim_schema req.claim/2` and `rules_v 9` (and `built from` the rev your project pins). Anything else is an older or newer build: stop and report it; do not work around it.
- To find a command, run `req --help`; `req <command> --help` is that command's contract, with its `exit:` line (both work with no corpus). Act on the exit code and the named line, not on prose. The loop is `docs/adopting.md` § *For an agent*.
- The claim is the unit: one obligation per file under `claims/`, one file per claim.
- A claim is authored (`req.claim/2`): its parse, canonical form and address are derived from `statement` at load and stored nowhere — never add `address`, `canonical` or a parse slot to a claim file.
- `pins:` states what the parse must be where it is load-bearing; a pin agrees or is a flag. Never edit a pin to match a parse the statement was not meant to have — re-word the statement, or leave the flag for a human.
- `req canon` shows the derived parse and each pin's verdict and does not fail on a flag: it is diagnostics, never a pass.
- `req check --test-report <junit.xml>` before a merge (repeat `--test-report` once per report when the tests write several — nextest, pytest, `tools/junit_case.py`; the gate judges their union): it runs one `req gate` per `.req/variants` line and one `req render --check` per `.req/projections` path and folds them into one exit (0 pass, 1 defect, 3 unverifiable, 2 malformed list). Read its step lines: a green `check` ends `verdict: PASS — every declared step ran and passed`. A single selection is `req gate key=value ...` (e.g. `req gate deployment_mode=air_gapped`), whose first line names what it examined — every applying claim decided, 0 undecided.
- A bare `req gate` exiting 3 on a corpus carrying a selector is correct, not a bug: the claims it left undecided were not examined.
- A committed projection is checked, never edited: `req render --check docs/spec.md`, and regenerated with `req render --out docs/spec.md`.
- A claim whose meaning changes moves its address, and a signed Baseline over the old address goes stale — sign deliberately.
- An exit 3 is not a pass: report what could not be decided and ask a human, and never relax a check to land a change.
- Run, before merging: `req canon`, and `req check --test-report <junit.xml>` (`pins_hold` fails on a flagged pin). Until every claim has a linked test the gate cannot be green; use `req canon --fail-on-flag` meanwhile (exit 1 on a flagged pin, not a release verdict) and move to the gate when the last claim is linked.
- Never break this one rule: no stored or hand-written `address` or `canonical.form` anywhere in a claim file.
```
