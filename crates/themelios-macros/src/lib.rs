//! Construction macros for themelios — sugar over the program tier's
//! constructors. Each macro is a `themelios_syntax` token source that parses
//! ASP at compile time and codegens `themelios_program` constructor calls
//! (docs/design/macros.md).
//!
//! Each macro fixes a grammatical category and a target family, parses ASP under
//! `Dialect::Clingo` with splices, and expands to the program tier's §7.1
//! constructor calls that build the value — the same value the by-hand
//! constructors it names build, structurally equal up to and including
//! provenance (docs/design/macros.md §8, §11). A directive macro supplies its
//! leading `#`-keyword from its own name; the caller writes the payload alone.
#![forbid(unsafe_code)]

mod codegen;
mod diagnostics;
mod engine;
mod source;

use proc_macro::TokenStream;

use crate::engine::{Entry, run};

/// `fact!(head)` → a [`Rule`](themelios_program::program::Rule): a fact, a head with the
/// empty body (docs/design/macros.md §8). Equals `Rule::fact`.
#[proc_macro]
pub fn fact(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, None).into()
}

/// `rule!(head :- body)` → a [`Rule`](themelios_program::program::Rule): a rule read as
/// the rule it denotes (docs/design/macros.md §8). Equals `Head::when`.
#[proc_macro]
pub fn rule(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, None).into()
}

/// `constraint!(:- body)` → a [`Rule`](themelios_program::program::Rule): an integrity
/// constraint, a falsum head over the body (docs/design/macros.md §8). Equals
/// `Rule::constraint`.
#[proc_macro]
pub fn constraint(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, None).into()
}

/// `minimize!({ … })` → an [`Optimize`](themelios_program::program::Optimize): a
/// `#minimize` statement, each element a weighted term at a priority
/// (docs/design/macros.md §8). Equals `minimize` (program §4.7).
#[proc_macro]
pub fn minimize(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, Some("minimize")).into()
}

/// `maximize!({ … })` → an [`Optimize`](themelios_program::program::Optimize): a
/// `#maximize` statement, the twin of [`minimize`] (docs/design/macros.md §8). Equals
/// `maximize` (program §4.7).
#[proc_macro]
pub fn maximize(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, Some("maximize")).into()
}

/// `show!(…)` → a [`Show`](themelios_program::program::Show): a `#show` directive, in any
/// of its four forms — a signature, a term, a term under a body, or all
/// (docs/design/macros.md §8). Equals the `Show` constructor (program §4.8).
#[proc_macro]
pub fn show(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, Some("show")).into()
}

/// `external!(…)` → an [`External`](themelios_program::program::External): a `#external`
/// directive — the atom and its body (docs/design/macros.md §8). Equals `External::new`.
#[proc_macro]
pub fn external(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Statement, Some("external")).into()
}

/// `atom!(head)` → an [`Atom`](themelios_program::program::Atom) in head position
/// (docs/design/macros.md §8), reached by assembling a fact and extracting its single head
/// atom. A leading `-` is the atom's positional strong negation (`-p` → `Sign::Negative`,
/// program §3.3) — head position is where a strong-negated atom is built, the term door
/// reading `-` as arithmetic negation instead. A head that is not a single atom — a
/// disjunction, a choice or aggregate, a theory atom, a comparison, or a constraint — is a
/// compile error. Equals `Atom::new` (or its `Neg` for a strong-negated atom).
#[proc_macro]
pub fn atom(input: TokenStream) -> TokenStream {
    run(input.into(), Entry::Atom, None).into()
}
