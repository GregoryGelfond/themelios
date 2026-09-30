//! The backend contract (docs/design/solve.md §4): the `Backend` trait, its
//! capability declaration, and the fault and locus vocabulary — the one door an
//! audit reads.
//!
//! The [`Backend`] trait (§4.1, §4.3) is the sole crossing between the
//! engine-free core and any engine: four methods required of every backend —
//! the declaration, the solve, the bridge, and the ground-program observer —
//! and, beyond them, methods required exactly when the matching capability
//! bit is declared, each provided with a default that refuses, so an
//! undeclared capability is a typed refusal at the seam, never a compile
//! burden and never a silent degrade (§4.1).
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
//! empty, a source label only where the fault has one, and whether it is a
//! backend bug — and renders through `Display`. It is not, in general, a
//! diagnostic: `base`'s `Diagnostic` is located by construction, and a fault
//! without a source span (an engine, resource, or adapter fault) is not a
//! degenerate diagnostic with a fabricated span but a different thing. Only a
//! [`LocatedFault`], reached through [`Fault::located`], lowers to a
//! `Diagnostic` — under a real span, never an invented one.

use std::fmt;
use std::time::Duration;

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity, ToDiagnostic};
use themelios_program::Symbol;
use themelios_program::program::Part;

use crate::agent::Scenario;
use crate::bridge::{Door, GroundProgram};
use crate::extend::{Function, Propagator};
use crate::outcome::{NativeAnswer, Optimized, Solved};

// ---- The backend contract (§4.1, §4.3) ----

/// The backend contract (docs/design/solve.md §4.1, §4.3): the sole crossing
/// between the engine-free core and any engine — the one door an audit
/// reads. A backend implements engine mechanism alone; the derived readings —
/// consequences by enumeration (§4.2) and blame (§5.4) — are the core's, over
/// this surface, never a backend author's.
///
/// Four methods are required of every backend: [`capabilities`], [`solve`],
/// [`lower`], and [`ground_program`]. The design marks the rest required
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
/// cancels; [`consequences_native`], refusing unless the backend has the
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
    /// program — the assert path lowers only the delta (§6.2) — so a rebuild
    /// is `reset` then `lower` the amended whole; on a single-shot backend a
    /// `lower` replaces the program, so a rebuild is one `lower`.
    fn lower(&mut self, door: Door<'_>) -> Result<(), Fault>;

    /// Required. The ground program the backend exposes — the committed
    /// observer (§10.4); `None` when it has none to expose.
    fn ground_program(&self) -> Option<&GroundProgram>;

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
        Err(Fault::unsupported())
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
        Err(Fault::unsupported())
    }

    /// Required under `capabilities().multi_shot`. Instantiate the named
    /// program parts under the options (§6.2); refuses otherwise.
    fn ground(&mut self, _parts: &[Part], _options: &GroundOptions) -> Result<(), Fault> {
        Err(Fault::unsupported())
    }

    /// Required under `capabilities().multi_shot`. Assign an external atom its
    /// truth value (§6.2). An atom that is not external — every atom, where the
    /// backend declares no externals — is refused at the request surface, not
    /// with [`Fault::unsupported`]: the method is there, the atom is not one it
    /// assigns. Without `multi_shot`, the default refuses as unsupported.
    fn assign_external(&mut self, _external: Symbol, _value: TruthValue) -> Result<(), Fault> {
        Err(Fault::unsupported())
    }

    /// Required under `capabilities().multi_shot`. Clear the engine's
    /// accumulated program so the agent can rebuild it — the rebuild-class
    /// retraction path (§6.2), `lower` then reloading the amended whole.
    /// Distinct from `assign_external`, a toggle, and `ground`, an addition;
    /// not called on a single-shot backend, where `lower` replaces. Refuses
    /// otherwise.
    fn reset(&mut self) -> Result<(), Fault> {
        Err(Fault::unsupported())
    }

    /// Required under `capabilities().functions`. Register an `@`-function
    /// for ground-time evaluation (§7); refuses otherwise.
    fn register_function(&mut self, _function: Box<dyn Function>) -> Result<(), Fault> {
        Err(Fault::unsupported())
    }

    /// Required under `capabilities().propagators`. Register a custom
    /// propagator (§8); refuses otherwise.
    fn register_propagator(&mut self, _propagator: Box<dyn Propagator>) -> Result<(), Fault> {
        Err(Fault::unsupported())
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
        Err(Fault::unsupported())
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
/// only while a run is open, which suffices only where its notion of *open*
/// closes no later than the engine's own search — a claim to establish for the
/// pinned engine, with a race harness holding the concurrent close.
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
// The eight bits are distinct capabilities, each gating its own method of the
// contract (docs/design/solve.md §4.1); folding them into one field would
// obscure the declaration a consumer reads, not clarify it.
#[allow(clippy::struct_excessive_bools)]
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
    /// Whether the backend enforces a time budget — natively, or through the
    /// `interrupt` primitive on a timer (§6.3).
    pub time: bool,
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
    /// The program: a statement the backend could not lower or ground. Such a
    /// fault carries the statement's source location, so it lowers to a
    /// diagnostic.
    Program,
    /// The request: an ask beyond the backend's declared capabilities (§4.1),
    /// or a request-side value that cannot be honoured — a stale statement
    /// handle, a spent observation (§6.2), a non-pattern where a pattern is
    /// asked for (docs/design/query.md §2.5).
    Request,
    /// A resource: a limit of the environment reached while the request was
    /// being served.
    Resource,
    /// The engine: a failure the engine itself reported, carried verbatim.
    Engine,
    /// The adapter: the seam between the contract and the engine — where a
    /// backend contract violation is located.
    Adapter,
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

/// A backend or request fault (docs/design/solve.md §5.4). Reserved for
/// engine and request failures: an inconsistent or inconclusive program is a
/// `Determination` value (§5.1), never a fault.
///
/// A fault owns its model — its [`Locus`], a message that is never empty, a
/// source label only where it has one, and the closed backend-bug bit — and
/// renders through `Display`. It lowers to a `base::Diagnostic` only where it
/// is located, through [`Fault::located`]: a fault without a source span is
/// not a degenerate diagnostic. Owned plain data (`Send + Sync + 'static`).
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fault {
    locus: Locus,
    /// The headline; never empty.
    message: String,
    /// The source label, present only where the fault has a source location
    /// (a program fault).
    label: Option<Label>,
    /// Whether the fault is a backend contract violation.
    backend_bug: bool,
}

/// The headline a fault carries when it was raised without one. The adapter
/// builds faults from engine strings it does not control, and a headline is
/// what every view of a fault leads with (docs/design/base.md §6.4), so an
/// empty one is replaced at construction: a fault is never refused for
/// wanting a message, and never lowers to a headline-less diagnostic.
const NO_MESSAGE: &str = "a fault was raised without a message";

impl Fault {
    /// The one constructor every door routes through: it holds the invariant
    /// that the message is never empty, so a located fault always lowers.
    /// Total; O(message).
    fn new(
        locus: Locus,
        message: impl Into<String>,
        label: Option<Label>,
        backend_bug: bool,
    ) -> Fault {
        let mut message = message.into();
        if message.is_empty() {
            message.push_str(NO_MESSAGE);
        }
        Fault {
            locus,
            message,
            label,
            backend_bug,
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

    /// The located form of the fault: `Some` exactly when the fault carries a
    /// source location (a program fault), so a diagnostic is only ever
    /// lowered under a real span; `None` for a fault that renders through
    /// `Display` alone. Total; O(1).
    pub fn located(&self) -> Option<LocatedFault<'_>> {
        self.label
            .as_ref()
            .map(|label| LocatedFault { fault: self, label })
    }

    /// A request beyond the backend's declared capabilities (§4.1): the typed
    /// refusal every capability-gated operation issues when its bit is off.
    /// Not a backend bug — the capability was declared, and read, before the
    /// request was paid for. Total; O(1).
    pub fn unsupported() -> Fault {
        Fault::new(Locus::Request, "unsupported request", None, false)
    }

    /// A failure the engine reported, carried verbatim. Total; O(message).
    pub fn engine(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Engine, message, None, false)
    }

    /// A request that cannot be honoured, with the reason. Total; O(message).
    pub fn request(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Request, message, None, false)
    }

    /// A limit of the environment reached, with the limit named. Total;
    /// O(message).
    pub fn resource(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Resource, message, None, false)
    }

    /// A backend contract violation — the one door that sets
    /// [`Fault::is_backend_bug`]. Total; O(message).
    pub fn adapter_bug(message: impl Into<String>) -> Fault {
        Fault::new(Locus::Adapter, message, None, true)
    }

    /// A statement the backend could not lower or ground, with the statement's
    /// source label — the located fault, the one that lowers to a diagnostic.
    /// Total; O(message).
    pub fn program(message: impl Into<String>, label: Label) -> Fault {
        Fault::new(Locus::Program, message, Some(label), false)
    }
}

impl fmt::Display for Fault {
    /// The message — how an unlocated fault renders (§5.4).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Fault {}

/// A fault that carries a source location — the only form of a fault that is
/// a `base::Diagnostic` (docs/design/solve.md §5.4). Reached through
/// [`Fault::located`], so the label is present by construction and the
/// lowering never invents a span. A view: two borrows, freely copied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LocatedFault<'a> {
    fault: &'a Fault,
    label: &'a Label,
}

impl<'a> LocatedFault<'a> {
    /// The label the fault carries — its source location, with a message where
    /// it has one — readable without lowering to a diagnostic. Total; O(1).
    pub fn label(&self) -> &'a Label {
        self.label
    }
}

impl ToDiagnostic for LocatedFault<'_> {
    /// The fault in `base`'s normal form (docs/design/base.md §6.5): the
    /// locus's `solve`-space identity, the fault's message as the headline,
    /// its own label as the primary — an error, since a fault defeats the
    /// operation it reports on. Total; O(message + label).
    fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            self.fault.locus.diagnostic_id(),
            Severity::Error,
            self.fault.message.clone(),
            self.label.clone(),
        )
        .expect("a fault's message is never empty: construction replaces an empty one")
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
            Err(Fault::unsupported())
        }

        fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
            Err(Fault::unsupported())
        }

        fn ground_program(&self) -> Option<&GroundProgram> {
            None
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
        assert_eq!(refused, Some(Fault::unsupported()));
    }

    #[test]
    fn a_backend_with_nothing_lowered_exposes_no_ground_program() {
        assert!(Nothing.ground_program().is_none());
    }
}
