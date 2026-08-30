//! Laws of the assembled `Program` value (docs/design/program.md §4, §5, §12.1): the
//! set-shaped children are sets (a duplicate vanishes, a reordering is the same value);
//! a program's equality is canonical-form equality up to provenance, and the ingest door
//! canonicalizes the terms it admits so a raw and a collapsed spelling are one value; and
//! the structural substrate reports a rule's variables, groundness, and head and body
//! predicate signatures, a predicate inside a negated aggregate carrying two dependency
//! modes.

use themelios_program::analyze::DependencyKind;
use themelios_program::program::{
    Aggregate, AggregateFunction, Arguments, Atom, Body, BodyAggregateElement, BodyElement,
    Condition, DefaultNegation, Disjunction, DisjunctionElement, FunctionAggregate, Head, Literal,
    LiteralInner, Program, Rule, Statement,
};
use themelios_program::provenance::{Origin, Provenance, WithProvenance};
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{Term, Variable};

fn name(text: &str) -> Name {
    Name::new(text).expect("a lowercase identifier")
}

fn named(text: &str) -> Variable {
    Variable::Named(VarName::new(text).expect("a variable"))
}

fn var(text: &str) -> Term {
    Term::Variable(named(text))
}

fn atom(predicate: &str, arguments: Vec<Term>) -> Atom {
    Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    }
}

fn positive(atom: Atom) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
}

fn negative(atom: Atom) -> Literal {
    Literal {
        negation: DefaultNegation::Not,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
}

fn signature(predicate: &str, arity: u32) -> Signature {
    Signature {
        sign: Sign::Positive,
        name: name(predicate),
        arity,
    }
}

#[test]
fn a_body_is_a_set_a_duplicate_vanishes_and_a_reordering_is_the_same_value() {
    let p = || BodyElement::Literal(positive(atom("p", vec![])));
    let q = || BodyElement::Literal(positive(atom("q", vec![])));

    let with_duplicate = Body::new([p(), q(), p()]);
    assert_eq!(with_duplicate.elements().count(), 2);

    let one_order = Body::new([p(), q()]);
    let other_order = Body::new([q(), p()]);
    assert_eq!(one_order, other_order);
}

#[test]
fn a_disjunction_is_a_set_a_duplicate_head_atom_vanishes() {
    let element = || DisjunctionElement::new(positive(atom("a", vec![])), Condition::empty());
    let disjunction = Disjunction::new([element(), element()]);
    assert_eq!(disjunction.elements().count(), 1);
}

#[test]
fn a_rule_with_its_body_in_two_orders_is_one_rule() {
    let head = || positive(atom("p", vec![]));
    let one = Rule::new(
        head(),
        Body::new([
            BodyElement::Literal(positive(atom("q", vec![]))),
            BodyElement::Literal(positive(atom("r", vec![]))),
        ]),
    );
    let other = Rule::new(
        head(),
        Body::new([
            BodyElement::Literal(positive(atom("r", vec![]))),
            BodyElement::Literal(positive(atom("q", vec![]))),
        ]),
    );
    assert_eq!(one, other);
}

#[test]
fn program_equality_is_canonical_form_equality_up_to_provenance() {
    let rule = || Statement::Rule(Rule::new(positive(atom("p", vec![])), Body::empty()));
    let here = WithProvenance::new(rule(), Provenance::from(Origin::Constructed));
    let there = WithProvenance::new(
        rule(),
        Provenance::from(Origin::Transformed(
            themelios_program::provenance::TransformTag::new("t"),
        )),
    );
    // Same rule, different provenance: the two programs are equal.
    assert_eq!(Program::of([here]), Program::of([there]));
}

#[test]
fn the_ingest_canonicalizes_the_terms_it_admits() {
    // `p(f(1))` built with a raw ground `Function` argument, and built with the collapsed
    // `Symbol`, are one value once each passes the ingest door (§5.1 ground collapse).
    let raw = Rule::new(
        positive(atom(
            "p",
            vec![Term::Function {
                name: name("f"),
                arguments: vec![Term::Symbolic(Symbol::Number(1))],
            }],
        )),
        Body::empty(),
    );
    let collapsed = Rule::new(
        positive(atom(
            "p",
            vec![Term::Symbolic(Symbol::Function {
                name: name("f"),
                arguments: vec![Symbol::Number(1)],
                sign: Sign::Positive,
            })],
        )),
        Body::empty(),
    );
    // The two rules differ before the door — the raw one is not canonical.
    assert_ne!(raw, collapsed);
    // After the door, the programs are equal.
    let raw_program = Program::of([WithProvenance::constructed(Statement::Rule(raw))]);
    let collapsed_program = Program::of([WithProvenance::constructed(Statement::Rule(collapsed))]);
    assert_eq!(raw_program, collapsed_program);
}

#[test]
fn the_base_part_is_always_present_and_holds_the_admitted_statements() {
    let program = Program::of([WithProvenance::constructed(Statement::Rule(Rule::new(
        positive(atom("p", vec![])),
        Body::empty(),
    )))]);
    assert_eq!(program.base().statements().count(), 1);
    assert_eq!(program.statements().count(), 1);
}

#[test]
fn the_substrate_reports_the_variables_groundness_and_predicates_of_a_rich_rule() {
    // p(X) :- q(X), not r(Y).
    let rule = Rule::new(
        positive(atom("p", vec![var("X")])),
        Body::new([
            BodyElement::Literal(positive(atom("q", vec![var("X")]))),
            BodyElement::Literal(negative(atom("r", vec![var("Y")]))),
        ]),
    );

    let variables: Vec<Variable> = rule.variables().cloned().collect();
    assert_eq!(variables, vec![named("X"), named("Y")]);
    assert!(!rule.is_ground());

    let head: Vec<Signature> = rule.head_signatures().collect();
    assert_eq!(head, vec![signature("p", 1)]);

    let body: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    assert_eq!(
        body,
        vec![
            (DependencyKind::Positive, signature("q", 1)),
            (DependencyKind::Negative, signature("r", 1)),
        ]
    );
}

#[test]
fn a_ground_rule_reports_is_ground_and_no_variables() {
    let rule = Rule::new(
        positive(atom("p", vec![Term::Symbolic(Symbol::Number(1))])),
        Body::empty(),
    );
    assert!(rule.is_ground());
    assert_eq!(rule.variables().count(), 0);
}

#[test]
fn a_predicate_inside_a_negated_aggregate_yields_both_through_aggregate_and_negative() {
    // :- not #count { X : s(X) }.  — the aggregate is default-negated.
    let element = BodyAggregateElement::new(
        [var("X")],
        Condition::new([positive(atom("s", vec![var("X")]))]),
    );
    let aggregate = Aggregate::Function(FunctionAggregate::new(
        None,
        AggregateFunction::Count,
        [element],
        None,
    ));
    let rule = Rule::new(
        Head::Falsum,
        Body::new([BodyElement::Aggregate {
            negation: DefaultNegation::Not,
            aggregate,
        }]),
    );

    let body: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    assert_eq!(
        body,
        vec![
            (DependencyKind::ThroughAggregate, signature("s", 1)),
            (DependencyKind::Negative, signature("s", 1)),
        ]
    );
}

#[test]
fn the_empty_program_has_a_present_empty_base_and_one_form() {
    let empty = Program::default();
    // base() is total on the default program — no panic (§4.1).
    assert_eq!(empty.base().statements().count(), 0);
    assert!(empty.statements().next().is_none());
    // default() and of([]) denote the one empty program.
    assert_eq!(empty, Program::of([]));
}

#[test]
fn a_predicate_in_a_head_element_condition_is_a_dependency() {
    // `a : b.` — a is derived under the condition b, so a depends on b (the grounder tracks
    // it: `a : b.` with `b :- a.` is unsatisfiable). The edge must reach body_signatures.
    let rule = Rule::new(
        Head::Disjunction(Disjunction::new([DisjunctionElement::new(
            positive(atom("a", vec![])),
            Condition::new([positive(atom("b", vec![]))]),
        )])),
        Body::empty(),
    );
    let head: Vec<Signature> = rule.head_signatures().collect();
    assert_eq!(head, vec![signature("a", 0)]); // a is derived
    let body: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    assert_eq!(body, vec![(DependencyKind::Positive, signature("b", 0))]); // and depends on b
}

#[test]
fn each_anonymous_variable_is_distinct_while_a_named_one_is_deduped() {
    // p(X, X, _, _): X once (named, deduped); each _ a distinct fresh variable, as the
    // grounder treats `_` (§12.1).
    let anon = || Term::Variable(Variable::Anonymous);
    let rule = Rule::new(
        positive(atom("p", vec![var("X"), var("X"), anon(), anon()])),
        Body::empty(),
    );
    let variables: Vec<Variable> = rule.variables().cloned().collect();
    assert_eq!(
        variables,
        vec![named("X"), Variable::Anonymous, Variable::Anonymous]
    );
}
