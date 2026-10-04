//! The backend contract (docs/design/solve.md §4): the `Backend` trait, its
//! capability declaration, and the fault vocabulary — the one door an audit
//! reads.
//!
//! The [`Backend`] trait (§4.1, §4.3) is the sole crossing between the
//! engine-free core and any engine: three methods required of every backend —
//! the declaration, the solve, and the bridge — and, beyond them, methods
//! required exactly when the matching capability bit is declared, each
//! provided with a default that refuses naming the [`Capability`] it needed,
//! or, for the cancellation primitive and the ground-program observer, answers
//! `None` — so an undeclared capability is a typed refusal or an honest
//! absence at the seam, never a compile burden and never a silent degrade
//! (§4.1).
//!
//! The capability declaration (§4.1) is a closed set of bits and enums a
//! backend answers for itself, read before a request is paid for: a request
//! beyond it is a typed fault, never a silent degrade, and the one
//! refuse-or-derive path — consequences by enumeration — is disclosed there
//! (§4.2). The request-side values (§5.2, §6.3) are typed, `Default` where the
//! empty case is meaningful, and non-exhaustive, so a new knob is a new field,
//! not a breaking change.
//!
//! The fault vocabulary (§5.4) is a value with a closed locus taxonomy at the
//! seam. A [`Fault`] owns its model — where it arose, a message that is never
//! empty, whether it is a backend bug, what it refused, and optionally the
//! engine's typed cause — and renders through `Display`. What it refused is a
//! closed sum keyed by locus, [`Refused`]: a statement or a parse at the
//! program locus, the [`Presupposition`] that failed at the request locus, or
//! nothing, so a consumer acts on a refusal by matching it, never by reading
//! its message. A fault is not, in general, a diagnostic: `base`'s
//! `Diagnostic` is located by construction, and a fault without a source span
//! is not a degenerate diagnostic with a fabricated span but a different
//! thing. [`Fault::diagnostics`] lowers it to as many diagnostics as its
//! source has parsed origins to place them at — none for an unlocated fault.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity};
use themelios_program::program::Part;
use themelios_program::provenance::{Origin, WithProvenance};
use themelios_program::{Statement, Symbol};

use crate::agent::Scenario;
use crate::bridge::{Door, GroundProgram, NotAdmitted};
use crate::extend::{Function, Propagator};
use crate::outcome::{NativeAnswer, Optimized, Solved, Truncation};

// ---- The backend contract (§4.1, §4.3) ----

/// The backend contract (docs/design/solve.md §4.1, §4.3): the sole crossing
/// between the engine-free core and any engine — the one door an audit
/// reads. A backend implements engine mechanism alone; the derived readings —
/// consequences by enumeration (§4.2) and blame (§5.4) — are the core's, over
/// this surface, never a backend author's.
///
/// Three methods are required of every backend: [`capabilities`], [`solve`],
/// and [`lower`]. The design marks the rest required
/// exactly when the matching capability bit is declared — `optimize` under
/// `optimization`; `solve_assuming` under `assumptions`; `ground`,
/// `assign_external`, and `reset` under `multi_shot`; `register_function`
/// under `functions`; `register_propagator` under `propagators`. The trait
/// encodes that obligation as a provided default that refuses: a backend that
/// declares the bit overrides the method, and one that does not inherits the
/// typed refusal — a request beyond the declaration is a [`Fault`] at the
/// request surface, never a silent degrade (§4.1) — with no method to write
/// for a capability it lacks. The remaining methods are provided outright and
/// overridden by a capable engine: [`interrupt`], `None` unless the backend
/// cancels; [`ground_program`], `None` unless the backend declares the
/// observer; [`consequences_native`], refusing unless the backend has the
/// native door.
///
/// Usable as a trait object: the core holds any engine behind this one door.
///
/// [`capabilities`]: Backend::capabilities
/// [`solve`]: Backend::solve
/// [`lower`]: Backend::lower
/// [`ground_program`]: Backend::ground_program
/// [`interrupt`]: Backend::interrupt
/// [`consequences_native`]: Backend::consequences_native
pub trait Backend {
    /// Required. What this backend can do, read before a request is paid for
    /// (§4.1). Pure; O(1).
    fn capabilities(&self) -> Capabilities;

    /// Required. Consistency and enumeration: the handle resolves the
    /// trichotomy and streams the models lazily (§5.2). It enumerates
    /// `solve`'s model set, which ignores any objective (§5.2): the stable
    /// models, as if the program had none, so an engine that optimises a
    /// program with an objective by default owes the enumeration without it.
    fn solve(&mut self, request: &SolveRequest) -> Result<Solved<'_>, Fault>;

    /// Required. The bridge (§10): consume a program through a door. On a
    /// multi-shot backend a repeat `lower` accumulates into the engine's
    /// program, so a rebuild is `reset` then `lower` the amended whole; on a
    /// single-shot backend a `lower` replaces the program, so a rebuild is one
    /// `lower`.
    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault>;

    /// Provided. The ground program the backend exposes — the observer, a
    /// declared capability (§10.4): overridden exactly when
    /// `capabilities().ground_program`, so a backend that does not declare it
    /// writes nothing and inherits the default `None`. What a declaring
    /// backend exposes is complete or absent: `Some` holds the ground program
    /// as the grounder emitted it, from every grounding that finished since the
    /// last `reset` or replacing `lower`; `None` before any has finished, after
    /// a `reset` or a replacing `lower` until the next one finishes, and while
    /// the backend needs a rebuild — never a prefix or a failed grounding's
    /// partial output. The conformance suite (§13.1) holds the declaration and
    /// the attribution's membership.
    fn ground_program(&self) -> Option<&GroundProgram> {
        None
    }

    /// Provided. The engine's cancellation primitive, to cut an in-flight
    /// solve short from another thread — `Some` exactly when
    /// `capabilities().cancellation` (§6.1, §6.3). The default answers `None`,
    /// so a non-cancelling backend inherits it and a cancelling one overrides.
    /// It is the one primitive the request-side time budget (§6.3) and the
    /// agent's interrupt handle — the core's, over it (§6.2) — are realised
    /// over; the conformance suite (§13.1) checks that a declared cancellation
    /// answers `Some`, so the bit cannot lie.
    fn interrupt(&self) -> Option<Box<dyn Cancel>> {
        None
    }

    /// Required under `capabilities().optimization`. The proven optimum, with
    /// the improving trajectory iff the request asks (§5.3); refuses
    /// otherwise.
    fn optimize(&mut self, _request: &OptimizeRequest) -> Result<Optimized<'_>, Fault> {
        Err(Fault::unsupported(Capability::Optimization))
    }

    /// Required under `capabilities().assumptions`. Solve under a scenario
    /// (§6.3): the models of `solve`'s set the scenario admits (§5.2). Blame
    /// (§5.4) is the core's reading over this, not the backend's — its
    /// derivation not yet realised — so there is no separate blame method.
    /// Refuses otherwise.
    fn solve_assuming(
        &mut self,
        _scenario: &Scenario,
        _request: &SolveRequest,
    ) -> Result<Solved<'_>, Fault> {
        Err(Fault::unsupported(Capability::Assumptions))
    }

    /// Required under `capabilities().multi_shot`. Instantiate the named
    /// program parts under the options (§6.2); refuses otherwise.
    fn ground(&mut self, _parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::MultiShot))
    }

    /// Required under `capabilities().multi_shot`. Assign an external atom its
    /// truth value (§6.2). An atom that is not external — every atom, where the
    /// backend declares no externals — is refused as
    /// [`Presupposition::NotExternal`], not with [`Fault::unsupported`]: the
    /// method is there, the atom is not one it assigns. Without `multi_shot`,
    /// the default refuses as unsupported.
    fn assign_external(&mut self, _external: Symbol, _value: TruthValue) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::MultiShot))
    }

    /// Required under `capabilities().multi_shot`. Clear the engine's
    /// accumulated program so the agent can rebuild it — the rebuild-class
    /// retraction path (§6.2), `lower` then reloading the amended whole.
    /// Distinct from `assign_external`, a toggle, and `ground`, an addition;
    /// not called on a single-shot backend, where `lower` replaces. Refuses
    /// otherwise.
    fn reset(&mut self) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::MultiShot))
    }

    /// Required under `capabilities().functions`. Register an `@`-function
    /// for ground-time evaluation (§7); refuses otherwise.
    fn register_function(&mut self, _function: Box<dyn Function>) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::Functions))
    }

    /// Required under `capabilities().propagators`. Register a custom
    /// propagator (§8); refuses otherwise.
    fn register_propagator(&mut self, _propagator: Box<dyn Propagator>) -> Result<(), Fault> {
        Err(Fault::unsupported(Capability::Propagators))
    }

    /// Optional: overridden exactly when `capabilities().native_consequences`
    /// is `Native`. The engine's own cautious or brave door, in one solve,
    /// ranging over the models `solve_assuming(request.scenario)` denotes — the
    /// empty scenario being the unscoped program (§4.2; docs/design/query.md
    /// §2.4). Only a backend that declares `assumptions` is handed a non-empty
    /// scenario: the agent reads that declaration first (§6.2). Absent, the core
    /// derives the consequences by enumeration — over `solve`, or under a
    /// scenario over `solve_assuming` — and the request surface says which path
    /// runs. Refuses by default.
    ///
    /// It reports what the engine's search established, a [`NativeAnswer`]: the
    /// set it computed over a space it closed having seen a model, no model over
    /// a closed space, or where it stopped short. The core builds the
    /// consequences from that answer, or refuses — over no model, and over a
    /// search that did not close the space, as the derived door refuses — so
    /// the two doors refuse alike as they answer alike, the free differential of
    /// `query.md` §2.4. The refusals' decision and wording are the core's; the
    /// report's honesty is the backend's, which the conformance suite checks. A
    /// fault the search raised is the method's own refusal.
    fn consequences_native(
        &mut self,
        _mode: Mode,
        _request: &ConsequenceRequest,
    ) -> Result<NativeAnswer, Fault> {
        Err(Fault::unsupported(Capability::NativeConsequences))
    }
}

/// The engine's cancellation primitive (docs/design/solve.md §4.1, §6.3): one
/// call cuts the in-flight solve short. `Send + Sync`, so the core's timer
/// thread and the caller's handle pull it from another thread. A pull signals
/// and returns — `O(1)`, never blocking on the solving thread — and the stop is
/// read through the run's `conclusion`.
///
/// A pull with no solve in flight is a no-op — never a cancellation of the
/// next question. An implementation over an engine whose own primitive cuts
/// "the active call or the following one" compensates by arming its forward
/// only while a run is open: armed from before the engine's search can begin
/// until it ends, and disarmed no later than it ends, so the window coincides
/// with the engine's active call — a pull inside it is never dropped, and one
/// outside it never reaches the engine. That coincidence is a claim to
/// establish for the pinned engine, with a race harness holding the concurrent
/// open and close.
///
/// The primitive carries no authority over a dropped engine: a pull reaches a
/// slot the backend owns and clears on drop, shared with the handle, never a
/// pointer into the engine — so the engine's lifetime is the backend's, and a
/// handle that outlives its backend holds nothing and cuts nothing.
pub trait Cancel: Send + Sync {
    /// Signal the in-flight solve to stop, and return; a no-op with none in
    /// flight, or once the backend is dropped. `O(1)`.
    fn cancel(&self);
}

// ---- The capability declaration (§4.1, §4.2) ----

/// A backend's declared capabilities (docs/design/solve.md §4.1): the closed
/// set of bits and enums a backend answers for itself, read — `O(1)`, pure —
/// before a request is paid for. A request beyond the declaration is a typed
/// [`Fault`], never a silent degrade; each bit gates the contract method that
/// is the sole engine primitive for its witness.
///
/// The empty declaration — every bit off, consequences derived, no theory, no
/// budget — is `Default`: a backend that declares nothing beyond the required
/// surface, and so refuses every gated request. A backend declares itself by
/// setting the bits it can answer for; a later capability is a new field, not
/// a migration.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the nine bits are distinct capabilities, each gating its own method of the \
              contract (docs/design/solve.md §4.1); folding them into one field would obscure \
              the declaration a consumer reads, not clarify it"
)]
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Capabilities {
    /// Whether `solve` enumerates the answer sets, beyond deciding
    /// consistency (§5.2).
    pub enumeration: bool,
    /// Whether the backend proves optima — `optimize` (§5.3).
    pub optimization: bool,
    /// Which path a consequence request takes: the engine's own door, or
    /// derivation by enumeration over `solve` — `solve_assuming`, under a
    /// scenario (§4.2).
    pub native_consequences: ConsequenceSupport,
    /// Which theories the backend evaluates.
    pub theories: TheorySupport,
    /// Whether the backend honours external atoms, assigned through
    /// `assign_external` — so it presumes `multi_shot`, which provides that
    /// method.
    pub externals: bool,
    /// Whether the backend evaluates `@`-functions — `register_function`
    /// (§7).
    pub functions: bool,
    /// Whether the backend runs custom propagators — `register_propagator`
    /// (§8).
    pub propagators: bool,
    /// Whether the backend keeps its program across solves, so `ground`,
    /// `assign_external`, and `reset` are honoured (§6.2).
    pub multi_shot: bool,
    /// Whether the backend solves under assumptions — `solve_assuming`
    /// (§6.3).
    pub assumptions: bool,
    /// Whether an in-flight solve can be interrupted from another thread —
    /// `interrupt` (§6.1, §6.3).
    pub cancellation: bool,
    /// Which budgets the backend enforces (§6.3).
    pub budgets: BudgetSupport,
    /// Whether the backend exposes the complete ground program — the observer,
    /// `ground_program` (§10.4) — so an explanation client learns before it
    /// lowers whether the backend serves it.
    pub ground_program: bool,
}

/// Which path a consequence request takes (docs/design/solve.md §4.2): the
/// engine's own cautious/brave door, in one solve, or the core's derivation by
/// enumeration over `solve` (`solve_assuming`, under a scenario) — a different
/// computational beast, folding every
/// model — so a cost divergence of that size is legible before the request is
/// paid for, never disclosed only in the receipt. The absent native door is
/// `Default`: a backend that declares none is served by enumeration.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ConsequenceSupport {
    /// The engine computes cautious and brave consequences itself.
    Native,
    /// The core folds the models `solve` — under a scenario, `solve_assuming`
    /// — enumerates.
    #[default]
    DerivedByEnumeration,
}

/// Which theories a backend evaluates (docs/design/solve.md §4.1). Reserved:
/// carries no evaluated theory yet, so the empty declaration is the one there
/// is, and `Default`; a theory the backend evaluates is a new field, not a
/// migration.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TheorySupport {}

/// Which budgets a backend enforces (docs/design/solve.md §6.3): time at
/// minimum, with room for a model-count cap as a new field. `Default` enforces
/// none.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BudgetSupport {
    /// Whether the backend enforces a time budget natively: the request
    /// carries the budget to it (§6.3). Over a backend that declares only
    /// `cancellation`, the core's own timer enforces a budget instead, once
    /// cancellation is realised; over one that declares neither, a budgeted
    /// request refuses.
    pub time: bool,
}

/// A declared capability, named (docs/design/solve.md §4.1) — as a refusal
/// names the one a request needed ([`Fault::unsupported`], §5.4) and a
/// conformance report the one it checked (§13.1). A refusal names one of the six
/// whose method refuses — optimization, native consequences, assumptions,
/// multi-shot, functions, propagators; undeclared cancellation and the observer
/// answer `None` instead, and externals and the time budget name checks, never
/// refusals: a budget nothing realises refuses with
/// [`Presupposition::UnrealisableBudget`] (§6.3). Non-exhaustive, growing with
/// [`Capabilities`].
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
    /// Enforcing a time budget natively — the request's `time` (§6.3).
    TimeBudget,
    /// Evaluating `@`-functions — `register_function` (§7).
    Functions,
    /// Running custom propagators — `register_propagator` (§8).
    Propagators,
    /// Exposing the ground program — `ground_program` (§10.4).
    GroundProgram,
}

impl Capabilities {
    /// Whether this declaration names `capability` — the bit, or the enum value,
    /// a request needing it reads (§4.1): the one reading, for the agent's gate
    /// and the conformance suite's honesty checks alike. Total; O(1).
    pub(crate) fn declares(&self, capability: Capability) -> bool {
        match capability {
            Capability::Optimization => self.optimization,
            Capability::NativeConsequences => {
                self.native_consequences == ConsequenceSupport::Native
            }
            Capability::Assumptions => self.assumptions,
            Capability::MultiShot => self.multi_shot,
            Capability::Externals => self.externals,
            Capability::Cancellation => self.cancellation,
            Capability::TimeBudget => self.budgets.time,
            Capability::Functions => self.functions,
            Capability::Propagators => self.propagators,
            Capability::GroundProgram => self.ground_program,
        }
    }
}

impl fmt::Display for Capability {
    /// The capability, as the noun phrase a refusal and a report both print
    /// (§1.3: every value has a human `Display`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Capability::Optimization => "optimization",
            Capability::NativeConsequences => "native consequences",
            Capability::Assumptions => "assumptions",
            Capability::MultiShot => "multi-shot solving",
            Capability::Externals => "external atoms",
            Capability::Cancellation => "cancellation",
            Capability::TimeBudget => "a time budget",
            Capability::Functions => "@-functions",
            Capability::Propagators => "propagators",
            Capability::GroundProgram => "the ground program",
        })
    }
}

// ---- The request-side values (§5.2, §6.3) ----

/// The ask `solve` serves (docs/design/solve.md §5.2): enumerate the answer
/// sets, or decide consistency. Empty is the pristine ask — the abstract
/// object alone, no budget — and is `Default`; a new knob is a new field, not
/// a breaking change. The long tail of engine parameters is not here: it is
/// grown on demand behind typed knobs, never a string passthrough (§6.3).
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SolveRequest {
    /// The time budget, when the ask carries one (§6.3): enforcement is a
    /// declared capability (`Capabilities::budgets`), and a hit budget resolves
    /// as what it is, `Conclusion::Budget`, never as a clean end. A backend that
    /// does not declare enforcing one refuses a request carrying one at the
    /// request surface — `solve` and `solve_assuming` alike — never solving
    /// without it: the silent degrade §4.1 forbids.
    pub time: Option<Duration>,
}

/// The ask `optimize` serves (docs/design/solve.md §5.3): the proven optimum,
/// with the improving trajectory reported iff asked. Empty — the optimum
/// alone — is `Default`.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OptimizeRequest {
    /// Whether the improving sequence of optima is reported beside the proven
    /// optimum (§5.3).
    pub report_trajectory: bool,
}

/// A cautious or brave consequence request (docs/design/solve.md §5.2): the
/// scenario the consequences range over, so the agent's scoped consequence
/// doors (§6.2) range over that scenario's models, never the unscoped program
/// (docs/design/query.md §2.4). Empty — the unscoped program, under
/// no assumption — is `Default`. The mode is the method's own parameter, not
/// carried here a second time.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ConsequenceRequest {
    /// The assumptions the consequences range over; an empty scenario is the
    /// unscoped program.
    pub scenario: Scenario,
}

/// The options `ground` instantiates a set of program parts under
/// (docs/design/solve.md §4.1, §6.3). Reserved: carries no parameter yet, so
/// the empty options are the ones there are, and `Default`; a parameter is a
/// new field, not a migration.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GroundOptions {}

/// The value an external atom is assigned — `assign_external`
/// (docs/design/solve.md §4.1): the three values of clingo's
/// `clingo_truth_value_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TruthValue {
    /// The atom holds.
    True,
    /// The atom does not hold.
    False,
    /// The atom's truth value is left open.
    Free,
}

/// Which consequences are asked for (docs/design/solve.md §5.2): cautious —
/// what holds in every answer set, the intersection — or brave — what holds
/// in some answer set, the union. A `Consequences` value carries the mode
/// that produced it, so a value that has travelled still says which question
/// it answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// The intersection of the answer sets: what holds in every one.
    Cautious,
    /// The union of the answer sets: what holds in some one.
    Brave,
}

// ---- The fault vocabulary (§5.4) ----

/// Where a fault arose — the closed taxonomy at the seam (docs/design/solve.md
/// §5.4). Closed is the contract: a consumer matches exhaustively on the five
/// loci, and admitting a sixth is a visible breaking change.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Locus {
    /// The program: a statement the backend refuses — at `lower`, or where its
    /// engine meets it — or a parse refused at Door A (§10.2). Such a fault
    /// refers to its source, and is located where that source carries a
    /// parsed origin.
    Program,
    /// The request: a presupposition of it that fails — a capability the
    /// backend does not declare (§4.1), a stale statement handle or a spent
    /// observation (§6.2), a reading over no model (§5.2), each a
    /// [`Presupposition`] the fault names.
    Request,
    /// A resource: a limit of the environment reached while the request was
    /// being served — a backend's own configured ceiling, or an allocation
    /// failure, never the request's budget (docs/design/solve.md §5.1).
    Resource,
    /// The engine: a failure the engine itself reported, carried verbatim.
    Engine,
    /// The adapter: the seam between the contract and the engine — where a
    /// backend contract violation is located.
    Adapter,
}

impl fmt::Display for Locus {
    /// The locus, as the word a sentence names it by (docs/design/solve.md
    /// §1.3: every value has a human `Display`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Locus::Program => "program",
            Locus::Request => "request",
            Locus::Resource => "resource",
            Locus::Engine => "engine",
            Locus::Adapter => "adapter",
        })
    }
}

/// The solve tier's diagnostic namespace (docs/design/base.md §6.1).
const SOLVE: &str = "solve";

impl Locus {
    /// The stable machine identity a located fault at this locus lowers under
    /// (docs/design/base.md §6.1): one `solve`-namespace identity per locus,
    /// named for the locus, so the table is total over the closed taxonomy and
    /// a consumer keys on the locus it already matches. Total, `const`; O(1).
    const fn diagnostic_id(self) -> DiagnosticId {
        let name = match self {
            Locus::Program => "program-fault",
            Locus::Request => "request-fault",
            Locus::Resource => "resource-fault",
            Locus::Engine => "engine-fault",
            Locus::Adapter => "adapter-fault",
        };
        DiagnosticId::new(SOLVE, name)
    }
}

/// Why a request was refused: a presupposition of it that fails
/// (docs/design/solve.md §5.4), named so a consumer acts on it by matching —
/// retrying under a larger budget, routing to another backend, rebuilding —
/// never by reading the message. Each variant is a refusal the tier makes, at
/// the site cited. Non-exhaustive: a request refused for a reason not yet named
/// gains a variant, never a message to tell it by.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Presupposition {
    /// A capability the backend does not declare, one whose method refuses
    /// (§4.1).
    Unsupported(Capability),
    /// The backend refuses until its rebuild (§4.1, a backend's own state).
    NeedsRebuild,
    /// A reading over no model: the program, or its scenario, admits none
    /// (§5.2).
    NoAnswerSet,
    /// A reading that needs a closed space, over a search stopped short at this
    /// truncation (§5.2).
    Unclosed(Truncation),
    /// A complete collection, from a handle whose models were streamed (§5.2).
    Taken,
    /// A statement handle naming nothing live in this agent's knowledge (§6.2).
    NotLive,
    /// An observation already forgotten, or another agent's (§6.2).
    Spent,
    /// An observed fact that is not an atom (§6.2, §7.3).
    NotAnAtom,
    /// A truth value assigned to an atom that is not external (§6.2).
    NotExternal,
    /// A budget the backend neither enforces nor lets the core enforce (§6.3).
    UnrealisableBudget,
}

/// What a fault refused — closed, one of four, keyed by locus
/// (docs/design/solve.md §5.4). Closed on purpose: a row grows a typed reason
/// inside the sum when a consumer first reads one — the `Statement` row, a
/// router's reason why a backend refused the statement (§12, §14) — never a
/// reason held beside it. A view into the fault, freely copied.
#[derive(Clone, Copy, Debug)]
pub enum Refused<'a> {
    /// A program fault refusing a statement, with its provenance.
    Statement(&'a WithProvenance<Statement>),
    /// A program fault refusing a parse at Door A, its refusal carried whole
    /// (§10.2).
    Parse(&'a NotAdmitted),
    /// A request fault: the presupposition that failed.
    Request(Presupposition),
    /// A resource, engine, or adapter fault, which refused nothing it can name.
    Nothing,
}

/// What a fault refused, owned — the storage behind [`Refused`]. A refused
/// statement or parse is boxed, so a fault that refused nothing stays small.
#[derive(Clone, Debug)]
enum Refusal {
    Statement(Box<WithProvenance<Statement>>),
    Parse(Box<NotAdmitted>),
    Request(Presupposition),
    Nothing,
}

impl PartialEq for Refusal {
    /// A refused statement by its content and its origins — two statements at
    /// different locations differ, annotations alone do not, where the
    /// carrier's own equality compares content alone (docs/design/program.md
    /// §6.2); a refused parse by its diagnostics; a request by its
    /// presupposition. O(statement), or O(diagnostics).
    fn eq(&self, other: &Refusal) -> bool {
        match (self, other) {
            (Refusal::Statement(one), Refusal::Statement(two)) => {
                one.get() == two.get() && one.provenance().origins().eq(two.provenance().origins())
            }
            (Refusal::Parse(one), Refusal::Parse(two)) => one == two,
            (Refusal::Request(one), Refusal::Request(two)) => one == two,
            (Refusal::Nothing, Refusal::Nothing) => true,
            _ => false,
        }
    }
}

/// A backend or request fault (docs/design/solve.md §5.4). Reserved for engine
/// and request failures: an inconsistent or inconclusive program is a
/// `Determination` value (§5.1), never a fault.
///
/// A fault owns its model — its [`Locus`], a message that is never empty, the
/// closed backend-bug bit, what it refused ([`Fault::refused`]), and,
/// optionally, the engine's typed cause ([`Fault::caused_by`]) — renders
/// through `Display`, and lowers to zero, one, or several diagnostics
/// ([`Fault::diagnostics`]). Equality compares the message, the locus, the
/// bit, and what was refused, never the cause, which is detail, not identity;
/// `Fault` is not `Hash` — a report, not a key. Clone is `O(statement)`, or
/// `O(diagnostics)` for a refused parse, the cause shared. Owned plain data
/// (`Send + Sync + 'static`).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Fault {
    locus: Locus,
    /// The headline; never empty.
    message: String,
    /// What the fault refused, keyed by its locus.
    refusal: Refusal,
    /// Whether the fault is a backend contract violation.
    backend_bug: bool,
    /// The engine's own typed failure, shared and opaque, where one was
    /// attached.
    cause: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

impl PartialEq for Fault {
    /// The message, the locus, the bit, and what was refused — never the cause
    /// (§5.4).
    fn eq(&self, other: &Fault) -> bool {
        self.locus == other.locus
            && self.message == other.message
            && self.backend_bug == other.backend_bug
            && self.refusal == other.refusal
    }
}

impl Eq for Fault {}

/// The headline a fault carries when it was raised without one. The adapter
/// builds faults from engine strings it does not control, and a headline is
/// what every view of a fault leads with (docs/design/base.md §6.4), so an
/// empty one is replaced at construction: a fault is never refused for
/// wanting a message, and never lowers to a headline-less diagnostic.
const NO_MESSAGE: &str = "a fault was raised without a message";

impl Fault {
    /// The one constructor every door routes through: it holds the invariant
    /// that the message is never empty, so a located fault always lowers, and
    /// starts with no cause. Total; O(message).
    fn new(locus: Locus, message: impl Into<String>, refusal: Refusal, backend_bug: bool) -> Fault {
        let mut message = message.into();
        if message.is_empty() {
            message.push_str(NO_MESSAGE);
        }
        Fault {
            locus,
            message,
            refusal,
            backend_bug,
            cause: None,
        }
    }

    /// A statement the backend refuses — outside its language at `lower`, or
    /// where its engine meets it (§5.4) — kept whole, so no span is made up and
    /// no statement goes unnamed; located iff it carries a parsed origin. A
    /// refused parse's door is `From<NotAdmitted>` (§10.2). Total;
    /// O(message + statement).
    pub fn program(message: impl Into<String>, statement: &WithProvenance<Statement>) -> Fault {
        Fault::new(
            Locus::Program,
            message,
            Refusal::Statement(Box::new(statement.clone())),
            false,
        )
    }

    /// A request whose presupposition fails, naming it (§5.4). Total;
    /// O(message).
    pub fn request(message: impl Into<String>, presupposition: Presupposition) -> Fault {
        Fault::new(
            Locus::Request,
            message,
            Refusal::Request(presupposition),
            false,
        )
    }

    /// A request beyond the backend's declared capabilities (§4.1), naming the
    /// one it needed: the typed refusal every capability-gated method issues
    /// when its bit is off. Not a backend bug — the capability was declared,
    /// and read, before the request was paid for. Total; O(1).
    pub fn unsupported(capability: Capability) -> Fault {
        Fault::new(
            Locus::Request,
            format!("this backend does not declare {capability}"),
            Refusal::Request(Presupposition::Unsupported(capability)),
            false,
        )
    }

    /// A limit of the environment reached, with the limit named: a backend's
    /// own configured ceiling — a grounding size, a work count, a storage
    /// bound, the width of a representation — or an allocation failure, never
    /// the request's time budget, which a run reports as `Conclusion::Budget`
    /// (docs/design/solve.md §5.1). The engine's typed cause rides with it
    /// through [`caused_by`](Fault::caused_by), for a caller to downcast rather
    /// than parse the message. Total; O(message).
    pub fn resource(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Resource, message, Refusal::Nothing, false)
    }

    /// A failure the engine reported, carried verbatim. Total; O(message).
    pub fn engine(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Engine, message, Refusal::Nothing, false)
    }

    /// A backend contract violation — the one door that sets
    /// [`Fault::is_backend_bug`]. Total; O(message).
    pub fn adapter_bug(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Adapter, message, Refusal::Nothing, true)
    }

    /// This fault, carrying the engine's own typed failure, shared and opaque:
    /// [`Error::source`](std::error::Error::source) returns it for the caller
    /// who downcasts it — no engine type in the signature, and not part of
    /// equality (§5.4). Total; O(1).
    #[must_use]
    pub fn caused_by(self, cause: impl std::error::Error + Send + Sync + 'static) -> Fault {
        Fault {
            cause: Some(Arc::new(cause)),
            ..self
        }
    }

    /// Where the fault arose. Total; O(1).
    pub fn locus(&self) -> Locus {
        self.locus
    }

    /// Whether the fault is a backend contract violation — the closed bit
    /// (§5.4), set by [`Fault::adapter_bug`] alone. Total; O(1).
    pub fn is_backend_bug(&self) -> bool {
        self.backend_bug
    }

    /// What the fault refused: a statement, a parse, the request, or nothing.
    /// Total; O(1).
    pub fn refused(&self) -> Refused<'_> {
        match &self.refusal {
            Refusal::Statement(statement) => Refused::Statement(statement),
            Refusal::Parse(refusal) => Refused::Parse(refusal),
            Refusal::Request(presupposition) => Refused::Request(*presupposition),
            Refusal::Nothing => Refused::Nothing,
        }
    }

    /// The fault lowered to base diagnostics (§5.4): none for an unlocated
    /// fault; one for a refused statement with a parsed origin — the least such
    /// origin its primary label, any others secondaries (docs/design/program.md
    /// §6.3), under the locus's `solve`-space identity, as an error, since a
    /// fault defeats the operation it reports on; one per diagnostic for a
    /// refused parse. Total; O(statement), or O(diagnostics).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        match &self.refusal {
            Refusal::Statement(statement) => {
                let mut parsed =
                    statement
                        .provenance()
                        .origins()
                        .filter_map(|origin| match origin {
                            Origin::Parsed(location) => Some(*location),
                            Origin::Constructed | Origin::Transformed(_) => None,
                        });
                let Some(primary) = parsed.next() else {
                    return Vec::new();
                };
                let diagnostic = Diagnostic::new(
                    self.locus.diagnostic_id(),
                    Severity::Error,
                    self.message.clone(),
                    Label {
                        location: primary,
                        message: None,
                    },
                )
                .expect("a fault's message is never empty: construction replaces an empty one");
                vec![parsed.fold(diagnostic, |diagnostic, location| {
                    diagnostic.with_secondary(Label {
                        location,
                        message: None,
                    })
                })]
            }
            Refusal::Parse(refusal) => refusal.diagnostics(),
            Refusal::Request(_) | Refusal::Nothing => Vec::new(),
        }
    }
}

impl fmt::Display for Fault {
    /// The message — how an unlocated fault renders (§5.4).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Fault {
    /// The engine's own typed failure, where one was attached (§5.4).
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

impl From<NotAdmitted> for Fault {
    /// A program fault refusing the parse, carrying the refusal whole (§10.2):
    /// the program text is where it lies. Total; O(1) beyond the message.
    fn from(refusal: NotAdmitted) -> Fault {
        Fault::new(
            Locus::Program,
            refusal.to_string(),
            Refusal::Parse(Box::new(refusal)),
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The complete identity table, pinned (docs/design/base.md §6.1): one
    /// `solve`-namespace, kebab-case identity per locus, distinct across the
    /// closed taxonomy. An identity, once shipped, is stable.
    #[test]
    fn every_locus_has_its_own_solve_namespace_identity() {
        let table = [
            (Locus::Program, "solve::program-fault"),
            (Locus::Request, "solve::request-fault"),
            (Locus::Resource, "solve::resource-fault"),
            (Locus::Engine, "solve::engine-fault"),
            (Locus::Adapter, "solve::adapter-fault"),
        ];
        for (locus, rendered) in table {
            assert_eq!(locus.diagnostic_id().to_string(), rendered);
        }
    }

    /// A consequence request built here, in the defining crate, by the struct
    /// expression a non-exhaustive struct admits nowhere else: over the
    /// unscoped scenario.
    fn some_consequence_request() -> ConsequenceRequest {
        ConsequenceRequest {
            scenario: Scenario::default(),
        }
    }

    #[test]
    fn a_consequence_request_carries_the_scenario_it_ranges_over() {
        assert_eq!(some_consequence_request().scenario, Scenario::default());
    }

    #[test]
    fn the_default_consequence_request_ranges_over_the_unscoped_program() {
        assert_eq!(ConsequenceRequest::default().scenario, Scenario::default());
    }

    #[test]
    fn a_cloned_consequence_request_equals_its_original() {
        let request = some_consequence_request();
        assert_eq!(request.clone(), request);
    }

    #[test]
    fn a_consequence_request_s_debug_view_names_its_scenario() {
        let rendered = format!("{:?}", some_consequence_request());
        assert!(rendered.contains("scenario"), "{rendered}");
    }

    /// A backend that implements the required surface alone, so the provided
    /// default of `consequences_native` is the one it inherits — exercised
    /// here, in the defining crate, because the request it takes is built
    /// here.
    struct Nothing;

    impl Backend for Nothing {
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }

        fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
            Err(Fault::engine("this backend solves nothing"))
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Err(Fault::engine("this backend lowers nothing"))
        }
    }

    #[test]
    fn each_locus_renders_the_word_a_sentence_names_it_by() {
        for (locus, word) in [
            (Locus::Program, "program"),
            (Locus::Request, "request"),
            (Locus::Resource, "resource"),
            (Locus::Engine, "engine"),
            (Locus::Adapter, "adapter"),
        ] {
            assert_eq!(locus.to_string(), word);
        }
    }

    #[test]
    fn consequences_native_refuses_without_the_native_door() {
        let mut nothing = Nothing;
        // The declaration names the derived path (§4.2), so the native door
        // is the refusal.
        assert_eq!(
            nothing.capabilities().native_consequences,
            ConsequenceSupport::DerivedByEnumeration
        );
        let refused = nothing
            .consequences_native(Mode::Cautious, &some_consequence_request())
            .err();
        assert_eq!(
            refused,
            Some(Fault::unsupported(Capability::NativeConsequences))
        );
    }

    #[test]
    fn a_backend_without_the_observer_exposes_no_ground_program() {
        // The provided default: a backend that does not declare the observer
        // writes nothing and answers nothing (§10.4).
        assert!(!Nothing.capabilities().ground_program);
        assert!(Nothing.ground_program().is_none());
    }
}
