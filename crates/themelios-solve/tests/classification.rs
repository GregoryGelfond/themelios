//! How a run ends, classified (docs/design/solve.md §5.1): each way a run can end, before the first
//! model and after models, read through the public surface over a scripted backend — the
//! determination, the conclusion, and the refusal a complete collection meets. `Budget` is the
//! request's time budget alone and `Target` a target or a deciding backend's witness; a backend's
//! own configured ceiling is a Resource fault carrying the engine's typed cause, reached by
//! downcasting rather than by reading the message. A cut is never a fault, and a fault is never a
//! closed space; that a faulted run materialises no complete `Snapshot` is the query tier's
//! witness (themelios-query `tests/world_view.rs`).

use std::error::Error;
use std::fmt;

use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::Door;
use themelios_solve::contract::{
    Backend, Capabilities, Fault, Locus, Presupposition, Refused, SolveRequest,
};
use themelios_solve::outcome::{
    AnswerSet, Conclusion, Determination, Model, Run, ShowRule, Solved, Stopped, Truncation,
};

/// The ground constant `name`.
fn atom(name: &str) -> Symbol {
    Symbol::function(Name::new(name).expect("an identifier"), [], Sign::Positive)
}

/// The answer set holding the single constant `name`.
fn answer_set(name: &str) -> AnswerSet {
    [atom(name)].into_iter().collect()
}

/// The engine's own typed failure a Resource fault carries: a configured ceiling, reached.
#[derive(Debug, PartialEq, Eq)]
struct Ceiling {
    atoms: u64,
}

impl fmt::Display for Ceiling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the grounding ceiling of {} atoms was reached",
            self.atoms
        )
    }
}

impl Error for Ceiling {}

/// A Resource fault at a configured ceiling, carrying its typed cause (§5.1).
fn ceiling() -> Fault {
    Fault::resource("the backend's grounding ceiling was reached")
        .caused_by(Ceiling { atoms: 1000 })
}

/// How the scripted run ends once its models are read.
#[derive(Clone)]
enum End {
    /// A clean end at this conclusion.
    Concluded(Conclusion),
    /// A fault as the last item, then the end, with no conclusion.
    Faulted(Fault),
}

/// A run that yields its models, then ends as `end` says — honouring the run protocol: fused after
/// its end, its conclusion `None` while open and after a fault.
struct Script {
    sets: std::vec::IntoIter<AnswerSet>,
    end: End,
    ended: bool,
}

impl Run for Script {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if let Some(set) = self.sets.next() {
            return Some(Ok(Model::of(set)));
        }
        if self.ended {
            return None;
        }
        self.ended = true;
        match &self.end {
            End::Concluded(_) => None,
            End::Faulted(fault) => Some(Err(fault.clone())),
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        match &self.end {
            End::Concluded(conclusion) if self.ended => Some(*conclusion),
            _ => None,
        }
    }
}

/// A backend whose solve refuses with `refusal` where it has one, and otherwise runs its script.
struct Scripted {
    sets: Vec<AnswerSet>,
    end: End,
    refusal: Option<Fault>,
}

impl Backend for Scripted {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        if let Some(fault) = &self.refusal {
            return Err(fault.clone());
        }
        Ok(Solved::running(
            Box::new(Script {
                sets: self.sets.clone().into_iter(),
                end: self.end.clone(),
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

/// An agent over a backend that yields `sets` and ends as `end` says.
fn agent(sets: &[&str], end: End) -> Agent<Scripted> {
    let sets = sets.iter().map(|name| answer_set(name)).collect();
    Agent::new(
        Program::empty(),
        Scripted {
            sets,
            end,
            refusal: None,
        },
    )
}

/// The determination a reading names, by its variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reading {
    Consistent,
    Inconsistent,
    Inconclusive,
}

/// A table cell (§5.1): the determination one solve reads, then the conclusion its stream ends at,
/// the stream read to its end in between.
fn cell(agent: &mut Agent<Scripted>) -> (Reading, Option<Conclusion>) {
    let mut solved = agent.solve().expect("the backend solves");
    let reading = match solved.determination() {
        Determination::Consistent(_) => Reading::Consistent,
        Determination::Inconsistent(_) => Reading::Inconsistent,
        Determination::Inconclusive(_) => Reading::Inconclusive,
    };
    solved.models().for_each(drop);
    (reading, solved.conclusion())
}

#[test]
fn a_closed_space_with_no_model_reads_inconsistent_and_exhausted() {
    let mut agent = agent(&[], End::Concluded(Conclusion::Exhausted));
    assert_eq!(
        cell(&mut agent),
        (Reading::Inconsistent, Some(Conclusion::Exhausted))
    );
}

#[test]
fn a_closed_space_after_models_reads_consistent_and_exhausted() {
    let mut agent = agent(&["a", "b"], End::Concluded(Conclusion::Exhausted));
    assert_eq!(
        cell(&mut agent),
        (Reading::Consistent, Some(Conclusion::Exhausted))
    );
}

#[test]
fn a_budget_cut_before_any_model_reads_inconclusive_at_the_budget() {
    let mut agent = agent(&[], End::Concluded(Conclusion::Budget));
    assert_eq!(
        cell(&mut agent),
        (Reading::Inconclusive, Some(Conclusion::Budget))
    );
}

#[test]
fn a_budget_cut_after_models_reads_consistent_at_the_budget() {
    let mut agent = agent(&["a"], End::Concluded(Conclusion::Budget));
    assert_eq!(
        cell(&mut agent),
        (Reading::Consistent, Some(Conclusion::Budget))
    );
}

#[test]
fn a_budget_cut_after_models_refuses_a_complete_collection_as_unclosed() {
    let mut agent = agent(&["a"], End::Concluded(Conclusion::Budget));
    let mut solved = agent.solve().expect("the backend solves");
    let refusal = Fault::from(
        solved
            .all_models()
            .expect_err("a cut space is not complete"),
    );
    assert!(matches!(
        refusal.refused(),
        Refused::Request(Presupposition::Unclosed(Truncation::Budget))
    ));
}

#[test]
fn an_interruption_before_any_model_reads_inconclusive_at_the_interruption() {
    let mut agent = agent(&[], End::Concluded(Conclusion::Interrupted));
    assert_eq!(
        cell(&mut agent),
        (Reading::Inconclusive, Some(Conclusion::Interrupted))
    );
}

#[test]
fn an_interruption_after_models_reads_consistent_at_the_interruption() {
    let mut agent = agent(&["a"], End::Concluded(Conclusion::Interrupted));
    assert_eq!(
        cell(&mut agent),
        (Reading::Consistent, Some(Conclusion::Interrupted))
    );
}

#[test]
fn a_witness_stop_reads_consistent_at_the_target() {
    // A deciding backend's stop at its witness (§4.1), or a target the request set.
    let mut agent = agent(&["a"], End::Concluded(Conclusion::Target));
    assert_eq!(
        cell(&mut agent),
        (Reading::Consistent, Some(Conclusion::Target))
    );
}

#[test]
fn a_resource_fault_before_the_run_is_the_solves_refusal() {
    let mut agent = Agent::new(
        Program::empty(),
        Scripted {
            sets: Vec::new(),
            end: End::Concluded(Conclusion::Exhausted),
            refusal: Some(ceiling()),
        },
    );
    let fault = agent
        .solve()
        .map(|_| ())
        .expect_err("the ceiling refuses the solve");
    assert_eq!(fault.locus(), Locus::Resource);
}

#[test]
fn a_resource_faults_typed_cause_is_reached_by_downcasting() {
    let fault = ceiling();
    let cause = fault.source().expect("a cause is attached");
    assert_eq!(
        cause.downcast_ref::<Ceiling>(),
        Some(&Ceiling { atoms: 1000 })
    );
}

#[test]
fn a_resource_fault_as_the_first_item_reads_inconclusive_with_its_cause() {
    let mut agent = agent(&[], End::Faulted(ceiling()));
    let determination = agent.determination().expect("the run opens");
    let Determination::Inconclusive(partial) = determination else {
        panic!("a fault before any model is inconclusive")
    };
    let Stopped::Faulted(fault) = partial.stopped() else {
        panic!("a faulted search reached no conclusion")
    };
    assert_eq!(fault.locus(), Locus::Resource);
    let cause = fault.source().expect("a cause is attached");
    assert!(cause.downcast_ref::<Ceiling>().is_some());
}

#[test]
fn a_resource_fault_after_models_reads_consistent_with_no_conclusion() {
    let mut agent = agent(&["a"], End::Faulted(ceiling()));
    assert_eq!(cell(&mut agent), (Reading::Consistent, None));
}

#[test]
fn a_resource_fault_after_models_is_the_streams_last_item() {
    let mut agent = agent(&["a"], End::Faulted(ceiling()));
    let mut solved = agent.solve().expect("the run opens");
    let items: Vec<bool> = solved.models().map(|item| item.is_ok()).collect();
    assert_eq!(items, [true, false], "the model, then the fault");
}

#[test]
fn a_resource_fault_after_models_refuses_a_complete_collection_with_its_cause() {
    let mut agent = agent(&["a"], End::Faulted(ceiling()));
    let mut solved = agent.solve().expect("the run opens");
    let refusal = Fault::from(
        solved
            .all_models()
            .expect_err("a faulted space is not complete"),
    );
    assert_eq!(refusal.locus(), Locus::Resource);
    let cause = refusal.source().expect("the fault's cause is kept");
    assert!(cause.downcast_ref::<Ceiling>().is_some());
}

#[test]
fn a_program_fault_names_its_statement() {
    let statement = WithProvenance::constructed(Statement::from(Rule::fact(Atom::new(
        Name::new("p").expect("an identifier"),
        [],
    ))));
    let fault = Fault::program(
        "this statement is outside the backend's language",
        &statement,
    );
    assert_eq!(fault.locus(), Locus::Program);
    let Refused::Statement(refused) = fault.refused() else {
        panic!("a Program fault refuses its statement")
    };
    assert_eq!(refused, &statement);
}
