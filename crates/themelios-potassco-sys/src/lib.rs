//! The pinned bindings over libclingo's C API — clingo 5.8.2, the vendored
//! authority — the binding half of the potassco trusted computing base
//! (docs/design/solve.md §2.1, §11.3, §16). Feature-gated: behind `clingo` the
//! crate binds the library; under the default features it is an empty shell
//! that links nothing, so a default workspace build is FFI-free.
