//! The agent and the reasoning loop (docs/design/solve.md §6): a program
//! reified as a reasoner, driven by observe, modify, ask, act — with its
//! assumptions, per-operation options, and cancellation.

/// A named, owned set of assumptions to solve under (docs/design/solve.md
/// §6.3); its constructors and readers are defined with the agent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Scenario;

/// A handle that interrupts an in-flight solve from another thread. Reserved;
/// its surface is defined with §6.1 and §6.3.
pub struct Interrupt;
