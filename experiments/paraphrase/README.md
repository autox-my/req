# experiments/paraphrase — P-1, Tier-1 against independent paraphrase

**What this measures.** How often independently worded text for the same obligation fails to share
a Tier-1 address. Every equivalent pair in `external-v1..v4` is produced by the transforms Tier-1
implements, so a 0% false-split rate there is a tautology; this instrument asks the question the
rule-derived corpora cannot.

## Grade of the evidence (read this before any number)

> **Model-authored paraphrase, blind to Tier-1, judge-filtered; not human-written.** The
> generators were never shown the rules or any canonical form, and a judge from a different model
> family confirmed each restatement keeps the obligation — but the generators share
> training-data biases with each other and with the judges. **The headline is a lower bound on how
> far real writers diverge from one another, not an estimate of it.** It claims neither validation
> nor refutation of the bet: Tier-1's own scope excludes paraphrase, voice and word order by
> design, so "Tier-1 does not collapse paraphrase" is the expected result, and the open questions
> are *why* (the ranked difference shapes) and *how much a cheap rule extension could recover*.

A first number from data already in the repo, with its limits (2026-11-03,
`bench/corpus/tier2-positives-v1.yml` in a scratch root): 0 of 15 authored restatements share an
address (false-split 15/15), 0 of 14 one-element alterations collapse. 15 pairs, one author (an
agent), labels informed not blind. It says Tier-1 does not collapse independent paraphrase; it does
not say how much.

## The first run (2026-11-03, rules v6)

Sixty statements, three generators on three model families — `claude-haiku-5-5` (Claude),
`opencode-go/deepseek-v4.1-flash` (DeepSeek), `opencode-go/glm-5.3-flash` (GLM) — each asked, per
statement and in a fresh context, for 3 restatements and 1 one-element alteration. A restatement
enters the headline only if **both** judges of other families (the three models above, with the
generator's own family excluded) answered *yes*. Per generator, never pooled:

| generator | written | admitted | admitted with a different address | 95% Wilson lower bound | alterations that collapsed with their original |
|---|---|---|---|---|---|
| claude-haiku-5-5 | 180 | 136 | **136 / 136** | 0.973 | 0 / 60 |
| deepseek-v4.1-flash | 180 | 74 | **74 / 74** | 0.951 | 0 / 60 |
| glm-5.3-flash | 180 | 62 | **62 / 62** | 0.942 | 0 / 60 |

What the splits are (`results/…json` has every pair): about 100 of the 272 differ in **subject and
action wording** (voice and verb choice: "provide" → "show", "the product … once a game is
initiated" → "after a game has been started, the product"); about 100 are restatements that leave
the **structured parse** altogether ("It is required that …", "It is mandatory for …"); about 40
differ in **action alone**. Only a handful differ in a bound or a modality — those Tier-1 carries.
So a few more rules would recover little: most splits are wording, which needs a lexicon or a
Tier-2 model.

How to read it, honestly:

- The **false-merge side held**: none of the 180 one-element alterations shares an address with its
  original. This is the safe direction of the asymmetric cost.
- The judges dropped many restatements (deepseek: 34 *different*, 72 *unsure*; GLM: 59, 59; Haiku:
  41, 2, 1 unjudged), so the admitted set leans to easy paraphrases and **100% is conservative**.
- Model-authored, not human; the generators and judges share biases. It does not establish what
  real writers do, only that **independently worded text does not share an address under Tier-1**,
  which is its stated scope. It neither validates nor refutes the bet.
- Provenance and process: statements the generators initially failed on (an empty or truncated
  reply, an alteration that was a copy) were re-asked in a new call, up to twice; three GLM verdicts
  were unparseable and count as unjudged. Nothing was edited by hand; `raw/` holds every reply.
- `statements.json` was sampled before the CI review of the harness; its `tuned_check` text records
  154 scanned files, the tree now has 150. A re-sample draws the identical 60 statements, so the
  file (and the data keyed to its hash) was kept.

## What is here

| path | role |
|---|---|
| `statements.json` | the sample: 60 statements from the `a` sides of `bench/corpus/external-v3.yml`, with source citations, and a `sampling` block recording how they were drawn and what was excluded |
| `PROTOCOL.md` | the generator and judge prompts (pinned by `--self-check`), the filter, the file schemas |
| `restatements/<generator-id>.json` | generator output, assembled from `raw/` by `tools/assemble.py` |
| `judgements/<judge-id>.json` | judge output (likewise) |
| `raw/` | the provenance: the exact task lines sent (`generate-tasks.jsonl`, `judge-tasks-*.jsonl`) and every raw model reply (`gen-*.jsonl`, `judge-*.jsonl`), one fresh CLI call per line |
| `tools/` | `run_parallel.py` (one fresh `claude -p --tools ""` or `opencode run` call per task line, resumable) and `assemble.py` (raw replies to the schema files); orchestration glue, not part of the crate |
| `results/<result-named>.json` | the scorer's record, named by the hash of its content, never by the clock; a second identical run is not rewritten |
| `fixtures/` | a 3-statement, 2-generator, 2-judge hand-written dataset in the same layout; the end-to-end self-check scores it against hand-computed counts |
| `src/` | the crate: sampler, schemas + validators, protocol, filter, Wilson interval, shape bucketing, scorer, self-check |

The crate is **standalone** (own `Cargo.lock`, `[workspace]` table, `req` by path): the root
`Cargo.toml` and `Cargo.lock` are identity-path pins and this research must not be able to move
them. Nothing here feeds the release gate.

## Commands

Build state lives where the gate keeps it: from the repository root, with the toolchain exports of
`AGENTS.md`, `CARGO_TARGET_DIR=target cargo run -q --offline --manifest-path
experiments/paraphrase/Cargo.toml -- <command>`.

```
req-paraphrase --self-check                      # the pure half + the fixture end to end; exit 0/1
req-paraphrase sample [--count 60]               # (re)writes statements.json deterministically
req-paraphrase tasks generate                    # one JSON line per statement: the exact prompt
req-paraphrase tasks judge --generator <id>      # one JSON line per restatement/alteration
req-paraphrase validate [--dir <dir>]            # schemas, hashes, prompts, copies; exit 0 valid / 1 invalid
req-paraphrase score [--dir <dir>] [--no-record] [--format text|json]
```

`score` exit codes follow req: **0** PASS (no alteration collapsed, every generator measured),
**1** FAIL (an alteration shares its original's address — a false merge, listed by id), **3**
UNVERIFIABLE (invalid inputs, a registry that will not load, a generator with no judge from another
family or no admitted restatement), **2** usage. The false-split rate is a *measurement*, not a
gate: it never changes the exit code.

## What the scorer reports — per generator, never pooled

- **False-split rate** = judge-confirmed restatements with a different address / judge-confirmed
  restatements, with the counts and a 95% Wilson interval.
- The same split by **structure class**: `both_structured`, `restatement_unstructured`,
  `original_unstructured`, `both_unstructured` (the unstructured fallback hashes the raw text).
- For each false split, **which canonical-form fields differ** (`subject`, `action`, `qualifier`,
  `modality`, `operator`, `value`, `unit`, `conditions`, …; `structure` when one side is
  unstructured, `raw` when both are), and the top 10 **difference shapes** (the fields joined with
  `+`) with 2 examples each. This ranked list is the actionable output: the rules that would
  recover the most pairs. A shape of `unexplained` means the comparison missed a field — itself a
  finding.
- **False merges on the alterations**, by id. FMR must be 0; a collapse is a finding, never
  averaged away.
- **Coverage**: statements covered, restatements generated, admitted, and excluded by reason
  (judged different / unsure / not judged).

## The sampler, and what it cannot tell

`sample` sorts the `a` side of every pair by `sha256(pair id)` and takes the first 60 that are
single-obligation `shall`/`must` sentences of 6–40 words (one sentence, exactly one `shall`/`must`,
no `should`/`may`/`will`, no `:` `;` bullets or backslash artifacts), with a text not already
taken. A candidate is **excluded if it is found quoted** in the committed files the rules were
developed against — `src`, `tests`, `docs`, `claims`, `bench/results`, the seed `parity.yml`, and
the top-level docs — by pair id, or by any run of 8 consecutive words (case- and
punctuation-insensitive). The counts and the excluded ids are in `statements.json`'s `sampling`.

What it **cannot** tell: tuning done from a paraphrased mention, from uncommitted work or from
conversation; and the corpus's source repositories were read while the rules were developed. So
"not tuned on" cannot be established, only "not found quoted". The generated corpora
(`external-v1..v4`, `direction-v1`, `tier2-*`) are deliberately not scanned: they are built from
the same pinned prose and would exclude every candidate. Re-running `sample` at a later commit can
exclude more (docs that quote a statement); the committed `statements.json` is the sample, not the
command.

## Not a claim

This instrument measures; it does not ratify. A number from it must be reported with the corpus
that produced it (the 60 external-v3 statements), the generators' families, the judges' families,
`rules_v`, and the paragraph above.
