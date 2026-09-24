//! The Potassco adapter — the mechanism-only kernel over the clingo bindings and
//! the safe adapter implementing the solve tier's `Backend` contract against
//! clingo 5.8.2 (design of record: `docs/design/solve.md` §11): the trusted
//! computing base, named for the engine family it adapts. Behind the `potassco`
//! feature; under the default features it is an empty shell and the stack is
//! FFI-free.
