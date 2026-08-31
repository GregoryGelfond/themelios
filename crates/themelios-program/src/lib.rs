//! The program tier: the logician's owned, total representation of an ASP
//! program and the operations over it. Present: the ground-symbol and term
//! algebra (`symbol`, `term`); the `Program` value, a part-structured set of
//! rules and directives (`program`); provenance as in-node model data
//! (`provenance`); the structural accessors the analysis client reads
//! (`analyze`); the two construction doors onto a program, one shared
//! well-formedness authority behind both — the declarative construction
//! surface, spelled-out constructors whose Rust mirrors the logic
//! (`construct`), and the raise from the syntax tier, lowering ASP concrete
//! syntax (`raise`); the substitution core, the most general unifier, and the
//! pattern language over the term algebra — resolving substitution, a
//! collision-free source of fresh names, the near-linear Martelli–Montanari
//! unifier with its forced occurs check, and the constructor-fragment pattern
//! check with range-scan matching (`unify`); the `Program` -> `Program`
//! transformation surface, a read-only visitor and a provenance-tracing,
//! canonicalizing rewriter (`transform`); and canonical, round-trippable
//! rendering to concrete syntax (`render`).
//!
//! Design of record: `docs/design/program.md`; the grammar it is held to:
//! `docs/grammar.md`; the tiers beneath it: `docs/design/base.md`,
//! `docs/design/syntax.md`. Every value this tier produces is owned plain
//! data — `Send + Sync + 'static`, holding no borrow — so a program is
//! constructed on one thread and solved on another, transformed and kept,
//! without a lifetime. Every public operation's failure semantics and
//! computational cost are stated on the operation and consolidated in
//! program §15. A derivation is a pure function of its inputs: this crate
//! does no I/O, holds no global state, interns nothing, evaluates no
//! rule-embedded term save at the explicit ground-value door (§3.5), and
//! hands out no structure whose walk is proportional in depth to a value's
//! nesting (§13).
#![forbid(unsafe_code)]

pub mod symbol;
pub mod term;
pub mod provenance;
pub mod program;
pub mod construct;
pub mod raise;
pub mod analyze;
pub mod unify;
pub mod transform;
pub mod render;

pub mod prelude;

// Crate-root re-exports of the most-used surface (Rust API guideline C-REEXPORT):
// the types a client names constantly, the conversion and construction traits,
// and the two foreign types this crate hands across its own boundary — so
// `themelios_program::Program` (and `Dialect`, `Location`) resolve without
// walking the module tree or taking a dependency only to name a returned type.
// The full working vocabulary, for a one-line glob import, is `prelude`.
pub use crate::program::{
    Atom, Body, HasGuards, Head, IntoBody, IntoHead, Literal, Program, Rule, Statement,
};
pub use crate::provenance::{Origin, Provenance, WithProvenance};
pub use crate::symbol::{FromSymbol, Name, Sign, Signature, Symbol, ToSymbol};
pub use crate::term::{Term, Variable};
pub use themelios_base::span::Location;
pub use themelios_syntax::dialect::Dialect;
