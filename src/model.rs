//! Claim model + corpus/registry payloads, read with `yaml_serde`.
//!
//! Field names and semantics follow [docs/claim-format.md] exactly. `value` stays a
//! `String` (the exact decimal string, never a float — claim-format authoring rule).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rationale {
    pub source: String,
    pub date: String,
    #[serde(default)]
    pub rejected_alternatives: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    #[serde(rename = "type")]
    pub kind: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalCache {
    /// Normalization-rules version. `None` while unfilled (the v0 seed state).
    pub rules_v: Option<u32>,
    /// `aliases/subjects.yml` version the canonical form was computed against.
    pub subject_registry: u32,
    /// The stored canonical form bytes (`None` until `req canon` fills them).
    pub form: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimFile {
    pub schema: String,
    pub claim_id: String,
    pub statement: String,
    pub modality: String,
    pub subject: String,
    /// `None` for unstructured claims (claim-format field reference).
    pub attribute: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub unit: Option<String>,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub applicability: Vec<String>,
    pub exception: Option<String>,
    pub temporal: Option<String>,
    pub quantifier: Option<String>,
    pub unstructured: bool,
    pub canonical: CanonicalCache,
    pub address: Option<String>,
    pub rationale: Rationale,
    #[serde(default)]
    pub relations: Vec<Relation>,
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
        let file: RegistryFile =
            yaml_serde::from_str(&text).map_err(|e| format!("parse {path}: {e}"))?;
        let mut alias_to_subject = BTreeMap::new();
        for (subject_id, def) in &file.subjects {
            for alias in &def.aliases {
                let key = alias.to_lowercase();
                if let Some(existing) = alias_to_subject.get(&key) {
                    if existing != subject_id {
                        return Err(format!(
                            "alias collision in {path}: {key:?} resolves to both {existing} and {subject_id}"
                        ));
                    }
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
        self.alias_to_subject
            .get(&key)
            .cloned()
            .unwrap_or_else(|| key)
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
        let corpus: Corpus = yaml_serde::from_str(&text).map_err(|e| format!("parse {path}: {e}"))?;
        Ok(corpus)
    }
}
