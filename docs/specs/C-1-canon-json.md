# C-1: `req canon --format json`, the `req.canon/1` document

Task id: `C-1`. Base: the commit that adds this spec, on top of `origin/main` at `b7504e9`. The owner asked for it on
2026-11-18 as the first step towards the repository-checks ADR. `docs/research/checks-expressibility.md` §1 found that
the derived parse (operator, value, unit), the lints and the pins are in no JSON document today: `req canon` prints them
as text only, and `req canon --format json` is a usage error (exit 2).

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

The contracts are:
- `src/format.rs`'s module doc (the JSON rules every `--format json` command follows): text stays the default; `--format`
  never changes an exit code; the output is deterministic; there is no float; the schema string is versioned
  independently of `RULES_V`.
- ADR 0022: nothing derived lives in a claim. This document shows what is derived; it writes nothing.
- `req canon --help`, which is the command's contract.

No new ADR is needed. This adds one more document of the same family as `req.trace/1` and `req.render/1`.

## Choices fixed here

1. **The flag.** `req canon --format json` prints a `req.canon/1` document on stdout. Use the shared `take_format`
   (`src/format.rs`) and add `SCHEMA_CANON = "req.canon/1"` beside the others.
   - `--format text` is today's output, byte for byte.
   - `--fail-on-flag` works under both formats, with the same exit.
   - `--by-area` with `--format json` is a usage error (exit 2) naming the combination. The summary stays text-only in
     this unit.
2. **The document.** The top level holds `schema`, `generator` and `rules_v`, as the other documents do (read what
   `req.trace/1` writes and follow it), then `claims`: one object per loaded claim, ordered by `claim_id`. Each claim
   object has these keys, in this order:
   - `claim_id`;
   - `address`;
   - `structured`, a boolean. It is false exactly when the statement takes the unstructured fallback.
   - `form`: the claim's canonical form as the JSON object of its canonical bytes (`Derived::canonical`, T1.9). Parse
     those exact bytes and embed the result, so its keys and spellings are the frozen T1.9 ones and not a second
     rendering. Do **not** re-derive or rename them.
   - `lints`: an array of `{"code", "detail"}`, in the order the canonicaliser raised them.
   - `pins`: an array, in `Pins::entries` order, of `{"key", "pinned", "verdict", "derived"}`.
     - `verdict` is `agrees`, `contradiction` or `not_carried`.
     - `derived` is the value the parse holds for a `contradiction`, and `null` otherwise.
     - An empty array when the claim pins nothing.
   - `area`: `null` when absent.
   - `kind` and `fields`, exactly as `req.trace/1` nodes render them (F-3):
     - `kind` is `null` when absent;
     - `fields` is an object of the fields present, keys sorted by byte;
     - integers and decimals are strings, a `bool` is a JSON boolean, and a list is an array sorted by byte.

     Reuse F-3's serialiser; do not write a second one.

   Nothing else goes in: no rationale, relations, tests or statement text, and no Baseline or decision data. Those live
   in `req.render/1` and `req.trace/1`, and a later schema version may add more, additively.
3. **Same answer as the text.** The document renders what the text path already computed. A test compares the two over
   this repository's corpus, field by field: for every claim, the JSON's modality, subject, operator, value, unit,
   lints and pin verdicts equal what the text line says. A disagreement fails it.
4. **Refusals.** A corpus that does not load is exit 3 with the same message as text, and **no JSON on stdout**. A
   malformed `.req/kinds/` declaration is exit 2, the F-1 rule. Follow what `req trace --format json` does on a refusal
   and say which it is.
5. **Determinism.** Two runs are byte-identical. `BTreeMap` throughout, no float (a decimal is its canonical string,
   which T1.9 already guarantees inside `form`), compact JSON and one trailing newline, as the other documents end.

## Tests first (envelope `red`)

Use public seams only: the CLI over this repository and over scratch corpora. The oracles are the T1.9 canonical bytes
and the text output, never a run of the new code.

- `req canon --format json` on this repository:
  - exits 0;
  - `schema` is `req.canon/1`, and `rules_v` is 9;
  - `claims` is ordered by id and as long as the corpus;
  - each `address` equals the address `req trace --format json` reports for that claim.
- For every claim in this repository, `form` re-serialised compactly with sorted keys equals the claim's canonical
  bytes. Get the bytes from `req::canon::load_claims` (`Derived::canonical`) in the test; that is a public seam and is
  independent of the new printer.
- The text-agreement test of choice 3.
- A scratch claim with a contradicting pin gives `verdict: contradiction` and the parsed value in `derived`. A pin on an
  unstructured statement gives `not_carried`. No pins gives `[]`.
- A scratch `req.claim/4` claim with a kind and fields gives `kind`/`fields` equal to its `req.trace/1` node's. A claim
  with neither gives `null` and `{}`.
- An unstructured statement gives `structured: false`, and its `lints` match the text.
- `--fail-on-flag --format json` exits 1 on a flagged pin, and its stdout is still the full document.
- `--format json --by-area` is exit 2.
- A corpus that cannot load is exit 3 with empty stdout.
- Two runs are byte-identical.
- `--format text` output equals the bare command's.

Prove each red at the base, where `--format` is exit 2.

## Mutation checks (envelope `mutations`)

- Re-render `form` from `CanonicalForm` fields instead of parsing the canonical bytes, renaming one key: the
  `form`-equals-canonical-bytes test fails.
- Drop the lints from the document: the text-agreement test fails.
- Print `derived` for an agreeing pin: the pin test fails.
- Order claims by path instead of by id: the ordering test fails (use two scratch files whose path order and id order
  differ).

## Verify

- Full `scripts/check.sh` with `HOME=<main checkout>/.tools/home`: exit 0, no SKIP, tree clean after the run.
- `docs/spec.md`, `docs/trace.md` and the three Baselines are unchanged and still verify. `RULES_V` is still 9.

## Docs, one home each

- `req canon --help`: the `--format` flag, the document's keys, the `--by-area` refusal, and the refusal behaviour.
- `docs/claim-format.md` or `docs/adopting.md`, wherever the other `--format json` documents are listed. If neither lists
  them, leave both alone and say so.
- `docs/research/checks-expressibility.md` is a dated record. Do **not** edit it.

## Allowed paths

- `src/canon.rs` (the `req canon` command path only; nothing in `derive`, `parse_claim` or the canonical form),
  `src/format.rs`, `src/view.rs` (only to reuse F-3's field serialiser), `src/main.rs` (help pages).
- `tests/**`.
- `docs/adopting.md`, `docs/claim-format.md` (the document list only).

Forbidden:
- `RULES_V` and any change to the canonical form, T1.9 bytes or `derive`;
- `src/baseline.rs`, `src/model.rs`, `src/kinds.rs`, `src/diff.rs`, `src/gate.rs`, `src/select.rs`;
- `bench/**`, `claims/**`, `baselines/**`, `decisions/**`, `aliases/**`, `.req/**`;
- `docs/spec.md`, `docs/trace.md`, `docs/adr/**`, `docs/research/**`, `scripts/**`;
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/**`.

If a forbidden file must change, stop and report.
