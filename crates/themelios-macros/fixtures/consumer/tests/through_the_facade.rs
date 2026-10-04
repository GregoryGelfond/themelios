//! Every construction macro through a facade the consumer renamed `z` (docs/design/macros.md §9,
//! §11): each value equals the one the program tier's constructors build, reached through the
//! facade's own re-exports, so the macros name the facade's runtime — the canonical program tier —
//! and nothing the consumer would have to depend on itself. The optimization and `#external`
//! constructions here build values; no solve capability is claimed by building them.

use z::construct;
use z::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Condition, DefaultNegation, External, FunctionAggregate, Guard, Head,
    HeadAggregate, HeadAggregateElement, IntoHead, Literal, LiteralInner, OptimizeElement, Program,
    Relation, Rule, Show, Statement, weight,
};
use z::provenance::Origin;
use z::symbol::{Name, Sign, Signature, VarName};
use z::term::Term;

/// `Name::new(text)`, discharged as the codegen's `.expect()` is.
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// `VarName::new(text)`, discharged as [`name`] is.
fn var(text: &str) -> VarName {
    VarName::new(text).expect("a valid variable")
}

/// The fact `p(1)`, built by hand.
fn p_one() -> Rule {
    Rule::fact(Atom::new(name("p"), [Term::from(1i32)]))
}

/// The rule `q(X) :- p(X)`, built by hand.
fn q_if_p() -> Rule {
    Atom::new(name("q"), [Term::variable(var("X"))])
        .into_head()
        .when(Atom::new(name("p"), [Term::variable(var("X"))]))
}

/// The literal `#true`.
fn verum() -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::True,
    }
}

// `#[rustfmt::skip]`: a block writes its statements ASP-side, which rustfmt would read as Rust
// and reflow.
#[rustfmt::skip]
#[test]
fn a_qualified_program_macro_builds_the_hand_built_program() {
    let by_macro: Program = z::program! { p(1). q(X) :- p(X). };
    let by_hand = Program::of([Statement::from(p_one()), Statement::from(q_if_p())]);
    assert_eq!(by_macro, by_hand);
}

#[rustfmt::skip]
#[test]
fn an_imported_program_macro_builds_the_hand_built_program() {
    // `use z::program;` imports the macro beside the module of the same name.
    use z::program;
    let by_macro: Program = program! { p(1). q(X) :- p(X). };
    let by_hand = Program::of([Statement::from(p_one()), Statement::from(q_if_p())]);
    assert_eq!(by_macro, by_hand);
}

#[test]
fn an_empty_program_macro_builds_the_empty_program() {
    let by_macro: Program = z::program! {};
    assert_eq!(by_macro, Program::empty());
}

#[rustfmt::skip]
#[test]
fn a_statement_built_through_the_facade_carries_the_constructed_origin() {
    let by_macro: Program = z::program! { p(1). };
    let statement = by_macro.statements().next().expect("one statement");
    let origins: Vec<&Origin> = statement.provenance().origins().collect();
    assert_eq!(origins, [&Origin::Constructed]);
}

#[test]
fn the_atom_macro_builds_a_strongly_negated_atom() {
    let by_macro = z::atom!(-p(1));
    assert_eq!(by_macro, -Atom::new(name("p"), [Term::from(1i32)]));
}

#[test]
fn the_fact_macro_builds_a_spliced_value() {
    let origin = 1i32;
    assert_eq!(z::fact!(p($origin)), p_one());
}

#[test]
fn the_rule_macro_builds_a_rule() {
    assert_eq!(z::rule!(q(X) :- p(X)), q_if_p());
}

#[test]
fn the_constraint_macro_builds_a_constraint() {
    let by_macro = z::constraint!(:- p(X));
    let by_hand = Rule::constraint(Atom::new(name("p"), [Term::variable(var("X"))]));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn the_fact_macro_builds_a_choice_head() {
    let by_macro = z::fact!(1 { a : q(X) });
    let by_hand = Rule::fact(Choice::new(
        Some(Guard {
            relation: None,
            term: Term::from(1i32),
        }),
        [ChoiceElement::new(
            Literal::from(Atom::new(name("a"), [])),
            Condition::new([Literal::from(Atom::new(
                name("q"),
                [Term::variable(var("X"))],
            ))]),
        )],
        None,
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn the_fact_macro_builds_a_head_aggregate() {
    let by_macro = z::fact!(#count { X : p(X) });
    let by_hand = Rule::fact(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [Term::variable(var("X"))],
            Literal::from(Atom::new(name("p"), [Term::variable(var("X"))])),
            Condition::empty(),
        )],
        None,
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn the_rule_macro_builds_a_body_aggregate() {
    let by_macro = z::rule!(p :- 1 <= #sum { X : q(X) });
    let by_hand = Atom::new(name("p"), [])
        .into_head()
        .when(Body::new([BodyElement::from(Aggregate::Function(
            FunctionAggregate::new(
                Some(Guard {
                    relation: Some(Relation::Le),
                    term: Term::from(1i32),
                }),
                AggregateFunction::Sum,
                [BodyAggregateElement::new(
                    [Term::variable(var("X"))],
                    Condition::new([Literal::from(Atom::new(
                        name("q"),
                        [Term::variable(var("X"))],
                    ))]),
                )],
                None,
            ),
        ))]));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn a_counted_repeat_survives_the_facade() {
    // `1 { #true; #true } 1`: two by-occurrence elements the program counts (program §4.4).
    let by_macro = z::fact!(1 { #true; #true } 1);
    let guard = || {
        Some(Guard {
            relation: None,
            term: Term::from(1i32),
        })
    };
    let element = || ChoiceElement::new(verum(), Condition::empty());
    let by_hand = Rule::fact(Choice::new(guard(), [element(), element()], guard()));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn a_counted_repeat_built_through_the_facade_keeps_both_elements() {
    let by_macro = z::fact!(1 { #true; #true } 1);
    let Head::Choice(choice) = by_macro.head().get() else {
        panic!("a choice head, not {:?}", by_macro.head())
    };
    assert_eq!(choice.elements().count(), 2);
}

#[test]
fn the_minimize_macro_builds_a_minimize_statement() {
    let by_macro = z::minimize!({ 3@1 });
    let by_hand = construct::minimize([OptimizeElement::new(
        weight(Term::from(3i32)).at_priority(Term::from(1i32)),
        [],
        Condition::empty(),
    )]);
    assert_eq!(by_macro, by_hand);
}

#[test]
fn the_maximize_macro_builds_a_maximize_statement() {
    let by_macro = z::maximize!({ 5 });
    let by_hand = construct::maximize([OptimizeElement::new(
        weight(Term::from(5i32)),
        [],
        Condition::empty(),
    )]);
    assert_eq!(by_macro, by_hand);
}

#[test]
fn the_show_macro_builds_a_signature_show() {
    let by_macro = z::show!(p / 1);
    assert_eq!(
        by_macro,
        Show::Signature(Signature::new(Sign::Positive, name("p"), 1))
    );
}

#[test]
fn the_external_macro_builds_an_external() {
    let by_macro = z::external!(p(X));
    let by_hand = External::new(
        Atom::new(name("p"), [Term::variable(var("X"))]),
        Body::empty(),
        None,
    );
    assert_eq!(by_macro, by_hand);
}
