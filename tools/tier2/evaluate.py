#!/usr/bin/env python3
"""Evaluate a Tier-2 provider honestly: choose the confidence floor on one half, report it on the
other.

Reads a capture written by a provider wrapper (one JSON object per line: the two statements and the
answer, with `choice`, `confidence` and `probabilities`) together with the gold corpus, and reports:

  * the floor picked on the **tune** half, and its precision and recall on the **held-out** half;
  * both curves, so a reader sees the trade rather than a single number;
  * a JSON record under `bench/results/`, append-only: one file per run, never rewritten.

**Why a split.** A threshold chosen and scored on the same pairs is fitted, not measured. The split
is `sha256(pair id) % 2`, so it is deterministic, reproducible, and cannot be steered by ordering or
by re-running the generator.

**What the split does not fix**, and what the record therefore states: both halves are drawn from the
same generator and the same templates, so this is a held-out sample of the same distribution rather
than an independent one. And confidence is the model's own report — the serving documentation says
the probabilities are not calibrated for our data. Both caveats travel with the numbers into the
record, because a number without its caveat is the thing this project exists to prevent.

Usage:
    python3 tools/tier2/evaluate.py --capture FILE --gold bench/corpus/tier2-v1.yml \
        --provider "d1-3b@bb1e436/llama.cpp@master" [--no-record]
"""

import argparse
import hashlib
import json
import pathlib
import re
import sys
import time

LABELS = ("equivalent", "related", "unrelated")


def load_gold(path):
    """The corpus is generated and regular; no yaml module is guaranteed on a bare machine."""
    text = pathlib.Path(path).read_text()
    pairs = {}
    for block in text.split('  - id: "')[1:]:
        pid = block.split("\n")[0].rstrip('"')

        def field(name):
            match = re.search(r'\n    %s: "(.*)"' % name, block)
            return match.group(1) if match else None

        pairs[pid] = {"a": field("a"), "b": field("b"), "label": field("label")}
    return pairs


def load_capture(path, gold):
    """Match capture rows to gold pairs by text. The capture carries no pair id yet — a recorded
    gap; text matching works because both sides are the statements verbatim, but it is fragile and
    the fix is to put the id in the provider contract."""
    by_pair = {}
    disagreements = 0
    total_rows = 0
    for line in pathlib.Path(path).read_text().splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        total_rows += 1
        key = (row.get("a"), row.get("b"))
        for pid, pair in gold.items():
            if (pair["a"], pair["b"]) != key:
                continue
            answer = row.get("answer", {})
            if pid in by_pair:
                if (by_pair[pid]["choice"], by_pair[pid]["confidence"]) != (
                    answer.get("choice"),
                    int(answer.get("confidence", 0)),
                ):
                    disagreements += 1
            else:
                by_pair[pid] = {
                    "choice": answer.get("choice"),
                    "confidence": int(answer.get("confidence", 0)),
                    "label": pair["label"],
                }
            break
    return by_pair, total_rows, disagreements


def half(pid):
    """Deterministic and unsteerable: the split comes from the id, not from file order."""
    return "tune" if int(hashlib.sha256(pid.encode()).hexdigest(), 16) % 2 == 0 else "held"


def sweep(rows, floor):
    tp = sum(1 for r in rows if r["label"] == "equivalent" and r["choice"] == "equivalent" and r["confidence"] >= floor)
    fn = sum(1 for r in rows if r["label"] == "equivalent") - tp
    fp = sum(1 for r in rows if r["label"] != "equivalent" and r["choice"] == "equivalent" and r["confidence"] >= floor)
    precision = (tp * 100 // (tp + fp)) if (tp + fp) else None
    recall = (tp * 100 // (tp + fn)) if (tp + fn) else None
    return {"floor": floor, "tp": tp, "fp": fp, "fn": fn, "precision": precision, "recall": recall}


def choose(rows):
    """The rule, not a preference: no false merge is the gate, and among the floors that meet it the
    lowest one keeps the most recall. So the smallest floor with `fp == 0`."""
    for floor in range(0, 101):
        result = sweep(rows, floor)
        if result["fp"] == 0 and result["tp"] > 0:
            return result
    return sweep(rows, 100)


def curve(rows):
    out = []
    for floor in range(0, 101, 5):
        result = sweep(rows, floor)
        if not out or (result["tp"], result["fp"], result["fn"]) != (out[-1]["tp"], out[-1]["fp"], out[-1]["fn"]):
            out.append(result)
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture", required=True)
    parser.add_argument("--gold", default="bench/corpus/tier2-v1.yml")
    parser.add_argument("--provider", required=True, help="the provenance string to record verbatim")
    parser.add_argument("--no-record", action="store_true")
    args = parser.parse_args()

    gold = load_gold(args.gold)
    rows, total_rows, disagreements = load_capture(args.capture, gold)
    if not rows:
        print("no gold pair matched the capture: the capture is empty, or it is from another corpus")
        return 3

    tune = [r for pid, r in rows.items() if half(pid) == "tune"]
    held = [r for pid, r in rows.items() if half(pid) == "held"]
    labels = {}
    for r in rows.values():
        labels[r["label"]] = labels.get(r["label"], 0) + 1

    print(f"provider: {args.provider}")
    print(f"capture:  {args.capture}  ({total_rows} rows, {len(rows)} distinct gold pairs)")
    print(f"labels:   {labels}")
    if disagreements:
        print(f"NOTE: {disagreements} pair(s) answered differently across rows — the capture holds "
              f"more than one pass, so this is a stability signal, not a duplicate")

    print(f"\nsplit sha256(id)%2: tune {len(tune)}, held-out {len(held)}")
    chosen = choose(tune)
    print(f"\nfloor chosen on the TUNE half: {chosen['floor']}  (no false merge, most recall)")
    held_result = sweep(held, chosen["floor"])
    print("  held-out result at that floor:")
    print(f"    tp {held_result['tp']}  fp {held_result['fp']}  fn {held_result['fn']}  "
          f"precision {held_result['precision']}%  recall {held_result['recall']}%")

    print("\nheld-out curve (so the trade is visible, not just the point):")
    print("  floor  tp  fp  fn  precision  recall")
    for result in curve(held):
        print(f"  {result['floor']:5d}  {result['tp']:3d} {result['fp']:3d} {result['fn']:3d}  "
              f"{result['precision'] if result['precision'] is not None else 'n/a':>9}  "
              f"{result['recall'] if result['recall'] is not None else 'n/a':>6}")

    in_sample = sweep(list(rows.values()), chosen["floor"])
    print(f"\nfor comparison, the same floor on ALL pairs (in-sample, i.e. fitted): "
          f"tp {in_sample['tp']} fp {in_sample['fp']} fn {in_sample['fn']} "
          f"precision {in_sample['precision']}% recall {in_sample['recall']}%")

    if not args.no_record:
        stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
        slug = re.sub(r"[^a-z0-9]+", "-", args.provider.lower()).strip("-")[:40]
        path = pathlib.Path("bench/results") / f"tier2-v1-{slug}-{stamp}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({
            "kind": "tier2-provider-evaluation",
            "provider": args.provider,
            "corpus": args.gold,
            "capture": args.capture,
            "capture_rows": total_rows,
            "pairs": len(rows),
            "labels": labels,
            "repeat_disagreements": disagreements,
            "split": "sha256(pair id) % 2",
            "tune_pairs": len(tune),
            "held_out_pairs": len(held),
            "floor_chosen_on_tune": chosen["floor"],
            "tune_at_chosen_floor": chosen,
            "held_out_at_chosen_floor": held_result,
            "in_sample_at_chosen_floor": in_sample,
            "held_out_curve": curve(held),
            "caveats": [
                "both halves come from the same generator and templates: a held-out sample of the "
                "same distribution, not an independent one",
                "confidence is the model's self-report; the serving documentation states the "
                "probabilities are not calibrated for this data",
                "capture rows carry no pair id, so rows are matched to labels by statement text",
            ],
        }, indent=2) + "\n")
        print(f"\nrecorded: {path}")

    return 0 if held_result["fp"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
