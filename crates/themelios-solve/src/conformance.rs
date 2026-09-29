//! The conformance suite (docs/design/solve.md §13.1): the executable suite
//! every adapter passes. [`run`] drives a backend through the contract (§4) — a
//! corpus of small programs whose answer sets are known independently of any
//! engine, each capability as declared, and the pathologies the vocabulary
//! forbids (§5.3) — and returns a [`ConformanceReport`]: typed data, one
//! [`Verdict`] per [`Check`], never prose a consumer parses (§1.3).
//!
//! The suite checks what the compiler cannot. **Outcome correctness:** each
//! corpus program's determination is its known one, and — where the backend
//! enumerates — its complete collection is exactly its known answer sets. **The
//! `enumeration` bit's soundness obligation** (§4.1): a search concluded as
//! closing the space yielded every answer set there is, since every universal
//! reading trusts that conclusion. **Capability honesty, in both directions**
//! (§4.1, §4.2): a declared capability's method answers — a provided method
//! needs no override, so a declared bit whose method still refuses is a lie the
//! type cannot see — and an undeclared one's refuses at the request locus,
//! never degrading silently. **The refusal to assign an atom that is not
//! external,** at the request locus, never the silent no-op an engine may give.
//! **The ground program's provenance,** where the backend exposes one (§10.4):
//! every ground rule attributed to a statement of the program it grounds.
//!
//! The named pathologies are unconstructible in the vocabulary (§5.3). The
//! suite attempts the one a backend could reach at run time — a touched stream
//! passing as a complete collection — and the `Inconsistent ⇒ Exhausted`
//! invariant the core's classification guarantees. The other two — an
//! enumeration reporting an improving trajectory, and a proven optimum built
//! outside this crate — are compile-time facts the crate's compile-fail
//! witnesses pin. That a cancelled search never concludes as closing the space
//! is reported skipped: the interrupt handle is reserved, so no search can yet
//! be cancelled through it.

use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use themelios_program::raise::raise_str;
use themelios_program::{Dialect, Name, Origin, Program, Sign, Symbol};

use crate::agent::{Assumption, Scenario};
use crate::bridge::Door;
use crate::contract::{
    Backend, Capabilities, ConsequenceRequest, ConsequenceSupport, Fault, GroundOptions, Locus,
    Mode, OptimizeRequest, SolveRequest, TruthValue,
};
use crate::extend::{Function, GroundFault, Propagator};
use crate::outcome::{AnswerSet, Conclusion, Consequences, Determination};

// ---- The report ----

/// Run the suite over `backend` (docs/design/solve.md §13.1): the corpus, each
/// capability as declared, and the pathologies attempted — a [`Verdict`] per
/// [`Check`] in the returned report. Total: whatever the backend answers,
/// refuses, or faults with becomes a verdict. The suite loads its own programs —
/// on a multi-shot backend a `reset` first, since `lower` accumulates there
/// (§6.2) — and registers probe extensions where the backend declares them, so
/// the backend's state afterwards is the suite's: run it over a backend kept
/// for it. Cost: a handful of solves per corpus program.
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
            cancellation_is_not_exhaustion(),
        ),
        (
            Check::GroundProgramIsFaithful,
            ground_program_is_faithful(backend, &corpus),
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
    /// One line per check: the obligation, then its verdict.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (check, verdict) in &self.entries {
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
    /// backend enumerates, its complete collection is its known answer sets.
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
    /// statement of the program it grounds (§10.4).
    GroundProgramIsFaithful,
    /// Assigning a truth value to an atom that is not external refuses at the
    /// request locus.
    NonExternalAssignmentRefuses,
    /// The capability's declaration is honest: declared, its method answers;
    /// undeclared, its method refuses at the request locus (§4.1, §4.2).
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
            Check::NonExternalAssignmentRefuses => {
                f.write_str("assigning an atom that is not external refuses")
            }
            Check::Capability(capability) => write!(f, "the {capability} declaration is honest"),
        }
    }
}

/// A declared capability the suite holds to its declaration (docs/design/solve.md
/// §4.1): each gates the contract method that is its sole engine primitive.
/// Non-exhaustive: a capability the contract grows is a new variant, not a
/// migration.
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
    /// Evaluating `@`-functions — `register_function` (§7).
    Functions,
    /// Running custom propagators — `register_propagator` (§8).
    Propagators,
    /// Interrupting an in-flight solve — `interrupt` (§6.1, §6.3).
    Cancellation,
    /// Enforcing a time budget on a solve (§6.3).
    TimeBudget,
}

impl fmt::Display for Capability {
    /// The capability, as a phrase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Capability::Optimization => "optimization",
            Capability::NativeConsequences => "native-consequences",
            Capability::Assumptions => "assumptions",
            Capability::MultiShot => "multi-shot",
            Capability::Functions => "@-functions",
            Capability::Propagators => "propagators",
            Capability::Cancellation => "cancellation",
            Capability::TimeBudget => "time-budget",
        })
    }
}

/// Every capability the suite probes, in the order it probes them: the
/// extension registrations last, so no probe runs against an engine carrying
/// another's registration.
const CAPABILITIES: [Capability; 8] = [
    Capability::Optimization,
    Capability::NativeConsequences,
    Capability::Assumptions,
    Capability::MultiShot,
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
    /// The backend breaks the obligation; the message says where.
    Failed(String),
    /// The obligation does not bind the backend — it rests on a capability the
    /// backend does not declare, or the suite cannot yet drive it; the message
    /// says which.
    Skipped(String),
}

impl fmt::Display for Verdict {
    /// The verdict, then its reason where it carries one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Passed => f.write_str("passed"),
            Verdict::Failed(why) => write!(f, "failed — {why}"),
            Verdict::Skipped(why) => write!(f, "skipped — {why}"),
        }
    }
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
    /// The case named `name`: the program `source` denotes, with `answer_sets`.
    fn new(
        name: &'static str,
        source: &'static str,
        answer_sets: impl IntoIterator<Item = AnswerSet>,
    ) -> Case {
        let mut answer_sets: Vec<AnswerSet> = answer_sets.into_iter().collect();
        answer_sets.sort();
        Case {
            name,
            source,
            program: program_of(source),
            answer_sets,
        }
    }

    /// Whether the program has an answer set.
    fn is_consistent(&self) -> bool {
        !self.answer_sets.is_empty()
    }
}

/// The program `source` denotes, in the clingo dialect. A suite program is
/// fixed text far within the coordinate limit — the one condition raising
/// refuses — so the expect discharges an invariant.
fn program_of(source: &str) -> Program {
    raise_str(source, Dialect::Clingo)
        .expect("a suite program is far within the coordinate limit")
        .into_program()
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

/// The corpus.
fn corpus() -> Vec<Case> {
    let p = |n| atom("p", [Symbol::number(n)], Sign::Positive);
    let q = |n| atom("q", [Symbol::number(n)], Sign::Positive);
    vec![
        Case::new("the empty program", "", [answer_set([])]),
        Case::new("a fact", FACT, [answer_set([constant("a")])]),
        Case::new(
            "an even loop",
            EVEN_LOOP,
            [answer_set([constant("a")]), answer_set([constant("b")])],
        ),
        Case::new("an odd loop", "a :- not a.", []),
        Case::new("a violated constraint", "a. :- a.", []),
        Case::new(
            "classical negation",
            "-a. b :- -a.",
            [answer_set([atom("a", [], Sign::Negative), constant("b")])],
        ),
        Case::new(
            "a disjunction",
            "a ; b.",
            [answer_set([constant("a")]), answer_set([constant("b")])],
        ),
        Case::new(
            "a choice",
            "{ a }.",
            [answer_set([]), answer_set([constant("a")])],
        ),
        Case::new(
            "a rule over an interval",
            "p(1..2). q(X) :- p(X).",
            [answer_set([p(1), p(2), q(1), q(2)])],
        ),
    ]
}

// ---- The checks ----

/// Load `program` as the whole of what the backend reasons over (§6.2): on a
/// multi-shot backend, whose `lower` accumulates, a `reset` first.
fn load(backend: &mut dyn Backend, program: &Program) -> Result<(), Fault> {
    if backend.capabilities().multi_shot {
        backend.reset()?;
    }
    backend.lower(Door::Program(program))
}

/// Load `case`'s program, the refusal phrased for a verdict — naming the exact
/// source refused, so an adapter's author sees which program to reproduce.
fn load_case(backend: &mut dyn Backend, case: &Case) -> Result<(), String> {
    load(backend, &case.program)
        .map_err(|fault| format!("the program `{}` was refused: {fault}", case.source))
}

/// A solve's refusal, phrased for a verdict.
fn refused_solve(fault: &Fault) -> String {
    format!("the solve was refused: {fault}")
}

/// The verdict of a per-case check over the corpus: the first case it finds
/// broken, named, or passed.
fn over_corpus(corpus: &[Case], mut check: impl FnMut(&Case) -> Result<(), String>) -> Verdict {
    for case in corpus {
        if let Err(why) = check(case) {
            return Verdict::Failed(format!("{}: {why}", case.name));
        }
    }
    Verdict::Passed
}

/// "consistent" or "inconsistent".
fn consistency(consistent: bool) -> &'static str {
    if consistent {
        "consistent"
    } else {
        "inconsistent"
    }
}

/// Each corpus program's determination is its known one; where the backend
/// declares `enumeration`, its complete collection is exactly its known answer
/// sets, and where it does not, every model it yields is one of them.
fn outcome_correctness(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    let enumerates = backend.capabilities().enumeration;
    over_corpus(corpus, |case| {
        load_case(backend, case)?;
        let mut solved = backend
            .solve(&SolveRequest::default())
            .map_err(|fault| refused_solve(&fault))?;
        let consistent = match solved.determination() {
            Determination::Consistent(_) => true,
            Determination::Inconsistent(_) => false,
            Determination::Inconclusive(partial) => return Err(Fault::from(partial).to_string()),
        };
        if consistent != case.is_consistent() {
            return Err(format!(
                "read {} where the program is {}",
                consistency(consistent),
                consistency(case.is_consistent()),
            ));
        }
        if enumerates {
            let mut found = solved
                .all_answer_sets()
                .map_err(|refusal| format!("the complete collection was refused: {refusal}"))?;
            found.sort();
            if found != case.answer_sets {
                return Err(format!(
                    "enumerated {} answer sets that are not its {} known ones",
                    found.len(),
                    case.answer_sets.len(),
                ));
            }
        } else {
            for yielded in solved.answer_sets() {
                let set = yielded.map_err(|fault| format!("the stream faulted: {fault}"))?;
                if case.answer_sets.binary_search(&set).is_err() {
                    return Err("yielded a set that is not one of its answer sets".to_owned());
                }
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
        load_case(backend, case)?;
        let mut solved = backend
            .solve(&SolveRequest::default())
            .map_err(|fault| refused_solve(&fault))?;
        let mut yielded: Vec<AnswerSet> = solved.answer_sets().map_while(Result::ok).collect();
        if solved.conclusion() == Some(Conclusion::Exhausted) {
            yielded.sort();
            if yielded != case.answer_sets {
                return Err(format!(
                    "concluded that the search closed the space having yielded {} sets, not its {} answer sets",
                    yielded.len(),
                    case.answer_sets.len(),
                ));
            }
        }
        Ok(())
    })
}

/// An inconsistent reading rests on a search that closed the space —
/// `Inconsistent ⇒ Exhausted`. The core's classification reads a clean end short
/// of the space as inconclusive, so no backend can break it; attempted all the
/// same, over every corpus program.
fn inconsistency_is_exhausted(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_corpus(corpus, |case| {
        load_case(backend, case)?;
        let mut solved = backend
            .solve(&SolveRequest::default())
            .map_err(|fault| refused_solve(&fault))?;
        let inconsistent = matches!(solved.determination(), Determination::Inconsistent(_));
        if inconsistent && solved.conclusion() != Some(Conclusion::Exhausted) {
            return Err("read inconsistent over a search that did not close the space".to_owned());
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
        load_case(backend, case)?;
        let mut solved = backend
            .solve(&SolveRequest::default())
            .map_err(|fault| refused_solve(&fault))?;
        drop(solved.answer_sets().next());
        if solved.all_answer_sets().is_ok() {
            return Err("a touched stream yielded a complete collection".to_owned());
        }
        Ok(())
    })
}

/// A cancelled search never concludes as closing the space. Not yet drivable:
/// the interrupt handle is reserved, so no search can be cancelled through it.
fn cancellation_is_not_exhaustion() -> Verdict {
    Verdict::Skipped(
        "the interrupt handle is reserved, so no search can yet be cancelled through it".to_owned(),
    )
}

/// Where the backend exposes its ground program, every ground rule is
/// attributed to a statement of the program it grounds (§10.4) — the provenance
/// an explanation reads back to source, never an invented one.
fn ground_program_is_faithful(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    let mut exposed = false;
    let verdict = over_corpus(corpus, |case| {
        load_case(backend, case)?;
        drop(
            backend
                .solve(&SolveRequest::default())
                .map_err(|fault| refused_solve(&fault))?,
        );
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
            return Err("a ground rule is attributed to no statement of the program".to_owned());
        }
        Ok(())
    });
    if verdict == Verdict::Passed && !exposed {
        return Verdict::Skipped("the backend exposes no ground program".to_owned());
    }
    verdict
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
    if let Err(fault) = load(backend, &program_of(FACT)) {
        return Verdict::Failed(format!("the program was refused: {fault}"));
    }
    match backend.assign_external(constant("a"), TruthValue::True) {
        Ok(()) => Verdict::Failed(
            "assigning an atom that is not external was accepted — a silent no-op".to_owned(),
        ),
        Err(fault) if fault == Fault::unsupported() => Verdict::Failed(
            "assign_external refused as unsupported, though the backend declares multi-shot solving"
                .to_owned(),
        ),
        Err(fault) if fault.locus() == Locus::Request => Verdict::Passed,
        Err(fault) => Verdict::Failed(format!(
            "assigning an atom that is not external refused at the {} locus, not the request's: {fault}",
            locus_phrase(fault.locus()),
        )),
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
    /// The probe could not reach the method: the phrase says why.
    Unprobed(String),
}

/// The response a method's result makes: answered, or refused with its fault.
fn respond<T>(result: Result<T, Fault>) -> Response {
    match result {
        Ok(_) => Response::Answered,
        Err(fault) => Response::Refused(fault),
    }
}

/// The probe that could not load its program.
fn unprobed(fault: &Fault) -> Response {
    Response::Unprobed(format!("its program was refused: {fault}"))
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
        Capability::Functions => capabilities.functions,
        Capability::Propagators => capabilities.propagators,
        Capability::Cancellation => capabilities.cancellation,
        Capability::TimeBudget => capabilities.budgets.time,
    }
}

/// The contract method a capability's probe drives, as a verdict names it.
fn method_of(capability: Capability) -> &'static str {
    match capability {
        Capability::Optimization => "optimize",
        Capability::NativeConsequences => "consequences_native",
        Capability::Assumptions => "solve_assuming",
        Capability::MultiShot => "the multi-shot methods",
        Capability::Functions => "register_function",
        Capability::Propagators => "register_propagator",
        Capability::Cancellation => "interrupt",
        Capability::TimeBudget => "a solve under a time budget",
    }
}

/// One capability's declaration is honest (§4.1, §4.2): declared, its method
/// answers; undeclared, it refuses at the request locus.
fn capability_is_honest(
    backend: &mut dyn Backend,
    capability: Capability,
    corpus: &[Case],
) -> Verdict {
    let declared = is_declared(capability, &backend.capabilities());
    let response = match capability {
        Capability::Optimization => probe_optimization(backend),
        Capability::NativeConsequences => probe_native_consequences(backend, corpus),
        Capability::Assumptions => probe_assumptions(backend),
        Capability::MultiShot => probe_multi_shot(backend, declared),
        Capability::Functions => respond(backend.register_function(Box::new(Echo))),
        Capability::Propagators => respond(backend.register_propagator(Box::new(Inert))),
        // `interrupt` answers `None` where another method refuses (§4.1).
        Capability::Cancellation => match backend.interrupt() {
            Some(_) => Response::Answered,
            None => Response::Refused(Fault::unsupported()),
        },
        Capability::TimeBudget => probe_time_budget(backend),
    };
    judge(method_of(capability), declared, response)
}

/// The verdict on a probe's response to a capability declared, or not.
fn judge(method: &str, declared: bool, response: Response) -> Verdict {
    match (declared, response) {
        (_, Response::Unprobed(why)) => {
            Verdict::Failed(format!("{method} could not be probed: {why}"))
        }
        (true, Response::Answered) => Verdict::Passed,
        (true, Response::Refused(fault)) => {
            Verdict::Failed(format!("declared, yet {method} refused: {fault}"))
        }
        (true, Response::Misanswered(why)) => {
            Verdict::Failed(format!("declared, yet {method} {why}"))
        }
        (false, response) if refuses_at_the_request(&response) => Verdict::Passed,
        (false, Response::Refused(fault)) => Verdict::Failed(format!(
            "undeclared, and {method} refused at the {} locus, not the request's: {fault}",
            locus_phrase(fault.locus()),
        )),
        (false, Response::Answered | Response::Misanswered(_)) => Verdict::Failed(format!(
            "undeclared, yet {method} answered: a request beyond the declaration must refuse"
        )),
    }
}

/// Optimization's probe: the optimum of a small program.
fn probe_optimization(backend: &mut dyn Backend) -> Response {
    if let Err(fault) = load(backend, &program_of(FACT)) {
        return unprobed(&fault);
    }
    respond(backend.optimize(&OptimizeRequest::default()))
}

/// The native door's probe: over every consistent corpus program, its cautious
/// and brave consequences are its known answer sets' `⋂` and `⋃`; over every
/// inconsistent one, it refuses — `⋂`/`⋃` over no answer set is undefined, not
/// `∅` (§4.1). Where the backend also declares `assumptions` — the only backend
/// handed a non-empty scenario (§6.2) — the door ranges over the models a
/// request's scenario admits, so it agrees with the fold over `solve_assuming`.
fn probe_native_consequences(backend: &mut dyn Backend, corpus: &[Case]) -> Response {
    let assumes = backend.capabilities().assumptions;
    let (consistent, inconsistent): (Vec<&Case>, Vec<&Case>) =
        corpus.iter().partition(|case| case.is_consistent());
    for case in consistent {
        if let Err(fault) = load(backend, &case.program) {
            return unprobed(&fault);
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
        if let Err(fault) = load(backend, &case.program) {
            return unprobed(&fault);
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
    if let Err(fault) = load(backend, &program_of(EVEN_LOOP)) {
        return unprobed(&fault);
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
/// loop, it admits exactly `{a}`, or exactly `{b}`. A suite constant is an atom,
/// so the expect discharges an invariant.
fn fixing_a(holds: bool) -> Scenario {
    let assumption = Assumption::new(constant("a"), holds).expect("a suite constant is an atom");
    [assumption].into_iter().collect()
}

/// Assumptions' probe: the even loop under a scenario fixing `a` to hold, then
/// not to — each narrowed to the one answer set the scenario admits, the models
/// ranging over the scenario asked.
fn probe_assumptions(backend: &mut dyn Backend) -> Response {
    let enumerates = backend.capabilities().enumeration;
    if let Err(fault) = load(backend, &program_of(EVEN_LOOP)) {
        return unprobed(&fault);
    }
    for (holds, admitted) in [(true, "a"), (false, "b")] {
        let scenario = fixing_a(holds);
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
        if enumerates && solved.all_answer_sets() != Ok(vec![answer_set([constant(admitted)])]) {
            return Response::Misanswered(
                "enumerated models that are not the ones the scenario admits".to_owned(),
            );
        }
    }
    Response::Answered
}

/// Multi-shot's probe. Declared: `reset` and `ground` of no part answer — the
/// assignment of a non-external is its own check. Undeclared: all three
/// multi-shot methods refuse at the request locus; the first that does not is
/// the response.
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

/// A time budget no solve of the probe program approaches — so a solve under it
/// answers, never concluding at the budget.
const PROBE_BUDGET: Duration = Duration::from_mins(1);

/// The time budget's probe: a small program solved under a budget.
fn probe_time_budget(backend: &mut dyn Backend) -> Response {
    if let Err(fault) = load(backend, &program_of(FACT)) {
        return unprobed(&fault);
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

    #[test]
    fn every_suite_program_raises_without_a_diagnostic() {
        for case in corpus() {
            let raised = raise_str(case.source, Dialect::Clingo).expect("a suite program raises");
            assert!(raised.diagnostics().is_empty(), "{}", case.name);
        }
        for source in [FACT, EVEN_LOOP] {
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

    /// A report of the given entries — built here, where its field is.
    fn report_of(entries: Vec<(Check, Verdict)>) -> ConformanceReport {
        ConformanceReport { entries }
    }

    #[test]
    fn a_report_with_a_failure_does_not_conform() {
        let report = report_of(vec![
            (Check::OutcomeCorrectness, Verdict::Passed),
            (Check::ExhaustionIsEarned, Verdict::Failed("why".to_owned())),
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
    fn a_report_renders_one_line_per_check() {
        let report = report_of(vec![
            (Check::OutcomeCorrectness, Verdict::Passed),
            (
                Check::Capability(Capability::Assumptions),
                Verdict::Failed("declared, yet solve_assuming refused".to_owned()),
            ),
        ]);
        assert_eq!(
            report.to_string(),
            "each corpus program's outcome is its known one: passed\n\
             the assumptions declaration is honest: failed — declared, yet solve_assuming refused\n",
        );
    }

    #[test]
    fn every_check_renders_its_obligation() {
        let checks = [
            Check::OutcomeCorrectness,
            Check::ExhaustionIsEarned,
            Check::InconsistencyIsExhausted,
            Check::TruncationCannotPoseAsComplete,
            Check::CancellationIsNotExhaustion,
            Check::GroundProgramIsFaithful,
            Check::NonExternalAssignmentRefuses,
        ]
        .into_iter()
        .chain(CAPABILITIES.map(Check::Capability));
        let rendered: BTreeSet<String> = checks.map(|check| check.to_string()).collect();
        // Seven obligations and eight capability declarations, each phrased
        // apart from the rest.
        assert_eq!(rendered.len(), 15);
    }

    #[test]
    fn a_skipped_verdict_renders_its_reason() {
        let skipped = Verdict::Skipped("no ground program".to_owned());
        assert_eq!(skipped.to_string(), "skipped — no ground program");
    }

    #[test]
    fn every_locus_has_a_phrase() {
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

    /// A search that closes the space at once, with no model.
    struct Closed;

    impl crate::outcome::Run for Closed {
        fn next_answer_set(&mut self) -> Option<Result<AnswerSet, Fault>> {
            None
        }

        fn conclusion(&self) -> Option<Conclusion> {
            Some(Conclusion::Exhausted)
        }
    }

    /// A backend exposing a ground program with one ground rule per origin of
    /// the program it lowered — built here, where a ground program is — and,
    /// when it `invents`, one more attributed to a statement it never saw.
    struct Grounding {
        invents: bool,
        ground: crate::bridge::GroundProgram,
    }

    impl Backend for Grounding {
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<crate::outcome::Solved<'_>, Fault> {
            Ok(crate::outcome::Solved::running(
                Box::new(Closed),
                Scenario::default(),
            ))
        }

        fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
            let Door::Program(program) = door else {
                return Err(Fault::unsupported());
            };
            let mut origins: Vec<Origin> = program
                .statements()
                .flat_map(|node| node.provenance().origins().cloned())
                .collect();
            if self.invents {
                origins.push(Origin::Constructed);
            }
            self.ground = crate::bridge::GroundProgram {
                rules: origins
                    .into_iter()
                    .map(|origin| crate::bridge::GroundRule { origin })
                    .collect(),
            };
            Ok(())
        }

        fn ground_program(&self) -> Option<&crate::bridge::GroundProgram> {
            Some(&self.ground)
        }
    }

    /// A grounding backend that invents, or not, with nothing yet lowered.
    fn grounding(invents: bool) -> Grounding {
        Grounding {
            invents,
            ground: crate::bridge::GroundProgram { rules: Vec::new() },
        }
    }

    #[test]
    fn a_ground_program_attributing_every_rule_to_a_statement_passes() {
        let verdict = ground_program_is_faithful(&mut grounding(false), &corpus());
        assert_eq!(verdict, Verdict::Passed);
    }

    #[test]
    fn a_ground_rule_attributed_to_no_statement_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(true), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(why) if why.contains("attributed to no statement")),
            "{verdict}"
        );
    }

    #[test]
    fn the_echo_function_answers_its_arguments() {
        let arguments = [constant("a"), Symbol::number(1)];
        assert_eq!(Echo.call(&arguments), Ok(arguments.to_vec()));
    }
}
