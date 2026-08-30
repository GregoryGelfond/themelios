//! Laws of the one provenance-merging ingest door (docs/design/program.md §6.3): two
//! content-equal statements admitted from distinct origins collapse to one statement
//! whose provenance is the union of both — nothing lost, nothing fabricated — while
//! distinct statements do not merge. The ingest is the only path that mutates a part's
//! set, so the preservation law is structural. (A statement parsed from two source spans
//! is the motivating case; the merge is the same for any two origins.)

use themelios_program::program::{
    Arguments, Atom, Body, DefaultNegation, Literal, LiteralInner, Program, Rule, Statement,
};
use themelios_program::provenance::{Origin, Provenance, TransformTag, WithProvenance};
use themelios_program::symbol::{Name, Sign};

fn name(text: &str) -> Name {
    Name::new(text).expect("a lowercase identifier")
}

fn fact(predicate: &str) -> Statement {
    let atom = Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(vec![]),
    };
    let literal = Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    };
    Statement::Rule(Rule::new(literal, Body::empty()))
}

fn origin(tag: &str) -> Provenance {
    Provenance::from(Origin::Transformed(TransformTag::new(tag)))
}

#[test]
fn content_equal_statements_from_distinct_origins_collapse_with_unioned_provenance() {
    let here = WithProvenance::new(fact("p"), origin("here"));
    let there = WithProvenance::new(fact("p"), origin("there"));
    let program = Program::of([here, there]);

    let admitted: Vec<&WithProvenance<Statement>> = program.base().statements().collect();
    assert_eq!(
        admitted.len(),
        1,
        "the two content-equal statements collapse to one"
    );

    let origins: Vec<&Origin> = admitted[0].provenance().origins().collect();
    assert_eq!(
        origins.len(),
        2,
        "the merge is the union of both provenances"
    );
    assert!(origins.contains(&&Origin::Transformed(TransformTag::new("here"))));
    assert!(origins.contains(&&Origin::Transformed(TransformTag::new("there"))));
}

#[test]
fn a_single_statement_keeps_exactly_its_provenance() {
    let one = WithProvenance::new(fact("p"), origin("sole"));
    let program = Program::of([one]);
    let admitted = program.base().statements().next().expect("one statement");
    let origins: Vec<&Origin> = admitted.provenance().origins().collect();
    assert_eq!(
        origins,
        vec![&Origin::Transformed(TransformTag::new("sole"))]
    );
}

#[test]
fn distinct_statements_do_not_merge() {
    let p = WithProvenance::new(fact("p"), origin("a"));
    let q = WithProvenance::new(fact("q"), origin("b"));
    let program = Program::of([p, q]);
    assert_eq!(program.base().statements().count(), 2);
}
