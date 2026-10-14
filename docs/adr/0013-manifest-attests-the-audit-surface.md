# 0013. The manifest attests the audit surface: relations and rationale, hashed beside the address

**Status: accepted** — proposed by the agent, ratified by the maintainer on 2026-10-13. It amends
ADR 0010, whose manifest carried `(claim_id, address)` only. Date: 2026-10-13.

## Context

Review finding [F8]: a signed Baseline covered the requirement's *meaning* and nothing else. The
address is meaning-only by design (ADR 0007) — a meaning-preserving rewording keeps its identity, so
`relations` and `rationale` sit outside it, and a post-signing edit to either still verifies as
VALID. The decision log is unkeyed (recorded during the review), so git history is the only anchor —
and for the audiences this tool names as its company, that is thin.

The industry practice is consistent across the three domains req names. Automotive (ISO 26262-8
§6/§7) baselines safety requirements *and* their bidirectional traceability as configuration items;
an assessment walks requirement → rationale/allocation → verification. Airborne (DO-178C) makes
trace data a verification artifact that software configuration management must identify and control.
Medical (IEC 62304, FDA 21 CFR 820.30) keeps requirement-to-risk-control linkage — the rationale,
the "which hazard, why" — inside the design history file that is audited. None of them conflates
identity with audit surface; all of them require that what an auditor relies on is what was signed.

## Decision

The Baseline manifest attests the requirement **and its audit surface**. Each `claims` entry keeps
`claim_id` and `address` — identity stays meaning-only, per ADR 0007 — and gains:

- `relations_hash` — `sha256("req.relations.v1\0" ‖ serde_json::to_string(relations))`;
- `rationale_hash` — `sha256("req.rationale.v1\0" ‖ serde_json::to_string(rationale))`.

The hashes are over the **parsed values, deterministically serialized**: `serde_json::to_string`
of the typed struct, whose fields are in declaration order, so the bytes are a pure function of
the parsed values — formatting in the YAML file does not churn the manifest, and a semantic edit
cannot hide behind whitespace. It is *not* `canon::canonical_json`, which is the address's
fixed-shape serializer for the canonical form and cannot serialize a `Relation` or a `Rationale`;
"canonical" here means exactly the writer named above, so a re-implementation hashes the same
bytes. The tag is NUL-delimited (`req.relations.v1\0`) so one surface's prefix can never straddle
another's. `req baseline --verify` re-derives both. A relation or a rationale edited after signing
is **FAIL** — the same class as a hand-edited digest.

## Consequences

- The manifest schema is the signed format, so its version is bumped deliberately: `"version": 2`.
  This is **not** a `RULES_V` bump — canonicalization is untouched, no address moves, the corpus is
  byte-identical.
- **Existing signed manifests must be re-issued and re-signed.** The signing act is the human's
  (ADR 0010): req re-emits, the maintainer re-signs. Until then the committed Baseline is stale, and
  verifying it fails — so wire [F5]'s gate check *after* re-signing, or the gate blocks on a
  signature only the human can refresh.
- Rationale wording is now attested: rewording an assumption churns the manifest. That is the point —
  provenance is part of what an auditor relies on (FDA/ISO treat the "why" as design-control
  evidence, not decoration).
- Cost: two hash fields per claim, derived from rows verification already loads. No extra reads, no
  network, no model I/O.
- **Falsification:** if no consumer of a signature ever re-checks the audit surface after signing,
  the two hashes are overhead. The review's answer stands in the record: in every domain req names,
  an auditor does exactly that.
