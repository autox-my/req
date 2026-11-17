#!/usr/bin/env python3
"""Make the README's drift replay, `docs/architecture/drift/req-replay.gif`, and its how-it-works picture,
`docs/architecture/drift/req-how-it-works.png` (unit P-3 and P-3-fix; ADR 0040, ADR 0052).

    python3 tools/demo/drift_replay.py [--out DIR]
    python3 tools/demo/drift_replay.py --html-only DIR

The replay tells the README's headline sentence: a tool that flags every edit would raise the alarm three times; req raises it
once. Two panels sit side by side, the same layout in both: req's overview diagram reduced to its parts.

  left   "Flags every edit": a generic baseline that marks a part for re-check whenever a linked requirement's text changes.
         It is not a named tool. At each edit its Evidence box turns red and its alarm counter rises.
  right  "req": marks a part only as `req satisfy` judged it. The tightening frames the Evidence box in red (ADR 0052's red
         frame, read from that edit's overlay); the re-word and the loosening leave it holding.

It is paced to be followed on a first viewing (owner review of PR #100): an intro that says in plain words what the reader is
about to see; then each of the three committed edits of `docs/architecture/drift/edits.json`, in order (re-word, loosen,
tighten), as three beats: (1) the edit alone, shown full width as a diff; (2) the left reaction, the right panel dimmed;
(3) the right reaction, the left panel dimmed, with req's own line for the `src/bench.rs -> REQ-0010` link in its terminal;
then a summary. A banner above the panels says in a plain sentence what each beat is. **Every word of req's output is read
from the committed files, never typed**: its line from each edit's `satisfy.txt`, the status words and part names from the
committed overlays (`overview-overlay.json` for the parts and the starting state, each edit's `overlay.json` for what that
edit left), the red frame's label from the overlay. What is typed here is the baseline's rule, the titles, the layout and the
banner's framing sentences; those make no claim the files do not carry.

The how-it-works picture is the README's five steps drawn in the same visual language (palette, type, box style, red frame),
replacing a Mermaid block whose pan-and-zoom buttons covered its right-hand boxes on GitHub. Its text is fixed wording.

It writes one HTML page per state (eleven), screenshots each with local headless Google Chrome and assembles the GIF with
ffmpeg (palette generation, so the text stays clean); it screenshots the picture the same way. Beside each it writes a record
(`req-replay.gif.json`: the SHA-256 of every file the GIF was made from, of this script and of the GIF, with the renderer
versions; `req-how-it-works.png.json`: the script and the PNG). `tests/drift_replay.rs` fails when any of them changes:
regenerate by hand with the command above. Python standard library only. Neither Chrome nor ffmpeg is a repository dependency
and neither runs in the gate or in CI.

Chrome is `REQ_REPLAY_CHROME`, else the macOS app (`/Applications/Google Chrome.app`), else `google-chrome` on PATH; ffmpeg is
`REQ_REPLAY_FFMPEG`, else `ffmpeg` on PATH. The script never falls back to another renderer: when either is absent it names
what is missing, writes nothing and exits 3.

Exit codes:
  0  the GIF, the PNG and their records were written (or, with --html-only, the pages);
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
HOW_NAME = "req-how-it-works.png"
HOW_RECORD_NAME = "req-how-it-works.png.json"
HOW_SCHEMA = "req.drift-diagram/1"
MAC_CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

WIDTH, HEIGHT = 1680, 1016
HOW_HEIGHT = 372
MAX_GIF_BYTES = 3 * 1024 * 1024
# How long each state is held, in seconds (owner review of PR #100: it was too fast to follow). An intro of about five seconds;
# each edit as three beats of 3.5 to 4 s (the edit, the left reaction, the right reaction); a summary held about six.
HOLD_INTRO, HOLD_EDIT, HOLD_LEFT, HOLD_RIGHT, HOLD_SUMMARY = 5.0, 3.8, 3.6, 4.0, 6.0

INTRO_TEXT = (
    "The same three edits to one requirement, shown to two reviewers. "
    "Left: a check that flags every text change. "
    "Right: req, which judges the direction of the change."
)
# What each beat is, in plain words. `kind` is edits.json's own word for the edit.
EDIT_CAPTIONS = {
    "re-word": "the wording changes, the meaning does not",
    "loosen": "the bound is loosened",
    "tighten": "the bound is tightened",
}
# What req's verdict word means (ADR 0040), keyed by the word `req satisfy` printed.
RIGHT_CAPTIONS = {
    "HOLDS": "req compares meaning, not wording. The meaning is the same, so the link holds and nothing is raised.",
    "HOLDS (weakened)": "req sees a looser requirement. What met the stronger one meets the weaker one, so the link holds and nothing is raised.",
    "SUSPECT": "req sees a tighter requirement. The design may no longer meet it, so the part is framed in red for a person to re-check.",
}
BEAT_LABELS = {"intro": "Before any edit", "edit": "The edit", "left": "Left reacts", "right": "Right reacts", "summary": "Summary"}

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


def changed_tokens(before, after):
    """The words that differ between two statements, as (old, new) lists: derived, never typed."""
    a_words, b_words = before.split(" "), after.split(" ")
    old, new = [], []
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a_words, b_words, autojunk=False).get_opcodes():
        if tag != "equal":
            old += a_words[i1:i2]
            new += b_words[j1:j2]
    return old, new


def edit_caption(step, k, count):
    what = EDIT_CAPTIONS.get(step["kind"], f"the statement changes ({step['kind']})")
    old, new = changed_tokens(step["before"], step["after"])
    if step["kind"] in ("loosen", "tighten") and len(old) == 1 and len(new) == 1:
        what += f", from {old[0]} to {new[0]}"
    return f"Edit {k} of {count}: {what}"


def build_states(model):
    """The eleven states, in order: the intro, three beats for each edit (the edit alone, the left reaction, the right reaction)
    and the summary, held longest."""
    steps = model["steps"]
    count = len(steps)
    base = dict(
        beat="intro",
        k=0,
        seconds=HOLD_INTRO,
        baseline_alarms=0,
        req_alarms=0,
        baseline_red=False,
        word=model["start_word"],
        sublabel=model["evidence"]["sublabel"],
        frame=None,
        line=None,
        step=None,
        dim_left=False,
        dim_right=False,
        caption=INTRO_TEXT,
    )
    states = [base]
    baseline = req = 0
    for k, step in enumerate(steps, start=1):
        states.append(
            dict(
                base,
                beat="edit",
                k=k,
                seconds=HOLD_EDIT,
                baseline_alarms=baseline,
                req_alarms=req,
                step=step,
                caption=edit_caption(step, k, count),
            )
        )
        baseline += 1 if step["baseline_alarms"] else 0
        states.append(
            dict(
                states[-1],
                beat="left",
                seconds=HOLD_LEFT,
                baseline_alarms=baseline,
                baseline_red=step["baseline_alarms"],
                dim_right=True,
                caption=f"Left: the text changed, so the check raises an alarm. Alarms so far: {baseline}.",
            )
        )
        req += 1 if step["req_alarms"] else 0
        states.append(
            dict(
                states[-1],
                beat="right",
                seconds=HOLD_RIGHT,
                req_alarms=req,
                word=step["word"],
                sublabel=step["sublabel"],
                frame=step["frame"],
                line=step["line"],
                dim_left=True,
                dim_right=False,
                caption="Right: " + RIGHT_CAPTIONS.get(step["word"], f"req says {step['word']}."),
            )
        )
    baseline_total = sum(1 for s in steps if s["baseline_alarms"])
    req_total = sum(1 for s in steps if s["req_alarms"])
    states.append(
        dict(
            states[-1],
            beat="summary",
            seconds=HOLD_SUMMARY,
            dim_left=False,
            caption=(
                f"{NUMBER_WORDS[count].capitalize()} edits: the check alarmed {COUNT_WORDS[baseline_total]}; "
                f"req alarmed {COUNT_WORDS[req_total]}."
            ),
        )
    )
    return states


# ---------------------------------------------------------------------------------------------------------------- the page

CSS = """
:root { --bg:#fafaf9; --fg:#1c1917; --muted:#57534e; --card:#ffffff; --line:#d6d3d1; --accent:#1d4ed8; --red:#b91c1c;
        --frame:#e11d48; --frame-tint:#ffe4e6; --frame-fg:#be123c; --code:#f5f5f4; --green:#15803d; }
* { box-sizing: border-box; }
html, body { margin:0; padding:0; }
body { width:%(w)dpx; height:%(h)dpx; overflow:hidden; background:var(--bg); color:var(--fg);
       font:16px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif; padding:24px; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.banner { height:108px; background:var(--card); border:2px solid var(--line); border-left-width:10px; border-radius:12px;
          display:flex; align-items:center; gap:22px; padding:0 24px; }
.banner[data-beat="intro"] { border-left-color:#78716c; }
.banner[data-beat="edit"] { border-left-color:var(--accent); }
.banner[data-beat="left"], .banner[data-beat="right"] { border-left-color:var(--fg); }
.banner[data-beat="summary"] { border-left-color:var(--red); }
.banner .pill { flex:none; width:210px; text-align:center; font-size:21px; font-weight:800; letter-spacing:.02em; text-transform:uppercase;
                border:3px solid var(--fg); border-radius:999px; padding:3px 0; }
.banner[data-beat="edit"] .pill { border-color:var(--accent); color:var(--accent); }
.banner[data-beat="intro"] .pill { border-color:#78716c; color:#57534e; }
.banner[data-beat="summary"] .pill { border-color:var(--red); color:var(--red); }
.banner .cap { font-size:31px; font-weight:650; line-height:1.25; letter-spacing:-.01em; }
.banner[data-beat="summary"] .cap { color:var(--red); }
.card { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; margin-top:12px; height:132px; background:var(--fg); color:var(--code); border:4px solid var(--fg); border-radius:10px; padding:10px 20px;
        font-size:24px; line-height:1.42; white-space:pre-wrap; overflow-wrap:anywhere; }
.card.emph { border-color:var(--accent); box-shadow:0 0 0 5px rgba(29,78,216,.35); }
.card .dim { color:#a8a29e; }
.card .del { color:#fca5a5; }
.card .add { color:#86efac; }
.card mark.del { background:rgba(248,113,113,.38); color:#fff; border-radius:3px; }
.card mark.add { background:rgba(74,222,128,.34); color:#fff; border-radius:3px; }
.card .recap { display:grid; grid-template-columns:260px 280px 1fr; gap:0 10px; }
.card .HOLDS { color:#86efac; font-weight:700; }
.card .HOLDS-weakened { color:#fcd34d; font-weight:700; }
.card .SUSPECT { color:#fca5a5; font-weight:700; }
.panels { display:grid; grid-template-columns:804px 804px; gap:24px; margin-top:12px; }
.panel { background:var(--card); border:2px solid var(--line); border-radius:12px; padding:16px 20px 18px; height:704px; transition:none; }
.panel[data-dim="true"] { opacity:.3; }
.panel .head { height:108px; }
.panel .row { display:flex; align-items:center; justify-content:space-between; gap:12px; }
.panel h2 { margin:0; font-size:34px; line-height:1.15; }
.panel .sub { color:var(--muted); font-size:19px; line-height:1.3; margin-top:2px; max-width:520px; flex:1 1 auto; min-width:0; }
.edit-count { font-size:27px; font-weight:700; white-space:nowrap; }
.edit-count span { color:var(--muted); font-weight:600; }
.chips { flex:none; display:flex; gap:10px; align-items:center; justify-content:flex-end; }
.chip { display:inline-block; font-weight:800; font-size:27px; border:3px solid var(--fg); border-radius:8px; padding:1px 12px; white-space:nowrap; }
.chip[data-verdict="SUSPECT"] { border-color:var(--red); color:var(--red); }
.chip[data-verdict^="HOLDS"] { border-color:var(--green); color:var(--green); }
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
.term { background:var(--fg); color:var(--code); border-radius:8px; padding:12px 18px; height:190px; font-size:21px; line-height:1.45;
        white-space:pre-wrap; overflow-wrap:anywhere; }
.term .dim { color:#a8a29e; }
.term .cmd { color:#fff; font-weight:700; }
.term .alarm { color:#fca5a5; font-weight:700; }
.term .HOLDS { color:#86efac; font-weight:700; }
.term .HOLDS-weakened { color:#fcd34d; font-weight:700; }
.term .SUSPECT { color:#fca5a5; font-weight:700; }
.term .cursor { display:inline-block; width:.6em; background:#e7e5e4; height:1.1em; vertical-align:text-bottom; }
""" % {"w": WIDTH, "h": HEIGHT}


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


def banner(state):
    return (
        f'<div class="banner" data-beat="{state["beat"]}"><span class="pill">{esc(BEAT_LABELS[state["beat"]])}</span>'
        f'<span class="cap">{esc(state["caption"])}</span></div>'
    )


def card(model, states, state):
    """The full-width strip under the banner: the statement as committed, the edit as a diff, or the summary's recap."""
    if state["beat"] == "summary":
        rows = "".join(
            f'<span>{n} &middot; {esc(s["kind"])}</span><span>check: {"alarm" if s["baseline_alarms"] else "quiet"}</span>'
            f'<span>req: <span class="{verdict_class(s["word"])}">{esc(s["word"])}</span></span>'
            for n, s in enumerate(model["steps"], start=1)
        )
        return f'<div class="card"><div class="recap">{rows}</div></div>'
    step = state["step"]
    if step is None:
        return (
            f'<div class="card"><span class="dim">{esc(model["claim"])}, as committed</span>\n'
            f'<span>{esc(model["start_statement"])}</span></div>'
        )
    emph = " emph" if state["beat"] == "edit" else ""
    return (
        f'<div class="card{emph}"><span class="dim">claims/{esc(model["claim"])}.yml &middot; {esc(step["kind"])}</span>\n'
        f'<span class="del">- {words(step["before"], step["after"], "del")}</span>\n'
        f'<span class="add">+ {words(step["after"], step["before"], "add")}</span></div>'
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
            if is_evidence and state["baseline_red"]:
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
    path = f'claims/{esc(model["claim"])}.yml'
    if not state["baseline_red"]:
        return f'<span class="dim">watching {path}</span>'
    return (
        f'<span class="dim">{path}: text changed</span>\n'
        f'<span class="alarm">&rarr; re-check {esc(model["element"])}</span>\n'
        f'<span class="dim">alarm {state["baseline_alarms"]}</span>'
    )


def right_terminal(state):
    if state["line"] is None:
        return '<span class="dim">$</span> <span class="cursor"></span>'
    head = '<span class="dim">$</span> <span class="cmd">req satisfy</span>'
    line = state["line"]
    word = line[1 : line.index("]")]
    rest = line[line.index("]") + 1 :]
    return f'{head}\n<span class="{verdict_class(word)}">[{esc(word)}]</span>{esc(rest)}'


def panel(model, state, side):
    count = len(model["steps"])
    step = state["step"]
    alarms = state["baseline_alarms"] if side == "baseline" else state["req_alarms"]
    dim = state["dim_left"] if side == "baseline" else state["dim_right"]
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
        f'<section class="panel" data-side="{side}" data-alarms="{alarms}" data-dim="{"true" if dim else "false"}">'
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
        f'{banner(state)}{card(model, states, state)}'
        f'<div class="panels">{panel(model, state, "baseline")}{panel(model, state, "req")}</div>'
        "</body></html>\n"
    )


# ------------------------------------------------------------------------------------------- the how-it-works picture

HOW_STEPS = (
    "A requirement is edited",
    "Its meaning fingerprint is derived again",
    "The direction of the change is classified",
    "Each recorded design link is judged by that direction",
)
HOW_DIRECTIONS = ("same", "looser", "tighter", "another direction")

HOW_CSS = """
:root { --bg:#fafaf9; --fg:#1c1917; --muted:#57534e; --card:#ffffff; --line:#d6d3d1; --accent:#1d4ed8; --red:#b91c1c;
        --frame:#e11d48; --frame-tint:#ffe4e6; --frame-fg:#be123c; --green:#15803d; }
* { box-sizing: border-box; }
html, body { margin:0; padding:0; }
body { width:%(w)dpx; height:%(h)dpx; overflow:hidden; background:var(--bg); color:var(--fg);
       font:16px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif; padding:24px; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.flow { display:flex; align-items:stretch; justify-content:space-between; height:%(fh)dpx; margin-top:12px; }
.step { flex:none; width:226px; background:var(--card); border:3px solid var(--line); border-radius:12px; padding:18px 18px; display:flex; flex-direction:column; gap:14px; }
.step .n { flex:none; width:44px; height:44px; border-radius:50%%; border:3px solid var(--fg); display:grid; place-items:center; font-size:24px; font-weight:800; }
.step h2 { margin:0; font-size:28px; line-height:1.2; letter-spacing:-.01em; }
.step.wide { width:370px; }
.step.s1 { width:240px; }
.step.s4 { width:250px; }
.dirs { display:flex; flex-wrap:wrap; gap:8px; margin-top:2px; }
.dir { font-size:21px; font-weight:700; border:2px solid var(--muted); border-radius:8px; padding:2px 12px; background:#f5f5f4; }
.dir.other { background:var(--card); flex:1 1 100%%; }
.dir small { display:block; font-size:19px; font-weight:500; color:var(--muted); font-family:system-ui, sans-serif; line-height:1.25; }
.arrow { flex:none; width:60px; display:flex; align-items:center; justify-content:center; }
.outcomes { width:280px; display:flex; flex-direction:column; justify-content:space-between; }
.outcome { position:relative; height:128px; border-width:3px; border-style:solid; border-radius:10px; display:flex; flex-direction:column; align-items:center;
           justify-content:center; text-align:center; padding:0 12px; background:#d7f3e6; border-color:#059669; }
.outcome .word { font-size:32px; font-weight:800; line-height:1.15; }
.outcome .alt { font-size:23px; font-weight:700; margin-top:4px; }
.outcome .note { font-size:21px; color:var(--muted); margin-top:6px; line-height:1.25; }
.outcome .frame { position:absolute; inset:-11px; border:3px dashed var(--frame); border-radius:13px; pointer-events:none; }
.outcome .frame .tag { position:absolute; top:-19px; left:12px; background:var(--frame-tint); color:var(--frame-fg); border:2px solid var(--frame);
                       border-radius:6px; font-size:17px; font-weight:700; padding:0 9px; line-height:1.35; white-space:nowrap; }
""" % {"w": WIDTH, "h": HOW_HEIGHT, "fh": HOW_HEIGHT - 48 - 12}

ARROW_SVG = (
    '<svg width="50" height="30" viewBox="0 0 50 30"><path d="M2 15H44" stroke="#57534e" stroke-width="3" fill="none"/>'
    '<path d="M35 6L46 15L35 24" stroke="#57534e" stroke-width="3" fill="none" stroke-linejoin="round" stroke-linecap="round"/></svg>'
)


def how_page():
    flow = []
    for n, text in enumerate(HOW_STEPS, start=1):
        extra = ""
        if n == 3:
            dirs = "".join(
                f'<div class="dir mono{" other" if d == "another direction" else ""}">{esc(d)}'
                + ("<small>a polarity flip, a scope change</small>" if d == "another direction" else "")
                + "</div>"
                for d in HOW_DIRECTIONS
            )
            extra = f'<div class="dirs">{dirs}</div>'
        wide = " wide" if n == 3 else f" s{n}"
        flow.append(f'<div class="step{wide}"><div class="n">{n}</div><h2>{esc(text)}</h2>{extra}</div>')
        flow.append(f'<div class="arrow">{ARROW_SVG}</div>')
    # The fork: step 4 reaches both outcomes. Box centres sit at 64 and (height - 64) of the flow's height.
    flow_h = HOW_HEIGHT - 48 - 12
    top, bottom, mid = 64, flow_h - 64, flow_h // 2
    fork = (
        f'<div class="arrow" style="width:70px"><svg width="70" height="{flow_h}" viewBox="0 0 70 {flow_h}" style="overflow:visible">'
        f'<path d="M2 {mid}H30V{top}H60M30 {mid}V{bottom}H60" stroke="#57534e" stroke-width="3" fill="none" stroke-linejoin="round"/>'
        f'<path d="M51 {top - 9}L62 {top}L51 {top + 9}M51 {bottom - 9}L62 {bottom}L51 {bottom + 9}" stroke="#57534e" stroke-width="3" fill="none" '
        'stroke-linejoin="round" stroke-linecap="round"/></svg></div>'
    )
    flow[-1] = fork
    outcomes = (
        '<div class="outcomes">'
        '<div class="outcome"><div class="word mono">HOLDS</div><div class="alt mono">or HOLDS (weakened)</div></div>'
        '<div class="outcome"><div class="frame"><span class="tag mono">re-check: SUSPECT</span></div>'
        '<div class="word mono">SUSPECT</div><div class="note">framed for a human<br>to re-check</div></div>'
        "</div>"
    )
    return (
        '<!doctype html><html lang="en"><head><meta charset="utf-8"><title>how req judges an edit</title>'
        f'<style>{HOW_CSS}</style></head><body><div class="flow">{"".join(flow)}{outcomes}</div></body></html>\n'
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


def screenshot(chrome, html_path, png_path, profile, size=None):
    width, height = size or (WIDTH, HEIGHT)
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
        f"--window-size={width},{height}",
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
    parser = argparse.ArgumentParser(prog="drift_replay.py", description="Make the README's drift replay GIF, its how-it-works picture and their records.")
    parser.add_argument("--out", help=f"write {GIF_NAME}, {HOW_NAME} and their records here (default: {DRIFT}/)")
    parser.add_argument("--html-only", metavar="DIR", help="write the HTML pages (eleven frames and the how-it-works page) to DIR and stop (needs neither Chrome nor ffmpeg)")
    args = parser.parse_args(argv)
    if args.out and args.html_only:
        parser.error("--out and --html-only are two different jobs")

    try:
        if args.html_only:
            model, _ = load_model()
            states = build_states(model)
            paths = write_pages(model, states, os.path.abspath(args.html_only))
            with open(os.path.join(os.path.abspath(args.html_only), "how-it-works.html"), "w", encoding="utf-8") as handle:
                handle.write(how_page())
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
            # The how-it-works picture, by the same path: one HTML page, one Chrome screenshot.
            how_html = os.path.join(work, "how-it-works.html")
            with open(how_html, "w", encoding="utf-8") as handle:
                handle.write(how_page())
            how_tmp = os.path.join(work, HOW_NAME)
            screenshot(chrome, how_html, how_tmp, os.path.join(work, "profile"), (WIDTH, HOW_HEIGHT))
            if png_size(how_tmp) != (WIDTH, HOW_HEIGHT):
                print(f"drift_replay: the how-it-works picture is {png_size(how_tmp)}, not {(WIDTH, HOW_HEIGHT)}", file=sys.stderr)
                return 1
            how_path = os.path.join(out_dir, HOW_NAME)
            shutil.copyfile(how_tmp, how_path)

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
        how_record = {
            "schema": HOW_SCHEMA,
            "png": f"{DRIFT}/{HOW_NAME}",
            "png_sha256": sha256(how_path),
            "script": SCRIPT,
            "script_sha256": record["script_sha256"],
            "chrome": record["chrome"],
            "frame_size": [WIDTH, HOW_HEIGHT],
            "recipe": "python3 tools/demo/drift_replay.py (the wording is fixed in the script, so the record binds the script and the PNG; "
            f"one HTML page screenshotted by headless Chrome at {WIDTH}x{HOW_HEIGHT})",
        }
        with open(os.path.join(out_dir, HOW_RECORD_NAME), "w", encoding="utf-8") as handle:
            json.dump(how_record, handle, indent=2)
            handle.write("\n")
        print(f"drift_replay: wrote {gif_path} ({size} bytes, {frames} frames), {how_path} and their records in {out_dir}")
        return 0
    except Unverifiable as error:
        print(f"drift_replay: UNVERIFIABLE: {error}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
