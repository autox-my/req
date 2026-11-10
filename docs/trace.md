<!-- req projection: generator=req trace | rules_v=8 | selection=corpus_set=parity_and_adversarial deployment_mode=air_gapped field=value set=direction_gold_structural | claims=38 | identity=sha256:ff2928a83079efa5760aed2fbfeffafeb15c0b53ec72d439e6040cc9ed84e62b -->
# Traceability: corpus_set=parity_and_adversarial, deployment_mode=air_gapped, field=value, set=direction_gold_structural
edges are stored once, on the outgoing side (REQ-0020); the incoming
view below is derived by traversal, never stored

## Edges
- REQ-0002 -derives-from-> REQ-0001
- REQ-0003 -derives-from-> REQ-0001
- REQ-0004 -refines-> REQ-0001
- REQ-0005 -derives-from-> REQ-0004
- REQ-0006 -derives-from-> REQ-0004
- REQ-0007 -refines-> REQ-0004
- REQ-0008 -derives-from-> REQ-0005
- REQ-0009 -refines-> REQ-0008
- REQ-0010 -refines-> REQ-0008
- REQ-0012 -derives-from-> REQ-0007
- REQ-0013 -refines-> REQ-0004
- REQ-0014 -derives-from-> REQ-0013
- REQ-0014 -refines-> REQ-0002
- REQ-0015 -refines-> REQ-0014
- REQ-0016 -refines-> REQ-0003
- REQ-0017 -refines-> REQ-0007
- REQ-0018 -refines-> REQ-0017
- REQ-0019 -refines-> REQ-0007
- REQ-0020 -derives-from-> REQ-0002
- REQ-0021 -derives-from-> REQ-0004
- REQ-0022 -refines-> REQ-0020
- REQ-0023 -refines-> REQ-0004
- REQ-0024 -refines-> REQ-0016
- REQ-0031 -derives-from-> REQ-0002

## Incoming (derived)
- REQ-0001 <- derives-from: REQ-0002, derives-from: REQ-0003, refines: REQ-0004
- REQ-0002 <- refines: REQ-0014, derives-from: REQ-0020, derives-from: REQ-0031
- REQ-0003 <- refines: REQ-0016
- REQ-0004 <- derives-from: REQ-0005, derives-from: REQ-0006, refines: REQ-0007, refines: REQ-0013, derives-from: REQ-0021, refines: REQ-0023
- REQ-0005 <- derives-from: REQ-0008
- REQ-0007 <- derives-from: REQ-0012, refines: REQ-0017, refines: REQ-0019
- REQ-0008 <- refines: REQ-0009, refines: REQ-0010
- REQ-0013 <- derives-from: REQ-0014
- REQ-0014 <- refines: REQ-0015
- REQ-0016 <- refines: REQ-0024
- REQ-0017 <- refines: REQ-0018
- REQ-0020 <- refines: REQ-0022

## Findings
(none)
