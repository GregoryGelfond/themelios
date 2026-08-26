//! The `Program` value: a part-structured set of rules and directives (docs/design/
//! program.md §4). Every structural node is a *content* value paired with the
//! provenance carrier `WithProvenance<T>` (§6.2), and a container field holding such
//! a node holds `WithProvenance<Child>`; the content types derive their identity
//! **over content**, and the carrier erases provenance from it (§5, §6.2). Each
//! content type is grammar-bounded — it does not self-nest — so a derived `Ord`
//! descends a bounded number of levels and bottoms out in `Term`'s iterative one
//! (§13); only `Term`, `Symbol`, and `TheoryTerm` are self-recursive.
//!
//! `program` is a directory of private submodules under one public module (§1); the
//! public surface is re-exported here.

mod aggregate;
mod directive;
mod rule;

pub use aggregate::{
    Aggregate, AggregateFunction, BodyAggregateElement, Direction, FunctionAggregate, Guard,
    HasGuards, HeadAggregate, HeadAggregateElement, Optimize, OptimizeElement, SetAggregate,
    SetElement,
};
pub use directive::{
    Const, ConstPolicy, Defined, Include, IncludeTarget, Script, TheoryAtom, TheoryAtomDefinition,
    TheoryAtomGuardDefinition, TheoryDefinition, TheoryElement, TheoryGuard, TheoryOccurrence,
    TheoryOperator, TheoryOperatorArity, TheoryOperatorDefinition, TheoryTerm,
    TheoryTermDefinition, TheoryTermParts,
};
pub use rule::{
    Atom, Comparison, Condition, ConditionalLiteral, DefaultNegation, Literal, LiteralInner,
    Relation,
};
