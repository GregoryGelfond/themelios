//! A minimal backend the query tier's reading tests drive an agent through
//! (docs/design/solve.md §5.2): it answers with a fixed enumeration ending in a
//! chosen conclusion, so a test builds a consistent world view — closed or
//! budget-cut — from owned answer sets, with no real solver.
//!
//! `dead_code` is allowed because this helper is shared across the reading-surface
//! test files and each uses only the part it needs (Cargo recompiles `common` per
//! test crate, so an unused helper there is expected, not a defect).
#![allow(dead_code)]

use themelios_program::program::Program;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_query::WorldView;
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::{AnswerSet, Conclusion, Determination, Run, Solved};

/// The ground constant `name`.
pub fn atom(name: &str) -> Symbol {
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        [],
        Sign::Positive,
    )
}

/// The answer set holding exactly the named constants.
pub fn answer_set<'a>(names: impl IntoIterator<Item = &'a str>) -> AnswerSet {
    names.into_iter().map(atom).collect()
}

/// A backend's own enumeration: a scripted answer-set sequence, then a search that
/// ended with `terminal` (the terminal-conclusion obligation, reported once its
/// stream runs out).
struct Enumeration {
    sets: std::vec::IntoIter<AnswerSet>,
    terminal: Conclusion,
    ended: bool,
}

impl Run for Enumeration {
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
        if let Some(set) = self.sets.next() {
            Some(Ok(set))
        } else {
            self.ended = true;
            None
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(self.terminal)
    }
}

/// A backend answering every question with `sets`, its search ending `terminal`.
pub struct Fixed {
    sets: Vec<AnswerSet>,
    terminal: Conclusion,
}

impl Backend for Fixed {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Ok(Solved::running(
            Box::new(Enumeration {
                sets: self.sets.clone().into_iter(),
                terminal: self.terminal,
                ended: false,
            }),
            Scenario::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }
}

/// Build a consistent world view over `sets`, whose search ended with `terminal`,
/// and hand it to `f`. The agent lives for the call, so the world view — which
/// borrows it — is valid throughout `f`, without a self-referential return.
/// Panics if the fixture is not consistent (any non-empty `sets` is).
pub fn with_world_view<R>(
    sets: Vec<AnswerSet>,
    terminal: Conclusion,
    f: impl FnOnce(WorldView<'_>) -> R,
) -> R {
    let mut agent = Agent::new(Program::empty(), Fixed { sets, terminal });
    match agent.determination().expect("the backend solves") {
        Determination::Consistent(models) => f(WorldView::of(models)),
        _ => panic!("the fixture yields a consistent world view"),
    }
}

/// Build an agent over a backend answering with `sets`, its search ending
/// `terminal`, and hand it to `f`. The reading-facade tests drive the agent itself
/// — its `AgentReading` methods — so they receive the agent, not a world view;
/// unlike [`with_world_view`] this does not require consistency, so a fixture with
/// no answer set (or a truncated search) exercises the refusing readings.
pub fn with_agent<R>(
    sets: Vec<AnswerSet>,
    terminal: Conclusion,
    f: impl FnOnce(&mut Agent<Fixed>) -> R,
) -> R {
    let mut agent = Agent::new(Program::empty(), Fixed { sets, terminal });
    f(&mut agent)
}

/// The enumeration behind [`Faulting`]: one model, then an engine fault, then the
/// end — a search that is consistent but cannot be driven to completion.
struct FaultingRun {
    yielded_model: bool,
    faulted: bool,
}

impl Run for FaultingRun {
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
        if !self.yielded_model {
            self.yielded_model = true;
            Some(Ok(answer_set(["a"])))
        } else if !self.faulted {
            self.faulted = true;
            Some(Err(Fault::engine("a stub engine failure")))
        } else {
            None
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        // The search stopped at the fault without closing the space; the fault the
        // run reported is the cause a completeness drain keeps, not this.
        None
    }
}

/// A backend whose search witnesses one model and then faults when driven further.
struct Faulting;

impl Backend for Faulting {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Ok(Solved::running(
            Box::new(FaultingRun {
                yielded_model: false,
                faulted: false,
            }),
            Scenario::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }
}

/// Build a consistent world view whose search faults mid-stream, and hand it to
/// `f`: it witnesses one model — so the outcome is consistent — and then reports an
/// engine fault when driven further, so a completeness drain over it refuses.
pub fn with_faulting_world_view<R>(f: impl FnOnce(WorldView<'_>) -> R) -> R {
    let mut agent = Agent::new(Program::empty(), Faulting);
    match agent.determination().expect("the backend solves") {
        Determination::Consistent(models) => f(WorldView::of(models)),
        _ => panic!("the fixture yields a consistent world view"),
    }
}
