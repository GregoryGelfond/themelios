//! Source-text model, spans, line indexing, and the diagnostics model:
//! the shared vocabulary of *location* and *report* under every tier.
//!
//! Design of record: `docs/design/base.md`. Every public operation's
//! failure semantics and computational cost are stated on the
//! operation and consolidated in base.md §9. This crate does no I/O,
//! holds no global state, and knows nothing about any language.
#![forbid(unsafe_code)]
