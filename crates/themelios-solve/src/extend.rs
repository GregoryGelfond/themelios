//! The extension surface (docs/design/solve.md §7–§9): bulk fact conversion,
//! `@`-functions, propagators, and read-time extraction.

/// An `@`-function a backend evaluates at ground time, registered boxed.
/// Reserved; its calling surface is defined with §7.1.
pub trait Function {}

/// A custom propagator a backend runs, registered boxed. Reserved; its
/// surface is defined with §8.1.
pub trait Propagator {}
