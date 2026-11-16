#!/usr/bin/env python3
"""Build, or check, docs/explorer.html: the page that navigates the coverage graph (ADR 0054, unit E-1).

The page is GENERATED, never hand-edited. It is the fixed template `tools/explorer/explorer.html` (hand-written HTML, CSS and
script: the same bytes for every corpus) with ONE data document inserted, in
`<script type="application/json" id="req-explorer-data">`. The document is computed from three things and nothing else:

    req trace --format json <selection>   the claims: statement, area, evidence, source, tests (ADR 0046, 0051, 0053)
    req satisfy --format json             the design links and each one's outcome (ADR 0040)
    docs/architecture/overview-overlay.json and overlay.json
                                          the architecture overlays' component maps, only to label a design file with its
                                          component and to link it (ADR 0052): a file is mapped to the first component that
                                          cites it, the overview's before the detail's

The selection is the first line of `.req/variants` (the one `docs/trace.md` is made with); the page states it. No test outcome
is read: a test outcome depends on a run, not on the repository, so the committed page holds none (ADR 0054 decision 6).
Nothing here is chosen by a model (ADR 0004), and nothing is added to what those documents hold: a node or an edge on the
page is a node or an edge in them (*Falsification* 1). The output has no timestamp and every key and list is sorted, so
two builds over one repository are the same bytes (*Falsification* 4).

    python3 tools/explorer/build.py [--req <path>] --out <page>       write the page
    python3 tools/explorer/build.py [--req <path>] --check <page>     exit 0 current, 1 stale

The corpus is the repository this script lives in (its parent's parent), so a copy of the repository checks itself.
Python standard library only, offline. `req` is taken from `--req`, else PATH, like tools/overview/build.py.

Exit: 0 written / current; 1 stale (the page is named); 2 usage; 3 UNVERIFIABLE, nothing was compared or written: `req` cannot
be run, or did not print the complete `req.trace/1` and `req.satisfy/1` documents (a schema this build was not written
against, or a file `req satisfy` could not read), or an overlay cannot be read.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TEMPLATE = os.path.join(ROOT, "tools", "explorer", "explorer.html")
TOKEN = "__REQ_EXPLORER_DATA__"
SCHEMA = "req.explorer/1"
TRACE_SCHEMA = "req.trace/1"
SATISFY_SCHEMA = "req.satisfy/1"
# Where the architecture pages are published (the URL README.md links); a design node opens its component there.
PAGES_URL = "https://autox-my.github.io/req"
# (the page key, its overlay, the page the overlay is rendered to), the overview first.
OVERLAYS = (
    ("overview", "docs/architecture/overview-overlay.json", f"{PAGES_URL}/architecture.html"),
    ("detail", "docs/architecture/overlay.json", f"{PAGES_URL}/architecture-detail.html"),
)


class Unverifiable(Exception):
    """`req` could not be run, or what it printed (or an input) is not the document this build reads."""


def req_json(binary, schema, *args):
    """Run `req --root <repo> <args>` and return its stdout document, if it is the complete document `schema` names.

    `trace` and `satisfy` exit non-zero for a verdict (an undecided edge, a suspect link) and still print the complete
    document, so the exit code is not what is judged. What is judged is the document: it must carry `schema`, and a
    `satisfy` document must list nothing `unreadable` (a file req could not read has no links in it, so the page would show
    its claims as having no design element). Anything else is unverifiable, naming what req reported."""
    env = {k: v for k, v in os.environ.items() if k != "REQ_ROOT"}
    command = f"req {' '.join(args)}"
    try:
        done = subprocess.run([binary, "--root", ROOT, *args], capture_output=True, text=True, check=False,
                              stdin=subprocess.DEVNULL, timeout=600, env=env)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`{command}` could not be run: {why}")
    try:
        document = json.loads(done.stdout)
    except ValueError:
        raise Unverifiable(f"`{command}` exited {done.returncode} without a JSON document: {done.stderr.strip()}")
    if not isinstance(document, dict) or document.get("schema") != schema:
        found = document.get("schema") if isinstance(document, dict) else type(document).__name__
        raise Unverifiable(f"`{command}` printed schema {found!r}, not {schema!r} (exit {done.returncode}): "
                           "this build reads only the schema it was written against")
    if schema == SATISFY_SCHEMA:
        unreadable = document.get("unreadable")
        if not isinstance(unreadable, list):
            raise Unverifiable(f"`{command}` printed no `unreadable` list (exit {done.returncode})")
        if unreadable:
            raise Unverifiable(f"`{command}` reported what it could not read (exit {done.returncode}), so its links "
                               "are incomplete: " + "; ".join(str(item) for item in unreadable))
    return document


def selection_words():
    """The first line of `.req/variants` as `key=value` words: the selection `docs/trace.md` is made with.

    `#` starts a comment and a line empty after stripping it is skipped (the grammar `req check` reads). No file, or no
    line, is the default selection, which `req trace` prints as `{}`."""
    path = os.path.join(ROOT, ".req", "variants")
    try:
        with open(path, encoding="utf-8") as handle:
            lines = handle.read().splitlines()
    except FileNotFoundError:
        return []
    except (OSError, UnicodeDecodeError) as why:
        raise Unverifiable(f"{path} cannot be read: {why}")
    for line in lines:
        words = line.split("#", 1)[0].split()
        if words:
            return words
    return []


def component_map():
    """The overlays' file -> component map: `{path: {"id", "label", "page"}}`, the first component to cite a file winning.

    The overview is read first, so a file the overview draws links to the overview page, else to the detail page."""
    mapped = {}
    for key, relative, _ in OVERLAYS:
        path = os.path.join(ROOT, relative)
        try:
            with open(path, encoding="utf-8") as handle:
                overlay = json.load(handle)
            components = overlay["components"]
            if not isinstance(components, list):
                raise TypeError("`components` is not a list")
            for component in components:
                for source in component.get("sources", []):
                    mapped.setdefault(source["path"], {"id": component["id"], "label": component["label"], "page": key})
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as why:
            raise Unverifiable(f"{path} cannot be read as an architecture overlay: {why}")
    return mapped


def text_or_none(value):
    return value if isinstance(value, str) and value != "" else None


def build_data(trace, satisfy, components):
    """The data document: nodes (sources, claims, design elements, test groups), edges and gaps, all sorted."""
    nodes = sorted(trace["nodes"], key=lambda node: node["claim_id"])
    claims = []
    sources = {}
    source_of = []
    tests = []
    verified_by = []
    known = set()
    for node in nodes:
        claim_id = node["claim_id"]
        known.add(claim_id)
        claims.append({
            "id": claim_id,
            "statement": node["statement"],
            "modality": node.get("modality"),
            "area": text_or_none(node.get("area")),
            "applies": node["applies"],
            "evidence": node.get("evidence", "machine"),
        })
        source = text_or_none(node.get("source"))
        if source is not None:
            sources.setdefault(source, []).append(claim_id)
        if node.get("tests"):
            group = f"T:{claim_id}"
            tests.append({"id": group, "claim": claim_id, "ids": list(node["tests"])})
            verified_by.append({"claim": claim_id, "group": group})
    source_ids = {text: f"S{index:04d}" for index, text in enumerate(sorted(sources), start=1)}
    for text in sorted(sources):
        for claim_id in sources[text]:
            source_of.append({"source": source_ids[text], "claim": claim_id})
    satisfied_by = []
    dangling = []
    for link in satisfy["links"]:
        edge = {"claim": link["claim_id"], "design": link["element"], "outcome": link["outcome"], "line": link.get("line")}
        if link["claim_id"] in known:
            satisfied_by.append(edge)
        else:
            dangling.append({"claim": link["claim_id"], "design": link["element"], "outcome": link["outcome"]})
    satisfied_by.sort(key=lambda e: (e["claim"], e["design"], e["outcome"]))
    dangling.sort(key=lambda e: (e["claim"], e["design"], e["outcome"]))
    linked = {edge["claim"] for edge in satisfied_by}
    design = [{"id": path, "component": components.get(path)} for path in sorted({edge["design"] for edge in satisfied_by})]
    with_source = {edge["claim"] for edge in source_of}
    with_tests = {edge["claim"] for edge in verified_by}
    gap_entries = []
    not_asked = []
    for claim in claims:
        if claim["applies"] != "applies":
            continue  # a claim the selection does not apply is not asked for coverage (the matrix leaves it out, ADR 0053)
        kinds = []
        if claim["id"] not in linked:
            kinds.append("no_design")
        if claim["id"] not in with_source:
            kinds.append("no_source")
        if claim["id"] not in with_tests:
            if claim["evidence"] == "machine":
                kinds.append("no_test")
            else:  # not a gap: the gate does not ask a process claim for a test (ADR 0019, ADR 0053 decision 2)
                not_asked.append({"claim": claim["id"], "evidence": claim["evidence"]})
        if kinds:
            gap_entries.append({"claim": claim["id"], "kinds": sorted(kinds)})
    areas = [{"path": area["path"], "label": area["label"]} for area in trace.get("areas", [])]
    return {
        "schema": SCHEMA,
        "rules_v": trace.get("rules_v"),
        "selection": trace["selection"],
        "pages": {key: url for key, _, url in OVERLAYS},
        "areas": areas,
        "claims": claims,
        "sources": [{"id": source_ids[text], "text": text} for text in sorted(sources)],
        "design": design,
        "tests": tests,
        "edges": {"source_of": source_of, "satisfied_by": satisfied_by, "verified_by": verified_by},
        "gaps": gap_entries,
        "not_asked": not_asked,
        "dangling_links": dangling,
    }


def render(binary):
    """The page's text, or raises Unverifiable."""
    try:
        with open(TEMPLATE, encoding="utf-8", newline="") as handle:
            template = handle.read()
    except OSError as why:
        raise Unverifiable(f"{TEMPLATE} cannot be read: {why}")
    if template.count(TOKEN) != 1:
        raise Unverifiable(f"{TEMPLATE} must name the data's place ({TOKEN}) exactly once")
    trace = req_json(binary, TRACE_SCHEMA, "trace", "--format", "json", *selection_words())
    satisfy = req_json(binary, SATISFY_SCHEMA, "satisfy", "--format", "json")
    for key in ("nodes", "selection"):
        if key not in trace:
            raise Unverifiable(f"`req trace --format json` printed no `{key}`")
    if not isinstance(satisfy.get("links"), list):
        raise Unverifiable("`req satisfy --format json` printed no `links` list")
    data = build_data(trace, satisfy, component_map())
    document = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    # The element is data, never markup: `</` could close it, `<!--` could open an escaped-script state. Both stay valid JSON.
    document = document.replace("</", "<\\/").replace("<!--", "\\u003c!--")
    before, _, after = template.partition(TOKEN)
    return before + document + after


def main(argv):
    mode, target, binary = None, None, None
    args = iter(argv)
    usage = "usage: build.py [--req <path>] --out <page> | --check <page>"
    for arg in args:
        if arg in ("--out", "--check"):
            if mode is not None:
                print(f"{usage} (one mode per run: {mode} is already given, then {arg})", file=sys.stderr)
                return 2
            mode, target = arg, next(args, None)
        elif arg == "--req":
            binary = next(args, None)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"{usage} (unknown: {arg})", file=sys.stderr)
            return 2
    if mode is None or not target or binary == "":
        print(usage, file=sys.stderr)
        return 2
    binary = os.path.abspath(binary) if binary else shutil.which("req")
    if binary is None:
        print("UNVERIFIABLE: `req` is not on PATH (and no --req was given); the page is computed from its real output and "
              "cannot be built or checked without it", file=sys.stderr)
        return 3
    try:
        page = render(binary)
    except Unverifiable as why:
        print(f"UNVERIFIABLE: {why}", file=sys.stderr)
        return 3
    if mode == "--out":
        # Written beside the target and moved into place, so a short write (a full disk) never leaves a truncated page:
        # exit 3 means the old page, if any, is untouched.
        staged = None
        try:
            directory = os.path.dirname(os.path.abspath(target))
            with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="", dir=directory,
                                             prefix=".explorer-", suffix=".tmp", delete=False) as handle:
                staged = handle.name
                handle.write(page)
            os.replace(staged, target)
            staged = None
        except OSError as why:
            if staged is not None:
                try:
                    os.unlink(staged)
                except OSError:
                    pass
            # Not 1: exit 1 means a page is stale, and nothing was compared here.
            print(f"UNVERIFIABLE: cannot write {target}: {why}", file=sys.stderr)
            return 3
        print(f"wrote {target}")
        return 0
    try:
        with open(target, "rb") as handle:
            current = handle.read()
    except OSError as why:
        print(f"STALE {target} cannot be read: {why}", file=sys.stderr)
        return 1
    if current != page.encode("utf-8"):
        print(f"STALE {target}\nregenerate with `python3 tools/explorer/build.py --out {target}` (the page is generated, "
              "never hand-edited)", file=sys.stderr)
        return 1
    print(f"[CURRENT] {target}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
