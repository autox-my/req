#!/usr/bin/env python3
"""Build, or check, docs/drift.html: the guided page that shows what drift does (unit P-2; ADR 0040, 0052, 0054).

The page is GENERATED, never hand-edited. It is the fixed template `tools/demo/drift.html` (hand-written HTML, CSS and script:
the same bytes for every corpus) with ONE data document inserted, in `<script type="application/json" id="req-drift-data">`.
It tells one story three ways: a real claim (REQ-0010, linked from `src/bench.rs`, drawn in the overview's Evidence box) is
re-worded, loosened and tightened, each on a throw-away copy (O-6), and the page shows what `req` says about each. The document
is computed from committed output and one `req compare` run, and from nothing else:

    docs/architecture/drift/edits.json     the three edits: statement before and after, the outcome each must give (O-6)
    docs/architecture/drift/<edit>/satisfy.txt
                                           the exact `req satisfy` text for that edit; its line for the edited link is shown
    docs/architecture/drift/<edit>/overlay.json and docs/architecture/overview-overlay.json
                                           the Evidence box's status line and red frame, and whether a diagram changed at all
    docs/architecture/drift/req-*.png and their `.png.json` records
                                           the three pictures, each bound to the overlay it was rendered from (a record names
                                           the overlay and holds the sha-256 of its bytes and of the picture's)
    req compare                            run on the claim and its edited copy, in a TEMPORARY COPY of the corpus, for the
                                           direction word and the two meaning fingerprints
    claims/REQ-0010.yml                    read, never written: the statement every edit starts from

A word on the page is a word `req` printed; a claim the page makes about the diagram (that it did not change, that every part
holds) is computed from the overlays and carried as a flag, and the template says it only when the flag is set. Nothing is chosen
by a model (ADR 0004). The output has no timestamp and every key is sorted, so two builds over one repository are the same bytes.
This script never opens this checkout's claims for writing: the edit is applied to a copy, which is deleted.

    python3 tools/demo/drift_page.py [--req <path>] --out <page>       write the page
    python3 tools/demo/drift_page.py [--req <path>] --check <page>     exit 0 current, 1 stale

The corpus is the repository this script lives in (its parent's parent's parent), so a copy of the repository checks itself.
Python standard library only, offline. `req` is taken from `--req`, else PATH.

Exit: 0 written / current; 1 stale (the page is named; or a picture no longer matches the overlay it was rendered from, or its
record: the picture is named, render it again); 2 usage; 3 UNVERIFIABLE, nothing was compared or written: `req` cannot be
run or did not answer as this build reads, a committed output disagrees with the outcome its edit expects (the edit is named; fix
the demonstration with `tools/graph/build_architecture.py --drift`, not the page), a picture is missing, or the page cannot be written.
"""
import difflib
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TEMPLATE = os.path.join(ROOT, "tools", "demo", "drift.html")
TOKEN = "__REQ_DRIFT_DATA__"
SCHEMA = "req.drift-page/1"
EDITS_SCHEMA = "req.drift-edits/1"
DRIFT_DIR = os.path.join(ROOT, "docs", "architecture", "drift")
OVERVIEW_OVERLAY = os.path.join(ROOT, "docs", "architecture", "overview-overlay.json")
DOCS = os.path.join(ROOT, "docs")
GITHUB_ADR = "https://github.com/autox-my/req/blob/main/docs/adr/"
# The picture of the diagram before any edit, and one picture per edit that changes it (relative to docs/, where Pages serves
# them). An edit with no entry changes nothing the diagram shows: the page says so instead of drawing it twice. The pictures are
# rendered the same way, on one canvas, so they have one pixel size and the diagram sits in the same place in each (P-2-fix);
# the build refuses a set that does not.
BEFORE_IMAGE = "architecture/drift/req-before.png"
IMAGES = {
    "cosmetic": None,
    "loosening": "architecture/drift/req-loosening.png",
    "tightening": "architecture/drift/req-tightening.png",
}
# `req satisfy`'s outcome label -> the word the page keys its sentences and classes by. The page tells three stories, so any
# other outcome is not one it can tell.
OUTCOMES = {"HOLDS": "holds", "HOLDS (weakened)": "holds_weakened", "SUSPECT": "suspect"}
SATISFY_LINE = re.compile(r"^\[(?P<outcome>[^\]]+)\] (?P<element>\S+) -> (?P<claim>[^\s:]+)(?P<rest>:.*)?$")
COMPARE_LINE = re.compile(r"^(?P<a>\S+) -> (?P<b>\S+): (?P<direction>\S+)")
FINGERPRINTS = re.compile(r"^\s+(?P<before>[0-9a-f]+) -> (?P<after>[0-9a-f]+)$")
PNG = b"\x89PNG\r\n\x1a\n"
PICTURE_SCHEMA = "req.drift-picture/1"
BEFORE_OVERLAY = "docs/architecture/overview-overlay.json"


class Stale(Exception):
    """A picture no longer matches the overlay it was rendered from, or its own record: render it again (exit 1)."""


class Unverifiable(Exception):
    """A committed output cannot be read as this build reads it, or `req` cannot answer: nothing is compared or written."""


def read_text(path, what):
    try:
        with open(path, encoding="utf-8", newline="") as handle:
            return handle.read()
    except (OSError, UnicodeDecodeError) as why:
        raise Unverifiable(f"{what} {os.path.relpath(path, ROOT)} cannot be read: {why}")


def read_json(path, what):
    try:
        return json.loads(read_text(path, what))
    except ValueError as why:
        raise Unverifiable(f"{what} {os.path.relpath(path, ROOT)} is not JSON: {why}")


def run_req(binary, cwd, *args):
    """`req --root <cwd> <args>` run in `cwd`: (exit code, stdout, stderr)."""
    env = {k: v for k, v in os.environ.items() if k != "REQ_ROOT"}
    try:
        done = subprocess.run([binary, "--root", cwd, *args], capture_output=True, text=True, check=False, cwd=cwd,
                              stdin=subprocess.DEVNULL, timeout=300, env=env)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`req {' '.join(args)}` could not be run: {why}")
    return done.returncode, done.stdout, done.stderr


def rules_version(binary):
    """`rules_v` as `req --version` prints it: the rules the committed outputs were made under."""
    try:
        done = subprocess.run([binary, "--version"], capture_output=True, text=True, check=False,
                              stdin=subprocess.DEVNULL, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as why:
        raise Unverifiable(f"`req --version` could not be run: {why}")
    found = re.search(r"^\s*rules_v\s+(\d+)\s*$", done.stdout, re.MULTILINE)
    if found is None:
        raise Unverifiable("`req --version` printed no `rules_v` line")
    return int(found.group(1))


def folded_statement(claim_text):
    """The claim's folded `statement: >-` block as one line, or None when it has none this build can read."""
    lines = claim_text.split("\n")
    starts = [i for i, line in enumerate(lines) if line == "statement: >-"]
    if len(starts) != 1:
        return None, None, None
    end = starts[0] + 1
    while end < len(lines) and lines[end].startswith("  "):
        end += 1
    return " ".join(line.strip() for line in lines[starts[0] + 1:end]), lines, (starts[0] + 1, end)


def with_statement(claim_text, statement):
    """The claim file's text with its statement replaced: used on a copy only."""
    _, lines, (first, end) = folded_statement(claim_text)
    wrapped = textwrap.wrap(statement, width=78, initial_indent="  ", subsequent_indent="  ",
                            break_long_words=False, break_on_hyphens=False)
    return "\n".join(lines[:first] + wrapped + lines[end:])


def load_edits():
    document = read_json(os.path.join(DRIFT_DIR, "edits.json"), "the edits")
    if not isinstance(document, dict) or document.get("schema") != EDITS_SCHEMA or not isinstance(document.get("edits"), list):
        raise Unverifiable(f"edits.json is not a {EDITS_SCHEMA} document")
    edits = document["edits"]
    if [e.get("id") for e in edits if isinstance(e, dict)] != list(IMAGES):
        raise Unverifiable("edits.json must hold exactly the edits this page tells, in order: " + ", ".join(IMAGES))
    for e in edits:
        for key in ("id", "kind", "claim_id", "before", "after", "expect"):
            if key not in e:
                raise Unverifiable(f"edit {e.get('id')!r} has no `{key}`")
        expect = e["expect"]
        if not isinstance(expect, dict) or expect.get("element") is None or expect.get("outcome") not in OUTCOMES.values():
            raise Unverifiable(f"edit {e['id']!r}: `expect` must name an element and one of {sorted(OUTCOMES.values())}")
    claims = {e["claim_id"] for e in edits}
    elements = {e["expect"]["element"] for e in edits}
    if len(claims) != 1 or len(elements) != 1:
        raise Unverifiable("the page tells one claim and one design element; edits.json names several")
    return edits


def satisfy_line(edit, element, claim_id):
    """The one line of the edit's committed `satisfy.txt` for the edited link, and the outcome word it carries."""
    path = os.path.join(DRIFT_DIR, edit["id"], "satisfy.txt")
    found = []
    for line in read_text(path, "the satisfy output").split("\n"):
        match = SATISFY_LINE.match(line)
        if match and match.group("element") == element and match.group("claim") == claim_id:
            found.append((line, match))
    if len(found) != 1:
        raise Unverifiable(f"edit {edit['id']!r}: {os.path.relpath(path, ROOT)} has {len(found)} lines for {element} -> "
                           f"{claim_id}, not one")
    line, match = found[0]
    label = match.group("outcome")
    if label not in OUTCOMES:
        raise Unverifiable(f"edit {edit['id']!r}: req satisfy printed [{label}], which is not one of the three outcomes this "
                           "page tells")
    word = OUTCOMES[label]
    if word != edit["expect"]["outcome"]:
        raise Unverifiable(f"edit {edit['id']!r}: edits.json expects {edit['expect']['outcome']!r} but the committed "
                           f"satisfy.txt says [{label}]; regenerate the demonstration (tools/graph/build_architecture.py "
                           "--drift) before the page")
    direction = edit["expect"].get("direction")
    if direction is not None and direction not in (match.group("rest") or ""):
        raise Unverifiable(f"edit {edit['id']!r}: the committed line does not name the direction {direction!r}: {line}")
    return line, label, word


def overlay_view(path, element):
    """(statuses by component id, red frames by component id, the component citing `element`) of an archify overlay."""
    overlay = read_json(path, "the overlay")
    try:
        components = overlay["components"]
        statuses = {c["id"]: c["sublabel"] for c in components}
        frames = {}
        for boundary in overlay.get("boundaries", []):
            for wrapped in boundary.get("wraps", []):
                frames[wrapped] = boundary["label"]
        cited = [c for c in components if any(s.get("path") == element for s in c.get("sources", []))]
    except (KeyError, TypeError, AttributeError) as why:
        raise Unverifiable(f"{os.path.relpath(path, ROOT)} is not an architecture overlay this build reads: {why}")
    if not cited:
        raise Unverifiable(f"{os.path.relpath(path, ROOT)}: no component cites {element}")
    return statuses, frames, {"id": cited[0]["id"], "label": cited[0]["label"]}


def image_check(relative):
    """The picture's path and its pixel size (read from the PNG header), or Unverifiable."""
    path = os.path.join(DOCS, relative)
    try:
        with open(path, "rb") as handle:
            head = handle.read(24)
    except OSError as why:
        raise Unverifiable(f"the picture docs/{relative} cannot be read: {why}")
    if head[:8] != PNG or len(head) < 24 or head[12:16] != b"IHDR":
        raise Unverifiable(f"the picture docs/{relative} is not a PNG")
    return relative, (int.from_bytes(head[16:20], "big"), int.from_bytes(head[20:24], "big"))


def sha256_file(path):
    try:
        with open(path, "rb") as handle:
            return hashlib.sha256(handle.read()).hexdigest()
    except OSError as why:
        raise Unverifiable(f"{os.path.relpath(path, ROOT)} cannot be read: {why}")


def picture_binding(picture, overlay):
    """Bind `picture` (a path under docs/) to the overlay it must have been rendered from (a path under the repository).

    A sidecar `<picture>.json` records the overlay it was rendered from, the sha-256 of that overlay's exact bytes and of the
    picture's. A sidecar that names another overlay is refused (a picture swapped for another edit's, with its record);
    a picture or an overlay whose bytes are not the recorded ones is STALE (an overlay changed and the picture was not rendered
    again, or a picture was swapped without its record). Nothing here judges how the picture was made."""
    named = f"docs/{picture}"
    record = read_json(os.path.join(DOCS, picture + ".json"), "the picture's record")
    if not isinstance(record, dict) or record.get("schema") != PICTURE_SCHEMA:
        raise Unverifiable(f"{named}.json is not a {PICTURE_SCHEMA} record")
    for key in ("overlay", "overlay_sha256", "png_sha256"):
        if not isinstance(record.get(key), str):
            raise Unverifiable(f"{named}.json has no `{key}`")
    if record["overlay"] != overlay:
        raise Unverifiable(f"the picture {named} is recorded as rendered from {record['overlay']}, but its edit's overlay is "
                           f"{overlay}: it belongs to another edit")
    if sha256_file(os.path.join(DOCS, picture)) != record["png_sha256"]:
        raise Stale(f"{named} is not the picture recorded in {named}.json (its bytes differ): render it again from {overlay} "
                    "and record it")
    if sha256_file(os.path.join(ROOT, overlay)) != record["overlay_sha256"]:
        raise Stale(f"{named} was rendered from an earlier {overlay} (the overlay's bytes changed): render it again and "
                    "record it")


def claim_pins(claim_text):
    """The claim's `pins:` block as `{key: value}` (quotes dropped), `{}` when it has none. Read for the page, never edited."""
    pins, inside = {}, False
    for line in claim_text.split("\n"):
        if inside:
            found = re.match(r"^  ([A-Za-z_][\w-]*): (.*)$", line)
            if not found:
                break
            pins[found.group(1)] = found.group(2).strip().strip('"')
        elif line == "pins:":
            inside = True
    return pins


def word_diff(before, after):
    """The two statements as runs of words, `{"t": text, "c": changed}`, from their words alone."""
    a, b = before.split(), after.split()
    if " ".join(a) != before or " ".join(b) != after:
        raise Unverifiable("a statement has irregular whitespace, so its changed words cannot be marked")
    sides = ([], [])
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        if tag == "equal":
            sides[0].append({"t": " ".join(a[i1:i2]), "c": False})
            sides[1].append({"t": " ".join(b[j1:j2]), "c": False})
            continue
        if i2 > i1:
            sides[0].append({"t": " ".join(a[i1:i2]), "c": True})
        if j2 > j1:
            sides[1].append({"t": " ".join(b[j1:j2]), "c": True})
    return {"before": sides[0], "after": sides[1]}


def compare_edits(binary, edits, claim_id):
    """`req compare` on the claim and its edited copy, per edit, in ONE temporary copy of the corpus (never this checkout)."""
    results = {}
    with tempfile.TemporaryDirectory(prefix="req-drift-page-") as copy:
        for tree in ("claims", "aliases", ".req"):
            try:
                shutil.copytree(os.path.join(ROOT, tree), os.path.join(copy, tree))
            except OSError as why:
                raise Unverifiable(f"{tree} cannot be copied for `req compare`: {why}")
        claim_path = os.path.join("claims", claim_id + ".yml")
        text = read_text(os.path.join(ROOT, claim_path), "the claim")
        current, _, _ = folded_statement(text)
        if current is None:
            raise Unverifiable(f"{claim_path} has no folded `statement: >-` block this build can read")
        for edit in edits:
            if edit["before"] != current:
                raise Unverifiable(f"edit {edit['id']!r}: `before` is not the statement of {claim_id} as committed "
                                   f"(it reads {current!r})")
            name = f"after-{edit['id']}.yml"
            with open(os.path.join(copy, name), "w", encoding="utf-8", newline="\n") as handle:
                handle.write(with_statement(text, edit["after"]))
            code, out, err = run_req(binary, copy, "compare", claim_path, name)
            if code != 0:
                raise Unverifiable(f"edit {edit['id']!r}: `req compare` exited {code}: {(err or out).strip()}")
            lines = out.splitlines()
            first = COMPARE_LINE.match(lines[0]) if lines else None
            prints = [FINGERPRINTS.match(line) for line in lines[1:]]
            prints = [m for m in prints if m]
            if first is None or len(prints) != 1:
                raise Unverifiable(f"edit {edit['id']!r}: `req compare` did not print the lines this build reads: {out!r}")
            rest = [line for line in lines[1:] if line.startswith(" ") and not FINGERPRINTS.match(line)]
            results[edit["id"]] = {
                "line": lines[0],
                "output": lines,
                "detail": [line.strip() for line in rest if not line.strip().startswith("note: ")],
                "notes": [line.strip()[len("note: "):] for line in rest if line.strip().startswith("note: ")],
                "fingerprints": {"before": prints[0].group("before"), "after": prints[0].group("after")},
                "direction": first.group("direction"),
            }
    return results


def build_data(binary):
    edits = load_edits()
    claim_id = edits[0]["claim_id"]
    element = edits[0]["expect"]["element"]
    compares = compare_edits(binary, edits, claim_id)
    overview, overview_frames, component = overlay_view(OVERVIEW_OVERLAY, element)
    before_all_hold = not overview_frames and all(s == "no claim" or s.startswith("HOLDS · ") for s in overview.values())
    story = []
    before_image, before_size = image_check(BEFORE_IMAGE)
    picture_binding(before_image, BEFORE_OVERLAY)
    sizes = {before_image: before_size}
    for edit in edits:
        line, label, word = satisfy_line(edit, element, claim_id)
        compare = compares[edit["id"]]
        expected = edit["expect"].get("direction", "identical")
        if compare["direction"] != expected:
            raise Unverifiable(f"edit {edit['id']!r}: `req compare` says {compare['direction']!r}, edits.json expects "
                               f"{expected!r}")
        statuses, frames, evidence = overlay_view(os.path.join(DRIFT_DIR, edit["id"], "overlay.json"), element)
        if evidence != component:
            raise Unverifiable(f"edit {edit['id']!r}: the overlay draws {element} in {evidence}, the overview in {component}")
        status = statuses[evidence["id"]]
        if status.split(" · ", 1)[0] != label:
            raise Unverifiable(f"edit {edit['id']!r}: the overlay's status line reads {status!r}, req satisfy said [{label}]")
        frame = frames.get(evidence["id"])
        if (frame is not None) != (word == "suspect"):
            raise Unverifiable(f"edit {edit['id']!r}: the red frame ({frame!r}) does not agree with the outcome {word!r}")
        unchanged = statuses == overview and frames == overview_frames
        image = IMAGES[edit["id"]]
        if image is None and not unchanged:
            raise Unverifiable(f"edit {edit['id']!r}: the diagram changed but the page has no picture of it")
        if image is not None:
            image, size = image_check(image)
            sizes[image] = size
            picture_binding(image, f"docs/architecture/drift/{edit['id']}/overlay.json")
        story.append({
            "id": edit["id"],
            "kind": edit["kind"],
            "claim_id": claim_id,
            "element": element,
            "before": edit["before"],
            "after": edit["after"],
            "diff": word_diff(edit["before"], edit["after"]),
            "compare": {k: compare[k] for k in ("line", "detail", "notes", "fingerprints", "output")},
            "direction": compare["direction"],
            "outcome": word,
            "outcome_label": label,
            "satisfy_line": line,
            "status_line": status,
            "frame": frame,
            "framed": len(frames),
            "image": image,
            "diagram_unchanged": unchanged,
        })
    if len(set(sizes.values())) != 1:
        raise Unverifiable("the pictures must have one pixel size, so the diagram sits in the same place in each: "
                           + ", ".join(f"docs/{name} is {w}x{h}" for name, (w, h) in sorted(sizes.items())))
    adrs = sorted(name for name in os.listdir(os.path.join(DOCS, "adr")) if name.startswith("0040-") and name.endswith(".md"))
    if len(adrs) != 1:
        raise Unverifiable(f"docs/adr holds {len(adrs)} files for ADR 0040, not one")
    for relative in ("explorer.html", "architecture.html", "architecture-drift.html"):
        if not os.path.isfile(os.path.join(DOCS, relative)):
            raise Unverifiable(f"docs/{relative} does not exist, so the page cannot link it")
    return {
        "schema": SCHEMA,
        "rules_v": rules_version(binary),
        "claim": {"id": claim_id, "element": element, "component": component,
                  "pins": claim_pins(read_text(os.path.join(ROOT, "claims", claim_id + ".yml"), "the claim"))},
        "before": {
            "image": before_image,
            "status_line": overview[component["id"]],
            "all_hold": before_all_hold,
            "framed": len(overview_frames),
        },
        "edits": story,
        "links": [
            {"label": f"The coverage explorer, focused on {claim_id}", "href": f"explorer.html#focus={claim_id}"},
            {"label": "req's own architecture, as it really is", "href": "architecture.html"},
            {"label": "The same architecture with the claim tightened", "href": "architecture-drift.html"},
            {"label": "ADR 0040: a change is judged by its direction", "href": GITHUB_ADR + adrs[0]},
        ],
    }


def render(binary):
    """The page's text, or raises Unverifiable."""
    template = read_text(TEMPLATE, "the template")
    if template.count(TOKEN) != 1:
        raise Unverifiable(f"{TEMPLATE} must name the data's place ({TOKEN}) exactly once")
    document = json.dumps(build_data(binary), sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    # The element is data, never markup: `</` could close it, `<!--` could open an escaped-script state. Both stay valid JSON.
    document = document.replace("</", "<\\/").replace("<!--", "\\u003c!--")
    before, _, after = template.partition(TOKEN)
    return before + document + after


def main(argv):
    mode, target, binary = None, None, None
    args = iter(argv)
    usage = "usage: drift_page.py [--req <path>] --out <page> | --check <page>"
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
    except Stale as why:
        print(f"STALE {why}", file=sys.stderr)
        return 1
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
                                             prefix=".drift-", suffix=".tmp", delete=False) as handle:
                staged = handle.name
                handle.write(page)
            os.chmod(staged, 0o644)  # a temporary file is private; a published page is not
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
        print(f"STALE {target}\nregenerate with `python3 tools/demo/drift_page.py --out {target}` (the page is generated, "
              "never hand-edited)", file=sys.stderr)
        return 1
    print(f"[CURRENT] {target}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
