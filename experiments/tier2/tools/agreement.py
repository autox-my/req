#!/usr/bin/env python3
"""The ceiling: two labellers on the same worksheet, and how often they agree.

Two people read every pilot pair independently and write a label. Their agreement is the ceiling any
model should be measured against — if careful readers disagree about whether a marginal pair is
equivalent, then a model scoring below the agreement rate is not obviously wrong, and a model
scored against one hundred percent is being failed for a question that has no agreed answer.

Four outcomes per pair, kept apart because folding them would produce a confident number from an
unanswered question:

  agreed              both wrote the same label
  disagreed           both wrote a label, and they differ       -> the named, interesting cases
  one_undecided       exactly one declined to decide
  both_undecided      neither would

The ceiling is `agreed / (agreed + disagreed)` as an integer percent: undecided pairs are reported
and excluded from the denominator rather than counted as agreement or as disagreement.

Refuses to run when both files name the same labeller: agreeing with yourself is not agreement.

Outcomes as everywhere: 0 measured, 1 a worksheet is invalid, 2 usage, 3 UNVERIFIABLE — nothing was
decided twice, so there is no ceiling to report (stated, never reported as zero).

Usage:
    python3 experiments/tier2/tools/agreement.py --worksheet experiments/tier2/corpus/tier2-pilot-v0.yml \\
        --a filled-alice.yml --b filled-bob.yml
"""

import argparse
import collections
import hashlib
import json
import os
import pathlib
import sys
import time


def cohen_kappa(a_labels, b_labels):
    """Chance-corrected agreement. Raw agreement overstates: two raters with a small label vocabulary
    agree often by luck, and Fleiss's bands (above 0.75 excellent, 0.40-0.75 fair to good, below poor)
    are stated on kappa, not on a percentage. Returns None when chance agreement is degenerate."""
    n = len(a_labels)
    if n == 0 or n != len(b_labels):
        return None
    categories = sorted(set(a_labels) | set(b_labels))
    counts_a = collections.Counter(a_labels)
    counts_b = collections.Counter(b_labels)
    po = sum(1 for x, y in zip(a_labels, b_labels) if x == y) / n
    pe = sum((counts_a[c] / n) * (counts_b[c] / n) for c in categories)
    if po >= 1.0:
        # Perfect agreement is kappa 1.0 by convention, even when every item carries one label — which
        # is exactly the gate's binary question, where both readers agreed on all thirty pairs. A guard
        # written against the degenerate denominator was returning None there and made the most
        # important number unreportable.
        return 1.0
    if pe >= 1.0:
        return None
    return (po - pe) / (1.0 - pe)


def self_check():
    """The tool's own arithmetic, checked without needing any files. Added now because a merge message
    once claimed this tool carried a self-check and it did not — a false statement in the record is the
    one defect this project exists to prevent."""
    assert abs(cohen_kappa(["y"] * 4 + ["n"] * 4 + ["y", "n"],
                           ["y"] * 4 + ["n"] * 4 + ["n", "y"]) - 0.6) < 1e-9
    assert cohen_kappa(["same"] * 5, ["same"] * 5) == 1.0
    assert cohen_kappa([], []) is None
    print("self-check PASS: kappa 0.6 on the 8-of-10 balanced case, 1.0 on perfect agreement, "
          "None on empty input")
    return 0



sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate import validate  # noqa: E402


def load(path):
    code, findings, pairs = validate(path, filled=True)
    if code == 3:
        print("UNVERIFIABLE — %s: %s" % (path, findings[0]))
        return code, None
    if code != 0:
        for finding in findings:
            print("INVALID — %s: %s" % (path, finding))
        return 1, None
    labels = {}
    for pair in pairs:
        raw = str(pair.get("label") or "").strip().lower()
        labels[pair["id"]] = {
            # No coercion: the validator has already refused anything outside the outcome
            # vocabulary, so an unexpected value here would be a bug rather than a typo.
            "label": raw,
            "labeller": str(pair.get("labeller") or "").strip(),
            "confidence": int(str(pair.get("confidence") or "0")),
            "tier": pair.get("tier"),
        }
    return 0, labels


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--worksheet", required=False)
    parser.add_argument("--a", required=False, help="worksheet filled by the first labeller")
    parser.add_argument("--b", required=False, help="worksheet filled by the second labeller")
    parser.add_argument("--no-record", action="store_true")
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--caveat", action="append", default=[],
                        help="a condition the number is only true under; recorded with it")
    args = parser.parse_args()
    if args.self_check:
        return self_check()
    missing = [name for name, value in (("--worksheet", args.worksheet), ("--a", args.a),
                                        ("--b", args.b)) if not value]
    if missing:
        print("%s required (or --self-check)" % ", ".join(missing))
        return 2

    code, left = load(args.a)
    if code:
        return code
    code, right = load(args.b)
    if code:
        return code

    names = sorted({v["labeller"] for v in list(left.values()) + list(right.values())})
    if len(names) < 2:
        print("usage error: both files name the same labeller %r — agreeing with yourself is not "
              "agreement" % names)
        return 2
    # An author must not label their own pair: their construction is a claim, and the point of the
    # second reader is to test it. Rejecting two fillings by one labeller is a different check.
    import yaml as _yaml
    doc = _yaml.safe_load(open(args.worksheet)) or {}
    authors = {str(x).strip().lower() for x in (doc.get("authors") or [])}
    for name in names:
        if name.lower() in authors:
            print("usage error: %r is an author of this worksheet, so they cannot be one of its "
                  "labellers — an author marking their own paraphrase is not evidence" % name)
            return 2

    ids = [pid for pid in left if pid in right]
    missing = sorted(set(left) ^ set(right))
    if missing:
        print("INVALID — the two files cover different pairs: %s" % ", ".join(missing[:8]))
        return 1

    counts = collections.Counter()
    per_tier = collections.defaultdict(collections.Counter)
    disagreed = []
    for pid in ids:
        a, b = left[pid]["label"], right[pid]["label"]
        if a == "undecided" and b == "undecided":
            outcome = "both_undecided"
        elif a == "undecided" or b == "undecided":
            outcome = "one_undecided"
        elif a == b:
            outcome = "agreed"
        else:
            outcome = "disagreed"
            disagreed.append((pid, left[pid]["tier"], a, b))
        counts[outcome] += 1
        per_tier[left[pid]["tier"]][outcome] += 1

    decided_twice = counts["agreed"] + counts["disagreed"]
    ceiling = (100 * counts["agreed"]) // decided_twice if decided_twice else None

    print("worksheet: %s   %d pair(s)   labellers: %s and %s"
          % (args.worksheet, len(ids), names[0], names[1]))
    for outcome in ("agreed", "disagreed", "one_undecided", "both_undecided"):
        print("  %-15s %d" % (outcome, counts[outcome]))
    for tier in ("A", "B", "C"):
        row = per_tier.get(tier)
        if row:
            print("  tier %s: agreed %d, disagreed %d, undecided %d"
                  % (tier, row["agreed"], row["disagreed"],
                     row["one_undecided"] + row["both_undecided"]))

    if ceiling is None:
        print("\nceiling: UNVERIFIABLE — no pair was decided by both labellers, so there is no "
              "agreement to measure. Reported as unmeasurable, never as zero.")
        return 3
    print("\nceiling: %d%% agreement over %d pair(s) decided by both "
          "(agreed %d, disagreed %d)" % (ceiling, decided_twice, counts["agreed"], counts["disagreed"]))
    decided = [pid for pid in ids
               if left[pid]["label"] != "undecided" and right[pid]["label"] != "undecided"]
    kappa = cohen_kappa([left[pid]["label"] for pid in decided],
                        [right[pid]["label"] for pid in decided])
    if kappa is not None:
        print("  cohen's kappa: %+.2f over the same %d pair(s) — chance-corrected; above 0.75 "
              "excellent, 0.40-0.75 fair to good, below 0.40 poor" % (kappa, len(decided)))
    if disagreed:
        print("the pairs they disagreed about, named:")
        for pid, tier, a, b in disagreed[:20]:
            print("  %s [tier %s]: %s vs %s" % (pid, tier, a, b))
        if len(disagreed) > 20:
            print("  … %d more" % (len(disagreed) - 20))

    if not args.no_record:
        blob = json.dumps({"kind": "tier2-ceiling", "worksheet": args.worksheet,
                           "caveats": args.caveat,
                           "pairs": len(ids), "labellers": names,
                           "outcomes": dict(counts), "ceiling_percent": ceiling, "kappa": kappa,
                           "decided_by_both": decided_twice},
                          sort_keys=True)
        digest = hashlib.sha256(blob.encode()).hexdigest()[:8]
        path = pathlib.Path("bench/results") / ("tier2-ceiling-pilot-%s.json" % digest)
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists():
            print("\nrecord already exists for this result: %s — not rewritten" % path)
        else:
            record = json.loads(blob)
            record["evaluated_at"] = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
            record["per_tier"] = {t: dict(r) for t, r in per_tier.items()}
            record["disagreements"] = [{"id": p, "tier": t, "a": x, "b": y} for p, t, x, y in disagreed]
            path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
            print("\nrecorded: %s" % path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
