//! Laws of the assembled analysis (docs/design/analysis.md §3, §8, §10). The
//! assembly computes the four facets in one shared-graph pass, so its central law is
//! that the facets equal the independent readings — the one that lets the facet law
//! suites (construct, depend, safe, classify) stand for the whole. On top of it: the
//! load-bearing soundness law, that whenever the predicate-level `tightness` or
//! `head_cycle_free` verdict is `Holds` the *ground* program — ground by a naive
//! reference grounder over a bounded, growth-free domain — has the property (a false
//! `Holds` would be caught by a ground graph carrying the cycle the predicate level
//! missed); the definite verdicts against naive references (stratification by the
//! cycle kinds, safety by the binding occurrences); the components against a naive
//! reachability reference, in reverse-topological order; the scan complete, each
//! flag's witness a bearing statement; and totality on every generated program,
//! including a partial one.
//!
//! The naive reference grounder is a simplicity-for-trust oracle: obviously correct
//! by inspection and exempt from the best-known-practical rule, because it grounds a
//! *bounded* generated program (small, function-growth-free domains, a finite
//! Herbrand base) whose grounding terminates by inspection.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use themelios_analysis::analysis::Analysis;
use themelios_analysis::classify::{Classes, ProgramClass, Stratification, Verdict};
use themelios_analysis::construct::Constructs;
use themelios_analysis::depend::{DependencyGraph, DependencyKind, Signature};
use themelios_analysis::safe::Safety;
use themelios_program::construct::not;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, BodyAggregateElement, BodyElement, Choice, ChoiceElement,
    Comparison, Condition, DefaultNegation, Disjunction, DisjunctionElement, FunctionAggregate,
    Guard, Head, Literal, LiteralInner, Program, Relation, Rule, Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Symbol, VarName};
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

fn program_of(statements: impl IntoIterator<Item = Statement>) -> Program {
    Program::of(statements.into_iter().map(WithProvenance::constructed))
}

fn analysis_of(statements: impl IntoIterator<Item = Statement>) -> Analysis {
    Analysis::of(&program_of(statements))
}

// `p :- q.`  and  `p :- not q.`  over nullary predicates.
fn rule(head: &str, body: &str) -> Statement {
    Statement::Rule(Rule::new(atom(head), atom(body)))
}

fn rule_not(head: &str, body: &str) -> Statement {
    Statement::Rule(Rule::new(atom(head), not(atom(body))))
}

// ---- naive references (the oracles, shared with the facet law suites) ----

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

// Stratified iff no dependency cycle runs through a `Negative` or `ThroughAggregate`
// edge — for each such edge `u → v`, whether `v` reaches `u`, independent of the
// Tarjan components (§6.2).
fn naive_stratified(graph: &DependencyGraph) -> bool {
    let successors: BTreeMap<Signature, BTreeSet<Signature>> = graph
        .predicates()
        .map(|node| {
            (
                node.clone(),
                graph.edges_from(node).map(|(_, to)| to.clone()).collect(),
            )
        })
        .collect();
    for u in graph.predicates() {
        for (kind, v) in graph.edges_from(u) {
            if matches!(
                kind,
                DependencyKind::Negative | DependencyKind::ThroughAggregate
            ) && reachable_from(&successors, v).contains(u)
            {
                return false;
            }
        }
    }
    true
}

// The obviously-correct O(nodes²) partition: `u` and `v` share a component iff each
// reaches the other (§4).
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

// The naive safety reference for an aggregate-free rule: a named variable is safe iff
// it is in the least binding set — the variables of positive body atoms, closed under
// assignments `X = t` (a brute-force fixpoint) (§5).
fn term_named_vars(term: &Term, out: &mut BTreeSet<Variable>) {
    for subterm in term.subterms() {
        if let Term::Variable(v @ Variable::Named(_)) = subterm {
            out.insert(v.clone());
        }
    }
}

fn assignment_bindings(comparison: &Comparison) -> Vec<(Variable, BTreeSet<Variable>)> {
    let steps: Vec<(Relation, &Term)> = comparison.steps().collect();
    if steps.len() != 1 || steps[0].0 != Relation::Eq {
        return Vec::new();
    }
    let sides = [comparison.first(), steps[0].1];
    let mut assignments = Vec::new();
    for (i, side) in sides.iter().enumerate() {
        if let Term::Variable(v @ Variable::Named(_)) = side {
            let mut other = BTreeSet::new();
            term_named_vars(sides[1 - i], &mut other);
            if !other.contains(v) {
                assignments.push((v.clone(), other));
            }
        }
    }
    assignments
}

fn naive_unbound(rule: &Rule) -> BTreeSet<Variable> {
    let mut all = BTreeSet::new();
    for variable in rule.variables() {
        if let Variable::Named(_) = variable {
            all.insert(variable.clone());
        }
    }
    let mut bound = BTreeSet::new();
    let mut assignments = Vec::new();
    for element in rule.body().get().elements() {
        if let BodyElement::Literal(literal) = element.get()
            && literal.negation == DefaultNegation::None
        {
            match &literal.inner {
                LiteralInner::Atom(a) => {
                    for term in &a.get().arguments {
                        term_named_vars(term, &mut bound);
                    }
                }
                LiteralInner::Comparison(c) => assignments.extend(assignment_bindings(c.get())),
                LiteralInner::True | LiteralInner::False => {}
            }
        }
    }
    loop {
        let mut changed = false;
        for (lhs, rhs) in &assignments {
            if !bound.contains(lhs) && rhs.iter().all(|v| bound.contains(v)) {
                bound.insert(lhs.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    all.difference(&bound).cloned().collect()
}

// ---- a generated program: k unary predicates, random heads and kinded bodies ----
//
// The broad generator (shared with classify's law suite) for the facet-agreement,
// definite-verdict, graph, scan, and totality laws — normal, disjunctive, and choice
// heads over positive, negative, and aggregate body dependencies.

#[derive(Clone, Copy, Debug)]
enum HeadShape {
    Normal,
    Disjunctive,
    Choice,
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Absent,
    Positive,
    Negative,
    Aggregate,
}

fn pname(i: usize) -> String {
    format!("p{i}")
}

// `[not] #count { X : predicate(X) } >= 1` as a body element (an aggregate dependency).
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

fn build_rule(k: usize, i: usize, shape: HeadShape, modes: &[Mode]) -> Rule {
    let body: Vec<BodyElement> = modes
        .iter()
        .enumerate()
        .filter_map(|(j, mode)| match mode {
            Mode::Absent => None,
            Mode::Positive => Some(BodyElement::from(pred1(&pname(j), "X"))),
            Mode::Negative => Some(not(pred1(&pname(j), "X"))),
            Mode::Aggregate => Some(count_over(DefaultNegation::None, &pname(j))),
        })
        .collect();
    match shape {
        HeadShape::Disjunctive if k >= 2 => Head::Disjunction(Disjunction::new([
            DisjunctionElement::new(Literal::from(pred1(&pname(i), "X")), Condition::empty()),
            DisjunctionElement::new(
                Literal::from(pred1(&pname((i + 1) % k), "X")),
                Condition::empty(),
            ),
        ]))
        .when(body),
        HeadShape::Choice => Head::Choice(Choice::new(
            None,
            [ChoiceElement::new(
                Literal::from(pred1(&pname(i), "X")),
                Condition::empty(),
            )],
            None,
        ))
        .when(body),
        HeadShape::Normal | HeadShape::Disjunctive => Rule::new(pred1(&pname(i), "X"), body),
    }
}

fn build_program(k: usize, rows: &[(HeadShape, Vec<Mode>)]) -> Program {
    program_of(
        rows.iter()
            .enumerate()
            .map(|(i, (shape, modes))| Statement::Rule(build_rule(k, i, *shape, modes))),
    )
}

fn any_head_shape() -> impl Strategy<Value = HeadShape> {
    prop_oneof![
        3 => Just(HeadShape::Normal),
        1 => Just(HeadShape::Disjunctive),
        1 => Just(HeadShape::Choice),
    ]
}

fn any_mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        2 => Just(Mode::Absent),
        2 => Just(Mode::Positive),
        1 => Just(Mode::Negative),
        1 => Just(Mode::Aggregate),
    ]
}

fn any_program() -> impl Strategy<Value = Program> {
    (1usize..5).prop_flat_map(|k| {
        prop::collection::vec((any_head_shape(), prop::collection::vec(any_mode(), k)), k)
            .prop_map(move |rows| build_program(k, &rows))
    })
}

// ---- a generated aggregate-free rule (shared with safe's law suite) ----

#[derive(Clone, Debug)]
enum Elem {
    Positive(usize, Vec<usize>),
    Negative(usize, Vec<usize>),
    Assign(usize, Rhs),
}

#[derive(Clone, Debug)]
enum Rhs {
    Const,
    Var(usize),
    VarPlus(usize),
}

const PREDS: [&str; 3] = ["p", "q", "r"];

fn var_name(i: usize) -> String {
    format!("X{i}")
}

fn pred(text: &str, vars: &[&str]) -> Atom {
    Atom::new(name(text), vars.iter().map(|v| var(v)))
}

// `lhs = rhs` as a body element.
fn assign(lhs: &str, rhs: Term) -> BodyElement {
    BodyElement::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
            var(lhs),
            Relation::Eq,
            rhs,
        ))),
    })
}

fn build_atom(p: usize, idxs: &[usize]) -> Atom {
    let names: Vec<String> = idxs.iter().map(|&i| var_name(i)).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    pred(PREDS[p], &refs)
}

fn build_body(elements: &[Elem]) -> Vec<BodyElement> {
    elements
        .iter()
        .map(|element| match element {
            Elem::Positive(p, idxs) => BodyElement::from(build_atom(*p, idxs)),
            Elem::Negative(p, idxs) => not(build_atom(*p, idxs)),
            Elem::Assign(lhs, rhs) => {
                let right = match rhs {
                    Rhs::Const => num(0),
                    Rhs::Var(j) => var(&var_name(*j)),
                    Rhs::VarPlus(j) => var(&var_name(*j)) + num(1),
                };
                assign(&var_name(*lhs), right)
            }
        })
        .collect()
}

fn any_rule() -> impl Strategy<Value = Rule> {
    let index = 0usize..5;
    let atom_ref = (0usize..3, prop::collection::vec(0usize..5, 0..3));
    let element = prop_oneof![
        atom_ref
            .clone()
            .prop_map(|(p, idxs)| Elem::Positive(p, idxs)),
        atom_ref.prop_map(|(p, idxs)| Elem::Negative(p, idxs)),
        (
            index.clone(),
            prop_oneof![
                Just(Rhs::Const),
                (0usize..5).prop_map(Rhs::Var),
                (0usize..5).prop_map(Rhs::VarPlus),
            ]
        )
            .prop_map(|(lhs, rhs)| Elem::Assign(lhs, rhs)),
    ];
    let head_idxs = prop::collection::vec(0usize..5, 0..3);
    let body = prop::collection::vec(element, 0..6);
    (head_idxs, body)
        .prop_map(|(head_idxs, body)| Rule::new(build_atom(0, &head_idxs), build_body(&body)))
}

// ---- the naive reference grounder (the load-bearing oracle, §10) ----
//
// A bounded, growth-free program over a small finite domain grounds by naive
// substitution, and its ground positive dependency graph is built directly. Every
// atom is unary over the rule's single variable X or a domain constant, so no term
// grows and the Herbrand base is finite — grounding terminates by inspection. Only
// positive body atoms make a positive ground edge, mirroring the analysis's positive
// dependency graph (§4). The verdict laws hold the analysis's predicate-level `Holds`
// honest against this ground truth (§6.2, §10).

const DOMAIN: [i32; 2] = [0, 1];

#[derive(Clone, Copy, Debug)]
enum Arg {
    Var,
    Const(i32),
}

#[derive(Clone, Copy, Debug)]
struct GenAtom {
    predicate: usize,
    arg: Arg,
}

#[derive(Clone, Debug)]
enum GenHead {
    Normal(GenAtom),
    Disjunctive(GenAtom, GenAtom),
}

#[derive(Clone, Debug)]
struct GenLiteral {
    atom: GenAtom,
    negated: bool,
}

#[derive(Clone, Debug)]
struct GenRule {
    head: GenHead,
    body: Vec<GenLiteral>,
}

fn gen_arg_term(arg: Arg) -> Term {
    match arg {
        Arg::Var => var("X"),
        Arg::Const(c) => num(c),
    }
}

fn gen_atom(atom: GenAtom) -> Atom {
    Atom::new(name(&pname(atom.predicate)), [gen_arg_term(atom.arg)])
}

fn gen_literal_element(literal: &GenLiteral) -> BodyElement {
    if literal.negated {
        not(gen_atom(literal.atom))
    } else {
        BodyElement::from(gen_atom(literal.atom))
    }
}

fn gen_rule_statement(rule: &GenRule) -> Statement {
    let body: Vec<BodyElement> = rule.body.iter().map(gen_literal_element).collect();
    match &rule.head {
        GenHead::Normal(atom) => Statement::Rule(Rule::new(gen_atom(*atom), body)),
        GenHead::Disjunctive(a, b) => {
            let head = Head::Disjunction(Disjunction::new([
                DisjunctionElement::new(Literal::from(gen_atom(*a)), Condition::empty()),
                DisjunctionElement::new(Literal::from(gen_atom(*b)), Condition::empty()),
            ]));
            Statement::Rule(head.when(body))
        }
    }
}

fn gen_program(rules: &[GenRule]) -> Program {
    program_of(rules.iter().map(gen_rule_statement))
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct GroundAtom {
    predicate: usize,
    arg: i32,
}

fn ground(atom: GenAtom, x: i32) -> GroundAtom {
    let arg = match atom.arg {
        Arg::Var => x,
        Arg::Const(c) => c,
    };
    GroundAtom {
        predicate: atom.predicate,
        arg,
    }
}

struct GroundGraph {
    positive_edges: BTreeMap<GroundAtom, BTreeSet<GroundAtom>>,
    disjunctive_heads: Vec<(GroundAtom, GroundAtom)>,
}

impl GroundGraph {
    fn of(rules: &[GenRule]) -> GroundGraph {
        let mut positive_edges: BTreeMap<GroundAtom, BTreeSet<GroundAtom>> = BTreeMap::new();
        let mut disjunctive_heads = Vec::new();
        for rule in rules {
            for &x in &DOMAIN {
                let heads: Vec<GroundAtom> = match &rule.head {
                    GenHead::Normal(atom) => vec![ground(*atom, x)],
                    GenHead::Disjunctive(a, b) => {
                        let (ga, gb) = (ground(*a, x), ground(*b, x));
                        disjunctive_heads.push((ga.clone(), gb.clone()));
                        vec![ga, gb]
                    }
                };
                let positive_body: Vec<GroundAtom> = rule
                    .body
                    .iter()
                    .filter(|literal| !literal.negated)
                    .map(|literal| ground(literal.atom, x))
                    .collect();
                for head in &heads {
                    let targets = positive_edges.entry(head.clone()).or_default();
                    for body in &positive_body {
                        targets.insert(body.clone());
                    }
                }
            }
        }
        GroundGraph {
            positive_edges,
            disjunctive_heads,
        }
    }

    fn positive_reachable(&self, start: &GroundAtom) -> BTreeSet<GroundAtom> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<GroundAtom> = self
            .positive_edges
            .get(start)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        while let Some(atom) = stack.pop() {
            if seen.insert(atom.clone()) {
                stack.extend(
                    self.positive_edges
                        .get(&atom)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
        }
        seen
    }

    // Acyclic iff no ground atom reaches itself: any atom on a positive cycle has an
    // out-edge on that cycle, so it is a key here.
    fn is_acyclic(&self) -> bool {
        self.positive_edges
            .keys()
            .all(|atom| !self.positive_reachable(atom).contains(atom))
    }

    // No two atoms of a ground disjunctive head lie in one ground positive cycle —
    // i.e. no two are mutually positive-reachable.
    fn is_head_cycle_free(&self) -> bool {
        self.disjunctive_heads.iter().all(|(a, b)| {
            a == b
                || !(self.positive_reachable(a).contains(b)
                    && self.positive_reachable(b).contains(a))
        })
    }
}

fn any_gen_atom(k: usize) -> impl Strategy<Value = GenAtom> {
    (
        0..k,
        prop_oneof![Just(Arg::Var), Just(Arg::Const(0)), Just(Arg::Const(1))],
    )
        .prop_map(|(predicate, arg)| GenAtom { predicate, arg })
}

fn any_gen_head(k: usize) -> impl Strategy<Value = GenHead> {
    prop_oneof![
        2 => any_gen_atom(k).prop_map(GenHead::Normal),
        1 => (any_gen_atom(k), any_gen_atom(k)).prop_map(|(a, b)| GenHead::Disjunctive(a, b)),
    ]
}

fn any_gen_literal(k: usize) -> impl Strategy<Value = GenLiteral> {
    (any_gen_atom(k), any::<bool>()).prop_map(|(atom, negated)| GenLiteral { atom, negated })
}

fn any_gen_rule(k: usize) -> impl Strategy<Value = GenRule> {
    (
        any_gen_head(k),
        prop::collection::vec(any_gen_literal(k), 0..3),
    )
        .prop_map(|(head, body)| GenRule { head, body })
}

fn any_ground_program() -> impl Strategy<Value = (Program, Vec<GenRule>)> {
    (1usize..4).prop_flat_map(|k| {
        prop::collection::vec(any_gen_rule(k), 0..5).prop_map(|rules| (gen_program(&rules), rules))
    })
}

// ---- The assembly: the facets equal the independent readings ----

proptest! {
    /// The one shared-graph pass is behavior-preserving: each facet equals reading it
    /// independently (§3). This is what lets the facet law suites stand for the whole.
    #[test]
    fn the_facets_equal_the_independent_readings(program in any_program()) {
        let analysis = Analysis::of(&program);
        prop_assert_eq!(analysis.constructs(), &Constructs::of(&program));
        prop_assert_eq!(analysis.dependencies(), &DependencyGraph::of(&program));
        prop_assert_eq!(analysis.safety(), &Safety::of(&program));
        prop_assert_eq!(analysis.classes(), &Classes::of(&program));
    }
}

// ---- The load-bearing soundness law, against the naive reference grounder ----

proptest! {
    /// Whenever the predicate-level `tightness` or `head_cycle_free` verdict is
    /// `Holds`, the ground program — ground by the naive reference grounder over the
    /// bounded domain — has the property (§10). A false `Holds` would be caught by a
    /// ground graph carrying the cycle the predicate level missed.
    #[test]
    fn a_holds_verdict_is_honest_against_the_ground_program((program, rules) in any_ground_program()) {
        let analysis = Analysis::of(&program);
        let ground = GroundGraph::of(&rules);
        if matches!(analysis.classes().tightness(), Verdict::Holds) {
            prop_assert!(
                ground.is_acyclic(),
                "a `Holds` tightness must mean the ground positive graph is acyclic",
            );
        }
        if matches!(analysis.classes().head_cycle_free(), Verdict::Holds) {
            prop_assert!(
                ground.is_head_cycle_free(),
                "a `Holds` head-cycle-freeness must mean no ground disjunctive head shares a ground positive cycle",
            );
        }
    }
}

// ---- The definite verdicts, both directions, against naive references ----

proptest! {
    /// Stratification is `Stratified` iff the graph has no cycle through a `Negative`
    /// or `ThroughAggregate` edge, and its `NotStratified` witness runs through a
    /// non-monotone edge (§5, §6.2).
    #[test]
    fn stratification_agrees_with_the_naive_reference(program in any_program()) {
        let analysis = Analysis::of(&program);
        let reference = DependencyGraph::of(&program);
        prop_assert_eq!(
            matches!(analysis.classes().stratification(), Stratification::Stratified),
            naive_stratified(&reference),
        );
        if let Stratification::NotStratified { cycle } = analysis.classes().stratification() {
            prop_assert!(cycle.is_recursive());
            prop_assert!(
                cycle.has_negative_cycle() || cycle.has_aggregate_cycle(),
                "the witness runs through a non-monotone edge",
            );
        }
    }

    /// Safety flags a rule iff a variable has no binding occurrence, and reports
    /// exactly those variables — against the naive binding fixpoint (§5).
    #[test]
    fn safety_flags_a_rule_iff_a_variable_is_unbound(rule in any_rule()) {
        let program = program_of([Statement::Rule(rule.clone())]);
        let analysis = Analysis::of(&program);
        let flagged: BTreeSet<Variable> = analysis
            .safety()
            .unsafe_rules()
            .next()
            .map(|unsafe_rule| unsafe_rule.unbound().cloned().collect())
            .unwrap_or_default();
        prop_assert_eq!(flagged, naive_unbound(&rule));
    }
}

// ---- The graph and components, against the naive reachability reference ----

proptest! {
    /// The components partition the predicates, agree with the naive reachability
    /// reference, and are in reverse-topological order (§4).
    #[test]
    fn the_components_agree_with_the_naive_reference_and_are_reverse_topological(
        program in any_program(),
    ) {
        let analysis = Analysis::of(&program);
        let reference = DependencyGraph::of(&program);
        prop_assert_eq!(tarjan_partition(analysis.dependencies()), naive_partition(&reference));

        let graph = analysis.dependencies();
        let position: BTreeMap<BTreeSet<Signature>, usize> = graph
            .components()
            .enumerate()
            .map(|(i, component)| (component.members().cloned().collect(), i))
            .collect();
        for u in graph.predicates() {
            let from: BTreeSet<Signature> =
                graph.component_of(u).expect("a node").members().cloned().collect();
            for (_, v) in graph.edges_from(u) {
                let to: BTreeSet<Signature> =
                    graph.component_of(v).expect("a node").members().cloned().collect();
                if from != to {
                    prop_assert!(position[&to] < position[&from]);
                }
            }
        }
    }
}

// ---- The scan is complete, each flag's witness a bearing statement ----

proptest! {
    /// The construct scan equals the independent reading, and every flag's `first`
    /// names a statement that bears the construct (§7).
    #[test]
    fn the_scan_is_complete_with_a_bearing_witness(program in any_program()) {
        let analysis = Analysis::of(&program);
        prop_assert_eq!(analysis.constructs(), &Constructs::of(&program));
        for (construct, witness) in analysis.constructs().all() {
            let witnessed = Constructs::of(&Program::of([witness]));
            prop_assert!(
                witnessed.uses(construct),
                "the witness statement bears the construct it is recorded for",
            );
        }
    }
}

// ---- Totality ----

proptest! {
    /// `Analysis::of` never panics on a generated program, and every facet and
    /// projection reads (§8).
    #[test]
    fn analysis_of_is_total_on_generated_programs(program in any_program()) {
        let analysis = Analysis::of(&program);
        let _predicates = analysis.dependencies().predicates().count();
        let _components = analysis.dependencies().components().count();
        let _constructs = analysis.constructs().all().count();
        let _safe = analysis.safety().is_safe();
        prop_assert!(analysis.classes().confirmed().count() <= 7);
    }

    /// `Analysis::of` never panics on a bounded ground program either (§8).
    #[test]
    fn analysis_of_is_total_on_ground_programs((program, _rules) in any_ground_program()) {
        let analysis = Analysis::of(&program);
        prop_assert!(analysis.classes().confirmed().count() <= 7);
    }
}

// ---- Deterministic cases ----

#[test]
fn the_analysis_assembles_the_four_facets() {
    // p ⇄ q (a positive cycle → not tight) and a rule bearing negation (→ not Horn).
    let program = program_of([rule("p", "q"), rule("q", "p"), rule_not("r", "s")]);
    let analysis = Analysis::of(&program);
    assert_eq!(analysis.constructs(), &Constructs::of(&program));
    assert_eq!(analysis.dependencies(), &DependencyGraph::of(&program));
    assert_eq!(analysis.safety(), &Safety::of(&program));
    assert_eq!(analysis.classes(), &Classes::of(&program));
    assert!(matches!(
        analysis.classes().tightness(),
        Verdict::Unknown { .. }
    ));
}

#[test]
fn a_recursive_program_is_not_proven_tight() {
    // p :- q.  q :- p.  — a positive cycle: the assembled analysis reports `Unknown`.
    let analysis = analysis_of([rule("p", "q"), rule("q", "p")]);
    assert!(matches!(
        analysis.classes().tightness(),
        Verdict::Unknown { .. }
    ));
    assert!(
        !analysis
            .classes()
            .confirmed()
            .any(|class| class == ProgramClass::Tight)
    );
}

#[test]
fn the_reference_grounder_finds_a_ground_cycle() {
    // A self-test of the oracle: p(X) :- q(X).  q(X) :- p(X).  grounds to two ground
    // cycles p(0) ⇄ q(0), p(1) ⇄ q(1), so the ground positive graph is not acyclic;
    // and the analysis cannot prove the predicate level tight.
    let rules = vec![
        GenRule {
            head: GenHead::Normal(GenAtom {
                predicate: 0,
                arg: Arg::Var,
            }),
            body: vec![GenLiteral {
                atom: GenAtom {
                    predicate: 1,
                    arg: Arg::Var,
                },
                negated: false,
            }],
        },
        GenRule {
            head: GenHead::Normal(GenAtom {
                predicate: 1,
                arg: Arg::Var,
            }),
            body: vec![GenLiteral {
                atom: GenAtom {
                    predicate: 0,
                    arg: Arg::Var,
                },
                negated: false,
            }],
        },
    ];
    let ground = GroundGraph::of(&rules);
    assert!(
        !ground.is_acyclic(),
        "the ground positive graph has p(d) ⇄ q(d)"
    );
    let program = gen_program(&rules);
    assert!(matches!(
        Analysis::of(&program).classes().tightness(),
        Verdict::Unknown { .. }
    ));
}
