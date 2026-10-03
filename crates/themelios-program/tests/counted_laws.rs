//! Laws of the counted elements (docs/design/program.md §4.4): a counted collection keeps a
//! repeat the authority counts at every occurrence — a comparison or boolean element — and
//! merges a repeat it keys by content — an atom element — whichever door built it: the raise,
//! construction, substitution, `unpool`, or canonicalization.

use themelios_base::source::{Source, SourceId};
use themelios_program::program::{
    Aggregate, Arguments, Atom, Body, BodyElement, Choice, ChoiceElement, Comparison, Condition,
    ConditionalLiteral, DefaultNegation, Head, Identity, Literal, LiteralInner, Program, Relation,
    Rule, SetAggregate, SetElement, Statement, TheoryAtom, TheoryElement, TheoryTerm,
};
use themelios_program::provenance::TransformTag;
use themelios_program::provenance::{Origin, WithProvenance};
use themelios_program::raise::raise;
use themelios_program::render::render;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::transform::{Rewrite, rewrite, unpool};
use themelios_program::unify::{mgu, substitute};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

// ---- harness ----

/// Raise a whole program under the clingo dialect, refusing a fixture that raises with a
/// diagnostic.
fn raised(text: &str) -> Program {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let raised = raise(&parse(&source, Dialect::Clingo));
    assert!(
        raised.diagnostics().is_empty(),
        "fixture raises cleanly: {text}"
    );
    raised.into_program()
}

/// The program's one rule.
fn only_rule(program: &Program) -> Rule {
    let rules: Vec<&Rule> = program
        .statements()
        .filter_map(|node| match node.get() {
            Statement::Rule(rule) => Some(rule),
            _ => None,
        })
        .collect();
    assert_eq!(rules.len(), 1, "the fixture has one rule");
    rules[0].clone()
}

/// A rule's choice head.
fn choice_of(rule: &Rule) -> Choice {
    match rule.head().get() {
        Head::Choice(choice) => choice.clone(),
        other => panic!("a choice head, not {other:?}"),
    }
}

/// The parsed origins of an element, in their order.
fn parsed_origins<T>(node: &WithProvenance<T>) -> Vec<Origin> {
    node.provenance()
        .origins()
        .filter(|origin| matches!(origin, Origin::Parsed(_)))
        .cloned()
        .collect()
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a lowercase identifier")
}

fn var(text: &str) -> Term {
    Term::Variable(Variable::Named(VarName::new(text).expect("a variable")))
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn atom(predicate: &str, arguments: Vec<Term>) -> Atom {
    Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(arguments),
    }
}

fn literal(negation: DefaultNegation, inner: LiteralInner) -> Literal {
    Literal { negation, inner }
}

fn atom_literal(negation: DefaultNegation) -> Literal {
    literal(
        negation,
        LiteralInner::Atom(WithProvenance::constructed(atom("a", vec![]))),
    )
}

fn comparison_literal(negation: DefaultNegation) -> Literal {
    let comparison = Comparison::new(num(1), Relation::Lt, num(2));
    literal(
        negation,
        LiteralInner::Comparison(WithProvenance::constructed(comparison)),
    )
}

fn truth() -> ChoiceElement {
    ChoiceElement::new(
        literal(DefaultNegation::None, LiteralInner::True),
        Condition::empty(),
    )
}

fn plain(predicate: &str) -> ChoiceElement {
    ChoiceElement::new(atom_literal_named(predicate), Condition::empty())
}

fn atom_literal_named(predicate: &str) -> Literal {
    literal(
        DefaultNegation::None,
        LiteralInner::Atom(WithProvenance::constructed(atom(predicate, vec![]))),
    )
}

const NEGATIONS: [DefaultNegation; 3] = [
    DefaultNegation::None,
    DefaultNegation::Not,
    DefaultNegation::NotNot,
];

// ---- the identity (§4.4) ----

#[test]
fn an_atom_literal_is_counted_by_content_under_any_negation() {
    for negation in NEGATIONS {
        assert_eq!(atom_literal(negation).identity(), Identity::ByContent);
    }
}

#[test]
fn a_comparison_literal_is_counted_by_occurrence_under_any_negation() {
    for negation in NEGATIONS {
        assert_eq!(
            comparison_literal(negation).identity(),
            Identity::ByOccurrence
        );
    }
}

#[test]
fn a_boolean_literal_is_counted_by_occurrence_under_any_negation() {
    for negation in NEGATIONS {
        for inner in [LiteralInner::True, LiteralInner::False] {
            assert_eq!(literal(negation, inner).identity(), Identity::ByOccurrence);
        }
    }
}

#[test]
fn a_choice_element_takes_its_literal_s_identity() {
    assert_eq!(plain("a").identity(), Identity::ByContent);
    assert_eq!(truth().identity(), Identity::ByOccurrence);
}

// ---- construction ----

#[test]
fn a_constructed_choice_keeps_a_repeated_boolean_element() {
    assert_eq!(
        Choice::new(None, [truth(), truth()], None)
            .elements()
            .count(),
        2
    );
}

#[test]
fn a_constructed_choice_merges_a_repeated_atom_element() {
    assert_eq!(
        Choice::new(None, [plain("a"), plain("a")], None)
            .elements()
            .count(),
        1
    );
}

#[test]
fn a_choice_does_not_depend_on_its_element_order() {
    assert_eq!(
        Choice::new(None, [plain("b"), truth(), plain("a")], None),
        Choice::new(None, [plain("a"), plain("b"), truth()], None)
    );
}

#[test]
fn a_kept_repeat_makes_a_different_choice() {
    assert_ne!(
        Choice::new(None, [truth(), truth()], None),
        Choice::new(None, [truth()], None)
    );
}

#[test]
fn the_default_choice_has_no_elements() {
    assert_eq!(Choice::default().elements().count(), 0);
}

#[test]
fn a_choice_orders_after_a_prefix_of_its_entries() {
    // The entries compare in order, as a set's elements do (§5.2), so a kept repeat orders the
    // choice after the one without it.
    assert!(Choice::new(None, [truth()], None) < Choice::new(None, [truth(), truth()], None));
}

// ---- the raise ----

#[test]
fn a_raised_repeated_boolean_element_keeps_each_occurrence() {
    let choice = choice_of(&only_rule(&raised("1 { #true; #true } 1.")));
    let origins: Vec<Vec<Origin>> = choice.elements().map(parsed_origins).collect();
    assert_eq!(origins.len(), 2);
    assert!(
        origins.iter().all(|each| each.len() == 1),
        "each entry has its own origin"
    );
    assert_ne!(
        origins[0], origins[1],
        "the two entries are two occurrences"
    );
}

#[test]
fn a_raised_repeated_atom_element_unions_its_occurrences() {
    let choice = choice_of(&only_rule(&raised("{ a; a }.")));
    let entries: Vec<&WithProvenance<ChoiceElement>> = choice.elements().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(parsed_origins(entries[0]).len(), 2);
}

#[test]
fn a_merged_atom_element_keeps_the_later_occurrence_s_atom() {
    let choice = choice_of(&only_rule(&raised("{ a; a }.")));
    let entry = choice.elements().next().expect("one entry");
    let LiteralInner::Atom(atom) = &entry.get().literal().inner else {
        panic!("an atom element");
    };
    let later = parsed_origins(entry).pop().expect("two origins");
    assert_eq!(parsed_origins(atom), vec![later]);
}

// ---- unpool and substitution ----

#[test]
fn unpool_keeps_a_repeat_it_makes_of_a_boolean_element() {
    let unpooled = unpool(&raised("{ #true : p(1; 1) } = 2."));
    assert_eq!(choice_of(&only_rule(&unpooled)).elements().count(), 2);
}

#[test]
fn unpool_merges_a_repeat_it_makes_of_an_atom_element() {
    let unpooled = unpool(&raised("{ p(1; 1) }."));
    assert_eq!(choice_of(&only_rule(&unpooled)).elements().count(), 1);
}

#[test]
fn substitution_keeps_a_repeat_it_makes_of_a_boolean_element() {
    let rule = only_rule(&raised("{ #true : p(X); #true : p(1) } :- q(X)."));
    let binding = mgu(&atom("p", vec![var("X")]), &atom("p", vec![num(1)]))
        .expect("a pattern")
        .expect("unifiable");
    assert_eq!(choice_of(&substitute(rule, &binding)).elements().count(), 2);
}

#[test]
fn substitution_merges_a_repeat_it_makes_of_an_atom_element() {
    let rule = only_rule(&raised("{ p(X); p(1) } :- q(X)."));
    let binding = mgu(&atom("p", vec![var("X")]), &atom("p", vec![num(1)]))
        .expect("a pattern")
        .expect("unifiable");
    let choice = choice_of(&substitute(rule, &binding));
    let entries: Vec<&WithProvenance<ChoiceElement>> = choice.elements().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        parsed_origins(entries[0]).len(),
        2,
        "both entries' origins survive the merge"
    );
}

// ---- canonicalization ----

#[test]
fn canonicalizing_a_choice_merges_atom_elements_it_makes_equal() {
    // `p((1))` — a one-alternative pool — and `p(1)` differ until canonicalization collapses the
    // pool (§5.1); the ingest door canonicalizes, and the counted constructor then merges the two
    // by-content entries (§4.4).
    let element = |atom: Atom| {
        ChoiceElement::new(
            literal(
                DefaultNegation::None,
                LiteralInner::Atom(WithProvenance::constructed(atom)),
            ),
            Condition::empty(),
        )
    };
    let pooled = Atom {
        sign: Sign::Positive,
        name: name("p"),
        arguments: Arguments::Pooled(vec![vec![num(1)]]),
    };
    let choice = Choice::new(
        None,
        [element(pooled), element(atom("p", vec![num(1)]))],
        None,
    );
    assert_eq!(
        choice.elements().count(),
        2,
        "the two differ until canonicalized"
    );
    let program = Program::of([Rule::new(choice, Body::empty())]);
    assert_eq!(choice_of(&only_rule(&program)).elements().count(), 1);
}

// ---- the rewrite ----

/// A rewrite that changes nothing but the tag.
struct Unchanged;

impl Rewrite for Unchanged {
    fn tag(&self) -> TransformTag {
        TransformTag::new("unchanged")
    }
}

/// A rewrite that binds the variable `X` to `1` wherever it stands.
struct BindX;

impl Rewrite for BindX {
    fn tag(&self) -> TransformTag {
        TransformTag::new("bind-x")
    }

    fn rewrite_term(&mut self, term: Term) -> Term {
        if term == var("X") { num(1) } else { term }
    }
}

#[test]
fn a_rewrite_keeps_a_kept_repeat() {
    let rewritten = rewrite(raised("1 { #true; #true } 1."), &mut Unchanged);
    assert_eq!(choice_of(&only_rule(&rewritten)).elements().count(), 2);
}

#[test]
fn a_rewrite_merges_the_atom_elements_it_makes_equal() {
    let rewritten = rewrite(raised("{ p(X); p(1) } :- q(X)."), &mut BindX);
    let choice = choice_of(&only_rule(&rewritten));
    let entries: Vec<&WithProvenance<ChoiceElement>> = choice.elements().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        parsed_origins(entries[0]).len(),
        2,
        "both entries' origins survive the merge"
    );
}

// ---- render ----

#[test]
fn a_kept_repeat_renders_and_raises_back() {
    let program = raised("1 { #true; #true } 1.");
    let text = render(&program, Dialect::Clingo).expect("renders");
    assert_eq!(raised(&text), program);
}

// ---- set aggregates (§4.7) ----

/// A rule's one body set aggregate.
fn set_aggregate_of(rule: &Rule) -> SetAggregate {
    let sets: Vec<SetAggregate> = rule
        .body()
        .get()
        .elements()
        .filter_map(|element| match element.get() {
            BodyElement::Aggregate {
                aggregate: Aggregate::Set(set),
                ..
            } => Some(set.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(sets.len(), 1, "the rule has one set aggregate");
    sets[0].clone()
}

fn true_set_element() -> SetElement {
    SetElement::Literal(literal(DefaultNegation::None, LiteralInner::True))
}

fn atom_set_element(predicate: &str) -> SetElement {
    SetElement::Literal(atom_literal_named(predicate))
}

#[test]
fn a_set_element_takes_its_literal_s_identity() {
    assert_eq!(true_set_element().identity(), Identity::ByOccurrence);
    let conditional = SetElement::ConditionalLiteral(ConditionalLiteral {
        literal: atom_literal_named("a"),
        condition: Condition::empty(),
    });
    assert_eq!(conditional.identity(), Identity::ByContent);
}

#[test]
fn a_constructed_set_aggregate_keeps_a_repeated_boolean_element() {
    let set = SetAggregate::new(None, [true_set_element(), true_set_element()], None);
    assert_eq!(set.elements().count(), 2);
}

#[test]
fn a_constructed_set_aggregate_merges_a_repeated_atom_element() {
    let set = SetAggregate::new(None, [atom_set_element("a"), atom_set_element("a")], None);
    assert_eq!(set.elements().count(), 1);
}

#[test]
fn a_raised_set_aggregate_keeps_a_repeated_comparison_element() {
    let program = raised("a :- { X < 3 : p(X); X < 3 : p(X) } = 4.");
    assert_eq!(set_aggregate_of(&only_rule(&program)).elements().count(), 2);
}

#[test]
fn unpool_keeps_a_repeat_it_makes_in_a_set_aggregate() {
    let unpooled = unpool(&raised("a :- { #true : p(1; 1) } = 2."));
    assert_eq!(
        set_aggregate_of(&only_rule(&unpooled)).elements().count(),
        2
    );
}

#[test]
fn substitution_keeps_a_repeat_it_makes_in_a_set_aggregate() {
    let rule = only_rule(&raised("a(X) :- q(X), { #true : p(X); #true : p(1) } = 2."));
    let binding = mgu(&atom("p", vec![var("X")]), &atom("p", vec![num(1)]))
        .expect("a pattern")
        .expect("unifiable");
    assert_eq!(
        set_aggregate_of(&substitute(rule, &binding))
            .elements()
            .count(),
        2
    );
}

#[test]
fn canonicalizing_a_set_aggregate_merges_atom_elements_it_makes_equal() {
    // As for a choice: `p((1))` and `p(1)` meet only once the pool collapses (§5.1, §4.4).
    let element = |atom: Atom| {
        SetElement::Literal(literal(
            DefaultNegation::None,
            LiteralInner::Atom(WithProvenance::constructed(atom)),
        ))
    };
    let pooled = Atom {
        sign: Sign::Positive,
        name: name("p"),
        arguments: Arguments::Pooled(vec![vec![num(1)]]),
    };
    let set = SetAggregate::new(
        None,
        [element(pooled), element(atom("p", vec![num(1)]))],
        None,
    );
    assert_eq!(
        set.elements().count(),
        2,
        "the two differ until canonicalized"
    );
    let body = BodyElement::Aggregate {
        negation: DefaultNegation::None,
        aggregate: Aggregate::Set(set),
    };
    let program = Program::of([Rule::new(atom("a", vec![]), body)]);
    assert_eq!(set_aggregate_of(&only_rule(&program)).elements().count(), 1);
}

#[test]
fn a_kept_set_aggregate_repeat_renders_and_raises_back() {
    let program = raised("a :- { #true; #true } = 2.");
    let text = render(&program, Dialect::Clingo).expect("renders");
    assert_eq!(raised(&text), program);
}

// ---- theory atoms (§4.9) ----

/// A rule's theory-atom head.
fn theory_head_of(rule: &Rule) -> TheoryAtom {
    match rule.head().get() {
        Head::TheoryAtom(atom) => atom.clone(),
        other => panic!("a theory-atom head, not {other:?}"),
    }
}

fn theory_element(n: i32) -> TheoryElement {
    TheoryElement::new([TheoryTerm::Symbolic(Symbol::Number(n))], None)
}

#[test]
fn a_theory_element_is_counted_by_occurrence() {
    assert_eq!(theory_element(1).identity(), Identity::ByOccurrence);
}

#[test]
fn a_theory_atom_does_not_depend_on_its_element_order() {
    assert_eq!(
        TheoryAtom::new(
            name("sum"),
            [],
            [theory_element(2), theory_element(1)],
            None
        ),
        TheoryAtom::new(
            name("sum"),
            [],
            [theory_element(1), theory_element(2)],
            None
        )
    );
}

#[test]
fn a_raised_theory_atom_keeps_a_repeated_element() {
    let program = raised("&sum { x; x } = 4.");
    assert_eq!(theory_head_of(&only_rule(&program)).elements().count(), 2);
}

#[test]
fn a_kept_theory_repeat_renders_and_raises_back() {
    let program = raised("&sum { x; x } = 4.");
    let text = render(&program, Dialect::Clingo).expect("renders");
    assert_eq!(raised(&text), program);
}
