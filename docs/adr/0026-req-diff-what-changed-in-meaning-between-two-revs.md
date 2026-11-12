# 0026. `req diff` reports what changed in meaning between two revisions, offline, and can fail a pull request on it

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-05, former Forgejo PR #125); issue #123
(from the architecture review, finding F8). Date proposed: 2026-11-05. Adds a command; changes no existing
contract, no `RULES_V`, no address.

Amended by [ADR 0041](0041-a-condition-added-to-an-obligation-weakens-it.md) (§2): a guard added to an obligation or a
prohibition is `weakened` and a guard removed is `tightened`, the reverse of what §2 says below; a guard changed on a
permission is `reshaped`.

Extended by [ADR 0045](0045-req-diff-names-the-contradictions-a-change-introduces.md): a `contradictions:` section and the flag `contradiction-added <other-id>` (with `--fail-on contradiction-added`), judged per line of the head's `.req/variants`.

Amended by [ADR 0044](0044-a-change-on-several-axes-has-a-direction-only-when-they-agree.md) (§2): a change on several axes is `weakened` or `tightened` only when every axis that moved agrees, and is `reshaped` otherwise.

## Context

The review's strategic finding: the wedge is a check a team can run on a pull request, not a new authoring
format. req already answers the parts of that question one at a time. `req compare` classifies the direction
between two claim *files* (modality weakening, numeric loosening, scope widening…); `req canon` shows what a
statement derives and whether each pin holds; `req gate` and `req check` judge a *state* of the corpus. Nothing
answers the question a reviewer actually has on a pull request: **across this change, which obligations moved,
in which direction, and did anything get quietly weaker?** Today that is a human reading a YAML diff, which is
the failure the asymmetric-cost rule exists to prevent: a weakening reported as no change.

## Decision

1. **`req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]` exists.** It compares
   the corpus (the directory holding `claims/` and `aliases/`, ADR 0012) at two states: `--base` is a git
   revision; `--head` is a git revision, or the working tree when absent. It reads each side without checking
   anything out, derives each claim under **this binary's rules** and **that side's own registry**, and reports.
   It is read-only: it writes nothing into the corpus, signs nothing and decides nothing.
2. **Claims are matched by `claim_id`** (ADR 0001): the id is the identity, the address is a fingerprint of the
   current meaning. Each claim id present on either side gets exactly one **class**:
   - `added`, `removed` — the id exists on one side only;
   - `weakened` — the statement's meaning moved so that strictly more complies (`Direction::is_loosening`:
     modality weakening, numeric loosening, scope widening);
   - `tightened` — the reverse (modality strengthening, numeric tightening, scope narrowing);
   - `reshaped` — a different obligation rather than a stronger or weaker one (polarity flip, scope
     divergence, unrelated subject or predicate): a human must read it;
   - `not-judged` — the meaning changed but one side has no structured form, so no direction is claimed;
   - `reworded` — the statement text changed and the meaning fingerprint did not;
   - `metadata` — only fields outside identity changed (relations, rationale, tests, evidence, pins);
   - claims identical on both sides are counted, not listed.
   Beside its class a claim carries **flags**: `pins-changed` (the authored pins differ — an edited pin is
   how a flagged parse gets silenced, AGENTS.md), `pin-flagged` (a pin on the head side disagrees with the
   parse), `tests-lost` (a linked test id present at base is absent at head), `tests-added`, and
   `same-address-as <id>` (an added claim has the meaning fingerprint of a removed one: a candidate renumber,
   reported as a candidate and never inferred as a rename, ADR 0017 §2). A registry change is reported once, at
   the top (`registry: changed`), because it can move many addresses at once and the reader must not mistake it
   for many edited statements.
3. **The report is the answer; the exit is not a verdict.** With no `--fail-on` the exit is **0** whatever the
   report lists, **3** when it cannot be produced (UNVERIFIABLE: `git` missing, the corpus not inside a git work
   tree, a revision that does not resolve, a side whose claims do not load or whose registry is unreadable,
   including a claim declaring a file format this build does not write), and **2** for a usage error. The output
   never says PASS or FAIL: this is a description of a change, not a release verdict (`req check` is that).
4. **`--fail-on` turns named classes into exit 1.** Its value is a comma-separated subset of `weakened`,
   `removed`, `reshaped`, `not-judged`, `pins-changed`, `pin-flagged`, `tests-lost`; any other spelling is exit 2,
   and an empty list is exit 2. Exit **1** if any claim carries a named class or flag, else the exit of §3.
   Nothing is on by default: a team chooses which movements block a merge, the way `canon --fail-on-flag` is
   explicit (issue #106). An unlisted class is still printed.
5. **Offline, and git only as a local process.** The core has no git library and gains none (ADR 0009); the
   command runs the local `git` binary as a process boundary, with its environment cleaned of `GIT_*`
   redirections, no user or system configuration and no hooks, and reads trees and blobs only — no fetch, no
   network. A missing `git` is exit 3, never a silent empty diff.
6. **JSON is `req.diff/1`.** `schema`, `generator: "diff"`, `rules_v`, `base` and `head` (the label given, the
   resolved full SHA, or `working-tree`), `registry_changed`, `fail_on`, `triggered` (the classes and flags that
   matched), counts per class, and `claims[]` with `claim_id`, `class`, `flags`, both addresses and, for a changed
   meaning, the direction word `req compare` prints. It is a stdout view only; text and JSON carry the same
   classes.

## What stays outside req

Posting a comment, labelling a pull request or reading a forge's API is the CI step's, not the engine's; the
command is the one call such a step makes. req still never merges, ratifies or approves: `weakened` is a prompt
for a human, the same way a `compare` classification is.

## Consequences

- A reviewer (or a CI step) can see on one screen that REQ-0004 went from SHALL to SHOULD, that a pin was edited
  in the same change, and that a linked test disappeared, instead of finding it in a YAML hunk.
- The direction classifier is now exercised on whole-corpus changes, so a class it cannot decide shows up as
  `not-judged` rather than as a quiet "changed".
- Cost: one more command contract, a `git` process dependency for this command only, and a classification that
  is exactly as good as `req compare`'s (Tier-1 structure), which the documentation says in the same breath.

## Rejected

- **Two directories instead of revisions.** Simpler and git-free, but every CI would need to materialise the
  base tree itself, and the common case (this PR against its target branch) would be a shell recipe again.
  Reading revisions directly makes the one call the whole recipe.
- **A verdict (PASS/FAIL) from `diff`.** Whether a weakening is acceptable is the owner's decision; the command
  makes it visible and lets a repository say which classes block (§4).
- **Matching by address or by text similarity.** A renumbered claim would look like a removal plus an addition
  and a heavily reworded one like a different claim; the id is the identity, and a shared address is reported
  only as a candidate.
- **Posting to the forge from req.** A network client and a token inside the engine.
