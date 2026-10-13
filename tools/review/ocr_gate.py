#!/usr/bin/env python3
"""The verdict on an OCR review result: pass, fail, or a legitimate skip.

The counting lives here rather than inline in the workflow for the reason this repository
keeps rediscovering — a check written twice drifts, and the copy that drifts is the one
nobody reads. The workflow takes this file from `origin/main`, never from the pull request
under review, so a change cannot supply the gate that judges it.

Three states, as everywhere in req:

* **pass** — no critical or high finding.
* **pass, skipped** — a diff with nothing reviewable (docs only), *and* the fingerprint
  agrees: zero files reviewed and no comments. A skip that claims otherwise is a failure.
* **fail** — any critical or high finding; a review that did not complete; a result file
  missing, unreadable, empty or unparseable; or an unrecognised schema.

A review that did not complete is never a pass. OCR writes its result file even when the
model call fails, with `status: failed` and `comments: null`, so counting findings before
checking the status would read a broken run as a clean one — which is the defect the
sibling repository recorded first and the reason the status is checked first here.

Usage:
    python3 tools/review/ocr_gate.py <result.json> [--emit-json <path>] [--self-check]

Exit codes: 0 pass (including a legitimate skip), 1 fail, 2 usage (argparse).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

EXIT_PASS = 0
EXIT_FAIL = 1

COMPLETE = frozenset({"complete", "success"})
BLOCKING = frozenset({"critical", "high"})
KNOWN = ("critical", "high", "medium", "low")


def verdict(document: object) -> tuple[int, list[str]]:
    """Decide an already-parsed result. Pure, so --self-check can table-drive it."""
    if not isinstance(document, dict):
        return EXIT_FAIL, ["FAIL  result is not a JSON object"]

    if "comments" not in document:
        if "issues" in document:
            return EXIT_FAIL, [
                "FAIL  this result uses a legacy schema: 'issues' instead of 'comments'.",
                "      Modern OCR emits comments[] with a severity field. Fix the caller;",
                "      do not weaken this gate.",
            ]
        return EXIT_FAIL, ["FAIL  neither 'comments' nor 'issues' — unrecognised schema"]

    status = document.get("status", "missing")
    comments = document.get("comments") or []
    if not isinstance(comments, list):
        return EXIT_FAIL, ["FAIL  'comments' is present but not a list"]

    if status == "skipped":
        summary = document.get("summary") or {}
        reviewed = summary.get("files_reviewed", 0) if isinstance(summary, dict) else 0
        if reviewed in (0, "0") and not comments:
            return EXIT_PASS, ["PASS  skipped: nothing reviewable in this diff"]
        return EXIT_FAIL, [
            "FAIL  status is 'skipped' but the summary disagrees "
            f"(files_reviewed={reviewed!r}, comments={len(comments)})",
        ]

    if status not in COMPLETE:
        return EXIT_FAIL, [
            f"FAIL  the review did not complete (status: {status!r}) — rerun the job, do not merge",
        ]

    blocking = 0
    unknown: list[str] = []
    hist: dict[str, int] = {}
    for comment in comments:
        if not isinstance(comment, dict):
            unknown.append("a comment that is not an object")
            blocking += 1
            continue
        severity = str(comment.get("severity", "")).strip().lower()
        hist[severity or "missing"] = hist.get(severity or "missing", 0) + 1
        if severity in BLOCKING:
            blocking += 1
        elif severity not in KNOWN:
            # Fail closed on a severity we do not recognise: a schema drift must be loud,
            # not a silent pass. Counted as blocking and named.
            unknown.append(f"unrecognised severity {comment.get('severity')!r} at "
                           f"{comment.get('path')}:{comment.get('start_line')}")
            blocking += 1

    lines = [f"      total findings: {len(comments)}",
             "      by severity: " + (", ".join(f"{s}={hist[s]}" for s in KNOWN if s in hist)
                                     or "none")
             + ((", " + ", ".join(f"{k}={v}" for k, v in hist.items() if k not in KNOWN))
                if any(k not in KNOWN for k in hist) else "")]
    for comment in comments:
        if not isinstance(comment, dict):
            continue
        lines.append("      [%s] %s:%s — %s" % (comment.get("severity", "?"),
                                                comment.get("path", "?"),
                                                comment.get("start_line", "?"),
                                                str(comment.get("content", ""))[:200]))
    for note in unknown:
        lines.append("      ! " + note)
    if blocking:
        lines.append(f"FAIL  {blocking} blocking finding(s) — fix them and re-run the review")
        return EXIT_FAIL, lines
    return EXIT_PASS, lines + ["PASS  no critical or high findings"]


def load(path: Path) -> tuple[object | None, int, list[str]]:
    if not path.exists():
        return None, EXIT_FAIL, [f"FAIL  {path} not found — the review step did not complete"]
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        return None, EXIT_FAIL, [f"FAIL  {path} could not be read: {error}"]
    if not raw.strip():
        return None, EXIT_FAIL, [f"FAIL  {path} is empty"]
    try:
        return json.loads(raw), EXIT_PASS, []
    except json.JSONDecodeError as error:
        return None, EXIT_FAIL, [f"FAIL  {path} is not valid JSON: {error}"]


def self_check() -> int:
    cases = [
        ("a clean completed review passes", {"status": "complete", "comments": []}, EXIT_PASS),
        ("a critical finding blocks", {"status": "complete",
         "comments": [{"severity": "critical", "path": "a.rs", "start_line": 1}]}, EXIT_FAIL),
        ("a high finding blocks", {"status": "complete",
         "comments": [{"severity": "high", "path": "a.rs", "start_line": 1}]}, EXIT_FAIL),
        ("medium and low do not block", {"status": "complete",
         "comments": [{"severity": "medium"}, {"severity": "low"}]}, EXIT_PASS),
        ("severity matching is case-insensitive", {"status": "complete",
         "comments": [{"severity": "HIGH"}]}, EXIT_FAIL),
        ("an unrecognised severity fails closed", {"status": "complete",
         "comments": [{"severity": "catastrophic"}]}, EXIT_FAIL),
        ("a failed run is not a clean run", {"status": "failed", "comments": None}, EXIT_FAIL),
        ("a missing status fails", {"comments": []}, EXIT_FAIL),
        ("the legacy schema is named", {"status": "complete", "issues": []}, EXIT_FAIL),
        ("an unmatched schema fails", {"status": "complete"}, EXIT_FAIL),
        ("a docs-only skip passes", {"status": "skipped",
         "summary": {"files_reviewed": 0}, "comments": []}, EXIT_PASS),
        ("a skip that disagrees with its summary fails", {"status": "skipped",
         "summary": {"files_reviewed": 3}, "comments": []}, EXIT_FAIL),
        ("a non-object result fails", ["not", "an", "object"], EXIT_FAIL),
    ]
    failures = 0
    for label, document, expected in cases:
        got, _ = verdict(document)
        mark = "ok  " if got == expected else "FAIL"
        if got != expected:
            failures += 1
        print("  %s %-48s expected %d, got %d" % (mark, label, expected, got))
    print("  %d case(s), %d failure(s)" % (len(cases), failures))
    return EXIT_FAIL if failures else EXIT_PASS


def main() -> int:
    parser = argparse.ArgumentParser(description="the verdict on an OCR review result")
    parser.add_argument("result", nargs="?", help="the ocr-result.json to judge")
    parser.add_argument("--emit-json", help="write normalized findings here")
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()

    if args.self_check:
        return self_check()
    if not args.result:
        parser.error("a result file is required unless --self-check is given")

    document, code, lines = load(Path(args.result))
    if document is None:
        print("\n".join(lines))
        return code
    code, lines = verdict(document)
    print("\n".join(lines))
    if args.emit_json and isinstance(document, dict):
        comments = document.get("comments") or []
        blocking = [c for c in comments if isinstance(c, dict)
                    and (str(c.get("severity", "")).lower() in BLOCKING
                         or str(c.get("severity", "")).lower() not in KNOWN)]
        Path(args.emit_json).write_text(json.dumps(
            {"status": "pass" if code == EXIT_PASS else "fail",
             "counts": {"total": len(comments), "blocking": len(blocking)},
             "findings": comments}, indent=2) + "\n", encoding="utf-8")
        print("      normalized findings written to %s" % args.emit_json)
    return code


if __name__ == "__main__":
    sys.exit(main())
