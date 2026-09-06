//! The declarative construction surface (docs/design/program.md §7): strong versus
//! arithmetic negation, role-typed default negation, the rule-reads-as-the-rule
//! constructors, arithmetic and intervals, the widening coercions and the closed
//! body/head coercion class, and the two-audiences-one-value seed — a program built
//! through the surface is structurally equal to the same program assembled from the
//! primitive constructors (the *first-solve* witness, §7.3, §16). The compile-fail
//! half of the role-typing (that `not` in head position does not compile) is a
//! `compile_fail` doc example on `construct::not`, and on the aggregate coercion for
//! a negated aggregate.

use std::collections::BTreeSet;

use themelios_program::construct::{maximize, minimize, not, not_not};
use themelios_program::program::{
    Aggregate, AggregateFunction, Arguments, Atom, Body, BodyElement, Choice, ChoiceElement,
    Comparison, Condition, ConditionalLiteral, Const, DefaultNegation, Defined, Direction,
    Disjunction, DisjunctionElement, Edge, External, Guard, Head, HeadAggregate,
    HeadAggregateElement, Heuristic, Include, IncludeTarget, IntoBody, IntoHead, Literal,
    LiteralInner, Optimize, OptimizeElement, Program, Project, Query, Relation, Rule, Script,
    SetAggregate, SetElement, Show, Statement, TheoryAtom, TheoryDefinition, TheoryElement,
    TheoryTerm, WeakConstraint, weight,
};
use themelios_program::provenance::{Origin, WithProvenance};
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};

// ---- small helpers (the terse spellings are the macro tier's, §7.1) ----

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

// ---- The faithful argument-list pool (§4.6): a pooled atom carries every
// alternative, and its alternatives may differ in arity (`p(1; 2, 3)` is p/1 and
// p/2). A one-alternative pool is a `Single` atom by canonicalization (§5.1). ----

#[test]
fn an_empty_pool_is_refused_at_the_constructor_door() {
    // A pool is a disjunction of one or more alternatives; an empty one is a malformed value
    // (§7.2). `Atom::pooled` and `Term::pool` refuse it with a typed error, rather than a silent
    // value that would delete its statement at `unpool` (§9). A one-alternative pool is valid — it
    // collapses to the single tuple/term at canonicalization (§5.1).
    assert!(Atom::pooled(name("p"), Vec::<Vec<Term>>::new()).is_err());
    assert!(Term::pool(Vec::<Term>::new()).is_err());
    assert!(Atom::pooled(name("p"), [vec![num(0)], vec![num(1)]]).is_ok());
    assert!(Term::pool([num(0), num(1)]).is_ok());
    assert!(Term::pool([num(0)]).is_ok());
}

#[test]
fn a_pooled_atom_carries_every_alternative() {
    let atom =
        Atom::pooled(name("p"), [vec![num(1)], vec![num(2), num(3)]]).expect("a non-empty pool");
    assert!(atom.is_pooled());
    let alternatives: Vec<Vec<Term>> = atom.alternatives().map(<[Term]>::to_vec).collect();
    assert_eq!(alternatives, vec![vec![num(1)], vec![num(2), num(3)]]);
}

#[test]
fn a_one_alternative_pool_canonicalizes_to_a_single_atom() {
    let atom = Atom::pooled(name("p"), [vec![num(1)]]).expect("a non-empty pool");
    assert!(!atom.is_pooled());
    let alternatives: Vec<Vec<Term>> = atom.alternatives().map(<[Term]>::to_vec).collect();
    assert_eq!(alternatives, vec![vec![num(1)]]);
}

// ---- Strong versus arithmetic negation (§4.6) ----

#[test]
fn strong_negation_flips_the_sign_and_is_involutive() {
    let atom = Atom::new(name("p"), [var("X")]);
    assert_eq!(atom.sign, Sign::Positive);

    let negated = -atom.clone();
    assert_eq!(
        negated.sign,
        Sign::Negative,
        "strong negation flips the sign"
    );
    assert_eq!(-(-atom.clone()), atom, "strong negation is involutive");
}

#[test]
fn arithmetic_negation_is_a_unary_negate() {
    match -var("X") {
        Term::UnaryOperation { operator, .. } => assert_eq!(operator, UnaryOp::Negate),
        other => panic!("arithmetic negation is a UnaryOperation, got {other:?}"),
    }
}

#[test]
fn strong_and_arithmetic_negation_do_not_conflate() {
    // `-p(X)` is a strongly-negated atom; `-(X)` is an arithmetic term — different
    // types, different values, and no coercion turns one into the other.
    let strong = -Atom::new(name("p"), [var("X")]);
    let arithmetic = -var("X");
    assert_eq!(strong.sign, Sign::Negative);
    assert!(matches!(arithmetic, Term::UnaryOperation { .. }));
}

// ---- Role-typed default negation (§4.5, §7.1) ----

#[test]
fn default_negation_over_an_atom_is_a_body_literal() {
    match not(Atom::new(name("p"), [var("X")])) {
        BodyElement::Literal(literal) => {
            assert_eq!(literal.negation, DefaultNegation::Not);
            match literal.inner {
                LiteralInner::Atom(atom) => assert_eq!(atom.get().name, name("p")),
                other => panic!("expected an atom literal, got {other:?}"),
            }
        }
        other => panic!("`not(atom)` is a body literal, got {other:?}"),
    }
}

#[test]
fn double_default_negation_is_its_own_case() {
    let once = not(Atom::new(name("p"), [var("X")]));
    let twice = not_not(Atom::new(name("p"), [var("X")]));
    let bare = BodyElement::from(Atom::new(name("p"), [var("X")]));

    assert_ne!(once, twice, "`not not` is distinct from `not`");
    assert_ne!(once, bare, "`not` is distinct from bare");
    assert_ne!(twice, bare, "`not not` is distinct from bare");

    match twice {
        BodyElement::Literal(literal) => assert_eq!(literal.negation, DefaultNegation::NotNot),
        other => panic!("`not_not(atom)` is a body literal, got {other:?}"),
    }
}

#[test]
fn default_negation_over_an_aggregate_is_a_negated_body_element() {
    let literal = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(Atom::new(
            name("q"),
            [var("X")],
        ))),
    };
    let aggregate = Aggregate::Set(SetAggregate::new(
        None,
        [SetElement::Literal(literal)],
        None,
    ));

    match not(aggregate) {
        BodyElement::Aggregate { negation, .. } => assert_eq!(negation, DefaultNegation::Not),
        other => panic!("`not(aggregate)` is a negated body element, got {other:?}"),
    }
}

// ---- The rule surface reads as the rule, and is total (§4.3, §7.1, §7.2) ----

#[test]
fn a_fact_is_a_single_literal_head_and_an_empty_body() {
    let fact = Rule::fact(Atom::new(name("p"), [num(1)]));
    assert!(fact.is_fact());
    assert!(fact.body().get().is_empty());
    assert!(matches!(fact.head().get(), Head::Literal(_)));
}

#[test]
fn a_constraint_is_a_falsum_head_over_its_body() {
    let constraint = Rule::constraint(not(Atom::new(name("p"), [num(1)])));
    assert!(constraint.is_constraint());
    assert!(matches!(constraint.head().get(), Head::Falsum));
}

#[test]
fn a_rule_holds_when_its_body_does() {
    let rule = Atom::new(name("reach"), [var("X"), var("Z")])
        .into_head()
        .when([
            Atom::new(name("reach"), [var("X"), var("Y")]),
            Atom::new(name("edge"), [var("Y"), var("Z")]),
        ]);
    assert!(!rule.is_fact());
    assert!(!rule.is_constraint());
    assert_eq!(rule.body().get().elements().count(), 2);
}

// ---- Canonicalization at the door (§5.1) ----

#[test]
fn atom_new_canonicalizes_ground_function_arguments() {
    let ground_function = Term::Function {
        name: name("f"),
        arguments: vec![num(1)],
    };
    let atom = Atom::new(name("p"), [ground_function]);
    assert_eq!(
        atom.arguments,
        Arguments::Single(vec![Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        })]),
        "a ground constructor argument collapses to a Symbolic leaf",
    );
}

#[test]
fn atom_new_deep_repairs_a_raw_non_canonical_argument() {
    // The atom, ingest, and statement doors are the deep-repair boundary (§5.1, §7.2): a raw term a
    // caller assembles by hand — a ground function never collapsed — is repaired whole when it
    // enters through `Atom::new`, at the top of an argument and, the discriminating half, nested
    // under an operator where a one-level step would never reach it. The operator doors' one-level
    // canonicalization (§7.1) rests on this boundary for the program-wide invariant, so an atom
    // door routed to one level would fail here.
    let raw = || Term::Function {
        name: name("f"),
        arguments: vec![num(1)],
    };
    let repaired = || {
        Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        })
    };
    let under_an_operator = |inner: Term| Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(var("X")),
        right: Box::new(inner),
    };
    let atom = Atom::new(name("p"), [raw(), under_an_operator(raw())]);
    assert_eq!(
        atom.arguments,
        Arguments::Single(vec![repaired(), under_an_operator(repaired())]),
        "the atom door repairs a raw argument at its top and below it",
    );
}

#[test]
fn a_rule_built_through_the_surface_is_already_canonical() {
    let ground = Term::Function {
        name: name("f"),
        arguments: vec![num(1)],
    };
    let built = Rule::fact(Atom::new(name("p"), [ground]));
    let collapsed = Rule::fact(Atom::new(
        name("p"),
        [Term::Symbolic(Symbol::Function {
            name: name("f"),
            arguments: vec![Symbol::Number(1)],
            sign: Sign::Positive,
        })],
    ));
    assert_eq!(
        built, collapsed,
        "the surface yields the canonical form directly"
    );
}

// ---- Coercion widens the one obvious spelling, it does not branch (§7.1) ----

#[test]
fn an_i32_argument_coerces_to_a_number_term() {
    let from_i32 = Atom::new(name("p"), [Term::from(1)]);
    let from_symbol = Atom::new(name("p"), [Term::Symbolic(Symbol::Number(1))]);
    assert_eq!(from_i32, from_symbol);
}

#[test]
fn a_str_argument_coerces_to_a_string_term() {
    // A `&str` denotes a string term — the `Symbolic(String)` leaf, exactly (§3.4).
    let from_str = Atom::new(name("p"), [Term::from("a")]);
    let from_symbol = Atom::new(name("p"), [Term::Symbolic(Symbol::String("a".to_owned()))]);
    assert_eq!(from_str, from_symbol);
    assert_eq!(
        Term::from("a"),
        Term::Symbolic(Symbol::String("a".to_owned()))
    );
}

#[test]
fn an_owned_string_coerces_to_the_same_string_term() {
    // The owned and the borrowed spellings are two doors to one value (§7.1).
    assert_eq!(Term::from(String::from("a")), Term::from("a"));
    assert_eq!(
        Term::from(String::from("a")),
        Term::Symbolic(Symbol::String("a".to_owned()))
    );
}

#[test]
fn a_string_term_is_not_the_constant_of_its_text() {
    // `"a"` and `a` are two values the type keeps apart (§3.4, §7.1): a constant is a
    // validated `Name` through `Term::constant`; a Rust string denotes a string term and
    // is never read as a name.
    assert_ne!(Term::from("a"), Term::constant(name("a")));
    assert_ne!(Term::from(String::from("a")), Term::constant(name("a")));
}

#[test]
fn the_term_and_symbol_string_coercions_agree() {
    // The two types' scalar coercions are twins (§7.1): the string a `Term` takes is the
    // string a `Symbol` takes, lifted through `From<Symbol>`.
    assert_eq!(
        Term::from(String::from("a")),
        Term::from(Symbol::from(String::from("a")))
    );
    assert_eq!(Term::from("a"), Term::from(Symbol::from("a")));
}

#[test]
fn a_literal_and_an_atom_both_reach_a_one_literal_head() {
    let atom = Atom::new(name("p"), [var("X")]);
    let via_atom: Head = atom.clone().into_head();
    let via_literal: Head = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
    .into_head();

    assert_eq!(via_atom, via_literal);
    assert!(matches!(via_atom, Head::Literal(_)));
}

#[test]
fn a_single_element_and_a_one_element_sequence_reach_the_same_body() {
    let element = || BodyElement::from(Atom::new(name("p"), [var("X")]));
    let single: Body = element().into_body();
    let sequence: Body = [element()].into_body();

    assert_eq!(single, sequence);
    assert_eq!(single.elements().count(), 1);
}

// ---- The body/head coercion class closes by a rule (§7.1): every body-able value is
// `Into<BodyElement>` and `IntoBody`, every head-able value is `IntoHead`, and default
// negation stays `not`/`not_not`'s alone (its compile-fail half is a `compile_fail`
// doc example on the aggregate coercion). Each coercion is a deep-repair door (§5.1),
// save the comparison, canonical by construction. ----

/// A raw ground function term a caller assembles by hand — never collapsed, so a value
/// carrying it is non-canonical until a deep-repair door reaches it (§5.1).
fn raw() -> Term {
    Term::Function {
        name: name("f"),
        arguments: vec![num(1)],
    }
}

/// The canonical form of [`raw`]: the ground function collapsed to its `Symbolic` leaf.
fn repaired() -> Term {
    Term::Symbolic(Symbol::Function {
        name: name("f"),
        arguments: vec![Symbol::Number(1)],
        sign: Sign::Positive,
    })
}

/// The positive literal `p(term)` assembled from the struct literals — through no door,
/// so a raw argument stays raw.
fn literal_over(term: Term) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(Atom {
            sign: Sign::Positive,
            name: name("p"),
            arguments: Arguments::Single(vec![term]),
        })),
    }
}

/// The set aggregate `term { p(1) }` — a `Guard` has no door of its own (§5.1), so its
/// bound enters as given.
fn set_aggregate_bounded_by(term: Term) -> Aggregate {
    Aggregate::Set(SetAggregate::new(
        Some(Guard {
            relation: None,
            term,
        }),
        [SetElement::Literal(literal_over(num(1)))],
        None,
    ))
}

/// The theory atom `&sum { 1 : p(term) }` — `TheoryAtom::new` collapses its ordinary
/// arguments, but an element's condition enters as given.
fn theory_atom_conditioned_on(term: Term) -> TheoryAtom {
    TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Symbolic(Symbol::Number(1))],
            Some(Condition::new([literal_over(term)])),
        )],
        None,
    )
}

/// The conditional literal `p(term) : p(1)`.
fn conditional_over(term: Term) -> ConditionalLiteral {
    ConditionalLiteral {
        literal: literal_over(term),
        condition: Condition::new([literal_over(num(1))]),
    }
}

/// The disjunction `p(term) | p(2)`.
fn disjunction_over(term: Term) -> Disjunction {
    Disjunction::new([
        DisjunctionElement::new(literal_over(term), Condition::empty()),
        DisjunctionElement::new(literal_over(num(2)), Condition::empty()),
    ])
}

/// The choice `term { p(1) }` — its left guard bounds at the given term.
fn choice_bounded_by(term: Term) -> Choice {
    Choice::new(
        Some(Guard {
            relation: None,
            term,
        }),
        [ChoiceElement::new(literal_over(num(1)), Condition::empty())],
        None,
    )
}

/// The head aggregate `#count { 1 : p(1) } <= term` — its right guard bounds at the
/// given term.
fn head_aggregate_bounded_by(term: Term) -> HeadAggregate {
    HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [num(1)],
            literal_over(num(1)),
            Condition::empty(),
        )],
        Some(Guard {
            relation: Some(Relation::Le),
            term,
        }),
    )
}

/// The one element of a one-element body.
fn the_one_element(rule: &Rule) -> &BodyElement {
    let mut elements = rule.body().get().elements();
    let element = elements.next().expect("a one-element body");
    assert!(elements.next().is_none(), "a one-element body");
    element.get()
}

/// The rule as the ingest door admits it — deep-canonicalized (§5.1, §6.3): the public
/// oracle for "already canonical".
fn ingested(rule: &Rule) -> Rule {
    let program = Program::of([rule.clone()]);
    let mut statements = program.statements();
    let statement = statements.next().expect("one statement");
    assert!(statements.next().is_none(), "one statement");
    match statement.get() {
        Statement::Rule(rule) => rule.clone(),
        other => panic!("a rule ingests as a rule, got {other:?}"),
    }
}

#[test]
fn a_comparison_is_a_one_element_constraint_body() {
    let comparison = Comparison::new(var("X"), Relation::Lt, 5);
    let constraint = Rule::constraint(comparison.clone());
    assert!(constraint.is_constraint());
    match the_one_element(&constraint) {
        BodyElement::Literal(Literal {
            negation,
            inner: LiteralInner::Comparison(inner),
        }) => {
            assert_eq!(*negation, DefaultNegation::None);
            assert_eq!(inner.get(), &comparison);
        }
        other => panic!("a comparison is a positive comparison literal, got {other:?}"),
    }
}

#[test]
fn an_aggregate_is_a_one_element_constraint_body() {
    let aggregate = set_aggregate_bounded_by(num(1));
    let constraint = Rule::constraint(aggregate.clone());
    assert!(constraint.is_constraint());
    match the_one_element(&constraint) {
        BodyElement::Aggregate {
            negation,
            aggregate: coerced,
        } => {
            assert_eq!(*negation, DefaultNegation::None);
            assert_eq!(coerced, &aggregate);
        }
        other => panic!("an aggregate is a positive aggregate element, got {other:?}"),
    }
}

#[test]
fn a_theory_atom_is_a_one_element_constraint_body() {
    let atom = theory_atom_conditioned_on(num(1));
    let constraint = Rule::constraint(atom.clone());
    assert!(constraint.is_constraint());
    match the_one_element(&constraint) {
        BodyElement::TheoryAtom {
            negation,
            atom: coerced,
        } => {
            assert_eq!(*negation, DefaultNegation::None);
            assert_eq!(coerced, &atom);
        }
        other => panic!("a theory atom is a positive theory-atom element, got {other:?}"),
    }
}

#[test]
fn a_conditional_is_a_one_element_constraint_body() {
    let conditional = conditional_over(var("X"));
    let constraint = Rule::constraint(conditional.clone());
    assert!(constraint.is_constraint());
    match the_one_element(&constraint) {
        BodyElement::Conditional(coerced) => assert_eq!(coerced, &conditional),
        other => panic!("a conditional literal is a conditional element, got {other:?}"),
    }
}

#[test]
fn a_head_holds_when_a_comparison_does() {
    let comparison = Comparison::new(var("X"), Relation::Gt, 0);
    let rule = Atom::new(name("positive"), [var("X")])
        .into_head()
        .when(comparison.clone());
    assert!(!rule.is_constraint());
    match the_one_element(&rule) {
        BodyElement::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Comparison(inner),
        }) => assert_eq!(inner.get(), &comparison),
        other => panic!("a comparison is a positive comparison literal, got {other:?}"),
    }
}

#[test]
fn a_disjunction_is_a_disjunctive_head() {
    let disjunction = disjunction_over(num(1));
    let rule = Rule::fact(disjunction.clone());
    assert!(rule.body().get().is_empty());
    match rule.head().get() {
        Head::Disjunction(coerced) => assert_eq!(coerced, &disjunction),
        other => panic!("a disjunction is a disjunctive head, got {other:?}"),
    }
}

#[test]
fn a_choice_is_a_choice_head() {
    let choice = choice_bounded_by(num(1));
    let rule = Rule::fact(choice.clone());
    assert!(rule.body().get().is_empty());
    match rule.head().get() {
        Head::Choice(coerced) => assert_eq!(coerced, &choice),
        other => panic!("a choice is a choice head, got {other:?}"),
    }
}

#[test]
fn a_head_aggregate_is_an_aggregate_head() {
    let aggregate = head_aggregate_bounded_by(num(1));
    let rule = Rule::fact(aggregate.clone());
    assert!(rule.body().get().is_empty());
    match rule.head().get() {
        Head::Aggregate(coerced) => assert_eq!(coerced, &aggregate),
        other => panic!("a head aggregate is an aggregate head, got {other:?}"),
    }
}

#[test]
fn a_theory_atom_is_a_theory_atom_head() {
    let atom = theory_atom_conditioned_on(num(1));
    let rule = Rule::fact(atom.clone());
    assert!(rule.body().get().is_empty());
    match rule.head().get() {
        Head::TheoryAtom(coerced) => assert_eq!(coerced, &atom),
        other => panic!("a theory atom is an unsigned theory-atom head, got {other:?}"),
    }
}

#[test]
fn a_comparison_coercion_wraps_the_chain_as_built() {
    // `Comparison::new` is its own canonicalizing door (§5.1): the chain arrives with its
    // terms collapsed, and the coercion wraps that very value in a positive literal.
    let comparison = Comparison::new(raw(), Relation::Eq, 2);
    assert_eq!(
        comparison.first(),
        &repaired(),
        "the comparison door collapsed the term"
    );
    let literal = Literal::from(comparison.clone());
    assert_eq!(literal.negation, DefaultNegation::None);
    assert_eq!(
        literal.inner,
        LiteralInner::Comparison(WithProvenance::constructed(comparison))
    );
}

#[test]
fn an_aggregate_coercion_repairs_a_raw_bound() {
    assert_ne!(
        set_aggregate_bounded_by(raw()),
        set_aggregate_bounded_by(repaired()),
        "the input is non-canonical as built"
    );
    match BodyElement::from(set_aggregate_bounded_by(raw())) {
        BodyElement::Aggregate { aggregate, .. } => {
            assert_eq!(aggregate, set_aggregate_bounded_by(repaired()));
        }
        other => panic!("an aggregate coerces to an aggregate element, got {other:?}"),
    }
}

#[test]
fn a_theory_atom_coercion_repairs_a_raw_condition() {
    // The theory terms never collapse (§4.9); the ordinary literal under an element's
    // condition does, and the coercion's deep pass reaches it.
    assert_ne!(
        theory_atom_conditioned_on(raw()),
        theory_atom_conditioned_on(repaired()),
        "the input is non-canonical as built"
    );
    match BodyElement::from(theory_atom_conditioned_on(raw())) {
        BodyElement::TheoryAtom { atom, .. } => {
            assert_eq!(atom, theory_atom_conditioned_on(repaired()));
        }
        other => panic!("a theory atom coerces to a theory-atom element, got {other:?}"),
    }
}

#[test]
fn a_conditional_coercion_repairs_a_raw_argument() {
    assert_ne!(
        conditional_over(raw()),
        conditional_over(repaired()),
        "the input is non-canonical as built"
    );
    match BodyElement::from(conditional_over(raw())) {
        BodyElement::Conditional(conditional) => {
            assert_eq!(conditional, conditional_over(repaired()));
        }
        other => panic!("a conditional coerces to a conditional element, got {other:?}"),
    }
}

#[test]
fn a_disjunction_coercion_repairs_a_raw_argument() {
    assert_ne!(
        disjunction_over(raw()),
        disjunction_over(repaired()),
        "the input is non-canonical as built"
    );
    match disjunction_over(raw()).into_head() {
        Head::Disjunction(disjunction) => assert_eq!(disjunction, disjunction_over(repaired())),
        other => panic!("a disjunction coerces to a disjunctive head, got {other:?}"),
    }
}

#[test]
fn a_choice_coercion_repairs_a_raw_bound() {
    assert_ne!(
        choice_bounded_by(raw()),
        choice_bounded_by(repaired()),
        "the input is non-canonical as built"
    );
    match choice_bounded_by(raw()).into_head() {
        Head::Choice(choice) => assert_eq!(choice, choice_bounded_by(repaired())),
        other => panic!("a choice coerces to a choice head, got {other:?}"),
    }
}

#[test]
fn a_head_aggregate_coercion_repairs_a_raw_bound() {
    assert_ne!(
        head_aggregate_bounded_by(raw()),
        head_aggregate_bounded_by(repaired()),
        "the input is non-canonical as built"
    );
    match head_aggregate_bounded_by(raw()).into_head() {
        Head::Aggregate(aggregate) => assert_eq!(aggregate, head_aggregate_bounded_by(repaired())),
        other => panic!("a head aggregate coerces to an aggregate head, got {other:?}"),
    }
}

#[test]
fn a_coerced_rule_is_already_canonical() {
    // The public oracle for the deep pass is the ingest door (§6.3): a rule the coercions
    // built from raw inputs is admitted unchanged, so the coercions are the deep-repair
    // doors (§5.1, §7.2) — and the comparison, canonical by construction, needs none.
    let rules = [
        Rule::constraint(Comparison::new(raw(), Relation::Eq, 2)),
        Rule::constraint(set_aggregate_bounded_by(raw())),
        Rule::constraint(theory_atom_conditioned_on(raw())),
        Rule::constraint(conditional_over(raw())),
        Rule::fact(disjunction_over(raw())),
        Rule::fact(choice_bounded_by(raw())),
        Rule::fact(head_aggregate_bounded_by(raw())),
        Rule::fact(theory_atom_conditioned_on(raw())),
    ];
    for rule in &rules {
        assert_eq!(
            &ingested(rule),
            rule,
            "the ingest door admits the coerced rule unchanged"
        );
    }
}

// ---- Arithmetic and intervals compose as written (§7.1) ----

#[test]
fn arithmetic_composes_over_term_shaped_values() {
    match var("X") + 1 {
        Term::BinaryOperation { operator, .. } => assert_eq!(operator, BinaryOp::Add),
        other => panic!("`X + 1` is a BinaryOperation, got {other:?}"),
    }
    match var("X").to(10) {
        Term::Interval { .. } => {}
        other => panic!("`X.to(10)` is an Interval, got {other:?}"),
    }
    match var("X").pow(2) {
        Term::BinaryOperation { operator, .. } => assert_eq!(operator, BinaryOp::Pow),
        other => panic!("`X.pow(2)` is a Pow, got {other:?}"),
    }
    match var("X").complement() {
        Term::UnaryOperation { operator, .. } => assert_eq!(operator, UnaryOp::BitwiseNot),
        other => panic!("`X.complement()` is a BitwiseNot, got {other:?}"),
    }
    match var("X").abs() {
        Term::Absolute(_) => {}
        other => panic!("`X.abs()` is an Absolute, got {other:?}"),
    }
}

// ---- Optimization builds on `weight(w).at_priority(p)` (§4.7) ----

#[test]
fn optimization_builds_on_a_first_class_weight_at_priority() {
    let element = OptimizeElement::new(weight(1).at_priority(2), [var("X")], Condition::empty());

    let minimized = minimize([element.clone()]);
    assert_eq!(minimized.direction, Direction::Minimize);
    assert_eq!(minimized.elements().count(), 1);

    let maximized = maximize([element]);
    assert_eq!(maximized.direction, Direction::Maximize);

    let weak = WeakConstraint::new(
        Body::new([BodyElement::from(Atom::new(name("p"), [var("X")]))]),
        weight(1).at_priority(2),
        [var("X")],
    );
    assert_eq!(weak.weight(), &weight(1).at_priority(2));
}

// ---- The two audiences, one value (the first-solve construction seed, §7.3, §16) ----

/// A small reachability program built through the declarative surface: facts, rules
/// with bodies, arithmetic, and a constraint under default negation.
fn reachability_through_the_surface() -> Program {
    let edge_fact = Rule::fact(Atom::new(name("edge"), [Term::from(1), Term::from(2)]));

    let reach_base = Atom::new(name("reach"), [var("X"), var("Y")])
        .into_head()
        .when(Atom::new(name("edge"), [var("X"), var("Y")]));

    let reach_step = Atom::new(name("reach"), [var("X"), var("Z")])
        .into_head()
        .when([
            Atom::new(name("reach"), [var("X"), var("Y")]),
            Atom::new(name("edge"), [var("Y"), var("Z")]),
        ]);

    let step = Atom::new(name("step"), [var("X"), var("X") + 1])
        .into_head()
        .when(Atom::new(name("edge"), [var("X"), var("Y")]));

    let must_edge = Rule::constraint(not(Atom::new(name("edge"), [Term::from(1), Term::from(2)])));

    Program::of([edge_fact, reach_base, reach_step, step, must_edge])
}

/// The same program assembled from the primitive constructors — explicitly built
/// heads, bodies, and literals.
fn reachability_through_the_primitives() -> Program {
    let atom = |predicate: &str, arguments: Vec<Term>| Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    };
    let head = |predicate: &str, arguments: Vec<Term>| {
        Head::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(predicate, arguments))),
        })
    };
    let body_atom = |predicate: &str, arguments: Vec<Term>| {
        BodyElement::Literal(Literal {
            negation: DefaultNegation::None,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(predicate, arguments))),
        })
    };

    let edge_fact = Rule::new(head("edge", vec![num(1), num(2)]), Body::empty());

    let reach_base = Rule::new(
        head("reach", vec![var("X"), var("Y")]),
        Body::new([body_atom("edge", vec![var("X"), var("Y")])]),
    );

    let reach_step = Rule::new(
        head("reach", vec![var("X"), var("Z")]),
        Body::new([
            body_atom("reach", vec![var("X"), var("Y")]),
            body_atom("edge", vec![var("Y"), var("Z")]),
        ]),
    );

    let step = Rule::new(
        head(
            "step",
            vec![
                var("X"),
                Term::BinaryOperation {
                    operator: BinaryOp::Add,
                    left: Box::new(var("X")),
                    right: Box::new(num(1)),
                },
            ],
        ),
        Body::new([body_atom("edge", vec![var("X"), var("Y")])]),
    );

    let must_edge = Rule::new(
        Head::Falsum,
        Body::new([BodyElement::Literal(Literal {
            negation: DefaultNegation::Not,
            inner: LiteralInner::Atom(WithProvenance::constructed(atom(
                "edge",
                vec![num(1), num(2)],
            ))),
        })]),
    );

    Program::of([edge_fact, reach_base, reach_step, step, must_edge])
}

#[test]
fn the_two_audiences_converge_on_one_value() {
    let declarative = reachability_through_the_surface();
    let primitive = reachability_through_the_primitives();
    assert_eq!(
        declarative, primitive,
        "the declarative surface and the primitive constructors build one program",
    );
    // Both are canonical: ingesting the program's own statements changes nothing.
    assert_eq!(
        declarative,
        Program::of_nodes(declarative.statements().cloned())
    );
}

// ---- The statement coercion class, and the bare assembly door (§4.2, §7.1) ----
//
// For every statement family `X`: `From<X> for Statement`, the variant wrapped as built
// (canonicalization is the ingest door's, §6.3). `Program::of` takes bare values through
// that class — a `Statement` itself by the reflexive `From` — mints a `Constructed` origin
// for each, and routes them through the provenance door `Program::of_nodes`, so the one
// ingest door canonicalizes and merges exactly as it does for a carried node.

/// The nullary atom `p`.
fn p() -> Atom {
    Atom::constant(name("p"))
}

#[test]
fn every_statement_family_coerces_to_its_variant() {
    let rule = Rule::fact(p());
    assert_eq!(Statement::from(rule.clone()), Statement::Rule(rule));

    let weak = WeakConstraint::new(Body::empty(), weight(1), []);
    assert_eq!(
        Statement::from(weak.clone()),
        Statement::WeakConstraint(weak)
    );

    let element = OptimizeElement::new(weight(1), [], Condition::empty());
    let optimize = Optimize::new(Direction::Minimize, [element]);
    assert_eq!(
        Statement::from(optimize.clone()),
        Statement::Optimize(optimize)
    );

    assert_eq!(Statement::from(Show::All), Statement::Show(Show::All));

    let project = Project::atom_body(p(), Body::empty());
    assert_eq!(
        Statement::from(project.clone()),
        Statement::Project(project)
    );

    let defined = Defined {
        signature: Signature {
            sign: Sign::Positive,
            name: name("p"),
            arity: 0,
        },
    };
    assert_eq!(
        Statement::from(defined.clone()),
        Statement::Defined(defined)
    );

    let edge = Edge::new([(Term::from(1), Term::from(2))], Body::empty());
    assert_eq!(Statement::from(edge.clone()), Statement::Edge(edge));

    let heuristic = Heuristic::new(p(), Body::empty(), 1, None, Term::constant(name("sign")));
    assert_eq!(
        Statement::from(heuristic.clone()),
        Statement::Heuristic(heuristic)
    );

    let external = External::new(p(), Body::empty(), None);
    assert_eq!(
        Statement::from(external.clone()),
        Statement::External(external)
    );

    let constant = Const {
        name: name("n"),
        value: Term::from(1),
        policy: None,
    };
    assert_eq!(
        Statement::from(constant.clone()),
        Statement::Const(constant)
    );

    let include = Include::new(IncludeTarget::Path("file.lp".to_owned()));
    assert_eq!(
        Statement::from(include.clone()),
        Statement::Include(include)
    );

    let script = Script::new(name("python"), "pass");
    assert_eq!(Statement::from(script.clone()), Statement::Script(script));

    let theory = TheoryDefinition {
        name: name("t"),
        terms: BTreeSet::new(),
        atoms: BTreeSet::new(),
    };
    assert_eq!(
        Statement::from(theory.clone()),
        Statement::TheoryDefinition(theory)
    );

    let query = Query::new(p());
    assert_eq!(Statement::from(query.clone()), Statement::Query(query));
}

#[test]
fn a_program_assembles_from_bare_rules() {
    // No carrier and no variant at the element: the rules go in as written.
    let program = Program::of([Rule::fact(p()), Rule::fact(Atom::constant(name("q")))]);
    let admitted: Vec<&WithProvenance<Statement>> = program.base().statements().collect();
    assert_eq!(admitted.len(), 2, "both rules join the base part");
    for statement in admitted {
        assert!(matches!(statement.get(), Statement::Rule(_)));
        assert_eq!(
            statement.provenance().origins().collect::<Vec<_>>(),
            vec![&Origin::Constructed],
            "the bare door mints a Constructed origin"
        );
    }
}

#[test]
fn mixed_families_assemble_through_statement() {
    // The collection is homogeneous, so two families meet at `Statement`.
    let program = Program::of([Statement::from(Rule::fact(p())), Statement::from(Show::All)]);
    assert_eq!(program.base().statements().count(), 2);
    assert!(
        program
            .statements()
            .any(|statement| matches!(statement.get(), Statement::Rule(_)))
    );
    assert!(
        program
            .statements()
            .any(|statement| matches!(statement.get(), Statement::Show(Show::All)))
    );
}
