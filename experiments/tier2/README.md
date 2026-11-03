# experiments/tier2 — the Tier-2 funnel, as research

This is the former `src/funnel.rs` and everything that measured it, moved out of the core by
[ADR 0021](../../docs/adr/0021-tier-2-leaves-the-core.md). It is **research with honest negative
results**: blocking, a provider seam, gold-set scoring, a confidence floor and a flip rate, plus the
instruments, corpora and captures behind the Tier-2 measurements. **Nothing in it gates a release.**
`scripts/check.sh` keeps it *compiling* and its own tests green (`cargo check --locked` and `cargo
test --locked` on this manifest, full run only); neither step feeds the JUnit report the release gate
reads, and no claim in `claims/` links a test from here.

What the core keeps is one seam, `req propose <a.yml> <b.yml> [--provider-cmd <cmd>]`: an external
provider's answer about one explicit pair, printed as an *unratified proposal*, never an edit.

## Layout

| Path | What it is |
| --- | --- |
| `Cargo.toml`, `Cargo.lock` | a standalone crate (`req-tier2`), **not** a member of the root workspace; depends on `req` by path. The root `Cargo.toml`/`Cargo.lock` are identity-path pins and are not touched. |
| `src/funnel.rs`, `src/main.rs` | blocking, `Provider`/`CommandProvider`, gold-set scoring, `--max-pairs`, `--min-confidence`, flip rate; the `req-tier2` binary is the former `req propose` |
| `tests/` | `funnel.rs`, `funnel_flags.rs` (the CLI flag parser at its seam) |
| `corpus/` | the Tier-2 gold set (`tier2-v1.yml`, built by `tools/build_tier2.py`), the pilot worksheets and fillings, the positives set and its captures |
| `tools/` | `evaluate.py`, `agreement.py`, `entropy.py`, `validate.py`, the decomposer pipeline, `req-d1.sh` and `thx-serve.py` (provider wrappers), `build_tier2.py` |
| `docs/tier2-sample-protocol.md` | the sample protocol, its ceiling measurement and the negative results |

Measured results live in `bench/results/` (append-only history, untouched by the move); records there
that cite `bench/corpus/tier2-*` or `tools/tier2/` describe the tree as it was.

## Running it

All commands run from the **repository root** (the instruments write their result records under
`bench/results/`, and the corpus root is found by walking up to `claims/`). Use the pinned toolchain
(`scripts/check.sh` resolves it).

```bash
cargo test --locked --manifest-path experiments/tier2/Cargo.toml          # the crate's own tests
cargo run --locked -q --manifest-path experiments/tier2/Cargo.toml -- \
  --gold experiments/tier2/corpus/tier2-v1.yml --repeats 2 --provider-cmd 'sh experiments/tier2/tools/req-d1.sh'
python3 experiments/tier2/tools/evaluate.py --capture FILE --provider ID \
  --gold experiments/tier2/corpus/tier2-v1.yml                            # score a captured run offline
python3 experiments/tier2/tools/agreement.py --self-check                 # the Python instruments each carry one
```

With no `--provider-cmd` the offline provider answers `Unverifiable` on every pair: Tier-2 metrics are
UNVERIFIABLE in an environment with no provider, never reported as zero. The flip-rate threshold the
retired REQ-0011 stated (at most 2 percent) is still the rule this crate reports against.
`validate.py --self-check` prints SKIPPED, not a pass, when PyYAML is not installed.
