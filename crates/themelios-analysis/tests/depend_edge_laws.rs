//! Laws of the predicate dependency graph (docs/design/analysis.md §4): the nodes
//! are exactly the program's head and body predicate signatures with `p` and `-p`
//! distinct; the edges are the rules' dependencies tagged by kind, checked against a
//! hand-computed reference — a plain literal `Positive`, a `not`-ed literal
//! `Negative`, a predicate inside an aggregate `ThroughAggregate`, one inside a
//! *negated* aggregate **both**, and one inside a condition reached; a choice or
//! disjunctive head contributes from each head predicate; and `positive()` retains
//! exactly the `Positive` edges.

use std::collections::BTreeSet;

use themelios_analysis::depend::{DependencyGraph, DependencyKind, Signature};
use themelios_program::construct::not;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, BodyAggregateElement, BodyElement, Choice, ChoiceElement,
    Condition, ConditionalLiteral, DefaultNegation, Disjunction, DisjunctionElement,
    FunctionAggregate, Guard, Head, Literal, Program, Relation, Rule, Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{Term, Variable};

// ---- helpers ----

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

fn atom(text: &str) -> Atom {
    Atom::constant(name(text))
}

fn pred1(text: &str, arg: &str) -> Atom {
    Atom::new(name(text), [var(arg)])
}

fn pos(text: &str, arity: u32) -> Signature {
    Signature {
        sign: Sign::Positive,
        name: name(text),
        arity,
    }
}

fn neg(text: &str, arity: u32) -> Signature {
    Signature {
        sign: Sign::Negative,
        name: name(text),
        arity,
    }
}

fn graph_of(statements: impl IntoIterator<Item = Statement>) -> DependencyGraph {
    DependencyGraph::of(&Program::of(
        statements.into_iter().map(WithProvenance::constructed),
    ))
}

fn out(graph: &DependencyGraph, from: &Signature) -> BTreeSet<(DependencyKind, Signature)> {
    graph
        .edges_from(from)
        .map(|(kind, signature)| (kind, signature.clone()))
        .collect()
}

fn nodes(graph: &DependencyGraph) -> BTreeSet<Signature> {
    graph.predicates().cloned().collect()
}

// `[not] #count { X : predicate(X) } >= 1` as a body element.
fn count_over(negation: DefaultNegation, predicate: &str) -> BodyElement {
    BodyElement::Aggregate {
        negation,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            None,
            AggregateFunction::Count,
            [BodyAggregateElement::new(
                [var("X")],
                Condition::new([Literal::from(pred1(predicate, "X"))]),
            )],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: num(1),
            }),
        )),
    }
}

fn edge(kind: DependencyKind, to: Signature) -> BTreeSet<(DependencyKind, Signature)> {
    [(kind, to)].into_iter().collect()
}

// ---- owned plain data (§8) ----

const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<DependencyGraph>();
};

// ---- Law: nodes are the head and body predicates, the strong sign distinguished ----

#[test]
fn nodes_are_the_head_and_body_predicates_with_the_strong_sign_distinguished() {
    // p :- q.   -p :- r.
    let graph = graph_of([
        Statement::Rule(Rule::new(atom("p"), atom("q"))),
        Statement::Rule(Rule::new(-atom("p"), atom("r"))),
    ]);
    let expected: BTreeSet<Signature> = [pos("p", 0), pos("q", 0), neg("p", 0), pos("r", 0)]
        .into_iter()
        .collect();
    assert_eq!(nodes(&graph), expected);
    assert_ne!(pos("p", 0), neg("p", 0), "p and -p are distinct nodes");

    // A fact contributes its head as a node with no out-edge.
    let graph = graph_of([Statement::Rule(Rule::fact(atom("t")))]);
    assert!(nodes(&graph).contains(&pos("t", 0)));
    assert!(
        out(&graph, &pos("t", 0)).is_empty(),
        "a fact's head has no out-edge",
    );
}

// ---- Law: edges carry each occurrence's dependency kind ----

#[test]
fn edges_carry_the_dependency_kind_of_each_occurrence() {
    // p :- q.  → Positive
    let graph = graph_of([Statement::Rule(Rule::new(atom("p"), atom("q")))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        edge(DependencyKind::Positive, pos("q", 0)),
    );

    // p :- not q.  → Negative
    let graph = graph_of([Statement::Rule(Rule::new(atom("p"), not(atom("q"))))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        edge(DependencyKind::Negative, pos("q", 0)),
    );

    // p :- #count { X : q(X) } >= 1.  → ThroughAggregate
    let graph = graph_of([Statement::Rule(Rule::new(
        atom("p"),
        vec![count_over(DefaultNegation::None, "q")],
    ))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        edge(DependencyKind::ThroughAggregate, pos("q", 1)),
    );

    // p :- not #count { X : q(X) } >= 1.  → BOTH ThroughAggregate and Negative
    let graph = graph_of([Statement::Rule(Rule::new(
        atom("p"),
        vec![count_over(DefaultNegation::Not, "q")],
    ))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        [
            (DependencyKind::ThroughAggregate, pos("q", 1)),
            (DependencyKind::Negative, pos("q", 1)),
        ]
        .into_iter()
        .collect(),
    );
}

// ---- Law: a predicate inside a condition is reached ----

#[test]
fn a_predicate_inside_a_condition_is_reached() {
    // p :- q : r.   → p→q and p→r, both Positive.
    let conditional = BodyElement::Conditional(ConditionalLiteral {
        literal: Literal::from(atom("q")),
        condition: Condition::new([Literal::from(atom("r"))]),
    });
    let graph = graph_of([Statement::Rule(Rule::new(atom("p"), vec![conditional]))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        [
            (DependencyKind::Positive, pos("q", 0)),
            (DependencyKind::Positive, pos("r", 0)),
        ]
        .into_iter()
        .collect(),
    );
    assert!(
        nodes(&graph).contains(&pos("r", 0)),
        "the condition predicate is a node",
    );
}

// ---- Law: each head predicate gets an edge; edges_from yields every kind ----

#[test]
fn each_head_predicate_gets_an_edge_and_edges_from_yields_every_kind() {
    // p | q :- r.   → p→r and q→r.
    let disjunction = Head::Disjunction(Disjunction::new([
        DisjunctionElement::new(Literal::from(atom("p")), Condition::empty()),
        DisjunctionElement::new(Literal::from(atom("q")), Condition::empty()),
    ]));
    let graph = graph_of([Statement::Rule(disjunction.when(atom("r")))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        edge(DependencyKind::Positive, pos("r", 0)),
    );
    assert_eq!(
        out(&graph, &pos("q", 0)),
        edge(DependencyKind::Positive, pos("r", 0)),
    );

    // { p; q } :- r.   → same.
    let choice = Head::Choice(Choice::new(
        None,
        [
            ChoiceElement::new(Literal::from(atom("p")), Condition::empty()),
            ChoiceElement::new(Literal::from(atom("q")), Condition::empty()),
        ],
        None,
    ));
    let graph = graph_of([Statement::Rule(choice.when(atom("r")))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        edge(DependencyKind::Positive, pos("r", 0)),
    );
    assert_eq!(
        out(&graph, &pos("q", 0)),
        edge(DependencyKind::Positive, pos("r", 0)),
    );

    // p :- q, not q.   → edges_from(p) yields both kinds.
    let graph = graph_of([Statement::Rule(Rule::new(
        atom("p"),
        vec![BodyElement::from(atom("q")), not(atom("q"))],
    ))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        [
            (DependencyKind::Positive, pos("q", 0)),
            (DependencyKind::Negative, pos("q", 0)),
        ]
        .into_iter()
        .collect(),
    );
}

// ---- Law: positive() retains exactly the Positive edges ----

#[test]
fn positive_retains_the_positive_edges_and_drops_the_rest() {
    // p :- q, not r, #count { X : s(X) } >= 1.
    let graph = graph_of([Statement::Rule(Rule::new(
        atom("p"),
        vec![
            BodyElement::from(atom("q")),
            not(atom("r")),
            count_over(DefaultNegation::None, "s"),
        ],
    ))]);
    assert_eq!(
        out(&graph, &pos("p", 0)),
        [
            (DependencyKind::Positive, pos("q", 0)),
            (DependencyKind::Negative, pos("r", 0)),
            (DependencyKind::ThroughAggregate, pos("s", 1)),
        ]
        .into_iter()
        .collect(),
    );

    let positive = graph.positive();
    assert_eq!(
        out(&positive, &pos("p", 0)),
        edge(DependencyKind::Positive, pos("q", 0)),
        "positive() keeps only the Positive edge",
    );
    assert_eq!(
        nodes(&positive),
        nodes(&graph),
        "positive() keeps every node",
    );
}
