//! The declarative construction surface (docs/design/program.md §7): typed Rust
//! values in, a program value out, the shape of the expression mirroring the shape of
//! the rule. This is one of the two doors onto a `Program` — the raise from concrete
//! syntax (§8) is the other — and both reach the value through the one ingest door
//! (§6.3), so a program a human declares and a program a machine assembles are
//! validated identically and compare with structural equality (the two audiences,
//! §7.3; the *first-solve* witness, §16).
//!
//! Strong and arithmetic negation are two operators the type keeps apart (§4.6):
//! `-atom` flips an atom's strong [`Sign`], `-term` is arithmetic `Negate`. Default
//! negation is a word — [`not`], [`not_not`] — role-typed so it yields a
//! [`BodyElement`] and nothing else: a negated value is never one a head accepts, so
//! `not` in head position does not compile (§4.5, §7.3). Arithmetic and intervals
//! compose as written over anything term-shaped, the right-hand side coercing so a
//! number needs no wrapping (§7.1). Canonicalization (§5.1) runs at every door here
//! that takes a raw term, so the ergonomic path never yields a non-canonical value
//! (§7.2); the one value that can carry a non-canonical term is a struct literal a
//! caller fills directly, which the ingest door collapses on entry (§5.1).

use crate::program::{
    Aggregate, Arguments, Atom, Body, BodyElement, DefaultNegation, Direction, Head, IntoBody,
    IntoHead, Literal, LiteralInner, Optimize, OptimizeElement, Rule, TheoryAtom,
};
use crate::provenance::WithProvenance;
use crate::symbol::{Name, Sign, Symbol};
use crate::term::{BinaryOp, Term, UnaryOp};

// ---- Strong and arithmetic negation: two operators, one spelling each (§4.6) ----

impl std::ops::Neg for Atom {
    type Output = Atom;

    /// Strong negation (§4.6): `-p(X)` flips the atom's strong sign, `Positive` and
    /// `Negative` exchanging, and is involutive — `-(-a) == a`. It touches only the
    /// sign, never the argument terms, so a canonical atom stays canonical. The type
    /// picks the meaning: this is not arithmetic negation (that is [`Neg`] on a
    /// [`Term`]).
    ///
    /// [`Neg`]: std::ops::Neg
    fn neg(self) -> Atom {
        Atom {
            sign: match self.sign {
                Sign::Positive => Sign::Negative,
                Sign::Negative => Sign::Positive,
            },
            name: self.name,
            arguments: self.arguments,
        }
    }
}

impl std::ops::Neg for Term {
    type Output = Term;

    /// Arithmetic negation (§4.6, grammar §5.1): `-(X + 1)` wraps the term in
    /// `UnaryOp::Negate` and canonicalizes (§5.1). A ground *operator* term does not
    /// fold (§3.5), so a double negation stays a `UnaryOperation`; the involution is
    /// strong negation's ([`Neg`](std::ops::Neg) on an [`Atom`]), not this one's.
    fn neg(self) -> Term {
        unary(UnaryOp::Negate, self)
    }
}

// ---- Default negation: a word, role-typed to body position (§4.5, §7.1) ----

/// The values default negation applies to (§4.5, §7.1): an atom, an aggregate, or a
/// theory atom. Sealed — these three are the only ones, and [`not`]/[`not_not`] the
/// only spellings — so `not` cannot be written over a type the grammar does not admit
/// it over. Every spelling yields a [`BodyElement`]: default negation is a property of
/// a *body* occurrence (§4.5), so a negated value is never one a head accepts.
pub trait Negatable: sealed::Sealed {
    /// The mechanism behind [`not`] and [`not_not`]; those are the spellings, not this.
    #[doc(hidden)]
    fn negated(self, negation: DefaultNegation) -> BodyElement;
}

mod sealed {
    /// Seals [`Negatable`](super::Negatable): only this crate's three negatable values
    /// implement it, so no outside type can be made `not`-able.
    pub trait Sealed {}
    impl Sealed for super::Atom {}
    impl Sealed for super::Aggregate {}
    impl Sealed for super::TheoryAtom {}
}

impl Negatable for Atom {
    fn negated(self, negation: DefaultNegation) -> BodyElement {
        BodyElement::Literal(Literal {
            negation,
            inner: LiteralInner::Atom(WithProvenance::constructed(self.canonicalize())),
        })
    }
}

impl Negatable for Aggregate {
    fn negated(self, negation: DefaultNegation) -> BodyElement {
        BodyElement::Aggregate {
            negation,
            aggregate: self.canonicalize(),
        }
    }
}

impl Negatable for TheoryAtom {
    fn negated(self, negation: DefaultNegation) -> BodyElement {
        BodyElement::TheoryAtom {
            negation,
            atom: self.canonicalize(),
        }
    }
}

/// Default negation `not` over a body value (§4.5, §7.1): `not p` from an atom, or a
/// negated aggregate or theory atom. The result is a [`BodyElement`], because default
/// negation is a property of a body occurrence — so `not` is exactly right in body
/// position:
///
/// ```
/// use themelios_program::construct::not;
/// use themelios_program::program::{Atom, Rule};
/// use themelios_program::symbol::Name;
///
/// let p = Atom::constant(Name::new("p").unwrap());
/// let _constraint = Rule::constraint(not(p)); // :- not p.
/// ```
///
/// and does not compile in head position — a [`BodyElement`] is not a value any head
/// accepts (§4.5, §7.3):
///
/// ```compile_fail
/// use themelios_program::construct::not;
/// use themelios_program::program::{Atom, Rule};
/// use themelios_program::symbol::Name;
///
/// let p = Atom::constant(Name::new("p").unwrap());
/// let _fact = Rule::fact(not(p)); // `not(p)` is a BodyElement, which is not IntoHead
/// ```
pub fn not(value: impl Negatable) -> BodyElement {
    value.negated(DefaultNegation::Not)
}

/// Double default negation `not not` (§4.5): its own operator, never the identity —
/// under the stable-model semantics `not not p` differs from both `p` and `not p`, so
/// a name that collapsed it would lie. Like [`not`], it yields a [`BodyElement`].
pub fn not_not(value: impl Negatable) -> BodyElement {
    value.negated(DefaultNegation::NotNot)
}

// ---- The rule reads as the rule (§4.3, §7.1); each constructor is total (§7.2) ----

impl Head {
    /// The rule with this head and the given body (§7.1): `head.when(body)` reads as
    /// the rule it denotes — the head holds when the body does. Total (§7.2): a
    /// [`Head`] and a [`Body`] are already well-formed. The program-level boolean-head
    /// fold (§5.1) runs when the rule enters a program (§6.3).
    pub fn when(self, body: impl IntoBody) -> Rule {
        Rule::new(self, body)
    }
}

impl Rule {
    /// A fact — a single-literal head over an empty body (§4.3, §7.1), `p(1).`. Total.
    pub fn fact(head: impl IntoHead) -> Rule {
        Rule::new(head, Body::empty())
    }

    /// A constraint — a falsum head over the given body (§4.3, §7.1), `:- body.`.
    /// Total: a constraint *is* a rule (`⊥ ← body`).
    pub fn constraint(body: impl IntoBody) -> Rule {
        Rule::new(Head::Falsum, body)
    }
}

// ---- Arithmetic and intervals compose as written (§7.1) ----

// Each arithmetic operator builds its `BinaryOperation` and canonicalizes at the door
// (§5.1); the right-hand side coerces (`impl Into<Term>`), so `X + 1` needs no
// wrapping (§7.1). `Rem` (`%`) is ASP's `\` (`Mod`) and `BitOr` (`|`) is ASP's `?`;
// exponentiation has no Rust operator and is `Term::pow`, and bitwise complement is
// `Term::complement` (Rust's `!` is reserved for a meaning §4.6 does not spell here).
macro_rules! binary_operator {
    ($trait:ident, $method:ident, $operator:ident) => {
        impl<R: Into<Term>> std::ops::$trait<R> for Term {
            type Output = Term;
            fn $method(self, rhs: R) -> Term {
                binary(BinaryOp::$operator, self, rhs.into())
            }
        }
    };
}

binary_operator!(Add, add, Add);
binary_operator!(Sub, sub, Sub);
binary_operator!(Mul, mul, Mul);
binary_operator!(Div, div, Div);
binary_operator!(Rem, rem, Mod);
binary_operator!(BitAnd, bitand, BitAnd);
binary_operator!(BitOr, bitor, BitOr);
binary_operator!(BitXor, bitxor, BitXor);

impl Term {
    /// The interval `a .. b` (grammar §5.1), a semantic term-former canonicalized at
    /// the door (§5.1); the upper bound coerces, so `a.to(10)` needs no wrapping.
    #[must_use]
    pub fn to(self, upper: impl Into<Term>) -> Term {
        Term::Interval {
            lower: Box::new(self),
            upper: Box::new(upper.into()),
        }
        .canonicalize()
    }

    /// Exponentiation `a ** b` (grammar §5.1): a method, since Rust has no `**`. The
    /// raise re-associates `**` to the right (§8); built here it is one `BinaryOp::Pow`.
    #[must_use]
    pub fn pow(self, exponent: impl Into<Term>) -> Term {
        binary(BinaryOp::Pow, self, exponent.into())
    }

    /// Bitwise complement `~a` (grammar §5.1): a method spelling `UnaryOp::BitwiseNot`,
    /// kept off Rust's `!` and named apart from strong `-` and default `not` — the
    /// three-negation discipline (§3.1, §4.6).
    #[must_use]
    pub fn complement(self) -> Term {
        unary(UnaryOp::BitwiseNot, self)
    }

    /// Absolute value `|a|` (grammar §5.1), canonicalized at the door (§5.1).
    #[must_use]
    pub fn abs(self) -> Term {
        Term::Absolute(Box::new(self)).canonicalize()
    }
}

/// Build a canonicalized binary-operation term (§5.1): the operand terms collapse
/// where the algebra folds them, the operator itself never does (§3.5).
fn binary(operator: BinaryOp, left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
    .canonicalize()
}

/// Build a canonicalized unary-operation term (§5.1), as [`binary`] for one operand.
fn unary(operator: UnaryOp, argument: Term) -> Term {
    Term::UnaryOperation {
        operator,
        argument: Box::new(argument),
    }
    .canonicalize()
}

// ---- Coercion widens the one obvious spelling; it never adds a second (§7.1) ----

impl From<i32> for Term {
    /// A number is a term (§3.3, §3.4): `i32`, the engine's own width, lifted to the
    /// ground `Symbolic(Number)` leaf. Wider or narrower integers reach a term through
    /// their `ToSymbol` (§3.4) and then `From<Symbol>`; this widens the one obvious
    /// spelling, it does not add a second.
    fn from(value: i32) -> Term {
        Term::Symbolic(Symbol::Number(value))
    }
}

impl From<Atom> for Literal {
    /// An atom is a positive body literal (§4.6): no default negation, its argument
    /// terms canonicalized at the door (§5.1). The one obvious coercion; [`not`]
    /// negates it.
    fn from(atom: Atom) -> Literal {
        Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom.canonicalize())),
        }
    }
}

impl From<Atom> for BodyElement {
    fn from(atom: Atom) -> BodyElement {
        BodyElement::Literal(Literal::from(atom))
    }
}

impl From<Literal> for BodyElement {
    fn from(literal: Literal) -> BodyElement {
        BodyElement::Literal(literal)
    }
}

impl IntoHead for Atom {
    /// An atom is a one-literal head (§7.1): a positive [`Literal`] in head position.
    fn into_head(self) -> Head {
        Head::Literal(Literal::from(self))
    }
}

impl IntoBody for BodyElement {
    fn into_body(self) -> Body {
        Body::new([self])
    }
}

impl IntoBody for Literal {
    fn into_body(self) -> Body {
        Body::new([BodyElement::from(self)])
    }
}

impl IntoBody for Atom {
    fn into_body(self) -> Body {
        Body::new([BodyElement::from(self)])
    }
}

impl<T: Into<BodyElement>> IntoBody for Vec<T> {
    fn into_body(self) -> Body {
        Body::new(self.into_iter().map(Into::into))
    }
}

impl<T: Into<BodyElement>, const N: usize> IntoBody for [T; N] {
    fn into_body(self) -> Body {
        Body::new(self.into_iter().map(Into::into))
    }
}

// ---- The named nullary and empty constructors (§7.1) ----

impl Atom {
    /// An atom `p(t, …)` with positive strong sign (§4.6): the name is an already
    /// validated identifier (§3.2), so this is total (§7.2), and its argument terms
    /// canonicalize at the door (§5.1). Strong negation is `-atom`
    /// ([`Neg`](std::ops::Neg)).
    pub fn new(name: Name, arguments: impl IntoIterator<Item = Term>) -> Atom {
        Atom {
            sign: Sign::Positive,
            name,
            arguments: Arguments::Single(arguments.into_iter().collect()),
        }
        .canonicalize()
    }

    /// An atom `p(t…; u…)` with an argument-list pool (§4.6): each alternative is one
    /// argument tuple, the alternatives possibly of different arity. The name is a
    /// validated identifier (§3.2), so this is total (§7.2); the arguments canonicalize
    /// and a one-alternative pool collapses to `Single` at the door (§5.1).
    pub fn pooled(name: Name, alternatives: impl IntoIterator<Item = Vec<Term>>) -> Atom {
        Atom {
            sign: Sign::Positive,
            name,
            arguments: Arguments::Pooled(alternatives.into_iter().collect()),
        }
        .canonicalize()
    }

    /// A constant `p` — the empty-argument atom (§7.1): its own named constructor, so
    /// there is no typed-empty sentinel and a simple thing stays simple.
    pub fn constant(name: Name) -> Atom {
        Atom {
            sign: Sign::Positive,
            name,
            arguments: Arguments::Single(Vec::new()),
        }
    }
}

// ---- Optimization: the direction reads as the directive (§4.7, §7.1) ----

/// `#minimize` over the given optimize elements (§4.7, §7.1): the direction-tagged
/// value built directly, each element carrying a `weight(w).at_priority(p)` (§4.7).
/// [`maximize`] is its twin; the two read as the directives they are.
pub fn minimize(elements: impl IntoIterator<Item = OptimizeElement>) -> Optimize {
    Optimize::new(Direction::Minimize, elements)
}

/// `#maximize` over the given optimize elements (§4.7, §7.1): the twin of [`minimize`].
pub fn maximize(elements: impl IntoIterator<Item = OptimizeElement>) -> Optimize {
    Optimize::new(Direction::Maximize, elements)
}
