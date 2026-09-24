//! The agent and the reasoning loop (docs/design/solve.md §6): a program
//! reified as a reasoner, driven by observe, modify, ask, act — with its
//! assumptions, per-operation options, and cancellation.

use crate::contract::Backend;
use themelios_program::Program;

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

/// A single assumption — an atom asserted true or false for a scenario-scoped
/// solve (docs/design/solve.md §6.3); its constructors are defined with the
/// agent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Assumption;

/// A named, owned set of assumptions to solve under (docs/design/solve.md
/// §6.3); its constructors and readers are defined with the agent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Scenario;

/// A handle that interrupts an in-flight solve from another thread. Reserved;
/// its surface is defined with §6.1 and §6.3.
pub struct Interrupt;
