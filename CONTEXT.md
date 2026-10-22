# req

A requirements engine whose source of truth is a semantic claim graph, serialized into a
git repository. It exists so that specifications, traceability, and audit packets are
*compiled on demand* from a signed baseline instead of maintained by hand.

## Language

### The unit

**Claim**:
An atomic assertion that can be true or false; the unit of authorship, review, and merge.
_Avoid_: requirement (when the unit is meant), statement, item, record

**Requirement**:
A claim whose modality imposes obligation — the SHALL subset of the graph.
_Avoid_: user story, ticket, issue, epic

**Canonical form**:
The normalized representation of a claim's meaning, from which its address is derived.
Claims sharing a canonical form are the same claim.
_Avoid_: normalized text, canonical text

**Address**:
A claim's identity — the digest of its canonical form.
_Avoid_: ID, key, hash (for the human label)

**Claim ID**:
The stable human-facing label, deliberately separate from the address so that continuity
survives rewording.
_Avoid_: address, hash

**False merge**:
Two different claims resolving to one address; it silently deletes a requirement.
_Avoid_: collision

**False split**:
One claim resolving to two addresses; noise, never danger.

### Structure

**Relation**:
A typed, directed edge between claims — `derives-from`, `refines`, `satisfies`,
`verifies`, `conflicts-with`, `supersedes`, `allocated-to`.
_Avoid_: link, trace

**Rationale**:
The justification for a claim: its source, date, rejected alternatives, and assumptions.
_Avoid_: description, note, comment

**Decision**:
A recorded accept, reject, or merge event carrying authority, role, meaning, and
timestamp, together with the dissent and the road not taken.
_Avoid_: approval (one kind of decision), sign-off

**Evidence**:
A link from a claim to the artifact that supports it — a test run, an analysis, an
inspection.
_Avoid_: proof, attachment

**Linked test**:
A test a claim names as the test that proves it; authored on the claim, attested in the
signed Baseline, and never part of the claim's identity.
_Avoid_: proof (the test is evidence; the link is a claim about it), coverage

**Process evidence**:
An authored declaration that a claim is established by the process itself — a Decision, a
signature, a workflow — rather than by a linked test; reported as declared, never
machine-proven, and never inferred from an empty test list.
_Avoid_: machine-proven, verified (no test ran)

**Baseline**:
The claim addresses a human has ratified for **one variant**, with the rules version and
the selection they were ratified under. A projection over the graph, not the graph itself.
_Avoid_: version, release, snapshot

**Conflict**:
Two claims that cannot both hold in the same scope, recorded as a first-class object that
preserves the losing side. It is information, not an error.
_Avoid_: merge conflict, clash, error

**Proposal**:
A change to a claim that has not yet been accepted; what an edit from a non-engineer
produces.
_Avoid_: draft, edit

**Working set**:
The claims a person is actively authoring or reviewing, scoped so their edits become
proposals rather than direct writes.
_Avoid_: branch, checkout

### Variation

**Variant**:
A permanently coexisting configuration of the product, expressed as a selection over the
claim graph.
_Avoid_: branch, fork, version

**Applicability predicate**:
The condition under which a claim belongs to a variant; a claim carries one or more.
_Avoid_: flag, tag

**150% model**:
The graph holding every claim for every variant, each guarded by its applicability
predicate; a variant's specification is a query over it.
_Avoid_: master, superset

**Tri-state scope**:
Applicability resolves to TRUE, FALSE, or UNKNOWN — never silently FALSE, because FALSE
would drop a safety claim out of a variant.
_Avoid_: boolean, included/excluded

### Change

**Direction of change**:
The classification of an edit as strengthening, weakening, or lateral; computable only
from the canonical layer.
_Avoid_: diff, delta

**Modality weakening**:
An edit that drops obligation — SHALL to MAY or SHOULD. The most dangerous class.
_Avoid_: softening, downgrade

**Numeric loosening**:
An edit that widens a bound, such as 200 ms to 500 ms.

**Cosmetic change**:
An edit with the same canonical form as its predecessor; auto-mergeable.

### Views and enforcement

**View**:
A document compiled from the graph for a particular audience — a specification,
traceability matrix, safety case, or audit packet.
_Avoid_: document (when the source is meant), spec

**Audience**:
The reader a view is compiled for — engineer, auditor, regulator, or product manager.
_Avoid_: persona, stakeholder

**Gate**:
A CI check that fails a build when claims in scope lack required evidence.
_Avoid_: lint, validation

**Live trace**:
Traceability answered as a query over relations and evidence, never as a maintained
artifact.
_Avoid_: traceability matrix

### Authority

**Authoring**:
Changes made before a decision; deliberately friction-free.

**Decision boundary**:
The point at which a change acquires authority; deliberately high-friction and auditable.
