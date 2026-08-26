//! Laws of the numeric bridge and ground evaluation (docs/design/program.md §3.4,
//! §3.5, §14): the ToSymbol/FromSymbol round-trip on the lossless integer set, the
//! fallible rounding adapters at their edges, the explicit ground evaluator that
//! refuses on overflow rather than wrapping, and the std-trait posture (Display and
//! Error) every refusal carries.

use themelios_program::symbol::{
    FromSymbol, Name, NotAnInteger, Sign, Symbol, ToSymbol, VarName, ceil, floor, round, trunc,
};
use themelios_program::term::{BinaryOp, EvalError, Term, UnaryOp, Variable, evaluate};

#[test]
fn to_symbol_maps_the_lossless_integers_to_number() {
    assert_eq!(0_i8.to_symbol(), Symbol::Number(0));
    assert_eq!(i8::MIN.to_symbol(), Symbol::Number(-128));
    assert_eq!(i16::MAX.to_symbol(), Symbol::Number(32767));
    assert_eq!(42_i32.to_symbol(), Symbol::Number(42));
    assert_eq!(i32::MIN.to_symbol(), Symbol::Number(i32::MIN));
    assert_eq!(255_u8.to_symbol(), Symbol::Number(255));
    assert_eq!(u16::MAX.to_symbol(), Symbol::Number(65535));
}

#[test]
fn from_symbol_round_trips_to_symbol_on_the_lossless_set() {
    for x in [0_i32, 1, -1, i32::MIN, i32::MAX] {
        assert_eq!(i32::from_symbol(&x.to_symbol()), Ok(x));
    }
    for x in [0_i8, i8::MIN, i8::MAX] {
        assert_eq!(i8::from_symbol(&x.to_symbol()), Ok(x));
    }
    for x in [0_u16, u16::MAX] {
        assert_eq!(u16::from_symbol(&x.to_symbol()), Ok(x));
    }
    let hello = Symbol::String("hi".to_owned());
    assert_eq!(String::from_symbol(&hello), Ok("hi".to_owned()));
}

#[test]
fn from_symbol_refuses_the_wrong_variant_or_out_of_range_carrying_the_symbol() {
    let text = Symbol::String("x".to_owned());
    assert_eq!(i32::from_symbol(&text).unwrap_err().found, text);
    let big = Symbol::Number(1000);
    assert_eq!(i8::from_symbol(&big).unwrap_err().found, big);
    assert!(String::from_symbol(&Symbol::Number(1)).is_err());
}

#[test]
fn rounding_lands_reals_in_the_integer_domain_under_a_stated_policy() {
    assert_eq!(floor(2.7), Ok(Symbol::Number(2)));
    assert_eq!(ceil(2.1), Ok(Symbol::Number(3)));
    assert_eq!(round(2.5), Ok(Symbol::Number(3)));
    assert_eq!(trunc(-2.7), Ok(Symbol::Number(-2)));
    // Non-finite refuses.
    assert_eq!(floor(f64::NAN), Err(NotAnInteger::NotFinite));
    assert_eq!(ceil(f64::INFINITY), Err(NotAnInteger::NotFinite));
    assert_eq!(round(f64::NEG_INFINITY), Err(NotAnInteger::NotFinite));
    // Out of range refuses — never a garbage integer.
    assert_eq!(floor(1e20), Err(NotAnInteger::OutOfRange));
    assert_eq!(ceil(-1e20), Err(NotAnInteger::OutOfRange));
    // The exact i32 bounds admit; one beyond refuses.
    assert_eq!(trunc(f64::from(i32::MAX)), Ok(Symbol::Number(i32::MAX)));
    assert_eq!(trunc(f64::from(i32::MIN)), Ok(Symbol::Number(i32::MIN)));
    assert_eq!(
        ceil(f64::from(i32::MAX) + 1.0),
        Err(NotAnInteger::OutOfRange)
    );
    assert_eq!(
        floor(f64::from(i32::MIN) - 1.0),
        Err(NotAnInteger::OutOfRange)
    );
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

fn binop(operator: BinaryOp, left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn function(name: &str, arguments: Vec<Term>) -> Term {
    Term::Function {
        name: Name::new(name).expect("identifier"),
        arguments,
    }
}

fn symbol_function(name: &str, arguments: Vec<Symbol>) -> Symbol {
    Symbol::Function {
        name: Name::new(name).expect("identifier"),
        arguments,
        sign: Sign::Positive,
    }
}

#[test]
fn evaluate_folds_ground_arithmetic_and_refuses_over_wrapping() {
    assert_eq!(
        evaluate(&binop(BinaryOp::Add, num(1), num(2))),
        Ok(Symbol::Number(3))
    );
    assert_eq!(
        evaluate(&binop(BinaryOp::Div, num(6), num(0))),
        Err(EvalError::Undefined)
    );
    // Overflow refuses; the wrapped value is never returned.
    assert_eq!(
        evaluate(&binop(BinaryOp::Add, num(i32::MAX), num(1))),
        Err(EvalError::Overflow)
    );
    // A ground constructor evaluates to its symbol.
    assert_eq!(
        evaluate(&function("f", vec![num(1), num(2)])),
        Ok(symbol_function(
            "f",
            vec![Symbol::Number(1), Symbol::Number(2)]
        ))
    );
    // Arithmetic nested under a constructor evaluates (unlike canonicalize, §5.1).
    assert_eq!(
        evaluate(&function("f", vec![binop(BinaryOp::Add, num(1), num(2))])),
        Ok(symbol_function("f", vec![Symbol::Number(3)]))
    );
}

#[test]
fn evaluate_handles_unary_and_absolute() {
    let neg = Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(num(5)),
    };
    assert_eq!(evaluate(&neg), Ok(Symbol::Number(-5)));
    assert_eq!(
        evaluate(&Term::Absolute(Box::new(num(-7)))),
        Ok(Symbol::Number(7))
    );
    let min_neg = Term::UnaryOperation {
        operator: UnaryOp::Negate,
        argument: Box::new(num(i32::MIN)),
    };
    assert_eq!(evaluate(&min_neg), Err(EvalError::Overflow));
}

#[test]
fn evaluate_refuses_variables_and_external_calls_carrying_the_offender() {
    let named = || Variable::Named(VarName::new("X").expect("variable"));
    assert_eq!(
        evaluate(&Term::Variable(named())),
        Err(EvalError::NotGround { variable: named() })
    );
    // A variable under arithmetic refuses NotGround, never a wrong answer.
    assert_eq!(
        evaluate(&binop(BinaryOp::Add, Term::Variable(named()), num(1))),
        Err(EvalError::NotGround { variable: named() })
    );
    // An @-call refuses External carrying its name.
    let call = Term::External {
        name: Name::new("f").expect("identifier"),
        arguments: vec![num(1)],
    };
    assert_eq!(
        evaluate(&call),
        Err(EvalError::External {
            name: Name::new("f").expect("identifier")
        })
    );
    assert_eq!(call.evaluate(), evaluate(&call));
}

#[test]
fn evaluate_refuses_a_set_former() {
    // A pool and an interval name a set, not a single symbol (flagged for review, §3.5).
    assert_eq!(
        evaluate(&Term::Pool(vec![num(1), num(2)])),
        Err(EvalError::Undefined)
    );
    assert_eq!(
        evaluate(&Term::Interval {
            lower: Box::new(num(1)),
            upper: Box::new(num(3))
        }),
        Err(EvalError::Undefined)
    );
}

#[test]
fn arithmetic_on_a_non_number_is_undefined() {
    // A constructor operand is not a number; arithmetic over it refuses Undefined.
    let f = function("f", vec![num(1)]);
    assert_eq!(
        evaluate(&binop(BinaryOp::Add, f, num(1))),
        Err(EvalError::Undefined)
    );
}

#[test]
fn every_refusal_carries_the_std_error_posture() {
    // Display is non-empty and the type composes as std::error::Error (§14, base §8.5),
    // over the names' refusals (Task 2's types included) and this task's.
    fn message<E: std::error::Error>(error: &E) -> String {
        error.to_string()
    }
    assert!(!message(&Name::new("X").unwrap_err()).is_empty());
    assert!(!message(&VarName::new("p").unwrap_err()).is_empty());
    assert!(!message(&floor(f64::NAN).unwrap_err()).is_empty());
    assert!(!message(&i8::from_symbol(&Symbol::Number(1000)).unwrap_err()).is_empty());
    assert!(!message(&evaluate(&Term::Variable(Variable::Anonymous)).unwrap_err()).is_empty());
}
