//! Laws of safety and grounding finiteness (docs/design/analysis.md §5): safety is
//! definite (a rule is flagged **iff** a variable has no binding occurrence — checked
//! against a naive reference on generated rules, and case by case: a negative literal
//! does not bind, a positive one does, an assignment `X = t` binds `X` when `t`'s
//! variables are bound (the fixpoint), and an aggregate-local binding does not leak to
//! a rule-global variable); the witness is the offending rule and exactly its unbound
//! variables; finiteness is a sound approximation (`Holds` for a non-growing program,
//! `Unknown` with the growing component for `p(f(X)) :- p(X)`, never a third arm, and
//! read from structure not provenance). **Performance:** the binding fixpoint stays
//! linear on a long assignment chain (a naive re-scan would go quadratic).

use std::collections::BTreeSet;

use proptest::prelude::*;
use themelios_analysis::classify::Verdict;
use themelios_analysis::safe::Safety;
use themelios_program::construct::not;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, BodyAggregateElement, BodyElement, Choice, ChoiceElement,
    Comparison, Condition, ConditionalLiteral, DefaultNegation, Disjunction, DisjunctionElement,
    FunctionAggregate, Guard, Head, HeadAggregate, HeadAggregateElement, Literal, LiteralInner,
    Program, Relation, Rule, SetAggregate, SetElement, Statement, TheoryAtom, TheoryElement,
    TheoryTerm,
};
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{Term, Variable};

// ---- helpers ----

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

fn named(text: &str) -> Variable {
    Variable::Named(VarName::new(text).expect("a valid variable"))
}

fn var(text: &str) -> Term {
    Term::Variable(named(text))
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
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

fn safety_of(statements: impl IntoIterator<Item = Statement>) -> Safety {
    Safety::of(&Program::of(
        statements.into_iter().map(WithProvenance::constructed),
    ))
}

fn unbound_of(rule: Rule) -> BTreeSet<Variable> {
    let safety = safety_of([Statement::Rule(rule)]);
    let mut unsafe_rules = safety.unsafe_rules();
    unsafe_rules
        .next()
        .map(|r| r.unbound().cloned().collect())
        .unwrap_or_default()
}

// ---- the naive safety reference (the oracle, §5/§10) ----
//
// The obviously-correct reading for an aggregate-free rule: a named variable is safe
// iff it is in the least binding set — the variables of positive body atoms, closed
// under assignments `X = t` (a brute-force fixpoint). Unbound = rule variables minus
// the binding set.

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

// A generated aggregate-free rule over variables X0..X4 and predicates p/q/r.
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

// ---- Correctness: safety cases ----

#[test]
fn safety_flags_a_rule_iff_a_variable_has_no_binding_occurrence() {
    // :- not p(X).  → X unsafe (a negative literal does not bind).
    assert_eq!(
        unbound_of(Rule::constraint(not(pred("p", &["X"])))),
        [named("X")].into_iter().collect(),
    );

    // q(X) :- p(X).  → X safe (a positive literal binds).
    assert!(unbound_of(Rule::new(pred("q", &["X"]), pred("p", &["X"]))).is_empty());

    // q(X) :- X = Y + 1, p(Y).  → X and Y safe (the assignment fixpoint over p(Y)).
    assert!(
        unbound_of(Rule::new(
            pred("q", &["X"]),
            vec![
                assign("X", var("Y") + num(1)),
                BodyElement::from(pred("p", &["Y"]))
            ],
        ))
        .is_empty(),
    );

    // q(X) :- X = Y + 1.  → X and Y unsafe (Y is unbound, so the assignment cannot fire).
    assert_eq!(
        unbound_of(Rule::new(pred("q", &["X"]), assign("X", var("Y") + num(1)))),
        [named("X"), named("Y")].into_iter().collect(),
    );
}

#[test]
fn an_aggregate_local_binding_does_not_leak_to_a_rule_global_variable() {
    // q(Y) :- #count { X : p(X) } >= 1.  → Y unsafe (global, unbound); X is bound
    // *within* the aggregate element and does not leak to bind Y.
    let aggregate = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            None,
            AggregateFunction::Count,
            [BodyAggregateElement::new(
                [var("X")],
                Condition::new([Literal::from(pred("p", &["X"]))]),
            )],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: num(1),
            }),
        )),
    };
    let unbound = unbound_of(Rule::new(pred("q", &["Y"]), vec![aggregate]));
    assert_eq!(
        unbound,
        [named("Y")].into_iter().collect(),
        "the global Y is unbound; the aggregate-local X does not leak",
    );
}

proptest! {
    /// Safety agrees with the naive brute-force-fixpoint reference on generated
    /// aggregate-free rules — the load-bearing binding fixpoint (§5, §10).
    #[test]
    fn safety_agrees_with_the_naive_reference(rule in any_rule()) {
        let expected = naive_unbound(&rule);
        let actual = unbound_of(rule.clone());
        prop_assert_eq!(actual, expected);
    }
}

// ---- Correctness: finiteness ----

fn finiteness_of(statements: impl IntoIterator<Item = Statement>) -> Verdict {
    safety_of(statements).finiteness().clone()
}

// `p(f(X)) :- p(X).` — the term-growing recursive rule.
fn growing_rule() -> Statement {
    let deeper = Term::Function {
        name: name("f"),
        arguments: vec![var("X")],
    };
    Statement::Rule(Rule::new(Atom::new(name("p"), [deeper]), pred("p", &["X"])))
}

#[test]
fn finiteness_is_a_sound_approximation() {
    // A non-recursive program grounds finitely.
    let holds = finiteness_of([Statement::Rule(Rule::new(
        pred("p", &["X"]),
        pred("q", &["X"]),
    ))]);
    assert_eq!(holds, Verdict::Holds, "a non-recursive program is finite");

    // A non-growing recursive program (Datalog transitive closure) grounds finitely.
    let datalog = finiteness_of([
        Statement::Rule(Rule::new(
            pred("reach", &["X", "Y"]),
            pred("edge", &["X", "Y"]),
        )),
        Statement::Rule(Rule::new(
            pred("reach", &["X", "Z"]),
            vec![
                BodyElement::from(pred("reach", &["X", "Y"])),
                BodyElement::from(pred("edge", &["Y", "Z"])),
            ],
        )),
    ]);
    assert_eq!(datalog, Verdict::Holds, "a non-growing recursion is finite");

    // `p(f(X)) :- p(X).` grows — Unknown carrying the recursive component of p.
    let growing = finiteness_of([growing_rule()]);
    match growing {
        Verdict::Unknown { witness } => {
            assert!(
                witness.members().any(|s| s.name.as_str() == "p"),
                "the witness is the recursive component of p",
            );
            assert!(witness.is_recursive());
        }
        Verdict::Holds => panic!("a term-growing recursion is not proven finite"),
    }
}

#[test]
fn finiteness_reads_structure_not_provenance() {
    let constructed = Program::of([WithProvenance::constructed(growing_rule())]);
    let tagged = Program::of([WithProvenance::new(
        growing_rule(),
        Provenance::from(Origin::Transformed(TransformTag::new("t"))),
    )]);
    assert_eq!(
        Safety::of(&constructed).finiteness(),
        Safety::of(&tagged).finiteness(),
        "finiteness reads structure, not provenance",
    );
}

// ---- Finiteness: growth carried by a body `=`-assignment (reread-2, the re-derivation P1–P7) ----
//
// The direct former `q(f(Y)) :- q(Y)` (P1) is `growing_rule` above. These pin the growth that
// reaches the head through a body `=`-assignment — the false-`Holds` hole reread-2 found — and the
// aliasing that must NOT be read as growth.

fn function(name_text: &str, arg: &str) -> Term {
    Term::Function {
        name: name(name_text),
        arguments: vec![var(arg)],
    }
}

#[test]
fn finiteness_flags_equality_assigned_growth() {
    // P3: q(X) :- q(Y), X = f(Y).  The deepening is in the body `=`; the head carries X *bare*.
    // Grounds infinitely (q(0), q(f(0)), q(f(f(0))), …) — must be Unknown, not a false Holds.
    let rule = Rule::new(
        pred("q", &["X"]),
        vec![
            BodyElement::from(pred("q", &["Y"])),
            assign("X", function("f", "Y")),
        ],
    );
    match finiteness_of([Statement::Rule(rule)]) {
        Verdict::Unknown { witness } => assert!(
            witness.members().any(|s| s.name.as_str() == "q"),
            "the growing component is q",
        ),
        Verdict::Holds => panic!("`=`-assignment-carried growth (P3) must be Unknown, not Holds"),
    }
}

#[test]
fn finiteness_flags_an_assignment_deepening_chain() {
    // P4: q(X0) :- q(X3), X0 = f(X1), X1 = f(X2), X2 = f(X3).  The deepening reaches the bare head
    // X0 transitively along a chain of assignments.
    let rule = Rule::new(
        pred("q", &["X0"]),
        vec![
            BodyElement::from(pred("q", &["X3"])),
            assign("X0", function("f", "X1")),
            assign("X1", function("f", "X2")),
            assign("X2", function("f", "X3")),
        ],
    );
    assert!(
        matches!(
            finiteness_of([Statement::Rule(rule)]),
            Verdict::Unknown { .. }
        ),
        "a chain of `=`-deepenings to a bare head (P4) must be Unknown",
    );
}

#[test]
fn finiteness_flags_arithmetic_successor_growth() {
    // P7: q(X) :- q(Y), X = Y + 1.  The integer successor is the same act as the term successor —
    // a Church numeral — and is flagged uniformly with `q(Y+1) :- q(Y)`.
    let rule = Rule::new(
        pred("q", &["X"]),
        vec![
            BodyElement::from(pred("q", &["Y"])),
            assign("X", var("Y") + num(1)),
        ],
    );
    assert!(
        matches!(
            finiteness_of([Statement::Rule(rule)]),
            Verdict::Unknown { .. }
        ),
        "arithmetic successor growth (P7) must be Unknown",
    );
}

#[test]
fn finiteness_reads_equality_aliasing_precisely() {
    // A bare `=`-aliased variable does NOT grow: p(X) :- p(Y), X = Y is the tautology p(Y) :- p(Y).
    let bare = Rule::new(
        pred("p", &["X"]),
        vec![BodyElement::from(pred("p", &["Y"])), assign("X", var("Y"))],
    );
    assert_eq!(
        finiteness_of([Statement::Rule(bare)]),
        Verdict::Holds,
        "a bare `=`-aliased variable does not grow (P2 alias): Holds",
    );

    // But an alias *under a head former* does grow: q(f(X)) :- q(Y), X = Y is q(f(Y)) :- q(Y).
    let formed = Rule::new(
        Atom::new(name("q"), [function("f", "X")]),
        vec![BodyElement::from(pred("q", &["Y"])), assign("X", var("Y"))],
    );
    assert!(
        matches!(
            finiteness_of([Statement::Rule(formed)]),
            Verdict::Unknown { .. }
        ),
        "an alias under a head former grows (P2): Unknown",
    );
}

fn eq_literal(left: Term, right: Term) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
            left,
            Relation::Eq,
            right,
        ))),
    }
}

#[test]
fn finiteness_reads_the_reversed_assignment() {
    // f(Y) = X is the same deepening as X = f(Y) (P3), with the former on the left.
    let rule = Rule::new(
        pred("q", &["X"]),
        vec![
            BodyElement::from(pred("q", &["Y"])),
            BodyElement::Literal(eq_literal(function("f", "Y"), var("X"))),
        ],
    );
    assert!(
        matches!(
            finiteness_of([Statement::Rule(rule)]),
            Verdict::Unknown { .. }
        ),
        "`f(Y) = X` deepens like `X = f(Y)`",
    );
}

#[test]
fn finiteness_terminates_on_cyclic_deepening() {
    // q(X) :- q(Y), X = f(Y), Y = f(X).  — mutually deepening; the closure's visited-set must
    // terminate (no hang), and X deepening carried Y makes it Unknown.
    let rule = Rule::new(
        pred("q", &["X"]),
        vec![
            BodyElement::from(pred("q", &["Y"])),
            assign("X", function("f", "Y")),
            assign("Y", function("f", "X")),
        ],
    );
    assert!(
        matches!(
            finiteness_of([Statement::Rule(rule)]),
            Verdict::Unknown { .. }
        ),
        "cyclic deepening terminates as Unknown",
    );
}

#[test]
fn safety_binds_an_assignment_local_to_an_aggregate_element() {
    // p :- #count { : q(W), X = f(W) } >= 1.  — the aggregate element is a local scope carrying a
    // local `=`-assignment X = f(W); the fixpoint closes it over the (empty) global base. Exercises
    // the assignment worklist of the scoped closure, not just the empty-scope path.
    let element = BodyAggregateElement::new(
        Vec::<Term>::new(),
        Condition::new([
            Literal::from(pred("q", &["W"])),
            eq_literal(var("X"), function("f", "W")),
        ]),
    );
    let aggregate = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            None,
            AggregateFunction::Count,
            [element],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: num(1),
            }),
        )),
    };
    let safety = safety_of([Statement::Rule(Rule::new(
        Atom::constant(name("p")),
        vec![aggregate],
    ))]);
    assert!(
        safety.is_safe(),
        "a local `=`-assignment binds its variable within the element scope",
    );
}

// ---- Performance: the binding fixpoint stays linear ----

#[test]
fn a_long_assignment_chain_is_analyzed_without_going_quadratic() {
    // p(X_N) :- X0 = 0, X1 = X0 + 1, …, X_N = X_{N-1} + 1.  — a chain of N assignments
    // where each binds after the previous. A naive re-scan fixpoint is O(N²); the
    // worklist is O(N), so this finishes fast.
    const N: usize = 50_000;
    let mut body = vec![assign("X0", num(0))];
    for i in 1..=N {
        body.push(assign(&var_name(i), var(&var_name(i - 1)) + num(1)));
    }
    let rule = Rule::new(pred("p", &[&var_name(N)]), body);
    let safety = safety_of([Statement::Rule(rule)]);
    assert!(
        safety.is_safe(),
        "the assignment chain binds every variable, so the rule is safe",
    );
}

// ---- Breadth: every head and element form is scoped ----

#[test]
fn safety_scopes_every_head_and_element_form() {
    // p(X) | q(X) :- r(X).  — a disjunctive head; X bound by r(X).
    let disjunction = Head::Disjunction(Disjunction::new([
        DisjunctionElement::new(Literal::from(pred("p", &["X"])), Condition::empty()),
        DisjunctionElement::new(Literal::from(pred("q", &["X"])), Condition::empty()),
    ]));
    assert!(unbound_of(disjunction.when(pred("r", &["X"]))).is_empty());

    // { p(X) } :- r(X).  — a choice head.
    let choice = Head::Choice(Choice::new(
        None,
        [ChoiceElement::new(
            Literal::from(pred("p", &["X"])),
            Condition::empty(),
        )],
        None,
    ));
    assert!(unbound_of(choice.when(pred("r", &["X"]))).is_empty());

    // #count { X : p(X) } = Y :- r(Y).  — a head aggregate; X local (bound by p(X)), Y global.
    let head_aggregate = Head::Aggregate(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [var("X")],
            Literal::from(pred("a", &["X"])),
            Condition::new([Literal::from(pred("p", &["X"]))]),
        )],
        Some(Guard {
            relation: Some(Relation::Eq),
            term: var("Y"),
        }),
    ));
    assert!(unbound_of(head_aggregate.when(pred("r", &["Y"]))).is_empty());

    // s :- p(X) : r(X).  — a body conditional literal; X local, bound by r(X).
    let conditional = BodyElement::Conditional(ConditionalLiteral {
        literal: Literal::from(pred("p", &["X"])),
        condition: Condition::new([Literal::from(pred("r", &["X"]))]),
    });
    assert!(unbound_of(Rule::new(pred("s", &[]), vec![conditional])).is_empty());

    // :- { p(X); q(Y) : r(Y) } >= 1.  — a set aggregate, bare and conditional elements.
    let set = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Set(SetAggregate::new(
            None,
            [
                SetElement::Literal(Literal::from(pred("p", &["X"]))),
                SetElement::ConditionalLiteral(ConditionalLiteral {
                    literal: Literal::from(pred("q", &["Y"])),
                    condition: Condition::new([Literal::from(pred("r", &["Y"]))]),
                }),
            ],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: num(1),
            }),
        )),
    };
    assert!(unbound_of(Rule::constraint(vec![set])).is_empty());

    // :- &diff(Y) { X : p(X) }, q(Y).  — a theory atom with an ordinary argument Y
    // (global, bound by q(Y)); its element condition binds the ordinary X.
    let theory = BodyElement::TheoryAtom {
        negation: DefaultNegation::None,
        atom: TheoryAtom::new(
            name("diff"),
            [var("Y")],
            [TheoryElement::new(
                [TheoryTerm::Variable(named("X"))],
                Some(Condition::new([Literal::from(pred("p", &["X"]))])),
            )],
            None,
        ),
    };
    assert!(
        unbound_of(Rule::constraint(vec![
            theory,
            BodyElement::from(pred("q", &["Y"])),
        ]))
        .is_empty()
    );

    // s :- #true.  — a boolean literal binds nothing and is safe.
    let boolean = Rule::new(
        pred("s", &[]),
        Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::True,
        },
    );
    assert!(unbound_of(boolean).is_empty());

    // p(X) | q(Y) :- r(X).  — a disjunctive head with Y unbound.
    let mixed = Head::Disjunction(Disjunction::new([
        DisjunctionElement::new(Literal::from(pred("p", &["X"])), Condition::empty()),
        DisjunctionElement::new(Literal::from(pred("q", &["Y"])), Condition::empty()),
    ]));
    assert_eq!(
        unbound_of(mixed.when(pred("r", &["X"]))),
        [named("Y")].into_iter().collect(),
    );
}

#[test]
fn safety_flags_an_unbound_variable_in_each_local_and_comparison_form() {
    // The mirror of `safety_scopes_every_head_and_element_form`: the same local and
    // comparison forms with an *unbound* variable, so each binding path is pinned on
    // both sides — a dropped element form or a widened assignment guard would let one
    // of these wrongly read safe.

    // :- #count { X : q(Y) } >= 1.  — X is in the element term but the condition q(Y)
    // binds Y, not X; X is purely local and unbound, distinguishing it from a variable
    // shared with the rule global.
    let aggregate = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            None,
            AggregateFunction::Count,
            [BodyAggregateElement::new(
                [var("X")],
                Condition::new([Literal::from(pred("q", &["Y"]))]),
            )],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: num(1),
            }),
        )),
    };
    assert!(
        unbound_of(Rule::constraint(vec![aggregate])).contains(&named("X")),
        "a purely-local unbound aggregate variable is flagged",
    );

    // s :- p(X) : r(Y).  — the conditional's literal p(X) needs X, but its condition
    // r(Y) binds Y, not X: X is unbound and flagged.
    let conditional = BodyElement::Conditional(ConditionalLiteral {
        literal: Literal::from(pred("p", &["X"])),
        condition: Condition::new([Literal::from(pred("r", &["Y"]))]),
    });
    assert!(
        unbound_of(Rule::new(pred("s", &[]), vec![conditional])).contains(&named("X")),
        "an unbound variable in a body conditional literal is flagged",
    );

    // p(X) :- X < 5.  — a single NON-equality comparison is not an assignment, so it
    // binds nothing; X is unbound (only `X = t` binds, §5).
    let comparison = BodyElement::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
            var("X"),
            Relation::Lt,
            num(5),
        ))),
    });
    assert!(
        unbound_of(Rule::new(pred("p", &["X"]), vec![comparison])).contains(&named("X")),
        "a variable constrained only by a non-equality comparison is unbound",
    );

    // :- &diff(Y) { X : p(X) }.  — the theory atom's ordinary argument Y has no binding
    // occurrence (the element condition p(X) binds only the theory-local X): Y is flagged.
    let theory = BodyElement::TheoryAtom {
        negation: DefaultNegation::None,
        atom: TheoryAtom::new(
            name("diff"),
            [var("Y")],
            [TheoryElement::new(
                [TheoryTerm::Variable(named("X"))],
                Some(Condition::new([Literal::from(pred("p", &["X"]))])),
            )],
            None,
        ),
    };
    assert!(
        unbound_of(Rule::constraint(vec![theory])).contains(&named("Y")),
        "an unbound ordinary argument of a theory atom is flagged",
    );
}

// ---- Breadth: finiteness through every term-former ----

fn functional(name_text: &str, argument: Term) -> Atom {
    Atom::new(
        name("p"),
        [Term::Function {
            name: name(name_text),
            arguments: vec![argument],
        }],
    )
}

#[test]
fn finiteness_flags_growth_through_the_term_formers() {
    // p((X, Y)) :- p(X), edge(Y).  — a tuple deepens the recursive X.
    let tuple = Atom::new(name("p"), [Term::Tuple(vec![var("X"), var("Y")])]);
    let grows_tuple = finiteness_of([Statement::Rule(Rule::new(
        tuple,
        vec![
            BodyElement::from(pred("p", &["X"])),
            BodyElement::from(pred("edge", &["Y"])),
        ],
    ))]);
    assert!(
        matches!(grows_tuple, Verdict::Unknown { .. }),
        "a tuple grows the recursion",
    );

    // p(X + 1) :- p(X).  — arithmetic deepens.
    let arithmetic = Atom::new(name("p"), [var("X") + num(1)]);
    let grows_arithmetic =
        finiteness_of([Statement::Rule(Rule::new(arithmetic, pred("p", &["X"])))]);
    assert!(
        matches!(grows_arithmetic, Verdict::Unknown { .. }),
        "arithmetic grows the recursion",
    );

    // p(f(X)) | s :- p(X).  — a disjunctive head, one arm deepens.
    let disjunctive = Head::Disjunction(Disjunction::new([
        DisjunctionElement::new(Literal::from(functional("f", var("X"))), Condition::empty()),
        DisjunctionElement::new(Literal::from(pred("s", &[])), Condition::empty()),
    ]));
    let grows_disjunctive = finiteness_of([Statement::Rule(disjunctive.when(pred("p", &["X"])))]);
    assert!(
        matches!(grows_disjunctive, Verdict::Unknown { .. }),
        "a deepening disjunctive head grows",
    );

    // { p(f(X)) } :- p(X).  — a choice head deepens.
    let choice = Head::Choice(Choice::new(
        None,
        [ChoiceElement::new(
            Literal::from(functional("f", var("X"))),
            Condition::empty(),
        )],
        None,
    ));
    let grows_choice = finiteness_of([Statement::Rule(choice.when(pred("p", &["X"])))]);
    assert!(
        matches!(grows_choice, Verdict::Unknown { .. }),
        "a deepening choice head grows",
    );

    // #count { X : p(f(X)) } >= 0 :- p(X).  — a head aggregate deepens the derived atom.
    let aggregate = Head::Aggregate(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [var("X")],
            Literal::from(functional("f", var("X"))),
            Condition::empty(),
        )],
        Some(Guard {
            relation: Some(Relation::Ge),
            term: num(0),
        }),
    ));
    let grows_aggregate = finiteness_of([Statement::Rule(aggregate.when(pred("p", &["X"])))]);
    assert!(
        matches!(grows_aggregate, Verdict::Unknown { .. }),
        "a deepening head aggregate grows",
    );

    // p(f(1)) :- p(X).  — a ground functional head collapses to a symbol and does not grow.
    let ground = finiteness_of([Statement::Rule(Rule::new(
        functional("f", num(1)),
        pred("p", &["X"]),
    ))]);
    assert_eq!(
        ground,
        Verdict::Holds,
        "a ground functional head does not grow"
    );

    // p(f(Y)) :- p(X), edge(Y).  — the former deepens a non-recursive variable, so no growth.
    let non_recursive = finiteness_of([Statement::Rule(Rule::new(
        functional("f", var("Y")),
        vec![
            BodyElement::from(pred("p", &["X"])),
            BodyElement::from(pred("edge", &["Y"])),
        ],
    ))]);
    assert_eq!(
        non_recursive,
        Verdict::Holds,
        "deepening a non-recursive variable does not grow",
    );
}

#[test]
fn finiteness_flags_equality_aliased_growth() {
    // p(g(Y)) :- p(X), Y = X.  — Y is not carried by a body literal on p, but the
    // equality Y = X aliases it to the recursive X, so g(Y) deepens the recursion just
    // as p(g(X)) :- p(X) would. A growth scan blind to `=` aliasing reports a false
    // `Holds` here (the ground program p(g(g(…(c)…))) is unbounded) — the direction §6.1
    // rules out. So Unknown.
    let head = Atom::new(
        name("p"),
        [Term::Function {
            name: name("g"),
            arguments: vec![var("Y")],
        }],
    );
    let aliased = finiteness_of([Statement::Rule(Rule::new(
        head,
        vec![BodyElement::from(pred("p", &["X"])), assign("Y", var("X"))],
    ))]);
    assert!(
        matches!(aliased, Verdict::Unknown { .. }),
        "an equality aliasing a recursive variable carries the growth",
    );

    // p(g(Z)) :- p(X), Z = 0.  — Z is equated to a constant, not to the recursion, so it
    // is not recursion-carried and the program does not grow: Holds. Pins that the `=`
    // closure does not over-flag a variable merely because it is `=`-bound.
    let bound_to_constant = finiteness_of([Statement::Rule(Rule::new(
        Atom::new(
            name("p"),
            [Term::Function {
                name: name("g"),
                arguments: vec![var("Z")],
            }],
        ),
        vec![BodyElement::from(pred("p", &["X"])), assign("Z", num(0))],
    ))]);
    assert_eq!(
        bound_to_constant,
        Verdict::Holds,
        "an equality binding a constant does not carry the recursion",
    );

    // p(g(Y)) :- p(X), edge(Y), not X = Y.  — a *negated* equality is a disequality (X ≠ Y),
    // not an alias, so Y is bound by the non-member edge(Y), not carried from the recursion:
    // finitely many g(y), so Holds. The negation guard's precision gain, pinned — an
    // unguarded closure that read `not X = Y` as an alias would flip this to a spurious
    // Unknown, and reverting the guard would leave every positive-equality law green.
    let negated_equality = BodyElement::Literal(Literal {
        negation: DefaultNegation::Not,
        inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
            var("X"),
            Relation::Eq,
            var("Y"),
        ))),
    });
    let disequality = finiteness_of([Statement::Rule(Rule::new(
        Atom::new(
            name("p"),
            [Term::Function {
                name: name("g"),
                arguments: vec![var("Y")],
            }],
        ),
        vec![
            BodyElement::from(pred("p", &["X"])),
            BodyElement::from(pred("edge", &["Y"])),
            negated_equality,
        ],
    ))]);
    assert_eq!(
        disequality,
        Verdict::Holds,
        "a negated equality is a disequality, not an alias, so it carries no recursion",
    );
}

#[test]
fn finiteness_flags_aggregate_carried_growth() {
    // p(f(X)) :- X = #max { Y : p(Y) }.  — X carries the aggregate's value over the
    // recursive p to the head, deepened under f. The scan sees the recursion only inside
    // the aggregate element's condition p(Y); the growth reaches the head through the
    // aggregate's assignment guard X. A scan that walks only plain body literals misses
    // it entirely (a false `Holds`); the sound reading is Unknown (§6.1's Unknown is
    // always safe — only a false `Holds` is unsound). The syntactic analysis flags any
    // aggregate-carried recursion conservatively, which is the whole point of `Unknown`.
    let aggregate = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            Some(Guard {
                relation: Some(Relation::Eq),
                term: var("X"),
            }),
            AggregateFunction::Max,
            [BodyAggregateElement::new(
                [var("Y")],
                Condition::new([Literal::from(pred("p", &["Y"]))]),
            )],
            None,
        )),
    };
    let grows = finiteness_of([Statement::Rule(Rule::new(
        functional("f", var("X")),
        vec![aggregate],
    ))]);
    assert!(
        matches!(grows, Verdict::Unknown { .. }),
        "an aggregate carrying the recursion to a deepened head variable grows",
    );

    // p(f(Z)) :- { p(X) } >= Z.  — a SET aggregate over the recursive p with a variable
    // bound Z that the head deepens under f: the same carrying, through the set form's
    // guard rather than a function aggregate's. Flagged Unknown, conservatively.
    let set = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Set(SetAggregate::new(
            None,
            [SetElement::Literal(Literal::from(pred("p", &["X"])))],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: var("Z"),
            }),
        )),
    };
    let grows_set = finiteness_of([Statement::Rule(Rule::new(
        functional("f", var("Z")),
        vec![set],
    ))]);
    assert!(
        matches!(grows_set, Verdict::Unknown { .. }),
        "a set aggregate carrying the recursion through its bound grows",
    );
}
