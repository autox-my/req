#!/usr/bin/env python3
"""Entropy of a provider's returned distribution, grouped by what it chose and what the gold says.

Written because the scalar confidence turned out not to be the only candidate signal. A decision
model returns a full distribution over the options, and entropy is a different functional of it: a
model can be wrong but confident, or right but uncertain, and the two are indistinguishable in the
scalar alone. For d1 the sweep was empty; this tool is how we found out whether any functional of the
same distribution does better, and it is why the answer for three of four candidates is a measured no
rather than an assumption.

The result it reports, in the form that matters: whether a cut on entropy exists that removes every
false merge, and how many true positives survive it. `NONE` means the distributions overlap.

Usage:
    python3 tools/tier2/entropy.py --capture FILE --gold bench/corpus/tier2-positives-v1.yml
    python3 tools/tier2/entropy.py --self-check

Exit: 0 measured, 1 invalid input, 2 usage, 3 nothing to measure (no answered pair).
"""

import argparse
import collections
import json
import math
import pathlib
import statistics
import sys


def sha_entropy(probabilities):
    """Shannon entropy in bits of a distribution given as a mapping. Returns None when the mapping is
    empty or sums to nothing, because a distribution that was never returned is not a zero."""
    values = [float(v) for v in probabilities.values()]
    total = sum(values)
    if not values or total <= 0:
        return None
    entropy = -sum((x / total) * math.log2(x / total) for x in values if x > 0)
    # A point mass computes to -0.0, which prints as "-0.000" and claims a sign the quantity does not
    # have. Adding zero normalises it without changing the value.
    return entropy + 0.0


def load_gold(path):
    """YAML first. A hand-rolled scan of the same file missed a whole corpus once because the ids
    were unquoted, and reported every provider as unmeasured."""
    import yaml
    doc = yaml.safe_load(pathlib.Path(path).read_text())
    rows = doc.get("pairs") if isinstance(doc, dict) else None
    if not rows:
        raise SystemExit("no pairs in %s — the gold corpus must carry a `pairs` list" % path)
    return {str(r["id"]): str(r["label"]) for r in rows if isinstance(r, dict) and "id" in r}


def collect(capture, gold):
    rows = {}
    for line in pathlib.Path(capture).read_text().splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        pid = row.get("id")
        if pid not in gold:
            continue
        answer = row.get("answer", {})
        rows[pid] = (gold[pid], answer.get("choice"),
                     sha_entropy(answer.get("probabilities") or {}))
    return rows


def report(rows):
    groups = collections.defaultdict(list)
    for label, choice, entropy in rows.values():
        if entropy is None:
            continue
        groups["%s/%s" % (label, choice)].append(entropy)
    if not groups:
        return None
    for key in sorted(groups):
        values = sorted(groups[key])
        print("  %-22s n=%2d  median %.3f bits  min %.3f  max %.3f"
              % (key, len(values), statistics.median(values), values[0], values[-1]))
    positive = groups.get("equivalent/equivalent", [])
    negative = groups.get("related/equivalent", [])
    if positive and negative:
        verdict = ("the false merges are MORE uncertain" if statistics.median(negative) > statistics.median(positive)
                   else "the false merges are MORE confident")
        print("  true positives median %.3f vs false merges median %.3f — %s"
              % (statistics.median(positive), statistics.median(negative), verdict))
        cuts = [t for t in sorted(set(round(h, 3) for h in positive + negative))
                if all(h > t for h in negative)]
        if cuts:
            kept = sum(1 for h in positive if h <= cuts[0])
            print("  a cut removing every false merge: keep %d of %d true positives at <= %.3f bits"
                  % (kept, len(positive), cuts[0]))
        else:
            print("  a cut removing every false merge: NONE — the distributions overlap")
    return groups


def self_check():
    """The arithmetic, on cases whose answers are known without a model."""
    uniform3 = sha_entropy({"a": 1, "b": 1, "c": 1})
    assert abs(uniform3 - math.log2(3)) < 1e-12, uniform3
    assert sha_entropy({"a": 1}) == 0.0
    assert math.copysign(1.0, sha_entropy({"a": 1})) > 0, "a point mass must not print as -0.000"
    assert sha_entropy({}) is None
    assert sha_entropy({"a": 0, "b": 0}) is None, "an all-zero distribution is not an entropy of zero"
    # Unnormalised inputs are normalised, so a provider returning counts is read the same as one
    # returning probabilities.
    assert abs(sha_entropy({"a": 2, "b": 2}) - 1.0) < 1e-12
    print("self-check PASS: uniform-3 = %.5f bits, a point mass = 0, empty and all-zero = None, "
          "counts normalise" % uniform3)
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture")
    parser.add_argument("--gold")
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        return self_check()
    if not args.capture or not args.gold:
        print("--capture and --gold are required (or --self-check)")
        return 2
    try:
        gold = load_gold(args.gold)
    except Exception as error:  # noqa: BLE001 — the reason is the finding
        print("UNVERIFIABLE — cannot read %s: %s" % (args.gold, error))
        return 3
    rows = collect(args.capture, gold)
    if not rows:
        print("UNVERIFIABLE — no capture row matches the gold corpus, so there is nothing to measure")
        return 3
    print("%s against %s: %d answered pair(s)" % (args.capture, args.gold, len(rows)))
    if report(rows) is None:
        print("UNVERIFIABLE — no row carried a usable distribution")
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
