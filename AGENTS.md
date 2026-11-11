# AGENTS.md — req
req: the git-native requirements engine for meaning drift and its direction. A **claim** is the unit of work; one claim per file in
git. Identity is the `claim_id`; the address is a content-addressed *meaning fingerprint* over canonical
meaning (ADR 0020). Specs, traceability matrices and audit packets are compiled views over a signed
baseline. The bet, stated so it can fail: deterministic Tier-1 canonicalization yields **zero false
merges** on the parity + adversarial-trap corpora. It is *measured* (`req bench`), never asserted.

Sources of truth, in order: `docs/design.md` → `docs/adr/` (supersession recorded, never silent) →
`docs/canonical-hashing.md` (Tier-1 rules, freeze policy, benchmark protocol) → `docs/claim-format.md`
(the file contract) → `CONTEXT.md` (glossary **only**). **Each rule has one home**: a command's contract is
its `req <cmd> --help` page, the adopter's loop is `docs/adopting.md`; this file restates neither.

## Agent rules
- You are an autonomous workflow assistant: execute the SDLC below step by step, keep state across stages, count
  error handling and edge cases as "done", keep existing architecture styles; if unsure, ask one targeted question.
- **Vocabulary comes from `CONTEXT.md`** (Claim, Requirement, View, Variant, Decision, Baseline); add a missing term there in the same commit.
- **Nothing derived lives in a claim file** (`req.claim/2`, ADR 0022): the parse, canonical form and address are
  computed at load. Never add an `address`, a `canonical` block or a parse slot to a claim — the loader refuses it,
  naming the key; state a load-bearing slot as a **pin** (`pins:`), and never edit a pin to fit a parse.
- **Never weaken the asymmetric cost.** A false merge silently deletes a requirement — always a failure; a
  false split is noise. Never auto-merge; ratification is a human act.
- **Version pins are provenance, not stragglers** (not to be confused with a claim's `pins:`): `rust-toolchain.toml`, `Cargo.lock`, `tools/ocr/package-lock.json`.
  Verify a version at *pin time*; never bump opportunistically. The first two are fingerprint-path pins: any
  bump is a Tier-1 rules bump (bump `RULES_V`, regenerate goldens deliberately, record re-canonicalization
  candidates, hashing spec §5). The OCR pin is review-process provenance: bump deliberately, verify
  `ocr --version`, state it in the commit — no `RULES_V` bump.
- **Never change the git identity** or pass `-c user.name=`/`user.email=`: commits are the human's, provenance feeds ADR 0005.
- **External material is read at named provenance, or as a lead.** A number with a source, a sample size and
  its author's commercial interest stated is evidence; one without is a lead, never a finding to repeat.
- **Do not overclaim.** A seed-corpus pass is not the external-corpora falsification; report which corpus
  produced which number. **Nothing leaves the machine**: the core has no network dependency, providers are
  a process boundary, air-gapped operation works with zero network (REQ-0012).

## Correctness rules
- **FMR = 0 on the corpus is a hard gate.** FAIL or UNVERIFIABLE blocks; never relax it to land a change.
- **Tier-1 determinism**: no `f32`/`f64` math, no `HashMap` iteration where an address is computed (sort
  keys or `BTreeMap`), no locale-dependent formatting, no model I/O. Six clippy lints in `Cargo.toml`
  (`as_conversions`, `cast_possible_truncation`, `cast_sign_loss`, `allow_attributes_without_reason`,
  `unwrap_used`, `float_arithmetic`) enforce this on every path, not only on the ones reviewed.
- **Three-state outcomes**: PASS / FAIL / UNVERIFIABLE. An unreadable corpus, a degraded registry or a
  skipped check is reported as such, never folded into PASS; `degraded` entries are part of the record.
- **The statement is the source of truth**: its parse is *derived*, never authored. Never "fix" a flagged pin by
  editing the pin to the parse — re-word the statement or leave the flag; `pins_hold` fails the gate on it.
- **Claims are data**: `value` is a canonical exact decimal string (`"0.3"`, never `"0.30"`, never a float);
  a rationale is an object (source, date, rejected alternatives, assumptions).
- **Docs are claims too**: when `docs/` asserts a rule the code contradicts, fix one of them in the commit
  that exposed it. **Test expectations are independent**: corpus E/N labels come from the spec, never
  regenerated from the implementation. **No secrets; stage by name** (`git add <paths>`). External
  reference material is read-only and never quoted verbatim into our docs.

## Project discipline (do not renegotiate silently)
- The **six tensions** (`docs/design.md` §2) and **ADRs 0001–0009** are settled; changing one needs a superseding
  ADR, not an edit in passing (ADR 0020 is the precedent). The unit is a claim, not a document (0001); git is
  transport, not identity or merge (0002); variants are **selectors**, not branches (0003); the model **proposes**,
  a human disposes (0004); authority is a signed **Decision** (0005); friction at the decision boundary, none in
  authoring (0006); fingerprints collapse only on meaning-preserving rules (0007); core Rust, porcelain TS later (0009).
- **Git-native storage is table stakes, not the moat**; the differentiators are the meaning fingerprint as a
  drift-and-direction signal (not a paraphrase collapser, README "Status") and variability selectors. Never claim the
  bet is validated on seed evidence.

## Commands
Contracts live on `req <command> --help` (usage, the `exit:` line; works with no corpus); the loop is
`docs/adopting.md`. The corpus root resolves at runtime (`--root`, else `REQ_ROOT`, else the nearest ancestor
holding `claims/`; ADR 0012); `init` alone resolves none. **Rebuild before probing a branch**: compare `req
--version`'s `built from` with `git rev-parse --short HEAD`; `rules_v` is the first check when a verdict
surprises you. Run as `cargo run -q -- <command>`.

| command | the question it answers |
| --- | --- |
| `init` | start a corpus in the working directory |
| `selftest` | does this binary, here, derive what the project's reference build derives (known answers compiled in; reads no corpus) |
| `canon` | what each claim's statement derives, and whether each pin holds (diagnostics; `--fill`/`--verify` removed) |
| `bench [corpus.yml]` | the FMR = 0 gate (seed parity by default); also scores the direction gold set |
| `compare <a> <b>` | the direction of a change between two claims — for a human to ratify |
| `diff` | what changed in meaning between two revisions |
| `pin` | is the corpus exactly the commit a consumer pinned, and how has it moved since |
| `select` | which claims apply in a variant (tri-state: UNKNOWN is an answer) |
| `render`, `trace` | the spec view, the traceability matrix (projections; `--out` / `--check`) |
| `impact <id>` | what depends on a claim, nearest first |
| `next-id` | the next free claim id(s); `--reserve` holds a block in `.req/reserved` so parallel authors do not collide |
| `gate` | may this selection ship (six policies, tri-state) |
| `check` | may this repository ship: every declared variant and projection, plus the structural omissions (ADR 0039), folded (ADR 0023) |
| `baseline` | the manifest a human signs; verify one (req never signs) |
| `audit` | the audit packet: coverage, provenance, undecided |
| `decision` | the decision log and its hash chain |
| `propose <a> <b>` | ONE pair put to a provider: an unratified proposal (ADR 0021) |

## The gate
`scripts/check.sh` is the single local gate (run it with `HOME=<main checkout>/.tools/home`, or fmt/clippy
SKIP). It runs every check whose tooling exists, prints `SKIP` (never `PASS`) for missing tooling and fails on
the first non-zero status; the Baseline step has no SKIP (no minisign is UNVERIFIABLE, exit 3, and blocks).
**Green before every commit and merge**; state any skip or `UNVERIFIABLE` in the commit message. Order: build →
`cargo nextest run --profile ci` (writes `target/nextest/ci/junit.xml`) → `req gate --test-report <it>` once per
`.req/variants` line. `--fast` is the iteration mode (~20s against a longer full run; drops clippy, the external corpora, the
direction gold set and the reproducible-build check): **the full run is required before a commit**, and CI always runs it. `cargo nextest run
--profile ci` replaces `cargo test` (nextest pinned 0.9.149, a CI-side pin and not a `Cargo.lock` change; profile in
`.config/nextest.toml`; doctests are the named gap). **Never pipe a check through `tail`/`grep`**: the pipeline's
status is not the command's — check `$?` on the cargo command.

## Review
**There is no automated review workflow in this repository: a human reads every pull request** (ADR 0029 §7, which
ends the owner ruling of 2026-11-03 that "CI reviews"). CI is `.github/workflows/ci.yml`, the same
`scripts/check.sh` as local, read-only and with no secret; it reviews nothing. The review tooling stays for a
maintainer to run **by hand**: `tools/review/` (the verdict script `ocr_gate.py`), `review/rule.json` (the rules) and
`tools/ocr/` (the pinned CLI). Its output is **unverified** (`--no-filter`): a clean run means *the reviewer reported
nothing*, not that nothing was there — confirm each finding against the code. Unknown rule-file keys are silently
ignored; a custom rule **replaces** the built-in rubric for its paths; a glob matching nothing silently falls back and
nothing checks it (risk accepted) — rename a path and its rule together. Markdown and `*.minisig` are outside its view.
A review workflow for GitHub is a decision of its own (it would hold an LLM credential, which does not belong in a
workflow that runs on pull requests from strangers).

## Where things live
- `README.md` front page (consistent with this file); `CONTEXT.md` glossary; `docs/adopting.md` the one adopter page,
  rehearsed offline by `tools/quickstart.sh` (`QUICKSTART <n>s`, fails at 300); `examples/` copy-ready adopter files
  (the CI snippet is pinned three ways by `tests/ci_template.rs`).
- `docs/releasing.md` the owner's runbook for a prebuilt release (ADR 0025): `scripts/build-release.sh`, `release.yml`.
- `claims/` req's own requirements; `aliases/subjects.yml` the subject registry (disjoint alias sets);
  `decisions/DEC-*.yml` the decision log (hash chain); `docs/spec.md` generated by `req render --out`, never hand-edited.
- `bench/corpus/` the seed parity corpus plus external families — **generated data** from `tools/corpus/`, reviewed
  through the generator and its pins; `bench/results/` append-only, one record per corpus and rules version.
- `src/` `main.rs` thin CLI + help pages, one module per command; `tests/` public-seam tests.
- `baselines/` `trust.yml` (ADR 0010, the **single home of the public key**), `<variant>.json` + `.minisig`: `air-gapped.json`
  and one `release-*.json` per `.req/variants` line; the gate verifies every one. req
  emits and verifies, never signs (`minisign -Sm`). In CI the trust anchor is taken from `origin/main`, not the
  PR; rotation is two PRs (add the new key beside the old and re-sign; revoke the old only after both merge).
- `.req/variants` this repo's release list; `.req/projections` pinned projections; `.req/completeness` roots and strictness for
  the completeness step (optional); `.config/nextest.toml`; `tools/`
  (`adopt.py` the extractor, `needs_import.py` its Sphinx-Needs sibling and `reqif_import.py` its ReqIF sibling: copy and adjust; `ocr/` the pinned review CLI; `review/`; `orchestration/`);
  `review/rule.json` the review rules; `experiments/tier2/` the Tier-2 research crate (ADR 0021), nothing in it gates
  a release; `docs/specs/`, `docs/handoffs/` orchestration; `.scratch/` gitignored, never a deliverable.

## Orchestration (delegated work)
One small **orchestrator** thread; the bulk goes to delegated children. **Specs are committed** under
`docs/specs/<milestone>-*.md` before a child starts (a worktree child sees only committed files). Out: a
pointer-only brief (`docs/specs/brief-template.md`), never a restated spec. Back: only the JSON envelope of
`docs/specs/task-envelope.schema.json`, checked by `python3 tools/orchestration/validate_envelope.py <file>`; a
fix unit (`task_id` ends `-fix`) carries `red` and `mutations`. Writers are `delegate_task` children with
`isolation: "worktree"`, one per worktree; they push a feature branch and open a PR, never `main`. A reviewer runs
on a different model family, read-only, `task_id` containing `-review-`. The orchestrator checks `git diff
<branch-point>...<branch> --name-only` against the spec's allowed paths (a stray path rejects the branch), never
reads a transcript, and resolves models through `orchestrator_capabilities` and `p3-models.md`. Milestone close:
`docs/handoffs/<milestone>-close.md`, then a fresh thread.

## SDLC
Work in a worktree (`git worktree add worktrees/<name> -b feature/<name>`; guard EDH001 refuses writes outside the
session's checkout). **TDD is mandatory for code** (the session `edh-tdd` skill): agree the seam
(`canonicalize_statement`, the registry resolvers, the bench classification, the CLI exit codes), write the
failing test first, then only enough code; tests use public interfaces and independent oracles. **Rules-change
protocol**: a Tier-1 rule change moves addresses — bump `RULES_V`, regenerate goldens deliberately, record
re-canonicalization candidates, say so in the commit. **Mutation-check the load-bearing rule**: delete the rule
behind each trap class (LE-vs-LT N-002, modality N-001, unit factors E-002/E-005, condition extraction N-004,
subject boundary N-006, ambiguous comparator N-005) and confirm its corpus test fails. Commits: `feat:` `fix:`
`refactor:` `test:` `docs:` `chore:` (scope optional); pre-convention history stands. Changes reach the remote by
**feature branch and pull request** — an agent never pushes `main` (EDH003); after the human merges,
`git fetch origin && git pull --ff-only`. CI (`.github/workflows/ci.yml`) runs the same
`scripts/check.sh` — one implementation, two invocations, no check logic in workflow YAML. Branch protection on
`main` (the owner's GitHub setting) requires its `gate` check, so a PR merges only on a green run of the real gate;
there is no second required check, and a human reads the diff.
