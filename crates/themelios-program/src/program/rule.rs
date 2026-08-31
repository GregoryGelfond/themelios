//! Rules and their parts (docs/design/program.md §4.3–§4.6): the literal core — atoms
//! with their strong sign, the guarded comparison chain, literals under default negation,
//! conditions, and conditional literals — and the heads, bodies, and rules built over
//! them. Each type is grammar-bounded and derives its identity over content (§13), with
//! provenance-bearing children wrapped in `WithProvenance` (§6.2).

use std::collections::BTreeSet;

use super::aggregate::{Aggregate, Guard, HeadAggregate, Weight};
use super::directive::TheoryAtom;
use crate::provenance::WithProvenance;
use crate::symbol::{Name, Sign};
use crate::term::Term;

/// Default negation (grammar §5.2, §5.6). `NotNot` is its own case — double default
/// negation is not the identity under the stable-model semantics, so a name that
/// collapsed it would lie (§4). Distinct in the type from `Sign` (strong negation on
/// an atom) and from `UnaryOp::BitwiseNot` (a term operator): three different things.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DefaultNegation {
    /// No default negation.
    None,
    /// `not`.
    Not,
    /// `not not`.
    NotNot,
}

/// A literal (grammar §5.2): a default-negation prefix over an atom, a comparison, or
/// a boolean constant (§4.6).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Literal {
    /// The default negation.
    pub negation: DefaultNegation,
    /// What it negates.
    pub inner: LiteralInner,
}

/// What a literal is over (§4.6): an atom, a comparison, or a boolean constant. The
/// atom and comparison carry provenance through the `WithProvenance` carrier (§6.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum LiteralInner {
    /// A signed atom.
    Atom(WithProvenance<Atom>),
    /// A comparison chain.
    Comparison(WithProvenance<Comparison>),
    /// `#true`.
    True,
    /// `#false`.
    False,
}

/// An atom (grammar §5.2): a strong sign, a predicate name, and an argument list that
/// is one tuple or — an argument-list pool `p(a; b)` — several (§8). The sign is
/// **strong** negation (`-p`); the default negation is the literal's (§4.6). Its
/// argument terms canonicalize, and a one-alternative pool collapses to `Single`, when
/// the atom passes an ingest door (§5.1, §7.2, §9).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Atom {
    /// The strong sign.
    pub sign: Sign,
    /// The predicate name.
    pub name: Name,
    /// The argument list — one tuple (`Single`) or an argument-list pool.
    pub arguments: Arguments,
}

/// An atom's arguments (§8): one tuple, or an argument-list pool of two or more tuples
/// whose alternatives may differ in arity (`p(a; b, c)` is p/1 and p/2). A pool of
/// whole tuples is the atom's own shape, not a [`Term::Pool`] (one argument position) —
/// the one pooling concept at the level the term/atom split gives it (§4.6). The two-or-
/// more normal form is canonicalization's (§5.1): a one-alternative pool collapses to
/// `Single`, an empty one is refused at the constructor door ([`Atom::pooled`](crate::program::Atom::pooled)
/// returns `Err`) — the ≥2 shape is a normal form, so this stays a public value like `Term::Pool`,
/// which carries the same non-empty precondition.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Arguments {
    /// One argument tuple, `p(a, b)`.
    Single(Vec<Term>),
    /// An argument-list pool, `p(a; b)` — two or more alternatives.
    Pooled(Vec<Vec<Term>>),
}

impl Arguments {
    /// Map every term, preserving the `Single`/`Pooled` structure (§9): the rewrite and
    /// substitution rebuild through this, so neither collapses a pool nor promotes a
    /// single tuple to one.
    pub(crate) fn map_terms(&self, mut f: impl FnMut(&Term) -> Term) -> Arguments {
        match self {
            Arguments::Single(terms) => Arguments::Single(terms.iter().map(&mut f).collect()),
            Arguments::Pooled(alternatives) => Arguments::Pooled(
                alternatives
                    .iter()
                    .map(|tuple| tuple.iter().map(&mut f).collect())
                    .collect(),
            ),
        }
    }
}

impl Atom {
    /// The argument-list alternatives: one for a `Single` atom, several for a pool. The
    /// structural accessor (§4.6) — render, the rewrite, and `unpool` (§9) read it.
    /// There is deliberately **no** accessor for "the single argument list": a pool is
    /// unpooled before it is read as one tuple, so no consumer silently truncates it.
    pub fn alternatives(&self) -> impl Iterator<Item = &[Term]> {
        match &self.arguments {
            Arguments::Single(terms) => std::slice::from_ref(terms),
            Arguments::Pooled(alternatives) => alternatives.as_slice(),
        }
        .iter()
        .map(Vec::as_slice)
    }

    /// Every argument term across every alternative, flattened — for consumers that walk
    /// terms without needing the tuple boundaries (§4.6). Never `zip` two atoms'
    /// alternatives (it truncates to the shorter): match [`Arguments`] for a pairwise walk.
    pub fn argument_terms(&self) -> impl Iterator<Item = &Term> {
        self.alternatives().flatten()
    }

    /// Whether the atom is an argument-list pool (two or more alternatives).
    pub fn is_pooled(&self) -> bool {
        matches!(self.arguments, Arguments::Pooled(_))
    }
}

/// A comparison chain (grammar §5.2): a first term and **one or more** relation/term
/// steps. `1 < X < 5` is one literal carrying a guard sequence, not a conjunction. The
/// at-least-one-step invariant is guarded — the fields are private, so an empty chain,
/// which the grammar does not admit, is unrepresentable (§4.6).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Comparison {
    first: Term,
    steps: Vec<(Relation, Term)>,
}

impl Comparison {
    /// A one-step chain `first R second`. The terms canonicalize at the door — the
    /// pass discipline (§5.1); this is the first constructor door that canonicalizes,
    /// and every later door repeats it (§7.2). O(terms).
    pub fn new(first: impl Into<Term>, relation: Relation, second: impl Into<Term>) -> Comparison {
        Comparison {
            first: first.into().canonicalize(),
            steps: vec![(relation, second.into().canonicalize())],
        }
    }

    /// Extend the chain by one further relation/term step (`… < 5`), canonicalizing
    /// the term at the door (§5.1). O(term).
    #[must_use]
    pub fn chain(mut self, relation: Relation, term: impl Into<Term>) -> Comparison {
        self.steps.push((relation, term.into().canonicalize()));
        self
    }

    /// The first term of the chain.
    pub fn first(&self) -> &Term {
        &self.first
    }

    /// The relation/term steps — one or more, in order.
    pub fn steps(&self) -> impl Iterator<Item = (Relation, &Term)> {
        self.steps.iter().map(|(relation, term)| (*relation, term))
    }

    /// The first term and the relation/term steps, owned — the complement to
    /// [`first`](Comparison::first) and [`steps`](Comparison::steps)'s borrows, for a
    /// by-value rewrite (§9.1).
    pub(crate) fn into_parts(self) -> (Term, Vec<(Relation, Term)>) {
        (self.first, self.steps)
    }
}

/// A comparison relation (grammar §5.2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Relation {
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
    /// `=`.
    Eq,
    /// `!=`.
    Neq,
}

/// A condition (grammar §5.4): the literals after a `:`, a sequence — present and
/// empty when the colon is (`p : .`). Its literals carry provenance (§6.2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Condition {
    literals: Vec<WithProvenance<Literal>>,
}

impl Condition {
    /// A condition over the given literals, each carrying a `Constructed` origin
    /// (§6.2); the raise gives them parsed origins (§8). O(literals).
    pub fn new(literals: impl IntoIterator<Item = Literal>) -> Condition {
        Condition {
            literals: literals
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
        }
    }

    /// A condition over already-provenanced literals — the raise's door, carrying
    /// each literal's parsed origin (§6.2, §8). A sequence, so order is kept and no
    /// merge is owed. O(literals).
    pub(crate) fn from_nodes(
        literals: impl IntoIterator<Item = WithProvenance<Literal>>,
    ) -> Condition {
        Condition {
            literals: literals.into_iter().collect(),
        }
    }

    /// The empty condition (also `Default`).
    pub fn empty() -> Condition {
        Condition::default()
    }

    /// The literals, in order, each with its provenance (§6.2).
    pub fn literals(&self) -> impl Iterator<Item = &WithProvenance<Literal>> {
        self.literals.iter()
    }

    /// Whether the condition holds no literals.
    pub fn is_empty(&self) -> bool {
        self.literals.is_empty()
    }

    /// The literals, owned with their provenance — the complement to
    /// [`literals`](Condition::literals)'s borrow, for a by-value rewrite (§9.1).
    pub(crate) fn into_literals(self) -> impl Iterator<Item = WithProvenance<Literal>> {
        self.literals.into_iter()
    }
}

/// A conditional literal (grammar §5.4): a literal under a condition (§4.6).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConditionalLiteral {
    /// The literal.
    pub literal: Literal,
    /// The condition it holds under.
    pub condition: Condition,
}

/// A rule head (grammar §5.5). `Falsum` is the head of a constraint — `:- body.` or
/// `#false :- body.`, one head ⊥; `Verum` is `#true`, which the engine grounds (§4.4).
/// `Choice` and `Disjunction` are **distinct** for a model-theoretic reason (`a | b`
/// has answer sets `{a}`, `{b}`; `{ a; b }` has `∅`, `{a}`, `{b}`, `{a, b}`); a head
/// aggregate is a function aggregate (a set head is a `Choice`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Head {
    /// A single literal head.
    Literal(Literal),
    /// A disjunction, `a | b`.
    Disjunction(Disjunction),
    /// A choice, `{ a; b }`.
    Choice(Choice),
    /// A head function aggregate deriving atoms (grammar §5.3).
    Aggregate(HeadAggregate),
    /// A head theory atom (grammar §5.8), unsigned.
    TheoryAtom(TheoryAtom),
    /// ⊥ — the head of a constraint.
    Falsum,
    /// ⊤ — `#true`.
    Verum,
}

/// A disjunctive head (grammar §5.5): a set of conditioned literals, `a | b | …`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Disjunction {
    elements: BTreeSet<WithProvenance<DisjunctionElement>>,
}

impl Disjunction {
    /// A disjunction over the given elements, each carrying a `Constructed` origin (§6.2).
    pub fn new(elements: impl IntoIterator<Item = DisjunctionElement>) -> Disjunction {
        Disjunction {
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
        }
    }

    /// A disjunction over already-provenanced elements, unioning provenance on any
    /// content collision (§6.3) — the raise's door, carrying each element's parsed
    /// origin (§6.2, §8). O(elements).
    pub(crate) fn from_nodes(
        elements: impl IntoIterator<Item = WithProvenance<DisjunctionElement>>,
    ) -> Disjunction {
        Disjunction {
            elements: super::merge_collect(elements),
        }
    }

    /// The elements — a set, each with its provenance (§6.2).
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<DisjunctionElement>> {
        self.elements.iter()
    }
}

/// A disjunction element (grammar §5.5): a literal under an optional condition — the
/// singleton conditioned head `p(X) : q(X)` among them (§4.4).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DisjunctionElement {
    literal: Literal,
    condition: Condition,
}

impl DisjunctionElement {
    /// A disjunction element.
    pub fn new(literal: Literal, condition: Condition) -> DisjunctionElement {
        DisjunctionElement { literal, condition }
    }

    /// The derived literal.
    pub fn literal(&self) -> &Literal {
        &self.literal
    }

    /// The condition.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
}

/// A choice head (grammar §5.3): two optional guards over a set of conditioned literals,
/// `1 { a; b } 2` (§4.4).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Choice {
    left_guard: Option<WithProvenance<Guard>>,
    elements: BTreeSet<WithProvenance<ChoiceElement>>,
    right_guard: Option<WithProvenance<Guard>>,
}

impl Choice {
    /// A choice over the given guards and elements, each element carrying a `Constructed`
    /// origin — and each guard likewise (§6.2).
    pub fn new(
        left_guard: Option<Guard>,
        elements: impl IntoIterator<Item = ChoiceElement>,
        right_guard: Option<Guard>,
    ) -> Choice {
        Choice {
            left_guard: left_guard.map(WithProvenance::constructed),
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
            right_guard: right_guard.map(WithProvenance::constructed),
        }
    }

    /// A choice over already-provenanced elements and guards, unioning provenance on any
    /// content collision (§6.3) — the raise's door for a head set form (§4.4, §8),
    /// carrying each element's and guard's parsed origin (§6.2). O(elements).
    pub(crate) fn from_nodes(
        left_guard: Option<WithProvenance<Guard>>,
        elements: impl IntoIterator<Item = WithProvenance<ChoiceElement>>,
        right_guard: Option<WithProvenance<Guard>>,
    ) -> Choice {
        Choice {
            left_guard,
            elements: super::merge_collect(elements),
            right_guard,
        }
    }

    /// The left guard, with its provenance, if any (§6.2).
    pub fn left_guard(&self) -> Option<&WithProvenance<Guard>> {
        self.left_guard.as_ref()
    }

    /// The right guard, with its provenance, if any (§6.2).
    pub fn right_guard(&self) -> Option<&WithProvenance<Guard>> {
        self.right_guard.as_ref()
    }

    /// The elements — a set, each with its provenance (§6.2).
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<ChoiceElement>> {
        self.elements.iter()
    }
}

/// A choice element (grammar §5.3): a literal under an optional condition (§4.4).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChoiceElement {
    literal: Literal,
    condition: Condition,
}

impl ChoiceElement {
    /// A choice element.
    pub fn new(literal: Literal, condition: Condition) -> ChoiceElement {
        ChoiceElement { literal, condition }
    }

    /// The literal.
    pub fn literal(&self) -> &Literal {
        &self.literal
    }

    /// The condition.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
}

/// A rule body: a conjunction, hence a set (grammar §5.6). Its one filter axis is the
/// **default-negation partition** (the reduct's B⁺/B⁻, §4.5).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Body {
    elements: BTreeSet<WithProvenance<BodyElement>>,
}

impl Body {
    /// A body over the given elements, each carrying a `Constructed` origin (§6.2).
    pub fn new(elements: impl IntoIterator<Item = BodyElement>) -> Body {
        Body {
            elements: elements
                .into_iter()
                .map(WithProvenance::constructed)
                .collect(),
        }
    }

    /// A body over already-provenanced elements, unioning provenance on any content
    /// collision (§6.3) — the raise's door, carrying each element's parsed origin
    /// (§6.2, §8). O(elements).
    pub(crate) fn from_nodes(
        elements: impl IntoIterator<Item = WithProvenance<BodyElement>>,
    ) -> Body {
        Body {
            elements: super::merge_collect(elements),
        }
    }

    /// The empty body (also `Default`).
    pub fn empty() -> Body {
        Body::default()
    }

    /// The elements — a set, each with its provenance (§6.2).
    pub fn elements(&self) -> impl Iterator<Item = &WithProvenance<BodyElement>> {
        self.elements.iter()
    }

    /// The elements, owned with their provenance — the complement to
    /// [`elements`](Body::elements)'s borrow, for a by-value rewrite (§9.1).
    pub(crate) fn into_elements(self) -> impl Iterator<Item = WithProvenance<BodyElement>> {
        self.elements.into_iter()
    }

    /// Whether the body holds no elements.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// The **positive** partition — B⁺ of the reduct (§4.5): the elements *not* under
    /// default negation, over every element kind.
    pub fn positive(&self) -> impl Iterator<Item = &WithProvenance<BodyElement>> {
        self.elements
            .iter()
            .filter(|element| !is_default_negated(element.get()))
    }

    /// The **negative** partition — B⁻ of the reduct (§4.5): the elements under default
    /// negation (`not`/`not not`), a `not`-ed aggregate among them.
    pub fn negative(&self) -> impl Iterator<Item = &WithProvenance<BodyElement>> {
        self.elements
            .iter()
            .filter(|element| is_default_negated(element.get()))
    }
}

/// Whether a body element carries default negation at its top level — its B⁺/B⁻ axis
/// (§4.5). This is *default* negation (`not`), not `Sign::Negative` (strong negation on
/// an atom, §4.6).
fn is_default_negated(element: &BodyElement) -> bool {
    match element {
        BodyElement::Literal(literal) => literal.negation != DefaultNegation::None,
        BodyElement::Conditional(conditional) => {
            conditional.literal.negation != DefaultNegation::None
        }
        BodyElement::Aggregate { negation, .. } | BodyElement::TheoryAtom { negation, .. } => {
            *negation != DefaultNegation::None
        }
    }
}

/// The two-tier body (grammar §5.6). A literal and a conditional literal are one tier; an
/// aggregate and a theory atom, which may stand **only** at body-element position, are the
/// other, carrying their own default negation. Keeping the aggregate and theory atom out
/// of `Literal` is what makes `p(X) : #count{…}` unrepresentable (§4.5).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum BodyElement {
    /// A literal.
    Literal(Literal),
    /// A conditional literal.
    Conditional(ConditionalLiteral),
    /// A negatable aggregate.
    Aggregate {
        /// The default negation.
        negation: DefaultNegation,
        /// The aggregate.
        aggregate: Aggregate,
    },
    /// A negatable theory atom.
    TheoryAtom {
        /// The default negation.
        negation: DefaultNegation,
        /// The theory atom.
        atom: TheoryAtom,
    },
}

/// A value that coerces to a rule head (§7.1) — the coercion born here, widened by the
/// construction surface (§7).
pub trait IntoHead {
    /// The rule head this value denotes.
    fn into_head(self) -> Head;
}

impl IntoHead for Head {
    fn into_head(self) -> Head {
        self
    }
}
impl IntoHead for Literal {
    fn into_head(self) -> Head {
        Head::Literal(self)
    }
}

/// A value that coerces to a rule body (§7.1) — the coercion born here, widened by the
/// construction surface (§7).
pub trait IntoBody {
    /// The rule body this value denotes.
    fn into_body(self) -> Body;
}

impl IntoBody for Body {
    fn into_body(self) -> Body {
        self
    }
}

/// A rule: a head and a body (grammar §5.7). A *fact* is the shape "a single literal head,
/// an empty body"; a *constraint* is the shape "a falsum head". One type — a constraint
/// **is** a rule (`⊥ ← body`) and a fact **is** a rule (`h ← ⊤`) (§4.3).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Rule {
    head: WithProvenance<Head>,
    body: WithProvenance<Body>,
}

impl Rule {
    /// A rule from a head and a body — total, since a `Head` and a `Body` are already
    /// well-formed (§7.2). Each part carries a `Constructed` origin (§6.2). The
    /// program-level canonicalization (the boolean-head fold, §5.1) runs when the rule
    /// enters a program (the ingest door).
    pub fn new(head: impl IntoHead, body: impl IntoBody) -> Rule {
        Rule {
            head: WithProvenance::constructed(head.into_head()),
            body: WithProvenance::constructed(body.into_body()),
        }
    }

    /// A rule over an already-provenanced head and body — the raise's door,
    /// carrying the parsed origin of each (§6.2, §8). The program-level
    /// canonicalization runs at the ingest door (§6.3).
    pub(crate) fn from_nodes(head: WithProvenance<Head>, body: WithProvenance<Body>) -> Rule {
        Rule { head, body }
    }

    /// The head, with its provenance (§6.2).
    pub fn head(&self) -> &WithProvenance<Head> {
        &self.head
    }

    /// The body, with its provenance (§6.2).
    pub fn body(&self) -> &WithProvenance<Body> {
        &self.body
    }

    /// The head and body carriers, owned — the complement to [`head`](Rule::head) and
    /// [`body`](Rule::body)'s borrows, for a by-value rewrite that reuses each carrier's
    /// provenance rather than clone it (§6.2, §9.2).
    pub(crate) fn into_parts(self) -> (WithProvenance<Head>, WithProvenance<Body>) {
        (self.head, self.body)
    }

    /// Whether the rule is a fact — a single-literal head and an empty body (§4.3).
    pub fn is_fact(&self) -> bool {
        matches!(self.head.get(), Head::Literal(_)) && self.body.get().is_empty()
    }

    /// Whether the rule is a constraint — a falsum head (§4.3).
    pub fn is_constraint(&self) -> bool {
        matches!(self.head.get(), Head::Falsum)
    }
}

/// A weak constraint (grammar §5.7): a body and a bracket of a weight at a priority and a
/// term tuple. Distinct from `Optimize` (§4.2) — the two written forms of optimization are
/// kept structurally distinct, one of §5's equality carve-outs.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct WeakConstraint {
    body: WithProvenance<Body>,
    weight: Weight,
    terms: Vec<Term>,
}

impl WeakConstraint {
    /// A weak constraint over the given body, `weight(w).at_priority(p)`, and term tuple,
    /// its terms canonicalized at the door (§5.1); the weight carries its own, and the
    /// body a `Constructed` origin (§6.2). O(terms).
    pub fn new(
        body: Body,
        weight: Weight,
        terms: impl IntoIterator<Item = Term>,
    ) -> WeakConstraint {
        WeakConstraint {
            body: WithProvenance::constructed(body),
            weight,
            terms: terms.into_iter().map(Term::canonicalize).collect(),
        }
    }

    /// A weak constraint over an already-provenanced body — the raise's door, carrying
    /// the body's parsed origin (§6.2, §8). Canonicalization runs at the ingest door (§6.3).
    pub(crate) fn from_nodes(
        body: WithProvenance<Body>,
        weight: Weight,
        terms: Vec<Term>,
    ) -> WeakConstraint {
        WeakConstraint {
            body,
            weight,
            terms,
        }
    }

    /// The body, with its provenance (§6.2).
    pub fn body(&self) -> &WithProvenance<Body> {
        &self.body
    }

    /// The weight at its priority level.
    pub fn weight(&self) -> &Weight {
        &self.weight
    }

    /// The term tuple, in order (§4).
    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.terms.iter()
    }
}

// ---- Canonicalization (§5.1) ----
//
// The structural half of the pass: it descends the grammar-bounded spine and applies
// `Term::canonicalize` (§3.6, iterative) at every term position — an `Atom`'s arguments
// and a `Guard`'s bound have no constructor door of their own — and folds the boolean
// heads (§4.4). Each method is self-contained (it re-canonicalizes even the terms an
// element door already collapsed, which is idempotent), so `canonicalize` produces a
// fully canonical value whatever the input's prior state. The descent is a bounded
// recursion crossing the grammar layers and bottoming out in `Term`'s iterative walk
// (§13). Provenance is preserved: every wrapped child is rebuilt through the carrier's
// `map` (§6.2), which carries it.

impl Atom {
    /// Canonicalize the argument terms (§5.1), and collapse a one-alternative pool to
    /// `Single` — the atom-level image of the one-alternative `Term::Pool` drop.
    pub(crate) fn canonicalize(self) -> Atom {
        let canonicalize_tuple = |terms: Vec<Term>| {
            terms
                .into_iter()
                .map(Term::canonicalize)
                .collect::<Vec<_>>()
        };
        let arguments = match self.arguments {
            Arguments::Single(terms) => Arguments::Single(canonicalize_tuple(terms)),
            Arguments::Pooled(alternatives) => {
                let mut alternatives: Vec<Vec<Term>> =
                    alternatives.into_iter().map(canonicalize_tuple).collect();
                if alternatives.len() == 1 {
                    Arguments::Single(alternatives.pop().expect("one alternative"))
                } else {
                    Arguments::Pooled(alternatives)
                }
            }
        };
        Atom {
            sign: self.sign,
            name: self.name,
            arguments,
        }
    }
}

impl Literal {
    pub(crate) fn canonicalize(self) -> Literal {
        let inner = match self.inner {
            LiteralInner::Atom(atom) => LiteralInner::Atom(atom.map(Atom::canonicalize)),
            // A comparison is canonical by construction (its terms collapse at the
            // `Comparison` door); the boolean constants carry no term.
            LiteralInner::Comparison(comparison) => LiteralInner::Comparison(comparison),
            LiteralInner::True => LiteralInner::True,
            LiteralInner::False => LiteralInner::False,
        };
        Literal {
            negation: self.negation,
            inner,
        }
    }
}

impl Condition {
    pub(crate) fn canonicalize(self) -> Condition {
        Condition {
            literals: self
                .literals
                .into_iter()
                .map(|l| l.map(Literal::canonicalize))
                .collect(),
        }
    }
}

impl ConditionalLiteral {
    pub(crate) fn canonicalize(self) -> ConditionalLiteral {
        ConditionalLiteral {
            literal: self.literal.canonicalize(),
            condition: self.condition.canonicalize(),
        }
    }
}

impl DisjunctionElement {
    pub(crate) fn canonicalize(self) -> DisjunctionElement {
        DisjunctionElement {
            literal: self.literal.canonicalize(),
            condition: self.condition.canonicalize(),
        }
    }
}

impl ChoiceElement {
    pub(crate) fn canonicalize(self) -> ChoiceElement {
        ChoiceElement {
            literal: self.literal.canonicalize(),
            condition: self.condition.canonicalize(),
        }
    }
}

impl Disjunction {
    pub(crate) fn canonicalize(self) -> Disjunction {
        Disjunction {
            elements: super::merge_collect(
                self.elements
                    .into_iter()
                    .map(|element| element.map(DisjunctionElement::canonicalize)),
            ),
        }
    }
}

impl Choice {
    pub(crate) fn canonicalize(self) -> Choice {
        Choice {
            left_guard: self.left_guard.map(|guard| guard.map(Guard::canonicalize)),
            elements: super::merge_collect(
                self.elements
                    .into_iter()
                    .map(|element| element.map(ChoiceElement::canonicalize)),
            ),
            right_guard: self.right_guard.map(|guard| guard.map(Guard::canonicalize)),
        }
    }
}

impl Head {
    /// Fold an un-negated boolean head-literal to `Verum`/`Falsum` (§4.4), and canonicalize
    /// the terms of every other head shape (§5.1). A *negated* boolean head is kept as its
    /// literal, having no `Verum`/`Falsum` counterpart.
    pub(crate) fn canonicalize(self) -> Head {
        match self {
            Head::Verum
            | Head::Literal(Literal {
                negation: DefaultNegation::None,
                inner: LiteralInner::True,
            }) => Head::Verum,
            Head::Falsum
            | Head::Literal(Literal {
                negation: DefaultNegation::None,
                inner: LiteralInner::False,
            }) => Head::Falsum,
            Head::Literal(literal) => Head::Literal(literal.canonicalize()),
            Head::Disjunction(disjunction) => Head::Disjunction(disjunction.canonicalize()),
            Head::Choice(choice) => Head::Choice(choice.canonicalize()),
            Head::Aggregate(aggregate) => Head::Aggregate(aggregate.canonicalize()),
            Head::TheoryAtom(atom) => Head::TheoryAtom(atom.canonicalize()),
        }
    }
}

impl Body {
    pub(crate) fn canonicalize(self) -> Body {
        Body {
            elements: super::merge_collect(
                self.elements
                    .into_iter()
                    .map(|element| element.map(BodyElement::canonicalize)),
            ),
        }
    }
}

impl BodyElement {
    pub(crate) fn canonicalize(self) -> BodyElement {
        match self {
            BodyElement::Literal(literal) => BodyElement::Literal(literal.canonicalize()),
            BodyElement::Conditional(conditional) => {
                BodyElement::Conditional(conditional.canonicalize())
            }
            BodyElement::Aggregate {
                negation,
                aggregate,
            } => BodyElement::Aggregate {
                negation,
                aggregate: aggregate.canonicalize(),
            },
            BodyElement::TheoryAtom { negation, atom } => BodyElement::TheoryAtom {
                negation,
                atom: atom.canonicalize(),
            },
        }
    }
}

impl Rule {
    /// Canonicalize a rule (§5.1): the boolean-head fold and the term-level collapse across
    /// the head and the body, each part's provenance preserved by the carrier's `map`
    /// (§6.2). Run at the ingest door (§6.3).
    pub(crate) fn canonicalize(self) -> Rule {
        Rule {
            head: self.head.map(Head::canonicalize),
            body: self.body.map(Body::canonicalize),
        }
    }
}

impl WeakConstraint {
    pub(crate) fn canonicalize(self) -> WeakConstraint {
        WeakConstraint {
            body: self.body.map(Body::canonicalize),
            weight: self.weight.canonicalize(),
            terms: self.terms.into_iter().map(Term::canonicalize).collect(),
        }
    }
}
