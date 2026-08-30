//! Laws of the literal core (docs/design/program.md §4.6): the guarded comparison
//! chain, equality up to provenance through the carrier, canonicalization at the
//! comparison door, and the three negations held apart in the type.

use std::collections::BTreeSet;

use themelios_program::program::{
    Arguments, Atom, Comparison, Condition, DefaultNegation, Literal, LiteralInner, Relation,
};
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_program::term::{Term, UnaryOp};

fn name(text: &str) -> Name {
    Name::new(text).expect("identifier")
}

fn number(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn atom(sign: Sign, predicate: &str, arguments: Vec<Term>) -> Atom {
    Atom {
        sign,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    }
}

#[test]
fn a_comparison_chain_carries_its_steps_in_order() {
    let chain = Comparison::new(number(1), Relation::Lt, number(5)).chain(Relation::Le, number(10));
    assert_eq!(chain.first(), &number(1));
    let steps: Vec<(Relation, &Term)> = chain.steps().collect();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0], (Relation::Lt, &number(5)));
    assert_eq!(steps[1], (Relation::Le, &number(10)));
}

#[test]
fn a_comparison_canonicalizes_its_terms_at_the_door() {
    // A Function-shaped ground term entering the comparison door collapses to a
    // Symbolic (the pass discipline, §5.1). This is the first canonicalizing door.
    let ground = Term::Function {
        name: name("f"),
        arguments: vec![number(1)],
    };
    let comparison = Comparison::new(ground, Relation::Eq, number(2));
    let collapsed = Term::Symbolic(Symbol::Function {
        name: name("f"),
        arguments: vec![Symbol::Number(1)],
        sign: Sign::Positive,
    });
    assert_eq!(comparison.first(), &collapsed);
}

#[test]
fn identity_is_up_to_provenance_through_the_carrier() {
    let p = atom(Sign::Positive, "p", vec![number(1)]);
    // Same content, different provenance: the carriers compare equal.
    let parsed = WithProvenance::new(p.clone(), Provenance::from(Origin::Constructed));
    let transformed = WithProvenance::new(
        p.clone(),
        Provenance::from(Origin::Transformed(TransformTag::new("t"))),
    );
    assert_eq!(parsed, transformed);

    // Two literals equal up to provenance dedupe in a set.
    let literal = |carrier| Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(carrier),
    };
    let set: BTreeSet<Literal> = [literal(parsed), literal(transformed)]
        .into_iter()
        .collect();
    assert_eq!(set.len(), 1);
}

#[test]
fn the_three_negations_are_distinct_constructions() {
    let bare = atom(Sign::Positive, "p", vec![]);
    let strong = atom(Sign::Negative, "p", vec![]); // -p
    let carrier = |a: Atom| LiteralInner::Atom(WithProvenance::constructed(a));
    let not_p = Literal {
        negation: DefaultNegation::Not,
        inner: carrier(bare.clone()),
    };
    let not_not_p = Literal {
        negation: DefaultNegation::NotNot,
        inner: carrier(bare.clone()),
    };
    let strong_neg_p = Literal {
        negation: DefaultNegation::None,
        inner: carrier(strong),
    };
    // Default `not`, default `not not`, and strong `-` are three distinct literals.
    assert_ne!(not_p, not_not_p);
    assert_ne!(not_p, strong_neg_p);
    assert_ne!(not_not_p, strong_neg_p);
    // The bitwise `~` is a term operator — a fourth thing, not a literal negation.
    let bitwise = Term::UnaryOperation {
        operator: UnaryOp::BitwiseNot,
        argument: Box::new(number(1)),
    };
    assert!(matches!(
        bitwise,
        Term::UnaryOperation {
            operator: UnaryOp::BitwiseNot,
            ..
        }
    ));
}

#[test]
fn a_condition_is_a_possibly_empty_sequence_of_literals() {
    assert!(Condition::empty().is_empty());
    let literal = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom(
            Sign::Positive,
            "q",
            vec![],
        ))),
    };
    let condition = Condition::new([literal.clone(), literal]);
    assert!(!condition.is_empty());
    assert_eq!(condition.literals().count(), 2);
}
