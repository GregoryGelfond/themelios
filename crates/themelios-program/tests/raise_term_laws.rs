//! The term raise (docs/design/program.md §8): the flat per-precedence chain
//! re-associated to the term tree (exponentiation right-associative), the ground
//! constructor collapse, the dialect-correct string read, the pooled-argument
//! distribution, the parse-to-symbol middle step (§3.5), and totality under recovery.

use themelios_base::source::{Source, SourceId};
use themelios_program::raise::{LowerError, LowerErrorKind, raise_term};
use themelios_program::symbol::{Symbol, VarName};
use themelios_program::term::{BinaryOp, EvalError, Term, UnaryOp, Variable};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::parse::{NestingLimit, parse_term, parse_term_value};

// ---- harness ----

fn raise(text: &str, dialect: Dialect) -> (Option<Term>, Vec<LowerError>) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let lexer = Lexer::new(&source, dialect);
    raise_term(&parse_term(&lexer, NestingLimit::DEFAULT))
}

fn raise_value(text: &str) -> (Option<Term>, Vec<LowerError>) {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("admits");
    let lexer = Lexer::new(&source, Dialect::Clingo);
    raise_term(&parse_term_value(&lexer, NestingLimit::DEFAULT))
}

/// The clean raise: a term, no diagnostics.
fn raised(text: &str) -> Term {
    let (term, errors) = raise(text, Dialect::Clingo);
    assert!(
        errors.is_empty(),
        "unexpected lowering errors for `{text}`: {errors:?}"
    );
    term.unwrap_or_else(|| panic!("`{text}` raised to no term"))
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn binary(operator: BinaryOp, left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
}

// ---- Re-association: the flat chain becomes the tree (§8) ----

#[test]
fn a_left_associative_chain_folds_left() {
    // `1 - 2 - 3` is `((1 - 2) - 3)`; the numbers are ground leaves, the operator
    // term does not fold (§3.5).
    assert_eq!(
        raised("1 - 2 - 3"),
        binary(BinaryOp::Sub, binary(BinaryOp::Sub, num(1), num(2)), num(3)),
    );
}

#[test]
fn exponentiation_folds_right() {
    // `2 ** 3 ** 2` is `2 ** (3 ** 2)`.
    assert_eq!(
        raised("2 ** 3 ** 2"),
        binary(BinaryOp::Pow, num(2), binary(BinaryOp::Pow, num(3), num(2))),
    );
}

#[test]
fn mixed_precedence_nests_as_the_grammar_dictates() {
    // `*` binds tighter than `+`, so `1 + 2 * 3` is `1 + (2 * 3)`.
    assert_eq!(
        raised("1 + 2 * 3"),
        binary(BinaryOp::Add, num(1), binary(BinaryOp::Mul, num(2), num(3))),
    );
}

// ---- Ground collapse (§5.1) ----

#[test]
fn a_ground_function_collapses_to_a_symbol() {
    match &raised("f(1, 2)") {
        Term::Symbolic(Symbol::Function {
            name, arguments, ..
        }) => {
            assert_eq!(name.as_str(), "f");
            assert_eq!(arguments, &vec![Symbol::Number(1), Symbol::Number(2)]);
        }
        other => panic!("`f(1, 2)` should collapse to a Symbolic function, got {other:?}"),
    }
}

#[test]
fn a_function_over_an_operator_argument_does_not_collapse() {
    match &raised("f(1 + 2)") {
        Term::Function { arguments, .. } => {
            assert_eq!(arguments, &vec![binary(BinaryOp::Add, num(1), num(2))]);
        }
        other => panic!("`f(1 + 2)` stays a Function over an operator argument, got {other:?}"),
    }
}

#[test]
fn a_ground_tuple_collapses_to_a_symbol() {
    assert_eq!(
        raised("(1, 2)"),
        Term::Symbolic(Symbol::Tuple(vec![Symbol::Number(1), Symbol::Number(2)])),
    );
}

// ---- Variables and the term formers (§3.3, grammar §5.1) ----

#[test]
fn variables_named_and_anonymous() {
    assert_eq!(
        raised("X"),
        Term::Variable(Variable::Named(VarName::new("X").unwrap())),
    );
    assert_eq!(raised("_"), Term::Variable(Variable::Anonymous));
}

#[test]
fn an_interval_is_its_own_former() {
    match &raised("1 .. 3") {
        Term::Interval { lower, upper } => {
            assert_eq!(**lower, num(1));
            assert_eq!(**upper, num(3));
        }
        other => panic!("`1 .. 3` is an Interval, got {other:?}"),
    }
}

#[test]
fn a_parenthesized_pool_is_a_pool() {
    match &raised("(a; b)") {
        Term::Pool(alternatives) => assert_eq!(alternatives.len(), 2),
        other => panic!("`(a; b)` is a Pool, got {other:?}"),
    }
}

#[test]
fn an_at_call_is_an_external() {
    match &raised("@f(X)") {
        Term::External { name, arguments } => {
            assert_eq!(name.as_str(), "f");
            assert_eq!(
                arguments,
                &vec![Term::Variable(Variable::Named(VarName::new("X").unwrap()))],
            );
        }
        other => panic!("`@f(X)` is an External, got {other:?}"),
    }
}

#[test]
fn a_term_position_minus_is_arithmetic_negation() {
    // `-Y` at term level is `UnaryOp::Negate`, never a strong sign (§4.6).
    match &raised("-Y") {
        Term::UnaryOperation { operator, argument } => {
            assert_eq!(*operator, UnaryOp::Negate);
            assert_eq!(
                **argument,
                Term::Variable(Variable::Named(VarName::new("Y").unwrap()))
            );
        }
        other => panic!("`-Y` is a UnaryOperation Negate, got {other:?}"),
    }
}

// ---- Dialect correctness: the string reads through the parse's dialect (§8) ----

#[test]
fn a_string_reads_its_value_under_the_parses_dialect() {
    let clingo = raise("\"a\\nb\"", Dialect::Clingo).0.expect("a term");
    let aspcore = raise("\"a\\nb\"", Dialect::AspCore2).0.expect("a term");
    assert_ne!(
        clingo, aspcore,
        "the two dialects read the escape differently"
    );
    assert_eq!(clingo, Term::Symbolic(Symbol::String("a\nb".to_owned())));
    assert_eq!(aspcore, Term::Symbolic(Symbol::String("a\\nb".to_owned())));
}

// ---- The term-value path: parse → raise → evaluate → symbol (§3.5) ----

#[test]
fn the_term_value_path_evaluates_a_ground_arithmetic_term() {
    let (term, errors) = raise_value("1 + 2");
    assert!(errors.is_empty());
    assert_eq!(term.unwrap().evaluate(), Ok(Symbol::Number(3)));
}

#[test]
fn the_term_value_path_refuses_a_non_ground_term() {
    let term = raise_value("X + 1").0.expect("a term");
    assert!(matches!(term.evaluate(), Err(EvalError::NotGround { .. })));
}

#[test]
fn the_term_value_path_reads_back_a_ground_symbol() {
    let term = raise_value("f(1, 2)").0.expect("a term");
    assert!(matches!(term.evaluate(), Ok(Symbol::Function { .. })));
}

// ---- Totality: a diagnostic is a value; deep input does not overflow (§8, §13) ----

#[test]
fn a_number_beyond_the_engine_width_is_a_diagnostic() {
    let (term, errors) = raise("999999999999999999", Dialect::Clingo);
    assert!(
        term.is_some(),
        "a best-effort partial stands beside the diagnostic"
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(*errors[0].kind(), LowerErrorKind::NumberOutOfRange);
}

#[test]
fn an_incomplete_fragment_raises_to_no_term_without_panicking() {
    // `1 +` ends before its right operand: a read-more signal, not a panic.
    assert!(raise("1 +", Dialect::Clingo).0.is_none());
}

#[test]
fn a_deep_term_raises_without_overflowing() {
    // A term nested near the parse limit assembles iteratively (§13) — no stack
    // overflow, and the ground nest collapses to one symbol.
    let depth = 120;
    let text = format!("{}1{}", "f(".repeat(depth), ")".repeat(depth));
    let (term, errors) = raise(&text, Dialect::Clingo);
    assert!(
        errors.is_empty(),
        "a well-formed deep term has no diagnostics: {errors:?}"
    );
    assert!(matches!(
        term,
        Some(Term::Symbolic(Symbol::Function { .. }))
    ));
}
