# Agents snippet

Paste the block below into your project's `AGENTS.md` (or whatever instruction file your agent
reads). It is the invariant set from req's own agent rules, cut to what an adopter's agent needs.

```markdown
- The claim is the unit: one obligation per file under `claims/`, one file per claim.
- A claim's structured slots are an authored parse of `statement`; never edit the parse to match a flag — re-word the statement, or leave the flag for a human.
- Only `req canon --fill` writes the derived `canonical.form` and `address` (and `req init`, for a new corpus's starter claim); nothing else may write or edit them.
- `req canon` reports and always exits 0: it is diagnostics, never a pass.
- `req canon --verify` before a merge: exit 0 verified, 1 a stored digest that does not recompute, 3 a candidate — re-fill it, never retype it.
- `req gate key=value ...` before a merge (one invocation per `.req/variants` line, e.g. `req gate deployment_mode=air_gapped`), and read its first line: a green gate names what it examined — every applying claim decided, 0 undecided.
- A bare `req gate` exiting 3 on a corpus carrying a selector is correct, not a bug: the claims it left undecided were not examined.
- A committed projection is checked, never edited: `req render --check docs/spec.md`, and regenerated with `req render --out docs/spec.md`.
- A claim whose meaning changes moves its address, and a signed Baseline over the old address goes stale — sign deliberately.
- An exit 3 is not a pass: report what could not be decided and ask a human, and never relax a check to land a change.
- Run, before merging: `req canon`, `req canon --verify`, and `req gate <key=value ...>` once for every line of `.req/variants`.
- Never break this one rule: no hand-written `address` or `canonical.form` digest anywhere.
```
