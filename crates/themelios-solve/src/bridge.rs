//! The bridge and the seam (docs/design/solve.md §10): the doors from the
//! program IR to an engine, the aspif-level sink, the ground-program IR, and
//! the interning contract.
//!
//! The `Symbol` correspondence (§10.5): `themelios_program::Symbol` carries
//! the engine's own number width (`i32`, docs/design/program.md §3.1), so no
//! value is lost or reshaped crossing the seam — the correspondence is total
//! on the `i32` range, and no lowering refuses a valid number. Creating the
//! engine's symbol *handle* from a `Symbol` is nonetheless an interning
//! write, serialised under the single [`InterningDiscipline`] — not a free
//! correspondence.

use std::marker::PhantomData;

use themelios_program::Origin;

use crate::contract::Fault;

/// A door from the program IR into an engine, borrowing what it carries for
/// `'a`. Reserved; its forms are defined with §10.2.
pub struct Door<'a> {
    _carried: PhantomData<&'a ()>,
}

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
