//! Rules and their parts (docs/design/program.md §4.3–§4.6): the literal core now —
//! atoms with their strong sign, the guarded comparison chain, literals under default
//! negation, conditions, and conditional literals — heads, bodies, and rules at the
//! keystone task. Each type is grammar-bounded and derives its identity over content
//! (§13), with provenance-bearing children wrapped in `WithProvenance` (§6.2).

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

/// An atom (grammar §5.2): a strong sign, a predicate name, and arguments. The sign
/// is **strong** negation (`-p`); the default negation is the literal's (§4.6). No
/// invariant beyond its fields' own, so a struct literal is its constructor (§4); its
/// argument terms canonicalize when the atom passes an ingest door (§5.1, §7.2, §9).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Atom {
    /// The strong sign.
    pub sign: Sign,
    /// The predicate name.
    pub name: Name,
    /// The argument terms.
    pub arguments: Vec<Term>,
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
}

/// A conditional literal (grammar §5.4): a literal under a condition (§4.6).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConditionalLiteral {
    /// The literal.
    pub literal: Literal,
    /// The condition it holds under.
    pub condition: Condition,
}
