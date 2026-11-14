//! Claim model + corpus/registry payloads, read with `yaml_serde`.
//!
//! Field names and semantics follow [docs/claim-format.md] exactly. `value` stays a
//! `String` (the exact decimal string, never a float — claim-format authoring rule).
//! req-satisfies: REQ-0032 — ClaimFile and its nested structs deny unknown fields

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The nested structs are strict too, and every key is required. `deny_unknown_fields`
/// on `ClaimFile` alone leaves the same silent-loss class one level down; so does a
/// defaulted list, because the rationale sits outside the canonical form: a dropped
/// or misspelled `rejected_alternatives:` would leave the address untouched and lose
/// the record, and nothing in the tooling reads the field to notice.
///
/// Its serialized bytes are attested in signed manifests (ADR 0013), so its fields must remain
/// serde_json-encodable: `String` and `Vec<String>` encode infallibly, and
/// [`rationale_digest`] `expect`s on that — a future field type serde cannot encode would turn
/// an attestation call into a panic rather than a typed error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rationale {
    pub source: String,
    pub date: String,
    pub rejected_alternatives: Vec<String>,
    pub assumptions: Vec<String>,
}

/// Its serialized bytes are attested in signed manifests (ADR 0013), so its fields must remain
/// serde_json-encodable — the same invariant [`Rationale`] carries; [`relations_digest`]
/// `expect`s on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relation {
    #[serde(rename = "type")]
    pub kind: String,
    pub to: String,
}

// ---------------------------------------------------------------------------
// The audit-surface hashes (ADR 0013)
// ---------------------------------------------------------------------------

/// The domain prefix that keeps the relations digest from ever trading hashes with another
/// surface — a named constant, like [`crate::canon::ADDRESS_TAG`] and [`crate::decision::DOMAIN`],
/// so the domain-separation registry stays greppable in one style and a future surface cannot
/// misspell or omit a prefix.
pub const RELATIONS_TAG: &[u8] = b"req.relations.v1\0";

/// The rationale's domain prefix, for the same reason.
pub const RATIONALE_TAG: &[u8] = b"req.rationale.v1\0";

/// The authored test links' domain prefix (ADR 0019), for the same reason: the tests digest must
/// never trade hashes with the relations or rationale surfaces it sits beside.
pub const TESTS_TAG: &[u8] = b"req.tests.v1\0";

/// The relations as the manifest attests them: `sha256(RELATIONS_TAG ‖ serde_json::to_string(
/// relations))` — **plain serde JSON** of the typed struct, fields in declaration order. Not
/// `canon::canonical_json`, which normalizes numbers and strings and produces different bytes;
/// naming the writer here is what makes the byte format unambiguous to a re-implementation.
/// Serialization of a typed struct is deterministic, so no hand-ordered writer is needed.
pub fn relations_digest(relations: &[Relation]) -> String {
    crate::digest::sha256_hex_prefixed(
        RELATIONS_TAG,
        serde_json::to_string(relations)
            .expect("Relation serializes infallibly")
            .as_bytes(),
    )
}

/// The rationale as the manifest attests them: `sha256(RATIONALE_TAG ‖ serde_json::to_string(
/// rationale))`, the same plain serde JSON — provenance is part of what was signed.
pub fn rationale_digest(rationale: &Rationale) -> String {
    crate::digest::sha256_hex_prefixed(
        RATIONALE_TAG,
        serde_json::to_string(rationale)
            .expect("Rationale serializes infallibly")
            .as_bytes(),
    )
}

/// How a claim's evidence is established (ADR 0019).
///
/// **The default is `machine`, and that default is load-bearing.** A claim with no `tests:`
/// entries is a claim whose evidence is *absent* — it must be reported, never read as
/// process-evidenced. A process declaration is explicit and authored; an empty list can never
/// imply one. The gate's mutation note records the check that proves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Evidence {
    /// A linked test in the report proves the claim.
    #[default]
    Machine,
    /// The claim is established by the process itself (a Decision, a signature, a workflow),
    /// declared by a human. Reported PASS-as-declared, named per claim, and counted — never
    /// shrunk into the machine count.
    Process,
}

impl Evidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Evidence::Machine => "machine",
            Evidence::Process => "process",
        }
    }
}

/// The linked tests as the manifest attests them: `sha256(TESTS_TAG ‖ serde_json::to_string(
/// {evidence, tests}))` — **plain serde JSON** of the declared evidence mode and the sorted
/// test ids, the same writer as [`relations_digest`], so the byte format is unambiguous to a
/// re-implementation.
///
/// The sort is the whole point of the separation: a claim's `tests:` list is **authored and
/// order-free**, and hashing it in document order would let a meaningless reordering of two
/// test ids change what the Baseline attests. Sorted at hash time, never written into the
/// canonical form — a test rename must not move the claim's address (ADR 0019).
///
/// The evidence mode rides the same digest because it is part of the same declaration: without
/// it, flipping `machine` to `process` (and dropping the links) would leave both the address and
/// the attested list untouched, which is precisely the quiet unlink the Baseline exists to catch.
pub fn tests_digest(tests: &[String], evidence: Evidence) -> String {
    #[derive(Serialize)]
    struct EvidenceSurface<'a> {
        evidence: &'a str,
        tests: Vec<&'a String>,
    }
    let mut sorted: Vec<&String> = tests.iter().collect();
    sorted.sort();
    let surface = EvidenceSurface {
        evidence: evidence.as_str(),
        tests: sorted,
    };
    crate::digest::sha256_hex_prefixed(
        TESTS_TAG,
        serde_json::to_string(&surface)
            .expect("the evidence surface serializes infallibly")
            .as_bytes(),
    )
}

/// Validate a claim's authored evidence declaration: its test links and the mode they sit under.
///
/// Three file-contract rules, not identity rules, and the loader is where they are enforced:
/// a `tests:` entry that is empty names nothing while looking like a link (the gate would read it
/// as a test absent from the report, which is the right verdict for a rename but the wrong one for
/// a typo); a duplicate makes the attested digest depend on a repetition that means nothing; and a
/// **process declaration that also links tests** is a contradiction — the evidence is machine or
/// process, never both (ADR 0019). The check is deliberately *not* a normalizer: an id is compared
/// exactly as authored, so a whitespace variant is a link to a test that does not exist rather
/// than a silent match.
pub fn validate_tests(tests: &[String], evidence: Evidence) -> Result<(), String> {
    // A process declaration and machine links are two different answers to the same question.
    // Refusing the contradiction here keeps a claim from being counted as process-evidenced
    // while carrying links nobody runs, or the reverse.
    if evidence == Evidence::Process && !tests.is_empty() {
        return Err(
            "declares `evidence: process` and also links tests: a claim's evidence is machine \
             (proven by its linked tests) or process (declared by a human), never both — remove \
             one of them"
                .to_string(),
        );
    }
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    for test in tests {
        if test.trim().is_empty() {
            return Err(
                "has a `tests:` entry that is empty: an empty id names no test — remove it or \
                 write the test id the report carries"
                    .to_string(),
            );
        }
        if seen.insert(test.as_str(), ()).is_some() {
            return Err(format!(
                "lists the test id {test:?} twice: the link is a set, and a repetition would \
                 change the attested `tests_hash` without naming another test"
            ));
        }
    }
    Ok(())
}

/// The format that removed [`REMOVED_V1_KEYS`]. A fact about history, not about the build: a `/3` file
/// carrying one of them is told which format dropped it.
pub const REMOVED_V1_KEYS_IN: &str = "req.claim/2";

/// The keys `req.claim/1` carried and `req.claim/2` removed (ADR 0022): the eleven authored
/// parse slots that re-stated the statement, and the two stored derived fields (thirteen keys). A `req.claim/2` file that
/// still carries one is a **parse error naming the key** — never silently ignored, because the
/// author who wrote `value: "0.3"` beside the statement meant something, and the place to say it
/// now is `pins:`.
pub const REMOVED_V1_KEYS: &[&str] = &[
    "modality",
    "subject",
    "attribute",
    "operator",
    "value",
    "unit",
    "conditions",
    "exception",
    "temporal",
    "quantifier",
    "unstructured",
    "canonical",
    "address",
];

/// What an author states a claim **must parse to**, where the parse is load-bearing (ADR 0022).
///
/// Optional, and every key in it is optional. A pin is judged against the parse derived at load
/// ([`crate::canon::evaluate_pins`]): it agrees, or it is a flag. There is no third,
/// non-flagging category — the qualitative annotation `req.claim/1` had is exactly what hid
/// REQ-0020's defect. A pin never enters the canonical form, so a flagged pin moves no address:
/// it reports, and `req gate`'s `pins_hold` fails on it.
///
/// Strict: any other key is a parse error. The values are validated at load
/// ([`validate_pins`]) — `value` is a canonical exact decimal string, as everywhere in a claim.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    /// `SHALL`, `SHALL-NOT`, `SHOULD` or `MAY`.
    #[serde(default)]
    pub modality: Option<String>,
    /// The registered subject id the statement's subject must resolve to.
    #[serde(default)]
    pub subject: Option<String>,
    /// `LT`, `LE`, `EQ`, `GE`, `GT`, `NE` or `IN`.
    #[serde(default)]
    pub operator: Option<String>,
    /// The bound, as a canonical exact decimal string in the parse's canonical unit (`"0.2"`
    /// for `200 ms`; never `"0.20"`, never a float).
    #[serde(default)]
    pub value: Option<String>,
    /// The canonical unit symbol the parse derives (`s` for `200 ms`), or `none`.
    #[serde(default)]
    pub unit: Option<String>,
}

impl Pins {
    /// The pinned keys in a fixed order, with their values: the order every report uses.
    pub fn entries(&self) -> Vec<(&'static str, &str)> {
        [
            ("modality", &self.modality),
            ("subject", &self.subject),
            ("operator", &self.operator),
            ("value", &self.value),
            ("unit", &self.unit),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.as_deref().map(|v| (key, v)))
        .collect()
    }
}

/// The modality words a pin may state — the canonicalizer's own spellings.
pub const PIN_MODALITIES: &[&str] = &["SHALL", "SHALL-NOT", "SHOULD", "MAY"];
/// The comparators a pin may state — the canonicalizer's own spellings (T1.5).
pub const PIN_OPERATORS: &[&str] = &["LT", "LE", "EQ", "GE", "GT", "NE", "IN"];

/// The file contract for `pins:` values. A pin that cannot be compared — a misspelled modality,
/// an operator outside T1.5, a non-canonical decimal — is refused where the file can be named,
/// rather than becoming a contradiction flag whose cause is a typo.
pub fn validate_pins(pins: &Pins) -> Result<(), String> {
    if let Some(m) = &pins.modality
        && !PIN_MODALITIES.contains(&m.as_str())
    {
        return Err(format!(
            "pins a modality {m:?}, which is not one of {}",
            PIN_MODALITIES.join(", ")
        ));
    }
    if let Some(o) = &pins.operator
        && !PIN_OPERATORS.contains(&o.as_str())
    {
        return Err(format!(
            "pins an operator {o:?}, which is not one of {}",
            PIN_OPERATORS.join(", ")
        ));
    }
    if let Some(v) = &pins.value
        && !crate::canon::is_canonical_decimal(v)
    {
        return Err(format!(
            "pins a value {v:?}, which is not a canonical exact decimal string (`\"0.3\"`, \
             never `\"0.30\"` or `\".3\"`)"
        ));
    }
    for (key, value) in [("subject", &pins.subject), ("unit", &pins.unit)] {
        if value
            .as_deref()
            .is_some_and(|v| v.trim().is_empty() || v.trim() != v)
        {
            return Err(format!(
                "pins an empty or space-padded {key}: a pin is compared exactly"
            ));
        }
    }
    Ok(())
}

/// A claim file, strictly (`req.claim/2` and `/3`, ADR 0022 and 0051): what a human **authors**, and
/// nothing derived. The parse, the canonical form and the address are computed at load
/// ([`crate::canon::load_claims`]) and stored nowhere.
///
/// An unrecognized key is a parse error (F2) — a removed `req.claim/1` key names itself
/// ([`REMOVED_V1_KEYS`]) — and `applicability` / `relations` are required rather than defaulted
/// to empty: a dropped `relations:` key leaves the address untouched while the edges vanish from
/// `trace` and `impact`, and a misspelled `applicability:` silently makes a variant-scoped claim
/// unconditional (applicability is an input to the canonical form, so that is a conflation of two
/// requirements, not a wrong variant membership).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimFile {
    pub schema: String,
    pub claim_id: String,
    pub statement: String,
    pub applicability: Vec<String>,
    pub relations: Vec<Relation>,
    /// The tests that prove this claim, authored by a human: `[classname::name]` ids as the
    /// JUnit report carries them. **Optional** (absent = no link, which the gate reports as a
    /// finding), **order-free** (hashed sorted), **attested** in the Baseline (`tests_hash`,
    /// envelope v4) and **never part of identity** — the canonical form does not carry it, so
    /// adding, removing or renaming a link moves no address (ADR 0019). A rename is therefore
    /// visible as "linked test not in the report", never as a pass.
    #[serde(default)]
    pub tests: Vec<String>,
    /// How this claim's evidence is established: `machine` (the default — a linked test in the
    /// report must pass) or `process` (declared by a human; reported PASS-as-declared, named and
    /// counted, never inferred from an empty `tests:` list). Authored, outside identity, and
    /// attested together with the links (`tests_hash`, ADR 0019).
    #[serde(default)]
    pub evidence: Evidence,
    pub rationale: Rationale,
    /// What the claim must parse to, where the parse is load-bearing (optional; ADR 0022).
    #[serde(default)]
    pub pins: Option<Pins>,
    /// The one place the claim is filed in the reader's layout (`req.claim/3`, ADR 0051): a path of
    /// stable keys, `diagnostics/startup`. **Authored, optional and outside identity** — not an
    /// input to the canonical form and in no attested hash, so re-filing a claim moves no address
    /// and needs no signature. Judged at load by [`validate_area`] and, because only `req.claim/3`
    /// carries it, by [`crate::canon::parse_claim`] against the declared format.
    ///
    /// Not serialised when absent: the serialisation of a claim without an area is the one it had
    /// before the key existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
}

/// The first claim file format that carries [`ClaimFile::area`] (ADR 0051 decision 6). A file that
/// declares an earlier format and still carries the key is a parse error naming it.
pub const AREA_SINCE_FORMAT: u32 = 3;

/// The file contract for an area path (ADR 0051 decision 2), the one grammar `.req/areas` shares:
/// segments joined by `/`, each `[a-z0-9][a-z0-9-]*`, with no empty segment and no leading or
/// trailing `/`. A segment is a stable **key** (it must survive the source renumbering a section), so
/// the alphabet is ASCII and lower case: a reader-facing name is the registry's label, not the key.
///
/// The error says what is wrong with the value and not which claim or file holds it: the caller
/// names that, as every other file-contract check here does.
pub fn validate_area(area: &str) -> Result<(), String> {
    let well_formed = !area.is_empty()
        && area.split('/').all(|segment| {
            let mut chars = segment.chars();
            chars
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    if well_formed {
        Ok(())
    } else {
        Err(format!(
            "{area:?} is not an area path: segments joined by `/`, each `[a-z0-9][a-z0-9-]*`, with \
             no empty segment and no leading or trailing `/` (for example `diagnostics/startup`). A \
             segment is a stable key, not a heading or a section number; its reader-facing name is \
             the label in `.req/areas`"
        ))
    }
}

// ---------------------------------------------------------------------------
// aliases/subjects.yml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectDef {
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryFile {
    pub schema: String,
    pub version: u8,
    #[serde(default)]
    pub rules: Option<String>,
    pub subjects: BTreeMap<String, SubjectDef>,
}

/// Flattened alias map with the registry's own disjointness rule enforced
/// (each surface alias must resolve to exactly one subject).
pub struct Registry {
    pub version: u8,
    alias_to_subject: BTreeMap<String, String>,
}

impl Registry {
    /// An empty registry for degraded runs (the canonicalizer resolves surfaces to
    /// literals); the fact of the degradation must be recorded by the caller.
    pub fn empty() -> Self {
        Self {
            version: 0,
            alias_to_subject: BTreeMap::new(),
        }
    }

    /// `load` errors are kept distinct from an empty registry: a failed load must
    /// never look like a successful empty one (REQ-0008's three-state rule).
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        Self::parse(&text, path)
    }

    /// Parse a registry from text already in hand; `origin` only names it in an error.
    pub fn parse(text: &str, path: &str) -> Result<Self, String> {
        let file: RegistryFile =
            yaml_serde::from_str(text).map_err(|e| format!("parse {path}: {e}"))?;
        let mut alias_to_subject = BTreeMap::new();
        for (subject_id, def) in &file.subjects {
            for alias in &def.aliases {
                let key = alias.to_lowercase();
                if let Some(existing) = alias_to_subject.get(&key)
                    && existing != subject_id
                {
                    return Err(format!(
                        "alias collision in {path}: {key:?} resolves to both {existing} and {subject_id}"
                    ));
                }
                alias_to_subject.insert(key, subject_id.clone());
            }
        }
        Ok(Self {
            version: file.version,
            alias_to_subject,
        })
    }

    /// Resolve a surface phrase. Unregistered surfaces are NOT collapsed — they
    /// return as the literal normalized surface (hashing spec T1.7).
    pub fn resolve(&self, surface: &str) -> String {
        let key = surface.trim().to_lowercase();
        self.alias_to_subject.get(&key).cloned().unwrap_or(key)
    }
}

// ---------------------------------------------------------------------------
// bench/corpus/parity.yml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pair {
    pub id: String,
    pub a: String,
    pub b: String,
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Expected change direction, for a corpus that is a **direction gold set**: the
    /// label names the mutation that produced `b`, so it is an independent oracle for
    /// `req compare` in exactly the way `not_equivalent` is for identity. A misspelled
    /// label fails the gate rather than disabling the check.
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Corpus {
    pub schema: String,
    pub version: u32,
    #[serde(default)]
    pub rules: Option<String>,
    pub equivalent: Vec<Pair>,
    pub not_equivalent: Vec<Pair>,
}

impl Corpus {
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
        Self::parse(&text, path)
    }

    /// Parse a corpus from text already in hand; `origin` only names it in an error. `req selftest`
    /// reads the parity corpus this binary was built with, not a file on this machine.
    pub fn parse(text: &str, origin: &str) -> Result<Self, String> {
        yaml_serde::from_str(text).map_err(|e| format!("parse {origin}: {e}"))
    }
}
