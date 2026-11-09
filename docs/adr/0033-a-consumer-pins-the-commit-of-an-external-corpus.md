# 0033. A consumer pins the commit of an external corpus: `req pin`

**Status: proposed** — the owner ratifies it by merging the pull request that carries it; issue #16 (opened from an adopter
evaluation whose operating policy lives in a separate repository). Date proposed: 2026-11-09. **Extends [ADR 0012](0012-runtime-corpus-root.md)**
(the corpus root is a runtime parameter) and reuses the git boundary of [ADR 0026](0026-req-diff-what-changed-in-meaning-between-two-revisions.md);
adds a command and changes no existing contract, no `RULES_V`, no address.

## Context

ADR 0012 lets the corpus root be anywhere, so a repository can be gated against a corpus kept in another one: the policy repository
owns the claims, and the code repository's CI runs `req --root ../policy gate …`. Nothing said **which commit** of that corpus a
green gate was about. A consumer's CI that checks out the policy's `main` is green today and red tomorrow without the consumer
changing a line, and a green run does not say which policy it passed. Nor is there a signal when the policy moves on.

## Decision

1. **A pin file in the consumer's repository.** Two lines, `repo: <label>` (optional, printed as given) and `rev: <40 hex>`, with
   blank lines and `#` comments allowed and anything else refused. The `rev` is a **full, lower-case commit SHA**: a short SHA is
   ambiguous, and a branch or tag moves, which defeats a pin. The file lives wherever the consumer keeps it (the examples use
   `corpus.pin`); `req` is told its path.
2. **`req pin --verify <file>`** says whether the corpus at the resolved root *is* that commit: `HEAD` equals the pin, and `claims/` and
   `aliases/` hold no uncommitted or untracked change (a gate would judge something other than the pin; the registry can move many
   addresses). It prints `corpus pinned: <repo> @ <sha>` first, so the consumer's gate log names the policy it judged. Exit 0
   verified; **1** the corpus is not the pin (ahead, behind or diverged, said with the counts) or is dirty; **3** UNVERIFIABLE.
3. **UNVERIFIABLE (3) is "it cannot be told"**: the pin file is missing, unreadable or malformed; the pinned SHA is not in the
   checkout, or is an object that is not a commit (a tree, or a tag object); `git` is missing or failing; the corpus is not inside
   a git work tree. A pin that cannot be established is never a pass.
4. **`req` never fetches.** Getting the commit into the checkout is the consumer's CI (`git clone`/`git fetch` and `git checkout
   <sha>`), as it already installs everything else. `pin` only refuses to let a corpus that is not the pin be taken for it. This keeps
   the core free of a network client (REQ-0012) and puts credentials for a private policy repository in the CI that already holds them.
5. **`req pin --drift <file> --against <rev>`** is the drift report: exactly `req diff --base <pin> --head <rev>` in the corpus's
   repository, introduced by a `pin drift:` line. Each changed claim gets its class and flags, identical claims are counted, and it is a
   description, never a verdict (exit 0 when produced, 3 when it cannot be). It does not need the checkout to be at the pin. `--against`
   is required: "the source's main" is a fact about the consumer's remote, which req does not look up; the CI fetches and passes
   `origin/main`.
6. **The gate itself is untouched.** The "gate's first line names the pin" is met by running `req pin --verify` before `req gate` in the same
   job: a failed verify stops the job before any gate runs. A `--pin` flag on `gate`, and a key in the `req.gate/2` document, were
   considered and left out (below).

## Consequences

- A consumer's CI gains two steps (fetch the pinned commit; `req pin --verify`) and the pin file moves only in a reviewed commit of the
  consumer. Moving the pin is a pull request that can carry the `pin --drift` output as its description.
- A single pin per invocation: one corpus root, one pin file. A consumer gated against two corpora runs `pin --verify` twice.
- The pin is **not signed**. It is a statement in the consumer's repository, reviewed like any other file there; the policy's own
  Baseline (ADR 0010) remains the policy's attestation, and `req baseline --verify` against the pinned checkout is how a consumer
  checks it.
- `diff`'s limits apply to `--drift`: both revisions must be in the checkout (a shallow clone needs `fetch-depth: 0`), and only
  `claims/` and `aliases/` are read.

## Rejected

- **`req` fetching the pinned commit.** A network client in the core, and the credentials of a private policy repository in the one tool
  that must work air-gapped.
- **A branch or tag as the pin.** It moves; the point is that it cannot.
- **Accepting a short SHA.** Ambiguous; it would also resolve differently as the repository grows.
- **A `--pin` flag on `gate`.** It would run a git process inside the verdict command and add a key to `req.gate/2`, whose consumers pin the
  document's shape. Composing two commands in CI keeps both contracts as they are.
- **A lockfile of several corpora.** No consumer has two yet; one pin file per root is enough until one does.
