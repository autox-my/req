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

**Derived parse**:
What a claim's statement parses to under the current rules — structure, modality, subject,
comparator, value, unit, conditions, lints — computed when the claim is loaded and stored nowhere
(ADR 0022). The author writes the statement; the parse is the tool's.
_Avoid_: slots, authored parse (the `req.claim/1` notion), cache

**Canonical form**:
The normalized representation of a claim's meaning — its derived parse plus its applicability —
from which its address is derived. Claims sharing a canonical form are the same claim.
_Avoid_: normalized text, canonical text

**Address**:
The digest of a claim's canonical form; the spelling of its **Meaning fingerprint**. Derived at load,
never stored in a claim file; a signed Baseline stores it per claim. It is not the claim's identity —
that is the **Claim ID**.
_Avoid_: identity, ID, key, hash (for the human label)

**Meaning fingerprint**:
What a claim's meaning hashes to under a stated rules version. It answers "has this meaning moved
since it was signed?" and "do these two claims look like one?"; it names nothing and refers to
nothing. A claim whose meaning changes keeps its Claim ID and gets a new fingerprint. Computed when
the claim is loaded, and compared against a signed manifest — never against a copy in the claim.
_Avoid_: identity, ID

**Tool operational requirement**:
A claim stating one observable behaviour of req itself that a user relies on — "report a gate invocation that leaves a claim
undecided as UNVERIFIABLE" — written as an ordinary claim, linked to the test that proves it, and gated like any other. REQ-0024 to
REQ-0040 are the first set. It is evidence that a behaviour is pinned by a test, not that the behaviour is sufficient for a use.
_Avoid_: tool spec, feature

**Pin**:
An author's statement of what a claim's derived parse must hold for one key (`modality`, `subject`,
`operator`, `value`, `unit`), where the parse is load-bearing. It agrees, or it is a flag — a
contradiction, or a value the parse does not carry; never an annotation. It moves no address.
_Avoid_: slot, annotation, assertion

**Claim ID**:
The stable identity of a claim and its human-facing label, deliberately separate from the address
so that continuity survives rewording and changed meaning. Relations, test links and Decisions
refer to it.
_Avoid_: address, hash

**False merge**:
Two different claims resolving to one address; it silently deletes a requirement.
_Avoid_: collision

**False split**:
One claim resolving to two addresses; noise, never danger.

**Uncovered content**:
A word of a claim's statement that no slot of its canonical form holds, so changing it would
move no address. A claim with any is unstructured, never silently structured.
_Avoid_: dropped word, ignored text (and coverage, which the audit packet already uses for
whether a claim still matches a signed Baseline)

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

**Scope narrowing**:
An edit that adds a condition, so the claim applies less often. On an obligation or a prohibition it
is a weakening — the claim binds in fewer situations (ADR 0041); its mirror is scope widening.
_Avoid_: tightening (for an added condition)

**Numeric loosening**:
An edit that widens a bound, such as 200 ms to 500 ms.

**Membership**:
An obligation to choose from an enumerated set — "name one of the four sources: A, B, C or D".
Its operator is `IN` and its members are a set, so reordering them is cosmetic, removing one is a
narrowing and adding one a widening (ADR 0030). An unenumerated "one of" is not a membership.

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

**Structural omission**:
A gap req can prove from its own files: a claim no declared release examines, one no Baseline
covers, or one with no relation. Reported by name; it never certifies the requirements complete,
because req holds nothing to be complete against (ADR 0039).
_Avoid_: completeness (as a verdict), coverage gap

**Satisfaction link**:
A design element's authored statement that it meets a claim, recorded with the claim's statement
at the moment a human ratified it. It holds while the meaning is unchanged or weakened in a class
measured never to hide a strengthening, and is suspect otherwise (ADR 0040).
_Avoid_: trace link (a relation is claim to claim), coverage

**False carry**:
A satisfaction link left holding across a change that was in fact a strengthening — the
satisfaction analogue of a false merge, held to zero on the direction gold set (ADR 0040).

### Authority

**Authoring**:
Changes made before a decision; deliberately friction-free.

**Decision boundary**:
The point at which a change acquires authority; deliberately high-friction and auditable.
