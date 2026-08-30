//! The unpool pass (docs/design/program.md §9): a pool is eliminated before analysis and
//! solve, expanding each node at the level the grounder does (`Statement::unpool`,
//! `libgringo/src/input/statement.cc`) — a top-level literal into separate rules, an
//! element pool within its container. The pass is the estate's mirror of the grounder's
//! unpool-before-simplify order.

use themelios_program::program::{
    Atom, Body, BodyElement, Comparison, Condition, ConditionalLiteral, DefaultNegation,
    Disjunction, DisjunctionElement, External, Head, Literal, LiteralInner, Program, Query,
    Relation, Rule, Statement,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};
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
/// A function application `f(args)`.
fn func(f: &str, arguments: Vec<Term>) -> Term {
    Term::Function {
        name: name(f),
        arguments,
    }
}
/// A term pool `(alt0; alt1; …)`.
fn pool(alternatives: Vec<Term>) -> Term {
    Term::Pool(alternatives)
}
/// A positive body literal over `pred(args)`.
fn body_atom(pred: &str, arguments: Vec<Term>) -> BodyElement {
    BodyElement::from(Atom::new(name(pred), arguments))
}
/// A positive body literal over a comparison.
fn comparison_element(comparison: Comparison) -> BodyElement {
    BodyElement::Literal(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(comparison)),
    })
}
/// `q :- p(<arg>).` unpools to one `q :- p(<image>).` rule per expected argument image — the
/// within-term expansion of a pool nested in a compound argument (§9), verified against clingo.
fn assert_atom_argument_unpools(argument: Term, images: Vec<Term>) {
    let program = program_of([rule("q", vec![body_atom("p", vec![argument])])]);
    let expected = program_of(
        images
            .into_iter()
            .map(|image| rule("q", vec![body_atom("p", vec![image])])),
    );
    assert_eq!(unpool(&program), expected);
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
fn unpool_is_linear_in_a_long_pool_free_body() {
    // The cross-product's growing-prefix clone was O(N²) on a pool-free body of N literals — the
    // quadratic the analysis tier's linear scaling forbids (analysis §8). This fixed large body is
    // an absolute tripwire: unpool must *finish* (the pool-free position extends in place, O(N)). §9.
    const N: i32 = 50_000;
    let body: Vec<BodyElement> = (0..N)
        .map(|i| BodyElement::from(Atom::new(name("p"), [num(i)])))
        .collect();
    let program = program_of([rule("q", body)]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        1,
        "a pool-free body unpools to the one rule"
    );
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

// A pool nested inside a compound term expands at that node, cross-producting the pooled
// positions (`Term::unpool`, `libgringo/src/input/term.cc`) — one image per choice. Each arm
// of `unpool_term` (§9) is verified against clingo's grounding of the corresponding surface.

#[test]
fn a_pool_inside_a_function_term_expands_the_atom() {
    // p(f(0; 1))  ⟹  p(f(0)), p(f(1))   (clingo: p(f(0)). p(f(1)).)
    assert_atom_argument_unpools(
        func("f", vec![pool(vec![num(0), num(1)])]),
        vec![func("f", vec![num(0)]), func("f", vec![num(1)])],
    );
}

#[test]
fn a_pool_inside_a_tuple_term_expands_the_atom() {
    // p((f(0; 1), 9))  ⟹  p((f(0), 9)), p((f(1), 9))   (clingo: p((f(a),c)). p((f(b),c)).)
    let tuple = |first: Term| Term::Tuple(vec![first, num(9)]);
    assert_atom_argument_unpools(
        tuple(func("f", vec![pool(vec![num(0), num(1)])])),
        vec![
            tuple(func("f", vec![num(0)])),
            tuple(func("f", vec![num(1)])),
        ],
    );
}

#[test]
fn a_pool_in_a_binary_operation_expands_both_alternatives() {
    // p((X; Y) + 1)  ⟹  p(X + 1), p(Y + 1)   (unpool splits the pool; it does not evaluate, §3.5)
    let add = |left: Term| Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(num(1)),
    };
    assert_atom_argument_unpools(
        add(pool(vec![var("X"), var("Y")])),
        vec![add(var("X")), add(var("Y"))],
    );
}

#[test]
fn a_pool_in_an_interval_bound_expands_the_interval() {
    // p((0; 5) .. 9)  ⟹  p(0 .. 9), p(5 .. 9)   (the interval is expanded at grounding, not here)
    let interval = |lower: Term| Term::Interval {
        lower: Box::new(lower),
        upper: Box::new(num(9)),
    };
    assert_atom_argument_unpools(
        interval(pool(vec![num(0), num(5)])),
        vec![interval(num(0)), interval(num(5))],
    );
}

#[test]
fn a_pool_under_a_prefix_operator_expands() {
    // p(-(X; Y))  ⟹  p(-X), p(-Y)   (clingo: p(-a). p(-b). for -(a;b))
    let neg = |argument: Term| Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(argument),
    };
    assert_atom_argument_unpools(
        neg(pool(vec![var("X"), var("Y")])),
        vec![neg(var("X")), neg(var("Y"))],
    );
}

#[test]
fn a_pool_inside_an_absolute_value_expands() {
    // p(|(X; Y)|)  ⟹  p(|X|), p(|Y|)
    let abs = |inner: Term| Term::Absolute(Box::new(inner));
    assert_atom_argument_unpools(
        abs(pool(vec![var("X"), var("Y")])),
        vec![abs(var("X")), abs(var("Y"))],
    );
}

#[test]
fn a_pool_inside_an_external_call_expands() {
    // p(@f(0; 1))  ⟹  p(@f(0)), p(@f(1))   (the @-call is left unevaluated (§3.5); unpool still
    // splits a pool in its arguments, exactly as for an ordinary functor)
    let ext = |argument: Term| Term::External {
        name: name("f"),
        arguments: vec![argument],
    };
    assert_atom_argument_unpools(
        ext(pool(vec![num(0), num(1)])),
        vec![ext(num(0)), ext(num(1))],
    );
}

#[test]
fn a_pooled_comparison_chain_expands_into_separate_rules() {
    // q :- 0 < (1; 9) < 3.  ⟹  q :- 0 < 1 < 3.   q :- 0 < 9 < 3.   (clingo agrees: 0 < 1 < 3 fires q)
    let chain =
        |middle: Term| Comparison::new(num(0), Relation::Lt, middle).chain(Relation::Lt, num(3));
    let program = program_of([rule(
        "q",
        vec![comparison_element(chain(pool(vec![num(1), num(9)])))],
    )]);
    let expected = program_of([
        rule("q", vec![comparison_element(chain(num(1)))]),
        rule("q", vec![comparison_element(chain(num(9)))]),
    ]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_query_atom_argument_pool_expands_into_separate_queries() {
    // ? p(0; 1)  ⟹  ? p(0)   ? p(1)   (the query atom's argument-list pool)
    let program = program_of_statements([Statement::Query(Query::new(Atom::pooled(
        name("p"),
        [vec![num(0)], vec![num(1)]],
    )))]);
    let expected = program_of_statements([
        Statement::Query(Query::new(Atom::new(name("p"), [num(0)]))),
        Statement::Query(Query::new(Atom::new(name("p"), [num(1)]))),
    ]);
    assert_eq!(unpool(&program), expected);
}
