//! The conformance suite (docs/design/solve.md §13.1): the executable suite
//! every adapter passes. [`run`] drives a backend through the contract (§4) — a
//! corpus of small programs whose answer sets and displays are known
//! independently of any engine, each lowered through both doors, each
//! capability as declared, and the pathologies the vocabulary forbids (§5.3) —
//! and returns a [`ConformanceReport`]: typed data, one [`Verdict`] per
//! [`Check`] in the order the suite runs them, a failure carrying how the
//! backend broke the obligation, the corpus program it broke it on, and the
//! backend's own fault, a skip carrying why the check did not bind — never
//! prose a consumer parses (§1.3).
//!
//! The suite checks what the compiler cannot, one obligation to a check, in
//! the order its [`Check`] enum lists them. **Outcome correctness:** each corpus
//! program, lowered through Door A as a parse the core admits and through Door
//! B as a program, the two agreeing, has its known determination; every model
//! it yields is consistent — no atom beside its contrary — and a set of
//! literals; and, where the backend enumerates, its answer sets are exactly its
//! known ones, its search closing the space, every stream ending for good once
//! it ends. The corpus holds the programs a shortcut semantics gets wrong: a
//! positive loop, which completion reads with an unsupported model, the same
//! loop constrained to hold, which completion reads as consistent, and a head
//! cycle, which shifting the disjunction reads with no model; a choice under an
//! objective, whose solve yields its non-optimal model too (§5.2); programs
//! whose `#show` directives hide atoms or display terms, whose models carry
//! their whole answer sets whatever they display (§5.1); a projection, whose
//! models range over every stable model whole; counted repeats, which reach the
//! engine as written; and a tuple counted once. **The display:** each model's
//! display is the one the program's directives select. **The `enumeration`
//! bit's soundness obligation** (§4.1): a search concluded as closing the space
//! yielded every answer set there is. **Inconsistency is exhausted,
//! truncation cannot pose as complete, and cancellation is not exhaustion** —
//! the pathologies a run can attempt (below). **The observer,** where the
//! backend declares it (§10.4): a ground program once a grounding has finished,
//! every ground rule naming a statement of the program lowered — through Door
//! A, an occurrence of the parse — and a fact never grounded to nothing.
//! **Fault loci:** a program the backend cannot
//! ground is refused at the program locus, naming the statement that cannot be
//! grounded and located within it (§5.4); assigning an atom that is not
//! external is refused at the request locus, never the silent no-op an engine
//! may give — and a statement built in Rust is refused unlocated, since no
//! span was written for it. **A rebuild leaves nothing behind** (§4.1, §6.3):
//! no statement, model, or cancellation of the run it replaced. **A backend's
//! own state** (§4.1): a refusal its lowering's check makes adds nothing; a
//! failed grounding leaves a multi-shot backend refusing as needing its rebuild
//! until the `reset` that rebuilds, and fails a single-shot backend's solve,
//! the backend staying ready; and a registration survives the rebuild.
//! **Capability honesty, in both directions** (§4.1, §4.2): a declared
//! capability's method answers, and answers rightly — a provided method needs
//! no override, so a declared bit whose method still refuses is a lie the type
//! cannot see — and an undeclared one's refuses as unsupported, naming the
//! capability, or, for `interrupt` and the observer, answers nothing, never
//! degrading silently, while a budget nothing realises refuses as unrealisable;
//! the native door's answer is its known one, no model over a program with
//! none.
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
//! Every stream the suite reads, it reads to one model past its program's
//! answer sets, so a run that never ends is caught at that bound rather than
//! holding the suite: outcome correctness fails it there, as does a capability's
//! probe. A check that cannot be driven over a backend — its program refused,
//! its stream faulted or run past its bound — is skipped with the failure that
//! stopped it, and the check that owns that failure fails. A program only a
//! capability's probe lowers, refused, fails that capability's check, though:
//! no other check would see the refusal.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

use themelios_base::span::{ByteOffset, Location};
use themelios_program::program::{Part, PartKey};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::{raise_source, raise_str};
use themelios_program::symbol::VarName;
use themelios_program::{
    Atom, Dialect, Name, Origin, Program, Rule, Sign, Source, SourceId, Statement, Symbol, Term,
};
use themelios_syntax::parse;

use crate::agent::{Assumption, Scenario};
use crate::bridge::{Admitted, Door};
use crate::contract::{
    Backend, Capabilities, Capability, ConsequenceRequest, ConsequenceSupport, Fault,
    GroundOptions, Locus, Mode, OptimizeRequest, Presupposition, Refused, SolveRequest, TruthValue,
};
use crate::extend::{Function, GroundFault, Propagator};
use crate::outcome::{
    AnswerSet, Conclusion, Consequences, Determination, NativeAnswer, Solved, Stopped,
};

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
        (Check::Display, display_is_selected(backend, &corpus)),
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
        (Check::FaultLoci, fault_loci(backend)),
        (
            Check::RebuildLeavesNothingBehind,
            rebuild_leaves_nothing_behind(backend),
        ),
        (Check::BackendState, backend_state(backend)),
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
    /// Each corpus program, lowered through both doors, which agree, has its
    /// known determination; every model it yields is consistent and a set of
    /// literals; and where the backend enumerates, its answer sets are its
    /// known ones, its search closing the space.
    OutcomeCorrectness,
    /// Each model's display is the one the program's directives select (§5.1).
    Display,
    /// A search concluded as closing the space yielded every answer set there
    /// is — the `enumeration` bit's soundness obligation (§4.1).
    ExhaustionIsEarned,
    /// An inconsistent reading rests on a search that closed the space.
    InconsistencyIsExhausted,
    /// A stream once touched cannot yield a complete collection (§5.3).
    TruncationCannotPoseAsComplete,
    /// A cancelled search never concludes as closing the space.
    CancellationIsNotExhaustion,
    /// A declared observer answers once a grounding has finished, every ground
    /// rule naming a statement of the program lowered, and a fact grounds to a
    /// rule (§10.4, §13.1).
    GroundProgramIsFaithful,
    /// Each fault lands where it belongs (§5.4): a program the backend cannot
    /// ground is refused at the program locus, naming the statement that cannot
    /// be grounded — located where it was written, unlocated where it was
    /// built in Rust; assigning an atom that is not external refuses naming it
    /// as one.
    FaultLoci,
    /// A rebuild carries no statement, model, or cancellation of the run it
    /// replaced into the next (§4.1, §6.3).
    RebuildLeavesNothingBehind,
    /// A backend's own state (§4.1): a refusal its lowering's check makes adds
    /// nothing, a failed grounding leaves it needing its rebuild, and a
    /// registration survives the rebuild.
    BackendState,
    /// The capability's declaration is honest: declared, its method answers
    /// rightly; undeclared, it refuses as unsupported, naming the capability —
    /// a budget, as unrealisable — or answers nothing (§4.1, §4.2, §6.3).
    Capability(Capability),
}

impl fmt::Display for Check {
    /// The obligation, as a phrase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Check::OutcomeCorrectness => {
                f.write_str("each corpus program's outcome is its known one")
            }
            Check::Display => {
                f.write_str("each model displays what the program's directives select")
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
                f.write_str("every ground rule names a statement of the program lowered")
            }
            Check::FaultLoci => f.write_str("each fault lands where it belongs"),
            Check::RebuildLeavesNothingBehind => {
                f.write_str("a rebuild leaves nothing of the program it replaced")
            }
            Check::BackendState => {
                f.write_str("a backend's own state follows its refusals and rebuilds")
            }
            Check::Capability(capability) => write!(f, "the declaration of {capability} is honest"),
        }
    }
}

/// A capability's row (docs/design/solve.md §4.1): the contract method — or
/// request — its probe drives, and the probe, so a capability the contract
/// grows is one row; the contract's `Display` names it, and its declaration
/// reads which bit it is. The probe is handed whether the capability is
/// declared, and the corpus.
#[derive(Clone, Copy)]
struct Row {
    method: &'static str,
    probe: fn(&mut dyn Backend, bool, &[Case]) -> Response,
}

/// A capability's row — the table, written as one exhaustive match, so a
/// capability the contract gains has no reading until its row is written. O(1).
fn row(capability: Capability) -> Row {
    match capability {
        Capability::Optimization => Row {
            method: "optimize",
            probe: |backend, _, _| probe_optimization(backend),
        },
        Capability::NativeConsequences => Row {
            method: "consequences_native",
            probe: |backend, _, corpus| probe_native_consequences(backend, corpus),
        },
        Capability::Assumptions => Row {
            method: "solve_assuming",
            probe: |backend, _, _| probe_assumptions(backend),
        },
        Capability::MultiShot => Row {
            method: "the multi-shot methods",
            probe: |backend, declared, _| probe_multi_shot(backend, declared),
        },
        Capability::Externals => Row {
            method: "assign_external",
            probe: |backend, _, _| probe_externals(backend),
        },
        Capability::Cancellation => Row {
            method: "interrupt",
            // `interrupt` answers `None` where another method refuses (§4.1).
            probe: |backend, _, _| match backend.interrupt() {
                Some(_) => Response::Answered,
                None => Response::Absent,
            },
        },
        Capability::TimeBudget => Row {
            method: "a solve under a time budget",
            probe: |backend, declared, _| probe_time_budget(backend, declared),
        },
        Capability::GroundProgram => Row {
            method: "ground_program",
            probe: |backend, _, _| probe_ground_program(backend),
        },
        Capability::Functions => Row {
            method: "register_function",
            probe: |backend, _, _| respond(backend.register_function(Box::new(Echo))),
        },
        Capability::Propagators => Row {
            method: "register_propagator",
            probe: |backend, _, _| respond(backend.register_propagator(Box::new(Inert))),
        },
    }
}

/// Every capability the suite probes, in the order it probes them: the
/// extension registrations last, so no probe runs against an engine carrying
/// another's registration.
const CAPABILITIES: [Capability; 10] = [
    Capability::Optimization,
    Capability::NativeConsequences,
    Capability::Assumptions,
    Capability::MultiShot,
    Capability::Externals,
    Capability::Cancellation,
    Capability::TimeBudget,
    Capability::GroundProgram,
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
    /// The obligation does not bind the backend, for the reason the skip gives.
    Skipped(Skip),
}

impl fmt::Display for Verdict {
    /// The verdict, then its reason where it carries one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Passed => f.write_str("passed"),
            Verdict::Failed(failure) => write!(f, "failed — {failure}"),
            Verdict::Skipped(skip) => write!(f, "skipped — {skip}"),
        }
    }
}

/// Why a check did not bind a backend (docs/design/solve.md §13.1) — typed, so
/// a consumer can accept one kind of skip and refuse another (a check the
/// backend's own refusal left undriven, say), with the sentence a person reads
/// as its `Display`. Non-exhaustive: a reason the suite comes to give is a new
/// variant, not a migration.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Skip {
    /// The obligation rests on a capability the backend does not declare.
    Undeclared(Capability),
    /// The contract reserves what the check needs — the interrupt handle — so
    /// no backend can yet be driven through it.
    Reserved,
    /// The backend refused, faulted, or ran on at a step before the obligation,
    /// as the failure says — a failure the check that owns that step reports.
    Undriven(Failure),
}

impl fmt::Display for Skip {
    /// The reason, as a phrase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Skip::Undeclared(capability) => write!(f, "the backend does not declare {capability}"),
            Skip::Reserved => f.write_str(
                "the contract reserves the interrupt handle, so no search can yet be cancelled through it",
            ),
            Skip::Undriven(failure) => write!(f, "could not be driven: {failure}"),
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
    /// The program the source denotes, for Door B.
    program: Program,
    /// The source's parse, admitted, for Door A.
    admitted: Admitted,
    /// The answer sets, sorted, so a sorted enumeration compares equal to them.
    answer_sets: Vec<AnswerSet>,
    /// Each answer set's display, the one the program's directives select —
    /// parallel to `answer_sets`.
    displays: Vec<BTreeSet<Symbol>>,
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

/// The parse of `source` as the source text `id` names, admitted at Door A
/// (§10.2). Every suite program raises without a diagnostic — a law of the
/// suite's own — so the expects discharge invariants.
fn admitted_under(id: SourceId, source: &str) -> Admitted {
    let source = Source::new(id, source.to_owned())
        .expect("a suite program is far within the coordinate limit");
    Admitted::of(&parse(&source, Dialect::Clingo)).expect("a suite program is admitted")
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

/// A choice under an objective, whose proven optimum is `{}` — the program the
/// optimization probe asks `optimize` about — and whose stable models, the
/// objective ignored as a solve ignores it, are `{}` and `{a}`.
const OPTIMIZATION: &str = "{ a }. #minimize { 1 : a }.";

/// The positive loop over `a` and `b`: one answer set, `{}` — completion also
/// admits the unsupported `{a, b}`.
const POSITIVE_LOOP: &str = "a :- b. b :- a.";

/// The positive loop constrained to hold `a`: no answer set, though completion
/// admits `{a, b}`.
const CONSTRAINED_LOOP: &str = "a :- b. b :- a. :- not a.";

/// The rule the multi-shot probe lowers over the fact `a.`, with no reset
/// between them: together, `{a, b}`.
const RULE: &str = "b :- a.";

/// An external atom and a rule over it: `{a, b}` while `a` is assigned true,
/// `{}` while it is assigned false.
const EXTERNAL: &str = "#external a. b :- a.";

/// A fact, then a fact over a variable nothing binds: no grounder can
/// instantiate the second statement, so a refusal is located at it, not at the
/// program's start.
const UNSAFE: &str = "a. p(X).";

/// A fact other than `a.`, then a fact over a variable nothing binds: the
/// program the backend-state check has a backend refuse at its lowering's
/// check, whose prefix a backend keeping it would show.
const PREFIXED_UNSAFE: &str = "b. p(X).";

/// The fact a rebuild replaces `a.` with: one answer set, `{b}`.
const REBUILT: &str = "b.";

/// A fact over a call of the suite's faulting `@`-function: its grounding
/// fails.
const FAULTING_CALL: &str = "p(@fault).";

/// A fact over a call of the suite's echoing `@`-function: `{p(1)}`.
const ECHOED_CALL: &str = "p(@echo(1)).";

/// The source id the unsafe program is raised under: one no corpus program has
/// (theirs count up from zero), so a location in any of them is not its own.
const UNSAFE_SOURCE: SourceId = SourceId::new(u32::MAX);

/// A corpus entry: how a verdict names the program, its source, and its
/// models — each answer set beside its display.
type Entry = (
    &'static str,
    &'static str,
    Vec<(AnswerSet, BTreeSet<Symbol>)>,
);

/// The corpus: small programs whose answer sets and displays are known
/// independently of any engine, each raised and admitted under its own source
/// id.
fn corpus() -> Vec<Case> {
    displaying_themselves()
        .into_iter()
        .chain(hiding_or_displaying())
        .zip(0..)
        .map(|((name, source, mut models), id)| {
            models.sort();
            let (answer_sets, displays) = models.into_iter().unzip();
            Case {
                name,
                source,
                program: program_under(SourceId::new(id), source),
                admitted: admitted_under(SourceId::new(id), source),
                answer_sets,
                displays,
            }
        })
        .collect()
}

/// The corpus programs without `#show` directives — each model displaying its
/// answer set.
fn displaying_themselves() -> Vec<Entry> {
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
        // A solve asks for the stable models, the objective ignored: an engine
        // optimising by default reads only the optimum, {}.
        (
            "a choice under an objective",
            OPTIMIZATION,
            vec![answer_set([]), answer_set([constant("a")])],
        ),
        (
            "a rule over an interval",
            "p(1..2). q(X) :- p(X).",
            vec![answer_set([p(1), p(2), q(1), q(2)])],
        ),
        // Completion admits the unsupported {a, b}; the stable reading is {}.
        ("a positive loop", POSITIVE_LOOP, vec![answer_set([])]),
        // Completion admits {a, b}; the stable reading has no answer set.
        ("a constrained positive loop", CONSTRAINED_LOOP, vec![]),
        // Shifting the disjunction leaves no answer set; the stable reading is
        // {a, b}.
        (
            "a head cycle",
            "a ; b. a :- b. b :- a.",
            vec![answer_set([constant("a"), constant("b")])],
        ),
        // `#project` restricts what an engine reports, never the stable models:
        // a solve ranges over every one, whole (§5.2).
        (
            "a projection",
            "a. {b}. c. #project c/0.",
            vec![
                answer_set([constant("a"), constant("c")]),
                answer_set([constant("a"), constant("b"), constant("c")]),
            ],
        ),
        // Counted repeats reach the engine as written (program.md §4.4).
        ("a kept boolean repeat", "1 { #true; #true } 1.", vec![]),
        (
            "a repeat made by a pool",
            "{ #true : p(1;1) } = 2. p(1).",
            vec![answer_set([p(1)])],
        ),
        // A tuple counts once however many of its conditions hold (program.md
        // §4.7).
        (
            "a tuple counted once",
            "x :- #count{ 1 : a; 1 : b } = 1. a. b.",
            vec![answer_set([constant("a"), constant("b"), constant("x")])],
        ),
    ];
    known
        .into_iter()
        .map(|(name, source, sets)| {
            let models = sets.into_iter().map(|set| (set.clone(), set)).collect();
            (name, source, models)
        })
        .collect()
}

/// The corpus programs whose `#show` directives hide atoms or display terms:
/// each model carries its whole answer set, whatever it displays (§5.1).
fn hiding_or_displaying() -> Vec<Entry> {
    vec![
        (
            "a hidden atom",
            "a. #show.",
            vec![(answer_set([constant("a")]), answer_set([]))],
        ),
        (
            "a choice, hidden",
            "{a}. #show.",
            vec![
                (answer_set([]), answer_set([])),
                (answer_set([constant("a")]), answer_set([])),
            ],
        ),
        (
            "a displayed term",
            "q. #show p : q.",
            vec![(
                answer_set([constant("q")]),
                answer_set([constant("p"), constant("q")]),
            )],
        ),
        (
            "a displayed term alone",
            "q. #show. #show p : q.",
            vec![(answer_set([constant("q")]), answer_set([constant("p")]))],
        ),
        (
            "a hidden negation",
            "-p. #show q/0.",
            vec![(answer_set([atom("p", [], Sign::Negative)]), answer_set([]))],
        ),
    ]
}

// ---- Driving a backend ----

/// Why a per-case check fell short of holding on a case.
enum Shortfall {
    /// The backend broke the obligation.
    Broke(Failure),
    /// The check could not be driven over the case, as the failure to drive it
    /// says — a breach of no obligation this check holds.
    Undriven(Failure),
}

/// The verdict of a per-case check over the corpus: the first case it finds
/// broken, named; otherwise skipped with the first case it could not drive, if
/// any; otherwise passed.
fn over_corpus(corpus: &[Case], mut check: impl FnMut(&Case) -> Result<(), Shortfall>) -> Verdict {
    let mut undrivable = None;
    for case in corpus {
        match check(case) {
            Ok(()) => {}
            Err(Shortfall::Broke(failure)) => return Verdict::Failed(failure.on(case.name)),
            Err(Shortfall::Undriven(failure)) => {
                undrivable.get_or_insert_with(|| failure.on(case.name));
            }
        }
    }
    undrivable.map_or(Verdict::Passed, |failure| {
        Verdict::Skipped(Skip::Undriven(failure))
    })
}

/// The verdict of a per-case check over the corpus, each case driven through
/// both doors (§10.2) — its program at Door B, then its parse at Door A — and
/// judged as [`over_corpus`] judges it.
fn over_both_doors(
    corpus: &[Case],
    mut check: impl FnMut(&Case, Through) -> Result<(), Shortfall>,
) -> Verdict {
    over_corpus(corpus, |case| {
        check(case, Through::Program)?;
        check(case, Through::Parsed)
    })
}

/// Clear what a multi-shot backend has accumulated, so the next program is the
/// whole of what it reasons over (§6.2) — `lower` accumulates there. The
/// refusal names the step.
fn reset_to_load(backend: &mut dyn Backend) -> Result<(), Failure> {
    if backend.capabilities().multi_shot {
        backend.reset().map_err(|fault| {
            Failure::new(Breach::Refused, "the reset before loading was refused").with_fault(fault)
        })?;
    }
    Ok(())
}

/// Lower `program`, raised from `source`, as one more program the backend
/// reasons over; the refusal names the program.
fn lower_program(
    backend: &mut dyn Backend,
    program: &Program,
    source: &str,
) -> Result<(), Failure> {
    backend
        .lower(Door::Program(program))
        .map_err(|fault| Failure::new(Breach::Refused, refused_program(source)).with_fault(fault))
}

/// The refusal of the program `source`, as a phrase.
fn refused_program(source: &str) -> String {
    if source.is_empty() {
        "the empty program was refused".to_owned()
    } else {
        format!("the program `{source}` was refused")
    }
}

/// Load `program` — raised from `source` — as the whole of what the backend
/// reasons over: the reset a multi-shot backend needs, then the program.
fn load(backend: &mut dyn Backend, program: &Program, source: &str) -> Result<(), Failure> {
    reset_to_load(backend)?;
    lower_program(backend, program, source)
}

/// Load `case` through Door A — its parse, admitted (§10.2) — as the whole of
/// what the backend reasons over: the reset a multi-shot backend needs, then
/// the parse.
fn load_parsed(backend: &mut dyn Backend, case: &Case) -> Result<(), Failure> {
    reset_to_load(backend)?;
    backend
        .lower(Door::Parsed(&case.admitted))
        .map_err(|fault| {
            Failure::new(
                Breach::Refused,
                format!("{} at Door A", refused_program(case.source)),
            )
            .with_fault(fault)
        })
}

/// Load the program `source` denotes, as [`load`] does.
fn load_source(backend: &mut dyn Backend, source: &str) -> Result<(), Failure> {
    load(backend, &program_of(source), source)
}

/// Load a program only a capability's probe lowers: a refused reset leaves the
/// method unprobed, but the program refused is the capability's own refusal —
/// no other check would see it.
fn load_own(backend: &mut dyn Backend, source: &str) -> Result<(), Response> {
    reset_to_load(backend).map_err(Response::Unprobed)?;
    lower_program(backend, &program_of(source), source).map_err(Response::ProgramRefused)
}

/// Solve the loaded program, the refusal as a failure.
fn solve(backend: &mut dyn Backend) -> Result<Solved<'_>, Failure> {
    backend
        .solve(&SolveRequest::default())
        .map_err(|fault| Failure::new(Breach::Refused, "the solve was refused").with_fault(fault))
}

/// What a bounded read of a stream found: the answer sets of the models it
/// yielded, whether each of those models was consistent, and whether the
/// stream ended within the bound.
struct Pulled {
    sets: Vec<AnswerSet>,
    /// Each yielded model's display, parallel to `sets`.
    displays: Vec<BTreeSet<Symbol>>,
    consistent: bool,
    /// Whether every yielded model's answer set held only literals.
    literals: bool,
    ended: bool,
}

/// Read at most one model more than `bound` — enough to see the stream end, or
/// to see it yield past a program's answer sets — so a run that never ends is
/// caught at the bound rather than drained forever, noting each model's
/// display and whether each was consistent and a set of literals. A mid-stream
/// fault is the refusal.
fn pull(solved: &mut Solved<'_>, bound: usize) -> Result<Pulled, Fault> {
    let mut sets = Vec::new();
    let mut displays = Vec::new();
    let mut consistent = true;
    let mut literals = true;
    for yielded in solved.models() {
        let model = yielded?;
        if sets.len() == bound {
            return Ok(Pulled {
                sets,
                displays,
                consistent,
                literals,
                ended: false,
            });
        }
        consistent &= model.is_consistent();
        literals &= model.is_set_of_literals();
        sets.push(model.atoms().clone());
        displays.push(model.shown().symbols().clone());
    }
    Ok(Pulled {
        sets,
        displays,
        consistent,
        literals,
        ended: true,
    })
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

/// Which door a case is lowered through (§10.2).
#[derive(Clone, Copy)]
enum Through {
    /// Door A: the case's parse, admitted.
    Parsed,
    /// Door B: the case's program.
    Program,
}

/// Each corpus program, lowered through both doors (§10.2), has its known
/// determination; every model it yields is consistent — no atom beside its
/// contrary (query.md §2.3) — and a set of literals; and, where the backend
/// enumerates, its answer sets are its known ones, its search closing the
/// space. The doors agree because each is held to the program's known outcome:
/// the same determination, and, where the backend enumerates, the same answer
/// sets — a witness-only backend may name a different witness through each.
fn outcome_correctness(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    let enumerates = backend.capabilities().enumeration;
    over_both_doors(corpus, |case, through| {
        observe(backend, case, through, enumerates)
    })
}

/// Load `case` through the door `through` names.
fn load_through(backend: &mut dyn Backend, case: &Case, through: Through) -> Result<(), Failure> {
    match through {
        Through::Program => load(backend, &case.program, case.source),
        Through::Parsed => load_parsed(backend, case),
    }
}

/// One door's run of `case`, held to outcome correctness's per-run obligations.
fn observe(
    backend: &mut dyn Backend,
    case: &Case,
    through: Through,
    enumerates: bool,
) -> Result<(), Shortfall> {
    load_through(backend, case, through).map_err(Shortfall::Broke)?;
    let mut solved = solve(backend).map_err(Shortfall::Broke)?;
    let consistent = match solved.determination() {
        Determination::Consistent(models) => {
            if models.scenario().assumptions().next().is_some() {
                return Err(Shortfall::Broke(Failure::new(
                    Breach::Misanswered,
                    "ranged a plain solve's models over a scenario",
                )));
            }
            true
        }
        Determination::Inconsistent(_) => false,
        Determination::Inconclusive(partial) => {
            return Err(Shortfall::Broke(match partial.stopped() {
                Stopped::Concluded(truncation) => Failure::new(
                    Breach::Refused,
                    format!("the search stopped undecided: {truncation}"),
                ),
                Stopped::Faulted(fault) => {
                    Failure::new(Breach::Refused, "the search stopped at a fault, undecided")
                        .with_fault(fault.clone())
                }
            }));
        }
    };
    if consistent != case.is_consistent() {
        return Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            format!(
                "read {} where the program is {}",
                consistency(consistent),
                consistency(case.is_consistent()),
            ),
        )));
    }
    let Pulled {
        sets,
        consistent: each_consistent,
        literals,
        ended,
        ..
    } = pull(&mut solved, case.answer_sets.len())
        .map_err(|fault| Shortfall::Broke(faulted(fault)))?;
    if !ended {
        return Err(Shortfall::Broke(past_the_bound()));
    }
    if solved.models().next().is_some() {
        return Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            "yielded a model after its stream ended",
        )));
    }
    if solved.conclusion().is_none() {
        return Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            "ended its stream with its search still open",
        )));
    }
    if !each_consistent {
        return Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            "yielded a model holding an atom and its contrary",
        )));
    }
    if !literals {
        return Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            "yielded a model holding a symbol that is no literal",
        )));
    }
    if sets
        .iter()
        .any(|set| case.answer_sets.binary_search(set).is_err())
    {
        return Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            "yielded a set that is not one of its answer sets",
        )));
    }
    if enumerates {
        let mut sorted = sets;
        sorted.sort();
        if sorted != case.answer_sets {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                "enumerated sets other than exactly its answer sets",
            )));
        }
        if solved.conclusion() != Some(Conclusion::Exhausted) {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                "ended its search without closing the space",
            )));
        }
    }
    Ok(())
}

/// Each model's display is the one the program's directives select (§5.1):
/// for every model a corpus program yields, through either door, its display
/// is the case's known display for its answer set. A model whose answer set is
/// not one of the program's is outcome correctness's to judge, so it leaves
/// this check undriven.
fn display_is_selected(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_both_doors(corpus, |case, through| {
        load_through(backend, case, through).map_err(Shortfall::Undriven)?;
        let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
        let Pulled {
            sets,
            displays,
            ended,
            ..
        } = pull(&mut solved, case.answer_sets.len())
            .map_err(|fault| Shortfall::Undriven(faulted(fault)))?;
        if !ended {
            return Err(Shortfall::Undriven(past_the_bound()));
        }
        for (set, display) in sets.iter().zip(&displays) {
            let Ok(index) = case.answer_sets.binary_search(set) else {
                return Err(Shortfall::Undriven(Failure::new(
                    Breach::Misanswered,
                    "yielded a set that is not one of its answer sets",
                )));
            };
            if display != &case.displays[index] {
                return Err(Shortfall::Broke(Failure::new(
                    Breach::Misanswered,
                    "displayed other than the program's directives select",
                )));
            }
        }
        Ok(())
    })
}

/// `n` answer sets, as a phrase.
fn counted(n: usize) -> String {
    if n == 1 {
        "one answer set".to_owned()
    } else {
        format!("{n} answer sets")
    }
}

/// A stream read past its program's answer sets, as a failure.
fn past_the_bound() -> Failure {
    Failure::new(
        Breach::Misanswered,
        "yielded more answer sets than the program has",
    )
}

/// A search concluded as closing the space yielded every answer set there is —
/// the `enumeration` bit's soundness obligation (§4.1), held over every corpus
/// program through both doors. A backend that stops at
/// a witness, or anywhere short, must say so; every universal reading trusts an
/// `Exhausted` conclusion. A stream that faults, or runs past its bound, has
/// concluded nothing to judge: the case is undriven, and outcome correctness
/// fails it.
fn exhaustion_is_earned(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_both_doors(corpus, |case, through| {
        load_through(backend, case, through).map_err(Shortfall::Undriven)?;
        let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
        let Pulled { sets, ended, .. } = pull(&mut solved, case.answer_sets.len())
            .map_err(|fault| Shortfall::Undriven(faulted(fault)))?;
        if !ended {
            return Err(Shortfall::Undriven(past_the_bound()));
        }
        // What else the stream yielded is outcome correctness's to judge; this
        // obligation is that none of the answer sets went unseen.
        let seen = case
            .answer_sets
            .iter()
            .filter(|set| sets.contains(set))
            .count();
        if solved.conclusion() == Some(Conclusion::Exhausted) && seen < case.answer_sets.len() {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                format!(
                    "concluded that the search closed the space having seen {seen} of the program's {}",
                    counted(case.answer_sets.len()),
                ),
            )));
        }
        Ok(())
    })
}

/// An inconsistent reading rests on a search that closed the space —
/// `Inconsistent ⇒ Exhausted`, the termination reading never at odds with the
/// logical one. The core's classification reads a clean end short of the space
/// as inconclusive, so no backend can break it; attempted all the same, over
/// every corpus program through both doors.
fn inconsistency_is_exhausted(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_both_doors(corpus, |case, through| {
        load_through(backend, case, through).map_err(Shortfall::Undriven)?;
        let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
        let inconsistent = matches!(solved.determination(), Determination::Inconsistent(_));
        if inconsistent && solved.conclusion() != Some(Conclusion::Exhausted) {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                "read inconsistent over a search that did not close the space",
            )));
        }
        Ok(())
    })
}

/// A stream once touched cannot yield a complete collection (§5.3): the
/// exhaustion gate refuses it, so a truncated search cannot pass as all the
/// answer sets. Structural; attempted over every consistent corpus program
/// through both doors.
fn truncation_cannot_pose_as_complete(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    over_both_doors(corpus, |case, through| {
        if !case.is_consistent() {
            return Ok(());
        }
        load_through(backend, case, through).map_err(Shortfall::Undriven)?;
        let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
        drop(solved.models().next());
        // The gate refuses a touched handle at once, draining nothing.
        if solved.all_models().is_ok() {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                "a touched stream yielded a complete collection",
            )));
        }
        Ok(())
    })
}

/// A cancelled search never concludes as closing the space — nor does a pull
/// with no search in flight cancel a later one (§4.1). Not yet drivable: the
/// interrupt handle is reserved, so no search can be cancelled through it.
fn cancellation_is_not_exhaustion(backend: &dyn Backend) -> Verdict {
    Verdict::Skipped(if backend.capabilities().cancellation {
        Skip::Reserved
    } else {
        Skip::Undeclared(Capability::Cancellation)
    })
}

/// A declared observer is faithful (§10.4): once the corpus case's solve,
/// through either door, has drained — so an engine that grounds as it searches
/// has grounded — the observer answers `Some`; every ground rule names a member
/// of the program lowered — a statement of the rule's part, equal in content,
/// carrying an origin, every one of its origins among the member's, since the
/// set merge unions them, so an occurrence of the parse Door A carries is one at
/// the per-occurrence grain; and a fact grounds to a rule. Membership is not correctness: which statement a
/// rule came from is checked once a rule carries its head and body. An
/// undeclared observer binds nothing here — that its method answers `None` is
/// the honesty check's.
fn ground_program_is_faithful(backend: &mut dyn Backend, corpus: &[Case]) -> Verdict {
    if !backend.capabilities().ground_program {
        return Verdict::Skipped(Skip::Undeclared(Capability::GroundProgram));
    }
    over_both_doors(corpus, |case, through| {
        load_through(backend, case, through).map_err(Shortfall::Undriven)?;
        {
            // Read the stream out, bounded, so an engine that grounds as it
            // searches has grounded before its ground program is read.
            let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
            match pull(&mut solved, case.answer_sets.len()) {
                Ok(Pulled { ended: true, .. }) => {}
                Ok(_) => return Err(Shortfall::Undriven(past_the_bound())),
                Err(fault) => return Err(Shortfall::Undriven(faulted(fault))),
            }
        }
        let Some(ground) = backend.ground_program() else {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Refused,
                "declared, yet ground_program answered nothing once a grounding had finished",
            )));
        };
        let members: BTreeMap<(&PartKey, &Statement), &WithProvenance<Statement>> = case
            .program
            .parts()
            .flat_map(|part| {
                part.statements()
                    .map(move |node| ((part.key(), node.get()), node))
            })
            .collect();
        // A statement of the program lowered carries its provenance: at least
        // one origin, each among the member's.
        let belongs = |part: &PartKey, statement: &WithProvenance<Statement>| {
            let mut origins = statement.provenance().origins().peekable();
            origins.peek().is_some()
                && members.get(&(part, statement.get())).is_some_and(|member| {
                    origins.all(|origin| member.provenance().origins().any(|held| held == origin))
                })
        };
        if ground
            .rules()
            .any(|(_, part, statement)| !belongs(part, statement))
        {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                "a ground rule names a statement that is not of the program lowered",
            )));
        }
        if case.source == FACT && ground.rules().next().is_none() {
            return Err(Shortfall::Broke(Failure::new(
                Breach::Misanswered,
                "grounded a fact to no rule",
            )));
        }
        Ok(())
    })
}

/// A program the backend cannot ground — a fact over a variable nothing binds,
/// after a fact it can — is refused at the program locus (§5.4), whether at
/// lowering, at the solve, or at the stream's first item, naming the statement
/// that cannot be grounded and located there: the refusal's diagnostic lies
/// within that statement's span.
fn program_fault_is_located(backend: &mut dyn Backend) -> Verdict {
    let program = program_under(UNSAFE_SOURCE, UNSAFE);
    match refusal_of_unsafe(backend, &program) {
        Ok(fault) => located(fault, &program),
        Err(verdict) => verdict,
    }
}

/// The verdict on a refusal of the program that cannot be grounded: passed at
/// the program locus, refusing the unsafe statement, and lowered to a
/// diagnostic located within it.
fn located(fault: Fault, program: &Program) -> Verdict {
    if fault.locus() != Locus::Program {
        return Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                format!(
                    "a program that cannot be grounded was refused at the {} locus, not the program's",
                    fault.locus(),
                ),
            )
            .with_fault(fault),
        );
    }
    let (statement, location) = unsafe_statement(program);
    let names_it = matches!(
        fault.refused(),
        Refused::Statement(refused) if refused.get() == statement.get()
    );
    if !names_it {
        return Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                "a program that cannot be grounded was refused without naming the statement that cannot be",
            )
            .with_fault(fault),
        );
    }
    let diagnostics = fault.diagnostics();
    let within = matches!(&diagnostics[..], [diagnostic] if {
        let at = diagnostic.primary().location;
        at.source == location.source && location.span.contains_span(at.span)
    });
    if within {
        Verdict::Passed
    } else {
        Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                "a program that cannot be grounded was refused at a location outside the statement that cannot be",
            )
            .with_fault(fault),
        )
    }
}

/// The unsafe program's second statement — the fact over a variable nothing
/// binds — with where it was parsed. The suite raised the program from its
/// fixed text, every statement parsed, so the expects discharge invariants.
fn unsafe_statement(program: &Program) -> (&WithProvenance<Statement>, Location) {
    let at = UNSAFE
        .find("p(X)")
        .expect("the unsafe program holds its unsafe fact");
    let at = ByteOffset::new(
        u32::try_from(at).expect("a suite program is far within the coordinate limit"),
    );
    program
        .statements()
        .find_map(|node| {
            node.provenance().origins().find_map(|origin| match origin {
                Origin::Parsed(location) if location.span.contains(at) => Some((node, *location)),
                _ => None,
            })
        })
        .expect("a raised statement is located where it was parsed")
}

/// The verdict of an obligation held through several sub-checks: the first
/// that fails; otherwise passed if any passed; otherwise the first skip.
fn combined(verdicts: impl IntoIterator<Item = Verdict>) -> Verdict {
    let mut passed = false;
    let mut skipped = None;
    for verdict in verdicts {
        match verdict {
            Verdict::Failed(_) => return verdict,
            Verdict::Passed => passed = true,
            Verdict::Skipped(_) => {
                skipped.get_or_insert(verdict);
            }
        }
    }
    if passed {
        Verdict::Passed
    } else {
        skipped.unwrap_or(Verdict::Passed)
    }
}

/// The refusal of `program` — at its lowering, its solve, or its stream's first
/// item, as an engine that grounds as it searches refuses — or the failure of a
/// backend that accepted it. A refused reset before it leaves the check
/// undriven.
fn refusal_of_unsafe(backend: &mut dyn Backend, program: &Program) -> Result<Fault, Verdict> {
    reset_to_load(backend).map_err(|failure| Verdict::Skipped(Skip::Undriven(failure)))?;
    if let Err(fault) = backend.lower(Door::Program(program)) {
        return Ok(fault);
    }
    let mut solved = match backend.solve(&SolveRequest::default()) {
        Ok(solved) => solved,
        Err(fault) => return Ok(fault),
    };
    match solved.models().next() {
        Some(Err(fault)) => Ok(fault),
        _ => Err(Verdict::Failed(Failure::new(
            Breach::Accepted,
            format!("the program `{UNSAFE}`, whose variable nothing binds, was accepted"),
        ))),
    }
}

/// The unsafe program built through the constructors: its statements carry no
/// parsed origin. Fixed names, so the expects discharge invariants.
fn built_unsafe() -> Program {
    let name = |text| Name::new(text).expect("a suite name is an identifier");
    let variable = VarName::new("X").expect("a suite variable is a variable name");
    Program::of([
        Statement::from(Rule::fact(Atom::constant(name("a")))),
        Statement::from(Rule::fact(Atom::new(name("p"), [Term::variable(variable)]))),
    ])
}

/// A Program fault refusing a statement built in Rust is unlocated (§5.4): the
/// unsafe program built through the constructors is refused naming its unsafe
/// statement, with no diagnostic — no span was written for it to lie within.
fn built_fault_is_unlocated(backend: &mut dyn Backend) -> Verdict {
    let program = built_unsafe();
    let fault = match refusal_of_unsafe(backend, &program) {
        Ok(fault) => fault,
        Err(verdict) => return verdict,
    };
    if fault.locus() != Locus::Program {
        return Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                format!(
                    "a built program that cannot be grounded was refused at the {} locus, not the program's",
                    fault.locus(),
                ),
            )
            .with_fault(fault),
        );
    }
    let fact = Statement::from(Rule::fact(Atom::constant(
        Name::new("a").expect("a suite name is an identifier"),
    )));
    let unsafe_statement = program
        .statements()
        .find(|node| node.get() != &fact)
        .expect("the built program holds its unsafe fact beside the fact");
    let names_it = matches!(
        fault.refused(),
        Refused::Statement(refused) if refused.get() == unsafe_statement.get()
    );
    if !names_it {
        return Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                "a built program that cannot be grounded was refused without naming the statement that cannot be",
            )
            .with_fault(fault),
        );
    }
    if fault.diagnostics().is_empty() {
        Verdict::Passed
    } else {
        Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                "a statement built in Rust was refused at a location no source holds",
            )
            .with_fault(fault),
        )
    }
}

/// Each fault lands where it belongs (§5.4), through three sub-checks under one
/// obligation: a Program fault names its refused statement, located within it
/// where it was parsed; a Program fault refusing a statement built in Rust is
/// unlocated; and a Request fault names its presupposition.
fn fault_loci(backend: &mut dyn Backend) -> Verdict {
    combined([
        program_fault_is_located(backend),
        built_fault_is_unlocated(backend),
        non_external_assignment_refuses(backend),
    ])
}

/// A rebuild leaves nothing behind (§4.1, §6.3): the fact `a.` lowered and
/// solved, then a rebuild with `b.` — a second `lower` on a single-shot
/// backend, `reset` then `lower` on a multi-shot one — solves to exactly `{b}`;
/// and a cancellation pulled during the first run, where the backend declares
/// one, does not stop the second.
fn rebuild_leaves_nothing_behind(backend: &mut dyn Backend) -> Verdict {
    verdict_of(rebuilt(backend))
}

/// The rebuild check, driven: undriven where a step before its reading
/// cannot be.
fn rebuilt(backend: &mut dyn Backend) -> Result<(), Shortfall> {
    load_source(backend, FACT).map_err(Shortfall::Undriven)?;
    let cancel = if backend.capabilities().cancellation {
        backend.interrupt()
    } else {
        None
    };
    {
        let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
        if let Some(cancel) = &cancel {
            cancel.cancel();
        }
        pull(&mut solved, 1).map_err(stream_fault)?;
    }
    load_source(backend, REBUILT).map_err(Shortfall::Undriven)?;
    let mut solved = solve(backend).map_err(Shortfall::Undriven)?;
    let rebuilt = [answer_set([constant("b")])];
    let read = pull(&mut solved, rebuilt.len()).map_err(stream_fault)?;
    if !read.ended {
        return Err(Shortfall::Undriven(past_the_bound()));
    }
    if read.sets != rebuilt {
        return Err(
            if strays_from(&read.sets, &[constant("a"), constant("b")]) {
                stranger()
            } else {
                broke(
                    Breach::Misanswered,
                    "a rebuild carried something of the program it replaced into the next",
                )
            },
        );
    }
    if solved.conclusion() == Some(Conclusion::Interrupted) {
        return Err(broke(
            Breach::Misanswered,
            "a cancellation pulled during the replaced run stopped the next",
        ));
    }
    Ok(())
}

/// The shortfall of an obligation broken by `breach`, as `detail` says.
fn broke(breach: Breach, detail: &str) -> Shortfall {
    Shortfall::Broke(Failure::new(breach, detail))
}

/// A stream's mid-read fault, leaving a check undriven.
fn stream_fault(fault: Fault) -> Shortfall {
    Shortfall::Undriven(faulted(fault))
}

/// The refusal of the step `step` names, breaking the obligation.
fn refused_at(step: &'static str) -> impl FnOnce(Fault) -> Shortfall {
    move |fault| Shortfall::Broke(Failure::new(Breach::Refused, step).with_fault(fault))
}

/// The refusal of the step `step` names, leaving the check undriven.
fn undriven_at(step: &'static str) -> impl FnOnce(Fault) -> Shortfall {
    move |fault| Shortfall::Undriven(Failure::new(Breach::Refused, step).with_fault(fault))
}

/// The base part of the program `source` denotes, as a grounding names it.
fn base_of(source: &str) -> Part {
    program_of(source).base().clone()
}

/// Solve what is lowered, as every check does, and read it, bounded by
/// `expected`, as exactly `expected`: a refused solve, or another answer within
/// `vocabulary`, breaks the obligation — the latter as the caller's sentence
/// names — while a stream that faults, runs past its bound, or strays from the
/// vocabulary leaves it undriven.
fn solves_to(
    backend: &mut dyn Backend,
    expected: &[AnswerSet],
    vocabulary: &[Symbol],
    otherwise: &str,
) -> Result<(), Shortfall> {
    let mut solved = solve(backend).map_err(Shortfall::Broke)?;
    match pull(&mut solved, expected.len()) {
        Ok(Pulled { ended: false, .. }) => Err(Shortfall::Undriven(past_the_bound())),
        Ok(Pulled { sets, .. }) if sets == expected => Ok(()),
        Ok(Pulled { sets, .. }) if strays_from(&sets, vocabulary) => Err(stranger()),
        Ok(_) => Err(Shortfall::Broke(Failure::new(
            Breach::Misanswered,
            otherwise,
        ))),
        Err(fault) => Err(Shortfall::Undriven(faulted(fault))),
    }
}

/// Whether some set of `sets` holds an atom outside `vocabulary` — the atoms
/// of the programs a check lowers: a set no program it lowered could answer,
/// which outcome correctness judges, so it leaves the check undriven.
fn strays_from(sets: &[AnswerSet], vocabulary: &[Symbol]) -> bool {
    sets.iter().flatten().any(|atom| !vocabulary.contains(atom))
}

/// The shortfall of a stream that yielded a set no program a check lowered
/// could answer.
fn stranger() -> Shortfall {
    Shortfall::Undriven(Failure::new(
        Breach::Misanswered,
        "yielded a set that is not one of its answer sets",
    ))
}

/// The verdict of a sub-check that `outcome` drove to its end: passed, failed
/// where it broke, skipped where it could not be driven.
fn verdict_of(outcome: Result<(), Shortfall>) -> Verdict {
    match outcome {
        Ok(()) => Verdict::Passed,
        Err(Shortfall::Broke(failure)) => Verdict::Failed(failure),
        Err(Shortfall::Undriven(failure)) => Verdict::Skipped(Skip::Undriven(failure)),
    }
}

/// A refusal its lowering's check makes adds nothing (§4.1), on every backend:
/// the fact `a.` lowered, then a program the backend refuses at its check — a
/// fact `b.` before a fact over a variable nothing binds — and a solve reads
/// `{a}`, so a multi-shot backend added nothing of the refused program and a
/// single-shot one kept the program lowered before it. A backend whose check
/// accepts the program, refusing it later, made no refusal there to hold.
fn refused_lowering_adds_nothing(backend: &mut dyn Backend) -> Verdict {
    if let Err(failure) = load_source(backend, FACT) {
        return Verdict::Skipped(Skip::Undriven(failure));
    }
    if backend
        .lower(Door::Program(&program_of(PREFIXED_UNSAFE)))
        .is_ok()
    {
        return Verdict::Skipped(Skip::Undriven(Failure::new(
            Breach::Accepted,
            format!("the lowering's check accepted `{PREFIXED_UNSAFE}`, so it made no refusal"),
        )));
    }
    match solves_to(
        backend,
        &[answer_set([constant("a")])],
        &[constant("a"), constant("b")],
        "a refused lowering added to what was lowered before it",
    ) {
        Ok(()) => Verdict::Passed,
        Err(Shortfall::Broke(failure)) if failure.breach() == Breach::Misanswered => {
            Verdict::Failed(failure)
        }
        Err(Shortfall::Broke(failure) | Shortfall::Undriven(failure)) => {
            Verdict::Skipped(Skip::Undriven(failure))
        }
    }
}

/// Whether `result` is the refusal a backend needing its rebuild owes: an
/// answer, or another refusal, breaks the obligation.
fn needs_its_rebuild<T>(result: Result<T, Fault>) -> Result<(), Shortfall> {
    let Err(fault) = result else {
        return Err(broke(
            Breach::Accepted,
            "a backend needing its rebuild answered where it owed a refusal",
        ));
    };
    if let Refused::Request(Presupposition::NeedsRebuild) = fault.refused() {
        return Ok(());
    }
    let misnamed = "a backend needing its rebuild refused other than as needing it";
    Err(Shortfall::Broke(
        Failure::new(Breach::Mislocated, misnamed).with_fault(fault),
    ))
}

/// A failed grounding — an `@`-function that faults while grounding — leaves a
/// multi-shot backend refusing with `Presupposition::NeedsRebuild` at every
/// method that touches its program or runs a search, while `capabilities` and
/// `reset` answer and its observer answers nothing, until the `reset` that
/// rebuilds; and fails a single-shot backend's solve, the backend staying
/// ready (§4.1). Binds a backend declaring functions.
fn failed_grounding_needs_a_rebuild(backend: &mut dyn Backend) -> Verdict {
    let capabilities = backend.capabilities();
    if !capabilities.functions {
        return Verdict::Skipped(Skip::Undeclared(Capability::Functions));
    }
    verdict_of(failed_grounding(backend, &capabilities))
}

/// The failed-grounding sub-check, driven.
fn failed_grounding(
    backend: &mut dyn Backend,
    capabilities: &Capabilities,
) -> Result<(), Shortfall> {
    reset_to_load(backend).map_err(Shortfall::Undriven)?;
    backend
        .register_function(Box::new(Faulting))
        .map_err(undriven_at(REGISTRATION_REFUSED))?;
    lower_program(backend, &program_of(FAULTING_CALL), FAULTING_CALL)
        .map_err(Shortfall::Undriven)?;
    if capabilities.multi_shot {
        needs_a_rebuild_after_a_failed_grounding(backend, capabilities)
    } else {
        ready_after_a_failed_solve(backend)
    }
}

/// The refusal of one of the suite's own `@`-functions' registration.
const REGISTRATION_REFUSED: &str = "the suite's function's registration was refused";

/// The fact `a.`, read back: `{a}`.
fn the_fact() -> [AnswerSet; 1] {
    [answer_set([constant("a")])]
}

/// A single-shot backend grounds within each solve (§4.1): a solve whose
/// grounding's `@`-function faults is refused — at the solve or its stream's
/// first item — and the backend stays ready: a replacing `lower` of the fact
/// `a.` solves to `{a}`.
fn ready_after_a_failed_solve(backend: &mut dyn Backend) -> Result<(), Shortfall> {
    let refused = match backend.solve(&SolveRequest::default()) {
        Err(_) => true,
        Ok(mut solved) => matches!(solved.models().next(), Some(Err(_))),
    };
    if !refused {
        return Err(broke(
            Breach::Accepted,
            "a solve whose grounding's @-function faults was accepted",
        ));
    }
    lower_program(backend, &program_of(FACT), FACT).map_err(Shortfall::Broke)?;
    solves_to(
        backend,
        &the_fact(),
        &[constant("a")],
        "after a failed grounding, the replacing program did not solve",
    )
}

/// A multi-shot backend whose grounding failed needs its rebuild (§4.1): every
/// method that touches its program or runs a search refuses with
/// `Presupposition::NeedsRebuild` — those `capabilities` declares among them —
/// its observer answers nothing, and its `reset` answers, after which the fact
/// `a.` lowers, grounds, and solves to `{a}`.
fn needs_a_rebuild_after_a_failed_grounding(
    backend: &mut dyn Backend,
    capabilities: &Capabilities,
) -> Result<(), Shortfall> {
    let base = [base_of(FAULTING_CALL)];
    if backend.ground(&base, &GroundOptions::default()).is_ok() {
        return Err(broke(
            Breach::Accepted,
            "a grounding whose @-function faults was accepted",
        ));
    }
    let solved = backend.solve(&SolveRequest::default()).map(drop);
    needs_its_rebuild(solved)?;
    let lowered = backend.lower(Door::Program(&program_of(FACT)));
    needs_its_rebuild(lowered)?;
    let grounded = backend.ground(&base, &GroundOptions::default());
    needs_its_rebuild(grounded)?;
    let assigned = backend.assign_external(constant("a"), TruthValue::True);
    needs_its_rebuild(assigned)?;
    let registered = backend.register_function(Box::new(Echo));
    needs_its_rebuild(registered)?;
    if capabilities.assumptions {
        let assumed = backend
            .solve_assuming(&Scenario::default(), &SolveRequest::default())
            .map(drop);
        needs_its_rebuild(assumed)?;
    }
    if capabilities.native_consequences == ConsequenceSupport::Native {
        let native = backend.consequences_native(Mode::Cautious, &ConsequenceRequest::default());
        needs_its_rebuild(native)?;
    }
    if capabilities.optimization {
        let optimized = backend.optimize(&OptimizeRequest::default()).map(drop);
        needs_its_rebuild(optimized)?;
    }
    if capabilities.propagators {
        let registered = backend.register_propagator(Box::new(Inert));
        needs_its_rebuild(registered)?;
    }
    if backend.ground_program().is_some() {
        return Err(broke(
            Breach::Misanswered,
            "exposed a ground program while needing its rebuild",
        ));
    }
    backend
        .reset()
        .map_err(refused_at("the reset that rebuilds was refused"))?;
    lower_program(backend, &program_of(FACT), FACT).map_err(Shortfall::Broke)?;
    solves_to(
        backend,
        &the_fact(),
        &[constant("a")],
        "after the rebuild, the program did not solve",
    )
}

/// A registration survives the rebuild (§4.1): `Echo` registered, the fact
/// `a.` lowered and solved, then a rebuild with `p(@echo(1)).` — `reset` then
/// `lower` on a multi-shot backend, one `lower` on a single-shot one — solves
/// to a set holding `p(1)`. Binds a backend declaring functions.
fn registration_survives_the_rebuild(backend: &mut dyn Backend) -> Verdict {
    if !backend.capabilities().functions {
        return Verdict::Skipped(Skip::Undeclared(Capability::Functions));
    }
    verdict_of(registration_kept(backend))
}

/// The registration sub-check, driven.
fn registration_kept(backend: &mut dyn Backend) -> Result<(), Shortfall> {
    reset_to_load(backend).map_err(Shortfall::Undriven)?;
    backend
        .register_function(Box::new(Echo))
        .map_err(undriven_at(REGISTRATION_REFUSED))?;
    lower_program(backend, &program_of(FACT), FACT).map_err(Shortfall::Undriven)?;
    solves_to(backend, &the_fact(), &[constant("a")], "misread the fact").map_err(
        |(Shortfall::Broke(failure) | Shortfall::Undriven(failure))| Shortfall::Undriven(failure),
    )?;
    load_source(backend, ECHOED_CALL).map_err(Shortfall::Broke)?;
    let echo = atom("p", [Symbol::number(1)], Sign::Positive);
    solves_to(
        backend,
        &[answer_set([echo.clone()])],
        &[constant("a"), echo],
        "a registration did not survive the rebuild",
    )
}

/// A backend's own state follows its refusals and rebuilds (§4.1), through
/// three sub-checks under one obligation: a refusal its lowering's check makes
/// adds nothing; a failed grounding leaves it needing its rebuild; and a
/// registration survives the rebuild.
fn backend_state(backend: &mut dyn Backend) -> Verdict {
    combined([
        refused_lowering_adds_nothing(backend),
        failed_grounding_needs_a_rebuild(backend),
        registration_survives_the_rebuild(backend),
    ])
}

/// Assigning a truth value to an atom that is not external refuses at the
/// request locus — never the silent no-op an engine may give, which would let
/// the knowledge base and the engine disagree at a retraction's toggle (§4.2,
/// §6.2). Binds a backend declaring multi-shot solving, under which
/// `assign_external` is required.
fn non_external_assignment_refuses(backend: &mut dyn Backend) -> Verdict {
    if !backend.capabilities().multi_shot {
        return Verdict::Skipped(Skip::Undeclared(Capability::MultiShot));
    }
    if let Err(failure) = load_source(backend, FACT) {
        return Verdict::Skipped(Skip::Undriven(failure));
    }
    match backend.assign_external(constant("a"), TruthValue::True) {
        Ok(()) => Verdict::Failed(Failure::new(
            Breach::Accepted,
            "assigning an atom that is not external was accepted — a silent no-op",
        )),
        Err(fault)
            if matches!(
                fault.refused(),
                Refused::Request(Presupposition::Unsupported(_))
            ) =>
        {
            Verdict::Failed(
                Failure::new(
                    Breach::Refused,
                    "assign_external refused as unsupported, though the backend declares multi-shot solving",
                )
                .with_fault(fault),
            )
        }
        Err(fault)
            if matches!(
                fault.refused(),
                Refused::Request(Presupposition::NotExternal)
            ) =>
        {
            Verdict::Passed
        }
        Err(fault) => Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                "assigning an atom that is not external refused, though not naming it as one",
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
    /// It answered nothing — `interrupt`'s and the observer's honest `None`
    /// (§4.1), where another method refuses.
    Absent,
    /// It answered, then the stream the probe read faulted, with this fault.
    Faulted(Fault),
    /// It answered, but wrongly: how, and the corpus program it answered
    /// wrongly on, where there is one.
    Misanswered {
        case: Option<&'static str>,
        how: String,
    },
    /// The capability's own probe program was refused — one only a backend
    /// with the capability need carry, so no other check sees the refusal.
    ProgramRefused(Failure),
    /// A step before the method — a reset, a corpus program whose refusal
    /// outcome correctness fails, a plain solve — was refused, so the probe
    /// could not reach it.
    Unprobed(Failure),
}

/// A wrong answer on no one corpus program, as a response.
fn misanswered(how: &str) -> Response {
    Response::Misanswered {
        case: None,
        how: how.to_owned(),
    }
}

/// The response a method's result makes: answered, or refused with its fault.
fn respond<T>(result: Result<T, Fault>) -> Response {
    match result {
        Ok(_) => Response::Answered,
        Err(fault) => Response::Refused(fault),
    }
}

/// Whether `response` is the refusal an undeclared `capability` owes (§4.1,
/// §6.3): unsupported, naming that capability — or, for the time budget, the
/// budget nothing realises.
fn refuses_as_owed(capability: Capability, response: &Response) -> bool {
    let Response::Refused(fault) = response else {
        return false;
    };
    match fault.refused() {
        Refused::Request(Presupposition::Unsupported(named)) => named == capability,
        Refused::Request(Presupposition::UnrealisableBudget) => {
            capability == Capability::TimeBudget
        }
        _ => false,
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
    let row = row(capability);
    let declared = backend.capabilities().declares(capability);
    if capability == Capability::Externals && !declared {
        return Verdict::Skipped(Skip::Undeclared(Capability::Externals));
    }
    judge(
        capability,
        row.method,
        declared,
        (row.probe)(backend, declared, corpus),
    )
}

/// The verdict on a probe's response to `capability` declared, or not.
fn judge(capability: Capability, method: &str, declared: bool, response: Response) -> Verdict {
    match (declared, response) {
        (_, Response::Unprobed(failure)) | (false, Response::ProgramRefused(failure)) => {
            Verdict::Skipped(Skip::Undriven(failure))
        }
        (true, Response::Answered) | (false, Response::Absent) => Verdict::Passed,
        (true, Response::Absent) => Verdict::Failed(Failure::new(
            Breach::Refused,
            format!("declared, yet {method} answered nothing"),
        )),
        (true, Response::Refused(fault)) => Verdict::Failed(
            Failure::new(Breach::Refused, format!("declared, yet {method} refused"))
                .with_fault(fault),
        ),
        (true, Response::Faulted(fault)) => Verdict::Failed(
            Failure::new(
                Breach::Refused,
                format!("declared, yet the stream faulted while probing {method}"),
            )
            .with_fault(fault),
        ),
        (true, Response::Misanswered { case, how }) => Verdict::Failed(Failure {
            case,
            ..Failure::new(Breach::Misanswered, format!("declared, yet {method} {how}"))
        }),
        (true, Response::ProgramRefused(failure)) => Verdict::Failed(Failure {
            detail: format!("declared, yet {}", failure.detail),
            ..failure
        }),
        (false, response) if refuses_as_owed(capability, &response) => Verdict::Passed,
        (false, Response::Refused(fault)) => Verdict::Failed(
            Failure::new(
                Breach::Mislocated,
                format!(
                    "undeclared, and {method} refused, though not with the refusal an undeclared \
                     {capability} owes"
                ),
            )
            .with_fault(fault),
        ),
        (false, Response::Answered | Response::Faulted(_) | Response::Misanswered { .. }) => {
            Verdict::Failed(Failure::new(
                Breach::Accepted,
                format!(
                    "undeclared, yet {method} answered: a request beyond the declaration must refuse"
                ),
            ))
        }
    }
}

/// The observer's probe: ground the fact — load it, solve, and drain the stream
/// — then read the observer, `Some` its answer and `None` its honest absence
/// (§10.4). A step before the read that is refused leaves it unprobed.
fn probe_ground_program(backend: &mut dyn Backend) -> Response {
    if let Err(failure) = load_source(backend, FACT) {
        return Response::Unprobed(failure);
    }
    {
        let mut solved = match solve(backend) {
            Ok(solved) => solved,
            Err(failure) => return Response::Unprobed(failure),
        };
        match pull(&mut solved, 1) {
            Ok(Pulled { ended: true, .. }) => {}
            Ok(_) => return Response::Unprobed(past_the_bound()),
            Err(fault) => return Response::Unprobed(faulted(fault)),
        }
    }
    match backend.ground_program() {
        Some(_) => Response::Answered,
        None => Response::Absent,
    }
}

/// Optimization's probe: the optimum of a choice under an objective.
fn probe_optimization(backend: &mut dyn Backend) -> Response {
    if let Err(response) = load_own(backend, OPTIMIZATION) {
        return response;
    }
    respond(backend.optimize(&OptimizeRequest::default()))
}

/// The native door's answer over `sets` in `mode`: the `⋂` or `⋃` of the sets
/// over a space closed, or no model where there are none — `⋂`/`⋃` over no
/// answer set is undefined, not `∅` (query.md §2.3).
fn native_answer(mode: Mode, sets: &[AnswerSet]) -> NativeAnswer {
    Consequences::fold(mode, sets).map_or(NativeAnswer::NoModel, |folded| {
        NativeAnswer::Closed(folded.as_set().clone())
    })
}

/// How a native answer misread the known one, as a phrase.
fn misread(found: &NativeAnswer, known: &NativeAnswer) -> &'static str {
    match (found, known) {
        (NativeAnswer::Stopped(_), _) => "stopped short of the space, though no budget was asked",
        (NativeAnswer::Closed(_), NativeAnswer::NoModel) => {
            "answered consequences of a program with no answer set"
        }
        (NativeAnswer::NoModel, _) => "reported no model of a program with an answer set",
        (NativeAnswer::Closed(_), _) => "gave consequences other than those of its answer sets",
    }
}

/// The native door's probe: over every corpus program, through both doors
/// (§10.2), in each mode, its answer is its known one — the `⋂` or `⋃` of the
/// program's answer sets over a space closed, and no model over a program with
/// none. Where the backend also
/// declares `assumptions` — the only backend handed a non-empty scenario (§6.2)
/// — the door ranges over the models a request's scenario admits, so it agrees
/// with the fold over `solve_assuming`.
fn probe_native_consequences(backend: &mut dyn Backend, corpus: &[Case]) -> Response {
    let assumes = backend.capabilities().assumptions;
    for case in corpus {
        for through in [Through::Program, Through::Parsed] {
            if let Err(failure) = load_through(backend, case, through) {
                return Response::Unprobed(failure);
            }
            for mode in [Mode::Cautious, Mode::Brave] {
                let known = native_answer(mode, &case.answer_sets);
                match backend.consequences_native(mode, &ConsequenceRequest::default()) {
                    Err(fault) => return Response::Refused(fault),
                    Ok(found) if found != known => {
                        return Response::Misanswered {
                            case: Some(case.name),
                            how: misread(&found, &known).to_owned(),
                        };
                    }
                    Ok(_) => {}
                }
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
/// program's — and under a scenario that admits no model it reports none, as
/// over a program with none: the positive loop under `a` fixed to hold (an atom
/// no answer set holds), and the fact `a.` under `a` fixed not to.
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
                Ok(found) if found != native_answer(mode, &admitted) => {
                    return misanswered(
                        "gave consequences under a scenario other than those of the models it admits",
                    );
                }
                Ok(_) => {}
            }
        }
    }
    for (source, holds) in [(POSITIVE_LOOP, true), (FACT, false)] {
        if let Err(failure) = load_source(backend, source) {
            return Response::Unprobed(failure);
        }
        let request = ConsequenceRequest {
            scenario: fixing_a(holds),
        };
        for mode in [Mode::Cautious, Mode::Brave] {
            match backend.consequences_native(mode, &request) {
                Err(fault) => return Response::Refused(fault),
                Ok(NativeAnswer::NoModel) => {}
                Ok(_) => {
                    return misanswered(
                        "answered consequences under a scenario that admits no model",
                    );
                }
            }
        }
    }
    Response::Answered
}

/// The scenario fixing `a` to hold (`true`) or not to (`false`) — over the even
/// loop, it admits exactly `{a}`, or exactly `{b}`; over the fact `a.`, fixing
/// `a` not to hold admits nothing, as fixing it to hold does over the positive
/// loop. A suite constant is an atom, so the expect discharges an invariant.
fn fixing_a(holds: bool) -> Scenario {
    let assumption = Assumption::new(constant("a"), holds).expect("a suite constant is an atom");
    [assumption].into_iter().collect()
}

/// Assumptions' probe: the even loop under a scenario fixing `a` to hold, then
/// not to — each read as consistent, its models ranging over the scenario asked
/// and each the one answer set it admits; then two scenarios that admit no
/// model, each read inconsistent — the positive loop under `a` fixed to hold,
/// an atom no answer set holds (an assumption encoded as a fact, or dropped for
/// want of the atom, reads it consistent), and the fact `a.` under `a` fixed
/// not to; and last a plain solve of the fact, which reads its one answer set
/// over no scenario — a scenario kept past its solve would empty it.
fn probe_assumptions(backend: &mut dyn Backend) -> Response {
    let enumerates = backend.capabilities().enumeration;
    if let Err(failure) = load_source(backend, EVEN_LOOP) {
        return Response::Unprobed(failure);
    }
    for (holds, admitted) in [(true, "a"), (false, "b")] {
        let scenario = fixing_a(holds);
        let admitted = [answer_set([constant(admitted)])];
        let mut solved = match backend.solve_assuming(&scenario, &SolveRequest::default()) {
            Ok(solved) => solved,
            Err(fault) => return Response::Refused(fault),
        };
        match solved.determination() {
            Determination::Consistent(models) if *models.scenario() == scenario => {}
            Determination::Consistent(_) => {
                return misanswered("ranged its models over a scenario other than the one asked");
            }
            _ => return misanswered("did not read a satisfiable scenario as consistent"),
        }
        match pull(&mut solved, admitted.len()) {
            Ok(Pulled {
                sets, ended: true, ..
            }) if sets.iter().all(|set| admitted.contains(set))
                && (!enumerates || sets == admitted) => {}
            Ok(_) => return misanswered("yielded models other than the one the scenario admits"),
            Err(fault) => return Response::Faulted(fault),
        }
    }
    for (source, holds) in [(POSITIVE_LOOP, true), (FACT, false)] {
        if let Err(failure) = load_source(backend, source) {
            return Response::Unprobed(failure);
        }
        let mut solved = match backend.solve_assuming(&fixing_a(holds), &SolveRequest::default()) {
            Ok(solved) => solved,
            Err(fault) => return Response::Refused(fault),
        };
        if !matches!(solved.determination(), Determination::Inconsistent(_)) {
            return misanswered("did not read an unsatisfiable scenario as inconsistent");
        }
    }
    let fact = [answer_set([constant("a")])];
    let mut solved = match solve(backend) {
        Ok(solved) => solved,
        Err(failure) => return Response::Unprobed(failure),
    };
    let unscoped = matches!(
        solved.determination(),
        Determination::Consistent(models) if models.scenario().assumptions().next().is_none()
    );
    match pull(&mut solved, fact.len()) {
        Ok(Pulled {
            sets, ended: true, ..
        }) if unscoped && sets == fact => Response::Answered,
        Ok(_) => {
            misanswered("kept a scenario past its solve: a plain solve after it misread the fact")
        }
        Err(fault) => Response::Faulted(fault),
    }
}

/// Multi-shot's probe. Declared: `lower` accumulates — the fact `a.`, then the
/// rule `b :- a.` lowered with no reset between, read `{a, b}` — and `ground` of
/// no part answers; that a reset clears what accumulated, every corpus load
/// holds. The assignment of an external is the externals probe's, and of a
/// non-external its own check. Undeclared: all three multi-shot methods refuse
/// as unsupported, naming multi-shot solving; the first that does not is the
/// response.
fn probe_multi_shot(backend: &mut dyn Backend, declared: bool) -> Response {
    if !declared {
        let reset = respond(backend.reset());
        let ground = respond(backend.ground(&[], &GroundOptions::default()));
        let assign = respond(backend.assign_external(constant("a"), TruthValue::True));
        return [ground, assign].into_iter().fold(reset, |kept, next| {
            if refuses_as_owed(Capability::MultiShot, &kept) {
                next
            } else {
                kept
            }
        });
    }
    if let Err(fault) = backend.reset() {
        return Response::Refused(fault);
    }
    if let Err(failure) = lower_program(backend, &program_of(FACT), FACT) {
        return Response::Unprobed(failure);
    }
    if let Err(failure) = lower_program(backend, &program_of(RULE), RULE) {
        return Response::ProgramRefused(failure);
    }
    let together = [answer_set([constant("a"), constant("b")])];
    let mut solved = match solve(backend) {
        Ok(solved) => solved,
        Err(failure) => return Response::Unprobed(failure),
    };
    match pull(&mut solved, together.len()) {
        Ok(Pulled {
            sets, ended: true, ..
        }) if sets == together => {}
        Ok(_) => {
            return misanswered(
                "did not keep the program across lowerings: the fact and the rule over it read other than {a, b}",
            );
        }
        Err(fault) => return Response::Faulted(fault),
    }
    drop(solved);
    respond(backend.ground(&[], &GroundOptions::default()))
}

/// The externals probe: over an external atom and a rule on it, assigning the
/// atom true reads `{a, b}` and assigning it false reads `{}` — the assignment
/// answered, and honoured in the answer sets.
fn probe_externals(backend: &mut dyn Backend) -> Response {
    if let Err(response) = load_own(backend, EXTERNAL) {
        return response;
    }
    for (value, expected) in [
        (TruthValue::True, answer_set([constant("a"), constant("b")])),
        (TruthValue::False, answer_set([])),
    ] {
        if let Err(fault) = backend.assign_external(constant("a"), value) {
            return Response::Refused(fault);
        }
        let expected = [expected];
        let mut solved = match solve(backend) {
            Ok(solved) => solved,
            Err(failure) => return Response::Unprobed(failure),
        };
        match pull(&mut solved, expected.len()) {
            Ok(Pulled {
                sets, ended: true, ..
            }) if sets == expected => {}
            Ok(_) => {
                return misanswered(
                    "answered, then read answer sets other than the assignment gives",
                );
            }
            Err(fault) => return Response::Faulted(fault),
        }
    }
    Response::Answered
}

/// A time budget no solve of the probe program approaches — so a solve under it
/// answers, never concluding at the budget.
const PROBE_BUDGET: Duration = Duration::from_mins(1);

/// A program no search enumerates within the cut's budget: a choice over forty
/// atoms, whose 2^40 answer sets no engine yields in time — so a search of it
/// concluded as closing the space under that budget was cut, and says otherwise.
const UNBOUNDED: &str = "{ a(1..40) }.";

/// The budget the cut solves the unbounded program under.
const CUT_BUDGET: Duration = Duration::from_millis(50);

/// The most models the cut reads: far more than any engine yields within its
/// budget, so a stream still running past them was never cut.
const CUT_CAP: usize = 1 << 20;

/// The time budget's probe: over the fact `a.`, each solve the backend serves —
/// `solve`, and `solve_assuming` (assuming nothing) where the backend declares
/// `assumptions`, since it takes the same request — asked under a budget, where
/// the same solve unbudgeted reads the fact; where it does not, another check
/// owns what went wrong. Declared: each answers, reading the fact's one answer
/// set; and, where the backend enumerates, the cut — each asked of the
/// unbounded program under a budget it cannot finish within — ends its stream
/// within the cap and concludes `Budget`, never `Exhausted`, the conclusion
/// every complete collection trusts (§5.3, §6.3). A deciding backend stops at
/// its witness, which no budget the suite sets cuts, so it owes no cut here.
/// Undeclared: each refuses at the request locus. The first that does not is
/// the response.
fn probe_time_budget(backend: &mut dyn Backend, declared: bool) -> Response {
    if let Err(failure) = load_source(backend, FACT) {
        return Response::Unprobed(failure);
    }
    let unbudgeted = SolveRequest::default();
    let budgeted = SolveRequest {
        time: Some(PROBE_BUDGET),
    };
    let mut responses = Vec::new();
    let solves = matches!(
        read_the_fact(backend.solve(&unbudgeted)),
        Response::Answered
    );
    if solves {
        responses.push(read_the_fact(backend.solve(&budgeted)));
    }
    let nothing = Scenario::default();
    let assumes = backend.capabilities().assumptions
        && matches!(
            read_the_fact(backend.solve_assuming(&nothing, &unbudgeted)),
            Response::Answered
        );
    if assumes {
        responses.push(read_the_fact(backend.solve_assuming(&nothing, &budgeted)));
    }
    if (solves || assumes) && backend.capabilities().enumeration {
        if let Err(response) = load_own(backend, UNBOUNDED) {
            return response;
        }
        let cut = SolveRequest {
            time: Some(CUT_BUDGET),
        };
        if solves {
            responses.push(read_the_cut(backend.solve(&cut)));
        }
        if assumes {
            responses.push(read_the_cut(backend.solve_assuming(&nothing, &cut)));
        }
    }
    let holds = |response: &Response| {
        if declared {
            matches!(response, Response::Answered)
        } else {
            refuses_as_owed(Capability::TimeBudget, response)
        }
    };
    match responses.iter().position(|response| !holds(response)) {
        Some(breach) => responses.swap_remove(breach),
        None => responses.pop().unwrap_or_else(|| {
            Response::Unprobed(Failure::new(
                Breach::Refused,
                "no solve read the fact without a budget",
            ))
        }),
    }
}

/// How a solve of the unbounded program under the cut's budget met the probe:
/// refused, or answered — rightly only when its stream ends within the cap and
/// its search concludes at the budget. The models are counted, never kept.
fn read_the_cut(solved: Result<Solved<'_>, Fault>) -> Response {
    let mut solved = match solved {
        Ok(solved) => solved,
        Err(fault) => return Response::Refused(fault),
    };
    let read = solved
        .models()
        .take(CUT_CAP + 1)
        .try_fold(0_usize, |read, yielded| yielded.map(|_| read + 1));
    match read {
        Err(fault) => Response::Faulted(fault),
        Ok(read) if read > CUT_CAP => misanswered("ran a search past its budget, never cutting it"),
        Ok(_) if solved.conclusion() == Some(Conclusion::Budget) => Response::Answered,
        Ok(_) => misanswered("concluded a search cut at its budget other than at the budget"),
    }
}

/// How a solve of the fact met the probe: refused, or answered — rightly only
/// when it reads the fact's one answer set.
fn read_the_fact(solved: Result<Solved<'_>, Fault>) -> Response {
    let mut solved = match solved {
        Ok(solved) => solved,
        Err(fault) => return Response::Refused(fault),
    };
    let fact = [answer_set([constant("a")])];
    match pull(&mut solved, fact.len()) {
        Ok(Pulled {
            sets, ended: true, ..
        }) if sets == fact => Response::Answered,
        Ok(_) => misanswered("read answer sets other than the fact's"),
        Err(fault) => Response::Faulted(fault),
    }
}

/// The `@`-function the suite registers to probe `functions`: it answers its
/// arguments.
struct Echo;

impl Function for Echo {
    fn call(&self, arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault> {
        Ok(arguments.to_vec())
    }
}

/// The `@`-function the suite registers to fail a grounding: it refuses every
/// call.
struct Faulting;

impl Function for Faulting {
    fn call(&self, _arguments: &[Symbol]) -> Result<Vec<Symbol>, GroundFault> {
        Err(GroundFault::refused(
            "the suite's faulting function refuses every call",
        ))
    }
}

/// The propagator the suite registers to probe `propagators`: it propagates
/// nothing.
struct Inert;

impl Propagator for Inert {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{Grain, GroundProgram, GroundRule};
    use crate::contract::Capabilities;
    use crate::outcome::{Model, Run, ShowRule};
    use themelios_program::Provenance;

    #[test]
    fn every_capability_s_row_names_its_own_method() {
        let methods: BTreeSet<&str> = CAPABILITIES.map(|capability| row(capability).method).into();
        assert_eq!(methods.len(), CAPABILITIES.len());
    }

    #[test]
    fn every_capability_is_probed_once() {
        let probed: BTreeSet<String> = CAPABILITIES.map(|capability| capability.to_string()).into();
        assert_eq!(probed.len(), CAPABILITIES.len());
    }

    #[test]
    fn every_suite_program_raises_without_a_diagnostic() {
        let sources = corpus()
            .iter()
            .map(|case| case.source)
            .chain([
                FACT,
                EVEN_LOOP,
                OPTIMIZATION,
                EXTERNAL,
                RULE,
                UNSAFE,
                PREFIXED_UNSAFE,
                REBUILT,
                FAULTING_CALL,
                ECHOED_CALL,
                UNBOUNDED,
            ])
            .collect::<Vec<_>>();
        for source in sources {
            let raised = raise_str(source, Dialect::Clingo).expect("a suite program raises");
            assert!(raised.diagnostics().is_empty(), "{source}");
        }
    }

    #[test]
    fn a_corpus_program_has_a_display_for_each_answer_set() {
        for case in corpus() {
            assert_eq!(case.displays.len(), case.answer_sets.len(), "{}", case.name);
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
            Verdict::Skipped(Skip::Reserved),
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
                        .with_fault(Fault::unsupported(Capability::Assumptions)),
                ),
            ),
        ]);
        assert_eq!(
            report.to_string(),
            "each corpus program's outcome is its known one: passed\n\
             the declaration of assumptions is honest: failed — declared, yet solve_assuming \
             refused: this backend does not declare assumptions\n",
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
    fn a_failure_exposes_its_parts_as_data() {
        let failure = Failure::new(Breach::Refused, "the solve was refused")
            .with_fault(Fault::engine("gone"))
            .on("a fact");
        assert_eq!(failure.breach(), Breach::Refused);
        assert_eq!(failure.case(), Some("a fact"));
        assert_eq!(failure.fault().map(Fault::locus), Some(Locus::Engine));
    }

    #[test]
    fn every_check_renders_a_distinct_obligation() {
        // The checks a run reports, whatever their verdicts.
        let report = run(&mut grounding(Grounds::Faithfully));
        let rendered: BTreeSet<String> = report
            .entries()
            .map(|(check, _)| check.to_string())
            .collect();
        assert_eq!(rendered.len(), report.entries().count());
    }

    #[test]
    fn a_skipped_verdict_renders_its_reason() {
        let skipped = Verdict::Skipped(Skip::Undeclared(Capability::GroundProgram));
        assert_eq!(
            skipped.to_string(),
            "skipped — the backend does not declare the ground program"
        );
    }

    #[test]
    fn every_skip_renders_a_distinct_reason() {
        let skips = [
            Skip::Undeclared(Capability::Cancellation),
            Skip::Reserved,
            Skip::Undriven(Failure::new(Breach::Refused, "the solve was refused")),
        ];
        let reasons: BTreeSet<String> = skips.iter().map(ToString::to_string).collect();
        assert_eq!(reasons.len(), skips.len());
    }

    #[test]
    fn an_undriven_skip_renders_the_failure_that_stopped_it() {
        let skip = Skip::Undriven(
            Failure::new(Breach::Refused, "the solve was refused")
                .with_fault(Fault::engine("gone")),
        );
        assert_eq!(
            skip.to_string(),
            "could not be driven: the solve was refused: gone"
        );
    }

    #[test]
    fn the_echo_function_answers_its_arguments() {
        let arguments = [constant("a"), Symbol::number(1)];
        assert_eq!(Echo.call(&arguments), Ok(arguments.to_vec()));
    }

    // ---- The observer, over a backend that declares it ----

    /// A grounding backend's search: one that closes the space at once, with
    /// no model; one that faults at its first pull; or one that yields the empty
    /// set forever — only the first concluding.
    enum Search {
        Closed,
        Faulting,
        Endless,
    }

    impl Run for Search {
        fn next_model(&mut self) -> Option<Result<Model, Fault>> {
            match self {
                Search::Closed => None,
                Search::Faulting => Some(Err(Fault::engine("the search faulted"))),
                Search::Endless => Some(Ok(Model::of(AnswerSet::new()))),
            }
        }

        fn conclusion(&self) -> Option<Conclusion> {
            matches!(self, Search::Closed).then_some(Conclusion::Exhausted)
        }
    }

    /// How a grounding backend builds the ground program of what it lowers.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Grounds {
        /// One rule per statement of the lowered program, naming it — per
        /// occurrence of the parse at Door A.
        Faithfully,
        /// Those, and one more naming a statement the program never held.
        Inventing,
        /// Faithfully through Door B; through Door A, one more rule naming a
        /// statement the program never held.
        InventingAtDoorA,
        /// One rule per statement, each statement relocated to an origin the
        /// program never had.
        Relocating,
        /// Nothing at all.
        Nothing,
        /// Faithfully, yet the observer answers nothing.
        Withholding,
        /// Faithfully, without declaring the observer.
        Undeclared,
        /// Faithfully, each statement stripped of its provenance.
        Stripping,
        /// Faithfully, yet every search faults.
        FaultingItsSearch,
        /// Faithfully, yet every search runs past its bound.
        SearchingPastItsBound,
    }

    /// A backend exposing a ground program built from the program it lowered —
    /// built here, where a ground program is.
    struct Grounding {
        grounds: Grounds,
        ground: GroundProgram,
    }

    impl Backend for Grounding {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                ground_program: self.grounds != Grounds::Undeclared,
                ..Capabilities::default()
            }
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            let search = match self.grounds {
                Grounds::FaultingItsSearch => Search::Faulting,
                Grounds::SearchingPastItsBound => Search::Endless,
                _ => Search::Closed,
            };
            Ok(Solved::running(
                Box::new(search),
                Scenario::default(),
                ShowRule::default(),
            ))
        }

        fn lower(&mut self, door: Door<'_>) -> Result<(), Fault> {
            let parsed = matches!(door, Door::Parsed(_));
            let (grain, mut statements): (Grain, Vec<(PartKey, WithProvenance<Statement>)>) =
                match door {
                    // Door A, read in source order: each occurrence its own
                    // statement.
                    Door::Parsed(admitted) => (
                        Grain::Occurrence,
                        admitted
                            .statements()
                            .map(|occurrence| {
                                (occurrence.part().clone(), occurrence.statement().clone())
                            })
                            .collect(),
                    ),
                    Door::Program(program) => (
                        Grain::Statement,
                        program
                            .parts()
                            .flat_map(|part| {
                                part.statements()
                                    .map(|node| (part.key().clone(), node.clone()))
                            })
                            .collect(),
                    ),
                };
            if self.grounds == Grounds::Nothing {
                statements.clear();
            }
            if self.grounds == Grounds::Stripping {
                for (_, statement) in &mut statements {
                    *statement = WithProvenance::new(statement.get().clone(), Provenance::empty());
                }
            }
            if self.grounds == Grounds::Relocating {
                for (_, statement) in &mut statements {
                    *statement = WithProvenance::new(
                        statement.get().clone(),
                        Provenance::from(Origin::Constructed),
                    );
                }
            }
            if self.grounds == Grounds::Inventing
                || (self.grounds == Grounds::InventingAtDoorA && parsed)
            {
                let invented = program_of("invented.");
                let base = invented.base();
                let node = base.statements().next().expect("the fact raises").clone();
                statements.push((base.key().clone(), node));
            }
            let rules = (0..statements.len()).map(GroundRule::naming).collect();
            self.ground = GroundProgram::of(grain, statements, rules);
            Ok(())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            (self.grounds != Grounds::Withholding).then_some(&self.ground)
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
    fn a_ground_program_naming_statements_of_the_program_passes() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Faithfully), &corpus());
        assert_eq!(verdict, Verdict::Passed);
    }

    #[test]
    fn a_ground_rule_naming_a_statement_the_program_never_held_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Inventing), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(failure) if failure.breach() == Breach::Misanswered),
            "{verdict}"
        );
    }

    #[test]
    fn a_ground_rule_inventing_a_statement_at_door_a_fails() {
        let verdict =
            ground_program_is_faithful(&mut grounding(Grounds::InventingAtDoorA), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(failure) if failure.breach() == Breach::Misanswered),
            "{verdict}"
        );
    }

    #[test]
    fn a_ground_rule_naming_a_statement_without_an_origin_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Stripping), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(failure) if failure.breach() == Breach::Misanswered),
            "{verdict}"
        );
    }

    #[test]
    fn a_ground_rule_naming_a_statement_at_a_foreign_origin_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Relocating), &corpus());
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

    #[test]
    fn a_declared_observer_answering_nothing_after_a_grounding_fails() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Withholding), &corpus());
        assert!(
            matches!(&verdict, Verdict::Failed(failure) if failure.breach() == Breach::Refused),
            "{verdict}"
        );
    }

    #[test]
    fn an_observer_whose_search_faults_leaves_its_check_undriven() {
        let verdict =
            ground_program_is_faithful(&mut grounding(Grounds::FaultingItsSearch), &corpus());
        assert!(
            matches!(verdict, Verdict::Skipped(Skip::Undriven(_))),
            "{verdict}"
        );
    }

    #[test]
    fn an_observer_whose_search_runs_past_its_bound_leaves_its_check_undriven() {
        let verdict =
            ground_program_is_faithful(&mut grounding(Grounds::SearchingPastItsBound), &corpus());
        assert!(
            matches!(verdict, Verdict::Skipped(Skip::Undriven(_))),
            "{verdict}"
        );
    }

    #[test]
    fn an_undeclared_observer_is_not_held_to_faithfulness() {
        let verdict = ground_program_is_faithful(&mut grounding(Grounds::Undeclared), &corpus());
        assert_eq!(
            verdict,
            Verdict::Skipped(Skip::Undeclared(Capability::GroundProgram))
        );
    }
}
