//! Aggregates and optimization (docs/design/program.md §4.7): the function and set
//! aggregates with their guards, the position-typed head-versus-body elements, and
//! `#minimize`/`#maximize`. An aggregate's elements are a set (§4); the terms within
//! an element are a sequence. Position is in the type, not a runtime tag: a
//! `FunctionAggregate` holds body elements and a `HeadAggregate` holds head elements,
//! so a body aggregate cannot hold a head element (§4.5's unrepresentability, §4.7).

use std::collections::BTreeSet;

use super::rule::{Condition, ConditionalLiteral, Literal, Relation};
use crate::provenance::WithProvenance;
use crate::term::Term;

/// A guard (grammar §5.3): a relation and a term. A `None` relation is the grammar's
/// default (`<=` on its side), stated as absence because that is what the author wrote
/// (§4.7).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Guard {
    /// The relation, or the grammar's default when absent.
    pub relation: Option<Relation>,
    /// The bound.
    pub term: Term,
}

/// Reads the two guards of a guarded aggregate uniformly (§4.7): the small structure a
/// `FunctionAggregate`, a `HeadAggregate`, and a `SetAggregate` share.
pub trait HasGuards {
    /// The left guard, if any.
    fn left_guard(&self) -> Option<&Guard>;
    /// The right guard, if any.
    fn right_guard(&self) -> Option<&Guard>;
}

/// An aggregate function (grammar §5.3). `SumPlus` is `#sum+`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AggregateFunction {
    /// `#count`.
    Count,
    /// `#sum`.
    Sum,
    /// `#sum+`.
    SumPlus,
    /// `#min`.
    Min,
    /// `#max`.
    Max,
}

/// An aggregate at body-element position (§4.7): a function aggregate over body elements
/// that *test*, or a set (cardinality) aggregate. A head function aggregate is a
/// `Head::Aggregate` (§4.4), a distinct type — the position lives here in the type.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Aggregate {
    /// A function aggregate — `#count`, `#sum`, and their kin.
    Function(FunctionAggregate),
    /// A set (cardinality) aggregate — `{ … }` in a body.
    Set(SetAggregate),
}

/// A body function aggregate (grammar §5.3): two guards, the function, and body elements
/// that *test* (§4.7). Its elements are a set (§4).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FunctionAggregate {
    left_guard: Option<Guard>,
    function: AggregateFunction,
    elements: BTreeSet<WithProvenance<BodyAggregateElement>>,
    right_guard: Option<Guard>,
}

impl FunctionAggregate {
    /// A body function aggregate over the given elements, each carrying a `Constructed`
    /// origin (§6.2). O(elements).
    pub fn new(
        left_guard: Option<Guard>,
        function: AggregateFunction,
        elements: impl IntoIterator<Item = BodyAggregateElement>,
        right_guard: Option<Guard>,
    ) -> FunctionAggregate {
        FunctionAggregate {
            left_guard,
            function,
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
            right_guard,
        }
    }

    /// The aggregate function.
    pub fn function(&self) -> AggregateFunction {
        self.function
    }

    /// The elements — a set, each with its provenance (§4, §6.2).
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<BodyAggregateElement>> {
        self.elements.iter()
    }
}

impl HasGuards for FunctionAggregate {
    fn left_guard(&self) -> Option<&Guard> {
        self.left_guard.as_ref()
    }
    fn right_guard(&self) -> Option<&Guard> {
        self.right_guard.as_ref()
    }
}

/// A head function aggregate (grammar §5.3): the same two guards and function over head
/// elements that *derive* (§4.4, §4.7). `Head::Aggregate(HeadAggregate)` at the keystone
/// task. Two distinct concrete types keep the taxonomy regular (§4.7).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct HeadAggregate {
    left_guard: Option<Guard>,
    function: AggregateFunction,
    elements: BTreeSet<WithProvenance<HeadAggregateElement>>,
    right_guard: Option<Guard>,
}

impl HeadAggregate {
    /// A head function aggregate over the given elements, each carrying a `Constructed`
    /// origin (§6.2). O(elements).
    pub fn new(
        left_guard: Option<Guard>,
        function: AggregateFunction,
        elements: impl IntoIterator<Item = HeadAggregateElement>,
        right_guard: Option<Guard>,
    ) -> HeadAggregate {
        HeadAggregate {
            left_guard,
            function,
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
            right_guard,
        }
    }

    /// The aggregate function.
    pub fn function(&self) -> AggregateFunction {
        self.function
    }

    /// The elements — a set, each with its provenance.
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<HeadAggregateElement>> {
        self.elements.iter()
    }
}

impl HasGuards for HeadAggregate {
    fn left_guard(&self) -> Option<&Guard> {
        self.left_guard.as_ref()
    }
    fn right_guard(&self) -> Option<&Guard> {
        self.right_guard.as_ref()
    }
}

/// A set (cardinality) aggregate (grammar §5.3): two guards over set elements — the body
/// `{ … }` form (§4.7). Its elements are a set (§4).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SetAggregate {
    left_guard: Option<Guard>,
    elements: BTreeSet<WithProvenance<SetElement>>,
    right_guard: Option<Guard>,
}

impl SetAggregate {
    /// A set aggregate over the given elements, each carrying a `Constructed` origin
    /// (§6.2). O(elements).
    pub fn new(
        left_guard: Option<Guard>,
        elements: impl IntoIterator<Item = SetElement>,
        right_guard: Option<Guard>,
    ) -> SetAggregate {
        SetAggregate {
            left_guard,
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
            right_guard,
        }
    }

    /// The elements — a set, each with its provenance.
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<SetElement>> {
        self.elements.iter()
    }
}

impl HasGuards for SetAggregate {
    fn left_guard(&self) -> Option<&Guard> {
        self.left_guard.as_ref()
    }
    fn right_guard(&self) -> Option<&Guard> {
        self.right_guard.as_ref()
    }
}

/// A body aggregate element (grammar §5.3): a term tuple under a condition. It *tests*,
/// so it carries no head literal — a `FunctionAggregate`'s element (§4.7).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BodyAggregateElement {
    terms: Vec<Term>,
    condition: Condition,
}

impl BodyAggregateElement {
    /// A body element over the given terms and condition, the terms canonicalized at the
    /// door (§5.1). O(terms).
    pub fn new(
        terms: impl IntoIterator<Item = Term>,
        condition: Condition,
    ) -> BodyAggregateElement {
        BodyAggregateElement {
            terms: terms.into_iter().map(Term::canonicalize).collect(),
            condition,
        }
    }

    /// The term tuple, in order (§4).
    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.terms.iter()
    }

    /// The condition.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
}

/// A head aggregate element (grammar §5.3): a term tuple, the literal it *derives*, and a
/// condition. The literal is what makes it a *head* element, and it exists only here, so
/// a `FunctionAggregate` cannot hold one (§4.7).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct HeadAggregateElement {
    terms: Vec<Term>,
    literal: Literal,
    condition: Condition,
}

impl HeadAggregateElement {
    /// A head element over the given terms, derived literal, and condition, the terms
    /// canonicalized at the door (§5.1). O(terms).
    pub fn new(
        terms: impl IntoIterator<Item = Term>,
        literal: Literal,
        condition: Condition,
    ) -> HeadAggregateElement {
        HeadAggregateElement {
            terms: terms.into_iter().map(Term::canonicalize).collect(),
            literal,
            condition,
        }
    }

    /// The term tuple, in order (§4).
    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.terms.iter()
    }

    /// The literal this element derives.
    pub fn literal(&self) -> &Literal {
        &self.literal
    }

    /// The condition.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
}

/// A set aggregate element (grammar §5.3): a literal or a conditional literal (§4.7).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SetElement {
    /// A bare literal.
    Literal(Literal),
    /// A conditional literal.
    ConditionalLiteral(ConditionalLiteral),
}

/// Optimization by `#minimize`/`#maximize` (grammar §5.7). The direction is a tag; the
/// maximize-to-minimize desugaring (by negating weights) is the solve tier's (§4.2), so
/// it is kept structural here and `i32::MIN` never overflows. Its elements are a set.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Optimize {
    /// The direction.
    pub direction: Direction,
    elements: BTreeSet<WithProvenance<OptimizeElement>>,
}

impl Optimize {
    /// An optimization statement over the given elements, each carrying a `Constructed`
    /// origin (§6.2). O(elements).
    pub fn new(
        direction: Direction,
        elements: impl IntoIterator<Item = OptimizeElement>,
    ) -> Optimize {
        Optimize {
            direction,
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
        }
    }

    /// The elements — a set, each with its provenance.
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<OptimizeElement>> {
        self.elements.iter()
    }
}

/// The optimization direction (grammar §5.7).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Direction {
    /// `#minimize`.
    Minimize,
    /// `#maximize`.
    Maximize,
}

/// An optimize element (grammar §5.7): a weight, an optional priority, a term tuple, and
/// a condition. The weight, priority, and terms canonicalize at the door (§5.1).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct OptimizeElement {
    weight: Term,
    priority: Option<Term>,
    terms: Vec<Term>,
    condition: Condition,
}

impl OptimizeElement {
    /// An optimize element, its weight, priority, and terms canonicalized at the door
    /// (§5.1). O(terms).
    pub fn new(
        weight: impl Into<Term>,
        priority: Option<Term>,
        terms: impl IntoIterator<Item = Term>,
        condition: Condition,
    ) -> OptimizeElement {
        OptimizeElement {
            weight: weight.into().canonicalize(),
            priority: priority.map(Term::canonicalize),
            terms: terms.into_iter().map(Term::canonicalize).collect(),
            condition,
        }
    }

    /// The weight term.
    pub fn weight(&self) -> &Term {
        &self.weight
    }

    /// The priority term, if any.
    pub fn priority(&self) -> Option<&Term> {
        self.priority.as_ref()
    }

    /// The term tuple, in order (§4).
    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.terms.iter()
    }

    /// The condition.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
}

// ---- Canonicalization (§5.1) ----
//
// The aggregate spine of the pass (see `rule.rs`): guard bounds and the ordinary terms
// and conditions an element carries are canonicalized, provenance preserved through the
// carrier's `map` (§6.2). Grammar-bounded, so a bounded recursion (§13).

impl Guard {
    pub(crate) fn canonicalize(self) -> Guard {
        Guard {
            relation: self.relation,
            term: self.term.canonicalize(),
        }
    }
}

impl Aggregate {
    pub(crate) fn canonicalize(self) -> Aggregate {
        match self {
            Aggregate::Function(aggregate) => Aggregate::Function(aggregate.canonicalize()),
            Aggregate::Set(aggregate) => Aggregate::Set(aggregate.canonicalize()),
        }
    }
}

impl FunctionAggregate {
    pub(crate) fn canonicalize(self) -> FunctionAggregate {
        FunctionAggregate {
            left_guard: self.left_guard.map(Guard::canonicalize),
            function: self.function,
            elements: self
                .elements
                .into_iter()
                .map(|element| element.map(BodyAggregateElement::canonicalize))
                .collect(),
            right_guard: self.right_guard.map(Guard::canonicalize),
        }
    }
}

impl HeadAggregate {
    pub(crate) fn canonicalize(self) -> HeadAggregate {
        HeadAggregate {
            left_guard: self.left_guard.map(Guard::canonicalize),
            function: self.function,
            elements: self
                .elements
                .into_iter()
                .map(|element| element.map(HeadAggregateElement::canonicalize))
                .collect(),
            right_guard: self.right_guard.map(Guard::canonicalize),
        }
    }
}

impl SetAggregate {
    pub(crate) fn canonicalize(self) -> SetAggregate {
        SetAggregate {
            left_guard: self.left_guard.map(Guard::canonicalize),
            elements: self
                .elements
                .into_iter()
                .map(|element| element.map(SetElement::canonicalize))
                .collect(),
            right_guard: self.right_guard.map(Guard::canonicalize),
        }
    }
}

impl BodyAggregateElement {
    pub(crate) fn canonicalize(self) -> BodyAggregateElement {
        BodyAggregateElement {
            terms: self.terms.into_iter().map(Term::canonicalize).collect(),
            condition: self.condition.canonicalize(),
        }
    }
}

impl HeadAggregateElement {
    pub(crate) fn canonicalize(self) -> HeadAggregateElement {
        HeadAggregateElement {
            terms: self.terms.into_iter().map(Term::canonicalize).collect(),
            literal: self.literal.canonicalize(),
            condition: self.condition.canonicalize(),
        }
    }
}

impl SetElement {
    pub(crate) fn canonicalize(self) -> SetElement {
        match self {
            SetElement::Literal(literal) => SetElement::Literal(literal.canonicalize()),
            SetElement::ConditionalLiteral(conditional) => {
                SetElement::ConditionalLiteral(conditional.canonicalize())
            }
        }
    }
}

impl Optimize {
    pub(crate) fn canonicalize(self) -> Optimize {
        Optimize {
            direction: self.direction,
            elements: self
                .elements
                .into_iter()
                .map(|element| element.map(OptimizeElement::canonicalize))
                .collect(),
        }
    }
}

impl OptimizeElement {
    pub(crate) fn canonicalize(self) -> OptimizeElement {
        OptimizeElement {
            weight: self.weight.canonicalize(),
            priority: self.priority.map(Term::canonicalize),
            terms: self.terms.into_iter().map(Term::canonicalize).collect(),
            condition: self.condition.canonicalize(),
        }
    }
}
