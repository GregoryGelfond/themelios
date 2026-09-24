//! Laws of the agent's ownership (docs/design/solve.md §6.1): an agent is
//! instantiated from a program that becomes its knowledge base; it owns that
//! knowledge and the backend that reasons for it, reads the knowledge back,
//! and — holding no borrow off a stack frame — is `'static` whenever its
//! backend is. Dropping the agent is revocation; there is no ambient engine.

use std::any::Any;

use themelios_program::{Atom, Name, Program, Rule};
use themelios_solve::agent::Agent;
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::Solved;

/// A backend an agent holds but never drives: it declares nothing, refuses
/// to solve, accepts a lowering, and holds no ground program.
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

fn test_backend() -> Dormant {
    Dormant
}

/// The fact `predicate.` — a bodiless rule over a constant atom.
fn fact(predicate: &str) -> Rule {
    Rule::fact(Atom::constant(Name::new(predicate).expect("an identifier")))
}

#[test]
fn an_agent_owns_its_knowledge_base() {
    // `new` takes the program by value and `knowledge` reads it back
    // unchanged: the program moved in stays owned by the agent (§6.1).
    let knowledge = Program::of([fact("a")]);
    let agent = Agent::new(knowledge.clone(), test_backend());
    assert_eq!(agent.knowledge(), &knowledge);
} // the agent drops here — revocation; no ambient engine remains

#[test]
fn an_agent_is_static_when_its_backend_is() {
    // `Any` is `'static`: an agent holding its program by borrow off this
    // frame could not be boxed as one (§6.1, the service posture).
    let agent = Agent::new(Program::of([fact("a")]), test_backend());
    let boxed: Box<dyn Any> = Box::new(agent);
    assert!(boxed.is::<Agent<Dormant>>());
}
