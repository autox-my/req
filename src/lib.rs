//! req — library surface: the claim model, the Tier-1 canonicalizer, and the gate.
//!
//! The `req` binary is a thin CLI over this library. Keeping the engine a real
//! library (not a bin crate with loose `mod`s) matters beyond tidiness: the
//! public API is the surface the future porcelain and any re-implementation read
//! from, and dead-code analysis then means what it says about private items only.

pub mod audit;
pub mod baseline;
pub mod bench;
pub mod canon;
pub mod compare;
pub mod decision;
pub mod digest;
pub mod fill;
pub mod funnel;
pub mod gate;
pub mod impact;
pub mod model;
pub mod select;
pub mod view;
