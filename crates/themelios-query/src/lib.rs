//! The themelios query tier — the epistemic reading over the program tier's
//! patterns and the solve tier's outcomes (design of record:
//! `docs/design/query.md`): the three-valued answer, the world view, cautious
//! and brave consequence, and bindings. Engine-free.
//!
//! The keystone is the epistemic question — *is this true, given the program* —
//! whose answer has three values, not two (§1, §2.2). This root holds the
//! vocabulary that question is asked and answered in: [`Answer`], the closed
//! trichotomy; [`Query`], the ground question, which refuses at construction
//! anything it could not answer, so a query that exists denotes; and
//! [`NotAQuery`], that refusal. The matching a query rests on is the program
//! tier's own (§3.1); this tier owns only the policy over a collection of answer
//! sets (§3.2).
#![forbid(unsafe_code)]

use themelios_program::program::{Arguments, Atom};
use themelios_program::symbol::Symbol;
use themelios_program::term::{EvalError, Term};
use themelios_program::unify::NotAPattern;

pub mod prelude;

// The reading-side re-export (docs/design/query.md §2.1): cautious and brave
// consequences are the solve tier's typed sets, each carrying the mode that
// produced it, and the query tier hands them back under the one name.
pub use themelios_solve::outcome::Consequences;

/// The epistemic answer to a ground query (docs/design/query.md §2.2): `Yes`
/// iff the query is true in every member of the world view, `No` iff it is
/// false in every member — its *contrary* present, never merely the query
/// absent — and `Unknown` otherwise, a genuine value never collapsed into `No`.
///
/// Closed — not `#[non_exhaustive]`: the trichotomy is the affordance, and a
/// fourth reading is what this type exists to forbid (§2.1). Owned plain data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer {
    /// True in every member of the world view.
    Yes,
    /// False in every member of the world view: the contrary is present in
    /// each, which is more than the query being absent (§2.2).
    No,
    /// Neither — true in some member and not in all, or settled by none.
    Unknown,
}

impl std::fmt::Display for Answer {
    /// The human reading of the value, one word each.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Answer::Yes => "yes",
            Answer::No => "no",
            Answer::Unknown => "unknown",
        })
    }
}

/// A ground query (docs/design/query.md §2.1, §2.2; Gelfond and Kahl, Def.
/// 2.2.2 as corrected by the authors' errata): a literal, or a conjunction or
/// disjunction of queries — a closed set of *denoting* shapes. Construction is
/// the one door that can refuse: [`of`](Query::of) turns a ground atom into a
/// literal and refuses an atom it could not answer for, so a `Query` that
/// exists denotes and the readings over it never fail on the query's validity.
/// A conjunction ([`all`](Query::all)) and a disjunction ([`any`](Query::any))
/// compose queries and are total.
///
/// Owned plain data; its nesting is the caller's own composition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Query {
    shape: Shape,
}

/// The closed set of a query's shapes (docs/design/query.md §2.1). A literal
/// holds the ground symbol the atom denotes — the value an answer set contains
/// — so reading a query against a member is membership, never re-evaluation.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Shape {
    /// A ground literal, held as the symbol it denotes.
    Literal(Symbol),
    /// A conjunction (∧) of queries, evaluated per member as the weakest part.
    Conjunction(Vec<Query>),
    /// A disjunction (∨) of queries, evaluated per member as the strongest part.
    Disjunction(Vec<Query>),
}

impl Query {
    /// The literal query of a ground atom (docs/design/query.md §2.1): the
    /// atom's arguments evaluate at the program tier's ground door
    /// (`Term::evaluate`, program.md §3.5) to the symbol the atom denotes.
    /// Refuses, with the reason: a variable-bearing argument is `NotGround` — a
    /// query is ground, and a variable-bearing atom is a *pattern*, the
    /// bindings' question — and an argument that does not denote (an interval
    /// or a pool, which name a *set*; an unevaluated `@`-call; an undefined or
    /// out-of-range operation) or an argument-list pool `p(a; b)` is
    /// `NotAPattern`, the program tier's own refusal carried (§3.1). O(nodes).
    pub fn of(atom: Atom) -> Result<Self, NotAQuery> {
        // An argument-list pool names a set of atoms, not one: the atom-level
        // twin of a pooled argument's refusal, and the program tier's own
        // (program.md §11.2).
        let Arguments::Single(terms) = atom.arguments else {
            return Err(NotAQuery::NotAPattern(NotAPattern::Pooled));
        };
        // Each argument denotes the symbol its ground evaluation yields, or
        // refuses with the ground door's own reason: a variable is the one
        // reason that is this tier's (the atom is a pattern, not a query);
        // every other — a set-former, an `@`-call, an undefined or
        // out-of-range operation — is the program tier's non-denoting term.
        let mut arguments = Vec::with_capacity(terms.len());
        for term in terms {
            match term.evaluate() {
                Ok(symbol) => arguments.push(symbol),
                Err(EvalError::NotGround { .. }) => {
                    return Err(NotAQuery::NotGround { term });
                }
                Err(EvalError::External { .. } | EvalError::Undefined | EvalError::Overflow) => {
                    return Err(NotAQuery::NotAPattern(NotAPattern::NonDenoting { term }));
                }
            }
        }
        Ok(Query {
            shape: Shape::Literal(Symbol::function(atom.name, arguments, atom.sign)),
        })
    }

    /// The conjunction (∧) of `parts` (docs/design/query.md §2.1, §2.2):
    /// evaluated within each member of a world view as the weakest of its
    /// parts over `false < unknown < true`. Total; the empty conjunction is the
    /// query true everywhere. O(parts).
    pub fn all(parts: impl IntoIterator<Item = Query>) -> Query {
        Query {
            shape: Shape::Conjunction(parts.into_iter().collect()),
        }
    }

    /// The disjunction (∨) of `parts` (docs/design/query.md §2.1, §2.2):
    /// evaluated within each member of a world view as the strongest of its
    /// parts over `false < unknown < true`. Total; the empty disjunction is the
    /// query false everywhere. O(parts).
    pub fn any(parts: impl IntoIterator<Item = Query>) -> Query {
        Query {
            shape: Shape::Disjunction(parts.into_iter().collect()),
        }
    }
}

/// Why an atom is not a query (docs/design/query.md §2.1, §3.1), carrying the
/// offending term where there is one. Non-exhaustive, so a later reason is a
/// new variant, not a migration.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NotAQuery {
    /// An argument bears a variable, so the atom is not ground: it is a
    /// *pattern*, whose question is its bindings (§2.5), not a query.
    NotGround {
        /// The offending argument term.
        term: Term,
    },
    /// The atom is not a pattern at all (§3.1): an argument does not denote a
    /// single ground term, or the atom is an argument-list pool — the program
    /// tier's refusal, carried as this refusal's source.
    NotAPattern(NotAPattern),
}

impl std::fmt::Display for NotAQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotAQuery::NotGround { term } => {
                write!(f, "not a query: the term {term:?} is not ground")
            }
            NotAQuery::NotAPattern(_) => f.write_str("not a query: the atom is not a pattern"),
        }
    }
}

impl std::error::Error for NotAQuery {
    /// The program tier's refusal beneath a `NotAPattern`; nothing beneath
    /// `NotGround`, which is this tier's own.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NotAQuery::NotGround { .. } => None,
            NotAQuery::NotAPattern(inner) => Some(inner),
        }
    }
}
