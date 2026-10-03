//! Laws of the bridge's engine-free surface (docs/design/solve.md §10.4,
//! §10.5): the ground program is plain data — empty by default, its rules
//! read through one iterator, its rule type plain data too — and the `Symbol`
//! correspondence is total on the engine's own number width.

use std::fmt::Debug;

use themelios_program::Symbol;
use themelios_solve::bridge::{GroundProgram, GroundRule};

/// The engine's number width at its bounds and around zero: every one a
/// value the correspondence carries whole (docs/design/solve.md §10.5).
const NUMBER_WIDTH_WITNESSES: [i32; 5] = [i32::MIN, -1, 0, 1, i32::MAX];

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
