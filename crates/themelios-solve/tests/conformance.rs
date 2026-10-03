//! The conformance suite over engine-free stub backends (docs/design/solve.md
//! §13.1): a backend answering the suite's corpus from a table of its known
//! answer sets — streamed through the real `Backend`/`Run`/`Solved::running`
//! door — meets every obligation, and a backend broken in one way fails exactly
//! the checks that name that way, each with its typed breach, so the suite has
//! teeth. The authoritative runs are a real engine's; these run the suite's own
//! machinery through the contract.

use std::collections::HashSet;

use themelios_base::span::{ByteOffset, Location};
use themelios_program::program::Part;
use themelios_program::raise::raise_str;
use themelios_program::{
    Dialect, Name, Origin, Program, Provenance, Sign, SourceId, Statement, Symbol, WithProvenance,
};
use themelios_solve::agent::{Assumption, Scenario};
use themelios_solve::bridge::{Door, GroundProgram};
use themelios_solve::conformance::{self, Breach, Check, ConformanceReport, Skip, Verdict};
use themelios_solve::contract::{
    Backend, Cancel, Capabilities, Capability, ConsequenceRequest, ConsequenceSupport, Fault,
    GroundOptions, Locus, Mode, Presupposition, SolveRequest, TruthValue,
};
use themelios_solve::extend::{Function, Propagator};
use themelios_solve::outcome::{
    AnswerSet, Conclusion, Consequences, Model, NativeAnswer, Run, Solved, Truncation,
};

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

/// The positive loop constrained to hold `a`: no answer set, though its
/// completion admits `{a, b}`.
const CONSTRAINED_LOOP: &str = "a :- b. b :- a. :- not a.";

/// The external program: its `a` is the one external atom in the table.
const EXTERNAL: &str = "#external a. b :- a.";

/// The rule the multi-shot probe lowers over the fact `a.`, with no reset
/// between them.
const RULE: &str = "b :- a.";

/// A fact, then a fact over a variable nothing binds: no grounder can
/// instantiate the second statement.
const UNSAFE: &str = "a. p(X).";

/// A choice under an objective: its stable models, the objective ignored as a
/// solve ignores it, are `{}` and `{a}`; its optimum is `{}`.
const OBJECTIVE: &str = "{ a }. #minimize { 1 : a }.";

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
        (CONSTRAINED_LOOP, known(vec![])),
        (RULE, known(vec![set([])])),
        (OBJECTIVE, known(vec![set([]), set([constant("a")])])),
        (EXTERNAL, Answers::External),
        (UNSAFE, Answers::Unsafe),
    ]
    .into_iter()
    .map(|(source, answers)| (source, program(source), answers))
    .collect()
}

/// The positive atom a strongly negated `symbol` is the contrary of, if it is
/// one.
fn positive_of(symbol: &Symbol) -> Option<Symbol> {
    match symbol {
        Symbol::Function {
            name,
            arguments,
            sign: Sign::Negative,
        } => Some(Symbol::function(
            name.clone(),
            arguments.iter().cloned(),
            Sign::Positive,
        )),
        _ => None,
    }
}

/// Where each statement of `program` was parsed.
fn parsed_locations(program: &Program) -> impl Iterator<Item = Location> + '_ {
    program
        .statements()
        .flat_map(|node| node.provenance().origins())
        .filter_map(|origin| match origin {
            Origin::Parsed(location) => Some(*location),
            _ => None,
        })
}

/// The offset, within the unsafe program's text, of its atom over a variable
/// nothing binds.
fn unsafe_offset() -> ByteOffset {
    let at = UNSAFE
        .find("p(X)")
        .expect("the unsafe program holds its unsafe atom");
    ByteOffset::new(u32::try_from(at).expect("a short text"))
}

/// The unsafe program's statement whose parsed location does — or, with
/// `unsafe_one` false, does not — hold the atom over a variable nothing binds.
fn statement_of(program: &Program, unsafe_one: bool) -> &WithProvenance<Statement> {
    program
        .statements()
        .find(|node| {
            node.provenance().origins().any(|origin| {
                matches!(origin, Origin::Parsed(location)
                    if location.span.contains(unsafe_offset()) == unsafe_one)
            })
        })
        .expect("each of the unsafe program's statements is located")
}

/// A program fault refusing `statement`, as a backend refusing the unsafe
/// program raises it.
fn refusal_of(statement: &WithProvenance<Statement>) -> Fault {
    Fault::program("a variable nothing binds", statement)
}

/// The refusal of the program no grounder can instantiate: a program fault
/// refusing its unsafe statement.
fn located_refusal(program: &Program) -> Fault {
    refusal_of(statement_of(program, true))
}

/// The same refusal of the program's other statement, the leading fact — as an
/// adapter mapping its engine's error to the wrong statement raises it.
fn mislocated_refusal(program: &Program) -> Fault {
    refusal_of(statement_of(program, false))
}

/// The same refusal of the unsafe statement, placed at its offsets in a source
/// the program is not — as an adapter reporting every location in its own text
/// raises it.
fn refusal_in_another_source(program: &Program) -> Fault {
    let statement = statement_of(program, true);
    let Location { span, .. } = parsed_locations(program)
        .find(|location| location.span.contains(unsafe_offset()))
        .expect("the unsafe statement is located");
    let elsewhere = Origin::Parsed(Location {
        source: SourceId::new(ANOTHER_SOURCE),
        span,
    });
    refusal_of(&WithProvenance::new(
        statement.get().clone(),
        Provenance::from(elsewhere),
    ))
}

/// A source id no suite program is raised under.
const ANOTHER_SOURCE: u32 = 4242;

/// The scenario fixing `a` to hold (`true`) or not to (`false`).
fn fixing_a(holds: bool) -> Scenario {
    let assumption = Assumption::new(constant("a"), holds).expect("a constant is an atom");
    [assumption].into_iter().collect()
}

// ---- The stub ----

/// A scripted enumeration: the answer sets, then the search's end, concluded as
/// `terminal` — unless it `concludes` nothing, its search left open.
struct Enumeration {
    sets: std::vec::IntoIter<AnswerSet>,
    terminal: Conclusion,
    concludes: bool,
    ended: bool,
}

impl Run for Enumeration {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        let next = self.sets.next().map(|set| Ok(Model::of(set)));
        if next.is_none() {
            self.ended = true;
        }
        next
    }

    fn conclusion(&self) -> Option<Conclusion> {
        (self.ended && self.concludes).then_some(self.terminal)
    }
}

/// Past this many models, an endless run fails the test outright: the suite
/// reads at most one model past a program's answer sets, so a read this far is
/// the suite holding on — the hang its bound exists to prevent — stopped
/// before it can fill memory.
const ENDLESS_TRIPWIRE: usize = 1024;

/// A search that yields the same model forever — the missing blocking clause.
struct Endless {
    model: AnswerSet,
    yielded: usize,
}

impl Run for Endless {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        self.yielded += 1;
        assert!(
            self.yielded <= ENDLESS_TRIPWIRE,
            "the suite read {ENDLESS_TRIPWIRE} models of a run that never ends: its bound failed"
        );
        Some(Ok(Model::of(self.model.clone())))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        None
    }
}

/// A search that faults: its first model, where it has one, then the fault,
/// then the end — with no conclusion, a faulted search having reached none.
struct Faulting {
    first: Option<AnswerSet>,
    fault: Option<Fault>,
}

impl Run for Faulting {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if let Some(first) = self.first.take() {
            return Some(Ok(Model::of(first)));
        }
        self.fault.take().map(Err)
    }

    fn conclusion(&self) -> Option<Conclusion> {
        None
    }
}

/// A search that, having ended, yields its first model once more — a stream
/// that is not fused.
struct Unfused {
    sets: std::vec::IntoIter<AnswerSet>,
    again: Option<AnswerSet>,
    ended: bool,
}

impl Run for Unfused {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if self.ended {
            return self.again.take().map(|set| Ok(Model::of(set)));
        }
        let next = self.sets.next().map(|set| Ok(Model::of(set)));
        self.ended = next.is_none();
        next
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(Conclusion::Exhausted)
    }
}

/// When the stub grounds — and so refuses — the program no grounder can
/// instantiate: at lowering, at the solve, or at the first model its search
/// would yield, as a lazily grounding engine does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GroundsAt {
    Lowering,
    TheSolve,
    TheFirstModel,
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
    /// Faults at the engine before the first model of a program that has one.
    FaultsBeforeAModel,
    /// Yields its first model once more after its stream ended.
    YieldsPastItsEnd,
    /// Ends the stream of a program that has answer sets without concluding
    /// its search.
    LeavesItsSearchOpen,
    /// Ranges a plain solve's models over a scenario fixing `a`.
    RangesAPlainSolveOverAScenario,
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
    /// Yields each model with the contrary of its every strongly negated atom
    /// beside it.
    YieldsAnInconsistentModel,
    /// Solves under the objective, as an engine optimising by default does,
    /// yielding the program under an objective its optimum alone.
    OptimizesTheSolve,
    /// Reads the positive loops by completion, with their unsupported model.
    SolvesByCompletion,
    /// Reads the head cycle by shifting its disjunction, with no model.
    ShiftsTheHeadCycle,
    /// Refuses to lower every program.
    RefusesEveryProgram,
    /// Refuses to lower the external program.
    RefusesTheExternalProgram,
    /// Refuses to lower the rule the multi-shot probe lowers over a fact.
    RefusesTheRule,
    /// Refuses every unscoped solve.
    RefusesTheSolve,
    /// Exposes an empty ground program for every program.
    ExposesAnEmptyGroundProgram,
    /// Refuses the program no grounder can instantiate at the engine locus.
    RefusesTheUnsafeProgramOffItsLocus,
    /// Accepts the program no grounder can instantiate.
    AcceptsTheUnsafeProgram,
    /// Refuses the program no grounder can instantiate at the location of its
    /// other statement.
    LocatesTheFaultElsewhere,
    /// Refuses the program no grounder can instantiate at its unsafe
    /// statement's offsets, in another source.
    LocatesTheFaultInAnotherSource,
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
    /// Declares `assumptions`, yet drops an assumption on an atom no answer set
    /// holds — in `solve_assuming` and, declaring the native door, there too.
    DropsAnUnderivableAssumption,
    /// Declares `assumptions`, yet a plain solve keeps the last scoped solve's
    /// scenario.
    LeaksTheScenario,
    /// Declares the native door, yet answers the brave consequences for the
    /// cautious.
    MisanswersTheNativeCautious,
    /// Declares the native door, yet answers the consequences of a program with
    /// no answer set.
    AnswersTheConsequencesOfNoModel,
    /// Declares the native door and `assumptions`, yet refuses a request
    /// carrying a scenario.
    RefusesTheNativeScenario,
    /// Declares the native door and `assumptions`, yet where a scenario admits
    /// some model, its door ranges over the whole program.
    IgnoresTheNativeScenario,
    /// Declares the native door and `assumptions`, yet answers under a scenario
    /// that admits no model.
    AnswersAnUnsatisfiableScenario,
    /// Declares the native door, yet reports every search stopped at its budget,
    /// though none was asked.
    StopsTheNativeSearchShort,
    /// Declares multi-shot solving, yet refuses `reset`.
    RefusesTheReset,
    /// Declares multi-shot solving, yet its `lower` replaces what it keeps.
    ReplacesWhatItLowers,
    /// Declares multi-shot solving, yet its `reset` keeps what was lowered.
    ResetsNothing,
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
    /// Answers a timed `solve_assuming` without declaring a time budget.
    AnswersAnUndeclaredTimedScopedSolve,
    /// Declares a time budget, yet a timed solve yields a set that is no answer
    /// set.
    MisanswersATimedSolve,
    /// Registers an `@`-function without declaring `functions`.
    AnswersUndeclaredFunctions,
    /// Registers a propagator without declaring `propagators`.
    AnswersUndeclaredPropagators,
}

/// A cancellation primitive that cuts nothing short — the stub's searches are
/// scripted, with nothing to cut.
struct Unheeded;

impl Cancel for Unheeded {
    fn cancel(&self) {}
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
    /// When the stub grounds the program no grounder can instantiate.
    grounds_at: GroundsAt,
    /// That program's refusal, lowered but not yet grounded.
    pending_refusal: Option<Fault>,
    /// The last scoped solve's scenario, which a leaking stub keeps.
    leaked: Scenario,
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
            grounds_at: GroundsAt::Lowering,
            pending_refusal: None,
            leaked: Scenario::default(),
        }
    }

    /// This stub, grounding the program no grounder can instantiate at
    /// `stage`.
    fn grounding_at(mut self, stage: GroundsAt) -> Stub {
        self.grounds_at = stage;
        self
    }

    /// The source of the one program loaded, if one alone is.
    fn loaded_source(&self) -> Option<&'static str> {
        match self.loaded[..] {
            [index] => Some(self.table[index].0),
            _ => None,
        }
    }

    /// The answer sets of what is loaded: one program, or the one accumulation
    /// the suite lowers — the fact `a.`, then the rule over it.
    fn answer_sets(&self) -> Result<Vec<AnswerSet>, Fault> {
        let (source, mut sets) = match self.loaded[..] {
            [index] => (self.table[index].0, self.known(index)),
            [fact, rule] if self.table[fact].0 == "a." && self.table[rule].0 == RULE => {
                ("a. b :- a.", vec![set([constant("a"), constant("b")])])
            }
            [] => return Err(Fault::engine("no program is loaded")),
            _ => return Err(Fault::engine("several programs have accumulated")),
        };
        match self.flaw {
            Flaw::DropsAnAnswerSet if sets.len() > 1 => {
                sets.pop();
            }
            Flaw::SolvesByCompletion if source == POSITIVE_LOOP || source == CONSTRAINED_LOOP => {
                sets.push(set([constant("a"), constant("b")]));
            }
            Flaw::ShiftsTheHeadCycle if source == HEAD_CYCLE => sets.clear(),
            Flaw::OptimizesTheSolve if source == OBJECTIVE => sets.retain(AnswerSet::is_empty),
            Flaw::YieldsAnInconsistentModel => {
                for set in &mut sets {
                    let contraries: Vec<Symbol> = set.iter().filter_map(positive_of).collect();
                    set.extend(contraries);
                }
            }
            _ => {}
        }
        Ok(sets)
    }

    /// The answer sets of the table's program at `index`.
    fn known(&self, index: usize) -> Vec<AnswerSet> {
        match &self.table[index].2 {
            Answers::Known(sets) => sets.clone(),
            Answers::External => vec![if self.a_holds {
                set([constant("a"), constant("b")])
            } else {
                set([])
            }],
            Answers::Unsafe => Vec::new(),
        }
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
                    yielded: 0,
                }),
                scenario,
            );
        }
        if self.flaw == Flaw::FaultsMidStream && !sets.is_empty() {
            return Solved::running(
                Box::new(Faulting {
                    first: Some(sets.swap_remove(0)),
                    fault: Some(Fault::engine("the stub's engine died mid-search")),
                }),
                scenario,
            );
        }
        if self.flaw == Flaw::FaultsBeforeAModel && !sets.is_empty() {
            return Solved::running(
                Box::new(Faulting {
                    first: None,
                    fault: Some(Fault::engine(
                        "the stub's engine died before its first model",
                    )),
                }),
                scenario,
            );
        }
        if self.flaw == Flaw::YieldsPastItsEnd && !sets.is_empty() {
            return Solved::running(
                Box::new(Unfused {
                    again: sets.first().cloned(),
                    sets: sets.into_iter(),
                    ended: false,
                }),
                scenario,
            );
        }
        let concludes = sets.is_empty() || self.flaw != Flaw::LeavesItsSearchOpen;
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
                concludes,
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
            return Err(Fault::request(
                "the stub enforces no time budget",
                Presupposition::UnrealisableBudget,
            ));
        }
        if self.flaw == Flaw::RefusesTheSolve {
            return Err(Fault::engine("the stub refuses to solve"));
        }
        if let Some(refusal) = self.pending_refusal.clone() {
            match self.grounds_at {
                GroundsAt::TheSolve => return Err(refusal),
                GroundsAt::TheFirstModel => {
                    return Ok(Solved::running(
                        Box::new(Faulting {
                            first: None,
                            fault: Some(refusal),
                        }),
                        Scenario::default(),
                    ));
                }
                GroundsAt::Lowering => {}
            }
        }
        if request.time.is_some() && self.flaw == Flaw::MisanswersATimedSolve {
            return Ok(self.enumerate(vec![set([constant("stranger")])], Scenario::default()));
        }
        let mut sets = self.answer_sets()?;
        if self.flaw == Flaw::LeaksTheScenario {
            sets.retain(|set| admits(&self.leaked, set));
        }
        let ranged = if self.flaw == Flaw::RangesAPlainSolveOverAScenario {
            fixing_a(true)
        } else {
            Scenario::default()
        };
        Ok(self.enumerate(sets, ranged))
    }

    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
        let Door::Program(lowered) = door else {
            return Err(Fault::engine("the stub lowers Door B alone"));
        };
        if self.flaw == Flaw::RefusesEveryProgram {
            return Err(Fault::engine("the stub refuses every program"));
        }
        let index = self
            .table
            .iter()
            .position(|(_, program, _)| program == lowered)
            .ok_or_else(|| Fault::engine("the program is outside the stub's table"))?;
        if self.flaw == Flaw::RefusesTheExternalProgram && self.table[index].0 == EXTERNAL {
            return Err(Fault::engine("the stub declares no external atom"));
        }
        if self.flaw == Flaw::RefusesTheRule && self.table[index].0 == RULE {
            return Err(Fault::engine("the stub lowers no rule over a fact"));
        }
        let mut pending = None;
        if matches!(self.table[index].2, Answers::Unsafe) {
            match self.flaw {
                Flaw::AcceptsTheUnsafeProgram => {}
                Flaw::RefusesTheUnsafeProgramOffItsLocus => {
                    return Err(Fault::engine("a variable nothing binds"));
                }
                Flaw::LocatesTheFaultElsewhere => return Err(mislocated_refusal(lowered)),
                Flaw::LocatesTheFaultInAnotherSource => {
                    return Err(refusal_in_another_source(lowered));
                }
                _ if self.grounds_at == GroundsAt::Lowering => {
                    return Err(located_refusal(lowered));
                }
                _ => pending = Some(located_refusal(lowered)),
            }
        }
        if self.capabilities.multi_shot && self.flaw != Flaw::ReplacesWhatItLowers {
            self.loaded.push(index);
        } else {
            self.loaded = vec![index];
        }
        self.pending_refusal = pending;
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        (self.flaw == Flaw::ExposesAnEmptyGroundProgram).then_some(&self.nothing)
    }

    fn interrupt(&self) -> Option<Box<dyn Cancel>> {
        let offered = match self.flaw {
            Flaw::WithholdsTheInterruptHandle => false,
            Flaw::OffersAnUndeclaredInterrupt => true,
            _ => self.capabilities.cancellation,
        };
        offered.then(|| Box::new(Unheeded) as Box<dyn Cancel>)
    }

    fn solve_assuming(
        &mut self,
        scenario: &Scenario,
        request: &SolveRequest,
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
            return Err(Fault::unsupported(Capability::Assumptions));
        }
        if request.time.is_some()
            && !self.capabilities.budgets.time
            && self.flaw != Flaw::AnswersAnUndeclaredTimedScopedSolve
        {
            return Err(Fault::request(
                "the stub enforces no time budget",
                Presupposition::UnrealisableBudget,
            ));
        }
        let request_scenario = scenario;
        let all = self.answer_sets()?;
        if self.flaw == Flaw::LeaksTheScenario {
            self.leaked = scenario.clone();
        }
        let honoured: Scenario = if self.flaw == Flaw::DropsAnUnderivableAssumption {
            scenario
                .assumptions()
                .filter(|assumption| all.iter().any(|set| set.contains(assumption.atom())))
                .cloned()
                .collect()
        } else {
            scenario.clone()
        };
        let scenario = &honoured;
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
            request_scenario.clone()
        };
        Ok(self.enumerate(admitted, ranged))
    }

    fn ground(&mut self, _parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        if self.capabilities.multi_shot || self.flaw == Flaw::AnswersAnUndeclaredGround {
            Ok(())
        } else {
            Err(Fault::unsupported(Capability::MultiShot))
        }
    }

    fn assign_external(&mut self, external: Symbol, value: TruthValue) -> Result<(), Fault> {
        if !self.capabilities.multi_shot {
            return Err(Fault::unsupported(Capability::MultiShot));
        }
        match self.flaw {
            Flaw::RefusesAssignmentAsUnsupported => {
                return Err(Fault::unsupported(Capability::MultiShot));
            }
            Flaw::RefusesAssignmentAtTheEngine => {
                return Err(Fault::engine("the stub cannot assign"));
            }
            Flaw::AssignsNoExternal => {
                return Err(Fault::request(
                    "the stub assigns nothing",
                    Presupposition::NotExternal,
                ));
            }
            _ => {}
        }
        // The external program's `a` is the one external atom in the table.
        let external_a = self.loaded_source() == Some(EXTERNAL) && external == constant("a");
        if external_a {
            self.a_holds = value == TruthValue::True && self.flaw != Flaw::IgnoresTheAssignment;
            Ok(())
        } else if self.flaw == Flaw::AcceptsANonExternal {
            Ok(())
        } else {
            Err(Fault::request(
                "the atom is not external",
                Presupposition::NotExternal,
            ))
        }
    }

    fn reset(&mut self) -> Result<(), Fault> {
        match (self.capabilities.multi_shot, self.flaw) {
            (true, Flaw::RefusesTheReset) => Err(Fault::engine("the stub cannot reset")),
            (true, Flaw::ResetsNothing) => Ok(()),
            (false, Flaw::AnswersAnUndeclaredReset) | (true, _) => {
                self.loaded.clear();
                self.a_holds = false;
                self.pending_refusal = None;
                Ok(())
            }
            (false, _) => Err(Fault::unsupported(Capability::MultiShot)),
        }
    }

    fn register_function(&mut self, _function: Box<dyn Function>) -> Result<(), Fault> {
        if self.capabilities.functions || self.flaw == Flaw::AnswersUndeclaredFunctions {
            Ok(())
        } else {
            Err(Fault::unsupported(Capability::Functions))
        }
    }

    fn register_propagator(&mut self, _propagator: Box<dyn Propagator>) -> Result<(), Fault> {
        if self.capabilities.propagators || self.flaw == Flaw::AnswersUndeclaredPropagators {
            Ok(())
        } else {
            Err(Fault::unsupported(Capability::Propagators))
        }
    }

    fn consequences_native(
        &mut self,
        mode: Mode,
        request: &ConsequenceRequest,
    ) -> Result<NativeAnswer, Fault> {
        if self.capabilities.native_consequences != ConsequenceSupport::Native {
            return Err(Fault::unsupported(Capability::NativeConsequences));
        }
        if self.flaw == Flaw::RefusesTheNativeScenario
            && request.scenario.assumptions().next().is_some()
        {
            return Err(Fault::unsupported(Capability::Assumptions));
        }
        if self.flaw == Flaw::StopsTheNativeSearchShort {
            return Ok(NativeAnswer::Stopped(Truncation::Budget));
        }
        let scoped = self.flaw != Flaw::IgnoresTheNativeScenario;
        let all = self.answer_sets()?;
        let honoured: Scenario = if self.flaw == Flaw::DropsAnUnderivableAssumption {
            request
                .scenario
                .assumptions()
                .filter(|assumption| all.iter().any(|set| set.contains(assumption.atom())))
                .cloned()
                .collect()
        } else {
            request.scenario.clone()
        };
        let admitted: Vec<AnswerSet> = all
            .iter()
            .filter(|set| admits(&honoured, set))
            .cloned()
            .collect();
        let sets = if scoped || admitted.is_empty() {
            admitted
        } else {
            all
        };
        let scoped_request = request.scenario.assumptions().next().is_some();
        let answers_anyway = self.flaw == Flaw::AnswersTheConsequencesOfNoModel
            || (scoped_request && self.flaw == Flaw::AnswersAnUnsatisfiableScenario);
        if sets.is_empty() && !answers_anyway {
            return Ok(NativeAnswer::NoModel);
        }
        let answered = match (mode, self.flaw) {
            (Mode::Cautious, Flaw::MisanswersTheNativeCautious) => Mode::Brave,
            _ => mode,
        };
        // Answering over no model anyway, the flawed door answers the empty set.
        Ok(NativeAnswer::Closed(
            Consequences::fold(answered, &sets)
                .map(|consequences| consequences.as_set().clone())
                .unwrap_or_default(),
        ))
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

/// The reason the report skipped `check`, if it did.
fn skip(report: &ConformanceReport, check: Check) -> Option<&Skip> {
    match report.verdict(check) {
        Some(Verdict::Skipped(skip)) => Some(skip),
        _ => None,
    }
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
    }
}

#[test]
fn every_capability_declared_alone_passes_its_own_check() {
    for capability in REALISABLE {
        let report = report(only(capability), Flaw::Faithful);
        assert_eq!(
            report.verdict(Check::Capability(capability)),
            Some(&Verdict::Passed),
            "{capability}: {report}",
        );
    }
}

#[test]
fn a_backend_grounding_lazily_still_locates_its_refusal() {
    // An engine that grounds as it searches refuses the program no grounder
    // can instantiate at the solve, or at the first model, not at lowering.
    for grounding in [GroundsAt::TheSolve, GroundsAt::TheFirstModel] {
        let report =
            conformance::run(&mut Stub::new(enumerating(), Flaw::Faithful).grounding_at(grounding));
        assert_eq!(
            report.verdict(Check::ProgramFaultIsLocated),
            Some(&Verdict::Passed),
            "{grounding:?}: {report}",
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
    assert_eq!(
        skip(&report, Check::CancellationIsNotExhaustion),
        Some(&Skip::Reserved),
        "{report}"
    );
}

#[test]
fn an_unexposed_ground_program_skips_its_faithfulness() {
    let report = report(enumerating(), Flaw::Faithful);
    assert_eq!(
        skip(&report, Check::GroundProgramIsFaithful),
        Some(&Skip::NoGroundProgram),
        "{report}"
    );
}

#[test]
fn the_non_external_refusal_binds_only_a_multi_shot_backend() {
    let report = report(enumerating(), Flaw::Faithful);
    assert_eq!(
        skip(&report, Check::NonExternalAssignmentRefuses),
        Some(&Skip::Undeclared(Capability::MultiShot)),
        "{report}"
    );
}

#[test]
fn an_undeclared_externals_bit_binds_nothing() {
    let report = report(only(Capability::MultiShot), Flaw::Faithful);
    assert_eq!(
        skip(&report, Check::Capability(Capability::Externals)),
        Some(&Skip::Undeclared(Capability::Externals)),
        "{report}"
    );
}

#[test]
fn a_stream_cut_short_leaves_exhaustion_undriven() {
    // A stream that runs past its bound, or faults, has concluded nothing the
    // exhaustion check could judge; outcome correctness fails it instead.
    for flaw in [Flaw::NeverEnds, Flaw::FaultsMidStream] {
        let report = report(enumerating(), flaw);
        assert!(
            matches!(
                skip(&report, Check::ExhaustionIsEarned),
                Some(Skip::Undriven(_))
            ),
            "{flaw:?}: {report}"
        );
    }
}

#[test]
fn an_undriven_check_carries_the_backend_s_refusal() {
    // A refused program fails outcome correctness; the checks it leaves
    // undriven are skipped with the refusal, the backend's fault and all.
    let report = report(enumerating(), Flaw::RefusesEveryProgram);
    let Some(Skip::Undriven(failure)) = skip(&report, Check::ExhaustionIsEarned) else {
        panic!("the refused program leaves exhaustion undriven: {report}");
    };
    assert_eq!(failure.fault().map(Fault::locus), Some(Locus::Engine));
}

// ---- A broken backend fails exactly the checks that name its break ----

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one table: each flaw beside the declaration it runs under and the exact checks it \
              fails; splitting it would scatter the one universal the law states"
)]
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
            Flaw::NeverEnds,
            realising(),
            vec![
                (Outcome, Misanswered),
                (Declared(C::Assumptions), Misanswered),
                (Declared(C::MultiShot), Misanswered),
                (Declared(C::Externals), Misanswered),
            ],
        ),
        (
            Flaw::FaultsMidStream,
            enumerating(),
            vec![(Outcome, Refused)],
        ),
        (
            // Every probe that reads a stream finds it faulting; the budget's
            // probe finds no solve that reads the fact, so it is undriven.
            Flaw::FaultsMidStream,
            realising(),
            vec![
                (Outcome, Refused),
                (Declared(C::Assumptions), Refused),
                (Declared(C::MultiShot), Refused),
                (Declared(C::Externals), Refused),
            ],
        ),
        (
            Flaw::FaultsBeforeAModel,
            enumerating(),
            vec![(Outcome, Refused)],
        ),
        (
            Flaw::YieldsPastItsEnd,
            enumerating(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::LeavesItsSearchOpen,
            deciding(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::RangesAPlainSolveOverAScenario,
            enumerating(),
            vec![(Outcome, Misanswered)],
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
            // Its stream is wrong, and the answer set it corrupted went unseen.
            Flaw::YieldsAnInconsistentModel,
            enumerating(),
            vec![(Outcome, Misanswered), (Earned, Misanswered)],
        ),
        (
            Flaw::OptimizesTheSolve,
            enumerating(),
            vec![(Outcome, Misanswered), (Earned, Misanswered)],
        ),
        (
            Flaw::SolvesByCompletion,
            enumerating(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::SolvesByCompletion,
            deciding(),
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
            Flaw::RefusesTheExternalProgram,
            realising(),
            vec![(Declared(C::Externals), Refused)],
        ),
        (
            Flaw::RefusesTheRule,
            realising(),
            vec![(Declared(C::MultiShot), Refused)],
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
            Flaw::LocatesTheFaultElsewhere,
            enumerating(),
            vec![(Located, Mislocated)],
        ),
        (
            Flaw::LocatesTheFaultInAnotherSource,
            enumerating(),
            vec![(Located, Mislocated)],
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
            Flaw::DropsAnUnderivableAssumption,
            realising(),
            vec![
                (Declared(C::Assumptions), Misanswered),
                (Declared(C::NativeConsequences), Misanswered),
            ],
        ),
        (
            Flaw::LeaksTheScenario,
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
            Flaw::AnswersTheConsequencesOfNoModel,
            only(Capability::NativeConsequences),
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
            Flaw::AnswersAnUnsatisfiableScenario,
            realising(),
            vec![(Declared(C::NativeConsequences), Misanswered)],
        ),
        (
            Flaw::StopsTheNativeSearchShort,
            realising(),
            vec![(Declared(C::NativeConsequences), Misanswered)],
        ),
        (
            // Without assumptions no scoped probe runs, so the unscoped door's
            // own check is what catches the stop.
            Flaw::StopsTheNativeSearchShort,
            only(Capability::NativeConsequences),
            vec![(Declared(C::NativeConsequences), Misanswered)],
        ),
        (
            Flaw::RefusesTheReset,
            realising(),
            vec![(Outcome, Refused), (Declared(C::MultiShot), Refused)],
        ),
        (
            Flaw::ReplacesWhatItLowers,
            realising(),
            vec![(Declared(C::MultiShot), Misanswered)],
        ),
        (
            // Every program after the first piles onto it, so every check that
            // reloads fails with outcome correctness.
            Flaw::ResetsNothing,
            realising(),
            vec![
                (Outcome, Refused),
                (Declared(C::NativeConsequences), Refused),
                (Declared(C::Assumptions), Refused),
                (Declared(C::Externals), Refused),
            ],
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
            Flaw::AnswersAnUndeclaredTimedScopedSolve,
            only(Capability::Assumptions),
            vec![(Declared(C::TimeBudget), Accepted)],
        ),
        (
            Flaw::MisanswersATimedSolve,
            only(Capability::TimeBudget),
            vec![(Declared(C::TimeBudget), Misanswered)],
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
    let mismatched: Vec<String> = table
        .into_iter()
        .filter_map(|(flaw, declared, expected)| {
            let report = report(declared, flaw);
            let expected: HashSet<(Check, Breach)> = expected.into_iter().collect();
            let failed = failures(&report);
            (failed != expected)
                .then(|| format!("{flaw:?}: expected {expected:?}, failed {failed:?}\n{report}"))
        })
        .collect();
    assert!(mismatched.is_empty(), "{}", mismatched.join("\n"));
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
fn an_inconsistent_model_fails_its_outcome_as_one() {
    let report = report(enumerating(), Flaw::YieldsAnInconsistentModel);
    let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
        panic!("the inconsistent model fails its outcome: {report}");
    };
    assert_eq!(failure.case(), Some("classical negation"));
    assert!(failure.to_string().contains("contrary"), "{failure}");
}

#[test]
fn a_solve_under_the_objective_fails_on_the_program_under_one() {
    let report = report(enumerating(), Flaw::OptimizesTheSolve);
    let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
        panic!("the optimising solve fails its outcome: {report}");
    };
    assert_eq!(failure.case(), Some("a choice under an objective"));
}

#[test]
fn a_faulted_stream_s_failure_carries_the_fault() {
    // Whether the engine dies before the first model or after it.
    for flaw in [Flaw::FaultsBeforeAModel, Flaw::FaultsMidStream] {
        let report = report(enumerating(), flaw);
        let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
            panic!("{flaw:?}: the faulted stream fails its outcome: {report}");
        };
        assert_eq!(
            failure.fault().map(Fault::locus),
            Some(Locus::Engine),
            "{flaw:?}"
        );
    }
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
