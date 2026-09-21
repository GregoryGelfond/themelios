//! The construction-equality acceptance (program §16, the *first-solve*
//! witness's construction half): for every macro, the value it builds is
//! **structurally equal — up to and including provenance (`Origin::Constructed`,
//! program §6) — to the value built through the spelled-out program-tier §7.1
//! constructors** it names (docs/design/macros.md §8, §11). The codegen (AST →
//! constructor calls) and a hand-written constructor chain are two spellings of
//! one construction, and this witness holds them exact.
//!
//! The witness **accretes per macro** (TDD): each macro's first test is its
//! equality witness, comparing a value built through the macro against the same
//! value built by hand — the term-shape witnesses (every arm the codegen emits)
//! among them, the load-bearing proof that each `codegen_term` arm spells the
//! *right* constructor and not merely a frozen emission (§11, §16).

use themelios_macros::{atom, constraint, external, fact, maximize, minimize, rule, show};
use themelios_program::construct;
use themelios_program::program::{
    Atom, Body, BodyElement, Condition, External, IntoHead, Literal, OptimizeElement, Rule, Show,
    TheoryAtom, TheoryElement, TheoryGuard, TheoryOperator, TheoryTerm, weight,
};
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::{Term, Variable};

/// `Name::new(text)`, discharged as the codegen's `.expect()` is (the fixture text is a
/// valid identifier by inspection).
fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// `VarName::new(text)`, discharged as [`name`] is.
fn var(text: &str) -> VarName {
    VarName::new(text).expect("a valid variable")
}

/// `Rule::fact(Atom::new(N("p"), [term]))` — the single-argument `p(term)` fact the
/// term-shape witnesses (§6) compare their `fact!(p(term))` against.
fn fact_p(term: Term) -> Rule {
    Rule::fact(Atom::new(name("p"), [term]))
}

// ---- the rule macros (§8) ----

#[test]
fn fact_macro_equals_the_constructor() {
    let by_macro: Rule = fact!(p(1, a));
    let by_hand = Rule::fact(Atom::new(
        name("p"),
        [Term::from(1i32), Term::constant(name("a"))],
    ));
    // Structural equality; provenance is `Constructed` on both (erased from identity).
    assert_eq!(by_macro, by_hand);
}

#[test]
fn rule_macro_equals_head_when_body() {
    let by_macro = rule!(q(X) :- p(X));
    let by_hand = Atom::new(name("q"), [Term::variable(var("X"))])
        .into_head()
        .when(Atom::new(name("p"), [Term::variable(var("X"))]));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn constraint_macro_equals_the_constructor() {
    let by_macro = constraint!(:- p(X));
    let by_hand = Rule::constraint(Atom::new(name("p"), [Term::variable(var("X"))]));
    assert_eq!(by_macro, by_hand);
}

// ---- the head-atom macro (§8): reaching an `Atom` by assembling a fact and extracting
// its single head atom, strong negation read positionally ----

#[test]
fn atom_macro_equals_atom_new() {
    let by_macro: Atom = atom!(p(1, a));
    let by_hand = Atom::new(name("p"), [Term::from(1i32), Term::constant(name("a"))]);
    // Structural equality; provenance is `Constructed` on both (erased from identity).
    assert_eq!(by_macro, by_hand);
}

#[test]
fn strong_negation_atom_macro_equals_the_negated_atom() {
    // `-p` in head position is the atom's positional strong negation (§8): `Neg` on an `Atom`
    // flips the sign to `Negative`, distinct from the arithmetic `-` of a term.
    let by_macro = atom!(-p(1));
    let by_hand = -Atom::new(name("p"), [Term::from(1i32)]);
    assert_eq!(by_macro, by_hand);
}

// ---- the directive macros (§8) ----

#[test]
fn minimize_macro_equals_the_constructor() {
    let by_macro = minimize!({ 3@1 });
    let by_hand = construct::minimize([OptimizeElement::new(
        weight(Term::from(3i32)).at_priority(Term::from(1i32)),
        [],
        Condition::empty(),
    )]);
    assert_eq!(by_macro, by_hand);
}

#[test]
fn maximize_macro_equals_the_constructor() {
    let by_macro = maximize!({ 5 });
    let by_hand = construct::maximize([OptimizeElement::new(
        weight(Term::from(5i32)),
        [],
        Condition::empty(),
    )]);
    assert_eq!(by_macro, by_hand);
}

#[test]
fn show_signature_macro_equals_the_variant() {
    let by_macro = show!(p / 1);
    let by_hand = Show::Signature(Signature::new(Sign::Positive, name("p"), 1));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn show_all_macro_equals_the_variant() {
    let by_macro = show!();
    let by_hand = Show::All;
    assert_eq!(by_macro, by_hand);
}

#[test]
fn show_term_macro_equals_the_variant() {
    let by_macro = show!(a);
    let by_hand = Show::Term(Term::constant(name("a")));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn show_term_body_macro_equals_the_constructor() {
    let by_macro = show!(a : p(X));
    let by_hand = Show::term_body(
        Term::constant(name("a")),
        Body::new([BodyElement::from(Atom::new(
            name("p"),
            [Term::variable(var("X"))],
        ))]),
    );
    assert_eq!(by_macro, by_hand);
}

#[test]
fn external_macro_equals_the_constructor() {
    let by_macro = external!(p(X));
    let by_hand = External::new(
        Atom::new(name("p"), [Term::variable(var("X"))]),
        Body::empty(),
        None,
    );
    assert_eq!(by_macro, by_hand);
}

#[test]
fn external_macro_over_a_body_equals_the_constructor() {
    let by_macro = external!(a : p(X));
    let by_hand = External::new(
        Atom::constant(name("a")),
        Body::new([BodyElement::from(Atom::new(
            name("p"),
            [Term::variable(var("X"))],
        ))]),
        None,
    );
    assert_eq!(by_macro, by_hand);
}

// ---- the term-shape witnesses (§6): every `codegen_term` arm spells the right
// constructor, not a frozen emission — the value proof beside the Task-6 goldens ----

#[test]
fn an_interval_term_equals_the_constructor() {
    assert_eq!(
        fact!(p(1..3)),
        fact_p(Term::from(1i32).to(Term::from(3i32)))
    );
}

#[test]
fn a_pool_term_equals_the_constructor() {
    assert_eq!(
        fact!(p((a; b))),
        fact_p(Term::pool([Term::constant(name("a")), Term::constant(name("b"))]).unwrap())
    );
}

#[test]
fn an_absolute_term_equals_the_constructor() {
    assert_eq!(fact!(p(|X|)), fact_p(Term::variable(var("X")).abs()));
}

#[test]
fn a_binary_arithmetic_term_equals_the_constructor() {
    // `*`-over-`+` in Rust mirrors the ASP parse `1 + (2 * 3)`, so the hand-spelled
    // operator tree matches the parsed one, and neither folds (an operator term never
    // does, program §3.5). One arithmetic witness stands for the whole `binary_operator!`
    // family — `+`/`*` are representative, generated identically (construct.rs).
    assert_eq!(
        fact!(p(1 + 2 * 3)),
        fact_p(Term::from(1i32) + Term::from(2i32) * Term::from(3i32))
    );
}

#[test]
fn a_tuple_term_equals_the_constructor() {
    assert_eq!(
        fact!(p((a, b))),
        fact_p(Term::tuple([
            Term::constant(name("a")),
            Term::constant(name("b"))
        ]))
    );
}

#[test]
fn an_arithmetic_negation_term_equals_the_constructor() {
    // Arithmetic negate in term position, distinct from the strong `-p` of an atom head.
    assert_eq!(fact!(p(-X)), fact_p(-Term::variable(var("X"))));
}

#[test]
fn a_bitwise_complement_term_equals_the_constructor() {
    assert_eq!(fact!(p(~X)), fact_p(Term::variable(var("X")).complement()));
}

// `#[rustfmt::skip]`: the power operator is two joint `*` tokens, which rustfmt would
// reformat to `* *3` — splitting the run the dialect munches into one `STAR_STAR`
// (grammar §4.6). The skip keeps the fixture's `**` intact.
#[rustfmt::skip]
#[test]
fn a_power_term_equals_the_constructor() {
    // `.pow()` is a distinct method, not the `binary_operator!` family, so it carries
    // its own witness (its map-row twin `Interval` uses `.to()`).
    assert_eq!(fact!(p(2 ** 3)), fact_p(Term::from(2i32).pow(Term::from(3i32))));
}

#[test]
fn an_external_call_term_equals_the_constructor() {
    // Direct public-enum construction, bypassing canonicalization (program §3.3).
    assert_eq!(
        fact!(p(@f(1))),
        fact_p(Term::External {
            name: name("f"),
            arguments: vec![Term::from(1i32)],
        })
    );
}

#[test]
fn the_anonymous_variable_term_equals_the_constructor() {
    assert_eq!(fact!(p(_)), fact_p(Term::anonymous()));
}

#[test]
fn a_string_term_equals_the_constructor() {
    assert_eq!(fact!(p("s")), fact_p(Term::from("s")));
}

#[test]
fn the_empty_tuple_term_equals_the_constructor() {
    // The empty tuple `()`, compiling the empty-array element-type inference that the
    // codegen's `[#(#terms),*]` rests on (a Task-6 concern, locked here).
    assert_eq!(fact!(p(())), fact_p(Term::tuple([])));
}

// ---- the theory-atom witnesses (§7): the non-`Symbolic` theory codegen a splice
// round-trip does not reach — `TheoryAtom::new`, `TheoryElement`, `TheoryGuard`, and the
// `Variable` / `Symbolic` / `Operation` theory-term arms ----

#[test]
fn theory_atom_macro_equals_the_constructor() {
    let by_macro = fact!(&sum { X } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Variable(Variable::Named(var("X")))],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn theory_atom_operation_macro_equals_the_constructor() {
    let by_macro = fact!(&sum { X + 1 } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            // `operators[i]` is the run before `operands[i]`, so a leading empty run
            // precedes `X` and `+` precedes `1` (program §4.9).
            [TheoryTerm::Operation {
                operators: vec![vec![], vec![TheoryOperator::new("+")]],
                operands: vec![
                    TheoryTerm::Variable(Variable::Named(var("X"))),
                    TheoryTerm::Symbolic(Symbol::Number(1)),
                ],
            }],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn a_string_theory_symbol_equals_the_constructor() {
    // The `Symbol::String` theory-symbol leaf: a string in theory-term position lifts to
    // `TheoryTerm::Symbolic(Symbol::string(…))`, told exact against the hand spelling. The
    // numeric, variable, and operation elements witnessed above leave this arm — and its
    // `Infimum` / `Supremum` / nullary-`Function` siblings — to a change-detector golden;
    // this pins one symbol-leaf value.
    let by_macro = fact!(&sum { "hi" } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Symbolic(Symbol::string("hi"))],
            None,
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}

#[test]
fn conditioned_theory_element_macro_equals_the_constructor() {
    // A theory element's condition is a `Condition` of ordinary literals, lowered
    // through the same door a rule body's condition takes.
    let by_macro = fact!(&sum { X: p(X) } <= 3);
    let by_hand = Rule::fact(TheoryAtom::new(
        name("sum"),
        [],
        [TheoryElement::new(
            [TheoryTerm::Variable(Variable::Named(var("X")))],
            Some(Condition::new([Literal::from(Atom::new(
                name("p"),
                [Term::variable(var("X"))],
            ))])),
        )],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Symbolic(Symbol::Number(3)),
        }),
    ));
    assert_eq!(by_macro, by_hand);
}
