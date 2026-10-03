//! Laws of the theory atoms and body-free directives (docs/design/program.md §4.8,
//! §4.9): a theory atom's elements are counted, each by occurrence, and its guard is
//! optional, its ordinary arguments canonicalize at the door, `#const` carries an
//! unevaluated term, and the opaque regions (`#script`, `#include`) are carried but never
//! acted on.

use std::collections::BTreeSet;

use themelios_program::program::{
    Const, ConstPolicy, Defined, Include, IncludeTarget, Program, Script, Statement, TheoryAtom,
    TheoryDefinition, TheoryElement, TheoryGuard, TheoryOperator, TheoryTerm,
};
use themelios_program::raise::raise_str;
use themelios_program::symbol::{Name, Sign, Signature, Symbol};
use themelios_program::term::{BinaryOp, Term, Variable};
use themelios_syntax::dialect::Dialect;

fn name(text: &str) -> Name {
    Name::new(text).expect("identifier")
}

fn num(n: i32) -> Term {
    Term::Symbolic(Symbol::Number(n))
}

/// Raise a whole program under the clingo dialect, refusing a fixture that does not parse
/// and raise cleanly.
fn raised(text: &str) -> Program {
    let raised = raise_str(text, Dialect::Clingo).expect("the source admits");
    assert!(
        raised.syntax_diagnostics().is_empty(),
        "fixture parses cleanly: {text}"
    );
    assert!(
        raised.lowering_diagnostics().is_empty(),
        "fixture raises cleanly: {text}"
    );
    raised.into_program()
}

#[test]
fn a_theory_atom_keeps_a_repeated_element() {
    let element = || TheoryElement::new([TheoryTerm::Symbolic(Symbol::Number(1))], None);
    let atom = TheoryAtom::new(name("sum"), [], [element(), element()], None);
    assert_eq!(atom.elements().count(), 2); // every theory element counts by occurrence (§4.9)
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

// ---- the global definitions (§4.2) ----

#[test]
fn a_constant_is_a_global_definition() {
    let constant = Statement::from(Const::new(name("n"), num(1), None));
    assert!(constant.is_global_definition());
}

#[test]
fn a_theory_definition_is_a_global_definition() {
    let theory = Statement::from(TheoryDefinition {
        name: name("t"),
        terms: BTreeSet::new(),
        atoms: BTreeSet::new(),
    });
    assert!(theory.is_global_definition());
}

#[test]
fn no_other_statement_is_a_global_definition() {
    let program = raised(
        "a. :- b. :~ a. [1@0] #minimize { 1@0 : a }. #show a/0. #project a/0. #defined a/0. \
         #edge (a, b). #heuristic a. [1, level] #external a. #include \"x.lp\".",
    );
    assert!(
        program
            .statements()
            .all(|node| !node.get().is_global_definition())
    );
}
