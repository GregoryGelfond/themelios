//! The agent and the reasoning loop (docs/design/solve.md §6): a program
//! reified as a reasoner, driven by observe, modify, ask, act — with its
//! assumptions, per-operation options, and cancellation.

/// A named, owned set of assumptions to solve under (docs/design/solve.md
/// §6.3); its constructors and readers are defined with the agent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Scenario;
