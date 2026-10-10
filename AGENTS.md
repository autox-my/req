# AGENTS.md — req

req: the semantic, git-native requirements engine. A **claim** is the unit of work;
one claim per file in git; identity is a content address over *canonical meaning*;
specs, traceability matrices and audit packets are compiled views over a signed
baseline.

Sources of truth, in order: `docs/design.md` (design of record) → `docs/adr/`
(decisions; supersession is recorded, never silent) → `docs/canonical-hashing.md`
(Tier-1 rules, freeze policy, benchmark protocol) → `docs/claim-format.md` (the file
contract) → `CONTEXT.md` (glossary **only** — no implementation detail).

The bet, stated so it can fail: deterministic Tier-1 canonicalization yields **zero
false merges** on the parity + adversarial-trap corpora. It is *measured*
(`req bench`), never asserted.

## Agent rules

- You are an autonomous workflow assistant. Execute instructions step-by-step
  through the SDLC below; keep state across multi-stage work.
- Break reasoning into explicit steps before writing code. Error handling and edge
  cases are part of "done". Maintain existing architecture styles.
- If unsure about the next move, ask one targeted clarification question instead of
  guessing.
- **Vocabulary comes from `CONTEXT.md`.** Use Claim, Requirement, View, Variant,
  Decision, Baseline as defined there — this project exists because those words were
  used interchangeably. Add a missing term to `CONTEXT.md` in the same commit that
  introduces it.
- **Never hand-write an `address` or a `canonical.form`.** Both are derived; only
  `req canon --fill` may write them. A hand-written digest is a critical finding.
- **Never weaken the asymmetric cost.** A false merge silently deletes a
  requirement — always a failure. A false split is noise. Never auto-merge;
  ratification is a human act.
- **Pins are provenance, not stragglers.** `rust-toolchain.toml`, `Cargo.lock`, and
  `tools/ocr/package-lock.json` are the pins. Verify a version at *pin time*; do not
  bump opportunistically. Any rules/toolchain/crate/CLI bump is a Tier-1 rules bump:
  bump `RULES_V`, regenerate goldens deliberately, record re-canonicalization
  candidates (hashing spec §5).
- **Never change the git identity** and never pass `-c user.name=` / `user.email=`
  to impersonate an author. Commits are authored by the human; provenance feeds the
  decision record (ADR 0005).
- **External material is read at named provenance, or as a lead.** A number with a source, a sample
  size, and its author's commercial interest stated is evidence; a number without them is a lead to
  investigate, never a finding to repeat. Two documents reviewed in this repository's history show the
  difference: one report whose every figure came from a vendor benchmark or a model card, and one video
  that named each sample size, each vendor's stake, and one of its own cases as "a warning light". The
  first supplied leads. The second supplied evidence.
- **Do not overclaim.** A seed-corpus pass is not the external-corpora
  falsification. Always report which corpus produced which number.
- **Nothing leaves the machine.** The core crate has no network dependency and
  providers are a process boundary; air-gapped operation must work with zero
  network (REQ-0012).

## Commands

This sandbox denies writes to `$HOME`, so both toolchains keep their state in-repo.
Every command below assumes these exports (or `scripts/check.sh`, which sets them):

```bash
# Toolchain pinned by rust-toolchain.toml; rustfmt/clippy live in that toolchain,
# not in `stable`. scripts/check.sh resolves the directory — by hand:
CHANNEL="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml)"
TC="$HOME/.rustup/toolchains/${CHANNEL}-x86_64-unknown-linux-gnu/bin"
export PATH="$TC:$PWD/tools/ocr/node_modules/.bin:$PATH"
export CARGO_HOME="$PWD/.cargo-home"
export OCR_NO_UPDATE=1
```

- Test suite: `cargo test` — check `EXIT=$?` on the *cargo* command.
  **Never pipe a check through `tail`/`grep`**: the pipeline's status is not the
  command's, and `EXIT=$?` after a pipe reports the wrong program.
- The gate that matters: `cargo run -q -- bench [corpus.yml]` — FMR = 0 hard gate,
  over the seed parity corpus by default. Exit codes: **0 = PASS, 1 = FAIL,
  3 = UNVERIFIABLE**. Writes `bench/results/<corpus-stem>-rules-v<N>.json`; results
  are append-only, one record per rules version.
- Claim diagnostics: `cargo run -q -- canon` — per-claim parity-invariant report
  (unstructured tag, modality, subject, value/unit, operator). Exit 3 when the
  registry or a claim file cannot be read.
- Derived fields: `cargo run -q -- canon --fill` writes `canonical.form` + `address`,
  and **refuses any claim whose parse disagrees with its authored slots** (exit 3) —
  an address over a contradicted parse would enshrine the contradiction.
  `cargo run -q -- canon --verify` checks the stored fields against a fresh parse:
  exit **1** mismatch (a hand-edited digest), **3** candidate (unfilled, or filled
  under other rules — re-fill before merging a rules bump). Both are in the gate.
- Lint/format: `cargo fmt --all -- --check` and
  `cargo clippy --all-targets -- -D warnings` — both part of the gate. The
  components live in the *pinned* toolchain, not in `stable`, so the toolchain
  directory is resolved rather than hardcoded. If a component is ever missing the
  check prints `SKIP`, and the commit message must say so: **a skipped check is
  never a pass.**
- Single local gate: `scripts/check.sh` — runs every check whose tooling exists,
  prints `SKIP` (never `PASS`) for missing tooling, and fails on the first
  non-zero status. Green before every commit and merge.
- Review-rule coverage: `python3 tools/review/check_rules.py` (part of `check.sh` when
  `ocr` is present) — asserts every `review/rule.json` entry still matches a tracked
  file. The CLI **silently** falls back to the built-in rubric for a glob that matches
  nothing, so a renamed directory would drop a checklist while every exit code stayed
  zero; the script checks the effect (`source: custom`) instead of the exit status.
- Direction: `cargo run -q -- compare <a.yml> <b.yml>` — classifies the change
  (`modality_weakening`, `numeric_tightening`, `polarity_flip`, `scope_divergence`, …),
  exit **3** when a side has no structured form to compare. A classification is for a
  human to ratify, never an approval. The **direction gold set** (`req bench
  bench/corpus/direction-v1.yml`) scores these: the hard gate is **dangerous
  confusions = 0** (a loosening reported as no change, or as the opposite direction —
  the direction analogue of a false merge), while accuracy is reported against a 0.95
  soft target. A misspelled `direction:` label fails the gate rather than switching the
  check off.
- Selectors: `cargo run -q -- select key=value [key=value ...]` — which claims apply in a
  variant (ADR 0003). **Tri-state**: a claim whose `applicability` predicate the context
  does not decide is reported `UNKNOWN`, never as excluded (REQ-0016) — that would drop a
  safety claim out of a variant silently. Exit **0** all decided, **3** any undecided,
  **2** usage. The grammar is deliberately tiny (`key == value`, `key >= number`); anything
  else is `UNKNOWN` with its text shown. Selection changes no address: a variant is a
  projection over one model.
- Views: `cargo run -q -- render [key=value ...]` and `cargo run -q -- trace
  [key=value ...]` — the spec view and the traceability matrix for a selection (design
  step 9). Both are **projections**: they read claims, never write one. `render` shows
  applies / undecided / outside-the-selection with each claim's address, modality,
  statement, selector and outgoing relations; `trace` prints the edges, the **derived**
  incoming edges (REQ-0020 stores each relation once, on the outgoing side), and its
  findings. Exit **0** settled, **3** anything an undecided claim or a missing claim
  leaves open, **2** usage. Two rules carry over and must not be relaxed: an undecided
  claim is **shown in its own section** (never dropped), and every edge that leaves the
  selection is **named** — to an excluded claim (legitimate, reported) or to an undecided
  one (a gap needing a decision). A relation pointing at a claim that is not in the corpus
  is `UNVERIFIABLE`, never ignored.
- Impact and release gate: `cargo run -q -- impact <claim-id> [key=value ...]` walks the
  relations **backwards** (derived, REQ-0020) and reports every dependent nearest-first
  with its selection state; `cargo run -q -- gate [key=value ...]` runs four policies over
  the **applying** claims only — `addresses_current`, `relations_resolve`,
  `dependencies_decided`, `rationale_present`. `gate` exits **0** all passed, **1** a
  defect, **3** something that cannot be confirmed (an unfilled claim, a relation that does
  not resolve, a dependency left undecided), **2** usage; a **Fail outranks** an
  Unverifiable, because "we found a problem" is stronger than "we could not tell". Both
  keep the tri-state honest: a claim the variant **excludes** is not the variant's problem,
  and an **undecided** one is never treated as absent. An `impact` answer that a broken
  relation could have made incomplete says so (exit 3) instead of quietly shortening the
  list.
- Tier-2 funnel: `cargo run -q -- propose [--min-shared N] [--gold <file>] [--provider-cmd <cmd>] [--repeats N] [--min-confidence N] [--pairs] [key=value ...]` — the funnel's
  deterministic half. It skips pairs Tier-1 already resolved (equal address), **blocks** the
  rest on shared content terms (default 2), and hands only those to a provider. Exit **0**
  every candidate adjudicated, **3** anything unadjudicated, **2** usage. The default provider
  is the **offline** one and it answers `Unverifiable` — never "unrelated", because no local
  model exists here and REQ-0012 forbids requiring a network call, so Tier-2 *metrics*
  (flip-rate stability, precision/recall) are UNVERIFIABLE in this environment rather than
  reported as zero. Blocking is a cost control, so its failure is asymmetric like everything
  else: a pair sharing too few terms is **never proposed**, which is a duplicate silently
  dropped — `blocking_misses` measures that against the corpus labels and a test pins it at
  zero. Every proposal records the provider's id verbatim (ADR 0004: the model proposes, a
  human disposes).
  With `--gold <file>` it also reports the gold set: label counts, blocking **recall** over the
  equivalents (a reachable-pair miss is FAIL), and the provider metrics — precision, recall and the
  REQ-0011 flip rate — as UNVERIFIABLE until a provider exists to measure.
  `--provider-cmd <cmd>` (run through `sh -c`, so a pipeline is allowed) makes it measurable: req
  writes `ID: <pair id>`, `A: <statement>`, `B: <statement>`, `DIFF: <differing content terms>`, `CHOOSE: equivalent|related|unrelated` to the command's
  stdin and reads the chosen label from stdout, so a model-server wrapper only has to read two lines
  and print one word — or a JSON object carrying `choice` and a `confidence` in integer
  percent, which is what a confidence sweep thresholds on. It then reports precision and recall over the equivalents as integer percents,
  the true/false positives and negatives, the unverifiable and not-proposed counts, and the
  **REQ-0011 flip rate** across `--repeats N` identical runs (default 1). A **false merge** — calling
  a non-equivalent pair equivalent — or a flip rate above **2%** is FAIL. The provider's command is
  recorded verbatim as its id, which is its provenance.
  The gold-set score also **names the offending pairs** — false merges and missed equivalents —
  because a count says how bad it is and the ids say what to fix. `tools/req-d1.sh` additionally
  appends every answer (the pair, the choice, the confidence and the full probability
  distribution) to a capture file (`.scratch/req-d1-capture.jsonl`, `REQ_D1_CAPTURE` to override),
  so a threshold can be swept **offline** rather than paying an inference run per threshold; a
  missing capture file is a measurement problem, reported as such, never mistaken for a result.
  `--min-confidence N` declines an `equivalent` below N percent: it becomes `related`, is counted as
  declined, and its cost lands in the affordable direction — a missed duplicate rather than a silent
  merge. It is the measured remedy for a model that splits equivalent/related 0.478 to 0.4385; at 90
  the false merges reach zero and precision 100%, at the price of recall 33%.
  `--pairs` prints the whole proposal list instead of the first twelve, because the list *is* the
  funnel's output and a cost measurement needs every edge; re-deriving the blocking graph anywhere else
  would be a second implementation of the tokenizer, free to drift from the one that decides what is
  proposed. The flag is plumbing and carries no test of its own — recorded rather than implied.
  **The claims pass is always offline.** `--provider-cmd` is wired to the gold-set score, not to the
  pass over our own claims, so running it without `--gold` adjudicates nothing — the shape report is
  all it produces. That is a gap rather than a design: ADR 0011 decides Tier-2 is a change-scoped
  survey, and a survey needs the provider on the claims pass with `--max-pairs` capping the run, the
  remainder deferred and reported rather than dumped. Until that lands, no path spends a provider's
  time on our own claims, and a cap would have nothing to cap.
  **With `--gold`, the gold result is the verdict** — every equivalent reachable, no false merge, and
  a flip rate actually **measured** (`--repeats 2` or more). Stability unmeasured is UNVERIFIABLE,
  never a pass, and the funnel pass over our own claims is reported as shape only because it answers
  a different question.
- Baselines: `cargo run -q -- baseline [key=value ...]` prints the **canonical manifest** — the
  rules version, the selection, and every applying claim's address — which is the exact byte
  string a human signs; `cargo run -q -- baseline --verify <file> [--signature <file>]
  [--key <id>] [--now <date>]` verifies one. Exit **0** verified (or emitted), **1** FAIL, **3**
  UNVERIFIABLE, **2** usage. Verification re-derives every address, so a claim edited after
  signing is **FAIL** — the same class as a hand-edited digest; and it consults the committed
  trust anchor at `baselines/trust.yml` (ADR 0010): a **revoked** key is FAIL, an **expired**
  one is UNVERIFIABLE (a Baseline is historical and no trusted signing time exists, so the
  question is unanswerable rather than answered badly), an unknown key or a missing signature
  is UNVERIFIABLE — a manifest is **not yet a Baseline** until a human signs it. The signature
  check is a **process boundary** to a pinned verifier, so the core links no cryptography and
  still no network; with no verifier installed the outcome is UNVERIFIABLE, never a pass.
  **The signing act is the human's**: req emits and verifies, and a key this tool could reach
  would prove nothing. The **manifest records the variant**, so verification re-derives the
  addresses under *that* selection: an explicit context that contradicts it is a **usage error**
  (exit 2), never drift in the claims — re-deriving under the wrong variant reports the variant's
  own claims as "no longer applies", which reads as a defect in the requirements instead of a
  mistake in the invocation.
- Audit packet: `cargo run -q -- audit [--baseline <file>] [key=value ...]` — the last compiled
  view: for every **applying** claim it prints the address, the coverage state, the provenance
  (rationale source and date) and the outgoing relations, then the **undecided** claims in their
  own section, then its findings. Exit **0** every applying claim covered and still matching,
  **1** a defect (drift, or no provenance), **3** nothing ratified or something uncovered, **2**
  usage. The selection comes from the Baseline being audited, the same rule as `--verify`.
  Coverage is **derived from `baseline::drift`**, not recomputed, so the packet and
  `req baseline --verify` cannot disagree about whether a claim still matches what was signed.
  With no `--baseline` it still prints and says plainly that nothing is ratified — an audit that
  passed on an unsigned model would be worse than no audit. It reports **coverage**, and says
  so: whether a signature is *valid* belongs to `req baseline --verify`, and this packet never
  asserts authority it did not check.
- Decision log: `cargo run -q -- decision [--fill] [--verify]` — the recorded decisions and their
  **hash chain** (design step 8, ADR 0005). One decision per file under `decisions/DEC-*.yml`
  (zero-padded so file order is chronological), each committing to its predecessor by
  `sha256("req.decision.v1" ‖ prev_hash ‖ canonical content)`. `--fill` writes the derived
  `prev_hash`/`hash` **into** the record, preserving comments and the note a human wrote; it never
  overwrites the file. `--verify` (the default) walks the chain. Exit **0** VALID, **1** TAMPERED,
  **3** UNVERIFIABLE, **2** usage. The three outcomes are the point: a stored hash that does not
  recompute, or a `prev_hash` that disagrees with the entry before it, is **TAMPERED** — a definite
  finding — while an unfilled hash, an unparsable file, an **empty log**, or a log that does not
  begin at the genesis anchor is **UNVERIFIABLE**. That last case is deliberate: a truncated history
  and a forged first entry are indistinguishable from the file set alone, so the honest answer is
  that the question cannot be settled — never that it passed. A decision records the *address* it
  ratified, so a claim reworded meaning-preservingly keeps its decision while an edit that moves the
  address leaves the decision pointing at something that no longer exists — which `req audit` and
  `req baseline --verify` report, not this log: a log is history, and superseding an address later is
  not tampering.
- Docs-only changes: the same gate, with its skips stated in the commit message.

### Review — OCR in delegation mode (no LLM, no API key)

`ocr` is installed **locally and pinned** (a global install would land in
`/usr`, which is root-owned and read-only here, and would not be lockfile-pinned):

```bash
# from the repo root, to (re)install the pinned CLI
npm --prefix tools/ocr install --no-fund --no-audit --cache "$PWD/.npm-cache"
ocr --version        # must print open-code-review v1.12.12
```

Delegation mode uses **no LLM**: OCR performs deterministic file selection and rule
resolution; the agent performs the review. Pin the rules to a file we own and pass
it on every invocation:

```bash
ocr delegate preview --format json [--from main --to feature/<name>] [--rule review/rule.json]
ocr delegate rule --format json --rule review/rule.json <paths from the preview>
ocr --version        # --format needs >= 1.9.0; we pin 1.12.12
```

If `--format` fails with `unknown flag: --format`, use text output for the rest of
that run; never parse text as JSON and never invent schema fields.
This npm skips the package's `postinstall` under its `allowScripts` policy; the CLI
binary comes from the platform optionalDependency — always verify `ocr --version`
after installing.

**The rule-file shape we own** (`review/rule.json`), verified against 1.12.12 — the
vendor ships no schema, so it was established by probing the CLI, and two behaviours
matter:

```json
{"rules": [{"path": "<gitignore-style glob>", "rule": "<the checklist text>"}]}
```

- A matched file's group becomes `source: "custom"` with exactly our text, and an
  unmatched file falls back to `source: "system"`. Custom rules **replace** the
  built-in rubric for matched paths — they do not merge — which is why our
  `src/**/*.rs` entry ends by telling the reviewer to also apply the built-in
  language checklist.
- **Unknown keys are silently ignored** (Go drops them). A misspelled key is
  therefore a no-op that still "loads", so a rule file must be *verified to take
  effect* — check `source`/`rule` in the output, never just the exit code. The two
  fields are `path` and `rule`; anything else is decoration.
- The built-in `source: "system"` rule is **language-specific** (for Rust: typos,
  ownership/lifetimes, error handling and panics, unsafe boundaries, concurrency,
  async/cancellation, collections/perf, type and API design, macros, security) — not
  a generic rubric. To read it: `ocr delegate rule --format json <path>` without
  `--rule`.

## Where things live / how data flows

- `CONTEXT.md` — glossary only; no implementation detail.
- `docs/design.md` — design of record: the six tensions, the layers, the build
  order and its gates.
- `docs/adr/` — numbered decisions; a superseded ADR keeps its text and gains a
  status line (ADR 0008 → 0009 is the precedent).
- `docs/canonical-hashing.md` — Tier-1 rules, the two tiers, freeze policy,
  benchmark protocol.
- `docs/claim-format.md` — one claim per file; the parity invariant; authoring rules.
- `claims/REQ-*.yml` + `claims/README.md` — the dogfood corpus: req's own
  requirements, in req's own format.
- `aliases/subjects.yml` — subject registry. Alias sets must be disjoint; an
  unregistered surface stays literal (never silently collapsed).
- `bench/corpus/parity.yml` — the seed E/N pairs and adversarial traps, labels written
  from the spec. `bench/corpus/external-v{1,2,3}.yml` — other projects' real
  requirement prose, labelled by construction, built by
  `tools/corpus/build_external.py` (sources pinned by commit SHA; v3 adds the PROMISE
  NFR set, a deterministic sample of 969 statements across 47 project ids).
  `bench/corpus/tier2-v1.yml` — the Tier-2 **evaluation** corpus (design step 5): three labels
  (equivalent / related / unrelated) built by `tools/corpus/build_tier2.py`, every pair recording
  the transformation that produced it, so labels are by construction and the generator is the
  reviewable surface. Equivalence transforms are the ones the Tier-1 rules already sanction;
  `related` pairs are reachable hard negatives that measure a provider's precision, and `unrelated`
  ones are foreign-domain pairs blocking should never propose.
  `bench/results/` — one record per corpus and rules version; append, do not rewrite.
  `tools/tier2/evaluate.py` scores a **captured** provider run without re-running the model: it
  chooses the confidence floor on each half of the gold set (`sha256(pair id) % 2`) and reports
  it on the other — two folds, both directions — quoting the **stable floor** that is clean in
  both rather than the flattering direction, prints both curves, and writes a JSON record under `bench/results/` named by the **result** rather than by the clock, so two runs that agree produce the same path (the second refuses to rewrite it) and two that disagree cannot collide; the capture records the pair id, so rows pair to labels by identity. A floor picked and
  scored on the same pairs is fitted, not measured, and the record states that both halves still come
  from one generator and that confidence is the model's own uncalibrated report.
  `bench/corpus/tier2-pilot-v0.yml` is the Tier-2 **human-labelled** sample in progress: thirty
  candidate pairs drawn from pinned real prose, crossing requirement ids, stratified by lexical
  overlap, with **empty labels** — the worksheet is the reviewable surface, and
  `docs/tier2-sample-protocol.md` is the protocol that makes a label on it interpretable, including
  the ceiling that must be measured (two labellers' agreement) before any model is judged against
  one hundred percent.
  `tools/tier2/validate.py` checks a worksheet against the schema, the outcome vocabulary and the two
  structural rules (pairs cross requirement ids; the recorded shared count matches the terms listed),
  tri-state as everywhere: valid, invalid, UNVERIFIABLE when it cannot be read. `undecided` is a
  first-class outcome — the protocol asks a labeller to admit a pair they cannot decide, and a
  vocabulary of three labels would push them to guess. `tools/tier2/agreement.py` takes two fillings
  of one worksheet, refuses to run when both name the same labeller, keeps agreed, disagreed and
  undecided apart, reports the **ceiling** as agreement over pairs decided by both, names the
  disagreements, and writes a result-named record under `bench/results/`. Both carry a `--self-check`.
  `tools/tier2/decompose-phase1.py` and `tools/tier2/decompose-phase2.py` are the two halves of a
  decoupled pipeline for measuring a *decomposer* — an instruct model that names the structural
  differences between two statements — separately from the judge that weighs them. One model runs at a
  time because four cores cannot host both, and each stage is resumable because the first attempt lost
  an hour of completed work to a judge that never answered. The experiment they ran is recorded in the
  protocol as a negative result: at 2B and 4B the decomposer reports no difference on pairs authored to
  contain one.
  `tools/tier2/entropy.py` reads the same captures and reports the entropy of each returned
  distribution grouped by what the provider chose and what the gold says, plus whether any cut on it
  removes every false merge. It exists because the scalar is not the only functional of a
  distribution, and on this sample the answer is that for three of four providers no functional
  separates the classes at all — while for the fourth the scalar works and entropy does not.
  These corpora are **generated data**: a corpus of this size is reviewed through its
  generator (`tools/corpus/`) and its label scheme, not line by line — the generator,
  the source pins and the sampling rule are the reviewable surface, and the review
  preview lists the corpus itself as a single item for that reason.
- `src/` — `lib.rs` (the library surface), `main.rs` (thin CLI over it), `model.rs`
  (claim/registry/corpus types), `canon.rs` (Tier-1 pipeline + diagnostics),
  `bench.rs` (the benchmark gate + its tests), `select.rs` (tri-state variant
  selection), `compare.rs` (change direction), `view.rs` (render/trace + the shared
  row), `impact.rs` (reverse traversal), `gate.rs` (release policies),
  `baseline.rs` (Baseline manifest, drift, trust and verification),
  `audit.rs` (the audit packet, derived from the Baseline's drift),
  `decision.rs` (the decision log and its hash chain),
  `funnel.rs` (Tier-2 blocking, the provider seam, the command provider and the
  gold-set scoring), `fill.rs` (derived fields).
- `baselines/` — `trust.yml` is the committed trust anchor (ADR 0010) and the **single home of
  the public key**; `<variant>.json` is a signed Baseline's canonical manifest and
  `<variant>.json.minisig` its signature; `trust.example.yml` is the template. The standalone
  `req.pub` that minisign writes is deliberately **not** kept here: two copies of one key drift,
  and `minisign -R -s <secret key> -p <path>` regenerates it from the secret half on demand. req
  emits and verifies; it never signs.
- `tools/ocr/` — the pinned review CLI (`package.json` + lockfile committed;
  `node_modules/` ignored).
- `review/rule.json` — the review rules we own: one checklist per path class, passed
  to every `ocr` invocation. Its shape and the two CLI behaviours worth knowing are
  documented under **Review** above.
- `.scratch/` — session scratch, gitignored; never a deliverable.
- Path facts: the workspace directory is `/home/dsh/understand`; `/home/dsh/req`
  exists but is **read-only** — never write there. The DSH workspace *name* is `req`.
- Data flow: claim file → typed parse (`yaml_serde`) → Tier-1 canonical form →
  canonical bytes → `sha256("req.claim.v1\0" ‖ bytes)` = address. Only `req bench`
  (results) and `req canon --fill` (derived fields) write anything — and `--fill`
  writes only what the parity invariant confirms.

## Correctness rules

- **FMR = 0 on the corpus is a hard gate.** FAIL or UNVERIFIABLE blocks. Never relax
  the gate to make a change land.
- **Tier-1 determinism**: no `f32`/`f64` math, no `HashMap` iteration where an
  address is computed (sort keys or use `BTreeMap`), no locale-dependent formatting,
  no model I/O.
- **Three-state outcomes**: PASS / FAIL / UNVERIFIABLE. An unreadable corpus, a
  degraded registry, or a skipped check is reported as such — never folded into
  PASS. `degraded` entries are part of the record.
- **The parity invariant**: a claim's structured slots are an *authored parse* of
  `statement`; `req canon` re-parses and reports divergence per field. Never "fix" a
  claim by silently editing the parse — re-word the statement or leave the flag.
- **Claims are data**: one claim per file; `value` is a canonical exact decimal
  string (`"0.3"`, never `"0.30"`, never a float); a rationale is an object with
  source, date, rejected alternatives, assumptions.
- **Docs are claims too**: when `docs/` asserts a rule the code contradicts, that is
  a defect — fix one of them in the same commit that exposed it.
- **Test expectations are independent**: the corpus's E/N labels come from the spec,
  not from the canonicalizer's output. Never regenerate labels from the
  implementation.
- **No secrets; stage by name** (`git add <paths>`). Bulk staging is refused by the
  environment guard, and the reason is real.
- External reference material is read-only and is never quoted verbatim into our docs.

## Project discipline (do not renegotiate silently)

- The **six tensions** (`docs/design.md` §2) and **ADRs 0001–0009** are settled.
  Changing one requires a superseding ADR, not an edit in passing.
- Unit is a **claim**, not a document (0001). Git is transport/serialization, not
  identity or merge (0002). Variants are **selectors**, not branches (0003). The
  model **proposes**; a human disposes (0004). Identity collapses only on
  meaning-preserving rules (0007). Authority is a signed **Decision**, not a commit
  hash (0005). Friction at the decision boundary, none in authoring (0006). Core is
  Rust; porcelain is TypeScript, later (0009).
- **Git-native storage is table stakes, not the moat** — never sell it as one. The
  differentiators are semantic content-addressing and variability selectors.
- Never claim the bet is validated on seed evidence; the external corpora run is the
  headline measurement.

---

## SDLC workflow (local — git worktree)

Enforcement is agent discipline: there is no remote, no PR, no server-side gate yet
(see **Phases**).

### Start a feature

```bash
# From repo root; worktrees/ is gitignored and lives inside the workspace
git worktree add worktrees/<name> -b feature/<name>
# Work entirely inside worktrees/<name>/ — the environment's worktree guard
# (EDH001) refuses writes outside the session's checkout, and it is right.
```

### Implement — TDD is mandatory for code

Use the session **`edh-tdd`** skill for the loop (seams-first, red before green, one
vertical slice, mocks only at boundaries, refactoring after green). It is the
mattpocock TDD skill (MIT) adapted with EDH-specific seam machinery — take its loop,
not its registry/`ctx.*`/gate sections, which do not apply here. req's seams are
`canonicalize_statement`, the registry resolvers, the bench classification, and the
CLI exit codes.

1. Agree the seam before writing the test.
2. Write the failing test first; then only enough code to pass it.
3. Tests verify behaviour through public interfaces and must survive refactors.
   Expected values come from an independent oracle (the corpus labels, a
   hand-computed decimal) — never recomputed the way the code computes them.
4. **Rules-change protocol**: if a Tier-1 rule changes, addresses move. Bump
   `RULES_V`, regenerate the golden addresses deliberately, record
   re-canonicalization candidates, and say so in the commit message.
5. **Mutation-check the load-bearing rule.** For each trap class, delete the rule
   that handles it and confirm the corresponding corpus test fails: LE-vs-LT
   (N-002), modality (N-001), unit factors (E-002/E-005), condition extraction
   (N-004), subject boundary (N-006), ambiguous comparator (N-005). A trap whose
   rule can be removed without a failing test was never testing the rule.

Commit conventions: `feat:` `fix:` `refactor:` `test:` `docs:` `chore:` (scope
optional, e.g. `feat(canon):`). Gate green before every commit; state skips and
`UNVERIFIABLE` outcomes in the message. Pre-convention history (`req v0`,
`req tier-1 v1`) stands; do not rewrite it.

### Self-review before merge (a loop, not a pass)

Default mechanism: OCR **delegation mode** — it selects the reviewable files and
resolves the rules; **you** are the reviewer. Stay read-only while reviewing.

1. `ocr delegate preview --format json` → `mode`, `repository`, `total_files`,
   `reviewable_count`, `excluded_count`, `reviewable_files[]`
   (`path`, `status`, `insertions`, `deletions`), `excluded_files[]` (same fields
   plus `exclude_reason`). Every `reviewable_files` entry becomes a checklist item
   keyed by `(path, status)`.
   **Exclusion is not coverage.** Observed `exclude_reason` values: `unsupported_ext`
   — markdown, i.e. **all of `docs/`, `CONTEXT.md`, `AGENTS.md`, `claims/README.md`**
   — and `default_path` — lockfiles. Those files are reviewed by hand, always: here
   the docs are claims, so an unreviewed doc change is itself the finding.
2. `ocr delegate rule --format json --rule review/rule.json <paths>` →
   `{schema_version, groups[]}`, each group
   `{group_id, source, pattern, files[], rule}`. Our own rules live in
   `review/rule.json` (per-path checklists carrying req's invariants: FMR gate
   untouched; no floats or unordered-map iteration in the address path; no
   hand-written `address`/`canonical.form`; docs contradicted by code; canonical
   decimal values; no auto-merge). The file is reviewed like any other artifact, and
   its shape is documented above.
3. Diffs come from git using the preview's mode/ref metadata: range →
   `git diff <merge_base>..<to> -- <path>`; commit → `git show <commit> -- <path>`;
   workspace → `git diff HEAD -- <path>`, and read untracked files directly.
4. Review each file against its rule group and **Correctness rules** above, in
   bounded batches for large changes. Do not stop at the first high-severity find.
5. **Coverage is mandatory**: every previewed file ends `reviewed` or `skipped` with
   a concrete reason; report `total_files`, `reviewed_files`, `skipped_files`,
   `coverage_rate`. An empty preview (or an unavailable `ocr`) means review the whole
   diff by hand — **never** a pass.
6. Findings use the OCR schema: `path`, `content`, `start_line`, `end_line`,
   `category`, `severity` (critical/high/medium/low). Fix every critical/high in the
   worktree, commit, and re-run from step 1 until the blocking count is 0. A false
   positive is stated in the merge commit with reasoning; `medium`/`low` left
   unfixed are listed there too.

Review rules we own live in-repo and are passed with `--rule <path>`; the file is
reviewed like any other artifact — and its *effect* is verified in the output, not
assumed from a zero exit code.

### Merge locally (no PR)

From the **main checkout** — a worktree cannot `git checkout main`, because `main`
is already checked out there:

```bash
git merge --no-ff feature/<name> -m "merge: <what and why>; seams tested; findings left unfixed"
git worktree remove worktrees/<name>
git branch -d feature/<name>
```

The merge commit carries what a PR description would: what changed, the seams
tested, the gate's outcome (including any `UNVERIFIABLE`/`SKIP`), and findings
deliberately left unfixed.

---

## Phases

**Phase 1 (now — no remote).** Local gate (`scripts/check.sh`), worktree per
feature, local `--no-ff` merge. OCR delegation mode is the self-review mechanism.

**Phase 2 (when the Forgejo remote lands, on maturity).** The same gate invoked by
`.forgejo/workflows/` — one implementation, two invocations; never a second copy of
the check logic in workflow YAML. The flow becomes PR-based (`edh-sdlc`: issue →
worktree → checks → commit → PR → CI → diagnosis). Branch-protection contexts must
be exactly what the workflows produce (`<workflow> / <job> (<event>)`) and are
verified with `forge_repo_setup` before anyone relies on a required check: a
required context that nothing reports hangs every PR, and a required name with its
event suffix stripped never reports at all.
