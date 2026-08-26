//! The program tier: the logician's owned, total representation of an ASP
//! program and the operations over it. Present: the ground-symbol and term
//! algebra (`symbol`, `term`); the `Program` value, a part-structured set of
//! rules and directives (`program`); provenance as in-node model data
//! (`provenance`); the structural accessors the analysis client reads
//! (`analyze`); the two construction doors onto a program, one shared
//! well-formedness authority behind both — the declarative construction
//! surface, spelled-out constructors whose Rust mirrors the logic
//! (`construct`), and the raise from the syntax tier, lowering ASP concrete
//! syntax (`raise`); the substitution core over the term algebra — resolving
//! substitution and a collision-free source of fresh names (`unify`); the
//! `Program` -> `Program` transformation surface, a read-only visitor and a
//! provenance-tracing, canonicalizing rewriter (`transform`); and canonical,
//! round-trippable rendering to concrete syntax (`render`). Forthcoming: the
//! most general unifier and the pattern language that complete `unify`.
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
