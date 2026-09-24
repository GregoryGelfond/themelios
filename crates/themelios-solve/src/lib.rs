//! The themelios solve tier — the abstract-solver contract over the `Program`
//! IR (design of record: `docs/design/solve.md`). Engine-free; the potassco
//! adapter and the reference solver implement the `Backend` contract.
#![forbid(unsafe_code)]

pub mod contract; // §4 — the Backend trait, Capabilities, the fault/locus vocabulary
pub mod outcome; // §5 — Determination/Conclusion/Solved/Models/Consequences/Unsat
pub mod bridge; // §10 — Door A/B/C, the aspif sink, GroundProgram, the interning contract
pub mod agent; // §6 — Agent<B>, the reasoning loop, assumptions/options/cancel
pub mod extend; // §7–§9 — Facts; Function/Propagator/Extract declared
pub mod conformance; // §13.1 — the executable suite every adapter passes
pub mod prelude;
