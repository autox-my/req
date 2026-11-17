#!/usr/bin/env python3
"""Make the README's drift replay, `docs/architecture/drift/req-replay.gif` (unit P-3; ADR 0040, ADR 0052).

    python3 tools/demo/drift_replay.py [--out DIR]
    python3 tools/demo/drift_replay.py --html-only DIR

The replay tells the README's headline sentence in three steps: a tool that flags every edit would raise the alarm three
times; req raises it once. Two panels sit side by side, the same layout in both: req's overview diagram reduced to its parts.

  left   "Flags every edit": a generic baseline that marks a part for re-check whenever a linked requirement's text changes.
         It is not a named tool. At each edit its Evidence box turns red and its alarm counter rises.
  right  "req": marks a part only as `req satisfy` judged it. The tightening frames the Evidence box in red (ADR 0052's red
         frame, read from that edit's overlay); the re-word and the loosening leave it holding.

The three steps are the three committed edits of `docs/architecture/drift/edits.json`, in order: re-word, loosen, tighten. Under
each panel is a terminal pane: on the right req's own line for the `src/bench.rs -> REQ-0010` link, read from that edit's
committed `satisfy.txt`; on the left the edit itself (statement before and after, from `edits.json`) with no verdict. **Every
word of req's output is read from those files, never typed.** The status words, the part names and the red frame's label
are read from the committed overlays (`overview-overlay.json` for the parts and the starting state, each edit's `overlay.json`
for what that edit left); what is typed here is the baseline's rule, the titles and the layout.

It writes one HTML page per state (eight: the start, then for each edit the state when it is made and the state when the
verdicts are in, and the last state held longest), screenshots each with local headless Google Chrome, and assembles the GIF
with ffmpeg (palette generation, so the text stays clean). Beside the GIF it writes `req-replay.gif.json`, the record that
binds the GIF to the SHA-256 of every file it was made from, of this script and of the GIF, with the renderer versions
(`tests/drift_replay.rs` fails when any of them changes: regenerate by hand with the command above). Python standard library
only. Neither Chrome nor ffmpeg is a repository dependency and neither runs in the gate or in CI.

Chrome is `REQ_REPLAY_CHROME`, else the macOS app (`/Applications/Google Chrome.app`), else `google-chrome` on PATH; ffmpeg is
`REQ_REPLAY_FFMPEG`, else `ffmpeg` on PATH. The script never falls back to another renderer: when either is absent it names
what is missing, writes nothing and exits 3.

Exit codes:
  0  the GIF and its record were written (or, with --html-only, the eight pages);
  1  a renderer ran and what it made is not what the spec asks for (a frame of the wrong size, a GIF over 3 MB);
  2  usage;
  3  UNVERIFIABLE, nothing was made: Chrome or ffmpeg is missing, or an input file is missing or is not what this script reads.
"""
import argparse
import difflib
import hashlib
import html
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DRIFT = "docs/architecture/drift"
OVERVIEW = "docs/architecture/overview-overlay.json"
GIF_NAME = "req-replay.gif"
RECORD_NAME = "req-replay.gif.json"
SCRIPT = "tools/demo/drift_replay.py"
EDITS_SCHEMA = "req.drift-edits/1"
RECORD_SCHEMA = "req.drift-replay/1"
MAC_CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

WIDTH, HEIGHT = 1680, 846
MAX_GIF_BYTES = 3 * 1024 * 1024
# How long each state is held, in seconds: a short hold on the start, about three seconds per step (a moment with the edit
# made, then the verdicts), a longer hold on the final frame. Under 25 s in all.
HOLD_START, HOLD_MADE, HOLD_JUDGED, HOLD_FINAL = 2.5, 0.8, 2.2, 5.0

COUNT_WORDS = {1: "once", 2: "twice", 3: "three times"}
NUMBER_WORDS = {1: "one", 2: "two", 3: "three", 4: "four", 5: "five"}


class Unverifiable(Exception):
    """Nothing can be made: a renderer or an input is missing. Exit 3, never folded into a failure."""


def die(code, message):
    print(f"drift_replay: {message}", file=sys.stderr)
    sys.exit(code)


def sha256(path):
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def read_text(relative):
    path = os.path.join(ROOT, relative)
    try:
        with open(path, encoding="utf-8") as handle:
            return handle.read()
    except OSError as error:
        raise Unverifiable(f"{relative} cannot be read: {error}")


def read_json(relative):
    try:
        return json.loads(read_text(relative))
    except ValueError as error:
        raise Unverifiable(f"{relative} is not JSON: {error}")


# --------------------------------------------------------------------------------------------------------------- the model


def load_model():
    """Everything the replay shows, read from the committed files. Returns (model, the input files it read)."""
    inputs = [f"{DRIFT}/edits.json", OVERVIEW]
    edits_doc = read_json(f"{DRIFT}/edits.json")
    if edits_doc.get("schema") != EDITS_SCHEMA:
        raise Unverifiable(f"{DRIFT}/edits.json is not `{EDITS_SCHEMA}`")
    edits = edits_doc.get("edits") or []
    if not edits:
        raise Unverifiable(f"{DRIFT}/edits.json holds no edit")
    if len({(e["claim_id"], e["before"], e["expect"]["element"]) for e in edits}) != 1:
        raise Unverifiable(
            f"{DRIFT}/edits.json: the edits do not change one claim from one statement, so there is no single starting state"
        )
    claim, element = edits[0]["claim_id"], edits[0]["expect"]["element"]

    overview = read_json(OVERVIEW)
    parts = [
        {
            "id": c["id"],
            "type": c["type"],
            "label": c["label"],
            "sublabel": c.get("sublabel", ""),
            "files": [s["path"] for s in c.get("sources", [])],
        }
        for c in overview["components"]
    ]
    carriers = [p for p in parts if element in p["files"]]
    if len(carriers) != 1:
        raise Unverifiable(f"{OVERVIEW}: {len(carriers)} parts cite {element}, expected exactly one (the part that carries {claim})")
    evidence = carriers[0]
    start_word = evidence["sublabel"].split(" · ")[0]

    steps = []
    for edit in edits:
        edit_id = edit["id"]
        satisfy = f"{DRIFT}/{edit_id}/satisfy.txt"
        overlay_path = f"{DRIFT}/{edit_id}/overlay.json"
        inputs += [satisfy, overlay_path]
        needle = f"] {element} -> {claim}"
        lines = [l for l in read_text(satisfy).splitlines() if needle in l]
        if len(lines) != 1:
            raise Unverifiable(f"{satisfy}: {len(lines)} lines for `{element} -> {claim}`, expected exactly one")
        line = lines[0]
        word = line[1 : line.index("]")]
        overlay = read_json(overlay_path)
        shown = [c for c in overlay["components"] if c["id"] == evidence["id"]]
        if len(shown) != 1:
            raise Unverifiable(f"{overlay_path}: no `{evidence['id']}` component")
        frames = [
            b
            for b in overlay.get("boundaries", [])
            if b.get("kind") == "security-group" and evidence["id"] in b.get("wraps", [])
        ]
        steps.append(
            {
                "id": edit_id,
                "kind": edit["kind"],
                "before": edit["before"],
                "after": edit["after"],
                "line": line,
                "word": word,
                "sublabel": shown[0]["sublabel"],
                "frame": frames[0]["label"] if frames else None,
                # The baseline marks a part whenever the text changed; req alarms when it did not hold.
                "baseline_alarms": edit["before"] != edit["after"],
                "req_alarms": not word.startswith("HOLDS"),
            }
        )
    model = {
        "claim": claim,
        "element": element,
        "parts": parts,
        "evidence": evidence,
        "start_word": start_word,
        "start_statement": edits[0]["before"],
        "steps": steps,
    }
    return model, inputs


def build_states(model):
    """The eight states, in order: the start, two per edit (made, judged), and the last one held longest."""
    steps = model["steps"]
    states = [
        dict(
            phase="start",
            k=0,
            seconds=HOLD_START,
            baseline_alarms=0,
            req_alarms=0,
            baseline_red=False,
            word=model["start_word"],
            sublabel=model["evidence"]["sublabel"],
            frame=None,
            line=None,
            step=None,
        )
    ]
    baseline = req = 0
    for k, step in enumerate(steps, start=1):
        states.append(
            dict(
                states[0],
                phase="made",
                k=k,
                seconds=HOLD_MADE,
                baseline_alarms=baseline,
                req_alarms=req,
                step=step,
            )
        )
        baseline += 1 if step["baseline_alarms"] else 0
        req += 1 if step["req_alarms"] else 0
        states.append(
            dict(
                phase="judged",
                k=k,
                seconds=HOLD_JUDGED,
                baseline_alarms=baseline,
                req_alarms=req,
                baseline_red=step["baseline_alarms"],
                word=step["word"],
                sublabel=step["sublabel"],
                frame=step["frame"],
                line=step["line"],
                step=step,
            )
        )
    states.append(dict(states[-1], phase="final", seconds=HOLD_FINAL))
    return states


# ---------------------------------------------------------------------------------------------------------------- the page

CSS = """
:root { --bg:#fafaf9; --fg:#1c1917; --muted:#57534e; --card:#ffffff; --line:#d6d3d1; --accent:#1d4ed8; --red:#b91c1c;
        --frame:#e11d48; --frame-tint:#ffe4e6; --frame-fg:#be123c; --code:#f5f5f4; }
* { box-sizing: border-box; }
html, body { margin:0; padding:0; }
body { width:%(w)dpx; height:%(h)dpx; overflow:hidden; background:var(--bg); color:var(--fg);
       font:16px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif; padding:24px; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.strip { height:56px; display:flex; align-items:center; font-size:29px; font-weight:600; letter-spacing:-.01em; padding:0 4px; }
.strip.tally { color:var(--red); }
.strip b { font-weight:800; }
.panels { display:grid; grid-template-columns:804px 804px; gap:24px; margin-top:12px; }
.panel { background:var(--card); border:2px solid var(--line); border-radius:12px; padding:16px 20px 18px; height:712px; }
.panel .head { height:108px; }
.panel .row { display:flex; align-items:center; justify-content:space-between; gap:12px; }
.panel h2 { margin:0; font-size:34px; line-height:1.15; }
.panel .sub { color:var(--muted); font-size:19px; line-height:1.3; margin-top:2px; max-width:520px; flex:1 1 auto; min-width:0; }
.edit-count { font-size:27px; font-weight:700; white-space:nowrap; }
.edit-count span { color:var(--muted); font-weight:600; }
.chips { flex:none; display:flex; gap:10px; align-items:center; justify-content:flex-end; }
.chip { display:inline-block; font-weight:800; font-size:27px; border:3px solid var(--fg); border-radius:8px; padding:1px 12px; white-space:nowrap; }
.chip[data-verdict="SUSPECT"] { border-color:var(--red); color:var(--red); }
.chip[data-verdict^="HOLDS"] { border-color:#15803d; color:#15803d; }
.alarms { font-size:27px; font-weight:800; border:3px solid var(--line); color:var(--muted); border-radius:999px; padding:1px 16px; white-space:nowrap; }
.alarms.raised { background:var(--red); border-color:var(--red); color:#fff; }
.diagram { display:grid; grid-template-columns:repeat(4, 182px); grid-auto-rows:96px; gap:30px 12px; margin:8px 0 18px; height:348px; }
.cell { position:relative; }
.box { height:96px; border-width:3px; border-style:solid; border-radius:10px; display:flex; flex-direction:column; align-items:center;
       justify-content:center; text-align:center; padding:0 6px; }
.box .name { font-size:22px; font-weight:700; line-height:1.15; }
.box .state { font-size:15px; color:var(--muted); margin-top:3px; line-height:1.2; }
.box.evidence .state { font-size:16px; color:var(--fg); }
.box.evidence .state b { font-size:17px; }
.t-frontend { background:#d9f4f8; border-color:#0891b2; }
.t-backend { background:#d7f3e6; border-color:#059669; }
.t-database { background:#ebe3fc; border-color:#7c3aed; }
.t-security { background:#fde6ea; border-color:#e11d48; }
.t-external { background:#e7e9ee; border-color:#6b7280; }
.box.alarmed { background:#dc2626; border-color:#7f1d1d; color:#fff; }
.box.alarmed .state { color:#fff; font-weight:700; }
.claim { position:absolute; bottom:-14px; right:8px; z-index:2; background:var(--fg); color:#fff; font-size:15px; font-weight:700; border-radius:6px; padding:1px 8px; }
.frame { position:absolute; inset:-9px; border:3px dashed var(--frame); border-radius:12px; pointer-events:none; }
.frame .tag { position:absolute; top:-17px; left:10px; background:var(--frame-tint); color:var(--frame-fg); border:2px solid var(--frame);
              border-radius:6px; font-size:15px; font-weight:700; padding:0 8px; line-height:1.35; white-space:nowrap; }
.term { background:var(--fg); color:var(--code); border-radius:8px; padding:12px 18px; height:204px; font-size:21px; line-height:1.45;
        white-space:pre-wrap; overflow-wrap:anywhere; }
.term .dim { color:#a8a29e; }
.term .cmd { color:#fff; font-weight:700; }
.term .del { color:#fca5a5; }
.term .add { color:#86efac; }
.term mark.del { background:rgba(248,113,113,.38); color:#fff; border-radius:3px; }
.term mark.add { background:rgba(74,222,128,.34); color:#fff; border-radius:3px; }
.term .HOLDS { color:#86efac; font-weight:700; }
.term .HOLDS-weakened { color:#fcd34d; font-weight:700; }
.term .SUSPECT { color:#fca5a5; font-weight:700; }
.term .cursor { display:inline-block; width:.6em; background:#e7e5e4; height:1.1em; vertical-align:text-bottom; }
""" % {"w": WIDTH - 48, "h": HEIGHT - 48}


def esc(text):
    return html.escape(text, quote=True)


def words(text, other, cls):
    """`text` with the words that differ from `other` marked: a word-level diff, derived from the two statements."""
    a, b = text.split(" "), other.split(" ")
    out = []
    for tag, i1, i2, _j1, _j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        chunk = esc(" ".join(a[i1:i2]))
        if not chunk:
            continue
        out.append(chunk if tag == "equal" else f'<mark class="{cls}">{chunk}</mark>')
    return " ".join(out)


def verdict_class(word):
    return word.replace(" ", "-").replace("(", "").replace(")", "")


def title_strip(model, states_all, state):
    count = len(model["steps"])
    if state["phase"] == "final":
        baseline = sum(1 for s in model["steps"] if s["baseline_alarms"])
        req = sum(1 for s in model["steps"] if s["req_alarms"])
        return (
            '<div class="strip tally"><span>'
            f"{NUMBER_WORDS[count].capitalize()} edits: the baseline alarmed <b>{COUNT_WORDS[baseline]}</b>, req alarmed <b>{COUNT_WORDS[req]}</b>."
            "</span></div>"
        )
    return (
        '<div class="strip"><span>'
        f"One requirement, {esc(model['claim'])}, edited {NUMBER_WORDS[count]} ways, each on a copy of req&rsquo;s own corpus"
        "</span></div>"
    )


def diagram(model, state, side):
    cells = []
    for part in model["parts"]:
        is_evidence = part["id"] == model["evidence"]["id"]
        count = part["sublabel"].split(" · ", 1)[1] if " · " in part["sublabel"] else part["sublabel"]
        classes = ["box", f"t-{part['type']}"]
        frame = ""
        claim = ""
        if side == "baseline":
            # The baseline knows only that a linked requirement's text changed: it carries no verdict word.
            shown = esc(count)
            if is_evidence:
                shown = esc(count)
                if state["baseline_red"]:
                    classes.append("alarmed")
                    shown = f"<b>re-check</b><br>{esc(count)}"
        else:
            shown = esc(part["sublabel"])
            if is_evidence:
                word, _, rest = state["sublabel"].partition(" · ")
                shown = f"<b>{esc(word)}</b><br>{esc(rest)}"
                if state["frame"]:
                    frame = f'<div class="frame"><span class="tag mono">{esc(state["frame"])}</span></div>'
        if is_evidence:
            classes.append("evidence")
            claim = f'<span class="claim mono">{esc(model["claim"])}</span>'
        cells.append(
            f'<div class="cell">{frame}<div class="{" ".join(classes)}" data-part="{esc(part["id"])}">{claim}'
            f'<div class="name">{esc(part["label"])}</div><div class="state mono">{shown}</div></div></div>'
        )
    return f'<div class="diagram">{"".join(cells)}</div>'


def left_terminal(model, state):
    step = state["step"]
    if step is None:
        return (
            f'<span class="dim">{esc(model["claim"])}, as committed</span>\n'
            f'<span>{esc(model["start_statement"])}</span>'
        )
    return (
        f'<span class="dim">claims/{esc(model["claim"])}.yml · {esc(step["kind"])}</span>\n'
        f'<span class="del">- {words(step["before"], step["after"], "del")}</span>\n'
        f'<span class="add">+ {words(step["after"], step["before"], "add")}</span>'
    )


def right_terminal(state):
    if state["phase"] == "start":
        return '<span class="dim">$</span> <span class="cursor"></span>'
    head = '<span class="dim">$</span> <span class="cmd">req satisfy</span>'
    if state["line"] is None:
        return f'{head} <span class="cursor"></span>'
    line = state["line"]
    word = line[1 : line.index("]")]
    rest = line[line.index("]") + 1 :]
    return f'{head}\n<span class="{verdict_class(word)}">[{esc(word)}]</span>{esc(rest)}'


def panel(model, state, side):
    count = len(model["steps"])
    step = state["step"]
    alarms = state["baseline_alarms"] if side == "baseline" else state["req_alarms"]
    kind = f' <span>&middot; {esc(step["kind"])}</span>' if step else ""
    title, sub = (
        ("Flags every edit", "a generic baseline: marks a part for re-check whenever a linked requirement&rsquo;s text changes")
        if side == "baseline"
        else ("req", "marks a part only as <span class=\"mono\">req satisfy</span> judged it")
    )
    chip = ""
    if side == "req":
        chip = f'<span class="chip mono" data-verdict="{esc(state["word"])}">{esc(state["word"])}</span>'
    raised = " raised" if alarms else ""
    terminal = left_terminal(model, state) if side == "baseline" else right_terminal(state)
    return (
        f'<section class="panel" data-side="{side}" data-alarms="{alarms}">'
        f'<div class="head"><div class="row"><h2>{title}</h2><div class="edit-count">Edit {state["k"]} of {count}{kind}</div></div>'
        f'<div class="row"><div class="sub">{sub}</div><div class="chips">{chip}'
        f'<span class="alarms{raised}">alarms &times;{alarms}</span></div></div></div>'
        f"{diagram(model, state, side)}"
        f'<div class="term mono">{terminal}</div></section>'
    )


def page(model, states, state):
    return (
        '<!doctype html><html lang="en"><head><meta charset="utf-8">'
        "<title>req drift replay</title>"
        f"<style>{CSS}</style></head><body>"
        f'{title_strip(model, states, state)}<div class="panels">{panel(model, state, "baseline")}{panel(model, state, "req")}</div>'
        "</body></html>\n"
    )


# ------------------------------------------------------------------------------------------------------------- renderers


def find_chrome():
    override = os.environ.get("REQ_REPLAY_CHROME")
    if override:
        return override if os.path.isfile(override) and os.access(override, os.X_OK) else None
    if os.path.isfile(MAC_CHROME):
        return MAC_CHROME
    return shutil.which("google-chrome") or shutil.which("google-chrome-stable")


def find_ffmpeg():
    override = os.environ.get("REQ_REPLAY_FFMPEG")
    if override:
        return override if os.path.isfile(override) and os.access(override, os.X_OK) else None
    return shutil.which("ffmpeg")


def require_renderers():
    chrome, ffmpeg = find_chrome(), find_ffmpeg()
    missing = []
    if not chrome:
        named = os.environ.get("REQ_REPLAY_CHROME") or f"Google Chrome (looked for {MAC_CHROME}, then google-chrome on PATH)"
        missing.append(f"headless Google Chrome is missing: {named}")
    if not ffmpeg:
        named = os.environ.get("REQ_REPLAY_FFMPEG") or "ffmpeg on PATH"
        missing.append(f"ffmpeg is missing: {named}")
    if missing:
        raise Unverifiable("; ".join(missing) + ". Nothing was rendered, and no other renderer is tried.")
    return chrome, ffmpeg


def tool_version(command, flag):
    out = subprocess.run([command, flag], capture_output=True, text=True, check=False).stdout.strip()
    return out.splitlines()[0] if out else ""


def png_size(path):
    with open(path, "rb") as handle:
        head = handle.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
        return None
    return struct.unpack(">II", head[16:24])


def gif_frames(path):
    """(width, height, the number of frames) read from the GIF's own blocks."""
    with open(path, "rb") as handle:
        data = handle.read()
    width, height = struct.unpack("<HH", data[6:10])
    at = 13 + (3 * (1 << ((data[10] & 7) + 1)) if data[10] & 0x80 else 0)

    def skip(pos):
        while True:
            n = data[pos]
            pos += 1
            if n == 0:
                return pos
            pos += n

    frames = 0
    while data[at] != 0x3B:
        if data[at] == 0x21:
            at = skip(at + 2)
        elif data[at] == 0x2C:
            local = data[at + 9]
            pos = at + 10 + (3 * (1 << ((local & 7) + 1)) if local & 0x80 else 0)
            at = skip(pos + 1)
            frames += 1
        else:
            raise ValueError(f"unexpected block 0x{data[at]:02x}")
    return width, height, frames


def screenshot(chrome, html_path, png_path, profile):
    command = [
        chrome,
        "--headless=new",
        "--disable-gpu",
        "--hide-scrollbars",
        "--no-first-run",
        "--disable-extensions",
        "--disable-background-networking",
        "--force-device-scale-factor=1",
        f"--user-data-dir={profile}",
        f"--window-size={WIDTH},{HEIGHT}",
        f"--screenshot={png_path}",
        "file://" + html_path,
    ]
    done = subprocess.run(command, capture_output=True, text=True, check=False, timeout=120)
    if not os.path.isfile(png_path):
        raise Unverifiable(f"Chrome made no screenshot of {html_path} (exit {done.returncode}): {done.stderr.strip()[-400:]}")


def assemble(ffmpeg, work, files, seconds, gif_path):
    listing = os.path.join(work, "frames.txt")
    with open(listing, "w", encoding="utf-8") as handle:
        for name, hold in zip(files, seconds):
            handle.write(f"file '{name}'\nduration {hold}\n")
    graph = (
        "split[a][b];[a]palettegen=stats_mode=full:max_colors=128[p];"
        "[b][p]paletteuse=dither=none:diff_mode=rectangle"
    )
    # A GIF frame's delay is the gap to the next frame, so the last hold is given to the muxer, in centiseconds.
    final = str(round(seconds[-1] * 100))
    command = [ffmpeg, "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", listing, "-vf", graph, "-fps_mode", "vfr", "-final_delay", final, "-loop", "0", gif_path]
    done = subprocess.run(command, capture_output=True, text=True, check=False, timeout=300)
    if done.returncode != 0 or not os.path.isfile(gif_path):
        raise Unverifiable(f"ffmpeg did not make the GIF (exit {done.returncode}): {done.stderr.strip()[-400:]}")


# -------------------------------------------------------------------------------------------------------------------- main


def write_pages(model, states, directory):
    os.makedirs(directory, exist_ok=True)
    paths = []
    for n, state in enumerate(states):
        path = os.path.join(directory, f"frame-{n}.html")
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(page(model, states, state))
        paths.append(path)
    return paths


def main(argv):
    parser = argparse.ArgumentParser(prog="drift_replay.py", description="Make the README's drift replay GIF and its record.")
    parser.add_argument("--out", help=f"write {GIF_NAME} and {RECORD_NAME} here (default: {DRIFT}/)")
    parser.add_argument("--html-only", metavar="DIR", help="write the eight HTML pages to DIR and stop (needs neither Chrome nor ffmpeg)")
    args = parser.parse_args(argv)
    if args.out and args.html_only:
        parser.error("--out and --html-only are two different jobs")

    try:
        if args.html_only:
            model, _ = load_model()
            states = build_states(model)
            paths = write_pages(model, states, os.path.abspath(args.html_only))
            print(f"drift_replay: wrote {len(paths)} pages to {args.html_only}")
            return 0

        chrome, ffmpeg = require_renderers()
        model, inputs = load_model()
        states = build_states(model)
        out_dir = os.path.abspath(args.out) if args.out else os.path.join(ROOT, DRIFT)
        os.makedirs(out_dir, exist_ok=True)

        with tempfile.TemporaryDirectory(prefix="req-drift-replay-") as work:
            pages = write_pages(model, states, work)
            pngs = []
            for n, path in enumerate(pages):
                png = os.path.join(work, f"frame-{n}.png")
                screenshot(chrome, path, png, os.path.join(work, "profile"))
                if png_size(png) != (WIDTH, HEIGHT):
                    print(f"drift_replay: frame {n} is {png_size(png)}, not {(WIDTH, HEIGHT)}", file=sys.stderr)
                    return 1
                pngs.append(os.path.basename(png))
            gif_tmp = os.path.join(work, GIF_NAME)
            assemble(ffmpeg, work, pngs, [s["seconds"] for s in states], gif_tmp)
            size = os.path.getsize(gif_tmp)
            width, height, frames = gif_frames(gif_tmp)
            if size >= MAX_GIF_BYTES or (width, height) != (WIDTH, HEIGHT) or frames != len(states):
                print(
                    f"drift_replay: the GIF is {size} bytes, {width}x{height}, {frames} frames; the spec asks for under {MAX_GIF_BYTES} bytes, "
                    f"{WIDTH}x{HEIGHT}, {len(states)} frames",
                    file=sys.stderr,
                )
                return 1
            gif_path = os.path.join(out_dir, GIF_NAME)
            shutil.copyfile(gif_tmp, gif_path)

        record = {
            "schema": RECORD_SCHEMA,
            "gif": f"{DRIFT}/{GIF_NAME}",
            "gif_sha256": sha256(gif_path),
            "inputs": {path: sha256(os.path.join(ROOT, path)) for path in sorted(inputs)},
            "script": SCRIPT,
            "script_sha256": sha256(os.path.join(ROOT, SCRIPT)),
            "chrome": tool_version(chrome, "--version"),
            "ffmpeg": " ".join(tool_version(ffmpeg, "-version").split()[:3]),
            "frame_size": [WIDTH, HEIGHT],
            "frame_count": frames,
            "recipe": "python3 tools/demo/drift_replay.py (one HTML page per state, screenshotted by headless Chrome at "
            f"{WIDTH}x{HEIGHT}, assembled by ffmpeg concat + palettegen/paletteuse)",
        }
        record_path = os.path.join(out_dir, RECORD_NAME)
        with open(record_path, "w", encoding="utf-8") as handle:
            json.dump(record, handle, indent=2)
            handle.write("\n")
        print(f"drift_replay: wrote {gif_path} ({size} bytes, {frames} frames) and {record_path}")
        return 0
    except Unverifiable as error:
        print(f"drift_replay: UNVERIFIABLE: {error}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
