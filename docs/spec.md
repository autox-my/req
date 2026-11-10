<!-- req projection: generator=req render | rules_v=8 | selection=none | claims=34 | identity=sha256:38dc6d8fd5a5cc8855d631f23fb21e6d016bb0fc9bc1fe21b6adefa3ef4338b4 -->
# Selection: (no context: nothing is decided by the caller)
applies 34, undecided 5, outside the selection 0 (an undecided claim is never reported as excluded)

## Applies (34)
- REQ-0001  f559f5a905e4  SHALL  The req tool shall treat a single claim as the unit of requirements authorship, review, and merge.
    selector: (none — applies to every variant)
- REQ-0002  6e6ba2ec76ee  SHALL  The req tool shall record each acceptance or rejection of a change as a signed Decision carrying role, meaning, timestamp, and any dissent.
    selector: (none — applies to every variant)
    -> derives-from REQ-0001
- REQ-0003  57364f54a906  SHALL  The req tool shall express permanent product variation as a selection governed by applicability predicates over one claim set.
    selector: (none — applies to every variant)
    -> derives-from REQ-0001
- REQ-0004  ddc6992dd05b  SHALL  The req tool shall emit proposals for reviewer ratification.
    selector: (none — applies to every variant)
    -> refines REQ-0001
- REQ-0005  6bbc0295880a  SHALL  The req tool shall require human ratification before any two claims are merged.
    selector: (none — applies to every variant)
    -> derives-from REQ-0004
- REQ-0006  94d043e0d3c6  SHALL  The req tool shall permit splitting a claim into derived claims without constraint.
    selector: (none — applies to every variant)
    -> derives-from REQ-0004
- REQ-0007  d5f74f33838e  UNSTRUCTURED  Authoring shall be friction-free.
    selector: (none — applies to every variant)
    -> refines REQ-0004
- REQ-0013  170b4ba0205c  SHALL  The req tool shall deliver a non engineer author's edit as a Proposal inside an explicit Working set.
    selector: (none — applies to every variant)
    -> refines REQ-0004
- REQ-0014  4e99cbc538f7  SHALL  The req tool shall produce exactly one commit per ratified logical Decision.
    selector: (none — applies to every variant)
    -> derives-from REQ-0013
    -> refines REQ-0002
- REQ-0015  4a15b28c8eba  SHALL-NOT  The req tool shall not operate a continuous two-way synchronization bridge between a web database and the requirement files.
    selector: (none — applies to every variant)
    -> refines REQ-0014
- REQ-0016  04836a98d468  SHALL  The req tool shall resolve the applicability of an undecided feature as UNKNOWN rather than silently excluding it.
    selector: (none — applies to every variant)
    -> refines REQ-0003
- REQ-0017  f0c1c332f895  SHALL  The req tool shall derive each claim's address at load as the versioned-tag digest of its canonical form bytes, so that recomputing the digest over those bytes reproduces the address the signed Baseline attests.
    selector: (none — applies to every variant)
    -> refines REQ-0007
- REQ-0019  25713319754e  SHALL  The req tool shall emit a lint finding for any construct its normalization rules cannot decide rather than guessing a resolution.
    selector: (none — applies to every variant)
    -> refines REQ-0007
- REQ-0020  b455b84714c2  SHALL  The req tool shall serialize each relation on the outgoing side of its origin claim.
    selector: (none — applies to every variant)
    -> derives-from REQ-0002
- REQ-0021  fc5e269b978e  SHALL-NOT  The req tool shall never rewrite a claim or merge two claims automatically.
    selector: (none — applies to every variant)
    -> derives-from REQ-0004
- REQ-0022  01e9dc781ad7  SHALL  The req tool shall serialize each relation in exactly one claim file.
    selector: (none — applies to every variant)
    -> refines REQ-0020
- REQ-0023  f33145e56369  SHALL  The decision boundary shall be deliberately high-friction, demanding role, meaning, and evidence there.
    selector: (none — applies to every variant)
    -> refines REQ-0004
- REQ-0024  c3eadd9b4bc2  SHALL  The req tool shall report a gate invocation that leaves a claim undecided as UNVERIFIABLE.
    selector: (none — applies to every variant)
    -> refines REQ-0016
- REQ-0025  0c87837b3330  SHALL  The req tool shall report a claim with no linked test as UNVERIFIABLE.
    selector: (none — applies to every variant)
- REQ-0026  a015dc0ac17d  SHALL-NOT  The req tool shall not count a skipped test as a pass.
    selector: (none — applies to every variant)
- REQ-0027  264413b20370  SHALL  The req tool shall fail the gate when a claim's pin disagrees with the parse its statement derives.
    selector: (none — applies to every variant)
- REQ-0028  923f071ba7b5  SHALL  The req tool shall fail the verification of a Baseline when a claim was edited after signing.
    selector: (none — applies to every variant)
- REQ-0029  86509bfea214  SHALL-NOT  The req tool shall not report a Baseline as verified when its signature does not verify or no verifier is installed.
    selector: (none — applies to every variant)
- REQ-0030  0a0da15c2a51  SHALL  The req tool shall report a Baseline written under another rules version as UNVERIFIABLE.
    selector: (none — applies to every variant)
- REQ-0031  523381a2556c  SHALL  The req tool shall report an edited decision record as tampered.
    selector: (none — applies to every variant)
    -> derives-from REQ-0002
- REQ-0032  8921ad734b31  SHALL  The req tool shall refuse a claim file that carries a key it does not define.
    selector: (none — applies to every variant)
- REQ-0033  852ff0535240  SHALL  The req tool shall report a corpus with no claim files as UNVERIFIABLE when it derives the parse.
    selector: (none — applies to every variant)
- REQ-0034  79a92eeb5cc8  SHALL  The req tool shall report a change that loosens an obligation as weakened.
    selector: (none — applies to every variant)
- REQ-0035  b680aeaaa9a1  SHALL  The benchmark harness shall report a corpus with no not-equivalent pairs as UNVERIFIABLE.
    selector: (none — applies to every variant)
- REQ-0036  898b62a32ce9  SHALL  The req tool shall report a hand-edited projection as stale.
    selector: (none — applies to every variant)
- REQ-0037  dd73ffc53893  SHALL  The req tool shall rank a failure above an undecided line when it folds the checks of a repository.
    selector: (none — applies to every variant)
- REQ-0038  e8c8f136b0ed  SHALL  The req tool shall report a corpus that is not the pinned commit as a failure.
    selector: (none — applies to every variant)
- REQ-0039  042be179a510  SHALL  The install script shall refuse a tarball whose hash differs from the pin.
    selector: (none — applies to every variant)
- REQ-0040  faccefaea670  SHALL  The req tool shall exit with status 2 for a command it does not know.
    selector: (none — applies to every variant)

## Undecided (5)
- REQ-0008  29decbdac12b  SHALL  The benchmark harness shall report a false merge rate of exactly zero on the parity and adversarial mutation corpora.
    selector: corpus_set == parity_and_adversarial
    -> derives-from REQ-0005
- REQ-0009  9f25a65419a3  SHALL  The benchmark harness shall classify direction of change on the structural direction gold set with accuracy of at least 0.95.
    selector: set == direction_gold_structural
    -> refines REQ-0008
- REQ-0010  e4f9b4878326  SHOULD  The benchmark harness should show a false split rate of at most 0.30 on the parity corpus at v0.
    selector: corpus_set == parity
    -> refines REQ-0008
- REQ-0012  efa29f8ef62d  SHALL  The req tool shall need no network access for any claim operation when the deployment is configured offline.
    selector: deployment_mode == air_gapped
    -> derives-from REQ-0007
- REQ-0018  5e95be2d018f  SHALL  The req tool shall require numeric claim fields to be serialized as exact decimal strings.
    selector: field == value
    -> refines REQ-0017

## Outside the selection (0)
(none)
