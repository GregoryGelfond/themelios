//! The interrupt handle at the public surface (docs/design/solve.md §6.3). An application obtains
//! the agent's handle before asking, and pulls it from any thread. A pull cuts the question in
//! flight — its search stops and concludes `Interrupted`, the models already read kept — and a pull
//! with no question in flight cuts nothing. The core attributes the stop: a pulled question its
//! backend cut at the deadline reads `Interrupted`, an unpulled one `Budget`, and a search that
//! closed its space before the pull took effect `Exhausted`. Every witness is deterministic: the
//! test backend's runs read the pull between models, a pull from another thread is joined before
//! the next read, and a pull landing inside a backend call is made by the backend itself, at the
//! seam the witness names.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use themelios_program::program::Program;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_solve::agent::{Agent, Interrupt, Scenario};
use themelios_solve::bridge::Door;
use themelios_solve::contract::{
    Backend, Cancel, Capabilities, ConsequenceRequest, ConsequenceSupport, Fault, Mode,
    Presupposition, Refused, SolveRequest,
};
use themelios_solve::outcome::{
    Conclusion, Determination, Model, NativeAnswer, Run, ShowRule, Solved, Stopped, Truncation,
};

/// The backend's cancellation slot (§4.1): armed only while a run is open, so a pull outside a run
/// reaches nothing, and a pull inside it sets `pulled`, which the run reads.
#[derive(Default)]
struct Slot {
    armed: AtomicBool,
    pulled: AtomicBool,
}

impl Slot {
    /// Open a run's window: nothing pulled yet.
    fn arm(&self) {
        self.pulled.store(false, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Close the window: a pull after this reaches nothing, and none carries into the next run.
    fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
        self.pulled.store(false, Ordering::SeqCst);
    }
}

/// The backend's primitive over its slot.
struct Primitive(Arc<Slot>);

impl Cancel for Primitive {
    fn cancel(&self) {
        if self.0.armed.load(Ordering::SeqCst) {
            self.0.pulled.store(true, Ordering::SeqCst);
        }
    }
}

/// How a test run's space ends when nothing pulls it.
#[derive(Clone, Copy, Default)]
enum Space {
    /// A finite space of this many models, its search closing after them.
    Closes(usize),
    /// The deadline passes after this many models.
    Deadline(usize),
    /// A space no search finishes: a model on every read, until a pull.
    #[default]
    Unbounded,
}

/// The ground constant `a{index}`.
fn member(index: usize) -> Symbol {
    Symbol::function(
        Name::new(format!("a{index}")).expect("an identifier"),
        [],
        Sign::Positive,
    )
}

/// A run over the slot. Before each model it reads its space's end, then the pull: a closed space
/// concludes `Exhausted`, and a passed deadline `Budget`, even when a pull landed in the same window
/// — the core's attribution is what reads that `Budget` as `Interrupted` — while a pulled run
/// otherwise concludes `Interrupted`. It disarms the slot when it ends or drops.
struct Polled {
    slot: Arc<Slot>,
    space: Space,
    yielded: usize,
    conclusion: Option<Conclusion>,
}

impl Run for Polled {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if self.conclusion.is_some() {
            return None;
        }
        let ending = match self.space {
            Space::Closes(count) if self.yielded == count => Some(Conclusion::Exhausted),
            Space::Deadline(count) if self.yielded == count => Some(Conclusion::Budget),
            _ if self.slot.pulled.load(Ordering::SeqCst) => Some(Conclusion::Interrupted),
            _ => None,
        };
        if let Some(conclusion) = ending {
            self.conclusion = Some(conclusion);
            self.slot.disarm();
            return None;
        }
        self.yielded += 1;
        Some(Ok(Model::of([member(self.yielded)].into_iter().collect())))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.conclusion
    }
}

impl Drop for Polled {
    fn drop(&mut self) {
        self.slot.disarm();
    }
}

/// Where the backend itself pulls the agent's handle, standing in for another thread's pull
/// landing at that seam of a question.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Seam {
    #[default]
    Nowhere,
    /// While the agent lowers its knowledge, before the search begins.
    Lowering,
    /// While `solve` opens the run, before the backend has armed its slot.
    Opening,
    /// During the backend's own consequence search, whose deadline then passes.
    Consequences,
}

/// A cancelling backend over a slot armed only while a run is open, its runs reading the pull
/// between models. It pulls the handle in `hook` at its `seam`, counts the searches asked of it, and
/// declares assumptions and a native consequence door where `assumes` and `native` say.
#[derive(Default)]
struct Cancelling {
    slot: Arc<Slot>,
    space: Space,
    seam: Seam,
    hook: Arc<Mutex<Option<Interrupt>>>,
    searches: Arc<AtomicUsize>,
    assumes: bool,
    native: bool,
}

impl Cancelling {
    /// Pull the hooked handle, if one is hooked.
    fn pull_hook(&self) {
        if let Some(handle) = self.hook.lock().expect("an unpoisoned hook").as_ref() {
            handle.pull();
        }
    }
}

impl Backend for Cancelling {
    fn capabilities(&self) -> Capabilities {
        // Non-exhaustive, so declared by assignment.
        let mut capabilities = Capabilities::default();
        capabilities.cancellation = true;
        capabilities.assumptions = self.assumes;
        if self.native {
            capabilities.native_consequences = ConsequenceSupport::Native;
        }
        capabilities
    }

    fn interrupt(&self) -> Option<Box<dyn Cancel>> {
        Some(Box::new(Primitive(Arc::clone(&self.slot))))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        if self.seam == Seam::Lowering {
            self.pull_hook();
        }
        Ok(())
    }

    fn solve_assuming(
        &mut self,
        _scenario: &Scenario,
        request: &SolveRequest,
    ) -> Result<Solved<'_>, Fault> {
        self.solve(request)
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        self.searches.fetch_add(1, Ordering::SeqCst);
        if self.seam == Seam::Opening {
            // Before the slot is armed: the primitive drops this pull, so only the core's second
            // forward, once the run is open, can reach it.
            self.pull_hook();
        }
        self.slot.arm();
        Ok(Solved::running(
            Box::new(Polled {
                slot: Arc::clone(&self.slot),
                space: self.space,
                yielded: 0,
                conclusion: None,
            }),
            Scenario::default(),
            ShowRule::default(),
        ))
    }

    fn consequences_native(
        &mut self,
        _mode: Mode,
        _request: &ConsequenceRequest,
    ) -> Result<NativeAnswer, Fault> {
        self.slot.arm();
        self.pull_hook();
        self.slot.disarm();
        // The search's deadline passed in the same window as the pull.
        Ok(NativeAnswer::Stopped(Truncation::Budget))
    }
}

/// An agent over a cancelling backend whose runs end as `space` says.
fn agent_over(space: Space) -> Agent<Cancelling> {
    Agent::new(
        Program::empty(),
        Cancelling {
            space,
            ..Cancelling::default()
        },
    )
}

/// The models a question yields and the conclusion it ends at, read to its end.
fn run_out(agent: &mut Agent<Cancelling>) -> (usize, Option<Conclusion>) {
    let mut solved = agent.solve().expect("the question is answered");
    let read = solved.models().count();
    (read, solved.conclusion())
}

#[test]
fn the_interrupt_handle_is_send_and_sync() {
    fn shareable<T: Send + Sync>() {}
    shareable::<Interrupt>();
}

#[test]
fn a_handle_pulled_from_another_thread_interrupts_the_question() {
    let mut agent = agent_over(Space::Unbounded);
    let interrupt = agent.interrupt().expect("the backend cancels");
    let mut solved = agent.solve().expect("the question is answered");
    let first = solved.models().next();
    assert!(matches!(first, Some(Ok(_))), "an active, unfinished search");
    thread::scope(|scope| {
        scope.spawn(|| interrupt.pull());
    });
    assert_eq!(
        solved.models().count(),
        0,
        "the pull takes effect at the next read"
    );
    assert_eq!(solved.conclusion(), Some(Conclusion::Interrupted));
}

#[test]
fn an_interrupted_question_keeps_the_models_it_read() {
    let mut agent = agent_over(Space::Unbounded);
    let interrupt = agent.interrupt().expect("the backend cancels");
    let mut solved = agent.solve().expect("the question is answered");
    let read: Vec<Model> = solved
        .models()
        .take(3)
        .map(|item| item.expect("a model"))
        .collect();
    interrupt.pull();
    assert_eq!(solved.models().count(), 0);
    assert_eq!(read.len(), 3);
    assert!(matches!(
        solved.into_determination(),
        Determination::Consistent(_)
    ));
}

#[test]
fn a_pull_before_the_question_cuts_nothing() {
    let mut agent = agent_over(Space::Closes(2));
    agent.interrupt().expect("the backend cancels").pull();
    assert_eq!(run_out(&mut agent), (2, Some(Conclusion::Exhausted)));
}

#[test]
fn a_pull_after_the_run_ended_cuts_nothing_later() {
    let mut agent = agent_over(Space::Closes(2));
    let interrupt = agent.interrupt().expect("the backend cancels");
    assert_eq!(run_out(&mut agent), (2, Some(Conclusion::Exhausted)));
    interrupt.pull();
    assert_eq!(run_out(&mut agent), (2, Some(Conclusion::Exhausted)));
}

#[test]
fn a_pull_after_the_handle_dropped_cuts_nothing_later() {
    let mut agent = agent_over(Space::Closes(2));
    let interrupt = agent.interrupt().expect("the backend cancels");
    {
        let mut solved = agent.solve().expect("the question is answered");
        assert!(matches!(solved.models().next(), Some(Ok(_))));
    }
    interrupt.pull();
    assert_eq!(run_out(&mut agent), (2, Some(Conclusion::Exhausted)));
}

#[test]
fn a_pull_after_the_agent_dropped_cuts_nothing() {
    let agent = agent_over(Space::Unbounded);
    let interrupt = agent.interrupt().expect("the backend cancels");
    drop(agent);
    interrupt.pull();
}

#[test]
fn a_question_pulled_while_lowering_begins_no_search() {
    let backend = Cancelling {
        seam: Seam::Lowering,
        ..Cancelling::default()
    };
    let (hook, searches) = (Arc::clone(&backend.hook), Arc::clone(&backend.searches));
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    assert_eq!(run_out(&mut agent), (0, Some(Conclusion::Interrupted)));
    assert_eq!(searches.load(Ordering::SeqCst), 0, "no search was begun");
}

#[test]
fn a_pull_while_solve_opens_the_run_reaches_it_once_open() {
    let backend = Cancelling {
        seam: Seam::Opening,
        ..Cancelling::default()
    };
    let hook = Arc::clone(&backend.hook);
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    assert_eq!(run_out(&mut agent), (0, Some(Conclusion::Interrupted)));
}

#[test]
fn a_pulled_question_cut_at_its_deadline_reads_interrupted() {
    let mut agent = agent_over(Space::Deadline(2));
    let interrupt = agent.interrupt().expect("the backend cancels");
    let mut solved = agent.solve().expect("the question is answered");
    assert!(matches!(solved.models().next(), Some(Ok(_))));
    interrupt.pull();
    // The backend reports its deadline, observed in the same window as the pull.
    solved.models().for_each(drop);
    assert_eq!(solved.conclusion(), Some(Conclusion::Interrupted));
}

#[test]
fn an_unpulled_question_cut_at_its_deadline_reads_budget() {
    let mut agent = agent_over(Space::Deadline(2));
    assert_eq!(run_out(&mut agent), (2, Some(Conclusion::Budget)));
}

#[test]
fn a_search_that_closed_before_the_pull_reads_exhausted() {
    let mut agent = agent_over(Space::Closes(1));
    let interrupt = agent.interrupt().expect("the backend cancels");
    let mut solved = agent.solve().expect("the question is answered");
    assert!(matches!(solved.models().next(), Some(Ok(_))));
    interrupt.pull();
    assert_eq!(solved.models().count(), 0);
    assert_eq!(solved.conclusion(), Some(Conclusion::Exhausted));
}

#[test]
fn a_consequence_door_pulled_during_its_search_refuses_as_interrupted() {
    let backend = Cancelling {
        seam: Seam::Consequences,
        native: true,
        ..Cancelling::default()
    };
    let hook = Arc::clone(&backend.hook);
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    let refusal = agent.cautious().expect_err("a cut search decides nothing");
    assert!(matches!(
        refusal.refused(),
        Refused::Request(Presupposition::Unclosed(Truncation::Interrupted))
    ));
}

#[test]
fn a_consequence_door_pulled_while_lowering_refuses_as_interrupted() {
    let backend = Cancelling {
        seam: Seam::Lowering,
        ..Cancelling::default()
    };
    let hook = Arc::clone(&backend.hook);
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    let refusal = agent.brave().expect_err("a cut search decides nothing");
    assert!(matches!(
        refusal.refused(),
        Refused::Request(Presupposition::Unclosed(Truncation::Interrupted))
    ));
}

#[test]
fn a_determination_pulled_while_lowering_reads_inconclusive_at_the_interruption() {
    let backend = Cancelling {
        seam: Seam::Lowering,
        ..Cancelling::default()
    };
    let hook = Arc::clone(&backend.hook);
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    let Determination::Inconclusive(partial) = agent.determination().expect("a cut is no fault")
    else {
        panic!("a question cut before its search is inconclusive")
    };
    assert!(matches!(
        partial.stopped(),
        Stopped::Concluded(Truncation::Interrupted)
    ));
}

#[test]
fn a_native_consequence_door_pulled_while_lowering_refuses_as_interrupted() {
    let backend = Cancelling {
        seam: Seam::Lowering,
        native: true,
        ..Cancelling::default()
    };
    let hook = Arc::clone(&backend.hook);
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    let refusal = agent.cautious().expect_err("a cut search decides nothing");
    assert!(matches!(
        refusal.refused(),
        Refused::Request(Presupposition::Unclosed(Truncation::Interrupted))
    ));
}

#[test]
fn a_scoped_question_pulled_while_lowering_begins_no_search() {
    let backend = Cancelling {
        seam: Seam::Lowering,
        assumes: true,
        ..Cancelling::default()
    };
    let (hook, searches) = (Arc::clone(&backend.hook), Arc::clone(&backend.searches));
    let mut agent = Agent::new(Program::empty(), backend);
    *hook.lock().expect("an unpoisoned hook") = agent.interrupt();
    let mut solved = agent
        .solve_assuming(&Scenario::default())
        .expect("a cut is no fault");
    assert_eq!(solved.models().count(), 0);
    assert_eq!(solved.conclusion(), Some(Conclusion::Interrupted));
    assert_eq!(searches.load(Ordering::SeqCst), 0, "no search was begun");
}
