#!/usr/bin/env python3
"""Like run_model.py but N workers; every line is still a fresh CLI call. 
usage: run_parallel.py <claude|opencode> <model> <tasks.jsonl> <out.jsonl> <workers>"""
import json, subprocess, sys, os, threading
from concurrent.futures import ThreadPoolExecutor
kind, model, tasks, out, workers = sys.argv[1:6]
done = set()
if os.path.exists(out):
    for l in open(out):
        done.add(json.loads(l)["_k"])
lock = threading.Lock()
def call(prompt):
    cmd = (["claude", "-p", "--model", model, "--tools", "", "--no-session-persistence", prompt] if kind == "claude"
           else ["opencode", "run", "-m", model, prompt])
    p = subprocess.run(cmd, cwd="/tmp/p1/work", capture_output=True, text=True, timeout=300)
    lines = [x for x in p.stdout.replace("\x1b[0m", "").splitlines() if not x.startswith("> build") and x.strip()]
    return "\n".join(lines).strip()
def work(t):
    key = (t.get("statement_id") or t.get("item_id")) + "|" + t.get("generator", "")
    if key in done: return
    try: reply = call(t["prompt"])
    except Exception as e: reply = "ERROR: %r" % (e,)
    rec = {k: v for k, v in t.items() if k != "prompt"}; rec["reply"] = reply; rec["_k"] = key
    with lock:
        with open(out, "a") as fo: fo.write(json.dumps(rec) + "\n")
with ThreadPoolExecutor(int(workers)) as ex:
    list(ex.map(work, [json.loads(l) for l in open(tasks)]))
print("done", out)
