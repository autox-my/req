#!/usr/bin/env python3
"""Build the **direction gold set**: what kind of change is this?

Labels are the mutation applied — never the classifier's answer. Same discipline as the
identity corpora, for the same reason: a corpus labelled by the thing it tests cannot
falsify it. Sources are the same three real-requirement repositories, pinned by commit,
fetched through `build_external` so there is one fetcher, not two.

Usage: python3 tools/corpus/build_direction.py > bench/corpus/direction-v1.yml
       python3 tools/corpus/build_direction.py --version 2 > bench/corpus/direction-v2.yml
       python3 tools/corpus/build_direction.py --self-check    # the pure half; no network

Version 2 (S-1, ADR 0040 decision 5) is every v1 pair plus the **mirrors** of the loosening/tightening
families (the pair swapped, labelled with the opposite class — the label is still the mutation, read
backwards) and the **membership** mutations on any source statement carrying an enumeration. It exists to
measure false carries with a support large enough to mean something: v1 holds 5 modality_strengthening and
9 numeric_tightening pairs, so "zero false carries" out of that is vacuous. Both versions read the same
pinned commits (`V4_PINS`), so v1 regenerates byte-identical and v2 differs only by the added pairs.
"""
import re
import sys
from decimal import Decimal

from build_external import (
    V4_PINS,
    doorstop_statements,
    promise_statements,
    strictdoc_statements,
    t_case_punct,
    yaml_scalar,
)

# Which way a bound moves depends on which side of the bound it is — the sign that is
# easy to get backwards, which is exactly why the gold set exists.
#
# The **longest** matching phrase decides, so the negated forms are listed explicitly and a bare
# "more than" inside "no more than" does not count (R-1: it was read as a lower bound, and "less than"
# inside "no less than" as an upper one, so the labels agreed with the parser's wrong reading).
UPPER_WORDS = ("within", "at most", "no later than", "less than", "fewer than", "under",
               "no more than", "not more than", "no greater than", "not greater than")
LOWER_WORDS = ("at least", "no less than", "not less than", "no fewer than", "greater than",
               "more than")
# A number preceded by a backslash is an escape sequence ("\\92s", an escaped quote and an s), not a
# quantity: it matched as 92 seconds and produced pairs that change no bound at all.
NUMBER_UNIT = (r"(?<!\\)(\d+(?:\.\d+)?)\s*"
               r"(ms|s|%|kN|N|kHz|Hz|A|amps|V|records|seconds|minutes)")


def bound_side(s: str):
    """"upper", "lower" or None: which side of a bound the statement's comparator phrase is.

    Every phrase of both lists is located, any phrase lying inside a longer one is dropped (the
    longest match wins, as in the parser's table), and an upper phrase outranks a lower one, as
    before. Substring matching is kept as it was, so only the negated forms move.
    """
    low = s.lower()
    hits = []
    for side, words in (("upper", UPPER_WORDS), ("lower", LOWER_WORDS)):
        for w in words:
            i = low.find(w)
            while i != -1:
                hits.append((i, i + len(w), side))
                i = low.find(w, i + 1)
    kept = [h for h in hits
            if not any(o != h and o[0] <= h[0] and h[1] <= o[1] and (o[1] - o[0]) > (h[1] - h[0])
                       for o in hits)]
    sides = {h[2] for h in kept}
    return "upper" if "upper" in sides else "lower" if "lower" in sides else None


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
    side = bound_side(s)
    if side == "upper":
        return _scale(s, Decimal("1.25"))
    if side == "lower":
        return _scale(s, Decimal("0.8"))
    return None


def d_numeric_tightening(s: str):
    """The reverse: fewer values comply."""
    side = bound_side(s)
    if side == "upper":
        return _scale(s, Decimal("0.8"))
    if side == "lower":
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


def _enumeration(s: str):
    """Split a statement around a "one of" enumeration the rules-v8 parser reads as `IN` (ADR 0030).

    Returns (prefix, members, suffix) or None. Only the delimited shapes whose list is unambiguous:
    `one of {a, b, c}` and `one of the following: a, b or c`. A counted header ("one of the four
    standards") is skipped: adding or removing a member would leave a stale count, which is a
    different change than the label would claim.
    """
    m = re.search(r"\bone of\s*\{([^}]*)\}", s, re.I)
    if m:
        members = [x.strip() for x in m.group(1).split(",") if x.strip()]
        return s[: m.start(1)], members, s[m.end(1):], "brace"
    m = re.search(r"\bone of the following:\s*(.+?)\s*(\.?)$", s.rstrip(), re.I)
    if m:
        body = m.group(1)
        parts = re.split(r",\s*(?:or\s+)?|\s+or\s+", body)
        members = [x.strip() for x in parts if x.strip()]
        return s[: m.start(1)], members, m.group(2), "following"
    return None


def _join(members: list[str], style: str) -> str:
    if style == "brace":
        return ", ".join(members)
    return ", ".join(members[:-1]) + " or " + members[-1]


def d_membership_narrowing(s: str):
    """One member removed from an enumerated "one of" set: fewer values comply."""
    e = _enumeration(s)
    if not e or len(e[1]) < 3:  # a set of two would shrink to a single value: not a set
        return None
    prefix, members, suffix, style = e
    return prefix + _join(members[:-1], style) + suffix


def d_membership_widening(s: str):
    """One member added to an enumerated "one of" set: more values comply."""
    e = _enumeration(s)
    if not e or len(e[1]) < 2:
        return None
    prefix, members, suffix, style = e
    return prefix + _join(members + ["an unlisted alternative"], style) + suffix


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

# v2 only: membership, appended after the v1 families so v1's order is untouched.
MEMBERSHIP = [
    ("MN", "membership_narrowing", d_membership_narrowing),
    ("MI", "membership_widening", d_membership_widening),
]

# The label of the same change read backwards. A pair swapped is the opposite mutation, so the label
# comes from this table (the mutation), never from the classifier.
OPPOSITE = {
    "modality_weakening": "modality_strengthening",
    "numeric_loosening": "numeric_tightening",
    "numeric_tightening": "numeric_loosening",
    "membership_narrowing": "membership_widening",
    "membership_widening": "membership_narrowing",
}


def mirror(pair):
    """(id, a, b, src, label, note) -> its mirror, or None when the label has no opposite."""
    pid, a, b, src, label, note = pair
    if label not in OPPOSITE:
        return None
    return (pid + "r", b, a, src, OPPOSITE[label], f"mirror of {pid} (a and b swapped): {note}")


def main(version: int = 1) -> int:
    sd_sha, sd = strictdoc_statements(V4_PINS["strictdoc"])
    ds_sha, ds = doorstop_statements(V4_PINS["doorstop"])
    pm_sha, pm, pm_total = promise_statements(V4_PINS["promise"])

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

    v1_changed = len(changed)
    membership = []
    if version >= 2:
        for src, sha, items in (("strictdoc", sd_sha, sd), ("doorstop", ds_sha, ds),
                                ("promise", pm_sha, pm)):
            tag = src[:2].upper()
            for uid, text in items:
                for code, label, fn in MEMBERSHIP:
                    out_s = fn(text)
                    if out_s and out_s != text:
                        membership.append((f"D-{tag}-{uid}-{code}", text, out_s,
                                           f"{src}@{sha[:12]} {uid}", label,
                                           (fn.__doc__ or code).strip().splitlines()[0]))
        base = changed + membership
        mirrors = [m for m in (mirror(p) for p in base) if m]
        changed = changed + membership + mirrors

    for group in (identical, changed):
        for pid, a, b, _src, _label, _note in group:
            assert a != b, f"{pid}: pair sides are identical"
    ids = [pid for pid, *_ in identical + changed]
    assert len(ids) == len(set(ids)), "duplicate pair ids"

    v2_note = ""
    if version >= 2:
        n_mirror = len(mirrors)
        if membership:
            m_note = f"{len(membership)} membership mutations on enumerated `one of` sets"
        else:
            m_note = ("0 membership mutations: the pinned sources hold no enumerated `one of {...}` "
                      "statement the rules-v8 parser reads as IN, and none was synthesised to reach "
                      "a number")
        v2_note = (
            f" Version 2 (S-1, ADR 0040 decision 5) = every v1 pair, plus {n_mirror} mirrors (the "
            "modality/numeric/membership pairs swapped and labelled with the opposite class, ids "
            f"suffixed `r`) and {m_note}. Measures false carries: a strengthening, tightening, "
            "flip or scope change reported as a weakening class.")
    out = ["schema: req.bench.corpus/0", f"version: {version}"]
    out.append("rules: " + yaml_scalar(
        "Direction gold set: every pair is a real requirement from another project (pinned "
        f"by commit — strictdoc@{sd_sha}, doorstop@{ds_sha}, promise-nfr@{pm_sha}, sampled "
        f"every 5th of {pm_total}) with one mutation applied, and the `direction` label IS "
        "that mutation. Labels are therefore an independent oracle for `req compare`, "
        "never the classifier's output. Vocabulary: identical | modality_weakening | "
        "modality_strengthening | polarity_flip | numeric_loosening | numeric_tightening | "
        "scope_narrowing | scope_widening | scope_divergence | unrelated. A misspelled "
        "label FAILS the gate rather than disabling the check." + v2_note))
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

    print(f"# identical={len(identical)} changed={len(changed)} (v1 families: {v1_changed}) "
          f"(sources: strictdoc={len(sd)}, doorstop={len(ds)}, promise={len(pm)}/{pm_total})",
          file=sys.stderr)
    return 0


def self_check() -> int:
    """The pure half, offline: membership mutations and mirrors."""
    s = "The report shall cite one of {ISO 9001, IEC 61508, IEC 62304}."
    assert d_membership_narrowing(s) == "The report shall cite one of {ISO 9001, IEC 61508}."
    assert d_membership_widening(s) == (
        "The report shall cite one of {ISO 9001, IEC 61508, IEC 62304, an unlisted alternative}.")
    f = "The report shall cite one of the following: ISO 9001, IEC 61508 or IEC 62304."
    assert d_membership_narrowing(f) == (
        "The report shall cite one of the following: ISO 9001 or IEC 61508.")
    assert d_membership_widening(f) == (
        "The report shall cite one of the following: ISO 9001, IEC 61508, IEC 62304 or "
        "an unlisted alternative.")
    assert d_membership_narrowing("The report shall cite one of {A, B}.") is None
    assert d_membership_narrowing("The report shall cite one of the four standards: A, B, C, D.") is None
    assert d_membership_narrowing("The product shall allow one of the target market countries.") is None
    # R-1: the longest phrase decides; the negated forms are the bound the English says.
    for text, side in (
        ("The system shall respond in no more than 3 seconds.", "upper"),
        ("The system shall respond in not more than 3 seconds.", "upper"),
        ("The system shall respond in no greater than 3 seconds.", "upper"),
        ("The system shall respond in not greater than 3 seconds.", "upper"),
        ("The system shall store no less than 3 records.", "lower"),
        ("The system shall store not less than 3 records.", "lower"),
        ("The system shall store no fewer than 3 records.", "lower"),
        ("The system shall respond more than 3 seconds.", "lower"),
        ("The system shall respond in less than 3 seconds.", "upper"),
        ("The system shall respond within 3 seconds.", "upper"),
        ("The system shall respond quickly.", None),
    ):
        assert bound_side(text) == side, (text, bound_side(text))
    assert d_numeric_loosening("The system shall respond in no more than 4 seconds.") == (
        "The system shall respond in no more than 5 seconds.")
    assert d_numeric_tightening("The system shall respond in no more than 5 seconds.") == (
        "The system shall respond in no more than 4 seconds.")
    assert d_numeric_loosening("The system shall store no less than 5 records.") == (
        "The system shall store no less than 4 records.")
    # ...and a number after a backslash is an escape, not a quantity.
    assert _scale("It shall render \\92s as a quote.", Decimal("2")) is None
    assert _scale("It shall respond in 92s.", Decimal("2")) == "It shall respond in 184s."
    m = mirror(("D-X-1-MW", "a", "b", "src", "modality_weakening", "n"))
    assert m == ("D-X-1-MWr", "b", "a", "src", "modality_strengthening",
                 "mirror of D-X-1-MW (a and b swapped): n")
    assert mirror(("D-X-1-PF", "a", "b", "src", "polarity_flip", "n")) is None
    print("build_direction self-check: ok", file=sys.stderr)
    return 0


if __name__ == "__main__":
    args = sys.argv[1:]
    if "--self-check" in args:
        raise SystemExit(self_check())
    ver = 1
    if "--version" in args:
        ver = int(args[args.index("--version") + 1])
        if ver not in (1, 2):
            raise SystemExit(f"unknown --version {ver}: 1 or 2")
    raise SystemExit(main(ver))
