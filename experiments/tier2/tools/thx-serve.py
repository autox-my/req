#!/usr/bin/env python3
"""A resident THX-01 server for req's Tier-2 provider seam.

Same shape as tools/gliner-serve.py: the weights load once, the reader is our own stdlib HTTP layer,
and req's existing wrapper (`experiments/tier2/tools/req-d1.sh`) speaks to it unchanged — because THX-01's answer for a
`choice` question carries `choice`, per-option `probabilities` and `confidence`, which is exactly the
JSON our harness already parses.

Run it (the weights download into the gitignored `.models/hf` on first use):

    PYTHONPATH=/home/dsh/understand/.tools/thx HF_HOME=/home/dsh/understand/.models/hf \\
      .tools/gliner-venv/bin/python .scratch/thx-serve.py --port 8080

The model card is candid that fine-grained near-synonymous option sets are its weak spot, which
equivalent-versus-related is. The first smoke test agreed with that: a near-uniform distribution and a
confidence of 0.05 on a pair that is genuinely confusable. This server is how we find out whether that
honesty generalises into a usable operating point.
"""

import argparse
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MODEL = "doofz/THX-01"
LABELS = ("equivalent", "related", "unrelated")
agent = None


def load():
    global agent
    import thx01

    started = time.time()
    agent = thx01.load(MODEL)
    print("loaded %s in %.1fs" % (MODEL, time.time() - started), flush=True)


def decide(request):
    """One three-way decision, on exactly the state and questions the wrapper sent.

    The d1 wrapper speaks llama.cpp's systemone shape: `{"state": ..., "questions": {...}}`. The first
    version of this server read `a`/`b`/`diff` — the GLiNER wrapper's payload — so every one of the
    wrapper's requests was answered from the constant string "A: None\nB: None", and all 29 pairs
    returned the identical answer. A server that answers before reading the question is the failure
    mode this whole project exists to prevent, and it looked exactly like a model that always says
    `unrelated`.""",
    result = agent.decide(request.get("state", ""), request.get("questions") or {})
    answer = result["answers"]["relation"]
    if answer.get("choice") not in LABELS:
        raise ValueError("no label among %s in the answer: %s" % (LABELS, answer))
    # Pass the model's own shape through unchanged. The first version of this server unwrapped it to
    # the GLiNER wrapper's flat contract and then paired with the d1 wrapper, which parses the
    # wrapped one — 29 requests answered and 29 requests unread. THX-01's native answer already
    # matches what experiments/tier2/tools/req-d1.sh parses, which is the reason this candidate needed no new code.
    return {"answers": result["answers"]}


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path not in ("/decide", "/v1/systemone"):
            # The d1 wrapper posts to /v1/systemone, the llama.cpp path — serving only /decide here
            # answered every one of its requests with a 404 while my own curl tests kept succeeding,
            # because they tested the wrong client.
            self.send_error(404, "POST /decide or /v1/systemone")
            return
        length = int(self.headers.get("content-length", 0))
        try:
            request = json.loads(self.rfile.read(length) or b"{}")
            # decide() already raises when the choice is missing or outside the vocabulary; a second
            # check here still indexed the old flat contract after the pass-through and turned every
            # request into `KeyError: 'choice'`.
            body = json.dumps(decide(request)).encode()
            self.send_response(200)
        except Exception as error:  # noqa: BLE001 — the reason travels to the caller, which reports it
            body = json.dumps({"error": "%s: %s" % (type(error).__name__, error)}).encode()
            self.send_response(500)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--host", default="127.0.0.1")
    args = parser.parse_args()
    load()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print("serving %s on http://%s:%d/decide" % (MODEL, args.host, args.port), flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
