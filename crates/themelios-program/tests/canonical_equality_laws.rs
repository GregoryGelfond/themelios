//! Laws of canonical-syntactic equality (docs/design/program.md §5.1, §5.2): the
//! boolean-head fold makes a `#false.` head and a `:- .` constraint head one value while
//! a *negated* boolean head is kept; the equality is strictly finer than ordinary
//! equivalence; part keys distinguish by their spelled formals (no α-equivalence); the
//! two written forms of optimization are kept structurally distinct; and canonicalization
//! is idempotent, so re-admitting a canonical statement is stable.

use themelios_program::program::{
    Arguments, Atom, Body, BodyElement, Condition, DefaultNegation, Direction, Head, Literal,
    LiteralInner, Optimize, OptimizeElement, PartKey, Program, Rule, Statement, WeakConstraint,
    weight,
};
use themelios_program::provenance::WithProvenance;
use themelios_program::symbol::{Name, Sign, Symbol};
use themelios_program::term::Term;

fn name(text: &str) -> Name {
    Name::new(text).expect("a lowercase identifier")
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn atom(predicate: &str) -> Atom {
    Atom {
        sign: Sign::Positive,
        name: name(predicate),
        arguments: Arguments::Single(vec![]),
    }
}

fn positive(atom: Atom) -> Literal {
    Literal {
        negation: DefaultNegation::None,
        inner: LiteralInner::Atom(WithProvenance::constructed(atom)),
    }
}

fn boolean(inner: LiteralInner, negation: DefaultNegation) -> Head {
    Head::Literal(Literal { negation, inner })
}

fn program_of(rule: Rule) -> Program {
    Program::of([WithProvenance::constructed(Statement::Rule(rule))])
}

#[test]
fn a_false_head_and_a_constraint_head_are_one_value() {
    let false_head = Rule::new(
        boolean(LiteralInner::False, DefaultNegation::None),
        Body::empty(),
    );
    let constraint = Rule::new(Head::Falsum, Body::empty());
    // Distinct before the door — one is a literal head, the other a folded ⊥.
    assert_ne!(false_head, constraint);
    // The ingest folds the un-negated `#false` head to `Falsum`, so they are one value.
    assert_eq!(program_of(false_head), program_of(constraint));
}

#[test]
fn a_true_head_and_a_verum_head_are_one_value() {
    let true_head = Rule::new(
        boolean(LiteralInner::True, DefaultNegation::None),
        Body::empty(),
    );
    let verum = Rule::new(Head::Verum, Body::empty());
    assert_eq!(program_of(true_head), program_of(verum));
}

#[test]
fn a_negated_boolean_head_is_kept_as_its_literal() {
    // `not #false` has no ⊤/⊥ counterpart, so it is not folded.
    let negated_false = Rule::new(
        boolean(LiteralInner::False, DefaultNegation::Not),
        Body::empty(),
    );
    let constraint = Rule::new(Head::Falsum, Body::empty());
    assert_ne!(program_of(negated_false), program_of(constraint));
}

#[test]
fn equality_is_strictly_finer_than_ordinary_equivalence() {
    // `{ p :- q.  q :- p. }` and the empty program share the single answer set ∅ — they
    // are ordinarily, indeed strongly, equivalent — yet their canonical forms differ.
    let p_from_q = WithProvenance::constructed(Statement::Rule(Rule::new(
        positive(atom("p")),
        Body::new([BodyElement::Literal(positive(atom("q")))]),
    )));
    let q_from_p = WithProvenance::constructed(Statement::Rule(Rule::new(
        positive(atom("q")),
        Body::new([BodyElement::Literal(positive(atom("p")))]),
    )));
    let cyclic = Program::of([p_from_q, q_from_p]);
    let empty = Program::default();
    assert_ne!(cyclic, empty);
}

#[test]
fn part_keys_distinguish_by_their_spelled_formals() {
    // `step(t)` and `step(u)` coexist rather than merge — no α-equivalence of formals.
    let step_t = PartKey {
        name: name("step"),
        formals: vec![name("t")],
    };
    let step_u = PartKey {
        name: name("step"),
        formals: vec![name("u")],
    };
    assert_ne!(step_t, step_u);
}

#[test]
fn the_two_written_forms_of_optimization_are_kept_distinct() {
    // `:~ b. [1]` (a weak constraint) and `#minimize { 1 : b }` denote the same
    // optimization, which the authority folds; this tier keeps them structurally distinct.
    let body = || Body::new([BodyElement::Literal(positive(atom("b")))]);
    let weak = Statement::WeakConstraint(WeakConstraint::new(body(), weight(num(1)), []));
    let minimize = Statement::Optimize(Optimize::new(
        Direction::Minimize,
        [OptimizeElement::new(
            weight(num(1)),
            [],
            Condition::new([positive(atom("b"))]),
        )],
    ));
    assert_ne!(weak, minimize);
}

#[test]
fn canonicalization_is_idempotent() {
    // A raw ground argument collapses at the door; re-admitting the now-canonical
    // statement is stable.
    let raw = Rule::new(
        positive(Atom {
            sign: Sign::Positive,
            name: name("p"),
            arguments: Arguments::Single(vec![Term::Function {
                name: name("f"),
                arguments: vec![num(1)],
            }]),
        }),
        Body::empty(),
    );
    let once = program_of(raw);
    let canonical = once
        .statements()
        .next()
        .expect("one statement")
        .get()
        .clone();
    let twice = Program::of([WithProvenance::constructed(canonical)]);
    assert_eq!(once, twice);
}
