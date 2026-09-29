//! The conformance suite over engine-free stub backends (docs/design/solve.md
//! §13.1): a backend answering the suite's corpus from a table of its known
//! answer sets — streamed through the real `Backend`/`Run`/`Solved::running`
//! door — meets every obligation, and a backend broken in one way fails exactly
//! the checks that name that way, each with its typed breach, so the suite has
//! teeth. The authoritative runs are a real engine's; these run the suite's own
//! machinery through the contract.

use std::collections::HashSet;

use themelios_base::diagnostic::Label;
use themelios_program::program::Part;
use themelios_program::raise::raise_str;
use themelios_program::{Dialect, Name, Origin, Program, Sign, Symbol};
use themelios_solve::agent::{Interrupt, Scenario};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::conformance::{self, Breach, Capability, Check, ConformanceReport, Verdict};
use themelios_solve::contract::{
    Backend, Capabilities, ConsequenceRequest, ConsequenceSupport, Fault, GroundOptions, Locus,
    Mode, SolveRequest, TruthValue,
};
use themelios_solve::extend::{Function, Propagator};
use themelios_solve::outcome::{AnswerSet, Conclusion, Consequences, Run, Solved};

// ---- The table a stub answers the suite from ----

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

/// The positive loop, whose completion admits an unsupported model.
const POSITIVE_LOOP: &str = "a :- b. b :- a.";

/// The head cycle, whose shifted disjunction admits no model.
const HEAD_CYCLE: &str = "a ; b. a :- b. b :- a.";

/// How the stub answers a program.
enum Answers {
    /// These answer sets, known independently of any engine.
    Known(Vec<AnswerSet>),
    /// `{a, b}` while the external `a` is assigned true, `{}` otherwise.
    External,
    /// None: no grounder can instantiate the program, so lowering it is refused.
    Unsafe,
}

/// Every program the suite loads, with its answers — the corpus and the
/// capability probes' programs, known independently of any engine.
fn table() -> Vec<(&'static str, Program, Answers)> {
    let p = |n| atom("p", [Symbol::number(n)], Sign::Positive);
    let q = |n| atom("q", [Symbol::number(n)], Sign::Positive);
    let known = |sets: Vec<AnswerSet>| Answers::Known(sets);
    vec![
        ("", known(vec![set([])])),
        ("a.", known(vec![set([constant("a")])])),
        (
            "a :- not b. b :- not a.",
            known(vec![set([constant("a")]), set([constant("b")])]),
        ),
        ("a :- not a.", known(vec![])),
        ("a. :- a.", known(vec![])),
        (
            "-a. b :- -a.",
            known(vec![set([atom("a", [], Sign::Negative), constant("b")])]),
        ),
        (
            "a ; b.",
            known(vec![set([constant("a")]), set([constant("b")])]),
        ),
        ("{ a }.", known(vec![set([]), set([constant("a")])])),
        (
            "p(1..2). q(X) :- p(X).",
            known(vec![set([p(1), p(2), q(1), q(2)])]),
        ),
        (POSITIVE_LOOP, known(vec![set([])])),
        (HEAD_CYCLE, known(vec![set([constant("a"), constant("b")])])),
        (
            "{ a }. #minimize { 1 : a }.",
            known(vec![set([]), set([constant("a")])]),
        ),
        ("#external a. b :- a.", Answers::External),
        ("p(X).", Answers::Unsafe),
    ]
    .into_iter()
    .map(|(source, answers)| (source, program(source), answers))
    .collect()
}

/// The refusal of a program no grounder can instantiate: a program fault at the
/// location of its statement.
fn located_refusal(program: &Program) -> Fault {
    let location = program
        .statements()
        .flat_map(|node| node.provenance().origins())
        .find_map(|origin| match origin {
            Origin::Parsed(location) => Some(*location),
            _ => None,
        })
        .expect("a raised statement is located");
    Fault::program(
        "a variable nothing binds",
        Label {
            location,
            message: None,
        },
    )
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

/// A search that yields the same model forever — the missing blocking clause.
struct Endless {
    model: AnswerSet,
}

impl Run for Endless {
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
        Some(Ok(self.model.clone()))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        None
    }
}

/// A search whose engine dies after its first model: the model, then an engine
/// fault, then the end, concluded as cut short.
struct Faulting {
    first: Option<AnswerSet>,
    faulted: bool,
}

impl Run for Faulting {
    fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
        if let Some(first) = self.first.take() {
            return Some(Ok(first));
        }
        if self.faulted {
            return None;
        }
        self.faulted = true;
        Some(Err(Fault::engine("the stub's engine died mid-search")))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.faulted.then_some(Conclusion::Interrupted)
    }
}

/// How a stub departs from the contract — one departure at a time, so each
/// failed check is the departure's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flaw {
    /// None: the stub conforms.
    Faithful,
    /// Drops one answer set of every program that has several, and still
    /// concludes that the search closed the space.
    DropsAnAnswerSet,
    /// Without declaring enumeration, concludes that the search closed the space
    /// at its first witness.
    ClaimsExhaustionAtItsWitness,
    /// Yields its model forever, never ending the search.
    NeverEnds,
    /// Yields its first model, then faults at the engine.
    FaultsMidStream,
    /// Yields the empty set for a program with no answer set.
    ReadsInconsistencyAsConsistency,
    /// Stops at its budget over a program with no answer set.
    LeavesTheSearchUndecided,
    /// Yields every answer set of a program that has one, yet concludes at its
    /// target, as a model limit would — never that the space closed.
    LeavesTheSpaceOpen,
    /// Yields, as the first model of a program that has answer sets, a set that
    /// is none of them.
    YieldsAStranger,
    /// Reads the positive loop by completion, with its unsupported model.
    SolvesByCompletion,
    /// Reads the head cycle by shifting its disjunction, with no model.
    ShiftsTheHeadCycle,
    /// Refuses to lower every program.
    RefusesEveryProgram,
    /// Refuses every unscoped solve.
    RefusesTheSolve,
    /// Exposes an empty ground program for every program.
    ExposesAnEmptyGroundProgram,
    /// Refuses the program no grounder can instantiate at the engine locus.
    RefusesTheUnsafeProgramOffItsLocus,
    /// Accepts the program no grounder can instantiate.
    AcceptsTheUnsafeProgram,
    /// Answers `solve_assuming` without declaring `assumptions`.
    AnswersUndeclaredAssumptions,
    /// Refuses `solve_assuming`, undeclared, at the engine locus.
    RefusesUndeclaredAssumptionsAtTheEngine,
    /// Declares `assumptions`, yet refuses `solve_assuming`.
    RefusesDeclaredAssumptions,
    /// Declares `assumptions`, yet ranges a scoped solve's models over the empty
    /// scenario.
    MisplacesTheScenario,
    /// Declares `assumptions`, yet a scoped solve yields no model.
    LosesTheScenariosModels,
    /// Declares `assumptions`, yet a scoped solve yields the unscoped models.
    IgnoresTheScenario,
    /// Declares `assumptions`, yet a scenario that admits no model reads as the
    /// unscoped program's first.
    ReadsAnyScenarioAsSatisfiable,
    /// Declares `assumptions` and reads each scenario's consistency rightly, yet
    /// witnesses a satisfiable one with the unscoped program's first model.
    WitnessesOutsideTheScenario,
    /// Declares the native door, yet answers the brave consequences for the
    /// cautious.
    MisanswersTheNativeCautious,
    /// Declares the native door, yet answers the consequences of a program with
    /// no answer set.
    AnswersTheConsequencesOfNoModel,
    /// Declares the native door and `assumptions`, yet refuses a request
    /// carrying a scenario.
    RefusesTheNativeScenario,
    /// Declares the native door and `assumptions`, yet its door ranges over the
    /// whole program whatever the scenario.
    IgnoresTheNativeScenario,
    /// Declares multi-shot solving, yet refuses `reset`.
    RefusesTheReset,
    /// Answers `reset` without declaring multi-shot solving.
    AnswersAnUndeclaredReset,
    /// Answers `ground` without declaring multi-shot solving.
    AnswersAnUndeclaredGround,
    /// Declares multi-shot solving, yet accepts assigning an atom that is not
    /// external.
    AcceptsANonExternal,
    /// Declares multi-shot solving, yet refuses every assignment as unsupported.
    RefusesAssignmentAsUnsupported,
    /// Declares multi-shot solving, yet refuses every assignment at the engine.
    RefusesAssignmentAtTheEngine,
    /// Declares externals, yet refuses every assignment at the request locus —
    /// the external atom's among them.
    AssignsNoExternal,
    /// Declares externals, and answers an assignment, yet holds the external
    /// false whatever it is assigned.
    IgnoresTheAssignment,
    /// Declares `cancellation`, yet answers no interrupt handle.
    WithholdsTheInterruptHandle,
    /// Answers an interrupt handle without declaring `cancellation`.
    OffersAnUndeclaredInterrupt,
    /// Answers a timed solve without declaring a time budget.
    AnswersAnUndeclaredTimedSolve,
    /// Registers an `@`-function without declaring `functions`.
    AnswersUndeclaredFunctions,
    /// Registers a propagator without declaring `propagators`.
    AnswersUndeclaredPropagators,
}

/// A backend answering the suite from its table, honouring exactly what it
/// declares unless its flaw says otherwise.
struct Stub {
    capabilities: Capabilities,
    flaw: Flaw,
    table: Vec<(&'static str, Program, Answers)>,
    /// The programs lowered since the last reset: at most one on a single-shot
    /// stub, whose `lower` replaces; any number on a multi-shot one, whose
    /// `lower` accumulates — so a suite that skipped the reset would be caught.
    loaded: Vec<usize>,
    /// Whether the external atom `a` is assigned true.
    a_holds: bool,
    /// The empty ground program a flawed stub exposes.
    nothing: GroundProgram,
}

impl Stub {
    /// A stub declaring `capabilities`, departing from the contract by `flaw`.
    fn new(capabilities: Capabilities, flaw: Flaw) -> Stub {
        Stub {
            capabilities,
            flaw,
            table: table(),
            loaded: Vec::new(),
            a_holds: false,
            nothing: GroundProgram::default(),
        }
    }

    /// The source of the one program loaded, if one alone is.
    fn loaded_source(&self) -> Option<&'static str> {
        match self.loaded[..] {
            [index] => Some(self.table[index].0),
            _ => None,
        }
    }

    /// The answer sets of the program loaded.
    fn answer_sets(&self) -> Result<Vec<AnswerSet>, Fault> {
        let [index] = self.loaded[..] else {
            return Err(Fault::engine(if self.loaded.is_empty() {
                "no program is loaded"
            } else {
                "several programs have accumulated"
            }));
        };
        let (source, _, answers) = &self.table[index];
        let mut sets = match answers {
            Answers::Known(sets) => sets.clone(),
            Answers::External => vec![if self.a_holds {
                set([constant("a"), constant("b")])
            } else {
                set([])
            }],
            Answers::Unsafe => Vec::new(),
        };
        match self.flaw {
            Flaw::DropsAnAnswerSet if sets.len() > 1 => {
                sets.pop();
            }
            Flaw::SolvesByCompletion if *source == POSITIVE_LOOP => {
                sets.push(set([constant("a"), constant("b")]));
            }
            Flaw::ShiftsTheHeadCycle if *source == HEAD_CYCLE => sets.clear(),
            _ => {}
        }
        Ok(sets)
    }

    /// The handle over `sets`, ranging over `scenario`: every set when the stub
    /// enumerates; otherwise the first, as a witness.
    fn enumerate(&self, mut sets: Vec<AnswerSet>, scenario: Scenario) -> Solved<'static> {
        if sets.is_empty() && self.flaw == Flaw::ReadsInconsistencyAsConsistency {
            sets.push(set([]));
        }
        if self.flaw == Flaw::YieldsAStranger && !sets.is_empty() {
            sets.insert(0, set([constant("stranger")]));
        }
        if self.flaw == Flaw::NeverEnds && !sets.is_empty() {
            return Solved::running(
                Box::new(Endless {
                    model: sets.swap_remove(0),
                }),
                scenario,
            );
        }
        if self.flaw == Flaw::FaultsMidStream && !sets.is_empty() {
            return Solved::running(
                Box::new(Faulting {
                    first: Some(sets.swap_remove(0)),
                    faulted: false,
                }),
                scenario,
            );
        }
        let terminal = if sets.is_empty() && self.flaw == Flaw::LeavesTheSearchUndecided {
            Conclusion::Budget
        } else if !sets.is_empty() && self.flaw == Flaw::LeavesTheSpaceOpen {
            Conclusion::Target
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
        if request.time.is_some()
            && !self.capabilities.budgets.time
            && self.flaw != Flaw::AnswersAnUndeclaredTimedSolve
        {
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
            .position(|(_, program, _)| program == lowered)
            .ok_or_else(|| Fault::engine("the program is outside the stub's table"))?;
        if matches!(self.table[index].2, Answers::Unsafe) {
            match self.flaw {
                Flaw::AcceptsTheUnsafeProgram => {}
                Flaw::RefusesTheUnsafeProgramOffItsLocus => {
                    return Err(Fault::engine("a variable nothing binds"));
                }
                _ => return Err(located_refusal(lowered)),
            }
        }
        if self.capabilities.multi_shot {
            self.loaded.push(index);
        } else {
            self.loaded = vec![index];
        }
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        (self.flaw == Flaw::ExposesAnEmptyGroundProgram).then_some(&self.nothing)
    }

    fn interrupt(&self) -> Option<Interrupt> {
        let offered = match self.flaw {
            Flaw::WithholdsTheInterruptHandle => false,
            Flaw::OffersAnUndeclaredInterrupt => true,
            _ => self.capabilities.cancellation,
        };
        offered.then_some(Interrupt)
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
            Flaw::WitnessesOutsideTheScenario => {
                if all.iter().any(|set| admits(scenario, set)) {
                    all.into_iter().take(1).collect()
                } else {
                    Vec::new()
                }
            }
            Flaw::ReadsAnyScenarioAsSatisfiable => {
                let admitted: Vec<AnswerSet> = all
                    .iter()
                    .filter(|set| admits(scenario, set))
                    .cloned()
                    .collect();
                if admitted.is_empty() {
                    all.into_iter().take(1).collect()
                } else {
                    admitted
                }
            }
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
        if self.capabilities.multi_shot || self.flaw == Flaw::AnswersAnUndeclaredGround {
            Ok(())
        } else {
            Err(Fault::unsupported())
        }
    }

    fn assign_external(&mut self, external: Symbol, value: TruthValue) -> Result<(), Fault> {
        if !self.capabilities.multi_shot {
            return Err(Fault::unsupported());
        }
        match self.flaw {
            Flaw::RefusesAssignmentAsUnsupported => return Err(Fault::unsupported()),
            Flaw::RefusesAssignmentAtTheEngine => {
                return Err(Fault::engine("the stub cannot assign"));
            }
            Flaw::AssignsNoExternal => return Err(Fault::request("the stub assigns nothing")),
            _ => {}
        }
        // The external program's `a` is the one external atom in the table.
        let external_a =
            self.loaded_source() == Some("#external a. b :- a.") && external == constant("a");
        if external_a {
            self.a_holds = value == TruthValue::True && self.flaw != Flaw::IgnoresTheAssignment;
            Ok(())
        } else if self.flaw == Flaw::AcceptsANonExternal {
            Ok(())
        } else {
            Err(Fault::request("the atom is not external"))
        }
    }

    fn reset(&mut self) -> Result<(), Fault> {
        match (self.capabilities.multi_shot, self.flaw) {
            (true, Flaw::RefusesTheReset) => Err(Fault::engine("the stub cannot reset")),
            (false, Flaw::AnswersAnUndeclaredReset) | (true, _) => {
                self.loaded.clear();
                self.a_holds = false;
                Ok(())
            }
            (false, _) => Err(Fault::unsupported()),
        }
    }

    fn register_function(&mut self, _function: Box<dyn Function>) -> Result<(), Fault> {
        if self.capabilities.functions || self.flaw == Flaw::AnswersUndeclaredFunctions {
            Ok(())
        } else {
            Err(Fault::unsupported())
        }
    }

    fn register_propagator(&mut self, _propagator: Box<dyn Propagator>) -> Result<(), Fault> {
        if self.capabilities.propagators || self.flaw == Flaw::AnswersUndeclaredPropagators {
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

/// The declaration of a backend that decides consistency without enumerating,
/// and honours assumptions.
fn deciding_under_assumptions() -> Capabilities {
    let mut capabilities = deciding();
    capabilities.assumptions = true;
    capabilities
}

/// The declaration of an enumerating backend realising `capability` alone —
/// with the multi-shot solving an external's assignment needs.
fn only(capability: Capability) -> Capabilities {
    let mut capabilities = enumerating();
    match capability {
        Capability::NativeConsequences => {
            capabilities.native_consequences = ConsequenceSupport::Native;
        }
        Capability::Assumptions => capabilities.assumptions = true,
        Capability::MultiShot => capabilities.multi_shot = true,
        Capability::Externals => {
            capabilities.multi_shot = true;
            capabilities.externals = true;
        }
        Capability::Cancellation => capabilities.cancellation = true,
        Capability::TimeBudget => capabilities.budgets.time = true,
        Capability::Functions => capabilities.functions = true,
        Capability::Propagators => capabilities.propagators = true,
        _ => {}
    }
    capabilities
}

/// Every capability a stub outside the crate can realise: all but optimization,
/// whose outcome has no public constructor.
const REALISABLE: [Capability; 8] = [
    Capability::NativeConsequences,
    Capability::Assumptions,
    Capability::MultiShot,
    Capability::Externals,
    Capability::Cancellation,
    Capability::TimeBudget,
    Capability::Functions,
    Capability::Propagators,
];

/// The declaration of a backend realising every capability a stub outside the
/// crate can.
fn realising() -> Capabilities {
    let mut capabilities = enumerating();
    capabilities.native_consequences = ConsequenceSupport::Native;
    capabilities.assumptions = true;
    capabilities.multi_shot = true;
    capabilities.externals = true;
    capabilities.cancellation = true;
    capabilities.budgets.time = true;
    capabilities.functions = true;
    capabilities.propagators = true;
    capabilities
}

/// The suite's report on a stub declaring `capabilities` with `flaw`.
fn report(capabilities: Capabilities, flaw: Flaw) -> ConformanceReport {
    conformance::run(&mut Stub::new(capabilities, flaw))
}

/// The checks the report failed, each with how the backend broke it.
fn failures(report: &ConformanceReport) -> HashSet<(Check, Breach)> {
    report
        .entries()
        .filter_map(|(check, verdict)| match verdict {
            Verdict::Failed(failure) => Some((check, failure.breach())),
            _ => None,
        })
        .collect()
}

/// A flaw, the declaration a stub carrying it runs under, and every check it
/// fails, each with how.
type Expectation = (Flaw, Capabilities, Vec<(Check, Breach)>);

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
fn a_backend_that_stops_at_its_witness_conforms() {
    // Without declaring enumeration, the stub yields one witness and concludes
    // that it met its target — never that the space closed.
    let report = report(deciding(), Flaw::Faithful);
    assert!(report.is_conformant(), "{report}");
}

#[test]
fn a_deciding_backend_honouring_assumptions_conforms() {
    let report = report(deciding_under_assumptions(), Flaw::Faithful);
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
    for capability in REALISABLE {
        assert_eq!(
            report.verdict(Check::Capability(capability)),
            Some(&Verdict::Passed),
            "{report}",
        );
    }
}

#[test]
fn every_capability_declared_alone_conforms() {
    // Declaring one bit at a time holds the suite to reading each bit as that
    // capability's own, and no other's.
    for capability in REALISABLE {
        let report = report(only(capability), Flaw::Faithful);
        assert!(report.is_conformant(), "{capability}: {report}");
        assert_eq!(
            report.verdict(Check::Capability(capability)),
            Some(&Verdict::Passed),
            "{capability}: {report}",
        );
    }
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
fn the_cancellation_check_is_skipped_while_no_search_can_be_cancelled() {
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

#[test]
fn an_undeclared_externals_bit_binds_nothing() {
    let report = report(only(Capability::MultiShot), Flaw::Faithful);
    assert!(
        skipped(&report, Check::Capability(Capability::Externals)),
        "{report}"
    );
}

// ---- A broken backend fails exactly the checks that name its break ----

// One table: each flaw beside the declaration it runs under and the exact
// checks it fails; splitting it would scatter the one universal the law states.
#[test]
#[allow(clippy::too_many_lines)]
fn each_flaw_fails_exactly_the_checks_that_name_it() {
    use Breach::{Accepted, Misanswered, Mislocated, Refused};
    use Capability as C;
    use Check::{
        Capability as Declared, ExhaustionIsEarned as Earned, GroundProgramIsFaithful as Ground,
        NonExternalAssignmentRefuses as NonExternal, OutcomeCorrectness as Outcome,
        ProgramFaultIsLocated as Located,
    };
    let table: Vec<Expectation> = vec![
        (
            Flaw::DropsAnAnswerSet,
            enumerating(),
            vec![(Outcome, Misanswered), (Earned, Misanswered)],
        ),
        (
            Flaw::ClaimsExhaustionAtItsWitness,
            deciding(),
            vec![(Earned, Misanswered)],
        ),
        (Flaw::NeverEnds, enumerating(), vec![(Outcome, Misanswered)]),
        (
            Flaw::FaultsMidStream,
            enumerating(),
            vec![(Outcome, Refused)],
        ),
        (
            Flaw::ReadsInconsistencyAsConsistency,
            enumerating(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::LeavesTheSearchUndecided,
            enumerating(),
            vec![(Outcome, Refused)],
        ),
        (
            Flaw::LeavesTheSpaceOpen,
            enumerating(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::YieldsAStranger,
            deciding(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::SolvesByCompletion,
            enumerating(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::ShiftsTheHeadCycle,
            enumerating(),
            vec![(Outcome, Misanswered), (Earned, Misanswered)],
        ),
        (
            Flaw::RefusesEveryProgram,
            enumerating(),
            vec![(Outcome, Refused), (Located, Mislocated)],
        ),
        (
            Flaw::RefusesTheSolve,
            enumerating(),
            vec![(Outcome, Refused)],
        ),
        (
            Flaw::ExposesAnEmptyGroundProgram,
            enumerating(),
            vec![(Ground, Misanswered)],
        ),
        (
            Flaw::RefusesTheUnsafeProgramOffItsLocus,
            enumerating(),
            vec![(Located, Mislocated)],
        ),
        (
            Flaw::AcceptsTheUnsafeProgram,
            enumerating(),
            vec![(Located, Accepted)],
        ),
        (
            Flaw::AnswersUndeclaredAssumptions,
            enumerating(),
            vec![(Declared(C::Assumptions), Accepted)],
        ),
        (
            Flaw::RefusesUndeclaredAssumptionsAtTheEngine,
            enumerating(),
            vec![(Declared(C::Assumptions), Mislocated)],
        ),
        (
            Flaw::RefusesDeclaredAssumptions,
            realising(),
            vec![(Declared(C::Assumptions), Refused)],
        ),
        (
            Flaw::MisplacesTheScenario,
            realising(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::LosesTheScenariosModels,
            realising(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::IgnoresTheScenario,
            realising(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::IgnoresTheScenario,
            deciding_under_assumptions(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::ReadsAnyScenarioAsSatisfiable,
            realising(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::WitnessesOutsideTheScenario,
            realising(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::WitnessesOutsideTheScenario,
            deciding_under_assumptions(),
            vec![(Declared(C::Assumptions), Misanswered)],
        ),
        (
            Flaw::MisanswersTheNativeCautious,
            realising(),
            vec![(Declared(C::NativeConsequences), Misanswered)],
        ),
        (
            Flaw::AnswersTheConsequencesOfNoModel,
            realising(),
            vec![(Declared(C::NativeConsequences), Misanswered)],
        ),
        (
            Flaw::RefusesTheNativeScenario,
            realising(),
            vec![(Declared(C::NativeConsequences), Refused)],
        ),
        (
            Flaw::IgnoresTheNativeScenario,
            realising(),
            vec![(Declared(C::NativeConsequences), Misanswered)],
        ),
        (
            Flaw::RefusesTheReset,
            realising(),
            vec![(Outcome, Refused), (Declared(C::MultiShot), Refused)],
        ),
        (
            Flaw::AnswersAnUndeclaredReset,
            enumerating(),
            vec![(Declared(C::MultiShot), Accepted)],
        ),
        (
            Flaw::AnswersAnUndeclaredGround,
            enumerating(),
            vec![(Declared(C::MultiShot), Accepted)],
        ),
        (
            Flaw::AcceptsANonExternal,
            realising(),
            vec![(NonExternal, Accepted)],
        ),
        (
            Flaw::RefusesAssignmentAsUnsupported,
            realising(),
            vec![(NonExternal, Refused), (Declared(C::Externals), Refused)],
        ),
        (
            Flaw::RefusesAssignmentAtTheEngine,
            realising(),
            vec![(NonExternal, Mislocated), (Declared(C::Externals), Refused)],
        ),
        (
            Flaw::AssignsNoExternal,
            realising(),
            vec![(Declared(C::Externals), Refused)],
        ),
        (
            Flaw::IgnoresTheAssignment,
            realising(),
            vec![(Declared(C::Externals), Misanswered)],
        ),
        (
            Flaw::WithholdsTheInterruptHandle,
            realising(),
            vec![(Declared(C::Cancellation), Refused)],
        ),
        (
            Flaw::OffersAnUndeclaredInterrupt,
            enumerating(),
            vec![(Declared(C::Cancellation), Accepted)],
        ),
        (
            Flaw::AnswersAnUndeclaredTimedSolve,
            enumerating(),
            vec![(Declared(C::TimeBudget), Accepted)],
        ),
        (
            Flaw::AnswersUndeclaredFunctions,
            enumerating(),
            vec![(Declared(C::Functions), Accepted)],
        ),
        (
            Flaw::AnswersUndeclaredPropagators,
            enumerating(),
            vec![(Declared(C::Propagators), Accepted)],
        ),
    ];
    for (flaw, declared, expected) in table {
        let report = report(declared, flaw);
        let expected: HashSet<(Check, Breach)> = expected.into_iter().collect();
        assert_eq!(failures(&report), expected, "{flaw:?}: {report}");
    }
}

#[test]
fn a_run_that_never_ends_fails_rather_than_holding_the_suite() {
    let report = report(enumerating(), Flaw::NeverEnds);
    assert!(
        matches!(
            report.verdict(Check::OutcomeCorrectness),
            Some(Verdict::Failed(failure)) if failure.breach() == Breach::Misanswered
        ),
        "{report}"
    );
}

#[test]
fn a_failure_names_the_corpus_program_it_broke_on() {
    let report = report(enumerating(), Flaw::DropsAnAnswerSet);
    let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
        panic!("the dropped answer set fails its outcome: {report}");
    };
    assert_eq!(failure.case(), Some("an even loop"));
}

#[test]
fn a_faulted_stream_s_failure_carries_the_fault() {
    let report = report(enumerating(), Flaw::FaultsMidStream);
    let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
        panic!("the faulted stream fails its outcome: {report}");
    };
    assert_eq!(failure.breach(), Breach::Refused);
    assert_eq!(failure.fault().map(Fault::locus), Some(Locus::Engine));
}

#[test]
fn a_refused_obligation_carries_the_backend_s_fault() {
    let report = report(enumerating(), Flaw::RefusesTheSolve);
    let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
        panic!("the refused solve fails its outcome: {report}");
    };
    assert_eq!(failure.fault().map(Fault::locus), Some(Locus::Engine));
}

// ---- The pathologies the vocabulary makes unconstructible ----

#[test]
fn an_enumeration_has_no_improving_trajectory() {
    // The improving trajectory is `Optimized`'s alone (docs/design/solve.md
    // §5.3): asking a `Solved` for one does not compile.
    trybuild::TestCases::new().compile_fail("tests/ui/solved_trajectory.rs");
}
