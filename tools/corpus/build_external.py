#!/usr/bin/env python3
"""Build req's external corpus from real requirement text in other projects' repos.

Why this exists: the seed parity corpus is self-authored, so it can only show that
req agrees with itself. This corpus takes requirement prose written by *other*
projects, in *other* tools (StrictDoc's own L1 requirements; Doorstop's own
requirements), and labels pairs **by construction** — the transformation applied —
never by asking the canonicalizer. The labels are therefore an independent oracle.

Scope: Tier-1 only. Equivalence pairs use meaning-preserving Tier-1 normalizations
(emphasis stripping, modality synonyms, case/punctuation). Paraphrase, voice and
word-order changes are Tier-2 work and are deliberately out of scope here.

Sources are pinned by commit SHA, and the SHA is recorded in the output.
Usage: python3 tools/corpus/build_external.py > bench/corpus/external-v1.yml
"""
import json
import re
import sys
import urllib.request

STRICTDOC = ("strictdoc-project/strictdoc", "main",
             "docs/strictdoc_20_l1_system_requirements.sdoc")
DOORSTOP = ("doorstop-dev/doorstop", "develop")


def get(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": "req-corpus-builder"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read().decode("utf-8", "replace")


def sha_of(repo: str, branch: str) -> str:
    return json.loads(get(f"https://api.github.com/repos/{repo}/commits/{branch}"))["sha"]


def raw(repo: str, sha: str, path: str) -> str:
    return get(f"https://raw.githubusercontent.com/{repo}/{sha}/{path}")


def strictdoc_statements() -> tuple[str, list[tuple[str, str]]]:
    sha = sha_of(*STRICTDOC[:2])
    text = raw(STRICTDOC[0], sha, STRICTDOC[2])
    out = []
    for block in text.split("[REQUIREMENT]")[1:]:
        uid = re.search(r"UID: (\S+)", block)
        stmt = re.search(r"STATEMENT: >>>\n(.*?)\n<<<", block, re.S)
        if uid and stmt:
            out.append((uid.group(1), " ".join(stmt.group(1).split())))
    return sha, out


def doorstop_statements() -> tuple[str, list[tuple[str, str]]]:
    sha = sha_of(*DOORSTOP)
    tree = json.loads(get(
        f"https://api.github.com/repos/{DOORSTOP[0]}/git/trees/{sha}?recursive=1"))
    paths = sorted(e["path"] for e in tree["tree"]
                   if re.fullmatch(r"reqs/REQ\d+\.yml", e["path"]))
    out = []
    for path in paths:
        lines = raw(DOORSTOP[0], sha, path).splitlines()
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
                    out.append((path.rsplit("/", 1)[-1].removesuffix(".yml"), text))
                break
    return sha, out


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
         ("CP", "case/whitespace/period (T1.1-T1.2)", t_case_punct)]
NOTEQUIV = [("MW", "modality-weakening shall->may (T1.6)", t_modality_weakening),
            ("PF", "polarity-flip shall->shall not", t_polarity_flip),
            ("PS", "predicate-swap (same subject+modality, different verb)", t_predicate_swap)]


def yaml_scalar(s: str) -> str:
    """A JSON string is a valid YAML double-quoted scalar — no YAML lib here."""
    return json.dumps(s, ensure_ascii=False)


def main() -> int:
    sd_sha, sd = strictdoc_statements()
    ds_sha, ds = doorstop_statements()

    eq, ne = [], []
    for src, sha, items in (("strictdoc", sd_sha, sd), ("doorstop", ds_sha, ds)):
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
    out.append("version: 2")
    out.append("rules: " + yaml_scalar(
        "External corpus: real requirement prose from other projects' repositories, "
        f"pinned by commit — strictdoc@{sd_sha}, doorstop@{ds_sha}. Pairs are labelled by "
        "construction (the transformation applied), not by the canonicalizer, so the labels "
        "are an independent oracle. Scope is Tier-1 only: equivalence uses meaning-preserving "
        "Tier-1 normalizations; paraphrase/voice/word-order are Tier-2 and out of scope. "
        "Modality transforms replace the FIRST occurrence, so multi-clause statements also "
        "exercise whether the tail beyond the first obligation is decomposed — see the "
        "findings recorded in docs/canonical-hashing.md 6.3."))
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
          f"(sources: strictdoc={len(sd)}, doorstop={len(ds)})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
