//! The Potassco adapter, reserved and not yet implemented: the mechanism-only
//! kernel over the bindings and the safe adapters implementing the solve tier's
//! `Backend` contract against clingo 5.8.2 and clingcon (design of record:
//! `docs/design/solve.md` §11) — the trusted computing base, named for the
//! engine family it adapts. Until the adapter lands the crate is an empty shell
//! under every feature, and the stack is FFI-free; the `potassco` feature will
//! gate the adapter.
