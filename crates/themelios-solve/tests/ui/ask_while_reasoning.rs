// Asking a question borrows the agent for the returned handle's life
// (docs/design/solve.md §6.1): the borrow checker is the "no mutation while
// reasoning" lock, so an amendment of the knowledge base — or a second
// question — while a `Solved` is held is refused by the compile error held
// beside this file, not by a runtime check.
use themelios_program::{Atom, Name, Program, Rule};
use themelios_solve::agent::Agent;
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::Solved;

/// A backend an agent holds but never drives.
struct Dormant;

impl Backend for Dormant {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Err(Fault::unsupported())
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }
}

fn amends_while_reasoning() {
    let mut agent = Agent::new(Program::empty(), Dormant);
    let solved = agent.solve();
    let _ = agent.assert(Rule::fact(Atom::constant(Name::new("a").unwrap())));
    drop(solved);
}

fn asks_while_reasoning() {
    let mut agent = Agent::new(Program::empty(), Dormant);
    let first = agent.solve();
    let second = agent.solve();
    drop(first);
    drop(second);
}

fn main() {
    amends_while_reasoning();
    asks_while_reasoning();
}
