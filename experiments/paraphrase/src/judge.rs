//! The judge filter (P-1 spec §3). A restatement enters the headline only on a unanimous `yes`
//! from every eligible judge; a generator can drift in meaning, and a drifted "restatement"
//! correctly splits, so it must not count against Tier-1.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Yes,
    No,
    Unsure,
}

/// Why a restatement is outside the headline. Counted and listed, never silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    /// At least one judge said the obligation differs.
    Rejected,
    /// No judge said no, but at least one was unsure.
    Unsure,
    /// A judge gave no verdict, or there was no judge at all: nothing was confirmed.
    Unjudged,
}

impl Exclusion {
    pub fn as_str(self) -> &'static str {
        match self {
            Exclusion::Rejected => "judged different obligation",
            Exclusion::Unsure => "judged unsure",
            Exclusion::Unjudged => "not judged",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Admitted,
    Excluded(Exclusion),
}

/// One verdict per eligible judge, `None` where that judge gave none. `no` outranks `unsure`
/// outranks a missing verdict; an empty panel admits nothing.
pub fn admit(verdicts: &[Option<Verdict>]) -> Admission {
    if verdicts.is_empty() {
        return Admission::Excluded(Exclusion::Unjudged);
    }
    if verdicts.contains(&Some(Verdict::No)) {
        return Admission::Excluded(Exclusion::Rejected);
    }
    if verdicts.contains(&Some(Verdict::Unsure)) {
        return Admission::Excluded(Exclusion::Unsure);
    }
    if verdicts.contains(&None) {
        return Admission::Excluded(Exclusion::Unjudged);
    }
    Admission::Admitted
}
