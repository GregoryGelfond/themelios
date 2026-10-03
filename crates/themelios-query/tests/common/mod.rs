//! A minimal backend the query tier's reading tests drive an agent through
//! (docs/design/solve.md §5.2): it answers with a fixed enumeration ending in a
//! chosen conclusion, so a test builds a consistent world view — closed or
//! budget-cut — from owned answer sets, with no real solver.
#![expect(
    dead_code,
    reason = "this helper is shared across the reading-surface test files and each uses only \
              the part it needs; Cargo recompiles `common` per test crate, so an unused helper \
              there is expected, not a defect"
)]

use themelios_program::program::Program;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_query::WorldView;
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::{
    AnswerSet, Conclusion, Determination, Model, Run, ShowRule, Solved,
};

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

/// The answer sets of `models`, in order — what every reading reads of them.
pub fn atoms_of<'m>(models: impl IntoIterator<Item = &'m Model>) -> Vec<AnswerSet> {
    models
        .into_iter()
        .map(|model| model.atoms().clone())
        .collect()
}

/// A backend's own enumeration: a scripted model sequence, then a search that
/// ended with `terminal` (the terminal-conclusion obligation, reported once its
/// stream runs out).
struct Enumeration {
    sets: std::vec::IntoIter<AnswerSet>,
    terminal: Conclusion,
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
            ShowRule::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }
}

/// Whether `set` holds every assumption of `scenario` as fixed — the answer sets
/// `solve_assuming(scenario)` ranges over keep an atom fixed to hold and omit one
/// fixed not to.
fn admits(scenario: &Scenario, set: &AnswerSet) -> bool {
    scenario
        .assumptions()
        .all(|assumption| set.contains(assumption.atom()) == assumption.holds())
}

/// A backend that honours assumptions: `solve` answers with `sets`, and
/// `solve_assuming` with the members a scenario admits — its search ending
/// `terminal` either way — so a scoped reading differs from the unscoped one.
pub struct Hypothetical {
    sets: Vec<AnswerSet>,
    terminal: Conclusion,
}

impl Backend for Hypothetical {
    fn capabilities(&self) -> Capabilities {
        // Non-exhaustive, so declared by assignment: a struct expression is not
        // admitted outside the crate that defines it.
        let mut capabilities = Capabilities::default();
        capabilities.assumptions = true;
        capabilities
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Ok(Solved::running(
            Box::new(Enumeration {
                sets: self.sets.clone().into_iter(),
                terminal: self.terminal,
                ended: false,
            }),
            Scenario::default(),
            ShowRule::default(),
        ))
    }

    fn solve_assuming(
        &mut self,
        scenario: &Scenario,
        _request: &SolveRequest,
    ) -> Result<Solved<'_>, Fault> {
        let admitted: Vec<AnswerSet> = self
            .sets
            .iter()
            .filter(|set| admits(scenario, set))
            .cloned()
            .collect();
        Ok(Solved::running(
            Box::new(Enumeration {
                sets: admitted.into_iter(),
                terminal: self.terminal,
                ended: false,
            }),
            scenario.clone(),
            ShowRule::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }
}

/// Build an agent over a backend that honours assumptions over `sets`, its search
/// ending `terminal`, and hand it to `f` — the scenario-scoped sibling of
/// [`with_agent`].
pub fn with_hypothetical_agent<R>(
    sets: Vec<AnswerSet>,
    terminal: Conclusion,
    f: impl FnOnce(&mut Agent<Hypothetical>) -> R,
) -> R {
    let mut agent = Agent::new(Program::empty(), Hypothetical { sets, terminal });
    f(&mut agent)
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
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if !self.yielded_model {
            self.yielded_model = true;
            Some(Ok(Model::of(answer_set(["a"]))))
        } else if !self.faulted {
            self.faulted = true;
            Some(Err(Fault::engine("a stub engine failure")))
        } else {
            None
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        // A faulted search reached no conclusion; the fault the run reported is
        // the cause a completeness drain keeps.
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
            ShowRule::default(),
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

/// A backend's enumeration whose models carry terms only an engine evaluates —
/// `#show t : body.`'s — then a closed search.
struct Displayed {
    models: std::vec::IntoIter<(AnswerSet, Vec<Symbol>)>,
    ended: bool,
}

impl Run for Displayed {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if let Some((atoms, terms)) = self.models.next() {
            Some(Ok(Model::of(atoms).with_terms(terms)))
        } else {
            self.ended = true;
            None
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(Conclusion::Exhausted)
    }
}

/// A backend whose program displays something other than its answer sets: each
/// model with its terms, under the show rule of the program's directives.
pub struct Displaying {
    models: Vec<(AnswerSet, Vec<Symbol>)>,
    show: ShowRule,
}

impl Backend for Displaying {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Ok(Solved::running(
            Box::new(Displayed {
                models: self.models.clone().into_iter(),
                ended: false,
            }),
            Scenario::default(),
            self.show.clone(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }
}

/// Build an agent over a backend whose models display their terms under `show`,
/// its search closed, and hand it to `f`.
pub fn with_displaying_agent<R>(
    models: Vec<(AnswerSet, Vec<Symbol>)>,
    show: ShowRule,
    f: impl FnOnce(&mut Agent<Displaying>) -> R,
) -> R {
    let mut agent = Agent::new(Program::empty(), Displaying { models, show });
    f(&mut agent)
}
