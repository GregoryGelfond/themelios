//! The declarative construction surface (docs/design/program.md §7): strong versus
//! arithmetic negation, role-typed default negation, the rule-reads-as-the-rule
//! constructors, arithmetic and intervals, the widening coercions, and the
//! two-audiences-one-value seed — a program built through the surface is
//! structurally equal to the same program assembled from the primitive constructors
//! (the *first-solve* witness, §7.3, §16). The compile-fail half of the role-typing
//! (that `not` in head position does not compile) is a `compile_fail` doc example on
//! `construct::not`.

use themelios_program::construct::{maximize, minimize, not, not_not};
use themelios_program::program::{
    Aggregate, Arguments, Atom, Body, BodyElement, Condition, DefaultNegation, Direction, Head,
    IntoBody, IntoHead, Literal, LiteralInner, OptimizeElement, Program, Rule, SetAggregate,
    SetElement, Statement, WeakConstraint, weight,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};

// ---- small helpers (the terse spellings are the macro tier's, §7.1) ----

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn var(text: &str) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable"),
    ))
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

// ---- The faithful argument-list pool (§4.6): a pooled atom carries every
// alternative, and its alternatives may differ in arity (`p(1; 2, 3)` is p/1 and
// p/2). A one-alternative pool is a `Single` atom by canonicalization (§5.1). ----

#[test]
fn a_pooled_atom_carries_every_alternative() {
    let atom = Atom::pooled(name("p"), [vec![num(1)], vec![num(2), num(3)]]);
    assert!(atom.is_pooled());
    let alternatives: Vec<Vec<Term>> = atom.alternatives().map(<[Term]>::to_vec).collect();
    assert_eq!(alternatives, vec![vec![num(1)], vec![num(2), num(3)]]);
}

#[test]
fn a_one_alternative_pool_canonicalizes_to_a_single_atom() {
    let atom = Atom::pooled(name("p"), [vec![num(1)]]);
    assert!(!atom.is_pooled());
    let alternatives: Vec<Vec<Term>> = atom.alternatives().map(<[Term]>::to_vec).collect();
    assert_eq!(alternatives, vec![vec![num(1)]]);
}

// ---- Strong versus arithmetic negation (§4.6) ----

#[test]
fn strong_negation_flips_the_sign_and_is_involutive() {
    let atom = Atom::new(name("p"), [var("X")]);
    assert_eq!(atom.sign, Sign::Positive);

    let negated = -atom.clone();
    assert_eq!(
        negated.sign,
        Sign::Negative,
        "strong negation flips the sign"
    );
    assert_eq!(-(-atom.clone()), atom, "strong negation is involutive");
}

#[test]
fn arithmetic_negation_is_a_unary_negate() {
    match -var("X") {
        Term::UnaryOperation { operator, .. } => assert_eq!(operator, UnaryOp::Negate),
        other => panic!("arithmetic negation is a UnaryOperation, got {other:?}"),
    }
}

#[test]
fn strong_and_arithmetic_negation_do_not_conflate() {
    // `-p(X)` is a strongly-negated atom; `-(X)` is an arithmetic term — different
    // types, different values, and no coercion turns one into the other.
    let strong = -Atom::new(name("p"), [var("X")]);
    let arithmetic = -var("X");
    assert_eq!(strong.sign, Sign::Negative);
    assert!(matches!(arithmetic, Term::UnaryOperation { .. }));
}

// ---- Role-typed default negation (§4.5, §7.1) ----

#[test]
fn default_negation_over_an_atom_is_a_body_literal() {
    match not(Atom::new(name("p"), [var("X")])) {
        BodyElement::Literal(literal) => {
            assert_eq!(literal.negation, DefaultNegation::Not);
            match literal.inner {
                LiteralInner::Atom(atom) => assert_eq!(atom.get().name, name("p")),
                other => panic!("expected an atom literal, got {other:?}"),
            }
        }
        other => panic!("`not(atom)` is a body literal, got {other:?}"),
    }
}

#[test]
fn double_default_negation_is_its_own_case() {
    let once = not(Atom::new(name("p"), [var("X")]));
    let twice = not_not(Atom::new(name("p"), [var("X")]));
    let bare = BodyElement::from(Atom::new(name("p"), [var("X")]));

    assert_ne!(once, twice, "`not not` is distinct from `not`");
    assert_ne!(once, bare, "`not` is distinct from bare");
    assert_ne!(twice, bare, "`not not` is distinct from bare");

    match twice {
        BodyElement::Literal(literal) => assert_eq!(literal.negation, DefaultNegation::NotNot),
        other => panic!("`not_not(atom)` is a body literal, got {other:?}"),
    }
}

#[test]
fn default_negation_over_an_aggregate_is_a_negated_body_element() {
    let literal = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(Atom::new(
            name("q"),
            [var("X")],
        ))),
    };
    let aggregate = Aggregate::Set(SetAggregate::new(
        None,
        [SetElement::Literal(literal)],
        None,
    ));

    match not(aggregate) {
        BodyElement::Aggregate { negation, .. } => assert_eq!(negation, DefaultNegation::Not),
        other => panic!("`not(aggregate)` is a negated body element, got {other:?}"),
    }
}

// ---- The rule surface reads as the rule, and is total (§4.3, §7.1, §7.2) ----

#[test]
fn a_fact_is_a_single_literal_head_and_an_empty_body() {
    let fact = Rule::fact(Atom::new(name("p"), [num(1)]));
    assert!(fact.is_fact());
    assert!(fact.body().get().is_empty());
    assert!(matches!(fact.head().get(), Head::Literal(_)));
}

#[test]
fn a_constraint_is_a_falsum_head_over_its_body() {
    let constraint = Rule::constraint(not(Atom::new(name("p"), [num(1)])));
    assert!(constraint.is_constraint());
    assert!(matches!(constraint.head().get(), Head::Falsum));
}

#[test]
fn a_rule_holds_when_its_body_does() {
    let rule = Atom::new(name("reach"), [var("X"), var("Z")])
        .into_head()
        .when([
            Atom::new(name("reach"), [var("X"), var("Y")]),
            Atom::new(name("edge"), [var("Y"), var("Z")]),
        ]);
    assert!(!rule.is_fact());
    assert!(!rule.is_constraint());
    assert_eq!(rule.body().get().elements().count(), 2);
}

// ---- Canonicalization at the door (§5.1) ----

#[test]
fn atom_new_canonicalizes_ground_function_arguments() {
    let ground_function = Term::Function {
        name: name("f"),
        arguments: vec![num(1)],
    };
    let atom = Atom::new(name("p"), [ground_function]);
    assert_eq!(
        atom.arguments,
        Arguments::Single(vec![Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        })]),
        "a ground constructor argument collapses to a Symbolic leaf",
    );
}

#[test]
fn a_rule_built_through_the_surface_is_already_canonical() {
    let ground = Term::Function {
        name: name("f"),
        arguments: vec![num(1)],
    };
    let built = Rule::fact(Atom::new(name("p"), [ground]));
    let collapsed = Rule::fact(Atom::new(
        name("p"),
        [Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        })],
    ));
    assert_eq!(
        built, collapsed,
        "the surface yields the canonical form directly"
    );
}

// ---- Coercion widens the one obvious spelling, it does not branch (§7.1) ----

#[test]
fn an_i32_argument_coerces_to_a_number_term() {
    let from_i32 = Atom::new(name("p"), [Term::from(1)]);
    let from_symbol = Atom::new(name("p"), [Term::Symbolic(Symbol::Number(1))]);
    assert_eq!(from_i32, from_symbol);
}

#[test]
fn a_literal_and_an_atom_both_reach_a_one_literal_head() {
    let atom = Atom::new(name("p"), [var("X")]);
    let via_atom: Head = atom.clone().into_head();
    let via_literal: Head = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
    .into_head();

    assert_eq!(via_atom, via_literal);
    assert!(matches!(via_atom, Head::Literal(_)));
}

#[test]
fn a_single_element_and_a_one_element_sequence_reach_the_same_body() {
    let element = || BodyElement::from(Atom::new(name("p"), [var("X")]));
    let single: Body = element().into_body();
    let sequence: Body = [element()].into_body();

    assert_eq!(single, sequence);
    assert_eq!(single.elements().count(), 1);
}

// ---- Arithmetic and intervals compose as written (§7.1) ----

#[test]
fn arithmetic_composes_over_term_shaped_values() {
    match var("X") + 1 {
        Term::BinaryOperation { operator, .. } => assert_eq!(operator, BinaryOp::Add),
        other => panic!("`X + 1` is a BinaryOperation, got {other:?}"),
    }
    match var("X").to(10) {
        Term::Interval { .. } => {}
        other => panic!("`X.to(10)` is an Interval, got {other:?}"),
    }
    match var("X").pow(2) {
        Term::BinaryOperation { operator, .. } => assert_eq!(operator, BinaryOp::Pow),
        other => panic!("`X.pow(2)` is a Pow, got {other:?}"),
    }
    match var("X").complement() {
        Term::UnaryOperation { operator, .. } => assert_eq!(operator, UnaryOp::BitwiseNot),
        other => panic!("`X.complement()` is a BitwiseNot, got {other:?}"),
    }
    match var("X").abs() {
        Term::Absolute(_) => {}
        other => panic!("`X.abs()` is an Absolute, got {other:?}"),
    }
}

// ---- Optimization builds on `weight(w).at_priority(p)` (§4.7) ----

#[test]
fn optimization_builds_on_a_first_class_weight_at_priority() {
    let element = OptimizeElement::new(weight(1).at_priority(2), [var("X")], Condition::empty());

    let minimized = minimize([element.clone()]);
    assert_eq!(minimized.direction, Direction::Minimize);
    assert_eq!(minimized.elements().count(), 1);

    let maximized = maximize([element]);
    assert_eq!(maximized.direction, Direction::Maximize);

    let weak = WeakConstraint::new(
        Body::new([BodyElement::from(Atom::new(name("p"), [var("X")]))]),
        weight(1).at_priority(2),
        [var("X")],
    );
    assert_eq!(weak.weight(), &weight(1).at_priority(2));
}

// ---- The two audiences, one value (the first-solve construction seed, §7.3, §16) ----

/// A small reachability program built through the declarative surface: facts, rules
/// with bodies, arithmetic, and a constraint under default negation.
fn reachability_through_the_surface() -> Program {
    let edge_fact = Rule::fact(Atom::new(name("edge"), [Term::from(1), Term::from(2)]));

    let reach_base = Atom::new(name("reach"), [var("X"), var("Y")])
        .into_head()
        .when(Atom::new(name("edge"), [var("X"), var("Y")]));

    let reach_step = Atom::new(name("reach"), [var("X"), var("Z")])
        .into_head()
        .when([
            Atom::new(name("reach"), [var("X"), var("Y")]),
            Atom::new(name("edge"), [var("Y"), var("Z")]),
        ]);

    let step = Atom::new(name("step"), [var("X"), var("X") + 1])
        .into_head()
        .when(Atom::new(name("edge"), [var("X"), var("Y")]));

    let must_edge = Rule::constraint(not(Atom::new(name("edge"), [Term::from(1), Term::from(2)])));

    program_of([edge_fact, reach_base, reach_step, step, must_edge])
}

/// The same program assembled from the primitive constructors — explicitly built
/// heads, bodies, and literals.
fn reachability_through_the_primitives() -> Program {
    let atom = |predicate: &str, arguments: Vec<Term>| Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    };
    let head = |predicate: &str, arguments: Vec<Term>| {
        Head::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(predicate, arguments))),
        })
    };
    let body_atom = |predicate: &str, arguments: Vec<Term>| {
        BodyElement::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(predicate, arguments))),
        })
    };

    let edge_fact = Rule::new(head("edge", vec![num(1), num(2)]), Body::empty());

    let reach_base = Rule::new(
        head("reach", vec![var("X"), var("Y")]),
        Body::new([body_atom("edge", vec![var("X"), var("Y")])]),
    );

    let reach_step = Rule::new(
        head("reach", vec![var("X"), var("Z")]),
        Body::new([
            body_atom("reach", vec![var("X"), var("Y")]),
            body_atom("edge", vec![var("Y"), var("Z")]),
        ]),
    );

    let step = Rule::new(
        head(
            "step",
            vec![
                var("X"),
                Term::BinaryOperation {
                    operator: BinaryOp::Add,
                    left: Box::new(var("X")),
                    right: Box::new(num(1)),
                },
            ],
        ),
        Body::new([body_atom("edge", vec![var("X"), var("Y")])]),
    );

    let must_edge = Rule::new(
        Head::Falsum,
        Body::new([BodyElement::Literal(Literal {
            negation: DefaultNegation::Not,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(
                "edge",
                vec![num(1), num(2)],
            ))),
        })]),
    );

    program_of([edge_fact, reach_base, reach_step, step, must_edge])
}

fn program_of(rules: [Rule; 5]) -> Program {
    Program::of(rules.map(|rule| WithProvenance::constructed(Statement::Rule(rule))))
}

#[test]
fn the_two_audiences_converge_on_one_value() {
    let declarative = reachability_through_the_surface();
    let primitive = reachability_through_the_primitives();
    assert_eq!(
        declarative, primitive,
        "the declarative surface and the primitive constructors build one program",
    );
    // Both are canonical: ingesting the program's own statements changes nothing.
    assert_eq!(declarative, Program::of(declarative.statements().cloned()));
}
