//! req-tier2 — the Tier-2 funnel, as research (ADR 0021).
//!
//! This crate was `src/funnel.rs` in the core until M2-2. It depends on the `req` library by path
//! and is not a member of the root workspace. The core keeps only the single-pair `req propose`
//! seam; blocking, the gold-set score, the flip rate and the caps live here. Nothing in this crate
//! gates a release.

pub mod funnel;
