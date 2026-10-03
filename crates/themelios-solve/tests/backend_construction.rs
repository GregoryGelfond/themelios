//! A backend outside `themelios-solve` builds the `Solved` its `solve` returns
//! by wrapping its own enumeration in a `Run` and handing it to `Solved::running`
//! (docs/design/solve.md §5.2) — the construction door every backend, a native
//! engine or the clingo adapter, builds its answers through. This test lives in
//! `tests/`, a separate compilation unit, so it also proves the door and the run
//! protocol are genuinely public: reachable by a backend author who is not inside
//! this crate, which a `pub(crate)` protocol would not be.
use themelios_program::program::Program;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::Door;
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::{
    AnswerSet, Conclusion, Determination, Model, Run, ShowRule, Solved,
};

/// The ground constant `name`, a member of an answer set.
fn atom(name: &str) -> Symbol {
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        [],
        Sign::Positive,
    )
}

/// The answer set holding the single constant `name`.
fn answer_set(name: &str) -> AnswerSet {
    [atom(name)].into_iter().collect()
}

/// A backend's own enumeration state: a scripted model sequence, then a closed
/// space — the smallest honest [`Run`], reporting `Exhausted` once its stream
/// ends (the terminal-conclusion obligation). Each model is built through the
/// backend's door, `Model::of`.
struct Enumeration {
    sets: std::vec::IntoIter<AnswerSet>,
    ended: bool,
}

impl Run for Enumeration {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if let Some(set) = self.sets.next() {
            Some(Ok(Model::of(set)))
        } else {
            self.ended = true;
            None
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(Conclusion::Exhausted)
    }
}

/// The smallest out-of-crate backend: it answers every question with a fixed,
/// exhausted enumeration, standing in for a real engine.
struct Fixed {
    sets: Vec<AnswerSet>,
}

impl Backend for Fixed {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Ok(Solved::running(
            Box::new(Enumeration {
                sets: self.sets.clone().into_iter(),
                ended: false,
            }),
            Scenario::default(),
            ShowRule::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }
}

#[test]
fn a_backend_streams_the_enumeration_it_built_the_solved_over() {
    let mut agent = Agent::new(
        Program::empty(),
        Fixed {
            sets: vec![answer_set("a"), answer_set("b")],
        },
    );
    let mut solved = agent.solve().expect("the backend solves");
    let streamed: Vec<AnswerSet> = solved
        .models()
        .map(|item| item.expect("no engine fault").atoms().clone())
        .collect();
    assert_eq!(
        streamed,
        vec![answer_set("a"), answer_set("b")],
        "the door streams exactly the enumeration the backend wrapped",
    );
}

#[test]
fn a_closed_search_yields_its_complete_collection_through_the_door() {
    let mut agent = Agent::new(
        Program::empty(),
        Fixed {
            sets: vec![answer_set("a")],
        },
    );
    let mut solved = agent.solve().expect("the backend solves");
    assert_eq!(
        solved.all_models().expect("an untouched, closed search"),
        vec![Model::of(answer_set("a"))],
        "a fresh handle over a closed search is complete",
    );
}

#[test]
fn the_core_classifies_an_empty_exhausted_enumeration_as_inconsistent() {
    // The backend supplies zero answer sets over a closed space; the core, not the
    // backend, reads that as inconsistent. A backend hands over only its
    // enumeration and terminal conclusion — it cannot hand back a determination,
    // so it cannot pose an empty search as consistent.
    let mut agent = Agent::new(Program::empty(), Fixed { sets: vec![] });
    let solved = agent.solve().expect("the backend solves");
    assert!(
        matches!(solved.into_determination(), Determination::Inconsistent(_)),
        "an empty, closed enumeration resolves inconsistent",
    );
}
