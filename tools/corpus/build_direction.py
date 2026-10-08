#!/usr/bin/env python3
"""Build the **direction gold set**: what kind of change is this?

Labels are the mutation applied — never the classifier's answer. Same discipline as the
identity corpora, for the same reason: a corpus labelled by the thing it tests cannot
falsify it. Sources are the same three real-requirement repositories, pinned by commit,
fetched through `build_external` so there is one fetcher, not two.

Usage: python3 tools/corpus/build_direction.py > bench/corpus/direction-v1.yml
"""
import re
import sys
from decimal import Decimal

from build_external import (
    doorstop_statements,
    promise_statements,
    strictdoc_statements,
    t_case_punct,
    yaml_scalar,
)

# Which way a bound moves depends on which side of the bound it is — the sign that is
# easy to get backwards, which is exactly why the gold set exists.
UPPER_WORDS = ("within", "at most", "no later than", "less than", "fewer than", "under")
LOWER_WORDS = ("at least", "no less than", "greater than", "more than")
NUMBER_UNIT = r"(\d+(?:\.\d+)?)\s*(ms|s|%|kN|N|kHz|Hz|A|amps|V|records|seconds|minutes)"


def _scale(s: str, factor: Decimal):
    m = re.search(rf"\b{NUMBER_UNIT}\b", s)
    if not m:
        return None
    # Exact decimal arithmetic: the label claims a numeric direction, so the generator
    # must not make it approximate.
    value = format((Decimal(m.group(1)) * factor).normalize(), "f")
    return s[: m.start(1)] + value + s[m.end(1):]


def d_numeric_loosening(s: str):
    """An upper bound raised, or a lower bound lowered: more values comply."""
    low = s.lower()
    if any(w in low for w in UPPER_WORDS):
        return _scale(s, Decimal("1.25"))
    if any(w in low for w in LOWER_WORDS):
        return _scale(s, Decimal("0.8"))
    return None


def d_numeric_tightening(s: str):
    """The reverse: fewer values comply."""
    low = s.lower()
    if any(w in low for w in UPPER_WORDS):
        return _scale(s, Decimal("0.8"))
    if any(w in low for w in LOWER_WORDS):
        return _scale(s, Decimal("1.25"))
    return None


def d_lt_tightening(s: str):
    """within 200 ms -> in fewer than 200 ms: LE to LT at the same number.

    Requires a number after "within": the preposition appears in prose that way ("within
    the EU"), and replacing that produced a pair whose label assumed a bound that was
    never there — the gold set flagged it, and it was the *label* that was wrong.
    """
    if not re.search(r"\bwithin\s+\d", s, re.I):
        return None
    return re.sub(r"\bwithin\b", "in fewer than", s, count=1, flags=re.I)


def d_scope_narrowing(s: str):
    """A guard added: the claim applies less often."""
    if not re.search(r"\b(shall|must|may|should)\b", s, re.I):
        return None
    body = s.rstrip().rstrip(".")
    if re.search(r"\b(when|if|unless|while|during)\b", body, re.I):
        return None  # already conditional: the guard would be replaced, not added
    return f"{body} when the brake is engaged."


def d_scope_widening(s: str):
    """A guard removed: it applies more often."""
    m = re.match(
        r"^(.*?)[,;]?\s+(when|if|unless|while|during)\b.*$", s.rstrip().rstrip("."), re.I
    )
    if not m:
        return None
    return m.group(1).rstrip(",; ") + "."


def d_scope_divergence(s: str):
    """The same number bounding a different *kind* of thing.

    Restricted to statements that carry a comparator word: the label claims the unit is
    the bound's, and when the unit is not in the value slot the change reads as an action
    change instead (a parser-coverage fact the report names, not a class the classifier
    should be told to invent).
    """
    if not any(w in s.lower() for w in UPPER_WORDS + LOWER_WORDS):
        return None
    # ...and the unit must sit on a number, or it is not the bound's unit.
    for old, new in (("A", "V"),):
        if re.search(rf"\d+(?:\.\d+)?\s*{old}\b", s):
            return re.sub(rf"\b{old}\b", new, s, count=1)
    return None


def d_unrelated(s: str):
    """A different predicate: not a change to *this* requirement."""
    m = re.search(r"\b(shall|must)\b\s+(\w+)", s, re.I)
    if not m or m.group(2).lower() in ("not", "never"):
        return None
    repl = "log" if m.group(2).lower() == "delete" else "delete"
    return s[: m.start(2)] + repl + s[m.end(2):]


def d_modality_weakening(s: str):
    if re.search(r"\bshall not\b", s, re.I) or not re.search(r"\bshall\b", s, re.I):
        return None
    return re.sub(r"\bshall\b", "may", s, count=1, flags=re.I)


def d_modality_strengthening(s: str):
    if not re.search(r"\bmay\b", s, re.I) or re.search(r"\bmay not\b", s, re.I):
        return None
    return re.sub(r"\bmay\b", "shall", s, count=1, flags=re.I)


def d_polarity_flip(s: str):
    # Skipping statements that are already prohibitions: replacing the "shall" inside
    # "shall not" produced a doubled negation, which is not a polarity flip at all — the
    # direction gold set flagged those two pairs as Unrelated, and it was right.
    if not re.search(r"\bshall\b", s, re.I) or re.search(r"\bshall not\b", s, re.I):
        return None
    return re.sub(r"\bshall\b", "shall not", s, count=1, flags=re.I)


DIRECTIONS = [
    ("MW", "modality_weakening", d_modality_weakening),
    ("MS", "modality_strengthening", d_modality_strengthening),
    ("PF", "polarity_flip", d_polarity_flip),
    ("NL", "numeric_loosening", d_numeric_loosening),
    ("NT", "numeric_tightening", d_numeric_tightening),
    ("LT", "numeric_tightening", d_lt_tightening),
    ("SN", "scope_narrowing", d_scope_narrowing),
    ("SW", "scope_widening", d_scope_widening),
    ("SD", "scope_divergence", d_scope_divergence),
    ("UN", "unrelated", d_unrelated),
]


def main() -> int:
    sd_sha, sd = strictdoc_statements()
    ds_sha, ds = doorstop_statements()
    pm_sha, pm, pm_total = promise_statements()

    identical, changed = [], []
    for src, sha, items in (("strictdoc", sd_sha, sd), ("doorstop", ds_sha, ds),
                            ("promise", pm_sha, pm)):
        tag = src[:2].upper()
        for uid, text in items:
            cosmetic = t_case_punct(text)
            if cosmetic and cosmetic != text:
                identical.append((f"I-{tag}-{uid}", text, cosmetic, f"{src}@{sha[:12]} {uid}",
                                  "identical", "cosmetic: case/whitespace/period"))
            for code, label, fn in DIRECTIONS:
                out = fn(text)
                if out and out != text:
                    changed.append((f"D-{tag}-{uid}-{code}", text, out,
                                    f"{src}@{sha[:12]} {uid}", label,
                                    (fn.__doc__ or code).strip().splitlines()[0]))

    for group in (identical, changed):
        for pid, a, b, _src, _label, _note in group:
            assert a != b, f"{pid}: pair sides are identical"
    ids = [pid for pid, *_ in identical + changed]
    assert len(ids) == len(set(ids)), "duplicate pair ids"

    out = ["schema: req.bench.corpus/0", "version: 1"]
    out.append("rules: " + yaml_scalar(
        "Direction gold set: every pair is a real requirement from another project (pinned "
        f"by commit — strictdoc@{sd_sha}, doorstop@{ds_sha}, promise-nfr@{pm_sha}, sampled "
        f"every 5th of {pm_total}) with one mutation applied, and the `direction` label IS "
        "that mutation. Labels are therefore an independent oracle for `req compare`, "
        "never the classifier's output. Vocabulary: identical | modality_weakening | "
        "modality_strengthening | polarity_flip | numeric_loosening | numeric_tightening | "
        "scope_narrowing | scope_widening | scope_divergence | unrelated. A misspelled "
        "label FAILS the gate rather than disabling the check."))
    for title, group in (("equivalent", identical), ("not_equivalent", changed)):
        out.append(f"{title}:")
        for pid, a, b, src, label, note in group:
            out.append(f"  - id: {pid}")
            out.append(f"    a: {yaml_scalar(a)}")
            out.append(f"    b: {yaml_scalar(b)}")
            out.append(f"    source: {yaml_scalar(src)}")
            out.append(f"    note: {yaml_scalar(note)}")
            out.append(f"    direction: {yaml_scalar(label)}")
    print("\n".join(out))

    print(f"# identical={len(identical)} changed={len(changed)} "
          f"(sources: strictdoc={len(sd)}, doorstop={len(ds)}, promise={len(pm)}/{pm_total})",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
