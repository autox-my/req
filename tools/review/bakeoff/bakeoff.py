#!/usr/bin/env python3
"""The review bake-off: the same model, the same historical diffs, three review arms.

The question (docs/specs/review-bakeoff.md): which review tool, given the model this
project already uses, finds the defects a reviewer later made us fix, at what noise?

Arms, all calling one model through one endpoint:

* ``O1``  OpenCodeReview, the pinned CLI, exactly the flags the retired Forgejo review ran.
* ``PR``  PR-Agent ``/review`` in its ``local`` git-provider mode.
* ``PI``  PR-Agent ``/improve``, same mode.

Subcommands: ``run`` (execute arms over cases, resumable), ``pack`` (blind judge packets),
``score`` (aggregate judge labels), ``--self-check`` (the pure parts, table-driven).

Nothing here reads a secret except ``run``, which reads the key file once, hands it to the
child process through its environment only, and redacts it from every log it keeps.

Exit codes: 0 ok, 1 a run or check failed, 2 usage.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import subprocess
import sys
import time
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ENDPOINT = "https://opencode.ai/zen/go/v1"
MODEL = "deepseek-v4.1-flash"
ARMS = ("O1", "PR", "PI")
# Generated corpora are reviewed through their generator, and a signature is unreadable to
# a model: both arms exclude the same classes the production review excluded.
EXCLUDES = ("bench/corpus/**", "bench/results/**")
VERDICTS = ("real", "minor", "noise", "unverifiable")


# --------------------------------------------------------------------------------------
# Normalisation: three tools, one finding shape. Pure, so --self-check can drive it.
# --------------------------------------------------------------------------------------


def _s(value: object) -> str:
    return value.strip() if isinstance(value, str) else ""


def _int(value: object) -> int | None:
    return value if isinstance(value, int) and not isinstance(value, bool) and value > 0 else None


def normalize_ocr(doc: object) -> dict:
    """OCR result -> {status, findings, complete}. A run that did not complete is not clean."""
    if not isinstance(doc, dict):
        return {"status": "unreadable", "complete": False, "findings": []}
    status = _s(doc.get("status"))
    complete = status in ("complete", "success")
    manifest = doc.get("manifest") if isinstance(doc.get("manifest"), dict) else {}
    coverage = manifest.get("coverage") if isinstance(manifest.get("coverage"), dict) else {}
    if coverage.get("failed"):
        complete = False  # a partial review is not a review of the whole diff
    findings = []
    for c in doc.get("comments") or []:
        if not isinstance(c, dict):
            continue
        findings.append(
            {
                "file": _s(c.get("path")),
                "start": _int(c.get("start_line")),
                "end": _int(c.get("end_line")),
                "title": _s(c.get("category")),
                "text": _s(c.get("content")),
                "tool_severity": _s(c.get("severity")).lower() or None,
            }
        )
    return {"status": status or "unknown", "complete": complete, "findings": findings}


def normalize_pa_review(doc: object) -> dict:
    if not isinstance(doc, dict) or not isinstance(doc.get("review"), dict):
        return {"status": "unreadable", "complete": False, "findings": []}
    review = doc["review"]
    findings = []
    for k in review.get("key_issues_to_review") or []:
        if not isinstance(k, dict):
            continue
        findings.append(
            {
                "file": _s(k.get("relevant_file")),
                "start": _int(k.get("start_line")),
                "end": _int(k.get("end_line")),
                "title": _s(k.get("issue_header")),
                "text": _s(k.get("issue_content")),
                "tool_severity": None,
            }
        )
    security = _s(review.get("security_concerns"))
    if security and not security.lower().startswith("no"):
        findings.append(
            {"file": "", "start": None, "end": None, "title": "security concern", "text": security,
             "tool_severity": None}
        )
    return {"status": "complete", "complete": True, "findings": findings}


def normalize_pa_improve(doc: object) -> dict:
    if not isinstance(doc, dict) or not isinstance(doc.get("code_suggestions"), list):
        return {"status": "unreadable", "complete": False, "findings": []}
    findings = []
    for s in doc["code_suggestions"]:
        if not isinstance(s, dict):
            continue
        score = s.get("score")
        findings.append(
            {
                "file": _s(s.get("relevant_file")),
                "start": None,
                "end": None,
                "title": _s(s.get("one_sentence_summary")),
                "text": _s(s.get("suggestion_content")),
                "existing_code": _s(s.get("existing_code")),
                "tool_severity": None,
                "tool_score": score if isinstance(score, int) and not isinstance(score, bool) else None,
            }
        )
    return {"status": "complete", "complete": True, "findings": findings}


NORMALIZERS = {"O1": normalize_ocr, "PR": normalize_pa_review, "PI": normalize_pa_improve}


# --------------------------------------------------------------------------------------
# Blind packets. The judge sees findings in a seeded shuffle with neutral ids and no arm,
# no tool severity, no tool score. The key that maps ids to arms never enters the packet.
# --------------------------------------------------------------------------------------


def build_packet(case: dict, gt: list[dict], per_arm: dict[str, list[dict]]) -> tuple[dict, dict]:
    pool = [(arm, i, f) for arm in sorted(per_arm) for i, f in enumerate(per_arm[arm])]
    rng = random.Random(int(hashlib.sha256(case["id"].encode()).hexdigest(), 16))
    rng.shuffle(pool)
    findings, key = [], {}
    for n, (arm, i, f) in enumerate(pool, 1):
        fid = f"F{n:02d}"
        key[fid] = {"arm": arm, "index": i}
        item = {"id": fid, "file": f["file"], "text": f["text"]}
        if f["title"]:
            item["title"] = f["title"]
        if f["start"]:
            item["lines"] = [f["start"], f["end"] or f["start"]]
        if f.get("existing_code"):
            item["code"] = f["existing_code"]
        findings.append(item)
    packet = {
        "case": case["id"],
        "reviewed_state": case["head"],
        "diff_range": f"{case['base']}..{case['head']}",
        "ground_truth": gt,
        "findings": findings,
    }
    return packet, key


def blind_violations(packet: dict) -> list[str]:
    """Structure only: a finding may carry just the allowed fields, and no field value may be
    an arm code. The prose is the tool's own and is not policed — this repository's findings
    legitimately discuss review tooling, and a word list would refuse them."""
    bad = []
    for f in packet.get("findings", []):
        for k, v in f.items():
            if k not in ("id", "file", "text", "title", "lines", "code"):
                bad.append(f"field {k}")
            if isinstance(v, str) and v.strip() in ARMS:
                bad.append(f"value {v!r} in {k}")
    for k in packet:
        if k not in ("case", "reviewed_state", "diff_range", "ground_truth", "findings"):
            bad.append(f"packet field {k}")
    return bad


# --------------------------------------------------------------------------------------
# Scoring. Labels come from the judge; the key (id -> arm) is joined here and nowhere else.
# --------------------------------------------------------------------------------------


def score(cases: list[dict], gts: dict, keys: dict, labels: dict, runs: dict) -> dict:
    """Per-arm totals. ``runs[(arm, case)]`` is the run's meta (status, seconds, tokens)."""
    out = {}
    for arm in ARMS:
        t = dict(findings=0, real=0, minor=0, noise=0, unverifiable=0, unlabelled=0, gt_total=0, gt_hit=0,
                 runs=0, runs_failed=0, seconds=0, tokens=0, unique_real=0, clean_cases=0,
                 clean_cases_with_real=0, clean_cases_with_findings=0)
        for c in cases:
            cid = c["id"]
            meta = runs.get((arm, cid))
            if meta is None:
                continue
            t["runs"] += 1
            t["seconds"] += meta.get("seconds", 0)
            t["tokens"] += meta.get("tokens") or 0
            if not meta.get("complete"):
                t["runs_failed"] += 1
            lab = {f["id"]: f for f in labels.get(cid, {}).get("findings", [])}
            key = keys.get(cid, {})
            mine = [fid for fid, k in key.items() if k["arm"] == arm]
            hit = set()
            clusters_other = {lab[fid].get("cluster") for fid, k in key.items()
                              if k["arm"] != arm and fid in lab and lab[fid].get("cluster")}
            real_here = 0
            for fid in mine:
                f = lab.get(fid)
                t["findings"] += 1
                if f is None or (f.get("verdict") not in VERDICTS and not f.get("gt")):
                    t["unlabelled"] += 1  # a skipped finding is reported, never folded into a verdict
                    continue
                v = f.get("verdict")
                if f.get("gt"):
                    v = "real"  # a finding that matches a ground-truth item is real by construction
                    hit.update(f["gt"])
                t[v] += 1
                if v == "real":
                    real_here += 1
                    if f.get("cluster") and f["cluster"] not in clusters_other:
                        t["unique_real"] += 1
            if c["kind"] == "fix":
                ids = {g["id"] for g in gts.get(cid, [])}
                t["gt_total"] += len(ids)
                t["gt_hit"] += len(ids & hit)
            else:
                t["clean_cases"] += 1
                t["clean_cases_with_findings"] += 1 if mine else 0
                t["clean_cases_with_real"] += 1 if real_here else 0
        judged = t["real"] + t["minor"] + t["noise"] + t["unverifiable"]  # unlabelled is not judged
        t["precision_real"] = round(t["real"] / judged, 3) if judged else None
        t["recall_gt"] = round(t["gt_hit"] / t["gt_total"], 3) if t["gt_total"] else None
        out[arm] = t
    return out


# --------------------------------------------------------------------------------------
# Running arms.
# --------------------------------------------------------------------------------------


def redact(text: str, secret: str) -> str:
    return text.replace(secret, "***") if secret else text


GIT_LOCK = threading.Lock()  # branch and worktree administration in one clone is not concurrent-safe


def git(clone: Path, *args: str) -> str:
    with GIT_LOCK:
        return subprocess.run(["git", "-C", str(clone), *args], capture_output=True, text=True,
                              check=True).stdout


def remove_worktree(clone: Path, wt: Path) -> None:
    with GIT_LOCK:
        subprocess.run(["git", "-C", str(clone), "worktree", "remove", "--force", str(wt)], check=False,
                       capture_output=True)


def arm_command(arm: str, ctx: dict) -> tuple[list[str], dict]:
    """(argv, extra env) for one run. ``ctx`` carries paths; nothing secret is in argv."""
    if arm == "O1":
        argv = [ctx["ocr"], "review", "--from", ctx["base_ref"], "--to", ctx["head"], "--format", "json",
                "--output", str(ctx["raw"]), "--rule", ctx["rule"], "--effort", "low", "--no-filter",
                "--max-tools", "150", "--timeout", "30", "--exclude", ",".join(EXCLUDES)]
        env = {"OCR_LLM_URL": ENDPOINT, "OCR_LLM_PROTOCOL": "openai", "OCR_LLM_MODEL": MODEL,
               "OCR_LLM_TOKEN": ctx["key"], "OCR_LLM_EXTRA_HEADERS": f"x-opencode-session={ctx['session']}"}
        return argv, env
    tool = "review" if arm == "PR" else "improve"
    argv = [ctx["pa"], "--json-output", str(ctx["raw"]), f"--pr_url={ctx['base_ref']}", tool]
    env = {
        "CONFIG__GIT_PROVIDER": "local", "CONFIG__MODEL": f"openai/{MODEL}",
        "CONFIG__FALLBACK_MODELS": json.dumps([f"openai/{MODEL}"]),
        "OPENAI__KEY": ctx["key"], "OPENAI__API_BASE": ENDPOINT,
        "CONFIG__CUSTOM_MODEL_MAX_TOKENS": "128000",
        # The default 120 s timed the reasoning model out in the smoke test; this is a harness
        # necessity, not a tuning of the tool, and the spec records it.
        "CONFIG__AI_TIMEOUT": "600",
        "LITELLM__EXTRA_HEADERS": json.dumps({"x-opencode-session": ctx["session"]}),
        "IGNORE__GLOB": json.dumps(list(EXCLUDES)),
        "LOCAL__REVIEW_PATH": str(ctx["raw"]) + ".review.md",
        "LOCAL__IMPROVE_PATH": str(ctx["raw"]) + ".improve.md",
    }
    return argv, env


def tokens_of(arm: str, doc: object) -> int | None:
    if not isinstance(doc, dict):
        return None
    if arm == "O1":
        s = doc.get("summary")
        return s.get("total_tokens") if isinstance(s, dict) else None
    u = doc.get("usage")
    return u.get("total_tokens") if isinstance(u, dict) else None


def run_one(arm: str, case: dict, args: argparse.Namespace, key: str) -> dict:
    out = Path(args.out) / arm / case["id"]
    meta_path = out / "meta.json"
    if meta_path.exists() and json.loads(meta_path.read_text()).get("complete") and not args.force:
        return json.loads(meta_path.read_text())
    out.mkdir(parents=True, exist_ok=True)
    clone = Path(args.clone)
    base_ref = f"bakeoff-base-{case['id']}"
    wt = Path(args.work) / f"{arm}-{case['id']}"
    git(clone, "branch", "-f", base_ref, case["base"])
    if wt.exists():
        remove_worktree(clone, wt)
    git(clone, "worktree", "add", "-q", "--detach", str(wt), case["head"])
    raw = out / "raw.json"
    session = f"bakeoff-{arm}-{case['id']}"
    ctx = dict(ocr=args.ocr, pa=args.pa, base_ref=base_ref, head=case["head"], raw=raw, rule=args.rule,
               key=key, session=session)
    argv, extra = arm_command(arm, ctx)
    home = out / "home"
    home.mkdir(exist_ok=True)
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(home), "LANG": "C.UTF-8", **extra}
    attempts, started, rc, log = 0, time.time(), -1, ""
    for attempts in range(1, 4):
        raw.unlink(missing_ok=True)
        try:
            p = subprocess.run(argv, cwd=wt, env=env, capture_output=True, text=True,
                               timeout=args.timeout_min * 60)
            rc, log = p.returncode, p.stdout + "\n--- stderr ---\n" + p.stderr
        except subprocess.TimeoutExpired as e:
            rc, log = 124, f"timed out after {args.timeout_min} min\n{e.stdout or ''}"
        norm = None
        if raw.exists():
            try:
                norm = NORMALIZERS[arm](json.loads(raw.read_text()))
            except json.JSONDecodeError:
                norm = None
        if rc == 0 and norm and norm["complete"]:
            break
        time.sleep(20)
    seconds = round(time.time() - started)
    (out / "run.log").write_text(redact(log, key)[-200_000:])
    remove_worktree(clone, wt)
    doc = json.loads(raw.read_text()) if raw.exists() and raw.stat().st_size else None
    norm = NORMALIZERS[arm](doc) if doc is not None else {"status": "no result file", "complete": False,
                                                          "findings": []}
    (out / "normalized.json").write_text(json.dumps(norm, indent=1))
    meta = dict(arm=arm, case=case["id"], model=MODEL, exit=rc, attempts=attempts, seconds=seconds,
                tokens=tokens_of(arm, doc), status=norm["status"], complete=bool(norm["complete"] and rc == 0),
                findings=len(norm["findings"]))
    meta_path.write_text(json.dumps(meta, indent=1))
    return meta


def cmd_run(args: argparse.Namespace) -> int:
    cases = json.loads((HERE / "cases.json").read_text())
    if args.cases != "all":
        want = set(args.cases.split(","))
        cases = [c for c in cases if c["id"] in want]
    arms = [a for a in args.arms.split(",") if a]
    bad = [a for a in arms if a not in ARMS]
    if bad or not cases:
        print(f"usage: unknown arm {bad} or no case matched", file=sys.stderr)
        return 2
    key = Path(args.key_file).read_text().strip()
    Path(args.work).mkdir(parents=True, exist_ok=True)
    jobs = [(a, c) for c in cases for a in arms]
    failed = 0
    with ThreadPoolExecutor(max_workers=args.jobs) as ex:
        for meta in ex.map(lambda j: run_one(j[0], j[1], args, key), jobs):
            ok = meta["complete"]
            failed += 0 if ok else 1
            print(f"{'ok  ' if ok else 'FAIL'} {meta['arm']} {meta['case']:<10} {meta['seconds']:>5}s "
                  f"findings={meta['findings']:<3} tokens={meta['tokens']} status={meta['status']}", flush=True)
    return 1 if failed else 0


def load_runs(out: Path, cases: list[dict]) -> tuple[dict, dict]:
    runs, per_case = {}, {}
    for c in cases:
        for arm in ARMS:
            d = out / arm / c["id"]
            if (d / "meta.json").exists():
                runs[(arm, c["id"])] = json.loads((d / "meta.json").read_text())
                per_case.setdefault(c["id"], {})[arm] = json.loads((d / "normalized.json").read_text())["findings"]
    return runs, per_case


def cmd_pack(args: argparse.Namespace) -> int:
    cases = json.loads((HERE / "cases.json").read_text())
    out, packets, keys = Path(args.out), Path(args.packets), Path(args.keys)
    packets.mkdir(parents=True, exist_ok=True)
    keys.mkdir(parents=True, exist_ok=True)
    _, per_case = load_runs(out, cases)
    gts = json.loads(Path(args.gt).read_text()) if args.gt else {}
    n = 0
    for c in cases:
        per_arm = per_case.get(c["id"])
        if not per_arm or not any(per_arm.values()):
            continue
        packet, key = build_packet(c, gts.get(c["id"], []), per_arm)
        bad = blind_violations(packet)
        if bad:
            print(f"refusing {c['id']}: packet is not blind: {bad}", file=sys.stderr)
            return 1
        (packets / f"{c['id']}.json").write_text(json.dumps(packet, indent=1))
        (keys / f"{c['id']}.json").write_text(json.dumps(key, indent=1))
        n += 1
    print(f"{n} packets in {packets}; keys in {keys} (keep the keys away from the judges)")
    return 0


def cmd_score(args: argparse.Namespace) -> int:
    cases = json.loads((HERE / "cases.json").read_text())
    runs, _ = load_runs(Path(args.out), cases)
    gts = json.loads(Path(args.gt).read_text()) if args.gt else {}
    keys, labels = {}, {}
    for c in cases:
        kp, lp = Path(args.keys) / f"{c['id']}.json", Path(args.labels) / f"{c['id']}.json"
        if kp.exists():
            keys[c["id"]] = json.loads(kp.read_text())
        if lp.exists():
            labels[c["id"]] = json.loads(lp.read_text())
    result = score(cases, gts, keys, labels, runs)
    print(json.dumps(result, indent=1))
    return 0


# --------------------------------------------------------------------------------------
# Self-check: the pure parts, table-driven, with a mutation note for each load-bearing case.
# --------------------------------------------------------------------------------------


def self_check() -> int:
    failures: list[str] = []

    def check(name: str, got: object, want: object) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    # OCR: a failed status is never a clean review, even with no comments.
    check("ocr failed is incomplete", normalize_ocr({"status": "failed", "comments": None})["complete"], False)
    check("ocr coverage.failed is incomplete",
          normalize_ocr({"status": "complete", "comments": [], "manifest": {"coverage": {"failed": [{"x": 1}]}}})["complete"],
          False)
    ocr = normalize_ocr({"status": "complete", "comments": [
        {"path": "src/a.rs", "start_line": 3, "end_line": 4, "category": "bug", "severity": "High", "content": " x "}]})
    check("ocr finding", ocr["findings"][0], {"file": "src/a.rs", "start": 3, "end": 4, "title": "bug",
                                              "text": "x", "tool_severity": "high"})
    check("ocr complete clean", normalize_ocr({"status": "complete", "comments": []})["complete"], True)
    check("ocr not an object", normalize_ocr([])["status"], "unreadable")
    # PR-Agent review: an empty issue list with "No" security is clean; a security note is a finding.
    pr = normalize_pa_review({"review": {"key_issues_to_review": [], "security_concerns": "No\n"}})
    check("pa review clean", (pr["complete"], pr["findings"]), (True, []))
    pr = normalize_pa_review({"review": {"key_issues_to_review": [
        {"relevant_file": "a.rs\n", "issue_header": "Possible Bug", "issue_content": "boom", "start_line": 9, "end_line": 9}],
        "security_concerns": "Yes: token in argv"}})
    check("pa review two findings", [f["title"] for f in pr["findings"]], ["Possible Bug", "security concern"])
    check("pa review file stripped", pr["findings"][0]["file"], "a.rs")
    check("pa review unreadable", normalize_pa_review({"usage": {}})["complete"], False)
    # PR-Agent improve: keeps the model's own score aside, never mixes it into severity.
    pi = normalize_pa_improve({"code_suggestions": [
        {"relevant_file": "b.rs\n", "one_sentence_summary": "Do x\n", "suggestion_content": "because\n",
         "existing_code": "let a;\n", "score": 6}]})
    check("pa improve", (pi["findings"][0]["tool_score"], pi["findings"][0]["tool_severity"]), (6, None))
    check("pa improve empty is complete", normalize_pa_improve({"code_suggestions": []})["complete"], True)
    # Packets: blind, shuffled deterministically, ids neutral.
    case = {"id": "t001", "base": "b" * 40, "head": "h" * 40}
    per_arm = {"O1": [{"file": "a", "start": 1, "end": 1, "title": "bug", "text": "one", "tool_severity": "high"}],
               "PR": [{"file": "b", "start": None, "end": None, "title": "", "text": "two", "tool_severity": None}],
               "PI": [{"file": "c", "start": None, "end": None, "title": "t", "text": "three",
                       "existing_code": "x", "tool_severity": None, "tool_score": 9}]}
    p1, k1 = build_packet(case, [{"id": "G1", "text": "g"}], per_arm)
    p2, k2 = build_packet(case, [{"id": "G1", "text": "g"}], per_arm)
    check("packet deterministic", (p1, k1), (p2, k2))
    check("packet blind", blind_violations(p1), [])
    check("packet ids neutral", sorted(f["id"] for f in p1["findings"]), ["F01", "F02", "F03"])
    check("key covers every arm", sorted(v["arm"] for v in k1.values()), ["O1", "PI", "PR"])
    check("tool score never reaches the judge", "tool_score" in json.dumps(p1) or "tool_severity" in json.dumps(p1), False)
    leaky = dict(p1, findings=[dict(p1["findings"][0], arm="O1")])
    check("a packet naming an arm is refused", bool(blind_violations(leaky)), True)
    # Scoring: gt match is real by construction; unique_real needs a cluster nobody else shares.
    cases = [{"id": "t001", "kind": "fix"}, {"id": "t002", "kind": "clean"}]
    gts = {"t001": [{"id": "G1"}, {"id": "G2"}]}
    keys = {"t001": {"F01": {"arm": "O1"}, "F02": {"arm": "PR"}, "F03": {"arm": "O1"}},
            "t002": {"F01": {"arm": "PR"}}}
    labels = {"t001": {"findings": [
        {"id": "F01", "verdict": "minor", "gt": ["G1"], "cluster": "c1"},
        {"id": "F02", "verdict": "noise"},
        {"id": "F03", "verdict": "real", "cluster": "c2"}]},
        "t002": {"findings": [{"id": "F01", "verdict": "real", "cluster": "c3"}]}}
    runs = {(a, c): {"complete": True, "seconds": 10, "tokens": 5} for a in ARMS for c in ("t001", "t002")}
    runs[("PI", "t001")]["complete"] = False
    s = score(cases, gts, keys, labels, runs)
    check("o1 real", s["O1"]["real"], 2)
    check("o1 recall", s["O1"]["recall_gt"], 0.5)
    check("o1 unique real", s["O1"]["unique_real"], 2)
    check("pr noise", s["PR"]["noise"], 1)
    check("pr clean case with a real finding", s["PR"]["clean_cases_with_real"], 1)
    check("pi failed run counted", s["PI"]["runs_failed"], 1)
    check("pi precision undefined with no findings", s["PI"]["precision_real"], None)
    labels2 = {"t001": {"findings": [{"id": "F01", "verdict": "real"}, {"id": "F02", "verdict": "maybe"}]}}
    s2 = score(cases, gts, keys, labels2, runs)
    check("a missing or invalid verdict is unlabelled, not judged", (s2["PR"]["unlabelled"], s2["PR"]["noise"]), (2, 0))
    check("unlabelled findings stay out of the precision base", s2["O1"]["precision_real"], 1.0)
    # Command lines: no secret in argv, and both PR-Agent arms share the same environment.
    ctx = dict(ocr="ocr", pa="pa", base_ref="b", head="h", raw=Path("/r"), rule="rule.json", key="SECRET",
               session="s")
    for arm in ARMS:
        argv, env = arm_command(arm, ctx)
        check(f"{arm} secret not in argv", any("SECRET" in a for a in argv), False)
        check(f"{arm} secret reaches env", "SECRET" in env.values(), True)
    check("O1 keeps production flags", "--no-filter" in arm_command("O1", ctx)[0], True)
    check("O1 and PR exclude the same classes",
          arm_command("O1", ctx)[0][arm_command("O1", ctx)[0].index("--exclude") + 1].split(","),
          json.loads(arm_command("PR", ctx)[1]["IGNORE__GLOB"]))
    check("redact", redact("a SECRET b", "SECRET"), "a *** b")
    cases_file = json.loads((HERE / "cases.json").read_text())
    ids = [c["id"] for c in cases_file]
    check("case ids unique", len(ids), len(set(ids)))
    check("every fix case names its fix commit",
          [c["id"] for c in cases_file if c["kind"] == "fix" and not re.fullmatch(r"[0-9a-f]{40}", c.get("fix", ""))], [])
    check("kinds", sorted({c["kind"] for c in cases_file}), ["clean", "fix"])
    for f in failures:
        print("FAIL", f)
    print(f"bakeoff self-check: {'FAIL' if failures else 'ok'} ({len(failures)} failures)")
    return 1 if failures else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--self-check", action="store_true")
    sub = ap.add_subparsers(dest="cmd")
    r = sub.add_parser("run", help="execute arms over cases (resumable)")
    r.add_argument("--clone", required=True, help="a clone of the repo; worktrees are made and removed here")
    r.add_argument("--work", required=True, help="directory for the per-run worktrees")
    r.add_argument("--out", required=True)
    r.add_argument("--key-file", required=True)
    r.add_argument("--ocr", required=True, help="path to the pinned ocr binary")
    r.add_argument("--pa", required=True, help="path to the pinned pr-agent binary")
    r.add_argument("--rule", required=True, help="review/rule.json to give the OCR arm")
    r.add_argument("--arms", default=",".join(ARMS))
    r.add_argument("--cases", default="all")
    r.add_argument("--jobs", type=int, default=3)
    r.add_argument("--timeout-min", type=int, default=50)
    r.add_argument("--force", action="store_true")
    p = sub.add_parser("pack", help="blind judge packets")
    p.add_argument("--out", required=True)
    p.add_argument("--packets", required=True)
    p.add_argument("--keys", required=True)
    p.add_argument("--gt", help="ground-truth JSON: {case: [{id, text}]}")
    s = sub.add_parser("score", help="aggregate judge labels")
    s.add_argument("--out", required=True)
    s.add_argument("--keys", required=True)
    s.add_argument("--labels", required=True)
    s.add_argument("--gt")
    a = ap.parse_args(argv)
    if a.self_check:
        return self_check()
    return {"run": cmd_run, "pack": cmd_pack, "score": cmd_score}.get(a.cmd, lambda _: (ap.print_usage(), 2)[1])(a)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
