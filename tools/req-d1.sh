#!/bin/sh
# A local Tier-2 provider for `req propose --provider-cmd`, backed by Liquid d1-3B served by
# llama.cpp's decision-model API (`POST /v1/systemone`).
#
# req's contract on stdin:  "ID:", "A:", "B:", "DIFF:", "CHOOSE:" — matched by prefix, never by
# position, because a contract that depends on line order breaks silently when the writer gains a
# field. Our contract on stdout: a JSON object carrying `choice`, a `confidence` in integer percent,
# and the probability distribution. The bare word is still a valid answer.
#
# `DIFF:` is the change under test: the content terms present in one statement and not the other.
# The model demonstrably has the discriminating information (equivalent 0.478 versus related 0.4385
# on a subject-changed pair) and still answered `equivalent`; the prompt never pointed at *what*
# differed. The listing localises the decision without enumerating the transformations the gold set
# uses — enumerating those would be teaching to the test, which raises the score and tells us less.
#
# Every answer is appended to a capture file (REQ_D1_CAPTURE, default .scratch/req-d1-capture.jsonl)
# with the pair id and the differing terms, so an evaluation pairs rows to labels by identity and a
# reader can see what the model was shown. Capture failure is reported on stderr and does not fail
# the proposal.
#
# Local only: the server binds 127.0.0.1 and the weights live in .models/ (gitignored). Nothing
# leaves the machine (REQ-0012).
id=""; a=""; b=""; diff=""
while IFS= read -r line; do
  case "$line" in
    ID:*)     id=${line#ID: } ;;
    A:*)      a=${line#A: } ;;
    B:*)      b=${line#B: } ;;
    DIFF:*)   diff=${line#DIFF: } ;;
    CHOOSE:*) : ;;
  esac
done
exec python3 - "$id" "$a" "$b" "$diff" <<'PY'
import json, os, sys, urllib.request

pair_id, a, b, diff = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
payload = {
    "state": "A: %s\nB: %s\nDiffering content terms: %s" % (a, b, diff or "(none)"),
    "questions": {"relation": {
        "type": "choice",
        "instructions": (
            "Compare requirement B against requirement A. Decide on STRUCTURE, not wording. The "
            "state lists the content terms that differ between them: decide whether those "
            "differences change the requirement, or only the phrasing of the same requirement. "
            "Judge equivalent only when every structural element matches and only the wording "
            "changes."
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
        handle.write(json.dumps({"id": pair_id, "a": a, "b": b, "diff": diff, "answer": answer}) + "\n")
except OSError as error:
    print("capture failed: %s" % error, file=sys.stderr)
print(json.dumps(answer))
PY
