<!-- req projection: generator=req render | rules_v=6 | selection=none | claims=16 | identity=sha256:0fa4dca9d893703430f7b3d0c02d855172264fce7a25c71b3b1d344638427f92 -->
# Selection: (no context: nothing is decided by the caller)
applies 16, undecided 5, outside the selection 0 (an undecided claim is never reported as excluded)

## Applies (16)
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
- REQ-0007  7689bfe6d984  UNSTRUCTURED  Authoring shall be friction-free and the decision boundary shall be deliberately high-friction, demanding role, meaning, and evidence there.
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
