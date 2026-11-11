# 0038. A leading condition is a condition, not part of the subject (rules v8)

Extends T1.8 of `docs/canonical-hashing.md` (guard clauses); nothing else in it, and no ADR, is superseded.

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-10, GitHub PR #43); issue #18. Date proposed: 2026-11-10.
**A rules change: `RULES_V` 7 → 8.** The Baseline `baselines/air-gapped.json` must be re-signed by the owner before this can be released
(the signed `rules_v` differs; the 18 attested claim hashes do not move, see *Consequences*).

## Context

An adopter's second pilot (req `1e5fa60`, rules v6) drafted fifteen statements and found that a statement opening with a condition,
"When a document is sent to a third-party provider, the provider shall be recorded in the findings file", has the whole clause read into the
**subject**: `when a document is sent to a third-party provider, the provider`. The same obligation written with its condition last
("… shall be recorded … when a document is sent to a third-party provider") already puts the condition in the condition slot (T1.8).

The consequence is the one ADR 0004 is built around: the subject is in the address, so rewording only the *trigger* ("is sent" → "is
transmitted") moves the subject, and `req compare` classes the pair `unrelated` (two different requirements), the strongest class it has,
instead of a change to the obligation's condition. The reviewer is told the obligation is gone when it only has a new trigger.

## Decision

1. A sentence that **starts** with a guard word (`when`, `while`, `if`, `unless`, `during`, and `where`) reads
   `<guard> <condition>, <subject> <modal> …`. The condition is the words between the guard and the **last comma before the subject**
   (the comma dropped), recorded as `"<guard>: <condition>"` exactly as a trailing guard is; the rest of the sentence, from the subject on, is
   parsed as it always was. A leading condition is taken **before** the trailing guards, so a guard word inside it
   ("When the valve is open while the tank is idle, …") stays in it.
2. **Only an unambiguous sentence is read.** It needs a modal phrase, a non-empty condition and a non-empty subject on either side of a comma.
   With no comma before the subject — **the common spelling, "When a game is ended the product shall …"** — or with an empty side, the statement
   parses exactly as it did under rules v7, the whole clause in the subject. A parser that guessed where a comma-less condition ends would put
   a word in the wrong slot, and a wrong slot is least visible where it is most consequential (ADR 0014).
3. **`where` is a guard in the leading position only.** As a trailing word it is a place or a relative pronoun ("log the change where it is
   configured") far more often than a condition, and making it a trailing guard would move addresses of statements that have nothing to do
   with this issue. At the start of a sentence it is the EARS optional-feature condition.
4. **The leading and the trailing form of one obligation are one claim**: "When X, Y shall Z" and "Y shall Z when X" have the same canonical
   form and the same address. This is a collapse, so it is argued here: it is a reordering of two clauses that changes no word, no condition
   and no subject, and the new parity pair E-011 is its gold label. It is the meaning-preserving case ADR 0007 permits.

## Consequences

- **Addresses move** for statements that begin with a guard word and have the comma. None of req's own claims is one, and none of the 66
  guard-initial statements in `external-v3`, `external-v4` and the direction gold set has the comma, so no recorded pair moved: parity is
  25 → 29 pairs (four hand-written, one equivalent, three not-equivalent), FMR 0/18, FSR 0/8; every external and direction count is identical to
  rules v7 (`docs/canonical-hashing.md`, *Recorded runs*). **The new rule therefore has teeth only in the parity pairs and
  `tests/leading_condition.rs`; it is not an external result,** and it does nothing for the comma-less spelling.
- **Re-canonicalization candidates** are the adopter claims that begin with a guard word and a comma, which `req canon` shows with a
  `conditions:` line where there was none. The claim files are untouched (nothing derived is stored, ADR 0022); their addresses change when the
  adopter takes rules v8, and a Baseline signed at v7 stops verifying until it is re-signed.
- **Dogfood:** `req canon` 0 flags; the re-emitted manifest differs from the signed one in `rules_v` alone, so there is no candidate in this
  repository. The owner re-signs (`req baseline --at … --out baselines/air-gapped.json deployment_mode=air_gapped`, then `minisign -Sm`).
- `req compare` on a reworded leading trigger now says `scope_divergence` (the obligation stayed, its condition was replaced); adding or removing
  a leading condition is `scope_narrowing` / `scope_widening`.
- `req selftest`'s address goldens were regenerated deliberately for the new version (`GOLDEN_RULES_V` 8): all seven are unchanged, since none
  of them starts with a guard word.

## Rejected

- **Reading a comma-less leading condition.** "When a game is ended the product shall allow …" has no marker where the condition ends. Splitting at
  a guessed noun-phrase boundary is the T1 mistake this repository has paid for twice (the prefix subject match, the dropped tail). If it is
  wanted, it is a Tier-2 proposal or an authoring rule ("put a comma after the condition"), not a Tier-1 guess; `docs/claim-format.md` says so.
- **Making `where` a trailing guard.** See decision 3.
- **Leaving the leading position out of scope and documenting it** (the issue's fallback). The trigger is exactly what a reviewer needs to see
  change; calling its rewording an unrelated requirement is the failure this tool exists to avoid.
