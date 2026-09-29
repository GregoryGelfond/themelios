//! The conformance suite (docs/design/solve.md §13.1): the executable suite
//! every adapter passes. [`run`] drives a backend through the contract (§4) — a
//! corpus of small programs whose answer sets are known independently of any
//! engine, each capability as declared, and the pathologies the vocabulary
//! forbids (§5.3) — and returns a [`ConformanceReport`]: typed data, one
//! [`Verdict`] per [`Check`], a failure carrying how the backend broke the
//! obligation, the corpus program it broke it on, and the backend's own fault —
//! never prose a consumer parses (§1.3).
//!
//! The suite checks what the compiler cannot. **Outcome correctness:** each
//! corpus program's determination is its known one, and — where the backend
//! enumerates — its answer sets are exactly its known ones, its search closing
//! the space. The corpus holds the programs a shortcut semantics gets wrong: a
//! positive loop, which completion reads with an unsupported model, and a head
//! cycle, which shifting the disjunction reads with none. **The `enumeration`
//! bit's soundness obligation** (§4.1): a search concluded as closing the space
//! yielded every answer set there is. **Capability honesty, in both directions**
//! (§4.1, §4.2): a declared capability's method answers, and answers rightly — a
//! provided method needs no override, so a declared bit whose method still
//! refuses is a lie the type cannot see — and an undeclared one's refuses at the
//! request locus, never degrading silently. **Fault loci:** a program the backend
//! cannot ground is refused at the program locus, which carries the statement's
//! source location (§5.4); assigning an atom that is not external is refused at
//! the request locus, never the silent no-op an engine may give. **The ground
//! program's provenance,** where the backend exposes one (§10.4): every ground
//! rule attributed to a statement of the program it grounds, and a fact never
//! grounded to nothing.
//!
//! The named pathologies are unconstructible in the vocabulary (§5.3). The suite
//! attempts the two a backend could reach at run time — a touched stream passing
//! as a complete collection, and a termination reading at odds with the logical
//! one (an inconsistent reading must rest on a search that closed the space).
//! The third — an enumeration reporting an improving trajectory — is a
//! compile-time fact the crate's compile-fail witnesses pin, as is the absent
//! public constructor of a proven optimum, which closes the last gap. That a
//! cancelled search never concludes as closing the space is reported skipped:
//! the interrupt handle is reserved, so no search can yet be cancelled through
//! it.
//!
//! Every stream the suite reads is bounded by the answer sets its program has, so
//! a run that yields past them — one that never ends — fails the check at that
//! bound rather than holding the suite. A check the suite cannot drive over a
//! backend — its program refused, say — is skipped, with the reason; the check
//! whose obligation the refusal breaks fails.

use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use themelios_program::raise::{raise_source, raise_str};
use themelios_program::{Dialect, Name, Origin, Program, Sign, Source, SourceId, Symbol};

use crate::agent::{Assumption, Scenario};
use crate::bridge::Door;
use crate::contract::{
    Backend, Capabilities, ConsequenceRequest, ConsequenceSupport, Fault, GroundOptions, Locus,
    Mode, OptimizeRequest, SolveRequest, TruthValue,
};
use crate::extend::{Function, GroundFault, Propagator};
use crate::outcome::{AnswerSet, Conclusion, Consequences, Determination, Solved};

// ---- The report ----

/// Run the suite over `backend` (docs/design/solve.md §13.1): the corpus, each
/// capability as declared, and the pathologies attempted — a [`Verdict`] per
/// [`Check`] in the returned report. Whatever the backend answers, refuses, or
/// faults with becomes a verdict, and every stream is read to a bound, so a run
/// that never ends fails its check rather than holding the suite. The suite
/// loads its own programs — on a multi-shot backend a `reset` first, since
/// `lower` accumulates there (§6.2) — and probes every capability, registering
/// its probe extensions where the backend accepts them, so the backend's state
/// afterwards is the suite's: run it over a backend kept for it. Cost: a handful
/// of solves per corpus program.
#[must_use]
pub fn run(backend: &mut dyn Backend) -> ConformanceReport {
    let corpus = corpus();
    let mut entries = vec![
        (
            Check::OutcomeCorrectness,
            outcome_correctness(backend, &corpus),
        ),
        (
            Check::ExhaustionIsEarned,
            exhaustion_is_earned(backend, &corpus),
        ),
        (
            Check::InconsistencyIsExhausted,
            inconsistency_is_exhausted(backend, &corpus),
        ),
        (
            Check::TruncationCannotPoseAsComplete,
            truncation_cannot_pose_as_complete(backend, &corpus),
        ),
        (
            Check::CancellationIsNotExhaustion,
            cancellation_is_not_exhaustion(backend),
        ),
        (
            Check::GroundProgramIsFaithful,
            ground_program_is_faithful(backend, &corpus),
        ),
        (
            Check::ProgramFaultIsLocated,
            program_fault_is_located(backend),
        ),
        (
            Check::NonExternalAssignmentRefuses,
            non_external_assignment_refuses(backend),
        ),
    ];
    // The capability probes run last: they reset the engine and register
    // extensions, which the corpus checks above must not see.
    entries.extend(CAPABILITIES.into_iter().map(|capability| {
        (
            Check::Capability(capability),
            capability_is_honest(backend, capability, &corpus),
        )
    }));
    ConformanceReport { entries }
}

/// What [`run`] found (docs/design/solve.md §13.1): one [`Verdict`] per
/// [`Check`], in the order the suite ran them — typed data a consumer matches
/// on, with a human `Display` beside it (§1.3). Owned plain data.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConformanceReport {
    entries: Vec<(Check, Verdict)>,
}

impl ConformanceReport {
    /// Each check the suite ran, with its verdict, in the order run. O(1) to
    /// begin; O(checks) to walk.
    pub fn entries(&self) -> impl Iterator<Item = (Check, &Verdict)> + '_ {
        self.entries
            .iter()
            .map(|(check, verdict)| (*check, verdict))
    }

    /// The verdict the suite reached on `check`, or `None` for a check it did
    /// not run. O(checks).
    pub fn verdict(&self, check: Check) -> Option<&Verdict> {
        self.entries
            .iter()
            .find(|(ran, _)| *ran == check)
            .map(|(_, verdict)| verdict)
    }

    /// Whether the backend conforms: no check found an obligation broken. A
    /// skipped check breaks none. O(checks).
    pub fn is_conformant(&self) -> bool {
        self.entries
            .iter()
            .all(|(_, verdict)| !matches!(verdict, Verdict::Failed(_)))
    }
}

impl fmt::Display for ConformanceReport {
    /// One entry per check — the obligation, then its verdict — a verdict's own
    /// line breaks, an engine's multi-line fault, indented beneath it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (check, verdict) in &self.entries {
            let verdict = verdict.to_string().replace('\n', "\n    ");
            writeln!(f, "{check}: {verdict}")?;
        }
        Ok(())
    }
}

/// An obligation the suite holds a backend to (docs/design/solve.md §13.1).
/// Non-exhaustive: the suite grows with the contract — a theory's cases, the
/// optimization and extension obligations — so a new check is a new variant,
/// not a migration.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Check {
    /// Each corpus program's determination is its known one, and where the
    /// backend enumerates, its answer sets are its known ones, its search
    /// closing the space.
    OutcomeCorrectness,
    /// A search concluded as closing the space yielded every answer set there
    /// is — the `enumeration` bit's soundness obligation (§4.1).
    ExhaustionIsEarned,
    /// An inconsistent reading rests on a search that closed the space.
    InconsistencyIsExhausted,
    /// A stream once touched cannot yield a complete collection (§5.3).
    TruncationCannotPoseAsComplete,
    /// A cancelled search never concludes as closing the space.
    CancellationIsNotExhaustion,
    /// Every ground rule of an exposed ground program is attributed to a
    /// statement of the program it grounds, and a fact grounds to a rule
    /// (§10.4).
    GroundProgramIsFaithful,
    /// A program the backend cannot ground is refused at the program locus,
    /// which carries the statement's source location (§5.4).
    ProgramFaultIsLocated,
    /// Assigning a truth value to an atom that is not external refuses at the
    /// request locus.
    NonExternalAssignmentRefuses,
    /// The capability's declaration is honest: declared, its method answers
    /// rightly; undeclared, it refuses at the request locus (§4.1, §4.2).
    Capability(Capability),
}

impl fmt::Display for Check {
    /// The obligation, as a phrase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Check::OutcomeCorrectness => {
                f.write_str("each corpus program's outcome is its known one")
            }
            Check::ExhaustionIsEarned => {
                f.write_str("a search concluded as closing the space yielded every answer set")
            }
            Check::InconsistencyIsExhausted => {
                f.write_str("an inconsistent reading rests on a search that closed the space")
            }
            Check::TruncationCannotPoseAsComplete => {
                f.write_str("a touched stream cannot yield a complete collection")
            }
            Check::CancellationIsNotExhaustion => {
                f.write_str("a cancelled search never concludes as closing the space")
            }
            Check::GroundProgramIsFaithful => {
                f.write_str("every ground rule is attributed to a statement of its program")
            }
            Check::ProgramFaultIsLocated => {
                f.write_str("a program that cannot be grounded is refused where it fails")
            }
            Check::NonExternalAssignmentRefuses => {
                f.write_str("assigning an atom that is not external refuses")
            }
            Check::Capability(capability) => write!(f, "the {capability} declaration is honest"),
        }
    }
}

/// A declared capability the suite holds to its declaration (docs/design/solve.md
/// §4.1): each gates the contract method, or the request field, that is its sole
/// engine primitive. Non-exhaustive: a capability the contract grows is a new
/// variant, not a migration.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Capability {
    /// Proving optima — `optimize` (§5.3).
    Optimization,
    /// The engine's own consequence door — `consequences_native` (§4.2).
    NativeConsequences,
    /// Solving under assumptions — `solve_assuming` (§6.3).
    Assumptions,
    /// Keeping the program across solves — `reset`, `ground`, and
    /// `assign_external` (§6.2).
    MultiShot,
    /// Honouring external atoms — an assignment through `assign_external`
    /// read back in the answer sets (§6.2).
    Externals,
    /// Interrupting an in-flight solve — `interrupt` (§6.1, §6.3).
    Cancellation,
    /// Enforcing a time budget on a solve — the request's `time` (§6.3).
    TimeBudget,
    /// Evaluating `@`-functions — `register_function` (§7).
    Functions,
    /// Running custom propagators — `register_propagator` (§8).
    Propagators,
}

impl fmt::Display for Capability {
    /// The capability, as a phrase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Capability::Optimization => "optimization",
            Capability::NativeConsequences => "native-consequences",
            Capability::Assumptions => "assumptions",
            Capability::MultiShot => "multi-shot",
            Capability::Externals => "externals",
            Capability::Cancellation => "cancellation",
            Capability::TimeBudget => "time-budget",
            Capability::Functions => "@-functions",
            Capability::Propagators => "propagators",
        })
    }
}

/// Every capability the suite probes, in the order it probes them: the
/// extension registrations last, so no probe runs against an engine carrying
/// another's registration.
const CAPABILITIES: [Capability; 9] = [
    Capability::Optimization,
    Capability::NativeConsequences,
    Capability::Assumptions,
    Capability::MultiShot,
    Capability::Externals,
    Capability::Cancellation,
    Capability::TimeBudget,
    Capability::Functions,
    Capability::Propagators,
];

/// A check's verdict — the conformance-testing verdicts, passed or failed, with
/// skipped in place of the literature's "inconclusive", which names a
/// determination here (§5.1). Closed, deliberately not `#[non_exhaustive]`: a
/// check either binds the backend and holds or breaks, or does not bind it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// The backend meets the obligation.
    Passed,
    /// The backend breaks the obligation, as the failure says.
    Failed(Failure),
    /// The obligation does not bind the backend — it rests on a capability the
    /// backend does not declare, or the suite could not drive it over this
    /// backend; the message says which.
    Skipped(String),
}

impl fmt::Display for Verdict {
    /// The verdict, then its reason where it carries one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Passed => f.write_str("passed"),
            Verdict::Failed(failure) => write!(f, "failed — {failure}"),
            Verdict::Skipped(why) => write!(f, "skipped — {why}"),
        }
    }
}

/// Why a check failed (docs/design/solve.md §1.3): how the backend broke the
/// obligation, the corpus program it broke it on where it broke it on one, and
/// the backend's own fault where the breach is a fault it raised — typed, with
/// the sentence a person reads as its `Display`. Non-exhaustive: a failure that
/// may carry more.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    breach: Breach,
    case: Option<&'static str>,
    fault: Option<Fault>,
    detail: String,
}

impl Failure {
    /// A failure by `breach`, as `detail` says.
    fn new(breach: Breach, detail: impl Into<String>) -> Failure {
        Failure {
            breach,
            case: None,
            fault: None,
            detail: detail.into(),
        }
    }

    /// This failure, carrying the backend's own `fault`.
    fn with_fault(mut self, fault: Fault) -> Failure {
        self.fault = Some(fault);
        self
    }

    /// This failure, on the corpus program named `case`.
    fn on(mut self, case: &'static str) -> Failure {
        self.case = Some(case);
        self
    }

    /// How the backend broke the obligation. O(1).
    pub fn breach(&self) -> Breach {
        self.breach
    }

    /// The corpus program the backend broke the obligation on, by the name a
    /// report gives it — `None` where the check drives no corpus program. O(1).
    pub fn case(&self) -> Option<&str> {
        self.case
    }

    /// The backend's own fault, where the breach is one it raised — a refusal
    /// it owed an answer to, or one at the wrong locus. O(1).
    pub fn fault(&self) -> Option<&Fault> {
        self.fault.as_ref()
    }
}

impl fmt::Display for Failure {
    /// The corpus program, then how the obligation broke, then the backend's
    /// own fault where there is one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(case) = self.case {
            write!(f, "{case}: ")?;
        }
        f.write_str(&self.detail)?;
        if let Some(fault) = &self.fault {
            write!(f, ": {fault}")?;
        }
        Ok(())
    }
}

/// How a backend broke an obligation (docs/design/solve.md §13.1). Non-exhaustive:
/// a way of breaking the contract the suite comes to tell apart is a new variant.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Breach {
    /// It refused, or faulted, where it owed an answer.
    Refused,
    /// It answered, but wrongly.
    Misanswered,
    /// It answered where it owed a refusal: a request beyond its declaration,
    /// an atom that is not external, a program it cannot ground.
    Accepted,
    /// It refused where it owed a refusal, but at a locus other than the one
    /// owed.
    Mislocated,
}

// ---- The corpus (§13.1) ----

/// A corpus program with its answer sets, known independently of any engine —
/// each the textbook reading of the construct it exercises.
struct Case {
    /// How a verdict names the program.
    name: &'static str,
    /// The program's clingo-dialect source.
    source: &'static str,
    /// The program the source denotes.
    program: Program,
    /// The answer sets, sorted, so a sorted enumeration compares equal to them.
    answer_sets: Vec<AnswerSet>,
}

impl Case {
    /// Whether the program has an answer set.
    fn is_consistent(&self) -> bool {
        !self.answer_sets.is_empty()
    }
}

/// The program `source` denotes, in the clingo dialect. A suite program is fixed
/// text far within the coordinate limit — the one condition raising refuses — so
/// the expect discharges an invariant.
fn program_of(source: &str) -> Program {
    raise_str(source, Dialect::Clingo)
        .expect("a suite program is far within the coordinate limit")
        .into_program()
}

/// The program `source` denotes, raised as the source text `id` names — so the
/// origins of one corpus program's statements are never another's. The expect
/// discharges the same invariant as [`program_of`]'s.
fn program_under(id: SourceId, source: &str) -> Program {
    let source = Source::new(id, source.to_owned())
        .expect("a suite program is far within the coordinate limit");
    raise_source(&source, Dialect::Clingo).into_program()
}

/// The ground atom `name`, under `sign`, applied to `arguments`. A suite name is
/// a fixed identifier, so the expect discharges an invariant.
fn atom(name: &str, arguments: impl IntoIterator<Item = Symbol>, sign: Sign) -> Symbol {
    Symbol::function(
        Name::new(name).expect("a suite name is an identifier"),
        arguments,
        sign,
    )
}

/// The ground constant `name`.
fn constant(name: &str) -> Symbol {
    atom(name, [], Sign::Positive)
}

/// The answer set of the given atoms.
fn answer_set(atoms: impl IntoIterator<Item = Symbol>) -> AnswerSet {
    atoms.into_iter().collect()
}

/// The fact `a.`: one answer set, `{a}`.
const FACT: &str = "a.";

/// The even loop over `a` and `b`: two answer sets, `{a}` and `{b}` — the
/// smallest program a scenario fixing `a` narrows.
const EVEN_LOOP: &str = "a :- not b. b :- not a.";

/// A choice under an objective: `solve` reads its answer sets `{}` and `{a}`
/// whatever the objective (§5.2); `optimize` proves `{}` optimal.
const OPTIMIZATION: &str = "{ a }. #minimize { 1 : a }.";

/// An external atom and a rule over it: `{a, b}` while `a` is assigned true,
/// `{}` while it is assigned false.
const EXTERNAL: &str = "#external a. b :- a.";

/// A fact over a variable nothing binds: no grounder can instantiate it.
const UNSAFE: &str = "p(X).";

/// The corpus: small programs whose answer sets are known independently of any
/// engine, each raised under its own source id.
fn corpus() -> Vec<Case> {
    let p = |n| atom("p", [Symbol::number(n)], Sign::Positive);
    let q = |n| atom("q", [Symbol::number(n)], Sign::Positive);
    let known: Vec<(&'static str, &'static str, Vec<AnswerSet>)> = vec![
        ("the empty program", "", vec![answer_set([])]),
        ("a fact", FACT, vec![answer_set([constant("a")])]),
        (
            "an even loop",
            EVEN_LOOP,
            vec![answer_set([constant("a")]), answer_set([constant("b")])],
        ),
        ("an odd loop", "a :- not a.", vec![]),
        ("a violated constraint", "a. :- a.", vec![]),
        (
            "classical negation",
            "-a. b :- -a.",
            vec![answer_set([atom("a", [], Sign::Negative), constant("b")])],
        ),
        (
            "a disjunction",
            "a ; b.",
            vec![answer_set([constant("a")]), answer_set([constant("b")])],
        ),
        (
            "a choice",
            "{ a }.",
            vec![answer_set([]), answer_set([constant("a")])],
        ),
        (
            "a rule over an interval",
            "p(1..2). q(X) :- p(X).",
            vec![answer_set([p(1), p(2), q(1), q(2)])],
        ),
        // Completion admits the unsupported {a, b}; the stable reading is {}.
        ("a positive loop", "a :- b. b :- a.", vec![answer_set([])]),
        // Shifting the disjunction leaves no answer set; the stable reading is
        // {a, b}.
        (
            "a head cycle",
            "a ; b. a :- b. b :- a.",
            vec![answer_set([constant("a"), constant("b")])],
        ),
    ];
    known
        .into_iter()
        .zip(0..)
        .map(|((name, source, mut answer_sets), id)| {
            answer_sets.sort();
            Case {
                name,
                source,
                program: program_under(SourceId::new(id), source),
                answer_sets,
            }
        })
        .collect()
}

// ---- Driving a backend ----

/// Why a per-case check stopped short of holding on a case.
enum Stop {
    /// The backend broke the obligation.
    Broke(Failure),
    /// The check could not be driven over the case, as the refusal to drive it
    /// says — a failure of no obligation this check holds.
    Undriven(Failure),
}

/// The verdict of a per-case check over the corpus: the first case it finds
/// broken, named; otherwise skipped with the first case it could not drive, if
/// any; otherwise passed.
fn over_corpus(corpus: &[Case], mut check: impl FnMut(&Case) -> Result<(), Stop>) -> Verdict {
    let mut undrivable = None;
    for case in corpus {
        match check(case) {
            Ok(()) => {}
            Err(Stop::Broke(failure)) => return Verdict::Failed(failure.on(case.name)),
            Err(Stop::Undriven(why)) => {
                undrivable.get_or_insert_with(|| format!("{}: {why}", case.name));
            }
        }
    }
    undrivable.map_or(Verdict::Passed, Verdict::Skipped)
}

/// Load `program` — raised from `source` — as the whole of what the backend
/// reasons over (§6.2): on a multi-shot backend, whose `lower` accumulates, a
/// `reset` first. The refusal names the step refused.
fn load(backend: &mut dyn Backend, program: &Program, source: &str) -> Result<(), Failure> {
    if backend.capabilities().multi_shot {
        backend.reset().map_err(|fault| {
            Failure::new(Breach::Refused, "the reset before loading was refused").with_fault(fault)
        })?;
    }
    backend.lower(Door::Program(program)).map_err(|fault| {
        Failure::new(
            Breach::Refused,
            format!("the program `{source}` was refused"),
        )
        .with_fault(fault)
    })
}

/// Load the program `source` denotes, as [`load`] does.
fn load_source(backend: &mut dyn Backend, source: &str) -> Result<(), Failure> {
    load(backend, &program_of(source), source)
}

/// Solve the loaded program, the refusal as a failure.
fn solve(backend: &mut dyn Backend) -> Result<Solved<'_>, Failure> {
    backend
        .solve(&SolveRequest::default())
        .map_err(|fault| Failure::new(Breach::Refused, "the solve was refused").with_fault(fault))
}

/// What a bounded read of a stream found: the answer sets it yielded, and
/// whether it ended within the bound.
struct Pulled {
    sets: Vec<AnswerSet>,
    ended: bool,
}

/// Read at most one answer set more than `bound` — enough to see the stream end,
/// or to see it yield past a program's answer sets — so a run that never ends is
/// caught at the bound rather than drained forever. A mid-stream fault is the
/// refusal.
fn pull(solved: &mut Solved<'_>, bound: usize) -> Result<Pulled, Fault> {
    let mut sets = Vec::new();
    for yielded in solved.answer_sets() {
        let set = yielded?;
        if sets.len() == bound {
            return Ok(Pulled { sets, ended: false });
        }
        sets.push(set);
    }
    Ok(Pulled { sets, ended: true })
}

/// A mid-stream fault, as a failure.
fn faulted(fault: Fault) -> Failure {
    Failure::new(Breach::Refused, "the stream faulted").with_fault(fault)
}

/// "consistent" or "inconsistent".
fn consistency(consistent: bool) -> &'static str {
    if consistent {
        "consistent"
    } else {
        "inconsistent"
    }
}

// ---- The checks ----

/// Each corpus program's determination is its known one; where the backend
/// declares `enumeration`, its answer sets are exactly its known ones and its
/// search closes the space, and where it does not, every model it yields is one
/// of them. A refusal to load or solve a corpus program breaks this obligation.
fn outcome_correctness(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    let enumerates = backend.capabilities().enumeration;
    over_corpus(corpus, |case| {
        load(backend, &case.program, case.source).map_err(Stop::Broke)?;
        let mut solved = solve(backend).map_err(Stop::Broke)?;
        let consistent = match solved.determination() {
            Determination::Consistent(_) => true,
            Determination::Inconsistent(_) => false,
            Determination::Inconclusive(partial) => {
                let undecided = Failure::new(
                    Breach::Refused,
                    format!("the search stopped undecided: {}", partial.conclusion()),
                );
                return Err(Stop::Broke(match partial.cause() {
                    Some(cause) => undecided.with_fault(cause.clone()),
                    None => undecided,
                }));
            }
        };
        if consistent != case.is_consistent() {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                format!(
                    "read {} where the program is {}",
                    consistency(consistent),
                    consistency(case.is_consistent()),
                ),
            )));
        }
        let known = case.answer_sets.len();
        let Pulled { mut sets, ended } =
            pull(&mut solved, known).map_err(|fault| Stop::Broke(faulted(fault)))?;
        if !ended {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                format!("yielded more than its {known} answer sets"),
            )));
        }
        if sets
            .iter()
            .any(|set| case.answer_sets.binary_search(set).is_err())
        {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                "yielded a set that is not one of its answer sets",
            )));
        }
        if enumerates {
            sets.sort();
            if sets != case.answer_sets {
                return Err(Stop::Broke(Failure::new(
                    Breach::Misanswered,
                    format!(
                        "enumerated {} sets, not its {known} answer sets",
                        sets.len()
                    ),
                )));
            }
            if solved.conclusion() != Some(Conclusion::Exhausted) {
                return Err(Stop::Broke(Failure::new(
                    Breach::Misanswered,
                    "ended its search without closing the space",
                )));
            }
        }
        Ok(())
    })
}

/// A search concluded as closing the space yielded every answer set there is —
/// the `enumeration` bit's soundness obligation (§4.1). A backend that stops at
/// a witness, or anywhere short, must say so; every universal reading trusts an
/// `Exhausted` conclusion.
fn exhaustion_is_earned(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_corpus(corpus, |case| {
        load(backend, &case.program, case.source).map_err(Stop::Undriven)?;
        let mut solved = solve(backend).map_err(Stop::Undriven)?;
        // A stream that faults, or runs past the bound, has concluded nothing.
        let Ok(Pulled { mut sets, ended }) = pull(&mut solved, case.answer_sets.len()) else {
            return Ok(());
        };
        if ended && solved.conclusion() == Some(Conclusion::Exhausted) {
            sets.sort();
            if sets != case.answer_sets {
                return Err(Stop::Broke(Failure::new(
                    Breach::Misanswered,
                    format!(
                        "concluded that the search closed the space having yielded {} sets, not its {} answer sets",
                        sets.len(),
                        case.answer_sets.len(),
                    ),
                )));
            }
        }
        Ok(())
    })
}

/// An inconsistent reading rests on a search that closed the space —
/// `Inconsistent ⇒ Exhausted`, the termination reading never at odds with the
/// logical one. The core's classification reads a clean end short of the space
/// as inconclusive, so no backend can break it; attempted all the same, over
/// every corpus program.
fn inconsistency_is_exhausted(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_corpus(corpus, |case| {
        load(backend, &case.program, case.source).map_err(Stop::Undriven)?;
        let mut solved = solve(backend).map_err(Stop::Undriven)?;
        let inconsistent = matches!(solved.determination(), Determination::Inconsistent(_));
        if inconsistent && solved.conclusion() != Some(Conclusion::Exhausted) {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                "read inconsistent over a search that did not close the space",
            )));
        }
        Ok(())
    })
}

/// A stream once touched cannot yield a complete collection (§5.3): the
/// exhaustion gate refuses it, so a truncated search cannot pass as all the
/// answer sets. Structural; attempted over every consistent corpus program.
fn truncation_cannot_pose_as_complete(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_corpus(corpus, |case| {
        if !case.is_consistent() {
            return Ok(());
        }
        load(backend, &case.program, case.source).map_err(Stop::Undriven)?;
        let mut solved = solve(backend).map_err(Stop::Undriven)?;
        drop(solved.answer_sets().next());
        // The gate refuses a touched handle at once, draining nothing.
        if solved.all_answer_sets().is_ok() {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                "a touched stream yielded a complete collection",
            )));
        }
        Ok(())
    })
}

/// A cancelled search never concludes as closing the space. Not yet drivable:
/// the interrupt handle is reserved, so no search can be cancelled through it.
fn cancellation_is_not_exhaustion(backend: &dyn Backend) -> Verdict {
    Verdict::Skipped(
        if backend.capabilities().cancellation {
            "the interrupt handle is reserved, so no search can yet be cancelled through it"
        } else {
            "the backend does not declare cancellation"
        }
        .to_owned(),
    )
}

/// Where the backend exposes its ground program, every ground rule is
/// attributed to a statement of the program it grounds (§10.4) — the provenance
/// an explanation reads back to source, never an invented one — and a fact
/// grounds to at least one rule, so an empty ground program cannot pass as a
/// faithful one. (A rule nothing can support may ground to nothing, so only the
/// fact is owed a rule.)
fn ground_program_is_faithful(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    let mut exposed = false;
    let verdict = over_corpus(corpus, |case| {
        load(backend, &case.program, case.source).map_err(Stop::Undriven)?;
        {
            // Read the stream out, bounded, so an engine that grounds as it
            // searches has grounded before its ground program is read.
            let mut solved = solve(backend).map_err(Stop::Undriven)?;
            if pull(&mut solved, case.answer_sets.len()).is_err() {
                return Ok(());
            }
        }
        let Some(ground) = backend.ground_program() else {
            return Ok(());
        };
        exposed = true;
        let origins: BTreeSet<&Origin> = case
            .program
            .statements()
            .flat_map(|node| node.provenance().origins())
            .collect();
        if ground.rules().any(|rule| !origins.contains(rule.origin())) {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                "a ground rule is attributed to no statement of the program",
            )));
        }
        if case.source == FACT && ground.rules().next().is_none() {
            return Err(Stop::Broke(Failure::new(
                Breach::Misanswered,
                "grounded a fact to no rule",
            )));
        }
        Ok(())
    });
    if verdict == Verdict::Passed && !exposed {
        return Verdict::Skipped("the backend exposes no ground program".to_owned());
    }
    verdict
}

/// A program the backend cannot ground — a variable nothing binds — is refused
/// at the program locus (§5.4), whether at lowering, at the solve, or at the
/// stream's first item; a program fault is located by construction, so the
/// refusal carries the statement's source location.
fn program_fault_is_located(backend: &mut dyn Backend) -> Verdict {
    if backend.capabilities().multi_shot
        && let Err(fault) = backend.reset()
    {
        return Verdict::Skipped(format!("the reset before loading was refused: {fault}"));
    }
    if let Err(fault) = backend.lower(Door::Program(&program_of(UNSAFE))) {
        return located(fault);
    }
    let mut solved = match backend.solve(&SolveRequest::default()) {
        Ok(solved) => solved,
        Err(fault) => return located(fault),
    };
    match solved.answer_sets().next() {
        Some(Err(fault)) => located(fault),
        _ => Verdict::Failed(Failure::new(
            Breach::Accepted,
            format!("the program `{UNSAFE}`, whose variable nothing binds, was accepted"),
        )),
    }
}

/// The verdict on a refusal of a program that cannot be grounded: passed at
/// the program locus, where the fault carries its statement's location.
fn located(fault: Fault) -> Verdict {
    if fault.locus() == Locus::Program && fault.located().is_some() {
        Verdict::Passed
    } else {
        Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                format!(
                    "a program that cannot be grounded was refused at the {} locus, not the program's",
                    locus_phrase(fault.locus()),
                ),
            )
            .with_fault(fault),
        )
    }
}

/// Assigning a truth value to an atom that is not external refuses at the
/// request locus — never the silent no-op an engine may give, which would let
/// the knowledge base and the engine disagree at a retraction's toggle (§4.2,
/// §6.2). Binds a backend declaring multi-shot solving, under which
/// `assign_external` is required.
fn non_external_assignment_refuses(backend: &mut dyn Backend) -> Verdict {
    if !backend.capabilities().multi_shot {
        return Verdict::Skipped(
            "the backend does not declare multi-shot solving, which assigning an external needs"
                .to_owned(),
        );
    }
    if let Err(failure) = load_source(backend, FACT) {
        return Verdict::Skipped(failure.to_string());
    }
    match backend.assign_external(constant("a"), TruthValue::True) {
        Ok(()) => Verdict::Failed(Failure::new(
            Breach::Accepted,
            "assigning an atom that is not external was accepted — a silent no-op",
        )),
        Err(fault) if fault == Fault::unsupported() => Verdict::Failed(
            Failure::new(
                Breach::Refused,
                "assign_external refused as unsupported, though the backend declares multi-shot solving",
            )
            .with_fault(fault),
        ),
        Err(fault) if fault.locus() == Locus::Request => Verdict::Passed,
        Err(fault) => Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                format!(
                    "assigning an atom that is not external refused at the {} locus, not the request's",
                    locus_phrase(fault.locus()),
                ),
            )
            .with_fault(fault),
        ),
    }
}

// ---- Capability honesty (§4.1, §4.2) ----

/// How a capability's method met the suite's probe.
enum Response {
    /// It answered, as a declared capability's method owes.
    Answered,
    /// It refused, with this fault.
    Refused(Fault),
    /// It answered, but wrongly: the phrase says how.
    Misanswered(String),
    /// The probe could not reach the method.
    Unprobed(Failure),
}

/// The response a method's result makes: answered, or refused with its fault.
fn respond<T>(result: Result<T, Fault>) -> Response {
    match result {
        Ok(_) => Response::Answered,
        Err(fault) => Response::Refused(fault),
    }
}

/// Whether `response` is a refusal at the request locus — what an undeclared
/// capability's method owes.
fn refuses_at_the_request(response: &Response) -> bool {
    matches!(response, Response::Refused(fault) if fault.locus() == Locus::Request)
}

/// A locus, as a phrase.
fn locus_phrase(locus: Locus) -> &'static str {
    match locus {
        Locus::Program => "program",
        Locus::Request => "request",
        Locus::Resource => "resource",
        Locus::Engine => "engine",
        Locus::Adapter => "adapter",
    }
}

/// Whether `capabilities` declares `capability`.
fn is_declared(capability: Capability, capabilities: &Capabilities) -> bool {
    match capability {
        Capability::Optimization => capabilities.optimization,
        Capability::NativeConsequences => {
            capabilities.native_consequences == ConsequenceSupport::Native
        }
        Capability::Assumptions => capabilities.assumptions,
        Capability::MultiShot => capabilities.multi_shot,
        Capability::Externals => capabilities.externals,
        Capability::Cancellation => capabilities.cancellation,
        Capability::TimeBudget => capabilities.budgets.time,
        Capability::Functions => capabilities.functions,
        Capability::Propagators => capabilities.propagators,
    }
}

/// The contract method, or request, a capability's probe drives, as a verdict
/// names it.
fn method_of(capability: Capability) -> &'static str {
    match capability {
        Capability::Optimization => "optimize",
        Capability::NativeConsequences => "consequences_native",
        Capability::Assumptions => "solve_assuming",
        Capability::MultiShot => "the multi-shot methods",
        Capability::Externals => "assign_external",
        Capability::Cancellation => "interrupt",
        Capability::TimeBudget => "a solve under a time budget",
        Capability::Functions => "register_function",
        Capability::Propagators => "register_propagator",
    }
}

/// One capability's declaration is honest (§4.1, §4.2): declared, its method
/// answers rightly; undeclared, it refuses at the request locus. `externals`
/// gates no method alone — `assign_external` is multi-shot's — so undeclared,
/// it binds nothing.
fn capability_is_honest(
    backend: &mut dyn Backend,
    capability: Capability,
    corpus: &[Case],
) -> Verdict {
    let declared = is_declared(capability, &backend.capabilities());
    if capability == Capability::Externals && !declared {
        return Verdict::Skipped(
            "the backend does not declare externals, and no contract method refuses on that bit alone"
                .to_owned(),
        );
    }
    let response = match capability {
        Capability::Optimization => probe_optimization(backend),
        Capability::NativeConsequences => probe_native_consequences(backend, corpus),
        Capability::Assumptions => probe_assumptions(backend),
        Capability::MultiShot => probe_multi_shot(backend, declared),
        Capability::Externals => probe_externals(backend),
        // `interrupt` answers `None` where another method refuses (§4.1).
        Capability::Cancellation => match backend.interrupt() {
            Some(_) => Response::Answered,
            None => Response::Refused(Fault::unsupported()),
        },
        Capability::TimeBudget => probe_time_budget(backend),
        Capability::Functions => respond(backend.register_function(Box::new(Echo))),
        Capability::Propagators => respond(backend.register_propagator(Box::new(Inert))),
    };
    judge(method_of(capability), declared, response)
}

/// The verdict on a probe's response to a capability declared, or not.
fn judge(method: &str, declared: bool, response: Response) -> Verdict {
    match (declared, response) {
        (_, Response::Unprobed(failure)) => {
            Verdict::Skipped(format!("{method} could not be probed: {failure}"))
        }
        (true, Response::Answered) => Verdict::Passed,
        (true, Response::Refused(fault)) => Verdict::Failed(
            Failure::new(Breach::Refused, format!("declared, yet {method} refused"))
                .with_fault(fault),
        ),
        (true, Response::Misanswered(why)) => Verdict::Failed(Failure::new(
            Breach::Misanswered,
            format!("declared, yet {method} {why}"),
        )),
        (false, response) if refuses_at_the_request(&response) => Verdict::Passed,
        (false, Response::Refused(fault)) => Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                format!(
                    "undeclared, and {method} refused at the {} locus, not the request's",
                    locus_phrase(fault.locus()),
                ),
            )
            .with_fault(fault),
        ),
        (false, Response::Answered | Response::Misanswered(_)) => Verdict::Failed(Failure::new(
            Breach::Accepted,
            format!(
                "undeclared, yet {method} answered: a request beyond the declaration must refuse"
            ),
        )),
    }
}

/// Optimization's probe: the optimum of a choice under an objective.
fn probe_optimization(backend: &mut dyn Backend) -> Response {
    if let Err(failure) = load_source(backend, OPTIMIZATION) {
        return Response::Unprobed(failure);
    }
    respond(backend.optimize(&OptimizeRequest::default()))
}

/// The native door's probe: over every consistent corpus program, its cautious
/// and brave consequences are its known answer sets' `⋂` and `⋃`; over every
/// inconsistent one, it refuses — `⋂`/`⋃` over no answer set is undefined, not
/// `∅` (query.md §2.3). Where the backend also declares `assumptions` — the only
/// backend handed a non-empty scenario (§6.2) — the door ranges over the models
/// a request's scenario admits, so it agrees with the fold over `solve_assuming`.
fn probe_native_consequences(backend: &mut dyn Backend, corpus: &[Case]) -> Response {
    let assumes = backend.capabilities().assumptions;
    let (consistent, inconsistent): (Vec<&Case>, Vec<&Case>) =
        corpus.iter().partition(|case| case.is_consistent());
    for case in consistent {
        if let Err(failure) = load(backend, &case.program, case.source) {
            return Response::Unprobed(failure);
        }
        for mode in [Mode::Cautious, Mode::Brave] {
            match backend.consequences_native(mode, &ConsequenceRequest::default()) {
                Err(fault) => return Response::Refused(fault),
                Ok(found) if found != Consequences::fold(mode, case.answer_sets.iter()) => {
                    return Response::Misanswered(format!(
                        "gave consequences of {} that are not its answer sets'",
                        case.name,
                    ));
                }
                Ok(_) => {}
            }
        }
    }
    for case in inconsistent {
        if let Err(failure) = load(backend, &case.program, case.source) {
            return Response::Unprobed(failure);
        }
        for mode in [Mode::Cautious, Mode::Brave] {
            if backend
                .consequences_native(mode, &ConsequenceRequest::default())
                .is_ok()
            {
                return Response::Misanswered(format!(
                    "answered consequences of {}, which has no answer set",
                    case.name,
                ));
            }
        }
    }
    if assumes {
        return probe_scoped_native_consequences(backend);
    }
    Response::Answered
}

/// The native door under a scenario: the even loop's consequences under a
/// scenario fixing `a` to hold, then not to, are those of the one answer set
/// each admits — a door that dropped the scenario would answer the whole
/// program's.
fn probe_scoped_native_consequences(backend: &mut dyn Backend) -> Response {
    if let Err(failure) = load_source(backend, EVEN_LOOP) {
        return Response::Unprobed(failure);
    }
    for (holds, admitted) in [(true, "a"), (false, "b")] {
        let request = ConsequenceRequest {
            scenario: fixing_a(holds),
        };
        let admitted = [answer_set([constant(admitted)])];
        for mode in [Mode::Cautious, Mode::Brave] {
            match backend.consequences_native(mode, &request) {
                Err(fault) => return Response::Refused(fault),
                Ok(found) if found != Consequences::fold(mode, admitted.iter()) => {
                    return Response::Misanswered(
                        "gave consequences under a scenario that are not its admitted models'"
                            .to_owned(),
                    );
                }
                Ok(_) => {}
            }
        }
    }
    Response::Answered
}

/// The scenario fixing `a` to hold (`true`) or not to (`false`) — over the even
/// loop, it admits exactly `{a}`, or exactly `{b}`; over the fact `a.`, fixing
/// `a` not to hold admits nothing. A suite constant is an atom, so the expect
/// discharges an invariant.
fn fixing_a(holds: bool) -> Scenario {
    let assumption = Assumption::new(constant("a"), holds).expect("a suite constant is an atom");
    [assumption].into_iter().collect()
}

/// Assumptions' probe: the even loop under a scenario fixing `a` to hold, then
/// not to — each read as consistent, its models ranging over the scenario asked
/// and each the one answer set it admits — and the fact `a.` under `a` fixed not
/// to hold, which admits no model and reads inconsistent.
fn probe_assumptions(backend: &mut dyn Backend) -> Response {
    let enumerates = backend.capabilities().enumeration;
    if let Err(failure) = load_source(backend, EVEN_LOOP) {
        return Response::Unprobed(failure);
    }
    for (holds, admitted) in [(true, "a"), (false, "b")] {
        let scenario = fixing_a(holds);
        let admitted = answer_set([constant(admitted)]);
        let mut solved = match backend.solve_assuming(&scenario, &SolveRequest::default()) {
            Ok(solved) => solved,
            Err(fault) => return Response::Refused(fault),
        };
        match solved.determination() {
            Determination::Consistent(models) if *models.scenario() == scenario => {}
            Determination::Consistent(_) => {
                return Response::Misanswered(
                    "ranged its models over a scenario other than the one asked".to_owned(),
                );
            }
            _ => {
                return Response::Misanswered(
                    "did not read a satisfiable scenario as consistent".to_owned(),
                );
            }
        }
        let admits_only_its_model = match pull(&mut solved, 1) {
            Ok(Pulled { sets, ended }) => {
                ended && sets.iter().all(|set| *set == admitted) && (!enumerates || sets.len() == 1)
            }
            Err(fault) => return Response::Refused(fault),
        };
        if !admits_only_its_model {
            return Response::Misanswered(
                "yielded models other than the one the scenario admits".to_owned(),
            );
        }
    }
    if let Err(failure) = load_source(backend, FACT) {
        return Response::Unprobed(failure);
    }
    let mut solved = match backend.solve_assuming(&fixing_a(false), &SolveRequest::default()) {
        Ok(solved) => solved,
        Err(fault) => return Response::Refused(fault),
    };
    if !matches!(solved.determination(), Determination::Inconsistent(_)) {
        return Response::Misanswered(
            "did not read an unsatisfiable scenario as inconsistent".to_owned(),
        );
    }
    Response::Answered
}

/// Multi-shot's probe. Declared: `reset` and `ground` of no part answer — the
/// assignment of an external is the externals probe's, and of a non-external
/// its own check. Undeclared: all three multi-shot methods refuse at the
/// request locus; the first that does not is the response.
fn probe_multi_shot(backend: &mut dyn Backend, declared: bool) -> Response {
    if declared {
        if let Err(fault) = backend.reset() {
            return Response::Refused(fault);
        }
        return respond(backend.ground(&[], &GroundOptions::default()));
    }
    let reset = respond(backend.reset());
    let ground = respond(backend.ground(&[], &GroundOptions::default()));
    let assign = respond(backend.assign_external(constant("a"), TruthValue::True));
    [ground, assign].into_iter().fold(reset, |kept, next| {
        if refuses_at_the_request(&kept) {
            next
        } else {
            kept
        }
    })
}

/// The externals probe: over an external atom and a rule on it, assigning the
/// atom true reads `{a, b}` and assigning it false reads `{}` — the assignment
/// answered, and honoured in the answer sets.
fn probe_externals(backend: &mut dyn Backend) -> Response {
    if let Err(failure) = load_source(backend, EXTERNAL) {
        return Response::Unprobed(failure);
    }
    for (value, expected) in [
        (TruthValue::True, answer_set([constant("a"), constant("b")])),
        (TruthValue::False, answer_set([])),
    ] {
        if let Err(fault) = backend.assign_external(constant("a"), value) {
            return Response::Refused(fault);
        }
        let mut solved = match backend.solve(&SolveRequest::default()) {
            Ok(solved) => solved,
            Err(fault) => return Response::Refused(fault),
        };
        match pull(&mut solved, 1) {
            Ok(Pulled { sets, ended: true }) if sets == [expected] => {}
            Ok(_) => {
                return Response::Misanswered(
                    "answered, then read answer sets other than the assignment gives".to_owned(),
                );
            }
            Err(fault) => return Response::Refused(fault),
        }
    }
    Response::Answered
}

/// A time budget no solve of the probe program approaches — so a solve under it
/// answers, never concluding at the budget.
const PROBE_BUDGET: Duration = Duration::from_mins(1);

/// The time budget's probe: a small program solved under a budget.
fn probe_time_budget(backend: &mut dyn Backend) -> Response {
    if let Err(failure) = load_source(backend, FACT) {
        return Response::Unprobed(failure);
    }
    respond(backend.solve(&SolveRequest {
        time: Some(PROBE_BUDGET),
    }))
}

/// The `@`-function the suite registers to probe `functions`: it answers its
/// arguments.
struct Echo;

impl Function for Echo {
    fn call(&self, arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault> {
        Ok(arguments.to_vec())
    }
}

/// The propagator the suite registers to probe `propagators`: it propagates
/// nothing.
struct Inert;

impl Propagator for Inert {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{GroundProgram, GroundRule};
    use crate::outcome::Run;

    /// The checks that are not a capability's.
    const OBLIGATIONS: [Check; 8] = [
        Check::OutcomeCorrectness,
        Check::ExhaustionIsEarned,
        Check::InconsistencyIsExhausted,
        Check::TruncationCannotPoseAsComplete,
        Check::CancellationIsNotExhaustion,
        Check::GroundProgramIsFaithful,
        Check::ProgramFaultIsLocated,
        Check::NonExternalAssignmentRefuses,
    ];

    #[test]
    fn every_suite_program_raises_without_a_diagnostic() {
        let sources = corpus()
            .iter()
            .map(|case| case.source)
            .chain([FACT, EVEN_LOOP, OPTIMIZATION, EXTERNAL, UNSAFE])
            .collect::<Vec<_>>();
        for source in sources {
            let raised = raise_str(source, Dialect::Clingo).expect("a suite program raises");
            assert!(raised.diagnostics().is_empty(), "{source}");
        }
    }

    #[test]
    fn a_corpus_program_s_answer_sets_are_distinct() {
        for case in corpus() {
            let distinct: BTreeSet<&AnswerSet> = case.answer_sets.iter().collect();
            assert_eq!(distinct.len(), case.answer_sets.len(), "{}", case.name);
        }
    }

    #[test]
    fn no_two_corpus_programs_share_an_origin() {
        let mut seen = BTreeSet::new();
        for case in corpus() {
            for node in case.program.statements() {
                for origin in node.provenance().origins() {
                    assert!(
                        seen.insert(origin.clone()),
                        "{} shares an origin",
                        case.name
                    );
                }
            }
        }
    }

    /// A report of the given entries — built here, where its field is.
    fn report_of(entries: Vec<(Check, Verdict)>) -> ConformanceReport {
        ConformanceReport { entries }
    }

    #[test]
    fn a_report_with_a_failure_does_not_conform() {
        let report = report_of(vec![
            (Check::OutcomeCorrectness, Verdict::Passed),
            (
                Check::ExhaustionIsEarned,
                Verdict::Failed(Failure::new(Breach::Misanswered, "why")),
            ),
        ]);
        assert!(!report.is_conformant());
    }

    #[test]
    fn a_skipped_check_breaks_no_conformance() {
        let report = report_of(vec![(
            Check::CancellationIsNotExhaustion,
            Verdict::Skipped("why".to_owned()),
        )]);
        assert!(report.is_conformant());
    }

    #[test]
    fn a_check_the_suite_did_not_run_has_no_verdict() {
        let report = report_of(vec![(Check::OutcomeCorrectness, Verdict::Passed)]);
        assert_eq!(report.verdict(Check::ExhaustionIsEarned), None);
    }

    #[test]
    fn a_report_renders_one_entry_per_check() {
        let report = report_of(vec![
            (Check::OutcomeCorrectness, Verdict::Passed),
            (
                Check::Capability(Capability::Assumptions),
                Verdict::Failed(
                    Failure::new(Breach::Refused, "declared, yet solve_assuming refused")
                        .with_fault(Fault::unsupported()),
                ),
            ),
        ]);
        assert_eq!(
            report.to_string(),
            "each corpus program's outcome is its known one: passed\n\
             the assumptions declaration is honest: failed — declared, yet solve_assuming \
             refused: unsupported request\n",
        );
    }

    #[test]
    fn a_verdict_s_line_breaks_are_indented_beneath_its_check() {
        let report = report_of(vec![(
            Check::OutcomeCorrectness,
            Verdict::Failed(
                Failure::new(Breach::Refused, "the solve was refused")
                    .with_fault(Fault::engine("first line\nsecond line")),
            ),
        )]);
        assert!(
            report.to_string().contains("first line\n    second line"),
            "{report}"
        );
    }

    #[test]
    fn a_failure_on_a_case_renders_the_case_first() {
        let failure = Failure::new(Breach::Misanswered, "read inconsistent").on("an even loop");
        assert_eq!(failure.to_string(), "an even loop: read inconsistent");
    }

    #[test]
    fn a_failure_carries_its_case_and_fault_as_data() {
        let failure = Failure::new(Breach::Refused, "the solve was refused")
            .with_fault(Fault::engine("gone"))
            .on("a fact");
        assert_eq!(failure.breach(), Breach::Refused);
        assert_eq!(failure.case(), Some("a fact"));
        assert_eq!(failure.fault().map(Fault::locus), Some(Locus::Engine));
    }

    #[test]
    fn every_check_renders_a_distinct_obligation() {
        let checks = OBLIGATIONS
            .into_iter()
            .chain(CAPABILITIES.map(Check::Capability));
        let rendered: BTreeSet<String> = checks.map(|check| check.to_string()).collect();
        assert_eq!(rendered.len(), OBLIGATIONS.len() + CAPABILITIES.len());
    }

    #[test]
    fn a_skipped_verdict_renders_its_reason() {
        let skipped = Verdict::Skipped("no ground program".to_owned());
        assert_eq!(skipped.to_string(), "skipped — no ground program");
    }

    #[test]
    fn every_locus_has_a_distinct_phrase() {
        let loci = [
            Locus::Program,
            Locus::Request,
            Locus::Resource,
            Locus::Engine,
            Locus::Adapter,
        ];
        let phrases: BTreeSet<&str> = loci.into_iter().map(locus_phrase).collect();
        assert_eq!(phrases.len(), loci.len());
    }

    #[test]
    fn the_echo_function_answers_its_arguments() {
        let arguments = [constant("a"), Symbol::number(1)];
        assert_eq!(Echo.call(&arguments), Ok(arguments.to_vec()));
    }

    // ---- The ground program's provenance, over a backend that exposes one ----

    /// A search that closes the space at once, with no model.
    struct Closed;

    impl Run for Closed {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            None
        }

        fn conclusion(&self) -> Option<Conclusion> {
            Some(Conclusion::Exhausted)
        }
    }

    /// How a grounding backend builds the ground program of what it lowers.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Grounds {
        /// One rule per origin of the lowered program's statements.
        Faithfully,
        /// Those, and one more attributed to a statement it never saw.
        Inventing,
        /// Nothing at all.
        Nothing,
    }

    /// A backend exposing a ground program built from the program it lowered —
    /// built here, where a ground program is.
    struct Grounding {
        grounds: Grounds,
        ground: GroundProgram,
    }

    impl Backend for Grounding {
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Ok(Solved::running(Box::new(Closed), Scenario::default()))
        }

        fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
            let Door::Program(program) = door else {
                return Err(Fault::unsupported());
            };
            let mut origins: Vec<Origin> = match self.grounds {
                Grounds::Nothing => Vec::new(),
                Grounds::Faithfully | Grounds::Inventing => program
                    .statements()
                    .flat_map(|node| node.provenance().origins().cloned())
                    .collect(),
            };
            if self.grounds == Grounds::Inventing {
                origins.push(Origin::Constructed);
            }
            self.ground = GroundProgram {
                rules: origins
                    .into_iter()
                    .map(|origin| GroundRule { origin })
                    .collect(),
            };
            Ok(())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            Some(&self.ground)
        }
    }

    /// A grounding backend of the given kind, with nothing yet lowered.
    fn grounding(grounds: Grounds) -> Grounding {
        Grounding {
            grounds,
            ground: GroundProgram::default(),
        }
    }

    #[test]
    fn a_ground_program_attributing_every_rule_to_a_statement_passes() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Faithfully), &corpus());
        assert_eq!(verdict, Verdict::Passed);
    }

    #[test]
    fn a_ground_rule_attributed_to_no_statement_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Inventing), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(failure) if failure.breach() == Breach::Misanswered),
            "{verdict}"
        );
    }

    #[test]
    fn an_empty_ground_program_for_a_fact_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Nothing), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(failure) if failure.case() == Some("a fact")),
            "{verdict}"
        );
    }
}
