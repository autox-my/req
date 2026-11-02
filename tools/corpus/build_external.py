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

Version 4 (rules v6) adds two families of *not-equivalent* mutation (any-position substitution: AP, AF;
possessive head: PH, PR), aimed at the failure the
earlier classes could not reach: a word that sits in **no slot of the canonical form**. The v1-v3
mutations (shall->may, shall->shall not, first-verb swap) never touch a trailing clause or a
subject's head, so two false merges in req's own corpus (REQ-0020's dropped tail, REQ-0004's
prefix subject) passed every external run. v4 reads the same sources at the same pinned commits as
v3 (`V4_PINS`), so a difference between the two corpora is the new classes and nothing else.

Usage: python3 tools/corpus/build_external.py > bench/corpus/external-v3.yml
       python3 tools/corpus/build_external.py --version 4 > bench/corpus/external-v4.yml
"""
import csv
import hashlib
import io
import json
import re
import sys
import urllib.request
from decimal import Decimal
from pathlib import Path

STRICTDOC = ("strictdoc-project/strictdoc", "main",
             "docs/strictdoc_20_l1_system_requirements.sdoc")
DOORSTOP = ("doorstop-dev/doorstop", "develop")
# The upstream dataset is the PROMISE NFR natural-language set; this is the mirror
# that could be fetched and pinned by commit. Sampled deterministically — all 969
# statements x 5 transforms would make the corpus (and its review diff) unreviewable.
PROMISE = ("mahdikabootari/Software-Requirements-Classification", "main", "PROMISE.csv")
PROMISE_EVERY = 5
# The commits external-v3.yml records, held as constants so v4 reads the very same statements.
# v1-v3 resolve the branch head at generation time and record what they saw; v4 is pinned instead,
# because its only job is to differ from v3 in the mutation classes.
V4_PINS = {"strictdoc": "abf7be7daa2a25721a56980b5be15845336ea0b8",
           "doorstop": "f267ef67cb80b0eb0dcdf0597fe61f623e851e15",
           "promise": "b63dde159b41d6cb7be01f03fad2c2a59490de8b"}


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


def strictdoc_statements(pin: str | None = None) -> tuple[str, list[tuple[str, str]]]:
    sha = pin or sha_of(*STRICTDOC[:2])
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


def doorstop_statements(pin: str | None = None) -> tuple[str, list[tuple[str, str]]]:
    sha = pin or sha_of(*DOORSTOP)
    tree = json.loads(get(
        f"https://api.github.com/repos/{DOORSTOP[0]}/git/trees/{sha}?recursive=1"))
    paths = sorted(e["path"] for e in tree["tree"]
                   if re.fullmatch(r"reqs/REQ\d+\.yml", e["path"]))
    out = []
    for path in paths:
        for text in parse_doorstop(raw(DOORSTOP[0], sha, path).splitlines()):
            out.append((path.rsplit("/", 1)[-1].removesuffix(".yml"), text))
    return sha, out


def promise_statements(pin: str | None = None) -> tuple[str, list[tuple[str, str]], int]:
    """The PROMISE NFR set, sampled deterministically (every PROMISE_EVERY-th row).

    Returns (sha, sampled statements, total available upstream). The sample is taken
    by position, not by hand: nobody chose which requirements the corpus contains.
    """
    sha = pin or sha_of(*PROMISE[:2])
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


# --- v4: statement words that no slot of the canonical form holds (rules v6, T1.10) ---------
#
# Both classes take the pair id, because the position they touch is chosen from
# sha256(pair id): deterministic, reproducible from the id alone, and chosen by nobody. The label
# is by construction — one word of the statement was replaced by a token the statement does not
# contain, or the subject was made the possessor of a component, so the meaning changed — and is
# never read off the canonicalizer.

_STOPWORDS = {"a", "an", "the"}  # T1.10's stopword set; a replaced article is not "a meaning change"
_WORD = re.compile(r"^(\W*)(.*?)(\W*)$", re.S)
_COMPONENTS = ("index", "parser", "scheduler", "cache", "exporter", "validator")


def _pick(pair_id: str, n: int, salt: str = "") -> int:
    return int(hashlib.sha256((pair_id + salt).encode()).hexdigest(), 16) % n


def _substitute(s: str, pair_id: str, final_third: bool):
    """Replace one content word with a token absent from the statement.

    The position is `sha256(pair id) mod (candidates)`; with `final_third` the candidates are
    only the words in the last third of the statement — where a dropped tail lives, and where the
    first-verb mutations of v1-v3 never reached.
    """
    tokens = s.split()
    n = len(tokens)
    if n < 4:
        return None
    lo = (2 * n + 2) // 3 if final_third else 0  # ceil(2n/3)
    candidates = []
    for i in range(lo, n):
        pre, core, post = _WORD.match(tokens[i]).groups()
        if core and any(c.isalnum() for c in core) and core.lower() not in _STOPWORDS:
            candidates.append(i)
    if not candidates:
        return None
    i = candidates[_pick(pair_id, len(candidates))]
    pre, core, post = _WORD.match(tokens[i]).groups()
    seed = hashlib.sha256(pair_id.encode()).hexdigest()[:6]
    token = f"zq{seed}"
    assert token not in s.lower(), f"{pair_id}: replacement token present in the statement"
    out = tokens[:]
    out[i] = pre + token + post
    return " ".join(out)


def t_any_position(s: str, pair_id: str):
    """Any-position substitution: one content word anywhere becomes an unseen token."""
    return _substitute(s, pair_id, final_third=False)


def t_final_third(s: str, pair_id: str):
    """Any-position substitution, restricted to the final third of the statement."""
    return _substitute(s, pair_id, final_third=True)


def registry_aliases() -> list[str]:
    """The surface forms `aliases/subjects.yml` registers, read without a YAML library (the file's
    shape is fixed: `aliases:` followed by `- <surface>` lines). Needed by one class only: a
    possessive head can fool the registry only when the subject IS a registered alias, and real
    external prose seldom opens with "the req tool"."""
    path = Path(__file__).resolve().parents[2] / "aliases" / "subjects.yml"
    out, in_aliases = [], False
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if stripped == "aliases:":
            in_aliases = True
        elif in_aliases and stripped.startswith("- "):
            out.append(stripped[2:].strip())
        elif stripped:
            in_aliases = False
    assert out, f"no aliases read from {path}"
    return out


def t_possessive_registered(s: str, pair_id: str):
    """Possessive head on a *registered* subject. The statement's own subject is replaced by a
    registry alias on both sides, then the b side becomes `<alias>'s <component> shall`. Only the
    subject is synthetic; the predicate is the real prose. This is the class that has teeth against
    a prefix match: "the req tool's cache" must not be the req tool."""
    m = re.match(r"^([^,:;]+?)\s+(shall|must)\b", s)
    if not m or not 1 <= len(m.group(1).split()) <= 8:
        return None
    aliases = registry_aliases()
    alias = aliases[_pick(pair_id, len(aliases), "alias")]
    alias = alias[:1].upper() + alias[1:]
    component = _COMPONENTS[_pick(pair_id, len(_COMPONENTS), "component")]
    rest = s[m.start(2):]
    # The a side is the *rebased* statement, so the pair is (rebased, possessive-of-rebased).
    return (f"{alias} {rest}", f"{alias}'s {component} {rest}")


def t_possessive_head(s: str, pair_id: str):
    """`<subject> shall` -> `<subject>'s <component> shall`: the claim is now about a part of the
    subject. A registry alias that matched only the head of that span used to claim it whole."""
    m = re.match(r"^([^,:;]+?)\s+(shall|must)\b", s)
    if not m:
        return None
    subject = m.group(1).strip()
    if not 1 <= len(subject.split()) <= 8 or subject.endswith("'s"):
        return None
    component = _COMPONENTS[_pick(pair_id, len(_COMPONENTS), "component")]
    return f"{subject}'s {component} {s[m.start(2):]}"


EQUIV = [("EM", "emphasis-strip (T1.2b)", t_emphasis),
         ("MS", "modality-synonym shall->must (T1.6)", t_modality_synonym),
         ("CP", "case/whitespace/period (T1.1-T1.2)", t_case_punct),
         ("NG", "digit-grouping 1234->1,234 (T1.2)", t_numeric_grouping),
         ("US", "unit scale 2000 ms->2 s (T1.4)", t_unit_scale)]
NOTEQUIV = [("MW", "modality-weakening shall->may (T1.6)", t_modality_weakening),
            ("PF", "polarity-flip shall->shall not", t_polarity_flip),
            ("PS", "predicate-swap (same subject+modality, different verb)", t_predicate_swap)]
# Mutations that take the pair id (a position chosen from sha256(pair id)); v4 only.
NOTEQUIV_V4 = [("AP", "any-position substitution (T1.10): one content word -> an unseen token",
                t_any_position),
               ("AF", "any-position substitution in the final third (T1.10)", t_final_third),
               ("PH", "possessive head (T1.7): `<subject> shall` -> `<subject>'s <component> shall`",
                t_possessive_head),
               ("PR", "possessive head on a registered alias subject (T1.7 whole span)",
                t_possessive_registered)]


def yaml_scalar(s: str) -> str:
    """A JSON string is a valid YAML double-quoted scalar — no YAML lib here."""
    return json.dumps(s, ensure_ascii=False)


def main(version: int = 3) -> int:
    pins = V4_PINS if version == 4 else {}
    sd_sha, sd = strictdoc_statements(pins.get("strictdoc"))
    ds_sha, ds = doorstop_statements(pins.get("doorstop"))
    pm_sha, pm, pm_total = promise_statements(pins.get("promise"))

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
            if version >= 4:
                for code, label, fn in NOTEQUIV_V4:
                    pid = f"N-{src[:2].upper()}-{uid}-{code}"
                    b = fn(text, pid)
                    # a mutation may return (rebased a, b) when it must change the a side too
                    a, b = b if isinstance(b, tuple) else (text, b)
                    if b and b != a:
                        ne.append((pid, a, b, f"{src}@{sha[:12]} {uid}", label))

    for group in (eq, ne):
        for pid, a, b, _src, _label in group:
            assert a != b, f"{pid}: pair sides are identical"
    ids = [pid for pid, *_ in eq + ne]
    assert len(ids) == len(set(ids)), "duplicate pair ids"

    out = []
    out.append("schema: req.bench.corpus/0")
    out.append(f"version: {version}")
    out.append("rules: " + yaml_scalar(
        f"External corpus v{version}: real requirement prose from other projects' repositories, "
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
        "requirement statements."
        + (" v4 (rules v6) adds two not-equivalent classes, labelled by construction: "
           "any-position substitution (one content word replaced by a token absent from the "
           "statement, at a position chosen from sha256(pair id); a second pair restricted to "
           "the final third) and possessive head (`<subject> shall` -> `<subject>'s "
           "<component> shall`, plus a variant whose subject is first rebased onto a registered "
           "alias from aliases/subjects.yml so that a prefix match of the registry is exercised "
           "— external prose seldom opens with a registered subject). It reads the same pinned commits as v3, so the difference "
           "between the two is those classes and nothing else." if version >= 4 else "")))
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
    args = sys.argv[1:]
    if args in ([], ["--version", "3"]):
        raise SystemExit(main(3))
    if args == ["--version", "4"]:
        raise SystemExit(main(4))
    print("usage: build_external.py [--version 3|4]", file=sys.stderr)
    raise SystemExit(2)
