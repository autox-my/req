# The semantic layer proposes; a human disposes

LLMs are what make semantic equivalence, contradiction, and impact tractable on natural
language — the why-now. But a merge tool whose result varies between runs is unusable for a
safety requirement, and the costs are asymmetric: a false merge silently deletes a
requirement, while a false split is merely noise. So the core is deterministic and
structural (IDs, edges, canonical form), and the model's output is a set of **reviewable
proposed edges** — `may_be_equivalent`, `contradiction`, `impact`. It never rewrites a
claim and never auto-merges. Splitting is always allowed; merging always requires
ratification.

## Considered Options

- **LLM-driven merge with a deterministic fallback** — rejected: the primary path would be
  nondeterministic, which is precisely what the fallback exists to prevent.
- **No semantic layer at all** — rejected: textual merge on prose is the pain being solved.

## Consequences

The tool is slower to demo than an auto-merging one and correct instead. Every semantic
assertion it makes is auditable as a proposal with an author.
