//! Laws of the program classes (docs/design/analysis.md §6): correctness — the
//! recursion classes read off the dependency graph agree with a naive reference on
//! generated programs (tightness is `Holds` exactly when the positive graph is acyclic,
//! head-cycle-freeness when no two atoms of a disjunctive head share a positive cycle,
//! stratification when no cycle runs through a negative or aggregate edge), each
//! carrying a real positive component or the offending cycle as its witness; the
//! syntactic classes are definite, each carrying the first offending rule, and Horn's
//! negation is **rule-restricted** — a directive's negation is not part of the
//! least-model fragment; the containment `Horn ⟹ Normal ⟹ NonDisjunctive ∧ ChoiceFree ∧
//! HeadCycleFree` holds and `confirmed()` contains exactly the proven classes (an
//! `Unknown` verdict is absent) — and **performance**: a large disjunctive head over a
//! positive cycle is classified without the pairwise blow-up a naive check would incur.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use themelios_analysis::classify::{
    Classes, HornKind, Normality, ProgramClass, Stratification, Verdict,
};
use themelios_analysis::depend::{DependencyGraph, DependencyKind, Signature};
use themelios_program::construct::not;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, DefaultNegation, Disjunction, DisjunctionElement,
    External, FunctionAggregate, Guard, Head, HeadAggregate, HeadAggregateElement, Literal,
    LiteralInner, Program, Relation, Rule, Show, Statement,
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

fn pos(text: &str, arity: u32) -> Signature {
    Signature {
        sign: themelios_program::symbol::Sign::Positive,
        name: name(text),
        arity,
    }
}

fn program_of(statements: impl IntoIterator<Item = Statement>) -> Program {
    Program::of(statements.into_iter().map(WithProvenance::constructed))
}

fn classes_of(statements: impl IntoIterator<Item = Statement>) -> Classes {
    Classes::of(&program_of(statements))
}

// `p :- q.`  and  `p :- not q.`  over nullary predicates.
fn rule(head: &str, body: &str) -> Statement {
    Statement::Rule(Rule::new(atom(head), atom(body)))
}

fn rule_not(head: &str, body: &str) -> Statement {
    Statement::Rule(Rule::new(atom(head), not(atom(body))))
}

fn fact(text: &str) -> Statement {
    Statement::Rule(Rule::fact(atom(text)))
}

// `(h1 ; h2) :- body.`
fn disjunctive(h1: &str, h2: &str, body: &str) -> Statement {
    Statement::Rule(
        Head::Disjunction(Disjunction::new([
            DisjunctionElement::new(Literal::from(atom(h1)), Condition::empty()),
            DisjunctionElement::new(Literal::from(atom(h2)), Condition::empty()),
        ]))
        .when(atom(body)),
    )
}

// `{ h } :- body.`
fn choice(h: &str, body: &str) -> Statement {
    Statement::Rule(
        Head::Choice(Choice::new(
            None,
            [ChoiceElement::new(
                Literal::from(atom(h)),
                Condition::empty(),
            )],
            None,
        ))
        .when(atom(body)),
    )
}

// `#count { X : p(X) } = Y :- r(Y).` — a head aggregate.
fn head_aggregate(body: &str) -> Statement {
    Statement::Rule(
        Head::Aggregate(HeadAggregate::new(
            None,
            AggregateFunction::Count,
            [HeadAggregateElement::new(
                [var("X")],
                Literal::from(pred1("a", "X")),
                Condition::new([Literal::from(pred1("p", "X"))]),
            )],
            Some(Guard {
                relation: Some(Relation::Eq),
                term: var("Y"),
            }),
        ))
        .when(pred1(body, "Y")),
    )
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

fn member_set(component: impl Iterator<Item = Signature>) -> BTreeSet<Signature> {
    component.collect()
}

// The rule the program stores for the sole statement satisfying a predicate — the
// canonicalized witness a class reports (a hand-built rule may not equal it verbatim).
fn find_rule(program: &Program, wanted: impl Fn(&Rule) -> bool) -> Rule {
    program
        .statements()
        .find_map(|statement| match statement.get() {
            Statement::Rule(r) if wanted(r) => Some(r.clone()),
            _ => None,
        })
        .expect("a rule satisfying the predicate")
}

fn is_disjunctive(rule: &Rule) -> bool {
    matches!(rule.head().get(), Head::Disjunction(_))
}

// ---- naive references (the oracles, §6/§10) ----
//
// Each reads the program or the graph's public surface and computes the property the
// obviously-correct way, independent of the class algorithm's representation.

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
// edge — for each such edge `u → v`, whether `v` reaches `u` (a naive reachability
// cycle test over the full graph), independent of the Tarjan components.
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

fn disjunct_signatures(disjunction: &Disjunction) -> Vec<Signature> {
    // Per alternative (§9), matching production's `Atom::signatures`: a residual pooled
    // disjunct is several head atoms, each carrying its own alternative's arity — a first-alternative
    // (or all-terms-flattened) read would misjudge the head cycle.
    let mut signatures = Vec::new();
    for element in disjunction.elements() {
        if let LiteralInner::Atom(atom) = &element.get().literal().inner {
            let atom = atom.get();
            for alternative in atom.alternatives() {
                signatures.push(Signature {
                    sign: atom.sign,
                    name: atom.name.clone(),
                    arity: u32::try_from(alternative.len()).expect("an arity that fits a u32"),
                });
            }
        }
    }
    signatures
}

// Head-cycle-free iff no two atoms of a disjunctive head lie in one recursive positive
// component — a naive pairwise check over each disjunctive head, independent of the
// class algorithm's representative-keyed scan.
fn naive_head_cycle_free(program: &Program) -> bool {
    let positive = DependencyGraph::of(program).positive();
    for statement in program.statements() {
        if let Statement::Rule(rule) = statement.get()
            && let Head::Disjunction(disjunction) = rule.head().get()
        {
            let signatures = disjunct_signatures(disjunction);
            for a in 0..signatures.len() {
                for b in (a + 1)..signatures.len() {
                    if let (Some(ca), Some(cb)) = (
                        positive.component_of(&signatures[a]),
                        positive.component_of(&signatures[b]),
                    ) && ca == cb
                        && ca.is_recursive()
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

// ---- a generated program: k unary predicates, random heads and kinded bodies ----

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
        // A disjunctive head couples two distinct predicates, so it needs `k >= 2`.
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

// ---- Correctness: the recursion classes against a naive reference ----

proptest! {
    /// Tightness is `Holds` exactly when the positive graph is acyclic, and its
    /// `Unknown` witness is a real recursive positive component (§6.2).
    #[test]
    fn tightness_holds_iff_the_positive_graph_is_acyclic(program in any_program()) {
        let classes = Classes::of(&program);
        let positive = DependencyGraph::of(&program).positive();
        let verdict = classes.tightness();
        prop_assert_eq!(matches!(verdict, Verdict::Holds), positive.is_acyclic());
        if let Verdict::Unknown { witness } = verdict {
            prop_assert!(witness.is_recursive());
            let member = witness.members().next().expect("a non-empty component");
            let component = positive.component_of(member).expect("a positive node");
            prop_assert_eq!(&witness, component, "the witness is a positive SCC");
        }
    }

    /// Head-cycle-freeness agrees with the naive pairwise reference, and its `Unknown`
    /// witness is a real recursive positive component (§6.2).
    #[test]
    fn head_cycle_free_agrees_with_the_naive_reference(program in any_program()) {
        let classes = Classes::of(&program);
        let verdict = classes.head_cycle_free();
        prop_assert_eq!(matches!(verdict, Verdict::Holds), naive_head_cycle_free(&program));
        if let Verdict::Unknown { witness } = verdict {
            prop_assert!(witness.is_recursive());
            let positive = DependencyGraph::of(&program).positive();
            let member = witness.members().next().expect("a non-empty component");
            prop_assert_eq!(&witness, positive.component_of(member).expect("a positive node"));
        }
    }

    /// Stratification agrees with the naive cycle-kind reference, and its `NotStratified`
    /// witness is a recursive component running through a non-monotone edge (§6.2).
    #[test]
    fn stratification_agrees_with_the_naive_reference(program in any_program()) {
        let classes = Classes::of(&program);
        let graph = DependencyGraph::of(&program);
        prop_assert_eq!(
            matches!(classes.stratification(), Stratification::Stratified),
            naive_stratified(&graph),
        );
        if let Stratification::NotStratified { cycle } = classes.stratification() {
            prop_assert!(cycle.is_recursive());
            prop_assert!(
                cycle.has_negative_cycle() || cycle.has_aggregate_cycle(),
                "the witness runs through a non-monotone edge",
            );
        }
    }

    /// The containment holds: every `Horn` program is `Normal`, and every `Normal`
    /// program is `NonDisjunctive`, `ChoiceFree`, and `HeadCycleFree` (§6.4).
    #[test]
    fn the_containment_holds_on_generated_programs(program in any_program()) {
        let confirmed: BTreeSet<ProgramClass> = Classes::of(&program).confirmed().collect();
        if confirmed.contains(&ProgramClass::Horn) {
            prop_assert!(confirmed.contains(&ProgramClass::Normal), "Horn ⟹ Normal");
        }
        if confirmed.contains(&ProgramClass::Normal) {
            prop_assert!(confirmed.contains(&ProgramClass::NonDisjunctive), "Normal ⟹ NonDisjunctive");
            prop_assert!(confirmed.contains(&ProgramClass::ChoiceFree), "Normal ⟹ ChoiceFree");
            prop_assert!(confirmed.contains(&ProgramClass::HeadCycleFree), "Normal ⟹ HeadCycleFree");
        }
    }

    /// `confirmed()` contains a class exactly when its method proves it present — an
    /// `Unknown` or negative verdict is absent (the error direction, §6.1/§6.4).
    #[test]
    fn confirmed_contains_exactly_the_proven_classes(program in any_program()) {
        let classes = Classes::of(&program);
        let confirmed: BTreeSet<ProgramClass> = classes.confirmed().collect();
        prop_assert_eq!(confirmed.contains(&ProgramClass::Tight), matches!(classes.tightness(), Verdict::Holds));
        prop_assert_eq!(confirmed.contains(&ProgramClass::HeadCycleFree), matches!(classes.head_cycle_free(), Verdict::Holds));
        prop_assert_eq!(confirmed.contains(&ProgramClass::Stratified), matches!(classes.stratification(), Stratification::Stratified));
        prop_assert_eq!(confirmed.contains(&ProgramClass::Normal), matches!(classes.normality(), Normality::Normal));
        prop_assert_eq!(confirmed.contains(&ProgramClass::Horn), matches!(classes.horn(), HornKind::Horn));
        prop_assert_eq!(confirmed.contains(&ProgramClass::NonDisjunctive), !classes.uses_disjunction());
        prop_assert_eq!(confirmed.contains(&ProgramClass::ChoiceFree), !classes.uses_choice());
    }
}

// ---- Correctness: the recursion classes, case by case ----

#[test]
fn tightness_is_a_sound_predicate_level_verdict() {
    // b :- a.  c :- b.  — a positive DAG is tight.
    assert_eq!(
        classes_of([rule("b", "a"), rule("c", "b")]).tightness(),
        Verdict::Holds
    );

    // a :- b.  b :- a.  — a positive cycle is not proven tight; the witness is {a, b}.
    match classes_of([rule("a", "b"), rule("b", "a")]).tightness() {
        Verdict::Unknown { witness } => {
            assert!(witness.is_recursive());
            assert_eq!(
                member_set(witness.members().cloned()),
                [pos("a", 0), pos("b", 0)].into_iter().collect(),
            );
        }
        Verdict::Holds => panic!("a positive cycle is not proven tight"),
    }

    // a :- not b.  b :- not a.  — a negative cycle leaves the positive graph acyclic, so tight.
    assert_eq!(
        classes_of([rule_not("a", "b"), rule_not("b", "a")]).tightness(),
        Verdict::Holds,
        "a negative cycle has no positive recursion",
    );
}

#[test]
fn a_pooled_disjunct_head_couples_per_alternative() {
    // `p(X; Y) | s :- p(X).` — a residual pooled disjunct whose two alternatives are both p/1, in a
    // rule that makes p/1 recursive (the head p/1 depends on the body p/1). The pool's alternatives
    // are two p/1 head atoms of the one rule, so they close a head cycle in the recursive p/1
    // component — NOT head-cycle-free. Reading only the FIRST alternative's signature would register
    // p/1 once and miss the coupling (a false head-cycle-free); the per-alternative read
    // (`Atom::signatures`, §9) sees both. A pooled disjunct is a conjunctive head group, so
    // treating its alternatives as coupled head atoms is the conservative-safe reading — never a false
    // `Holds`, at worst a spurious `Unknown`.
    let pooled = Atom::pooled(name("p"), [vec![var("X")], vec![var("Y")]]).expect("non-empty pool");
    let rule = Statement::Rule(
        Head::Disjunction(Disjunction::new([
            DisjunctionElement::new(Literal::from(pooled), Condition::empty()),
            DisjunctionElement::new(Literal::from(atom("s")), Condition::empty()),
        ]))
        .when(pred1("p", "X")),
    );
    match classes_of([rule]).head_cycle_free() {
        Verdict::Unknown { witness } => {
            assert!(witness.is_recursive());
            assert_eq!(
                member_set(witness.members().cloned()),
                [pos("p", 1)].into_iter().collect(),
            );
        }
        Verdict::Holds => {
            panic!("two p/1 head atoms from a pooled disjunct in one positive cycle is not hcf")
        }
    }
}

#[test]
fn head_cycle_free_is_a_sound_predicate_level_verdict() {
    // (a ; b) :- c.  — a disjunctive head with no positive cycle among a, b.
    assert_eq!(
        classes_of([disjunctive("a", "b", "c")]).head_cycle_free(),
        Verdict::Holds,
    );

    // (a ; b) :- c.   a :- b.   b :- a.  — a and b share a positive cycle and one head.
    match classes_of([disjunctive("a", "b", "c"), rule("a", "b"), rule("b", "a")]).head_cycle_free()
    {
        Verdict::Unknown { witness } => {
            assert!(witness.is_recursive());
            assert_eq!(
                member_set(witness.members().cloned()),
                [pos("a", 0), pos("b", 0)].into_iter().collect(),
            );
        }
        Verdict::Holds => panic!("two head atoms in one positive cycle is not head-cycle-free"),
    }

    // a :- b.  b :- a.  — a positive cycle with no disjunctive head is vacuously head-cycle-free.
    assert_eq!(
        classes_of([rule("a", "b"), rule("b", "a")]).head_cycle_free(),
        Verdict::Holds,
        "no disjunctive head ⟹ head-cycle-free",
    );

    // (a ; X < 1) :- b.  — a non-atom disjunct has no predicate signature, so it is
    // skipped soundly (§8, no panic on any constructible program); one head atom cannot
    // couple, so head-cycle-freeness holds.
    let mixed = Head::Disjunction(Disjunction::new([
        DisjunctionElement::new(Literal::from(atom("a")), Condition::empty()),
        DisjunctionElement::new(
            Literal {
                negation: DefaultNegation::None,
                inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
                    var("X"),
                    Relation::Lt,
                    num(1),
                ))),
            },
            Condition::empty(),
        ),
    ]))
    .when(atom("b"));
    assert_eq!(
        classes_of([Statement::Rule(mixed)]).head_cycle_free(),
        Verdict::Holds,
        "a non-atom disjunct is skipped; one head atom cannot couple",
    );
}

#[test]
fn stratification_is_definite_against_the_cycle_kinds() {
    // a :- not b.  b :- not a.  — a negative cycle is not stratified.
    match classes_of([rule_not("a", "b"), rule_not("b", "a")]).stratification() {
        Stratification::NotStratified { cycle } => {
            assert!(cycle.has_negative_cycle());
            assert_eq!(
                member_set(cycle.members().cloned()),
                [pos("a", 0), pos("b", 0)].into_iter().collect(),
            );
        }
        Stratification::Stratified => panic!("a negative cycle is not stratified"),
    }

    // p :- #count { X : q(X) } >= 1.   q(X) :- p.  — a recursive aggregate is not stratified.
    let aggregate = classes_of([
        Statement::Rule(Rule::new(
            atom("p"),
            vec![count_over(DefaultNegation::None, "q")],
        )),
        Statement::Rule(Rule::new(pred1("q", "X"), atom("p"))),
    ]);
    match aggregate.stratification() {
        Stratification::NotStratified { cycle } => assert!(cycle.has_aggregate_cycle()),
        Stratification::Stratified => {
            panic!("a recursive non-monotone aggregate is not stratified")
        }
    }

    // a :- not b.  b.  — negation without a cycle is stratified.
    assert_eq!(
        classes_of([rule_not("a", "b"), fact("b")]).stratification(),
        &Stratification::Stratified,
    );

    // a :- b.  b :- a.  — a positive cycle is stratified (tightness, not stratification, flags it).
    assert_eq!(
        classes_of([rule("a", "b"), rule("b", "a")]).stratification(),
        &Stratification::Stratified,
    );
}

// ---- Correctness: the syntactic classes, definite with a witnessing rule ----

#[test]
fn normality_is_definite_with_the_first_non_normal_rule() {
    // (a ; b) :- c.  — a disjunctive head is not normal, witnessed by that rule.
    let program = program_of([disjunctive("a", "b", "c")]);
    let witness = find_rule(&program, is_disjunctive);
    assert_eq!(
        Classes::of(&program).normality(),
        Normality::NotNormal { rule: witness }
    );

    // { a } :- c.  — a choice head is not normal.
    assert!(matches!(
        classes_of([choice("a", "c")]).normality(),
        Normality::NotNormal { .. }
    ));

    // #count { X : p(X) } = Y :- r(Y).  — a head aggregate is not normal.
    assert!(matches!(
        classes_of([head_aggregate("r")]).normality(),
        Normality::NotNormal { .. }
    ));

    // a :- b.  b.  — every head a single literal: normal.
    assert_eq!(
        classes_of([rule("a", "b"), fact("b")]).normality(),
        Normality::Normal
    );

    // The witness is the FIRST non-normal rule in program order.
    let program = program_of([
        rule("x", "y"),
        disjunctive("a", "b", "c"),
        disjunctive("d", "e", "f"),
    ]);
    let first = find_rule(&program, is_disjunctive);
    assert_eq!(
        Classes::of(&program).normality(),
        Normality::NotNormal { rule: first }
    );
}

#[test]
fn horn_is_definite_and_broken_by_a_rule_negation() {
    // (a ; b) :- c.  — a disjunction breaks Horn, witnessed by that rule.
    let program = program_of([disjunctive("a", "b", "c")]);
    let witness = find_rule(&program, is_disjunctive);
    assert_eq!(
        Classes::of(&program).horn(),
        HornKind::NotHorn { reason: witness }
    );

    // a :- not b.  — default negation in a rule breaks Horn, though the rule is normal.
    let program = program_of([rule_not("a", "b")]);
    let negated = find_rule(&program, |_| true);
    assert_eq!(Classes::of(&program).normality(), Normality::Normal);
    assert_eq!(
        Classes::of(&program).horn(),
        HornKind::NotHorn { reason: negated }
    );

    // -a :- b.  — strong negation ALONE breaks Horn (the rule stays normal).
    let program = program_of([Statement::Rule(Rule::new(-atom("a"), atom("b")))]);
    let strong = find_rule(&program, |_| true);
    assert_eq!(
        Classes::of(&program).normality(),
        Normality::Normal,
        "strong negation is still normal"
    );
    assert_eq!(
        Classes::of(&program).horn(),
        HornKind::NotHorn { reason: strong },
        "strong negation alone breaks Horn",
    );

    // a :- b.  b.  — a definite program is Horn.
    assert_eq!(
        classes_of([rule("a", "b"), fact("b")]).horn(),
        HornKind::Horn
    );
}

#[test]
fn horn_negation_is_rule_restricted_to_the_derivation_rules() {
    // p :- q.  q.  #external -p.  — strong negation in a DIRECTIVE does not break Horn:
    // it is not part of the least-model fragment (the rule-restricted reading, §6.3).
    let external = Statement::External(External::new(-atom("p"), Body::empty(), None));
    assert_eq!(
        classes_of([rule("p", "q"), fact("q"), external]).horn(),
        HornKind::Horn,
        "a directive's strong negation does not break Horn",
    );

    // p :- q.  q.  #show 1 : not p.  — default negation in a directive does not break Horn.
    let show = Statement::Show(Show::term_body(num(1), Body::new([not(atom("p"))])));
    assert_eq!(
        classes_of([rule("p", "q"), fact("q"), show]).horn(),
        HornKind::Horn,
        "a directive's default negation does not break Horn",
    );
}

#[test]
fn uses_disjunction_and_uses_choice_report_the_head_extensions() {
    let plain = classes_of([rule("a", "b")]);
    assert!(!plain.uses_disjunction());
    assert!(!plain.uses_choice());

    let disjunction = classes_of([disjunctive("a", "b", "c")]);
    assert!(disjunction.uses_disjunction());
    assert!(!disjunction.uses_choice());

    let choice = classes_of([choice("a", "c")]);
    assert!(!choice.uses_disjunction());
    assert!(choice.uses_choice());
}

// ---- Correctness: the routable projection ----

#[test]
fn confirmed_projects_the_proven_classes_and_omits_the_unproven() {
    // a :- b.  b :- a.  — a positive cycle: NOT tight, but normal, Horn, stratified,
    // head-cycle-free, non-disjunctive, and choice-free.
    let confirmed: BTreeSet<ProgramClass> = classes_of([rule("a", "b"), rule("b", "a")])
        .confirmed()
        .collect();
    assert!(
        !confirmed.contains(&ProgramClass::Tight),
        "an Unknown tight is absent"
    );
    assert_eq!(
        confirmed,
        [
            ProgramClass::HeadCycleFree,
            ProgramClass::Stratified,
            ProgramClass::Normal,
            ProgramClass::Horn,
            ProgramClass::NonDisjunctive,
            ProgramClass::ChoiceFree,
        ]
        .into_iter()
        .collect(),
    );

    // a :- b.  b.  — a tight definite program is in every class.
    let confirmed: BTreeSet<ProgramClass> = classes_of([rule("a", "b"), fact("b")])
        .confirmed()
        .collect();
    assert!(confirmed.contains(&ProgramClass::Tight));
    assert!(confirmed.contains(&ProgramClass::Horn));

    // The empty program is trivially in every class (no rule violates any).
    let empty: BTreeSet<ProgramClass> = Classes::of(&program_of([])).confirmed().collect();
    assert!(empty.contains(&ProgramClass::Tight));
    assert!(empty.contains(&ProgramClass::Horn));
    assert!(empty.contains(&ProgramClass::Stratified));
}

// ---- Performance: head-cycle-freeness does not blow up on a large disjunctive head ----

#[test]
fn a_large_disjunctive_head_over_a_positive_cycle_is_classified_without_going_quadratic() {
    // (p0 ; p1 ; … ; p_{N-1}) :- q.   plus the positive cycle p0 → p1 → … → p0.  Every
    // head atom lies in one positive component, so head-cycle-freeness is `Unknown`; the
    // representative-keyed scan finds it in `O(head atoms · log)`, where a pairwise check
    // over the head would be `O(N²)` and not finish.
    const N: usize = 50_000;
    let elements =
        (0..N).map(|i| DisjunctionElement::new(Literal::from(atom(&pname(i))), Condition::empty()));
    let mut statements = vec![Statement::Rule(
        Head::Disjunction(Disjunction::new(elements)).when(atom("q")),
    )];
    for i in 0..N {
        statements.push(Statement::Rule(Rule::new(
            atom(&pname(i)),
            atom(&pname((i + 1) % N)),
        )));
    }
    assert!(
        matches!(
            classes_of(statements).head_cycle_free(),
            Verdict::Unknown { .. }
        ),
        "the disjunctive head's atoms share the positive cycle",
    );
}
