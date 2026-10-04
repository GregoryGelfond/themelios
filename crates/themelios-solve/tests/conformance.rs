//! The conformance suite over engine-free stub backends (docs/design/solve.md
//! §13.1): a backend answering the suite's corpus from a table of its known
//! answer sets — streamed through the real `Backend`/`Run`/`Solved::running`
//! door — meets every obligation, and a backend broken in one way fails exactly
//! the checks that name that way, each with its typed breach, so the suite has
//! teeth. The authoritative runs are a real engine's; these run the suite's own
//! machinery through the contract.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use themelios_base::span::{ByteOffset, Location, Span};
use themelios_program::program::Part;
use themelios_program::raise::raise_str;
use themelios_program::{
    Dialect, Name, Origin, Program, Provenance, Sign, SourceId, Statement, Symbol, WithProvenance,
};
use themelios_solve::agent::{Assumption, Scenario};
use themelios_solve::bridge::{Door, GroundProgram, NotAdmitted};
use themelios_solve::conformance::{self, Breach, Check, ConformanceReport, Skip, Verdict};
use themelios_solve::contract::{
    Backend, Cancel, Capabilities, Capability, ConsequenceRequest, ConsequenceSupport, Fault,
    GroundOptions, Locus, Mode, Presupposition, SolveRequest, TruthValue,
};
use themelios_solve::extend::{Function, Propagator};
use themelios_solve::outcome::{
    AnswerSet, Conclusion, Consequences, Model, NativeAnswer, Run, ShowRule, Solved, Truncation,
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

/// A fact other than `a.`, then a fact over a variable nothing binds: refused
/// at lowering, after its prefix `b.`.
const PREFIXED_UNSAFE: &str = "b. p(X).";

/// The fact a rebuild replaces `a.` with.
const REBUILT: &str = "b.";

/// The fact over a variable nothing binds, alone: the statement a refusal of
/// the unsafe programs names.
const UNSAFE_FACT: &str = "p(X).";

/// A choice over forty atoms: 2^40 answer sets, more than any search
/// enumerates within a budget — the program the time budget's cut is asked of.
const UNBOUNDED: &str = "{ a(1..40) }.";

/// How the stub answers a program.
enum Answers {
    /// These answer sets, known independently of any engine.
    Known(Vec<AnswerSet>),
    /// These answer sets, each model displaying these terms — the half of the
    /// display only an engine evaluates (docs/design/solve.md §5.1).
    Displaying(Vec<AnswerSet>, Vec<Symbol>),
    /// `{a, b}` while the external `a` is assigned true, `{}` otherwise.
    External,
    /// None: no grounder can instantiate the program, so lowering it is
    /// refused — after `prefix`, the source of the statements before the one
    /// refused, which a stub keeping a refused prefix keeps.
    Unsafe { prefix: &'static str },
    /// `{p(r) | r}` for the results `r` of the latest registered `@`-function
    /// called on these numbers; the call's fault, or no function to call,
    /// fails the grounding.
    Calls(Vec<i32>),
    /// Past counting — 2^40 answer sets — of which a search the stub cuts at
    /// its budget yields the first few.
    Unbounded,
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
        (
            "a. {b}. c. #project c/0.",
            known(vec![
                set([constant("a"), constant("c")]),
                set([constant("a"), constant("b"), constant("c")]),
            ]),
        ),
        ("1 { #true; #true } 1.", known(vec![])),
        ("{ #true : p(1;1) } = 2. p(1).", known(vec![set([p(1)])])),
        (
            "x :- #count{ 1 : a; 1 : b } = 1. a. b.",
            known(vec![set([constant("a"), constant("b"), constant("x")])]),
        ),
        ("a. #show.", known(vec![set([constant("a")])])),
        ("{a}. #show.", known(vec![set([]), set([constant("a")])])),
        (
            "q. #show p : q.",
            Answers::Displaying(vec![set([constant("q")])], vec![constant("p")]),
        ),
        (
            "q. #show. #show p : q.",
            Answers::Displaying(vec![set([constant("q")])], vec![constant("p")]),
        ),
        (
            "-p. #show q/0.",
            known(vec![set([atom("p", [], Sign::Negative)])]),
        ),
        (EXTERNAL, Answers::External),
        (UNSAFE, Answers::Unsafe { prefix: "a." }),
        (PREFIXED_UNSAFE, Answers::Unsafe { prefix: REBUILT }),
        (REBUILT, known(vec![set([constant("b")])])),
        ("p(@fault).", Answers::Calls(Vec::new())),
        ("p(@echo(1)).", Answers::Calls(vec![1])),
        (UNBOUNDED, Answers::Unbounded),
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

/// The offset, within the unsafe program's text, of its atom over a variable
/// nothing binds.
fn unsafe_offset() -> ByteOffset {
    let at = UNSAFE
        .find("p(X)")
        .expect("the unsafe program holds its unsafe atom");
    ByteOffset::new(u32::try_from(at).expect("a short text"))
}

/// The statement of `program` that is — or, with `unsafe_one` false, is not —
/// the fact over a variable nothing binds, found by its content, so a program
/// built in Rust is read as one raised from text is.
fn statement_of(program: &Program, unsafe_one: bool) -> &WithProvenance<Statement> {
    let unsafe_fact = program_text_statement(UNSAFE_FACT);
    program
        .statements()
        .find(|node| (node.get() == &unsafe_fact) == unsafe_one)
        .expect("each unsafe program holds its unsafe fact and a statement beside it")
}

/// The one statement `source` raises to.
fn program_text_statement(source: &str) -> Statement {
    program(source)
        .statements()
        .next()
        .expect("the source raises to a statement")
        .get()
        .clone()
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

/// The same refusal of the program's other statement — as an adapter mapping
/// its engine's error to the wrong statement raises it.
fn mislocated_refusal(program: &Program) -> Fault {
    refusal_of(statement_of(program, false))
}

/// The same refusal of the unsafe statement, placed at its offsets in a source
/// the program is not — as an adapter reporting every location in its own text
/// raises it, a statement built in Rust included.
fn refusal_in_another_source(program: &Program) -> Fault {
    let statement = statement_of(program, true);
    let span = statement
        .provenance()
        .origins()
        .find_map(|origin| match origin {
            Origin::Parsed(location) => Some(location.span),
            _ => None,
        })
        .unwrap_or_else(|| {
            Span::new(unsafe_offset(), unsafe_offset()).expect("an empty span is ordered")
        });
    let elsewhere = Origin::Parsed(Location {
        source: SourceId::new(ANOTHER_SOURCE),
        span,
    });
    refusal_of(&WithProvenance::new(
        statement.get().clone(),
        Provenance::from(elsewhere),
    ))
}

/// The refusal of a statement built in Rust at a location fabricated for it —
/// in the unsafe program's own source, at its unsafe offset — as an adapter
/// that invents a span raises it; a parsed statement is refused faithfully.
fn fabricated_refusal(program: &Program) -> Fault {
    let statement = statement_of(program, true);
    let parsed = statement
        .provenance()
        .origins()
        .any(|origin| matches!(origin, Origin::Parsed(_)));
    if parsed {
        return refusal_of(statement);
    }
    let invented = Origin::Parsed(Location {
        source: SourceId::new(ANOTHER_SOURCE),
        span: Span::new(unsafe_offset(), unsafe_offset()).expect("an empty span is ordered"),
    });
    refusal_of(&WithProvenance::new(
        statement.get().clone(),
        Provenance::from(invented),
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

/// A scripted enumeration: the answer sets, each model displaying `terms`,
/// then the search's end, concluded as `terminal` — unless it `concludes`
/// nothing, its search left open.
struct Enumeration {
    sets: std::vec::IntoIter<AnswerSet>,
    terms: Vec<Symbol>,
    terminal: Conclusion,
    concludes: bool,
    ended: bool,
}

impl Run for Enumeration {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        let next = self
            .sets
            .next()
            .map(|set| Ok(Model::of(set).with_terms(self.terms.iter().cloned())));
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

/// Past this many models, a search past its budget fails the test outright: the
/// suite reads at most its cut's cap of models and one more, so a read this far
/// is the suite holding on past its bound.
const PAST_ITS_BUDGET_TRIPWIRE: usize = 1 << 21;

/// A search that runs past its budget, yielding the empty set forever.
struct PastItsBudget {
    yielded: usize,
}

impl Run for PastItsBudget {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        self.yielded += 1;
        assert!(
            self.yielded <= PAST_ITS_BUDGET_TRIPWIRE,
            "the suite read {PAST_ITS_BUDGET_TRIPWIRE} models of a search past its budget: its cap failed"
        );
        Some(Ok(Model::of(AnswerSet::new())))
    }

    fn conclusion(&self) -> Option<Conclusion> {
        None
    }
}

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
/// that is not fused — each model displaying `terms`.
struct Unfused {
    sets: std::vec::IntoIter<AnswerSet>,
    again: Option<AnswerSet>,
    terms: Vec<Symbol>,
    ended: bool,
}

impl Run for Unfused {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        let model = |set| Ok(Model::of(set).with_terms(self.terms.iter().cloned()));
        if self.ended {
            return self.again.take().map(model);
        }
        let next = self.sets.next().map(model);
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
    /// Drops one answer set of every program that has several when it was
    /// lowered through Door A, and still concludes that the search closed the
    /// space.
    DropsAnAnswerSetAtDoorA,
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
    /// Declares the observer, yet exposes an empty ground program for every
    /// program.
    ExposesAnEmptyGroundProgram,
    /// Exposes a ground program without declaring the observer.
    ExposesAnUndeclaredObserver,
    /// Carries each model's display as its answer set.
    ReadsTheDisplay,
    /// Refuses every parse at Door A.
    RefusesDoorA,
    /// Omits the terms the program's directives display.
    ForgetsATerm,
    /// Adds the number 1 to its first model's answer set.
    YieldsANumber,
    /// Refuses the unsafe program as a program fault naming no statement.
    NamesNoStatement,
    /// Refuses the unsafe program built in Rust at a location it fabricates.
    LocatesABuiltStatement,
    /// Keeps the program a rebuild replaced beside the program rebuilt.
    KeepsTheReplacedProgram,
    /// Keeps a refused program's prefix, the statements before the one its
    /// lowering refuses.
    AddsOnARefusedLower,
    /// Answers after a failed grounding, as though it needed no rebuild.
    ForgetsItNeedsARebuild,
    /// Refuses the next method after a grounding failed within its
    /// single-shot solve, as though it needed a rebuild it has none of.
    RefusesAfterAFailedSolve,
    /// Forgets its registered functions at a reset.
    LosesRegistrationsOnReset,
    /// Refuses an undeclared function's registration without naming the
    /// capability.
    RefusesUnsupportedVaguely,
    /// Accepts a multi-shot grounding whose `@`-function faults.
    AcceptsAFaultingGrounding,
    /// Answers a single-shot solve whose grounding's `@`-function faults.
    AnswersAFaultingSolve,
    /// Carries a cancellation pulled during one run to the next.
    CarriesACancellation,
    /// Exposes a ground program while needing its rebuild.
    ObservesWhileNeedingARebuild,
    /// Refuses while needing its rebuild, naming another presupposition.
    MisnamesItsRebuild,
    /// Declares the native door, yet its door skips the refusal a needed
    /// rebuild owes.
    ForgetsItsRebuildAtTheNativeDoor,
    /// Declares functions, yet refuses every registration at the engine.
    RefusesDeclaredFunctions,
    /// Refuses the reset that rebuilds it after a failed grounding.
    RefusesTheRebuildingReset,
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
    /// Declares the native door, yet answers the brave consequences for the
    /// cautious of a program lowered through Door A.
    MisanswersTheNativeCautiousAtDoorA,
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
    /// Declares multi-shot solving, yet refuses to ground a program that
    /// calls no `@`-function.
    RefusesACallFreeGrounding,
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
    /// Declares a time budget and cuts a search at it, yet concludes that the
    /// search closed the space.
    ConcludesItsCutAsExhaustion,
    /// Declares a time budget, yet searches past it, never cutting the search.
    IgnoresTheBudget,
    /// Declares a time budget, yet faults the search it cuts.
    FaultsTheCutSearch,
    /// Refuses to lower the choice over forty atoms.
    RefusesTheUnboundedProgram,
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

/// A cancellation primitive that cuts nothing short now and stops the next run
/// instead — a pull carried past the run it was pulled during.
struct Carried(Arc<AtomicBool>);

impl Cancel for Carried {
    fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
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
    /// Whether a grounding failed since the last reset — the one state that
    /// changes what every method does (docs/design/solve.md §4.1).
    needs_rebuild: bool,
    /// The programs the last reset or replacing lower cleared, which a stub
    /// keeping the replaced program restores.
    cleared: Vec<usize>,
    /// The `@`-functions registered, kept across a reset and a replacing
    /// lower; an `@`-call reaches the latest, the stub having no names to
    /// dispatch by.
    functions: Vec<Box<dyn Function>>,
    /// Whether a grounding failed within a single-shot solve, under the flaw
    /// that then refuses the next method.
    failed_solve: bool,
    /// A cancellation pulled and carried to the next run, under the flaw that
    /// carries it.
    carried: Arc<AtomicBool>,
    /// The last scoped solve's scenario, which a leaking stub keeps.
    leaked: Scenario,
    /// The door the last program lowered came through.
    through: Through,
}

/// The door a program came through (docs/design/solve.md §10.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Through {
    /// Door A: a parse, admitted.
    Parsed,
    /// Door B: a program.
    Program,
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
            needs_rebuild: false,
            cleared: Vec::new(),
            functions: Vec::new(),
            failed_solve: false,
            carried: Arc::new(AtomicBool::new(false)),
            leaked: Scenario::default(),
            through: Through::Program,
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
            [index] => (self.table[index].0, self.known(index)?),
            [fact, rule] if self.table[fact].0 == "a." && self.table[rule].0 == RULE => {
                ("a. b :- a.", vec![set([constant("a"), constant("b")])])
            }
            // A flawed stub keeps one program beside another: facts, so their
            // answer sets unite.
            [first, second]
                if matches!(
                    self.flaw,
                    Flaw::AddsOnARefusedLower | Flaw::KeepsTheReplacedProgram
                ) =>
            {
                let mut united = AnswerSet::new();
                for index in [first, second] {
                    for set in self.known(index)? {
                        united.extend(set);
                    }
                }
                ("", vec![united])
            }
            [] => return Err(Fault::engine("no program is loaded")),
            _ => return Err(Fault::engine("several programs have accumulated")),
        };
        match self.flaw {
            Flaw::DropsAnAnswerSet if sets.len() > 1 => {
                sets.pop();
            }
            Flaw::DropsAnAnswerSetAtDoorA if self.through == Through::Parsed && sets.len() > 1 => {
                sets.pop();
            }
            Flaw::SolvesByCompletion if source == POSITIVE_LOOP || source == CONSTRAINED_LOOP => {
                sets.push(set([constant("a"), constant("b")]));
            }
            Flaw::ShiftsTheHeadCycle if source == HEAD_CYCLE => sets.clear(),
            Flaw::OptimizesTheSolve if source == OBJECTIVE => sets.retain(AnswerSet::is_empty),
            Flaw::YieldsANumber => {
                if let Some(first) = sets.first_mut() {
                    first.insert(Symbol::number(1));
                }
            }
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

    /// Hold the table's program at `index` as lowered: beside what a
    /// multi-shot stub accumulated, or in place of what a single-shot one held,
    /// recorded as cleared.
    fn keep(&mut self, index: usize) {
        if self.capabilities.multi_shot && self.flaw != Flaw::ReplacesWhatItLowers {
            self.loaded.push(index);
        } else {
            self.cleared = std::mem::replace(&mut self.loaded, vec![index]);
        }
    }

    /// The refusal a stub needing its rebuild owes at every method that
    /// touches its program or runs a search — unless it forgets the state. A
    /// stub refusing after a failed single-shot solve refuses the next such
    /// method too, then recovers.
    fn rebuild_owed(&mut self) -> Result<(), Fault> {
        let owed = (self.needs_rebuild && self.flaw != Flaw::ForgetsItNeedsARebuild)
            || std::mem::take(&mut self.failed_solve);
        if owed && self.flaw == Flaw::MisnamesItsRebuild {
            return Err(Fault::request(
                "the stub needs a rebuild",
                Presupposition::NotLive,
            ));
        }
        if owed {
            Err(Fault::request(
                "the stub needs a rebuild",
                Presupposition::NeedsRebuild,
            ))
        } else {
            Ok(())
        }
    }

    /// The answer set of the table's call program at `index`: the latest
    /// registered function called on `arguments`, each result `r` a fact
    /// `p(r)`; its fault, or no function to call, refuses the call's statement.
    fn called(&self, index: usize, arguments: &[i32]) -> Result<AnswerSet, Fault> {
        // A stub forgetting its failed grounding answers as though it held none.
        if self.needs_rebuild && self.flaw == Flaw::ForgetsItNeedsARebuild {
            return Ok(AnswerSet::new());
        }
        let statement = self.table[index]
            .1
            .statements()
            .next()
            .expect("a call program holds its call")
            .clone();
        let function = self
            .functions
            .last()
            .ok_or_else(|| Fault::program("no @-function is registered to call", &statement))?;
        let arguments: Vec<Symbol> = arguments.iter().copied().map(Symbol::number).collect();
        let results = function
            .call(&arguments)
            .map_err(|fault| Fault::program(fault.to_string(), &statement))?;
        Ok(results
            .into_iter()
            .map(|result| atom("p", [result], Sign::Positive))
            .collect())
    }

    /// The call programs loaded, grounded: the first call's refusal, if any.
    fn ground_calls(&self) -> Result<(), Fault> {
        for &index in &self.loaded {
            if let Answers::Calls(arguments) = &self.table[index].2 {
                self.called(index, arguments)?;
            }
        }
        Ok(())
    }

    /// The terms each model of the loaded program displays — none for a stub
    /// that forgets them.
    fn terms(&self) -> Vec<Symbol> {
        match self.loaded[..] {
            [index] if self.flaw != Flaw::ForgetsATerm => match &self.table[index].2 {
                Answers::Displaying(_, terms) => terms.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// The show rule of the `#show` directives the stub holds — every program
    /// lowered since its last reset (docs/design/solve.md §5.1).
    fn show_rule(&self) -> ShowRule {
        ShowRule::of(
            self.loaded
                .iter()
                .flat_map(|&index| self.table[index].1.statements())
                .filter_map(|node| match node.get() {
                    Statement::Show(show) => Some(show),
                    _ => None,
                }),
        )
    }

    /// The answer sets of the table's program at `index`.
    fn known(&self, index: usize) -> Result<Vec<AnswerSet>, Fault> {
        Ok(match &self.table[index].2 {
            Answers::Known(sets) | Answers::Displaying(sets, _) => sets.clone(),
            Answers::External => vec![if self.a_holds {
                set([constant("a"), constant("b")])
            } else {
                set([])
            }],
            Answers::Unsafe { .. } => Vec::new(),
            Answers::Calls(arguments) => vec![self.called(index, arguments)?],
            Answers::Unbounded => {
                let a = |n| atom("a", [Symbol::number(n)], Sign::Positive);
                vec![set([]), set([a(1)]), set([a(2)])]
            }
        })
    }

    /// The stub's search of the unbounded program, which it cuts at its budget:
    /// the first answer sets, then the budget — unless its flaw searches past
    /// the budget, faults the cut search, or concludes the cut as closing the
    /// space.
    fn cut(&self, sets: Vec<AnswerSet>, scenario: Scenario) -> Solved<'static> {
        let run: Box<dyn Run> = match self.flaw {
            Flaw::IgnoresTheBudget => Box::new(PastItsBudget { yielded: 0 }),
            Flaw::FaultsTheCutSearch => Box::new(Faulting {
                first: sets.into_iter().next(),
                fault: Some(Fault::engine("the stub's cut search faulted")),
            }),
            _ => Box::new(Enumeration {
                sets: sets.into_iter(),
                terms: Vec::new(),
                terminal: if self.flaw == Flaw::ConcludesItsCutAsExhaustion {
                    Conclusion::Exhausted
                } else {
                    Conclusion::Budget
                },
                concludes: true,
                ended: false,
            }),
        };
        Solved::running(run, scenario, self.show_rule())
    }

    /// The handle over `sets`, ranging over `scenario`: every set when the stub
    /// enumerates; otherwise the first, as a witness.
    fn enumerate(&self, mut sets: Vec<AnswerSet>, scenario: Scenario) -> Solved<'static> {
        let show = self.show_rule();
        let terms = self.terms();
        if self.flaw == Flaw::ReadsTheDisplay {
            sets = sets
                .into_iter()
                .map(|set| display_of(set, &terms, &show))
                .collect();
        }
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
                show.clone(),
            );
        }
        if self.flaw == Flaw::FaultsMidStream && !sets.is_empty() {
            return Solved::running(
                Box::new(Faulting {
                    first: Some(sets.swap_remove(0)),
                    fault: Some(Fault::engine("the stub's engine died mid-search")),
                }),
                scenario,
                show.clone(),
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
                show.clone(),
            );
        }
        if self.flaw == Flaw::YieldsPastItsEnd && !sets.is_empty() {
            return Solved::running(
                Box::new(Unfused {
                    again: sets.first().cloned(),
                    sets: sets.into_iter(),
                    terms,
                    ended: false,
                }),
                scenario,
                show.clone(),
            );
        }
        let concludes = sets.is_empty() || self.flaw != Flaw::LeavesItsSearchOpen;
        let terminal = if self.carried.swap(false, Ordering::SeqCst) {
            Conclusion::Interrupted
        } else if sets.is_empty() && self.flaw == Flaw::LeavesTheSearchUndecided {
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
                terms: if self.flaw == Flaw::ReadsTheDisplay {
                    Vec::new()
                } else {
                    terms
                },
                terminal,
                concludes,
                ended: false,
            }),
            scenario,
            show.clone(),
        )
    }
}

/// What `set`, displaying `terms`, displays under `show` — read through the
/// core's own derivation, as a stub handing back its engine's display would
/// read it.
fn display_of(set: AnswerSet, terms: &[Symbol], show: &ShowRule) -> AnswerSet {
    let run = Enumeration {
        sets: vec![set].into_iter(),
        terms: terms.to_vec(),
        terminal: Conclusion::Exhausted,
        concludes: true,
        ended: false,
    };
    let mut solved = Solved::running(Box::new(run), Scenario::default(), show.clone());
    solved
        .models()
        .next()
        .and_then(Result::ok)
        .map(|model| model.shown().symbols().clone())
        .unwrap_or_default()
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
        self.rebuild_owed()?;
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
                        ShowRule::default(),
                    ));
                }
                GroundsAt::Lowering => {}
            }
        }
        if request.time.is_some() && self.flaw == Flaw::MisanswersATimedSolve {
            return Ok(self.enumerate(vec![set([constant("stranger")])], Scenario::default()));
        }
        // A single-shot stub grounds within its solve: a failed grounding fails
        // the solve and leaves the stub ready, unless its flaw then refuses
        // everything.
        let mut sets = match self.answer_sets() {
            Ok(sets) => sets,
            Err(_) if self.flaw == Flaw::AnswersAFaultingSolve => vec![AnswerSet::new()],
            Err(fault) => {
                if self.flaw == Flaw::RefusesAfterAFailedSolve {
                    self.failed_solve = true;
                }
                return Err(fault);
            }
        };
        if self.flaw == Flaw::LeaksTheScenario {
            sets.retain(|set| admits(&self.leaked, set));
        }
        let ranged = if self.flaw == Flaw::RangesAPlainSolveOverAScenario {
            fixing_a(true)
        } else {
            Scenario::default()
        };
        if self.loaded_source() == Some(UNBOUNDED) {
            return Ok(self.cut(sets, ranged));
        }
        Ok(self.enumerate(sets, ranged))
    }

    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
        self.rebuild_owed()?;
        let lowered = door.program();
        if self.flaw == Flaw::RefusesDoorA && matches!(door, Door::Parsed(_)) {
            return Err(Fault::engine("the stub lowers Door B alone"));
        }
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
        if self.flaw == Flaw::RefusesTheUnboundedProgram && self.table[index].0 == UNBOUNDED {
            return Err(Fault::engine("the stub lowers no choice over forty atoms"));
        }
        let mut pending = None;
        if let Answers::Unsafe { prefix } = self.table[index].2 {
            match self.flaw {
                Flaw::AcceptsTheUnsafeProgram => {}
                Flaw::RefusesTheUnsafeProgramOffItsLocus => {
                    return Err(Fault::engine("a variable nothing binds"));
                }
                Flaw::NamesNoStatement => {
                    return Err(Fault::from(NotAdmitted::Lowering(Box::new([]))));
                }
                Flaw::LocatesTheFaultElsewhere => return Err(mislocated_refusal(lowered)),
                Flaw::LocatesTheFaultInAnotherSource => {
                    return Err(refusal_in_another_source(lowered));
                }
                Flaw::LocatesABuiltStatement => return Err(fabricated_refusal(lowered)),
                Flaw::AddsOnARefusedLower if self.grounds_at == GroundsAt::Lowering => {
                    let kept = self
                        .table
                        .iter()
                        .position(|(source, _, _)| *source == prefix)
                        .expect("a refused program's prefix is in the table");
                    self.keep(kept);
                    return Err(located_refusal(lowered));
                }
                _ if self.grounds_at == GroundsAt::Lowering => {
                    return Err(located_refusal(lowered));
                }
                _ => pending = Some(located_refusal(lowered)),
            }
        }
        self.keep(index);
        self.through = if matches!(door, Door::Parsed(_)) {
            Through::Parsed
        } else {
            Through::Program
        };
        if self.flaw == Flaw::KeepsTheReplacedProgram && self.table[index].0 == REBUILT {
            let kept = std::mem::take(&mut self.cleared);
            self.loaded.splice(0..0, kept);
        }
        self.pending_refusal = pending;
        Ok(())
    }

    fn ground_program(&self) -> Option<&GroundProgram> {
        let exposes = matches!(
            self.flaw,
            Flaw::ExposesAnEmptyGroundProgram | Flaw::ExposesAnUndeclaredObserver
        ) || (self.flaw == Flaw::ObservesWhileNeedingARebuild && self.needs_rebuild);
        exposes.then_some(&self.nothing)
    }

    fn interrupt(&self) -> Option<Box<dyn Cancel>> {
        let offered = match self.flaw {
            Flaw::WithholdsTheInterruptHandle => false,
            Flaw::OffersAnUndeclaredInterrupt => true,
            _ => self.capabilities.cancellation,
        };
        offered.then(|| {
            if self.flaw == Flaw::CarriesACancellation {
                Box::new(Carried(Arc::clone(&self.carried))) as Box<dyn Cancel>
            } else {
                Box::new(Unheeded) as Box<dyn Cancel>
            }
        })
    }

    fn solve_assuming(
        &mut self,
        scenario: &Scenario,
        request: &SolveRequest,
    ) -> Result<Solved<'_>, Fault> {
        self.rebuild_owed()?;
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
        if self.loaded_source() == Some(UNBOUNDED) {
            return Ok(self.cut(admitted, ranged));
        }
        Ok(self.enumerate(admitted, ranged))
    }

    fn ground(&mut self, _parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        self.rebuild_owed()?;
        if !self.capabilities.multi_shot && self.flaw != Flaw::AnswersAnUndeclaredGround {
            return Err(Fault::unsupported(Capability::MultiShot));
        }
        if self.flaw == Flaw::AcceptsAFaultingGrounding {
            return Ok(());
        }
        if self.flaw == Flaw::RefusesACallFreeGrounding
            && !self
                .loaded
                .iter()
                .any(|&index| matches!(self.table[index].2, Answers::Calls(_)))
        {
            return Err(Fault::engine(
                "the stub grounds only a program that calls an @-function",
            ));
        }
        // A grounding that fails is never accepted: it leaves the stub needing
        // its rebuild.
        self.ground_calls()
            .inspect_err(|_| self.needs_rebuild = true)
    }

    fn assign_external(&mut self, external: Symbol, value: TruthValue) -> Result<(), Fault> {
        self.rebuild_owed()?;
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
        if self.flaw == Flaw::RefusesTheRebuildingReset && self.needs_rebuild {
            // Its state forgotten with the refusal, so later loads proceed.
            self.needs_rebuild = false;
            return Err(Fault::engine(
                "the stub cannot reset after a failed grounding",
            ));
        }
        match (self.capabilities.multi_shot, self.flaw) {
            (true, Flaw::RefusesTheReset) => Err(Fault::engine("the stub cannot reset")),
            // It keeps what was lowered; the rebuild state is no program.
            (true, Flaw::ResetsNothing) => {
                self.needs_rebuild = false;
                Ok(())
            }
            (false, Flaw::AnswersAnUndeclaredReset) | (true, _) => {
                self.cleared = std::mem::take(&mut self.loaded);
                self.a_holds = false;
                self.pending_refusal = None;
                self.needs_rebuild = false;
                if self.flaw == Flaw::LosesRegistrationsOnReset {
                    self.functions.clear();
                }
                Ok(())
            }
            (false, _) => Err(Fault::unsupported(Capability::MultiShot)),
        }
    }

    fn register_function(&mut self, function: Box<dyn Function>) -> Result<(), Fault> {
        self.rebuild_owed()?;
        if self.flaw == Flaw::RefusesDeclaredFunctions {
            return Err(Fault::engine("the stub cannot register functions"));
        }
        if self.capabilities.functions || self.flaw == Flaw::AnswersUndeclaredFunctions {
            self.functions.push(function);
            Ok(())
        } else if self.flaw == Flaw::RefusesUnsupportedVaguely {
            Err(Fault::request(
                "the stub cannot register functions",
                Presupposition::NotLive,
            ))
        } else {
            Err(Fault::unsupported(Capability::Functions))
        }
    }

    fn register_propagator(&mut self, _propagator: Box<dyn Propagator>) -> Result<(), Fault> {
        self.rebuild_owed()?;
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
        if self.flaw != Flaw::ForgetsItsRebuildAtTheNativeDoor {
            self.rebuild_owed()?;
        }
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
            (Mode::Cautious, Flaw::MisanswersTheNativeCautiousAtDoorA)
                if self.through == Through::Parsed =>
            {
                Mode::Brave
            }
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
/// whose outcome has no public constructor, and the ground-program observer,
/// whose carrier has none yet.
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

/// The declaration of an enumerating backend that declares the ground-program
/// observer.
fn observing() -> Capabilities {
    let mut capabilities = enumerating();
    capabilities.ground_program = true;
    capabilities
}

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
fn a_multi_shot_backend_with_functions_alone_conforms() {
    // Its failed grounding is probed at the methods it declares, and only those.
    let mut capabilities = only(Capability::MultiShot);
    capabilities.functions = true;
    let report = report(capabilities, Flaw::Faithful);
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
            report.verdict(Check::FaultLoci),
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
fn an_undeclared_observer_skips_its_faithfulness() {
    let report = report(enumerating(), Flaw::Faithful);
    assert_eq!(
        skip(&report, Check::GroundProgramIsFaithful),
        Some(&Skip::Undeclared(Capability::GroundProgram)),
        "{report}"
    );
}

#[test]
fn the_non_external_refusal_binds_only_a_multi_shot_backend() {
    // Without multi-shot solving there is no assignment to hold to its locus,
    // so a stub that would accept a non-external atom still keeps fault loci.
    let report = report(enumerating(), Flaw::AcceptsANonExternal);
    assert_eq!(
        report.verdict(Check::FaultLoci),
        Some(&Verdict::Passed),
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
        BackendState as State, Capability as Declared, Display as Displayed,
        ExhaustionIsEarned as Earned, FaultLoci as Faults, GroundProgramIsFaithful as Ground,
        OutcomeCorrectness as Outcome, RebuildLeavesNothingBehind as Rebuild,
    };
    let table: Vec<Expectation> = vec![
        (
            Flaw::DropsAnAnswerSet,
            enumerating(),
            vec![(Outcome, Misanswered), (Earned, Misanswered)],
        ),
        (
            // Through Door B the stub is faithful; through Door A, every check
            // that reads the corpus through both doors finds the dropped set.
            Flaw::DropsAnAnswerSetAtDoorA,
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
            vec![(Outcome, Refused), (Faults, Mislocated)],
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
            observing(),
            vec![(Ground, Misanswered)],
        ),
        (
            Flaw::ExposesAnUndeclaredObserver,
            enumerating(),
            vec![(Declared(C::GroundProgram), Accepted)],
        ),
        // A deciding stub yields a witness alone, so a wrong answer set is
        // outcome correctness's failure and no unseen set's.
        (
            Flaw::ReadsTheDisplay,
            deciding(),
            vec![(Outcome, Misanswered)],
        ),
        (Flaw::RefusesDoorA, enumerating(), vec![(Outcome, Refused)]),
        (
            Flaw::ForgetsATerm,
            enumerating(),
            vec![(Displayed, Misanswered)],
        ),
        (
            Flaw::YieldsANumber,
            deciding(),
            vec![(Outcome, Misanswered)],
        ),
        (
            Flaw::NamesNoStatement,
            enumerating(),
            vec![(Faults, Mislocated)],
        ),
        (
            Flaw::LocatesABuiltStatement,
            enumerating(),
            vec![(Faults, Mislocated)],
        ),
        (
            Flaw::KeepsTheReplacedProgram,
            enumerating(),
            vec![(Rebuild, Misanswered)],
        ),
        (
            Flaw::KeepsTheReplacedProgram,
            realising(),
            vec![(Rebuild, Misanswered)],
        ),
        (
            Flaw::AddsOnARefusedLower,
            only(Capability::Functions),
            vec![(State, Misanswered)],
        ),
        (
            Flaw::AddsOnARefusedLower,
            realising(),
            vec![(State, Misanswered)],
        ),
        (
            Flaw::ForgetsItNeedsARebuild,
            realising(),
            vec![(State, Accepted)],
        ),
        (
            Flaw::RefusesAfterAFailedSolve,
            only(Capability::Functions),
            vec![(State, Refused)],
        ),
        (
            Flaw::LosesRegistrationsOnReset,
            realising(),
            vec![(State, Refused)],
        ),
        (
            Flaw::RefusesUnsupportedVaguely,
            enumerating(),
            vec![(Declared(C::Functions), Mislocated)],
        ),
        (
            Flaw::AcceptsAFaultingGrounding,
            realising(),
            vec![(State, Accepted)],
        ),
        (
            Flaw::AnswersAFaultingSolve,
            only(Capability::Functions),
            vec![(State, Accepted)],
        ),
        (
            Flaw::CarriesACancellation,
            realising(),
            vec![(Rebuild, Misanswered)],
        ),
        (
            Flaw::ObservesWhileNeedingARebuild,
            realising(),
            vec![(State, Misanswered)],
        ),
        (
            Flaw::MisnamesItsRebuild,
            realising(),
            vec![(State, Mislocated)],
        ),
        (
            // A check solves what it lowers, grounding nothing it does not
            // name, so only multi-shot honesty's grounding finds the refusal.
            Flaw::RefusesACallFreeGrounding,
            realising(),
            vec![(Declared(C::MultiShot), Refused)],
        ),
        (
            // Past the rebuild it owes, the native door reaches the faulting
            // call and refuses at its statement: a refusal, not the one owed.
            Flaw::ForgetsItsRebuildAtTheNativeDoor,
            realising(),
            vec![(State, Mislocated)],
        ),
        (
            Flaw::RefusesDeclaredFunctions,
            only(Capability::Functions),
            vec![(Declared(C::Functions), Refused)],
        ),
        (
            Flaw::RefusesTheRebuildingReset,
            realising(),
            vec![(State, Refused)],
        ),
        (
            Flaw::RefusesTheUnsafeProgramOffItsLocus,
            enumerating(),
            vec![(Faults, Mislocated)],
        ),
        (
            Flaw::AcceptsTheUnsafeProgram,
            enumerating(),
            vec![(Faults, Accepted)],
        ),
        (
            Flaw::LocatesTheFaultElsewhere,
            enumerating(),
            vec![(Faults, Mislocated)],
        ),
        (
            Flaw::LocatesTheFaultInAnotherSource,
            enumerating(),
            vec![(Faults, Mislocated)],
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
            // Faithful through Door B, the door's misanswer is found where the
            // probe lowers the corpus through Door A too.
            Flaw::MisanswersTheNativeCautiousAtDoorA,
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
            // reloads fails with outcome correctness — and a failed grounding's
            // program outlives the reset that rebuilds.
            Flaw::ResetsNothing,
            realising(),
            vec![
                (Outcome, Refused),
                (State, Refused),
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
            vec![(Faults, Accepted)],
        ),
        (
            Flaw::RefusesAssignmentAsUnsupported,
            realising(),
            vec![(Faults, Refused), (Declared(C::Externals), Refused)],
        ),
        (
            Flaw::RefusesAssignmentAtTheEngine,
            realising(),
            vec![(Faults, Mislocated), (Declared(C::Externals), Refused)],
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
            // Cut at its budget, yet read as closing the space: the truncated
            // search every complete collection would trust.
            Flaw::ConcludesItsCutAsExhaustion,
            only(Capability::TimeBudget),
            vec![(Declared(C::TimeBudget), Misanswered)],
        ),
        (
            Flaw::IgnoresTheBudget,
            only(Capability::TimeBudget),
            vec![(Declared(C::TimeBudget), Misanswered)],
        ),
        (
            Flaw::FaultsTheCutSearch,
            realising(),
            vec![(Declared(C::TimeBudget), Refused)],
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
fn a_refused_unbounded_program_leaves_the_time_budget_undriven() {
    let report = report(realising(), Flaw::RefusesTheUnboundedProgram);
    let undriven = skip(&report, Check::Capability(Capability::TimeBudget));
    assert!(matches!(undriven, Some(Skip::Undriven(_))), "{report}");
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
fn a_model_holding_a_number_fails_its_outcome_as_no_set_of_literals() {
    let report = report(deciding(), Flaw::YieldsANumber);
    let Some(Verdict::Failed(failure)) = report.verdict(Check::OutcomeCorrectness) else {
        panic!("the model holding a number fails its outcome: {report}");
    };
    assert!(failure.to_string().contains("no literal"), "{failure}");
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
