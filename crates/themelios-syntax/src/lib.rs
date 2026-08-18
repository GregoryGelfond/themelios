//! The syntax tier: a total lexer with the fusion oracle beside it; a
//! hand-written, error-resilient parser producing a lossless tree of
//! the one grammar under a declared dialect; comment attachment as
//! owned, exposed policy; a typed AST over the tree; the tier's own
//! typed diagnostics, lowering to the base model; and token-stream
//! equivalence, the certificate a layout-only or spelling-preserving
//! transformation claims.
//!
//! Design of record: `docs/design/syntax.md`; the grammar it is held to:
//! `docs/grammar.md`. Every public operation's failure semantics and
//! computational cost are stated on the operation and consolidated in
//! syntax.md §13. A parse is a pure function of its inputs; this crate
//! does no I/O, holds no global state, and hands out no structure whose
//! depth is proportional to the input's nesting.
#![forbid(unsafe_code)]

// The base tier, whole, under one name: the vocabulary every door here
// speaks — Source, ByteOffset, Span, Location, Severity, Diagnostic, the
// line index, the views — is reachable through this crate alone
// (docs/design/syntax.md §1).
pub use themelios_base as base;

pub mod dialect;
pub mod tree;
