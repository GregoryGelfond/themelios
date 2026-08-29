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
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, Const, DefaultNegation, Direction,
    Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard, Head, HeadAggregate,
    HeadAggregateElement, Heuristic, Literal, LiteralInner, Optimize, OptimizeElement, Program,
    Project,
    Relation, Rule, SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryElement,
    TheoryGuard, TheoryOperator, TheoryTerm, WeakConstraint, weight,
};
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, Variable};

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
    statement_unbound(Statement::Rule(rule))
}

// The unbound variables of any single statement — a rule or a bodied directive (§5).
fn statement_unbound(statement: Statement) -> BTreeSet<Variable> {
    let safety = safety_of([statement]);
    safety
        .unsafe_statements()
        .next()
        .map(|s| s.unbound().cloned().collect())
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

// Whether a term mentions the anonymous `_` — the reference reading, independent of the
// production's own `term_has_anonymous`.
fn term_has_anon(term: &Term) -> bool {
    term.subterms()
        .any(|subterm| matches!(subterm, Term::Variable(Variable::Anonymous)))
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
            // A `_` on the read side is an unbound fresh variable, so the assignment cannot fire.
            if term_has_anon(sides[1 - i]) {
                continue;
            }
            let mut other = BTreeSet::new();
            term_named_vars(sides[1 - i], &mut other);
            if !other.contains(v) {
                assignments.push((v.clone(), other));
            }
        }
    }
    assignments
}

// Whether a `_` occurs unbound. Each `_` is a distinct fresh variable, bound only by a positive
// body atom (it binds itself) or as the whole lone side of a single `=` whose other side is fully
// bound (it is assigned that value); anywhere else — the head, a negated literal, a nested or
// non-lone `=` position, a non-`=` comparison — it is unbound. Computed by occurrence, independent
// of the production's rename.
fn naive_anonymous_unbound(rule: &Rule, bound: &BTreeSet<Variable>) -> bool {
    let head_anon = match rule.head().get() {
        Head::Literal(literal) => match &literal.inner {
            LiteralInner::Atom(atom) => atom.get().arguments.iter().any(term_has_anon),
            _ => false,
        },
        _ => false,
    };
    let body_anon = rule.body().get().elements().any(|element| {
        let BodyElement::Literal(literal) = element.get() else {
            return false;
        };
        match &literal.inner {
            LiteralInner::Atom(atom) => {
                literal.negation != DefaultNegation::None
                    && atom.get().arguments.iter().any(term_has_anon)
            }
            LiteralInner::Comparison(comparison) => {
                comparison_anon_unbound(comparison.get(), bound)
            }
            LiteralInner::True | LiteralInner::False => false,
        }
    });
    head_anon || body_anon
}

fn comparison_anon_unbound(comparison: &Comparison, bound: &BTreeSet<Variable>) -> bool {
    let steps: Vec<(Relation, &Term)> = comparison.steps().collect();
    if steps.len() == 1 && steps[0].0 == Relation::Eq {
        let sides = [comparison.first(), steps[0].1];
        for (i, side) in sides.iter().enumerate() {
            if !term_has_anon(side) {
                continue;
            }
            // Bound only if this side is exactly `_` and the other side is fully bound.
            let lone = matches!(side, Term::Variable(Variable::Anonymous));
            let other = sides[1 - i];
            let mut other_vars = BTreeSet::new();
            term_named_vars(other, &mut other_vars);
            let other_bound = !term_has_anon(other) && other_vars.iter().all(|v| bound.contains(v));
            if !(lone && other_bound) {
                return true;
            }
        }
        false
    } else {
        term_has_anon(comparison.first()) || comparison.steps().any(|(_, term)| term_has_anon(term))
    }
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
    let mut unbound: BTreeSet<Variable> = all.difference(&bound).cloned().collect();
    if naive_anonymous_unbound(rule, &bound) {
        unbound.insert(Variable::Anonymous);
    }
    unbound
}

// A generated aggregate-free rule over variables X0..X4, the anonymous `_`, and predicates p/q/r.
#[derive(Clone, Debug)]
enum Elem {
    Positive(usize, Vec<Arg>),
    Negative(usize, Vec<Arg>),
    Assign(usize, Rhs),
    // `_ = rhs` — the anonymous on the assigned side.
    AssignAnon(Rhs),
}

// An atom argument: a named variable X0..X4, or the anonymous `_` (a distinct fresh variable).
#[derive(Clone, Debug)]
enum Arg {
    Var(usize),
    Anon,
}

#[derive(Clone, Debug)]
enum Rhs {
    Const,
    Var(usize),
    VarPlus(usize),
    Anon,
    // `f(_)` — a `_` nested inside a former, never the lone side.
    FunAnon,
}

const PREDS: [&str; 3] = ["p", "q", "r"];

fn var_name(i: usize) -> String {
    format!("X{i}")
}

fn arg_term(arg: &Arg) -> Term {
    match arg {
        Arg::Var(i) => var(&var_name(*i)),
        Arg::Anon => Term::Variable(Variable::Anonymous),
    }
}

fn build_atom(p: usize, args: &[Arg]) -> Atom {
    Atom::new(name(PREDS[p]), args.iter().map(arg_term))
}

fn rhs_term(rhs: &Rhs) -> Term {
    match rhs {
        Rhs::Const => num(0),
        Rhs::Var(j) => var(&var_name(*j)),
        Rhs::VarPlus(j) => var(&var_name(*j)) + num(1),
        Rhs::Anon => Term::Variable(Variable::Anonymous),
        Rhs::FunAnon => Term::Function {
            name: name("f"),
            arguments: vec![Term::Variable(Variable::Anonymous)],
        },
    }
}

// `lhs = rhs` as a body element, for an arbitrary left term (named or anonymous).
fn assign_lhs(lhs: Term, rhs: Term) -> BodyElement {
    BodyElement::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
            lhs,
            Relation::Eq,
            rhs,
        ))),
    })
}

fn build_body(elements: &[Elem]) -> Vec<BodyElement> {
    elements
        .iter()
        .map(|element| match element {
            Elem::Positive(p, args) => BodyElement::from(build_atom(*p, args)),
            Elem::Negative(p, args) => not(build_atom(*p, args)),
            Elem::Assign(lhs, rhs) => assign_lhs(var(&var_name(*lhs)), rhs_term(rhs)),
            Elem::AssignAnon(rhs) => assign_lhs(Term::Variable(Variable::Anonymous), rhs_term(rhs)),
        })
        .collect()
}

fn any_arg() -> impl Strategy<Value = Arg> {
    // Weighted toward named variables so `_` is exercised without dominating.
    prop_oneof![5 => (0usize..5).prop_map(Arg::Var), 1 => Just(Arg::Anon)]
}

// A fresh atom-reference strategy each call — `impl Strategy` erases `Clone`, so the positive and
// negative arms each build their own rather than sharing one.
fn any_atom_ref() -> impl Strategy<Value = (usize, Vec<Arg>)> {
    (0usize..3, prop::collection::vec(any_arg(), 0..3))
}

fn any_rhs() -> impl Strategy<Value = Rhs> {
    prop_oneof![
        Just(Rhs::Const),
        (0usize..5).prop_map(Rhs::Var),
        (0usize..5).prop_map(Rhs::VarPlus),
        Just(Rhs::Anon),
        Just(Rhs::FunAnon),
    ]
}

fn any_element() -> impl Strategy<Value = Elem> {
    prop_oneof![
        any_atom_ref().prop_map(|(p, args)| Elem::Positive(p, args)),
        any_atom_ref().prop_map(|(p, args)| Elem::Negative(p, args)),
        (0usize..5, any_rhs()).prop_map(|(lhs, rhs)| Elem::Assign(lhs, rhs)),
        any_rhs().prop_map(Elem::AssignAnon),
    ]
}

fn any_rule() -> impl Strategy<Value = Rule> {
    let head_args = prop::collection::vec(any_arg(), 0..3);
    let body = prop::collection::vec(any_element(), 0..6);
    (head_args, body)
        .prop_map(|(head_args, body)| Rule::new(build_atom(0, &head_args), build_body(&body)))
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
fn safety_reads_each_anonymous_as_a_distinct_fresh_variable() {
    let anon = || Term::Variable(Variable::Anonymous);

    // p(_) :- q.  → unsafe. The head `_` is a distinct fresh variable with no binding occurrence
    // (a binding occurrence is a positive body atom, always a different `_`); the grounder refuses
    // it. Reading it safe was a false `safe`, a definite-safety soundness break.
    assert_eq!(
        unbound_of(Rule::new(Atom::new(name("p"), [anon()]), pred("q", &[]))),
        [Variable::Anonymous].into_iter().collect(),
    );

    // p(X) :- q, X = _.  → unsafe. `q` does not bind `X`, so `X = _` binds neither side; both `X`
    // and the `_` are unbound. (Reading `_` as ground made `X = _` fire and hid this.)
    assert_eq!(
        unbound_of(Rule::new(
            pred("p", &["X"]),
            vec![BodyElement::from(pred("q", &[])), assign("X", anon())],
        )),
        [named("X"), Variable::Anonymous].into_iter().collect(),
    );

    // p(X) :- q(X), X = _.  → SAFE. `q(X)` binds `X`, so `X = _` binds the `_` — exactly as
    // `X = Y` would. A single anonymous *marker* wrongly flags this (it cannot tell this `_` is
    // bound by X); renaming each `_` to a distinct fresh variable reads it correctly.
    assert!(
        unbound_of(Rule::new(
            pred("p", &["X"]),
            vec![BodyElement::from(pred("q", &["X"])), assign("X", anon())],
        ))
        .is_empty(),
    );

    // p(X) :- q(X), r(_).  → SAFE. A `_` in a positive body atom binds itself.
    assert!(
        unbound_of(Rule::new(
            pred("p", &["X"]),
            vec![
                BodyElement::from(pred("q", &["X"])),
                BodyElement::from(Atom::new(name("r"), [anon()])),
            ],
        ))
        .is_empty(),
    );

    // p(X) :- q(X), not r(_).  → unsafe. A `_` in a negated literal is required, never bound.
    assert_eq!(
        unbound_of(Rule::new(
            pred("p", &["X"]),
            vec![
                BodyElement::from(pred("q", &["X"])),
                not(Atom::new(name("r"), [anon()])),
            ],
        )),
        [Variable::Anonymous].into_iter().collect(),
    );

    // p(_) :- q(_).  → unsafe. The head `_` and the body `_` are *distinct*; the body one binds
    // itself, the head one has no binder.
    assert_eq!(
        unbound_of(Rule::new(
            Atom::new(name("p"), [anon()]),
            Atom::new(name("q"), [anon()]),
        )),
        [Variable::Anonymous].into_iter().collect(),
    );

    // p(X) :- q(X), _ = X.  → SAFE. The `_` is the lone assigned side and `X` is bound, so the
    // `_` is bound — the assignment reads in either direction.
    assert!(
        unbound_of(Rule::new(
            pred("p", &["X"]),
            vec![
                BodyElement::from(pred("q", &["X"])),
                BodyElement::Literal(Literal {
                    negation: DefaultNegation::None,
                    inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
                        anon(),
                        Relation::Eq,
                        var("X"),
                    ))),
                }),
            ],
        ))
        .is_empty(),
    );

    // p :- _ = 1.  → SAFE. The `_` is assigned a ground term.
    assert!(
        unbound_of(Rule::new(
            pred("p", &[]),
            BodyElement::Literal(Literal {
                negation: DefaultNegation::None,
                inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
                    anon(),
                    Relation::Eq,
                    num(1),
                ))),
            }),
        ))
        .is_empty(),
    );

    // p(X) :- q(X), X = f(_).  → unsafe. A `_` nested inside a former is not the lone side, so it
    // is required-unbound (`=` does not decompose `f`), even though `X` is bound.
    let f_of_anon = Term::Function {
        name: name("f"),
        arguments: vec![anon()],
    };
    assert_eq!(
        unbound_of(Rule::new(
            pred("p", &["X"]),
            vec![BodyElement::from(pred("q", &["X"])), assign("X", f_of_anon),],
        )),
        [Variable::Anonymous].into_iter().collect(),
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

// ---- Correctness: the theory-term-local close (§5, §4.9) ----

#[test]
fn safety_vets_theory_term_variables() {
    // A theory atom `&t { element_term : condition }` (optionally guarded), in a constraint body.
    let theory =
        |element_term: TheoryTerm, condition: Option<Condition>, guard: Option<TheoryGuard>| {
            BodyElement::TheoryAtom {
                negation: DefaultNegation::None,
                atom: TheoryAtom::new(
                    name("t"),
                    Vec::<Term>::new(),
                    [TheoryElement::new([element_term], condition)],
                    guard,
                ),
            }
        };

    // :- &t { X }.  — X occurs only in a theory element term. A theory term binds no ordinary
    // variable, so X is unbound and the grounder refuses it: the theory-term-local close descends the
    // shared leaves (§4.9) and flags X, where the old liberal boundary read it safe.
    assert_eq!(
        unbound_of(Rule::constraint(vec![theory(
            TheoryTerm::Variable(named("X")),
            None,
            None,
        )])),
        [named("X")].into_iter().collect(),
        "a variable only in a theory term is unsafe",
    );

    // :- &t { X : q(X) }.  — the element condition binds X within the element: safe.
    assert!(
        unbound_of(Rule::constraint(vec![theory(
            TheoryTerm::Variable(named("X")),
            Some(Condition::new([Literal::from(pred("q", &["X"]))])),
            None,
        )]))
        .is_empty(),
        "an element condition binds the theory-term variable: safe",
    );

    // :- &t { X }, q(X).  — the ordinary body binds X globally: safe.
    assert!(
        unbound_of(Rule::constraint(vec![
            theory(TheoryTerm::Variable(named("X")), None, None),
            BodyElement::from(pred("q", &["X"])),
        ]))
        .is_empty(),
        "an ordinary body atom binds the theory-term variable: safe",
    );

    // :- &t { 0 } >= Y.  — the guard's theory term Y is at the atom scope, unbound: unsafe.
    assert_eq!(
        unbound_of(Rule::constraint(vec![theory(
            TheoryTerm::Symbolic(Symbol::Number(0)),
            None,
            Some(TheoryGuard {
                operator: TheoryOperator::new(">="),
                term: TheoryTerm::Variable(named("Y")),
            }),
        )])),
        [named("Y")].into_iter().collect(),
        "a theory guard variable is unbound at the atom scope: unsafe",
    );

    // :- &t { _ }.  — a constructed anonymous theory-term leaf can never be bound (§8), so unsafe;
    // the witness names `_`.
    assert_eq!(
        unbound_of(Rule::constraint(vec![theory(
            TheoryTerm::Variable(Variable::Anonymous),
            None,
            None,
        )])),
        [Variable::Anonymous].into_iter().collect(),
        "an anonymous theory-term leaf is unbindable, so unsafe",
    );
}

// ---- Correctness: the bodied-directive close (§5) ----

#[test]
fn safety_vets_bodied_directives() {
    // :~ q. [W@1]  — the weak-constraint weight W has no binder (`q` binds nothing): unsafe.
    assert_eq!(
        statement_unbound(Statement::WeakConstraint(WeakConstraint::new(
            Body::new([BodyElement::from(pred("q", &[]))]),
            weight(var("W")).at_priority(num(1)),
            Vec::<Term>::new(),
        ))),
        [named("W")].into_iter().collect(),
        "a weak-constraint weight variable with no binder is unsafe",
    );

    // :~ q(W). [W@1]  — the body binds W: safe.
    assert!(
        statement_unbound(Statement::WeakConstraint(WeakConstraint::new(
            Body::new([BodyElement::from(pred("q", &["W"]))]),
            weight(var("W")).at_priority(num(1)),
            Vec::<Term>::new(),
        )))
        .is_empty(),
        "the body binds the weight variable: safe",
    );

    // #show f(X) : q(X).  — safe (the body binds the shown term's X).
    assert!(
        statement_unbound(Statement::Show(Show::term_body(
            function("f", "X"),
            Body::new([BodyElement::from(pred("q", &["X"]))]),
        )))
        .is_empty(),
        "the body binds the shown term's variable: safe",
    );

    // #show f(X) : q(Y).  — X unbound (the body binds only Y): unsafe.
    assert_eq!(
        statement_unbound(Statement::Show(Show::term_body(
            function("f", "X"),
            Body::new([BodyElement::from(pred("q", &["Y"]))]),
        ))),
        [named("X")].into_iter().collect(),
        "the shown term's X is unbound when the body binds only Y: unsafe",
    );

    // #project p(X) : q(X).  — safe (the body binds the projected atom's X).
    assert!(
        statement_unbound(Statement::Project(Project::atom_body(
            pred("p", &["X"]),
            Body::new([BodyElement::from(pred("q", &["X"]))]),
        )))
        .is_empty(),
        "the body binds the projected atom's variable: safe",
    );

    // #external p(X).  — an empty body binds nothing, so X is unsafe (the empty-body form, grammar §5.9).
    assert_eq!(
        statement_unbound(Statement::External(External::new(
            pred("p", &["X"]),
            Body::empty(),
            None,
        ))),
        [named("X")].into_iter().collect(),
        "an #external atom variable with an empty body is unsafe",
    );

    // #edge (X, Y) : q(X), r(Y).  — safe (the body binds both node terms).
    assert!(
        statement_unbound(Statement::Edge(Edge::new(
            [(var("X"), var("Y"))],
            Body::new([
                BodyElement::from(pred("q", &["X"])),
                BodyElement::from(pred("r", &["Y"])),
            ]),
        )))
        .is_empty(),
        "the body binds the edge node terms: safe",
    );

    // #edge (X, Y).  — an empty body binds nothing: X and Y unsafe.
    assert_eq!(
        statement_unbound(Statement::Edge(Edge::new(
            [(var("X"), var("Y"))],
            Body::empty(),
        ))),
        [named("X"), named("Y")].into_iter().collect(),
        "an #edge with variable node terms and an empty body is unsafe",
    );

    // #heuristic p(X) : q(X). [1, 1]  — safe (the body binds the atom's X).
    assert!(
        statement_unbound(Statement::Heuristic(Heuristic::new(
            pred("p", &["X"]),
            Body::new([BodyElement::from(pred("q", &["X"]))]),
            num(1),
            None,
            num(1),
        )))
        .is_empty(),
        "the body binds the heuristic atom's variable: safe",
    );

    // #heuristic p(0) : . [W@V, M]  — the bias W, priority V, and modifier M are required and, with an
    // empty body and a ground atom, unbound: unsafe.
    assert_eq!(
        statement_unbound(Statement::Heuristic(Heuristic::new(
            Atom::new(name("p"), [num(0)]),
            Body::empty(),
            var("W"),
            Some(var("V")),
            var("M"),
        ))),
        [named("W"), named("V"), named("M")].into_iter().collect(),
        "the heuristic bias, priority, and modifier are required and unbound with an empty body",
    );

    // #minimize { W@P, X : q(X) }.  — the element condition binds X only; the weight W and priority P
    // are element-required and unbound: unsafe.
    let unbound = statement_unbound(Statement::Optimize(Optimize::new(
        Direction::Minimize,
        [OptimizeElement::new(
            weight(var("W")).at_priority(var("P")),
            [var("X")],
            Condition::new([Literal::from(pred("q", &["X"]))]),
        )],
    )));
    assert!(
        unbound.contains(&named("W")) && unbound.contains(&named("P")),
        "an optimize element's unbound weight and priority are flagged; got {unbound:?}",
    );

    // #minimize { W@P, X : q(X, W, P) }.  — the element condition binds W, P, X: safe.
    assert!(
        statement_unbound(Statement::Optimize(Optimize::new(
            Direction::Minimize,
            [OptimizeElement::new(
                weight(var("W")).at_priority(var("P")),
                [var("X")],
                Condition::new([Literal::from(pred("q", &["X", "W", "P"]))]),
            )],
        )))
        .is_empty(),
        "an optimize element condition binds its weight, priority, and terms: safe",
    );

    // #const x = 1.  — a no-obligation directive: no variable, safe.
    assert!(
        statement_unbound(Statement::Const(Const {
            name: name("x"),
            value: num(1),
            policy: None,
        }))
        .is_empty(),
        "a #const with a ground value is safe (no binding obligation)",
    );
}

#[test]
fn safety_vets_a_directive_borne_anonymous_variable() {
    // :~ q. [_@1]  — the weight is `_`. A directive-borne `_` in a requiring position must be
    // freshened (over the whole *statement*, not just a rule) and reported unbound. Extending the scan
    // to directives without generalizing the freshener would read this safe — the anonymous false-`safe`
    // (a `_` invisible to the binding scan because it is not a named variable) reopened one statement
    // kind over. The witness names `_`.
    assert_eq!(
        statement_unbound(Statement::WeakConstraint(WeakConstraint::new(
            Body::new([BodyElement::from(pred("q", &[]))]),
            weight(Term::Variable(Variable::Anonymous)).at_priority(num(1)),
            Vec::<Term>::new(),
        ))),
        [Variable::Anonymous].into_iter().collect(),
        "a directive-borne anonymous weight is unsafe (the freshener generalizes to statements)",
    );

    // #show f(_) : q(X).  — the shown term's `_` is required and unbindable by the body's X: unsafe.
    assert_eq!(
        statement_unbound(Statement::Show(Show::term_body(
            Term::Function {
                name: name("f"),
                arguments: vec![Term::Variable(Variable::Anonymous)],
            },
            Body::new([BodyElement::from(pred("q", &["X"]))]),
        ))),
        [Variable::Anonymous].into_iter().collect(),
        "an anonymous in a #show term, unbindable by the body, is unsafe",
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

    /// A weak constraint's safety reduces exactly to a rule's: its weight, priority, and tuple terms
    /// are required and bound by its body, just as a rule's head is bound by its body. So it agrees
    /// with the trusted rule reference on the reduced rule `pseudo(weight, priority, tuple) :- body` —
    /// the bodied-directive close's universal random-shape coverage, through the same oracle (§10). The
    /// generated arguments include the anonymous `_` (via `any_arg`), so the directive `_` is fuzzed.
    #[test]
    fn a_weak_constraint_agrees_with_the_reduced_rule_reference(
        head_args in prop::collection::vec(any_arg(), 1..4),
        body in prop::collection::vec(any_element(), 0..6),
    ) {
        let reduced = Rule::new(build_atom(0, &head_args), build_body(&body));
        let expected = naive_unbound(&reduced);

        let terms: Vec<Term> = head_args.iter().map(arg_term).collect();
        let (weight_term, rest) = terms.split_first().expect("1..4 args, so at least one");
        let mut w = weight(weight_term.clone());
        let tuple: Vec<Term> = match rest.split_first() {
            Some((priority, tail)) => {
                w = w.at_priority(priority.clone());
                tail.to_vec()
            }
            None => Vec::new(),
        };
        let weak =
            Statement::WeakConstraint(WeakConstraint::new(Body::new(build_body(&body)), w, tuple));
        prop_assert_eq!(statement_unbound(weak), expected);
    }

    /// A theory atom's ordinary variable leaves are safe iff bound: an element-term variable by its
    /// element condition or the global body, a guard variable by the global body alone (§5, §4.9). The
    /// universal law over theory-term variables (§10), checked against an independent reading of the
    /// shape — the element/guard scoping distinction (local vs global) read directly, not via the
    /// production. (The anonymous theory leaf is a unit case; here the leaves are named.)
    #[test]
    fn a_theory_term_variable_is_safe_iff_bound(
        element_var in 0usize..3,
        cond_var in prop::option::of(0usize..3),
        guard_var in prop::option::of(0usize..3),
        body_vars in prop::collection::vec(0usize..3, 0..3),
    ) {
        let element = TheoryElement::new(
            [TheoryTerm::Variable(named(&var_name(element_var)))],
            cond_var.map(|c| Condition::new([Literal::from(pred("p", &[&var_name(c)]))])),
        );
        let guard = guard_var.map(|g| TheoryGuard {
            operator: TheoryOperator::new(">="),
            term: TheoryTerm::Variable(named(&var_name(g))),
        });
        let theory = BodyElement::TheoryAtom {
            negation: DefaultNegation::None,
            atom: TheoryAtom::new(name("t"), Vec::<Term>::new(), [element], guard),
        };
        let mut body: Vec<BodyElement> = vec![theory];
        for v in &body_vars {
            body.push(BodyElement::from(pred("q", &[&var_name(*v)])));
        }
        let actual = unbound_of(Rule::constraint(body));

        // Independent oracle: the global body binds a variable everywhere; the element condition binds
        // its element-local variable only. The guard variable is global (bound by the body alone).
        let global_bound: BTreeSet<usize> = body_vars.iter().copied().collect();
        let mut element_bound = global_bound.clone();
        if let Some(c) = cond_var {
            element_bound.insert(c);
        }
        let mut expected = BTreeSet::new();
        if !element_bound.contains(&element_var) {
            expected.insert(named(&var_name(element_var)));
        }
        if let Some(g) = guard_var
            && !global_bound.contains(&g)
        {
            expected.insert(named(&var_name(g)));
        }
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

// ---- Finiteness: growth carried by a body `=`-assignment ----
//
// The direct former `q(f(Y)) :- q(Y)` is `growing_rule` above. These pin the growth that reaches
// the head through a body `=`-assignment — a false `Holds` if missed — and the aliasing that must
// NOT be read as growth.

fn function(name_text: &str, arg: &str) -> Term {
    Term::Function {
        name: name(name_text),
        arguments: vec![var(arg)],
    }
}

#[test]
fn finiteness_flags_equality_assigned_growth() {
    // q(X) :- q(Y), X = f(Y).  The deepening is in the body `=`; the head carries X *bare*.
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
        Verdict::Holds => panic!("`=`-assignment-carried growth must be Unknown, not Holds"),
    }
}

#[test]
fn finiteness_flags_an_assignment_deepening_chain() {
    // q(X0) :- q(X3), X0 = f(X1), X1 = f(X2), X2 = f(X3).  The deepening reaches the bare head
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
        "a chain of `=`-deepenings to a bare head must be Unknown",
    );
}

#[test]
fn finiteness_flags_arithmetic_successor_growth() {
    // q(X) :- q(Y), X = Y + 1.  The integer successor is the same act as the term successor —
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
        "arithmetic successor growth must be Unknown",
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
        "a bare `=`-aliased variable does not grow: Holds",
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
        "an alias under a head former grows: Unknown",
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
    // f(Y) = X is the same deepening as X = f(Y), with the former on the left.
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
fn finiteness_flags_growth_carried_by_a_head_disjunction_condition() {
    // p(f(X)) : p(X) :- base.  — the head element derives p(f(X)) under the condition p(X); the
    // condition makes p recursive and carries X, deepened under f in the derived literal. The
    // carrier is in the head element, not the body — grounds p(f^n(seed)), so Unknown, not Holds.
    let head = Head::Disjunction(Disjunction::new([DisjunctionElement::new(
        Literal::from(Atom::new(name("p"), [function("f", "X")])),
        Condition::new([Literal::from(pred("p", &["X"]))]),
    )]));
    match finiteness_of([Statement::Rule(head.when(Atom::constant(name("base"))))]) {
        Verdict::Unknown { witness } => assert!(
            witness.members().any(|s| s.name.as_str() == "p"),
            "the growing component is p",
        ),
        Verdict::Holds => panic!("growth carried by a head-element condition must be Unknown"),
    }
}

#[test]
fn finiteness_flags_growth_carried_by_a_choice_condition() {
    // { p(f(X)) : p(X) } :- base.  — the same carrier, in a choice head element.
    let head = Head::Choice(Choice::new(
        None,
        [ChoiceElement::new(
            Literal::from(Atom::new(name("p"), [function("f", "X")])),
            Condition::new([Literal::from(pred("p", &["X"]))]),
        )],
        None,
    ));
    assert!(
        matches!(
            finiteness_of([Statement::Rule(head.when(Atom::constant(name("base"))))]),
            Verdict::Unknown { .. }
        ),
        "growth carried by a choice-element condition must be Unknown",
    );
}

#[test]
fn finiteness_flags_growth_carried_by_a_head_aggregate_condition() {
    // #count { : p(f(X)) : p(X) } >= 1 :- base.  — the same carrier, in a head-aggregate element.
    let head = Head::Aggregate(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            Vec::<Term>::new(),
            Literal::from(Atom::new(name("p"), [function("f", "X")])),
            Condition::new([Literal::from(pred("p", &["X"]))]),
        )],
        Some(Guard {
            relation: Some(Relation::Ge),
            term: num(1),
        }),
    ));
    assert!(
        matches!(
            finiteness_of([Statement::Rule(head.when(Atom::constant(name("base"))))]),
            Verdict::Unknown { .. }
        ),
        "growth carried by a head-aggregate element condition must be Unknown",
    );
}

#[test]
fn finiteness_flags_a_head_condition_equality_deepening() {
    // p(g(X)) : X = f(Y), p(Y) :- base.  — the head-element condition carries Y (via p(Y)) AND
    // deepens X (via X = f(Y)); the derived p(g(X)) grows. The `=` is INSIDE the head condition, the
    // position the atom-only carrier walk dropped.
    let head = Head::Disjunction(Disjunction::new([DisjunctionElement::new(
        Literal::from(Atom::new(name("p"), [function("g", "X")])),
        Condition::new([
            eq_literal(var("X"), function("f", "Y")),
            Literal::from(pred("p", &["Y"])),
        ]),
    )]));
    match finiteness_of([Statement::Rule(head.when(Atom::constant(name("base"))))]) {
        Verdict::Unknown { witness } => assert!(
            witness.members().any(|s| s.name.as_str() == "p"),
            "the growing component is p",
        ),
        Verdict::Holds => panic!("a head-element condition `=`-deepening must be Unknown"),
    }
}

#[test]
fn finiteness_flags_a_head_condition_alias_under_a_former() {
    // { p(g(X)) : X = Y, p(Y) } :- base.  — the head condition aliases X = Y (not a former); the
    // derived p(g(X)) = p(g(Y)) deepens the aliased carried Y. A choice head.
    let head = Head::Choice(Choice::new(
        None,
        [ChoiceElement::new(
            Literal::from(Atom::new(name("p"), [function("g", "X")])),
            Condition::new([
                eq_literal(var("X"), var("Y")),
                Literal::from(pred("p", &["Y"])),
            ]),
        )],
        None,
    ));
    assert!(
        matches!(
            finiteness_of([Statement::Rule(head.when(Atom::constant(name("base"))))]),
            Verdict::Unknown { .. }
        ),
        "a head-element condition alias under a head former must be Unknown",
    );
}

// ---- Differential: a program that grows by construction is never a false `Holds` (§6.1) ----
//
// The soundness net for the carrier/graph congruence: a recursive program whose growth — a
// term-former over a variable carried around the recursion — is injected at a randomized position
// (a head former, a body `=`, a `=`-chain, a head-element-condition atom or `=` in any head kind, a
// `#max` aggregate element term, and across mutual recursion), under a randomized former (function
// or arithmetic). Each grounds unboundedly by construction, so finiteness MUST return `Unknown`; a
// `Holds` is a growth position the carrier walk missed — a false `Holds`, which no per-position
// audit is trusted to rule out, and whose blind spots are widened here to the positions prior
// versions missed (aggregate element terms, cross-predicate recursion, choice/head-aggregate heads).

#[derive(Clone, Debug)]
enum Former {
    Function,
    Arithmetic,
}

fn apply_former(former: &Former, inner: Term) -> Term {
    match former {
        Former::Function => Term::Function {
            name: name("f"),
            arguments: vec![inner],
        },
        Former::Arithmetic => Term::BinaryOperation {
            operator: BinaryOp::Add,
            left: Box::new(inner),
            right: Box::new(num(1)),
        },
    }
}

fn nested_former(depth: u8, former: &Former, inner: Term) -> Term {
    let mut term = inner;
    for _ in 0..depth.max(1) {
        term = apply_former(former, term);
    }
    term
}

#[derive(Clone, Debug)]
enum HeadKind {
    Disjunction,
    Choice,
    HeadAggregate,
}

// A head deriving one element `literal : condition`, in the chosen head kind.
fn single_element_head(kind: &HeadKind, literal: Literal, condition: Condition) -> Head {
    match kind {
        HeadKind::Disjunction => Head::Disjunction(Disjunction::new([DisjunctionElement::new(
            literal, condition,
        )])),
        HeadKind::Choice => Head::Choice(Choice::new(
            None,
            [ChoiceElement::new(literal, condition)],
            None,
        )),
        HeadKind::HeadAggregate => Head::Aggregate(HeadAggregate::new(
            None,
            AggregateFunction::Count,
            [HeadAggregateElement::new(
                Vec::<Term>::new(),
                literal,
                condition,
            )],
            Some(Guard {
                relation: Some(Relation::Ge),
                term: num(1),
            }),
        )),
    }
}

#[derive(Clone, Debug)]
enum Grow {
    HeadFormer,
    BodyEquality,
    BodyEqualityChain(u8),
    HeadConditionAtom,
    HeadConditionEquality,
    AggregateExtremum,
    MutualRecursion,
}

// The statements that grow the recursion of `p`, injecting the growth at `grow`'s position under
// `former`, using `kind` for any head-element position.
fn injected_growth(grow: &Grow, former: &Former, kind: &HeadKind, depth: u8) -> Vec<Statement> {
    let deeper = |variable: &str| nested_former(depth, former, var(variable));
    let rules = match grow {
        // p(f(Y)) :- p(Y).
        Grow::HeadFormer => vec![Rule::new(
            Atom::new(name("p"), [deeper("Y")]),
            pred("p", &["Y"]),
        )],
        // p(X) :- p(Y), X = f(Y).
        Grow::BodyEquality => vec![Rule::new(
            pred("p", &["X"]),
            vec![
                BodyElement::from(pred("p", &["Y"])),
                assign("X", deeper("Y")),
            ],
        )],
        // p(X0) :- p(Xn), X0 = f(X1), …, X_{n-1} = f(Xn).
        Grow::BodyEqualityChain(links) => {
            let n = (*links as usize).max(1);
            let mut body: Vec<BodyElement> =
                vec![BodyElement::from(pred("p", &[format!("X{n}").as_str()]))];
            for i in 0..n {
                body.push(assign(
                    &format!("X{i}"),
                    nested_former(depth, former, var(&format!("X{}", i + 1))),
                ));
            }
            vec![Rule::new(pred("p", &["X0"]), body)]
        }
        // p(f(X)) : p(X) :- base.  (in the chosen head kind)
        Grow::HeadConditionAtom => vec![
            single_element_head(
                kind,
                Literal::from(Atom::new(name("p"), [deeper("X")])),
                Condition::new([Literal::from(pred("p", &["X"]))]),
            )
            .when(Atom::constant(name("base"))),
        ],
        // p(X) : X = f(Y), p(Y) :- base.  — the deepening `=` inside the head condition.
        Grow::HeadConditionEquality => vec![
            single_element_head(
                kind,
                Literal::from(pred("p", &["X"])),
                Condition::new([
                    eq_literal(var("X"), deeper("Y")),
                    Literal::from(pred("p", &["Y"])),
                ]),
            )
            .when(Atom::constant(name("base"))),
        ],
        // p(X) :- X = #max { f(Y) : p(Y) }.  — a former element term to a bare head.
        Grow::AggregateExtremum => {
            let element = BodyAggregateElement::new(
                [deeper("Y")],
                Condition::new([Literal::from(pred("p", &["Y"]))]),
            );
            let aggregate = BodyElement::Aggregate {
                negation: DefaultNegation::None,
                aggregate: Aggregate::Function(FunctionAggregate::new(
                    Some(Guard {
                        relation: Some(Relation::Eq),
                        term: var("X"),
                    }),
                    AggregateFunction::Max,
                    [element],
                    None,
                )),
            };
            vec![Rule::new(pred("p", &["X"]), vec![aggregate])]
        }
        // p(f(Y)) :- q(Y).  q(X) :- p(X).  — mutual recursion; growth in p over q's carried Y.
        Grow::MutualRecursion => vec![
            Rule::new(Atom::new(name("p"), [deeper("Y")]), pred("q", &["Y"])),
            Rule::new(pred("q", &["X"]), pred("p", &["X"])),
        ],
    };
    rules.into_iter().map(Statement::Rule).collect()
}

fn any_grow() -> impl Strategy<Value = (Grow, Former, HeadKind, u8)> {
    (
        prop_oneof![
            Just(Grow::HeadFormer),
            Just(Grow::BodyEquality),
            (1u8..8).prop_map(Grow::BodyEqualityChain),
            Just(Grow::HeadConditionAtom),
            Just(Grow::HeadConditionEquality),
            Just(Grow::AggregateExtremum),
            Just(Grow::MutualRecursion),
        ],
        prop_oneof![Just(Former::Function), Just(Former::Arithmetic)],
        prop_oneof![
            Just(HeadKind::Disjunction),
            Just(HeadKind::Choice),
            Just(HeadKind::HeadAggregate),
        ],
        1u8..4,
    )
}

proptest! {
    #[test]
    fn a_constructed_growth_is_never_a_false_holds(
        (grow, former, kind, depth) in any_grow()
    ) {
        let verdict = finiteness_of(injected_growth(&grow, &former, &kind, depth));
        prop_assert!(
            matches!(verdict, Verdict::Unknown { .. }),
            "finiteness must be Unknown for a program that grows by construction: {grow:?} / {former:?} / {kind:?} / depth {depth}",
        );
    }
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
fn safety_binds_a_choice_or_disjunction_element_by_its_condition() {
    // { p(X) : q(X) } :- r.  — the canonical choice idiom; X is element-local, bound by its own
    // condition q(X), not required at the rule's top level. A scan that promotes the element
    // literal to the global scope reads it unsafe (a false `unsafe` — the whole choice-rule
    // generate idiom); the sound reading is safe.
    let choice_condition = Head::Choice(Choice::new(
        None,
        [ChoiceElement::new(
            Literal::from(pred("p", &["X"])),
            Condition::new([Literal::from(pred("q", &["X"]))]),
        )],
        None,
    ));
    assert!(unbound_of(choice_condition.when(pred("r", &[]))).is_empty());

    // p(X) : q(X) :- r.  — the disjunctive analog; same element-local binding.
    let disjunction_condition = Head::Disjunction(Disjunction::new([DisjunctionElement::new(
        Literal::from(pred("p", &["X"])),
        Condition::new([Literal::from(pred("q", &["X"]))]),
    )]));
    assert!(unbound_of(disjunction_condition.when(pred("r", &[]))).is_empty());

    // { p(X) : q(Y) } :- r.  — the scoping stays precise: the condition binds Y, not X, so X is
    // unbound (Y is element-local and bound). Not a blanket "condition present ⇒ safe".
    let choice_wrong_var = Head::Choice(Choice::new(
        None,
        [ChoiceElement::new(
            Literal::from(pred("p", &["X"])),
            Condition::new([Literal::from(pred("q", &["Y"]))]),
        )],
        None,
    ));
    assert_eq!(
        unbound_of(choice_wrong_var.when(pred("r", &[]))),
        [named("X")].into_iter().collect(),
    );

    // { p(X) : q(X) ; s(Y) : t(Y) } :- r.  — two elements, each variable local to its own element;
    // both bound within their element, so safe (a shared scope would be unsound cross-talk).
    let choice_two = Head::Choice(Choice::new(
        None,
        [
            ChoiceElement::new(
                Literal::from(pred("p", &["X"])),
                Condition::new([Literal::from(pred("q", &["X"]))]),
            ),
            ChoiceElement::new(
                Literal::from(pred("s", &["Y"])),
                Condition::new([Literal::from(pred("t", &["Y"]))]),
            ),
        ],
        None,
    ));
    assert!(unbound_of(choice_two.when(pred("r", &[]))).is_empty());
}

#[test]
fn safety_flags_the_unsafe_side_of_every_head_form() {
    // { p(X) : q(X) ; s(X) } :- r.  — cross-element isolation: element 2's s(X) has no condition,
    // and element 1's q(X) is LOCAL to element 1, so it cannot discharge element 2's X. A merged
    // head scope would read this safe (unsound cross-talk); the sound reading flags X.
    let cross_element = Head::Choice(Choice::new(
        None,
        [
            ChoiceElement::new(
                Literal::from(pred("p", &["X"])),
                Condition::new([Literal::from(pred("q", &["X"]))]),
            ),
            ChoiceElement::new(Literal::from(pred("s", &["X"])), Condition::empty()),
        ],
        None,
    ));
    assert_eq!(
        unbound_of(cross_element.when(pred("r", &[]))),
        [named("X")].into_iter().collect(),
    );

    // #count { : a(X) : q(Y) } >= 1 :- r.  — the derived literal's X is unbound (the condition
    // binds Y). A `require`→`bind` slip on the head-aggregate literal would read it safe.
    let aggregate_literal = Head::Aggregate(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            Vec::<Term>::new(),
            Literal::from(pred("a", &["X"])),
            Condition::new([Literal::from(pred("q", &["Y"]))]),
        )],
        Some(Guard {
            relation: Some(Relation::Ge),
            term: num(1),
        }),
    ));
    assert_eq!(
        unbound_of(aggregate_literal.when(pred("r", &[]))),
        [named("X")].into_iter().collect(),
    );

    // #count { Z : a : } >= 1 :- r.  — the element tuple term Z is unbound.
    let aggregate_tuple = Head::Aggregate(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [var("Z")],
            Literal::from(pred("a", &[])),
            Condition::empty(),
        )],
        Some(Guard {
            relation: Some(Relation::Ge),
            term: num(1),
        }),
    ));
    assert_eq!(
        unbound_of(aggregate_tuple.when(pred("r", &[]))),
        [named("Z")].into_iter().collect(),
    );

    // #count { X : a(X) : q(X) } = W :- r.  — the head-aggregate guard W is required, never bound.
    let aggregate_guard = Head::Aggregate(HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [var("X")],
            Literal::from(pred("a", &["X"])),
            Condition::new([Literal::from(pred("q", &["X"]))]),
        )],
        Some(Guard {
            relation: Some(Relation::Eq),
            term: var("W"),
        }),
    ));
    assert_eq!(
        unbound_of(aggregate_guard.when(pred("r", &[]))),
        [named("W")].into_iter().collect(),
    );

    // W { p(X) : q(X) } :- r.  — the choice guard W is required, never bound.
    let choice_guard = Head::Choice(Choice::new(
        Some(Guard {
            relation: Some(Relation::Le),
            term: var("W"),
        }),
        [ChoiceElement::new(
            Literal::from(pred("p", &["X"])),
            Condition::new([Literal::from(pred("q", &["X"]))]),
        )],
        None,
    ));
    assert_eq!(
        unbound_of(choice_guard.when(pred("r", &[]))),
        [named("W")].into_iter().collect(),
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

#[test]
fn finiteness_flags_a_max_over_a_former() {
    // p(X) :- X = #max { f(Y) : p(Y) }.  — the former lives INSIDE the aggregate, and the head
    // is the bare guard `p(X)`. The `#max` returns a *member value-term*, so `f(Y)` makes the
    // guard X one former deeper than the p-members it maxes over: if p holds m then f(m) is a
    // candidate, X = f(m), p(f(m)), f(f(m)), … — unbounded. Distinct from the head-former shape
    // (`p(f(X)) :- X = #max{Y:p(Y)}`) whose growth the head atom carries: here the growth is a
    // former ELEMENT term, which a scan reading only the head atom and body `=` misses (a false
    // `Holds`). The sound reading is Unknown (§6.1).
    let aggregate = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            Some(Guard {
                relation: Some(Relation::Eq),
                term: var("X"),
            }),
            AggregateFunction::Max,
            [BodyAggregateElement::new(
                [function("f", "Y")],
                Condition::new([Literal::from(pred("p", &["Y"]))]),
            )],
            None,
        )),
    };
    let grows = finiteness_of([Statement::Rule(Rule::new(
        pred("p", &["X"]),
        vec![aggregate],
    ))]);
    assert!(
        matches!(grows, Verdict::Unknown { .. }),
        "a #max over a former element term deepens the guard and grows",
    );

    // p(X) :- X = #max { Y : p(Y) }.  — the precision twin: a BARE element term, so X is an
    // existing member value, not deeper. This grounds finitely — `Holds` must be preserved (a
    // conservative `Unknown` here would be sound but needlessly imprecise, and the fix must not
    // reach for it).
    let bounded = BodyElement::Aggregate {
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
    let bounded_verdict =
        finiteness_of([Statement::Rule(Rule::new(pred("p", &["X"]), vec![bounded]))]);
    assert_eq!(
        bounded_verdict,
        Verdict::Holds,
        "a #max over a bare element term is an existing member value and does not grow",
    );
}
