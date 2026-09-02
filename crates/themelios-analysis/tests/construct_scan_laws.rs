//! Laws of the construct scan (docs/design/analysis.md §7): every construct that
//! occurs is flagged and one that does not is not; every flag's `first` names a
//! statement that bears the construct and `all` iterates exactly the used
//! constructs; the scan is provenance-blind (§8); and the scan is total on any
//! program, including one with a term far deeper than the call stack (§13).

use std::collections::BTreeSet;

use themelios_analysis::construct::{Construct, Constructs};
use themelios_program::construct::{minimize, not};
use themelios_program::program::{
    Aggregate, AggregateFunction, Atom, Body, BodyAggregateElement, BodyElement, Choice,
    ChoiceElement, Comparison, Condition, ConditionalLiteral, Const, DefaultNegation, Defined,
    Disjunction, DisjunctionElement, Edge, External, FunctionAggregate, Guard, Head, HeadAggregate,
    HeadAggregateElement, Heuristic, Literal, LiteralInner, OptimizeElement, Program, Project,
    Query, Relation, Rule, SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryElement,
    TheoryOperator, TheoryTerm, WeakConstraint, weight,
};
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
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

fn theory_var(text: &str) -> TheoryTerm {
    TheoryTerm::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable"),
    ))
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn atom(text: &str) -> Atom {
    Atom::constant(name(text))
}

fn comparison_literal(first: Term, relation: Relation, second: Term) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Comparison(WithProvenance::constructed(Comparison::new(
            first, relation, second,
        ))),
    }
}

fn wp(statement: Statement) -> WithProvenance<Statement> {
    WithProvenance::constructed(statement)
}

fn scan_of(statements: impl IntoIterator<Item = Statement>) -> Constructs {
    Constructs::of(&Program::of(statements.into_iter().map(wp)))
}

#[test]
fn an_argument_list_pool_reports_pooling() {
    // `p(a; b)` (an `Arguments::Pooled` atom) is pooling at the atom's own level — the same
    // `Construct::Pool` a term pool `p((a; b))` reports (§0); the faithful scan reports it.
    let scan = scan_of([Statement::Rule(Rule::fact(
        Atom::pooled(name("p"), [vec![num(0)], vec![num(1)]]).expect("a non-empty pool"),
    ))]);
    assert!(
        scan.uses(Construct::Pool),
        "an argument-list pool is pooling"
    );
}

fn guard(relation: Relation, term: Term) -> Guard {
    Guard {
        relation: Some(relation),
        term,
    }
}

// a | b.
fn disjunction() -> Statement {
    Statement::Rule(
        Head::Disjunction(Disjunction::new([
            DisjunctionElement::new(Literal::from(atom("a")), Condition::empty()),
            DisjunctionElement::new(Literal::from(atom("b")), Condition::empty()),
        ]))
        .when(Body::empty()),
    )
}

// { a; b }.
fn choice() -> Statement {
    Statement::Rule(
        Head::Choice(Choice::new(
            None,
            [
                ChoiceElement::new(Literal::from(atom("a")), Condition::empty()),
                ChoiceElement::new(Literal::from(atom("b")), Condition::empty()),
            ],
            None,
        ))
        .when(Body::empty()),
    )
}

// #count { 1 : a } >= 0.
fn head_aggregate() -> Statement {
    Statement::Rule(
        Head::Aggregate(HeadAggregate::new(
            None,
            AggregateFunction::Count,
            [HeadAggregateElement::new(
                [num(1)],
                Literal::from(atom("a")),
                Condition::empty(),
            )],
            Some(guard(Relation::Ge, num(0))),
        ))
        .when(Body::empty()),
    )
}

// :- #count { X : p(X) } >= 1.
fn body_aggregate() -> Statement {
    Statement::Rule(Rule::constraint(vec![BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            None,
            AggregateFunction::Count,
            [BodyAggregateElement::new(
                [var("X")],
                Condition::new([Literal::from(Atom::new(name("p"), [var("X")]))]),
            )],
            Some(guard(Relation::Ge, num(1))),
        )),
    }]))
}

// #minimize { 1@1, 1 : p }.
fn optimization() -> Statement {
    Statement::Optimize(minimize([OptimizeElement::new(
        weight(num(1)).at_priority(num(1)),
        [num(1)],
        Condition::new([Literal::from(atom("p"))]),
    )]))
}

// :~ p. [1@1, 1]
fn weak_constraint() -> Statement {
    Statement::WeakConstraint(WeakConstraint::new(
        Body::new([BodyElement::from(atom("p"))]),
        weight(num(1)).at_priority(num(1)),
        [num(1)],
    ))
}

// #heuristic p. [1, 1]
fn heuristic() -> Statement {
    Statement::Heuristic(Heuristic::new(
        atom("p"),
        Body::empty(),
        num(1),
        None,
        num(1),
    ))
}

// p(@f(1)).
fn external_call() -> Statement {
    Statement::Rule(Rule::fact(Atom::new(
        name("p"),
        [Term::External {
            name: name("f"),
            arguments: vec![num(1)],
        }],
    )))
}

/// A statement using exactly one construct, one per `Construct` — the scan reads
/// each in isolation (below) and all seventeen together (the kitchen sink).
fn one_of_each() -> Vec<(Construct, Statement)> {
    vec![
        (Construct::Disjunction, disjunction()),
        (Construct::Choice, choice()),
        (Construct::HeadAggregate, head_aggregate()),
        (Construct::BodyAggregate, body_aggregate()),
        // -p.
        (
            Construct::StrongNegation,
            Statement::Rule(Rule::fact(-atom("p"))),
        ),
        // :- not p.
        (
            Construct::DefaultNegation,
            Statement::Rule(Rule::constraint(not(atom("p")))),
        ),
        // :- X < 1.
        (
            Construct::Comparison,
            Statement::Rule(Rule::constraint(comparison_literal(
                var("X"),
                Relation::Lt,
                num(1),
            ))),
        ),
        // p(1..3).
        (
            Construct::Interval,
            Statement::Rule(Rule::fact(Atom::new(name("p"), [num(1).to(3)]))),
        ),
        // p((1;2)).
        (
            Construct::Pool,
            Statement::Rule(Rule::fact(Atom::new(
                name("p"),
                [Term::Pool(vec![num(1), num(2)])],
            ))),
        ),
        // p(X + 1).
        (
            Construct::Arithmetic,
            Statement::Rule(Rule::fact(Atom::new(name("p"), [var("X") + num(1)]))),
        ),
        (Construct::Optimization, optimization()),
        (Construct::WeakConstraint, weak_constraint()),
        // &diff { }.
        (
            Construct::TheoryAtom,
            Statement::Rule(
                Head::TheoryAtom(TheoryAtom::new(name("diff"), [], [], None)).when(Body::empty()),
            ),
        ),
        // #external p.
        (
            Construct::ExternalStatement,
            Statement::External(External::new(atom("p"), Body::empty(), None)),
        ),
        (Construct::Heuristic, heuristic()),
        // #edge (1, 2).
        (
            Construct::Edge,
            Statement::Edge(Edge::new([(num(1), num(2))], Body::empty())),
        ),
        (Construct::ExternalCall, external_call()),
    ]
}

// ---- owned plain data (§8) ----

const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<Constructs>();
    assert_send_sync_static::<Construct>();
};

// ---- Law: occurrence ----

#[test]
fn every_construct_that_occurs_is_flagged() {
    let table = one_of_each();
    assert_eq!(table.len(), 17, "the table covers all seventeen constructs");

    for (target, statement) in &table {
        let scan = scan_of([statement.clone()]);
        assert!(
            scan.uses(*target),
            "a program that uses {target:?} flags it"
        );
    }
}

#[test]
fn a_construct_that_does_not_occur_is_not_flagged() {
    let table = one_of_each();
    let all: Vec<Construct> = table.iter().map(|(construct, _)| *construct).collect();

    // A program using exactly one construct flags no other.
    for (target, statement) in &table {
        let scan = scan_of([statement.clone()]);
        for other in &all {
            if other != target {
                assert!(
                    !scan.uses(*other),
                    "a program that uses only {target:?} does not flag {other:?}",
                );
            }
        }
    }

    // A program that uses no construct flags none.
    let plain = scan_of([Statement::Rule(Rule::fact(atom("p")))]);
    for construct in &all {
        assert!(
            !plain.uses(*construct),
            "the plain fact `p.` flags no construct, not {construct:?}",
        );
    }
    assert_eq!(plain.all().count(), 0, "the plain fact's scan is empty");
}

// ---- Law: all covers exactly the used constructs; first names a using statement ----

#[test]
fn all_reports_exactly_the_used_constructs() {
    let table = one_of_each();
    let all: BTreeSet<Construct> = table.iter().map(|(construct, _)| *construct).collect();
    let scan = Constructs::of(&Program::of(table.iter().map(|(_, s)| wp(s.clone()))));

    let reported: BTreeSet<Construct> = scan.all().map(|(construct, _)| construct).collect();
    assert_eq!(reported, all, "all() reports exactly the used constructs");
}

#[test]
fn first_names_a_statement_that_uses_the_construct() {
    let table = one_of_each();
    let all: BTreeSet<Construct> = table.iter().map(|(construct, _)| *construct).collect();
    let scan = Constructs::of(&Program::of(table.iter().map(|(_, s)| wp(s.clone()))));

    // Every first() names a statement that genuinely uses that construct — re-scanning
    // the witness alone re-flags it.
    for construct in &all {
        let witness = scan
            .first(*construct)
            .expect("a used construct has a first witness");
        let rescanned = Constructs::of(&Program::of([witness]));
        assert!(
            rescanned.uses(*construct),
            "the witness recorded for {construct:?} uses it",
        );
    }

    // all() and first() agree.
    for (construct, witness) in scan.all() {
        assert_eq!(
            scan.first(construct),
            Some(witness),
            "all() and first() agree for {construct:?}",
        );
    }

    // An unused construct has no first.
    let plain = scan_of([Statement::Rule(Rule::fact(atom("p")))]);
    assert_eq!(plain.first(Construct::Disjunction), None);
}

// ---- Law: provenance-blind ----

#[test]
fn the_scan_is_provenance_blind() {
    let contents: Vec<Statement> = one_of_each().into_iter().map(|(_, s)| s).collect();

    let with_tag = |tag: &'static str| {
        Program::of(contents.iter().cloned().map(move |statement| {
            WithProvenance::new(
                statement,
                Provenance::from(Origin::Transformed(TransformTag::new(tag))),
            )
        }))
    };
    let constructed = Program::of(contents.iter().cloned().map(wp));
    let tagged_x = with_tag("x");
    let tagged_y = with_tag("y");

    // Content-equal programs with different provenance scan equal.
    assert_eq!(
        Constructs::of(&constructed),
        Constructs::of(&tagged_x),
        "constructed and transformed programs scan equal",
    );
    assert_eq!(
        Constructs::of(&tagged_x),
        Constructs::of(&tagged_y),
        "two different transform tags scan equal",
    );

    // The perturbation is real, not vacuous: a witness carries different provenance
    // across the two programs, yet the witnesses (and so the scans) compare equal.
    let from_constructed = Constructs::of(&constructed)
        .first(Construct::Disjunction)
        .expect("disjunction present");
    let from_tagged = Constructs::of(&tagged_x)
        .first(Construct::Disjunction)
        .expect("disjunction present");
    assert_ne!(
        from_constructed.provenance(),
        from_tagged.provenance(),
        "the witnesses carry different provenance",
    );
    assert_eq!(
        from_constructed, from_tagged,
        "yet content-equal witnesses compare equal",
    );

    // A constructed program (no source span) scans soundly.
    assert!(
        Constructs::of(&constructed).uses(Construct::Disjunction),
        "a constructed program scans without a span",
    );
}

// ---- Law: totality ----

#[test]
fn the_scan_is_total_on_any_program() {
    // The empty program.
    assert_eq!(
        Constructs::of(&Program::default()).all().count(),
        0,
        "the empty program uses no construct",
    );

    // Every construct at once.
    let sink = Constructs::of(&Program::of(one_of_each().into_iter().map(|(_, s)| wp(s))));
    assert_eq!(
        sink.all().count(),
        17,
        "the kitchen-sink program uses all seventeen constructs",
    );

    // A term far deeper than the call stack: the term-level walk is iterative (§13),
    // so it scans without overflow. Built by direct nesting so construction stays
    // linear — the ingest door canonicalizes it once (an arithmetic operator never
    // folds, §3.5), and the scan reads it through the iterative `subterms`.
    let mut deep = var("X");
    for _ in 0..100_000 {
        deep = Term::Absolute(Box::new(deep));
    }
    let scan = scan_of([Statement::Rule(Rule::fact(Atom::new(name("p"), [deep])))]);
    assert!(
        scan.uses(Construct::Arithmetic),
        "a deeply nested arithmetic term is flagged, without overflow",
    );
}

// ---- Breadth: constructs are found wherever they structurally occur ----

#[test]
fn conditional_literals_and_set_elements_are_walked() {
    // q :- a : X < 1.  — a comparison inside a conditional literal's condition.
    let conditional = BodyElement::Conditional(ConditionalLiteral {
        literal: Literal::from(atom("a")),
        condition: Condition::new([comparison_literal(var("X"), Relation::Lt, num(1))]),
    });
    let scan = scan_of([Statement::Rule(
        Head::Literal(Literal::from(atom("q"))).when(vec![conditional]),
    )]);
    assert!(scan.uses(Construct::Comparison));

    // :- { a; b : c } >= 1.  — a set aggregate with a bare and a conditional element.
    let set = Aggregate::Set(SetAggregate::new(
        None,
        [
            SetElement::Literal(Literal::from(atom("a"))),
            SetElement::ConditionalLiteral(ConditionalLiteral {
                literal: Literal::from(atom("b")),
                condition: Condition::new([Literal::from(atom("c"))]),
            }),
        ],
        Some(Guard {
            relation: Some(Relation::Ge),
            term: num(1),
        }),
    ));
    let scan = scan_of([Statement::Rule(Rule::constraint(vec![
        BodyElement::Aggregate {
            negation: DefaultNegation::None,
            aggregate: set,
        },
    ]))]);
    assert!(
        scan.uses(Construct::BodyAggregate),
        "a set aggregate is a body aggregate"
    );
}

#[test]
fn theory_atom_ordinary_arguments_are_walked_but_theory_terms_are_not() {
    // &diff(X + 1) { Y : q(Y) }.  — the ordinary argument's arithmetic is found; the
    // element's condition is walked.
    let theory = TheoryAtom::new(
        name("diff"),
        [var("X") + num(1)],
        [TheoryElement::new(
            [theory_var("Y")],
            Some(Condition::new([comparison_literal(
                var("Z"),
                Relation::Lt,
                num(1),
            )])),
        )],
        None,
    );
    let scan = scan_of([Statement::Rule(
        Head::TheoryAtom(theory).when(Body::empty()),
    )]);
    assert!(scan.uses(Construct::TheoryAtom));
    assert!(
        scan.uses(Construct::Arithmetic),
        "an ordinary-term argument's arithmetic is found",
    );
    assert!(
        scan.uses(Construct::Comparison),
        "a comparison in an element condition is found",
    );

    // &diff { X + Y } — the theory term's own operator is the peer algebra, not
    // ordinary arithmetic.
    let operation = TheoryTerm::Operation {
        operators: vec![vec![], vec![TheoryOperator::new("+")]],
        operands: vec![theory_var("X"), theory_var("Y")],
    };
    let theory = TheoryAtom::new(
        name("diff"),
        [],
        [TheoryElement::new([operation], None)],
        None,
    );
    let scan = scan_of([Statement::Rule(
        Head::TheoryAtom(theory).when(Body::empty()),
    )]);
    assert!(scan.uses(Construct::TheoryAtom));
    assert!(
        !scan.uses(Construct::Arithmetic),
        "a theory term's own operator is not ordinary arithmetic",
    );

    // :- not &diff { }.  — a default-negated theory atom at body-element position.
    let scan = scan_of([Statement::Rule(Rule::constraint(not(TheoryAtom::new(
        name("diff"),
        [],
        [],
        None,
    ))))]);
    assert!(
        scan.uses(Construct::TheoryAtom),
        "a body theory atom is found"
    );
    assert!(
        scan.uses(Construct::DefaultNegation),
        "a not-ed theory atom is default negation",
    );
}

#[test]
fn aggregate_and_choice_guards_are_walked() {
    // 1 { a }.  — a guarded choice.
    let scan = scan_of([Statement::Rule(
        Head::Choice(Choice::new(
            Some(Guard {
                relation: None,
                term: num(1),
            }),
            [ChoiceElement::new(
                Literal::from(atom("a")),
                Condition::empty(),
            )],
            None,
        ))
        .when(Body::empty()),
    )]);
    assert!(scan.uses(Construct::Choice));

    // :- not not #sum { 1 } >= 0.  — a left-guarded, doubly-default-negated aggregate.
    let aggregate = BodyElement::Aggregate {
        negation: DefaultNegation::NotNot,
        aggregate: Aggregate::Function(FunctionAggregate::new(
            Some(Guard {
                relation: None,
                term: num(0),
            }),
            AggregateFunction::Sum,
            [BodyAggregateElement::new([num(1)], Condition::empty())],
            None,
        )),
    };
    let scan = scan_of([Statement::Rule(Rule::constraint(vec![aggregate]))]);
    assert!(scan.uses(Construct::BodyAggregate));
    assert!(
        scan.uses(Construct::DefaultNegation),
        "`not not` on an aggregate is default negation",
    );
}

#[test]
fn directives_carry_their_inner_constructs() {
    // #external -p.  → strong negation in the atom.
    let scan = scan_of([Statement::External(External::new(
        -atom("p"),
        Body::empty(),
        None,
    ))]);
    assert!(scan.uses(Construct::StrongNegation));

    // #const x = 1 + 2.  → arithmetic in the value.
    let scan = scan_of([Statement::Const(Const {
        name: name("x"),
        value: num(1) + num(2),
        policy: None,
    })]);
    assert!(scan.uses(Construct::Arithmetic));

    // #show (1 .. 3).  → an interval in a shown term.
    let scan = scan_of([Statement::Show(Show::Term(num(1).to(3)))]);
    assert!(scan.uses(Construct::Interval));

    // #show a : not p.  → default negation in a shown body.
    let scan = scan_of([Statement::Show(Show::term_body(
        num(1),
        Body::new([not(atom("p"))]),
    ))]);
    assert!(scan.uses(Construct::DefaultNegation));

    // #project a : X < 1.  → a comparison in a projected body.
    let scan = scan_of([Statement::Project(Project::atom_body(
        atom("a"),
        Body::new([BodyElement::from(comparison_literal(
            var("X"),
            Relation::Lt,
            num(1),
        ))]),
    ))]);
    assert!(scan.uses(Construct::Comparison));

    // #heuristic p : q. [X + 1, 1]  → arithmetic in the bias.
    let scan = scan_of([Statement::Heuristic(Heuristic::new(
        atom("p"),
        Body::new([BodyElement::from(atom("q"))]),
        var("X") + num(1),
        None,
        num(1),
    ))]);
    assert!(scan.uses(Construct::Arithmetic));

    // #edge (1 .. 2, 3).  → an interval in an edge node.
    let scan = scan_of([Statement::Edge(Edge::new(
        [(num(1).to(2), num(3))],
        Body::empty(),
    ))]);
    assert!(scan.uses(Construct::Interval));

    // a query ?- @f(1).-carrying atom → an @-call in a queried atom's argument.
    let scan = scan_of([Statement::Query(Query::new(Atom::new(
        name("p"),
        [Term::External {
            name: name("f"),
            arguments: vec![num(1)],
        }],
    )))]);
    assert!(scan.uses(Construct::ExternalCall));
}

#[test]
fn unary_and_absolute_terms_are_arithmetic() {
    // p(|X|).
    let scan = scan_of([Statement::Rule(Rule::fact(Atom::new(
        name("p"),
        [var("X").abs()],
    )))]);
    assert!(
        scan.uses(Construct::Arithmetic),
        "an absolute-value term is arithmetic"
    );

    // p(~X).
    let scan = scan_of([Statement::Rule(Rule::fact(Atom::new(
        name("p"),
        [var("X").complement()],
    )))]);
    assert!(
        scan.uses(Construct::Arithmetic),
        "a bitwise-complement term is arithmetic"
    );
}

#[test]
fn term_free_and_boolean_positions_contribute_nothing() {
    // :- #true.  — a boolean literal is no construct, and does not panic.
    let scan = scan_of([Statement::Rule(Rule::constraint(Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::True,
    }))]);
    assert_eq!(scan.all().count(), 0, "a #true literal is no construct");

    // The signature and term-free directives contribute nothing.
    let signature = |text: &str, arity| Signature {
        sign: Sign::Positive,
        name: name(text),
        arity,
    };
    let scan = scan_of([
        Statement::Defined(Defined {
            signature: signature("p", 1),
        }),
        Statement::Show(Show::Signature(signature("q", 2))),
        Statement::Show(Show::All),
        Statement::Project(Project::Signature(signature("s", 0))),
    ]);
    assert_eq!(
        scan.all().count(),
        0,
        "signature and term-free directives are no construct"
    );
}
