Fixtures for `tests/release_version.rs`, the `Cargo.lock` exception of ADR 0055 decision 6: a version bump rewrites
req's own package entry and nothing else. Each file is a small hand-written lock file in Cargo's format.

- `base.lock`: the reference. It carries a decoy, a *registry* package that is also named `req`: req's own entry is the
  one with no `source` line.
- `req-bumped.lock`: only req's own entry differs (its `version`).
- `req-and-dependency-list.lock`: req's own entry differs in more than its `version`, still nothing else.
- `dependency-bumped.lock`: another package's entry differs.
- `req-and-dependency-bumped.lock`: req's own entry and another package's entry differ.
- `dependency-added.lock`: a package entry was added.
- `dependency-removed.lock`: a package entry was removed.
- `decoy-bumped.lock`: only the registry package named `req` differs: it is not req's own entry.
- `preamble-changed.lock`: the lock format version at the top differs.
- `no-own-entry.lock`: req's own entry is absent.
- `two-own-entries.lock`: two entries with no `source` are both named `req`.
