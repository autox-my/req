# req — safety manual (draft)

**Status: a draft written by the project's agent for the owner to review.** It is not reviewed by anyone qualified in a safety
standard, and it is **not a tool qualification**: nothing here claims that req is qualified, certified or assessed under DO-330,
ISO 26262-8, IEC 61508 or IEC 62304 (`docs/design.md` §9 defers that deliberately). It collects, in one place, what the repository
already states about how req may be relied on, so that a reader deciding whether to use it in a safety-related process can see the
guarantees, their evidence and the limits side by side. Every statement points at the document or test that owns it; where this
manual and its source disagree, the source is right and this page has a defect.

Version of req described: the `rules_v 8` line (`req --version` says which build you hold). Maintained by one person; `0.1.0`.

## 1. What req is for, and what it is not for

req manages requirements as **claims**, one per file in git, and answers a small set of questions about them deterministically:

| question | command | what the answer is evidence of |
|---|---|---|
| did the *meaning* of a requirement change? | `canon`, `compare`, `diff` | a meaning fingerprint (the **address**) moved or did not; the **direction** (weakened, tightened, reshaped, …) is a classification for a human to ratify |
| may this variant ship? | `gate`, `check` | every applying claim has settled policies; an undecided claim, a missing test report or an unreadable input makes the answer **UNVERIFIABLE (3)**, never a pass |
| what was ratified, by whom, when? | `baseline`, `decision`, `audit` | a minisign-signed manifest of addresses and attested hashes, judged against a committed trust file; a tamper-evident decision chain |
| what depends on a claim? | `impact`, `trace` | derived views over the relations stored once on the outgoing side |

**Intended use.** An aid for keeping a requirements set honest as it changes: it finds edits that changed meaning, refuses to call a
loosened obligation "no change", and makes "we could not check" a distinct, blocking answer. **A human decides** every merge and
every ratification (ADR 0004); req never auto-merges and never rewrites a claim.

**Not intended for, and not claimed:**
- judging whether a requirement is *correct*, *complete* or *consistent* with other requirements. req compares text it was given;
- replacing review, test or analysis. A linked test is a claim about evidence, not proof (§4);
- collapsing paraphrases. Tier-1 is a drift-and-direction fingerprint, not a paraphrase collapser (README, *Status*);
- being the only control. It is one check among others, and its failure modes (§5) are designed to be loud, not to be absent.

## 2. How a verdict is made (the parts that matter for trust)

- **Three outcomes, never two.** PASS (0), a definite finding (1), UNVERIFIABLE (3: it cannot be told), plus usage (2). A skipped
  check, an unreadable corpus, a degraded registry or a missing verifier is reported as such and is **never folded into a pass**
  (`AGENTS.md`, *Correctness rules*; each command's `--help` has its `exit:` line).
- **Identity is a pure function of meaning (ADR 0007, 0020, 0022).** The address is `sha256` over a canonical JSON form that is
  *derived* from the statement at load; nothing derived is stored in a claim file, so there is no stored address to drift from the text.
  Tier-1 rules are a closed set (encoding, casing, exact SI unit factors, a closed comparator table, a moderated modality table, a
  reviewed subject registry, condition extraction, the enumerated-membership rule). No model, no network, no floating-point
  arithmetic and no hash-map iteration order is in the address path; six clippy lints enforce the arithmetic and cast rules on every
  path (`Cargo.toml`, `AGENTS.md`).
- **When the parser cannot be sure, it refuses.** A statement that does not parse into the schema keeps a raw canonical form with
  `unstructured: true` and a lint (`untestable_comparator`, `untestable_qualifier`, `ambiguous_modality`, `multiple_obligations`,
  `uncovered_content`, `membership_unclear`, …). Unstructured claims never feed structural direction analysis, because guessable
  structure is what a false merge needs (`docs/canonical-hashing.md` §4). The coverage rule (T1.10) means no word of a structured
  statement leaves identity silently.
- **The asymmetry is structural.** A *false merge* (two different requirements sharing an address) silently deletes one: the failure
  the design exists to prevent. A *false split* (one meaning, two addresses) is noise, always survivable. Every rule is chosen so the
  error falls on the split side; a Tier-2 model may only **propose** an equivalence for a human to ratify (ADR 0004, 0021).
- **Authority is a signed Baseline over a committed trust file (ADR 0010, 0016).** `req baseline --verify` re-derives every address
  under the manifest's rules version and fails on drift, a bad signature, an untrusted or expired key, or an altered decision chain.
  A manifest written under another `rules_v` is UNVERIFIABLE, not judged.
- **Everything runs offline.** The core has no network client; `git` and `minisign` are local processes (REQ-0012).

## 3. The evidence the project keeps

| claim | evidence | where it is recorded |
|---|---|---|
| zero false merges on the corpora that measure it | `req bench` is a hard gate on every commit; one record per corpus and rules version, append-only | `bench/results/`, `docs/canonical-hashing.md` §6 and *Recorded runs* |
| the direction classifier does not confuse a loosening with no change or its opposite | the direction gold set; a *dangerous confusion* fails the gate | `bench/results/direction-v1-*`, `docs/canonical-hashing.md` §6.3 |
| each rule has teeth | the rule behind each trap class is deleted and a corpus test must fail (a mutation check, done by hand and recorded per change) | commit messages, `docs/canonical-hashing.md` |
| the tool's own requirements are managed by the tool | 39 claims in `claims/` (22 design claims and 17 tool operational requirements, each linked to a test that proves it), signed Baseline `baselines/air-gapped.json`, linked tests, a decision log | `claims/`, `baselines/`, `decisions/` |
| what is in the binary, and nothing forbidden is | `cargo deny` bans network crates anywhere in the graph, allows only reviewed licences and crates.io in every gate run; the bill of materials `docs/sbom.cdx.json` (CycloneDX 1.5, `scope` required/excluded) is drift-checked like a projection; advisories are checked weekly, not in the gate (ADR 0036) | `deny.toml`, `docs/sbom.cdx.json`, `tools/sbom.py`, `.github/workflows/audit.yml` |
| a change is gated before it merges | one local gate (`scripts/check.sh`) and the same job in CI, required by branch protection; a human reads every pull request | `scripts/check.sh`, `.github/workflows/ci.yml`, ADR 0029 §7 |

**What the headline numbers are.** At `rules_v 8`, per corpus and never pooled: the seed parity corpus (29 pairs) has FMR 0/18; the four
external corpora (other projects' real requirement prose, pinned by commit) have FMR 0/160, 0/227, 0/625 and 0/1595; the direction gold
set classified 1012 of 1016 pairs correctly with no dangerous confusion. **These are labelled by construction** (the label is the
mutation applied to a real statement). They are seed and self-corpus results plus other projects' prose; they are not an external
falsification of the bet, and the README says so.

## 4. What req does not guarantee — read this before relying on it

- **The central bet is unvalidated on independent human paraphrase.** The one probe run (model-authored, judge-filtered, 60
  statements) found none of 272 restatements sharing an address with its original: a lower bound on false splits, not a validation of
  anything. Expect Tier-1 to split genuine paraphrases; that is by design.
- **Tier-2 is not a decision-maker.** The default provider is `NoProvider`; no evaluated provider has survived held-out validation on a
  grown, blind-labelled corpus (README, *Status*). A proposal is unratified and carries its provider and prompt.
- **Direction is advice.** `weakened`, `tightened`, `reshaped` are for a human to ratify; a pair with an unstructured side is
  `not-judged`. Direction quality is exactly the classifier's, measured on a gold set labelled by construction.
- **Pins are not attested by the Baseline** (ADR 0022, a named loss): deleting a pin after signing still verifies. A pin is checked
  against the parse by the gate (`pins_hold`), and the parse is derived from the statement, so never edit a pin to fit a parse.
- **A linked test is a claim, not proof.** `tests_linked` says which claims have no passing test; it cannot say a test proves its
  claim, and a wrong link manufactures confidence (`docs/adopting.md`, *The honest limit*). `libtest` writes no report and is a named
  gap (use nextest); doctests are not run by the gate.
- **A Decision's `actor` is a name someone typed.** The chain is tamper-evident (`TAMPERED` is a distinct outcome); authenticity
  comes from the signed Baseline that attests the chain head and length. No individual Decision record is signed. The signing time is
  the signer's own assertion, with no external timestamp authority (`docs/claim-format.md`, *Deliberately deferred*).
- **Known parser limits.** One obligation per claim (a statement with a second modal is unstructured); clause decomposition is not
  implemented; an unenumerated "one of the target countries" still derives the quantity `1` (ADR 0030); ambiguous comparators are
  linted, not guessed; the closed tables cover what they list and nothing else.
- **Environment limits.** Windows is untested (WSL2 is expected, not verified). The macOS binary is unsigned and not notarized; the
  Linux binary was built on glibc 2.43 and ran with a loader warning on 2.35. Releases made before ADR 0035 are **not reproducible** (two builds of one
  rev gave different hashes); from it, two builds of a rev on one builder give identical bytes (the gate checks it), and a rebuild elsewhere is
  evidence, not a promise: `scripts/reproduce.sh --rev <rev> --against <tarball>` compares the binary inside, and a `DIFFERS` is reported, not
  explained away (the target, the pinned toolchain and, on Linux, the linker and libc must match). The pinned checksum stays the authority.
- **A release's record is self-attestation.** From ADR 0037 each release's `.build-info` lists the proofs its build ran on the bytes it shipped
  (the bare rev, the Baseline verification, the FMR gate, `req selftest`) with the binary's SHA-256. It is written by the release runner, is not
  signed and is not independent evidence; it does not claim the test suite, the gate, the mutation run or the reproduction check, which are not
  run in that job. Re-run `req selftest` and `req baseline --verify` on the downloaded binary rather than trusting the file.
- **One maintainer, early software.** Version `0.1.0`; the tool's own development uses AI coding agents under a human owner who merges
  every change (`AGENTS.md`, *Review* and *SDLC*). An automated review run by hand is an unverified lead, not a verdict
  (`docs/research/review-bakeoff.md`: unaudited, one model).

## 5. Known failure modes and how each shows

| failure | how you would see it | consequence if missed | what stops it |
|---|---|---|---|
| a false merge (different requirements, one address) | `req bench` FMR > 0; or `req diff` shows `same-address-as <id>` on an added claim | a requirement silently disappears | the FMR = 0 gate on every commit; every Tier-1 rule is meaning-preserving by construction; merge needs a human |
| a false split | a reworded claim reads `reshaped` or `removed`+`added` | review noise, not loss | none needed; splitting is always allowed |
| the parse is wrong but confident | a pin disagrees (`pins_hold` exit 1); a lint names the cause | a bound, subject or set means something else than the author intended | pins on load-bearing slots; the coverage rule; read `req canon` for claims that matter |
| the statement is unstructured | `req canon` shows `unstructured` and a lint | no structural direction analysis for that claim | the lint; re-word, or accept the claim as raw text |
| the wrong `req` is used | `req --version` `built from` ≠ the rev you pin, or `rules_v` differs | verdicts from another rule set | the installer refuses a `built from` that is not the pinned rev; `baseline --verify` is UNVERIFIABLE on a `rules_v` mismatch; `req selftest` fails if this build derives something else on this host |
| the Baseline is stale after a rules change | `baseline --verify` UNVERIFIABLE (rules mismatch) or FAIL (drift) | none, if the gate is required | the owner re-signs; releases flag a `rules_v` change in their notes (ADR 0031) |
| the trust anchor is swapped in a pull request | CI reads the anchor from `origin/main`, not the PR; rotation is two PRs | a forged baseline verifies | `baselines/trust.yml` is read from main; a key swap without a rotation story is a review finding |
| a shallow clone or missing report | exit 3 naming the missing revision or report | a gate nobody notices did not run | UNVERIFIABLE blocks, never passes |
| a check was skipped | `SKIP` in the gate log, an error in CI | a green that checked less | CI fails on any `SKIP`; the Baseline step has no `SKIP` |
| CRLF rewrote a signed or compared file | `baseline --verify` FAIL or `render --check` stale on a clean tree | a false alarm that blocks work | LF pinned per path in `.gitattributes` (`docs/adopting.md`, *Windows*) |

## 6. Operating rules for a safety-related project

1. **Pin the rev and its checksum** in your repository (`examples/install-req.sh`); from then on the pin is the authority. After install,
   `req --version` must say the `built from` you pinned and the `rules_v` your Baseline was signed under, and `req selftest` must pass
   (ADR 0034: known-answer checks of this build on this machine; not a defence against a tampered binary).
2. **Run the full gate in CI** (`req check`, which folds every declared variant and projection), with `fetch-depth: 0`, and make it a
   required check. Treat exit 3 as blocking, exactly like exit 1.
3. **Give every variant its own gate invocation** and list it in `.req/variants`; a bare gate on a corpus with a selector is UNVERIFIABLE by design (ADR 0015).
4. **Read the lints and `req canon` for the claims that matter** (a safety goal, a numeric bound, a set). A lint is the parser telling you what it did not decide.
5. **Never edit a pin to fit a parse; re-word the statement instead.** Do not hand-write an address, a hash or a derived field.
6. **Ratify by a human, with the losing side recorded** (a Decision's note; a claim's rationale lists rejected alternatives). Treat `weakened` as a prompt to look, not a rejection or an approval.
7. **Sign the Baseline with a key you control, outside CI**, and keep rotation to two pull requests (add the new key, re-sign; revoke the old one after both merge).
8. **Re-sign after any `rules_v` bump** (releases say when it moved) and treat re-canonicalization candidates as drift to ratify, never to rewrite silently.
9. **Keep `git` and `minisign` trusted.** req runs `git` as a local process with user and system configuration removed; it cannot defend against a compromised toolchain.
10. **Do not use Tier-2 output, a review run, or a bake-off number as a decision.** They are leads.

## 7. Identification and change control

- **Identification.** `req --version` prints the crate version and every axis that decides a verdict: `rules_v`, `baseline_envelope`,
  `decision_chain`, `claim_schema`, and `built from <rev>`. A release is a prebuilt, checksummed tarball per target for a pinned rev
  (ADR 0025, 0031, 0032); its notes say whether `rules_v`, the claim schema or the Baseline envelope moved.
- **Change control.** A change to a Tier-1 rule bumps `RULES_V`, regenerates goldens deliberately, records re-canonicalization candidates
  and moves the Baseline for the owner to re-sign (ADR 0030 is the worked example). Decisions are ADRs; a superseded one says so rather
  than being edited (`docs/adr/`). Version pins that sit on the fingerprint path (`rust-toolchain.toml`, `Cargo.lock`) are never bumped
  opportunistically (`AGENTS.md`).
- **Known problems.** Open issues labelled `tool-defect` are the known-problems list, and `wrong-verdict` marks the ones that matter
  most for reliance (a requirement change read as no change; a pass where nothing could be checked):
  <https://github.com/autox-my/req/issues?q=label%3Atool-defect>. The *Tool defect* form asks for the version, the use affected and
  the consequence. A vulnerability is reported
  privately (`SECURITY.md`).

## 8. What a qualification would additionally need (not provided here)

A qualification claim needs things this repository does not contain and this page cannot supply: a tool classification agreed with the
assessor for the target standard and use; tool operational requirements written against that use; an independent verification of the tool
against them; and a statement of the project's own process by someone who can attest to it. The existing evidence (§3) is raw material for
that dossier, not the dossier. The trigger and sequencing are in `docs/design.md` §9: a paying design partner whose audit needs it.
