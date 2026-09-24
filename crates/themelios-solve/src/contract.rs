//! The backend contract (docs/design/solve.md §4): the `Backend` trait, its
//! capability declaration, and the fault and locus vocabulary — the one door an
//! audit reads.
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

use themelios_base::diagnostic::{Diagnostic, DiagnosticId, Label, Severity, ToDiagnostic};

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
}
