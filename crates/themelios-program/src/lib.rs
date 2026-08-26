//! The program tier: the logician's owned, total representation of an ASP
//! program and the operations over it. The ground-symbol and term algebra
//! (`symbol`, `term`); the `Program` value, a part-structured set of rules
//! and directives (`program`); provenance as in-node model data
//! (`provenance`); the two construction doors — spelled-out constructors
//! (`construct`) and the raise from the syntax tier (`raise`) — under one
//! well-formedness authority; canonical round-trippable rendering
//! (`render`); `Program` -> `Program` transformation (`transform`); the
//! pattern language and the most general unifier (`unify`); and the
//! structural accessors the analysis client reads (`analyze`).
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
