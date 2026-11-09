# Mutation testing of the canonicalizer and the direction classifier

Run 2026-11-10 with `cargo-mutants 27.1.0` over `src/canon.rs` (Tier-1) and `src/compare.rs` (direction). Reproduce with
`scripts/mutants.sh`; it is **not** part of `scripts/check.sh` (a full run is about two hours, and it is evidence about the
tests, not a gate). cargo-mutants is a development tool, not a pin in `Cargo.lock`; this page names the version that produced
its numbers.

**Question.** The bet is that Tier-1 yields zero false merges on the corpora. A corpus pass says the tests *agree* with the
code on the cases someone thought of. It does not say that a change to a rule would be *noticed*. Mutation testing asks the
second question: change one operator, one constant or one branch at a time, and see whether any test fails. A mutant that
survives is a rule no test pins, and in this crate a rule nothing pins is a rule that can move an address silently.

**Status of the numbers.** One run, one machine, one tool version, two files. A *score* here is a statement about the test
suite, not about the correctness of the rules (a rule that is wrong and pinned scores perfectly). Not a safety argument.

## Result

| | before | after |
| --- | --- | --- |
| mutants generated | 632 | 632 |
| excluded (loaders needing git, see Limits) | 54 | 54 |
| unviable (do not compile) | 32 | 32 |
| testable | 546 | 546 |
| caught | 415 | 510 |
| survived (missed + timed out) | 131 | 36 |
| **score** (caught / testable) | **76.0 %** | **93.4 %** |

The 95 mutants killed came from two new test files, `tests/tier1_edges.rs` (35 tests) and `tests/direction_edges.rs`
(17 tests). **No source file changed.** Every test in them passed against the existing implementation once my own wrong
expectations were fixed (an absent unit is `None`, not `"none"`; `shall not exceed` is the absorbed comparison, SHALL with
`LE`, not SHALL-NOT; `one of the 100 standards, …` reads "the 100 standards" as a fourth member because three digits are
not a count). None of those disagreements was an implementation defect; each was checked against the code before the test
was changed, and the test's comment says why.

### What the survivors of the first run were

126 missed and 5 timeouts, in rules the corpora never exercise:

- word numbers above two, and most of the unit table: `ms`, `us`, `ns`, `mm`, `cm`, `ma`, `mv`, `%` had no test, so
  deleting a negative exponent would have changed `200 ms` to `200 s` with the suite green;
- the sign and decimal point of a number, digit-grouping separators, `normalize_word` case rules;
- guards as whole words (`shift` is not `if`), a chain bound of eight;
- the enumeration reader (ADR 0030) at its edges: header count digits, header colon, brackets, a second `or`, member length;
- `compare.rs`: the whole mirror table, strict-comparator moves, bounds that appear, incomparable families;
- `canonical_bytes` (no caller; now pinned equal to `canonical_json`) and `Derived::modality_label`, which is what a
  rendered view prints.

## The 36 that remain

Classified by reading the code, **not proven**. "Equivalent" means I could not construct an input that distinguishes the
mutant, with the reason; a reader who finds one has found a gap.

| count | where | why it survives |
| --- | --- | --- |
| 2 | `leading_numeric_part` :514 | a dead check: a digit was already required two lines earlier |
| 1 | `scale` :605 | `>` vs `>=` where `0` is handled by the branch above |
| 3 | `scan_modality` :1229 | tie-break on equal start; the phrase table lists the longest first, so a later phrase never wins a tie |
| 2 | `canonicalize_statement` :1618–1619 | `exception: None, temporal: None` repeats what `..Default` already gives |
| 7 | `compare` :178, :184, `numeric_direction` :241, :247 | the mutated test falls through to an arm that returns `ScopeDivergence` anyway |
| 21 | position arithmetic and guards in `canonicalize_statement`, `find_last_guard`, `word_span_start`, `value_led_subject` | **not shown equivalent.** They shift a span by one word or widen a guard; the T1.10 coverage invariant overlaps several of them, which may hide the difference. Follow-up, not closed. |

The 21 are the honest gap. They sit in the code that finds *where* the modal phrase, comparator and value are, which is
exactly where an off-by-one would mis-assign a word to a slot. The coverage invariant (T1.10) is the net under them: a
word no slot holds makes the claim unstructured, which is a false split, never a false merge. That is why I rank them
below the unit-table gaps, which could have moved a value silently.

## Limits

- **Two files of the core.** `gate`, `decision`, `pin`, `baseline`, `audit`, `diff`, `selftest` and `view` are unmutated
  (about 500 more mutants). Phase 2, not run. `baseline` and `decision` need git.
- **Git-dependent tests were not run.** cargo-mutants copies the tree without `.git`; these test binaries shell out to git
  and cannot run there: `baseline`, `decision`, `public_tree`, `projection`, `orchestration`, `junit_case`, `adopt`, `diff`,
  `corpus_pin`. The 54 excluded mutants are loader functions only those tests exercise. A mutant these tests would kill may
  therefore appear among the survivors; the 21 were not re-run with them.
- **Timeouts count as survivors.** The tool's 20 s limit was auto-set from the unmutated baseline.
- **The tool's default operators only** (replace a function body, flip an operator, delete a match arm or field).
- **Not the corpus protocol.** `AGENTS.md` asks that the rule behind each trap class be deleted by hand and its corpus test
  seen to fail (N-001, N-002, N-004, N-005, N-006, E-002/E-005). That stands; this is a broader, mechanical sweep beside it.
- The expectations in the new tests come from the definitions they test (SI prefixes, the hashing spec, ADR 0030), never
  from a run of the code.

## Reproducing

```
cargo install --locked --version 27.1.0 cargo-mutants     # nextest is already the project's runner
scripts/mutants.sh target/mutants-report                  # about 2 h here, -j 3
scripts/mutants.sh target/mutants-report --iterate        # rerun only what survived, after adding tests
```

The survivors of the final run are in `target/mutants-report/mutants.out/missed.txt` and `timeout.txt`.
