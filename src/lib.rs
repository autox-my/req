//! req — library surface: the claim model, the Tier-1 canonicalizer, and the gate.
//!
//! The `req` binary is a thin CLI over this library. Keeping the engine a real
//! library (not a bin crate with loose `mod`s) matters beyond tidiness: the
//! public API is the surface the future porcelain and any re-implementation read
//! from, and dead-code analysis then means what it says about private items only.

pub mod audit;
pub mod baseline;
pub mod bench;
// The build identity's decision: `build.rs` renders `req --version`'s `built from` line through
// this, and pulls the same file in with `#[path]` — one implementation, compiled twice, so the
// four states are unit-testable here even though a build script is not.
pub mod build_identity;
pub mod canon;
// The release lists folded into one verdict (ADR 0023): reads `.req/variants` and
// `.req/projections`, runs the gate and `render --check` code for each line, and applies the gate's
// own fold. Nothing here computes an address or a policy.
pub mod check;
pub mod compare;
pub mod decision;
// Internal plumbing: every caller is in-crate (canon, decision, bench, model), and the raw
// encoder is not the surface a re-implementation reads — the audit-surface contract is
// `model::relations_digest` / `rationale_digest` and ADR 0013's formula.
pub(crate) mod digest;
// `--format`: the text rendering or a JSON document, shared by the four commands pack C scopes
// (render, trace, audit, gate). A rendering seam, not an identity seam: nothing here computes an
// address or an identity.
pub mod format;
pub mod gate;
pub mod impact;
// The one command that *creates* a corpus root instead of reading one. It is answered in
// `main.rs` before root resolution, so its handler takes no corpus — see the module doc for
// why `--root` is a usage error there and not a second anchor.
pub mod init;
pub mod junit;
pub mod model;
// The single-pair Tier-2 seam (ADR 0021); the funnel it replaced lives in `experiments/tier2/`.
pub mod propose;
pub mod root;
pub mod select;
pub mod view;
