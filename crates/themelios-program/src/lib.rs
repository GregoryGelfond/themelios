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
// the types a client names constantly, the conversion and construction traits, the
// render doors and the door refusals, and the foreign leaves this crate hands
// across its own boundary — the base source seam (`base`, `Source`, `SourceId`,
// `TooLarge`, `Location`) and the syntax tier's `Dialect` — so
// `themelios_program::Program` (and `render`, `Source`, `Dialect`) resolve without
// walking the module tree or taking a dependency only to name a returned type. The
// full working vocabulary, for a one-line glob import, is `prelude`.
pub use crate::program::{
    Atom, Body, BodyElement, Comparison, HasGuards, Head, IntoBody, IntoHead, Literal, Program,
    Relation, Rule, Statement,
};
pub use crate::provenance::{Origin, Provenance, WithProvenance};
pub use crate::render::{Unspellable, render, render_documented};
pub use crate::symbol::{FromSymbol, FromSymbolError, Name, Sign, Signature, Symbol, ToSymbol};
pub use crate::term::{EmptyPool, Term, Variable};
pub use themelios_base as base;
pub use themelios_base::source::{Source, SourceId, TooLarge};
pub use themelios_base::span::Location;
pub use themelios_syntax::dialect::Dialect;

/// An answer set: a set of ground [`Symbol`]s in the tier's canonical `Ord`.
///
/// Declared here as the lowest tier that can express it and the type the pattern surface's
/// [`signature_range`](crate::unify::signature_range) scan ranges over
/// (`answer_set.range(signature_range(&pattern))`); the solve and query tiers re-export it for
/// the outcome-reading audience (the program tier itself does not name it — it is a declaration
/// point, not part of the [`prelude`]). It is a plain `BTreeSet<Symbol>` — an answer set *is* a
/// set of ground symbols under that order — so no wrapper stands between the `range` scan and the
/// set algebra.
pub type AnswerSet = std::collections::BTreeSet<Symbol>;
