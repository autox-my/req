#!/usr/bin/env python3
"""Validate a Tier-2 pilot worksheet, filled or not.

The worksheet is the reviewable surface for the human-labelled sample, so a worksheet the analysis
cannot interpret has to be rejected at the boundary rather than discovered halfway through a
measurement. This checks the schema, the fields, the label vocabulary, and the two structural rules
that make a pair a Tier-2 candidate at all: the two statements must cross requirement ids, and the
recorded `shared_count` must match the terms actually listed.

Outcomes are tri-state, as everywhere in this project:
  0  valid
  1  invalid — a concrete finding, named per pair
  3  UNVERIFIABLE — the file cannot be read (PyYAML missing, or the YAML does not parse)

Exit 3 is not a pass. A worksheet nobody could load is a measurement problem.

Usage:
    python3 tools/tier2/validate.py --worksheet bench/corpus/tier2-pilot-v0.yml
    python3 tools/tier2/validate.py --worksheet filled-by-alice.yml --filled
    python3 tools/tier2/validate.py --self-check
"""

import argparse
import sys
import tempfile

SCHEMA = "req.tier2.pilot/0"
LABELS = ("equivalent", "related", "unrelated")
TIERS = ("A", "B", "C", "P")
# `undecided` is a first-class outcome, not a missing value. The protocol asks a labeller to record a
# pair they cannot decide; a vocabulary that admits only three labels would push them to guess, and a
# guess is worse than an admitted unknown — the same rule the engine applies everywhere else.
OUTCOMES = LABELS + ("undecided",)
FIELDS = ("id", "tier", "source", "a_id", "b_id", "shared_count", "jaccard_pm", "shared_terms",
          "a", "b", "label", "labeller", "confidence", "note")
# tier P additionally requires constructed_label; checked below rather than in FIELDS, because the
# pilot tiers do not have one.


def yaml_module():
    try:
        import yaml
        return yaml
    except ImportError:
        return None


def validate(path, filled):
    """Return (code, findings, pairs)."""
    yaml = yaml_module()
    if yaml is None:
        return 3, ["PyYAML is not available, so no worksheet can be read at all"], []
    try:
        with open(path) as handle:
            doc = yaml.safe_load(handle)
    except Exception as error:  # noqa: BLE001 — the reason is the finding
        return 3, ["cannot parse %s: %s" % (path, error)], []
    if not isinstance(doc, dict):
        return 1, ["%s is not a mapping" % path], []
    if doc.get("schema") != SCHEMA:
        return 1, ["schema must be %s, got %r" % (SCHEMA, doc.get("schema"))], []
    pairs = doc.get("pairs")
    if not isinstance(pairs, list) or not pairs:
        return 1, ["no pairs in %s" % path], []

    findings, seen = [], set()
    for index, pair in enumerate(pairs, 1):
        if not isinstance(pair, dict):
            findings.append("pair %d is not a mapping" % index)
            continue
        where = str(pair.get("id") or "pair %d" % index)
        for key in FIELDS:
            if key not in pair:
                findings.append("%s: missing field %s" % (where, key))
        if where in seen:
            findings.append("%s: duplicate id" % where)
        seen.add(where)
        if pair.get("tier") not in TIERS:
            findings.append("%s: tier must be one of %s, got %r" % (where, "|".join(TIERS), pair.get("tier")))
        if pair.get("tier") == "P":
            # A positive pair is one requirement in two renderings, by construction, so the cross-id rule
            # does not apply; what must be present is the author's claim about the pair.
            if str(pair.get("constructed_label") or "").strip() not in ("equivalent", "related"):
                findings.append("%s: a tier-P pair needs constructed_label equivalent|related, got %r"
                                % (where, pair.get("constructed_label")))
        elif pair.get("a_id") and pair.get("a_id") == pair.get("b_id"):
            findings.append("%s: both statements carry requirement id %r — pairing within one id "
                            "reproduces the source corpus's own Tier-1 transforms" % (where, pair.get("a_id")))
        listed = [t for t in str(pair.get("shared_terms") or "").split("|") if t]
        if listed and len(listed) != pair.get("shared_count"):
            findings.append("%s: shared_count %r does not match the %d term(s) listed"
                            % (where, pair.get("shared_count"), len(listed)))
        label = str(pair.get("label") or "").strip()
        if label and label not in OUTCOMES:
            findings.append("%s: label %r is not one of %s" % (where, label, "|".join(LABELS)))
        if filled:
            if not label:
                findings.append("%s: no label — write a label, or `undecided` if you cannot decide"
                                % where)
            if not str(pair.get("labeller") or "").strip():
                findings.append("%s: no labeller" % where)
            confidence = str(pair.get("confidence") or "").strip()
            if not confidence.isdigit() or not 0 <= int(confidence) <= 100:
                findings.append("%s: confidence must be an integer 0..100, got %r"
                                % (where, pair.get("confidence")))
    return (1 if findings else 0), findings, pairs


def write_worksheet(path, labels):
    rows = []
    for index, label in enumerate(labels, 1):
        rows.append("  - id: \"PILOT-A-%02d\"\n"
                    "    tier: \"A\"\n"
                    "    source: \"self@check\"\n"
                    "    a_id: \"R1\"\n"
                    "    b_id: \"R2\"\n"
                    "    shared_count: 2\n"
                    "    jaccard_pm: 500\n"
                    "    shared_terms: \"alpha|beta\"\n"
                    "    a: \"alpha beta\"\n"
                    "    b: \"alpha beta gamma\"\n"
                    "    label: \"%s\"\n"
                    "    labeller: \"%s\"\n"
                    "    confidence: \"%s\"\n"
                    "    note: \"\"\n"
                    % (index, label[0], label[1], label[2]))
    with open(path, "w") as handle:
        handle.write("schema: %s\npairs:\n%s" % (SCHEMA, "".join(rows)))


def self_check():
    if yaml_module() is None:
        print("self-check SKIPPED: PyYAML is not available (this is not a pass)")
        return 77
    with tempfile.TemporaryDirectory() as tmp:
        unfilled = tmp + "/unfilled.yml"
        write_worksheet(unfilled, [("", "", ""), ("", "", "")])
        code, findings, _ = validate(unfilled, filled=False)
        assert code == 0, findings
        code, findings, _ = validate(unfilled, filled=True)
        assert code == 1 and any("no label" in f for f in findings), findings

        good = tmp + "/good.yml"
        write_worksheet(good, [("equivalent", "alice", "90"), ("undecided", "alice", "20")])
        code, findings, _ = validate(good, filled=True)
        assert code == 0, findings

        bad = tmp + "/bad.yml"
        write_worksheet(bad, [("probably", "alice", "90"), ("related", "", "150")])
        code, findings, _ = validate(bad, filled=True)
        assert code == 1, findings
        assert any("not one of" in f for f in findings), findings
        assert any("no labeller" in f for f in findings), findings
        assert any("confidence" in f for f in findings), findings

        broken = tmp + "/broken.yml"
        with open(broken, "w") as handle:
            handle.write("schema: [unclosed\n")
        code, _, _ = validate(broken, filled=False)
        assert code == 3, code
    print("self-check PASS: valid unfilled; missing labels refused when filled; `undecided` accepted "
          "as an admitted unknown; bad vocabulary and bad confidence refused; unparsable reported "
          "UNVERIFIABLE")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--worksheet")
    parser.add_argument("--filled", action="store_true",
                        help="require a label, a labeller and a confidence on every pair")
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        return self_check()
    if not args.worksheet:
        print("--worksheet is required (or --self-check)")
        return 2
    code, findings, pairs = validate(args.worksheet, args.filled)
    if code == 3:
        print("UNVERIFIABLE — %s" % findings[0])
        return 3
    for finding in findings:
        print("INVALID — %s" % finding)
    print("%s: %d pair(s)%s" % (args.worksheet, len(pairs),
                                " valid" if code == 0 else " — %d finding(s)" % len(findings)))
    return code


if __name__ == "__main__":
    sys.exit(main())
