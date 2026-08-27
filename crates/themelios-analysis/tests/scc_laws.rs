//! Laws of the strongly-connected-components decomposition (docs/design/analysis.md
//! §4): correctness — the components partition the predicates and agree with a naive
//! reachability reference on generated graphs, in reverse-topological order, each
//! reporting the kinds its internal recursion runs through, and the positive graph's
//! components finer — and **performance**: the hand-rolled iterative Tarjan
//! decomposes a graph tens of thousands deep in its dependency chain, and one giant
//! cycle, without overflowing the call stack and in linear time (a recursive
//! implementation would overflow, a quadratic reachability would not finish). The
//! precise scaling curve is `benches/scaling.rs`.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use themelios_analysis::depend::{DependencyGraph, Signature};
use themelios_program::construct::not;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, BodyAggregateElement, BodyElement, Condition,
    DefaultNegation, FunctionAggregate, Guard, Literal, Program, Relation, Rule, Statement,
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

fn graph_of(statements: impl IntoIterator<Item = Statement>) -> DependencyGraph {
    DependencyGraph::of(&Program::of(
        statements.into_iter().map(WithProvenance::constructed),
    ))
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

fn members(component: impl Iterator<Item = Signature>) -> BTreeSet<Signature> {
    component.collect()
}

fn component_members(graph: &DependencyGraph, predicate: &Signature) -> BTreeSet<Signature> {
    graph
        .component_of(predicate)
        .expect("the predicate is a node")
        .members()
        .cloned()
        .collect()
}

// ---- the naive reachability reference (the oracle, §4/§10) ----
//
// The obviously-correct O(nodes²) SCC: `u` and `v` share a component iff each
// reaches the other. It reads the graph's public edges, so it checks the iterative
// Tarjan without sharing its representation.

fn reachable_from(
    successors: &BTreeMap<Signature, BTreeSet<Signature>>,
    start: &Signature,
) -> BTreeSet<Signature> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<Signature> = successors
        .get(start)
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    while let Some(node) = stack.pop() {
        if seen.insert(node.clone()) {
            stack.extend(successors.get(&node).into_iter().flatten().cloned());
        }
    }
    seen
}

fn naive_partition(graph: &DependencyGraph) -> BTreeSet<BTreeSet<Signature>> {
    let nodes: Vec<Signature> = graph.predicates().cloned().collect();
    let successors: BTreeMap<Signature, BTreeSet<Signature>> = nodes
        .iter()
        .map(|node| {
            (
                node.clone(),
                graph.edges_from(node).map(|(_, to)| to.clone()).collect(),
            )
        })
        .collect();
    let reach: BTreeMap<Signature, BTreeSet<Signature>> = nodes
        .iter()
        .map(|node| (node.clone(), reachable_from(&successors, node)))
        .collect();
    let mut partition = BTreeSet::new();
    let mut assigned: BTreeSet<Signature> = BTreeSet::new();
    for u in &nodes {
        if assigned.contains(u) {
            continue;
        }
        let mut scc = BTreeSet::new();
        scc.insert(u.clone());
        for v in &nodes {
            if v != u && reach[u].contains(v) && reach[v].contains(u) {
                scc.insert(v.clone());
            }
        }
        assigned.extend(scc.iter().cloned());
        partition.insert(scc);
    }
    partition
}

fn tarjan_partition(graph: &DependencyGraph) -> BTreeSet<BTreeSet<Signature>> {
    graph
        .components()
        .map(|component| component.members().cloned().collect())
        .collect()
}

// A program whose dependency graph is the given adjacency over predicates `p0..`.
// A predicate with no successor is a fact, so it is still a node.
fn graph_from_matrix(adjacency: &[Vec<bool>]) -> DependencyGraph {
    let statements: Vec<Statement> = adjacency
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let successors: Vec<BodyElement> = row
                .iter()
                .enumerate()
                .filter(|&(_, &edge)| edge)
                .map(|(j, _)| BodyElement::from(atom(&format!("p{j}"))))
                .collect();
            if successors.is_empty() {
                Statement::Rule(Rule::fact(atom(&format!("p{i}"))))
            } else {
                Statement::Rule(Rule::new(atom(&format!("p{i}")), successors))
            }
        })
        .collect();
    graph_of(statements)
}

fn any_adjacency() -> impl Strategy<Value = Vec<Vec<bool>>> {
    (1usize..8).prop_flat_map(|k| prop::collection::vec(prop::collection::vec(any::<bool>(), k), k))
}

// ---- Correctness ----

proptest! {
    /// The components partition the predicates and agree with the naive reachability
    /// reference — the correctness oracle, over graphs of every small shape (§4, §10).
    #[test]
    fn the_partition_agrees_with_the_naive_reachability_reference(adjacency in any_adjacency()) {
        let graph = graph_from_matrix(&adjacency);
        prop_assert_eq!(tarjan_partition(&graph), naive_partition(&graph));
    }

    /// The component order is reverse-topological: for every edge between two
    /// components, the depended-on component appears first (§4).
    #[test]
    fn the_components_are_in_reverse_topological_order(adjacency in any_adjacency()) {
        let graph = graph_from_matrix(&adjacency);
        let position: BTreeMap<BTreeSet<Signature>, usize> = graph
            .components()
            .enumerate()
            .map(|(i, component)| (component.members().cloned().collect(), i))
            .collect();
        for u in graph.predicates() {
            let from = component_members(&graph, u);
            for (_, v) in graph.edges_from(u) {
                let to = component_members(&graph, v);
                if from != to {
                    prop_assert!(
                        position[&to] < position[&from],
                        "the depended-on component {:?} must precede {:?}", to, from,
                    );
                }
            }
        }
    }
}

#[test]
fn is_recursive_and_the_internal_edge_kinds_are_reported() {
    // A positive cycle: p :- q.  q :- p.
    let graph = graph_of([
        Statement::Rule(Rule::new(atom("p"), atom("q"))),
        Statement::Rule(Rule::new(atom("q"), atom("p"))),
    ]);
    let component = graph.component_of(&pos("p", 0)).expect("p is a node");
    assert_eq!(
        members(component.members().cloned()),
        [pos("p", 0), pos("q", 0)].into_iter().collect(),
    );
    assert!(component.is_recursive());
    assert!(component.has_positive_cycle());
    assert!(!component.has_negative_cycle());
    assert!(!component.has_aggregate_cycle());

    // A self-loop: p :- p.
    let graph = graph_of([Statement::Rule(Rule::new(atom("p"), atom("p")))]);
    let component = graph.component_of(&pos("p", 0)).unwrap();
    assert!(component.is_recursive(), "a self-loop is recursive");
    assert!(component.has_positive_cycle());

    // A DAG node: p :- q.  — neither predicate is recursive.
    let graph = graph_of([Statement::Rule(Rule::new(atom("p"), atom("q")))]);
    assert!(!graph.component_of(&pos("p", 0)).unwrap().is_recursive());
    assert!(!graph.component_of(&pos("q", 0)).unwrap().is_recursive());

    // A pure negative cycle: p :- not q.  q :- not p.
    let graph = graph_of([
        Statement::Rule(Rule::new(atom("p"), not(atom("q")))),
        Statement::Rule(Rule::new(atom("q"), not(atom("p")))),
    ]);
    let component = graph.component_of(&pos("p", 0)).unwrap();
    assert!(component.is_recursive());
    assert!(component.has_negative_cycle());
    assert!(
        !component.has_positive_cycle(),
        "a pure negative cycle has no positive edge"
    );

    // An aggregate cycle: p :- #count { X : q(X) } >= 1.   q(X) :- p.
    let graph = graph_of([
        Statement::Rule(Rule::new(
            atom("p"),
            vec![count_over(DefaultNegation::None, "q")],
        )),
        Statement::Rule(Rule::new(pred1("q", "X"), atom("p"))),
    ]);
    let component = graph.component_of(&pos("p", 0)).unwrap();
    assert_eq!(
        members(component.members().cloned()),
        [pos("p", 0), pos("q", 1)].into_iter().collect(),
    );
    assert!(component.has_aggregate_cycle());
    assert!(component.has_positive_cycle());
    assert!(!component.has_negative_cycle());
}

#[test]
fn positive_components_are_finer_and_is_acyclic_agrees() {
    // p :- q.   q :- not p.   — the full graph joins {p, q} through the negative
    // edge; the positive graph drops it, so they split.
    let graph = graph_of([
        Statement::Rule(Rule::new(atom("p"), atom("q"))),
        Statement::Rule(Rule::new(atom("q"), not(atom("p")))),
    ]);
    assert_eq!(
        component_members(&graph, &pos("p", 0)),
        [pos("p", 0), pos("q", 0)].into_iter().collect(),
    );
    assert!(graph.component_of(&pos("p", 0)).unwrap().is_recursive());
    assert!(
        !graph.is_acyclic(),
        "the full graph has a recursive component"
    );

    let positive = graph.positive();
    assert_ne!(
        component_members(&positive, &pos("p", 0)),
        component_members(&positive, &pos("q", 0)),
        "the negative edge no longer joins them",
    );
    assert!(positive.is_acyclic(), "the positive graph is acyclic");
}

// ---- Performance: the scaling tripwires (§4, algorithms-of-import) ----

#[test]
fn a_deep_chain_decomposes_without_overflowing_the_stack() {
    // p0 :- p1.  p1 :- p2.  …  a chain far deeper than the call stack. A recursive
    // Tarjan overflows here; a quadratic reachability would not finish.
    const N: usize = 50_000;
    let statements: Vec<Statement> = (0..N)
        .map(|i| {
            Statement::Rule(Rule::new(
                atom(&format!("p{i}")),
                atom(&format!("p{}", i + 1)),
            ))
        })
        .collect();
    let graph = graph_of(statements);
    assert_eq!(
        graph.components().count(),
        N + 1,
        "each predicate in a chain is its own component",
    );
    assert!(graph.is_acyclic(), "a chain is acyclic");
    assert!(
        graph
            .components()
            .all(|component| !component.is_recursive())
    );
}

#[test]
fn a_large_cycle_decomposes_without_overflowing_the_stack() {
    // p0 :- p1.  …  p_{N-1} :- p0.  — one strongly-connected component of N predicates
    // (a deep DFS and a large single-component pop, both on the explicit stack).
    const N: usize = 50_000;
    let statements: Vec<Statement> = (0..N)
        .map(|i| {
            Statement::Rule(Rule::new(
                atom(&format!("p{i}")),
                atom(&format!("p{}", (i + 1) % N)),
            ))
        })
        .collect();
    let graph = graph_of(statements);
    assert_eq!(graph.components().count(), 1, "a cycle is one component");
    let component = graph.components().next().unwrap();
    assert_eq!(
        component.members().count(),
        N,
        "the component spans the cycle"
    );
    assert!(component.is_recursive());
    assert!(component.has_positive_cycle());
    assert!(!graph.is_acyclic());
}
