//! The diagnostics model (docs/design/base.md §6): a report about
//! source, located by construction, with a stable machine identity.
//! Solve outcomes, faults, and progress events are not diagnostics —
//! they have their own models — and an unlocated report is not a
//! degenerate diagnostic but a different thing.

use std::collections::BTreeSet;
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
/// Closed is the contract: consumers match exhaustively on the
/// specification's own trichotomy, and admitting a later severity is
/// a breaking change through every such match — the tradeoff base.md
/// §6.2 rules, with its argument.
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

/// The one refusal construction can issue, carried as the condition
/// itself (base.md §6.4, §3.2): an empty headline would break every
/// view by construction. It is the one structural emptiness this
/// crate refuses — empty attachment strings are admitted unaltered,
/// because accepting a value as-is is not repair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EmptyMessage;

impl fmt::Display for EmptyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the headline message is empty; every view depends on it")
    }
}

impl std::error::Error for EmptyMessage {}

/// A report about source. Located by construction: the primary label
/// is required, so "a diagnostic without a precise span" is
/// unrepresentable (base.md §6.4). Equality and hash are structural —
/// and for the secondary labels that means *set* equality.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Diagnostic {
    id: DiagnosticId,
    severity: Severity,
    /// The headline; never empty.
    message: String,
    primary: Label,
    /// A set, mathematically: render order is derived by position
    /// (`Label`'s ordering is location-first), so emission order
    /// carries no meaning and a duplicate label is a defect —
    /// `BTreeSet` makes duplicates unrepresentable and iteration
    /// deterministic in exactly the derived order (base.md §6.4,
    /// §8.4).
    secondary: BTreeSet<Label>,
    /// A narrative, in order: order is meaning here (base.md §6.3).
    notes: Vec<String>,
    /// Likewise.
    helps: Vec<String>,
}

impl Diagnostic {
    /// Refuses `EmptyMessage`; O(1) beyond the owned text
    /// (base.md §6.4, §9).
    pub fn new(
        id: DiagnosticId,
        severity: Severity,
        message: String,
        primary: Label,
    ) -> Result<Diagnostic, EmptyMessage> {
        if message.is_empty() {
            return Err(EmptyMessage);
        }
        Ok(Diagnostic {
            id,
            severity,
            message,
            primary,
            secondary: BTreeSet::new(),
            notes: Vec::new(),
            helps: Vec::new(),
        })
    }

    /// Adds a secondary label — by-value chaining, so even building
    /// reads as declaring (base.md §8.3). Inserting a label already
    /// present yields the same set: set semantics, not repair. Total;
    /// O(log secondaries). Dropping the result loses the build, so
    /// every by-value builder is must-use.
    #[must_use]
    pub fn with_secondary(mut self, label: Label) -> Diagnostic {
        self.secondary.insert(label);
        self
    }

    /// Appends to the note narrative. Total; O(1) beyond owned text.
    #[must_use]
    pub fn with_note(mut self, note: String) -> Diagnostic {
        self.notes.push(note);
        self
    }

    /// Appends to the help narrative. Total; O(1) beyond owned text.
    #[must_use]
    pub fn with_help(mut self, help: String) -> Diagnostic {
        self.helps.push(help);
        self
    }

    /// The stable machine identity. Total; O(1).
    pub fn id(&self) -> DiagnosticId {
        self.id
    }

    /// The severity. Total; O(1).
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The headline; never empty. Total; O(1).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The required primary label. Total; O(1).
    pub fn primary(&self) -> &Label {
        &self.primary
    }

    /// The secondary labels — a set; iteration is position order.
    /// Total; O(1).
    pub fn secondary(&self) -> &BTreeSet<Label> {
        &self.secondary
    }

    /// The note narrative, in order. Total; O(1).
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The help narrative, in order. Total; O(1).
    pub fn helps(&self) -> &[String] {
        &self.helps
    }
}

/// Tier-typed diagnostics lower into the normal form by reference:
/// the typed value outlives its transport form (base.md §6.5). Each
/// tier defines its *own* fully typed diagnostics and lowers them
/// into this crate's normal form for uniform rendering and transport;
/// in-process consumers act on the tier's typed values, and pipelines
/// that only render or forward take `impl ToDiagnostic` uniformly.
///
/// The name departs the standard conversion vocabulary deliberately:
/// `Into` consumes and says only "can convert"; this trait borrows
/// and declares a semantic relationship — *this value is a diagnostic
/// in tier-typed form*. One method, no provided machinery: a
/// contract, not a framework.
pub trait ToDiagnostic {
    /// This value, in the normal form.
    fn to_diagnostic(&self) -> Diagnostic;
}

impl ToDiagnostic for Diagnostic {
    /// Identity, by clone: the normal form of a `Diagnostic` is
    /// itself.
    fn to_diagnostic(&self) -> Diagnostic {
        self.clone()
    }
}

impl<T: ToDiagnostic + ?Sized> ToDiagnostic for &T {
    fn to_diagnostic(&self) -> Diagnostic {
        (**self).to_diagnostic()
    }
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

    fn label_at(start: u32, end: u32) -> Label {
        Label {
            location: Location {
                source: SourceId::new(0),
                span: Span::new(ByteOffset::new(start), ByteOffset::new(end))
                    .expect("ordered endpoints"),
            },
            message: None,
        }
    }

    fn demo() -> Diagnostic {
        Diagnostic::new(
            UNEXPECTED,
            Severity::Error,
            "expected `.` after the rule body".to_owned(),
            label_at(10, 14),
        )
        .expect("non-empty headline")
    }

    #[test]
    fn construction_refuses_an_empty_headline() {
        assert_eq!(
            Diagnostic::new(UNEXPECTED, Severity::Error, String::new(), label_at(0, 1),),
            Err(EmptyMessage)
        );
    }

    #[test]
    fn chaining_builds_and_accessors_answer() {
        let diagnostic = demo()
            .with_secondary(label_at(2, 5))
            .with_note("the statement began here".to_owned())
            .with_help("add `.`".to_owned());
        assert_eq!(diagnostic.id(), UNEXPECTED);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.message(), "expected `.` after the rule body");
        assert_eq!(diagnostic.primary(), &label_at(10, 14));
        assert_eq!(diagnostic.secondary().len(), 1);
        assert_eq!(diagnostic.notes(), ["the statement began here".to_owned()]);
        assert_eq!(diagnostic.helps(), ["add `.`".to_owned()]);
    }

    #[test]
    fn secondary_labels_are_a_set() {
        // A duplicate insert yields the same set — set semantics, not
        // repair; equality is set equality: emission order carries no
        // meaning (base.md §6.4).
        let once = demo().with_secondary(label_at(2, 5));
        let twice = demo()
            .with_secondary(label_at(2, 5))
            .with_secondary(label_at(2, 5));
        assert_eq!(once, twice);

        let forward = demo()
            .with_secondary(label_at(2, 5))
            .with_secondary(label_at(7, 9));
        let backward = demo()
            .with_secondary(label_at(7, 9))
            .with_secondary(label_at(2, 5));
        assert_eq!(forward, backward);
        // Iteration is deterministic in exactly the derived order:
        // by position.
        let spans: Vec<u32> = forward
            .secondary()
            .iter()
            .map(|label| label.location.span.start().get())
            .collect();
        assert_eq!(spans, [2, 7]);
    }

    #[test]
    fn notes_are_a_narrative_in_order() {
        let diagnostic = demo()
            .with_note("first".to_owned())
            .with_note("second".to_owned());
        assert_eq!(
            diagnostic.notes(),
            ["first".to_owned(), "second".to_owned()]
        );
    }

    #[test]
    fn empty_attachments_are_admitted_unaltered() {
        // Accepting a value as-is is not repair; attachment quality
        // is the emitting tier's obligation (base.md §9).
        let diagnostic = demo().with_note(String::new());
        assert_eq!(diagnostic.notes(), [String::new()]);
    }

    #[test]
    fn lowering_is_identity_for_the_normal_form() {
        let diagnostic = demo();
        assert_eq!(diagnostic.to_diagnostic(), diagnostic);
        // And composes through references, for uniform pipelines.
        let by_ref: &dyn ToDiagnostic = &&diagnostic;
        assert_eq!(by_ref.to_diagnostic(), diagnostic);
    }

    #[test]
    fn equal_diagnostics_hash_equally() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let hash_of = |diagnostic: &Diagnostic| {
            let mut hasher = DefaultHasher::new();
            diagnostic.hash(&mut hasher);
            hasher.finish()
        };
        let forward = demo()
            .with_secondary(label_at(2, 5))
            .with_secondary(label_at(7, 9));
        let backward = demo()
            .with_secondary(label_at(7, 9))
            .with_secondary(label_at(2, 5));
        assert_eq!(hash_of(&forward), hash_of(&backward));
    }

    #[test]
    fn empty_message_displays_the_fixable_question() {
        assert_eq!(
            EmptyMessage.to_string(),
            "the headline message is empty; every view depends on it"
        );
        let _: &dyn std::error::Error = &EmptyMessage;
    }
}
