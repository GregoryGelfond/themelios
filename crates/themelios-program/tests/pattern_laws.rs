//! The pattern language over the term algebra (docs/design/program.md §11.2, §11.3): a
//! pattern is a signed atom whose arguments are the **constructor fragment**; ground
//! arithmetic composes with the evaluator (§3.5), while a variable-bearing arithmetic term,
//! an interval, a pool, or an `@`-call does not denote and **refuses**. The `Ok(None)`
//! (does not match) versus `Err` (cannot answer) distinction is load-bearing (spec §5.2), and
//! `signature_range` finds by an ordered range exactly what a full scan finds (§11.3, §16).

use std::collections::BTreeSet;

use themelios_program::program::Atom;
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{BinaryOp, Term, UnaryOp, Variable};
use themelios_program::unify::{NotAPattern, mgu, signature_range};

// ---- builders ----

fn var(text: &str) -> Variable {
    Variable::Named(VarName::new(text).expect("a valid variable name"))
}
fn tvar(text: &str) -> Term {
    Term::Variable(var(text))
}
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}
fn atom(functor: &str, arguments: impl IntoIterator<Item = Term>) -> Atom {
    Atom::new(name(functor), arguments)
}
fn func(functor: &str, arguments: impl IntoIterator<Item = Term>) -> Term {
    Term::Function {
        name: name(functor),
        arguments: arguments.into_iter().collect(),
    }
}
fn num(value: i32) -> Term {
    Term::from(value)
}
fn konst(text: &str) -> Term {
    Term::Symbolic(Symbol::Function {
        name: name(text),
        arguments: Vec::new(),
        sign: Sign::Positive,
    })
}
fn add(left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}

// ---- what a pattern is (§11.2) ----

#[test]
fn a_constructor_fragment_atom_is_a_pattern() {
    // Variables, ground symbols, function terms, and tuples — the whole constructor fragment;
    // mgu answers (here a successful unify), never Err.
    let a = atom(
        "p",
        [
            tvar("X"),
            konst("a"),
            func("f", [tvar("Y")]),
            Term::Tuple(vec![num(1), num(2)]),
        ],
    );
    let b = atom(
        "p",
        [
            konst("c"),
            tvar("Z"),
            func("f", [num(9)]),
            Term::Tuple(vec![tvar("W"), num(2)]),
        ],
    );
    assert!(
        matches!(mgu(&a, &b), Ok(Some(_))),
        "a constructor-fragment atom is a pattern that here unifies",
    );
}

#[test]
fn a_variable_bearing_arithmetic_argument_is_not_a_pattern() {
    // p(X + 1) would need inverting; it is not a pattern, and the refusal carries the very
    // term that does not denote.
    let plus = add(tvar("X"), num(1));
    match mgu(&Atom::new(name("p"), [plus.clone()]), &atom("p", [num(3)])) {
        Err(NotAPattern::NonDenoting { term }) => assert_eq!(term, plus),
        other => panic!("expected NonDenoting carrying X + 1, got {other:?}"),
    }
}

#[test]
fn variable_bearing_arithmetic_refuses_in_any_former() {
    // Unary negation and absolute value over a variable each fail to denote; and a refusal
    // reaches *under* a function (p(f(X + 1))).
    let unary = Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(tvar("X")),
    };
    let absolute = Term::Absolute(Box::new(tvar("X")));
    for former in [unary, absolute] {
        assert!(
            matches!(
                mgu(
                    &Atom::new(name("p"), [former.clone()]),
                    &atom("p", [num(0)])
                ),
                Err(NotAPattern::NonDenoting { .. }),
            ),
            "{former:?} does not denote a pattern",
        );
    }
    let nested = func("f", [add(tvar("X"), num(1))]);
    assert!(matches!(
        mgu(
            &Atom::new(name("p"), [nested]),
            &atom("p", [func("f", [num(2)])]),
        ),
        Err(NotAPattern::NonDenoting { .. }),
    ));
}

#[test]
fn a_set_former_or_external_call_is_not_a_pattern() {
    // An interval and a pool each name a *set* (even when ground), and an `@`-call is
    // unevaluated here — none is a pattern (§11.2). The other atom is a plain variable, so
    // only the non-pattern argument decides the refusal.
    let interval = Term::Interval {
        lower: Box::new(num(1)),
        upper: Box::new(num(3)),
    };
    let pool = Term::Pool(vec![num(1), num(2)]);
    let external = Term::External {
        name: name("ext"),
        arguments: vec![konst("a")],
    };
    for former in [interval, pool, external] {
        assert!(
            matches!(
                mgu(
                    &Atom::new(name("p"), [former.clone()]),
                    &atom("p", [tvar("Z")])
                ),
                Err(NotAPattern::NonDenoting { .. }),
            ),
            "{former:?} is not a pattern",
        );
    }
}

#[test]
fn ground_arithmetic_in_a_pattern_evaluates_and_matches() {
    // p(1 + 2) matches p(3): the ground arithmetic composes with the evaluator (§3.5, §11.2).
    assert!(matches!(
        mgu(
            &Atom::new(name("p"), [add(num(1), num(2))]),
            &atom("p", [num(3)])
        ),
        Ok(Some(_)),
    ));
    // And nested under a function: p(f(1 + 2)) matches p(f(3)).
    assert!(matches!(
        mgu(
            &Atom::new(name("p"), [func("f", [add(num(1), num(2))])]),
            &atom("p", [func("f", [num(3)])]),
        ),
        Ok(Some(_)),
    ));
    // A ground arithmetic that does not denote (division by zero) refuses, honestly.
    let divide_by_zero = Term::BinaryOperation {
        operator: BinaryOp::Div,
        left: Box::new(num(1)),
        right: Box::new(num(0)),
    };
    assert!(matches!(
        mgu(
            &Atom::new(name("p"), [divide_by_zero]),
            &atom("p", [num(0)])
        ),
        Err(NotAPattern::NonDenoting { .. }),
    ));
}

// ---- cannot-decide is a value, never folded into no (§11.2, spec §5.2) ----

#[test]
fn cannot_decide_is_never_folded_into_no() {
    // A non-matching pattern is Ok(None) — "no such match"; a non-pattern is Err — "cannot
    // answer". The two are distinct outcomes.
    assert_eq!(mgu(&atom("p", [num(1)]), &atom("p", [num(2)])), Ok(None));
    assert!(matches!(
        mgu(
            &Atom::new(name("p"), [add(tvar("X"), num(1))]),
            &atom("p", [num(2)])
        ),
        Err(NotAPattern::NonDenoting { .. }),
    ));
}

#[test]
fn a_non_pattern_refuses_even_when_the_predicates_differ() {
    // p(X + 1) is not a pattern, so mgu cannot answer — even against q(Y), which it could
    // never unify with in any case. Non-pattern-ness is a property of the term, not the pair,
    // so it is checked before the sign, name, and arity are compared.
    assert!(matches!(
        mgu(
            &Atom::new(name("p"), [add(tvar("X"), num(1))]),
            &atom("q", [tvar("Y")]),
        ),
        Err(NotAPattern::NonDenoting { .. }),
    ));
}

#[test]
fn the_pattern_check_reaches_both_atoms() {
    // The right atom's non-pattern argument refuses just as the left's does.
    assert!(matches!(
        mgu(
            &atom("p", [num(2)]),
            &Atom::new(name("p"), [add(tvar("X"), num(1))]),
        ),
        Err(NotAPattern::NonDenoting { .. }),
    ));
}

#[test]
fn the_pattern_refusal_carries_the_std_error_posture() {
    // NotAPattern is mgu's `Result` error, so it Displays a non-empty message and composes as
    // std::error::Error (§14) — a host can `?` it.
    fn message<E: std::error::Error>(error: &E) -> String {
        error.to_string()
    }
    let refusal = mgu(
        &Atom::new(name("p"), [add(tvar("X"), num(1))]),
        &atom("p", [num(3)]),
    )
    .expect_err("a variable-bearing arithmetic argument is not a pattern");
    assert!(
        !message(&refusal).is_empty(),
        "the refusal Displays its cause"
    );
}

// ---- matching against an answer set: signature_range (§11.3, §16) ----

fn sfun(functor: &str, arguments: Vec<Symbol>, sign: Sign) -> Symbol {
    Symbol::Function {
        name: name(functor),
        arguments,
        sign,
    }
}
fn snum(value: i32) -> Symbol {
    Symbol::Number(value)
}
fn sstr(text: &str) -> Symbol {
    Symbol::String(text.to_owned())
}

/// A rich answer set spanning the ordered term bands: numbers and strings; constants and
/// arity-bearing functions; a strongly-negated atom; several names and arities; a tuple; and
/// the two order sentinels — so a signature block must be carved out of genuine neighbours.
fn answer_set() -> BTreeSet<Symbol> {
    [
        sfun("p", vec![snum(1), snum(2)], Sign::Positive),
        sfun(
            "p",
            vec![
                sfun("a", vec![], Sign::Positive),
                sfun("b", vec![], Sign::Positive),
            ],
            Sign::Positive,
        ),
        sfun("p", vec![snum(5), sstr("x")], Sign::Positive),
        sfun("p", vec![snum(1), snum(2), snum(3)], Sign::Positive),
        sfun("p", vec![snum(9)], Sign::Positive),
        sfun("p", vec![], Sign::Positive),
        sfun("p", vec![snum(1), snum(2)], Sign::Negative),
        sfun("q", vec![snum(1), snum(2), snum(3)], Sign::Positive),
        sfun("q", vec![snum(1), snum(2)], Sign::Positive),
        snum(3),
        sstr("p"),
        Symbol::Tuple(vec![snum(1), snum(2)]),
        Symbol::Infimum,
        Symbol::Supremum,
    ]
    .into_iter()
    .collect()
}

fn signature_of(pattern: &Atom) -> Signature {
    Signature {
        sign: pattern.sign,
        name: pattern.name.clone(),
        arity: u32::try_from(pattern.arguments.len()).expect("a small test arity"),
    }
}

#[test]
fn the_signature_range_finds_exactly_the_signature_scan() {
    // For each pattern, the O(log n + k) range over the ordered answer set picks out exactly
    // the symbols the O(n) scan filtering by (sign, name, arity) finds (§11.3, §16).
    let answer = answer_set();
    let patterns = [
        atom("p", [tvar("X"), tvar("Y")]),
        atom("p", [tvar("X")]),
        Atom::constant(name("p")),
        -atom("p", [tvar("X"), tvar("Y")]),
        atom("q", [tvar("X"), tvar("Y"), tvar("Z")]),
        atom("absent", [tvar("X")]),
    ];
    for pattern in patterns {
        let signature = signature_of(&pattern);
        let via_range: BTreeSet<Symbol> =
            answer.range(signature_range(&pattern)).cloned().collect();
        let via_scan: BTreeSet<Symbol> = answer
            .iter()
            .filter(|symbol| symbol.signature().as_ref() == Some(&signature))
            .cloned()
            .collect();
        assert_eq!(
            via_range, via_scan,
            "the range and the scan disagree for {signature:?}",
        );
    }
}

#[test]
fn the_signature_range_is_total_even_for_a_non_pattern_atom() {
    // signature_range reads only the signature, so a non-pattern argument (an interval) does
    // not concern it — it still returns the predicate's block, never refusing (§11.3).
    let interval = Term::Interval {
        lower: Box::new(num(1)),
        upper: Box::new(num(3)),
    };
    let pattern = Atom::new(name("p"), [interval]);
    let range = signature_range(&pattern);
    assert_eq!(
        *range.start(),
        sfun("p", vec![Symbol::Infimum], Sign::Positive),
    );
    assert_eq!(
        *range.end(),
        sfun("p", vec![Symbol::Supremum], Sign::Positive),
    );
}
