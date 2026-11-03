//! P-1: how often independently worded text for the same obligation fails to share a Tier-1
//! address. An instrument, not a gate: it never feeds the release gate (see `README.md` for the
//! grade of the evidence).

pub mod cli;
pub mod data;
pub mod judge;
pub mod protocol;
pub mod sampler;
pub mod score;
pub mod selfcheck;
pub mod shape;
pub mod stats;
