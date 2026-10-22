# The agentic path: an empty repository to a gated corpus in five steps

For the agentic adopter and the human with it. The unit is the **claim** — one obligation, one file.
Each step gives the command and its deciding line; the traps are in `docs/adopting.md`.
Rehearsed by `tools/quickstart.sh`, offline: it printed `QUICKSTART 1s` here (1-2s across three
runs) and fails at n >= 300, so "under five minutes" is an instrument, not prose. The script does
not install req — the networked install in step 1 is the adopter's step and is not in the measured
path.

## 1. Install at a pinned rev

req is deliberately unpublished (`publish = false`), so **the rev is the provenance**: install a
commit, never a version.

```bash
GIT_SSH_COMMAND="ssh -o ControlMaster=no -o ControlPath=none -o StrictHostKeyChecking=accept-new" \
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo install \
  --git ssh://git@git.xantu-barb.ts.net/fdx/req.git \
  --rev <PIN> --root "$HOME/.local" --locked
export PATH="$HOME/.local/bin:$PATH"   # where --root "$HOME/.local" put req; adjust if yours differs
req --version
```

`<PIN>` is the merge SHA you depend on — read it with `git rev-parse --short origin/main`; never copy
one out of a document. Exit **0**. This is the **only networked step** of the path: everything below
runs offline against the corpus (REQ-0012). The `export` is not decoration: a fresh shell does not
have `$HOME/.local/bin` on `PATH`, and without it the next step's `req init` is `command not found`.

`req --version` (exit **0**) prints `req 0.1.0`, then the versions that decide a verdict (`rules_v 5`
among them), and `built from <short sha>`. Two lines decide what the binary is: **`rules_v`** makes a
stored address current — an address is derived under a rules version, so a verdict is not reproducible,
and a Baseline not comparable, under another one — and **`built from`** is the commit the binary came
from, a bare SHA meaning *clean*, `-dirty` meaning tracked files were modified, so the binary is not
the commit it names.

## 2. Start the corpus

```bash
cd myproject
req init
```

Exit **0**, first line `Wrote a working, verified corpus in the current directory:`, and exactly
three files: `claims/REQ-0001.yml` (the starter claim, already filled and verified through the same
derivation `req canon --fill` uses, so its `canonical.form` and `address` are real),
`claims/README.md` (the authoring loop) and `aliases/subjects.yml` (the subject registry). Confirming
it: `req canon --verify` → exit **0**, `rules_v 5; verified 1, candidates 0, mismatches 0`. The
refusals, one line each:

- `claims/` or `aliases/subjects.yml` already exists — exit **1**, nothing written: a corpus is never overwritten.
- `aliases` is a symlink — exit **1**: the registry would sit outside the tree `init` was asked to create.
- any argument, or `--root` — exit **2**: `init` takes neither, so run it where the corpus should live.
- the starter claim cannot be verified, or the target cannot be written — exit **3**, and whatever was written is removed rather than left as a red half-corpus.

## 3. Bring your own spec

req ships no importer and no `adopt` subcommand (issue #44): the mapping from your document to
claims is yours. Copy the extractor out of the req checkout at the rev you installed, then run it
from the corpus root:

```bash
cp <req checkout>/tools/adopt.py tools/adopt.py
python3 tools/adopt.py --from docs/capabilities.md --subject SYSTEM --date YYYY-MM-DD          # dry run
python3 tools/adopt.py --from docs/capabilities.md --subject SYSTEM --date YYYY-MM-DD --write  # write
```

The defaults are the safe direction: a **dry run** writes nothing, and a statement that parses as a
structured claim is **reported as skipped, never written** — its slots are the author's. Every skip
is named with its reason. Exit **0**:

```text
draft  REQ-0002  claims/REQ-0002.yml  docs/capabilities.md#L<n>
skip   docs/capabilities.md#L<m>  parses as a structured claim — only its author can supply the slots (author it, or re-word it as prose)
dry run: nothing written — pass --write to write them
```

`--write` writes the drafts and names them (`wrote: <n> claim(s) — claims/REQ-0002.yml, …`).
`--subject` is required and used verbatim (one decision per document); `--date` is required when the
document has no committed git history, because a date is authored, never invented. Each draft is
`unstructured: true` with `canonical.form` as the empty placeholder and `address: null` on purpose —
only the derivation writes those. The conventions (`bullets`, `req-lines`), heading scope and
provenance rule are in `docs/adopting.md` §Adopting from documents you already have; copy and adjust
the script.

## 4. Ratify: author, fill, verify

```bash
req canon            # reports — always exit 0, never a pass
req canon --fill     # writes canonical.form and address
req canon --verify   # the check that gates
```

`req canon` prints a per-claim table; the deciding part of its last line is
`claims: <n>; discrepancy flags: <n>`. It **reports and never gates**: a contradiction or an omission
is a flag, and the exit is 0 either way.

`req canon --fill` re-derives `canonical.form` and `address` into the anchor lines: exit **0**,
`rules_v 5; filled <n>, already current <n>, refused <n> (a refusal is UNVERIFIABLE, never a pass)`.
It **refuses** a claim whose authored slot contradicts the parse — exit **3**, `REFUSED`, nothing
written, because an address over a contradicted parse would enshrine it.

`req canon --verify` compares the stored fields with a fresh parse: exit **0** verified; **1** a
stored digest that does not recompute — the hand-edited-digest case, and, as measured, also a
`tools/adopt.py` draft, which carries the empty `canonical.form` anchor with `address: null` and is
reported `form stored but no address` (a `--fill` remedy, not a hand edit); **3** a candidate
(filled under other rules — re-fill it, never retype it).

The loop in one sentence: authoring moves the statement, `--fill` moves `canonical.form` and the
address, `--verify` is what you gate on. One rules-v5 trap: a claim's `applicability:` list is part of
its identity, so author it *before* `--fill`, and re-fill when you change it.

## 5. Gate it

Declare every release in `.req/variants`: one `key=value ...` context per line, `#` comments and blank
lines skipped, e.g. `deployment_mode=air_gapped`; `req` does not read it; the CI step runs one gate per line.

```bash
req gate deployment_mode=air_gapped
```

Exit **0**, first line `# Release gate: <n> applying claim(s), 0 undecided (an undecided claim is
never treated as absent)`, last line `verdict: PASS — every policy settled in the affirmative, and
every claim was decided`. The variant must be one your corpus describes: author the
`applicability: deployment_mode == air_gapped` on the claims it scopes, in the same commit as the line;
an entry nothing decides is refused — exit **3**, `selectors_confirmed` names the key (ADR 0006).

A bare `req gate` on a corpus carrying a selector exits **3**, names the claims the invocation left
undecided with the remedy, and ends `verdict: UNVERIFIABLE — nothing is wrong, but something cannot
be confirmed`. That is correct since ADR 0015, not a bug: an undecided claim was not examined, and an
unexamined claim is not a pass — while you declare no variants, the bare `req gate` is the gate (exit 0).

Commit the document as a projection: `req render --out docs/spec.md` writes it, and
`req render --check docs/spec.md` is the alarm. `--out` exits **0** (`[WROTE] …`); `--check` writes
nothing and exits **0** current (`[CURRENT] …`), **1** stale or hand-edited (first differing line
named — regenerate, never edit), **3** it cannot judge. Pin the path in `.req/projections` and run
`req render --check <path>` for each in CI, exactly as `.req/variants` is gated. The copy-ready
Action is `examples/github-workflows/req.yml` (the same bytes `req init` prints) and it runs
`req canon --verify`, the variant gates and the JSON documents — add the projection check beside
them if you commit a projection.

## What the agent may not do

Paste the block in `examples/agents-snippet.md` into your project's own `AGENTS.md` — it is this
list, cut for an adopter's agent. The four prohibitions, and why each one matters:

- **Never hand-write `address` or `canonical.form`.** Both are derived: `req canon --fill` writes them into an existing claim, and `req init` derives the starter claim through that same path. A hand-written digest is a critical finding, and `req canon --verify` is what detects it.
- **Never edit a committed projection.** Regenerate it with `req render --out`; `req render --check` is what proves it current. An edit moves bytes the corpus did not produce, and `--check` reports it.
- **Never treat `req canon`'s exit 0 as a pass.** It reports divergences and always exits 0; the check that gates is `req canon --verify`.
- **Never auto-merge.** A change direction is classified for a human to ratify, two addresses that differ are different requirements, and ratification is the human's act (ADR 0004).

## What it can read

`--format json` on `render`, `trace`, `gate` and `audit` prints the same answer as a document on
stdout — `req.render/1`, `req.trace/1`, `req.gate/1`, `req.audit/1` — with the exit code identical to
the text form. It renders the verdict, it does not add one, and it is stdout only: `--format json`
beside `--out` or `--check` is a usage error (exit 2).

In CI the copy-ready Action writes those documents to `req-views/` and uploads them as an artifact,
with **one gate document per declared release** (`gate-<n>.json`) — a bare `req gate` is
UNVERIFIABLE on a selector-carrying corpus (ADR 0015), so the per-variant document is the one that
carries the release verdict. Read the artifact, not the log; each document's exit class is recorded
beside it, and the gate step remains what decides the build.

The exit contract is tri-state everywhere, plus usage: **0** settled affirmatively, **1** a definite
finding (a defect, a mismatch, a stale projection), **3** UNVERIFIABLE — it cannot be told, which
blocks and is never folded into a pass — **2** usage. An agent consumes the exit code and the named
line, not the prose. `req render --check <path>` is the drift alarm: 0 current, 1 stale or
hand-edited, 3 cannot judge — treat 3 as not current, and never as a pass.

## What is not here yet

The test-report and the claim↔test linkage (pack D) are not built: nothing in this path reports test evidence, and audit coverage is not test coverage.
