//! The construction macros under a renamed runtime, beside a decoy (docs/design/macros.md §9,
//! §11). `::themelios_program` is an unrelated crate here, so every expansion below compiles only
//! because it names the root it selects: `::tp` by the runtime selection, or the facade's own
//! re-export through its wrapper.

use tp::program::{Atom, Program, Rule, Statement};
use tp::symbol::Name;
use tp::term::Term;

/// The fact `p(1)`, built by hand through the renamed runtime.
fn p_one() -> Rule {
    Rule::fact(Atom::new(
        Name::new("p").expect("an identifier"),
        [Term::from(1i32)],
    ))
}

#[test]
fn the_default_name_resolves_to_the_decoy() {
    // The crate under the default root's name is not the program tier, so an expansion that fell
    // back to `::themelios_program` would not compile.
    assert_eq!(themelios_program::NAME, "fixture-decoy");
}

// `#[rustfmt::skip]`: a block writes its statements ASP-side, which rustfmt would read as Rust
// and reflow.
#[rustfmt::skip]
#[test]
fn a_renamed_runtime_selected_by_path_builds_its_program() {
    let by_macro: Program = themelios_macros::program! { #![crate = ::tp] p(1). };
    assert_eq!(by_macro, Program::of([Statement::from(p_one())]));
}

#[test]
fn a_renamed_runtime_selected_by_path_builds_a_fact() {
    assert_eq!(themelios_macros::fact!(#![crate = ::tp] p(1)), p_one());
}

#[rustfmt::skip]
#[test]
fn the_facade_builds_the_canonical_program_beside_the_decoy() {
    // The facade's value is the renamed runtime's own type: the facade's re-export and `tp` are
    // one crate, the canonical program tier.
    let by_macro: tp::program::Program = facade::program! { p(1). };
    assert_eq!(by_macro, Program::of([Statement::from(p_one())]));
}
