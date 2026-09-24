//! Laws of assumptions and scenarios at the public surface
//! (docs/design/solve.md §6.3): an assumption fixes one program atom true or
//! false and refuses a non-atom symbol at construction, handing it back; a
//! scenario is a reusable bundle that re-applies exactly the assumptions it
//! collected, the empty bundle being the unscoped default; an assumption
//! converts into itself; and both travel as owned plain data.

use std::fmt::Debug;

use themelios_program::{Name, Sign, Symbol};
use themelios_solve::agent::{Assumption, IntoAssumption, NotAnAssumption, NotAnAtom, Scenario};

/// The constant atom `name` — the simplest program atom.
fn atom_symbol(name: &str) -> Symbol {
    Symbol::constant(Name::new(name).expect("an identifier"))
}

/// The predicate `name`, applied to `arguments`, under `sign`.
fn applied(name: &str, arguments: impl IntoIterator<Item = Symbol>, sign: Sign) -> Symbol {
    Symbol::function(Name::new(name).expect("an identifier"), arguments, sign)
}

/// The assumption that the constant atom `name` holds.
fn holds(name: &str) -> Assumption {
    Assumption::new(atom_symbol(name), true).expect("an atom")
}

/// One witness per symbol variant that is not an atom.
fn every_non_atom() -> [Symbol; 5] {
    [
        Symbol::number(3),
        Symbol::string("text"),
        Symbol::tuple([Symbol::number(1)]),
        Symbol::Infimum,
        Symbol::Supremum,
    ]
}

/// Owned plain data: what a value that travels across threads and service
/// boundaries must be.
fn plain<T: Send + Sync + Clone + Eq + Debug + 'static>() {}

/// A refusal that composes with the standard error machinery.
fn is_an_error<E: std::error::Error>() {}

// --- an assumption refuses a non-atom at construction ---

#[test]
fn an_assumption_refuses_a_non_atom_symbol() {
    assert!(Assumption::new(Symbol::number(3), true).is_err());
}

#[test]
fn an_assumption_refuses_every_non_atom_symbol() {
    for symbol in every_non_atom() {
        assert!(Assumption::new(symbol.clone(), true).is_err(), "{symbol:?}");
    }
}

#[test]
fn an_assumption_accepts_an_atom_symbol() {
    assert!(Assumption::new(atom_symbol("p"), true).is_ok());
}

#[test]
fn an_assumption_accepts_a_strongly_negated_atom() {
    // `-p` is an atom under its strong sign (docs/design/program.md §3.1).
    assert!(Assumption::new(applied("p", [], Sign::Negative), true).is_ok());
}

#[test]
fn an_assumption_accepts_an_atom_with_arguments() {
    let p_of_one = applied("p", [Symbol::number(1)], Sign::Positive);
    assert!(Assumption::new(p_of_one, false).is_ok());
}

#[test]
fn a_refusal_hands_back_the_symbol_it_refused() {
    let refused = Assumption::new(Symbol::number(3), true).unwrap_err();
    assert_eq!(*refused.symbol(), Symbol::number(3));
}

#[test]
fn a_refusal_explains_itself() {
    let refused = Assumption::new(Symbol::number(3), true).unwrap_err();
    assert!(format!("{refused}").contains("not an atom"), "{refused}");
}

#[test]
fn the_refusals_compose_as_errors() {
    is_an_error::<NotAnAtom>();
    is_an_error::<NotAnAssumption>();
}

// --- an assumption reads back what it fixes ---

#[test]
fn an_assumption_reports_the_atom_it_fixes() {
    assert_eq!(*holds("p").atom(), atom_symbol("p"));
}

#[test]
fn an_assumption_reports_the_polarity_it_fixes() {
    for polarity in [true, false] {
        let assumption = Assumption::new(atom_symbol("p"), polarity).expect("an atom");
        assert_eq!(assumption.holds(), polarity);
    }
}

#[test]
fn assumptions_differing_only_in_polarity_are_unequal() {
    let asserted = Assumption::new(atom_symbol("p"), true).expect("an atom");
    let denied = Assumption::new(atom_symbol("p"), false).expect("an atom");
    assert_ne!(asserted, denied);
}

#[test]
fn an_assumption_converts_into_itself() {
    let assumption = holds("p");
    assert_eq!(assumption.clone().into_assumption(), Ok(assumption));
}

#[test]
fn an_assumption_travels_as_owned_plain_data() {
    plain::<Assumption>();
}

// --- a scenario is a reusable bundle of assumptions ---

#[test]
fn a_scenario_is_a_reusable_named_bundle() {
    let scenario: Scenario = [Assumption::new(atom_symbol("p"), true).unwrap()]
        .into_iter()
        .collect();
    assert_eq!(scenario.assumptions().count(), 1);
}

#[test]
fn a_scenario_re_applies_the_assumptions_it_collected() {
    // Identity preserved: each assumption as given, in the order given.
    let collected = [holds("p"), holds("q")];
    let scenario: Scenario = collected.iter().cloned().collect();
    let re_applied: Vec<Assumption> = scenario.assumptions().cloned().collect();
    assert_eq!(re_applied, collected);
}

#[test]
fn reading_a_scenario_does_not_spend_it() {
    let scenario: Scenario = [holds("p")].into_iter().collect();
    let _first_reading = scenario.assumptions().count();
    assert_eq!(scenario.assumptions().count(), 1);
}

#[test]
fn the_default_scenario_assumes_nothing() {
    assert_eq!(Scenario::default().assumptions().count(), 0);
}

#[test]
fn an_empty_collection_is_the_default_scenario() {
    let empty: Scenario = std::iter::empty().collect();
    assert_eq!(empty, Scenario::default());
}

#[test]
fn a_cloned_scenario_equals_its_original() {
    let scenario: Scenario = [holds("p"), holds("q")].into_iter().collect();
    assert_eq!(scenario.clone(), scenario);
}

#[test]
fn a_scenario_travels_as_owned_plain_data() {
    plain::<Scenario>();
}
