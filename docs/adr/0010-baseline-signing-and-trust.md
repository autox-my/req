---
status: accepted — the manifest is extended by 0013 to attest the audit surface
---

# A Baseline is signed with minisign, over a committed trust file

ADR 0005 settled that authority is a signed **Decision**, not a commit hash, and left the
mechanism open. This is the mechanism. It was decided against the consumers of a signature
rather than against the features of a format, because the two formats people reach for first
(OpenPGP, and git's own `-s`) fail the audience at opposite ends.

## The consumers, and why the weakest one picks the default

Five parties touch a signature, and only two of them ever hold a key:

| Consumer | Needs | Key |
|---|---|---|
| `req` itself | deterministic, offline verification | public only |
| CI (enterprise) | non-interactive, exit codes, no agent or keyring | public only |
| auditor / customer / general user | "verify this packet" with nothing installed | public only |
| the release act | sign once per baseline | **private** |
| enterprise security/PKI | rotation, revocation, HSM, mandated formats | supplies the scheme |

The weakest-capability consumer is the auditor or general user, so the **default** must serve
them: verification has to be one command, offline, unprivileged, and reimplementable. OpenPGP
is the more capable format and the wrong default precisely because of that — "install GPG,
import the key, understand trust levels" is the friction git spent a decade removing, and
git's answer was to *add* SSH signing alongside OpenPGP rather than keep one format. A single
winner-take-all choice would repeat that mistake from the other side.

So the scheme is **pluggable with a simple default**:

- **minisign (default)** — Ed25519, two key files, no keyring, no trust model to learn; the
  signature format is small enough to reimplement in whatever language the future porcelain
  is written in.
- **OpenPGP / X.509 (optional)** — for deployments whose policy already mandates them and
  whose keys live in an HSM.
- **SSH signing (optional, deliberately not the default)** — lowest friction for an
  individual, since the key already exists, but that key also grants *access*: signing
  authority and push authority become the same thing. The purpose-specific minisign key keeps
  them separate, which is the whole reason to prefer it.
- **sigstore/cosign keyless** — rejected as any part of the default path: verification reaches
  a transparency log, and REQ-0012 requires zero network for claim operations.

## Expiry and revocation: real, and not the format's job

Both concerns are valid — a stolen key otherwise mints forever-valid baselines — but they are
solved in a **committed trust file** (`baselines/trust.yml`) rather than inside a signature
format, so all schemes get one behaviour:

- **revoked** — a human reviewed the key and withdrew trust: **FAIL**, unconditionally.
- **expired** (`valid_until`) — **UNVERIFIABLE**, not FAIL. This is the honest state and it
  exposes a real limitation: a Baseline is a *historical* record, and expiry can only be
  judged against the signing time. No trusted timestamp exists here, so a key that is expired
  *now* does not make an old signature wrong — it makes the question unanswerable. Failing it
  would destroy valid history; passing it would assert something unverified.
- **not in the trust file** — **UNVERIFIABLE**, never a silent pass.

That removes "GPG has revocation and minisign does not" as an argument for GPG, and it makes
rotation a reviewable diff: a new public key is committed beside the old one with its validity
window, old baselines stay verifiable, new ones must use the new key. Deep timestamping
(RFC 3161 or a recorded signing time in the manifest) is the work that would turn expiry from
UNVERIFIABLE into decidable; it is deferred, and recorded here rather than hidden.

## What gets signed

**A Baseline, by default** — one signature per release over a manifest of the applying claims'
addresses. Signing every claim would put key ceremony at the point ADR 0006 explicitly keeps
friction-free (authoring), for no gain: a claim is not authoritative until it is selected into
a Baseline. **Per-Decision signatures are an opt-in** for regulated flows that must show a
named human approving a specific decision, and they reuse this same scheme and trust file.

## Consequences

- **The core gains no cryptography and no network.** The manifest — sorted addresses, rules
  version, selection context — is emitted by our own canonical writer, and the signature check
  is a **process boundary** to a pinned verifier (`minisign`), exactly as providers are. It is
  installed in the development environment and on the CI runner; where it is absent the outcome
  is UNVERIFIABLE, never a pass, which is why the gate's Baseline step carries no SKIP.
- **Drift becomes detectable.** Addresses are re-derived at verification: a claim edited after
  signing no longer matches its manifest entry and is **FAIL** — the same class as a
  hand-edited digest, and the reason a Baseline is worth more than a tag.
- **A trust file is a new review surface**, and it is data: reviewed like a corpus, pinned in
  git, and consulted on every verification.
- **The verification states are the project's states**: verified / FAIL / UNVERIFIABLE, with
  revocation failing and everything unjudgeable staying unverifiable.

## Considered Options

- **minisign, baseline-level, committed trust file (chosen)** — serves the weakest verifier,
  keeps one revocation story across schemes, adds no dependency to the core.
- **OpenPGP as the default** — more capable, and rejected for exactly what it costs the
  general user; kept as a supported optional scheme.
- **SSH signing as the default** — least friction, rejected because it fuses signing authority
  with repository access.
- **No signatures; a signed git tag alone** — a tag signs a *commit*, not the addresses a
  release claims to contain, and it carries no rules version or selection context. It cannot
  answer "which requirements, exactly, were ratified".
- **A bespoke signature format** — rejected: inventing crypto-adjacent formats is how projects
  acquire unauditable bugs, and it would still need a trust anchor.
