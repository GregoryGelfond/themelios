//! The bridge and the seam (docs/design/solve.md §10): the two doors a program
//! enters a backend by — a parse the core admitted, or a `Program` — the typed
//! refusal of a parse at Door A, and the ground-program IR.
//!
//! The doors carry the language's objects (§10.2), never an engine's format: an
//! engine's own input format, with the identifiers it numbers and any interning
//! discipline its engine needs, belongs to the adapter whose engine reads it
//! (§10.3, §11). The `Symbol` correspondence (§10.5): `themelios_program::Symbol`
//! carries the engine's own number width (`i32`, docs/design/program.md §3.1),
//! so no value is lost or reshaped crossing the seam — the correspondence is
//! total on the `i32` range, and no lowering refuses a valid number.

use std::fmt;

use themelios_base::diagnostic::{Diagnostic, Severity, ToDiagnostic};
use themelios_program::raise::{LowerError, Occurrences, StatementOccurrence, raise_occurrences};
use themelios_program::{Origin, Program};
use themelios_syntax::{Parse, SyntaxError, ast};

/// The entry values a backend lowers (docs/design/solve.md §10.2): a parse the
/// core admitted, or a `Program`. Every backend takes both — it reads Door A
/// in source order or through [`Door::program`] — so the set is no capability;
/// it is closed, so a match over the doors is exhaustive without a wildcard,
/// and a new grade would be a new variant every backend's
/// [`lower`](crate::contract::Backend::lower) answers.
///
/// **Never text: nothing renders a program and parses it back across the
/// seam** (§10.2). Each door carries a typed value, so rendered text has no
/// door to enter, and an engine's own input format belongs to its adapter
/// (§10.3), never to a door.
#[derive(Clone, Copy, Debug)]
pub enum Door<'a> {
    /// Door A — a parse, admitted: its statements in source order, provenance
    /// intact.
    Parsed(&'a Admitted),
    /// Door B — a `Program`, canonical, carrying `Origin` provenance on every
    /// statement, ground or not: the primary entry. Programs constructed in
    /// Rust, transformed, or loaded through a client enter here.
    Program(&'a Program),
}

impl<'a> Door<'a> {
    /// The program the door carries, as a set — Door B's own, or the one Door
    /// A's statements collected to at admission — for a backend that reads a
    /// program as a set rather than in source order. Total; O(1).
    pub fn program(&self) -> &'a Program {
        match self {
            Door::Parsed(admitted) => &admitted.program,
            Door::Program(program) => program,
        }
    }
}

/// A parse the core admitted at Door A (docs/design/solve.md §10.2): its
/// statements raised once, in source order, each with its part and provenance
/// (docs/design/program.md §8), and the program they collect to. Its only
/// constructor is [`Admitted::of`], so an `Admitted` that exists raised cleanly
/// — which it certifies, and nothing of a backend's language, arithmetic, or
/// resources.
#[derive(Clone, Debug)]
pub struct Admitted {
    occurrences: Occurrences,
    program: Program,
}

impl Admitted {
    /// Admit a parse, or refuse it whole, by one rule of severity: with its
    /// error-severity syntax diagnostics, or, the parse being in the language,
    /// with the raise's whole batch, every one an error — a repeated definition
    /// among them (docs/design/program.md §6.3). A warning neither refuses nor
    /// rides in the `Admitted`; it stays on the caller's `Parse`. Cost: the
    /// raise, `O(tree)` up to the log factor of ordering its sets and counted
    /// collections, and the set's collection from a clone of the occurrences —
    /// the statements held twice, in source order and as the set.
    pub fn of(parse: &Parse<ast::Program>) -> Result<Admitted, NotAdmitted> {
        let syntax: Box<[SyntaxError]> = parse
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.severity() == Severity::Error)
            .cloned()
            .collect();
        if !syntax.is_empty() {
            return Err(NotAdmitted::Syntax(syntax));
        }
        let occurrences = raise_occurrences(parse);
        if !occurrences.diagnostics().is_empty() {
            return Err(NotAdmitted::Lowering(occurrences.diagnostics().into()));
        }
        let program = occurrences.clone().into_raised().into_program();
        Ok(Admitted {
            occurrences,
            program,
        })
    }

    /// The statements, in source order, each with its part and provenance.
    /// O(1) to begin.
    pub fn statements(&self) -> impl Iterator<Item = &StatementOccurrence> + '_ {
        self.occurrences.occurrences().iter()
    }
}

/// Why a parse was not admitted at Door A (docs/design/solve.md §10.2) —
/// exactly one of two, typed and located, each diagnostic lowering to a base
/// diagnostic. It reaches the caller before any backend is asked, and converts
/// into a program fault refusing the parse, carried whole, for a caller that
/// wants `?` (§5.4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NotAdmitted {
    /// The parse is not in the language: its error-severity diagnostics.
    Syntax(Box<[SyntaxError]>),
    /// The parse is in the language and its raise refused: the raise's whole
    /// batch (docs/design/program.md §8).
    Lowering(Box<[LowerError]>),
}

impl NotAdmitted {
    /// Either batch lowered to base diagnostics, in source order. Total;
    /// O(diagnostics).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        match self {
            NotAdmitted::Syntax(errors) => errors.iter().map(ToDiagnostic::to_diagnostic).collect(),
            NotAdmitted::Lowering(errors) => {
                errors.iter().map(ToDiagnostic::to_diagnostic).collect()
            }
        }
    }
}

impl fmt::Display for NotAdmitted {
    /// Which side refused the parse, and with how many errors — each of a
    /// raise's diagnostics is an error (docs/design/program.md §8).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (headline, count) = match self {
            NotAdmitted::Syntax(errors) => ("the parse is not in the language", errors.len()),
            NotAdmitted::Lowering(errors) => ("the parse did not raise cleanly", errors.len()),
        };
        let plural = if count == 1 { "" } else { "s" };
        write!(f, "{headline} ({count} error{plural})")
    }
}

impl std::error::Error for NotAdmitted {}

/// The ground program a backend exposes (§10.4): the machine-IR of §1.1 as a
/// first-class, engine-free value — plain data, not FFI — carrying `Origin`
/// provenance on every ground rule, the anchor through which an explanation
/// client attributes answer-set atoms back to source. A committed capability
/// of the contract (`Backend::ground_program`, §4.1), not a reserved seam:
/// an adapter produces it faithfully beside its lowering, and the conformance
/// suite checks that it does (§13.1).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GroundProgram {
    pub(crate) rules: Vec<GroundRule>,
}

/// One ground rule of a [`GroundProgram`], carrying the `Origin` of the
/// statement it was instantiated from (§10.4). Its ground head and body join
/// when the observer is realised; the provenance is the part every reader of
/// the value needs first.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GroundRule {
    pub(crate) origin: Origin,
}

impl GroundProgram {
    /// The ground rules, in the order they were produced.
    pub fn rules(&self) -> impl Iterator<Item = &GroundRule> + '_ {
        self.rules.iter()
    }
}

impl GroundRule {
    /// The provenance of the statement this rule was instantiated from
    /// (§10.4).
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use themelios_program::Origin;
    use themelios_program::provenance::TransformTag;

    /// The transformation a rewritten rule names.
    const REWRITE: &str = "rewrite";

    fn constructed() -> GroundRule {
        GroundRule {
            origin: Origin::Constructed,
        }
    }

    fn rewritten() -> GroundRule {
        GroundRule {
            origin: Origin::Transformed(TransformTag::new(REWRITE)),
        }
    }

    #[test]
    fn a_constructed_ground_rule_round_trips_its_origin() {
        assert_eq!(constructed().origin(), &Origin::Constructed);
    }

    #[test]
    fn a_ground_program_yields_its_rules_in_order() {
        let program = GroundProgram {
            rules: vec![constructed(), rewritten()],
        };
        let origins: Vec<&Origin> = program.rules().map(GroundRule::origin).collect();
        assert_eq!(
            origins,
            [
                &Origin::Constructed,
                &Origin::Transformed(TransformTag::new(REWRITE))
            ]
        );
    }

    #[test]
    fn a_ground_rule_is_plain_data() {
        let rule = rewritten();
        assert_eq!(rule.clone(), rule);
        assert_ne!(rule, constructed());
        assert!(format!("{rule:?}").contains(REWRITE), "{rule:?}");
    }

    #[test]
    fn a_ground_program_with_rules_is_plain_data() {
        let program = GroundProgram {
            rules: vec![constructed()],
        };
        assert_eq!(program.clone(), program);
        assert_ne!(program, GroundProgram::default());
    }
}
