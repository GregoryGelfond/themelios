//! The diagnostics model (docs/design/base.md §6): a report about
//! source, located by construction, with a stable machine identity.
//! Solve outcomes, faults, and progress events are not diagnostics —
//! they have their own models — and an unlocated report is not a
//! degenerate diagnostic but a different thing.

use std::fmt;

use crate::span::Location;

/// The stable machine identity of one diagnostic kind: a namespace
/// (the emitting tier) and a kebab-case name (base.md §6.1). No
/// numeric codes: at diagnostic scale, the no-magic-numbers policy
/// means the name *is* the identity.
///
/// The constructor is total and `const` — each emitting tier defines
/// its identities as compile-time constants and owns its table.
/// Quality (kebab-case, non-empty, meaningful) and stability are held
/// by each tier snapshot-testing its complete identity table: an
/// identity, once shipped, is stable; renaming is a visible breaking
/// change. This crate defines the type; it deliberately does not
/// police tables it cannot see.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DiagnosticId {
    namespace: &'static str,
    name: &'static str,
}

impl DiagnosticId {
    /// Names one diagnostic kind. Total, `const`; O(1).
    pub const fn new(namespace: &'static str, name: &'static str) -> DiagnosticId {
        DiagnosticId { namespace, name }
    }

    /// The emitting tier's namespace. Total, `const`; O(1).
    pub const fn namespace(self) -> &'static str {
        self.namespace
    }

    /// The kebab-case kind name. Total, `const`; O(1).
    pub const fn name(self) -> &'static str {
        self.name
    }
}

impl fmt::Display for DiagnosticId {
    /// The documented rendering `namespace::name` — contract carried
    /// by the type, stable (base.md §8.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.namespace, self.name)
    }
}

/// Closed. Declared least-severe first, so `Error` is the maximum and
/// worst-first sorting is descending order (base.md §6.2).
///
/// Closedness is a ruled tradeoff, both sides on the page: closed
/// buys every consumer exhaustive matching on the specification's own
/// trichotomy; the price, accepted, is that admitting a later
/// severity (the recorded `Hint` pressure) is a breaking change
/// through every exhaustive match — priced correctly by the pre-1.0
/// stability posture, since this surface will not have frozen before
/// the language-server consumer checkpoint runs. `#[non_exhaustive]`
/// was considered and rejected: it would tax every consumer with a
/// wildcard arm on a closed trichotomy today to hedge a pressure that
/// has no producer yet.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Severity {
    /// Informational, and a real standalone severity — a solver
    /// frontend ships its engine's informational class as its own
    /// face — not merely an attachment role.
    Note,
    /// A defect worth reporting that does not defeat the operation.
    Warning,
    /// A defect that defeats the operation reported on.
    Error,
}

impl fmt::Display for Severity {
    /// The documented lowercase rendering — contract carried by the
    /// type, stable (base.md §8.5).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        f.write_str(word)
    }
}

/// A located message (base.md §6.3). Fields are public: any location
/// with any optional message is a valid label; there is no invariant
/// to guard. Derived ordering is location-first, which is what lets
/// render order be *derived* by position.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Label {
    /// Where the label points.
    pub location: Location,
    /// `None` when the diagnostic's headline already covers it — an
    /// honest absence, not an empty-string sentinel.
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::span::{ByteOffset, Location, Span};

    // The tier-side idiom: identities as compile-time constants.
    const UNEXPECTED: DiagnosticId = DiagnosticId::new("syntax", "unexpected-token");

    #[test]
    fn identity_is_namespace_and_name() {
        assert_eq!(UNEXPECTED.namespace(), "syntax");
        assert_eq!(UNEXPECTED.name(), "unexpected-token");
    }

    #[test]
    fn identity_renders_as_namespace_colon_colon_name() {
        // The documented rendering IS the Display impl — contract,
        // stable (base.md §8.5).
        assert_eq!(UNEXPECTED.to_string(), "syntax::unexpected-token");
    }

    #[test]
    fn identity_orders_by_namespace_then_name() {
        let a = DiagnosticId::new("program", "zzz");
        let b = DiagnosticId::new("syntax", "aaa");
        assert!(a < b);
    }

    #[test]
    fn severity_declares_least_severe_first() {
        assert!(Severity::Note < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        // Worst-first sorting is therefore descending order.
        let mut severities = [Severity::Warning, Severity::Error, Severity::Note];
        severities.sort_by(|a, b| b.cmp(a));
        assert_eq!(
            severities,
            [Severity::Error, Severity::Warning, Severity::Note]
        );
    }

    #[test]
    fn severity_renders_lowercase() {
        // Contract, stable (base.md §8.5).
        assert_eq!(Severity::Note.to_string(), "note");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Error.to_string(), "error");
    }

    #[test]
    fn labels_order_by_location_first() {
        let early = Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(ByteOffset::new(1), ByteOffset::new(2)).expect("ordered endpoints"),
            },
            message: Some("zzz".to_owned()),
        };
        let late = Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(ByteOffset::new(5), ByteOffset::new(6)).expect("ordered endpoints"),
            },
            message: None,
        };
        assert!(early < late);
    }
}
