//! The bridge and the seam (docs/design/solve.md §10): the doors from the
//! program IR to an engine, the aspif-level sink, the ground-program IR, and
//! the interning contract.

use std::marker::PhantomData;

/// A door from the program IR into an engine, borrowing what it carries for
/// `'a`. Reserved; its forms are defined with §10.2.
pub struct Door<'a> {
    _carried: PhantomData<&'a ()>,
}

/// The ground program a backend exposes — engine-free data. Reserved; its
/// surface is defined with §10.4.
pub struct GroundProgram;
