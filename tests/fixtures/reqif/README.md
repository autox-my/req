# ReqIF fixtures (issue #135, ADR 0028)

Test data for `tools/reqif_import.py`. Three **public ReqIF files, whole and byte-for-byte as published**;
nothing was edited, trimmed or re-serialised.

| file | produced by | objects | retrieved | sha256 |
| --- | --- | --- | --- | --- |
| `doors-sample.reqif` | "IBM Rational DOORS, 12 October 2018" (`REQ-IF-TOOL-ID`) | 3, no relations | 2026-11-08 | `56179c470d10f94f88d0516926a4522d5e11464339c53d4da572f7d950fe7018` |
| `polarion-sample.reqif` | "Polarion ReqIF Roundtrip" (redacted, placeholder text) | 2, no relations | 2026-11-08 | `4300a1281f01137736bd4fe8a5950f84650359a6c62d3da6fa7744868d1897b7` |
| `rmf-sample.reqif` | Eclipse RMF / ProR (the title is an OEM→supplier exchange test case) | 6, no relations | 2026-11-08 | `b3178daff5fcd8a386d5965693b6fb9b381b8f9604d4f9ef0821e8ffd16f5e89` |

**Where they come from.** `tests/integration/examples/` of the open-source `reqif` Python library
(`https://github.com/strictdoc-project/reqif`, Apache-2.0, `main` at `b36694051d6b0502682bcd8f83567d4e1f387d1b` on
the retrieval date). That repository's README describes the examples as "samples of ReqIF files found on the
internet": **the library does not state who authored each file or under what licence**, so the Apache-2.0
licence is the library's, not a statement about these three files. They are small, published test inputs; the
owner is asked to accept that reading by merging the pull request that carries them (the same step ADR 0027 §8
asked for the `needs.json` excerpts). If the owner will not, the fixtures are deleted and the tests that name
them are rewritten against synthetic files; nothing else changes.

**What they are good for, and what they are not.** They are *structural* evidence, read before ADR 0028 was written:

- DOORS: the 14 object types have **no `LONG-NAME`** (only an `IDENTIFIER`); the three objects are
  chapter headings (`ReqIF.ChapterName`) with **no `ReqIF.Text` value**; `ReqIF.ForeignID` is an *integer*
  (the DOORS absolute number); rich text is `reqif-xhtml:div`.
- Polarion: types are named (`Heading`, `System Requirement`); `ReqIF.ForeignID` is a *string* (`LOREM-818`
  style); the `Status` enumeration resolves through `ENUM-VALUE`/`LONG-NAME`; one object's `ReqIF.Text` is
  declared XHTML but the heading's value is a plain `STRING` (the value element, not the declaration, says
  what it is); **one `IDENTIFIER` appears on two `SPEC-HIERARCHY` nodes** (not on objects).
- RMF: no `ReqIF.Text` attribute at all; the text-like attribute is `ReqIF.Name`, XHTML `<p>`; two
  specifications.
- **None of the three has a `SPEC-RELATION`, a table, a list, an embedded object or a multi-paragraph body.**
  The DOORS sample holds no requirement text. They say nothing about the size, markup and linking of a real
  automotive requirements specification; the reader's yield on such a document is **unmeasured** and is
  not to be reported as if it were. Shapes the files lack are tested on synthetic ReqIF the tests write into a
  temporary directory and label as synthetic.

The Polarion text is placeholder text; no doc quotes any of these files' content.

## `areas-sample.reqif` (task I-1)

Not a public file: a **synthetic** ReqIF written by hand for the `--areas` tests (`tests/reqif_import.rs`).
Its header comment draws the specification hierarchy (nested headings, a requirement under a requirement,
a requirement directly under the specification, one reached from two places, one under no node). It is an
export of no tool, and nothing is claimed from it about a real specification.
