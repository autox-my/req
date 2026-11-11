# 0030. An enumerated "one of" is a set: the `IN` operator

**Status: accepted** — ratified by the owner's merge of the pull request that carries it (2026-11-09, GitHub PR #24); issue #17. Date
proposed: 2026-11-09. Extends T1.5 of `docs/canonical-hashing.md` (the closed comparator table); nothing else in it, and no
ADR, is superseded. ADR 0014's concern — a wrong slot is least visible where it is most consequential — is the reason.

Amended by [ADR 0044](0044-a-change-on-several-axes-has-a-direction-only-when-they-agree.md) (§6): the prohibition sense swap also applies to a numeric bound, not only to a set.

## Context

An adopter's evaluation of req (2026-11, rules v5, measured again on rules v6) found a shape the operator table
cannot hold: a *membership* rule, "every compliance sentence must name **one of** four named sources". Its
two observed forms were "name one of the four sources, A, B, C or D" and "record …, as one of three
observations: `a`, `b` or `c`".

Under rules v6 the parse of that shape is wrong in three ways, each measured on a real statement:

1. **The number is invented.** The word-number rule reads "one" as the bound `1`, so the canonical form
   carries `value "1"` that nobody meant. A `pins: value:` on the claim would pin a number the author did not write.
2. **The members are free text.** They fall into `qualifier`, so reordering them — a meaning-preserving edit —
   moves the address. That is a false split: noise, but noise on exactly the rule where the change that matters is a member.
3. **Direction is lost.** Dropping a source is a narrowing and adding one a widening, but `req compare` reports
   `scope_divergence` for both. A widening read as "a different thing" is the loosening that gets ratified
   unnoticed (`compare.rs`: "a loosened obligation reported as a no-op"). The change is not safe to leave at
   `scope_divergence` for the shape whose entire point is the membership.

The two workarounds the issue named are no better: an unstructured claim opts out of structural direction, and
one claim per member loses "one of".

## Decision

1. **Operator `IN`.** T1.5 gains `IN`. The set is `CanonicalForm::members`, lower-cased and held sorted and
   de-duplicated (`BTreeSet`), so the order the members are written in is not identity. There is no number:
   `value` is `None` and `unit` is `none`. `req canon` prints the set as `{a|b|c}`; a claim may pin `operator: IN`.
2. **The set rides in the `value` slot, as a JSON array.** `"value":["a","b","c"]` for `IN`; every other claim keeps
   `"value":"<decimal>"` or `null`. The schema's keys, and so every other claim's canonical bytes and address, do not
   move. A new always-present key was rejected: it would move every address in every corpus for a shape that
   occurs in none of ours.
3. **Only an enumeration is read.** A "one of" becomes a set only with a delimiter that says where the list is: a
   bracket (`one of {a, b, c}`), a colon (`one of the following: a, b or c`), a counted header
   (`one of the four sources, a, b, c or d`), or a bare list that ends in `or` (`one of a, b or c`). A
   "one of" with no enumeration (`one of the target market countries` — the set is defined elsewhere) parses exactly
   as in rules v6, bytes included.
4. **Anything the parser cannot read with confidence is refused, never guessed.** The claim is unstructured with a
   lint: `membership_count_mismatch` (the statement says *four* and lists three — a wrong count is a dropped
   member), or `membership_unclear` (a member holding a conjunction or `except`/`unless`, more than eight words, a
   duplicate member, fewer than two members, an unclosed bracket, two `or` in one item). Unstructured is a false
   split at worst, which is the cheap direction (AGENTS.md: *never weaken the asymmetric cost*).
5. **Every word stays in a slot (T1.10).** The members are the value; the noun the list is about ("standards") and
   any clause after the last member ("and no other standard") are `qualifier`, joined with ` | `. The article, the stated
   count (already checked against the members) and "following" are the only words that leave identity, and they are
   derivable from it or stopwords. "Closed" is not "open": a trailing "and no other standard" is a different
   obligation and is a different address.
6. **Direction.** `compare` gains `membership_narrowing` (fewer values comply) and `membership_widening` (more do).
   A member removed from a required set narrows it; added, widens it. For a **prohibition** the set is what is
   forbidden, so the senses swap (a member added to a forbidden set narrows what may be done). Members both added and
   removed, or a set against a non-set, is `scope_divergence` — never "no change". `req diff` classes them
   `tightened` / `weakened` with the numeric and scope directions; `is_loosening` includes the widening
   (*amended by ADR 0041: `is_loosening` is replaced by `Direction::effect`, which still counts the widening as permitting more*).
7. **`RULES_V` 6 → 7.** Per the rules-change protocol. Addresses computed under v6 are re-canonicalization
   candidates, never silently rewritten; the only claims whose address can move are those holding an enumerated
   "one of", and the committed corpus holds none.

## Measured

Per corpus, never pooled (`docs/canonical-hashing.md`, *Recorded runs*): parity 25 pairs (8 new, hand-written from this
decision) FMR 0/15, FSR 0/7; external-v1 to v4 and the direction gold set are **identical to rules v6** — none of
them holds an enumerated "one of", so the new rule has teeth in the parity pairs and `tests/membership.rs`, not in an
external corpus. The re-emitted Baseline manifest differs from the signed one in `rules_v` alone (18 of 18 attested
hashes equal), so the owner's re-signature is the whole of the change to it. Mutation-checked: deleting the count
check, the connective check, the duplicate check, the tail, the header noun, the prohibition swap, the bare-list
`or` requirement, or the detection itself each fails a test.

## Consequences

- The adopter's statements now canonicalise as sets, and reordering the members keeps the address. They must be
  re-pinned: a `value` pin on such a claim now reads `not carried` (there is no number), which is correct.
- The Baseline must be re-signed (`rules_v` is in the envelope). Nothing else about it moves.
- **Known wrinkle, left alone:** a non-enumerated "one of" still derives the quantity `1`. It has never merged
  anything (the near-miss corpora hold dozens), and changing it would move addresses for no corpus-measured benefit.
- **Not done here:** no `members` pin (a pin states the operator only); no direction gold-set class (the gold set is
  generated by `tools/corpus/build_direction.py`, and a new mutation class there is a corpus version of its own); no
  Tier-2 handling of a set against a reworded set — a member renamed is a swap, `scope_divergence`, for a human.
- The grammar is deliberately narrow. A real statement that lists its members another way (a table, a bulleted
  sub-list, "A/B/C") is unstructured or parsed as before, and the remedy is a grammar change with a corpus pair, not a guess.

## Rejected

- **A new always-present `members` key in the canonical JSON.** Moves every address for a shape that is rare; the
  value slot already means "what the operator compares against".
- **Order-sensitive members.** The issue's own acceptance and the whole point of a set; an order is a rendering.
- **Reading any "one of … or …" as a set.** `one of the target market countries or regions` is prose; calling its two
  halves members makes a set of fragments, and the fragments are not where the obligation's meaning lives.
- **Dropping the header noun and the tail.** That would merge "four standards" with "four sources" and an open set
  with a closed one: words leaving identity silently, the failure T1.10 exists to prevent.
- **Silently de-duplicating a repeated member.** A repeat is a typo or a dropped third member; shrinking the set hides
  which, so it is a refusal.
- **Reusing `numeric_tightening` for a membership change.** The reviewer would read "numeric" on a change with no number.
