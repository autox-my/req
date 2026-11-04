#!/usr/bin/env python3
"""Build docs/overview.html, the view of req for people who do not read Rust.

The page is GENERATED, never hand-edited, and every terminal block on it is the real output of the
`req` on PATH, run in a scratch corpus: nothing on it is typed in by a person, so it cannot claim
what the tool does not do. It carries no invented metric (no "time saved", no "errors reduced"):
the only numbers on it are the ones the tool printed in this run.

    python3 tools/overview/build.py --out docs/overview.html     write it
    python3 tools/overview/build.py --check docs/overview.html    exit 0 current, 1 stale, 3 no req

Exit: 0 written / current, 1 the file is stale or the scenario did not behave as the page says,
2 usage, 3 `req` is not on PATH (UNVERIFIABLE: it cannot be rendered honestly without running it).
"""
import html
import os
import shutil
import subprocess
import sys
import tempfile

STATEMENT_OLD = "within 200 ms"


def run(binary, cwd, *args):
    try:
        done = subprocess.run([binary, *args], cwd=cwd, capture_output=True, text=True, check=False,
                              stdin=subprocess.DEVNULL, timeout=120,
                              env={k: v for k, v in os.environ.items() if k != "REQ_ROOT"})
    except subprocess.TimeoutExpired:
        raise SystemExit(f"`req {' '.join(args)}` did not finish in 120s")
    if done.returncode not in (0, 1) and args and args[0] == "init":
        raise SystemExit(f"req init exited {done.returncode}: {done.stderr.strip()}")
    return done.returncode, done.stdout


def scenario(binary):
    """Run the story in a scratch corpus. Returns the blocks (command, output) and facts."""
    with tempfile.TemporaryDirectory(prefix="req-overview-") as tmp:
        code, out = run(binary, tmp, "init")
        if code != 0:
            raise SystemExit(f"req init exited {code}")
        claim = os.path.join(tmp, "claims", "REQ-0001.yml")
        with open(claim, encoding="utf-8") as handle:
            original = handle.read()
        if STATEMENT_OLD not in original:
            raise SystemExit("the starter claim no longer says `within 200 ms`; update the page")

        def variant(name, new_phrase):
            text = original.replace(STATEMENT_OLD, new_phrase)
            if "\npins:" not in text:
                raise SystemExit("the starter claim no longer has a `pins:` block; update the page")
            text = text[: text.index("\npins:")] + "\n"  # a proposal is a statement, not a pin set
            with open(os.path.join(tmp, name), "w", encoding="utf-8") as handle:
                handle.write(text)

        blocks = {}
        found = [l for l in original.splitlines() if l.startswith("statement:")]
        if not found:
            raise SystemExit("the starter claim has no `statement:` line; update the page")
        blocks["statement"] = found[0]
        code, out = run(binary, tmp, "canon")
        blocks["canon"] = ("req canon", "\n".join(out.splitlines()[:4]), code)
        variant("reworded.yml", "within 0.2 s")
        code, out = run(binary, tmp, "compare", "claims/REQ-0001.yml", "reworded.yml")
        blocks["same"] = ("req compare claims/REQ-0001.yml reworded.yml", out.strip(), code)
        variant("loosened.yml", "within 500 ms")
        code, out = run(binary, tmp, "compare", "claims/REQ-0001.yml", "loosened.yml")
        blocks["loosened"] = ("req compare claims/REQ-0001.yml loosened.yml", out.strip(), code)
        with open(claim, "w", encoding="utf-8") as handle:
            handle.write(original.replace(STATEMENT_OLD, "within 500 ms"))
        code, out = run(binary, tmp, "gate")
        blocks["gate"] = ("req gate", "\n".join(out.splitlines()[:6]), code)
    expectations = [
        ("canon", 0, ["pin operator LE: agrees", "pin value 0.2: agrees", "pin unit s: agrees"]),
        ("same", 0, ["identical"]),
        ("loosened", 0, ["numeric_loosening"]),
        ("gate", 1, ["[FAIL] pins_hold", "REQ-0001: pin value 0.2 contradicts"]),
    ]
    for key, want_code, needles in expectations:
        command, text, code = blocks[key]
        missing = [n for n in needles if n not in text]
        if code != want_code or missing:
            raise SystemExit(f"the scenario no longer behaves as the page says ({key}: exit "
                             f"{code}, wanted {want_code}; missing {missing}):\n{text}")
    return blocks


def term(block):
    command, text, _ = block
    return (f'<pre class="term" tabindex="0" aria-label="Output of {html.escape(command)}">'
            f'<span class="prompt">$ {html.escape(command)}</span>\n'
            f"{html.escape(text)}</pre>")


PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>req, in one page</title>
<style>
:root{--bg:#fbfaf7;--fg:#1c1b19;--muted:#66625b;--card:#fff;--line:#e3dfd6;--accent:#2a5bd7;
--bad:#b3261e;--good:#1e7a3c;--termbg:#15171c;--termfg:#dfe3ea}
@media (prefers-color-scheme:dark){:root{--bg:#14161a;--fg:#ecebe7;
--muted:#a39f96;--card:#1c1f25;--line:#2d313a;--accent:#8fb0ff;--bad:#ff8a80;--good:#7fd69a;
--termbg:#0d0f13}}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.55 system-ui,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px 64px}
h1{font-size:2rem;margin:0 0 4px}h2{font-size:1.25rem;margin:40px 0 8px}
.sub{color:var(--muted);margin:0 0 24px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px 18px}
.lanes{display:grid;grid-template-columns:1fr 1fr;gap:14px}
@media (max-width:640px){.lanes{grid-template-columns:1fr}}
.lane h3{margin:0 0 8px;font-size:1rem}.lane.before h3{color:var(--bad)}.lane.after h3{color:var(--good)}
.lane ol{margin:0;padding-left:20px}.lane li{margin:6px 0}
pre.term{background:var(--termbg);color:var(--termfg);border-radius:8px;padding:12px 14px;
overflow-x:auto;font:13px/1.5 ui-monospace,Menlo,monospace;margin:10px 0}
.prompt{color:#8fd19e}.note{color:var(--muted);font-size:.92rem}
.beat{margin:22px 0}.beat h3{margin:0 0 2px;font-size:1.05rem}
.limits li{margin:6px 0}
code{font:0.92em ui-monospace,Menlo,monospace;background:var(--card);border:1px solid var(--line);
border-radius:4px;padding:1px 5px}
footer{margin-top:40px;color:var(--muted);font-size:.88rem}
</style>
</head>
<body>
<main>
<h1>req, in one page</h1>
<p class="sub">Requirements that live next to your code, with a tool that knows when a rewording
changed what you promised. For people who decide, plan or explain; the engineering view is
<code>docs/how-it-works.md</code>.</p>

<div class="card">
<strong>The problem.</strong> A requirement is a sentence. Someone edits the sentence in a pull
request, and everyone reads the diff as wording. Whether the edit <em>kept</em> the promise,
<em>loosened</em> it or <em>broke</em> it is decided by whoever happens to notice. req makes that
question something the tool answers, and leaves the decision to a person.
</div>

<h2>Before and after, on one real edit</h2>
<div class="lanes">
<div class="card lane before"><h3>Without a tool that reads meaning</h3><ol>
<li>The requirement says &ldquo;respond within 200&nbsp;ms&rdquo;.</li>
<li>A pull request changes it to &ldquo;within 500&nbsp;ms&rdquo;, among other edits.</li>
<li>Reviewers see changed words. The loosening is one number in a larger diff.</li>
<li>It merges if nobody spots it. Nothing in the process failed.</li>
</ol></div>
<div class="card lane after"><h3>With req</h3><ol>
<li>The requirement is a file with a short list of <em>pins</em>: the numbers that matter.</li>
<li>The same edit is made.</li>
<li>The tool states the direction of the change and fails the check on the stale pin.</li>
<li>A person decides, and the decision is recorded.</li>
</ol></div>
</div>

<h2>What that looks like (real output)</h2>
<p class="note">Every block below was produced by running <code>req</code> in a scratch corpus when
this page was built; none of it is typed in. The requirement is
<code>__STATEMENT__</code></p>

<div class="beat"><h3>1. The tool reads the sentence and checks what you pinned</h3>
__CANON__
<p class="note">The line starting <code>REQ-0001</code> is what the tool understood from the sentence;
each <code>pin</code> line is a number a person said must stay true, and the tool agrees.</p></div>

<div class="beat"><h3>2. A reword that keeps the promise is not noise</h3>
__SAME__
<p class="note">&ldquo;200&nbsp;ms&rdquo; and &ldquo;0.2&nbsp;s&rdquo; are the same promise. Same fingerprint,
nothing to review.</p></div>

<div class="beat"><h3>3. An edit that loosens it is named</h3>
__LOOSENED__
<p class="note">The tool classifies the direction. It does not approve it: ratification is a human
act.</p></div>

<div class="beat"><h3>4. An edit that forgets the pin fails the check</h3>
__GATE__
<p class="note">The release gate fails, naming the claim and the pin. Exit code 1 stops the merge
in CI.</p></div>

<h2>Who it is for</h2>
<div class="card">
<ul>
<li><strong>Teams that must show why a requirement is what it is</strong> (regulated work,
audits): each claim carries its source, date and rejected alternatives; a signed baseline freezes
the set a release was judged against; <code>req audit</code> compiles the packet.</li>
<li><strong>A developer working with AI agents:</strong> the same files and exit codes an agent
can read, with rules that stop it editing a pin to make a check green.</li>
</ul></div>

<h2>What it does not do</h2>
<div class="card limits"><ul>
<li>It does not write your requirements or decide they are right. A person authors every
statement.</li>
<li>It is deliberately conservative: some rewordings it cannot prove equivalent come out as
&ldquo;different&rdquo;. That is review noise, by design; the opposite error, merging two
different requirements, is the one it is built to never make.</li>
<li>That &ldquo;never&rdquo; is <em>measured</em> on req&rsquo;s own seed and trap corpora. It has
not yet been tested against outside corpora, so treat it as a claim under test, not a result.</li>
<li>A pin is checked when the gate runs; the signed baseline does not yet cover pins (a named,
recorded limit).</li>
</ul></div>
</main>
<footer style="max-width:860px;margin:0 auto;padding:0 16px 40px">Generated by <code>tools/overview/build.py</code>
from the real tool; checked by <code>cargo nextest</code> so it cannot drift. Do not edit by hand.</footer>
</body>
</html>
"""


def build(binary):
    blocks = scenario(binary)
    page = PAGE
    page = page.replace("__STATEMENT__", html.escape(blocks["statement"].split(": ", 1)[1].strip('"')))
    for key, token in (("canon", "__CANON__"), ("same", "__SAME__"),
                       ("loosened", "__LOOSENED__"), ("gate", "__GATE__")):
        page = page.replace(token, term(blocks[key]))
    return page


def main(argv):
    mode, path = None, None
    args = iter(argv)
    for arg in args:
        if arg in ("--out", "--check"):
            mode, path = arg, next(args, None)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"usage: build.py --out <file> | --check <file> (unknown: {arg})", file=sys.stderr)
            return 2
    if mode is None or not path:
        print("usage: build.py --out <file> | --check <file>", file=sys.stderr)
        return 2
    binary = shutil.which("req")
    if binary is None:
        print("UNVERIFIABLE: `req` is not on PATH; the page is rendered from its real output "
              "and cannot be built or checked without it", file=sys.stderr)
        return 3
    page = build(binary)
    if mode == "--out":
        with open(path, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(page)
        print(f"wrote {path}")
        return 0
    try:
        with open(path, encoding="utf-8") as handle:
            current = handle.read()
    except OSError as why:
        print(f"{path} cannot be read: {why}", file=sys.stderr)
        return 1
    if current != page:
        print(f"STALE {path}: regenerate with `python3 tools/overview/build.py --out {path}`",
              file=sys.stderr)
        return 1
    print(f"[CURRENT] {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
