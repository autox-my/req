#!/usr/bin/env python3
"""Phase 1: decompose all pairs, llama.cpp alone. Resumable - already-done ids are skipped."""
import argparse, json, os, sys, time, urllib.request
import yaml

LLAMA = "http://127.0.0.1:8082"
PROMPT = (
    "Two requirement statements are being compared. List the structural elements that DIFFER between "
    "them, using only these names: actor, object, bound, modality, condition.\n\n"
    "A: {a}\nB: {b}\n\n"
    'Answer with JSON only: {{"differences": [{{"element": "one of actor, object, bound, modality, '
    'condition", "a": "the text in A, or none", "b": "the text in B, or none"}}], '
    '"changes_meaning": true or false}}\n'
    "If they differ only in wording, return an empty list and changes_meaning false."
)

def decompose(a, b):
    req = urllib.request.Request(LLAMA + "/v1/chat/completions",
                                 data=json.dumps({"messages": [{"role": "user", "content": PROMPT.format(a=a, b=b)}],
                                                  "max_tokens": 300, "temperature": 0,
                                                  "chat_template_kwargs": {"enable_thinking": False}}).encode(),
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=900) as response:
        payload = json.load(response)
    return payload["choices"][0]["message"]["content"]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--decomp-file", default=".scratch/decompositions.jsonl")
    ap.add_argument("--limit", type=int, default=0)
    args = ap.parse_args()
    pairs = yaml.safe_load(open("bench/corpus/tier2-positives-v1.yml"))["pairs"]
    done = set()
    if os.path.exists(args.decomp_file):
        for line in open(args.decomp_file):
            try:
                done.add(json.loads(line)["id"])
            except (ValueError, KeyError):
                continue
    print("resuming: %d already done, %d to go" % (len(done), len(pairs) - len(done)), flush=True)
    handle = open(args.decomp_file, "a")
    for index, pair in enumerate(pairs, 1):
        if pair["id"] in done:
            continue
        started = time.time()
        try:
            raw = decompose(pair["a"], pair["b"])
        except Exception as error:  # noqa: BLE001
            print("%2d/%d %s FAILED: %s" % (index, len(pairs), pair["id"], error), flush=True)
            continue
        handle.write(json.dumps({"id": pair["id"], "a": pair["a"], "b": pair["b"],
                                 "raw_decomposition": raw}) + "\n")
        handle.flush()
        print("%2d/%d %s decomposed (%.0fs)" % (index, len(pairs), pair["id"], time.time() - started), flush=True)
    handle.close()
    print("phase 1 complete: %s" % args.decomp_file, flush=True)

if __name__ == "__main__":
    main()
