//! The bridge and the seam (docs/design/solve.md §10): the doors from the
//! program IR to an engine, the typed refusal of a parse at Door A, the
//! aspif-level sink, the ground-program IR, and the interning contract.
//!
//! The `Symbol` correspondence (§10.5): `themelios_program::Symbol` carries
//! the engine's own number width (`i32`, docs/design/program.md §3.1), so no
//! value is lost or reshaped crossing the seam — the correspondence is total
//! on the `i32` range, and no lowering refuses a valid number. Creating the
//! engine's symbol *handle* from a `Symbol` is nonetheless an interning
//! write, serialised under the single [`InterningDiscipline`] — not a free
//! correspondence.

use std::fmt;

use themelios_base::diagnostic::{Diagnostic, ToDiagnostic};
use themelios_program::raise::LowerError;
use themelios_program::{Origin, Program};
use themelios_syntax::{Parse, SyntaxError, ast};

use crate::contract::{Fault, TruthValue};

/// A door from the program IR into an engine (docs/design/solve.md §10.2),
/// borrowing what it carries for `'a`. Three grades, mirroring the engine's
/// own construction paths, and a closed set: a match over the doors is
/// exhaustive without a wildcard, so a new grade is a new variant every
/// backend's [`lower`](crate::contract::Backend::lower) must answer.
///
/// Doors A and B are two entry values into one grounding mechanism — the
/// engine's non-ground input, driven to ground — differing only in what
/// they preserve; Door C is the aspif-level ingestion the engine's
/// ground-by-construction backend exposes, which takes ground objects only.
/// A non-ground program — a variable, an aggregate, a `#program` part —
/// crosses only through A or B; mapping it to C is a category error.
///
/// **The discipline is absolute: never render to text and re-parse across
/// the seam** (§10.2). Every door carries a typed value — a parse, a
/// program, a source of ground objects — so rendered text has no door to
/// enter: the fragile, slow path a shell-out imposes is exactly what the
/// typed doors erase, and the huge ground instantiation lives in the
/// engine's compact internals, streamed, never on the owned side.
pub enum Door<'a> {
    /// Door A — the typed tree, order- and span-preserving: the highest
    /// fidelity the seam offers. Its full lowering is realised with the
    /// higher-fidelity path; declared here so the door set is closed.
    Ast(&'a Parse<ast::Program>),
    /// Door B — the owned `Program` in canonical order, carrying `Origin`
    /// provenance through to every ground rule, a capability the engine's
    /// own grounder lacks. Programs constructed in Rust, transformed, or
    /// loaded through a client enter here.
    Program(&'a Program),
    /// Door C — an aspif-level source of ground objects, driven into the
    /// solver's ingestion: a foreign grounder, the differential harness, or
    /// an agent's ground-fact additions where the values are already ground.
    Aspif(&'a mut dyn AspifSource),
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

/// A ground atom's id at the seam: the engine's `clingo_atom_t`, always
/// greater than zero. Distinct from [`AspifLit`], so an atom cannot pass
/// where a literal is wanted (§10.3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AspifAtom(pub u32);

/// A ground literal at the seam: the engine's `clingo_literal_t`, signed —
/// its magnitude names the atom, its sign the polarity. Distinct from
/// [`AspifAtom`], so a literal cannot pass where an atom is wanted (§10.3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AspifLit(pub i32);

/// A source of ground, aspif-level objects — what Door C carries (§10.2): a
/// foreign grounder, the differential harness, or an agent's ground-fact
/// additions. Driving it streams its objects into a sink; a trait object,
/// so a door carries any source.
pub trait AspifSource {
    /// Stream every ground object into `sink`, in the source's own order.
    /// The first fault — the sink's, or the source's own — ends the drive.
    fn drive(&mut self, sink: &mut dyn AspifSink) -> Result<(), Fault>;
}

/// The typed program-backend sink in the image of the engines' own backend
/// (§10.3): the seam between the grounder and the solver, typed with the
/// distinct [`AspifAtom`] and [`AspifLit`] newtypes for the id roles, which
/// Rust makes cheap. Its implementors are the native solver's ingestion, an
/// aspif writer, and the `--text`/reify projections; a trait object, so a
/// source drives a sink it never names. The committed surface is the
/// backend's full roster — the four declared here, and the remaining backend
/// methods (`bd_aggr`, `project`, `heuristic`, `edge`, `show`, step framing,
/// `next_lit`/`fact_lit`, §10.3) join with the first source that drives a
/// sink: a Door-C [`AspifSource`] — a foreign grounder's output, as the
/// differential harness reads it. Nothing drives a sink before then: Door B
/// lowers into the engine's grounder, not into a sink (§10.2).
pub trait AspifSink {
    /// A rule: `head` holds — or, with `choice`, may hold — when every
    /// literal of `body` holds. An empty `head` without `choice` is an
    /// integrity constraint; an empty `body` makes a fact.
    fn rule(&mut self, choice: bool, head: &[AspifAtom], body: &[AspifLit]) -> Result<(), Fault>;

    /// A minimize statement at `priority`: each literal with its weight, in
    /// the engine's own weight width.
    fn minimize(&mut self, priority: i32, literals: &[(AspifLit, i32)]) -> Result<(), Fault>;

    /// An external atom, at its initial truth value.
    fn external(&mut self, atom: AspifAtom, value: TruthValue) -> Result<(), Fault>;

    /// Literals assumed to hold for the next solve.
    fn assume(&mut self, literals: &[AspifLit]) -> Result<(), Fault>;
}

/// The companion theory-backend sink (§10.3); its methods join when the
/// theory door is realised.
pub trait TheorySink {}

/// The ground program a backend exposes (§10.4): the machine-IR of §1.1 as a
/// first-class, engine-free value — plain data, not FFI — carrying `Origin`
/// provenance on every ground rule, the anchor through which an explanation
/// client attributes answer-set atoms back to source. A committed capability
/// of the contract (`Backend::ground_program`, §4.1), not a reserved seam:
/// an adapter produces it faithfully beside its aspif lowering, and the
/// conformance suite checks that it does (§13.1).
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

/// The interning-discipline contract (§10.5). The engine's process-global
/// symbol interning is where the FFI cost concentrates, and concurrent
/// interning is unsafe in the pinned engine (libclingo 5.8) despite its
/// header's claim, so an adapter serialises every interning *writer* under
/// one lock, trips a reentrant-interning tripwire, and lints every direct
/// interning FFI call. Stated here, as a surface of the solve tier, so an
/// adapter is held to a named obligation rather than adapter-private lore;
/// implemented once, in the engine adapter (§11). Version-scoped to the
/// pinned engine and retired by the spike suite (specification §5.2).
///
/// Generic over the writer's result, so not usable as a trait object: the
/// discipline is a property of one adapter, never dispatched over.
pub trait InterningDiscipline {
    /// Run `f`, an interning writer, holding the single interning lock. A
    /// re-entrant interning attempt is a loud error — a typed fault — never a
    /// silent process-wide wedge; the writer's own result passes through
    /// unchanged.
    fn with_interning_lock<T>(&self, f: impl FnOnce() -> Result<T, Fault>) -> Result<T, Fault>;
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
