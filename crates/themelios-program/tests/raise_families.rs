//! The raise across every statement, directive, aggregate, and theory family
//! (docs/design/program.md §8): each construct the grammar admits lowers to its value,
//! read from the tree. Where raise_laws.rs holds the six positional corners, this holds
//! the breadth — every family and every branch of the lowering, so a member of the
//! language raises to the value it denotes with no diagnostic.

use themelios_base::source::{Source, SourceId};

use themelios_program::program::{
    Aggregate, AggregateFunction, BodyElement, Const, ConstPolicy, DefaultNegation, Direction,
    Head, Literal, LiteralInner, Optimize, Project, Relation, Rule, Show, Statement,
};
use themelios_program::raise::{Raised, raise};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

// ---- harness ----

fn raised(text: &str) -> Raised {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    raise(&parse(&source, Dialect::Clingo))
}

fn raised_aspcore(text: &str) -> Raised {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    raise(&parse(&source, Dialect::AspCore2))
}

/// Raise a member program and assert it lowers with no diagnostic.
fn clean(text: &str) -> Raised {
    let raised = raised(text);
    assert!(
        raised.diagnostics().is_empty(),
        "unexpected lowering diagnostics for `{text}`: {:?}",
        raised.diagnostics()
    );
    raised
}

/// The single statement of a program that holds exactly one.
fn only(text: &str) -> Statement {
    let raised = clean(text);
    let statements: Vec<_> = raised
        .program()
        .statements()
        .map(|statement| statement.get().clone())
        .collect();
    assert_eq!(
        statements.len(),
        1,
        "one statement for `{text}`: {statements:?}"
    );
    statements.into_iter().next().expect("one statement")
}

fn only_rule(text: &str) -> Rule {
    match only(text) {
        Statement::Rule(rule) => rule,
        other => panic!("`{text}` is not a rule: {other:?}"),
    }
}

fn first_body_element(rule: &Rule) -> BodyElement {
    rule.body()
        .get()
        .elements()
        .next()
        .expect("a body element")
        .get()
        .clone()
}

// ---- the body-free directives (§4.8) ----

#[test]
fn show_lowers_all_four_forms() {
    assert!(matches!(only("#show."), Statement::Show(Show::All)));
    assert!(matches!(
        only("#show p/1."),
        Statement::Show(Show::Signature(_))
    ));
    assert!(matches!(only("#show p."), Statement::Show(Show::Term(_))));
    assert!(matches!(
        only("#show f(X) : q(X)."),
        Statement::Show(Show::TermBody { .. })
    ));
    // A strongly-negated signature reads its sign.
    assert!(matches!(
        only("#show -p/2."),
        Statement::Show(Show::Signature(_))
    ));
}

#[test]
fn project_lowers_signature_and_atom_forms() {
    assert!(matches!(
        only("#project p/1."),
        Statement::Project(Project::Signature(_))
    ));
    assert!(matches!(
        only("#project a : b, c."),
        Statement::Project(Project::Atom { .. })
    ));
}

#[test]
fn defined_edge_heuristic_and_external_lower() {
    assert!(matches!(only("#defined p/1."), Statement::Defined(_)));
    let Statement::Edge(edge) = only("#edge (a, b; b, c).") else {
        panic!("an edge")
    };
    assert_eq!(edge.pairs().count(), 2);
    let Statement::Heuristic(heuristic) = only("#heuristic a : b. [3@2, sign]") else {
        panic!("a heuristic")
    };
    assert!(heuristic.priority().is_some());
    let Statement::External(external) = only("#external p(X) : q(X). [false]") else {
        panic!("an external")
    };
    assert!(external.value().is_some());
    // An external with no value annotation.
    assert!(matches!(only("#external p."), Statement::External(_)));
}

#[test]
fn script_and_include_lower() {
    let Statement::Script(script) = only("#script (lua) x = 1 #end.") else {
        panic!("a script")
    };
    assert_eq!(script.language().as_str(), "lua");
    // Both include targets.
    assert!(matches!(
        only("#include \"file.lp\"."),
        Statement::Include(_)
    ));
    assert!(matches!(only("#include <incmode>."), Statement::Include(_)));
}

#[test]
fn const_lowers_with_and_without_a_policy() {
    let Statement::Const(Const { policy: None, .. }) = only("#const a = 1.") else {
        panic!("a policy-free const")
    };
    let Statement::Const(Const {
        policy: Some(ConstPolicy::Default),
        ..
    }) = only("#const b = 2. [default]")
    else {
        panic!("a default const")
    };
    let Statement::Const(Const {
        policy: Some(ConstPolicy::Override),
        ..
    }) = only("#const c = 3. [override]")
    else {
        panic!("an override const")
    };
}

#[test]
fn a_query_lowers_under_asp_core_2() {
    let raised = raised_aspcore("a. b :- a. c(1)?");
    assert!(
        raised.diagnostics().is_empty(),
        "{:?}",
        raised.diagnostics()
    );
    assert!(
        raised
            .program()
            .statements()
            .any(|statement| matches!(statement.get(), Statement::Query(_))),
        "the last form is a query"
    );
}

// ---- optimization (§4.7) ----

#[test]
fn weak_constraints_and_optimize_lower() {
    let Statement::WeakConstraint(weak) = only(":~ p(X). [X@1, X, a]") else {
        panic!("a weak constraint")
    };
    assert!(weak.weight().priority().is_some());
    assert_eq!(weak.terms().count(), 2);

    let Statement::Optimize(minimize) = only("#minimize { X@1 : p(X); 1 : q }.") else {
        panic!("a minimize")
    };
    assert_eq!(minimize.direction, Direction::Minimize);
    assert_eq!(minimize.elements().count(), 2);

    let Statement::Optimize(Optimize {
        direction: Direction::Maximize,
        ..
    }) = only("#maximize { W : r(W) }.")
    else {
        panic!("a maximize")
    };
}

// ---- heads: disjunction, choice, head aggregate (§4.4) ----

#[test]
fn a_disjunctive_head_lowers_with_and_without_conditions() {
    let rule = only_rule("a | b(X) : q(X) | c :- d.");
    let Head::Disjunction(disjunction) = rule.head().get() else {
        panic!("a disjunction, got {:?}", rule.head().get())
    };
    assert_eq!(disjunction.elements().count(), 3);
}

#[test]
fn a_choice_head_lowers_its_guards_and_conditioned_elements() {
    let rule = only_rule("1 { a; b(X) : q(X) } 2 :- d.");
    let Head::Choice(choice) = rule.head().get() else {
        panic!("a choice, got {:?}", rule.head().get())
    };
    assert!(choice.left_guard().is_some());
    assert!(choice.right_guard().is_some());
    assert_eq!(choice.elements().count(), 2);
}

#[test]
fn a_head_function_aggregate_lowers_to_a_head_aggregate() {
    let rule = only_rule("#count { X : p(X) } = 1 :- q.");
    let Head::Aggregate(aggregate) = rule.head().get() else {
        panic!("a head aggregate, got {:?}", rule.head().get())
    };
    assert_eq!(aggregate.function(), AggregateFunction::Count);
    assert_eq!(aggregate.elements().count(), 1);
}

#[test]
fn a_true_head_folds_to_verum() {
    assert!(matches!(only_rule("#true.").head().get(), Head::Verum));
}

#[test]
fn a_false_head_folds_to_falsum() {
    assert!(matches!(
        only_rule("#false :- b.").head().get(),
        Head::Falsum
    ));
}

// ---- bodies: aggregates, conditionals, comparisons, boolean literals (§4.5, §4.6) ----

#[test]
fn a_body_function_aggregate_lowers_every_function_and_its_guards() {
    // Each aggregate function reaches its lowering.
    for (text, function) in [
        (":- 2 <= #count { a } <= 5.", AggregateFunction::Count),
        (":- #sum { W : p(W) } < 3.", AggregateFunction::Sum),
        (":- #sum+ { W : p(W) } < 3.", AggregateFunction::SumPlus),
        (":- #min { W : p(W) } != 3.", AggregateFunction::Min),
        (":- #max { W : p(W) } >= 3.", AggregateFunction::Max),
    ] {
        let rule = only_rule(text);
        match first_body_element(&rule) {
            BodyElement::Aggregate {
                aggregate: Aggregate::Function(aggregate),
                ..
            } => assert_eq!(aggregate.function(), function, "for `{text}`"),
            other => panic!("`{text}` is a function aggregate, got {other:?}"),
        }
    }
}

#[test]
fn a_negated_body_aggregate_carries_its_default_negation() {
    let rule = only_rule(":- not #count { a } < 1.");
    assert!(matches!(
        first_body_element(&rule),
        BodyElement::Aggregate {
            negation: DefaultNegation::Not,
            ..
        }
    ));
}

#[test]
fn a_conditional_literal_lowers_with_its_condition() {
    let rule = only_rule("q :- p(X) : r(X), s(X).");
    let BodyElement::Conditional(conditional) = first_body_element(&rule) else {
        panic!("a conditional literal")
    };
    assert_eq!(conditional.condition.literals().count(), 2);
}

#[test]
fn a_comparison_chain_lowers_every_relation() {
    let rule = only_rule(":- 1 < 2 <= 3 > X >= Y = Z != W.");
    let BodyElement::Literal(Literal {
        inner: LiteralInner::Comparison(comparison),
        ..
    }) = first_body_element(&rule)
    else {
        panic!("a comparison")
    };
    let relations: Vec<Relation> = comparison
        .get()
        .steps()
        .map(|(relation, _)| relation)
        .collect();
    assert_eq!(
        relations,
        [
            Relation::Lt,
            Relation::Le,
            Relation::Gt,
            Relation::Ge,
            Relation::Eq,
            Relation::Neq
        ]
    );
}

#[test]
fn boolean_body_literals_and_double_negation_lower() {
    let rule = only_rule(":- #true, not #false, not not p.");
    let negations: Vec<DefaultNegation> = rule
        .body()
        .get()
        .elements()
        .map(|element| match element.get() {
            BodyElement::Literal(literal) => literal.negation,
            other => panic!("a literal, got {other:?}"),
        })
        .collect();
    assert!(negations.contains(&DefaultNegation::NotNot));
    let inners: Vec<bool> = rule
        .body()
        .get()
        .elements()
        .map(|element| {
            matches!(
                element.get(),
                BodyElement::Literal(Literal {
                    inner: LiteralInner::True | LiteralInner::False,
                    ..
                })
            )
        })
        .collect();
    assert_eq!(inners.iter().filter(|held| **held).count(), 2);
}

// ---- theory atoms and definitions (§4.9) ----

#[test]
fn a_body_theory_atom_lowers_its_terms_functions_and_guard() {
    // A rich theory atom: a function, a list, a tuple, a set, a variable, and an
    // operation opterm, under a condition, with a functional guard — the theory-term
    // raise across its forms.
    let rule = only_rule(":- &sum { f(a), [1], (b, c), X, p - q : cond ; {z} } <= g(1).");
    let BodyElement::TheoryAtom { atom, .. } = first_body_element(&rule) else {
        panic!("a theory atom")
    };
    assert_eq!(atom.name().as_str(), "sum");
    assert_eq!(atom.elements().count(), 2);
    assert!(atom.guard().is_some());
}

#[test]
fn a_head_theory_atom_lowers() {
    let rule = only_rule("&a { x } :- b.");
    assert!(matches!(rule.head().get(), Head::TheoryAtom(_)));
}

#[test]
fn a_theory_definition_lowers_its_term_and_atom_definitions() {
    let Statement::TheoryDefinition(definition) = only(
        "#theory t { \
           expr { + : 0x1, binary, left; - : 2, unary; * : 3, binary, right }; \
           &a/0 : expr, {<=, >=}, expr, any; \
           &b/1 : expr, body \
         }.",
    ) else {
        panic!("a theory definition")
    };
    assert_eq!(definition.name.as_str(), "t");
    assert_eq!(definition.terms.len(), 1);
    assert_eq!(definition.atoms.len(), 2);
}

// ---- per-node parsed provenance rides the nested nodes (§6.1) ----

#[test]
fn duplicate_facts_union_their_provenance_at_the_statement_door() {
    // Two content-equal facts from distinct spans collapse to one whose provenance
    // unions both parsed origins — the statement-level union the ingest door keeps.
    let raised = clean("p(1). p(1).");
    let facts: Vec<_> = raised.program().base().statements().collect();
    assert_eq!(facts.len(), 1, "the content-equal facts collapse to one");
    let parsed = facts[0]
        .provenance()
        .origins()
        .filter(|origin| matches!(origin, themelios_program::provenance::Origin::Parsed(_)))
        .count();
    assert!(
        parsed >= 2,
        "both spans' provenance is unioned, got {parsed}"
    );
}
