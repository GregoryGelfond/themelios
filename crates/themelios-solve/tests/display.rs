//! The answer set and the display (docs/design/solve.md §5.1): a model carries
//! every true literal; what the program's `#show` directives display is the
//! core's derivation from the show rule and the model's terms, a type of its
//! own that no reading consults.

use std::collections::BTreeSet;

use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_solve::outcome::Model;

/// The ground constant `name`, under `sign`.
fn signed(name: &str, sign: Sign) -> Symbol {
    Symbol::function(Name::new(name).expect("a valid identifier"), [], sign)
}

/// The positive ground constant `name`.
fn constant(name: &str) -> Symbol {
    signed(name, Sign::Positive)
}

/// The set of `symbols`.
fn set(symbols: impl IntoIterator<Item = Symbol>) -> BTreeSet<Symbol> {
    symbols.into_iter().collect()
}

#[test]
fn a_model_without_terms_displays_its_answer_set() {
    let model = Model::of(set([constant("a")]));
    assert_eq!(model.shown().symbols(), model.atoms());
}

#[test]
fn a_displayed_term_need_not_be_a_true_atom() {
    // `q. #show p : q.` — the answer set {q}, the display {p, q}.
    let model = Model::of(set([constant("q")])).with_terms([constant("p")]);
    assert_eq!(model.atoms(), &set([constant("q")]));
    assert_eq!(
        model.shown().symbols(),
        &set([constant("p"), constant("q")])
    );
}

#[test]
fn a_displayed_term_is_held_once() {
    // A term equal to a displayed atom is one member of the display, a set.
    let model = Model::of(set([constant("q")])).with_terms([constant("q")]);
    assert_eq!(model.shown().symbols(), &set([constant("q")]));
}

#[test]
fn the_display_answers_membership() {
    let model = Model::of(set([constant("q")])).with_terms([constant("p")]);
    assert!(model.shown().contains(&constant("p")));
    assert!(!model.shown().contains(&constant("r")));
}

#[test]
fn a_model_of_function_symbols_is_a_set_of_literals() {
    let model = Model::of(set([constant("a"), signed("b", Sign::Negative)]));
    assert!(model.is_set_of_literals());
}

#[test]
fn a_model_holding_a_number_is_no_set_of_literals() {
    assert!(!Model::of(set([Symbol::Number(1)])).is_set_of_literals());
}

#[test]
fn models_with_equal_answer_sets_and_displays_are_equal() {
    let one = Model::of(set([constant("q")])).with_terms([constant("p")]);
    let two = Model::of(set([constant("q")])).with_terms([constant("p")]);
    assert_eq!(one, two);
}

#[test]
fn models_whose_displays_differ_are_unequal() {
    let plain = Model::of(set([constant("q")]));
    let showing = Model::of(set([constant("q")])).with_terms([constant("p")]);
    assert_ne!(plain, showing);
}

#[test]
fn a_term_equal_to_an_atom_leaves_the_model_equal_to_one_without_it() {
    // A stored display equal to the answer set is no different from none.
    let plain = Model::of(set([constant("q")]));
    let redundant = Model::of(set([constant("q")])).with_terms([constant("q")]);
    assert_eq!(plain, redundant);
}
