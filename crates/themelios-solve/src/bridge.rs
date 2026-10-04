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
use themelios_program::program::PartKey;
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::{LowerError, Occurrences, StatementOccurrence, raise_occurrences};
use themelios_program::{Program, Statement};
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

/// A ground program, as §10.4's law holds it (docs/design/solve.md §10.4) —
/// engine-free data, the machine-IR of §1.1, the anchor through which an
/// explanation client attributes answer-set atoms back to source. It holds each
/// statement its rules were instantiated from once and whole, with its part and
/// its provenance: the program's statements, or a parse's occurrences at the
/// per-occurrence grain ([`Grain`]). Each ground rule names its statement
/// within it, so a merged statement's every origin reaches its rules, two
/// content-equal statements under different parts stay apart, and no rule
/// clones its statement. The part is the statement's as declared, `step(t)`;
/// the instance a grounding gave it belongs to the fuller observer the reserved
/// seams carry (§14). Cost: the statements and their parts' keys once,
/// Θ(program), and one index per rule.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GroundProgram {
    grain: Grain,
    statements: Vec<(PartKey, WithProvenance<Statement>)>,
    rules: Vec<GroundRule>,
}

impl GroundProgram {
    /// The in-crate construction door, for the crate's own tests: the public
    /// one lands with the observer that produces it (§11.1), as `Incumbent`'s
    /// lands with `optimize`.
    #[cfg(test)]
    pub(crate) fn of(
        grain: Grain,
        statements: Vec<(PartKey, WithProvenance<Statement>)>,
        rules: Vec<GroundRule>,
    ) -> GroundProgram {
        GroundProgram {
            grain,
            statements,
            rules,
        }
    }

    /// Each ground rule with the part and the statement it was instantiated
    /// from, in the order produced. A rule naming no statement yields nothing;
    /// whether the observer's construction door refuses such a rule is settled
    /// with that door (§10.4, §11.1). O(1) per rule.
    pub fn rules(
        &self,
    ) -> impl Iterator<Item = (&GroundRule, &PartKey, &WithProvenance<Statement>)> + '_ {
        self.rules.iter().filter_map(|rule| {
            self.statements
                .get(rule.statement)
                .map(|(part, statement)| (rule, part, statement))
        })
    }

    /// What a rule's statement is: a statement of the program, or an
    /// occurrence of the parse. O(1).
    pub fn grain(&self) -> Grain {
        self.grain
    }
}

/// One ground rule of a [`GroundProgram`] (§10.4). Its ground head and body
/// join with the observer that produces them (§11.1); today it names its
/// statement, by position among its program's.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GroundRule {
    statement: usize,
}

impl GroundRule {
    /// A rule naming the statement at `statement` among its program's — for
    /// the crate's own tests, as [`GroundProgram`]'s construction is.
    #[cfg(test)]
    pub(crate) fn naming(statement: usize) -> GroundRule {
        GroundRule { statement }
    }
}

/// What a ground rule's statement is (§10.4): a statement of the program
/// lowered, merged as the set merges it, or an occurrence of the parse, its
/// nested provenance intact.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Grain {
    /// A statement of the program lowered.
    #[default]
    Statement,
    /// An occurrence of the parse admitted at Door A.
    Occurrence,
}

#[cfg(test)]
mod tests {
    use super::*;
    use themelios_program::{Atom, Name, Rule};

    /// A part key with no formals.
    fn key(name: &str) -> PartKey {
        PartKey {
            name: Name::new(name).expect("a valid identifier"),
            formals: Vec::new(),
        }
    }

    /// The fact `name.`, built in Rust.
    fn fact(name: &str) -> WithProvenance<Statement> {
        WithProvenance::constructed(Statement::from(Rule::fact(Atom::constant(
            Name::new(name).expect("a valid identifier"),
        ))))
    }

    #[test]
    fn a_ground_program_yields_each_rule_with_its_part_and_statement_in_order() {
        let program = GroundProgram::of(
            Grain::Statement,
            vec![(key("base"), fact("a")), (key("step"), fact("b"))],
            vec![GroundRule::naming(1), GroundRule::naming(0)],
        );
        let read: Vec<(&PartKey, &Statement)> = program
            .rules()
            .map(|(_, part, statement)| (part, statement.get()))
            .collect();
        assert_eq!(
            read,
            [
                (&key("step"), fact("b").get()),
                (&key("base"), fact("a").get())
            ]
        );
    }

    #[test]
    fn rules_naming_one_statement_share_it() {
        let program = GroundProgram::of(
            Grain::Statement,
            vec![(key("base"), fact("a"))],
            vec![GroundRule::naming(0), GroundRule::naming(0)],
        );
        let statements: Vec<&WithProvenance<Statement>> =
            program.rules().map(|(_, _, statement)| statement).collect();
        assert!(std::ptr::eq(statements[0], statements[1]));
    }

    #[test]
    fn content_equal_statements_under_two_parts_stay_apart() {
        let program = GroundProgram::of(
            Grain::Statement,
            vec![(key("base"), fact("a")), (key("step"), fact("a"))],
            vec![GroundRule::naming(0), GroundRule::naming(1)],
        );
        let parts: Vec<&PartKey> = program.rules().map(|(_, part, _)| part).collect();
        assert_eq!(parts, [&key("base"), &key("step")]);
    }

    #[test]
    fn a_rule_naming_no_statement_yields_nothing() {
        let program = GroundProgram::of(
            Grain::Statement,
            vec![(key("base"), fact("a"))],
            vec![GroundRule::naming(1)],
        );
        assert_eq!(program.rules().count(), 0);
    }

    #[test]
    fn a_ground_program_reports_its_grain() {
        let program = GroundProgram::of(Grain::Occurrence, Vec::new(), Vec::new());
        assert_eq!(program.grain(), Grain::Occurrence);
    }
}
