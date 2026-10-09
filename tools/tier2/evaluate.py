#!/usr/bin/env python3
"""Evaluate a Tier-2 provider honestly: choose the confidence floor on one half, report it on the
other.

Reads a capture written by a provider wrapper (one JSON object per line: the two statements and the
answer, with `choice`, `confidence` and `probabilities`) together with the gold corpus, and reports:

  * the floor picked on each half and reported on the other — two folds, both directions — plus the
    **stable floor** that is clean in both, which is the operating point worth quoting;
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
    """Parse with the YAML schema when PyYAML is available; fall back to a line scan otherwise.

    The first version scanned for `  - id: "` — quoted, because the hand-written corpora quote
    everything — and returned zero pairs for a corpus written by `safe_dump`, which quotes only when
    it must. A loader that finds no pairs reports every provider as unmeasured, which reads like a
    result and is not one. The fallback accepts quoted or bare ids.
    """
    text = pathlib.Path(path).read_text()
    try:
        import yaml
    except ImportError:
        yaml = None
    if yaml is not None:
        doc = yaml.safe_load(text)
        rows = doc.get("pairs") if isinstance(doc, dict) else None
        if rows:
            return {str(r["id"]): {"a": r.get("a"), "b": r.get("b"), "label": r.get("label")}
                    for r in rows if isinstance(r, dict) and "id" in r}
    pairs = {}
    for block in re.split(r"\n  - id: ", text)[1:]:
        pid = block.split("\n")[0].strip().strip('"')
        def field(name):
            match = re.search(r'\n    %s: "?(.*?)"?\s*$' % name, block, re.M)
            return match.group(1) if match else None
        pairs[pid] = {"a": field("a"), "b": field("b"), "label": field("label")}
    return pairs

def load_capture(path, gold):
    """Match capture rows to gold pairs by text. The capture carries no pair id yet — a recorded
    gap; text matching works because both sides are the statements verbatim, but it is fragile and
    the fix is to put the id in the provider contract."""
    by_pair = {}
    matched_by_id = [0]
    matched_by_text = [0]
    disagreements = 0
    total_rows = 0
    for line in pathlib.Path(path).read_text().splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        total_rows += 1
        key = (row.get("a"), row.get("b"))
        pid = row.get("id") if row.get("id") in gold else None
        if pid is not None:
            matched_by_id[0] += 1
        else:
            # Older captures predate the id in the contract; fall back to the text they do carry, and
            # say in the record how many rows needed it.
            for candidate, pair in gold.items():
                if (pair["a"], pair["b"]) == key:
                    pid = candidate
                    matched_by_text[0] += 1
                    break
        if pid is not None:
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
                    "label": gold[pid]["label"],
                }
    return by_pair, total_rows, disagreements, matched_by_id[0], matched_by_text[0]


def half(pid):
    """Deterministic and unsteerable: the split comes from the id, not from file order."""
    return "tune" if int(hashlib.sha256(pid.encode()).hexdigest(), 16) % 2 == 0 else "held"


def pct(value):
    """A rate that was never computable is `n/a` — not a raw None, and not a zero."""
    return "n/a" if value is None else "%d%%" % value


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
    # No floor is clean *and* non-empty: the confidence does not separate the classes. Returning
    # `sweep(rows, 100)` made that identical to a maximally conservative threshold that passes.
    return None


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
    rows, total_rows, disagreements, by_id, by_text = load_capture(args.capture, gold)
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
    print(f"paired:   {by_id} by id, {by_text} by statement text")
    if disagreements:
        print(f"NOTE: {disagreements} pair(s) answered differently across rows — the capture holds "
              f"more than one pass, so this is a stability signal, not a duplicate")

    print(f"\nsplit sha256(id)%2: tune {len(tune)}, held-out {len(held)}")
    # Two folds, both directions. Picking on one arbitrary half and reporting on the other is how
    # the earlier "zero false merges" turned out to be fitted: the halves disagree about where the
    # boundary lies, so the operating point worth quoting is the floor clean in *both* directions,
    # and both directions are printed rather than the flattering one.
    forward = choose(tune)
    backward = choose(held)
    forward_held = backward_tune = None
    stable_floor = stable = None

    if forward is None:
        print("\nno floor on the tune half is clean with any true positive: the confidence does "
              "not separate the classes")
    else:
        forward_held = sweep(held, forward["floor"])
        print(f"\ntune-half floor {forward['floor']} -> held-out: "
              f"tp {forward_held['tp']} fp {forward_held['fp']} fn {forward_held['fn']} "
              f"precision {pct(forward_held['precision'])} recall {pct(forward_held['recall'])}")
    if backward is None:
        print("no floor on the held-out half is clean with any true positive: the confidence does "
              "not separate the classes")
    else:
        backward_tune = sweep(tune, backward["floor"])
        print(f"held-half floor {backward['floor']} -> tune:      "
              f"tp {backward_tune['tp']} fp {backward_tune['fp']} fn {backward_tune['fn']} "
              f"precision {pct(backward_tune['precision'])} recall {pct(backward_tune['recall'])}")
    if forward is not None and backward is not None:
        stable_floor = max(forward["floor"], backward["floor"])
        stable = sweep(list(rows.values()), stable_floor)
        print(f"\nstable floor (clean in both directions): {stable_floor}")
        print(f"  all pairs at {stable_floor}: tp {stable['tp']} fp {stable['fp']} fn {stable['fn']} "
              f"precision {pct(stable['precision'])} recall {pct(stable['recall'])}")
    else:
        print("\nstable floor: NONE — NO USABLE FLOOR EXISTS. This provider's confidence does not "
              "separate equivalent from non-equivalent on this corpus")

    print("\nheld-out curve (so the trade is visible, not just the point):")
    print("  floor  tp  fp  fn  precision  recall")
    for result in curve(held):
        print(f"  {result['floor']:5d}  {result['tp']:3d} {result['fp']:3d} {result['fn']:3d}  "
              f"{result['precision'] if result['precision'] is not None else 'n/a':>9}  "
              f"{result['recall'] if result['recall'] is not None else 'n/a':>6}")

    in_sample = None
    if forward is not None:
        in_sample = sweep(list(rows.values()), forward["floor"])
        print(f"\nfor comparison, the same floor on ALL pairs (in-sample, i.e. fitted): "
              f"tp {in_sample['tp']} fp {in_sample['fp']} fn {in_sample['fn']} "
              f"precision {pct(in_sample['precision'])} recall {pct(in_sample['recall'])}")

    if not args.no_record:
        stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
        slug = re.sub(r"[^a-z0-9]+", "-", args.provider.lower()).strip("-")[:40]
        record = {
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
            "floor_chosen_on_tune": forward["floor"] if forward else None,
            "held_out_at_tune_floor": forward_held,
            "floor_chosen_on_held": backward["floor"] if backward else None,
            "tune_at_held_floor": backward_tune,
            "stable_floor": stable_floor,
            "all_pairs_at_stable_floor": stable,
            "usable_floor": stable_floor is not None,
            "in_sample_at_tune_floor": in_sample,
            "held_out_curve": curve(held),
            "matched_by_pair_id": by_id,
            "matched_by_statement_text": by_text,
            "caveats": [
                "both halves come from the same generator and templates: a held-out sample of the "
                "same distribution, not an independent one",
                "confidence is the model's self-report; the serving documentation states the "
                "probabilities are not calibrated for this data",
            ],
        }
        # Named by the *result*, not by the clock: two runs that agree produce the same path and the
        # second refuses to rewrite it, while two runs that disagree can never collide. A timestamp
        # in the name made "append-only" a promise the code could not keep — two runs in the same
        # second overwrote each other.
        key = json.dumps({k: record[k] for k in (
            "provider", "corpus", "pairs", "stable_floor", "all_pairs_at_stable_floor",
        )}, sort_keys=True)
        digest = hashlib.sha256(key.encode()).hexdigest()[:8]
        stem = pathlib.Path(args.gold).stem          # a record named for the corpus it scored
        path = pathlib.Path("bench/results") / f"{stem}-{slug}-{digest}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists():
            print(f"\nrecord already exists for this result: {path} — not rewritten")
        else:
            record["evaluated_at"] = stamp
            path.write_text(json.dumps(record, indent=2) + "\n")
            print(f"\nrecorded: {path}")

    return 0 if (stable is not None and stable["fp"] == 0) else 1


if __name__ == "__main__":
    sys.exit(main())
