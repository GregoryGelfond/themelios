//! Laws of the questions an agent answers (docs/design/solve.md §6.2, §6.4):
//! the ask vocabulary the bare `Program` shares — `solve`, `solve_with`,
//! `determination`, `optimize`, `solve_assuming` — each bringing the engine
//! level with the owned knowledge base before it delegates, each borrowing the
//! agent for the returned handle's life (the borrow checker is the "no
//! mutation while reasoning" lock, §6.1, held by the compile-fail witness in
//! `tests/ui`); the per-question options and the cancellation handle (§6.3).
//! The recording backend never solves, so a law reads the record and the
//! propagated fault, never a model — the resolution of a real run through the
//! agent is exercised in the defining crate, where a run can be built.

use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use std::time::Duration;

use themelios_program::program::Part;
use themelios_program::{Atom, Name, Program, Sign, Statement, Symbol};
use themelios_solve::agent::{Agent, Assumption, Interrupt, Scenario, SolveOptions};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{
    Backend, Cancel, Capabilities, Fault, GroundOptions, OptimizeRequest, SolveRequest, TruthValue,
};
use themelios_solve::outcome::{Determination, Optimized, Solved};

// ---- a recording backend the laws inspect ----

/// One request the backend received, in the order received. A lowering
/// carries the program that came through Door B, so a law can read which
/// knowledge base the ask handed the engine.
#[derive(Clone, PartialEq, Debug)]
enum Call {
    Lower(Program),
    Solve {
        time: Option<Duration>,
    },
    SolveAssuming {
        scenario: Scenario,
        time: Option<Duration>,
    },
    Optimize {
        report_trajectory: bool,
    },
}

/// The fault the recorder answers every question with: it holds no engine.
const NO_ENGINE: &str = "the recorder holds no engine";
/// The fault the refusing recorder's lowering issues.
const REFUSED_LOWERING: &str = "the recorder refuses this program";

const CANCELS: bool = true;
const NO_CANCELLATION: bool = false;
const ACCEPTS_LOWERING: bool = false;
const REFUSES_LOWERING: bool = true;

/// The time budget a configured ask carries.
const BUDGET: Duration = Duration::from_secs(1);

/// One question put to a recording agent, read for the fault it propagates.
type Ask = fn(&mut Agent<Recorder>) -> Option<Fault>;

/// One gated question put to an agent over a backend that declares nothing,
/// read for the fault it propagates.
type Overreach = fn(&mut Agent<Overreaching>) -> Option<Fault>;

/// A backend that records what it is asked and never solves. It declares the
/// capabilities its overrides answer for; cancellation is configurable so a
/// law can read the handle both ways.
struct Recorder {
    cancellation: bool,
    refuses_lowering: bool,
    calls: Rc<RefCell<Vec<Call>>>,
}

impl Backend for Recorder {
    fn capabilities(&self) -> Capabilities {
        let mut capabilities = Capabilities::default();
        capabilities.assumptions = true;
        capabilities.optimization = true;
        capabilities.budgets.time = true;
        capabilities.cancellation = self.cancellation;
        capabilities
    }

    fn solve(&mut self, request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        self.calls
            .borrow_mut()
            .push(Call::Solve { time: request.time });
        Err(Fault::engine(NO_ENGINE))
    }

    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
        // The agent lowers its owned knowledge base, so only Door B arrives.
        let Door::Program(program) = door else {
            return Err(Fault::adapter_bug(
                "the agent lowers its knowledge base through Door B",
            ));
        };
        self.calls.borrow_mut().push(Call::Lower(program.clone()));
        if self.refuses_lowering {
            Err(Fault::engine(REFUSED_LOWERING))
        } else {
            Ok(())
        }
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }

    fn interrupt(&self) -> Option<Box<dyn Cancel>> {
        self.cancellation
            .then(|| Box::new(Unheeded) as Box<dyn Cancel>)
    }

    fn optimize(&mut self, request: &OptimizeRequest) -> Result<Optimized<'_>, Fault> {
        self.calls.borrow_mut().push(Call::Optimize {
            report_trajectory: request.report_trajectory,
        });
        Err(Fault::engine(NO_ENGINE))
    }

    fn solve_assuming(
        &mut self,
        scenario: &Scenario,
        request: &SolveRequest,
    ) -> Result<Solved<'_>, Fault> {
        self.calls.borrow_mut().push(Call::SolveAssuming {
            scenario: scenario.clone(),
            time: request.time,
        });
        Err(Fault::engine(NO_ENGINE))
    }
}

/// A backend that implements the required surface alone, so every gated
/// question reaches the provided default that refuses.
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

/// A backend that declares nothing yet answers every gated method, recording
/// each call — so a law can tell the agent's refusal on the declaration from the
/// backend's own.
struct Overreaching {
    calls: Rc<RefCell<Vec<&'static str>>>,
}

impl Backend for Overreaching {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        self.calls.borrow_mut().push("solve");
        Err(Fault::engine(NO_ENGINE))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        self.calls.borrow_mut().push("lower");
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }

    fn optimize(&mut self, _request: &OptimizeRequest) -> Result<Optimized<'_>, Fault> {
        self.calls.borrow_mut().push("optimize");
        Err(Fault::engine(NO_ENGINE))
    }

    fn ground(&mut self, _parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        self.calls.borrow_mut().push("ground");
        Ok(())
    }

    fn assign_external(&mut self, _external: Symbol, _value: TruthValue) -> Result<(), Fault> {
        self.calls.borrow_mut().push("assign_external");
        Ok(())
    }
}

/// A cancellation primitive that cuts nothing short — the recorder's engine
/// runs no search to cut.
struct Unheeded;

impl Cancel for Unheeded {
    fn cancel(&self) {}
}

/// An agent over `program` and a fresh recorder, with the record handed back so
/// a law can read what the backend was asked.
fn agent_over(
    program: Program,
    cancellation: bool,
    refuses_lowering: bool,
) -> (Agent<Recorder>, Rc<RefCell<Vec<Call>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let backend = Recorder {
        cancellation,
        refuses_lowering,
        calls: Rc::clone(&calls),
    };
    (Agent::new(program, backend), calls)
}

/// An agent over `program` and a recorder that accepts every lowering.
fn recording_agent(program: Program) -> (Agent<Recorder>, Rc<RefCell<Vec<Call>>>) {
    agent_over(program, NO_CANCELLATION, ACCEPTS_LOWERING)
}

fn identifier(name: &str) -> Name {
    Name::new(name).expect("a valid identifier")
}

/// A ground fact `name.`.
fn fact(name: &str) -> Statement {
    Statement::from(themelios_program::Rule::fact(Atom::constant(identifier(
        name,
    ))))
}

/// A scenario assuming the constant atom `name` holds.
fn scenario_assuming(name: &str) -> Scenario {
    let atom = Symbol::function(identifier(name), [], Sign::Positive);
    [Assumption::new(atom, true).expect("a constant is an atom")]
        .into_iter()
        .collect()
}

/// The request `optimize` is handed, asking for the trajectory.
fn trajectory_request() -> OptimizeRequest {
    let mut request = OptimizeRequest::default();
    request.report_trajectory = true;
    request
}

/// The options a configured ask carries: the budget.
fn budgeted() -> SolveOptions {
    let mut options = SolveOptions::default();
    options.time = Some(BUDGET);
    options
}

// ---- each question brings the engine level, then delegates ----

#[test]
fn solve_lowers_the_knowledge_base_before_it_delegates() {
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    let _ = agent.solve();
    assert!(
        matches!(
            calls.borrow().as_slice(),
            [Call::Lower(_), Call::Solve { .. }]
        ),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn solve_with_lowers_the_knowledge_base_before_it_delegates() {
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    let _ = agent.solve_with(SolveOptions::default());
    assert!(
        matches!(
            calls.borrow().as_slice(),
            [Call::Lower(_), Call::Solve { .. }]
        ),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn determination_lowers_the_knowledge_base_before_it_delegates() {
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    let _ = agent.determination();
    assert!(
        matches!(
            calls.borrow().as_slice(),
            [Call::Lower(_), Call::Solve { .. }]
        ),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn optimize_lowers_the_knowledge_base_before_it_delegates() {
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    let _ = agent.optimize(&OptimizeRequest::default());
    assert!(
        matches!(
            calls.borrow().as_slice(),
            [Call::Lower(_), Call::Optimize { .. }]
        ),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn solve_assuming_lowers_the_knowledge_base_before_it_delegates() {
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    let _ = agent.solve_assuming(&Scenario::default());
    assert!(
        matches!(
            calls.borrow().as_slice(),
            [Call::Lower(_), Call::SolveAssuming { .. }]
        ),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn an_ask_lowers_the_knowledge_base_as_amended() {
    // The engine is brought level at the ask, not at the assertion (§6.2): the
    // lowering carries the knowledge base as it stands when the question is put.
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    agent.assert(fact("b")).expect("assert succeeds");
    let _ = agent.solve();
    let lowered = calls.borrow().first().cloned();
    assert_eq!(
        lowered,
        Some(Call::Lower(Program::of([fact("a"), fact("b")])))
    );
}

#[test]
fn a_repeated_ask_lowers_again() {
    // Each question brings the engine level anew; nothing is assumed retained
    // between questions.
    let (mut agent, calls) = recording_agent(Program::of([fact("a")]));
    let _ = agent.solve();
    let _ = agent.solve();
    let lowerings = calls
        .borrow()
        .iter()
        .filter(|call| matches!(call, Call::Lower(_)))
        .count();
    assert_eq!(lowerings, 2);
}

// ---- what each question hands the backend ----

#[test]
fn solve_asks_the_pristine_question() {
    let (mut agent, calls) = recording_agent(Program::empty());
    let _ = agent.solve();
    assert_eq!(calls.borrow().last(), Some(&Call::Solve { time: None }));
}

#[test]
fn solve_with_carries_the_time_budget_on_the_request() {
    let (mut agent, calls) = recording_agent(Program::empty());
    let _ = agent.solve_with(budgeted());
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::Solve { time: Some(BUDGET) })
    );
}

#[test]
fn solve_assuming_hands_the_scenario_to_the_backend() {
    let (mut agent, calls) = recording_agent(Program::empty());
    let scenario = scenario_assuming("p");
    let _ = agent.solve_assuming(&scenario);
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::SolveAssuming {
            scenario,
            time: None
        })
    );
}

#[test]
fn optimize_hands_the_request_to_the_backend() {
    let (mut agent, calls) = recording_agent(Program::empty());
    let _ = agent.optimize(&trajectory_request());
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::Optimize {
            report_trajectory: true
        })
    );
}

// ---- faults propagate ----

#[test]
fn every_ask_propagates_the_backend_s_fault() {
    let asks: [(&str, Ask); 5] = [
        ("solve", |agent| agent.solve().err()),
        ("solve_with", |agent| {
            agent.solve_with(SolveOptions::default()).err()
        }),
        ("determination", |agent| agent.determination().err()),
        ("optimize", |agent| {
            agent.optimize(&OptimizeRequest::default()).err()
        }),
        ("solve_assuming", |agent| {
            agent.solve_assuming(&Scenario::default()).err()
        }),
    ];
    for (name, ask) in asks {
        let (mut agent, _calls) = recording_agent(Program::empty());
        assert_eq!(ask(&mut agent), Some(Fault::engine(NO_ENGINE)), "{name}");
    }
}

#[test]
fn a_refused_lowering_propagates_as_the_ask_s_fault() {
    let (mut agent, _calls) = agent_over(Program::empty(), NO_CANCELLATION, REFUSES_LOWERING);
    assert_eq!(agent.solve().err(), Some(Fault::engine(REFUSED_LOWERING)));
}

#[test]
fn a_refused_lowering_is_not_followed_by_a_delegation() {
    let (mut agent, calls) = agent_over(Program::empty(), NO_CANCELLATION, REFUSES_LOWERING);
    let _ = agent.solve();
    assert!(
        matches!(calls.borrow().as_slice(), [Call::Lower(_)]),
        "{:?}",
        calls.borrow()
    );
}

// ---- a gated question over a backend without the capability is refused ----

#[test]
fn a_question_beyond_the_declaration_refuses_before_anything_is_lowered() {
    // The backend would answer each of these; the agent reads the declaration,
    // not the method, and refuses before paying for a lowering.
    let questions: [(&str, Overreach); 4] = [
        ("optimize", |agent| {
            agent.optimize(&OptimizeRequest::default()).err()
        }),
        ("a budgeted solve", |agent| {
            agent.solve_with(budgeted()).err()
        }),
        ("ground", |agent| agent.ground(&[]).err()),
        ("assign_external", |agent| {
            let atom = Symbol::function(identifier("a"), [], Sign::Positive);
            agent.assign_external(atom, TruthValue::True).err()
        }),
    ];
    for (name, question) in questions {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let backend = Overreaching {
            calls: Rc::clone(&calls),
        };
        let mut agent = Agent::new(Program::empty(), backend);
        assert_eq!(question(&mut agent), Some(Fault::unsupported()), "{name}");
        assert!(calls.borrow().is_empty(), "{name}: {:?}", calls.borrow());
    }
}

#[test]
fn optimize_over_a_backend_without_the_capability_is_refused() {
    let mut agent = Agent::new(Program::empty(), Dormant);
    assert_eq!(
        agent.optimize(&OptimizeRequest::default()).err(),
        Some(Fault::unsupported())
    );
}

#[test]
fn solve_assuming_over_a_backend_without_the_capability_is_refused() {
    let mut agent = Agent::new(Program::empty(), Dormant);
    assert_eq!(
        agent.solve_assuming(&Scenario::default()).err(),
        Some(Fault::unsupported())
    );
}

// ---- the questions borrow the agent (§6.1) ----

#[test]
fn each_ask_borrows_the_agent_for_the_handle_s_life() {
    // Each returned handle carries the agent's borrow, so the borrow checker
    // is the "no mutation while reasoning" lock; the fn-pointer coercions are
    // the lock on the signatures.
    let _: for<'a> fn(&'a mut Agent<Recorder>) -> Result<Solved<'a>, Fault> = Agent::solve;
    let _: for<'a> fn(&'a mut Agent<Recorder>, SolveOptions) -> Result<Solved<'a>, Fault> =
        Agent::solve_with;
    let _: for<'a> fn(&'a mut Agent<Recorder>) -> Result<Determination<'a>, Fault> =
        Agent::determination;
    let _: for<'a> fn(&'a mut Agent<Recorder>, &OptimizeRequest) -> Result<Optimized<'a>, Fault> =
        Agent::optimize;
    let _: for<'a> fn(&'a mut Agent<Recorder>, &Scenario) -> Result<Solved<'a>, Fault> =
        Agent::solve_assuming;
}

#[test]
fn holding_a_solved_handle_locks_the_agent() {
    // An amendment, or a second question, while a `Solved` is held does not
    // compile: the `tests/ui` witness tries both and holds the compile error
    // that refuses each.
    trybuild::TestCases::new().compile_fail("tests/ui/ask_*.rs");
}

// ---- cancellation (§6.3) ----

#[test]
fn the_interrupt_handle_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Interrupt>();
}

#[test]
fn the_cancellation_primitive_crosses_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Box<dyn Cancel>>();
}

#[test]
fn an_agent_over_a_non_cancelling_backend_offers_no_interrupt() {
    let (agent, _calls) = agent_over(Program::empty(), NO_CANCELLATION, ACCEPTS_LOWERING);
    assert!(agent.interrupt().is_none());
}

#[test]
fn an_agent_over_a_cancelling_backend_offers_an_interrupt() {
    let (agent, _calls) = agent_over(Program::empty(), CANCELS, ACCEPTS_LOWERING);
    assert!(agent.interrupt().is_some());
}

// ---- the per-question options (§6.3) ----

#[test]
fn the_default_solve_options_carry_no_time_budget() {
    assert!(SolveOptions::default().time.is_none());
}

#[test]
fn solve_options_are_owned_plain_data() {
    fn plain<T: Send + Sync + Clone + Eq + Debug + Default + 'static>() {}
    plain::<SolveOptions>();
}

#[test]
fn cloned_solve_options_equal_their_original() {
    let options = budgeted();
    assert_eq!(options.clone(), options);
}
