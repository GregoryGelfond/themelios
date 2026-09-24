//! The outcome vocabulary (docs/design/solve.md §5): the models and their
//! views — determination, conclusion, the solved outcome, models, consequences,
//! and unsatisfiability.

use std::marker::PhantomData;

/// The run handle `solve` returns, borrowing its engine for `'a`. Reserved;
/// its surface is defined with §5.2.
pub struct Solved<'a> {
    _engine: PhantomData<&'a ()>,
}

/// The run handle `optimize` returns, borrowing its engine for `'a`. Reserved;
/// its surface is defined with §5.2.
pub struct Optimized<'a> {
    _engine: PhantomData<&'a ()>,
}

/// Cautious or brave consequences: a set of ground symbols carrying the mode
/// that produced it. Reserved; its surface is defined with §5.2.
pub struct Consequences;
