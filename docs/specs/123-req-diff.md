# #123 — `req diff`: what changed in meaning between two revisions

Task ids: `123-1` (the command), `123-2` (the adopter docs and a copy-ready pull-request workflow; dispatched only
after `123-1` has merged). Code baseline: current `main`. Issue #123; the decision is **[ADR
0026](../adr/0026-req-diff-what-changed-in-meaning-between-two-revs.md)** (proposed; dispatched only after it
merges). Read the ADR first: the contract is there and is not restated here.

**Model:** `claude-sonnet-5-5` (high) for both units. `123-1` is reviewed read-only on a different family
(`123-1-review-…`): it decides what a reviewer is told did not move.

## Why

A pull request that changes a requirement's meaning is reviewed today as a YAML hunk. The classifier exists
(`compare`), the loader exists (`canon::load_claims`), the pin judgement exists (`Derived::pins`); nothing joins
them across two revisions. No `RULES_V` change, no address change.

## Unit 123-1 — `req diff`

1. **The command** (`src/diff.rs`, new; register in `src/main.rs`'s `Command` table with a `DIFF_HELP` page whose
   `exit:` line states 0 / 3 / 2 and, with `--fail-on`, 1, as ADR 0026 §3-4 decide).
   - Arguments: `--base <rev>` (required), `--head <rev>` (default: the working tree), `--fail-on <list>`,
     `--format text|json` (use `format::take_format`, as `gate`/`check` do), and the global `--root`. Anything
     else, a missing `--base`, an empty or unknown `--fail-on` class, a value that begins with `-`, an extra
     positional: exit 2 with `usage: req diff --base <rev> [--head <rev>] [--fail-on <classes>] [--format text|json]`.
   - **Reading a revision.** Find the corpus root as every command (ADR 0012). It must lie inside a git work tree
     (else exit 3 naming that). The root's path relative to the work tree's top is the prefix under which
     `claims/` and `aliases/` live in a revision. Materialise each revision's `claims/` and `aliases/` (and nothing
     else) into a temporary directory **without checking anything out**: `git ls-tree`/`git cat-file --batch`
     or `git archive` of those two paths, as the child judges simplest, and then call the existing loaders
     (`canon::load_registry`, `canon::load_claims`) on that directory so there is **one** place that judges a claim
     file. The working-tree side uses the root directly. The temporary directory is removed on every exit,
     including a signal.
   - **The `git` process.** `git` resolved from `PATH`; before every call remove the `GIT_*` variables that
     redirect it (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`,
     `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_CONFIG`, `GIT_CONFIG_COUNT`, `GIT_TEMPLATE_DIR`) and set
     `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`; no command used may run a
     hook, fetch or touch the network. A spawn failure or a non-zero `git` is exit 3 with git's own message.
     `--base`/`--head` values are passed after `--end-of-options` or verified to resolve with `rev-parse --verify
     --quiet <rev>^{commit}` first; one beginning with `-` is a usage error before any process runs.
   - **Matching and classes** exactly as ADR 0026 §2. Use `compare::compare` on the two `Derived::form`s for the
     direction and `Direction::is_loosening` for `weakened`; map the other directions as the ADR says. A claim whose
     two statements are byte-identical and whose non-identity fields are equal is counted, not listed. The
     direction word printed is `Direction::as_str()`, the same word `req compare` prints.
   - **Flags** as ADR 0026 §2: `pins-changed` compares the authored `pins` structurally (not the file bytes);
     `pin-flagged` is a pin of the **head** side whose `PinCheck` disagrees; `tests-lost`/`tests-added` compare the
     `tests:` id sets; `same-address-as <id>` pairs an `added` claim with a `removed` one of equal address.
     `registry: changed` is decided by comparing the two sides' `aliases/subjects.yml` bytes (absent on both = unchanged).
   - **Output.** Text: a first line `# req diff: <base> -> <head>` (labels as given, the resolved short SHAs
     beside revisions, `working-tree`), a `registry:` line, one line per listed claim
     `<class> <claim_id> [<direction>] [flags…]` sorted by claim id, then a counts line, then, when `--fail-on` is
     given, `fail-on: <list> — <n> triggered` / `none triggered`. The words PASS, FAIL and UNVERIFIABLE appear
     nowhere in stdout (they are verdicts; stderr may carry `[UNVERIFIABLE] <cause>` for exit 3 as `compare` does).
     JSON: `req.diff/1` as ADR 0026 §6; keys sorted, no float. Add `req.diff/1` wherever `req.check/1` is declared.
   - **Exit** as ADR 0026 §3-4: 0 report produced; 1 only with `--fail-on` and a named class or flag present;
     3 cannot be produced; 2 usage. A listed class that does not occur is not a failure.
2. **Tests** (`tests/diff.rs`, new; public seam = the compiled `req` run against temporary git repositories the
   test builds with real commits; expectations hand-written or taken from labelled corpus pairs, never from
   `diff`'s own output). Red first, at least: (a) identical revs → exit 0, counts only, nothing listed;
   (b) SHALL→SHOULD → `weakened`; (c) `within 200 ms`→`within 300 ms` on an upper bound → `weakened`, and the
   reverse → `tightened`; (d) a pair taken from `bench/corpus/parity.yml` whose label is *equivalent* → `reworded`;
   (e) a pair labelled *not equivalent* by a changed bound → not `reworded`; (f) added and removed ids;
   (g) a removed and an added claim with equal addresses → `same-address-as` on the added one, both still
   `added`/`removed`; (h) only a `tests:` link removed → class `metadata`, flag `tests-lost`; (i) an edited `pins:`
   → `pins-changed`, and a pin that no longer holds on head → `pin-flagged`; (j) an edited `aliases/subjects.yml`
   → `registry: changed`; (k) `--fail-on weakened` with a weakening → exit 1, without one → exit 0, and the same
   weakening without `--fail-on` → exit 0; every bad `--fail-on` spelling → exit 2; (l) `--head` absent compares
   the working tree including an uncommitted edit; (m) an unknown rev, a repository-less directory, no `git` on
   `PATH`, a base side whose claim declares `schema: req.claim/1`, an empty `claims/` at one side → exit 3 and
   no report on stdout; (n) a corpus root that is a subdirectory of the work tree is read at the right prefix;
   (o) `--format json` is `req.diff/1`, parses, and its classes equal the text's; (p) `GIT_DIR`/`GIT_CONFIG_GLOBAL`
   exported by the caller do not change the result (a config that rewrites line endings must not matter);
   (q) the output holds none of `PASS`, `FAIL`, `UNVERIFIABLE` on stdout; (r) a `--base` starting with `-` → exit 2,
   no process spawned. Extend `tests/cli_version.rs`'s command list and help expectations for `diff`.
3. **Docs in this unit:** the `AGENTS.md` command-table row (`diff` — what changed in meaning between two
   revisions) and the `req --help` command list; nothing else (`123-2` owns the adopter pages).

### Mutation checks (envelope `mutations`)

- Match claims by address instead of id → (g) fails. Classify every changed meaning as `reshaped` → (b)/(c) fail.
- Treat `weakened` as outside `--fail-on` → (k) fails. Return 0 when `git` is missing → (m) fails.
- Do not clean `GIT_*` / config → (p) fails. Leave the temporary directory behind → a test lists the temp dir
  before and after and fails.
- Compare pins by file bytes instead of structurally → a pin reordered but equal wrongly reports `pins-changed`.

### Allowed paths (123-1)

`src/diff.rs` (new), `src/main.rs`, `src/lib.rs`, `src/canon.rs` and `src/compare.rs` (visibility changes only,
no behaviour change), `tests/diff.rs` (new), `tests/cli_version.rs`, `AGENTS.md`. Forbidden: `claims/**`,
`baselines/**`, `bench/**`, `aliases/**`, `decisions/**`, `review/**`, `.forgejo/**`, `examples/**`, `docs/**`,
`scripts/**`, `Cargo.toml`, `Cargo.lock`, `src/init.rs`, and any change to the canonicalizer, `RULES_V` or an
address. No Baseline re-signing.

## Unit 123-2 — the adopter page and a pull-request workflow (after 123-1 merges)

1. `docs/adopting.md`: one section, *On a pull request*, after the CI section — what `req diff` answers, the
   classes and flags in a table, `--fail-on` with the recommended starting set (`weakened,removed,pins-changed,tests-lost`),
   that it is a description and not a verdict, that `weakened` is a prompt for a human, the registry-change caveat,
   and that its direction quality is `req compare`'s. State the `git` and `fetch-depth` requirement (the base revision must be in
   the checkout).
2. `examples/github-workflows/req-diff.yml` (new, a separate file: **not** one of the three pinned copies of
   `req.yml`): on `pull_request`, a checkout with `fetch-depth: 0`, the same install step as `req.yml` (copy its
   body, do not reinvent it), then `req diff --base "origin/${{ github.base_ref }}" --fail-on …` unpiped, no
   `|| true`, no `continue-on-error`, and the output also written to the job summary from a captured copy without
   masking the exit. Inert by location like every file in `examples/`.
3. `tests/diff_template.rs` (new): string pins — the trigger is `pull_request`, `fetch-depth: 0`, the command is
   unpiped and unmasked, the install body equals `examples/install-req.sh`'s (modulo indentation), no
   `REQ_RELEASE_BASE`/`REQ_CURL_PROTO`. Mutation-check each.
4. Allowed paths: `docs/adopting.md`, `examples/github-workflows/req-diff.yml` (new), `tests/diff_template.rs`
   (new), `README.md` (one line in its command list if one exists). Forbidden: everything `123-1` forbids except
   `docs/adopting.md`/`examples/**`/`README.md` as listed, plus `src/**`.

## Verify (both units)

`HOME=<main checkout>/.tools/home bash scripts/check.sh`, full run, **read `$?` before committing**, exit 0, no
SKIP, Baseline `[VERIFIED]` (unchanged signature), `QUICKSTART <n>s` present. No claim, address, Baseline or
`bench/results` change.
