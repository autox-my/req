# D-1: draft ADR 0056, typed claim fields (proposed)

Task id: `D-1`. Base: the commit that adds this spec, on top of `origin/main` at `960e7ed`. The owner asked on 2026-11-17 for
the typed-fields ADR, the first of two decisions informed by the X-1 experiments. The second, repository-defined checks, comes
later and depends on this one.

**Deliverable:** one proposed ADR, `docs/adr/0056-<slug>.md`, in the house style of ADRs 0051–0055. Read those first. Include
the Status line "proposed; the owner ratifies it by merging", Context, Decision (numbered), Consequences, Rejected and
Falsification. **No code.** The owner ratifies by merging. Add any new glossary term to `CONTEXT.md` in the same commit,
for example *Kind* or *Field*, if the ADR introduces one.

**Model:** writer `claude-sonnet-5-5`; reviewer `deepseek-v4.1-flash`, read-only.

## The evidence it rests on (read all of it)

- `docs/research/fields-separation.md`:
  - `rationale` and `tests` stay out of the address and are attested;
  - `pins` is not attested (ADR 0022's named loss);
  - attestation is wiring across nine sites, listed in §2.3;
  - the twelve leak risks, each with the test that catches it;
  - `diff` has no failure class for a rationale-only change;
  - the verify failure names no hash;
  - `compare` says identical.
- `docs/research/checks-expressibility.md`: two of the ten rules need typed fields. Note which.
- ADRs 0013 (provenance hashes), 0022 (nothing derived in a claim; the pins loss), 0026 (diff classes), 0051 (area: placement,
  deliberately unattested; cross-cutting classifications are not areas and were left undecided), 0004 and 0005 (a source
  status is not ratification), 0027 and 0028 (the importers do not carry status).
- The Baseline envelope and its versions (`src/baseline.rs`); `CLAIM_SCHEMA_V` and `READS_AS_IS` (`src/canon.rs`).

## What the ADR must decide (each explicitly, with the reason)

1. **The claim file shape.**
   - `kind:` and `fields:`, or another spelling.
   - Whether this needs `req.claim/4`, or can be an optional addition to `/3`. Weigh what a claim-schema bump means: it is
     an identity axis in `release-notes.sh` and makes an adopter migrate. Weigh how `READS_AS_IS` treats older formats.
2. **Where kinds are declared:** `.req/kinds/*.yml` or one file. The field types: string, integer, exact decimal (the
   canonical decimal string rule), bool, enum, list, claim reference, date. Also `required`, defaults, and what happens to
   an unknown kind, an unknown field, a wrong type, or a missing required field. Follow the loader's strict refusal
   discipline and the three-state rule.
3. **Identity.** Fields are outside the canonical form and never move an address. Say how that is enforced and tested: the
   leak risks from the research record.
4. **Attestation.**
   - A `fields_hash` in a new envelope v5: what it covers and its canonical serialisation.
   - What v5 means for existing v4 signatures; the research record measured that a v5 manifest is exit 3 today.
   - **Decide whether the same envelope bump also closes ADR 0022's pins loss with a `pins_hash`.** Recommend one answer,
     with the trade-off: one re-signature against two.
5. **Change reporting:**
   - a `diff` flag and a `--fail-on` class for a field change;
   - whether `compare` reports field changes;
   - whether the verify failure should name which hash failed, a gap the research found.
6. **What fields are not.**
   - A field such as `status: approved` is never ratification; ratification stays a signed Decision (ADRs 0004 and 0005).
     Say how the docs prevent the misreading.
   - Fields are not areas (ADR 0051).
   - Fields are not applicability: a variant stays a selector (ADR 0003).
   - Say whether a selector may read a field (probably not in this ADR). If it may not, name it as future work for the
     checks ADR.
7. **What stays outside:** inheritance, unions and tuples, and importer mapping. The importers may later map source
   attributes to fields; say whether that is in scope.

## Rules

- Every factual claim about today's code cites a file and line, or a command. Every claim about the research cites the record.
- **No customer or company material.** Examples are invented: ASIL is fine as a generic safety attribute.
- **Falsification:** conditions an implementation would break, at least the leak risks that matter most.
- **Do not overclaim:** the separation is "partly proven" per the research record, so the ADR must not call it proven.

## Allowed paths

- `docs/adr/0056-*.md` (new)
- `CONTEXT.md` (a new term, only if needed)

## Done when

- The full `scripts/check.sh` is green with no SKIP.
- `git diff <base>...HEAD --name-only` stays inside the allowed paths.
- The PR body summarises each decision in one line and states the recommended answer on the pins question.
