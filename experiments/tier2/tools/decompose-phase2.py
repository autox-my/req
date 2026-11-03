#!/usr/bin/env python3
"""Phase 2: judge the saved decompositions, Strands alone. Resumable."""
import argparse, json, os, time, urllib.request
import yaml

STRANDS = "http://127.0.0.1:8080"
CRITERIA = {
    "equivalent": "the same requirement, differently worded - same actor, object, bound, modality and conditions",
    "related": "the same subject matter with exactly one structural element different - an actor, an object, a bound, a modality, or a condition",
    "unrelated": "no shared requirement",
}
ELEMENTS = ("actor", "object", "bound", "modality", "condition")

def parse(raw):
    import re
    parsed = None
    for candidate in (raw, *re.findall(r"\{.*\}", raw, re.S)):
        try:
            parsed = json.loads(candidate.strip())
            break
        except ValueError:
            continue
    differences = []
    if isinstance(parsed, dict):
        for item in parsed.get("differences") or []:
            if isinstance(item, dict) and str(item.get("element", "")).lower() in ELEMENTS:
                differences.append({"element": str(item["element"]).lower(),
                                    "a": str(item.get("a", "")), "b": str(item.get("b", ""))})
    changes = bool(isinstance(parsed, dict) and parsed.get("changes_meaning"))
    return parsed is not None, differences, changes

def judge(a, b, differences):
    lines = ["A: %s" % a, "B: %s" % b]
    if differences:
        lines.append("A parser already identified these structural differences:")
        for d in differences:
            lines.append("  - %s: A has %r; B has %r" % (d["element"], d["a"], d["b"]))
    else:
        lines.append("A parser already compared these statements and found no structural differences; "
                     "they differ only in wording.")
    req = urllib.request.Request(STRANDS + "/v1/systemone",
                                 data=json.dumps({"state": "\n".join(lines),
                                                  "questions": {"relation": {"type": "choice",
                                                                             "instructions": "How does requirement B stand to requirement A?",
                                                                             "criteria": CRITERIA}}}).encode(),
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=180) as response:
        node = json.load(response)["answers"]["relation"]
    return {"choice": node.get("choice"),
            "confidence": round(float(node.get("confidence", 0.0)) * 100),
            "probabilities": node.get("probabilities", {})}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--decomp-file", default=".scratch/decompositions.jsonl")
    ap.add_argument("--out", default=".scratch/req-decomp-llama-capture.jsonl")
    args = ap.parse_args()
    decomps = [json.loads(line) for line in open(args.decomp_file)]
    done = set()
    if os.path.exists(args.out):
        for line in open(args.out):
            try:
                done.add(json.loads(line)["id"])
            except (ValueError, KeyError):
                continue
    print("judging: %d decompositions, %d already judged" % (len(decomps), len(done)), flush=True)
    handle = open(args.out, "a")
    for index, d in enumerate(decomps, 1):
        if d["id"] in done:
            continue
        parsed, differences, changes = parse(d["raw_decomposition"])
        started = time.time()
        try:
            answer = judge(d["a"], d["b"], differences)
        except Exception as error:  # noqa: BLE001
            print("%2d/%d %s FAILED: %s" % (index, len(decomps), d["id"], error), flush=True)
            continue
        handle.write(json.dumps({"id": d["id"], "a": d["a"], "b": d["b"],
                                 "raw_decomposition": d["raw_decomposition"], "parsed": parsed,
                                 "differences": differences, "changes_meaning": changes,
                                 "answer": answer}) + "\n")
        handle.flush()
        print("%2d/%d %s %d diff -> %s %d  (%.0fs)"
              % (index, len(decomps), d["id"], len(differences), answer["choice"],
                 answer["confidence"], time.time() - started), flush=True)
    handle.close()
    print("phase 2 complete: %s" % args.out, flush=True)

if __name__ == "__main__":
    main()
