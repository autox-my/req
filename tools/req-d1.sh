#!/bin/sh
# A local Tier-2 provider for `req propose --provider-cmd`, backed by Liquid d1-3B served by
# llama.cpp's decision-model API (`POST /v1/systemone`).
#
# req's contract on stdin:  "ID: <pair id>", "A: <statement>", "B: <statement>", "CHOOSE: <options>"
# our contract on stdout:   a JSON object — {"choice": ..., "confidence": 0-100, "probabilities": {...}}
#                           The bare word is still a valid answer; JSON is how the confidence
#                           travels, and the confidence is what a threshold sweep needs.
#
# The stdin lines are matched by prefix rather than read in a remembered order: a contract that
# depends on line position breaks silently when the writer gains a field, and a wrapper that sends
# the wrong text to a model produces plausible wrong answers rather than an error.
#
# The criteria are written as a PRINCIPLE plus one example, deliberately not as the list of
# transformations the gold set uses. Enumerating those (subject, object, bound, modality, polarity)
# would be teaching to the test: the score would rise and tell us less.
#
# Every answer is appended to a capture file (REQ_D1_CAPTURE, default
# .scratch/req-d1-capture.jsonl) with the pair id, so an evaluation can pair rows to labels by
# identity instead of matching statement text. Capture failure is reported on stderr and does not
# fail the proposal: the evidence file is for measurement, and a missing one is a measurement
# problem.
#
# Local only: the server binds 127.0.0.1 and the weights live in .models/ (gitignored). Nothing
# leaves the machine (REQ-0012).
id=""; a=""; b=""
while IFS= read -r line; do
  case "$line" in
    ID:*) id=${line#ID: } ;;
    A:*)  a=${line#A: } ;;
    B:*)  b=${line#B: } ;;
    CHOOSE:*) : ;;
  esac
done
exec python3 - "$id" "$a" "$b" <<'PY'
import json, os, sys, urllib.request

pair_id, a, b = sys.argv[1], sys.argv[2], sys.argv[3]
payload = {
    "state": "A: %s\nB: %s" % (a, b),
    "questions": {"relation": {
        "type": "choice",
        "instructions": (
            "Compare requirement B against requirement A. Decide on STRUCTURE, not wording: if the "
            "actor or subject, the object or scope, the bound or threshold, the modality, or the "
            "applicable conditions differ at all, then B is not equivalent. Judge equivalent only "
            "when every one of those matches and only the wording changes."
        ),
        "criteria": {
            "equivalent": "Every structural element matches: actor, action, object, bound, modality and conditions; only the wording differs.",
            "related": "At least one structural element differs while the subject matter still overlaps; for example a different actor, such as a system versus a module, or a different bound.",
            "unrelated": "No shared requirement; the subject matter differs.",
        },
    }},
}
request = urllib.request.Request(
    "http://127.0.0.1:8080/v1/systemone",
    data=json.dumps(payload).encode(),
    headers={"content-type": "application/json"},
)
with urllib.request.urlopen(request, timeout=180) as response:
    node = json.load(response)["answers"]["relation"]

answer = {
    "choice": node.get("choice"),
    "confidence": round(float(node.get("confidence", 0.0)) * 100),
    "probabilities": node.get("probabilities", {}),
}
try:
    with open(os.environ.get("REQ_D1_CAPTURE", ".scratch/req-d1-capture.jsonl"), "a") as handle:
        handle.write(json.dumps({"id": pair_id, "a": a, "b": b, "answer": answer}) + "\n")
except OSError as error:
    print("capture failed: %s" % error, file=sys.stderr)
print(json.dumps(answer))
PY
