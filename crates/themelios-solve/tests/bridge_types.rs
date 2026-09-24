//! Laws of the bridge's engine-free surface (docs/design/solve.md §10.4,
//! §10.5): the ground program is plain data — empty by default, its rules
//! read through one iterator, its rule type plain data too — the
//! interning-discipline contract is implementable outside the crate and runs
//! an interning writer under its lock, passing the writer's own result
//! through unchanged; and the `Symbol` correspondence is total on the
//! engine's own number width.

use std::cell::Cell;
use std::fmt::Debug;

use themelios_program::Symbol;
use themelios_solve::bridge::{GroundProgram, GroundRule, InterningDiscipline};
use themelios_solve::contract::Fault;

/// The engine's number width at its bounds and around zero: every one a
/// value the correspondence carries whole (docs/design/solve.md §10.5).
const NUMBER_WIDTH_WITNESSES: [i32; 5] = [i32::MIN, -1, 0, 1, i32::MAX];

/// What an interning writer reports when the engine refuses it.
const ENGINE_MESSAGE: &str = "the symbol table is full";

fn is_plain_data<T: Clone + Eq + Debug>() {}

// --- the ground program ---

#[test]
fn a_default_ground_program_has_no_rules() {
    assert_eq!(GroundProgram::default().rules().count(), 0);
}

#[test]
fn a_ground_program_is_plain_data() {
    is_plain_data::<GroundProgram>();
    let empty = GroundProgram::default();
    assert_eq!(empty.clone(), empty);
    assert!(format!("{empty:?}").contains("GroundProgram"));
}

#[test]
fn a_ground_rule_is_plain_data() {
    // A rule is built only below the seam, so the law a consumer can state is
    // the bounds: it clones, compares, and renders for debugging.
    is_plain_data::<GroundRule>();
}

// --- the interning discipline ---

/// A discipline as a consumer implements it: one flag stands for the single
/// interning lock, raised for exactly the writer's duration.
struct Guarded {
    held: Cell<bool>,
}

impl InterningDiscipline for Guarded {
    fn with_interning_lock<T>(&self, f: impl FnOnce() -> Result<T, Fault>) -> Result<T, Fault> {
        self.held.set(true);
        let written = f();
        self.held.set(false);
        written
    }
}

#[test]
fn an_interning_writer_runs_under_the_lock() {
    let discipline = Guarded {
        held: Cell::new(false),
    };
    let saw_the_lock = discipline.with_interning_lock(|| Ok(discipline.held.get()));
    assert_eq!(saw_the_lock, Ok(true));
    // The lock's extent is the writer's, not the discipline's.
    assert!(!discipline.held.get());
}

#[test]
fn an_interning_writer_s_fault_passes_through_unchanged() {
    let discipline = Guarded {
        held: Cell::new(false),
    };
    let refused: Result<(), Fault> =
        discipline.with_interning_lock(|| Err(Fault::engine(ENGINE_MESSAGE)));
    assert_eq!(refused, Err(Fault::engine(ENGINE_MESSAGE)));
}

// --- the Symbol correspondence ---

#[test]
fn the_symbol_correspondence_is_total_on_the_engine_s_number_width() {
    // The bridge documents that a `Symbol` carries the engine's own `i32`, so
    // no lowering refuses a valid number: the constructor is total over the
    // width and carries each value whole (docs/design/solve.md §10.5).
    for value in NUMBER_WIDTH_WITNESSES {
        assert_eq!(Symbol::number(value), Symbol::Number(value));
    }
}
