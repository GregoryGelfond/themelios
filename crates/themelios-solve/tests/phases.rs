//! The phases of a question (docs/design/solve.md §6.3): `lower` validates and retains, and `solve`
//! grounds and searches under a deadline fixed when it is called. Witnessed over a single-shot
//! test backend, directly and through the agent: a refused replacement leaves the earlier program
//! answering; a grounding that fails fails its solve alone, and the next solve grounds the same
//! program again; a deadline that passes during grounding concludes the run `Budget` with no
//! model, a cut and never a fault; and the agent lowers before it hands the budget to `solve`, so
//! its lowering runs outside the deadline. The test backend realises the contract; a real engine's
//! polling in each of its grounding modes is its own tests' to establish.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_solve::agent::{Agent, Scenario, SolveOptions};
use themelios_solve::bridge::Door;
use themelios_solve::contract::{Backend, Capabilities, Fault, Locus, SolveRequest};
use themelios_solve::outcome::{
    AnswerSet, Conclusion, Determination, Model, Run, ShowRule, Solved, Stopped, Truncation,
};

/// The fact `name.`, as a statement.
fn fact(name: &str) -> Statement {
    Statement::from(Rule::fact(Atom::new(
        Name::new(name).expect("an identifier"),
        [],
    )))
}

/// The program of the facts `names`.
fn program_of(names: &[&str]) -> Program {
    Program::of(names.iter().map(|name| fact(name)))
}

/// The answer set of the facts `names`.
fn answer_set(names: &[&str]) -> AnswerSet {
    names
        .iter()
        .map(|name| Symbol::function(Name::new(*name).expect("an identifier"), [], Sign::Positive))
        .collect()
}

/// A run yielding `sets`, then ending at `conclusion`.
struct Ending {
    sets: std::vec::IntoIter<AnswerSet>,
    conclusion: Conclusion,
    ended: bool,
}

impl Run for Ending {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        let next = self.sets.next().map(|set| Ok(Model::of(set)));
        self.ended = next.is_none();
        next
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(self.conclusion)
    }
}

/// A single-shot backend answering a program of facts with its one answer set. Its `lower`
/// refuses a program holding the fact `forbidden.`, keeping the program lowered before it; its
/// `solve` grounds the program lowered, failing as many groundings as `failing_groundings` says,
/// and cutting a budgeted run in its grounding when `deadline_in_grounding` is set. It records the
/// order of the calls made of it where a witness shares `calls`.
#[derive(Default)]
struct Phased {
    lowered: Option<Program>,
    failing_groundings: usize,
    deadline_in_grounding: bool,
    calls: Rc<RefCell<Vec<&'static str>>>,
}

impl Backend for Phased {
    fn capabilities(&self) -> Capabilities {
        // Non-exhaustive, so declared by assignment: it enforces a time budget natively.
        let mut capabilities = Capabilities::default();
        capabilities.budgets.time = true;
        capabilities
    }

    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
        self.calls.borrow_mut().push("lower");
        let program = door.program();
        let forbidden = fact("forbidden");
        if let Some(statement) = program
            .statements()
            .find(|statement| *statement.get() == forbidden)
        {
            return Err(Fault::program(
                "this statement is outside the backend's language",
                statement,
            ));
        }
        self.lowered = Some(program.clone());
        Ok(())
    }

    fn solve(&mut self, request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        self.calls.borrow_mut().push(if request.time.is_some() {
            "solve with a budget"
        } else {
            "solve"
        });
        let Some(program) = &self.lowered else {
            return Err(Fault::engine("nothing is lowered"));
        };
        if self.failing_groundings > 0 {
            self.failing_groundings -= 1;
            return Err(Fault::engine("the grounding failed"));
        }
        let (sets, conclusion) = if request.time.is_some() && self.deadline_in_grounding {
            (Vec::new(), Conclusion::Budget)
        } else {
            let names: Vec<String> = ["a", "b"]
                .into_iter()
                .filter(|name| {
                    program
                        .statements()
                        .any(|statement| *statement.get() == fact(name))
                })
                .map(str::to_owned)
                .collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            (vec![answer_set(&names)], Conclusion::Exhausted)
        };
        Ok(Solved::running(
            Box::new(Ending {
                sets: sets.into_iter(),
                conclusion,
                ended: false,
            }),
            Scenario::default(),
            ShowRule::default(),
        ))
    }
}

/// The answer sets a solve of `backend`'s lowered program yields, its search closed.
fn answer_sets_of(backend: &mut impl Backend) -> Vec<AnswerSet> {
    let mut solved = backend
        .solve(&SolveRequest::default())
        .expect("the backend solves");
    solved
        .all_models()
        .expect("a closed space")
        .into_iter()
        .map(|model| model.atoms().clone())
        .collect()
}

/// The answer sets an agent's question yields, its search closed.
fn answers_of(agent: &mut Agent<Phased>) -> Vec<AnswerSet> {
    let mut solved = agent.solve().expect("the question is answered");
    solved
        .all_models()
        .expect("a closed space")
        .into_iter()
        .map(|model| model.atoms().clone())
        .collect()
}

/// The time budget the witnesses ask under.
fn budgeted() -> SolveOptions {
    let mut options = SolveOptions::default();
    options.time = Some(Duration::from_mins(1));
    options
}

/// The truncation an inconclusive determination stopped at.
fn truncation_of(determination: Determination<'_>) -> Truncation {
    let Determination::Inconclusive(partial) = determination else {
        panic!("an inconclusive determination")
    };
    let Stopped::Concluded(truncation) = partial.stopped() else {
        panic!("a cut, not a fault")
    };
    truncation
}

#[test]
fn a_refused_replacement_leaves_the_earlier_program_answering() {
    let mut backend = Phased::default();
    backend
        .lower(Door::Program(&program_of(&["a"])))
        .expect("lowers");
    let refused = backend.lower(Door::Program(&program_of(&["b", "forbidden"])));
    assert!(refused.is_err(), "the replacement is refused");
    assert_eq!(answer_sets_of(&mut backend), vec![answer_set(&["a"])]);
}

#[test]
fn a_failed_grounding_leaves_the_lowered_program_for_the_next_solve() {
    let mut backend = Phased {
        failing_groundings: 1,
        ..Phased::default()
    };
    backend
        .lower(Door::Program(&program_of(&["a"])))
        .expect("lowers");
    let failed = backend.solve(&SolveRequest::default()).map(drop);
    assert!(failed.is_err(), "the first grounding fails");
    assert_eq!(answer_sets_of(&mut backend), vec![answer_set(&["a"])]);
}

#[test]
fn a_deadline_passing_in_grounding_concludes_budget_with_no_model() {
    let mut backend = Phased {
        deadline_in_grounding: true,
        ..Phased::default()
    };
    backend
        .lower(Door::Program(&program_of(&["a"])))
        .expect("lowers");
    let mut request = SolveRequest::default();
    request.time = Some(Duration::from_mins(1));
    let solved = backend.solve(&request).expect("a cut is no fault");
    assert_eq!(
        truncation_of(solved.into_determination()),
        Truncation::Budget
    );
}

#[test]
fn an_agent_question_answers_once_a_refused_statement_is_retracted() {
    let mut agent = Agent::new(program_of(&["a"]), Phased::default());
    let forbidden = agent.assert(fact("forbidden")).expect("asserted");
    let refusal = agent
        .solve()
        .map(drop)
        .expect_err("the backend refuses the statement");
    assert_eq!(refusal.locus(), Locus::Program);
    agent.retract(forbidden).expect("retracted");
    assert_eq!(answers_of(&mut agent), vec![answer_set(&["a"])]);
}

#[test]
fn an_agent_question_after_a_failed_grounding_answers() {
    let backend = Phased {
        failing_groundings: 1,
        ..Phased::default()
    };
    let mut agent = Agent::new(program_of(&["a"]), backend);
    assert!(
        agent.solve().map(drop).is_err(),
        "the first grounding fails"
    );
    assert_eq!(answers_of(&mut agent), vec![answer_set(&["a"])]);
}

#[test]
fn an_agent_budgeted_question_cut_in_grounding_reads_budget() {
    let backend = Phased {
        deadline_in_grounding: true,
        ..Phased::default()
    };
    let mut agent = Agent::new(program_of(&["a"]), backend);
    let solved = agent.solve_with(budgeted()).expect("a cut is no fault");
    assert_eq!(
        truncation_of(solved.into_determination()),
        Truncation::Budget
    );
}

#[test]
fn an_agent_lowers_before_it_hands_the_budget_to_solve() {
    // The deadline is fixed when `solve` is called; the agent's lowering precedes it (§6.3).
    let calls = Rc::new(RefCell::new(Vec::new()));
    let backend = Phased {
        calls: Rc::clone(&calls),
        ..Phased::default()
    };
    let mut agent = Agent::new(program_of(&["a"]), backend);
    agent
        .solve_with(budgeted())
        .map(drop)
        .expect("the question is answered");
    assert_eq!(*calls.borrow(), ["lower", "solve with a budget"]);
}
