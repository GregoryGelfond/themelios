//! Laws of assumption blame and the consequence set at the public surface
//! (docs/design/solve.md §5.4, §5.2): a refutation is a closed reading of
//! which assumptions are responsible for a scenario's inconsistency — raw
//! assumptions, not a named scenario — and travels as plain data; an
//! unsatisfiability answers the blame question as an option, `Some` only for
//! an assumption-scoped solve; and consequences are their own typed set, not
//! an answer set, carrying the mode that produced them. The payloads are the
//! engine's to build, so their value laws are pinned in the defining crate;
//! here the surface's shape is.

use std::fmt::Debug;

use themelios_program::{Name, Symbol};
use themelios_solve::agent::Assumption;
use themelios_solve::contract::Mode;
use themelios_solve::outcome::{Consequences, Refutation, Unsat};

/// The assumption a blame names: that the constant atom `p` holds.
fn culprit() -> Assumption {
    let p = Symbol::constant(Name::new("p").expect("an identifier"));
    Assumption::new(p, true).expect("an atom")
}

/// A blame that names one assumption.
fn blaming_some() -> Refutation {
    Refutation::These(Box::from([culprit()]))
}

/// The three readings of blame, each beside the head of its debug rendering.
fn every_refutation() -> [(Refutation, &'static str); 3] {
    [
        (blaming_some(), "These"),
        (Refutation::NotThese, "NotThese"),
        (Refutation::NoAssumptions, "NoAssumptions"),
    ]
}

/// Owned plain data: what a value that travels across threads and service
/// boundaries must be.
fn plain<T: Send + Sync + Clone + Eq + Debug + 'static>() {}

#[test]
fn a_refutation_is_a_closed_reading() {
    // A reading with no `_` arm: the three readings are the readings there
    // are (§5.4) — were a fourth ever added, this reading would fail to
    // compile.
    fn reading(refutation: &Refutation) -> &'static str {
        match refutation {
            Refutation::These(_) => "these assumptions are responsible",
            Refutation::NotThese => "the assumptions are not responsible",
            Refutation::NoAssumptions => "none were assumed",
        }
    }
    let _: fn(&Refutation) -> &'static str = reading;
}

#[test]
fn a_blame_that_names_assumptions_yields_the_assumptions_it_names() {
    let named: Box<[Assumption]> = Box::from([culprit()]);
    let Refutation::These(culprits) = Refutation::These(named.clone()) else {
        panic!("These");
    };
    assert_eq!(culprits, named);
}

#[test]
fn a_cloned_refutation_equals_its_original() {
    for (refutation, _) in every_refutation() {
        assert_eq!(refutation.clone(), refutation);
    }
}

#[test]
fn distinct_refutations_are_unequal() {
    let all = every_refutation();
    for (position, (left, _)) in all.iter().enumerate() {
        for (right, _) in all.iter().skip(position + 1) {
            assert_ne!(left, right);
        }
    }
}

#[test]
fn a_refutation_s_debug_view_names_its_reading() {
    for (refutation, head) in every_refutation() {
        let rendered = format!("{refutation:?}");
        assert!(rendered.starts_with(head), "{rendered}");
    }
}

#[test]
fn a_refutation_travels_as_owned_plain_data() {
    plain::<Refutation>();
}

#[test]
fn an_unsatisfiability_travels_as_owned_plain_data() {
    plain::<Unsat>();
}

#[test]
fn an_unsatisfiability_answers_blame_as_an_optional_refutation() {
    // `Some` only for an assumption-scoped solve (§5.4); the payload is the
    // engine's to build, so the law here is the question's shape.
    let _: fn(&Unsat) -> Option<Refutation> = Unsat::blame;
}

#[test]
fn consequences_travel_as_owned_plain_data() {
    // A value that has travelled still says which question it answers (§5.2).
    plain::<Consequences>();
}

#[test]
fn consequences_answer_which_mode_produced_them() {
    let _: fn(&Consequences) -> Mode = Consequences::mode;
}

#[test]
fn consequences_answer_whether_a_symbol_is_among_them() {
    let _: fn(&Consequences, &Symbol) -> bool = Consequences::contains;
}
