#!/usr/bin/env python3
"""Assemble raw model replies into the req.paraphrase.* files.
  assemble.py gen   <raw.jsonl> <generator-id> <family> <model> <statements.json> <out.json> <prompt-template-file>
  assemble.py judge <judge-id> <family> <model> <statements.json> <out.json> <prompt-template-file> <raw1.jsonl> [<raw2.jsonl> ...]
Unparseable replies are dropped and counted on stderr (a statement a generator left out is a
coverage note, never an error)."""
import json, sys, re, hashlib
def sha(path): return hashlib.sha256(open(path, "rb").read()).hexdigest()
def first_json(text):
    i = text.find("{")
    if i < 0: return None
    depth = 0
    for j in range(i, len(text)):
        if text[j] == "{": depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                try: return json.loads(text[i:j+1])
                except Exception: return None
    return None
ELEMS = {"modality", "quantity", "actor", "action", "condition"}
if sys.argv[1] == "gen":
    raw, gid, fam, model, stm, out, tmpl = sys.argv[2:9]
    items, dropped = [], []
    for l in open(raw):
        r = json.loads(l); sid = r["statement_id"]; d = first_json(r["reply"])
        ok = (isinstance(d, dict) and isinstance(d.get("restatements"), list) and len(d["restatements"]) == 3
              and all(isinstance(x, str) and x.strip() for x in d["restatements"])
              and isinstance(d.get("alteration"), dict) and isinstance(d["alteration"].get("text"), str)
              and d["alteration"].get("element") in ELEMS)
        if not ok: dropped.append(sid); continue
        items.append({"statement_id": sid,
                      "restatements": [{"id": "%s/R%d" % (sid, n+1), "text": t.strip()} for n, t in enumerate(d["restatements"])],
                      "alteration": {"id": sid + "/ALT", "text": d["alteration"]["text"].strip(), "element": d["alteration"]["element"]}})
    doc = {"schema": "req.paraphrase.restatements/1", "generator": {"id": gid, "family": fam, "model": model},
           "statements_sha256": sha(stm), "prompt": open(tmpl).read(), "items": items}
    json.dump(doc, open(out, "w"), indent=2, ensure_ascii=False); open(out, "a").write("\n")
    print("%s: %d items, dropped %d %s" % (gid, len(items), len(dropped), dropped), file=sys.stderr)
else:
    jid, fam, model, stm, out, tmpl = sys.argv[2:8]; raws = sys.argv[8:]
    verdicts, bad = [], 0
    for raw in raws:
        for l in open(raw):
            r = json.loads(l); w = re.findall(r"[a-z]+", r["reply"].lower())
            v = w[0] if w and w[0] in ("yes", "no", "unsure") else None
            if v is None: bad += 1; continue
            verdicts.append({"generator": r["generator"], "item_id": r["item_id"], "verdict": v})
    doc = {"schema": "req.paraphrase.judgements/1", "judge": {"id": jid, "family": fam, "model": model},
           "statements_sha256": sha(stm), "prompt": open(tmpl).read(), "verdicts": verdicts}
    json.dump(doc, open(out, "w"), indent=2); open(out, "a").write("\n")
    print("%s: %d verdicts, %d unparseable" % (jid, len(verdicts), bad), file=sys.stderr)
