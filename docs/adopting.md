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
req gate             # exit 0, "verdict: PASS — every policy settled in the affirmative, and every claim was decided"
```

If either is not exit 0, the corpus is not yours yet — report it with the command and the exit code
(bottom of this page).

## Adopting from documents you already have

A project usually starts with prose: a `spec.md`, a backlog, a capabilities table. `req init` gives
you a corpus, not those requirements, and a tool that guessed `modality`, `value` or `operator` from
a sentence would be the one path that can enshrine a contradicted parse. What *is* honest to draft
is the part the claim format already sanctions: an **`unstructured: true` claim** — the statement,
its provenance (the document **and the line**), and every structured slot absent.

req ships no importer and no `adopt` subcommand (issue #44): the mapping from a document to claims
is per-project. `tools/adopt.py` in req's repository is therefore a **starting point to copy and
adjust**, not an interface to depend on — copy it out of req's checkout at the rev you installed and
change it to fit your document.

```bash
# from the corpus root (the directory holding claims/), with adopt.py copied into your project
python3 tools/adopt.py --from docs/capabilities.md --subject system             # dry run
python3 tools/adopt.py --from docs/capabilities.md --subject system --write     # the drafts
req canon                # exit 0, "discrepancy flags: 0" — an unstructured draft is legal as written
req canon --fill         # derives canonical.form and address
req canon --verify       # exit 0 — the drafts are complete claims
```

**The loop is the point.** The drafts are *unfilled* on purpose — `canonical.form` is left as the
empty placeholder and `address` is `null` — because only the derivation writes those fields, and a
draft that carried a derived value would be an address over a parse nobody authored. After
`--fill` a draft is a complete, legal claim, and it is **still a proposal**: the human decides
whether to leave it qualitative or to author the slots that give it structure (ADR 0004). The tool
never writes a slot, never derives an address, and never decides whether a bullet is one
requirement or three. A draft is legal *only because it is unstructured* — the format's marker for
prose that has not been parsed, not a claim whose parse was skipped.

Two conventions are built in, and neither is a heuristic:

| `--convention` | what one claim is |
| --- | --- |
| `bullets` (default) | each markdown list item (`- `, `* `, `1. `) under `--under`, or in the whole document: the item's text, trimmed, its wrapped lines joined onto one line |
| `req-lines` | each line matching `- REQ-<n>: <text>`: the text after the colon. The source's id is **ignored** — req allocates `REQ-NNNN` — and the report says so |

`--under <heading>` limits the run to one section (a heading's subsections are under it).
`--subject <id>` is required and is the whole document's subject — one decision per document, not
per claim — and it is written verbatim into each claim: an unregistered surface stays literal in
the parse, so aliasing it is a later, deliberate act. `--date` is the rationale date; without it the
source document's **last git commit date** is used, and with neither the run is a usage error
(exit 2) — a date is authored, never invented, exactly as `req baseline --at` is.

**Every skipped line is named with its reason and counted.** A line that looks like a candidate and
is not drafted — an empty item, a `req-lines` item with no id, an item outside the named heading —
appears in the report as `skip <doc>#L<n> <reason>`, and the summary line says how many. A
requirement silently dropped at adoption is the failure this project exists to prevent, so the
report is the reviewable output.

The defaults are the safe direction: a **dry run** prints each draft, the file it would write
(`claims/REQ-NNNN.yml`, the next free number) and the skips, and writes nothing; `--write` writes
them. It never overwrites — an existing path is exit **1**, nothing written — and a document already
drafted into the corpus is refused (exit **1**) with the claims it produced named: draft that
document again only after deliberately deleting them, or copy the script and adjust it. Exit **0**
drafted (dry or written), **1** a definite problem, **2** usage, **3** the document, the corpus, the
subject registry or `req` cannot be read — no `claims/` means run `req init` first.

## The document your project can commit

A `spec.md` is usually a second source of truth: it drifts, and a "generated, do not edit" comment
with nothing behind it proves nothing. req makes the document a **projection** instead (ADR 0018) —
generated from the corpus, committed as an artefact, and proven current by regeneration:

```bash
req render --check docs/spec.md   # the gate: 0 current, 1 stale or hand-edited, 3 cannot judge
req render --out docs/spec.md     # the remedy: regenerate, then commit the result
```

`--check` regenerates the projection and compares **bytes**; it writes nothing. A **1** means the
corpus moved or the file was edited — the remedy is the same either way: change the corpus or
regenerate, never hand-edit. A **3** means it cannot judge: the file is not a projection (no
header), another build wrote it (`rules_v` differs, so its addresses mean something else — the
same rule as a Baseline's `rules_v` mismatch), its selection cannot be reproduced, or the corpus
cannot be read. Running `--out` first and then `--check` proves generation is reproducible, but the
**gate is `--check` alone**: `--out` before it would repair the staleness it is supposed to report.

The first line of a committed projection is a machine-readable header naming the `generator`,
`rules_v`, selection, applying-claim count and an `sha256:` identity over the sorted applying
addresses, so a reader can see what it was built from before running anything. It is not the check
— the corpus is the oracle, and the header alone agreeing while the body differs is exactly the
failure case `--check` exists to catch.

Pin the paths your project commits in **`.req/projections`** — one repo-relative path per line,
`#` comments, blank lines skipped, the same grammar as `.req/variants` — and run
`req render --check` for each in your CI job. A listed path that is missing is a failure (a
projection that vanished is silent decay), and a list that checked nothing is not a pass. The list
is deliberately small: a projection is committed where it is a **deliverable**, not as a cache of
every view, and adding a path is a decision. A `spec.md` cannot go stale by construction, because
the only way to change it is to change the corpus — which has its own gates (`req canon --verify`,
`req gate`, `req baseline --verify`).

## The authoring loop and its traps

Every command's `--help` carries its own `exit:` line — the codes that command answers,
including the tri-state — and `req <command> --help` works before a corpus exists.

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

## Sign the Baseline

`req baseline --at <ts>` prints the canonical manifest (envelope v3, ADR 0016) — the exact bytes a
signature covers — and **req never signs**: the signature is the human's act, and a key the tool could
reach would prove nothing (ADR 0010). A manifest with no valid signature over it is *not yet a Baseline*,
and verification says so.

```bash
req baseline --at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > baselines/release.json
                                                # the timestamp is YOUR assertion; the manifest goes to
                                                # stdout, and the resolved `corpus root:` line to stderr
minisign -Sm baselines/release.json             # signs it; prompts for the secret key's passphrase
req baseline --verify baselines/release.json    # [VERIFIED] once signed, "not yet a Baseline" before
```

- **`--at` is required and is the signing time.** It goes *inside* the signed bytes, so it cannot be
  backdated without breaking the signature. Req never invents one: emitting without `--at` is a usage
  error (exit 2). A v3 manifest also attests the Decision log's chain head, and emission refuses (exit 3)
  when that log is absent, unparsable or tampered — a head that attests nothing must not be signed.
- **`-Sm` is `-S` (sign) with `-m <manifest>`**, and its default output name is `<manifest>.minisig` —
  exactly the file `--verify` looks for. Name it otherwise with `-x <path>` and pass `--signature <path>`.
- **Sign the bytes `req baseline` emitted.** Do not reformat the file: the signature covers those bytes,
  and verification compares them to the corpus as well.
- **The secret key is yours and is not in the repository.** minisign reads `~/.minisign/minisign.key` by
  default, or `-s <secret key>`; `req` has no flag for it and needs none.
- **`signed_at` judges the key, never the claims.** A key whose validity ends **at or after** the signing
  time verifies for that Baseline even when today is later; a key that had already **expired** at the
  signing time fails (it was not valid when the signature was made), and a key **revoked after** the
  signing time no longer invalidates it — revocation is prospective. Whether a claim is *current* remains
  `canon --verify`'s and `audit`'s business, which re-derive addresses. `--now <date>` is the explicit
  override for the judgement time; it is a verification flag, not an emission one.
- **`baselines/trust.yml` must carry the matching public key**, or verification is UNVERIFIABLE — an
  unknown key is never a pass. Rotate by adding a key beside the old one with its own validity window
  (`valid_from`, `valid_until`, `revoked_at`): old Baselines stay verifiable against the old key, and a
  key revoked at or before a Baseline's signing time FAILS.
- **An edit after signing is FAIL, not drift**: verification re-derives every address, so a claim edited
  once the manifest was signed is the same class as a hand-edited digest. A v3 manifest also FAILs when
  the Decision log no longer re-derives the chain head it records; an absent log is UNVERIFIABLE, never
  FAIL.
- **A `rules_v` mismatch is UNVERIFIABLE** with the re-sign remedy: an address is derived from the rules
  version, so the comparison is void rather than failed. A v2 manifest still parses and is reported as
  *predates … re-issue and re-sign* — re-emit with `--at` and sign again.

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
      - name: the release gate — every declared variant, and the default when none is declared
        run: |
          set -eu
          # An absent, unreadable, or non-regular variants file must be red. A failed INPUT
          # REDIRECTION does not stop a `set -e` script: the loop then never runs and the
          # fallback below gates the bare variant, so the step can be green with no declared
          # variant gated. `[ -f ]` proves a regular file, not readability; `[ -r ]` proves
          # readability, not that a directory is not the path (a readable directory passes it
          # and the redirection still fails) — so both are asserted.
          [ -f .req/variants ] && [ -r .req/variants ] \
            || { echo "missing, unreadable, or not a regular file .req/variants" >&2; exit 1; }
          # A job summary above the log (runner >= 13.2 sets one of these).
          # Absent variable = an older runner: the step must still pass, so the target is /dev/null.
          summary="${FORGEJO_STEP_SUMMARY:-${GITHUB_STEP_SUMMARY:-/dev/null}}"
          printf '## req gate\n\n| variant | exit | verdict |\n| --- | --- | --- |\n' >> "$summary"
          rc=0
          gate() {
            out="$(req gate "$@" 2>&1)" && code=0 || code=$?
            verdict="$(printf '%s' "$out" | sed -n 's/^verdict: //p' | head -1)"
            printf '| `%s` | %s | %s |\n' "${*:-<none>}" "$code" "${verdict:-no verdict line}" >> "$summary"
            # Fail outranks unverifiable, and nothing non-zero may be swallowed. The rule: 1 is
            # sticky above every code; among the rest the **first** non-zero is kept. The first
            # failed invocation is the one to investigate, and ranking 2/3/127 by severity would be
            # this script inventing an order req's exit contract does not define (127 is the
            # shell's, not req's). The previous rule kept the *last*, so a missing binary (127)
            # followed by an UNVERIFIABLE (3) reported 3 — the tool never ran.
            if [ "$code" -eq 1 ]; then
              rc=1
            elif [ "$code" -ne 0 ] && [ "$rc" -eq 0 ]; then
              rc=$code
            fi
          }
          gated=0
          # `|| [ -n ... ]`: `read` returns non-zero on a final line with no trailing newline while
          # still assigning it, and a release appended without one would otherwise be silently
          # dropped. `read -a` splits on IFS and runs no pathname expansion, so a whitespace-only or
          # comment-only line yields no element and is skipped rather than becoming a bare run.
          while IFS= read -r variant_line || [ -n "${variant_line}" ]; do
            variant_line="${variant_line%$'\r'}"
            read -r -a release <<< "${variant_line%%#*}"
            [ "${#release[@]}" -gt 0 ] || continue
            # /dev/null: nothing in the body may consume the remaining releases from this
            # file's stdin — a command that reads stdin would shorten the list without error, and
            # the loop would end early with a green step. scripts/check.sh guards the same loop
            # the same way.
            gate "${release[@]}" < /dev/null
            gated=$((gated + 1))
          done < .req/variants
          # A comment-only file declares one variant — the default — and the bare invocation is
          # its gate. With a line declared, the loop is the gate: a bare run beside it reports
          # UNVERIFIABLE on any corpus carrying a selector (ADR 0015), and a false red beside
          # green variants is how a real red gets ignored.
          if [ "$gated" -eq 0 ]; then
            gate
          fi
          exit $rc
```

**`.req/variants`** is plain text beside the corpus, because the CI step is bash and YAML would need
a parser: one gate invocation per line, `key=value` pairs space-separated, `#` starts a comment, and
a line that is empty after stripping the comment is skipped. Two rules matter:

- **No release line declares the default variant.** A comment-only `.req/variants` describes exactly
  one variant — the default — so the step runs the bare `req gate` only when the loop read no line.
  Once a line is declared, **the loop is the gate**: a bare run beside it is UNVERIFIABLE on any
  corpus carrying a selector (ADR 0015), and a false red beside green variants is how a real red
  gets ignored. The unscoped variant is still not a line — a blank line cannot be distinguished from
  formatting, and a format where "nothing" means "the default variant" is a trap.
- **The file must be a readable regular file.** `[ -f ]` **and** `[ -r ]` are asserted, because an
  absent, unreadable, or non-regular file makes the loop's input redirection fail *without*
  stopping a `set -e` script — every declared variant would be skipped, the fallback would gate the
  bare variant, and the step would still exit 0. `[ -f ]` alone proves a regular file, not
  readability; `[ -r ]` alone also passes for a readable *directory*, whose redirection still
  fails. While you declare no variants, the file is valid with comments only.

One part of the snippet is not verified on a runner: whether the job summary renders above the log.
The shell logic was run locally against this exact text — an absent, unreadable, or non-regular
`.req/variants` → exit 1 (`missing, unreadable, or not a regular file .req/variants`); a comment-only file → one row, the bare default variant, and exit 0 **on a
corpus with no selector** (a fresh `req init` corpus; on a corpus with a selector the bare run is
UNVERIFIABLE, which is the right answer and not a mistake in the step); one declared variant → one
row and exit 0, with **no** bare row beside it; a release appended without a trailing newline → still
one row per release; a whitespace-only line → skipped, never a bare run; a variant the corpus does
not describe → a row reading `3 | UNVERIFIABLE` and exit 3; no summary variable → no output and
exit 0.
Confirm the table renders on your first pull request — if it does not, the step is still correct, it
just loses the summary.

## The coverage rule

`req gate` checks the **applying claims of its invocation**. A claim the invocation leaves
**undecided** is not examined by it — and since ADR 0015 that is a failure, not a disclosure: the
gate exits **3 (UNVERIFIABLE)** and names the claims it could not examine, with the remedy. A bare
`req gate` is therefore UNVERIFIABLE on any corpus carrying a selector, and the per-variant
invocation is the only one that can be green. So:

**Every variant the project declares gets its own gate invocation.** That is what `.req/variants`
is for. When you add an `applicability:` predicate to a claim, add its variant to `.req/variants`
in the same commit — otherwise the claim is inside no gate invocation at all, and the bare run tells
you so: it exits 3 and names that claim among the undecided.

## Without CI

Nothing about the tool changes; only whether it is run. The gate becomes a ritual, and a ritual that
is not enforced is not a gate. The cheapest enforcement is a `pre-push` hook that runs
`req canon --verify` and then the same loop over `.req/variants` shown above — on a corpus carrying
a selector the bare invocation is UNVERIFIABLE (ADR 0015), so the loop *is* the gate. Do not
oversell it: a local hook is a reminder, not a record, and nothing outside your machine can see
whether it ran.

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
