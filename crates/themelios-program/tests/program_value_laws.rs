//! Laws of the assembled `Program` value (docs/design/program.md §4, §5, §12.1): the
//! set-shaped children are sets (a duplicate vanishes, a reordering is the same value);
//! a program's equality is canonical-form equality up to provenance, and the ingest door
//! canonicalizes the terms it admits so a raw and a collapsed spelling are one value; and
//! the structural substrate reports a rule's variables, groundness, and head and body
//! predicate signatures, a predicate inside a negated aggregate carrying two dependency
//! modes.

use themelios_program::analyze::DependencyKind;
use themelios_program::program::{
    Aggregate, AggregateFunction, Arguments, Atom, Body, BodyAggregateElement, BodyElement,
    Condition, DefaultNegation, Disjunction, DisjunctionElement, FunctionAggregate, Head, Literal,
    LiteralInner, PartKey, Program, Rule, Statement,
};
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{Term, Variable};

fn name(text: &str) -> Name {
    Name::new(text).expect("a lowercase identifier")
}

fn named(text: &str) -> Variable {
    Variable::Named(VarName::new(text).expect("a variable"))
}

fn var(text: &str) -> Term {
    Term::Variable(named(text))
}

fn atom(predicate: &str, arguments: Vec<Term>) -> Atom {
    Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    }
}

fn positive(atom: Atom) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
}

fn negative(atom: Atom) -> Literal {
    Literal {
        negation: DefaultNegation::Not,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
}

fn signature(predicate: &str, arity: u32) -> Signature {
    Signature {
        sign: Sign::Positive,
        name: name(predicate),
        arity,
    }
}

#[test]
fn a_body_is_a_set() {
    let p = || BodyElement::Literal(positive(atom("p", vec![])));
    let q = || BodyElement::Literal(positive(atom("q", vec![])));

    let with_duplicate = Body::new([p(), q(), p()]);
    assert_eq!(with_duplicate.elements().count(), 2);

    let one_order = Body::new([p(), q()]);
    let other_order = Body::new([q(), p()]);
    assert_eq!(one_order, other_order);
}

#[test]
fn a_disjunction_is_a_set_a_duplicate_head_atom_vanishes() {
    let element = || DisjunctionElement::new(positive(atom("a", vec![])), Condition::empty());
    let disjunction = Disjunction::new([element(), element()]);
    assert_eq!(disjunction.elements().count(), 1);
}

#[test]
fn a_rule_with_its_body_in_two_orders_is_one_rule() {
    let head = || positive(atom("p", vec![]));
    let one = Rule::new(
        head(),
        Body::new([
            BodyElement::Literal(positive(atom("q", vec![]))),
            BodyElement::Literal(positive(atom("r", vec![]))),
        ]),
    );
    let other = Rule::new(
        head(),
        Body::new([
            BodyElement::Literal(positive(atom("r", vec![]))),
            BodyElement::Literal(positive(atom("q", vec![]))),
        ]),
    );
    assert_eq!(one, other);
}

#[test]
fn program_equality_is_canonical_form_equality_up_to_provenance() {
    let rule = || Statement::Rule(Rule::new(positive(atom("p", vec![])), Body::empty()));
    let here = WithProvenance::new(rule(), Provenance::from(Origin::Constructed));
    let there = WithProvenance::new(
        rule(),
        Provenance::from(Origin::Transformed(
            themelios_program::provenance::TransformTag::new("t"),
        )),
    );
    // Same rule, different provenance: the two programs are equal.
    assert_eq!(Program::of_nodes([here]), Program::of_nodes([there]));
}

#[test]
fn the_ingest_canonicalizes_the_terms_it_admits() {
    // `p(f(1))` built with a raw ground `Function` argument, and built with the collapsed
    // `Symbol`, are one value once each passes the ingest door (§5.1 ground collapse).
    let raw = Rule::new(
        positive(atom(
            "p",
            vec![Term::Function {
                name: name("f"),
                arguments: vec![Term::Symbolic(Symbol::Number(1))],
            }],
        )),
        Body::empty(),
    );
    let collapsed = Rule::new(
        positive(atom(
            "p",
            vec![Term::Symbolic(Symbol::Function {
                name: name("f"),
                arguments: vec![Symbol::Number(1)],
                sign: Sign::Positive,
            })],
        )),
        Body::empty(),
    );
    // The two rules differ before the door — the raw one is not canonical.
    assert_ne!(raw, collapsed);
    // After the door, the programs are equal.
    let raw_program = Program::of([raw]);
    let collapsed_program = Program::of([collapsed]);
    assert_eq!(raw_program, collapsed_program);
}

#[test]
fn the_base_part_is_always_present_and_holds_the_admitted_statements() {
    let program = Program::of([Rule::new(positive(atom("p", vec![])), Body::empty())]);
    assert_eq!(program.base().statements().count(), 1);
    assert_eq!(program.statements().count(), 1);
}

#[test]
fn the_substrate_reports_the_variables_groundness_and_predicates_of_a_rich_rule() {
    // p(X) :- q(X), not r(Y).
    let rule = Rule::new(
        positive(atom("p", vec![var("X")])),
        Body::new([
            BodyElement::Literal(positive(atom("q", vec![var("X")]))),
            BodyElement::Literal(negative(atom("r", vec![var("Y")]))),
        ]),
    );

    let variables: Vec<Variable> = rule.variables().cloned().collect();
    assert_eq!(variables, vec![named("X"), named("Y")]);
    assert!(!rule.is_ground());

    let head: Vec<Signature> = rule.head_signatures().collect();
    assert_eq!(head, vec![signature("p", 1)]);

    let body: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    assert_eq!(
        body,
        vec![
            (DependencyKind::Positive, signature("q", 1)),
            (DependencyKind::Negative, signature("r", 1)),
        ]
    );
}

#[test]
fn a_ground_rule_reports_is_ground_and_no_variables() {
    let rule = Rule::new(
        positive(atom("p", vec![Term::Symbolic(Symbol::Number(1))])),
        Body::empty(),
    );
    assert!(rule.is_ground());
    assert_eq!(rule.variables().count(), 0);
}

#[test]
fn a_predicate_inside_a_negated_aggregate_yields_both_through_aggregate_and_negative() {
    // :- not #count { X : s(X) }.  — the aggregate is default-negated.
    let element = BodyAggregateElement::new(
        [var("X")],
        Condition::new([positive(atom("s", vec![var("X")]))]),
    );
    let aggregate = Aggregate::Function(FunctionAggregate::new(
        None,
        AggregateFunction::Count,
        [element],
        None,
    ));
    let rule = Rule::new(
        Head::Falsum,
        Body::new([BodyElement::Aggregate {
            negation: DefaultNegation::Not,
            aggregate,
        }]),
    );

    let body: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    assert_eq!(
        body,
        vec![
            (DependencyKind::ThroughAggregate, signature("s", 1)),
            (DependencyKind::Negative, signature("s", 1)),
        ]
    );
}

#[test]
fn the_empty_program_has_a_present_empty_base_and_one_form() {
    let empty = Program::default();
    // base() is total on the default program — no panic (§4.1).
    assert_eq!(empty.base().statements().count(), 0);
    assert!(empty.statements().next().is_none());
    // default() and of over no statements denote the one empty program.
    assert_eq!(empty, Program::of(Vec::<Statement>::new()));
}

#[test]
fn the_named_empty_program_is_the_default_program() {
    // The family names its empty case — `Body::empty`, `Condition::empty` — and `Program::empty`
    // is that name for the one empty program (§7.1), the value `Default` gives.
    assert_eq!(Program::empty(), Program::default());
}

#[test]
fn the_named_empty_program_holds_only_the_present_empty_base() {
    // The base part is present and empty (§4.1): the one part, holding no statement.
    let empty = Program::empty();
    assert_eq!(empty.base().statements().count(), 0);
    assert_eq!(empty.parts().count(), 1);
}

#[test]
fn the_bare_door_is_the_provenance_door_over_a_constructed_wrap() {
    // `of` over a bare statement is exactly `of_nodes` over that statement under a
    // `Constructed` origin — one door behind two spellings, so the ingest's
    // canonicalization and merge are the same on either.
    let rule = || Rule::new(positive(atom("p", vec![])), Body::empty());
    let bare = Program::of([Statement::Rule(rule())]);
    let carried = Program::of_nodes([WithProvenance::constructed(Statement::Rule(rule()))]);
    assert_eq!(bare, carried);
    // Program equality erases provenance (§6.2); the admitted nodes agree on it as well.
    let origins = |program: &Program| -> Vec<Origin> {
        program
            .statements()
            .flat_map(|statement| statement.provenance().origins().cloned())
            .collect()
    };
    assert_eq!(origins(&bare), origins(&carried));
    assert_eq!(origins(&bare), vec![Origin::Constructed]);
}

#[test]
fn a_predicate_in_a_head_element_condition_is_a_dependency() {
    // `a : b.` — a is derived under the condition b, so a depends on b (the grounder tracks
    // it: `a : b.` with `b :- a.` is unsatisfiable). The edge must reach body_signatures.
    let rule = Rule::new(
        Head::Disjunction(Disjunction::new([DisjunctionElement::new(
            positive(atom("a", vec![])),
            Condition::new([positive(atom("b", vec![]))]),
        )])),
        Body::empty(),
    );
    let head: Vec<Signature> = rule.head_signatures().collect();
    assert_eq!(head, vec![signature("a", 0)]); // a is derived
    let body: Vec<(DependencyKind, Signature)> = rule.body_signatures().collect();
    assert_eq!(body, vec![(DependencyKind::Positive, signature("b", 0))]); // and depends on b
}

#[test]
fn each_anonymous_variable_is_distinct_while_a_named_one_is_deduped() {
    // p(X, X, _, _): X once (named, deduped); each _ a distinct fresh variable, as the
    // grounder treats `_` (§12.1).
    let anon = || Term::Variable(Variable::Anonymous);
    let rule = Rule::new(
        positive(atom("p", vec![var("X"), var("X"), anon(), anon()])),
        Body::empty(),
    );
    let variables: Vec<Variable> = rule.variables().cloned().collect();
    assert_eq!(
        variables,
        vec![named("X"), Variable::Anonymous, Variable::Anonymous]
    );
}

// --- `Program::of_keyed_nodes`: the public multi-part construction door (§7.1) ---

fn part_key(part: &str, formals: &[&str]) -> PartKey {
    PartKey {
        name: name(part),
        formals: formals.iter().map(|formal| name(formal)).collect(),
    }
}

fn fact_node(predicate: &str) -> WithProvenance<Statement> {
    WithProvenance::constructed(Statement::Rule(Rule::fact(atom(predicate, vec![]))))
}

#[test]
fn of_keyed_nodes_places_each_node_in_the_part_its_key_names() {
    let base = part_key("base", &[]);
    let step = part_key("step", &["t"]);
    let program = Program::of_keyed_nodes([
        (base.clone(), fact_node("p")),
        (step.clone(), fact_node("q")),
    ]);
    assert_eq!(
        program
            .part(&base)
            .expect("base present")
            .statements()
            .count(),
        1
    );
    assert_eq!(
        program
            .part(&step)
            .expect("step(t) opened")
            .statements()
            .count(),
        1
    );
    assert_eq!(program.parts().count(), 2);
}

#[test]
fn of_keyed_nodes_keeps_content_equal_statements_under_different_keys_distinct() {
    // Part identity (§4.1): the SAME statement under two different part keys stays two
    // statements, one per part — never merged. The invariant that distinguishes multi-part
    // construction from single-part, and the reason the door carries a `PartKey` per node.
    let step_t = part_key("step", &["t"]);
    let step_u = part_key("step", &["u"]);
    let program = Program::of_keyed_nodes([
        (step_t.clone(), fact_node("p")),
        (step_u.clone(), fact_node("p")),
    ]);
    assert_eq!(
        program.part(&step_t).expect("step(t)").statements().count(),
        1
    );
    assert_eq!(
        program.part(&step_u).expect("step(u)").statements().count(),
        1
    );
    assert_eq!(program.statements().count(), 2); // two statements total — not one merged
}

#[test]
fn of_keyed_nodes_merges_content_equal_statements_under_the_same_key_unioning_provenance() {
    let key = part_key("step", &["t"]);
    let origin = |tag: &str| Provenance::from(Origin::Transformed(TransformTag::new(tag)));
    let rule = || Statement::Rule(Rule::fact(atom("p", vec![])));
    let here = WithProvenance::new(rule(), origin("here"));
    let there = WithProvenance::new(rule(), origin("there"));
    let program = Program::of_keyed_nodes([(key.clone(), here), (key.clone(), there)]);
    assert_eq!(program.part(&key).expect("step(t)").statements().count(), 1);
    let origins: Vec<Origin> = program
        .statements()
        .flat_map(|statement| statement.provenance().origins().cloned())
        .collect();
    assert!(origins.contains(&Origin::Transformed(TransformTag::new("here"))));
    assert!(origins.contains(&Origin::Transformed(TransformTag::new("there"))));
}

#[test]
fn of_keyed_nodes_seeds_a_present_base_even_with_no_base_node() {
    let program = Program::of_keyed_nodes([(part_key("step", &["t"]), fact_node("p"))]);
    assert_eq!(program.base().statements().count(), 0); // base present and empty (§4.1)
    assert!(program.part(&part_key("base", &[])).is_some());
}

#[test]
fn of_keyed_nodes_is_invariant_under_input_permutation() {
    let a = (part_key("base", &[]), fact_node("p"));
    let b = (part_key("step", &["t"]), fact_node("q"));
    let one = Program::of_keyed_nodes([a.clone(), b.clone()]);
    let other = Program::of_keyed_nodes([b, a]);
    assert_eq!(one, other);
}

#[test]
fn of_keyed_nodes_round_trips_a_multi_part_program_through_its_public_parts() {
    let original = Program::of_keyed_nodes([
        (part_key("base", &[]), fact_node("p")),
        (part_key("step", &["t"]), fact_node("q")),
        (part_key("step", &["u"]), fact_node("q")),
    ]);
    let keyed: Vec<(PartKey, WithProvenance<Statement>)> = original
        .parts()
        .flat_map(|part| part.statements().map(|s| (part.key().clone(), s.clone())))
        .collect();
    assert_eq!(Program::of_keyed_nodes(keyed), original);
}
