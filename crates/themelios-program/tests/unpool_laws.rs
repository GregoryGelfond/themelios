//! The unpool pass (docs/design/program.md §9): a pool is eliminated before analysis and
//! solve, expanding each node at the level the grounder does (`Statement::unpool`,
//! `libgringo/src/input/statement.cc`) — a top-level literal into separate rules, an
//! element pool within its container. The pass is the estate's mirror of the grounder's
//! unpool-before-simplify order.

use themelios_program::program::{
    Atom, Body, BodyElement, Condition, ConditionalLiteral, DefaultNegation, Disjunction,
    DisjunctionElement, External, Head, Literal, LiteralInner, Program, Rule, Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::transform::unpool;

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
fn program_of(rules: impl IntoIterator<Item = Rule>) -> Program {
    Program::of(
        rules
            .into_iter()
            .map(|r| WithProvenance::constructed(Statement::Rule(r))),
    )
}
/// `head :- body.` with a nullary head predicate.
fn rule(head: &str, body: Vec<BodyElement>) -> Rule {
    Rule::new(Atom::constant(name(head)), Body::new(body))
}
/// A positive literal over `pred(args)` (a `Single` atom).
fn lit(pred: &str, args: Vec<Term>) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(Atom::new(name(pred), args))),
    }
}
/// A positive literal over the argument-list pool `pred(alt0; alt1; …)`.
fn pooled_lit(pred: &str, alternatives: Vec<Vec<Term>>) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(Atom::pooled(
            name(pred),
            alternatives,
        ))),
    }
}
fn external(atom: Atom) -> Statement {
    Statement::External(External::new(atom, Body::empty(), None))
}
fn disjunctive_rule(elements: Vec<DisjunctionElement>) -> Rule {
    Rule::new(Head::Disjunction(Disjunction::new(elements)), Body::empty())
}
fn program_of_statements(statements: impl IntoIterator<Item = Statement>) -> Program {
    Program::of(statements.into_iter().map(WithProvenance::constructed))
}

#[test]
fn an_external_directive_atom_pool_expands_into_separate_directives() {
    // #external p(0; 1).  ⟹  #external p(0).  #external p(1).
    let program = program_of_statements([external(Atom::pooled(
        name("p"),
        [vec![num(0)], vec![num(1)]],
    ))]);
    let expected = program_of_statements([
        external(Atom::new(name("p"), [num(0)])),
        external(Atom::new(name("p"), [num(1)])),
    ]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_body_conditional_condition_pool_expands_conjunctively_in_one_body() {
    // q :- t : c(0; 1).  ⟹  q :- t : c(0), t : c(1).  (both conditionals, one body — verified)
    let condition = Condition::new([lit("c", vec![Term::Pool(vec![num(0), num(1)])])]);
    let conditional = ConditionalLiteral {
        literal: lit("t", vec![]),
        condition,
    };
    let program = program_of([rule("q", vec![BodyElement::Conditional(conditional)])]);

    let expected = program_of([rule(
        "q",
        vec![
            BodyElement::Conditional(ConditionalLiteral {
                literal: lit("t", vec![]),
                condition: Condition::new([lit("c", vec![num(0)])]),
            }),
            BodyElement::Conditional(ConditionalLiteral {
                literal: lit("t", vec![]),
                condition: Condition::new([lit("c", vec![num(1)])]),
            }),
        ],
    )]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_disjunction_element_pool_expands_within_the_disjunction() {
    // p(0; 1) | q.  ⟹  p(0) | p(1) | q.  (one rule, three disjuncts)
    let program = program_of_statements([Statement::Rule(disjunctive_rule(vec![
        DisjunctionElement::new(
            pooled_lit("p", vec![vec![num(0)], vec![num(1)]]),
            Condition::empty(),
        ),
        DisjunctionElement::new(lit("q", vec![]), Condition::empty()),
    ]))]);
    let expected = program_of_statements([Statement::Rule(disjunctive_rule(vec![
        DisjunctionElement::new(lit("p", vec![num(0)]), Condition::empty()),
        DisjunctionElement::new(lit("p", vec![num(1)]), Condition::empty()),
        DisjunctionElement::new(lit("q", vec![]), Condition::empty()),
    ]))]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_body_literal_argument_list_pool_expands_into_separate_rules() {
    // q :- p(X; 0).  ⟹  q :- p(X).   and   q :- p(0).   (the grounder's cross-product)
    let pooled = Atom::pooled(name("p"), [vec![var("X")], vec![num(0)]]);
    let program = program_of([rule("q", vec![BodyElement::from(pooled)])]);

    let expected = program_of([
        rule(
            "q",
            vec![BodyElement::from(Atom::new(name("p"), [var("X")]))],
        ),
        rule("q", vec![BodyElement::from(Atom::new(name("p"), [num(0)]))]),
    ]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_term_pool_in_a_body_atom_expands_the_atom_too() {
    // q :- p((X; 0)).  — the pool is a Term::Pool argument — grounds to the same two atoms.
    let term_pool = Term::Pool(vec![var("X"), num(0)]);
    let program = program_of([rule(
        "q",
        vec![BodyElement::from(Atom::new(name("p"), [term_pool]))],
    )]);

    let expected = program_of([
        rule(
            "q",
            vec![BodyElement::from(Atom::new(name("p"), [var("X")]))],
        ),
        rule("q", vec![BodyElement::from(Atom::new(name("p"), [num(0)]))]),
    ]);
    assert_eq!(unpool(&program), expected);
}
