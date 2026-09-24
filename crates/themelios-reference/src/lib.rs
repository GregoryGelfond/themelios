//! The themelios reference solver — the naive, pure-Rust implementor of the
//! solve tier's `Backend` contract (design of record: `docs/design/solve.md`
//! §12): the small-case oracle, the second implementor that proves the contract
//! is not clingo-shaped, and the demonstration that a native backend built from
//! the foundation crates is a first-class implementor. Engine-free; unpublished.
#![forbid(unsafe_code)]
