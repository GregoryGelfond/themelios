//! Laws of the theory atoms and body-free directives (docs/design/program.md §4.8,
//! §4.9): a theory atom's elements are a set and its guard optional, its ordinary
//! arguments canonicalize at the door, `#const` carries an unevaluated term, and the
//! opaque regions (`#script`, `#include`) are carried but never acted on.

use themelios_program::program::{
    Const, ConstPolicy, Defined, Include, IncludeTarget, Script, TheoryAtom, TheoryElement,
    TheoryGuard, TheoryOperator, TheoryTerm,
};
use themelios_program::symbol::{Name, Sign, Signature, Symbol};
use themelios_program::term::{BinaryOp, Term, Variable};

fn name(text: &str) -> Name {
    Name::new(text).expect("identifier")
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

#[test]
fn a_theory_atom_s_elements_are_a_set() {
    let element = || TheoryElement::new([TheoryTerm::Symbolic(Symbol::Number(1))], None);
    let atom = TheoryAtom::new(name("sum"), [], [element(), element()], None);
    assert_eq!(atom.elements().count(), 1); // a duplicate element vanishes
}

#[test]
fn a_theory_atom_s_guard_is_optional() {
    let element = || TheoryElement::new([TheoryTerm::Symbolic(Symbol::Number(1))], None);
    let atom = TheoryAtom::new(name("sum"), [], [element()], None);
    assert!(atom.guard().is_none());
    let guarded = TheoryAtom::new(
        name("sum"),
        [],
        [element()],
        Some(TheoryGuard {
            operator: TheoryOperator::new("<="),
            term: TheoryTerm::Variable(Variable::Anonymous),
        }),
    );
    assert!(guarded.guard().is_some());
}

#[test]
fn a_theory_atom_s_ordinary_arguments_canonicalize_at_the_door() {
    let atom = TheoryAtom::new(
        name("sum"),
        [Term::Function {
            name: name("f"),
            arguments: vec![num(1)],
        }],
        [],
        None,
    );
    let collapsed = Term::Symbolic(Symbol::Function {
        name: name("f"),
        arguments: vec![Symbol::Number(1)],
        sign: Sign::Positive,
    });
    assert_eq!(atom.arguments().next(), Some(&collapsed));
}

#[test]
fn const_carries_an_unevaluated_term() {
    // `#const x = 1 + 2.` is a BinaryOperation, structurally distinct from `#const x = 3.`
    let sum = Const {
        name: name("x"),
        value: Term::BinaryOperation {
            operator: BinaryOp::Add,
            left: Box::new(num(1)),
            right: Box::new(num(2)),
        },
        policy: None,
    };
    let three = Const {
        name: name("x"),
        value: num(3),
        policy: None,
    };
    assert_ne!(sum, three);
    // A consumer that wants the denoted symbol evaluates (§3.5).
    assert_eq!(sum.value.evaluate(), Ok(Symbol::Number(3)));
    assert_eq!(three.value.evaluate(), Ok(Symbol::Number(3)));
    // The policy distinguishes.
    let overridden = Const {
        name: name("x"),
        value: num(3),
        policy: Some(ConstPolicy::Override),
    };
    assert_ne!(overridden, three);
}

#[test]
fn script_and_include_carry_opaque_regions_never_acted_on() {
    let script = Script::new(name("python"), "def main(): pass");
    assert_eq!(script.language(), &name("python"));
    assert_eq!(script.body(), "def main(): pass");
    let path = Include::new(IncludeTarget::Path("file.lp".to_owned()));
    assert_eq!(path.target(), &IncludeTarget::Path("file.lp".to_owned()));
    let system = Include::new(IncludeTarget::System(name("incmode")));
    assert_ne!(path.target(), system.target());
}

#[test]
fn defined_carries_a_signature() {
    let defined = Defined {
        signature: Signature {
            sign: Sign::Positive,
            name: name("p"),
            arity: 2,
        },
    };
    assert_eq!(defined.signature.arity, 2);
}
