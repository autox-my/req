#!/bin/sh
# examples/propose-provider.sh — a stand-in provider for `req propose`, to show the contract.
#
# It calls no model and no network: it decides from the `DIFF:` line alone. A real provider is
# YOUR process boundary (ADR 0021) — whatever you put behind `--provider-cmd`; req neither
# ships one nor trusts one. This script only shows what crosses the boundary.
#
# What `req propose` writes to this script's stdin (one pair of claims per run):
#   ID: REQ-0001            the first claim's id
#   A: <statement A>
#   B: <statement B>
#   DIFF: w1|w2|...         the words that are in one statement and not the other, sorted
#   CHOOSE: equivalent|related|unrelated
#
# What it must print on stdout: a label (equivalent, related or unrelated), or a JSON object
# with `choice` and, optionally, `confidence` (an integer percent). Anything else, or a
# non-zero exit, is UNVERIFIABLE — req never reads a silent provider as "unrelated".
#
#   req propose claims/REQ-0001.yml claims/REQ-0002.yml --provider-cmd examples/propose-provider.sh
#
# The rule below is deliberately timid: two statements made of the same words in the same
# set are "equivalent"; any word that differs is only "related". It never says "unrelated",
# because a stand-in that cannot read meaning has no business ruling a pair out. Whatever it
# says, the answer is a proposal: a human ratifies, and nothing is written.

diff=""
while IFS= read -r line; do
  case "$line" in
    "DIFF: "*) diff="${line#DIFF: }" ;;
    "DIFF:") diff="" ;;
  esac
done

if [ -z "$diff" ]; then
  printf '{"choice": "equivalent", "confidence": 60}\n'
else
  printf '{"choice": "related", "confidence": 60}\n'
fi
