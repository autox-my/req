# AGENTS.md — req

req: the semantic, git-native requirements engine. A **claim** is the unit of work;
one claim per file in git; identity is the `claim_id`, and the address is a content-addressed
*meaning fingerprint* over canonical meaning (ADR 0020);
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
- **Never hand-write an `address` or a `canonical.form`.** Both are *derived* by the
  canonicalizer and never authored: `req canon --fill` writes them into an existing claim,
  and `req init` derives a new corpus's starter claim through that same path. A
  hand-written digest is a critical finding.
- **Never weaken the asymmetric cost.** A false merge silently deletes a
  requirement — always a failure. A false split is noise. Never auto-merge;
  ratification is a human act.
- **Pins are provenance, not stragglers.** `rust-toolchain.toml`, `Cargo.lock`, and
  `tools/ocr/package-lock.json` are the pins. Verify a version at *pin time*; do not
  bump opportunistically. `rust-toolchain.toml` and `Cargo.lock` are fingerprint-path pins:
  any bump is a Tier-1 rules bump — bump `RULES_V`, regenerate goldens deliberately, and
  record re-canonicalization candidates (hashing spec §5). The review CLI pin
  (`tools/ocr/package-lock.json`) is provenance for the review process, not the fingerprint
  path: bump it deliberately, verify `ocr --version` at pin time, and state it in the
  commit — no `RULES_V` bump.
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

Every subcommand reads its corpus from a **runtime-resolved root** (ADR 0012): `--root <dir>`,
else `REQ_ROOT`, else the nearest ancestor of the working directory holding `claims/`. The
chosen root is printed once to stderr; with no corpus the answer is `[UNVERIFIABLE]` and exit
3 — never an empty result.

**`init` is the exception**: it *creates* a root, so it resolves none. It is answered before
root resolution, in the position where the subcommand is looked up (like `--version`/`--help`),
prints no `corpus root:` line, and consults neither `REQ_ROOT` nor `--root` — a `--root` on
`init` is a usage error (exit 2), because honouring it would give the flag two differently
anchored meanings and ignoring it would write a corpus somewhere the caller did not ask for.

```bash
# Toolchain pinned by rust-toolchain.toml; rustfmt/clippy live in that toolchain,
# not in `stable`. scripts/check.sh resolves the directory — by hand:
CHANNEL="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml)"
TC="$(ls -d "$HOME/.rustup/toolchains/${CHANNEL}-"*/bin 2>/dev/null | head -1)"
export PATH="$TC:$PWD/tools/ocr/node_modules/.bin:$PATH"
export CARGO_HOME="$PWD/.cargo-home"
export OCR_NO_UPDATE=1
```

- What this binary is: `cargo run -q -- --version` (`-V`) prints the versions that decide a
  verdict — the crate, `rules_v`, the Baseline envelope (`baseline_envelope 4`), the decision
  chain, the claim file schema (`claim_schema req.claim/1`) — and the build identity. The first
  line stays single-line and greppable; `--help`
  (`-h`) prints `USAGE`, and `<command> --help` (or `-h`) prints that command's page — its
  usage, one paragraph of what it does, its own `exit:` contract, and the anchor into this
  section. Every one of them is answered where the subcommand is looked up, **before** the
  corpus root is resolved, so they work in any directory (help is what you read before the
  corpus exists — `init --help` included), print no `corpus root:` line, and exit **0** — while
  bare `req`, an unknown subcommand and an unknown flag stay **2**. Among a corpus-reading
  command's arguments a help flag wins **wherever it appears**: written where an option expects
  a value it is still a request for the page, not that value, so `<command> --help` is the shape
  to use. (`init` is the exception — it answers the page only for an otherwise valid invocation,
  so a positional argument and `--root` stay usage errors even beside the flag.) `rules_v` is the axis that
  makes a stored address current, so it is the first thing to check when a verdict surprises
  you; `built from` is the short git SHA the binary was built from, or `unknown` when the
  commit itself cannot be resolved (no `.git`, no `git`). A resolved commit carries `-dirty` when a
  tracked file is modified and `-cleanliness-unknown` when the cleanliness probe could not answer —
  the commit is a fact git gave us even then — so a bare SHA always means *clean*, never "clean as
  far as we could tell". **Rebuild before probing a
  branch** — a worktree's `target/debug/req` can lag its own branch head — and compare that line
  with `git rev-parse --short HEAD` rather than remembering to.
- Bootstrapping: `cargo run -q -- init` — create a working corpus in the **current working
  directory**: `claims/REQ-0001.yml` (filled by the same derivation `canon --fill` runs, and
  asserted green through `fill::verify_text` before the command reports success),
  `claims/README.md`, and `aliases/subjects.yml`. Exit **0** created, **1** refused when
  `claims/` or `aliases/subjects.yml` already exists, or `aliases` is a symlink (a corpus is
  never overwritten, and a link would put the registry outside the tree `init` was asked to
  create), **2**
  usage (`init` takes no arguments and no `--root`), **3** UNVERIFIABLE (the starter claim
  could not be verified, or the target cannot be written — in which case whatever it wrote is
  removed rather than left as a half-corpus that is red and refuses the next run). It reads
  no root, prints no `corpus root:` line, and prints the next steps to stdout: the CI
  snippet, the per-variant rule, and where a req bug goes.
- Test suite: `cargo nextest run --profile ci` — one run, and it writes the JUnit
  report the release gate reads (`target/nextest/ci/junit.xml`), so the suite and its evidence are
  the same run: **`cargo test` is replaced, not kept beside it**. nextest is pinned at **0.9.149**
  (`cargo install cargo-nextest --version 0.9.149 --locked`, verified at pin time — a CI-side pin
  like `tools/ocr`'s, **not** a `Cargo.lock` change) and the profile is the reviewed
  `.config/nextest.toml`: `[profile.ci] fail-fast = false` (its CLI twin `--no-fail-fast` is
  redundant — the profile is the single home) and `[profile.ci.junit] path = "junit.xml"` with
  `report-skipped = "ignored"`. `report-skipped` belongs **under `[profile.ci.junit]`**, not at the
  profile's top level: the top-level spelling is silently ignored with a warning in 0.9.149 and
  would drop every `#[ignore]`d test from the artefact. The value is `"ignored"`, not `"all"` —
  `"all"` would also list tests skipped by filters or partitions. It is adopted for the
  **artefact, not for speed**:
  measured 2026-10-21, a warm `cargo test` took 2 s and `cargo nextest run` 2–3 s, both paying
  the ~24 s link, over the same 378 tests. **Doctests are the named gap** — nextest does not run
  them, so a future `#[doc]` example needs its own step in the gate; there are none today.
  Check `EXIT=$?` on the *cargo* command.
  **Never pipe a check through `tail`/`grep`**: the pipeline's status is not the
  command's, and `EXIT=$?` after a pipe reports the wrong program.
- The gate that matters: `cargo run -q -- bench [corpus.yml]` — FMR = 0 hard gate,
  over the seed parity corpus by default. Exit codes: **0 = PASS, 1 = FAIL,
  3 = UNVERIFIABLE**. Writes `bench/results/<corpus-stem>-rules-v<N>.json`; results
  are append-only, one record per rules version.
- Claim diagnostics: `cargo run -q -- canon` — per-claim parity-invariant report
  (unstructured tag, modality, subject, value/unit, operator). `value`, `unit` and `operator`
  are judged by ADR 0014's four cases (as amended): a derived slot the author **contradicts**
  is a **flag — contradiction**; a derived slot the author **omits** is a **flag — omission**;
  a slot the author wrote that the canonical form does not derive — the qualitative
  `attribute`/`value` triple (`unit: none`, non-numeric `value`, and `operator: EQ` or no
  operator at all: `none` and an absent operator are the same no-operator marker) — is an
  **annotation**, printed as `ann` in its own cell and counted by the summary line, never a
  flag and never part of the fingerprint. The annotation is **only the qualitative triple**: a
  quantity-shaped claim (a numeric authored value, a comparator other than `EQ`, or an authored
  unit) that authors a slot the parse did not derive is still a **contradiction and a flag**,
  because a bound the parse missed would silently leave the address (a false merge).
  `attribute` stays **unverified by design** (its lexicon has no parser to agree with). Exit 3 when the
  registry or a claim file cannot be read, and when a file's declared `schema:` is not the
  `req.claim/<n>` value this build writes (`CLAIM_SCHEMA_V = 1`, i.e. `req.claim/1` — the v1
  format adds `tests:` and `evidence:` — reported by `--version` as `claim_schema`): the file's parse is **NOT JUDGED** — an older schema is a migration candidate
  (a format bump moves no address), a newer one is a question for a build that writes it (the
  file is not wrong, the reader is old), and a value of no `req.claim/<n>` shape at all is a
  broken file, not a version question. The judgement lives in `claim_schema_reason`
  (`src/canon.rs`), called by the loader and by every reader that reads a claim by path
  (`compare`, the fill path), so no command can ignore the declaration, and `--fill` refuses to
  write derived fields into a format it does not write.
- Derived fields: `cargo run -q -- canon --fill` writes `canonical.form` + `address`,
  and **refuses any claim carrying a discrepancy flag — a contradiction or an omission**
  (exit 3) — an address over a contradicted parse would enshrine the contradiction. An
  annotation is not a refusal: it is the author's qualitative predicate, which the fingerprint does
  not carry and the fill must not delete.
  `cargo run -q -- canon --verify` checks the stored fields against a fresh parse:
  exit **1** mismatch (a hand-edited digest; and, measured, a draft still carrying the empty
  `form` anchor with `address: null`, reported as `form stored but no address`), **3**
  candidate (a claim with no `form` field at all, filled under other rules, or a declared
  schema this build does not write — re-fill before merging a rules bump). Both are in the gate.
- Lint/format: `cargo fmt --all -- --check` and
  `cargo clippy --all-targets -- -D warnings` — both part of the gate. The
  components live in the *pinned* toolchain, not in `stable`, so the toolchain
  directory is resolved rather than hardcoded. If a component is ever missing the
  check prints `SKIP`, and the commit message must say so: **a skipped check is
  never a pass.**
- Single local gate: `scripts/check.sh` — runs every check whose tooling exists,
  prints `SKIP` (never `PASS`) for missing tooling, and fails on the first
  non-zero status. The Baseline step is the exception: it has no SKIP, because
  with minisign missing the verify is UNVERIFIABLE (exit 3), and UNVERIFIABLE
  blocks — `brew install minisign` first. It runs the **release gate** too, once per release declared in
  `.req/variants`: `req gate` examines only the claims a variant leaves *applying*, and a bare
  invocation leaves the six scoped claims undecided, so it exits 3 now (ADR 0015) instead of
  reporting a PASS while six scoped claims in `claims/` went unexamined. Each declared line
  must decide every key an applying claim depends on: `dependencies_decided` refuses to answer
  (exit 3) for the dependency-guarded ones rather than skipping, and the undecided count keeps
  the rest honest. Green before every commit and merge.
  The step order is **build → `cargo nextest run --profile ci` (the JUnit is written) →
  `req gate --test-report target/nextest/ci/junit.xml` per `.req/variants` line**: the report is a
  product of the run that decides the suite, not a file found again later, and the release gate
  reads it.
  Between the suite and `canon` the full run also does `cargo check --locked --all-targets` and
  `cargo test --locked` on `experiments/tier2/Cargo.toml` (ADR 0021): the research crate stays
  compiling and its own tests green, outside the JUnit report and outside any claim's evidence;
  `--fast` prints a `FAST` line for the pair.
  **`scripts/check.sh --fast` is the iteration mode**, measured rather than asserted: a warm
  full run is ~34s and a fast one ~20s (single runs, 2026-11-03, after M2-1 removed the review-rule steps; the full run was 60s before),
  because the steps it drops are the ones whose *runtime* dominates — `clippy`, the external
  corpora (v1-v3) and the direction gold set. It keeps
  `fmt`, `test` (nextest, and the report it writes), `canon`, the seed parity bench, the Baseline
  and decision steps, the release gate, the projection check, the extractor's self-check and the **quickstart rehearsal**
  (`tools/quickstart.sh`, measured at 0-2s, so it is kept in both modes — it prints the
  `QUICKSTART <n>s` line `docs/agentic.md`'s "five minutes" is measured by, and fails at 300).
  It cannot be mistaken for the gate:
  each skipped step prints **`FAST`** (never `SKIP`, because the tooling is there) and the banner
  reads `GATE: PASS (FAST: …)`, so a fast green is an iteration verdict — **the full run is
  required before a commit**, and CI always runs the full one. `--fast` composes with `plan`;
  beside `baseline` or `decision` it is a usage error (exit 2), because one check is not the gate
  and neither is fast.
- Direction: `cargo run -q -- compare <a.yml> <b.yml>` — classifies the change
  (`modality_weakening`, `numeric_tightening`, `polarity_flip`, `scope_divergence`, …),
  exit **3** when a side has no structured form to compare, and when a side declares a
  file format this build does not write — a claim whose parse is therefore **NOT JUDGED**,
  the same class and the same loader contract `req canon --verify` refuses, never a
  classification over a declaration the build cannot establish. A classification is for a
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
- Views: `cargo run -q -- render [--out <path>] [--check <path>] [--format text|json]
  [key=value ...]` and `cargo run -q -- trace [--out <path>] [--check <path>]
  [--format text|json] [key=value ...]` — the spec view and
  the traceability matrix for a selection (design step 9). Both are **projections**: they read
  claims, never write one. `render` shows applies / undecided / outside-the-selection with each
  claim's address, modality, statement, selector and outgoing relations; `trace` prints the
  edges, the **derived** incoming edges (REQ-0020 stores each relation once, on the outgoing
  side), and its findings. Exit **0** settled, **3** something left open — `render` answers 3 for
  any claim the selection leaves undecided, `trace` for any edge into a claim the corpus does not
  hold or one the variant leaves undecided — **2** usage. Two rules carry over and must not be
  relaxed: `render` shows an undecided claim **in its own section** (never dropped), and
  `trace` **names** every edge that leaves the selection — to an excluded claim (legitimate,
  reported) or to an undecided one (a gap needing a decision) — and names a relation to a
  claim not in the corpus `UNVERIFIABLE`, never ignored.
  **JSON rendering (pack C)**: `--format text|json` defaults to `text`, which is byte-identical
  to a bare command; `--format json` prints the same view as a `req.render/1` (or
  `req.trace/1`) document on stdout — `schema`, `generator`, `rules_v` as payload data, the
  `selection`, the rows/edges and (render) the `identity` digest over the sorted applying
  addresses. The document is a rendering of the verdict, never a new class of answer: the exit
  code is the same under both formats. It is stdout only — `--out`/`--check` are text
  projections, so `--format json` beside either is a usage error (exit 2), naming the follow-on.
  A row is one shape for both commands (`claim_id`, `statement`, `modality`, `address`,
  `applies`, `selectors`, `relations`, `rationale`); the audit surface's internals stay out.
  **A projection may be committed** (ADR 0018): `--out <path>` writes the view with a one-line
  HTML-comment header naming the `generator`, `rules_v`, `selection`, applying-claim `claims`
  count and an `identity=sha256:<hex>` digest over the sorted applying addresses, exiting **0**
  written / **3** cannot write. The file is complete even when the plain command's verdict is
  open — the exit class belongs to the verdict, not the artefact. `--check <path>` regenerates
  the projection and compares **bytes** (never the header alone) and writes nothing: **0**
  current, **1** stale or hand-edited (the first differing line is named; regenerate with
  `--out`, never edit the file), **3** cannot judge — no header, another build's `rules_v` (the
  same class as a Baseline's mismatch), a selection this build cannot reproduce, another
  command's projection, or an unreadable corpus — **2** usage (`--out` with `--check`, a missing
  value). The header's selection is what the artefact is judged under; an explicit context
  beside `--check` must name the same one or it is a second question (3). A committed projection
  is **checked, never edited**; `.req/projections` is this repository's pinned list (one
  repo-relative path per line), read by `scripts/check.sh`, which runs the check per path, fails
  when a listed path is missing or the list checked none, and prints how many it checked.
- Impact and release gate: `cargo run -q -- impact <claim-id> [key=value ...]` walks the
  relations **backwards** (derived, REQ-0020) and reports every dependent nearest-first
  with its selection state; `cargo run -q -- gate [--test-report <junit.xml>]
  [--format text|json] [key=value ...]` runs
  six policies —
  `addresses_current`, `relations_resolve`, `dependencies_decided`, `rationale_present`
  and `tests_linked` over the **applying** claims, and `selectors_confirmed` over the
  **invocation**: a variant
  in which nothing applies verified nothing; a `key=value` entry no applicability predicate
  in the corpus mentions decided nothing at all; and an `Eq` entry whose value the corpus
  never declares is refused, naming the values it does — `gate field=val` prints
  `field == value`, because the entry silently dropped REQ-0018 out of the release. The rule
  is predicate-aware: a `Ge` key is confirmed by comparison (`runs=100` is legitimate), and a
  real variant the corpus does not describe (`deployment_mode=cloud`) is refused **on
  purpose** — the gate clears only what the model describes, and the friction belongs at the
  decision boundary (ADR 0006). `gate` exits **0** every policy passed **and** every claim was
  decided, **1** a defect, **3** something that cannot be confirmed (an unfilled claim, a
  relation that does not resolve, a dependency left undecided, an entry the corpus cannot
  confirm, an empty selection) **or any claim left undecided** — the claims are named with the
  remedy, and a bare `gate` on a corpus carrying a selector is therefore UNVERIFIABLE (ADR
  0015) — **2** usage; **Fail outranks** an Unverifiable, because "we found a problem" is
  stronger than "we could not tell", and an undecided claim outranks a pass. Both keep the
  tri-state honest: a claim the variant **excludes** is not the variant's problem, and an
  **undecided** one is never treated as absent. An `impact` answer that a broken relation
  could have made incomplete says so (exit 3) instead of quietly shortening the list.
  **Test evidence (ADR 0019)**: `--test-report <junit.xml>` feeds the `tests_linked` policy,
  a sibling of `rationale_present` over the applying claims. It reads the run's report and
  answers four ways, and there is no fifth:

  | outcome | condition | fold |
  |---|---|---|
  | **PASS** | the claim links at least one test and at least one linked test **passed** | contributes pass |
  | **FAIL** | every linked test is present in the report and **none passed** (a `skipped` test is not a pass) | exit 1 |
  | **UNVERIFIABLE — *no linked test*** | the claim carries no `tests:` entry — the flagship answer, named per claim | exit 3 |
  | **UNVERIFIABLE — *linked test not in the report*** | a linked test id has no testcase in the report (renamed or deleted) — never folded into PASS | exit 3 |

  The four outcomes judge **machine** evidence; a **process declaration is a reported fact, not a
  fifth outcome**. A **`skipped` test is not a pass**, and a **failed** linked test outranks a passing
  one on the same claim (one pass does not launder a failure), while a skipped link beside a passing one
  is not a failure; an absent, unparsable or zero-testcase report
  is UNVERIFIABLE **once**, naming the reader's reason, never once per claim; and **FAIL outranks
  any UNVERIFIABLE, both outrank PASS** — the existing fold, no new code. **With no
  `--test-report`, `tests_linked` cannot be exercised and the gate is UNVERIFIABLE (exit 3)** —
  unless every applying claim declares `evidence: process`, in which case nothing machine-evidenced
  needs a report; a policy that cannot be exercised is not a pass. A test id is the test's
  *identity*, so a renamed linked test is *linked test not in the report* — UNVERIFIABLE, never
  PASS (the rename defence).
  **Process evidence**: a claim may declare `evidence: process` (the default is `machine`) when the
  process itself establishes it — a Decision, a signature, a workflow — and it must carry no `tests:`
  (naming both is a corpus load error, exit 3: a claim's evidence is machine or process, never both).
  The policy answers it **PASS as declared**, prints it under the `tests_linked` block
  (`* process-evidenced (declared, not machine-proven): …`) and counts it in the summary line;
  `req.gate/1` carries a top-level `process_evidenced` array (sorted claim ids) beside the six
  `policies`, with `tests_linked` after `rationale_present`. **An empty `tests:` list never implies
  process** — a machine claim with no link is UNVERIFIABLE, *no linked test*, and the default being
  load-bearing is mutation-checked; a process claim must carry no links at all.
  `--format json` prints the same answers as a `req.gate/1` document: the `selection`, the
  folded `verdict` (`pass`/`fail`/`unverifiable` — the same fold the exit code applies, computed
  once), the six `policies` with `pass`/`fail`/`unverifiable` outcomes and their `details` (which
  name the offending claims, never just a count), the top-level `process_evidenced` claim ids
  (declared, not machine-proven), and
  the `undecided` claims with the `missing` context keys. Exit codes are identical under both
  formats.
- Tier-2 seam: `cargo run -q -- propose <a.yml> <b.yml> [--provider-cmd <cmd>]` (ADR 0021) — **one
  explicit pair**, no blocking, no corpus walk, no gold set (that machinery is research, below). With
  no provider the answer is UNVERIFIABLE (exit **3**), never "unrelated" — the offline rule stands. With
  `--provider-cmd` (run through `sh -c`) req writes `ID:`, `A:`, `B:`, `DIFF:`, `CHOOSE:` to the
  command's stdin and reads a label (`equivalent|related|unrelated`) or a JSON object with `choice`
  and `confidence` from its stdout, and prints it as **`proposal (unratified): <label>`** with the
  command recorded verbatim (ADR 0004: the model proposes, a human disposes). It never writes a file
  and never changes an address. Exit **0** a proposal was produced, **3** no provider or a provider
  that failed or declined, **2** usage. Its linked test (REQ-0004) fails if it ever writes or reports
  an answer as anything but an unratified proposal.
- `experiments/tier2/` — the corpus-wide Tier-2 funnel and everything that measured it (blocking,
  gold-set scoring, `--max-pairs`, `--min-confidence`, the flip rate, the evaluation instruments,
  corpora, captures and the sample protocol), moved out of the core by ADR 0021. A **standalone crate**
  (`req-tier2`, its own `Cargo.lock`, `req` by path), **not** a member of the root workspace, so the
  root `Cargo.toml`/`Cargo.lock` pins are untouched. Research with honest negative results: **nothing
  in it gates a release**, and no claim links a test from it. `scripts/check.sh` keeps it compiling and
  its tests green (full run only) without feeding the JUnit report. How to run it:
  `experiments/tier2/README.md`.
- Baselines: `cargo run -q -- baseline --at <RFC 3339 UTC timestamp> [key=value ...]` prints the
  **canonical manifest** (envelope v4, ADRs 0016 and 0019) — the signing time, the Decision log's
  chain head, the rules version, the selection, and every applying claim's address, audit surface
  (`relations_hash`, `rationale_hash`) and its evidence declaration with the test links
  (`tests_hash`, the declaration hashed first) — which is the exact byte string a human signs. The signing time is **authored**: emitting without `--at` is a
  **usage error (exit 2)**, and so is a timestamp req cannot read; req never invents one.
  Emission also **refuses** when the decision log is absent, unparsable or empty — **exit 3**,
  a head that attests nothing must not be signed — and when the log is **tampered**, which is a
  definite finding and exits **1** (the same class `verify` and `req decision` give it).
  `cargo run -q -- baseline --verify <file> [--signature <file>] [--key <id>] [--now <date>]`
  verifies one. Exit **0** verified, **1** FAIL, **3** UNVERIFIABLE, **2** usage (including
  `--at` on the verification path — the manifest carries its own time). Verification re-derives
  every address, so a claim edited after signing is **FAIL** — the same class as a hand-edited
  digest; a v3-or-later manifest also attests the Decision log's **chain head at a position**
  (`entries`), so a rewritten attested prefix, a truncated log or a record whose stored hash no
  longer recomputes is **FAIL** (authority mismatch) — while decisions appended *after* that
  prefix do not invalidate an earlier Baseline (history growth is not tampering), and an absent
  or unparsable log is **UNVERIFIABLE**, never FAIL — absence is not tampering. A manifest whose recorded
  `rules_v` disagrees with this build's is **UNVERIFIABLE** with the re-sign remedy and drift is
  **not judged** — an address is derived from the rules version, so the comparison is void
  rather than inconclusive (an older manifest is re-signed under the current rules; a newer one
  is verified with a build that writes it, never re-signed with this binary). Trust is consulted
  at `baselines/trust.yml` (ADR 0010), and the key is judged at the **manifest's `signed_at`**
  for v3 and later (today for v2; `--now` is the explicit override): a key whose expiry is **at or after**
  the signing time verifies even when today is later, a key **expired at signing** is FAIL (it
  was not valid when the signature was made), and **revocation is prospective** — a key revoked
  *after* the signing time still verifies, while one revoked at or before it is FAIL. Both are
  judged by calendar date, and the signing time is **signer-asserted** (no RFC 3161), so
  revocation bounds newly asserted times only; `valid_from` is recorded but not enforced. An
  envelope **newer** than this build's is a separate case: its `signed_at` is not a spelling this
  build validates, so a **dated** revocation is not a definite finding there — it stays
  UNVERIFIABLE, with the remedy of a build that writes that envelope — while an anchor whose own
  `revoked_at` cannot be read still fails closed on every envelope, because that is not a
  question about a time. An unknown key or a
  missing signature is UNVERIFIABLE — a manifest is **not yet a Baseline** until a human signs
  it. The signature check is a **process boundary** to a pinned verifier, so the core links no
  cryptography and still no network; with no verifier installed the outcome is UNVERIFIABLE,
  never a pass. A **v2** manifest still parses (its new fields are `Option`), keeps its historical
  signed bytes, and is reported as *predates … re-issue and re-sign* — never a parse error.
  A **v4** manifest attests each claim's `tests_hash`, so a test list or evidence declaration changed
  after signing is **FAIL** — the same class as a changed address. A **v3** manifest still verifies on its
  historical signed bytes and says **once** that it predates v4, so its tests are **not
  attested** — never a FAIL, because it never claimed to attest them; a release that wants test
  attestation re-issues and re-signs as v4. A v4 manifest read by a v3-only build is the existing
  *newer envelope* UNVERIFIABLE, not a new class.
  **The signing act is the human's**: req emits and verifies, and a key this tool could reach
  would prove nothing. The **manifest records the variant**, so verification re-derives the
  addresses under *that* selection: an explicit context that contradicts it is a **usage error**
  (exit 2), never drift in the claims — re-deriving under the wrong variant reports the variant's
  own claims as "no longer applies", which reads as a defect in the requirements instead of a
  mistake in the invocation.
  **In CI the trust anchor is taken from origin/main, not from the pull request** — the same
  trust boundary as the gate and the rule file: a change cannot supply the anchor that judges it.
  Under that anchor, rotation is **two PRs**: add the new key beside the old (old-key signatures
  still verify), then re-sign; revoke or expire the old key only after both have merged.
- Audit packet: `cargo run -q -- audit [--baseline <file>] [--format text|json]
  [key=value ...]` — the last compiled
  view: for every **applying** claim it prints the address, the coverage state, the provenance
  (rationale source and date) and the outgoing relations, then the **undecided** claims in their
  own section, then its findings. Exit **0** every applying claim covered and still matching,
  **1** a defect (drift, or no provenance), **3** nothing ratified, something uncovered, or
  coverage the build cannot judge because the manifest's `rules_v` disagrees with it — reported
  **NOT JUDGED**, with no claim covered or drifted — **2** usage. The selection comes from the
  Baseline being audited, the same rule as `--verify`.
  Coverage is **derived from `baseline::drift`**, not recomputed, so the packet and
  `req baseline --verify` cannot disagree about whether a claim still matches what was signed
  — including when neither may judge it: a manifest whose `rules_v` disagrees with the build's
  is reported coverage **NOT JUDGED**, with no claim covered or drifted.
  With no `--baseline` it still prints and says plainly that nothing is ratified — an audit that
  passed on an unsigned model would be worse than no audit. It reports **coverage**, and says
  so: whether a signature is *valid* belongs to `req baseline --verify`, and this packet never
  asserts authority it did not check.
  `--format json` prints the same packet as a `req.audit/1` document: `ratified` (whether a
  Baseline was supplied), the `coverage` lists (`not_judged` as its reason string, `null` when
  no Baseline was supplied and nothing was compared), and the packet's `findings` in order, each
  string carrying the class the text packet prints (`FAIL: …` / `UNVERIFIABLE: …`). Exit codes
  are identical under both formats.
- Decision log: `cargo run -q -- decision [--fill] [--verify]` — the recorded decisions and their
  **hash chain** (design step 8, ADR 0005). One decision per file under `decisions/DEC-*.yml`
  (zero-padded so file order is chronological), each committing to its predecessor by
  `sha256("req.decision.v1" ‖ prev_hash ‖ canonical content)`. `--fill` writes the derived
  `prev_hash`/`hash` **into** the record, preserving comments and the note a human wrote; it never
  overwrites the file, and it refuses — writing nothing at all, exit **1** — to change a derived
  field already present that disagrees with the content, because recomputing over it would launder
  the edit `--verify` reports as TAMPERED.
  `--verify` (the default) walks the chain. Exit **0** VALID, **1** TAMPERED,
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

### Review

**CI reviews; local self-review is retired** (owner ruling 2026-11-03). `review.yml` runs `ocr review` and
blocks the merge on any critical or high finding; the verdict script (`tools/review/ocr_gate.py`), the
rule file (`review/rule.json`) and the trust anchor come from `origin/main`, never from the PR. It runs
with **`--no-filter`**: comments are **unverified**, so green means *the reviewer reported nothing*, not
that nothing was there — confirm each finding against the code. Rule-file facts that still bite:
- **Unknown keys are silently ignored.** A custom rule **replaces** the built-in rubric for its paths.
- **A glob matching nothing silently falls back, and nothing checks it any more** (checker removed,
  M2-1; risk accepted): rename a path and its rule together.
- **Markdown and `*.minisig` are outside the reviewer's view**: read docs by hand; a signed change
  always carries its re-signature, which the gate's Baseline step verifies.

**Reading a failed CI review.** The Forgejo job-log API answers 404 for these runs, so the
findings of a failed `OCR Review / ocr-review` cannot be read from the PR or the API. They are on
the runner: CI runs in CT 114 (`lxc-ci`) on the Proxmox host `pve`, as the `forgejo-runner` user,
and OCR keeps every review as one session file under
`/home/forgejo-runner/.opencodereview/sessions/*/<uuid>.jsonl`. This needs the maintainer's ssh
access to `pve` (`pct exec 114 -- …`); ask before assuming it, and **read only** — touch nothing on
the runner, and never open `~forgejo-runner/.npmrc` or the runner's config (they hold tokens).
The first line of each session names the commit it reviewed (`"diffTo":"<sha>"`), so match the
failing check's head sha, then print the findings from the `review_item_done` records:

```bash
ssh root@<pve> 'pct exec 114 -- python3 -c "
import json,sys
for l in open(sys.argv[1]):
    r=json.loads(l)
    for c in r.get(\"comments\") or []:
        print(c[\"severity\"].upper(), c[\"path\"], c[\"start_line\"], c[\"content\"][:700])
" /home/forgejo-runner/.opencodereview/sessions/<dir>/<uuid>.jsonl'
```

Find the session with `head -n1 <file> | grep diffTo` over files newer than the push. A record
with `type: session_end` carries `llm_failures` and `run_manifest.terminal_state`: a review that
did not complete is a failure (see the gate above), and that is where it says so. The model that
reviewed is in the first line too; with `--no-filter` its findings are unverified, so confirm a
finding against the code before acting on it — the PR #80 review was right on both highs, but
the same diff can give different findings on another run.

`tools/ocr` is the CI pin of the review CLI: bump it deliberately and state it in the commit.


## Where things live / how data flows

- `README.md` — the human-facing front page: what req is, the bet, the commands,
  and the honest status of both tiers. Kept consistent with this file; where the two
  disagree, one of them is wrong.
- `CONTEXT.md` — glossary only; no implementation detail.
- `docs/design.md` — design of record: the six tensions, the layers, the build
  order and its gates.
- `docs/adr/` — numbered decisions; a superseded ADR keeps its text and gains a
  status line (ADR 0008 → 0009 is the precedent).
- `docs/canonical-hashing.md` — Tier-1 rules, the two tiers, freeze policy,
  benchmark protocol.
- `docs/claim-format.md` — one claim per file; the parity invariant; authoring rules.
- `docs/adopting.md` — how a project adopts req, on one page: install at a pinned rev,
  `req init`, **adopting from documents you already have** (the extractor's draft path,
  `tools/adopt.py`), the authoring loop and its traps, the CI snippet with the per-variant rule,
  and where a req bug is reported.
- `docs/agentic.md` — the **agentic** quickstart, one page: the same adoption path for an agent and
  its human in five steps (install at a pinned rev, `req init`, the extractor, `req canon` /
  `--fill` / `--verify`, `.req/variants` + `gate`), then what the agent may not do, what it can
  read (`--format json` documents, the exit contract, `render --check` as the drift alarm), and one
  line naming the honest limit of the test link (a link is a claim about evidence). Rehearsed by
  `tools/quickstart.sh`, which prints `QUICKSTART <n>s` and fails at 300 — "five minutes" is
  measured, never asserted.
- `examples/` — copy-ready adopter files, inert by location (neither GitHub nor Forgejo picks a
  workflow up under `examples/`): `github-workflows/req.yml`, the CI snippet — the *same bytes*
  `req init` prints and `docs/adopting.md` carries, pinned by `tests/ci_template.rs` rather than
  assumed — and `agents-snippet.md`, the block to paste into a project's own `AGENTS.md`.
- `tests/ci_template.rs` — the three-way byte equality of that snippet (the `init` const, the
  adopting page's fenced block, the example file). One copy of the advice, or a red build.
- `claims/REQ-*.yml` + `claims/README.md` — the dogfood corpus: req's own
  requirements, in req's own format.
- `aliases/subjects.yml` — subject registry. Alias sets must be disjoint; an
  unregistered surface stays literal (never silently collapsed). The registry rides the same
  reviews; its edits are self-detecting through address recomputation — no origin/main pinning
  is applied to it.
- `bench/corpus/parity.yml` — the seed E/N pairs and adversarial traps, labels written
  from the spec. `bench/corpus/external-v{1,2,3}.yml` — other projects' real
  requirement prose, labelled by construction, built by
  `tools/corpus/build_external.py` (sources pinned by commit SHA; v3 adds the PROMISE
  NFR set, a deterministic sample of 969 statements across 47 project ids).
  `bench/results/` — one record per corpus and rules version; append, do not rewrite (the Tier-2
  records among them cite the pre-move paths and stay as history, ADR 0021).
  The Tier-2 corpora (`tier2-*`), their capture directory, `build_tier2.py`, the evaluation
  instruments (`evaluate.py`, `agreement.py`, `entropy.py`, `validate.py`, the decomposer pair) and
  the sample protocol live in `experiments/tier2/` (corpora in `corpus/`, scripts in `tools/`, the
  protocol in `docs/`); their `--self-check`s are run by hand from the repository root, not by the gate.
  These corpora are **generated data**: a corpus of this size is reviewed through its
  generator (`tools/corpus/`) and its label scheme, not line by line — the generator,
  the source pins and the sampling rule are the reviewable surface, and the review
  preview lists the corpus itself as a single item for that reason.
- `src/` — `lib.rs` (the library surface), `main.rs` (thin CLI over it), `root.rs`
  (the runtime corpus root: `--root`, `REQ_ROOT`, walk-up — ADR 0012), `model.rs`
  (claim/registry/corpus types), `canon.rs` (Tier-1 pipeline + diagnostics),
  `bench.rs` (the benchmark gate + its tests), `select.rs` (tri-state variant
  selection), `compare.rs` (change direction), `view.rs` (render/trace + the shared
  row), `impact.rs` (reverse traversal), `gate.rs` (release policies),
  `baseline.rs` (Baseline manifest, drift, trust and verification),
  `audit.rs` (the audit packet, derived from the Baseline's drift),
  `decision.rs` (the decision log and its hash chain),
  `propose.rs` (the single-pair Tier-2 seam, ADR 0021), `fill.rs` (derived fields), `init.rs` (the corpus bootstrap and the
  templates it writes), `junit.rs` (the reader for nextest's JUnit subset: parsed, unparsable, or
  zero test cases — the last two UNVERIFIABLE, never "no failures found").
- `baselines/` — `trust.yml` is the committed trust anchor (ADR 0010) and the **single home of
  the public key**; `<variant>.json` is a signed Baseline's canonical manifest and
  `<variant>.json.minisig` its signature; `trust.example.yml` is the template. The standalone
  `req.pub` that minisign writes is deliberately **not** kept here: two copies of one key drift,
  and `minisign -R -s <secret key> -p <path>` regenerates it from the secret half on demand. req
  emits and verifies; it never signs — the act is `minisign -Sm <manifest>`, which writes
  `<manifest>.minisig` (the name `--verify` looks for) and prompts for the secret key's passphrase.
  `docs/adopting.md` carries the adopter-facing form.
- `tools/ocr/` — the pinned review CLI (`package.json` + lockfile committed;
  `node_modules/` ignored).
- `tools/review/ocr_gate.py` — the verdict on an OCR review result: pass, fail, or a legitimate
  docs-only skip. Pure decision function plus a `--self-check` table, so its logic is tested
  without a model. The workflow enforces the copy from `origin/main`, never the pull request's.
- `tools/adopt.py` — the document extractor (issue #44): drafts one `unstructured: true` claim
  per statement the canonicalizer leaves unstructured, with the document **and line** as
  provenance and **nothing derived** — `canonical.form` is the empty placeholder and `address` is
  `null`, so `req canon` reports 0 flags and `req canon --fill` is what completes it (and refuses
  to write over a contradicted parse). The candidates are checked through that same `--fill`
  derivation in a throwaway corpus first: a statement that parses as a structured claim
  (typically a `shall` with a subject and an action) is **reported as skipped, never written**,
  because its slots are the author's. Two conventions only (`bullets`, `req-lines`; a source's
  `REQ-<n>` id is ignored because req allocates the id), an optional heading scope, every
  skipped line named with its reason, a **dry run by default**, and a refusal to overwrite or to
  re-draft a document already adopted. Its `--self-check` table-drives the pure half and is in
  the gate; the corpus end-to-end is `tests/adopt.rs`. It is a starting point adopters **copy
  and adjust**, not a subcommand — the mapping from a document to claims is per-project.
- `review/rule.json` — the review rules we own: one checklist per path class, passed
  by the CI review. The rule-file facts worth knowing are under **Review**.
- `.req/variants` — this repository's own release list, read by `scripts/check.sh` and by any
  CI that runs it (req itself knows nothing about the file): one `key=value ...` release per
  line, `#` comments, blank lines skipped. Two releases are declared because `corpus_set` takes
  two mutually exclusive values — no single context can apply both REQ-0008 and REQ-0010. Each line
  must decide every scoped claim: dropping `set`, `deployment_mode` or `field` leaves that
  claim undecided in that release, which since ADR 0015 makes `req gate` exit 3 rather than 0. Only
  `corpus_set` is dependency-guarded, so `dependencies_decided` refuses (exit 3) for the claims that
  relate to REQ-0008; for the rest the undecided count is the guard. `tests/release_gate.rs`
  requires zero undecided claims per declared line, and the step fails if no release was gated at
  all.
- `.req/projections` + `docs/spec.md` — the pinned list of committed **projections** (ADR 0018),
  read by `scripts/check.sh`, and this repository's first instance of the mechanism. One
  repo-relative path per line, `#` comments, blank lines skipped — the `.req/variants` grammar —
  and `scripts/check.sh` runs `req render --check <path>` for each, fails when a listed path is
  missing or the list checked none, and prints how many it checked. `docs/spec.md` is generated by
  `req render --out docs/spec.md` from this corpus (16 applying, 5 undecided) and is never
  hand-edited: its header names the generator, `rules_v`, selection, applying-claim count and an
  `sha256:` identity, and a regenerated file replaces it in the same commit as the claim that
  moved it.
- `.config/nextest.toml` — the reviewed nextest profile: `[profile.ci] fail-fast = false`, and
  `[profile.ci.junit]` carrying `path = "junit.xml"` and `report-skipped = "ignored"` (under the
  `junit` table, because the top-level spelling is silently ignored in 0.9.149). Its artefact
  `target/nextest/ci/junit.xml` is what `req gate --test-report` reads, and `report-skipped` is
  what makes a skipped test *present* in it — which is how "a skipped test is not a pass" is
  observable rather than assumed.
- `.scratch/` — session scratch, gitignored; never a deliverable.
- Path facts: there are none to pin, and that is the design. Every subcommand resolves its corpus
  at runtime (ADR 0012) — `--root <dir>`, else `REQ_ROOT`, else the nearest ancestor of the working
  directory holding `claims/` — so **ask the shell** (`pwd`, `git rev-parse --show-toplevel`)
  instead of trusting a path written down here: a hardcoded path describes the deployment it was
  written for and misleads every other. The workspace *name* follows the checkout directory; it is
  not a fixed label. A read-only mount is a condition the reader checks before writing (`test -w
  <dir>`), never a path the reader is told exists.
- Data flow: claim file → typed parse (`yaml_serde`) → Tier-1 canonical form →
  canonical bytes → `sha256("req.claim.v1\0" ‖ bytes)` = address. Only `req bench`
  (results), `req canon --fill` (derived fields) and `req init` (a new corpus) write
  anything — `--fill` writes only what the parity invariant confirms, and `init` derives
  the starter claim through that same path and verifies it before reporting success.

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
  Changing one requires a superseding ADR, not an edit in passing. ADR 0020 (accepted) is the
  precedent: it replaces only ADR 0001's identity wording — `claim_id` is the identity, the address a
  meaning fingerprint — and 0001's decision (the unit is a claim) stands.
- Unit is a **claim**, not a document (0001). Git is transport/serialization, not
  identity or merge (0002). Variants are **selectors**, not branches (0003). The
  model **proposes**; a human disposes (0004). Fingerprints collapse only on
  meaning-preserving rules (0007). Authority is a signed **Decision**, not a commit
  hash (0005). Friction at the decision boundary, none in authoring (0006). Core is
  Rust; porcelain is TypeScript, later (0009).
- **Git-native storage is table stakes, not the moat** — never sell it as one. The
  differentiators are semantic fingerprinting and variability selectors.
- Never claim the bet is validated on seed evidence; the external corpora run is the
  headline measurement.

---

## Orchestration (delegated work)

Work is run from one small **orchestrator** thread; the bulk goes to delegated children. This is
process, not product: the *task envelope* below is unrelated to the Baseline's `baseline_envelope`.

- **Specs are committed** under `docs/specs/<milestone>-*.md` before a child starts: a worktree
  child sees only committed files (the untracked `prompts/` is invisible to it).
- **Out:** a pointer-only brief (`docs/specs/brief-template.md`) — task id, spec path, branch
  point, report contract, red test, verify command, forbidden paths. Never a restated spec.
- **Back:** the only accepted final message is the JSON object in
  `docs/specs/task-envelope.schema.json`, checked with
  `python3 tools/orchestration/validate_envelope.py <file>` (its `--self-check` is in the gate).
  A fix unit (`task_id` ends `-fix`) carries `red` and `mutations` — this file's TDD and
  mutation-check rules, made reportable. A malformed envelope is re-delegated once, then the
  task is `blocked`.
- **Writers** run as `delegate_task` children with `isolation: "worktree"`, one writer per
  worktree; they push a feature branch and open a pull request, never `main`. A **reviewer** runs
  on a different model family, read-only, and returns an envelope whose `task_id` contains
  `-review-`.
- The orchestrator reads the envelope and `git diff <branch-point>...<branch> --name-only`
  against the spec's allowed paths — a stray path rejects the branch — and does not read a
  child's transcript. Models resolve through `orchestrator_capabilities` and `p3-models.md`,
  never a hardcoded slug.
- **Milestone close:** `docs/handoffs/<milestone>-close.md` (template:
  `docs/handoffs/TEMPLATE.md`), then a fresh orchestrator thread seeded with that file.

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

### Merge locally (no PR)

**No longer how changes reach the remote.** `main` is protected on Forgejo and this
environment cannot push it, so a local `--no-ff` merge strands a commit the remote cannot
see: work in a worktree, push the feature branch, open a pull request, and after the human
merges, fast-forward locally with `git fetch origin && git pull --ff-only`. What follows
remains the shape of the work.

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

**Phase 1 (the local habit).** Local gate (`scripts/check.sh`), worktree per feature,
the PR's CI review (see **Review**) is the review that blocks merges.

**Phase 2 (partly landed, 2026-10-13).** The Forgejo remote is
`https://git.xantu-barb.ts.net/fdx/req.git`, and `.forgejo/workflows/ci.yml` invokes
this same gate there — one implementation, two invocations, no check logic in workflow
YAML. **Changes reach the remote by feature branch and pull request**, so a local
`--no-ff` merge into `main` strands a commit the remote cannot see; the agent
environment cannot push `main` at all (EDH003), which is that guard behaving as
intended. After a merge, fast-forward locally. Branch protection on `main` requires exactly that context, read back from the API rather than
assumed, so a pull request merges only on a green run of the real gate.
`.forgejo/workflows/review.yml` adds a second required check once its gate is on `main`.

