#!/usr/bin/env python3
"""Build req's external corpus from real requirement text in other projects' repos.

Why this exists: the seed parity corpus is self-authored, so it can only show that
req agrees with itself. This corpus takes requirement prose written by *other*
projects, in *other* tools, and labels pairs **by construction** — the transformation
applied — never by asking the canonicalizer. The labels are an independent oracle.

Sources (pinned by commit SHA, recorded in the output): StrictDoc's own L1
requirements, Doorstop's own requirements, and the PROMISE NFR set (a deterministic
sample; 47 project ids upstream). **Sphinx-Needs' bundled examples were evaluated and
excluded**: its own docs use standard Sphinx directives (`versionadded`, `note`), and
its demo project's needs carry a title plus attributes but no statement body — e.g.
`.. swreq:: Lane Marking Detection Algorithm` — so they are not requirement prose to
canonicalize.

Scope: Tier-1 only. Equivalence pairs use meaning-preserving Tier-1 normalizations
(emphasis stripping, modality synonyms, case/punctuation, digit grouping, exact
power-of-ten unit scaling). Paraphrase, voice and word-order changes are Tier-2 work
and are deliberately out of scope here.

Usage: python3 tools/corpus/build_external.py > bench/corpus/external-v3.yml
"""
import csv
import io
import json
import re
import sys
import urllib.request
from decimal import Decimal

STRICTDOC = ("strictdoc-project/strictdoc", "main",
             "docs/strictdoc_20_l1_system_requirements.sdoc")
DOORSTOP = ("doorstop-dev/doorstop", "develop")
# The upstream dataset is the PROMISE NFR natural-language set; this is the mirror
# that could be fetched and pinned by commit. Sampled deterministically — all 969
# statements x 5 transforms would make the corpus (and its review diff) unreviewable.
PROMISE = ("mahdikabootari/Software-Requirements-Classification", "main", "PROMISE.csv")
PROMISE_EVERY = 5


def get(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": "req-corpus-builder"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read().decode("utf-8", "replace")


def sha_of(repo: str, branch: str) -> str:
    return json.loads(get(f"https://api.github.com/repos/{repo}/commits/{branch}"))["sha"]


def raw(repo: str, sha: str, path: str) -> str:
    return get(f"https://raw.githubusercontent.com/{repo}/{sha}/{path}")


def parse_strictdoc(text: str) -> list[tuple[str, str]]:
    """pure: the file text in, (uid, statement) out — so any revision can be parsed with the same
    rules. Extracted rather than re-implemented for the Tier-2 sample builder: two parsers would
    drift, and the drift would be invisible in a corpus whose statements look plausible."""
    out = []
    for block in text.split("[REQUIREMENT]")[1:]:
        uid = re.search(r"UID: (\S+)", block)
        stmt = re.search(r"STATEMENT: >>>\n(.*?)\n<<<", block, re.S)
        if uid and stmt:
            out.append((uid.group(1), " ".join(stmt.group(1).split())))
    return out


def strictdoc_statements() -> tuple[str, list[tuple[str, str]]]:
    sha = sha_of(*STRICTDOC[:2])
    return sha, parse_strictdoc(raw(STRICTDOC[0], sha, STRICTDOC[2]))


def parse_doorstop(lines: list[str]) -> list[str]:
    out = []
    for i, line in enumerate(lines):
            if line.strip() == "text: |":
                body = []
                for x in lines[i + 1:]:
                    if x.startswith("  ") or not x.strip():
                        body.append(x[2:] if x.startswith("  ") else "")
                    else:
                        break
                text = " ".join(y.strip() for y in body if y.strip())
                if text:
                    out.append(text)
                break
    return out


def doorstop_statements() -> tuple[str, list[tuple[str, str]]]:
    sha = sha_of(*DOORSTOP)
    tree = json.loads(get(
        f"https://api.github.com/repos/{DOORSTOP[0]}/git/trees/{sha}?recursive=1"))
    paths = sorted(e["path"] for e in tree["tree"]
                   if re.fullmatch(r"reqs/REQ\d+\.yml", e["path"]))
    out = []
    for path in paths:
        for text in parse_doorstop(raw(DOORSTOP[0], sha, path).splitlines()):
            out.append((path.rsplit("/", 1)[-1].removesuffix(".yml"), text))
    return sha, out


def promise_statements() -> tuple[str, list[tuple[str, str]], int]:
    """The PROMISE NFR set, sampled deterministically (every PROMISE_EVERY-th row).

    Returns (sha, sampled statements, total available upstream). The sample is taken
    by position, not by hand: nobody chose which requirements the corpus contains.
    """
    sha = sha_of(*PROMISE[:2])
    rows = list(csv.DictReader(io.StringIO(raw(PROMISE[0], sha, PROMISE[2]))))
    texts = [(f"P{r['ProjectID']}:r{i}", " ".join(r["RequirementText"].split()))
             for i, r in enumerate(rows)
             if r.get("RequirementText", "").strip()]
    return sha, texts[::PROMISE_EVERY], len(texts)


# --- meaning-preserving transforms (equivalence by construction) --------------

def t_emphasis(s: str):
    """T1.2b: Markdown emphasis delimiters are typographic, not semantic."""
    return s.replace("**", "") if "**" in s else None


def t_modality_synonym(s: str):
    """T1.6: shall and must both normalize to SHALL."""
    if re.search(r"\bshall not\b", s, re.I) or not re.search(r"\bshall\b", s, re.I):
        return None
    return re.sub(r"\bshall\b", "must", s, count=1, flags=re.I)


def t_case_punct(s: str):
    """T1.1/T1.2: case, whitespace runs and a trailing period are not meaning."""
    out = s.rstrip()
    if out.endswith("."):
        out = out[:-1]
    out = out[:1].lower() + out[1:] if out[:1].isupper() else out
    parts = out.split(" ", 1)
    return parts[0] + "  " + parts[1] if len(parts) == 2 else out


def t_numeric_grouping(s: str):
    """T1.2: digit grouping is presentation, not value (1,000 == 1000)."""
    m = re.search(r"\b(\d{4,})\b", s)
    if not m:
        return None
    return s[: m.start(1)] + f"{int(m.group(1)):,}" + s[m.end(1):]


# unit -> (exact factor, base symbol). Only powers of ten (hashing spec T1.4).
_UNIT_SCALES = (("ms", Decimal("0.001"), "s"),
                ("kN", Decimal("1000"), "N"),
                ("kHz", Decimal("1000"), "Hz"))


def t_unit_scale(s: str):
    """T1.4: exact power-of-ten scaling (2000 ms == 2 s; 12 kN == 12000 N).

    Exact decimal arithmetic, never a binary float: the label claims equality, so the
    generator must not be the thing that makes it approximate.
    """
    for unit, factor, target in _UNIT_SCALES:
        m = re.search(rf"\b(\d+(?:\.\d+)?)\s*{unit}\b", s)
        if m:
            value = format((Decimal(m.group(1)) * factor).normalize(), "f")
            return s[: m.start()] + f"{value} {target}" + s[m.end():]
    return None


# --- meaning-changing mutations (not-equivalence by construction) -------------

def t_modality_weakening(s: str):
    """T1.6: SHALL -> MAY. The dangerous direction; must never collapse."""
    if not re.search(r"\bshall\b", s, re.I):
        return None
    return re.sub(r"\bshall\b", "may", s, count=1, flags=re.I)


def t_polarity_flip(s: str):
    """Obligation -> prohibition."""
    if not re.search(r"\bshall\b", s, re.I):
        return None
    return re.sub(r"\bshall\b", "shall not", s, count=1, flags=re.I)


def t_predicate_swap(s: str):
    """Same subject, same modality, different predicate — the class the parity corpus
    missed and the canonicalizer once merged silently (an unbounded claim's predicate
    was not part of its identity)."""
    m = re.search(r"\b(shall|must)\b\s+(\w+)", s, re.I)
    if not m or m.group(2).lower() in ("not", "never"):
        return None
    repl = "log" if m.group(2).lower() == "delete" else "delete"
    return s[: m.start(2)] + repl + s[m.end(2):]


EQUIV = [("EM", "emphasis-strip (T1.2b)", t_emphasis),
         ("MS", "modality-synonym shall->must (T1.6)", t_modality_synonym),
         ("CP", "case/whitespace/period (T1.1-T1.2)", t_case_punct),
         ("NG", "digit-grouping 1234->1,234 (T1.2)", t_numeric_grouping),
         ("US", "unit scale 2000 ms->2 s (T1.4)", t_unit_scale)]
NOTEQUIV = [("MW", "modality-weakening shall->may (T1.6)", t_modality_weakening),
            ("PF", "polarity-flip shall->shall not", t_polarity_flip),
            ("PS", "predicate-swap (same subject+modality, different verb)", t_predicate_swap)]


def yaml_scalar(s: str) -> str:
    """A JSON string is a valid YAML double-quoted scalar — no YAML lib here."""
    return json.dumps(s, ensure_ascii=False)


def main() -> int:
    sd_sha, sd = strictdoc_statements()
    ds_sha, ds = doorstop_statements()
    pm_sha, pm, pm_total = promise_statements()

    eq, ne = [], []
    for src, sha, items in (("strictdoc", sd_sha, sd), ("doorstop", ds_sha, ds),
                            ("promise", pm_sha, pm)):
        for uid, text in items:
            for code, label, fn in EQUIV:
                b = fn(text)
                if b and b != text:
                    eq.append((f"E-{src[:2].upper()}-{uid}-{code}", text, b,
                               f"{src}@{sha[:12]} {uid}", label))
            for code, label, fn in NOTEQUIV:
                b = fn(text)
                if b and b != text:
                    ne.append((f"N-{src[:2].upper()}-{uid}-{code}", text, b,
                               f"{src}@{sha[:12]} {uid}", label))

    for group in (eq, ne):
        for pid, a, b, _src, _label in group:
            assert a != b, f"{pid}: pair sides are identical"
    ids = [pid for pid, *_ in eq + ne]
    assert len(ids) == len(set(ids)), "duplicate pair ids"

    out = []
    out.append("schema: req.bench.corpus/0")
    out.append("version: 3")
    out.append("rules: " + yaml_scalar(
        "External corpus v3: real requirement prose from other projects' repositories, "
        f"pinned by commit — strictdoc@{sd_sha}, doorstop@{ds_sha}, "
        f"promise-nfr@{pm_sha} (deterministic sample: every {PROMISE_EVERY}th of "
        f"{pm_total} requirements, 47 project ids upstream). Pairs are labelled by "
        "construction (the transformation applied), not by the canonicalizer, so the "
        "labels are an independent oracle. Scope is Tier-1 only: equivalence uses "
        "meaning-preserving Tier-1 normalizations (emphasis, modality synonym, "
        "case/punctuation, digit grouping, exact power-of-ten unit scaling); paraphrase, "
        "voice and word-order are Tier-2 and out of scope. Modality transforms replace the "
        "FIRST occurrence, so multi-clause statements also exercise whether the tail beyond "
        "the first obligation is decomposed — see the findings in "
        "docs/canonical-hashing.md 6.3. Sphinx-Needs' bundled examples were evaluated and "
        "EXCLUDED: its own docs use standard Sphinx directives, and its demo project's 29 "
        "swreq needs carry a title plus attributes but no statement body, so they are not "
        "requirement statements."))
    for title, group in (("equivalent", eq), ("not_equivalent", ne)):
        out.append(f"{title}:")
        for pid, a, b, src, label in group:
            out.append(f"  - id: {pid}")
            out.append(f"    a: {yaml_scalar(a)}")
            out.append(f"    b: {yaml_scalar(b)}")
            out.append(f"    source: {yaml_scalar(src)}")
            out.append(f"    note: {yaml_scalar(label)}")
    print("\n".join(out))

    print(f"# equivalent={len(eq)} not_equivalent={len(ne)} "
          f"(sources: strictdoc={len(sd)}, doorstop={len(ds)}, "
          f"promise={len(pm)}/{pm_total} sampled)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
