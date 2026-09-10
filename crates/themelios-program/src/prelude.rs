//! The common working vocabulary for building, reading, parsing, and rendering a
//! program: the conversion and construction traits a client needs in scope, and
//! the types it names most. Glob-import it — `use themelios_program::prelude::*;`.
//!
//! It is a superset of the crate-root re-exports. Advanced surfaces are reached by
//! their module path, not here: the theory-atom family (`program`), unification
//! (`unify`), the transformation visitors (`transform`), the `SymbolParts` and
//! `TermParts` fold enums, and the raw operator enums (`term`). The construction
//! free functions — `not`, `minimize`, the rounding adapters — and the raise doors
//! — `raise`, `raise_source`, `raise_str` — are reached through their modules too;
//! the render doors, `render` and `render_documented`, are flat here (and at the
//! crate root), the tier's output door, so a client renders with no module path.
//!
//! # The one rule, and the cross-prelude no-collision property
//!
//! One rule decides what is flat here and what is reached by module path, and it is
//! that rule that lets this prelude be globbed beside `themelios_syntax::prelude::*`
//! with no ambiguity. Flat here are the doors and the infrastructure vocabulary:
//! the program IR (`Program`, `Rule`, `Statement`, `Atom`, `Direction`, …),
//! provenance, the symbol and term algebra, the render doors, the door refusals
//! (`EmptyPool`, `TooLarge`, `Unspellable`, `FromSymbolError`, …), and the base
//! source seam (`Source`, `SourceId`, `TooLarge`, and `base` as a module). NOT flat
//! — and this is what keeps the two preludes disjoint — are the syntax tier's
//! AST-family names and the helpers that would clash with a flat name here: the
//! typed AST shares this IR's very spellings and is reached through
//! `themelios_syntax::ast` (`ast::Program`, `ast::Rule`, `ast::Atom`, …), its
//! `ast::HasDocs`/`ast::HasGuards` stay behind `ast::` — `HasGuards` would collide
//! with this prelude's own — and rowan's `Direction` stays behind `tree::` because
//! this IR has a `Direction` of its own. So this prelude's flat-globbed set is
//! disjoint from `themelios_syntax::ast`, and a client that globs both preludes
//! meets no `E0659`: the shared spellings live behind `ast::` on the syntax side
//! and bare here, and the items both preludes re-export — `Dialect`, and `base` —
//! are one item each. The standing compile-lock is `tests/cross_prelude.rs`.

pub use crate::analyze::DependencyKind;
pub use crate::base;
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
pub use crate::raise::{LowerError, LowerErrorKind, Raised, RaisedSource};
pub use crate::render::{Unspellable, render, render_documented};
pub use crate::symbol::{
    FromSymbol, FromSymbolError, Name, NotAVariable, NotAnIdentifier, NotAnInteger, Segment, Sign,
    Signature, Symbol, ToSymbol, VarName,
};
pub use crate::term::{EmptyPool, EvalError, Term, Variable};
pub use themelios_base::source::{Source, SourceId, TooLarge};
pub use themelios_base::span::Location;
pub use themelios_syntax::dialect::Dialect;
