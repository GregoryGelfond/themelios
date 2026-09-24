//! The backend contract (docs/design/solve.md §4): the `Backend` trait, its
//! capability declaration, and the fault and locus vocabulary — the one door an
//! audit reads.
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

use crate::agent::Scenario;

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
    /// derivation by enumeration over `solve` (§4.2).
    pub native_consequences: ConsequenceSupport,
    /// Which theories the backend evaluates.
    pub theories: TheorySupport,
    /// Whether the backend honours external atoms, assigned through
    /// `assign_external`.
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
/// enumeration over `solve` — a different computational beast, folding every
/// model — so a cost divergence of that size is legible before the request is
/// paid for, never disclosed only in the receipt. The absent native door is
/// `Default`: a backend that declares none is served by enumeration.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ConsequenceSupport {
    /// The engine computes cautious and brave consequences itself.
    Native,
    /// The core folds the models `solve` enumerates.
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
    /// declared capability, and a hit budget resolves as what it is,
    /// `Conclusion::Budget`, never as a clean end.
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
/// scenario the consequences range over, so a scenario-scoped world view's
/// cautious and brave range over that scenario's models, never the unscoped
/// program (docs/design/query.md §2.4). The mode is the method's own
/// parameter, not carried here a second time.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
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

    /// A consequence request is built here, in the defining crate: it has no
    /// `Default` — its scenario's constructors are the agent's — and a
    /// non-exhaustive struct is not built by a struct expression elsewhere.
    fn some_consequence_request() -> ConsequenceRequest {
        ConsequenceRequest { scenario: Scenario }
    }

    #[test]
    fn a_consequence_request_carries_the_scenario_it_ranges_over() {
        assert_eq!(some_consequence_request().scenario, Scenario);
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
}
