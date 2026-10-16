# Adopting req

req is a requirements engine where a **claim** — one obligation, one file — is the unit, and a
claim's identity is a content address over its canonical meaning. This page is the whole
adoption path: install it, start a corpus, learn the two traps in the authoring loop, and gate
every variant in CI.

The commands below were run in a scratch corpus: the install form at `2afbd68`, and `req init` with
a build of the revision that introduced it.

## Install req

req is deliberately unpublished (`publish = false`), so **the rev is the provenance**: install it
at a pinned commit, never from a version number.

```bash
GIT_SSH_COMMAND="ssh -o ControlMaster=no -o ControlPath=none -o StrictHostKeyChecking=accept-new" \
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo install \
  --git ssh://git@git.xantu-barb.ts.net/fdx/req.git \
  --rev <PIN> --root "$HOME/.local" --locked
```

`<PIN>` is the merge SHA of the req commit you depend on — read it with
`git rev-parse --short origin/main`; do not copy a SHA out of any document. This exact form was
measured at `2afbd68`: exit 0, and the installed binary reports `rules_v 5`.

Three things make it work, and each one is a failure without it:

| piece | what fails without it |
| --- | --- |
| `ssh://git@host/path.git`, not the scp-style `git@host:path` | cargo rejects the scp form with `relative URL without a base` |
| `CARGO_NET_GIT_FETCH_WITH_CLI=true` | libgit2 ignores `GIT_SSH_COMMAND` and insists on `known_hosts`; with the git CLI fetcher, the ssh options above apply |
| `--rev <PIN> --locked` | without `--rev` there is no provenance to name; `--locked` installs the committed `Cargo.lock`, which is part of the identity path |

**Air-gapped:** clone req once at the rev you want and install from the clone —
`cargo install --path . --locked`. req's core has no network dependency and providers are a local
process boundary (REQ-0012), so a corpus, a gate and a Baseline verification all work offline.

## Start a corpus

```bash
cd myproject
req init
```

Exit 0, and exactly three files:

| path | what it is |
| --- | --- |
| `claims/REQ-0001.yml` | a starter claim, already filled and verified — a real parse of its statement, with the derived `canonical.form` and `address` written by the same path `req canon --fill` uses |
| `claims/README.md` | the authoring loop and its traps, in your project's tree |
| `aliases/subjects.yml` | the subject registry: what your prose calls a thing, and the id its address covers |

`init` writes into the **current directory** — never an ancestor — so a project can start inside
another one. It refuses (exit 1) if `claims/` or `aliases/subjects.yml` already exists — or if `aliases`
is a symlink, which would put the registry outside the tree you asked for — because the
alternative is overwriting your corpus. It takes no arguments and **no `--root`** (exit 2): a
silently ignored root would create a corpus somewhere you did not ask for, so run it where you want
the corpus. It prints the next steps, including the CI file. Confirm what you got:

```bash
req canon --verify   # exit 0, "rules_v 5; verified 1, candidates 0, mismatches 0"
req gate             # exit 0, "verdict: PASS — every policy settled in the affirmative"
```

If either is not exit 0, the corpus is not yours yet — report it with the command and the exit code
(bottom of this page).

## The authoring loop and its traps

1. **Edit `statement`, then re-run `req canon --fill`.** The slots below the statement are its
   *authored parse*; `--fill` re-derives `canonical.form` and `address` and writes them into the
   anchor lines. A hand-authored numeric slot that disagrees with the parse is **refused** — exit 3,
   nothing written — because an address over a contradicted parse would enshrine the contradiction.
   Never hand-write `address` or `canonical.form`: a hand-written digest is a critical finding, and
   `req canon --verify` is what detects it.
2. **Values are canonical SI.** `within 200 ms` is `value: "0.2"`, `unit: s` — the exact decimal
   string the parser derives, not the units the prose used. `unit: ms` and `unit: seconds` are parse
   flags, and `"0.30"` is not a canonical decimal (`"0.3"` is).
3. **`req canon` always exits 0.** It reports the divergences; it never gates. The check that gates
   is `req canon --verify`: exit 0 verified, 1 a digest that does not recompute, 3 a
   re-canonicalization candidate (unfilled, or filled under other rules) — re-fill it, do not retype
   it.
4. **An empty `claims/` is UNVERIFIABLE (exit 3)** — for both `canon --verify` and `gate`. Zero
   claims cannot be told from every claim having vanished, so never "clean up" a corpus down to
   nothing; delete a requirement by deleting its claim, deliberately.

## CI

Save this as `.github/workflows/req.yml` (GitHub) or `.forgejo/workflows/req.yml` (Forgejo — Forgejo
reads `.github/workflows/` only while no `.forgejo/workflows/` exists). The summary step below
targets Forgejo's variable and GitHub's, and works with neither.

```yaml
name: req
on:
  pull_request:
  push:
    branches: [main]
jobs:
  requirements:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      - name: req, installed at a pinned rev
        env:
          GIT_SSH_COMMAND: "ssh -o ControlMaster=no -o ControlPath=none -o StrictHostKeyChecking=accept-new"
        run: |
          set -eu
          CARGO_NET_GIT_FETCH_WITH_CLI=true cargo install \
            --git ssh://git@git.xantu-barb.ts.net/fdx/req.git \
            --rev <PIN> --root "$HOME/.local" --locked
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
          req --version
      - name: every stored address still recomputes (variant-independent)
        run: req canon --verify
      - name: the release gate — the unscoped variant first, then every declared one
        run: |
          set -eu
          # An absent variants file must be red. A failed INPUT REDIRECTION does not stop a
          # `set -e` script, so the file's presence is asserted rather than assumed —
          # otherwise every declared variant is silently skipped and the step still exits 0.
          [ -f .req/variants ] || { echo "missing .req/variants" >&2; exit 1; }
          # A job summary above the log (runner >= 13.2 sets one of these).
          # Absent variable = an older runner: the step must still pass, so the target is /dev/null.
          summary="${FORGEJO_STEP_SUMMARY:-${GITHUB_STEP_SUMMARY:-/dev/null}}"
          printf '## req gate\n\n| variant | exit | verdict |\n| --- | --- | --- |\n' >> "$summary"
          rc=0
          gate() {
            out="$(req gate "$@" 2>&1)" && code=0 || code=$?
            verdict="$(printf '%s' "$out" | sed -n 's/^verdict: //p' | head -1)"
            printf '| `%s` | %s | %s |\n' "${*:-<none>}" "$code" "${verdict:-no verdict line}" >> "$summary"
            [ "$code" -eq 0 ] || rc=$code
          }
          gate
          while IFS= read -r line; do
            variant="${line%%#*}"
            [ -n "$variant" ] || continue
            # unquoted on purpose: one line is several key=value arguments
            # shellcheck disable=SC2086
            gate $variant
          done < .req/variants
          exit $rc
```

**`.req/variants`** is plain text beside the corpus, because the CI step is bash and YAML would need
a parser: one gate invocation per line, `key=value` pairs space-separated, `#` starts a comment, and
a line that is empty after stripping the comment is skipped. Two rules matter:

- **The unscoped variant is not a line.** `req gate` runs bare before the loop, because a blank line
  cannot be distinguished from formatting and a format where "nothing" means "the default variant"
  is a trap.
- **The file must exist.** `[ -f .req/variants ]` is asserted, because an absent file makes the
  loop's input redirection fail *without* stopping a `set -e` script — every declared variant would
  be skipped and the step would still exit 0. While you declare no variants, the file is valid with
  comments only.

One part of the snippet is not verified on a runner: whether the job summary renders above the log.
The shell logic was run locally against this exact text — an absent `.req/variants` → exit 1
(`missing .req/variants`); a comment-only file → one row and exit 0; the unscoped invocation plus
one declared variant → two rows and exit 0; a variant the corpus does not describe → a row reading
`3 | UNVERIFIABLE` and exit 3; no summary variable → no output and exit 0. Confirm the table renders
on your first pull request — if it does not, the step is still correct, it just loses the summary.

## The coverage rule

`req gate` checks the **applying claims of its invocation**. A claim the invocation leaves
**undecided** is not checked by it, and the gate still exits 0 — that is a fact of the tool, not a
bug, and it is why CI gates each variant separately rather than trusting one bare invocation. So:

**Every variant the project declares gets its own gate invocation.** That is what `.req/variants`
is for. When you add an `applicability:` predicate to a claim, add its variant to `.req/variants`
in the same commit — otherwise the claim is inside no gate invocation at all, and the only commands
that will tell you are `req select` and `req render`, which report the undecided claim and exit 3
while `req gate` stays green.

## Without CI

Nothing about the tool changes; only whether it is run. The gate becomes a ritual, and a ritual that
is not enforced is not a gate. The cheapest enforcement is a `pre-push` hook that runs the same two
steps — `req canon --verify` and `req gate` — and, if you declare variants, the same loop over
`.req/variants` shown above; a hook that runs only the unscoped variant has exactly the coverage gap
this page just described. Do not oversell it: a local hook is a reminder, not a record, and nothing
outside your machine can see whether it ran.

## Reporting a bug in req

Report it at <https://git.xantu-barb.ts.net/fdx/req/issues>. Include four things, because each one
changes the triage:

1. the claim `statement` you were authoring, or the corpus file if it is a parsing problem;
2. the exact command you ran;
3. its exit code and the output line that decided it;
4. `req --version` — `rules_v` decides whether a stored address is current, so a verdict is not
   reproducible without it.

A verdict has three states, always: **PASS**, **FAIL** and **UNVERIFIABLE**. A false merge (two
different requirements collapsing to one address) is the failure this tool exists to prevent, so
report it first; a false split is noise by comparison.
