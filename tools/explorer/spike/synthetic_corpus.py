#!/usr/bin/env python3
"""Make a synthetic req corpus for the explorer's measurement (ADR 0054 decision 4, unit E-1). Never committed as data.

    python3 tools/explorer/spike/synthetic_corpus.py <claims> <dir>

The shape is the one ADR 0050's note describes (a heap-shaped tree of `derives-from`, a `refines` cross-link on every
seventh claim, every tenth claim standalone), extended for the explorer: every claim has an `area` (12 areas at two
depths, as A-2 did) and, for 95 % of them, a `rationale.source` (three claims share a section); two to five `tests:` ids
(none on 3 % of claims); and a `req-satisfies:` design link on about 70 % of them, recorded with `req satisfy --record`.
After recording, a few statements are tightened (a SUSPECT link) or loosened (HOLDS (weakened)), a few links are left
unrecorded (UNRATIFIED) and a few recorded declarations are removed (ORPHANED), so every outcome is in the page.

It writes nothing outside <dir>, which must not exist. Python standard library only."""
import json
import os
import sys

AREAS = [
    ("authoring", "Authoring"), ("authoring/claims", "Claims"), ("identity", "Identity"), ("identity/hashing", "Hashing"),
    ("decisions", "Decisions"), ("decisions/chain", "Chain"), ("variants", "Variants"), ("variants/select", "Select"),
    ("verification", "Verification"), ("verification/gate", "Gate"), ("operation", "Operation"), ("operation/offline", "Offline"),
]
REAL_COMPONENT_FILES = 24  # how many of the architecture overlay's cited files the corpus also uses as design elements


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)


def statement(i, threshold):
    return f"The system shall answer request type R{i} in no more than {threshold} ms."


def claim_text(i, n, threshold):
    area = AREAS[i % len(AREAS)][0]
    source = None if i % 20 == 3 else f"docs/spec/section-{(i - 1) // 3 + 1}.md"
    tests = [] if i % 33 == 5 else [f"suite{(i // 40) % 50}::case_{i}_{k}" for k in range(2 + (i % 4))]
    lines = ["schema: req.claim/3", f"claim_id: REQ-{i:04d}", f"area: {area}", "statement: >-",
             f"  {statement(i, threshold)}", "applicability: []"]
    lines += ["tests:"] + [f'  - "{t}"' for t in tests] if tests else ["tests: []"]
    lines += ["evidence: machine", "rationale:"]
    lines += [f'  source: "{source or ""}"']
    lines += ["  date: 2026-10-07", "  rejected_alternatives: []", "  assumptions: []"]
    relations = []
    if i > 1 and i % 10 != 0:
        relations.append(("derives-from", i // 2))
    if i % 7 == 0 and i > 2:
        relations.append(("refines", (i * 7) % (i - 1) + 1))
    lines += ["relations:"] + [f"  - type: {t}\n    to: REQ-{to:04d}" for t, to in relations] if relations else ["relations: []"]
    return "\n".join(lines) + "\n"


def main(argv):
    args = list(argv)
    if len(args) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    n, out = int(args[0]), os.path.abspath(args[1])
    if os.path.exists(out):
        print(f"{out} exists", file=sys.stderr)
        return 2
    here = os.path.dirname(os.path.abspath(__file__))
    repo = os.path.dirname(os.path.dirname(os.path.dirname(here)))
    thresholds = {i: 100 + (i * 37) % 900 for i in range(1, n + 1)}
    for i in range(1, n + 1):
        write(os.path.join(out, "claims", f"REQ-{i:04d}.yml"), claim_text(i, n, thresholds[i]))
    write(os.path.join(out, "aliases", "subjects.yml"),
          "schema: req.subject_registry/0\nversion: 1\nsubjects:\n  SYSTEM:\n    label: The system\n    aliases:\n      - the system\n")
    write(os.path.join(out, ".req", "areas"), "".join(f"{p} {label}\n" for p, label in AREAS))
    write(os.path.join(out, ".req", "design"), "design\nsrc\n")
    write(os.path.join(out, ".req", "variants"), "# one release: every claim applies\ndeployment_mode=air_gapped\n")
    # Design elements: files of the real architecture overlay (so some are labelled with a component) and synthetic ones.
    cited = []
    with open(os.path.join(repo, "docs", "architecture", "overlay.json"), encoding="utf-8") as handle:
        for component in json.load(handle)["components"]:
            for source in component.get("sources", []):
                if source["path"].startswith("src/") and source["path"] not in cited:
                    cited.append(source["path"])
    pool = cited[:REAL_COMPONENT_FILES] + [f"design/m{k // 20:03d}/part_{k:04d}.md" for k in range(max(1, n // 5))]
    declared = {}
    for i in range(1, n + 1):
        if i % 10 in (1, 2, 4, 5, 7, 8, 9):  # 70 %
            declared.setdefault(pool[(i * 13) % len(pool)], []).append(i)
            if i % 25 == 0:  # a second element for some claims
                declared.setdefault(pool[(i * 29 + 7) % len(pool)], []).append(i)
    for element, claims in sorted(declared.items()):
        comment = "//!" if element.endswith(".rs") else "<!--"
        close = "" if element.endswith(".rs") else " -->"
        write(os.path.join(out, element), "".join(f"{comment} req-satisfies: REQ-{i:04d}{close}\n" for i in sorted(set(claims))))
    links = sorted({(element, i) for element, claims in declared.items() for i in claims})
    skip_record = {link for k, link in enumerate(links) if k % 50 == 7}  # UNRATIFIED
    # What `req satisfy --record <element> <claim>` writes, written in one pass: one `--record` call loads the whole corpus,
    # which is hours at 5,000 claims. The 500-claim corpus was made with `--record` and this writer reproduces its file
    # byte for byte (checked when the spike was run, recorded in ADR 0054's note).
    recorded_links = [link for link in links if link not in skip_record]
    body = "".join(f"- element: {element}\n  claim_id: REQ-{i:04d}\n  statement: {statement(i, thresholds[i])}\n"
                   for element, i in sorted(recorded_links, key=lambda link: (link[0], f"REQ-{link[1]:04d}")))
    write(os.path.join(out, ".req", "satisfied"), "schema: req.satisfied/1\nlinks:\n" + body)
    # Drift after recording: tighten (SUSPECT), loosen (HOLDS (weakened)), and orphan a few links.
    recorded = [link for link in links if link not in skip_record]
    for k, (element, i) in enumerate(recorded):
        if k % 60 == 11:
            write(os.path.join(out, "claims", f"REQ-{i:04d}.yml"), claim_text(i, n, max(1, thresholds[i] // 2)))
        elif k % 60 == 29:
            write(os.path.join(out, "claims", f"REQ-{i:04d}.yml"), claim_text(i, n, thresholds[i] * 2))
        elif k % 80 == 43:  # remove the declaration: the record stays, so the link is ORPHANED
            path = os.path.join(out, element)
            with open(path, encoding="utf-8") as handle:
                kept = [line for line in handle if f"REQ-{i:04d}" not in line]
            write(path, "".join(kept))
    print(f"wrote {n} claims, {len(links)} links under {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
