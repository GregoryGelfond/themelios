//! The conformance suite over engine-free stub backends (docs/design/solve.md
//! §13.1): a backend answering the suite's corpus from a table of its known
//! answer sets — streamed through the real `Backend`/`Run`/`Solved::running`
//! door — meets every obligation, and a backend broken in one way fails the
//! check that names that way, so the suite has teeth. The authoritative runs are
//! a real engine's; these run the suite's own machinery through the contract.

use themelios_program::program::Part;
use themelios_program::raise::raise_str;
use themelios_program::{Dialect, Name, Program, Sign, Symbol};
use themelios_solve::agent::{Interrupt, Scenario};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::conformance::{self, Capability, Check, ConformanceReport, Verdict};
use themelios_solve::contract::{
    Backend, Capabilities, ConsequenceRequest, ConsequenceSupport, Fault, GroundOptions, Mode,
    SolveRequest, TruthValue,
};
use themelios_solve::extend::{Function, Propagator};
use themelios_solve::outcome::{AnswerSet, Conclusion, Consequences, Run, Solved};

// ---- The table a stub answers the corpus from ----

/// The ground atom `name` under `sign`, applied to `arguments`.
fn atom(name: &str, arguments: impl IntoIterator<Item = Symbol>, sign: Sign) -> Symbol {
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        arguments,
        sign,
    )
}

/// The ground constant `name`.
fn constant(name: &str) -> Symbol {
    atom(name, [], Sign::Positive)
}

/// The answer set of the given atoms.
fn set(atoms: impl IntoIterator<Item = Symbol>) -> AnswerSet {
    atoms.into_iter().collect()
}

/// The program `source` denotes, in the clingo dialect.
fn program(source: &str) -> Program {
    raise_str(source, Dialect::Clingo)
        .expect("a small program")
        .into_program()
}

/// The suite's corpus with each program's answer sets, known independently of
/// any engine — the table a stub reads its answers from.
fn table() -> Vec<(Program, Vec<AnswerSet>)> {
    let p = |n| atom("p", [Symbol::number(n)], Sign::Positive);
    let q = |n| atom("q", [Symbol::number(n)], Sign::Positive);
    vec![
        (program(""), vec![set([])]),
        (program("a."), vec![set([constant("a")])]),
        (
            program("a :- not b. b :- not a."),
            vec![set([constant("a")]), set([constant("b")])],
        ),
        (program("a :- not a."), vec![]),
        (program("a. :- a."), vec![]),
        (
            program("-a. b :- -a."),
            vec![set([atom("a", [], Sign::Negative), constant("b")])],
        ),
        (
            program("a ; b."),
            vec![set([constant("a")]), set([constant("b")])],
        ),
        (program("{ a }."), vec![set([]), set([constant("a")])]),
        (
            program("p(1..2). q(X) :- p(X)."),
            vec![set([p(1), p(2), q(1), q(2)])],
        ),
    ]
}

// ---- The stub ----

/// A scripted enumeration: the answer sets, then the search's end, concluded as
/// `terminal`.
struct Enumeration {
    sets: std::vec::IntoIter<AnswerSet>,
    terminal: Conclusion,
    ended: bool,
}

impl Run for Enumeration {
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
        let next = self.sets.next().map(Ok);
        if next.is_none() {
            self.ended = true;
        }
        next
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(self.terminal)
    }
}

/// How a stub departs from the contract — one departure at a time, so a failed
/// check is the departure's.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flaw {
    /// None: the stub conforms.
    Faithful,
    /// Drops one answer set of every program that has several, and still
    /// concludes that the search closed the space.
    DropsAnAnswerSet,
    /// Without declaring enumeration, concludes that the search closed the space
    /// at its first witness.
    ClaimsExhaustionAtItsWitness,
    /// Answers `solve_assuming` without declaring `assumptions`.
    AnswersUndeclaredAssumptions,
    /// Declares `assumptions` yet refuses `solve_assuming`.
    RefusesDeclaredAssumptions,
    /// Declares `cancellation` yet answers no interrupt handle.
    WithholdsTheInterruptHandle,
    /// Declares the native consequence door yet answers the brave consequences
    /// for the cautious.
    MisanswersTheNativeCautious,
    /// Declares the native consequence door and `assumptions`, yet its door
    /// ranges over the whole program whatever scenario the request carries.
    IgnoresTheNativeScenario,
    /// Declares multi-shot solving yet accepts assigning an atom that is not
    /// external — the silent no-op.
    AcceptsANonExternal,
    /// Refuses to lower every program.
    RefusesEveryProgram,
    /// Refuses every unscoped solve.
    RefusesTheSolve,
    /// Yields the empty set for a program with no answer set, so the search
    /// reads consistent.
    ReadsInconsistencyAsConsistency,
    /// Stops at its budget over a program with no answer set, so the search
    /// reads undecided.
    LeavesTheSearchUndecided,
    /// Without declaring enumeration, yields as its witness a set that is no
    /// answer set.
    YieldsAStranger,
    /// Refuses `solve_assuming`, undeclared, at the engine locus rather than the
    /// request's.
    RefusesUndeclaredAssumptionsAtTheEngine,
    /// Declares the native door yet answers the consequences of a program with
    /// no answer set.
    AnswersTheConsequencesOfNoModel,
    /// Declares the native door and `assumptions`, yet refuses any request
    /// carrying a scenario.
    RefusesTheNativeScenario,
    /// Declares `assumptions`, yet ranges a scoped solve's models over the empty
    /// scenario rather than the one asked.
    MisplacesTheScenario,
    /// Declares `assumptions`, yet a scoped solve yields no model.
    LosesTheScenariosModels,
    /// Declares `assumptions`, yet a scoped solve yields every model, whatever
    /// the scenario.
    IgnoresTheScenario,
    /// Declares multi-shot solving, yet refuses `reset`.
    RefusesTheReset,
    /// Answers `reset` without declaring multi-shot solving.
    AnswersAnUndeclaredReset,
    /// Declares multi-shot solving, yet refuses an assignment as unsupported.
    RefusesAssignmentAsUnsupported,
    /// Declares multi-shot solving, yet refuses assigning an atom that is not
    /// external at the engine locus rather than the request's.
    RefusesAssignmentAtTheEngine,
}

/// A backend answering the corpus from its table, honouring exactly what it
/// declares unless its flaw says otherwise.
struct Stub {
    capabilities: Capabilities,
    flaw: Flaw,
    table: Vec<(Program, Vec<AnswerSet>)>,
    loaded: Option<usize>,
}

impl Stub {
    /// A stub declaring `capabilities`, departing from the contract by `flaw`.
    fn new(capabilities: Capabilities, flaw: Flaw) -> Stub {
        Stub {
            capabilities,
            flaw,
            table: table(),
            loaded: None,
        }
    }

    /// The answer sets of the program last lowered.
    fn answer_sets(&self) -> Result<Vec<AnswerSet>, Fault> {
        let index = self
            .loaded
            .ok_or_else(|| Fault::engine("no program is loaded"))?;
        let mut sets = self.table[index].1.clone();
        if self.flaw == Flaw::DropsAnAnswerSet && sets.len() > 1 {
            sets.pop();
        }
        Ok(sets)
    }

    /// The handle over `sets`, ranging over `scenario`: every set when the stub
    /// enumerates; otherwise the first, as a witness.
    fn enumerate(&self, mut sets: Vec<AnswerSet>, scenario: Scenario) -> Solved<'static> {
        if sets.is_empty() && self.flaw == Flaw::ReadsInconsistencyAsConsistency {
            sets.push(set([]));
        }
        if self.flaw == Flaw::YieldsAStranger {
            sets.insert(0, set([constant("stranger")]));
        }
        let terminal = if sets.is_empty() && self.flaw == Flaw::LeavesTheSearchUndecided {
            Conclusion::Budget
        } else if self.capabilities.enumeration || sets.is_empty() {
            Conclusion::Exhausted
        } else {
            sets.truncate(1);
            if self.flaw == Flaw::ClaimsExhaustionAtItsWitness {
                Conclusion::Exhausted
            } else {
                Conclusion::Target
            }
        };
        Solved::running(
            Box::new(Enumeration {
                sets: sets.into_iter(),
                terminal,
                ended: false,
            }),
            scenario,
        )
    }
}

/// Whether `set` holds every assumption of `scenario` as fixed.
fn admits(scenario: &Scenario, set: &AnswerSet) -> bool {
    scenario
        .assumptions()
        .all(|assumption| set.contains(assumption.atom()) == assumption.holds())
}

impl Backend for Stub {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    fn solve(&mut self, request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        if request.time.is_some() && !self.capabilities.budgets.time {
            return Err(Fault::unsupported());
        }
        if self.flaw == Flaw::RefusesTheSolve {
            return Err(Fault::engine("the stub refuses to solve"));
        }
        let sets = self.answer_sets()?;
        Ok(self.enumerate(sets, Scenario::default()))
    }

    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
        let Door::Program(lowered) = door else {
            return Err(Fault::unsupported());
        };
        if self.flaw == Flaw::RefusesEveryProgram {
            return Err(Fault::engine("the stub refuses every program"));
        }
        let index = self
            .table
            .iter()
            .position(|(program, _)| program == lowered)
            .ok_or_else(|| Fault::engine("the program is outside the stub's table"))?;
        self.loaded = Some(index);
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }

    fn interrupt(&self) -> Option<Interrupt> {
        (self.capabilities.cancellation && self.flaw != Flaw::WithholdsTheInterruptHandle)
            .then_some(Interrupt)
    }

    fn solve_assuming(
        &mut self,
        scenario: &Scenario,
        _request: &SolveRequest,
    ) -> Result<Solved<'_>, Fault> {
        let answers = match self.flaw {
            Flaw::AnswersUndeclaredAssumptions => true,
            Flaw::RefusesDeclaredAssumptions => false,
            _ => self.capabilities.assumptions,
        };
        if !answers {
            if self.flaw == Flaw::RefusesUndeclaredAssumptionsAtTheEngine {
                return Err(Fault::engine("the stub cannot assume"));
            }
            return Err(Fault::unsupported());
        }
        let all = self.answer_sets()?;
        let admitted: Vec<AnswerSet> = match self.flaw {
            Flaw::LosesTheScenariosModels => Vec::new(),
            Flaw::IgnoresTheScenario => all,
            _ => all
                .into_iter()
                .filter(|set| admits(scenario, set))
                .collect(),
        };
        let ranged = if self.flaw == Flaw::MisplacesTheScenario {
            Scenario::default()
        } else {
            scenario.clone()
        };
        Ok(self.enumerate(admitted, ranged))
    }

    fn ground(&mut self, _parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        if self.capabilities.multi_shot {
            Ok(())
        } else {
            Err(Fault::unsupported())
        }
    }

    fn assign_external(&mut self, _external: Symbol, _value: TruthValue) -> Result<(), Fault> {
        if !self.capabilities.multi_shot {
            return Err(Fault::unsupported());
        }
        match self.flaw {
            Flaw::AcceptsANonExternal => Ok(()),
            Flaw::RefusesAssignmentAsUnsupported => Err(Fault::unsupported()),
            Flaw::RefusesAssignmentAtTheEngine => Err(Fault::engine("the stub cannot assign")),
            // The corpus declares no external atom, so every assignment is refused.
            _ => Err(Fault::request("the atom is not external")),
        }
    }

    fn reset(&mut self) -> Result<(), Fault> {
        match (self.capabilities.multi_shot, self.flaw) {
            (true, Flaw::RefusesTheReset) => Err(Fault::engine("the stub cannot reset")),
            (false, Flaw::AnswersAnUndeclaredReset) | (true, _) => {
                self.loaded = None;
                Ok(())
            }
            (false, _) => Err(Fault::unsupported()),
        }
    }

    fn register_function(&mut self, _function: Box<dyn Function>) -> Result<(), Fault> {
        if self.capabilities.functions {
            Ok(())
        } else {
            Err(Fault::unsupported())
        }
    }

    fn register_propagator(&mut self, _propagator: Box<dyn Propagator>) -> Result<(), Fault> {
        if self.capabilities.propagators {
            Ok(())
        } else {
            Err(Fault::unsupported())
        }
    }

    fn consequences_native(
        &mut self,
        mode: Mode,
        request: &ConsequenceRequest,
    ) -> Result<Consequences, Fault> {
        if self.capabilities.native_consequences != ConsequenceSupport::Native {
            return Err(Fault::unsupported());
        }
        if self.flaw == Flaw::RefusesTheNativeScenario
            && request.scenario.assumptions().next().is_some()
        {
            return Err(Fault::request("the stub's door ranges over no scenario"));
        }
        let scoped = self.flaw != Flaw::IgnoresTheNativeScenario;
        let sets: Vec<AnswerSet> = self
            .answer_sets()?
            .into_iter()
            .filter(|set| !scoped || admits(&request.scenario, set))
            .collect();
        if sets.is_empty() && self.flaw != Flaw::AnswersTheConsequencesOfNoModel {
            return Err(Fault::request(
                "no consequences: the program has no answer set",
            ));
        }
        let answered = match (mode, self.flaw) {
            (Mode::Cautious, Flaw::MisanswersTheNativeCautious) => Mode::Brave,
            _ => mode,
        };
        Ok(Consequences::fold(answered, sets.iter()))
    }
}

// ---- Declarations ----

/// The declaration of a backend that enumerates and does nothing more.
fn enumerating() -> Capabilities {
    let mut capabilities = Capabilities::default();
    capabilities.enumeration = true;
    capabilities
}

/// The declaration of a backend that decides consistency without enumerating.
fn deciding() -> Capabilities {
    Capabilities::default()
}

/// The declaration of a backend realising every capability a stub outside the
/// crate can: all but optimization, whose outcome has no public constructor.
fn realising() -> Capabilities {
    let mut capabilities = enumerating();
    capabilities.native_consequences = ConsequenceSupport::Native;
    capabilities.assumptions = true;
    capabilities.multi_shot = true;
    capabilities.cancellation = true;
    capabilities.functions = true;
    capabilities.propagators = true;
    capabilities.budgets.time = true;
    capabilities
}

/// The suite's report on a stub declaring `capabilities` with `flaw`.
fn report(capabilities: Capabilities, flaw: Flaw) -> ConformanceReport {
    conformance::run(&mut Stub::new(capabilities, flaw))
}

/// Whether the report's verdict on `check` is a failure.
fn failed(report: &ConformanceReport, check: Check) -> bool {
    matches!(report.verdict(check), Some(Verdict::Failed(_)))
}

/// The reason the report gives for failing `check` — or the empty string, which
/// no failure's reason is, where it did not fail.
fn failure(report: &ConformanceReport, check: Check) -> String {
    match report.verdict(check) {
        Some(Verdict::Failed(why)) => why.clone(),
        _ => String::new(),
    }
}

/// Whether the report's verdict on `check` is a skip.
fn skipped(report: &ConformanceReport, check: Check) -> bool {
    matches!(report.verdict(check), Some(Verdict::Skipped(_)))
}

// ---- A conforming backend conforms ----

#[test]
fn an_enumerating_backend_conforms() {
    let report = report(enumerating(), Flaw::Faithful);
    assert!(report.is_conformant(), "{report}");
}

#[test]
fn a_backend_realising_every_capability_it_declares_conforms() {
    let report = report(realising(), Flaw::Faithful);
    assert!(report.is_conformant(), "{report}");
}

#[test]
fn every_declared_capability_is_probed_rather_than_skipped() {
    let report = report(realising(), Flaw::Faithful);
    for capability in [
        Capability::NativeConsequences,
        Capability::Assumptions,
        Capability::MultiShot,
        Capability::Functions,
        Capability::Propagators,
        Capability::Cancellation,
        Capability::TimeBudget,
    ] {
        assert_eq!(
            report.verdict(Check::Capability(capability)),
            Some(&Verdict::Passed),
            "{report}",
        );
    }
}

#[test]
fn a_backend_that_stops_at_its_witness_conforms() {
    // Without declaring enumeration, the stub yields one witness and concludes
    // that it met its target — never that the space closed.
    let report = report(deciding(), Flaw::Faithful);
    assert!(report.is_conformant(), "{report}");
}

#[test]
fn a_native_door_without_assumptions_conforms() {
    // The door is never handed a scenario, so it is held to the unscoped
    // consequences alone.
    let mut capabilities = enumerating();
    capabilities.native_consequences = ConsequenceSupport::Native;
    let report = report(capabilities, Flaw::Faithful);
    assert!(report.is_conformant(), "{report}");
}

#[test]
fn the_structural_pathologies_are_attempted() {
    let report = report(enumerating(), Flaw::Faithful);
    for check in [
        Check::InconsistencyIsExhausted,
        Check::TruncationCannotPoseAsComplete,
    ] {
        assert_eq!(report.verdict(check), Some(&Verdict::Passed), "{report}");
    }
}

#[test]
fn a_cancelled_search_is_skipped_while_no_search_can_be_cancelled() {
    let report = report(realising(), Flaw::Faithful);
    assert!(
        skipped(&report, Check::CancellationIsNotExhaustion),
        "{report}"
    );
}

#[test]
fn an_unexposed_ground_program_skips_its_faithfulness() {
    let report = report(enumerating(), Flaw::Faithful);
    assert!(skipped(&report, Check::GroundProgramIsFaithful), "{report}");
}

#[test]
fn the_non_external_refusal_binds_only_a_multi_shot_backend() {
    let report = report(enumerating(), Flaw::Faithful);
    assert!(
        skipped(&report, Check::NonExternalAssignmentRefuses),
        "{report}"
    );
}

// ---- A broken backend fails the check that names its break ----

#[test]
fn a_dropped_answer_set_fails_outcome_correctness() {
    let report = report(enumerating(), Flaw::DropsAnAnswerSet);
    assert!(failed(&report, Check::OutcomeCorrectness), "{report}");
}

#[test]
fn exhaustion_claimed_at_a_witness_fails_its_obligation() {
    let report = report(deciding(), Flaw::ClaimsExhaustionAtItsWitness);
    assert!(failed(&report, Check::ExhaustionIsEarned), "{report}");
}

#[test]
fn answering_an_undeclared_capability_fails_its_honesty() {
    let report = report(enumerating(), Flaw::AnswersUndeclaredAssumptions);
    assert!(
        failed(&report, Check::Capability(Capability::Assumptions)),
        "{report}"
    );
}

#[test]
fn refusing_a_declared_capability_fails_its_honesty() {
    let report = report(realising(), Flaw::RefusesDeclaredAssumptions);
    assert!(
        failed(&report, Check::Capability(Capability::Assumptions)),
        "{report}"
    );
}

#[test]
fn a_withheld_interrupt_handle_fails_the_cancellation_honesty() {
    let report = report(realising(), Flaw::WithholdsTheInterruptHandle);
    assert!(
        failed(&report, Check::Capability(Capability::Cancellation)),
        "{report}"
    );
}

#[test]
fn a_misanswered_native_consequence_fails_its_honesty() {
    let report = report(realising(), Flaw::MisanswersTheNativeCautious);
    assert!(
        failed(&report, Check::Capability(Capability::NativeConsequences)),
        "{report}"
    );
}

#[test]
fn a_native_door_ignoring_its_scenario_fails_its_honesty() {
    let report = report(realising(), Flaw::IgnoresTheNativeScenario);
    assert!(
        failed(&report, Check::Capability(Capability::NativeConsequences)),
        "{report}"
    );
}

#[test]
fn an_accepted_non_external_assignment_fails_its_refusal() {
    let report = report(realising(), Flaw::AcceptsANonExternal);
    assert!(
        failed(&report, Check::NonExternalAssignmentRefuses),
        "{report}"
    );
}

#[test]
fn a_flaw_fails_only_the_checks_that_name_it() {
    // Refusing a declared capability breaks that capability's honesty and
    // nothing else: the corpus and the other declarations still conform.
    let report = report(realising(), Flaw::RefusesDeclaredAssumptions);
    let broken: Vec<Check> = report
        .entries()
        .filter(|(_, verdict)| matches!(verdict, Verdict::Failed(_)))
        .map(|(check, _)| check)
        .collect();
    assert_eq!(
        broken,
        [Check::Capability(Capability::Assumptions)],
        "{report}"
    );
}

// ---- The pathologies the vocabulary makes unconstructible ----

#[test]
fn an_enumeration_has_no_improving_trajectory() {
    // The improving trajectory is `Optimized`'s alone (docs/design/solve.md
    // §5.3): asking a `Solved` for one does not compile.
    trybuild::TestCases::new().compile_fail("tests/ui/solved_trajectory.rs");
}

// ---- Every verdict path names what broke ----

#[test]
fn a_refused_program_fails_outcome_correctness() {
    let report = report(enumerating(), Flaw::RefusesEveryProgram);
    assert!(
        failure(&report, Check::OutcomeCorrectness).contains("was refused"),
        "{report}"
    );
}

#[test]
fn a_probe_whose_program_is_refused_fails_as_unprobed() {
    let report = report(realising(), Flaw::RefusesEveryProgram);
    let why = failure(&report, Check::Capability(Capability::Assumptions));
    assert!(why.contains("could not be probed"), "{report}");
}

#[test]
fn a_refused_solve_fails_outcome_correctness() {
    let report = report(enumerating(), Flaw::RefusesTheSolve);
    assert!(
        failure(&report, Check::OutcomeCorrectness).contains("solve was refused"),
        "{report}"
    );
}

#[test]
fn an_inconsistent_program_read_as_consistent_fails_outcome_correctness() {
    let report = report(enumerating(), Flaw::ReadsInconsistencyAsConsistency);
    assert!(
        failure(&report, Check::OutcomeCorrectness)
            .contains("read consistent where the program is inconsistent"),
        "{report}"
    );
}

#[test]
fn an_undecided_search_fails_outcome_correctness() {
    let report = report(enumerating(), Flaw::LeavesTheSearchUndecided);
    assert!(
        failure(&report, Check::OutcomeCorrectness).contains("budget"),
        "{report}"
    );
}

#[test]
fn a_witness_that_is_no_answer_set_fails_outcome_correctness() {
    let report = report(deciding(), Flaw::YieldsAStranger);
    assert!(
        failure(&report, Check::OutcomeCorrectness).contains("not one of its answer sets"),
        "{report}"
    );
}

#[test]
fn an_undeclared_capability_refused_off_the_request_locus_fails_its_honesty() {
    let report = report(enumerating(), Flaw::RefusesUndeclaredAssumptionsAtTheEngine);
    assert!(
        failure(&report, Check::Capability(Capability::Assumptions)).contains("engine locus"),
        "{report}"
    );
}

#[test]
fn native_consequences_of_a_program_without_models_fail_its_honesty() {
    let report = report(realising(), Flaw::AnswersTheConsequencesOfNoModel);
    assert!(
        failure(&report, Check::Capability(Capability::NativeConsequences))
            .contains("no answer set"),
        "{report}"
    );
}

#[test]
fn a_native_door_refusing_a_scenario_fails_its_honesty() {
    let report = report(realising(), Flaw::RefusesTheNativeScenario);
    assert!(
        failed(&report, Check::Capability(Capability::NativeConsequences)),
        "{report}"
    );
}

#[test]
fn a_scoped_solve_ranging_over_another_scenario_fails_its_honesty() {
    let report = report(realising(), Flaw::MisplacesTheScenario);
    assert!(
        failure(&report, Check::Capability(Capability::Assumptions)).contains("other than"),
        "{report}"
    );
}

#[test]
fn a_scoped_solve_that_loses_its_models_fails_its_honesty() {
    let report = report(realising(), Flaw::LosesTheScenariosModels);
    assert!(
        failure(&report, Check::Capability(Capability::Assumptions)).contains("as consistent"),
        "{report}"
    );
}

#[test]
fn a_scoped_solve_ignoring_its_scenario_fails_its_honesty() {
    let report = report(realising(), Flaw::IgnoresTheScenario);
    assert!(
        failure(&report, Check::Capability(Capability::Assumptions))
            .contains("not the ones the scenario admits"),
        "{report}"
    );
}

#[test]
fn a_refused_reset_fails_the_multi_shot_honesty() {
    let report = report(realising(), Flaw::RefusesTheReset);
    assert!(
        failed(&report, Check::Capability(Capability::MultiShot)),
        "{report}"
    );
}

#[test]
fn an_answered_undeclared_reset_fails_the_multi_shot_honesty() {
    let report = report(enumerating(), Flaw::AnswersAnUndeclaredReset);
    assert!(
        failed(&report, Check::Capability(Capability::MultiShot)),
        "{report}"
    );
}

#[test]
fn a_non_external_refused_as_unsupported_fails_its_refusal() {
    let report = report(realising(), Flaw::RefusesAssignmentAsUnsupported);
    assert!(
        failure(&report, Check::NonExternalAssignmentRefuses).contains("as unsupported"),
        "{report}"
    );
}

#[test]
fn a_non_external_refused_off_the_request_locus_fails_its_refusal() {
    let report = report(realising(), Flaw::RefusesAssignmentAtTheEngine);
    assert!(
        failure(&report, Check::NonExternalAssignmentRefuses).contains("engine locus"),
        "{report}"
    );
}
