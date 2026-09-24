//! The agent and the reasoning loop (docs/design/solve.md §6): a program
//! reified as a reasoner, driven by observe, modify, ask, act — with its
//! assumptions, per-operation options, and cancellation.

use std::fmt;

use crate::contract::Backend;
use themelios_program::{Program, Symbol};

/// A program made active — the same knowledge seen not as an object of study
/// but as a reasoner one drives (docs/design/solve.md §6.1). An agent is
/// instantiated from a `Program` that becomes its knowledge base, and it owns
/// both that knowledge and the backend that reasons for it: the owned value is
/// the authority to drive the engine, dropping it is revocation, and there is
/// no ambient engine or global mutable state. Because the knowledge is owned
/// rather than borrowed off a stack frame, an agent is `'static` whenever its
/// backend is, and so embeds behind a service boundary without ceremony.
///
/// Reifying a program with the installation's default engine — the `Reason`
/// extension trait on `Program` and its `into_agent` (§6.1) — belongs to the
/// facade crate, since it names the facade's `DefaultEngine`; this crate stays
/// engine-free, so an agent here is built over an explicit backend.
pub struct Agent<B: Backend> {
    /// The engine that reasons for the agent, driven by the reasoning loop
    /// (§6.2).
    #[expect(
        dead_code,
        reason = "driven by the reasoning loop (docs/design/solve.md §6.2)"
    )]
    backend: B,
    /// The knowledge base: the program the agent was instantiated from,
    /// evolving as the loop asserts and retracts (§6.2).
    knowledge: Program,
    /// The ledger kept beside the knowledge base (§6.2).
    #[expect(
        dead_code,
        reason = "read by the reasoning loop (docs/design/solve.md §6.2)"
    )]
    ledger: KnowledgeLedger,
}

impl<B: Backend> Agent<B> {
    /// Instantiate an agent from a program, which becomes its knowledge base,
    /// over the backend that reasons for it (docs/design/solve.md §6.1). The
    /// agent owns both. Total; O(1) — both values move in.
    pub fn new(knowledge: Program, backend: B) -> Self {
        Agent {
            backend,
            knowledge,
            ledger: KnowledgeLedger::default(),
        }
    }

    /// The evolving knowledge base — the program at rest behind the agent
    /// (docs/design/solve.md §6.1). Total; O(1).
    pub fn knowledge(&self) -> &Program {
        &self.knowledge
    }
}

/// The side structure the agent keeps beside its knowledge base: each asserted
/// statement's identity and its retraction class, fixed at assertion
/// (docs/design/solve.md §6.2). Seeded empty at construction; its entries are
/// defined with the modification surface.
#[derive(Default)]
pub(crate) struct KnowledgeLedger {}

// ---- Assumptions and scenarios (§6.3) ----

/// A single assumption (docs/design/solve.md §6.3): one program atom fixed
/// true or false for the span of one question — a hypothesis, discharged after
/// it, as distinct from a retraction (§6.2), which amends the knowledge base
/// and persists. The atom is a function symbol, a predicate or constant under
/// its strong sign; [`Assumption::new`] refuses every other symbol, so an
/// assumption over a number, a string, a tuple, `#inf`, or `#sup` — none of
/// which a program asserts — cannot be constructed. The raw set of assumptions
/// is what the literature names, what `solve_assuming` scopes by, and what
/// blame reports (§5.4). Owned plain data.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Assumption {
    atom: Symbol,
    holds: bool,
}

impl Assumption {
    /// Fix `atom` to hold (`true`) or not to hold (`false`) for one question.
    /// Refuses a symbol that is not an atom with [`NotAnAtom`], handing the
    /// symbol back rather than dropping it: an atom is a function symbol, the
    /// one variant a program asserts. O(1).
    pub fn new(atom: Symbol, holds: bool) -> Result<Assumption, NotAnAtom> {
        if matches!(atom, Symbol::Function { .. }) {
            Ok(Assumption { atom, holds })
        } else {
            Err(NotAnAtom { symbol: atom })
        }
    }

    /// The atom fixed. O(1).
    pub fn atom(&self) -> &Symbol {
        &self.atom
    }

    /// Whether the atom is fixed to hold (`true`) or not to hold (`false`).
    /// O(1).
    pub fn holds(&self) -> bool {
        self.holds
    }
}

/// A reusable, named assumption configuration (docs/design/solve.md §6.3) — a
/// concept this library introduces, so this library names it (§1.4): the
/// literature's "assumptions" names the raw set, not a bundle bound to an
/// identity and re-applied across solves. Collected from assumptions
/// (`FromIterator`) and read back exactly as given, in order; `solve_assuming`
/// takes one, and blame (§5.4) reports the responsible raw subset. The empty
/// scenario — no atom fixed — is the unscoped program, and is `Default`.
/// Owned plain data.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Scenario {
    assumptions: Vec<Assumption>,
}

impl Scenario {
    /// The assumptions, each as given, in the order collected. Borrowed:
    /// reading does not spend the scenario. O(n) over the whole stream.
    pub fn assumptions(&self) -> impl Iterator<Item = &Assumption> + '_ {
        self.assumptions.iter()
    }
}

impl FromIterator<Assumption> for Scenario {
    /// Bundle the assumptions, each kept as given. O(n).
    fn from_iter<I: IntoIterator<Item = Assumption>>(assumptions: I) -> Scenario {
        Scenario {
            assumptions: assumptions.into_iter().collect(),
        }
    }
}

/// Ergonomic construction of an assumption from a value authored the §3.1 way
/// (docs/design/solve.md §6.3) — the one door the scenario macro (§3.2)
/// expands through, so there is one grammar and one representation. Refuses a
/// value that is not an assumption with [`NotAnAssumption`]; an assumption
/// converts into itself.
pub trait IntoAssumption {
    /// The assumption this value authors, or the refusal.
    fn into_assumption(self) -> Result<Assumption, NotAnAssumption>;
}

impl IntoAssumption for Assumption {
    /// An assumption is already one. Total; O(1).
    fn into_assumption(self) -> Result<Assumption, NotAnAssumption> {
        Ok(self)
    }
}

/// The refusal [`Assumption::new`] issues (docs/design/solve.md §6.3): the
/// symbol is not an atom — a number, a string, a tuple, `#inf`, or `#sup`,
/// none of which a program asserts. Carries the refused symbol, handed back
/// rather than dropped.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAnAtom {
    /// The symbol that is not an atom.
    pub(crate) symbol: Symbol,
}

impl NotAnAtom {
    /// The refused symbol. O(1).
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }
}

impl fmt::Display for NotAnAtom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not an atom: the symbol {:?} is not a predicate or constant",
            self.symbol
        )
    }
}

impl std::error::Error for NotAnAtom {}

/// The refusal an [`IntoAssumption`] conversion issues (docs/design/solve.md
/// §6.3): the value authored is not an assumption. Non-exhaustive: what was
/// refused joins the refusal as the fallible conversions are realised; until
/// then it is declared, with its rendering, so the trait's signature is shaped
/// by it already.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotAnAssumption {}

impl fmt::Display for NotAnAssumption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not an assumption")
    }
}

impl std::error::Error for NotAnAssumption {}

/// A handle that interrupts an in-flight solve from another thread. Reserved;
/// its surface is defined with §6.1 and §6.3.
pub struct Interrupt;

#[cfg(test)]
mod tests {
    use super::*;

    // The conversion refusal is built here, in the defining crate: a
    // non-exhaustive struct is not built by a struct expression elsewhere.

    #[test]
    fn a_conversion_refusal_explains_itself() {
        let refused = NotAnAssumption {};
        assert!(
            format!("{refused}").contains("not an assumption"),
            "{refused}"
        );
    }

    #[test]
    fn a_cloned_conversion_refusal_equals_its_original() {
        let refused = NotAnAssumption {};
        assert_eq!(refused.clone(), refused);
    }
}
