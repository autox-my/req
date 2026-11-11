# 0034. `req selftest`: known-answer checks of the installed binary

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-10, GitHub PR #33); asked for by the owner on
2026-11-10 (no issue). Date proposed: 2026-11-10. Adds a command; changes no existing contract, no `RULES_V`, no address.

## Context

An adopter installs a prebuilt `req` (ADR 0025) or builds one, and from then on every verdict depends on that binary deriving the
same addresses and directions as the project's reference build. Today the install proves two things: the tarball's SHA-256 is
the pinned one, and `req --version` says `built from <rev>`. Neither shows that the binary **computes** the right thing **on this
machine**: a platform that differs in some arithmetic or encoding detail, a build from a dirty tree, or a rules change whose
reference answers were not updated would all pass both. The project's tests prove the behaviour on the project's CI, not on the
adopter's host.

## Decision

1. **`req selftest`**, a command that runs a fixed set of **known-answer checks** and exits 0, 1 or 3. The answers are compiled in
   (`src/selftest.rs`), so it reads no corpus, resolves no root, needs no network, and works in any directory, like `--version`.
   It takes no argument, and `--root` is a usage error (there is nothing for it to name).
2. **What it checks**, each as a `PASS`/`FAIL` line: a SHA-256 test vector (NIST); what four statements must derive, written from the
   hashing spec's worked examples (`within 200 ms` is LE, 0.2, s; `less than` is LT; a prohibition; an enumerated set, ADR 0030); three
   pairs that must share an address and four that must not (the LE-versus-LT trap among them); seven address pins; the direction of six
   hand-labelled changes; and the seed parity corpus (`bench/corpus/parity.yml`, with its registry) through the false-merge gate.
3. **Two kinds of expectation, kept apart.** The field rows, the same/different pairs, the direction labels and the parity labels are
   **independent oracles**, written from the spec by hand. The address pins are **regression pins** frozen from the implementation at a
   rules version: they detect change, not error. The command and its help page say so.
4. **A rules change cannot forget its pins.** The pins carry the `RULES_V` they were recorded under, and a compile-time assertion refuses
   a build where that differs from `canon::RULES_V`. Bumping `RULES_V` therefore fails the build until the pins are regenerated in the
   same commit, which is the rules-change protocol (hashing spec §5) made mechanical.
5. **Not a defence against a tampered binary**, and the first paragraph of the page says so: a binary that lies about its own checks
   passes them. Integrity is the pinned checksum's job (ADR 0025); this checks that an unmodified build behaves.
6. **Environment is a note, not a check.** It prints `info:` lines for `git` and `minisign`, which `diff`, `pin` and `baseline --verify`
   need, and never counts them: a machine without `minisign` can still derive addresses correctly.
7. **Exit codes:** 0 every check passed; 1 at least one failed (the line says what was derived and what was expected); 3 UNVERIFIABLE,
   when the answers compiled into the build cannot be read (a build defect, never a pass); 2 usage.

## Consequences

- The check is a table in the source: adding a known answer is a reviewable one-line change, and a test makes each *kind* of check
  fail on a deliberately wrong answer (`tests/selftest.rs`). Mutating the real rules (the comparator table, the modality table, the
  guard words, polarity detection, set serialisation) turns the command red, observed when it was written.
- The binary embeds about 8 KB (the parity corpus and the subject registry). The corpus a build embeds is the corpus of its rev.
- The command is **not** a replacement for the gate, which checks the corpus; it checks the tool. `docs/adopting.md` suggests running
  it once after an install.
- It is evidence of one thing, that this build behaves as the reference does on this host, and it is listed that way in the safety manual.

## Rejected

- **A script shipped beside the tarball.** A second artifact to pin, and it would test a `req` found on `PATH`, not the one installed.
- **Running `req bench` for this.** It needs a checkout of the corpus, which an adopter's CI does not have.
- **A flag on `--version`.** `--version` answers instantly and offline by contract; running a suite behind it would change what
  every script that calls it pays.
- **Generating the expected values from the implementation at run time.** That is a check that cannot fail.
