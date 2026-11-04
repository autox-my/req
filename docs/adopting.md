# Adopting req

req is a requirements engine where a **claim** — one obligation, one file — is the unit, and a
claim's identity is a content address over its canonical meaning. This page is the whole
adoption path: install it, start a corpus, learn the two traps in the authoring loop, and gate
every variant in CI. An agent follows the same path: read it once, then use **For an agent** (near the end)
as the checklist — the five steps, what the agent may not do, what it can read, and the honest limit
of a test link. A command's exit codes live on its own `req <command> --help` page, not here.

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
measured at `2afbd68`: exit 0, and the installed binary reported `rules_v 5` (a build at rules v6 reports `rules_v 6`; the exit code is the measurement).

Three things make it work, and each one is a failure without it:

| piece | what fails without it |
| --- | --- |
| `ssh://git@host/path.git`, not the scp-style `git@host:path` | cargo rejects the scp form with `relative URL without a base` |
| `CARGO_NET_GIT_FETCH_WITH_CLI=true` | libgit2 ignores `GIT_SSH_COMMAND` and insists on `known_hosts`; with the git CLI fetcher, the ssh options above apply |
| `--rev <PIN> --locked` | without `--rev` there is no provenance to name; `--locked` installs the committed `Cargo.lock`, which is part of the identity path |

After the install, put `$HOME/.local/bin` on `PATH` (a fresh shell does not have it, and without it the next
step's `req init` is `command not found`) and run `req --version` (exit 0). Two of its lines decide what
the binary is: **`rules_v`** makes a stored address current — an address is derived under a rules
version, so a verdict is not reproducible, and a Baseline not comparable, under another one — and
**`built from`**, the commit the binary came from: a bare SHA means *clean*, `-dirty` means tracked files
were modified, so the binary is not the commit it names. This is the **only networked step**; everything
below runs offline against the corpus (REQ-0012).

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
| `claims/REQ-0001.yml` | a starter claim in `req.claim/2`: its statement, provenance and `pins:` (the bound its statement must parse to) — nothing derived; the loader derives its parse and address every time it is read (ADR 0022) |
| `claims/README.md` | the authoring loop and its traps, in your project's tree |
| `aliases/subjects.yml` | the subject registry: what your prose calls a thing, and the id its address covers |

`init` writes into the **current directory** — never an ancestor — so a project can start inside
another one. It refuses (exit 1) if `claims/` or `aliases/subjects.yml` already exists — or if `aliases`
is a symlink, which would put the registry outside the tree you asked for — because the
alternative is overwriting your corpus. It takes no arguments and **no `--root`** (exit 2): a
silently ignored root would create a corpus somewhere you did not ask for, so run it where you want
the corpus. It prints the next steps, including the CI file. Confirm what you got:

```bash
req canon            # exit 0; the derived parse, and "pins: 3 on 1 claim(s), 0 flagged"
req gate             # exit 3 — the starter claim links no test yet (ADR 0019); see "Link each claim…"
```

If `req canon` is not exit 0, or flags a pin, the corpus is not yours yet — report it with the command and
the exit code (bottom of this page). `req gate` exits **3** here by design, not by accident: the
starter claim is machine-evidenced (the default) with no link and no report, an unexercised policy is
never a pass, and it has no process declaration to fall back on. The section *Link each claim to its proving test* is what turns it green.

## Adopting from documents you already have

A project usually starts with prose: a `spec.md`, a backlog, a capabilities table. `req init` gives
you a corpus, not those requirements. Since `req.claim/2` (ADR 0022) a claim's parse is **derived**
from its statement at load and nothing derived is stored, so what is honest to draft is exactly what
a human authors first: the statement, its provenance (the document **and the line**, recorded
relative to the corpus root), and empty `applicability` and `relations` — no slot to guess, nothing
to fill, and no pin.

req ships no importer and no `adopt` subcommand (issue #44): the mapping from a document to claims
is per-project. `tools/adopt.py` in req's repository is therefore a **starting point to copy and
adjust**, not an interface to depend on — copy it out of req's checkout at the rev you installed and
change it to fit your document.

```bash
# from the corpus root (the directory holding claims/), with adopt.py copied into your project
python3 tools/adopt.py --from docs/capabilities.md --date YYYY-MM-DD           # dry run
python3 tools/adopt.py --from docs/capabilities.md --date YYYY-MM-DD --write   # the drafts
req canon                # exit 0 — the parse each draft derives, structured or not
```

**The loop is the point.** A draft is a complete, legal claim the moment it is written, and it is
**still a proposal**: the human reads the parse `req canon` shows, re-words a statement whose parse
is not what was meant, adds the applicability and relations it needs, and pins a load-bearing bound
(`pins:`) where there is one (ADR 0004). The tool never writes a pin, never derives anything, and
never decides whether a bullet is one requirement or three. A structured `shall` and a prose-only
sentence are drafted alike — the difference is in the derived parse, which is req's to compute.

Two conventions are built in, and neither is a heuristic:

| `--convention` | what one claim is |
| --- | --- |
| `bullets` (default) | each markdown list item (`- `, `* `, `1. `) under `--under`, or in the whole document: the item's text, trimmed, its wrapped lines joined onto one line. A list item inside a fenced code block is code, never a candidate |
| `req-lines` | each line matching `- REQ-<n>: <text>`: the text after the colon. The source's id is **ignored** — req allocates `REQ-NNNN` — and the report says so |

`--under <heading>` limits the run to one section (a heading's subsections are under it, and a
heading inside a fenced code block is not a heading).
There is no `--subject` (exit 2 if given): the subject is derived from each statement against
`aliases/subjects.yml`, and an unregistered surface stays literal in the parse, so aliasing it is a
later, deliberate act. `--date` is the rationale date; without it the
source document's **last git commit date** is used, and the report names the commit it borrowed.
A document with **uncommitted changes** has no commit that describes its text, so that run is a
usage error asking for `--date`; with no commit at all it is the same. A date is authored, never
invented, exactly as `req baseline --at` is.

**Every skipped line is named with its reason and counted.** A line that looks like a candidate and
is not drafted — an empty item, a `req-lines` item with no id, an item outside the named heading,
or a repeat of an earlier item — appears in the report as
`skip <doc>#L<n> <reason>`, and the summary line says how many. A
requirement silently dropped at adoption is the failure this project exists to prevent, so the
report is the reviewable output.

The defaults are the safe direction: a **dry run** prints each draft, the file it would write
(`claims/REQ-NNNN.yml`, the next free number) and the skips, and writes nothing; `--write` writes
them. It never overwrites — an existing path is exit **1**, nothing written — and a document already
drafted into the corpus is refused (exit **1**) with the claims it produced named: draft that
document again only after deliberately deleting them, or copy the script and adjust it. Only a
source carrying the extractor's own `<doc>#L<n>` shape counts as an earlier draft — `req init`'s
starter claim names `docs/requirements.md` as a pencil sketch, and that does not block adopting
from it. Exit **0** drafted (dry or written), **1** a definite problem, **2** usage (including a
date the tool will not borrow), **3** the document, the corpus, the subject registry or `req`
cannot be read — no `claims/` means run `req init` first.

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
the only way to change it is to change the corpus — which has its own gates (`req gate`,
`req baseline --verify`).

## The authoring loop and its traps

Every command's `--help` carries its own `exit:` line — the codes that command answers,
including the tri-state — and `req <command> --help` works before a corpus exists.

1. **Edit `statement`, then run `req canon` to see the parse.** Nothing derived is stored in a
   claim file (`req.claim/2`, ADR 0022): the parse, the canonical form and the address are computed
   every time the claim is loaded, so there is nothing to fill and nothing to hand-write. `req canon`
   prints, per claim, the derived structure, modality, subject, operator, value, unit and lints, and
   each pin's verdict.
2. **Pin what is load-bearing.** `pins:` (any of `modality`, `subject`, `operator`, `value`,
   `unit`) states what the statement must parse to. A pin agrees, or it is a flag — a
   *contradiction* (the parse says something else) or *not carried* (the parse holds no such
   value); there is no third category. A flagged pin moves no address; `req gate` fails on it
   (`pins_hold`, exit 1). A pin is **not attested by the Baseline**: deleting one after signing
   still verifies, so a pin is a check at gate time, not a signed promise.
3. **Values are canonical SI.** `within 200 ms` parses to `value "0.2"`, `unit s`: pin the exact
   decimal string the parser derives, not the units the prose used. `unit: ms` is a contradiction,
   and `"0.30"` is not a canonical decimal (`"0.3"` is), so it cannot be pinned.
4. **`req canon` never fails on a flag.** It reports (it exits 3 only when the corpus cannot be read); it never gates. The check that gates is `req gate`:
   the loop in one sentence is edit → `req canon` to see the parse → `req gate`. A claim's
   `applicability:` list is part of its address (since rules v5), so editing it moves the address
   exactly as editing the statement does.
5. **An empty `claims/` is UNVERIFIABLE (exit 3)** — for `canon` and `gate` alike. Zero claims
   cannot be told from every claim having vanished, so never "clean up" a corpus down to nothing;
   delete a requirement by deleting its claim, deliberately.
6. **A `req.claim/1` file is NOT JUDGED (exit 3).** Migrate it with `tools/migrate_claims.py` from
   req's repository — a dry run by default that prints every key it drops and every pin it creates;
   `--write` rewrites the file. A format bump moves no address.

## Sign the Baseline

`req baseline --at <ts>` prints the canonical manifest (envelope v4, ADRs 0016 and 0019) — the exact
bytes a signature covers — and **req never signs**: the signature is the human's act, and a key the tool could
reach would prove nothing (ADR 0010). A manifest with no valid signature over it is *not yet a Baseline*,
and verification says so.

Use `--out <file>`, not a shell redirect (`> file`): the shell truncates the target before `req` runs, so a
refusal (an older `req`, an absent decision log) leaves an empty file that `minisign -Sm` will sign without complaint.
`--out` writes a temp file and renames it, so a refusal leaves whatever was there, and exits non-zero.

```bash
req baseline --at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --out baselines/release.json
                                                # the timestamp is YOUR assertion; the file is written only
                                                # once the manifest is complete
minisign -Sm baselines/release.json             # signs it; prompts for the secret key's passphrase
req baseline --verify baselines/release.json    # [VERIFIED] once signed, "not yet a Baseline" before
```

- **`--at` is required and is the signing time.** It goes *inside* the signed bytes, so it cannot be
  backdated without breaking the signature. Re-emit with the current time just before you run
  `minisign -Sm`: copying an older manifest's time into a re-sign backdates it, and a key revoked in
  that gap would still verify. Req never invents one: emitting without `--at` is a usage
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
  `baseline --verify`'s and `audit`'s business, which re-derive addresses. `--now <date>` is the explicit
  override for the judgement time; it is a verification flag, not an emission one.
- **`baselines/trust.yml` must carry the matching public key**, or verification is UNVERIFIABLE — an
  unknown key is never a pass. Rotate by adding a key beside the old one with its own validity window
  (`valid_from`, `valid_until`, `revoked_at`): old Baselines stay verifiable against the old key, and a
  key revoked at or before a Baseline's signing time FAILS.
- **An edit after signing is FAIL, not drift**: verification re-derives every address, so a claim edited
  once the manifest was signed is the same class as a hand-edited digest. A v3-or-later manifest also
  FAILs when the Decision log no longer re-derives the chain head it records; an absent log is
  UNVERIFIABLE, never FAIL.
- **v4 attests the evidence declaration and its links; v3 does not.** A v4 manifest records each
  claim's `tests_hash`, so a `tests:` or `evidence:` edit after signing is **FAIL** — the same class as
  an edited address. A v3 manifest still
  verifies on its historical signed bytes and says **once** that it predates v4: its tests are *not
  attested*, which is not a failure, and a release that wants attestation re-emits and re-signs as v4.
- **A `rules_v` mismatch is UNVERIFIABLE** with the re-sign remedy: an address is derived from the rules
  version, so the comparison is void rather than failed. A v2 manifest still parses and is reported as
  *predates … re-issue and re-sign* — re-emit with `--at` and sign again.

## Link each claim to its proving test

A claim may carry `tests:` — the test ids that prove it. The list is **authored**, like its rationale,
and it is a claim about evidence: link a claim only to the test that demonstrably proves it. **A wrong
link is worse than no link** — no link is UNVERIFIABLE and says so, while a wrong link manufactures
confidence.

```yaml
tests:
  - "req::canon::a_rewording_keeps_its_address"
```

A test id is `classname::name` — the two attributes the report's `<testcase>` carries, joined by `::`
— so the `classname` may itself contain `::`. Read the id out of the report rather than retyping it:
an id compared exactly means a typo is "linked test not in the report", not a silent match.

- **It is outside identity.** Adding, removing or renaming a link moves no address — a test rename must
  never move a requirement. What moves instead is `tests_hash`, which the signed Baseline attests, so
  unlinking — or flipping the evidence mode — after signing is FAIL rather than a silent edit.
- **It is optional.** A **machine** claim with no `tests:` (or an empty list) simply links nothing; the
  gate names it, per claim. That is the flagship answer, not a parse error.
- **Leave unlinked what you cannot prove.** An unlinked claim is exit 3, never a PASS; an agent cannot
  make it go away without adding a link that survives review.

**Process evidence is a declaration, never a silence.** If the process itself establishes the claim — a
recorded Decision, a signature, a workflow — say so, and link nothing:

```yaml
evidence: process
tests: []
```

The gate reports it **PASS as declared**, names it (`* process-evidenced (declared, not machine-proven)`)
and counts it; it is never folded into the machine pass. A process claim must carry no links (naming both
is a load error, exit 3), and an empty `tests:` list on a machine claim is *no linked test* —
UNVERIFIABLE, never process by accident.

The report comes from **nextest** (ADR 0019) — adopted for the JUnit artefact, not for speed: measured
2026-10-21, a warm `cargo test` took 2 s and `cargo nextest run` 2–3 s, both paying the ~24 s link over
the same tests.

```bash
cargo install cargo-nextest --version 0.9.149 --locked   # pinned; verified at pin time
cargo nextest run --profile ci                            # writes target/nextest/ci/junit.xml
req gate --test-report target/nextest/ci/junit.xml deployment_mode=air_gapped
```

The profile is reviewed, not improvised (`.config/nextest.toml`):

```toml
[profile.ci]
fail-fast = false

[profile.ci.junit]
path = "junit.xml"
report-skipped = "ignored"
```

`report-skipped` belongs **under `[profile.ci.junit]`** — the top-level spelling is silently ignored in
0.9.149 and would drop every `#[ignore]`d test from the artefact — and its value is `"ignored"`, not
`"all"`, which would also list tests skipped by filters or partitions. `cargo test` is **replaced** by
this run, not kept beside it: one run, one truth. One gap, named rather than hidden: nextest does not
run doctests, so a `#[doc]` example needs its own step.

The `tests_linked` policy answers four ways over the applying claims, and there is no fifth:

| outcome | condition | fold |
|---|---|---|
| **PASS** | the claim links at least one test and at least one linked test **passed** | contributes pass |
| **FAIL** | every linked test is present in the report and **none passed** (a `skipped` test is not a pass) | exit 1 |
| **UNVERIFIABLE — *no linked test*** | the claim carries no `tests:` entry — the flagship answer, named per claim | exit 3 |
| **UNVERIFIABLE — *linked test not in the report*** | a linked test id has no testcase in the report (renamed or deleted) — never folded into PASS | exit 3 |

The four outcomes judge **machine** evidence: a claim declaring `evidence: process` is answered by its
declaration, which is a reported fact, not a fifth outcome.

An absent, unparsable or zero-testcase report is UNVERIFIABLE **once**, naming the reader's reason —
never once per claim. `FAIL` outranks any UNVERIFIABLE; both outrank PASS, and the exit code is the
existing fold. **A gate with no `--test-report` cannot exercise the policy and is UNVERIFIABLE (exit
3)** — unless every applying claim declares `evidence: process`, in which case nothing machine-evidenced
needs a report: a policy that cannot be exercised is not a pass, so the report is part of the gate
invocation, not an optional extra.

**The rename lesson.** A test id is the test's *identity*: rename the test and the claim points at an
id the report no longer carries. The answer is *linked test not in the report* — UNVERIFIABLE, exit 3 —
never a pass, because folding a rename into PASS is how a green build stops meaning anything. When you
rename or delete a test, edit its claim in the same commit; the gate is what tells you that you did
not.

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
      # The install fetches from req's **private** host, so the runner needs read access to it.
      # Set the `REQ_DEPLOY_KEY` secret to a key with read access (this step installs it); skip the
      # step when the runner already has one (a self-hosted runner), or install from a clone
      # instead (`cargo install --path . --locked`). req's own claim operations need no network.
      - name: the deploy key the private install needs, when the runner has none
        env:
          REQ_DEPLOY_KEY: ${{ secrets.REQ_DEPLOY_KEY }}
        run: |
          set -eu
          if [ -z "${REQ_DEPLOY_KEY:-}" ]; then
            echo "REQ_DEPLOY_KEY is not set — assuming the runner already has read access"
            exit 0
          fi
          # Per job, and removed by the cleanup step at the end: a key under `$HOME` survives on
          # a self-hosted runner, where the next job (an untrusted pull request included) could
          # read it. `umask 077` means the file is never briefly world-readable.
          umask 077
          printf '%s\n' "$REQ_DEPLOY_KEY" > "${RUNNER_TEMP:-$HOME}/req_deploy_key"
      - name: req, installed at a pinned rev
        run: |
          set -eu
          # The install reaches a private host. A key from `REQ_DEPLOY_KEY` (written by the step
          # above to a path of its own) is offered with `IdentitiesOnly`, so a self-hosted
          # runner's default identity is neither overwritten nor offered to this host by accident.
          ssh_command="ssh -o ControlMaster=no -o ControlPath=none -o StrictHostKeyChecking=accept-new"
          req_key="${RUNNER_TEMP:-$HOME}/req_deploy_key"
          if [ -f "$req_key" ]; then
            ssh_command="ssh -i $req_key -o IdentitiesOnly=yes $ssh_command"
          fi
          CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_SSH_COMMAND="$ssh_command" cargo install \
            --git ssh://git@git.xantu-barb.ts.net/fdx/req.git \
            --rev <PIN> --root "$HOME/.local" --locked
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
          # `$GITHUB_PATH` applies to *later* steps only; this step's own PATH is unchanged.
          export PATH="$HOME/.local/bin:$PATH"
          req --version
      # The tests that prove the claims (ADR 0019). This template is Rust and uses nextest because
      # it writes the JUnit/XUnit document `req gate --test-report` reads; on another stack, run
      # your own test command and point `--test-report` at the report it writes. A claim with no
      # linked test is a finding, never a pass — the gate below names it.
      # No stale report may answer for this run. `if: always()` is the point: when an earlier step
      # fails, every ordinary step after it is skipped — and on a cached or self-hosted workspace
      # yesterday's `target/nextest/ci/junit.xml` would then survive for the artifacts step below,
      # which would publish it as a `pass` beside a red build.
      - name: invalidate any stale test report
        if: always()
        run: rm -f "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml"

      - name: the tests that prove the claims, as the report the gate reads
        run: |
          set -eu
          cargo install cargo-nextest --version 0.9.149 --locked
          # The reviewed profile, only when the project does not already own one: fail-fast off,
          # so one failure cannot hide the rest, and the JUnit report with skipped tests present
          # (a skipped test must be visible — it is not a pass).
          if [ ! -f .config/nextest.toml ]; then
            mkdir -p .config
            cat > .config/nextest.toml <<'TOML'
          [profile.ci]
          fail-fast = false

          [profile.ci.junit]
          path = "junit.xml"
          report-skipped = "ignored"
          TOML
          fi
          # The profile and the report path are what `--profile ci` and `--test-report` assume. When
          # the project already owns a config, verify it declares both — do not assume: a config
          # without `[profile.ci]` makes `--profile ci` fail, and one without `[profile.ci.junit]`
          # writes no report at all, so the gate would read a missing file and answer UNVERIFIABLE
          # beside tests that passed. The likely adopter fix is to drop `--test-report`, which relaxes
          # the policy — so this is a named error, not a puzzle.
          if ! grep -q '^\[profile\.ci\]' .config/nextest.toml \
             || ! grep -q '^\[profile\.ci\.junit\]' .config/nextest.toml \
             || ! grep -q 'path = "junit.xml"' .config/nextest.toml \
             || ! grep -q 'fail-fast = false' .config/nextest.toml; then
            echo "::error::.config/nextest.toml exists but does not declare [profile.ci], [profile.ci.junit] and path = \"junit.xml\": add them, or point --test-report at the report your own test command writes"
            exit 1
          fi
          cargo nextest run --profile ci
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
          rejected=0
          gate() {
            out="$(req gate --test-report "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml" "$@" 2>&1)" && code=0 || code=$?
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
            # A release line is a `key=value` context, never an option. A help flag wins WHEREVER
            # it appears (`req gate deployment_mode=x --help` prints the page and exits 0 having
            # gated nothing), so every argument is checked, not just the first.
            release_option=0
            for release_arg in "${release[@]}"; do
              case "$release_arg" in -*) release_option=1 ;; esac
            done
            if [ "$release_option" -eq 1 ]; then
              printf 'release line carries an option, so it is not a release: %s\n' "${release[*]}" >&2
              rc=1
              rejected=$((rejected + 1))
              continue
            fi
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
          # A file whose lines were all rejected declared something, and it was malformed: it is
          # not the "no variants" state, so the bare default is not its fallback. The sticky `rc`
          # already makes the step red.
          if [ "$gated" -eq 0 ] && [ "$rejected" -eq 0 ]; then
            gate
          fi
          exit $rc
      # The agent-facing half (pack C): the verdicts as JSON documents, kept off the log.
      # An agent reviewing this pull request reads `req.gate/2`, never a scraped text line.
      - name: the JSON views, as build artifacts (an agent reads these, not the log)
        if: always()
        run: |
          set -eu
          mkdir -p req-views
          # A view document is complete even when its verdict is open (pack C), so each exit
          # class is recorded beside its document and never folded into this step: the gate
          # above decides the build, and `always()` keeps the views when it went red.
          # `--format json` renders to stdout only — beside `--out`/`--check` it is a usage
          # error (exit 2) — so a redirect, not a flag, is how a document reaches the disk.
          #
          # The redirect goes to a `.tmp` name first because a command can fail WITHOUT writing
          # a document: an unreadable corpus prints `[UNVERIFIABLE]` to stderr and exits 3, and
          # a bare `> req-views/gate.json` would leave a zero-byte file where a view should be.
          # A consumer that parses an empty document believes it holds a verdict. So a view
          # that wrote nothing is named on the log and leaves no file at all.
          capture() {
            name=$1
            shift
            req "$@" --format json > "req-views/$name.json.tmp" && code=0 || code=$?
            if [ -s "req-views/$name.json.tmp" ]; then
              mv "req-views/$name.json.tmp" "req-views/$name.json"
              printf 'req %s --format json -> req-views/%s.json (exit %s)\n' "$*" "$name" "$code"
            else
              rm -f "req-views/$name.json.tmp"
              printf 'req %s --format json -> NO DOCUMENT, nothing written (exit %s)\n' "$*" "$code"
            fi
          }
          capture render render
          capture trace trace
          capture audit audit
          # The gate document is ONE PER VARIANT, exactly as the gate step runs it: a bare
          # `req gate` is UNVERIFIABLE on a corpus carrying a selector (ADR 0015), so a bare
          # document would say "cannot tell" beside a release that passed. With no release
          # declared, the bare view IS the default variant's document.
          releases=0
          variants_read=0
          rejected=0
          if [ -f .req/variants ] && [ -r .req/variants ]; then
            variants_read=1
            while IFS= read -r variant_line || [ -n "${variant_line}" ]; do
              variant_line="${variant_line%$'\r'}"
              read -r -a release <<< "${variant_line%%#*}"
              [ "${#release[@]}" -gt 0 ] || continue
              # The gate step above is red on this line; capture nothing rather than help text.
              release_option=0
              for release_arg in "${release[@]}"; do
                case "$release_arg" in -*) release_option=1 ;; esac
              done
              if [ "$release_option" -eq 1 ]; then
                printf 'release line carries an option, not captured: %s\n' "${release[*]}" >&2
                rejected=$((rejected + 1))
                continue
              fi
              releases=$((releases + 1))
              capture "gate-${releases}" gate --test-report "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml" "${release[@]}" < /dev/null
            done < .req/variants
          fi
          # Only a file THIS STEP READ, declaring no line, is the default variant and its bare
          # gate the document. A missing, unreadable or non-regular file is the red the gate step
          # above already reported: the other views are kept, but no `gate.json` may read `pass`
          # beside it.
          if [ "$releases" -eq 0 ] && [ "$variants_read" -eq 1 ] && [ "$rejected" -eq 0 ]; then
            capture gate gate --test-report "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml" < /dev/null
          elif [ "$releases" -eq 0 ] && [ "$variants_read" -eq 0 ]; then
            printf 'no readable regular .req/variants: the gate step above is red, so no gate document is written\n' >&2
          elif [ "$releases" -eq 0 ]; then
            printf 'a release line carries an option: the gate step above is red, so no gate document is written\n' >&2
          fi
          # `if-no-files-found: error` on the upload only reds a COMPLETELY empty directory: one
          # variant's document lost while its text gate passed would ship silently. The step knows
          # how many documents it expects, so it checks instead of assuming.
          expected=3
          if [ "$releases" -gt 0 ]; then
            expected=$((expected + releases))
          elif [ "$variants_read" -eq 1 ] && [ "$rejected" -eq 0 ]; then
            expected=$((expected + 1))
          fi
          actual=0
          for document in req-views/*.json; do
            [ -f "$document" ] && actual=$((actual + 1))
          done
          if [ "$actual" -ne "$expected" ]; then
            printf 'incomplete view set: expected %s document(s), found %s\n' "$expected" "$actual" >&2
            exit 1
          fi
      - name: keep the views for the reviewing agent
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: req-views
          path: req-views/
          if-no-files-found: error
      - name: remove the deploy key this job wrote
        if: always()
        run: rm -f "${RUNNER_TEMP:-$HOME}/req_deploy_key"
```

**The install reaches req's private host.** The first step writes a read-only key from the
`REQ_DEPLOY_KEY` secret to **`$RUNNER_TEMP/req_deploy_key`** — per job, never `$HOME`, and created
under `umask 077` — and skips itself (with a message) when the secret is unset, the case of a runner
that already has read access. The install offers that key with `IdentitiesOnly=yes`, never the
runner's own identity, and the last step removes it under `if: always()`: a private key left on a
self-hosted runner is readable by every later job, an untrusted pull request included. With neither
a secret nor runner access, install from a clone instead (`cargo install --path . --locked`): req's
own claim operations need no network (REQ-0012), so only the install is affected. The install step
also exports `PATH="$HOME/.local/bin:$PATH"` — `$GITHUB_PATH` applies to later steps only, so
without it that step's own `req --version` would be `command not found`.

**The JSON views are the agent-facing half.** The last two steps write pack C's documents and
upload them as the `req-views` artifact, so an agent reviewing the pull request reads the verdict
instead of scraping a log. `render`, `trace` and `audit` are written once; the **gate document is
written once per declared release** (`gate-1.json`, `gate-2.json`, …), because a bare `req gate` is
UNVERIFIABLE on a corpus carrying a selector (ADR 0015) and a single bare document would say
"cannot tell" beside a release that passed. With a comment-only or empty `.req/variants`, `gate.json`
*is* the default variant's document; an **absent or unreadable** file is the gate step's red, so
the other views are kept and no gate document is written at all — a `pass` beside that red would be
worse than no document. The capture step runs `if: always()` and **records** each command's exit class
beside its document instead of folding it into the step: a document is complete even when its
verdict is open (ADR 0018, and pack C's rule), and the gate step above is what decides the build.
`--format json` renders to stdout only — beside `--out`/`--check` it is a usage error (exit 2) —
so the redirect is how a document reaches the disk, and it goes to a `.tmp` name first: a view that
fails **without writing a document** (an unreadable corpus prints `[UNVERIFIABLE]` and exits 3)
leaves no file and is named on the log, because a zero-byte document is the one thing a consumer
would parse and believe. `tests/ci_template.rs` pins `if: always()`, `if-no-files-found: error`, the captures and
the set check by **count**, so a coordinated removal from all three copies is still red.

The upload names `actions/upload-artifact@v4`. That version is the one **runner-specific** line in
this file: check the major your GitHub or Forgejo runner implements and change it if it differs —
the JSON files are written to `req-views/` either way, and only the upload is version-specific.

**This body is one text in three places**, and a test keeps it that way: `req init` prints it,
this page carries it, and `examples/github-workflows/req.yml` is the copy-ready file to `cp` into
`.github/workflows/` or `.forgejo/workflows/`. `tests/ci_template.rs` compares the three byte for
byte — the equality is checked, never assumed.

**`.req/variants`** is plain text beside the corpus, because the CI step is bash and YAML would need
a parser: one gate invocation per line, `key=value` pairs space-separated, `#` starts a comment, and
a line that is empty after stripping the comment is skipped. Two rules matter:

- **A release line is a `key=value` context, never an option.** Any argument that looks like an
  option rejects the line, because a help flag wins **wherever it appears**: `req gate --help` and
  `req gate deployment_mode=x --help` both exit **0** having gated nothing. The step goes red with
  the offending line named, and a file whose every line was rejected is not the default variant:
  neither the gate step nor the artifact step falls back to the bare run for it.
- **No release line declares the default variant.** A comment-only `.req/variants` describes exactly
  one variant — the default — so the step runs the bare-variant `req gate` (still with
  `--test-report target/nextest/ci/junit.xml`) only when the loop read no line.
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

Two parts of the snippet are not verified on a runner: whether the job summary renders above the log, and
the artifact upload — `actions/upload-artifact` and `if: always()` are the runner's behaviour, not req's.
Everything else is plain shell, and `tests/init.rs` **executes this exact step** against a stub `req` over
every shape it has to survive (no, comment-only, one-release, unreadable and option-shaped `.req/variants`;
a release appended without a trailing newline; an unreadable corpus, where every command exits **3** and the
step writes no documents and goes **red** because it knows how many it expects) — so those behaviours are
checked rather than described. Confirm the table renders on your first pull request; if it does not, the step
is still correct, it just loses the summary.

## The coverage rule

`req gate` checks the **applying claims of its invocation**. A claim the invocation leaves
**undecided** is not examined by it — and since ADR 0015 that is a failure, not a disclosure: the
gate exits **3 (UNVERIFIABLE)** and names the claims it could not examine, with the remedy. A bare
`req gate` is therefore UNVERIFIABLE on any corpus carrying a selector, and the per-variant
invocation is the only one that can be green. So:

**Every variant the project declares gets its own gate invocation.** That is what `.req/variants`
is for — and since ADR 0019 each invocation also takes the run's report
(`--test-report target/nextest/ci/junit.xml`), because without it the `tests_linked` policy cannot be
exercised and the gate is UNVERIFIABLE rather than green. When you add an `applicability:` predicate to a claim, add its variant to `.req/variants`
in the same commit — otherwise the claim is inside no gate invocation at all, and the bare run tells
you so: it exits 3 and names that claim among the undecided.

## Without CI

Nothing about the tool changes; only whether it is run. The gate becomes a ritual, and a ritual that
is not enforced is not a gate. The cheapest enforcement is a `pre-push` hook that runs
the same loop over `.req/variants` shown above — on a corpus carrying
a selector the bare invocation is UNVERIFIABLE (ADR 0015), so the loop *is* the gate. Run the suite
first (nextest, one profile, one report) and pass `--test-report` on every line, or the gate is
UNVERIFIABLE for lack of evidence. Do not
oversell it: a local hook is a reminder, not a record, and nothing outside your machine can see
whether it ran.

## For an agent

The same path for an agent and the human with it; the unit is the **claim** — one obligation, one file.
`tools/quickstart.sh` rehearses it offline in a scratch directory: it prints `QUICKSTART <n>s` (1s on the
last measured run, 0-2s across runs) and fails at n >= 300, so "under five minutes" is an instrument, not
prose. The script does not install req — the install is the adopter's step and is not in the measured
window. Each step names its deciding line; the explanation is in the section it points at.

1. **Install at a pinned rev** (*Install req*). Exit **0**, `req --version` printing `rules_v` and
   `built from`; `<PIN>` is read with `git rev-parse --short origin/main`, never copied out of a document.
2. **Start the corpus** (*Start a corpus*): `req init`, exit **0**, first line `Wrote a working, verified
   corpus in the current directory:`, exactly three files. Confirm with `req canon` → exit **0**, last line
   beginning `claims: 1 (0 unstructured); pins: 3 on 1 claim(s), 0 flagged`. Its refusals are on
   `req init --help`.
3. **Bring your own spec** (*Adopting from documents you already have*): copy `tools/adopt.py` out of the
   req checkout at the rev you installed and run it from the corpus root. A **dry run** is the default and
   prints, exit **0**:

   ```text
   draft  REQ-0002  claims/REQ-0002.yml  docs/capabilities.md#L<n>
   skip   docs/capabilities.md#L<m>  empty text
   dry run: nothing written — pass --write to write them
   ```

   `--write` writes the drafts and names them (`wrote: <n> claim(s) — claims/REQ-0002.yml, …`).
4. **Ratify: read the parse, pin the bound** (*The authoring loop and its traps*): `req canon` reports and
   never gates — one row per claim with its derived parse, one `pin <key> <value>: agrees` (or `FLAG …`)
   line per pin, and a last line `claims: <n> (<u> unstructured); pins: <p> on <c> claim(s), <f> flagged; …`.
   Nothing is filled: there is nothing derived to write. A flagged pin is the gate's to fail (`pins_hold`).
5. **Gate it** (*Link each claim to its proving test*, *CI*, *The coverage rule*): declare each release in
   `.req/variants`, run the suite for the report, then one `req gate --test-report
   target/nextest/ci/junit.xml <key=value ...>` per line. With every applying claim linked to a passing
   test the first line is `# Release gate: <n> applying claim(s), 0 undecided (an undecided claim is never
   treated as absent)` and the last is `verdict: PASS — every policy settled in the affirmative, and every
   claim was decided`. A bare `req gate` on a corpus carrying a selector exits **3** and ends `verdict:
   UNVERIFIABLE — nothing is wrong, but something cannot be confirmed`; that is correct since ADR 0015, not
   a bug. Commit the document as a projection (*The document your project can commit*): `req render
   --out docs/spec.md`, and `req render --check docs/spec.md` is the alarm.

To hand **one** of these steps to a coding agent in another project, fill in `examples/handoff-brief.md`: it
names the step, the req rev to precheck with `req --version`, the input file, the exit code and line that
mean done, and what the agent reports back. The agent finds the commands from `req --help`, not from you.

### What the agent may not do

Paste the block in `examples/agents-snippet.md` into your project's own `AGENTS.md` — it is this list, cut
for an adopter's agent. The four prohibitions, and why each one matters:

- **Never add a derived field to a claim file.** The parse, the canonical form and the address are derived at load and stored nowhere (`req.claim/2`); a file carrying `address`, `canonical` or any removed slot is a parse error that names the key. State a load-bearing slot as a pin instead — and never edit a pin to match a parse the statement was not meant to have.
- **Never edit a committed projection.** Regenerate it with `req render --out`; `req render --check` is what proves it current. An edit moves bytes the corpus did not produce, and `--check` reports it.
- **Never treat `req canon`'s exit 0 as a pass.** It reports the derived parse and each pin's verdict and does not fail on a flag; the check that gates is `req gate`.
- **Never auto-merge.** A change direction is classified for a human to ratify, two addresses that differ are different requirements, and ratification is the human's act (ADR 0004).

### What it can read

`--format json` on `render`, `trace`, `gate` and `audit` prints the same answer as a document on stdout —
`req.render/1`, `req.trace/1`, `req.gate/2`, `req.audit/1` — with the exit code identical to the text form.
It renders the verdict, it does not add one. The CI Action writes them to `req-views/` (see *CI*), with
**one gate document per declared release**; a `req.gate/2` document carries **six** `policies` (the first is
`pins_hold`) and a
top-level `process_evidenced` array, so a consumer that reads only the machine count would overstate what
was proven. Read the artifact, not the log; the gate step remains what decides the build.

The exit contract is on `req --help` and each command's `exit:` line: an agent consumes the exit code and
the named line, not the prose. `req render --check <path>` is the drift alarm — 0 current, 1 stale or
hand-edited, 3 cannot judge; treat 3 as not current, and never as a pass.

### The honest limit

`req gate --test-report` says **which claims have no passing test** — named per claim, exit 3, never folded
into a pass. It does not say that a linked test *proves* the claim: a link is a claim about evidence,
attested in the signed Baseline, and reviewed like a rationale. A `process` declaration is evidence of a
different kind: the gate can check that it is consistent and attested, never that the process check was
run — so it is reported as declared, not as proven. A wrong link is worse than no link, because it
manufactures confidence; `req audit`'s coverage is not test coverage either. And the report is an input,
not a verdict — an absent, unparsable or empty one is UNVERIFIABLE, never "no failures found". One gap is
named rather than hidden: nextest does not run doctests, so a future `#[doc]` example needs its own gate
step.

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
