//! The fixed prompts. One home: the text a generator or a judge is given is defined here, printed
//! by `tasks …`, required verbatim in each output file's `prompt` field, and mirrored (and
//! pinned) in `PROTOCOL.md`. A model that was given something else did not run this protocol.
//!
//! Neither prompt mentions Tier-1, canonical forms, addresses or fingerprints: generators are
//! never shown the rules they are measuring (neither to help nor to hurt them).

/// The five elements an alteration may change.
pub const ELEMENTS: &[&str] = &["modality", "quantity", "actor", "action", "condition"];

pub const GENERATOR_INSTRUCTION: &str = "Restate this requirement as a different engineer would write it, preserving exactly the same obligation (who, modality, action, bound, condition); do not copy phrasing; give 3 restatements; then give 1 alteration that changes exactly one element (modality, quantity, actor, action or condition) and name which element.";

pub const GENERATOR_FORMAT: &str = "Answer with one JSON object and nothing else: {\"restatements\": [\"...\", \"...\", \"...\"], \"alteration\": {\"text\": \"...\", \"element\": \"modality|quantity|actor|action|condition\"}}";

pub const JUDGE_INSTRUCTION: &str = "Two requirement statements follow. Do they impose the same obligation (same actor, modality, action, bound and condition)? Answer with exactly one word: yes, no or unsure. Answer unsure if you cannot tell; do not guess.";

/// The generator prompt template, as recorded in a restatements file's `prompt` field. The
/// requirement is appended after it.
pub fn generator_template() -> String {
    format!("{GENERATOR_INSTRUCTION}\n\n{GENERATOR_FORMAT}")
}

/// The judge prompt template, as recorded in a judgements file's `prompt` field. The two
/// statements are appended after it.
pub fn judge_template() -> String {
    JUDGE_INSTRUCTION.to_string()
}

pub fn generator_prompt(statement: &str) -> String {
    format!("{}\n\nREQUIREMENT:\n{statement}", generator_template())
}

pub fn judge_prompt(statement: &str, other: &str) -> String {
    format!("{}\n\nA:\n{statement}\n\nB:\n{other}", judge_template())
}

/// Words that would tell a model about the thing being measured. A prompt containing one is a
/// protocol defect (`--self-check` pins that the shipped prompts contain none).
pub const LEAK_TERMS: &[&str] = &[
    "canonical",
    "address",
    "fingerprint",
    "tier-1",
    "tier 1",
    "tier1",
    "hash",
    "normaliz",
    "req ",
];

/// The leak terms present in `prompt`, case-insensitively.
pub fn leaks(prompt: &str) -> Vec<&'static str> {
    let lower = prompt.to_lowercase();
    LEAK_TERMS
        .iter()
        .copied()
        .filter(|t| lower.contains(t))
        .collect()
}
