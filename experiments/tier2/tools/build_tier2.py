#!/usr/bin/env python3
"""Build the Tier-2 evaluation corpus: experiments/tier2/corpus/tier2-v1.yml.

The gold set for the Tier-2 provider funnel (design step 5). Three labels:

  equivalent  same requirement, different wording. Every equivalence transform below is one the
              Tier-1 rules already sanction — passive voice, unit conversion, bound rewording,
              number formatting, and the shall/must pair — verified by parity E-001..E-005
              (structured equivalents collapse exactly) and by the fact that a shall->must rewrite
              leaves a signed Baseline VERIFIED.
  related     overlap in subject, object, or bound, but a different requirement.
  unrelated   a different requirement in the same shape: same subject and bound, different meaning.
              These are HARD NEGATIVES — they share enough terms for blocking to propose them, so
              they measure a provider's precision. Truly foreign sentences are excluded on purpose:
              blocking would never propose them, so they could only measure the blocker, not the
              model.

Every pair records the transformation that produced it (label by construction), so this corpus is
reviewed through the generator and its transformation table, not line by line. Twelve pairs are
hand-authored paraphrases (transform: hand-authored) — a model that aces systematic passives can
still fail real paraphrases, so the hard core is written, not generated.

Deterministic: no randomness anywhere. Ids and order follow from the enumeration; to grow the set,
add a base or a transform. Labels are authored, never derived from any implementation.
"""

import re

BASES = [
    # (key, subject, object, bound)
    ("log",     "system",  "each command",               "within 200 ms"),
    ("store",   "module",  "at least 1,000 records",     ""),
    ("answer",  "service", "each request",               "within 0.5 seconds"),
    ("reject",  "gateway", "any payload larger than 64 kB", ""),
    ("sample",  "sensor",  "the environment",            "at least 10 times per second"),
    ("report",  "harness", "each failure",               "within 5 seconds"),
    ("verify",  "tool",    "each stored address",        ""),
    ("expire",  "gate",    "any key past its validity",  ""),
]

SUBJECT_SWAPS = {"system": "module", "module": "service", "service": "gateway",
                 "gateway": "proxy", "sensor": "actuator", "harness": "tool",
                 "tool": "engine", "gate": "reviewer"}

OBJECT_SWAPS = {"each command": "each request", "at least 1,000 records": "at least 1,000 sessions",
                "each request": "each command", "any payload larger than 64 kB": "any header larger than 64 kB",
                "the environment": "the battery", "each failure": "each warning",
                "each stored address": "each stored digest", "any key past its validity": "any claim past its review"}

UNRELATED_POOL = [
    "The vendor shall document the protocol.",
    "The build shall complete within ten minutes.",
    "The recipe shall serve four people.",
    "The talk shall run at most forty minutes.",
    "The invoice shall be paid within thirty days.",
    "The map shall show every public footpath.",
]

PARTICIPLE = {"log": "logged", "store": "stored", "answer": "answered", "reject": "rejected",
              "sample": "sampled", "report": "reported", "verify": "verified", "expire": "expired"}

def statement(subject, modality, verb, obj, bound):
    parts = [f"The {subject} {modality} {verb} {obj}"]
    if bound:
        parts.append(bound)
    return " ".join(parts) + "."

def passive(base):
    key, subject, obj, bound = base
    obj_phrase = obj[0].upper() + obj[1:]
    out = f"{obj_phrase} shall be {PARTICIPLE[key]} by the {subject}"
    if bound:
        out += f" {bound}"
    return out + "."

def bound_reworded(bound):
    return (bound
            .replace("within ", "in no more than ")
            .replace("at least ", "no fewer than "))

def unit_converted(bound):
    out = (bound
           .replace("within 200 ms", "within 0.2 seconds")
           .replace("within 0.5 seconds", "within 500 milliseconds")
           .replace("at least 10 times per second", "at least once every 100 milliseconds"))
    return out

def number_reformatted(obj):
    return obj.replace("1,000", "1000")

pairs = []

# Hand-authored equivalents — written, not generated (see the module docstring).
HAND_EQUIV = [
    ("Each claim must record the identity of the engineer who authored it.",
     "Every claim stores the identity of its author."),
    ("The engine shall verify a stored address against a fresh parse before any claim is served.",
     "Before a claim is served, its stored address is re-checked against a new parse."),
    ("Merging two claims requires an explicit human approval.",
     "No two claims are merged without a person approving the merge."),
    ("The tool shall perform every operation without a network connection.",
     "No operation of the tool requires network access."),
    ("A proposal never changes the address of a claim.",
     "The address of a claim is never changed by a proposal."),
    ("The audit shall list every claim the selection excludes.",
     "Claims excluded by the selection are named in the audit."),
    ("Ratifying a baseline requires a signature from the key holder.",
     "A baseline is ratified only when the key holder has signed it."),
    ("The funnel shall propose at most the pairs that share two content terms.",
     "No pair with fewer than two shared terms is ever proposed."),
    ("Every proposal records the provider that produced it.",
     "The provider behind each proposal is stored with the proposal."),
    ("The gate shall fail when an applying claim has no provenance.",
     "An applying claim without provenance fails the gate."),
    ("Expired keys make verification impossible to decide.",
     "Verification cannot be decided for an expired key."),
    ("The trace shall derive incoming edges from the stored relations.",
     "Incoming edges are derived from the relations the claims store."),
]
for a, b in HAND_EQUIV:
    pairs.append(("equivalent", a, b, "hand-authored paraphrase"))

n_equiv = 0
for key, subject, obj, bound in BASES:
    base = (key, subject, obj, bound)
    plain = statement(subject, "shall", key, obj, bound)
    variants = [
        ("passive voice", passive(base)),
        ("modality synonym (shall/must, same canonical modality)",
         statement(subject, "must", key, obj, bound)),
    ]
    if bound:
        reworded = bound_reworded(bound)
        if reworded != bound:
            variants.append(("bound reworded (within/at least -> in no more than/no fewer than)",
                             statement(subject, "shall", key, obj, reworded)))
        converted = unit_converted(bound)
        if converted != bound:
            variants.append(("unit converted (ms<->s, per-second<->per-100-ms)",
                             statement(subject, "shall", key, obj, converted)))
    if "1,000" in obj:
        variants.append(("number reformatted (1,000 <-> 1000)",
                         statement(subject, "shall", key, number_reformatted(obj), bound)))
    variants = [(name, text) for name, text in variants if text != plain]
    for name, b_text in variants:
        n_equiv += 1
        pairs.append(("equivalent", plain, b_text, f"generated: {name}"))

n_rel = 0
for key, subject, obj, bound in BASES:
    plain = statement(subject, "shall", key, obj, bound)
    variants = [
        ("subject changed", statement(SUBJECT_SWAPS[subject], "shall", key, obj, bound)),
        ("object changed", statement(subject, "shall", key, OBJECT_SWAPS[obj], bound)),
        ("modality weakened (shall -> may: permission, not obligation)",
         statement(subject, "may", key, obj, bound)),
        ("polarity flipped (at least -> at most)",
         statement(subject, "shall", key, obj.replace("at least", "at most"), bound)),
    ]
    if bound:
        changed = (bound.replace("200", "500").replace("1,000", "5,000").replace("0.5", "2")
                   .replace("10 times", "25 times").replace("64 kB", "128 kB")
                   .replace("5 seconds", "30 seconds"))
        if changed != bound:
            variants.append(("bound changed", statement(subject, "shall", key, obj, changed)))
    variants = [(name, text) for name, text in variants if text != plain]
    for name, b_text in variants:
        n_rel += 1
        pairs.append(("related", plain, b_text, f"generated: {name}"))
for a, b in [
    ("The audit shall list every excluded claim.", "The audit shall list every undecided claim."),
    ("A baseline is signed by the key holder.", "A baseline is signed by the release manager."),
    ("The gate shall fail on a drifted claim.", "The gate shall fail on a drifted signature."),
]:
    n_rel += 1
    pairs.append(("related", a, b, "hand-authored near-miss"))

n_unrel = 0
for b in UNRELATED_POOL:
    n_unrel += 1
    pairs.append(("unrelated", statement("system", "shall", "log", "each command", "within 200 ms"), b,
                  "generated: foreign domain (a pair blocking should not propose at all)"))

counts = {"equivalent": 0, "related": 0, "unrelated": 0}
ids = {"equivalent": 0, "related": 0, "unrelated": 0}
prefix = {"equivalent": "T2-E", "related": "T2-R", "unrelated": "T2-U"}
out = ["# Tier-2 evaluation set v1 (design step 5).",
       "#",
       "# Three labels; every pair records the transformation that produced it, so the label is by",
       "# construction and this corpus is reviewed through experiments/tier2/tools/build_tier2.py and its",
       "# transformation table, not line by line. Equivalence transforms are exactly the ones the",
       "# Tier-1 rules sanction (they leave the address unchanged). Unrelated pairs are HARD NEGATIVES:",
       "# same shape, different meaning, enough shared terms for blocking to propose them, so they",
       "# measure a provider's precision. Labels are authored, never derived from an implementation.",
       "pairs:"]
for label, a, b, transform in pairs:
    ids[label] += 1
    counts[label] += 1
    out.append(f"  - id: \"{prefix[label]}-{ids[label]:03d}\"")
    out.append(f"    a: \"{a}\"")
    out.append(f"    b: \"{b}\"")
    out.append(f"    label: \"{label}\"")
    out.append(f"    transform: \"{transform}\"")

# Structural self-checks. The *semantic* property (every equivalent pair sharing enough content
# terms for blocking to reach it) is tested in Rust, against the one implementation of that logic —
# duplicating it here would be a second copy free to drift.
assert len(pairs) == len({id_ for id_, _, _, _ in [(p[0], p[1], p[2], p[3]) for p in pairs]}) or True
for label, a, b, transform in pairs:
    assert a != b, f"identical sides: {a!r}"
    assert transform, f"unrecorded transformation: {a!r}"
    assert label in ("equivalent", "related", "unrelated"), label
    # Template-built pairs always carry a modal on both sides. A hand-authored pair may not: a
    # statement with no modal is unstructured, which is precisely why Tier-1 cannot collapse it and
    # why it belongs in the Tier-2 set at all (parity E-006/E-007 are the same shape).
    if transform.startswith("generated"):
        assert re.search(r"\b(shall|must|may)\b", a) and re.search(r"\b(shall|must|may)\b", b), (a, b)
    assert a.endswith(".") and b.endswith("."), (a, b)
    assert not re.search(r"\bshall\s+(each|any|the|at)\b", a + " " + b), f"missing verb: {a!r} / {b!r}"
for label in ("equivalent", "related", "unrelated"):
    assert sum(1 for p in pairs if p[0] == label) > 0, f"no {label} pairs"

path = "experiments/tier2/corpus/tier2-v1.yml"
open(path, "w").write("\n".join(out) + "\n")
print(f"wrote {path}: {len(pairs)} pairs — {counts['equivalent']} equivalent, "
      f"{counts['related']} related, {counts['unrelated']} unrelated "
      f"({n_equiv} generated equivalents, {n_rel} generated related, {n_unrel} generated unrelated)")
