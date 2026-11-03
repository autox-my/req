# The generator and judge protocol (P-1)

This is the protocol the orchestrator follows when it runs the models. The unit that built the
instrument does not call a model; the outputs are committed in a follow-up. The prompts below are
**pinned**: `req-paraphrase --self-check` fails if this page and the code disagree, and a
restatements or judgements file whose `prompt` field is not the code's text does not validate.

## Generators (at least three model families)

A *generator* is a model that sees only one statement and this instruction — no other statement,
no corpus, nothing about Tier-1:

```
Restate this requirement as a different engineer would write it, preserving exactly the same obligation (who, modality, action, bound, condition); do not copy phrasing; give 3 restatements; then give 1 alteration that changes exactly one element (modality, quantity, actor, action or condition) and name which element.
```

followed by the output format:

```
Answer with one JSON object and nothing else: {"restatements": ["...", "...", "..."], "alteration": {"text": "...", "element": "modality|quantity|actor|action|condition"}}
```

**Generators are never shown Tier-1's rules, any canonical form, any address, or the fact that
the output will be hashed** — neither to help the rules nor to hurt them. `req-paraphrase tasks
generate` prints one JSON line per statement with the exact prompt; a generator's reply is copied
into `restatements/<generator-id>.json` (schema `req.paraphrase.restatements/1`):

| field | meaning |
|---|---|
| `generator` | `{id, family, model}`; `id` equals the file stem (`[a-z0-9._-]`), `family` is what the different-family rule compares |
| `statements_sha256` | sha256 of the exact `statements.json` bytes the run used (`tasks generate` prints it on stderr) |
| `prompt` | the generator prompt template above, verbatim |
| `items[]` | one per statement: `statement_id`, exactly 3 `restatements` (`{id: "<statement>/R<n>", text}`), one `alteration` (`{id: "<statement>/ALT", text, element}`) |

`req-paraphrase validate` rejects: a wrong schema, a hash that does not match the sample, a prompt
that is not this one, an unknown statement id, a restatement count other than 3, an alteration
element outside the five, a restatement that is a copy of its statement (modulo case, spacing and
the final period), duplicate generated texts, and non-derived ids. A statement a generator left out
is **not** an error: it is a coverage note, reported per generator.

## Judges (a different model family from the generator it judges)

A *judge* sees a statement and one generated text and nothing else:

```
Two requirement statements follow. Do they impose the same obligation (same actor, modality, action, bound and condition)? Answer with exactly one word: yes, no or unsure. Answer unsure if you cannot tell; do not guess.
```

`req-paraphrase tasks judge --generator <id>` prints one line per restatement **and per
alteration**, with the exact prompt. The answers go to `judgements/<judge-id>.json` (schema
`req.paraphrase.judgements/1`, the same `judge`/`statements_sha256`/`prompt` fields, and
`verdicts[]` of `{generator, item_id, verdict}` with `verdict` one of `yes`, `no`, `unsure`).

The filter, per generator:

- A judge whose family equals the generator's family is **not used** for that generator (it is
  named in the record). With no judge of another family the generator is UNVERIFIABLE.
- A restatement enters the headline only on a **unanimous `yes`** from every judge used. `no`
  outranks `unsure` outranks a missing verdict; the rest are counted and listed with the reason.
  Reason: a generator can drift in meaning, and a drifted "restatement" correctly splits.
- **Alterations are not filtered.** An altered statement that shares its original's address is a
  false merge whatever the judges said — a collapse is a finding, never averaged away. The judges'
  verdicts on it are recorded as an annotation.

## Scoring

`req-paraphrase score` canonicalizes every statement and every text through the public
`canonicalize_statement` seam with the repository's `aliases/subjects.yml`, and compares
`address_hex(canonical_json(form))`. See `README.md` for the output and the grade of the evidence.
