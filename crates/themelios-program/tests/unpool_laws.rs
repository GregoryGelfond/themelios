//! The unpool pass (docs/design/program.md §9): a pool is eliminated before analysis and
//! solve, expanding each node at the level the grounder does (`Statement::unpool`,
//! `libgringo/src/input/statement.cc`) — a top-level literal into separate rules, an
//! element pool within its container. The pass is the estate's mirror of the grounder's
//! unpool-before-simplify order.

use themelios_program::program::{Atom, Body, BodyElement, Program, Rule, Statement};
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
