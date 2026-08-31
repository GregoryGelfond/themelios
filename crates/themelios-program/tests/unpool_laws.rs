//! The unpool pass (docs/design/program.md §9): a pool is eliminated before analysis and
//! solve, expanding each node at the level the grounder does (`Statement::unpool`,
//! `libgringo/src/input/statement.cc`) — a top-level literal into separate rules, an
//! element pool within its container. The pass is the estate's mirror of the grounder's
//! unpool-before-simplify order.

use themelios_base::source::{Source, SourceId};
use themelios_program::program::PartKey;
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, DefaultNegation, Direction,
    Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard, Head, HeadAggregate,
    HeadAggregateElement, Heuristic, Literal, LiteralInner, Optimize, OptimizeElement, Program,
    Project, Query, Relation, Rule, SetAggregate, SetElement, Show, Statement, WeakConstraint,
    weight,
};
use themelios_program::provenance::{Origin, TransformTag, WithProvenance};
use themelios_program::raise::raise;
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};
use themelios_program::transform::unpool;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

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
        inner: LiteralInner::Atom(WithProvenance::constructed(
            Atom::pooled(name(pred), alternatives).expect("a non-empty pool"),
        )),
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
/// `f(f(… bottom …))` — `depth` unary `f`-applications over `bottom`, a deep spine.
fn spine(bottom: Term, depth: usize) -> Term {
    let mut term = bottom;
    for _ in 0..depth {
        term = func("f", vec![term]);
    }
    term
}
/// `(…((0; 1); 2); …; n)` — a left-nested pool chain of `n` alternatives (`n >= 2`): the shape a
/// nested parenthesization `(…((0; 1); 2)…)` or a construction builds (a bare `0; 1; …; n` parses
/// *flat*, one pool of `n` tuples). Pooling is associative, so it flattens to the flat pool
/// `(0; 1; …; n)`.
fn nested_pool(alternatives: i32) -> Term {
    let mut term = Term::Pool(vec![num(0), num(1)]);
    for alternative in 2..alternatives {
        term = Term::Pool(vec![term, num(alternative)]);
    }
    term
}
/// `(0; (1; (2; …; n)))` — a right-nested pool chain of `n` alternatives (`n >= 2`), the mirror of
/// [`nested_pool`]. Also associative, so it flattens to the same flat pool `(0; 1; …; n)`; its
/// adversary is the *other* nesting direction, the one a flatten that reused the first nested
/// pool's vector made O(N²) (each level's leading leaf re-collected into the growing image).
fn right_nested_pool(alternatives: i32) -> Term {
    let mut term = Term::Pool(vec![num(alternatives - 2), num(alternatives - 1)]);
    for alternative in (0..alternatives - 2).rev() {
        term = Term::Pool(vec![num(alternative), term]);
    }
    term
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
    let program = program_of_statements([external(
        Atom::pooled(name("p"), [vec![num(0)], vec![num(1)]]).expect("a non-empty pool"),
    )]);
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
fn a_disjunction_literal_pool_is_left_pooled() {
    // p(0; 1) | q.  — clingo grounds the pooled disjunct as the conjunctive head group
    // (p(0) ∧ p(1)) ∨ q, which a single-literal `DisjunctionElement` cannot hold; the statement
    // split that would realise it (p(0)|q. p(1)|q.) is model-correct but exponential over K pooled
    // disjuncts, so `unpool` LEAVES the pool — a representation gap read per-alternative by analysis,
    // exactly like a pooled literal in a lone body conditional. The pass makes no change here; the
    // solve bridge maps the pool to the grounder's disjunction directly.
    let program = program_of_statements([Statement::Rule(disjunctive_rule(vec![
        DisjunctionElement::new(
            pooled_lit("p", vec![vec![num(0)], vec![num(1)]]),
            Condition::empty(),
        ),
        DisjunctionElement::new(lit("q", vec![]), Condition::empty()),
    ]))]);
    assert_eq!(
        unpool(&program),
        program,
        "a pooled disjunct literal is left pooled"
    );
}

#[test]
fn a_disjunction_condition_pool_expands_within_the_disjunction() {
    // p | q : c(0; 1).  ⟹  p | q : c(0) | q : c(1).  (one rule — a pooled element condition
    // expands into more elements, the grounder's element-condition unpool; only a pooled disjunct
    // literal splits the statement.)
    let program = program_of_statements([Statement::Rule(disjunctive_rule(vec![
        DisjunctionElement::new(lit("p", vec![]), Condition::empty()),
        DisjunctionElement::new(
            lit("q", vec![]),
            Condition::new([lit("c", vec![Term::Pool(vec![num(0), num(1)])])]),
        ),
    ]))]);
    let expected = program_of_statements([Statement::Rule(disjunctive_rule(vec![
        DisjunctionElement::new(lit("p", vec![]), Condition::empty()),
        DisjunctionElement::new(lit("q", vec![]), Condition::new([lit("c", vec![num(0)])])),
        DisjunctionElement::new(lit("q", vec![]), Condition::new([lit("c", vec![num(1)])])),
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
    let pooled = Atom::pooled(name("p"), [vec![var("X")], vec![num(0)]]).expect("a non-empty pool");
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
    let program = program_of_statements([Statement::Query(Query::new(
        Atom::pooled(name("p"), [vec![num(0)], vec![num(1)]]).expect("a non-empty pool"),
    ))]);
    let expected = program_of_statements([
        Statement::Query(Query::new(Atom::new(name("p"), [num(0)]))),
        Statement::Query(Query::new(Atom::new(name("p"), [num(1)]))),
    ]);
    assert_eq!(unpool(&program), expected);
}

// ---- A pool in every container and directive position (§3, §9) ----
//
// The grounder expands an element pool *within* its container (one statement, more elements) and a
// guard/directive/tuple pool into *separate* statements (the statement product). Each row of the
// §3 table is pinned here against clingo's own reading: the within cases by the expanded value, the
// statement-product cases by the count a residual pool could not reach (a left pool stays one
// statement). These also exercise every `unpool_*` container and directive arm directly, rather
// than only through the differential.

#[test]
fn a_body_aggregate_element_term_pool_expands_within_the_aggregate() {
    // q :- #sum{ (0; 1) }.  ⟹  q :- #sum{ 0; 1 }.  (one aggregate, two elements)
    let aggregate = |elements: Vec<BodyAggregateElement>| BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            None,
            AggregateFunction::Sum,
            elements,
            None,
        )),
    };
    let pooled = aggregate(vec![BodyAggregateElement::new(
        [pool(vec![num(0), num(1)])],
        Condition::empty(),
    )]);
    let expanded = aggregate(vec![
        BodyAggregateElement::new([num(0)], Condition::empty()),
        BodyAggregateElement::new([num(1)], Condition::empty()),
    ]);
    assert_eq!(
        unpool(&program_of([rule("q", vec![pooled])])),
        program_of([rule("q", vec![expanded])])
    );
}

#[test]
fn a_body_aggregate_guard_pool_expands_into_separate_rules() {
    // q :- (0; 1) <= #count{ 0 }.  ⟹  two rules (the guard multiplies the aggregate)
    let aggregate = |bound: Term| BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            Some(Guard {
                relation: Some(Relation::Le),
                term: bound,
            }),
            AggregateFunction::Count,
            [BodyAggregateElement::new([num(0)], Condition::empty())],
            None,
        )),
    };
    let program = program_of([rule("q", vec![aggregate(pool(vec![num(0), num(1)]))])]);
    let expected = program_of([
        rule("q", vec![aggregate(num(0))]),
        rule("q", vec![aggregate(num(1))]),
    ]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_set_aggregate_element_pool_expands_within_the_aggregate() {
    // q :- #count{ p(0; 1) }.  ⟹  q :- #count{ p(0); p(1) }.  (one aggregate, two elements)
    let aggregate = |elements: Vec<SetElement>| BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Set(SetAggregate::new(None, elements, None)),
    };
    let pooled = aggregate(vec![SetElement::Literal(pooled_lit(
        "p",
        vec![vec![num(0)], vec![num(1)]],
    ))]);
    let expanded = aggregate(vec![
        SetElement::Literal(lit("p", vec![num(0)])),
        SetElement::Literal(lit("p", vec![num(1)])),
    ]);
    assert_eq!(
        unpool(&program_of([rule("q", vec![pooled])])),
        program_of([rule("q", vec![expanded])])
    );
}

#[test]
fn a_choice_element_pool_expands_within_the_choice() {
    // { p(0; 1) }.  ⟹  { p(0); p(1) }.  (one choice, two elements)
    let choice = |elements: Vec<ChoiceElement>| {
        Rule::new(
            Head::Choice(Choice::new(None, elements, None)),
            Body::empty(),
        )
    };
    let pooled = choice(vec![ChoiceElement::new(
        pooled_lit("p", vec![vec![num(0)], vec![num(1)]]),
        Condition::empty(),
    )]);
    let expanded = choice(vec![
        ChoiceElement::new(lit("p", vec![num(0)]), Condition::empty()),
        ChoiceElement::new(lit("p", vec![num(1)]), Condition::empty()),
    ]);
    assert_eq!(unpool(&program_of([pooled])), program_of([expanded]));
}

#[test]
fn a_head_aggregate_element_pool_expands_within_the_aggregate() {
    // #sum{ (0; 1) : p }.  ⟹  #sum{ 0 : p; 1 : p }.  (one aggregate, two elements)
    let head = |elements: Vec<HeadAggregateElement>| {
        Rule::new(
            Head::Aggregate(HeadAggregate::new(
                None,
                AggregateFunction::Sum,
                elements,
                None,
            )),
            Body::empty(),
        )
    };
    let element = |t: Term| HeadAggregateElement::new([t], lit("p", vec![]), Condition::empty());
    let pooled = head(vec![HeadAggregateElement::new(
        [pool(vec![num(0), num(1)])],
        lit("p", vec![]),
        Condition::empty(),
    )]);
    let expanded = head(vec![element(num(0)), element(num(1))]);
    assert_eq!(unpool(&program_of([pooled])), program_of([expanded]));
}

#[test]
fn an_optimize_element_term_pool_expands_within_the_statement() {
    // #minimize{ 1@1, (0; 1) }.  ⟹  #minimize{ 1@1, 0; 1@1, 1 }.  (one statement, two elements)
    let optimize = |elements: Vec<OptimizeElement>| {
        Statement::Optimize(Optimize::new(Direction::Minimize, elements))
    };
    let element =
        |t: Term| OptimizeElement::new(weight(num(1)).at_priority(num(1)), [t], Condition::empty());
    let pooled = optimize(vec![OptimizeElement::new(
        weight(num(1)).at_priority(num(1)),
        [pool(vec![num(0), num(1)])],
        Condition::empty(),
    )]);
    let expanded = optimize(vec![element(num(0)), element(num(1))]);
    assert_eq!(
        unpool(&program_of_statements([pooled])),
        program_of_statements([expanded])
    );
}

#[test]
fn a_weak_constraint_tuple_pool_expands_into_separate_constraints() {
    // :~ q. [1@1, (0; 1)]  ⟹  two weak constraints (the tuple multiplies)
    let weak = |t: Term| {
        Statement::WeakConstraint(WeakConstraint::new(
            Body::new([body_atom("q", vec![])]),
            weight(num(1)).at_priority(num(1)),
            [t],
        ))
    };
    let program = program_of_statements([weak(pool(vec![num(0), num(1)]))]);
    let expected = program_of_statements([weak(num(0)), weak(num(1))]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn an_edge_endpoint_pool_expands_into_separate_edges() {
    // #edge (0; 1), 2.  ⟹  two edges (the endpoint multiplies)
    let edge = |from: Term| Statement::Edge(Edge::new([(from, num(2))], Body::empty()));
    let program = program_of_statements([edge(pool(vec![num(0), num(1)]))]);
    let expected = program_of_statements([edge(num(0)), edge(num(1))]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_heuristic_bias_pool_expands_into_separate_heuristics() {
    // #heuristic p. [(0; 1)@2, sign]  ⟹  two heuristics (the bias multiplies)
    let heuristic = |bias: Term| {
        Statement::Heuristic(Heuristic::new(
            Atom::new(name("p"), []),
            Body::empty(),
            bias,
            Some(num(2)),
            func("sign", vec![]),
        ))
    };
    let program = program_of_statements([heuristic(pool(vec![num(0), num(1)]))]);
    let expected = program_of_statements([heuristic(num(0)), heuristic(num(1))]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_project_atom_pool_expands_into_separate_projects() {
    // #project p(0; 1).  ⟹  two projects (the atom multiplies)
    let project = |atom: Atom| Statement::Project(Project::atom_body(atom, Body::empty()));
    let program = program_of_statements([project(
        Atom::pooled(name("p"), [vec![num(0)], vec![num(1)]]).expect("a non-empty pool"),
    )]);
    let expected = program_of_statements([
        project(Atom::new(name("p"), [num(0)])),
        project(Atom::new(name("p"), [num(1)])),
    ]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_show_term_pool_expands_into_separate_shows() {
    // #show (0; 1).  ⟹  two show statements (the term multiplies)
    let program = program_of_statements([Statement::Show(Show::Term(pool(vec![num(0), num(1)])))]);
    let expected = program_of_statements([
        Statement::Show(Show::Term(num(0))),
        Statement::Show(Show::Term(num(1))),
    ]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn an_external_value_pool_expands_into_separate_directives() {
    // #external p. [(0; 1)]  ⟹  two externals (the value multiplies)
    let ext = |value: Term| {
        Statement::External(External::new(
            Atom::new(name("p"), []),
            Body::empty(),
            Some(value),
        ))
    };
    let program = program_of_statements([ext(pool(vec![num(0), num(1)]))]);
    let expected = program_of_statements([ext(num(0)), ext(num(1))]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn a_set_aggregate_conditional_literal_pool_expands_within_the_aggregate() {
    // q :- #count{ p(0; 1) : c }.  ⟹  q :- #count{ p(0) : c; p(1) : c }.  (conditional literal
    // pool, within the aggregate — the pooled derived literal becomes more elements)
    let aggregate = |elements: Vec<SetElement>| BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Set(SetAggregate::new(None, elements, None)),
    };
    let condition = || Condition::new([lit("c", vec![])]);
    let pooled = aggregate(vec![SetElement::ConditionalLiteral(ConditionalLiteral {
        literal: pooled_lit("p", vec![vec![num(0)], vec![num(1)]]),
        condition: condition(),
    })]);
    let expanded = aggregate(vec![
        SetElement::ConditionalLiteral(ConditionalLiteral {
            literal: lit("p", vec![num(0)]),
            condition: condition(),
        }),
        SetElement::ConditionalLiteral(ConditionalLiteral {
            literal: lit("p", vec![num(1)]),
            condition: condition(),
        }),
    ]);
    assert_eq!(
        unpool(&program_of([rule("q", vec![pooled])])),
        program_of([rule("q", vec![expanded])])
    );
}

#[test]
fn a_show_term_body_term_pool_expands_into_separate_shows() {
    // #show (0; 1) : b.  ⟹  two show statements (the shown term multiplies over the body form)
    let show = |t: Term| Statement::Show(Show::term_body(t, Body::new([body_atom("b", vec![])])));
    let program = program_of_statements([show(pool(vec![num(0), num(1)]))]);
    let expected = program_of_statements([show(num(0)), show(num(1))]);
    assert_eq!(unpool(&program), expected);
}

#[test]
fn unpool_of_a_pool_free_program_returns_it_unchanged() {
    // Nearly every program carries no pool; `unpool` returns it unchanged (the short-circuit), so
    // the analysis and solve tiers meet the very same program and its provenance is untouched — a
    // statement `unpool` did not transform gains no `unpool` origin. Value equality is what the
    // downstream tiers read; the origin is checked below. §9
    let program = program_of([
        rule("q", vec![body_atom("p", vec![num(0), var("X")])]),
        rule("r", vec![body_atom("p", vec![func("f", vec![var("Y")])])]),
    ]);
    let unpooled = unpool(&program);
    assert_eq!(unpooled, program, "a pool-free program is unchanged");
    for statement in unpooled.base().statements() {
        assert!(
            !statement
                .provenance()
                .origins()
                .any(|origin| *origin == Origin::Transformed(TransformTag::new("unpool"))),
            "a statement unpool did not transform carries no unpool origin"
        );
    }
}

#[test]
fn unpool_preserves_the_part_a_pooled_statement_lives_in() {
    // A pool in a `#program` part expands *within that part*; `unpool` must not collapse the part
    // structure into `base` (a multi-shot consumer reads the parts, §4.1). §9
    let source = Source::new(SourceId::new(0), "a. #program acid. p(0; 1).".to_string())
        .expect("the text admits");
    let program = raise(&parse(&source, Dialect::Clingo)).into_program();
    let unpooled = unpool(&program);
    let acid = PartKey {
        name: name("acid"),
        formals: Vec::new(),
    };
    assert_eq!(
        unpooled
            .part(&acid)
            .map_or(0, |part| part.statements().count()),
        2,
        "p(0; 1) unpools to two statements, both in the `acid` part"
    );
    assert_eq!(
        unpooled.base().statements().count(),
        1,
        "the base fact stays in base, not joined by the acid part's expansion"
    );
}

#[test]
fn a_lone_body_conditional_with_a_pooled_literal_stays_pooled() {
    // r :- p(X; a) : q.  — a pooled derived literal in a lone body conditional would need a
    // disjunctive clause a single-literal `ConditionalLiteral` cannot hold, so `unpool` leaves it
    // pooled (a representation gap, recorded like the theory deferral §7); safety then fails closed
    // on the pooled atom (analysis §5). The pass makes no change here.
    let conditional = ConditionalLiteral {
        literal: pooled_lit("p", vec![vec![var("X")], vec![func("a", vec![])]]),
        condition: Condition::new([lit("q", vec![])]),
    };
    let program = program_of([rule("r", vec![BodyElement::Conditional(conditional)])]);
    assert_eq!(
        unpool(&program),
        program,
        "the lone-conditional pooled literal is left unchanged"
    );
}

// ---- Scaling tripwires (§9, §13; spec §12.4) ----
//
// `unpool_term` folds a whole term when it carries a pool anywhere, and the cross-product that
// assembles each level must not re-clone the growing structure — cloning it at each of a spine's
// levels is `O(depth²)`, and at each of a wide body's positions `O(N²)`, the superlinear cost the
// analysis tier's linear scaling forbids. These are absolute tripwires at a depth/width far past
// any the differential's corpus exercises: a regression to a growing-clone would make `unpool`
// hang here rather than finish in milliseconds. The genuinely exponential shape (a many-pool
// cross-product) is an output-size fact, and its own tripwire pins that the attributed `2^K` is
// the only growth.

#[test]
fn unpool_is_linear_in_a_deep_spine_over_a_bottom_pool() {
    // `q :- p(f(f(… (0; 1) …))).` — a pool at the bottom of a deep unary spine (the bottom pool
    // keeps the spine non-ground, so it does not collapse to a symbol). `unpool_term` folds the
    // whole spine; the cross-product moves each of the two alternatives up the spine rather than
    // cloning the growing child image, so this is O(depth) and finishes. §9
    const DEPTH: usize = 100_000;
    let program = program_of([rule(
        "q",
        vec![body_atom(
            "p",
            vec![spine(pool(vec![num(0), num(1)]), DEPTH)],
        )],
    )]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        2,
        "a 2-alternative bottom pool unpools to two rules, whatever the spine depth"
    );
}

#[test]
fn unpool_is_linear_in_a_deep_pool_free_spine_within_a_pool() {
    // `q :- p((f(f(… X …)) ; d)).` — a deep NON-ground pool-free spine (a variable at the bottom,
    // so it is not collapsed to a symbol) as one alternative of a top pool. The spine's nodes take
    // the cross-product's single-alternative path; moving the single image up the spine, rather
    // than cloning it, keeps this O(depth), not O(depth²). §9, §13
    const DEPTH: usize = 100_000;
    let argument = pool(vec![spine(var("X"), DEPTH), func("d", vec![])]);
    let program = program_of([rule("q", vec![body_atom("p", vec![argument])])]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        2,
        "the two top alternatives unpool to two rules, whatever the spine depth"
    );
}

#[test]
fn unpool_is_linear_in_a_wide_pool_free_body_beside_a_pool() {
    // A wide pool-free body (N literals) beside one pooled literal: the pool defeats any
    // program-level short-circuit, so the full pass runs and the N pool-free positions extend the
    // cross-product's one combo in place — O(N), not the O(N²) a growing-prefix clone would pay
    // (analysis §8). The pooled literal splits the body into two rules. §9
    const N: i32 = 50_000;
    let mut body: Vec<BodyElement> = (0..N)
        .map(|i| BodyElement::from(Atom::new(name("p"), [num(i)])))
        .collect();
    body.push(BodyElement::from(
        Atom::pooled(name("s"), [vec![num(0)], vec![num(1)]]).expect("a non-empty pool"),
    ));
    let program = program_of([rule("q", body)]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        2,
        "one 2-alternative pool splits the wide body into two rules"
    );
}

#[test]
fn unpool_is_linear_in_a_deeply_nested_pool_chain() {
    // `q :- p((…((0; 1); 2); …; N)).` — a left-nested pool chain. Canonicalization flattens it (a
    // pool is associative, `((a; b); c)` is `(a; b; c)`, verified against clingo) in one top-down
    // pass, O(N) whichever way it nests; `unpool_term` then meets a flat pool and produces the N
    // distinct atoms. An absolute tripwire: a growing per-level re-collect would hang here. §9
    const N: i32 = 40_000;
    let program = program_of([rule("q", vec![body_atom("p", vec![nested_pool(N)])])]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        N as usize,
        "a nested pool of N alternatives flattens and unpools to N distinct rules"
    );
}

#[test]
fn unpool_is_linear_in_a_deeply_right_nested_pool_chain() {
    // `q :- p((0; (1; …; N))).` — the right-nested mirror of the chain above, the shape that caught
    // the reuse-the-first-vector flatten: O(N) left-nested but O(N²) right-nested, since each level's
    // leading leaf prepended into the growing image. The single top-down `flatten_pools` gathers the
    // whole spine in one pass, O(N) in *this* direction too. An absolute tripwire: the O(N²) flatten
    // would hang at this N, where the left-nested twin above passed. §9
    const N: i32 = 40_000;
    let program = program_of([rule("q", vec![body_atom("p", vec![right_nested_pool(N)])])]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        N as usize,
        "a right-nested pool of N alternatives flattens and unpools to N distinct rules"
    );
}

#[test]
fn unpool_output_is_the_attributed_two_to_the_k_and_no_more() {
    // K pooled term positions in one atom, `p((0;1), (0;1), … , (0;1))`, ground to the 2^K distinct
    // tuples — the genuine cross-product, exponential in the number of pooled positions (an
    // output-size fact, §9, like `substitute`). This pins that the attributed 2^K is the *only*
    // growth: exactly 2^K distinct rules, none dropped and none double-counted.
    const K: usize = 16;
    let positions: Vec<Term> = (0..K).map(|_| pool(vec![num(0), num(1)])).collect();
    let program = program_of([rule("q", vec![body_atom("p", positions)])]);
    let unpooled = unpool(&program);
    assert_eq!(
        unpooled.base().statements().count(),
        1usize << K,
        "K pooled positions ground to exactly 2^K distinct rules"
    );
}
