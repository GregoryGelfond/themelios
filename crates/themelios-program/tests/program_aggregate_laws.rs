//! Laws of aggregates and optimization (docs/design/program.md §4.7): the elements
//! are a set, `HasGuards` reads both guards for either aggregate form, the head and
//! body elements are distinct types, and an element canonicalizes its terms at the
//! door.

use themelios_program::program::{
    AggregateFunction, Arguments, Atom, BodyAggregateElement, Condition, DefaultNegation,
    Direction, FunctionAggregate, Guard, HasGuards, HeadAggregate, HeadAggregateElement, Literal,
    LiteralInner, Optimize, OptimizeElement, Relation, SetAggregate, SetElement, weight,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_program::term::Term;

fn number(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn literal(predicate: &str) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(Atom {
            sign: Sign::Positive,
            name: Name::new(predicate).expect("identifier"),
            arguments: Arguments::Single(vec![]),
        })),
    }
}

fn body_element(n: i32) -> BodyAggregateElement {
    BodyAggregateElement::new([number(n)], Condition::empty())
}

#[test]
fn aggregate_and_optimize_elements_are_sets() {
    // A duplicate element vanishes.
    let deduped = FunctionAggregate::new(
        None,
        AggregateFunction::Count,
        [body_element(1), body_element(1)],
        None,
    );
    assert_eq!(deduped.elements().count(), 1);
    // A reordering is the same aggregate.
    let one = FunctionAggregate::new(
        None,
        AggregateFunction::Sum,
        [body_element(1), body_element(2)],
        None,
    );
    let other = FunctionAggregate::new(
        None,
        AggregateFunction::Sum,
        [body_element(2), body_element(1)],
        None,
    );
    assert_eq!(one, other);
    // Optimize elements are a set too.
    let element = || OptimizeElement::new(weight(number(1)), [number(1)], Condition::empty());
    let optimize = Optimize::new(Direction::Minimize, [element(), element()]);
    assert_eq!(optimize.elements().count(), 1);
}

#[test]
fn has_guards_reads_both_guards_and_an_absent_relation_is_none() {
    let left = Guard {
        relation: Some(Relation::Le),
        term: number(1),
    };
    let right = Guard {
        relation: None,
        term: number(5),
    }; // the grammar's default, as absence
    let function = FunctionAggregate::new(
        Some(left.clone()),
        AggregateFunction::Count,
        [body_element(1)],
        Some(right.clone()),
    );
    assert_eq!(function.left_guard().map(WithProvenance::get), Some(&left));
    assert_eq!(
        function.right_guard().map(WithProvenance::get),
        Some(&right)
    );
    assert_eq!(
        function.right_guard().expect("a guard").get().relation,
        None
    );
    // A set aggregate reads its guards through the same trait.
    let set = SetAggregate::new(
        Some(left.clone()),
        [SetElement::Literal(literal("p"))],
        None,
    );
    assert_eq!(set.left_guard().map(WithProvenance::get), Some(&left));
    assert_eq!(set.right_guard(), None);
    // A head aggregate too.
    let head = HeadAggregate::new(
        None,
        AggregateFunction::Count,
        [HeadAggregateElement::new(
            [number(1)],
            literal("p"),
            Condition::empty(),
        )],
        Some(right.clone()),
    );
    assert_eq!(head.right_guard().map(WithProvenance::get), Some(&right));
}

#[test]
fn head_and_body_elements_are_distinct_types() {
    // A body element tests: terms and a condition, no derived literal.
    let body = BodyAggregateElement::new([number(1), number(2)], Condition::empty());
    assert_eq!(body.terms().count(), 2);
    // A head element derives: it adds the literal, which lives only on this type — so a
    // FunctionAggregate holding a head element does not compile (§4.5's unrepresentability
    // in the type; the position lives in the aggregate's type, not a runtime tag).
    let head = HeadAggregateElement::new([number(1)], literal("p"), Condition::empty());
    assert_eq!(head.literal(), &literal("p"));
    assert_eq!(head.terms().count(), 1);
}

#[test]
fn an_element_canonicalizes_its_terms_at_the_door() {
    // A Function-shaped ground term entering an element's door collapses to a Symbolic.
    let ground = Term::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![number(1)],
    };
    let element = BodyAggregateElement::new([ground], Condition::empty());
    let collapsed = Term::Symbolic(Symbol::Function {
        name: Name::new("f").expect("identifier"),
        arguments: vec![Symbol::Number(1)],
        sign: Sign::Positive,
    });
    assert_eq!(element.terms().next(), Some(&collapsed));
}

#[test]
fn a_weight_carries_an_optional_priority_and_both_forms_share_it() {
    // weight(w) is at the default level; .at_priority(p) raises it, and the priority is
    // part of the weight's identity, so the two differ (`w` vs `w@p`).
    let plain = weight(number(1));
    assert_eq!(plain.term(), &number(1));
    assert!(plain.priority().is_none());

    let leveled = weight(number(1)).at_priority(number(2));
    assert_eq!(leveled.term(), &number(1));
    assert_eq!(leveled.priority(), Some(&number(2)));
    assert_ne!(plain, leveled);

    // An optimize element carries the one weight@priority value.
    let element = OptimizeElement::new(leveled, [number(1)], Condition::empty());
    assert_eq!(element.weight().priority(), Some(&number(2)));
}
