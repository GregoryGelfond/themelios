//! The failure walk (docs/design/program.md §15; spec §2 item 8): every refusing door of the
//! §15 table exercised **once**, each refusing with exactly its error type carrying the
//! **offending value** — a value, not a rendered string (spec §1.5) — and **never panicking**;
//! then the confirmation that the **total** doors do not refuse. This is the consolidated
//! catalogue that holds §15 honest: a door that grew a panic, or that started returning a
//! rendered string where a typed value belongs, or that quietly folded "cannot decide" into
//! "no", would fail here.
//!
//! These refusals are the tier's KR-soundness guarantees, not residual risk (refuse-over-repair,
//! program §7.2, §3.5): each door refuses *precisely where a value would otherwise be silently
//! wrong* — a wrapped overflow, a mangled string, a name the grammar rejects — so the caller
//! learns the question it must fix rather than carrying a lie forward. A door returning `Err(_)`
//! rather than unwinding is itself the never-panics evidence for that door.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::provenance::{TransformTag, WithProvenance};
use themelios_program::raise::raise;
use themelios_program::render::{Unspellable, render};
use themelios_program::symbol::{
    FromSymbol, Name, NotAVariable, NotAnIdentifier, NotAnInteger, Symbol, VarName, ceil, floor,
    round, trunc,
};
use themelios_program::term::{BinaryOp, EvalError, Term, Variable, evaluate};
use themelios_program::transform::{Rewrite, rewrite};
use themelios_program::unify::{NotAPattern, mgu};

// ---- builders (the estate's construction vocabulary) ----

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}
fn var(text: &str) -> Variable {
    Variable::Named(VarName::new(text).expect("a valid variable name"))
}
fn tvar(text: &str) -> Term {
    Term::Variable(var(text))
}
fn num(value: i32) -> Term {
    Term::from(value)
}
fn add(left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn binop(operator: BinaryOp, left: Term, right: Term) -> Term {
    Term::BinaryOperation {
        operator,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn atom(functor: &str, arguments: impl IntoIterator<Item = Term>) -> Atom {
    Atom::new(name(functor), arguments)
}
fn program_of(statement: Statement) -> Program {
    Program::of([WithProvenance::constructed(statement)])
}

// ---- the refusing doors of §15, one each, carrying the offending value ----

/// `Name::new` / `VarName::new` → `NotAnIdentifier` / `NotAVariable`, each carrying the text
/// the grammar's class rejects (§3.2). An uppercase word is a variable, so it is not a name; a
/// lowercase word is a name, so it is not a variable.
#[test]
fn name_construction_refuses_the_wrong_class_carrying_the_offending_text() {
    let not_a_name = Name::new("X").expect_err("an uppercase word is a variable, not a name");
    assert_eq!(
        not_a_name,
        NotAnIdentifier {
            text: "X".to_owned()
        }
    );

    let not_a_variable = VarName::new("p").expect_err("a lowercase word is a name, not a variable");
    assert_eq!(
        not_a_variable,
        NotAVariable {
            text: "p".to_owned()
        }
    );
}

/// `floor`/`ceil`/`round`/`trunc` → `NotAnInteger` by arm (§3.4): a non-finite real is
/// `NotFinite`; a finite real outside the `i32` range is `OutOfRange`. All four adapters and
/// both arms.
#[test]
fn the_rounding_adapters_refuse_the_out_of_domain_reals_by_arm() {
    assert_eq!(floor(f64::NAN), Err(NotAnInteger::NotFinite));
    assert_eq!(ceil(f64::INFINITY), Err(NotAnInteger::NotFinite));
    assert_eq!(round(f64::NEG_INFINITY), Err(NotAnInteger::NotFinite));
    assert_eq!(trunc(1e20), Err(NotAnInteger::OutOfRange));
    assert_eq!(floor(-1e20), Err(NotAnInteger::OutOfRange));
}

/// `evaluate` → each `EvalError` arm (§3.5), refusing rather than wrapping or guessing. Two
/// arms carry the offending value: `NotGround` the variable, `External` the extension name;
/// `Undefined` and `Overflow` are the conditions the authority itself rejects, carried as the
/// arm.
#[test]
fn ground_evaluation_refuses_each_way_carrying_the_offending_value() {
    // NotGround — the term is not ground; the refusal carries the very variable that occurred.
    let x = var("X");
    assert_eq!(
        evaluate(&Term::Variable(x.clone())),
        Err(EvalError::NotGround { variable: x }),
    );

    // External — an `@`-call needs the solve tier's registered context, so this tier reports
    // rather than guesses; the refusal carries the extension name.
    let call = Term::External {
        name: name("f"),
        arguments: vec![num(1)],
    };
    assert_eq!(
        evaluate(&call),
        Err(EvalError::External { name: name("f") }),
    );

    // Undefined — division by zero (a set-former or a non-number would refuse the same way).
    assert_eq!(
        evaluate(&binop(BinaryOp::Div, num(1), num(0))),
        Err(EvalError::Undefined),
    );

    // Overflow — a sum outside `i32`; this door refuses rather than silently wrapping (§3.5).
    assert_eq!(
        evaluate(&binop(BinaryOp::Add, num(i32::MAX), num(1))),
        Err(EvalError::Overflow),
    );
}

/// `FromSymbol::from_symbol` → `FromSymbolError` carrying the offending symbol *by value* and
/// the class it expected (§3.4): a string is not a number.
#[test]
fn the_symbol_conversion_refuses_the_wrong_variant_carrying_the_symbol() {
    let text = Symbol::String("x".to_owned());
    let refused = i32::from_symbol(&text).expect_err("a string is not a number");
    assert_eq!(refused.found, text);
    assert!(
        !refused.expected.is_empty(),
        "the refusal names the class it expected",
    );
}

/// `mgu` → `NotAPattern` carrying the offending term (§11.2) — **and** the load-bearing
/// distinction that `Ok(None)` is *no match*, a value, not a refusal, and `Ok(Some(_))` is the
/// affirmative third outcome. Collapsing the non-match into the refusal would report "no such
/// atom" when the truth is "I cannot decide" — the undetectable wrong answer this estate forbids.
#[test]
fn unification_refuses_a_non_pattern_yet_keeps_a_non_match_a_value() {
    // Err — a variable-bearing arithmetic argument does not denote; the refusal carries the term.
    let plus = add(tvar("X"), num(1));
    match mgu(&atom("p", [plus.clone()]), &atom("p", [num(3)])) {
        Err(NotAPattern::NonDenoting { term }) => assert_eq!(term, plus),
        other => panic!("expected NonDenoting carrying X + 1, got {other:?}"),
    }

    // Ok(None) — p(1) and p(2) do not unify: a *value*, not a refusal (§11.2).
    assert_eq!(mgu(&atom("p", [num(1)]), &atom("p", [num(2)])), Ok(None));

    // Ok(Some(_)) — the affirmative outcome: p(X) unifies with p(1).
    assert!(matches!(
        mgu(&atom("p", [tvar("X")]), &atom("p", [num(1)])),
        Ok(Some(_)),
    ));
}

/// `render` → `Unspellable` carrying the string value the dialect cannot spell and the dialect
/// (§10): a tab has no clingo string spelling, so the value is refused, carrying itself —
/// nothing is silently mangled.
#[test]
fn rendering_refuses_an_unspellable_string_carrying_the_value_and_dialect() {
    let fact = Rule::fact(atom("p", [Term::Symbolic(Symbol::String("\t".to_owned()))]));
    let program = program_of(Statement::Rule(fact));
    let refused = render(&program, Dialect::Clingo).expect_err("a tab has no clingo spelling");
    assert_eq!(
        refused,
        Unspellable {
            value: "\t".to_owned(),
            dialect: Dialect::Clingo,
        },
    );
}

// ---- the total doors: they do not refuse (§15) ----

/// A no-op rewrite: the framework walks the whole program, canonicalizing, and this rewriter
/// changes nothing — enough to exercise the transformation door as total.
struct Identity;
impl Rewrite for Identity {
    fn tag(&self) -> TransformTag {
        TransformTag::new("failure-walk-identity")
    }
}

/// The doors §15 marks total do not refuse and do not panic: `raise` yields a `Raised` even on
/// a malformed parse (a lowering diagnostic is a value it carries, never a `Result::Err`); the
/// constructors compose valid values; and the accessors, `canonicalize`, a transformation, and
/// equality / order / hash / clone / drop are all total.
#[test]
fn the_total_doors_do_not_refuse() {
    // `raise` is total. A deliberately malformed program still parses (error-resilient) and
    // raises to a `Raised` carrying the program it could recover and its lowering diagnostics —
    // a diagnostic is a value on a total raise, not a refusal (§8, §15). No `Result`, no panic.
    let malformed = "reachable(a) :- .\n:- not .\np(1 ..).\n";
    let source = Source::new(SourceId::new(0), malformed.to_owned()).expect("the source admits");
    let raised = raise(&parse(&source, Dialect::Clingo));
    let recovered: &Program = raised.program(); // always a value, never a refusal
    assert!(
        recovered.statements().count() > 0,
        "the raise recovers what it can from a malformed parse (§8)",
    );
    assert!(
        !raised.diagnostics().is_empty(),
        "the lowering diagnostic rode as a value in the Raised, not as a Result::Err (§8, §15)",
    );

    // The constructors compose valid values, total (§7.2); the accessors read.
    let program = program_of(Statement::Rule(Rule::fact(atom("p", [num(1)]))));
    assert_eq!(program.statements().count(), 1);

    // `canonicalize` is total and normalizing (§5).
    assert_eq!(num(1).canonicalize(), num(1));

    // A transformation is total (§9): the framework returns a `Program`, never a refusal.
    let transformed = rewrite(program.clone(), &mut Identity);
    assert_eq!(transformed.statements().count(), 1);

    // Equality, clone, and drop are total on a `Program` (§13); a whole program has equality
    // but no order or hash — those live on the set-elements it is built from.
    let twin = program.clone();
    assert_eq!(program, twin);
    drop(twin);

    // Ordering and hashing are total on `Term` (and `Symbol`), the ordered, hashed ground
    // vocabulary the sets order by (§3.1, §13) — a deep value is compared and hashed without
    // call-stack recursion, never refusing.
    let left = num(1);
    let right = add(tvar("X"), num(2));
    assert_eq!(left.cmp(&left), std::cmp::Ordering::Equal);
    let _ordering = left.cmp(&right);
    let mut hasher = DefaultHasher::new();
    right.hash(&mut hasher);
    let _digest = hasher.finish();
}
