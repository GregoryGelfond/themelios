//! The answer-set law at the reading surface (docs/design/query.md §2.3, §4;
//! solve.md §5.1): every reading reads a member's answer set, never its
//! display — an atom the program entails but does not display reads `Yes`, and
//! a displayed term that is no true atom never does.

mod common;

use common::{answer_set, atom, with_displaying_agent};
use themelios_program::AnswerSet;
use themelios_program::program::{Arguments, Atom, Show};
use themelios_program::symbol::{Name, Sign, Signature, Symbol, VarName};
use themelios_program::term::Term;
use themelios_query::{AgentReading, Answer, BindingPattern, Query};
use themelios_solve::outcome::ShowRule;

/// A model's answer set beside the terms the program's directives display in it.
type Displayed = Vec<(AnswerSet, Vec<Symbol>)>;

/// The ground literal query `name`, with `sign`.
fn literal(sign: Sign, name: &str) -> Query {
    let name = Name::new(name).expect("a valid identifier");
    Query::of(Atom {
        sign,
        name,
        arguments: Arguments::Single(vec![]),
    })
    .expect("a ground literal is a query")
}

/// The unary ground atom `name(a)`.
fn applied(name: &str) -> Symbol {
    Symbol::function(
        Name::new(name).expect("a valid identifier"),
        [atom("a")],
        Sign::Positive,
    )
}

/// The open pattern `name(X)`.
fn pattern(name: &str) -> BindingPattern {
    let variable = Term::variable(VarName::new("X").expect("a valid variable name"));
    BindingPattern::of(Atom {
        sign: Sign::Positive,
        name: Name::new(name).expect("a valid identifier"),
        arguments: Arguments::Single(vec![variable]),
    })
    .expect("a binding pattern")
}

/// `q. #show. #show p : q.` — the answer set `{q}`, the display `{p}`.
fn hidden_atom_and_displayed_term() -> Displayed {
    vec![(answer_set(["q"]), vec![atom("p")])]
}

/// The rule of `#show.` alone: no atom displayed.
fn showing_no_atom() -> ShowRule {
    ShowRule::of([&Show::All])
}

/// `-p. #show q/0.` — the answer set `{-p}`, nothing displayed.
fn hidden_negation() -> (Displayed, ShowRule) {
    let negated = Symbol::function(
        Name::new("p").expect("a valid identifier"),
        [],
        Sign::Negative,
    );
    let name = Name::new("q").expect("a valid identifier");
    let show = Show::Signature(Signature {
        sign: Sign::Positive,
        name,
        arity: 0,
    });
    (
        vec![([negated].into_iter().collect(), vec![])],
        ShowRule::of([&show]),
    )
}

/// `q(a). #show. #show p(a) : q(a).` — the answer set `{q(a)}`, the display
/// `{p(a)}`.
fn hidden_instance_and_displayed_term() -> Displayed {
    vec![([applied("q")].into_iter().collect(), vec![applied("p")])]
}

#[test]
fn an_atom_the_program_hides_reads_yes() {
    with_displaying_agent(
        hidden_atom_and_displayed_term(),
        showing_no_atom(),
        |agent| {
            let answer = agent
                .answer(&literal(Sign::Positive, "q"))
                .expect("a reading");
            assert_eq!(answer, Answer::Yes);
        },
    );
}

#[test]
fn a_displayed_term_that_is_no_atom_never_reads_yes() {
    with_displaying_agent(
        hidden_atom_and_displayed_term(),
        showing_no_atom(),
        |agent| {
            let answer = agent
                .answer(&literal(Sign::Positive, "p"))
                .expect("a reading");
            assert_ne!(answer, Answer::Yes);
        },
    );
}

#[test]
fn a_hidden_negation_reads_yes() {
    let (models, show) = hidden_negation();
    with_displaying_agent(models, show, |agent| {
        let answer = agent
            .answer(&literal(Sign::Negative, "p"))
            .expect("a reading");
        assert_eq!(answer, Answer::Yes);
    });
}

#[test]
fn the_contrary_of_a_hidden_negation_reads_no() {
    let (models, show) = hidden_negation();
    with_displaying_agent(models, show, |agent| {
        let answer = agent
            .answer(&literal(Sign::Positive, "p"))
            .expect("a reading");
        assert_eq!(answer, Answer::No);
    });
}

#[test]
fn a_pattern_binds_an_atom_the_program_hides() {
    with_displaying_agent(
        hidden_instance_and_displayed_term(),
        showing_no_atom(),
        |agent| {
            let bindings = agent.bindings(&pattern("q")).expect("bindings");
            assert_eq!(
                bindings.yes().cloned().collect::<Vec<_>>(),
                vec![applied("q")]
            );
        },
    );
}

#[test]
fn a_pattern_binds_no_displayed_term() {
    with_displaying_agent(
        hidden_instance_and_displayed_term(),
        showing_no_atom(),
        |agent| {
            let bindings = agent.bindings(&pattern("p")).expect("bindings");
            assert!(bindings.yes().next().is_none());
            assert!(bindings.unknown().next().is_none());
        },
    );
}

#[test]
fn the_cautious_consequences_are_atoms_of_the_answer_set() {
    with_displaying_agent(
        hidden_atom_and_displayed_term(),
        showing_no_atom(),
        |agent| {
            let snapshot = agent.snapshot().expect("a world view");
            assert_eq!(snapshot.cautious().as_set(), &answer_set(["q"]));
        },
    );
}
