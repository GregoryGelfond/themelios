//! Laws of the epistemic answer and the denoting query at the public surface
//! (docs/design/query.md §2.1, §2.2): the answer is a closed trichotomy; a
//! query is a ground literal, or a conjunction or disjunction of queries, whose
//! construction refuses a non-denoting or a non-ground atom with the reason —
//! so a query that exists denotes; and every value travels as owned plain data.

use std::error::Error;
use std::fmt::Debug;

use themelios_program::program::Atom;
use themelios_program::symbol::{Name, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, Variable};
use themelios_program::unify::NotAPattern;
use themelios_query::{Answer, NotAQuery, Query};

/// A validated identifier.
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// The number `n` as a term.
fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

/// The named variable `text` as a term.
fn var(text: &str) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable"),
    ))
}

/// The infix operation `left <operator> right`.
fn binary(operator: BinaryOp, left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// The interval `1..3` — a set-former, not a value.
fn interval() -> Term {
    Term::Interval {
        lower: Box::new(num(1)),
        upper: Box::new(num(3)),
    }
}

/// The atom `p(t)` — one argument, positive.
fn p(argument: Term) -> Atom {
    Atom::new(name("p"), [argument])
}

/// The atom `p(1..3)` — its argument is an interval, which names a *set* of
/// atoms, not one.
fn interval_atom() -> Atom {
    p(interval())
}

/// The refusal `Query::of` issues for `atom`, which it must issue.
fn refusal(atom: Atom) -> NotAQuery {
    Query::of(atom).expect_err("refused")
}

/// Owned plain data: what a value that travels across threads and service
/// boundaries must be.
fn plain<T: Send + Sync + Clone + Eq + Debug + 'static>() {}

// ---- The refusal: a query that exists denotes (§2.1, §3.1) ----

#[test]
fn a_query_refuses_a_non_denoting_term() {
    // An interval names a set — refused, not guessed (query.md §3.1).
    assert!(Query::of(interval_atom()).is_err());
}

#[test]
fn a_refused_non_denoting_argument_is_handed_back() {
    // The program tier's refusal, carrying the argument as authored.
    assert_eq!(
        refusal(interval_atom()),
        NotAQuery::NotAPattern(NotAPattern::NonDenoting { term: interval() })
    );
}

#[test]
fn a_pool_argument_is_refused_as_non_denoting() {
    let pool = Term::pool([num(1), num(2)]).expect("two alternatives");
    assert_eq!(
        refusal(p(pool.clone())),
        NotAQuery::NotAPattern(NotAPattern::NonDenoting { term: pool })
    );
}

#[test]
fn an_argument_list_pool_is_refused_as_pooled() {
    // `p(1; 2)` names a set of atoms: the atom-level twin of a pooled argument.
    let pooled = Atom::pooled(name("p"), [vec![num(1)], vec![num(2)]]).expect("two alternatives");
    assert_eq!(refusal(pooled), NotAQuery::NotAPattern(NotAPattern::Pooled));
}

#[test]
fn an_unevaluated_external_call_is_refused_as_non_denoting() {
    let call = Term::External {
        name: name("f"),
        arguments: vec![num(1)],
    };
    assert_eq!(
        refusal(p(call.clone())),
        NotAQuery::NotAPattern(NotAPattern::NonDenoting { term: call })
    );
}

#[test]
fn an_undefined_operation_is_refused_as_non_denoting() {
    // Division by zero: ground, yet it denotes nothing.
    let undefined = binary(BinaryOp::Div, num(1), num(0));
    assert_eq!(
        refusal(p(undefined.clone())),
        NotAQuery::NotAPattern(NotAPattern::NonDenoting { term: undefined })
    );
}

#[test]
fn an_out_of_range_operation_is_refused_as_non_denoting() {
    // One past the number ceiling: the ground door refuses rather than wraps.
    let overflowing = binary(BinaryOp::Add, num(i32::MAX), num(1));
    assert_eq!(
        refusal(p(overflowing.clone())),
        NotAQuery::NotAPattern(NotAPattern::NonDenoting { term: overflowing })
    );
}

#[test]
fn a_variable_bearing_atom_is_refused_as_not_ground() {
    // `p(X)` is a pattern — the bindings' question — not a ground query.
    assert_eq!(
        refusal(p(var("X"))),
        NotAQuery::NotGround { term: var("X") }
    );
}

#[test]
fn an_anonymous_variable_is_refused_as_not_ground() {
    assert_eq!(
        refusal(p(Term::anonymous())),
        NotAQuery::NotGround {
            term: Term::anonymous()
        }
    );
}

#[test]
fn arithmetic_over_a_variable_is_refused_as_not_ground() {
    // `X + 1` bears a variable: a query is ground before anything else.
    let open = binary(BinaryOp::Add, var("X"), num(1));
    assert_eq!(
        refusal(p(open.clone())),
        NotAQuery::NotGround { term: open }
    );
}

// ---- Construction: the denoting shapes (§2.1) ----

#[test]
fn a_constant_atom_is_a_query() {
    assert!(Query::of(Atom::constant(name("p"))).is_ok());
}

#[test]
fn a_ground_applied_atom_is_a_query() {
    let nested = Term::function(name("f"), [num(1), Term::constant(name("a"))]);
    assert!(Query::of(Atom::new(name("p"), [num(2), nested])).is_ok());
}

#[test]
fn ground_arithmetic_evaluates_at_the_door() {
    // `p(1 + 2)` and `p(3)` denote the same literal, so they are one query.
    let sum = binary(BinaryOp::Add, num(1), num(2));
    assert_eq!(
        Query::of(p(sum)).expect("denotes"),
        Query::of(p(num(3))).expect("denotes")
    );
}

#[test]
fn a_strongly_negated_atom_is_its_own_query() {
    let positive = Query::of(Atom::constant(name("p"))).expect("denotes");
    let negative = Query::of(-Atom::constant(name("p"))).expect("denotes");
    assert_ne!(positive, negative);
}

#[test]
fn a_conjunction_and_a_disjunction_of_the_same_parts_differ() {
    let a = Query::of(Atom::constant(name("a"))).expect("denotes");
    let b = Query::of(Atom::constant(name("b"))).expect("denotes");
    assert_ne!(Query::all([a.clone(), b.clone()]), Query::any([a, b]));
}

#[test]
fn queries_compose_to_any_nesting() {
    let a = Query::of(Atom::constant(name("a"))).expect("denotes");
    let b = Query::of(Atom::constant(name("b"))).expect("denotes");
    let c = Query::of(Atom::constant(name("c"))).expect("denotes");
    let composed = Query::any([Query::all([a, b]), c]);
    assert_eq!(composed.clone(), composed);
}

#[test]
fn the_empty_conjunction_constructs() {
    // Total: no part to refuse; the query true everywhere.
    let empty = Query::all([]);
    assert_eq!(empty.clone(), empty);
}

#[test]
fn the_empty_disjunction_constructs() {
    // Total: no part to refuse; the query false everywhere.
    let empty = Query::any([]);
    assert_eq!(empty.clone(), empty);
}

// ---- The answer: a closed trichotomy (§2.1, §2.2) ----

#[test]
fn answer_is_a_closed_trichotomy() {
    // A `match` with exactly the three arms compiles: the type is closed.
    fn closed(answer: Answer) -> u8 {
        match answer {
            Answer::Yes => 0,
            Answer::No => 1,
            Answer::Unknown => 2,
        }
    }
    assert_eq!(closed(Answer::Yes), 0);
    assert_eq!(closed(Answer::No), 1);
    assert_eq!(closed(Answer::Unknown), 2);
}

#[test]
fn answer_displays_one_word_each() {
    assert_eq!(Answer::Yes.to_string(), "yes");
    assert_eq!(Answer::No.to_string(), "no");
    assert_eq!(Answer::Unknown.to_string(), "unknown");
}

#[test]
fn answer_is_owned_plain_data() {
    fn copyable<T: Send + Sync + Copy + Eq + Debug + 'static>() {}
    copyable::<Answer>();
}

// ---- The refusal as a value (§3.1, §4) ----

#[test]
fn not_a_query_displays_its_reason() {
    let not_ground = refusal(p(var("X"))).to_string();
    assert!(not_ground.starts_with("not a query: the term "));
    assert!(not_ground.ends_with(" is not ground"));
    assert_eq!(
        refusal(interval_atom()).to_string(),
        "not a query: the atom is not a pattern"
    );
}

#[test]
fn not_a_query_chains_the_program_tier_refusal_as_its_source() {
    let pooled = Atom::pooled(name("p"), [vec![num(1)], vec![num(2)]]).expect("two alternatives");
    let source = refusal(pooled)
        .source()
        .and_then(|source| source.downcast_ref::<NotAPattern>())
        .cloned();
    assert_eq!(source, Some(NotAPattern::Pooled));
}

#[test]
fn not_ground_has_no_source_beneath_it() {
    assert!(refusal(p(var("X"))).source().is_none());
}

#[test]
fn a_query_is_owned_plain_data() {
    plain::<Query>();
}

#[test]
fn the_refusal_is_owned_plain_data() {
    plain::<NotAQuery>();
}
