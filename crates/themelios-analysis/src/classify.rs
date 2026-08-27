//! The program classes (docs/design/analysis.md §6): a program's membership in the
//! classes of the literature — tight, stratified, head-cycle-free, normal, Horn,
//! disjunctive, choice — read for a solver's algorithm selection. This module
//! provides `Verdict`, the shared approximation verdict (§6.1) every sound,
//! predicate-level reading of this crate returns; the recursion and syntactic
//! classes (§6.2, §6.3) and the routable projection (§6.4) read off it.

use crate::depend::Component;

/// A sound approximation of a ground-program property, read at the predicate level
/// (§6.1). `Holds` is **proven** — the property is guaranteed of the ground program.
/// `Unknown` is undecided at the predicate level — the ground program may or may not
/// have it — and carries the `Component` that blocked the proof. There is
/// deliberately **no** third `DoesNotHold` arm: a consumer specializes on a class's
/// *presence*, so a false `Holds` is an unsound result while a missed one is merely
/// slower, and the only safe design asserts `Holds` when it has a proof and folds
/// everything else into `Unknown` (the error direction, §6.1). Concrete over
/// `Component`, not generic: the approximation exists *because* this crate reads the
/// predicate level, so its witness is always a predicate-level component. Grounding
/// finiteness (§5) and — once they land — tightness and head-cycle-freeness (§6.2)
/// all return this one shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// The property is proven of the ground program.
    Holds,
    /// The property is undecided at the predicate level, with the component that
    /// blocked the proof.
    Unknown {
        /// The strongly-connected component whose recursion left the property
        /// unproven.
        witness: Component,
    },
}
