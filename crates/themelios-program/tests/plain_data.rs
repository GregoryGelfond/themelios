//! The owned-plain-data assurance (docs/design/program.md §1, §14; base §8's plain-data
//! rule): every public value type this tier hands out is `Send + Sync + 'static` and holds
//! no borrow, so a program is constructed on one thread and solved on another, transformed
//! and kept, without a lifetime. The proof is **structural, not by inspection**: the generic
//! `assert_owned_plain_data::<T>()` compiles only when `T: Send + Sync + 'static`, and this
//! file instantiates it over the whole public surface, so a type that grew a borrow, an `Rc`,
//! or a raw pointer would fail to compile here rather than pass a reviewer's eye. The
//! `'static` bound is the "holds no borrow" half; `Send + Sync` is the cross-thread half.
//!
//! Coverage is the *entire* surface, not a sample — a missed type is a silently unasserted
//! one — enumerated module by module: the ground vocabulary (`symbol`), the term algebra
//! (`term`), provenance (`provenance`), the `Program` value and its every node (`program`),
//! the substrate `DependencyKind` (`analyze`), the raise's diagnostic and result (`raise`),
//! the render refusal (`render`), and the unification vocabulary (`unify`). The four generic
//! carriers (`WithProvenance<T>`, `TermParts<T>`, `SymbolParts<T>`, `TheoryTermParts<T>`) are
//! instantiated at owned payloads, since a carrier is plain data exactly when its payload is.

use themelios_program::analyze::DependencyKind;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, Const, ConstPolicy, DefaultNegation,
    Defined, Direction, Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard,
    Head, HeadAggregate, HeadAggregateElement, Heuristic, Include, IncludeTarget, Literal,
    LiteralInner, Optimize, OptimizeElement, Part, PartKey, Program, Project, Query, Relation,
    Rule, Script, SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryAtomDefinition,
    TheoryAtomGuardDefinition, TheoryDefinition, TheoryElement, TheoryGuard, TheoryOccurrence,
    TheoryOperator, TheoryOperatorArity, TheoryOperatorDefinition, TheoryTerm,
    TheoryTermDefinition, TheoryTermParts, WeakConstraint, Weight,
};
use themelios_program::provenance::{
    Annotations, Origin, Provenance, TransformTag, WithProvenance,
};
use themelios_program::raise::{LowerError, LowerErrorKind, Raised};
use themelios_program::render::Unspellable;
use themelios_program::symbol::{
    FromSymbolError, Name, NotAVariable, NotAnIdentifier, NotAnInteger, Sign, Signature, Symbol,
    SymbolParts, VarName,
};
use themelios_program::term::{BinaryOp, EvalError, Term, TermParts, UnaryOp, Variable};
use themelios_program::unify::{Binding, Fresh, NotAPattern, Substitution};

/// Compiles only for an owned, thread-safe, borrow-free type. Instantiating it *is* the
/// assertion (§14): the bound is checked at monomorphization, so the whole file compiling is
/// the proof for every type below.
fn assert_owned_plain_data<T: Send + Sync + 'static>() {}

#[test]
fn every_public_value_type_is_owned_plain_data() {
    // The ground vocabulary (§3.1, §3.2, §3.4).
    assert_owned_plain_data::<Sign>();
    assert_owned_plain_data::<Symbol>();
    assert_owned_plain_data::<Name>();
    assert_owned_plain_data::<VarName>();
    assert_owned_plain_data::<Signature>();
    assert_owned_plain_data::<SymbolParts<Symbol>>();
    assert_owned_plain_data::<NotAnIdentifier>();
    assert_owned_plain_data::<NotAVariable>();
    assert_owned_plain_data::<NotAnInteger>();
    assert_owned_plain_data::<FromSymbolError>();

    // The term algebra (§3.3, §3.5, §3.6).
    assert_owned_plain_data::<Term>();
    assert_owned_plain_data::<Variable>();
    assert_owned_plain_data::<UnaryOp>();
    assert_owned_plain_data::<BinaryOp>();
    assert_owned_plain_data::<TermParts<Term>>();
    assert_owned_plain_data::<EvalError>();

    // Provenance as in-node model data (§6).
    assert_owned_plain_data::<WithProvenance<Statement>>();
    assert_owned_plain_data::<Provenance>();
    assert_owned_plain_data::<Origin>();
    assert_owned_plain_data::<TransformTag>();
    assert_owned_plain_data::<Annotations>();

    // The Program value and its every structural node (§4).
    assert_owned_plain_data::<Program>();
    assert_owned_plain_data::<Part>();
    assert_owned_plain_data::<PartKey>();
    assert_owned_plain_data::<Statement>();
    assert_owned_plain_data::<Query>();
    assert_owned_plain_data::<Rule>();
    assert_owned_plain_data::<WeakConstraint>();
    assert_owned_plain_data::<Head>();
    assert_owned_plain_data::<Body>();
    assert_owned_plain_data::<BodyElement>();
    assert_owned_plain_data::<Literal>();
    assert_owned_plain_data::<LiteralInner>();
    assert_owned_plain_data::<DefaultNegation>();
    assert_owned_plain_data::<Atom>();
    assert_owned_plain_data::<Comparison>();
    assert_owned_plain_data::<Relation>();
    assert_owned_plain_data::<Condition>();
    assert_owned_plain_data::<ConditionalLiteral>();
    assert_owned_plain_data::<Disjunction>();
    assert_owned_plain_data::<DisjunctionElement>();
    assert_owned_plain_data::<Choice>();
    assert_owned_plain_data::<ChoiceElement>();

    // The aggregate family (§4.7).
    assert_owned_plain_data::<Aggregate>();
    assert_owned_plain_data::<AggregateFunction>();
    assert_owned_plain_data::<FunctionAggregate>();
    assert_owned_plain_data::<HeadAggregate>();
    assert_owned_plain_data::<SetAggregate>();
    assert_owned_plain_data::<BodyAggregateElement>();
    assert_owned_plain_data::<HeadAggregateElement>();
    assert_owned_plain_data::<SetElement>();
    assert_owned_plain_data::<Guard>();
    assert_owned_plain_data::<Optimize>();
    assert_owned_plain_data::<OptimizeElement>();
    assert_owned_plain_data::<Direction>();
    assert_owned_plain_data::<Weight>();

    // The directives, including the theory family (§4.8, §4.9).
    assert_owned_plain_data::<Const>();
    assert_owned_plain_data::<ConstPolicy>();
    assert_owned_plain_data::<Defined>();
    assert_owned_plain_data::<Edge>();
    assert_owned_plain_data::<External>();
    assert_owned_plain_data::<Heuristic>();
    assert_owned_plain_data::<Include>();
    assert_owned_plain_data::<IncludeTarget>();
    assert_owned_plain_data::<Project>();
    assert_owned_plain_data::<Script>();
    assert_owned_plain_data::<Show>();
    assert_owned_plain_data::<TheoryTerm>();
    assert_owned_plain_data::<TheoryTermParts<TheoryTerm>>();
    assert_owned_plain_data::<TheoryOperator>();
    assert_owned_plain_data::<TheoryOperatorArity>();
    assert_owned_plain_data::<TheoryOccurrence>();
    assert_owned_plain_data::<TheoryElement>();
    assert_owned_plain_data::<TheoryGuard>();
    assert_owned_plain_data::<TheoryAtom>();
    assert_owned_plain_data::<TheoryOperatorDefinition>();
    assert_owned_plain_data::<TheoryTermDefinition>();
    assert_owned_plain_data::<TheoryAtomGuardDefinition>();
    assert_owned_plain_data::<TheoryAtomDefinition>();
    assert_owned_plain_data::<TheoryDefinition>();

    // The analysis-facing substrate (§12.1).
    assert_owned_plain_data::<DependencyKind>();

    // The raise: the lowering diagnostic (a value on a total raise, §8) and its result.
    assert_owned_plain_data::<LowerError>();
    assert_owned_plain_data::<LowerErrorKind>();
    assert_owned_plain_data::<Raised>();

    // The one render refusal (§10).
    assert_owned_plain_data::<Unspellable>();

    // The unification vocabulary (§11): the triangular substitution, the fresh source, a
    // binding, and the pattern refusal.
    assert_owned_plain_data::<Substitution>();
    assert_owned_plain_data::<Fresh>();
    assert_owned_plain_data::<Binding>();
    assert_owned_plain_data::<NotAPattern>();
}
