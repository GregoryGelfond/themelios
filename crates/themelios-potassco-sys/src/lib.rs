//! The pinned bindings over the libclingo and libclingcon C APIs, against clingo
//! 5.8.2, the authority to be vendored — the binding half of the potassco
//! trusted computing base (docs/design/solve.md §2.1, §11.3, §16), reserved and
//! not yet implemented. Until the binding lands the crate is an empty shell under every
//! feature and links nothing, so the workspace build is FFI-free; the `clingo`
//! feature will gate the binding.
