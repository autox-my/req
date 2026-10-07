# Authority is a signed Decision, not a commit hash

A commit hash proves content integrity and nothing else. It does not prove who approved a
change, under what authority, on what evidence, or whether the process was followed — and
git history is rewritable by anyone with push access. 21 CFR Part 11 wants electronic
signatures carrying a name, a timestamp, and a *meaning* (authored, reviewed, approved),
plus tamper-evident audit trails. We record Decisions as first-class signed objects
carrying role, meaning, and time, on append-only refs, optionally externally anchored.

## Considered Options

- **Treat signed commits as approvals** — rejected: git has no notion of role or authority
  at all, so a key proves only that somebody held it.
- **Rely on the hosting platform's event log** — rejected: mutable, and the auditor knows
  it.

## Consequences

The tool must model roles and authority — new surface an issue tracker never needed.
Verification reports VALID, TAMPERED, and UNVERIFIABLE as three distinct outcomes, so
"could not check" is never mistaken for "checked and passed".
