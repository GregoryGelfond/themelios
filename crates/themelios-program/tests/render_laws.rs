//! The canonical-form laws of rendering (docs/design/program.md §10, §4, §13). Rendering
//! is canonical — the same program renders the same text every time — and its shape is
//! fixed: binary operators and intervals fully parenthesized, a nullary function bare, a
//! one-element tuple keeping its distinguishing comma, the set-shaped children in `Ord`
//! order, one applied-form printer for a function term and an atom alike, and a single
//! refusal for a string value the chosen dialect cannot spell (grammar §4.4/§6.2/§9).

use themelios_base::source::{Source, SourceId};
use themelios_syntax::dialect::Dialect;
use themelios_syntax::parse::parse;

use themelios_program::program::{Atom, Program, Rule, Statement};
use themelios_program::provenance::WithProvenance;
use themelios_program::raise::raise;
use themelios_program::render::{Unspellable, render};
use themelios_program::symbol::{Name, Symbol};
use themelios_program::term::Term;

// ---- harness ----

/// Raise a program from clingo concrete syntax, asserting it lowers cleanly — a fixture
/// whose rendering the laws pin.
fn raised(text: &str) -> Program {
    let source = Source::new(SourceId::new(0), text.to_owned()).expect("the fixture admits");
    let lowered = raise(&parse(&source, Dialect::Clingo));
    assert!(
        lowered.diagnostics().is_empty(),
        "the fixture raises cleanly: {:?}",
        lowered.diagnostics(),
    );
    lowered.program().clone()
}

/// The clingo rendering of a fixture raised from concrete syntax.
fn rendered(text: &str) -> String {
    render(&raised(text), Dialect::Clingo).expect("the fixture renders")
}

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}

/// A one-statement program, for a directly-built value the laws pin.
fn program_of(statement: Statement) -> Program {
    Program::of([WithProvenance::constructed(statement)])
}

// ---- the laws ----

#[test]
fn rendering_is_canonical_the_same_program_renders_the_same_text() {
    let program = raised("q(X) :- p(X), X < 9.\np(1).\n");
    assert_eq!(
        render(&program, Dialect::Clingo).expect("renders"),
        render(&program, Dialect::Clingo).expect("renders"),
        "a program renders the same text every time",
    );
}

#[test]
fn a_program_is_rendered_in_canonical_order_independent_of_the_written_order() {
    // The statement set and the body set are ordered by content (§4), so two spellings that
    // differ only in order are one program and render identically.
    assert_eq!(rendered("a. b.\n"), rendered("b. a.\n"));
    assert_eq!(rendered("q :- a, b.\n"), rendered("q :- b, a.\n"));
}

#[test]
fn binary_operators_render_fully_parenthesized() {
    assert_eq!(rendered("p(X + 1).\n"), "p((X + 1)).\n");
    assert_eq!(rendered("p(X * Y).\n"), "p((X * Y)).\n");
    // Nested operators nest their parentheses — the tree's grouping is in the text.
    assert_eq!(rendered("p((X + 1) * 2).\n"), "p(((X + 1) * 2)).\n");
}

#[test]
fn intervals_render_fully_parenthesized() {
    assert_eq!(rendered("p(1..3).\n"), "p((1 .. 3)).\n");
    assert_eq!(rendered("p(1..N).\n"), "p((1 .. N)).\n");
}

#[test]
fn a_nullary_function_renders_bare() {
    // `a` as an argument is a constant — a nullary function — and renders without parentheses.
    assert_eq!(rendered("p(a).\n"), "p(a).\n");
    assert_eq!(rendered("q :- a.\n"), "q :- a.\n");
}

#[test]
fn a_one_element_tuple_keeps_its_comma_and_is_distinct_from_a_grouped_term() {
    // `(a,)` is a one-element tuple; `(a)` is `a` grouped (§5.1) — the comma distinguishes them.
    assert_eq!(rendered("p((a,)).\n"), "p((a,)).\n");
    assert_eq!(rendered("p((a)).\n"), "p(a).\n");
    assert_ne!(rendered("p((a,)).\n"), rendered("p((a)).\n"));
    // The empty tuple and the many-element tuple keep their shape.
    assert_eq!(rendered("p(()).\n"), "p(()).\n");
    assert_eq!(rendered("p((a, b)).\n"), "p((a, b)).\n");
}

#[test]
fn the_set_shaped_children_render_in_ord_order() {
    // A body written in either order is one value, rendered in the one canonical order.
    assert_eq!(rendered("q :- b, a.\n"), "q :- a, b.\n");
}

#[test]
fn the_applied_form_printer_is_shared_by_a_function_term_and_an_atom() {
    // A function term `f(X, 2)` and an atom `f(X, 2)` render their applied part identically —
    // one printer serves both, so the two cannot drift.
    let as_atom = rendered("f(X, 2).\n");
    let as_term = rendered("p(f(X, 2)).\n");
    assert_eq!(as_atom, "f(X, 2).\n");
    assert_eq!(as_term, "p(f(X, 2)).\n");
    assert!(as_atom.contains("f(X, 2)"));
    assert!(as_term.contains("f(X, 2)"));
}

#[test]
fn a_strongly_negated_atom_renders_its_sign() {
    assert_eq!(rendered("-p(a).\n"), "-p(a).\n");
}

#[test]
fn the_boolean_and_least_greatest_leaves_render_their_keywords() {
    assert_eq!(rendered("p(#inf).\n"), "p(#inf).\n");
    assert_eq!(rendered("p(#sup).\n"), "p(#sup).\n");
    assert_eq!(rendered("q :- #true, #false.\n"), "q :- #true, #false.\n");
}

#[test]
fn a_string_value_the_clingo_dialect_cannot_spell_refuses() {
    // A tab has no clingo string spelling (grammar §4.4/§9): the three escapes are `\"`,
    // `\\`, `\n`, and a raw control character is no ordinary character — so the value is
    // refused, carrying itself and the dialect, nothing mangled.
    let fact = Rule::fact(Atom::new(
        name("p"),
        [Term::Symbolic(Symbol::String("\t".to_owned()))],
    ));
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

#[test]
fn a_control_string_the_clingo_dialect_refuses_the_asp_core_2_dialect_spells() {
    // The gap is the clingo string rule's (§4.4); the ASP-Core-2 rule (§6.2) spells every
    // value — a backslash and a raw line break included — so the same program renders there.
    let fact = Rule::fact(Atom::new(
        name("p"),
        [Term::Symbolic(Symbol::String("\t".to_owned()))],
    ));
    let program = program_of(Statement::Rule(fact));
    assert!(render(&program, Dialect::Clingo).is_err());
    assert!(render(&program, Dialect::AspCore2).is_ok());
}

#[test]
fn an_ordinary_string_value_renders_quoted_and_escaped() {
    // The three clingo escapes and the quotes are spelled; an ordinary character is raw.
    let string = |value: &str| {
        let fact = Rule::fact(Atom::new(
            name("p"),
            [Term::Symbolic(Symbol::String(value.to_owned()))],
        ));
        render(&program_of(Statement::Rule(fact)), Dialect::Clingo).expect("renders")
    };
    assert_eq!(string("abc"), "p(\"abc\").\n");
    assert_eq!(string("a\"b"), "p(\"a\\\"b\").\n");
    assert_eq!(string("a\\b"), "p(\"a\\\\b\").\n");
    assert_eq!(string("a\nb"), "p(\"a\\nb\").\n");
}

#[test]
fn the_empty_program_renders_to_the_empty_string() {
    assert_eq!(
        render(&Program::default(), Dialect::Clingo).expect("renders"),
        ""
    );
}

#[test]
fn unspellable_reports_the_value_and_the_dialect_in_words() {
    let refused = Unspellable {
        value: "\t".to_owned(),
        dialect: Dialect::Clingo,
    };
    let shown = refused.to_string();
    assert!(shown.contains("clingo"), "names the dialect: {shown}");
}
