//! The common working vocabulary for building, reading, parsing, and rendering a
//! program: the conversion and construction traits a client needs in scope, and
//! the types it names most. Glob-import it — `use themelios_program::prelude::*;`.
//!
//! It is a superset of the crate-root re-exports. Advanced surfaces are reached by
//! their module path, not here: the theory-atom family (`program`), unification
//! (`unify`), the transformation visitors (`transform`), the `SymbolParts` and
//! `TermParts` fold enums, and the raw operator enums (`term`). The constructor
//! free functions — `not`, `minimize`, `render`, `raise`, the rounding adapters —
//! are reached through their modules too, so this prelude stays safe to glob.

pub use crate::analyze::DependencyKind;
pub use crate::program::{
    Aggregate, AggregateFunction, Arguments, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, Const, ConstPolicy, DefaultNegation,
    Defined, Direction, Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard,
    HasGuards, Head, HeadAggregate, HeadAggregateElement, Heuristic, Include, IncludeTarget,
    IntoBody, IntoHead, Literal, LiteralInner, Optimize, OptimizeElement, Part, PartKey, Program,
    Project, Query, Relation, Rule, Script, SetAggregate, SetElement, Show, Statement,
    WeakConstraint, Weight,
};
pub use crate::provenance::{Annotations, Origin, Provenance, TransformTag, WithProvenance};
pub use crate::raise::{LowerError, LowerErrorKind, Raised};
pub use crate::render::Unspellable;
pub use crate::symbol::{
    FromSymbol, FromSymbolError, Name, NotAVariable, NotAnIdentifier, NotAnInteger, Sign,
    Signature, Symbol, ToSymbol, VarName,
};
pub use crate::term::{EmptyPool, EvalError, Term, Variable};
pub use themelios_base::span::Location;
pub use themelios_syntax::dialect::Dialect;
