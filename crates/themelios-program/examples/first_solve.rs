//! The *first-solve* construction witness (docs/design/program.md §7.3, §16): one
//! small reachability program built two ways — through the declarative surface a
//! logician writes, and through the primitive constructors a generator or a language
//! model targets — and shown to be one and the same value. Run it with
//! `cargo run --example first_solve`.
//!
//! This is the construction half of *first-solve*. The macro-versus-constructor half
//! arrives with the macro surface (a later tier), and the ground-and-read-back half at
//! the solve tier; together they hold "written as ASP", "declared in Rust", and
//! "assembled node by node" to be one program.

use themelios_program::construct::not;
use themelios_program::program::{
    Atom, Body, BodyElement, DefaultNegation, Head, IntoHead, Literal, LiteralInner, Program, Rule,
    Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, Variable};

fn main() {
    let declarative = through_the_surface();
    let primitive = through_the_primitives();

    assert_eq!(
        declarative, primitive,
        "the declarative surface and the primitive constructors build one program",
    );

    println!(
        "one program, two audiences: {} statements, structurally equal.",
        declarative.base().statements().count(),
    );
}

/// The program as a logician declares it — the shape of each expression mirrors the
/// shape of the rule:
///
/// ```text
/// edge(1, 2).
/// reach(X, Y) :- edge(X, Y).
/// reach(X, Z) :- reach(X, Y), edge(Y, Z).
/// step(X, X + 1) :- edge(X, Y).
/// :- not edge(1, 2).
/// ```
fn through_the_surface() -> Program {
    let edge = Rule::fact(Atom::new(name("edge"), [Term::from(1), Term::from(2)]));

    let reach_from_edge = Atom::new(name("reach"), [var("X"), var("Y")])
        .into_head()
        .when(Atom::new(name("edge"), [var("X"), var("Y")]));

    let reach_transitively = Atom::new(name("reach"), [var("X"), var("Z")])
        .into_head()
        .when([
            Atom::new(name("reach"), [var("X"), var("Y")]),
            Atom::new(name("edge"), [var("Y"), var("Z")]),
        ]);

    let step = Atom::new(name("step"), [var("X"), var("X") + 1])
        .into_head()
        .when(Atom::new(name("edge"), [var("X"), var("Y")]));

    let edge_must_hold =
        Rule::constraint(not(Atom::new(name("edge"), [Term::from(1), Term::from(2)])));

    program_of([
        edge,
        reach_from_edge,
        reach_transitively,
        step,
        edge_must_hold,
    ])
}

/// The same program as a generator assembles it — explicit heads, bodies, and literals
/// over the typed algebra. It reaches the identical value: both doors validate through
/// the one ingest authority (§6.3, §7.3).
fn through_the_primitives() -> Program {
    let edge = Rule::new(head("edge", vec![num(1), num(2)]), Body::empty());

    let reach_from_edge = Rule::new(
        head("reach", vec![var("X"), var("Y")]),
        Body::new([body_atom("edge", vec![var("X"), var("Y")])]),
    );

    let reach_transitively = Rule::new(
        head("reach", vec![var("X"), var("Z")]),
        Body::new([
            body_atom("reach", vec![var("X"), var("Y")]),
            body_atom("edge", vec![var("Y"), var("Z")]),
        ]),
    );

    let step = Rule::new(
        head("step", vec![var("X"), add(var("X"), num(1))]),
        Body::new([body_atom("edge", vec![var("X"), var("Y")])]),
    );

    let edge_must_hold = Rule::new(
        Head::Falsum,
        Body::new([BodyElement::Literal(Literal {
            negation: DefaultNegation::Not,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(
                "edge",
                vec![num(1), num(2)],
            ))),
        })]),
    );

    program_of([
        edge,
        reach_from_edge,
        reach_transitively,
        step,
        edge_must_hold,
    ])
}

// ---- construction helpers ----

fn program_of(rules: [Rule; 5]) -> Program {
    Program::of(rules.map(|rule| WithProvenance::constructed(Statement::Rule(rule))))
}

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

fn add(left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn atom(predicate: &str, arguments: Vec<Term>) -> Atom {
    Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments,
    }
}

fn head(predicate: &str, arguments: Vec<Term>) -> Head {
    Head::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom(predicate, arguments))),
    })
}

fn body_atom(predicate: &str, arguments: Vec<Term>) -> BodyElement {
    BodyElement::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom(predicate, arguments))),
    })
}
