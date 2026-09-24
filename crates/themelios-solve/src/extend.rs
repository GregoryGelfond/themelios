//! The extension surface (docs/design/solve.md §7–§9): bulk fact conversion,
//! `@`-functions, propagators, and read-time extraction.
//!
//! [`Facts`] is the bulk analog of the program tier's `ToSymbol` (§7.3): a
//! Rust value that denotes a *set* of ground atoms — the codec a
//! data-shredding client or a code generator uses to turn a value into a
//! sub-program's facts, and the result side of an `@`-predicate.
//! [`Function`] is the ground-time extension (§7.1): the Rust an
//! `@name(args)` call reaches, its arguments and results crossing as typed
//! symbols and its failure a typed ground-time fault with a locus,
//! [`GroundFault`]. [`Propagator`] is the registration seam of the
//! theory-extension platform (§8.1), and [`Extract`] the read-time inverse of
//! `Facts` (§9); [`TheoryFault`] and [`ExtractError`] are their faults.
//!
//! Registration is a `Backend` door gated by the matching capability bit
//! (§4.1): `register_function` under `functions`, `register_propagator` under
//! `propagators`; a backend that declares neither refuses both as a typed
//! request fault. So `Function` and `Propagator` are trait objects, held as
//! `Box<dyn Function>` and `Box<dyn Propagator>` behind the door, while
//! `Facts` is a bound — consumed as `impl Facts`, never boxed — and `Extract`
//! is a constructor on the value it builds, never an object.

use std::fmt;

use themelios_program::{AnswerSet, Symbol};

use crate::contract::Locus;

// ---- The bulk conversion (§7.3) ----

/// A Rust value denoting a set of ground atoms (docs/design/solve.md §7.3):
/// the bulk analog of the program tier's `ToSymbol`, which denotes one
/// ground `Symbol`. Where `ToSymbol` is the codec for a value, `Facts` is the
/// codec for a relation — the edges of a graph, the rows of a table — turned
/// into the ground atoms a sub-program states as facts, and the result side
/// of an `@`-predicate; [`Extract`] (§9) is its read-time inverse, and a fix
/// to the shared codec pays out across all of them.
///
/// A bound, not an object: a consumer takes `impl Facts` and iterates what
/// the value denotes, so the iterator is the implementor's own, and nothing
/// is collected or boxed on the way. Cost: `Θ(atoms produced)`.
pub trait Facts {
    /// The ground atoms the value denotes — each a `Symbol`, a predicate
    /// applied to ground arguments. Borrowed: denoting does not spend the
    /// value.
    fn facts(&self) -> impl Iterator<Item = Symbol>;
}

// ---- Ground-time extension: `@`-functions (§7.1, §7.2) ----

/// An `@`-function a backend evaluates at ground time, registered boxed
/// (docs/design/solve.md §7.1): the Rust behind `@name(args)`. Arguments and
/// results cross as typed symbols — a multi-valued return is the ordinary
/// case, one result per element and none for a call that denotes nothing —
/// and a failing call is a typed ground-time fault with a locus, a
/// [`GroundFault`]. The results cross as a `Vec<Symbol>`, the program tier's
/// `IntoIterator<Item = Symbol>` shape, so the tier takes no dependency
/// beyond the tiers beneath it (§16).
///
/// A trait object: registered through `Backend::register_function`, under
/// the `functions` capability (§4.1), and refused by a backend that declares
/// none. The backend holds the function for the life of its program and
/// calls it under a panic-containing trampoline (§7.2), so a panic never
/// crosses the seam as anything but a fault of the backend's own.
///
/// The library door (§7.2): compute in Rust's own numeric tower inside the
/// function and convert at the boundary, refusing — a `GroundFault` — rather
/// than repairing a value the `i32`-width `Symbol` cannot represent. A
/// function is arbitrary Rust at ground time, so purity is its author's
/// contract: a clock, a random source, or the filesystem breaks a
/// deterministic, auditable run.
pub trait Function {
    /// Evaluate the function on ground arguments: the symbols the call
    /// denotes, or the fault that refuses it. Borrowed: a call does not
    /// spend the function, which the backend calls any number of times.
    /// Cost: the function's own.
    fn call(&self, arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault>;
}

/// A ground-time fault an `@`-function raises (docs/design/solve.md §7.1):
/// typed, with a locus. The function names the condition — the call is
/// refused, or a limit was reached — and the backend that dispatched the
/// call names the place, lowering the fault into a
/// [`Fault`](crate::contract::Fault) at the call's own statement (§5.4); so
/// a ground fault carries a [`Locus`] and a message, and no source label of
/// its own. Non-exhaustive: a field is a new field, not a migration. Owned
/// plain data.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GroundFault {
    locus: Locus,
    /// The headline.
    message: String,
}

impl GroundFault {
    /// A call the function refuses: arguments outside its domain — a wrong
    /// arity or sort, or a value the `i32`-width `Symbol` cannot represent
    /// (§7.2) — refused rather than repaired. The program's call is what
    /// could not be ground, so the locus is [`Locus::Program`]. Total;
    /// O(message).
    pub fn refused(message: impl Into<String>) -> GroundFault {
        GroundFault {
            locus: Locus::Program,
            message: message.into(),
        }
    }

    /// A limit of the environment reached while the call was evaluated, with
    /// the limit named: [`Locus::Resource`]. Total; O(message).
    pub fn resource(message: impl Into<String>) -> GroundFault {
        GroundFault {
            locus: Locus::Resource,
            message: message.into(),
        }
    }

    /// Where the fault arose. Total; O(1).
    pub fn locus(&self) -> Locus {
        self.locus
    }
}

impl fmt::Display for GroundFault {
    /// The message — how the fault renders.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GroundFault {}

// ---- Theory extension: propagators (§8) ----

/// A custom propagator a backend runs, registered boxed (docs/design/solve.md
/// §8.1) — the registration seam of the theory-extension platform (§8.3): a
/// theory written once on the contract's safe surface runs on any backend
/// that implements the contract. The seam is a trait object,
/// `Box<dyn Propagator>` through `Backend::register_propagator` under the
/// `propagators` capability (§4.1), so the registered form carries no
/// associated state type; the state-bearing surface — `init`, `propagate`,
/// `undo`, `check`, brokering a per-thread state and typed literals and
/// clauses — is governed by the design's litmus (§8.2) and defined where the
/// theory door is realised. Reserved until custom propagation is
/// implemented: a backend that runs none refuses the registration.
pub trait Propagator {}

/// A fault a propagator raises (docs/design/solve.md §8.1): typed, the
/// failure of the state-bearing surface's `init`, `propagate`, or `check`.
/// Non-exhaustive: its fields join when custom propagation is implemented;
/// until then the fault is declared, and empty, so the surface it belongs to
/// is shaped by it already.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TheoryFault {}

/// How a theory fault renders while it carries no detail of its own.
const THEORY_FAULT: &str = "a propagator raised a theory fault";

impl fmt::Display for TheoryFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(THEORY_FAULT)
    }
}

impl std::error::Error for TheoryFault {}

// ---- Read-time extraction (§9) ----

/// An answer set, or a projection of one, read into a user-defined Rust
/// value (docs/design/solve.md §9): the read-time inverse of [`Facts`]
/// (§7.3), over the same conversion pillar — the `FromSymbol` that reads an
/// `@`-function's argument reads an answer set's atom. A constructor on the
/// value it builds, not an object: `Sized`, so a reader names the type and
/// extracts. Reserved until read-time extraction is implemented: the derived
/// implementation and its failure report on a non-matching atom, an
/// [`ExtractError`], are defined with it. Cost: `Θ(atoms read)`.
pub trait Extract: Sized {
    /// The value the answer set denotes, or the error that refuses it.
    fn extract(model: &AnswerSet) -> Result<Self, ExtractError>;
}

/// The failure of an extraction (docs/design/solve.md §9): an atom of the
/// answer set that does not match the value's shape. Non-exhaustive: the
/// offending atom and its codec failure join when read-time extraction is
/// implemented; until then the error is declared, and empty, so the surface
/// it belongs to is shaped by it already.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExtractError {}

/// How an extraction error renders while it carries no detail of its own.
const EXTRACT_ERROR: &str = "the answer set does not match the value's shape";

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(EXTRACT_ERROR)
    }
}

impl std::error::Error for ExtractError {}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    // The reserved faults carry no public constructor yet, so they are
    // built here, in the defining crate, by the struct expression a
    // non-exhaustive struct admits nowhere else.

    #[test]
    fn a_theory_fault_says_what_it_is() {
        assert_eq!(TheoryFault {}.to_string(), THEORY_FAULT);
    }

    #[test]
    fn a_theory_fault_is_an_error_without_a_source() {
        let fault = TheoryFault {};
        let error: &dyn Error = &fault;
        assert!(error.source().is_none());
    }

    #[test]
    fn an_extract_error_says_what_it_is() {
        assert_eq!(ExtractError {}.to_string(), EXTRACT_ERROR);
    }

    #[test]
    fn an_extract_error_is_an_error_without_a_source() {
        let error = ExtractError {};
        let error: &dyn Error = &error;
        assert!(error.source().is_none());
    }
}
